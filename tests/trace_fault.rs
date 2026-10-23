use cocozero_rp2350::soc::CoCoZeroSoC;
use cocozero_rp2350::spi_sd::MemoryStorage;
use cocozero_rp2350::uf2::parse_uf2;

#[test]
fn trace_fault_step_by_step() {
    let sd_path = std::path::Path::new("coco");
    let sd_bytes = cocozero_rp2350::fat32::build_virtual_fat32(sd_path)
        .expect("Failed to build virtual FAT32");
    let storage = Box::new(MemoryStorage::from_bytes(sd_bytes));
    let mut soc = CoCoZeroSoC::new(storage).expect("SoC");
    soc.load_bootrom(&std::fs::read("roms/rp2350/bootrom-combined.bin").expect("bootrom"));

    let uf2_data = std::fs::read("roms/cocozero.uf2").expect("cocozero.uf2");
    let flash = parse_uf2(&uf2_data).expect("parse uf2");
    soc.load_firmware(&flash);

    for _ in 0..30 {
        soc.step_frame().expect("Should step frame");
        let pc0 = soc.emu.core(0).regs.pc();
        assert_ne!(
            pc0 & !1,
            0x1000be0e,
            "Core 0 must not enter panic handler at 0x1000be0e"
        );
    }
    assert_ne!(
        soc.emu.core(0).ppb.hfsr,
        0x4000_0000,
        "Core 0 must not HardFault"
    );
}

