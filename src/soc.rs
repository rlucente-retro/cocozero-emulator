//! CoCo Zero RP2350B System-on-a-Chip Emulator Engine.
//!
//! Ties together:
//! - Dual ARM Cortex-M33 cores (252 MHz sys_clk)
//! - Hardware TMDS encoder in SIO
//! - SPI1 MicroSD card controller with software CS on GPIO 43
//! - UART0 serial console telemetry
//! - 520 KB SRAM with 320×240 RGB565 Framebuffer extraction

use std::collections::VecDeque;
use std::fmt;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

use rp2350_emu::{Config, Emulator, EmulatorBuilder};

use crate::keyboard::KeyboardMatrix;
use crate::spi_sd::{SectorStorage, SpiSdCard};
use crate::symbols::FirmwareSymbols;
use crate::uf2::FlashImage;

pub const SYS_CLK_HZ: u32 = 252_000_000;
pub const FB_WIDTH: usize = 320;
pub const FB_HEIGHT: usize = 240;
pub const FB_PIXELS: usize = FB_WIDTH * FB_HEIGHT;
pub const AUDIO_RING_SAMPLES: usize = 8192;

pub struct CoCoZeroSoC {
    pub emu: Emulator,
    pub sd_card: Arc<Mutex<SpiSdCard>>,
    pub serial_tx_log: Arc<Mutex<Vec<u8>>>,
    pub serial_rx_queue: VecDeque<u8>,
    pub fb_addr: Option<u32>,
    pub frame_counter: u64,
    pub total_cycles: u64,
    pub keyboard_ready_forced: bool,
    pub keyboard: KeyboardMatrix,
    pub symbols: FirmwareSymbols,
    pub host_audio_r: u32,
    pub audio_samples_drained: u64,
    pub audio_lapped_samples: u64,
    pub turbo: u32,
    dc_x1: i32,
    dc_y1: i64,
    dc_initialized: bool,
}

impl CoCoZeroSoC {
    pub fn new(sd_storage: Box<dyn SectorStorage + Send>) -> Result<Self, String> {
        let config = Config {
            sys_clk_hz: SYS_CLK_HZ,
        };
        let mut emu = EmulatorBuilder::new(config)
            .step_quantum(512)
            .build()
            .map_err(|e| format!("Failed to build RP2350 emulator: {:?}", e))?;

        let sd_card = Arc::new(Mutex::new(SpiSdCard::new(sd_storage)));
        let serial_tx_log = Arc::new(Mutex::new(Vec::new()));

        // Wire SPI1 to SD card
        let sd_clone = Arc::clone(&sd_card);
        emu.bus.spi[1].slave_handler = Some(Box::new(move |mosi| {
            let mut card = sd_clone.lock().unwrap();
            card.transfer_byte(mosi)
        }));

        // Wire UART0 TX to serial telemetry log + stdout
        let tx_clone = Arc::clone(&serial_tx_log);
        emu.bus.uart[0].tx_sink = Some(Box::new(move |byte| {
            tx_clone.lock().unwrap().push(byte);
            print!("{}", byte as char);
            let _ = std::io::stdout().flush();
        }));

        let symbols = FirmwareSymbols::default();
        Ok(Self {
            emu,
            sd_card,
            serial_tx_log,
            serial_rx_queue: VecDeque::new(),
            fb_addr: Some(symbols.fb_addr),
            frame_counter: 0,
            total_cycles: 0,
            keyboard_ready_forced: false,
            keyboard: KeyboardMatrix::new(),
            symbols,
            host_audio_r: 0,
            audio_samples_drained: 0,
            audio_lapped_samples: 0,
            turbo: 1,
            dc_x1: 0,
            dc_y1: 0,
            dc_initialized: false,
        })
    }

    /// Load Flash memory image (from UF2 or raw binary) into XIP Flash space.
    pub fn load_firmware(&mut self, flash: &FlashImage) {
        self.load_firmware_with_elf(flash, None);
    }

