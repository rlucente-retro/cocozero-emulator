//! CoCo Zero Keyboard Matrix Emulation.
//!
//! Maps standard PC / USB keyboard keys and ASCII characters to the
//! Tandy Color Computer 2 keyboard matrix (8 columns x 7 rows) at SRAM address `0x2000_b304`.

use rp2350_emu::bus::Bus;
use sdl2::keyboard::Keycode;
use std::collections::VecDeque;

/// Physical SRAM address of `g_kb_col_row_mask` in the CoCo Zero firmware.
pub const KB_MATRIX_ADDR: u32 = 0x2000_b304;

// Scancodes as defined by XRoar's dkbd and key_translate.h
pub const K_0: u8 = 0x00;
pub const K_1: u8 = 0x01;
pub const K_2: u8 = 0x02;
pub const K_3: u8 = 0x03;
pub const K_4: u8 = 0x04;
pub const K_5: u8 = 0x05;
pub const K_6: u8 = 0x06;
pub const K_7: u8 = 0x07;
pub const K_8: u8 = 0x08;
pub const K_9: u8 = 0x09;
pub const K_COLON: u8 = 0x0A;
pub const K_SEMI: u8 = 0x0B;
pub const K_COMMA: u8 = 0x0C;
pub const K_MINUS: u8 = 0x0D;
pub const K_DOT: u8 = 0x0E;
pub const K_SLASH: u8 = 0x0F;
pub const K_AT: u8 = 0x10;
pub const K_A: u8 = 0x11;
pub const K_UP: u8 = 0x2B;
pub const K_DOWN: u8 = 0x2C;
pub const K_LEFT: u8 = 0x2D;
pub const K_RIGHT: u8 = 0x2E;
pub const K_SPACE: u8 = 0x2F;
pub const K_ENTER: u8 = 0x30;
pub const K_CLEAR: u8 = 0x31;
pub const K_BREAK: u8 = 0x32;
pub const K_SHIFT: u8 = 0x37;
pub const K_INVALID: u8 = 0x3F;

/// Convert dscan scancode to matrix (row, col).
///
/// Per upstream XRoar/src/dkbd.c: row = (raw_row + 4) % 6 for rows 0-5, row 6 unchanged.
pub fn dscan_to_row_col(dscan: u8) -> (u8, u8) {
    let raw_row = (dscan >> 3) & 7;
    let col = dscan & 7;
    let row = if raw_row == 6 { 6 } else { (raw_row + 4) % 6 };
    (row, col)
}

/// Convert an ASCII character to a (dscan, shift) chord.
///
/// Matches the reference key_translate.h implementation in CoCo Zero firmware.
pub fn kt_chord(c: char) -> Option<(u8, bool)> {
    let mut shift = false;
    let dscan = match c {
        'a'..='z' => K_A + (c as u8 - b'a'),
        'A'..='Z' => K_A + (c as u8 - b'A'),
        '0'..='9' => K_0 + (c as u8 - b'0'),
        ' ' => K_SPACE,
        '\r' | '\n' => K_ENTER,
        ':' => K_COLON,
        ';' => K_SEMI,
        ',' => K_COMMA,
        '-' => K_MINUS,
        '.' => K_DOT,
        '/' => K_SLASH,
        '@' => K_AT,
        '^' => K_UP,
        '!' => { shift = true; K_1 },
        '"' => { shift = true; K_2 },
        '#' => { shift = true; 0x03 },
        '$' => { shift = true; 0x04 },
        '%' => { shift = true; 0x05 },
        '&' => { shift = true; 0x06 },
        '\'' => { shift = true; 0x07 },
        '(' => { shift = true; 0x08 },
        ')' => { shift = true; 0x09 },
        '*' => { shift = true; K_COLON },
        '+' => { shift = true; K_SEMI },
        '<' => { shift = true; K_COMMA },
        '=' => { shift = true; K_MINUS },
        '>' => { shift = true; K_DOT },
        '?' => { shift = true; K_SLASH },
        '[' => { shift = true; K_DOWN },
        ']' => { shift = true; K_RIGHT },
        '\\' => { shift = true; K_CLEAR },
        '_' => { shift = true; K_UP },
        '\x08' | '\x7F' => K_LEFT,
        '\x03' | '\x1B' => K_BREAK,
        '\x0C' => K_CLEAR,
        _ => return None,
    };
    Some((dscan, shift))
}

