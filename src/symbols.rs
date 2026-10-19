//! Dynamic firmware symbol extraction and resolution.
//!
//! Automatically discovers firmware symbol addresses (`g_fb`, `g_kb_col_row_mask`,
//! `g_ovk`, `tuh_hid_report_received_cb`) via:
//! 1. Companion ELF file (`roms/cocozero.elf` or `--elf <PATH>`)
//! 2. In-flash opcode and literal pool signature scanning on `cocozero.uf2`
//! 3. Fallback baseline defaults

use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirmwareSymbols {
    pub fb_addr: u32,
    pub kb_matrix_addr: u32,
    pub menu_active_addr: u32,
    pub menu_mode_addr: u32,
    pub menu_count_addr: u32,
    pub hid_cb_addr: u32,
    pub audio_ring_addr: u32,
    pub audio_w_addr: u32,
    pub audio_r_addr: u32,
}

impl Default for FirmwareSymbols {
    fn default() -> Self {
        Self {
            fb_addr: 0x2002_2780,
            kb_matrix_addr: 0x2000_b304,
            menu_active_addr: 0x2000_c8e8,
            menu_mode_addr: 0x2000_c8ec,
            menu_count_addr: 0x2000_c8fc,
            hid_cb_addr: 0x1001_f3a4,
            audio_ring_addr: 0x2001_80c4,
            audio_w_addr: 0x2004_b8a0,
            audio_r_addr: 0x2004_b89c,
        }
    }
}

impl FirmwareSymbols {
    /// Attempt to parse symbols from a 32-bit Little-Endian ELF binary.
    pub fn from_elf(elf_bytes: &[u8]) -> Result<Self, String> {
        if elf_bytes.len() < 52 || &elf_bytes[..4] != b"\x7fELF" {
            return Err("Not a valid ELF file".to_string());
        }
        if elf_bytes[4] != 1 || elf_bytes[5] != 1 {
            return Err("Only 32-bit little-endian ELF supported".to_string());
        }

        let e_shoff = u32::from_le_bytes(elf_bytes[32..36].try_into().unwrap()) as usize;
        let e_shentsize = u16::from_le_bytes(elf_bytes[46..48].try_into().unwrap()) as usize;
        let e_shnum = u16::from_le_bytes(elf_bytes[48..50].try_into().unwrap()) as usize;
        let _e_shstrndx = u16::from_le_bytes(elf_bytes[50..52].try_into().unwrap()) as usize;

        if e_shentsize < 40 || e_shoff + e_shnum * e_shentsize > elf_bytes.len() {
            return Err("Invalid section header table".to_string());
        }

        struct Section {
            type_: u32,
            offset: usize,
            size: usize,
            link: usize,
            entsize: usize,
        }

        let mut sections = Vec::with_capacity(e_shnum);
        for i in 0..e_shnum {
            let off = e_shoff + i * e_shentsize;
            let sh = &elf_bytes[off..off + 40];
            let sh_type = u32::from_le_bytes(sh[4..8].try_into().unwrap());
            let sh_offset = u32::from_le_bytes(sh[16..20].try_into().unwrap()) as usize;
            let sh_size = u32::from_le_bytes(sh[20..24].try_into().unwrap()) as usize;
            let sh_link = u32::from_le_bytes(sh[24..28].try_into().unwrap()) as usize;
            let sh_entsize = u32::from_le_bytes(sh[36..40].try_into().unwrap()) as usize;
            sections.push(Section {
                type_: sh_type,
                offset: sh_offset,
                size: sh_size,
                link: sh_link,
                entsize: sh_entsize,
            });
        }

        let symtab_sec = sections.iter().find(|s| s.type_ == 2) // SHT_SYMTAB = 2
            .ok_or_else(|| "Symbol table not found in ELF".to_string())?;

        if symtab_sec.link >= sections.len() {
            return Err("Invalid string table link in symtab".to_string());
        }
        let strtab_sec = &sections[symtab_sec.link];

        let symtab_bytes = &elf_bytes[symtab_sec.offset..symtab_sec.offset + symtab_sec.size];
        let strtab_bytes = &elf_bytes[strtab_sec.offset..strtab_sec.offset + strtab_sec.size];

        let entsize = if symtab_sec.entsize >= 16 { symtab_sec.entsize } else { 16 };
        let count = symtab_bytes.len() / entsize;

        let mut syms: HashMap<String, u32> = HashMap::new();
        for i in 0..count {
            let entry = &symtab_bytes[i * entsize..(i + 1) * entsize];
            let st_name = u32::from_le_bytes(entry[0..4].try_into().unwrap()) as usize;
            let st_value = u32::from_le_bytes(entry[4..8].try_into().unwrap());

            if st_name != 0 && st_name < strtab_bytes.len() {
                let end = strtab_bytes[st_name..]
                    .iter()
                    .position(|&b| b == 0)
                    .map(|p| st_name + p)
                    .unwrap_or(strtab_bytes.len());
                if let Ok(name) = std::str::from_utf8(&strtab_bytes[st_name..end]) {
                    syms.insert(name.to_string(), st_value);
                }
            }
        }

        let mut res = Self::default();

        if let Some(&addr) = syms.get("_ZL4g_fb").or_else(|| syms.get("g_fb")) {
            res.fb_addr = addr;
        }
        if let Some(&addr) = syms.get("_ZL17g_kb_col_row_mask").or_else(|| syms.get("g_kb_col_row_mask")) {
            res.kb_matrix_addr = addr;
        }
        if let Some(&addr) = syms.get("_ZL5g_ovk").or_else(|| syms.get("g_ovk")) {
            res.menu_active_addr = addr;
            res.menu_mode_addr = addr + 4;
            res.menu_count_addr = addr + 20;
        }
        if let Some(&addr) = syms.get("tuh_hid_report_received_cb") {
            res.hid_cb_addr = addr & !1; // clear thumb bit
        }
        if let Some(&addr) = syms.get("_ZL12g_audio_ring").or_else(|| syms.get("g_audio_ring")) {
            res.audio_ring_addr = addr;
        }
        if let Some(&addr) = syms.get("_ZL9g_audio_w").or_else(|| syms.get("g_audio_w")) {
            res.audio_w_addr = addr;
        }
        if let Some(&addr) = syms.get("_ZL9g_audio_r").or_else(|| syms.get("g_audio_r")) {
            res.audio_r_addr = addr;
        }

        Ok(res)
    }

