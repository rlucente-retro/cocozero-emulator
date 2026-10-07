//! MicroSD card SPI protocol emulator.
//!
//! Emulates an SDHC/SDXC card connected via SPI (Mode 0):
//! - CMD0:  GO_IDLE_STATE
//! - CMD8:  SEND_IF_COND
//! - CMD55: APP_CMD
//! - ACMD41: SD_SEND_OP_COND
//! - CMD58: READ_OCR
//! - CMD16: SET_BLOCKLEN
//! - CMD17: READ_SINGLE_BLOCK
//! - CMD18: READ_MULTIPLE_BLOCK
//! - CMD12: STOP_TRANSMISSION
//! - CMD24: WRITE_SINGLE_BLOCK

use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

pub const SD_SECTOR_SIZE: usize = 512;

#[derive(Debug, PartialEq, Eq)]
#[allow(dead_code)]
enum SdState {

    PowerOn,
    Idle,
    Ready,
    ReadingSingleBlock { sector: u64, offset: usize },
    ReadingMultiBlock { sector: u64, offset: usize },
    WritingSingleBlock { sector: u64, buf: Vec<u8> },
}

/// Compute standard SD CRC16 (CRC-16-CCITT / IBM-3740: poly 0x1021, init 0x0000).
pub fn sd_crc16(data: &[u8]) -> u16 {
    let mut crc = 0u16;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

pub struct SpiSdCard {
    state: SdState,
    app_cmd_next: bool,
    cmd_buf: Vec<u8>,
    rx_queue: VecDeque<u8>,
    backing: Box<dyn SectorStorage + Send>,
    cs_asserted: bool,
    pub cmd_history: Vec<(u8, u32)>,
    curr_sector_buf: [u8; SD_SECTOR_SIZE],
    curr_sector_crc: u16,
}

pub trait SectorStorage: Send {
    fn num_sectors(&self) -> u64;
    fn read_sector(&mut self, sector: u64, out: &mut [u8; SD_SECTOR_SIZE]) -> bool;
    fn write_sector(&mut self, sector: u64, data: &[u8; SD_SECTOR_SIZE]) -> bool;
}

pub struct MemoryStorage {
    data: Vec<u8>,
}

impl MemoryStorage {
    pub fn new(size_bytes: usize) -> Self {
        Self {
            data: vec![0; size_bytes],
        }
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { data: bytes }
    }
}

impl SectorStorage for MemoryStorage {
    fn num_sectors(&self) -> u64 {
        (self.data.len() / SD_SECTOR_SIZE) as u64
    }

    fn read_sector(&mut self, sector: u64, out: &mut [u8; SD_SECTOR_SIZE]) -> bool {
        let offset = (sector as usize) * SD_SECTOR_SIZE;
        if offset + SD_SECTOR_SIZE <= self.data.len() {
            out.copy_from_slice(&self.data[offset..offset + SD_SECTOR_SIZE]);
            true
        } else {
            false
        }
    }

    fn write_sector(&mut self, sector: u64, data: &[u8; SD_SECTOR_SIZE]) -> bool {
        let offset = (sector as usize) * SD_SECTOR_SIZE;
        if offset + SD_SECTOR_SIZE <= self.data.len() {
            self.data[offset..offset + SD_SECTOR_SIZE].copy_from_slice(data);
            true
        } else {
            false
        }
    }
}

pub struct FileStorage {
    file: File,
    num_sectors: u64,
}

impl FileStorage {
    pub fn open<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let mut file = File::options().read(true).write(true).open(path)?;
        let len = file.seek(SeekFrom::End(0))?;
        Ok(Self {
            file,
            num_sectors: len / SD_SECTOR_SIZE as u64,
        })
    }
}

impl SectorStorage for FileStorage {
    fn num_sectors(&self) -> u64 {
        self.num_sectors
    }

    fn read_sector(&mut self, sector: u64, out: &mut [u8; SD_SECTOR_SIZE]) -> bool {
        if sector >= self.num_sectors {
            return false;
        }
        let pos = sector * SD_SECTOR_SIZE as u64;
        if self.file.seek(SeekFrom::Start(pos)).is_err() {
            return false;
        }
        self.file.read_exact(out).is_ok()
    }

