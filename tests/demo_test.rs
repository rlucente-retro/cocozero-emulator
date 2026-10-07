use cocozero_rp2350::soc::CoCoZeroSoC;
use cocozero_rp2350::spi_sd::MemoryStorage;
use cocozero_rp2350::uf2::parse_uf2;

#[test]
fn test_demo_uf2_execution() {
    let storage = Box::new(MemoryStorage::new(1024 * 1024));
    let mut soc = CoCoZeroSoC::new(storage).expect("Failed to create SoC");

    let uf2_data = std::fs::read("roms/demo.uf2").expect("Failed to read demo.uf2");
    let flash = parse_uf2(&uf2_data).expect("Failed to parse UF2");
    soc.load_firmware(&flash);

    println!("Initial PC: {:#x}", soc.emu.core(0).regs.pc());

    for i in 0..10 {
        soc.step_cycles(1).unwrap();
        let c0 = soc.emu.core(0);
        println!("step {}: PC={:#x}, r0={:#x}, r1={:#x}, r2={:#x}", i, c0.regs.pc(), c0.regs.r[0], c0.regs.r[1], c0.regs.r[2]);
    }

    soc.step_cycles(10000).unwrap();

    let c0 = soc.emu.core(0);
    println!("\nFinal PC={:#x}, r0={:#x}, r1={:#x}, r2={:#x}, TMDS CTRL={:#x}", 
        c0.regs.pc(), c0.regs.r[0], c0.regs.r[1], c0.regs.r[2], soc.emu.bus.sio.tmds[0].ctrl);
    assert_eq!(soc.emu.bus.sio.tmds[0].ctrl, 0x0800_0000);
}
