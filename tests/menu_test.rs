use cocozero_rp2350::fat32::build_virtual_fat32;
use cocozero_rp2350::soc::{CoCoZeroSoC, FB_PIXELS};
use cocozero_rp2350::spi_sd::MemoryStorage;
use cocozero_rp2350::uf2::parse_uf2;
use std::path::Path;

#[test]
fn test_menu_activation() {
    let sd_path = Path::new("coco");
    let sd_bytes = build_virtual_fat32(sd_path).expect("Failed to build virtual FAT32");
    let storage = Box::new(MemoryStorage::from_bytes(sd_bytes));

    let mut soc = CoCoZeroSoC::new(storage).expect("Failed to create SoC");
    soc.load_bootrom(&std::fs::read("roms/rp2350/bootrom-combined.bin").expect("bootrom"));
    soc.fb_addr = Some(0x2002_2744);

    let uf2_data = std::fs::read("roms/cocozero.uf2").expect("Failed to read cocozero.uf2");
    let flash = parse_uf2(&uf2_data).expect("Failed to parse UF2");
    soc.load_firmware(&flash);

    // Boot for 150 frames until Color BASIC is ready
    for _ in 0..150 {
        soc.step_frame().expect("step_frame");
    }

    let mut fb_before = [0u16; FB_PIXELS];
    soc.extract_frame(&mut fb_before);

    // Inspect values at 0x200128E8 and 0x2004A2B8
    let menu_active = soc.emu.bus.memory.sram_read8(0x128e8);
    let menu_mode = soc.emu.bus.memory.sram_read32(0x4a2b8);
    let c6f8 = soc.emu.bus.memory.sram_read32(0xc6f8);
    println!("At frame 150: menu_active={}, menu_mode={}, c6f8=0x{:08X}",
        menu_active, menu_mode, c6f8);

    print!("menu struct before: ");
    for i in 0..16 {
        let w = soc.emu.bus.read32(0x200128E8 + i * 4, 0);
        print!("0x{:08X} ", w);
    }
    println!();

    // Dump words around Core0 PC
    let pc = soc.emu.core(0).regs.pc();
    print!("Code at PC: ");
    for i in 0..8 {
        let hw = soc.emu.bus.read16(pc + i * 2, 0);
        print!("{:04X} ", hw);
    }
    println!();

    // Verify initially menu is not active
    assert!(!soc.is_menu_active(), "Menu should initially be inactive");

    // Trigger Disks menu (F12, HID 0x45)
    soc.inject_hid_keycode(0x45);
    assert!(soc.is_menu_active(), "Menu should be active after F12 injection");
    assert_eq!(soc.get_menu_mode(), 0, "Menu mode should be 0 (Disks)");

    // Step 5 frames to let Core 0 render the menu
    for _ in 0..5 {
        soc.step_frame().expect("step_frame");
    }

    let mut fb_menu = [0u16; FB_PIXELS];
    soc.extract_frame(&mut fb_menu);

    let mut diff = 0;
    for i in 0..FB_PIXELS {
        if fb_before[i] != fb_menu[i] {
            diff += 1;
        }
    }
    println!("Diff pixels after menu activation: {}", diff);
    assert!(diff > 1000, "Menu overlay should have rendered pixels!");

    // Test navigation: Down (HID 0x51) and Up (HID 0x52)
    soc.inject_hid_keycode(0x51); // Down
    soc.inject_hid_keycode(0x52); // Up

    // Send Escape (HID 0x29) to close menu
    soc.inject_hid_keycode(0x29);

    // Step 5 frames
    for _ in 0..5 {
        soc.step_frame().expect("step_frame");
    }

    assert!(!soc.is_menu_active(), "Menu should be closed after Escape");
    println!("Menu successfully closed after Escape!");

    // Test F1 Info overlay (HID 0x3A)
    soc.inject_hid_keycode(0x3A);
    assert!(soc.is_menu_active(), "Menu should be active after F1 injection");
    assert_eq!(soc.get_menu_mode(), 4, "Menu mode should be 4 (Info)");

    // Close with Escape
    soc.inject_hid_keycode(0x29);
    for _ in 0..5 {
        soc.step_frame().expect("step_frame");
    }
    assert!(!soc.is_menu_active(), "Menu should be closed after Escape");
    println!("F1 Info overlay verified successfully!");
}