    fn write_sector(&mut self, sector: u64, data: &[u8; SD_SECTOR_SIZE]) -> bool {
        if sector >= self.num_sectors {
            return false;
        }
        let pos = sector * SD_SECTOR_SIZE as u64;
        if self.file.seek(SeekFrom::Start(pos)).is_err() {
            return false;
        }
        self.file.write_all(data).is_ok()
    }
}

impl SpiSdCard {
    pub fn new(storage: Box<dyn SectorStorage + Send>) -> Self {
        Self {
            state: SdState::PowerOn,
            app_cmd_next: false,
            cmd_buf: Vec::with_capacity(6),
            rx_queue: VecDeque::new(),
            backing: storage,
            cs_asserted: true,
            cmd_history: Vec::new(),
            curr_sector_buf: [0u8; SD_SECTOR_SIZE],
            curr_sector_crc: 0,
        }
    }

    pub fn set_cs(&mut self, asserted: bool) {
        self.cs_asserted = asserted;
        if !asserted {
            self.cmd_buf.clear();
        }
    }

    fn load_sector(&mut self, sector: u64) {
        let mut buf = [0u8; SD_SECTOR_SIZE];
        self.backing.read_sector(sector, &mut buf);
        self.curr_sector_crc = sd_crc16(&buf);
        self.curr_sector_buf = buf;
    }

    fn read_cached_stream_byte(&self, offset: usize) -> u8 {
        if offset == 0 {
            0xFE // Data start token
        } else if offset <= SD_SECTOR_SIZE {
            self.curr_sector_buf[offset - 1]
        } else if offset == SD_SECTOR_SIZE + 1 {
            (self.curr_sector_crc >> 8) as u8
        } else {
            (self.curr_sector_crc & 0xFF) as u8
        }
    }

    /// Transfer one byte over SPI full-duplex (MOSI -> MISO).
    pub fn transfer_byte(&mut self, mosi: u8) -> u8 {
        if !self.cs_asserted {
            return 0xFF;
        }

        // Host sending a command (starts with 01xxxxxx, 0x40..=0x7F)
        if !self.cmd_buf.is_empty() || (mosi & 0xC0) == 0x40 {
            if self.cmd_buf.is_empty() {
                // If a new command begins, terminate any in-progress streaming read
                if matches!(self.state, SdState::ReadingSingleBlock { .. } | SdState::ReadingMultiBlock { .. }) {
                    self.state = SdState::Ready;
                }
            }
            self.cmd_buf.push(mosi);
            if self.cmd_buf.len() == 6 {
                let cmd = self.cmd_buf[0] & 0x3F;
                let arg = u32::from_be_bytes(self.cmd_buf[1..5].try_into().unwrap());
                self.cmd_buf.clear();
                self.process_command(cmd, arg);
            }
            return 0xFF;
        }

        // Return queued response bytes if available
        if let Some(resp) = self.rx_queue.pop_front() {
            return resp;
        }

        // Handle streaming data states
        match self.state {
            SdState::ReadingSingleBlock { sector, offset } => {
                let ret = self.read_cached_stream_byte(offset);
                let next_offset = offset + 1;
                if next_offset >= 1 + SD_SECTOR_SIZE + 2 {
                    self.state = SdState::Ready;
                } else {
                    self.state = SdState::ReadingSingleBlock { sector, offset: next_offset };
                }
                ret
            }
            SdState::ReadingMultiBlock { sector, offset } => {
                let ret = self.read_cached_stream_byte(offset);
                let next_offset = offset + 1;
                if next_offset >= 1 + SD_SECTOR_SIZE + 2 {
                    let next_sec = sector + 1;
                    self.load_sector(next_sec);
                    self.state = SdState::ReadingMultiBlock { sector: next_sec, offset: 0 };
                } else {
                    self.state = SdState::ReadingMultiBlock { sector, offset: next_offset };
                }
                ret
            }
            _ => 0xFF,
        }
    }

