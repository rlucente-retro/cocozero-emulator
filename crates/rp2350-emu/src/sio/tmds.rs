//! RP2350 SIO TMDS (Transition-Minimized Differential Signaling) Hardware Encoder.
//!
//! §3.1.8 of RP2350 datasheet:
//! Hardware acceleration for DVI/HDMI 1.0 TMDS pixel encoding:
//! - TMDS_CTRL            (0x1C0)
//! - TMDS_WDATA           (0x1C4)
//! - TMDS_PEEK_SINGLE     (0x1C8)
//! - TMDS_POP_SINGLE      (0x1CC)
//! - TMDS_PEEK_DOUBLE_L0  (0x1D0)
//! - TMDS_POP_DOUBLE_L0   (0x1D4)
//! - TMDS_PEEK_DOUBLE_L1  (0x1D8)
//! - TMDS_POP_DOUBLE_L1   (0x1DC)
//! - TMDS_PEEK_DOUBLE_L2  (0x1E0)
//! - TMDS_POP_DOUBLE_L2   (0x1E4)

pub const SIO_TMDS_CTRL_OFFSET: u32 = 0x1C0;
pub const SIO_TMDS_WDATA_OFFSET: u32 = 0x1C4;
pub const SIO_TMDS_PEEK_SINGLE_OFFSET: u32 = 0x1C8;
pub const SIO_TMDS_POP_SINGLE_OFFSET: u32 = 0x1CC;
pub const SIO_TMDS_PEEK_DOUBLE_L0_OFFSET: u32 = 0x1D0;
pub const SIO_TMDS_POP_DOUBLE_L0_OFFSET: u32 = 0x1D4;
pub const SIO_TMDS_PEEK_DOUBLE_L1_OFFSET: u32 = 0x1D8;
pub const SIO_TMDS_POP_DOUBLE_L1_OFFSET: u32 = 0x1DC;
pub const SIO_TMDS_PEEK_DOUBLE_L2_OFFSET: u32 = 0x1E0;
pub const SIO_TMDS_POP_DOUBLE_L2_OFFSET: u32 = 0x1E4;

pub const SIO_TMDS_CTRL_CLEAR_BALANCE: u32 = 1 << 28;
pub const SIO_TMDS_CTRL_PIX2_NOSHIFT: u32 = 1 << 27;

/// DVI 1.0 standard 8-to-10 bit TMDS encoding with running DC disparity.
pub fn tmds_encode_byte(d: u8, balance: &mut i32) -> u32 {
    let ones_d = d.count_ones();
    let mut q_m: u32;

    if ones_d > 4 || (ones_d == 4 && (d & 1) == 0) {
        q_m = (d as u32) & 1;
        for i in 1..8 {
            let bit_d = ((d >> i) & 1) as u32;
            let prev = (q_m >> (i - 1)) & 1;
            q_m |= (!(prev ^ bit_d) & 1) << i;
        }
    } else {
        q_m = (d as u32) & 1;
        for i in 1..8 {
            let bit_d = ((d >> i) & 1) as u32;
            let prev = (q_m >> (i - 1)) & 1;
            q_m |= ((prev ^ bit_d) & 1) << i;
        }
        q_m |= 1 << 8;
    }

    let ones_qm = (q_m & 0xFF).count_ones() as i32;
    let zeros_qm = 8 - ones_qm;
    let diff = ones_qm - zeros_qm;
    let bit8 = (q_m >> 8) & 1;

    let q_out: u32;
    if *balance == 0 || ones_qm == 4 {
        let bit9 = (!bit8) & 1;
        let data_bits = if bit8 != 0 { q_m & 0xFF } else { (!q_m) & 0xFF };
        q_out = (bit9 << 9) | (bit8 << 8) | data_bits;
        if bit8 == 0 {
            *balance -= diff;
        } else {
            *balance += diff;
        }
    } else if (*balance > 0 && diff > 0) || (*balance < 0 && diff < 0) {
        let bit9 = 1u32;
        let data_bits = (!q_m) & 0xFF;
        q_out = (bit9 << 9) | (bit8 << 8) | data_bits;
        *balance += (2 * bit8 as i32) - diff;
    } else {
        let bit9 = 0u32;
        let data_bits = q_m & 0xFF;
        q_out = (bit9 << 9) | (bit8 << 8) | data_bits;
        *balance -= (2 * ((!bit8 & 1) as i32)) - diff;
    }

    q_out
}

