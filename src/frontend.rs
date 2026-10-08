//! SDL2 Frontend for CoCo Zero RP2350B Emulator:
//! - 640×480 @ 60 Hz DVI Display
//! - 48 kHz 16-bit Stereo PCM Audio
//! - USB Keyboard and Gamepad input mapping

use std::collections::VecDeque;

use sdl2::audio::{AudioQueue, AudioSpecDesired};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::{Canvas, Texture, TextureCreator};
use sdl2::video::{Window, WindowContext};
use sdl2::{EventPump, Sdl};

pub const FRAME_WIDTH: usize = 320;
pub const FRAME_HEIGHT: usize = 240;
pub const DISPLAY_WIDTH: u32 = 640;
pub const DISPLAY_HEIGHT: u32 = 480;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKeyEvent {
    Down(Keycode),
    Up(Keycode),
}

pub struct Frontend {
    _sdl: Sdl,
    canvas: Canvas<Window>,
    _texture_creator: TextureCreator<WindowContext>,
    texture: Texture<'static>,
    audio_queue: Option<AudioQueue<i16>>,
    event_pump: EventPump,
    pub serial_input_queue: Vec<u8>,
    pub typed_chars: VecDeque<char>,
    pub key_events: Vec<InputKeyEvent>,
    pub hid_key_queue: VecDeque<u8>,
    pub key_matrix: [u8; 8], // 8x8 CoCo keyboard matrix
}

impl Frontend {
    pub fn new(headless: bool) -> Result<Self, String> {
        sdl2::hint::set("SDL_MAC_BACKGROUND_APP", "0");
        let sdl = sdl2::init().map_err(|e| e.to_string())?;
        let video_subsystem = sdl.video().map_err(|e| e.to_string())?;

        let mut window = if !headless {
            video_subsystem
                .window("CoCo Zero - Waveshare RP2350-PiZero Emulator", DISPLAY_WIDTH, DISPLAY_HEIGHT)
                .position_centered()
                .build()
                .map_err(|e| e.to_string())?
        } else {
            video_subsystem
                .window("CoCo Zero (Headless)", DISPLAY_WIDTH, DISPLAY_HEIGHT)
                .hidden()
                .build()
                .map_err(|e| e.to_string())?
        };

        if !headless {
            window.raise();
            video_subsystem.text_input().start();
        }

        let canvas = window
            .into_canvas()
            .build()
            .map_err(|e| e.to_string())?;

        let texture_creator = canvas.texture_creator();
        let texture = texture_creator
            .create_texture_streaming(PixelFormatEnum::RGB565, FRAME_WIDTH as u32, FRAME_HEIGHT as u32)
            .map_err(|e| e.to_string())?;

        let static_texture: Texture<'static> = unsafe { std::mem::transmute(texture) };


        // Initialize Audio Subsystem
        let audio_queue = if let Ok(audio_subsystem) = sdl.audio() {
            let desired_spec = AudioSpecDesired {
                freq: Some(48_000),
                channels: Some(2), // Stereo
                samples: Some(1024),
            };
            match audio_subsystem.open_queue::<i16, _>(None, &desired_spec) {
                Ok(queue) => {
                    queue.resume();
                    Some(queue)
                }
                Err(err) => {
                    eprintln!("Warning: Failed to open audio device: {}", err);
                    None
                }
            }
        } else {
            None
        };

        let event_pump = sdl.event_pump().map_err(|e| e.to_string())?;

        Ok(Self {
            _sdl: sdl,
            canvas,
            _texture_creator: texture_creator,
            texture: static_texture,
            audio_queue,
            event_pump,
            serial_input_queue: Vec::new(),
            typed_chars: VecDeque::new(),
            key_events: Vec::new(),
            hid_key_queue: VecDeque::new(),
            key_matrix: [0xFF; 8],
        })
    }

    /// Render a 320×240 RGB565 framebuffer from RP2350 SRAM.
    pub fn render_frame(&mut self, fb: &[u16]) -> Result<(), String> {
        if fb.len() < FRAME_WIDTH * FRAME_HEIGHT {
            return Err("Framebuffer size smaller than 320x240".to_string());
        }

        self.texture
            .with_lock(None, |buffer: &mut [u8], pitch: usize| {
                for y in 0..FRAME_HEIGHT {
                    let src_row = &fb[y * FRAME_WIDTH..(y + 1) * FRAME_WIDTH];
                    let dst_row = &mut buffer[y * pitch..y * pitch + FRAME_WIDTH * 2];
                    for x in 0..FRAME_WIDTH {
                        let pixel = src_row[x];
                        dst_row[x * 2] = (pixel & 0xFF) as u8;
                        dst_row[x * 2 + 1] = ((pixel >> 8) & 0xFF) as u8;
                    }
                }
            })
            .map_err(|e| e.to_string())?;

        self.canvas.clear();
        self.canvas.copy(&self.texture, None, None).map_err(|e| e.to_string())?;
        self.canvas.present();
        Ok(())
    }