    fn process_command(&mut self, cmd: u8, arg: u32) {
        self.cmd_history.push((cmd, arg));
        if self.app_cmd_next {
            self.app_cmd_next = false;
            match cmd {
                41 => {
                    // ACMD41: SD_SEND_OP_COND
                    self.state = SdState::Ready;
                    self.rx_queue.push_back(0x00); // Ready (not idle)
                }
                42 => {
                    // ACMD42: SET_CLR_CARD_DETECT
                    self.rx_queue.push_back(0x00);
                }
                _ => {
                    self.rx_queue.push_back(0x00);
                }
            }
            return;
        }

        match cmd {
            0 => {
                // CMD0: GO_IDLE_STATE
                self.state = SdState::Idle;
                self.rx_queue.push_back(0x01); // In idle state
            }
            8 => {
                // CMD8: SEND_IF_COND (VHS = 2.7-3.6V, Check pattern = 0xAA)
                self.rx_queue.push_back(0x01); // In idle state
                self.rx_queue.push_back(0x00);
                self.rx_queue.push_back(0x00);
                self.rx_queue.push_back(0x01); // 2.7-3.6V accepted
                self.rx_queue.push_back((arg & 0xFF) as u8); // Echo check pattern (0xAA)
            }
            9 => {
                // CMD9: SEND_CSD
                self.rx_queue.push_back(0x00); // R1 success
                self.rx_queue.push_back(0xFE); // Data token
                let c_size = ((self.backing.num_sectors() / 1024).saturating_sub(1) & 0x3F_FFFF) as u32;
                let mut csd = [0u8; 16];
                csd[0] = 0x40; // CSD v2.0 (SDHC)
                csd[1] = 0x0E;
                csd[2] = 0x00;
                csd[3] = 0x32;
                csd[4] = 0x5B;
                csd[5] = 0x59;
                csd[6] = 0x00;
                csd[7] = ((c_size >> 16) & 0x3F) as u8;
                csd[8] = ((c_size >> 8) & 0xFF) as u8;
                csd[9] = (c_size & 0xFF) as u8;
                csd[10] = 0x7F;
                csd[11] = 0x80;
                csd[12] = 0x0A;
                csd[13] = 0x40;
                csd[14] = 0x40;
                csd[15] = 0x01;
                let crc = sd_crc16(&csd);
                self.rx_queue.extend(csd);
                self.rx_queue.push_back((crc >> 8) as u8);
                self.rx_queue.push_back((crc & 0xFF) as u8);
            }
            10 => {
                // CMD10: SEND_CID
                self.rx_queue.push_back(0x00);
                self.rx_queue.push_back(0xFE);
                let mut cid = [0u8; 16];
                cid[0] = 0x03;
                cid[1] = b'S';
                cid[2] = b'D';
                cid[3..8].copy_from_slice(b"COCO0");
                cid[8] = 0x10;
                cid[9..13].copy_from_slice(&[0x12, 0x34, 0x56, 0x78]);
                cid[13] = 0x00;
                cid[14] = 0x24;
                cid[15] = 0x01;
                let crc = sd_crc16(&cid);
                self.rx_queue.extend(cid);
                self.rx_queue.push_back((crc >> 8) as u8);
                self.rx_queue.push_back((crc & 0xFF) as u8);
            }
            12 => {
                // CMD12: STOP_TRANSMISSION
                self.state = SdState::Ready;
                self.rx_queue.push_back(0xFF); // Stuff byte discard
                self.rx_queue.push_back(0x00); // R1 success
            }
            16 => {
                // CMD16: SET_BLOCKLEN
                self.rx_queue.push_back(0x00);
            }
            17 => {
                // CMD17: READ_SINGLE_BLOCK
                let sector = arg as u64; // SDHC/SDXC uses block address
                self.load_sector(sector);
                self.rx_queue.push_back(0x00); // R1 success
                self.state = SdState::ReadingSingleBlock { sector, offset: 0 };
            }
            18 => {
                // CMD18: READ_MULTIPLE_BLOCK
                let sector = arg as u64;
                self.load_sector(sector);
                self.rx_queue.push_back(0x00); // R1 success
                self.state = SdState::ReadingMultiBlock { sector, offset: 0 };
            }
            55 => {
                // CMD55: APP_CMD
                self.app_cmd_next = true;
                let r1 = if self.state == SdState::Idle { 0x01 } else { 0x00 };
                self.rx_queue.push_back(r1);
            }
            58 => {
                // CMD58: READ_OCR
                let r1 = if self.state == SdState::Idle { 0x01 } else { 0x00 };
                self.rx_queue.push_back(r1);
                // OCR: Card power up status bit (31) = 1, CCS bit (30) = 1 (SDHC/SDXC), VDD = 3.2-3.4V
                self.rx_queue.push_back(0xC0);
                self.rx_queue.push_back(0xFF);
                self.rx_queue.push_back(0x80);
                self.rx_queue.push_back(0x00);
            }
            59 => {
                // CMD59: CRC_ON_OFF
                self.rx_queue.push_back(0x00);
            }
            _ => {
                self.rx_queue.push_back(0x00); // Default success
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spi_sd_initialization_sequence() {
        let storage = Box::new(MemoryStorage::new(1024 * 1024));
        let mut sd = SpiSdCard::new(storage);
        sd.set_cs(true);

        // 1. Send CMD0 (0x40, 0x00, 0x00, 0x00, 0x00, 0x95)
        let cmd0 = [0x40, 0x00, 0x00, 0x00, 0x00, 0x95];
        for &b in &cmd0 {
            sd.transfer_byte(b);
        }
        let resp = sd.transfer_byte(0xFF);
        assert_eq!(resp, 0x01, "CMD0 should return 0x01 (in idle state)");

        // 2. Send CMD8 (0x48, 0x00, 0x00, 0x01, 0xAA, 0x87)
        let cmd8 = [0x48, 0x00, 0x00, 0x01, 0xAA, 0x87];
        for &b in &cmd8 {
            sd.transfer_byte(b);
        }
        let mut r7 = Vec::new();
        for _ in 0..5 {
            r7.push(sd.transfer_byte(0xFF));
        }
        assert_eq!(r7[0], 0x01); // R1
        assert_eq!(r7[4], 0xAA); // Check pattern echo

        // 3. Send CMD55 + ACMD41
        let cmd55 = [0x77, 0x00, 0x00, 0x00, 0x00, 0x65];
        for &b in &cmd55 {
            sd.transfer_byte(b);
        }
        let resp = sd.transfer_byte(0xFF);
        assert_eq!(resp, 0x01);

        let acmd41 = [0x69, 0x40, 0x00, 0x00, 0x00, 0x77];
        for &b in &acmd41 {
            sd.transfer_byte(b);
        }
        let resp = sd.transfer_byte(0xFF);
        assert_eq!(resp, 0x00, "ACMD41 should transition card to ready (0x00)");
    }

    #[test]
    fn test_spi_sd_block_read() {
        let mut mem = MemoryStorage::new(1024 * 1024);
        let test_sector = [0x42u8; SD_SECTOR_SIZE];
        mem.write_sector(5, &test_sector);

        let mut sd = SpiSdCard::new(Box::new(mem));
        sd.set_cs(true);
        sd.state = SdState::Ready;

        // Send CMD17 for sector 5
        let cmd17 = [0x51, 0x00, 0x00, 0x00, 0x05, 0xFF];
        for &b in &cmd17 {
            sd.transfer_byte(b);
        }
        let resp = sd.transfer_byte(0xFF);
        assert_eq!(resp, 0x00, "CMD17 should return R1 0x00");

        // Read data token
        let token = sd.transfer_byte(0xFF);
        assert_eq!(token, 0xFE, "Data start token should be 0xFE");

        // Read 512 bytes
        let mut read_data = Vec::new();
        for _ in 0..512 {
            read_data.push(sd.transfer_byte(0xFF));
        }
        assert_eq!(read_data, vec![0x42u8; 512]);

        // Read 2 CRC bytes and verify they match CRC16
        let crc1 = sd.transfer_byte(0xFF);
        let crc2 = sd.transfer_byte(0xFF);
        let expected_crc = sd_crc16(&test_sector);
        assert_eq!(((crc1 as u16) << 8) | (crc2 as u16), expected_crc);
    }
}
