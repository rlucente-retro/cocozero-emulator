use cocozero_rp2350::fat32::build_virtual_fat32;
use cocozero_rp2350::soc::{CoCoZeroSoC, FB_PIXELS};
use cocozero_rp2350::spi_sd::MemoryStorage;
use cocozero_rp2350::uf2::parse_uf2;
use std::path::Path;

#[test]
fn test_cocozero_framebuffer_output() {
    if !Path::new("coco/roms/bas12.rom").exists() {
        eprintln!(
            "Skipping test_cocozero_framebuffer_output: coco/roms/bas12.rom not present (user-supplied ROM required)"
        );
        return;
    }

    let sd_path = Path::new("coco");
    let sd_bytes = build_virtual_fat32(sd_path).expect("Failed to build virtual FAT32");
    let storage = Box::new(MemoryStorage::from_bytes(sd_bytes));

    let mut soc = CoCoZeroSoC::new(storage).expect("Failed to create SoC");
    soc.load_bootrom(&std::fs::read("roms/rp2350/bootrom-combined.bin").expect("bootrom"));

    let uf2_data = std::fs::read("roms/cocozero.uf2").expect("Failed to read cocozero.uf2");
    let flash = parse_uf2(&uf2_data).expect("Failed to parse UF2");
    soc.load_firmware(&flash);

    // Run for up to 180 frames (allow 6809 boot, RAM test, and prompt to display)
    let mut fb = [0u16; FB_PIXELS];
    for f in 0..180 {
        soc.step_frame().expect("Should step frame");
        if f % 15 == 0 {
            println!(
                "Frame {}: c0 PC={:#x}, c1 PC={:#x}, wfe0={}, wfe1={}",
                f,
                soc.emu.core(0).regs.pc(),
                soc.emu.core(1).regs.pc(),
                soc.emu.core(0).is_wfe_waiting(),
                soc.emu.core(1).is_wfe_waiting()
            );
        }
    }
    soc.extract_frame(&mut fb);

    // Count distinct pixel colors
    let mut colors = std::collections::HashSet::new();
    for &p in fb.iter() {
        colors.insert(p);
    }
    println!(
        "Distinct colors in framebuffer after 180 frames: {}",
        colors.len()
    );
    println!(
        "Sample colors: {:?}",
        colors.iter().take(5).collect::<Vec<_>>()
    );

    let history = soc.sd_card.lock().unwrap().cmd_history.clone();
    println!(
        "SD Command History ({} commands): {:?}",
        history.len(),
        history.iter().take(20).collect::<Vec<_>>()
    );
    let tx_bytes = soc.serial_tx_log.lock().unwrap().clone();
    println!(
        "Total UART output ({} bytes):\n{}",
        tx_bytes.len(),
        String::from_utf8_lossy(&tx_bytes)
    );

    // Write BMP images so we can view the rendered output!
    soc.dump_framebuffer_bmp("tests/frame_180.bmp")
        .expect("Failed to dump framebuffer BMP");

    // Verify Color Computer 2 screen rendered:
    assert!(
        colors.contains(&2016),
        "Framebuffer must contain authentic Color Computer green (0x07E0)"
    );
    assert!(
        colors.contains(&0),
        "Framebuffer must contain black text/border pixels (0x0000)"
    );
    println!("Colors present: {:?}", colors);
}

