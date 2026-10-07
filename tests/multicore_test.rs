use cocozero_rp2350::soc::CoCoZeroSoC;
use cocozero_rp2350::spi_sd::MemoryStorage;

#[test]
fn test_sio_fifo_multicore_communication() {
    let storage = Box::new(MemoryStorage::new(1024 * 1024));
    let mut soc = CoCoZeroSoC::new(storage).expect("Failed to create SoC");

    // SIO FIFO addresses:
    // SIO_BASE = 0xD000_0000
    // FIFO_ST = 0xD000_0050
    // FIFO_WR = 0xD000_0054
    // FIFO_RD = 0xD000_0058

    // Verify FIFOs are initially empty (VLD = 0, RDY = 1)
    let st0 = soc.emu.bus.read32(0xD000_0050, 0);
    let st1 = soc.emu.bus.read32(0xD000_0050, 1);
    assert_eq!(st0 & 1, 0, "Core 0 RX FIFO should be empty initially");
    assert_eq!(st1 & 1, 0, "Core 1 RX FIFO should be empty initially");
    assert_ne!(st0 & 2, 0, "Core 0 TX FIFO should be ready for write");
    assert_ne!(st1 & 2, 0, "Core 1 TX FIFO should be ready for write");

    // Core 0 writes a 32-bit word to FIFO_WR (routed to Core 1 RX)
    let test_word = 0xCAFE_BABE;
    soc.emu.bus.write32(0xD000_0054, test_word, 0);

    // Now Core 1 RX FIFO should have valid data (VLD = 1)
    let st1_after = soc.emu.bus.read32(0xD000_0050, 1);
    assert_ne!(st1_after & 1, 0, "Core 1 RX FIFO should have VLD set");

    // Core 1 reads from FIFO_RD
    let received = soc.emu.bus.read32(0xD000_0058, 1);
    assert_eq!(received, test_word, "Core 1 should read exactly what Core 0 sent");

    // After read, Core 1 RX FIFO should be empty again
    let st1_empty = soc.emu.bus.read32(0xD000_0050, 1);
    assert_eq!(st1_empty & 1, 0, "Core 1 RX FIFO should be empty after pop");

    // Core 1 sends a reply to Core 0
    let reply_word = 0xDEAD_BEEF;
    soc.emu.bus.write32(0xD000_0054, reply_word, 1);

    // Core 0 RX FIFO should have VLD set
    let st0_after = soc.emu.bus.read32(0xD000_0050, 0);
    assert_ne!(st0_after & 1, 0, "Core 0 RX FIFO should have VLD set");

    // Core 0 reads from FIFO_RD
    let core0_recv = soc.emu.bus.read32(0xD000_0058, 0);
    assert_eq!(core0_recv, reply_word, "Core 0 should read reply from Core 1");
}

#[test]
fn test_bootrom_core1_wfe_park() {
    let storage = Box::new(MemoryStorage::new(1024 * 1024));
    let mut soc = CoCoZeroSoC::new(storage).expect("Failed to create SoC");

    let bootrom_path = "roms/rp2350/bootrom-combined.bin";
    if let Ok(bootrom_bytes) = std::fs::read(bootrom_path) {
        soc.load_bootrom(&bootrom_bytes);

        // Run SoC for 5000 cycles so Boot ROM runs reset sequence
        soc.step_cycles(5000).expect("Boot ROM should step cleanly");

        // Core 1 in Boot ROM enters WFE waiting loop
        let core1 = soc.emu.core(1);
        println!("Core 1 state after Boot ROM boot: PC={:#x}, wfe={}", core1.regs.pc(), core1.is_wfe_waiting());
        assert!(core1.is_wfe_waiting(), "Core 1 should be parked in WFE waiting loop in Boot ROM");
    }
}

