use cocozero_rp2350::fat32::{build_image_from_tree, VEntry};
use cocozero_rp2350::soc::CoCoZeroSoC;
use cocozero_rp2350::spi_sd::MemoryStorage;
use cocozero_rp2350::uf2::{parse_uf2, FLASH_BASE, RP2350_ARM_S_FAMILY_ID, UF2_FLAG_FAMILY_ID_PRESENT, UF2_MAGIC_END, UF2_MAGIC_START_0, UF2_MAGIC_START_1};

#[test]
fn test_soc_initialization_and_firmware_execution() {
    // 1. Build a virtual SD card containing settings.txt and a 8192-byte bas12.rom
    let sd_tree = vec![
        VEntry::File {
            name: "SETTINGS.TXT".to_string(),
            data: b"video_mode=60\nvideo_encoder=hardware\n".to_vec(),
        },
        VEntry::Dir {
            name: "COCO".to_string(),
            entries: vec![
                VEntry::Dir {
                    name: "ROMS".to_string(),
                    entries: vec![
                        VEntry::File {
                            name: "BAS12.ROM".to_string(),
                            data: vec![0x39; 8192], // 8192 bytes RTS opcode
                        },
                    ],
                },
            ],
        },
    ];
    let sd_bytes = build_image_from_tree(sd_tree).expect("Failed to build virtual FAT32 SD image");
    let storage = Box::new(MemoryStorage::from_bytes(sd_bytes));

    // 2. Initialize SoC
    let mut soc = CoCoZeroSoC::new(storage).expect("Failed to create SoC");

    // 3. Create a test firmware binary in UF2 format
    // Cortex-M33 Thumb-2 test program:
    // Vector table:
    //   [0]: Initial SP = 0x20082000
    //   [4]: Reset PC = 0x10000009 (Thumb bit set)
    // Code at 0x10000008:
    //   movs r0, #0x42
    //   b . (tight loop)
    let mut uf2_block = vec![0u8; 512];
    uf2_block[0..4].copy_from_slice(&UF2_MAGIC_START_0.to_le_bytes());
    uf2_block[4..8].copy_from_slice(&UF2_MAGIC_START_1.to_le_bytes());
    uf2_block[508..512].copy_from_slice(&UF2_MAGIC_END.to_le_bytes());
    uf2_block[8..12].copy_from_slice(&UF2_FLAG_FAMILY_ID_PRESENT.to_le_bytes());
    uf2_block[12..16].copy_from_slice(&FLASH_BASE.to_le_bytes());
    uf2_block[16..20].copy_from_slice(&256u32.to_le_bytes());
    uf2_block[28..32].copy_from_slice(&RP2350_ARM_S_FAMILY_ID.to_le_bytes());

    // Payload: Vector table + instructions
    let initial_sp = 0x2008_2000u32;
    let reset_pc = 0x1000_0009u32;
    uf2_block[32..36].copy_from_slice(&initial_sp.to_le_bytes());
    uf2_block[36..40].copy_from_slice(&reset_pc.to_le_bytes());

    // Instructions at offset 8 (address 0x10000008):
    // 0x2042: movs r0, #0x42
    // 0xe7fe: b .
    uf2_block[40..42].copy_from_slice(&0x2042u16.to_le_bytes());
    uf2_block[42..44].copy_from_slice(&0xe7feu16.to_le_bytes());

    let flash = parse_uf2(&uf2_block).expect("Failed to parse UF2");
    soc.load_firmware(&flash);

    assert_eq!(soc.emu.core(0).regs.sp(), initial_sp);
    assert_eq!(soc.emu.core(0).regs.pc(), reset_pc & !1);

    // 4. Run for 100 cycles
    soc.step_cycles(100).expect("Should step cleanly");

    // Core 0 should have executed movs r0, #0x42 and be spinning on b .
    assert_eq!(soc.emu.core(0).regs.r[0], 0x42);
    assert_eq!(soc.emu.core(0).regs.pc(), 0x1000_000a);
}
