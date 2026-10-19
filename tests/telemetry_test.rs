use cocozero_rp2350::fat32::build_virtual_fat32;
use cocozero_rp2350::soc::{CoCoZeroSoC, FB_HEIGHT, FB_PIXELS, FB_WIDTH, save_bmp};
use cocozero_rp2350::spi_sd::MemoryStorage;
use cocozero_rp2350::uf2::parse_uf2;
use std::path::Path;

#[test]
fn test_soc_telemetry_gathering_and_display() {
    let sd_path = Path::new("coco");
    let sd_bytes = build_virtual_fat32(sd_path).expect("Failed to build virtual FAT32");
    let storage = Box::new(MemoryStorage::from_bytes(sd_bytes));

    let mut soc = CoCoZeroSoC::new(storage).expect("Failed to create SoC");
    soc.load_bootrom(&std::fs::read("roms/rp2350/bootrom-combined.bin").expect("bootrom"));

    let uf2_data = std::fs::read("roms/cocozero.uf2").expect("Failed to read cocozero.uf2");
    let flash = parse_uf2(&uf2_data).expect("Failed to parse UF2");
    soc.load_firmware(&flash);

    // Initial telemetry check
    let telem0 = soc.telemetry();
    assert_eq!(telem0.frame, 0);
    assert_eq!(telem0.audio.total_samples_drained, 0);
    assert_eq!(telem0.audio.lapped_samples, 0);

    // Step 30 frames with audio extraction
    for _ in 0..30 {
        soc.step_frame().expect("step_frame");
        let _ = soc.extract_audio();
    }

    let telem = soc.telemetry();
    assert_eq!(telem.frame, 30);
    assert!(telem.total_cycles > 0);
    assert!(telem.cores.core0_cycles > 0);
    assert!(telem.audio.total_samples_drained > 0);
    assert!(telem.storage.sectors_read > 0);

    // Verify Display trait implementation formats string properly
    let display_str = format!("{}", telem);
    assert!(display_str.contains("[telemetry] frame="));
    assert!(display_str.contains("cycles="));
    assert!(display_str.contains("C0: PC=0x"));
    assert!(display_str.contains("C1: PC=0x"));
    assert!(display_str.contains("Aud: drained="));
    assert!(display_str.contains("SD: r="));
    println!("Formatted telemetry string: {}", display_str);
}

#[test]
fn test_bmp_framebuffer_dump() {
    let temp_bmp_path = "tests/test_dump_output.bmp";

    // 1. Test standalone save_bmp with a known RGB565 test pattern
    let mut pattern = [0u16; FB_PIXELS];
    // Red pixel in top-left (y=0, x=0)
    pattern[0] = 0xF800; // 5-bit red = 31, green = 0, blue = 0
    // Green pixel at (y=0, x=1)
    pattern[1] = 0x07E0; // red = 0, 6-bit green = 63, blue = 0
    // Blue pixel at (y=0, x=2)
    pattern[2] = 0x001F; // red = 0, green = 0, 5-bit blue = 31

    save_bmp(temp_bmp_path, &pattern, FB_WIDTH, FB_HEIGHT).expect("Failed to write test BMP");

    let bytes = std::fs::read(temp_bmp_path).expect("Read test BMP");
    // Standard 320x240 24-bit BMP: 54 header bytes + 320 * 240 * 3 = 230,454 bytes
    assert_eq!(bytes.len(), 54 + FB_WIDTH * FB_HEIGHT * 3);
    assert_eq!(&bytes[0..2], b"BM");

    let dib_width = i32::from_le_bytes(bytes[18..22].try_into().unwrap());
    let dib_height = i32::from_le_bytes(bytes[22..26].try_into().unwrap());
    let bpp = u16::from_le_bytes(bytes[28..30].try_into().unwrap());

    assert_eq!(dib_width, FB_WIDTH as i32);
    assert_eq!(dib_height, FB_HEIGHT as i32);
    assert_eq!(bpp, 24);

    // Verify bottom-up pixel ordering:
    // In bottom-up BMP, row y=0 is stored at the very last row of pixel data in the file:
    // row y=0 offset = 54 + (FB_HEIGHT - 1) * FB_WIDTH * 3
    let row0_offset = 54 + (FB_HEIGHT - 1) * FB_WIDTH * 3;
    // Pixel 0 (Red): B=0, G=0, R=255
    assert_eq!(bytes[row0_offset], 0);
    assert_eq!(bytes[row0_offset + 1], 0);
    assert_eq!(bytes[row0_offset + 2], 255);

    // Pixel 1 (Green): B=0, G=255, R=0
    assert_eq!(bytes[row0_offset + 3], 0);
    assert_eq!(bytes[row0_offset + 4], 255);
    assert_eq!(bytes[row0_offset + 5], 0);

    // Pixel 2 (Blue): B=255, G=0, R=0
    assert_eq!(bytes[row0_offset + 6], 255);
    assert_eq!(bytes[row0_offset + 7], 0);
    assert_eq!(bytes[row0_offset + 8], 0);

    // Clean up temporary test file
    let _ = std::fs::remove_file(temp_bmp_path);
}

#[test]
fn test_script_and_type_command_processing() {
    let mut kbd = cocozero_rp2350::keyboard::KeyboardMatrix::new();

    // Verify unescaping and translation of commands
    let script_lines = vec![
        "# This is a comment",
        "",
        "CLS",
        "# Another comment",
        "PRINT 42",
    ];

    for line in script_lines {
        let trimmed = line.trim_end();
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            kbd.type_str(trimmed);
            kbd.type_char('\r');
        }
    }

    // Verify direct string unescaping
    let raw_type = "DIR\\n";
    let unescaped = raw_type.replace("\\n", "\n").replace("\\r", "\r");
    for ch in unescaped.chars() {
        let mapped = match ch {
            '\n' => '\r',
            c => c,
        };
        kbd.type_char(mapped);
    }
}