#[test]
fn test_watchdog_post_mortem_decoding() {
    use cocozero_rp2350::soc::{
        irq_name, phase_name, WatchdogPostMortem, WDP_ARMED, WDP_FAULT, WDP_HANG, WDP_MAGIC,
        WD_MAGIC, WP_LOOP, WP_PACE,
    };

    // 1. Cold power-on (all zeroes): None
    let wd_cold = [0u32; 8];
    let pw_cold = [0u32; 8];
    assert!(WatchdogPostMortem::parse(wd_cold, pw_cold).is_none());

    // 2. Classic freeze with WD_MAGIC only (no WDP_MAGIC in powman)
    let mut wd_classic = [0u32; 8];
    wd_classic[0] = WD_MAGIC;
    wd_classic[1] = WP_PACE; // Phase 9: WP_PACE
    wd_classic[2] = 1200; // Heartbeat frame 1200
    wd_classic[3] = (1 << 8) | WP_PACE; // 1 freeze, last phase WP_PACE
    wd_classic[5] = 0x1000_b240; // Fault PC
    wd_classic[6] = 0x1000_a110; // Fault LR

    let mut pw_classic = [0u32; 8];
    pw_classic[1] = (0 << 16) | 120; // Core 1 scanlines = 120 (alive)
    pw_classic[2] = (0xA042 << 16) | (1 << 8); // 6809 PC = 0xA042, 1 USB device

    let pm_classic = WatchdogPostMortem::parse(wd_classic, pw_classic).expect("Parse classic freeze");
    assert_eq!(pm_classic.phase_name, "WP_PACE");
    assert_eq!(pm_classic.fault_pc, 0x1000_b240);
    assert_eq!(pm_classic.guest_6809_pc, 0xA042);
    let desc_classic = pm_classic.describe();
    assert!(
        desc_classic.contains("[watchdog] post-mortem: core 0 stuck at 0x1000b240 in WP_PACE (6809 PC=0xA042, core 1 alive)"),
        "Classic description: {}",
        desc_classic
    );

    // 3. Early warning armed but never ran (WDP_ARMED)
    let mut wd_armed = [0u32; 8];
    wd_armed[0] = WD_MAGIC;
    wd_armed[7] = WDP_ARMED << 24;
    let mut pw_armed = [0u32; 8];
    pw_armed[7] = WDP_MAGIC;
    let pm_armed = WatchdogPostMortem::parse(wd_armed, pw_armed).expect("Parse armed record");
    assert_eq!(pm_armed.kind, WDP_ARMED);
    let desc_armed = pm_armed.describe();
    assert!(
        desc_armed.contains("the early warning never ran: core 0 had interrupts masked"),
        "Armed description: {}",
        desc_armed
    );

    // 4. Detailed HardFault record (WDP_FAULT)
    let mut wd_fault = [0u32; 8];
    wd_fault[0] = WD_MAGIC;
    wd_fault[1] = WP_LOOP;
    wd_fault[5] = 0x1000_c350; // PC
    wd_fault[6] = 0x1000_c200; // LR
    // kind = WDP_FAULT (3), core = 0, saves = 1, exception = 3 (HardFault)
    wd_fault[7] = (WDP_FAULT << 24) | (0 << 20) | (1 << 12) | 3;

    let mut pw_fault = [0u32; 8];
    pw_fault[0] = 54321; // uptime ms
    pw_fault[1] = (500 << 16) | 300; // span 500 ms, 300 lines
    pw_fault[2] = (0x0100 << 16) | (2 << 8) | 0xFD; // guest PC 0x0100, 2 USB devices, exc_return 0xFD
    pw_fault[3] = (1 << 15) | (1 << 7) | 0x0082; // CFSR: BFARVALID (bit 15) + MMARVALID (bit 7)
    pw_fault[4] = 0x4000_0000; // HFSR: FORCED
    pw_fault[5] = 0x2008_5000; // BFAR
    pw_fault[6] = 0x0000_0004; // MMFAR
    pw_fault[7] = WDP_MAGIC;

    let pm_fault = WatchdogPostMortem::parse(wd_fault, pw_fault).expect("Parse fault record");
    assert_eq!(pm_fault.kind, WDP_FAULT);
    assert_eq!(pm_fault.exception_num, 3);
    assert_eq!(irq_name(3), "HardFault");
    let desc_fault = pm_fault.describe();
    assert!(desc_fault.contains("HARD FAULT on core 0 pc=1000c350 lr=1000c200 in HardFault"));
    assert!(desc_fault.contains("bfar=20085000"));
    assert!(desc_fault.contains("mmfar=00000004"));
    assert!(desc_fault.contains("guest_pc=0100 usb=2 up=54321ms"));

    // 5. Execution deadlock / hang (WDP_HANG)
    let mut wd_hang = [0u32; 8];
    wd_hang[0] = WD_MAGIC;
    wd_hang[1] = WP_PACE;
    wd_hang[5] = 0x1000_9000;
    wd_hang[6] = 0x1000_8f00;
    // kind = WDP_HANG (2), core = 0, saves = 5, exception = 0 (thread mode)
    wd_hang[7] = (WDP_HANG << 24) | (0 << 20) | (5 << 12) | 0;

    let mut pw_hang = [0u32; 8];
    pw_hang[0] = 120_000; // 120s uptime
    pw_hang[1] = (1000 << 16) | 480; // 1000 ms span, 480 lines (alive)
    pw_hang[2] = (0x8000 << 16) | (1 << 8) | 0xFF;
    pw_hang[3] = 0x0000_0001; // active[0]
    pw_hang[4] = 0x0000_0000; // active[1]
    pw_hang[5] = 0x0000_0004; // pending[0]
    pw_hang[6] = 0x0000_0000; // pending[1]
    pw_hang[7] = WDP_MAGIC;

    let pm_hang = WatchdogPostMortem::parse(wd_hang, pw_hang).expect("Parse hang record");
    assert_eq!(pm_hang.kind, WDP_HANG);
    assert_eq!(pm_hang.saves, 5);
    assert_eq!(phase_name(WP_PACE), "WP_PACE");
    let desc_hang = pm_hang.describe();
    assert!(desc_hang.contains("hang, core 0 at pc=10009000 lr=10008f00 in thread mode (no interrupt)"));
    assert!(desc_hang.contains("5 warnings over 1000 ms, core 1 running (+480 lines)"));
    assert!(desc_hang.contains("guest_pc=8000 usb=1 up=120000ms"));
}