#[test]
fn test_dual_core_concurrent_fifo_exchange() {
    let storage = Box::new(MemoryStorage::new(1024 * 1024));
    let mut soc = CoCoZeroSoC::new(storage).expect("Failed to create SoC");

    // Machine code for Core 0 at 0x2000_0000:
    let mut c0_code = Vec::new();
    c0_code.extend_from_slice(&0x4904u16.to_le_bytes()); // 0: ldr r1, [pc, #16] (loads 0xD0000000)
    c0_code.extend_from_slice(&0x4a05u16.to_le_bytes()); // 2: ldr r2, [pc, #20] (loads 0x11223344)
    c0_code.extend_from_slice(&0x654au16.to_le_bytes()); // 4: str r2, [r1, #0x54] (FIFO_WR)
    c0_code.extend_from_slice(&0x6d0bu16.to_le_bytes()); // 6: ldr r3, [r1, #0x50] (FIFO_ST)
    c0_code.extend_from_slice(&0x07dbu16.to_le_bytes()); // 8: lsls r3, r3, #31
    c0_code.extend_from_slice(&0xd5fcu16.to_le_bytes()); // 10: bpl -4 (wait_reply)
    c0_code.extend_from_slice(&0x6d88u16.to_le_bytes()); // 12: ldr r0, [r1, #0x58] (FIFO_RD)
    c0_code.extend_from_slice(&0xe7feu16.to_le_bytes()); // 14: b .
    c0_code.extend_from_slice(&0x0000u16.to_le_bytes()); // 16: align pad
    c0_code.extend_from_slice(&0x0000u16.to_le_bytes()); // 18: align pad
    c0_code.extend_from_slice(&0xD000_0000u32.to_le_bytes()); // 20: SIO_BASE
    c0_code.extend_from_slice(&0x1122_3344u32.to_le_bytes()); // 24: test message

    // Machine code for Core 1 at 0x2000_0100:
    let mut c1_code = Vec::new();
    c1_code.extend_from_slice(&0x4904u16.to_le_bytes()); // 0: ldr r1, [pc, #16] (loads 0xD0000000)
    c1_code.extend_from_slice(&0x6d0bu16.to_le_bytes()); // 2: ldr r3, [r1, #0x50] (FIFO_ST)
    c1_code.extend_from_slice(&0x07dbu16.to_le_bytes()); // 4: lsls r3, r3, #31
    c1_code.extend_from_slice(&0xd5fcu16.to_le_bytes()); // 6: bpl -4 (wait_msg)
    c1_code.extend_from_slice(&0x6d88u16.to_le_bytes()); // 8: ldr r0, [r1, #0x58] (FIFO_RD)
    c1_code.extend_from_slice(&0x1c40u16.to_le_bytes()); // 10: adds r0, r0, #1
    c1_code.extend_from_slice(&0x6548u16.to_le_bytes()); // 12: str r0, [r1, #0x54] (FIFO_WR)
    c1_code.extend_from_slice(&0xe7feu16.to_le_bytes()); // 14: b .
    c1_code.extend_from_slice(&0x0000u16.to_le_bytes()); // 16: align pad
    c1_code.extend_from_slice(&0x0000u16.to_le_bytes()); // 18: align pad
    c1_code.extend_from_slice(&0xD000_0000u32.to_le_bytes()); // 20: SIO_BASE

    // Load both snippets directly into SRAM:
    soc.emu.load_image(0x2000_0000, &c0_code);
    soc.emu.load_image(0x2000_0100, &c1_code);

    // Initialize Core 0
    soc.emu.core_mut(0).regs.msp = 0x2004_0000;
    soc.emu.core_mut(0).regs.r[13] = 0x2004_0000;
    soc.emu.core_mut(0).regs.set_pc(0x2000_0000);
    soc.emu.core_mut(0).regs.xpsr = 1 << 24;

    // Initialize Core 1
    soc.emu.core_mut(1).regs.msp = 0x2008_0000;
    soc.emu.core_mut(1).regs.r[13] = 0x2008_0000;
    soc.emu.core_mut(1).regs.set_pc(0x2000_0100);
    soc.emu.core_mut(1).regs.xpsr = 1 << 24;
    soc.emu.bus.atomics.clear_wfe_waiting(1);
    soc.emu.bus.atomics.clear_halted(1);

    // Run dual-core simulation for 1000 cycles
    soc.step_cycles(1000).expect("Dual-core simulation should run cleanly");

    // Both cores should have exchanged messages and computed 0x11223345!
    let r0_c0 = soc.emu.core(0).regs.r[0];
    let r0_c1 = soc.emu.core(1).regs.r[0];
    println!("Dual-core FIFO exchange: Core 0 r0 = {:#x}, Core 1 r0 = {:#x}", r0_c0, r0_c1);
    assert_eq!(r0_c1, 0x1122_3345, "Core 1 should receive and increment test message");
    assert_eq!(r0_c0, 0x1122_3345, "Core 0 should receive incremented reply from Core 1");
}