    /// Load Flash memory image with an optional companion ELF file for symbol extraction.
    pub fn load_firmware_with_elf(&mut self, flash: &FlashImage, elf_path: Option<&std::path::Path>) {
        self.emu.load_flash(&flash.memory);

        // Auto-detect firmware symbols from ELF or in-flash signatures
        let syms = FirmwareSymbols::auto_detect(elf_path, &flash.memory, crate::uf2::FLASH_BASE);
        self.symbols = syms;
        self.fb_addr = Some(syms.fb_addr);
        self.keyboard.matrix_addr = syms.kb_matrix_addr;

        // If bootrom is not loaded, initialize core 0 registers directly from flash vector table
        if let (Some(sp), Some(pc)) = (flash.initial_sp, flash.entry_point) {
            println!("Initializing Core 0 vectors directly: SP=0x{:08X}, PC=0x{:08X}", sp, pc);
            self.emu.core_mut(0).regs.msp = sp;
            self.emu.core_mut(0).regs.r[13] = sp;
            self.emu.core_mut(0).regs.set_pc(pc & !1);
            self.emu.core_mut(0).regs.xpsr = 1 << 24; // Thumb mode
            // Core 1 waits in WFE until awakened by multicore launch handshake
            self.emu.bus.atomics.set_wfe_waiting(1);

            // When Core 0 bypasses Boot ROM directly into Flash, Boot ROM's
            // security initialization (which sets RCP salt on both cores)
            // was skipped. Initialize RCP salt so Core 1's Boot ROM monitor
            // validates the canary check upon waking and handles the launch handshake.
            self.emu.bus.atomics.rcp_salt_set(0, 0xCAFE_BABE);
            self.emu.bus.atomics.rcp_salt_set(1, 0xDEAD_BEEF);
        }
    }

    /// Load RP2350 Boot ROM binary into 0x00000000..0x00007FFF.
    pub fn load_bootrom(&mut self, bootrom_bytes: &[u8]) {
        self.emu.load_bootrom(bootrom_bytes);
        self.emu.reset();
    }

    /// Update MicroSD Chip Select from SIO GPIO 43.
    fn update_sd_cs(&mut self) {
        let gpio_hi = self.emu.bus.sio.gpio_hi_out;
        // GPIO 43 is bit 11 in GPIO_HI_OUT (active low)
        let cs_asserted = (gpio_hi & (1 << (43 - 32))) == 0;
        self.sd_card.lock().unwrap().set_cs(cs_asserted);
    }

    /// Flush any pending dirty sectors in SD card storage back to host files.
    pub fn flush_storage(&mut self) -> bool {
        self.sd_card.lock().unwrap().flush()
    }

    /// Inject a character into UART0 serial console.
    pub fn push_serial_char(&mut self, ch: u8) {
        self.emu.bus.uart[0].push_rx_byte(ch);
    }

    /// Type an ASCII character into the Color Computer keyboard matrix.
    pub fn type_char(&mut self, ch: char) {
        self.keyboard.type_char(ch);
    }

    /// Type a string into the Color Computer keyboard matrix.
    pub fn type_str(&mut self, s: &str) {
        self.keyboard.type_str(s);
    }

    /// Press a physical key on the keyboard matrix.
    pub fn key_down(&mut self, key: sdl2::keyboard::Keycode) {
        self.keyboard.key_down(key);
    }

    /// Release a physical key on the keyboard matrix.
    pub fn key_up(&mut self, key: sdl2::keyboard::Keycode) {
        self.keyboard.key_up(key);
    }

    /// Advance the SoC simulation by virtual cycles.
    pub fn step_cycles(&mut self, cycles: u64) -> Result<(), String> {
        self.update_sd_cs();

        // Feed any pending serial input
        if let Some(ch) = self.serial_rx_queue.pop_front() {
            self.emu.bus.uart[0].push_rx_byte(ch);
        }

        self.emu
            .run(cycles)
            .map_err(|e| format!("Emulator run error: {:?}", e))?;

        self.total_cycles += cycles;
        self.update_sd_cs();
        Ok(())
    }

    /// Check whether Color BASIC has completed boot and is ready for keyboard input.
    pub fn is_keyboard_ready(&self) -> bool {
        self.keyboard_ready_forced || self.total_cycles >= 630_000_000
    }