#[test]
fn test_soc_scratch_register_bus_roundtrip() {
    use cocozero_rp2350::soc::{CoCoZeroSoC, WDP_HANG, WDP_MAGIC, WD_MAGIC, WP_EMU};
    use cocozero_rp2350::spi_sd::MemoryStorage;

    let storage = Box::new(MemoryStorage::new(1024 * 1024));
    let mut soc = CoCoZeroSoC::new(storage).expect("Init SoC");

    // Initially all scratch registers read 0
    let wd_init = soc.read_watchdog_scratch();
    let pw_init = soc.read_powman_scratch();
    assert_eq!(wd_init, [0u32; 8]);
    assert_eq!(pw_init, [0u32; 8]);
    assert!(soc.read_watchdog_post_mortem().is_none());
    assert!(soc.format_post_mortem().is_none());

    // Write post-mortem data into simulated hardware registers:
    // Watchdog SCRATCH0..7 at 0x400D_800C..=0x400D_8028
    soc.emu.bus.write32(0x400D_800C, WD_MAGIC, 0); // scratch[0]
    soc.emu.bus.write32(0x400D_8010, WP_EMU, 0);   // scratch[1]
    soc.emu.bus.write32(0x400D_8014, 500, 0);      // scratch[2]
    soc.emu.bus.write32(0x400D_8018, (2 << 8) | WP_EMU, 0); // scratch[3]
    soc.emu.bus.write32(0x400D_8020, 0x1000_1234, 0); // scratch[5] (fault PC)
    soc.emu.bus.write32(0x400D_8024, 0x1000_5678, 0); // scratch[6] (fault LR)
    soc.emu.bus.write32(0x400D_8028, (WDP_HANG << 24) | (0 << 20) | (3 << 12) | 0, 0); // scratch[7]

    // Powman SCRATCH0..7 at 0x4010_00B0..=0x4010_00CC
    soc.emu.bus.write32(0x4010_00B0, 10_000, 0);   // pw[0]: uptime 10s
    soc.emu.bus.write32(0x4010_00B4, (600 << 16) | 120, 0); // pw[1]: span 600ms, c1 120 lines
    soc.emu.bus.write32(0x4010_00B8, (0x0F00 << 16) | (1 << 8) | 0xFD, 0); // pw[2]
    soc.emu.bus.write32(0x4010_00BC, 0x0001, 0);   // pw[3]
    soc.emu.bus.write32(0x4010_00C0, 0x0002, 0);   // pw[4]
    soc.emu.bus.write32(0x4010_00C4, 0x0004, 0);   // pw[5]
    soc.emu.bus.write32(0x4010_00C8, 0x0008, 0);   // pw[6]
    soc.emu.bus.write32(0x4010_00CC, WDP_MAGIC, 0); // pw[7]: WDP_MAGIC

    // Verify roundtrip read via SoC API
    let wd = soc.read_watchdog_scratch();
    assert_eq!(wd[0], WD_MAGIC);
    assert_eq!(wd[1], WP_EMU);
    assert_eq!(wd[5], 0x1000_1234);

    let pw = soc.read_powman_scratch();
    assert_eq!(pw[0], 10_000);
    assert_eq!(pw[7], WDP_MAGIC);

    // Verify structured post-mortem formatting
    let pm = soc.read_watchdog_post_mortem().expect("Valid post-mortem");
    assert_eq!(pm.fault_pc, 0x1000_1234);
    assert_eq!(pm.fault_lr, 0x1000_5678);
    assert_eq!(pm.phase_name, "WP_EMU");

    let formatted = soc.format_post_mortem().expect("Formatted string present");
    assert!(formatted.contains("[watchdog] post-mortem: hang, core 0 at pc=10001234 lr=10005678"));
    assert!(formatted.contains("3 warnings over 600 ms, core 1 running (+120 lines)"));
}
