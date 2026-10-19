#![allow(clippy::chunks_exact_to_as_chunks)]

use std::path::Path;

use cocozero_rp2350::fat32::build_virtual_fat32;
use cocozero_rp2350::soc::{CoCoZeroSoC, AUDIO_RING_SAMPLES};
use cocozero_rp2350::spi_sd::MemoryStorage;
use cocozero_rp2350::uf2::parse_uf2;

#[test]
fn test_audio_ring_drain_and_telemetry() {
    if !Path::new("coco/roms/bas12.rom").exists() {
        eprintln!("Skipping test_audio_ring_drain_and_telemetry: coco/roms/bas12.rom not present");
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

    // Initial state: 0 samples drained, read index at 0
    assert_eq!(soc.host_audio_r, 0);
    assert!(soc.extract_audio().is_empty());

    // Step for 60 frames (~1.0s real time)
    for _ in 0..60 {
        soc.step_frame().expect("Should step frame");
    }

    // Verify producer monotonic write index advanced in SRAM
    let w = soc.emu.bus.memory.sram_read32(soc.symbols.audio_w_addr - 0x2000_0000);
    println!("After 60 frames: audio_w = {}", w);
    assert!(
        w > 0,
        "Audio write index audio_w must advance during emulation, got {}",
        w
    );
    // At ~800 samples/frame across 60 frames, expected ~40,000 to ~55,000 samples
    assert!(
        (40_000..=60_000).contains(&w),
        "Expected ~48,000 samples after 60 frames, got {}",
        w
    );

    // Drain audio
    let samples = soc.extract_audio();
    assert!(
        !samples.is_empty(),
        "extract_audio should yield samples after 60 frames"
    );
    let min_sample = samples.iter().copied().min().unwrap_or(0);
    let max_sample = samples.iter().copied().max().unwrap_or(0);
    println!("Drained {} samples: min={}, max={}, sample[0..10]={:?}", samples.len(), min_sample, max_sample, &samples[..10]);

    // Because 60 frames produced > 8,192 samples (AUDIO_RING_SAMPLES), the producer
    // lapped the reader during the 60-frame run without draining.
    // extract_audio should catch up and return exactly AUDIO_RING_SAMPLES * 2 stereo values.
    assert_eq!(
        samples.len(),
        AUDIO_RING_SAMPLES * 2,
        "Un-drained audio ring buffer overflow must clamp to AUDIO_RING_SAMPLES stereo pairs"
    );
    assert_eq!(soc.host_audio_r, w, "host_audio_r must advance to w");

    // Verify SRAM telemetry g_audio_r is synchronized with w
    let r_sram = soc.emu.bus.memory.sram_read32(soc.symbols.audio_r_addr - 0x2000_0000);
    assert_eq!(r_sram, w, "Firmware SRAM g_audio_r must match w");

    // Verify stereo interleaving: L == R for all pairs
    for chunk in samples.chunks_exact(2) {
        assert_eq!(chunk[0], chunk[1], "Mono sample must be duplicated identically into left and right stereo channels");
    }

    // Calling extract_audio immediately without stepping frames must yield empty Vec
    let empty_samples = soc.extract_audio();
    assert!(empty_samples.is_empty(), "Immediate re-extraction without stepping must be empty");

    // Step 5 more frames (5 frames * ~800 samples = ~4,000 samples, well within 8,192 capacity)
    for _ in 0..5 {
        soc.step_frame().expect("Should step frame");
    }

    let w2 = soc.emu.bus.memory.sram_read32(soc.symbols.audio_w_addr - 0x2000_0000);
    assert!(w2 > w, "audio_w must advance after 5 more frames");
    let produced = w2 - w;
    println!("Produced {} samples in 5 frames", produced);

    let samples2 = soc.extract_audio();
    assert_eq!(
        samples2.len(),
        (produced as usize) * 2,
        "Per-frame drain must match exact produced count duplicated to stereo"
    );
    assert_eq!(soc.host_audio_r, w2);

    let r_sram2 = soc.emu.bus.memory.sram_read32(soc.symbols.audio_r_addr - 0x2000_0000);
    assert_eq!(r_sram2, w2);
}

#[test]
fn test_audio_ring_overflow_handling() {
    let storage = Box::new(MemoryStorage::new(1024 * 1024));
    let mut soc = CoCoZeroSoC::new(storage).expect("Failed to create SoC");

    // Manually set write pointer beyond ring capacity
    let fake_w = 20_000u32;
    soc.emu.bus.memory.sram_write32(soc.symbols.audio_w_addr - 0x2000_0000, fake_w);

    assert_eq!(soc.host_audio_r, 0);
    let samples = soc.extract_audio();

    // Must clamp to AUDIO_RING_SAMPLES * 2 stereo values
    assert_eq!(samples.len(), AUDIO_RING_SAMPLES * 2);
    assert_eq!(soc.host_audio_r, fake_w);

    let r_sram = soc.emu.bus.memory.sram_read32(soc.symbols.audio_r_addr - 0x2000_0000);
    assert_eq!(r_sram, fake_w);
}
