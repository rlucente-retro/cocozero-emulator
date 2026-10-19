use std::path::Path;
use cocozero_rp2350::fat32::build_virtual_fat32;
use cocozero_rp2350::soc::{CoCoZeroSoC, FB_PIXELS};
use cocozero_rp2350::spi_sd::MemoryStorage;
use cocozero_rp2350::uf2::parse_uf2;

#[test]
fn test_long_running_multicore_stability() {
    if !Path::new("coco/roms/bas12.rom").exists() {
        eprintln!("Skipping test_long_running_multicore_stability: coco/roms/bas12.rom not present");
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

    let mut fb = [0u16; FB_PIXELS];
    let mut total_samples_drained = 0usize;

    // Run 240 continuous frames (~4 seconds of real-time simulation)
    for frame in 0..240 {
        soc.step_frame().expect("step_frame should succeed without error");

        // Drain audio periodically every frame to simulate host audio callback
        let samples = soc.extract_audio();
        total_samples_drained += samples.len();

        if frame % 60 == 0 {
            soc.extract_frame(&mut fb);
            let telem = soc.telemetry();
            println!(
                "Stress Frame {}: C0 PC=0x{:08X} WFE={} | C1 PC=0x{:08X} WFE={} | Audio drained={}",
                frame,
                telem.cores.core0_pc,
                telem.cores.core0_wfe,
                telem.cores.core1_pc,
                telem.cores.core1_wfe,
                total_samples_drained
            );
        }
    }

    let final_telem = soc.telemetry();
    assert_eq!(final_telem.frame, 240);
    assert!(final_telem.total_cycles >= 240 * (252_000_000 / 60));
    assert!(total_samples_drained > 0, "Audio samples must be produced and drained");
    assert!(final_telem.cores.core0_cycles > 0);
    assert!(soc.is_keyboard_ready(), "Color BASIC should be fully initialized after 240 frames");
}

#[test]
fn test_concurrent_multisubsystem_stress() {
    let sd_path = Path::new("coco");
    let sd_bytes = build_virtual_fat32(sd_path).expect("Failed to build virtual FAT32");
    let storage = Box::new(MemoryStorage::from_bytes(sd_bytes));

    let mut soc = CoCoZeroSoC::new(storage).expect("Failed to create SoC");
    soc.load_bootrom(&std::fs::read("roms/rp2350/bootrom-combined.bin").expect("bootrom"));

    let uf2_data = std::fs::read("roms/cocozero.uf2").expect("Failed to read cocozero.uf2");
    let flash = parse_uf2(&uf2_data).expect("Failed to parse UF2");
    soc.load_firmware(&flash);

    let mut fb = [0u16; FB_PIXELS];
    let mut audio_accum = 0usize;

    // Phase A: Run 150 frames until boot is complete
    for _ in 0..150 {
        soc.step_frame().expect("step_frame");
        audio_accum += soc.extract_audio().len();
    }

    // Phase B: Interleave concurrent activities:
    // 1. Menu activation and navigation
    soc.inject_hid_keycode(0x45); // F12 Disks menu
    assert!(soc.is_menu_active());

    for step in 0..20 {
        // Interleave D-Pad navigation
        if step % 2 == 0 {
            soc.inject_hid_keycode(0x51); // Down
        } else {
            soc.inject_hid_keycode(0x52); // Up
        }

        // Perform sector writes directly to storage to simulate concurrent disk activity
        let dummy_sector = [0x55u8; 512];
        soc.sd_card.lock().unwrap().backing.write_sector(100 + step as u64, &dummy_sector);

        // Advance simulation frame
        soc.step_frame().expect("step_frame");
        audio_accum += soc.extract_audio().len();

        // Flush storage periodically
        if step % 5 == 0 {
            soc.flush_storage();
        }
    }

    // Close menu (Escape)
    soc.inject_hid_keycode(0x29);

    // Phase C: Typing while extracting audio and frame
    soc.type_str("PRINT 999\r");
    for _ in 0..30 {
        soc.step_frame().expect("step_frame");
        audio_accum += soc.extract_audio().len();
    }

    soc.extract_frame(&mut fb);
    let final_telem = soc.telemetry();

    assert!(final_telem.frame >= 200);
    assert!(audio_accum > 0);
    println!("Final multi-subsystem stress telemetry: {}", final_telem);
}

#[test]
fn test_dynamic_turbo_resilience() {
    let sd_path = Path::new("coco");
    let sd_bytes = build_virtual_fat32(sd_path).expect("Failed to build virtual FAT32");
    let storage = Box::new(MemoryStorage::from_bytes(sd_bytes));

    let mut soc = CoCoZeroSoC::new(storage).expect("Failed to create SoC");
    soc.load_bootrom(&std::fs::read("roms/rp2350/bootrom-combined.bin").expect("bootrom"));

    let uf2_data = std::fs::read("roms/cocozero.uf2").expect("Failed to read cocozero.uf2");
    let flash = parse_uf2(&uf2_data).expect("Failed to parse UF2");
    soc.load_firmware(&flash);

    // Step across different turbo scaling factors dynamically: 1x -> 2x -> 4x -> 6x -> 1x
    let turbo_profile = [1, 2, 4, 6, 4, 2, 1];
    for &turbo in &turbo_profile {
        for _ in 0..10 {
            soc.step_frame_turbo(turbo).expect("dynamic turbo step should succeed");
            let _ = soc.extract_audio();
        }
    }

    let telem = soc.telemetry();
    assert_eq!(telem.frame, (turbo_profile.len() * 10) as u64);
    assert!(telem.total_cycles > 0);
}