    /// Scan raw flash bytes for opcode and literal pool signatures.
    pub fn from_flash_signatures(flash_bytes: &[u8], base_addr: u32) -> Self {
        let mut res = Self::default();

        // 1. Scan for g_ovk: opcode pattern of disk_overlay_is_open():
        //    ldr r3, [pc, #4]; ldrb r0, [r3]; bx lr; (nop/align)
        //    Bytes: [0x01, 0x4B, 0x18, 0x78, 0x70, 0x47]
        let pat_is_open = [0x01, 0x4B, 0x18, 0x78, 0x70, 0x47];
        if let Some(pos) = flash_bytes.windows(pat_is_open.len()).position(|w| w == pat_is_open) {
            let lit_off = (pos + 8) & !3;
            if lit_off + 4 <= flash_bytes.len() {
                let g_ovk = u32::from_le_bytes(flash_bytes[lit_off..lit_off + 4].try_into().unwrap());
                if (0x2000_0000..=0x2008_0000).contains(&g_ovk) {
                    res.menu_active_addr = g_ovk;
                    res.menu_mode_addr = g_ovk + 4;
                    res.menu_count_addr = g_ovk + 20;
                }
            }
        }

        // 2. Scan for g_kb_col_row_mask: integer division constant 0xAAAAAAAB literal preceded by matrix pointer
        let pat_magic = 0xAAAA_AAABu32.to_le_bytes();
        for (i, window) in flash_bytes.windows(4).enumerate().step_by(4) {
            if window == pat_magic && i >= 4 {
                let candidate = u32::from_le_bytes(flash_bytes[i - 4..i].try_into().unwrap());
                if (0x2000_0000..=0x2008_0000).contains(&candidate) {
                    res.kb_matrix_addr = candidate;
                    break;
                }
            }
        }

        // 3. Scan for tuh_hid_report_received_cb: function prologue
        //    push.w {r4-r11, lr}; cmp r0, #15
        //    Bytes: [0x2D, 0xE9, 0xF0, 0x4F, 0x0F, 0x28]
        let pat_hid_cb = [0x2D, 0xE9, 0xF0, 0x4F, 0x0F, 0x28];
        if let Some(pos) = flash_bytes.windows(pat_hid_cb.len()).position(|w| w == pat_hid_cb) {
            res.hid_cb_addr = base_addr + pos as u32;
        }

        // 4. Scan for g_fb literal: present_card() sequence
        //    bx lr; nop; ldr r0, [pc, #4]
        //    Bytes: [0x70, 0x47, 0x00, 0xBF, 0x01, 0x48]
        let pat_present = [0x70, 0x47, 0x00, 0xBF, 0x01, 0x48];
        if let Some(pos) = flash_bytes.windows(pat_present.len()).position(|w| w == pat_present) {
            let lit_off = (pos + 12) & !3;
            if lit_off + 4 <= flash_bytes.len() {
                let candidate = u32::from_le_bytes(flash_bytes[lit_off..lit_off + 4].try_into().unwrap());
                if (0x2000_0000..=0x2008_0000).contains(&candidate) {
                    res.fb_addr = candidate;
                }
            }
        }

        // 5. Scan for audio ring buffer literal pool:
        //    Function epilogue of coco_machine_audio_read:
        //    mov r6, r2; b ...; nop
        //    Bytes: [0x16, 0x46, 0xF0, 0xE7, 0x00, 0xBF]
        let pat_audio = [0x16, 0x46, 0xF0, 0xE7, 0x00, 0xBF];
        if let Some(pos) = flash_bytes.windows(pat_audio.len()).position(|w| w == pat_audio) {
            let lit = pos + pat_audio.len();
            if lit + 16 <= flash_bytes.len() {
                let w0 = u32::from_le_bytes(flash_bytes[lit..lit + 4].try_into().unwrap());
                let w1 = u32::from_le_bytes(flash_bytes[lit + 4..lit + 8].try_into().unwrap());
                let w3 = u32::from_le_bytes(flash_bytes[lit + 12..lit + 16].try_into().unwrap());
                if (0x2000_0000..=0x2008_0000).contains(&w0)
                    && (0x2000_0000..=0x2008_0000).contains(&w1)
                    && (0x2000_0000..=0x2008_0000).contains(&w3)
                {
                    res.audio_w_addr = w0;
                    res.audio_r_addr = w1;
                    res.audio_ring_addr = w3;
                }
            }
        }

        res
    }