/// Convert an SDL2 keycode to a dscan code if directly mappable.
pub fn keycode_to_dscan(key: Keycode) -> Option<u8> {
    match key {
        Keycode::Return | Keycode::KpEnter => Some(K_ENTER),
        Keycode::Backspace | Keycode::Delete => Some(K_LEFT),
        Keycode::Escape => Some(K_BREAK),
        Keycode::Space => Some(K_SPACE),
        Keycode::Up => Some(K_UP),
        Keycode::Down => Some(K_DOWN),
        Keycode::Left => Some(K_LEFT),
        Keycode::Right => Some(K_RIGHT),
        Keycode::LShift | Keycode::RShift => Some(K_SHIFT),
        Keycode::Home => Some(K_CLEAR),
        _ => None,
    }
}

/// Emulated Keyboard Matrix Controller.
pub struct KeyboardMatrix {
    /// Physical SRAM address of `g_kb_col_row_mask`.
    pub matrix_addr: u32,
    /// Active-low matrix for physical key hold/release (0 = pressed, 1 = unpressed).
    physical_matrix: [u8; 8],
    /// Active-low matrix for currently typing character.
    typed_matrix: [u8; 8],
    /// Queue of ASCII characters to type sequentially.
    type_queue: VecDeque<char>,
    /// Frame countdown for holding currently typed character.
    hold_countdown: u8,
    /// Frame countdown for inter-character release gap.
    gap_countdown: u8,
}

impl Default for KeyboardMatrix {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyboardMatrix {
    /// Create a new, idle keyboard matrix controller.
    pub fn new() -> Self {
        Self {
            matrix_addr: KB_MATRIX_ADDR,
            physical_matrix: [0xFF; 8],
            typed_matrix: [0xFF; 8],
            type_queue: VecDeque::new(),
            hold_countdown: 0,
            gap_countdown: 0,
        }
    }

    /// Enqueue a character to be typed into Color BASIC.
    pub fn type_char(&mut self, c: char) {
        self.type_queue.push_back(c);
    }

    /// Enqueue a string of characters to be typed sequentially.
    pub fn type_str(&mut self, s: &str) {
        for c in s.chars() {
            self.type_queue.push_back(c);
        }
    }

    /// Press a physical key via its SDL Keycode.
    pub fn key_down(&mut self, key: Keycode) {
        if let Some(dscan) = keycode_to_dscan(key) {
            let (row, col) = dscan_to_row_col(dscan);
            self.physical_matrix[col as usize] &= !(1 << row);
        }
    }

    /// Release a physical key via its SDL Keycode.
    pub fn key_up(&mut self, key: Keycode) {
        if let Some(dscan) = keycode_to_dscan(key) {
            let (row, col) = dscan_to_row_col(dscan);
            self.physical_matrix[col as usize] |= 1 << row;
        }
    }

    /// Release all held physical keys.
    pub fn release_all(&mut self) {
        self.physical_matrix = [0xFF; 8];
        self.typed_matrix = [0xFF; 8];
        self.hold_countdown = 0;
        self.gap_countdown = 0;
    }

    /// Synchronize the keyboard matrix with the RP2350 bus for the current frame.
    ///
    /// * `turbo`: simulation turbo factor (scales hold/gap countdowns so debounce duration in virtual time is invariant)
    /// * `is_ready`: whether Color BASIC is initialized and ready to accept input
    pub fn sync_frame(&mut self, bus: &mut Bus, turbo: u32, is_ready: bool) {
        let t = turbo.max(1);
        // Advance typing state machine
        if self.hold_countdown > 0 {
            self.hold_countdown -= 1;
            if self.hold_countdown == 0 {
                self.typed_matrix = [0xFF; 8];
                self.gap_countdown = (2 * t).min(255) as u8; // 2 virtual 60 Hz frames release gap
            }
        } else if self.gap_countdown > 0 {
            self.gap_countdown -= 1;
        } else if is_ready && let Some(c) = self.type_queue.pop_front() {
            if let Some((dscan, shift)) = kt_chord(c) {
                self.typed_matrix = [0xFF; 8];
                let (row, col) = dscan_to_row_col(dscan);
                self.typed_matrix[col as usize] &= !(1 << row);
                if shift {
                    let (s_row, s_col) = dscan_to_row_col(K_SHIFT);
                    self.typed_matrix[s_col as usize] &= !(1 << s_row);
                }
                self.hold_countdown = (4 * t).min(255) as u8; // 4 virtual 60 Hz frames hold duration (~66.7 ms)
                eprintln!("[CoCo Keyboard] Injected key {:?} into matrix (held for {} virtual frames)", c, self.hold_countdown);
            }
        }

        // Combine physical layer and typed character layer (bitwise AND: active low)
        for c in 0..8 {
            let col_mask = self.physical_matrix[c] & self.typed_matrix[c];
            bus.write8(self.matrix_addr + c as u32, col_mask, 0);
        }
    }
}
