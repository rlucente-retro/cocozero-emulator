//! UF2 (USB Flashing Format) parser for Raspberry Pi RP2350 / RP2040 binaries.
//!
//! UF2 files consist of 512-byte blocks:
//! - 32-byte header
//! - 476-byte data payload area (typically 256 bytes used)
//! - 4-byte magic end marker

pub const UF2_MAGIC_START_0: u32 = 0x0A324655; // "UF2\n"
pub const UF2_MAGIC_START_1: u32 = 0x9E5D5157;
pub const UF2_MAGIC_END: u32 = 0x0AB16F30;

pub const UF2_FLAG_NOT_MAIN_FLASH: u32 = 0x00000001;
pub const UF2_FLAG_FILE_CONTAINER: u32 = 0x00001000;
pub const UF2_FLAG_FAMILY_ID_PRESENT: u32 = 0x00002000;
pub const UF2_FLAG_MD5_PRESENT: u32 = 0x00004000;

// Raspberry Pi Family IDs:
pub const RP2040_FAMILY_ID: u32 = 0xe48bff56;
pub const RP2350_ARM_S_FAMILY_ID: u32 = 0xe48bff59;
pub const RP2350_RISCV_FAMILY_ID: u32 = 0xe48bff5a;
pub const RP2350_ARM_NS_FAMILY_ID: u32 = 0xe48bff5b;

pub const FLASH_BASE: u32 = 0x1000_0000;
pub const DEFAULT_FLASH_SIZE: usize = 16 * 1024 * 1024; // 16 MB on Waveshare RP2350-PiZero

#[derive(Debug, Clone)]
pub struct Uf2Block {
    pub flags: u32,
    pub target_addr: u32,
    pub payload_size: u32,
    pub block_no: u32,
    pub num_blocks: u32,
    pub family_id: Option<u32>,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct FlashImage {
    pub base_addr: u32,
    pub memory: Vec<u8>,
    pub min_addr: u32,
    pub max_addr: u32,
    pub entry_point: Option<u32>,
    pub initial_sp: Option<u32>,
}

impl FlashImage {
    pub fn new(size: usize) -> Self {
        Self {
            base_addr: FLASH_BASE,
            memory: vec![0xFF; size],
            min_addr: u32::MAX,
            max_addr: 0,
            entry_point: None,
            initial_sp: None,
        }
    }

    /// Load a raw binary image directly into flash at a specified offset.
    pub fn load_raw_bin(&mut self, offset: u32, data: &[u8]) {
        let start = offset as usize;
        let end = (start + data.len()).min(self.memory.len());
        let to_copy = end - start;
        self.memory[start..end].copy_from_slice(&data[..to_copy]);

        let addr_start = self.base_addr + offset;
        let addr_end = addr_start + to_copy as u32;
        self.min_addr = self.min_addr.min(addr_start);
        self.max_addr = self.max_addr.max(addr_end);

        self.update_vectors();
    }

