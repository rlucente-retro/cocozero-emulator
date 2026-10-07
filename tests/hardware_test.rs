use cocozero_rp2350::soc::CoCoZeroSoC;
use cocozero_rp2350::spi_sd::MemoryStorage;
use cocozero_rp2350::uf2::{
    parse_uf2, FLASH_BASE, RP2350_ARM_S_FAMILY_ID, UF2_FLAG_FAMILY_ID_PRESENT, UF2_MAGIC_END,
    UF2_MAGIC_START_0, UF2_MAGIC_START_1,
};

#[test]
fn test_sio_tmds_cpu_execution() {
    let storage = Box::new(MemoryStorage::new(1024 * 1024));
    let mut soc = CoCoZeroSoC::new(storage).expect("Failed to create SoC");

    // Cortex-M33 machine code that configures TMDS and writes a pixel:
    // Addresses:
    //   SIO_BASE = 0xD0000000
    //   TMDS_CTRL = SIO_BASE + 0x1C0
    //   TMDS_WDATA = SIO_BASE + 0x1C4
    //   TMDS_PEEK_DOUBLE_L0 = SIO_BASE + 0x1D0
    //
    // Assembly:
    //   ldr r1, =0xD0000000       ; SIO base
    //   ldr r2, =0x08000000       ; TMDS_CTRL with PIX2_NOSHIFT
    //   str r2, [r1, #0x1c0]      ; write TMDS_CTRL
    //   ldr r2, =0x0000FFFF       ; White pixel
    //   str r2, [r1, #0x1c4]      ; write TMDS_WDATA
    //   ldr r0, [r1, #0x1d0]      ; read TMDS_PEEK_DOUBLE_L0
    //   b .

    let mut code = Vec::new();
    // 0: Initial SP
    code.extend_from_slice(&0x2008_2000u32.to_le_bytes());
    // 4: Reset PC = 0x1000_0009
    code.extend_from_slice(&0x1000_0009u32.to_le_bytes());

    // 8: ldr r1, [pc, #16] (loads 0xD0000000 from offset 28) -> 0x4904
    code.extend_from_slice(&0x4904u16.to_le_bytes());
    // 10: ldr r2, [pc, #20] (loads 0x08000000 from offset 32) -> 0x4a05
    code.extend_from_slice(&0x4a05u16.to_le_bytes());
    // 12: str r2, [r1, #0x1c0] (Thumb-2: str.w r2, [r1, #0x1c0] -> 0xf8c1 0x21c0)
    code.extend_from_slice(&[0xc1, 0xf8, 0xc0, 0x21]);
    // 16: ldr r2, [pc, #16] (loads 0x0000FFFF from offset 36) -> 0x4a04
    code.extend_from_slice(&0x4a04u16.to_le_bytes());
    // 18: str r2, [r1, #0x1c4] (Thumb-2: str.w r2, [r1, #0x1c4] -> 0xf8c1 0x21c4)
    code.extend_from_slice(&[0xc1, 0xf8, 0xc4, 0x21]);
    // 22: ldr r0, [r1, #0x1d0] (Thumb-2: ldr.w r0, [r1, #0x1d0] -> 0xf8d1 0x01d0)
    code.extend_from_slice(&[0xd1, 0xf8, 0xd0, 0x01]);
    // 26: b . (0xe7fe)
    code.extend_from_slice(&0xe7feu16.to_le_bytes());

    // Pad to word align
    while code.len() % 4 != 0 {
        code.push(0);
    }

    // Literal pool:
    // offset 28: 0xD000_0000
    // offset 32: 0x0800_0000
    // offset 36: 0x0000_FFFF
    code.extend_from_slice(&0xD000_0000u32.to_le_bytes());
    code.extend_from_slice(&0x0800_0000u32.to_le_bytes());
    code.extend_from_slice(&0x0000_FFFFu32.to_le_bytes());

    let mut uf2_block = vec![0u8; 512];
    uf2_block[0..4].copy_from_slice(&UF2_MAGIC_START_0.to_le_bytes());
    uf2_block[4..8].copy_from_slice(&UF2_MAGIC_START_1.to_le_bytes());
    uf2_block[508..512].copy_from_slice(&UF2_MAGIC_END.to_le_bytes());
    uf2_block[8..12].copy_from_slice(&UF2_FLAG_FAMILY_ID_PRESENT.to_le_bytes());
    uf2_block[12..16].copy_from_slice(&FLASH_BASE.to_le_bytes());
    uf2_block[16..20].copy_from_slice(&(code.len() as u32).to_le_bytes());
    uf2_block[28..32].copy_from_slice(&RP2350_ARM_S_FAMILY_ID.to_le_bytes());
    uf2_block[32..32 + code.len()].copy_from_slice(&code);

    let flash = parse_uf2(&uf2_block).expect("Should parse UF2");
    soc.load_firmware(&flash);

    soc.step_cycles(500).expect("Should step");

    // Verify SIO TMDS was configured and produced symbols into r0
    let r0 = soc.emu.core(0).regs.r[0];
    assert_eq!(r0, 0x000E_017F, "r0 should contain expected TMDS symbols");
    assert_eq!(soc.emu.bus.sio.tmds[0].ctrl, 0x0800_0000);
}