    /// Step by one frame quantum (~252 MHz / 60 Hz = 4,200,000 cycles).
    pub fn step_frame(&mut self) -> Result<(), String> {
        self.step_frame_turbo(1)
    }

    /// Step by one frame quantum divided by turbo factor.
    pub fn step_frame_turbo(&mut self, turbo: u32) -> Result<(), String> {
        let t = turbo.max(1);
        self.turbo = t;
        let ready = self.is_keyboard_ready();
        self.keyboard.sync_frame(&mut self.emu.bus, t, ready);
        let cycles_per_frame = ((SYS_CLK_HZ / 60) as u64) / (t as u64);
        self.step_cycles(cycles_per_frame)?;
        self.frame_counter += 1;
        Ok(())
    }

    /// Scan SRAM to discover the active 320×240 RGB565 framebuffer.
    pub fn detect_framebuffer(&mut self) -> Option<u32> {
        if let Some(addr) = self.fb_addr {
            return Some(addr);
        }

        // Look through SRAM (0x20000000..0x20080000) in 1KB steps
        // Look for regions with non-zero color data
        let sram_base = 0x2000_0000u32;
        let sram_size = 520 * 1024;
        let fb_byte_size = (FB_PIXELS * 2) as u32;

        for offset in (0..(sram_size - fb_byte_size)).step_by(1024) {
            let addr = sram_base + offset;
            let mut nonzero_count = 0;
            for i in 0..100 {
                let pixel = self.emu.bus.memory.sram_read16(offset + i * 2);
                if pixel != 0 {
                    nonzero_count += 1;
                }
            }
            if nonzero_count > 20 {
                println!("Detected active framebuffer at SRAM 0x{:08X}", addr);
                self.fb_addr = Some(addr);
                return Some(addr);
            }
        }

        None
    }

    /// Extract the 320×240 RGB565 frame from SRAM for rendering.
    pub fn extract_frame(&mut self, out: &mut [u16; FB_PIXELS]) -> bool {
        let addr = self.fb_addr.unwrap_or(self.symbols.fb_addr);
        let offset = addr.saturating_sub(0x2000_0000);
        let mem = &self.emu.bus.memory;
        for i in 0..(FB_PIXELS / 2) {
            let val = mem.sram_read32(offset + (i as u32) * 4);
            out[i * 2] = val as u16;
            out[i * 2 + 1] = (val >> 16) as u16;
        }
        true
    }

    /// Extract pending audio samples from the firmware ring buffer.
    ///
    /// Drains mono 48 kHz 16-bit PCM samples produced by Core 0, duplicates
    /// each sample into interleaved stereo pairs [L, R], advances the host read index,
    /// and synchronizes g_audio_r in firmware SRAM.
    pub fn extract_audio(&mut self) -> Vec<i16> {
        let w_addr = self.symbols.audio_w_addr;
        let ring_addr = self.symbols.audio_ring_addr;
        let r_addr = self.symbols.audio_r_addr;
        let ring_bytes = (AUDIO_RING_SAMPLES * 2) as u32;

        // Ensure addresses and full ring span are within physical SRAM space (0x2000_0000..0x2008_2000)
        if !(0x2000_0000..0x2008_2000).contains(&w_addr)
            || ring_addr < 0x2000_0000
            || ring_addr.saturating_add(ring_bytes) > 0x2008_2000
        {
            return Vec::new();
        }

        let w = self.emu.bus.memory.sram_read32(w_addr - 0x2000_0000);
        let mut r = self.host_audio_r;
        let mut avail = w.wrapping_sub(r);

        // If producer lapped reader, skip ahead to avoid reading stale or overwritten slots
        if avail > AUDIO_RING_SAMPLES as u32 {
            let lapped = avail - AUDIO_RING_SAMPLES as u32;
            self.audio_lapped_samples += lapped as u64;
            r = w.wrapping_sub(AUDIO_RING_SAMPLES as u32);
            avail = AUDIO_RING_SAMPLES as u32;
        }

        if avail == 0 {
            return Vec::new();
        }

        self.audio_samples_drained += avail as u64;
        let turbo_factor = self.turbo.max(1) as usize;
        let mut out = Vec::with_capacity((avail as usize) * 2 * turbo_factor);
        while r != w {
            let ring_idx = (r as usize) & (AUDIO_RING_SAMPLES - 1);
            let sample_addr = ring_addr + (ring_idx as u32) * 2;
            let raw_sample = self.emu.bus.memory.sram_read16(sample_addr - 0x2000_0000) as i16;
            let x = raw_sample as i32;

            if !self.dc_initialized {
                self.dc_x1 = x;
                self.dc_y1 = 0;
                self.dc_initialized = true;
            }

            // Single-pole DC blocker (high-pass filter, fc ≈ 5 Hz at 48 kHz):
            // Removes resting DAC DC offsets (e.g. resting -1920) so idle BASIC is pure silence.
            let decay = (self.dc_y1 * (32768 - 32746)) / 32768;
            let decay = if decay == 0 && self.dc_y1 != 0 {
                if self.dc_y1 > 0 { 1 } else { -1 }
            } else {
                decay
            };
            let y = ((x as i64 - self.dc_x1 as i64) << 8) + self.dc_y1 - decay;
            self.dc_x1 = x;
            self.dc_y1 = y;
            let sample = ((y + 128) >> 8).clamp(-32768, 32767) as i16;

            for _ in 0..turbo_factor {
                // Duplicate mono to stereo [L, R]
                out.push(sample);
                out.push(sample);
            }
            r = r.wrapping_add(1);
        }

        self.host_audio_r = w;

        // Synchronize g_audio_r in SRAM so firmware telemetry reflects current drain
        if (0x2000_0000..0x2008_0000).contains(&r_addr) {
            self.emu.bus.memory.sram_write32(r_addr - 0x2000_0000, w);
        }

        out
    }