    /// Push audio PCM samples (16-bit signed stereo, 48 kHz).
    pub fn push_audio(&mut self, samples: &[i16]) {
        if let Some(queue) = &mut self.audio_queue {
            // Keep queue bounded to avoid huge latency
            if queue.size() < 48_000 * 2 * 2 {
                let _ = queue.queue_audio(samples);
            }
        }
    }

    /// Drain and return any pending physical key events.
    pub fn drain_key_events(&mut self) -> Vec<InputKeyEvent> {
        std::mem::take(&mut self.key_events)
    }

    /// Poll SDL events and translate them to keyboard / control signals.
    /// Returns false if user requested exit.
    pub fn poll_events(&mut self, is_menu_active: bool) -> bool {
        let events: Vec<Event> = self.event_pump.poll_iter().collect();
        let has_text_input = events.iter().any(|e| matches!(e, Event::TextInput { .. }));

        for event in events {
            match event {
                Event::Quit { .. } => return false,
                Event::TextInput { text, .. } => {
                    // Only process TextInput for Color BASIC typing if menu overlay is not active
                    if !is_menu_active {
                        for ch in text.chars() {
                            eprintln!("[Input] Window TextInput: {:?}", ch);
                            self.typed_chars.push_back(ch);
                        }
                    }
                }
                Event::KeyDown {
                    keycode,
                    scancode,
                    keymod,
                    repeat,
                    ..
                } => {
                    let key = keycode.or_else(|| scancode.and_then(Keycode::from_scancode));
                    if let Some(key) = key
                        && !repeat
                    {
                        let shift = keymod.contains(sdl2::keyboard::Mod::LSHIFTMOD)
                            || keymod.contains(sdl2::keyboard::Mod::RSHIFTMOD)
                            || keymod.contains(sdl2::keyboard::Mod::CAPSMOD);
                        let cmd_or_ctrl = keymod.contains(sdl2::keyboard::Mod::LGUIMOD)
                            || keymod.contains(sdl2::keyboard::Mod::RGUIMOD)
                            || keymod.contains(sdl2::keyboard::Mod::LCTRLMOD)
                            || keymod.contains(sdl2::keyboard::Mod::RCTRLMOD);
                        self.handle_key_down(key, shift, cmd_or_ctrl, is_menu_active, has_text_input);
                    }
                }
                Event::KeyUp {
                    keycode,
                    scancode,
                    ..
                } => {
                    let key = keycode.or_else(|| scancode.and_then(Keycode::from_scancode));
                    if let Some(key) = key {
                        self.handle_key_up(key);
                    }
                }
                Event::MouseButtonDown { .. } => {
                    self.canvas.window_mut().raise();
                }
                _ => {}
            }
        }
        true
    }