    /// Helper to attempt reading and parsing symbols from an ELF file.
    fn try_load_elf(path: &Path) -> Option<Self> {
        let bytes = std::fs::read(path).ok()?;
        Self::from_elf(&bytes).ok()
    }

    /// Auto-detect symbols from an optional ELF file or by signature scanning flash bytes.
    pub fn auto_detect(elf_path: Option<&Path>, flash_bytes: &[u8], base_addr: u32) -> Self {
        // 1. Try specified ELF path
        if let Some(path) = elf_path
            && let Some(syms) = Self::try_load_elf(path)
        {
            eprintln!("[Symbols] Auto-detected from {}: fb=0x{:08X}, kb=0x{:08X}, menu=0x{:08X}, hid_cb=0x{:08X}, audio_ring=0x{:08X}",
                path.display(), syms.fb_addr, syms.kb_matrix_addr, syms.menu_active_addr, syms.hid_cb_addr, syms.audio_ring_addr);
            return syms;
        }

        // 2. Try conventional roms/cocozero.elf
        let default_elf = Path::new("roms/cocozero.elf");
        if let Some(syms) = Self::try_load_elf(default_elf) {
            eprintln!("[Symbols] Auto-detected from {}: fb=0x{:08X}, kb=0x{:08X}, menu=0x{:08X}, hid_cb=0x{:08X}, audio_ring=0x{:08X}",
                default_elf.display(), syms.fb_addr, syms.kb_matrix_addr, syms.menu_active_addr, syms.hid_cb_addr, syms.audio_ring_addr);
            return syms;
        }

        // 3. Fall back to flash signature scanning
        let syms = Self::from_flash_signatures(flash_bytes, base_addr);
        eprintln!("[Symbols] Auto-detected from Flash signature scan: fb=0x{:08X}, kb=0x{:08X}, menu=0x{:08X}, hid_cb=0x{:08X}, audio_ring=0x{:08X}",
            syms.fb_addr, syms.kb_matrix_addr, syms.menu_active_addr, syms.hid_cb_addr, syms.audio_ring_addr);
        syms
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uf2::parse_uf2;
    use std::path::Path;

    #[test]
    fn test_elf_symbol_parsing() {
        let elf_path = Path::new("roms/cocozero.elf");
        if !elf_path.exists() {
            eprintln!("Skipping test_elf_symbol_parsing: roms/cocozero.elf not present");
            return;
        }

        let bytes = std::fs::read(elf_path).expect("Failed to read ELF");
        let syms = FirmwareSymbols::from_elf(&bytes).expect("Failed to parse ELF symbols");
        assert_eq!(syms.fb_addr, 0x2002_2780);
        assert_eq!(syms.kb_matrix_addr, 0x2000_b304);
        assert_eq!(syms.menu_active_addr, 0x2000_c8e8);
        assert_eq!(syms.menu_mode_addr, 0x2000_c8ec);
        assert_eq!(syms.menu_count_addr, 0x2000_c8fc);
        assert_eq!(syms.hid_cb_addr, 0x1001_f3a4);
        assert_eq!(syms.audio_ring_addr, 0x2001_80c4);
        assert_eq!(syms.audio_w_addr, 0x2004_b8a0);
        assert_eq!(syms.audio_r_addr, 0x2004_b89c);
    }

    #[test]
    fn test_flash_signature_scanning() {
        let uf2_path = Path::new("roms/cocozero.uf2");
        if !uf2_path.exists() {
            eprintln!("Skipping test_flash_signature_scanning: roms/cocozero.uf2 not present");
            return;
        }

        let uf2_data = std::fs::read(uf2_path).expect("Failed to read UF2");
        let flash = parse_uf2(&uf2_data).expect("Failed to parse UF2");
        let syms = FirmwareSymbols::from_flash_signatures(&flash.memory, crate::uf2::FLASH_BASE);

        assert_eq!(syms.kb_matrix_addr, 0x2000_b304);
        assert_eq!(syms.menu_active_addr, 0x2000_c8e8);
        assert_eq!(syms.menu_mode_addr, 0x2000_c8ec);
        assert_eq!(syms.menu_count_addr, 0x2000_c8fc);
        assert_eq!(syms.hid_cb_addr, 0x1001_f3a4);
        assert_eq!(syms.fb_addr, 0x2002_2780);
        assert_eq!(syms.audio_ring_addr, 0x2001_80c4);
        assert_eq!(syms.audio_w_addr, 0x2004_b8a0);
        assert_eq!(syms.audio_r_addr, 0x2004_b89c);
    }
}