    /// Check whether a firmware menu overlay (Disks, Programs, Carts, Files, Info) is active.
    pub fn is_menu_active(&self) -> bool {
        self.emu.bus.memory.sram_read8(self.symbols.menu_active_addr - 0x2000_0000) != 0
    }

    /// Get current active menu mode (0 = Disks, 1 = Programs, 2 = Carts, 3 = Files, 4 = Info).
    pub fn get_menu_mode(&self) -> u32 {
        self.emu.bus.memory.sram_read32(self.symbols.menu_mode_addr - 0x2000_0000)
    }

    /// Inject a USB HID keycode report into the firmware's TinyUSB host handler on Core 0.
    pub fn inject_hid_keycode(&mut self, keycode: u8) {
        // Trampoline at TRAMP_ADDR: 'b .' (0xE7FE)
        self.emu.bus.write16(TRAMP_ADDR, 0xE7FE, 0);

        // Dedicated scratch HID report buffer located right below trampoline in upper SRAM
        let report_addr = TRAMP_ADDR - 0x10;

        // 1. Send Key Press report [modifier, reserved, keycode, 0, 0, 0, 0, 0]
        self.emu.bus.write8(report_addr, 0, 0);     // modifier
        self.emu.bus.write8(report_addr + 1, 0, 0); // reserved
        self.emu.bus.write8(report_addr + 2, keycode, 0); // keycode[0]
        for i in 3..8 {
            self.emu.bus.write8(report_addr + i, 0, 0);
        }

        let saved_r = self.emu.core(0).regs.r;
        let saved_xpsr = self.emu.core(0).regs.xpsr;
        let saved_msp = self.emu.core(0).regs.msp;

        self.emu.core_mut(0).regs.r[0] = 1; // dev_addr = 1
        self.emu.core_mut(0).regs.r[1] = 0; // instance = 0
        self.emu.core_mut(0).regs.r[2] = report_addr;
        self.emu.core_mut(0).regs.r[3] = 8; // len = 8
        self.emu.core_mut(0).regs.r[14] = TRAMP_ADDR | 1;
        self.emu.core_mut(0).regs.set_pc(self.symbols.hid_cb_addr);

        for _ in 0..10_000 {
            let _ = self.emu.run(100);
            if self.emu.core(0).regs.pc() == TRAMP_ADDR {
                break;
            }
        }

        // 2. Send Key Release report [0, 0, 0, 0, 0, 0, 0, 0]
        self.emu.bus.write8(report_addr + 2, 0, 0);
        self.emu.core_mut(0).regs.r[0] = 1;
        self.emu.core_mut(0).regs.r[1] = 0;
        self.emu.core_mut(0).regs.r[2] = report_addr;
        self.emu.core_mut(0).regs.r[3] = 8;
        self.emu.core_mut(0).regs.r[14] = TRAMP_ADDR | 1;
        self.emu.core_mut(0).regs.set_pc(self.symbols.hid_cb_addr);

        for _ in 0..10_000 {
            let _ = self.emu.run(100);
            if self.emu.core(0).regs.pc() == TRAMP_ADDR {
                break;
            }
        }

        // Restore Core 0 registers so it resumes previous execution cleanly
        self.emu.core_mut(0).regs.r = saved_r;
        self.emu.core_mut(0).regs.xpsr = saved_xpsr;
        self.emu.core_mut(0).regs.msp = saved_msp;
    }

