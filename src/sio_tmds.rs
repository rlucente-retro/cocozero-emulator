//! RP2350 SIO TMDS (Transition-Minimized Differential Signaling) Hardware Encoder.
//!
//! Emulates the hardware TMDS encoder in the RP2350 SIO block:
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

/// Perform DVI 1.0 standard 8-to-10 bit TMDS encoding with running DC disparity.
pub fn tmds_encode_byte(d: u8, balance: &mut i32) -> u32 {
    let ones_d = d.count_ones();
    let mut q_m: u32;

    if ones_d > 4 || (ones_d == 4 && (d & 1) == 0) {
        // XNOR transition minimization
        q_m = (d as u32) & 1;
        for i in 1..8 {
            let bit_d = ((d >> i) & 1) as u32;
            let prev = (q_m >> (i - 1)) & 1;
            q_m |= (!(prev ^ bit_d) & 1) << i;
        }
    } else {
        // XOR transition minimization
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

    /// Number of bits to shift WDATA on POP.
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

    /// Extract an 8-bit color channel for a given lane (0, 1, 2) from a 32-bit color word.
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

        // Expand nbits to 8-bit intensity
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

    /// Generate two TMDS symbols for a lane (packed into lower 20 bits of a 32-bit word).
    fn encode_double(&mut self, lane: usize) -> u32 {
        let pix2_noshift = (self.ctrl & SIO_TMDS_CTRL_PIX2_NOSHIFT) != 0;
        let c0 = self.extract_channel(self.wdata, lane);
        let sym0 = tmds_encode_byte(c0, &mut self.balance[lane]);

        let c1 = if pix2_noshift {
            c0 // Same pixel repeated (horizontal pixel doubling)
        } else {
            let shift = self.shift_amount();
            let shifted_wdata = self.wdata >> shift;
            self.extract_channel(shifted_wdata, lane)
        };
        let sym1 = tmds_encode_byte(c1, &mut self.balance[lane]);

        (sym1 << 10) | (sym0 & 0x3FF)
    }

    /// Perform pop shift on WDATA.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tmds_encode_byte_basic() {
        let mut balance = 0;
        let sym = tmds_encode_byte(0x00, &mut balance);
        assert_eq!(sym & 0x3FF, sym, "Symbol must be 10 bits");

        let sym2 = tmds_encode_byte(0xFF, &mut balance);
        assert_eq!(sym2 & 0x3FF, sym2);
    }

    #[test]
    fn test_sio_tmds_rgb565_doubled() {
        let mut tmds = SioTmds::new();
        // RGB565 configuration used by libdvi:
        // (13 << L0_ROT) | (4 << L0_NBITS) |
        // ( 3 << L1_ROT) | (5 << L1_NBITS) |
        // ( 8 << L2_ROT) | (4 << L2_NBITS) |
        // ( 5 << PIX_SHIFT) | PIX2_NOSHIFT
        let rgb565_ctrl = (13 << 0)
            | (4 << 12)
            | (3 << 4)
            | (5 << 15)
            | (8 << 8)
            | (4 << 18)
            | (5 << 24)
            | SIO_TMDS_CTRL_PIX2_NOSHIFT;

        tmds.write32(SIO_TMDS_CTRL_OFFSET, rgb565_ctrl);
        assert_eq!(tmds.ctrl, rgb565_ctrl);

        // White pixel (0xFFFF) in low 16 bits, Black pixel (0x0000) in high 16 bits
        tmds.write32(SIO_TMDS_WDATA_OFFSET, 0x0000_FFFF);

        // Read double symbols
        let l0 = tmds.read32(SIO_TMDS_PEEK_DOUBLE_L0_OFFSET);
        let l1 = tmds.read32(SIO_TMDS_PEEK_DOUBLE_L1_OFFSET);
        let l2 = tmds.read32(SIO_TMDS_POP_DOUBLE_L2_OFFSET); // pop shifts WDATA by 16 bits

        assert_ne!(l0, 0);
        assert_ne!(l1, 0);
        assert_ne!(l2, 0);

        // After pop_double_l2, WDATA should now be shifted to 0x0000
        assert_eq!(tmds.wdata, 0x0000);
    }
}
