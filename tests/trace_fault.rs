use cocozero_rp2350::soc::CoCoZeroSoC;
use cocozero_rp2350::spi_sd::MemoryStorage;
use cocozero_rp2350::uf2::parse_uf2;

#[test]
fn trace_fault_step_by_step() {
    let sd_path = std::path::Path::new("coco");
    let sd_bytes = cocozero_rp2350::fat32::build_virtual_fat32(sd_path).expect("Failed to build virtual FAT32");
    let storage = Box::new(MemoryStorage::from_bytes(sd_bytes));
    let mut soc = CoCoZeroSoC::new(storage).expect("SoC");
    soc.load_bootrom(&std::fs::read("roms/rp2350/bootrom-combined.bin").expect("bootrom"));

    let uf2_data = std::fs::read("roms/cocozero.uf2").expect("cocozero.uf2");
    let flash = parse_uf2(&uf2_data).expect("parse uf2");
    soc.load_firmware(&flash);

    for _ in 0..30 {
        soc.step_frame().expect("Should step frame");
        let pc0 = soc.emu.core(0).regs.pc();
        assert_ne!(pc0 & !1, 0x1000be0e, "Core 0 must not enter panic handler at 0x1000be0e");
    }
    assert_ne!(soc.emu.core(0).ppb.hfsr, 0x4000_0000, "Core 0 must not HardFault");
}