    /// Trigger a menu mode directly (0=Disks, 1=Programs, 2=Carts, 3=Files, 4=Info).
    pub fn trigger_menu(&mut self, mode: u32) {
        let keycode = match mode {
            0 => 0x45, // F12 Disks
            1 => 0x42, // F9 Programs
            2 => 0x43, // F10 Cartridges
            3 => 0x44, // F11 Files
            4 => 0x3A, // F1 Info
            _ => 0x45,
        };
        self.inject_hid_keycode(keycode);
    }

    /// Extract diagnostic telemetry across CPU cores, audio ring, and storage subsystems.
    pub fn telemetry(&self) -> SocTelemetry {
        let (sd_read, sd_written, sd_flushes) = self.sd_card.lock().unwrap().stats();
        let w_addr = self.symbols.audio_w_addr;
        let w = if (0x2000_0000..0x2008_2000).contains(&w_addr) {
            self.emu.bus.memory.sram_read32(w_addr - 0x2000_0000)
        } else {
            0
        };
        let ring_watermark = w.wrapping_sub(self.host_audio_r);

        SocTelemetry {
            frame: self.frame_counter,
            total_cycles: self.total_cycles,
            cores: CoreTelemetry {
                core0_pc: self.emu.core(0).regs.pc(),
                core1_pc: self.emu.core(1).regs.pc(),
                core0_wfe: self.emu.core(0).is_wfe_waiting(),
                core1_wfe: self.emu.core(1).is_wfe_waiting(),
                core0_cycles: self.emu.core(0).cycles(),
                core1_cycles: self.emu.core(1).cycles(),
            },
            audio: AudioTelemetry {
                total_samples_produced: w,
                total_samples_drained: self.audio_samples_drained,
                lapped_samples: self.audio_lapped_samples,
                ring_watermark,
            },
            storage: StorageTelemetry {
                sectors_read: sd_read,
                sectors_written: sd_written,
                flushes_performed: sd_flushes,
            },
        }
    }

    /// Extract the active 320×240 framebuffer and save it to a standard 24-bit BMP image.
    pub fn dump_framebuffer_bmp(&mut self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let mut fb = [0u16; FB_PIXELS];
        self.extract_frame(&mut fb);
        save_bmp(path, &fb, FB_WIDTH, FB_HEIGHT)
    }
}

/// Instantaneous and cumulative audio subsystem telemetry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AudioTelemetry {
    pub total_samples_produced: u32,
    pub total_samples_drained: u64,
    pub lapped_samples: u64,
    pub ring_watermark: u32,
}

/// MicroSD SPI storage telemetry metrics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StorageTelemetry {
    pub sectors_read: u64,
    pub sectors_written: u64,
    pub flushes_performed: u64,
}

/// Multi-core processor state and cycle telemetry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CoreTelemetry {
    pub core0_pc: u32,
    pub core1_pc: u32,
    pub core0_wfe: bool,
    pub core1_wfe: bool,
    pub core0_cycles: u64,
    pub core1_cycles: u64,
}

/// Consolidated system-on-chip telemetry snapshot.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SocTelemetry {
    pub frame: u64,
    pub total_cycles: u64,
    pub cores: CoreTelemetry,
    pub audio: AudioTelemetry,
    pub storage: StorageTelemetry,
}