#[test]
fn test_paletted_frame_extraction_video640() {
    use cocozero_rp2350::soc::{FB_HEIGHT, FB_PIXELS_640, FB_WIDTH_640};

    // Create a mock SoC with memory storage (no external ROMs needed)
    let storage = Box::new(MemoryStorage::new(1024));
    let mut soc = CoCoZeroSoC::new(storage).expect("Failed to create SoC");

    let pal_addr = 0x2001_0000u32;
    let fb_addr = 0x2002_0000u32;
    soc.symbols.pal565_addr = Some(pal_addr);
    soc.fb_addr = Some(fb_addr);

    assert!(
        soc.is_paletted(),
        "SoC must report is_paletted() when pal565_addr is present"
    );

    let pal_offset = pal_addr - 0x2000_0000;
    let fb_offset = fb_addr - 0x2000_0000;

    // 1. Populate g_pal565[256] in SRAM with known colors
    // Slot 0: Black (0x0000)
    // Slot 64: Factory green CARD_INK (0x07E0)
    // Slot 65: Black CARD_PAPER (0x0000)
    // Slot 80: VDG Green (0x07E0)
    // Slot 81: VDG Yellow (0xFFE0)
    // Slot 82: VDG Blue (0x001F)
    // Slot 83: VDG Red (0xF800)
    soc.emu.bus.memory.sram_write16(pal_offset + 0 * 2, 0x0000);
    soc.emu.bus.memory.sram_write16(pal_offset + 64 * 2, 0x07E0);
    soc.emu.bus.memory.sram_write16(pal_offset + 65 * 2, 0x0000);
    soc.emu.bus.memory.sram_write16(pal_offset + 80 * 2, 0x07E0);
    soc.emu.bus.memory.sram_write16(pal_offset + 81 * 2, 0xFFE0);
    soc.emu.bus.memory.sram_write16(pal_offset + 82 * 2, 0x001F);
    soc.emu.bus.memory.sram_write16(pal_offset + 83 * 2, 0xF800);

    // 2. Populate g_fb with paletted color indices
    soc.emu.bus.memory.sram_write8(fb_offset + 0, 83); // Pixel 0: Red
    soc.emu.bus.memory.sram_write8(fb_offset + 1, 81); // Pixel 1: Yellow
    soc.emu.bus.memory.sram_write8(fb_offset + 2, 82); // Pixel 2: Blue
    soc.emu.bus.memory.sram_write8(fb_offset + 3, 64); // Pixel 3: Factory Green
    let last_pixel_idx = (FB_WIDTH_640 * FB_HEIGHT - 1) as u32;
    soc.emu
        .bus
        .memory
        .sram_write8(fb_offset + last_pixel_idx, 64); // Last pixel: Factory Green

    // 3. Test 640x240 paletted extraction
    let mut fb640 = vec![0u16; FB_PIXELS_640];
    assert!(soc.extract_frame_640(&mut fb640));

    assert_eq!(
        fb640[0], 0xF800,
        "Pixel 0 must translate to Red (0xF800) via pal565[83]"
    );
    assert_eq!(
        fb640[1], 0xFFE0,
        "Pixel 1 must translate to Yellow (0xFFE0) via pal565[81]"
    );
    assert_eq!(
        fb640[2], 0x001F,
        "Pixel 2 must translate to Blue (0x001F) via pal565[82]"
    );
    assert_eq!(
        fb640[3], 0x07E0,
        "Pixel 3 must translate to Factory Green (0x07E0) via pal565[64]"
    );
    assert_eq!(
        fb640[FB_PIXELS_640 - 1],
        0x07E0,
        "Last pixel must translate to Factory Green (0x07E0)"
    );

    // 4. Test 320x240 downsampled extraction
    let mut fb320 = [0u16; FB_PIXELS];
    assert!(soc.extract_frame(&mut fb320));
    assert_eq!(
        fb320[0], 0xF800,
        "Downsampled pixel 0 must sample 640-pixel 0 (Red)"
    );
    assert_eq!(
        fb320[1], 0x001F,
        "Downsampled pixel 1 must sample 640-pixel 2 (Blue)"
    );

    // 5. Test BMP dump
    let dump_path = "tests/test_paletted_dump.bmp";
    soc.dump_framebuffer_bmp(dump_path)
        .expect("dump_framebuffer_bmp");
    assert!(
        Path::new(dump_path).exists(),
        "Dumped BMP file must exist on disk"
    );
    let bmp_bytes = std::fs::read(dump_path).expect("read bmp");
    // Standard BMP header checks: width at offset 18, height at offset 22
    let w = u32::from_le_bytes(bmp_bytes[18..22].try_into().unwrap());
    let h = u32::from_le_bytes(bmp_bytes[22..26].try_into().unwrap());
    assert_eq!(w, 640, "Paletted dump must produce 640-pixel wide BMP");
    assert_eq!(h, 240, "Paletted dump must produce 240-pixel high BMP");
    let _ = std::fs::remove_file(dump_path);
}