    fn handle_key_down(
        &mut self,
        key: Keycode,
        shift: bool,
        cmd_or_ctrl: bool,
        is_menu_active: bool,
        has_text_input: bool,
    ) {
        // 1. Function Keys and macOS Command/Control shortcuts
        if key == Keycode::F12 || (cmd_or_ctrl && key == Keycode::D) {
            eprintln!("[Menu] F12 / Cmd+D -> Disks menu");
            self.hid_key_queue.push_back(0x45); // HID F12
            return;
        }
        if key == Keycode::F9 || (cmd_or_ctrl && key == Keycode::P) {
            eprintln!("[Menu] F9 / Cmd+P -> Programs menu");
            self.hid_key_queue.push_back(0x42); // HID F9
            return;
        }
        if key == Keycode::F10 || (cmd_or_ctrl && key == Keycode::C) {
            eprintln!("[Menu] F10 / Cmd+C -> Cartridges menu");
            self.hid_key_queue.push_back(0x43); // HID F10
            return;
        }
        if key == Keycode::F11 {
            eprintln!("[Menu] F11 -> Files menu");
            self.hid_key_queue.push_back(0x44); // HID F11
            return;
        }
        if key == Keycode::F1 || (cmd_or_ctrl && key == Keycode::I) {
            eprintln!("[Menu] F1 / Cmd+I -> Info overlay");
            self.hid_key_queue.push_back(0x3A); // HID F1
            return;
        }
        if key == Keycode::F8 || (cmd_or_ctrl && key == Keycode::A) {
            eprintln!("[Menu] F8 / Cmd+A -> Artifact colors toggle");
            self.hid_key_queue.push_back(0x41); // HID F8
            return;
        }

        // 2. When menu overlay is active, route navigation keys directly to menu
        if is_menu_active {
            match key {
                Keycode::Up => {
                    eprintln!("[Menu Nav] Up");
                    self.hid_key_queue.push_back(0x52); // HID KEY_UP
                }
                Keycode::Down => {
                    eprintln!("[Menu Nav] Down");
                    self.hid_key_queue.push_back(0x51); // HID KEY_DOWN
                }
                Keycode::PageUp => {
                    eprintln!("[Menu Nav] PageUp");
                    self.hid_key_queue.push_back(0x4B); // HID KEY_PAGE_UP
                }
                Keycode::PageDown => {
                    eprintln!("[Menu Nav] PageDown");
                    self.hid_key_queue.push_back(0x4E); // HID KEY_PAGE_DOWN
                }
                Keycode::Left => {
                    eprintln!("[Menu Nav] Left");
                    self.hid_key_queue.push_back(0x50); // HID KEY_LEFT
                }
                Keycode::Right => {
                    eprintln!("[Menu Nav] Right");
                    self.hid_key_queue.push_back(0x4F); // HID KEY_RIGHT
                }
                Keycode::Return | Keycode::KpEnter => {
                    eprintln!("[Menu Nav] Select (Return)");
                    self.hid_key_queue.push_back(0x28); // HID KEY_ENTER
                }
                Keycode::Escape => {
                    eprintln!("[Menu Nav] Cancel (Escape)");
                    self.hid_key_queue.push_back(0x29); // HID KEY_ESCAPE
                }
                Keycode::Tab => {
                    eprintln!("[Menu Nav] Tab");
                    self.hid_key_queue.push_back(0x2B); // HID KEY_TAB
                }
                Keycode::Num0 | Keycode::Kp0 => {
                    eprintln!("[Menu Nav] Drive 0");
                    self.hid_key_queue.push_back(0x27); // HID '0'
                }
                Keycode::Num1 | Keycode::Kp1 => {
                    eprintln!("[Menu Nav] Drive 1");
                    self.hid_key_queue.push_back(0x1E); // HID '1'
                }
                Keycode::Num2 | Keycode::Kp2 => {
                    eprintln!("[Menu Nav] Drive 2");
                    self.hid_key_queue.push_back(0x1F); // HID '2'
                }
                Keycode::Num3 | Keycode::Kp3 => {
                    eprintln!("[Menu Nav] Drive 3");
                    self.hid_key_queue.push_back(0x20); // HID '3'
                }
                _ => {}
            }
            return;
        }

        // 3. Normal typing when menu is not active (Color BASIC typing)
        match key {
            Keycode::Return | Keycode::KpEnter => {
                eprintln!("[Input] KeyDown: Enter");
                self.typed_chars.push_back('\r');
            }
            Keycode::Backspace | Keycode::Delete => {
                eprintln!("[Input] KeyDown: Backspace");
                self.typed_chars.push_back('\x08');
            }
            Keycode::Escape => {
                eprintln!("[Input] KeyDown: Break");
                self.typed_chars.push_back('\x1B');
            }
            Keycode::Tab => {
                eprintln!("[Input] KeyDown: Tab");
                self.typed_chars.push_back('\t');
            }
            Keycode::Home => {
                eprintln!("[Input] KeyDown: Clear");
                self.typed_chars.push_back('\x0C');
            }
            Keycode::Up => {
                eprintln!("[Input] KeyDown: Up");
                self.typed_chars.push_back('^');
                self.key_events.push(InputKeyEvent::Down(key));
            }
            Keycode::Down => {
                eprintln!("[Input] KeyDown: Down");
                self.typed_chars.push_back('[');
                self.key_events.push(InputKeyEvent::Down(key));
            }
            Keycode::Left => {
                eprintln!("[Input] KeyDown: Left");
                self.typed_chars.push_back('\x08');
                self.key_events.push(InputKeyEvent::Down(key));
            }
            Keycode::Right => {
                eprintln!("[Input] KeyDown: Right");
                self.typed_chars.push_back(']');
                self.key_events.push(InputKeyEvent::Down(key));
            }
            Keycode::LShift | Keycode::RShift => {
                self.key_events.push(InputKeyEvent::Down(key));
            }
            _ => {
                // If TextInput is NOT handling this batch, fall back to keycode_to_char
                if !has_text_input
                    && let Some(ch) = keycode_to_char(key, shift)
                {
                    eprintln!("[Input] KeyDown (fallback): {:?}", ch);
                    self.typed_chars.push_back(ch);
                }
            }
        }
    }

    fn handle_key_up(&mut self, key: Keycode) {
        match key {
            Keycode::Up | Keycode::Down | Keycode::Left | Keycode::Right | Keycode::LShift | Keycode::RShift => {
                self.key_events.push(InputKeyEvent::Up(key));
            }
            _ => {}
        }
    }
}