impl fmt::Display for SocTelemetry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[telemetry] frame={:>6} cycles={:>10} | C0: PC=0x{:08X} wfe={:<5} | C1: PC=0x{:08X} wfe={:<5} | Aud: drained={:<8} prod={:<8} lapped={:<4} wmark={:<4} | SD: r={:<6} w={:<6} flush={:<4}",
            self.frame,
            self.total_cycles,
            self.cores.core0_pc,
            self.cores.core0_wfe,
            self.cores.core1_pc,
            self.cores.core1_wfe,
            self.audio.total_samples_drained,
            self.audio.total_samples_produced,
            self.audio.lapped_samples,
            self.audio.ring_watermark,
            self.storage.sectors_read,
            self.storage.sectors_written,
            self.storage.flushes_performed,
        )
    }
}

/// Save a 16-bit RGB565 framebuffer to a standard 24-bit uncompressed BMP file.
pub fn save_bmp(path: impl AsRef<Path>, fb: &[u16], width: usize, height: usize) -> std::io::Result<()> {
    if fb.len() < width * height {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Framebuffer buffer too small: expected {}, got {}", width * height, fb.len()),
        ));
    }
    let row_padding = (4 - (width * 3) % 4) % 4;
    let image_size = (width * 3 + row_padding) * height;
    let file_size = 54 + image_size;

    let mut bmp = Vec::with_capacity(file_size);

    // BMP Header (14 bytes)
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&(file_size as u32).to_le_bytes());
    bmp.extend_from_slice(&0u32.to_le_bytes()); // Reserved
    bmp.extend_from_slice(&54u32.to_le_bytes()); // Pixel data offset

    // DIB Header (BITMAPINFOHEADER, 40 bytes)
    bmp.extend_from_slice(&40u32.to_le_bytes()); // Header size
    bmp.extend_from_slice(&(width as i32).to_le_bytes());
    bmp.extend_from_slice(&(height as i32).to_le_bytes()); // Positive height = bottom-up
    bmp.extend_from_slice(&1u16.to_le_bytes()); // Color planes
    bmp.extend_from_slice(&24u16.to_le_bytes()); // Bits per pixel (24-bit RGB)
    bmp.extend_from_slice(&0u32.to_le_bytes()); // Compression (BI_RGB, uncompressed)
    bmp.extend_from_slice(&(image_size as u32).to_le_bytes());
    bmp.extend_from_slice(&2835u32.to_le_bytes()); // Horizontal resolution (72 DPI)
    bmp.extend_from_slice(&2835u32.to_le_bytes()); // Vertical resolution (72 DPI)
    bmp.extend_from_slice(&0u32.to_le_bytes()); // Colors in palette
    bmp.extend_from_slice(&0u32.to_le_bytes()); // Important colors

    // Pixel data (bottom-up: row height-1 down to row 0)
    for y in (0..height).rev() {
        for x in 0..width {
            let pixel565 = fb[y * width + x];
            let r = (((pixel565 >> 11) & 0x1F) as u32 * 255 / 31) as u8;
            let g = (((pixel565 >> 5) & 0x3F) as u32 * 255 / 63) as u8;
            let b = ((pixel565 & 0x1F) as u32 * 255 / 31) as u8;
            bmp.push(b); // Blue
            bmp.push(g); // Green
            bmp.push(r); // Red
        }
        bmp.resize(bmp.len() + row_padding, 0);
    }

    std::fs::write(path, bmp)
}

/// Physical SRAM address of menu active flag (1 = active, 0 = inactive).
pub const MENU_ACTIVE_ADDR: u32 = 0x2000_c8e8;
/// Physical SRAM address of active menu mode (0 = Disks, 1 = Programs, 2 = Carts, 3 = Files, 4 = Info).
pub const MENU_MODE_ADDR: u32 = 0x2000_c8ec;
/// Physical SRAM address of TinyUSB HID keyboard report buffer.
pub const HID_REPORT_ADDR: u32 = 0x2001_c18c;
/// Entry point of TinyUSB host HID report callback in firmware flash.
pub const HID_CB_ADDR: u32 = 0x1001_f3a4;
/// Scratch trampoline location in upper SRAM.
pub const TRAMP_ADDR: u32 = 0x2007_ffe0;
