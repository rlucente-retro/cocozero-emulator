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
    pub key_matrix: [u8; 8], // 8x8 CoCo keyboard matrix
}

impl Frontend {
    pub fn new(headless: bool) -> Result<Self, String> {
        let sdl = sdl2::init().map_err(|e| e.to_string())?;
        let video_subsystem = sdl.video().map_err(|e| e.to_string())?;

        let window = if !headless {
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

        let canvas = window
            .into_canvas()
            .present_vsync()
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

        if !headless {
            video_subsystem.text_input().start();
        }

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
    pub fn poll_events(&mut self) -> bool {
        let events: Vec<Event> = self.event_pump.poll_iter().collect();
        for event in events {
            match event {
                Event::Quit { .. } => return false,
                Event::TextInput { text, .. } => {
                    for c in text.chars() {
                        self.typed_chars.push_back(c);
                        self.serial_input_queue.push(c as u8);
                    }
                }
                Event::KeyDown {
                    keycode: Some(key),
                    repeat,
                    ..
                } => {
                    if !repeat {
                        self.handle_key_down(key);
                    }
                }
                Event::KeyUp {
                    keycode: Some(key), ..
                } => {
                    self.handle_key_up(key);
                }
                _ => {}
            }
        }
        true
    }

    fn handle_key_down(&mut self, key: Keycode) {
        match key {
            Keycode::Escape => {
                self.typed_chars.push_back('\x1B');
                self.serial_input_queue.push(0x1B);
                self.key_events.push(InputKeyEvent::Down(key));
            }
            Keycode::Return | Keycode::KpEnter => {
                self.typed_chars.push_back('\r');
                self.serial_input_queue.push(0x0D);
                self.key_events.push(InputKeyEvent::Down(key));
            }
            Keycode::Backspace => {
                self.typed_chars.push_back('\x08');
                self.serial_input_queue.push(0x08);
                self.key_events.push(InputKeyEvent::Down(key));
            }
            Keycode::Tab => {
                self.typed_chars.push_back('\t');
                self.serial_input_queue.push(b'\t');
            }
            Keycode::Up | Keycode::Down | Keycode::Left | Keycode::Right | Keycode::Space | Keycode::LShift | Keycode::RShift => {
                self.key_events.push(InputKeyEvent::Down(key));
            }
            Keycode::F1 => self.serial_input_queue.push(0x01), // INFO overlay toggle
            Keycode::F8 => self.serial_input_queue.push(0x06), // Artifact color cycle
            Keycode::F9 => self.serial_input_queue.push(0x0E), // Programs menu
            Keycode::F10 => self.serial_input_queue.push(0x0F), // Cartridges menu
            Keycode::F12 => self.serial_input_queue.push(0x10), // Disks menu
            _ => {}
        }
    }

    fn handle_key_up(&mut self, key: Keycode) {
        match key {
            Keycode::Escape | Keycode::Return | Keycode::KpEnter | Keycode::Backspace |
            Keycode::Up | Keycode::Down | Keycode::Left | Keycode::Right | Keycode::Space | Keycode::LShift | Keycode::RShift => {
                self.key_events.push(InputKeyEvent::Up(key));
            }
            _ => {}
        }
    }
}