#[derive(Debug, Clone, Default)]
pub struct SioTmds {
    pub ctrl: u32,
    pub wdata: u32,
    pub balance: [i32; 3], // Lane 0 (Blue), Lane 1 (Green), Lane 2 (Red)
}

impl SioTmds {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.ctrl = 0;
        self.wdata = 0;
        self.balance = [0; 3];
    }

    fn shift_amount(&self) -> u32 {
        let code = (self.ctrl >> 24) & 0x7;
        match code {
            0 => 0,
            1 => 1,
            2 => 2,
            3 => 4,
            4 => 8,
            5 => 16,
            _ => 0,
        }
    }

    fn extract_channel(&self, wdata: u32, lane: usize) -> u8 {
        let rot = match lane {
            0 => self.ctrl & 0xF,
            1 => (self.ctrl >> 4) & 0xF,
            2 => (self.ctrl >> 8) & 0xF,
            _ => 0,
        };
        let nbits = match lane {
            0 => ((self.ctrl >> 12) & 0x7) + 1,
            1 => ((self.ctrl >> 15) & 0x7) + 1,
            2 => ((self.ctrl >> 18) & 0x7) + 1,
            _ => 8,
        };
        let mask = if nbits >= 32 { u32::MAX } else { (1u32 << nbits) - 1 };
        let rotated = wdata.rotate_right(rot);
        let raw = rotated & mask;

        if nbits >= 8 {
            (raw & 0xFF) as u8
        } else if nbits == 0 {
            0
        } else {
            let shift = 8 - nbits;
            let high = (raw << shift) as u8;
            let low = (raw >> (nbits.saturating_sub(shift))) as u8;
            high | low
        }
    }

    fn encode_double(&mut self, lane: usize) -> u32 {
        let pix2_noshift = (self.ctrl & SIO_TMDS_CTRL_PIX2_NOSHIFT) != 0;
        let c0 = self.extract_channel(self.wdata, lane);
        let sym0 = tmds_encode_byte(c0, &mut self.balance[lane]);

        let c1 = if pix2_noshift {
            c0
        } else {
            let shift = self.shift_amount();
            let shifted_wdata = self.wdata >> shift;
            self.extract_channel(shifted_wdata, lane)
        };
        let sym1 = tmds_encode_byte(c1, &mut self.balance[lane]);

        (sym1 << 10) | (sym0 & 0x3FF)
    }

    fn do_pop_shift(&mut self) {
        let shift = self.shift_amount();
        self.wdata = self.wdata.wrapping_shr(shift);
    }

    pub fn read32(&mut self, offset: u32) -> u32 {
        match offset {
            SIO_TMDS_CTRL_OFFSET => self.ctrl,
            SIO_TMDS_WDATA_OFFSET => self.wdata,
            SIO_TMDS_PEEK_SINGLE_OFFSET => {
                let c = self.extract_channel(self.wdata, 0);
                tmds_encode_byte(c, &mut self.balance[0]) & 0x3FF
            }
            SIO_TMDS_POP_SINGLE_OFFSET => {
                let c = self.extract_channel(self.wdata, 0);
                let sym = tmds_encode_byte(c, &mut self.balance[0]) & 0x3FF;
                self.do_pop_shift();
                sym
            }
            SIO_TMDS_PEEK_DOUBLE_L0_OFFSET => self.encode_double(0),
            SIO_TMDS_POP_DOUBLE_L0_OFFSET => {
                let val = self.encode_double(0);
                self.do_pop_shift();
                val
            }
            SIO_TMDS_PEEK_DOUBLE_L1_OFFSET => self.encode_double(1),
            SIO_TMDS_POP_DOUBLE_L1_OFFSET => {
                let val = self.encode_double(1);
                self.do_pop_shift();
                val
            }
            SIO_TMDS_PEEK_DOUBLE_L2_OFFSET => self.encode_double(2),
            SIO_TMDS_POP_DOUBLE_L2_OFFSET => {
                let val = self.encode_double(2);
                self.do_pop_shift();
                val
            }
            _ => 0,
        }
    }

    pub fn write32(&mut self, offset: u32, val: u32) {
        match offset {
            SIO_TMDS_CTRL_OFFSET => {
                self.ctrl = val & !SIO_TMDS_CTRL_CLEAR_BALANCE;
                if (val & SIO_TMDS_CTRL_CLEAR_BALANCE) != 0 {
                    self.balance = [0; 3];
                }
            }
            SIO_TMDS_WDATA_OFFSET => {
                self.wdata = val;
            }
            _ => {}
        }
    }
}
