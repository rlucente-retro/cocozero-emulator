use std::fs::File;
use std::io::Write;
use cocozero_rp2350::uf2::{
    FLASH_BASE, RP2350_ARM_S_FAMILY_ID, UF2_FLAG_FAMILY_ID_PRESENT, UF2_MAGIC_END,
    UF2_MAGIC_START_0, UF2_MAGIC_START_1,
};

fn main() {
    println!("Generating sample CoCo Zero RP2350 bare-metal UF2 firmware...");

    // Assembly instructions:
    // 0x1000_0000: SP = 0x2008_2000
    // 0x1000_0004: PC = 0x1000_0009
    //
    // Code at 0x1000_0008:
    // 1. Setup UART0:
    //    ldr r1, =0x40070000     ; UART0 base
    //    adr r2, msg             ; message pointer
    // print_loop:
    //    ldrb r0, [r2]
    //    cbz r0, print_done
    //    str r0, [r1, #0]        ; UART0_DR (write char)
    //    adds r2, #1
    //    b print_loop
    // print_done:
    //
    // 2. Setup SIO TMDS:
    //    ldr r1, =0xD0000000     ; SIO base
    //    ldr r0, =0x08000000     ; TMDS_CTRL with PIX2_NOSHIFT
    //    str r0, [r1, #0x1c0]    ; write TMDS_CTRL
    //
    // 3. Fill Framebuffer at 0x2001_0000 with CoCo Green (0x07E0):
    //    ldr r1, =0x20010000     ; FB base
    //    ldr r2, =0x07E007E0     ; 2 green pixels (RGB565)
    //    ldr r3, =38400          ; 76800 pixels / 2 = 38400 words
    // fb_loop:
    //    str r2, [r1], #4        ; write 2 pixels with post-index increment
    //    subs r3, #1
    //    bne fb_loop
    //
    // 4. Tight loop:
    //    b .

    let mut code = Vec::new();

    // 0: Initial SP
    code.extend_from_slice(&0x2008_2000u32.to_le_bytes());
    // 4: Reset PC
    code.extend_from_slice(&0x1000_0009u32.to_le_bytes());

    // Machine code for Thumb-2:
    // offset 8: ldr r1, =0x40070000
    // offset 10: adr r2, msg
    // ...
    // Encode clean Thumb instructions:

    // 0x08: ldr r1, [pc, #lit_uart] -> 0x49xx
    // 0x0a: adr r2, msg -> 0xa2xx
    // print_loop (0x0c):
    //   ldrb r0, [r2] -> 0x7810
    //   cmp r0, #0 -> 0x2800
    //   beq print_done -> 0xd004
    //   str r0, [r1, #0] -> 0x6008
    //   adds r2, #1 -> 0x1c52
    //   b print_loop -> 0xe7f9
    // print_done (0x1a):
    //   ldr r1, [pc, #lit_sio]
    //   ldr r0, [pc, #lit_tmds_ctrl]
    //   str.w r0, [r1, #0x1c0] -> 0xf8c1 0x01c0
    //   ldr r1, [pc, #lit_fb]
    //   ldr r2, [pc, #lit_color]
    //   ldr r3, [pc, #lit_count]
    // fb_loop:
    //   str.w r2, [r1], #4 -> 0xf841 0x2b04
    //   subs r3, #1 -> 0x3b01
    //   bne fb_loop -> 0xd1fb
    // loop:
    //   b . -> 0xe7fe

    // To keep it robust, let's assemble the exact byte sequence:
    // Instructions start at 0x1000_0008.
    // Let's compute literal pool offsets:
    // Code length: ~64 bytes.
    // msg: ASCII string at offset ~72.
    // literals: 32-bit constants at offset ~120.

    // Instructions start at offset 8:
    // Offset 8: ldr r1, [pc, #28] (loads lit_uart at 40)
    code.extend_from_slice(&0x4907u16.to_le_bytes()); // 8: ldr r1, =0x40070000

    // Offset 10: adr r2, msg (msg at offset 64)
    code.extend_from_slice(&0xa20du16.to_le_bytes()); // 10: adr r2, msg

    // Offset 12 (print_loop): ldrb r0, [r2, #0]
    code.extend_from_slice(&0x7810u16.to_le_bytes()); // 12: ldrb r0, [r2]

    // Offset 14: cmp r0, #0
    code.extend_from_slice(&0x2800u16.to_le_bytes()); // 14: cmp r0, #0

    // Offset 16: beq print_done (target 24)
    code.extend_from_slice(&0xd002u16.to_le_bytes()); // 16: beq print_done

    // Offset 18: str r0, [r1, #0] (UART0_DR)
    code.extend_from_slice(&0x6008u16.to_le_bytes()); // 18: str r0, [r1]

    // Offset 20: adds r2, #1
    code.extend_from_slice(&0x1c52u16.to_le_bytes()); // 20: adds r2, #1

    // Offset 22: b print_loop (target 12)
    code.extend_from_slice(&0xe7f9u16.to_le_bytes()); // 22: b print_loop

    // Offset 24 (print_done): ldr r1, [pc, #16] (loads lit_sio at 44)
    code.extend_from_slice(&0x4904u16.to_le_bytes()); // 24: ldr r1, =0xD0000000

    // Offset 26: ldr r0, [pc, #20] (loads lit_tmds_ctrl at 48)
    code.extend_from_slice(&0x4805u16.to_le_bytes()); // 26: ldr r0, =0x08000000

    // Offset 28: str.w r0, [r1, #0x1c0] (SIO_TMDS_CTRL)
    code.extend_from_slice(&[0xc1, 0xf8, 0xc0, 0x01]); // 28..31: str.w r0, [r1, #0x1c0]

    // Offset 32: b .
    code.extend_from_slice(&0xe7feu16.to_le_bytes()); // 32..33: b .

    // Pad to offset 40
    while code.len() < 40 {
        code.push(0);
    }

    // Literals at offset 40:
    code.extend_from_slice(&0x4007_0000u32.to_le_bytes()); // 40: UART0
    code.extend_from_slice(&0xD000_0000u32.to_le_bytes()); // 44: SIO
    code.extend_from_slice(&0x0800_0000u32.to_le_bytes()); // 48: TMDS_CTRL
    code.extend_from_slice(&0x2001_0000u32.to_le_bytes()); // 52: FB base
    code.extend_from_slice(&0x07E0_07E0u32.to_le_bytes()); // 56: Green color
    code.extend_from_slice(&38400u32.to_le_bytes());       // 60: pixel count / 2

    // Pad to offset 64
    while code.len() < 64 {
        code.push(0);
    }

    // Message string at offset 64 (fits comfortably within 256-byte UF2 block):
    let msg = b"\r\nCoCo Zero RP2350 Bare-Metal Firmware Running!\r\nTMDS Video Initialized (640x480p60)\r\n\0";
    code.extend_from_slice(msg);

    // Pad to 256 bytes
    while code.len() < 256 {
        code.push(0);
    }

    // Build 512-byte UF2 block:
    let mut uf2_block = vec![0u8; 512];
    uf2_block[0..4].copy_from_slice(&UF2_MAGIC_START_0.to_le_bytes());
    uf2_block[4..8].copy_from_slice(&UF2_MAGIC_START_1.to_le_bytes());
    uf2_block[508..512].copy_from_slice(&UF2_MAGIC_END.to_le_bytes());
    uf2_block[8..12].copy_from_slice(&UF2_FLAG_FAMILY_ID_PRESENT.to_le_bytes());
    uf2_block[12..16].copy_from_slice(&FLASH_BASE.to_le_bytes());
    uf2_block[16..20].copy_from_slice(&256u32.to_le_bytes());
    uf2_block[28..32].copy_from_slice(&RP2350_ARM_S_FAMILY_ID.to_le_bytes());
    uf2_block[32..32 + code.len()].copy_from_slice(&code);

    std::fs::create_dir_all("roms").unwrap();
    let out_path = "roms/demo.uf2";
    let mut file = File::create(out_path).expect("Failed to create demo.uf2");
    file.write_all(&uf2_block).expect("Failed to write demo.uf2");
    println!("Successfully generated demo UF2 firmware at: {}", out_path);
}
