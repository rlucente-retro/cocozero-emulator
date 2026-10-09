use cocozero_rp2350::fat32::build_virtual_fat32;
use cocozero_rp2350::soc::{CoCoZeroSoC, FB_PIXELS};
use cocozero_rp2350::spi_sd::MemoryStorage;
use cocozero_rp2350::uf2::parse_uf2;
use std::path::Path;

#[test]
fn test_type_command_in_basic() {
    if !Path::new("coco/roms/bas12.rom").exists() {
        eprintln!("Skipping test_type_command_in_basic: coco/roms/bas12.rom not present (user-supplied ROM required)");
        return;
    }

    let sd_path = Path::new("coco");
    let sd_bytes = build_virtual_fat32(sd_path).expect("Failed to build virtual FAT32");
    let storage = Box::new(MemoryStorage::from_bytes(sd_bytes));

    let mut soc = CoCoZeroSoC::new(storage).expect("Failed to create SoC");
    soc.load_bootrom(&std::fs::read("roms/rp2350/bootrom-combined.bin").expect("bootrom"));
    soc.fb_addr = Some(0x2002_2794);

    let uf2_data = std::fs::read("roms/cocozero.uf2").expect("Failed to read cocozero.uf2");
    let flash = parse_uf2(&uf2_data).expect("Failed to parse UF2");
    soc.load_firmware(&flash);

    // Boot until the BASIC prompt is fully ready (150 frames)
    for _ in 0..150 {
        soc.step_frame().expect("step_frame");
    }

    let mut fb_before = [0u16; FB_PIXELS];
    soc.extract_frame(&mut fb_before);

    // Type "CLS\r" via the official SoC keyboard API
    println!("Typing 'CLS' into Color Computer via soc.type_str...");
    soc.type_str("CLS\r");

    // Advance 40 frames so the 4 characters are typed and executed
    for _ in 0..40 {
        soc.step_frame().expect("step_frame");
    }

    let mut fb_after = [0u16; FB_PIXELS];
    soc.extract_frame(&mut fb_after);

    let mut diff_pixels = 0;
    for i in 0..FB_PIXELS {
        if fb_before[i] != fb_after[i] {
            diff_pixels += 1;
        }
    }
    println!("Pixels changed after executing CLS: {}", diff_pixels);
    assert!(diff_pixels > 500, "Executing CLS must clear the screen and change many pixels!");
}