pub fn keycode_to_char(key: Keycode, shift: bool) -> Option<char> {
    match key {
        Keycode::A => Some(if shift { 'A' } else { 'a' }),
        Keycode::B => Some(if shift { 'B' } else { 'b' }),
        Keycode::C => Some(if shift { 'C' } else { 'c' }),
        Keycode::D => Some(if shift { 'D' } else { 'd' }),
        Keycode::E => Some(if shift { 'E' } else { 'e' }),
        Keycode::F => Some(if shift { 'F' } else { 'f' }),
        Keycode::G => Some(if shift { 'G' } else { 'g' }),
        Keycode::H => Some(if shift { 'H' } else { 'h' }),
        Keycode::I => Some(if shift { 'I' } else { 'i' }),
        Keycode::J => Some(if shift { 'J' } else { 'j' }),
        Keycode::K => Some(if shift { 'K' } else { 'k' }),
        Keycode::L => Some(if shift { 'L' } else { 'l' }),
        Keycode::M => Some(if shift { 'M' } else { 'm' }),
        Keycode::N => Some(if shift { 'N' } else { 'n' }),
        Keycode::O => Some(if shift { 'O' } else { 'o' }),
        Keycode::P => Some(if shift { 'P' } else { 'p' }),
        Keycode::Q => Some(if shift { 'Q' } else { 'q' }),
        Keycode::R => Some(if shift { 'R' } else { 'r' }),
        Keycode::S => Some(if shift { 'S' } else { 's' }),
        Keycode::T => Some(if shift { 'T' } else { 't' }),
        Keycode::U => Some(if shift { 'U' } else { 'u' }),
        Keycode::V => Some(if shift { 'V' } else { 'v' }),
        Keycode::W => Some(if shift { 'W' } else { 'w' }),
        Keycode::X => Some(if shift { 'X' } else { 'x' }),
        Keycode::Y => Some(if shift { 'Y' } else { 'y' }),
        Keycode::Z => Some(if shift { 'Z' } else { 'z' }),
        Keycode::Num0 => Some(if shift { ')' } else { '0' }),
        Keycode::Num1 => Some(if shift { '!' } else { '1' }),
        Keycode::Num2 => Some(if shift { '@' } else { '2' }),
        Keycode::Num3 => Some(if shift { '#' } else { '3' }),
        Keycode::Num4 => Some(if shift { '$' } else { '4' }),
        Keycode::Num5 => Some(if shift { '%' } else { '5' }),
        Keycode::Num6 => Some(if shift { '^' } else { '6' }),
        Keycode::Num7 => Some(if shift { '&' } else { '7' }),
        Keycode::Num8 => Some(if shift { '*' } else { '8' }),
        Keycode::Num9 => Some(if shift { '(' } else { '9' }),
        Keycode::Space => Some(' '),
        Keycode::Return | Keycode::KpEnter => Some('\r'),
        Keycode::Backspace | Keycode::Delete => Some('\x08'),
        Keycode::Tab => Some('\t'),
        Keycode::Escape => Some('\x1B'),
        Keycode::Minus => Some(if shift { '_' } else { '-' }),
        Keycode::Equals => Some(if shift { '+' } else { '=' }),
        Keycode::LeftBracket => Some(if shift { '{' } else { '[' }),
        Keycode::RightBracket => Some(if shift { '}' } else { ']' }),
        Keycode::Backslash => Some(if shift { '|' } else { '\\' }),
        Keycode::Semicolon => Some(if shift { ':' } else { ';' }),
        Keycode::Quote => Some(if shift { '"' } else { '\'' }),
        Keycode::Comma => Some(if shift { '<' } else { ',' }),
        Keycode::Period => Some(if shift { '>' } else { '.' }),
        Keycode::Slash => Some(if shift { '?' } else { '/' }),
        Keycode::Backquote => Some(if shift { '~' } else { '`' }),
        Keycode::Kp0 => Some('0'),
        Keycode::Kp1 => Some('1'),
        Keycode::Kp2 => Some('2'),
        Keycode::Kp3 => Some('3'),
        Keycode::Kp4 => Some('4'),
        Keycode::Kp5 => Some('5'),
        Keycode::Kp6 => Some('6'),
        Keycode::Kp7 => Some('7'),
        Keycode::Kp8 => Some('8'),
        Keycode::Kp9 => Some('9'),
        Keycode::KpPlus => Some('+'),
        Keycode::KpMinus => Some('-'),
        Keycode::KpMultiply => Some('*'),
        Keycode::KpDivide => Some('/'),
        Keycode::KpPeriod => Some('.'),
        Keycode::Up => Some('^'),
        Keycode::Down => Some('['),
        Keycode::Left => Some('\x08'),
        Keycode::Right => Some(']'),
        Keycode::Home => Some('\x0C'),
        _ => None,
    }
}