    /// Update initial SP and entry point from vector table at base address.
    pub fn update_vectors(&mut self) {
        if self.memory.len() >= 8 {
            let sp = u32::from_le_bytes(self.memory[0..4].try_into().unwrap());
            let pc = u32::from_le_bytes(self.memory[4..8].try_into().unwrap());
            if sp != 0xFFFF_FFFF && pc != 0xFFFF_FFFF {
                self.initial_sp = Some(sp);
                self.entry_point = Some(pc);
            }
        }
    }
}

/// Parse a UF2 file and populate a FlashImage.
pub fn parse_uf2(data: &[u8]) -> Result<FlashImage, String> {
    if data.len() % 512 != 0 {
        return Err(format!("UF2 file size ({}) must be a multiple of 512 bytes", data.len()));
    }

    let mut flash = FlashImage::new(DEFAULT_FLASH_SIZE);
    let num_blocks = data.len() / 512;
    let mut parsed_blocks = 0;

    for i in 0..num_blocks {
        let block_slice = &data[i * 512..(i + 1) * 512];
        let magic0 = u32::from_le_bytes(block_slice[0..4].try_into().unwrap());
        let magic1 = u32::from_le_bytes(block_slice[4..8].try_into().unwrap());
        let magic_end = u32::from_le_bytes(block_slice[508..512].try_into().unwrap());

        if magic0 != UF2_MAGIC_START_0 || magic1 != UF2_MAGIC_START_1 || magic_end != UF2_MAGIC_END {
            return Err(format!("Invalid UF2 magic at block {}", i));
        }

        let flags = u32::from_le_bytes(block_slice[8..12].try_into().unwrap());
        let target_addr = u32::from_le_bytes(block_slice[12..16].try_into().unwrap());
        let payload_size = u32::from_le_bytes(block_slice[16..20].try_into().unwrap()) as usize;
        let _block_no = u32::from_le_bytes(block_slice[20..24].try_into().unwrap());
        let _num_blocks = u32::from_le_bytes(block_slice[24..28].try_into().unwrap());
        let family_or_size = u32::from_le_bytes(block_slice[28..32].try_into().unwrap());

        // Check family ID if present
        if (flags & UF2_FLAG_FAMILY_ID_PRESENT) != 0 {
            // Verify if it's RP2350 or RP2040
            let fid = family_or_size;
            if fid != RP2350_ARM_S_FAMILY_ID
                && fid != RP2350_ARM_NS_FAMILY_ID
                && fid != RP2350_RISCV_FAMILY_ID
                && fid != RP2040_FAMILY_ID
            {
                // Not necessarily fatal, but warn or note
            }
        }

        if payload_size > 476 {
            return Err(format!("Payload size {} exceeds 476 bytes at block {}", payload_size, i));
        }

        // Map target address to flash offset
        if target_addr >= FLASH_BASE {
            let offset = (target_addr - FLASH_BASE) as usize;
            if offset + payload_size <= flash.memory.len() {
                flash.memory[offset..offset + payload_size]
                    .copy_from_slice(&block_slice[32..32 + payload_size]);
                flash.min_addr = flash.min_addr.min(target_addr);
                flash.max_addr = flash.max_addr.max(target_addr + payload_size as u32);
            }
        }

        parsed_blocks += 1;
    }

    flash.update_vectors();

    if parsed_blocks == 0 {
        return Err("No valid UF2 blocks found".to_string());
    }

    Ok(flash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uf2_block_parsing() {
        let mut block = vec![0u8; 512];
        block[0..4].copy_from_slice(&UF2_MAGIC_START_0.to_le_bytes());
        block[4..8].copy_from_slice(&UF2_MAGIC_START_1.to_le_bytes());
        block[508..512].copy_from_slice(&UF2_MAGIC_END.to_le_bytes());

        // flags: family ID present
        block[8..12].copy_from_slice(&UF2_FLAG_FAMILY_ID_PRESENT.to_le_bytes());
        // target addr: FLASH_BASE
        block[12..16].copy_from_slice(&FLASH_BASE.to_le_bytes());
        // payload size: 256
        block[16..20].copy_from_slice(&256u32.to_le_bytes());
        // family ID: RP2350_ARM_S
        block[28..32].copy_from_slice(&RP2350_ARM_S_FAMILY_ID.to_le_bytes());

        // SP = 0x20082000, PC = 0x10000185
        block[32..36].copy_from_slice(&0x20082000u32.to_le_bytes());
        block[36..40].copy_from_slice(&0x10000185u32.to_le_bytes());

        let flash = parse_uf2(&block).expect("Should parse valid block");
        assert_eq!(flash.initial_sp, Some(0x20082000));
        assert_eq!(flash.entry_point, Some(0x10000185));
        assert_eq!(flash.min_addr, FLASH_BASE);
        assert_eq!(flash.max_addr, FLASH_BASE + 256);
    }
}
