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
pub const FB_WIDTH_640: usize = 640;
pub const FB_HEIGHT: usize = 240;
pub const FB_PIXELS: usize = FB_WIDTH * FB_HEIGHT;
pub const FB_PIXELS_640: usize = FB_WIDTH_640 * FB_HEIGHT;
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
    pub prev_fdc_motor_on: bool,
    pub motor_off_syncs: u64,
    pub prev_volume: u8,
    pub is_muted: bool,
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

        // Release POWMAN from reset so power manager and persistent scratch registers are active
        emu.bus.resets_state &= !(1 << 17);

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
        let mut keyboard = KeyboardMatrix::new();
        keyboard.matrix_addr = symbols.kb_matrix_addr;
        keyboard.src_addr = symbols.kb_src_addr;
        keyboard.src_ready_addr = symbols.kb_src_ready_addr;

        Ok(Self {
            emu,
            sd_card,
            serial_tx_log,
            serial_rx_queue: VecDeque::new(),
            fb_addr: Some(symbols.fb_addr),
            frame_counter: 0,
            total_cycles: 0,
            keyboard_ready_forced: false,
            keyboard,
            symbols,
            host_audio_r: 0,
            audio_samples_drained: 0,
            audio_lapped_samples: 0,
            turbo: 1,
            prev_fdc_motor_on: false,
            motor_off_syncs: 0,
            prev_volume: 10,
            is_muted: false,
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
    pub fn load_firmware_with_elf(
        &mut self,
        flash: &FlashImage,
        elf_path: Option<&std::path::Path>,
    ) {
        self.emu.load_flash(&flash.memory);

        // Auto-detect firmware symbols from ELF or in-flash signatures
        let syms = FirmwareSymbols::auto_detect(elf_path, &flash.memory, crate::uf2::FLASH_BASE);
        self.symbols = syms;
        self.fb_addr = Some(syms.fb_addr);
        self.keyboard.matrix_addr = syms.kb_matrix_addr;
        self.keyboard.src_addr = syms.kb_src_addr;
        self.keyboard.src_ready_addr = syms.kb_src_ready_addr;

        // If bootrom is not loaded, initialize core 0 registers directly from flash vector table
        if let (Some(sp), Some(pc)) = (flash.initial_sp, flash.entry_point) {
            println!(
                "Initializing Core 0 vectors directly: SP=0x{:08X}, PC=0x{:08X}",
                sp, pc
            );
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

    /// Update analog joystick axis (port: 0 [Right/primary] or 1 [Left], axis: 0 [X] or 1 [Y], value: 0..=65535, center: 32767).
    pub fn set_joystick_axis(&mut self, port: usize, axis: usize, value: u16) {
        if port > 1 || axis > 1 {
            return;
        }
        if let Some(axis_base) = self.symbols.joy_axis_addr {
            let offset = ((port * 2 + axis) * 2) as u32;
            let target_addr = axis_base + offset;
            if (0x2000_0000..0x2008_2000).contains(&target_addr) {
                let sram_off = target_addr - 0x2000_0000;
                self.emu.bus.memory.sram_write16(sram_off, value);
            }
        }
    }

    /// Update joystick fire button state (port: 0 [Right/primary] or 1 [Left], pressed: bool).
    ///
    /// Bit 0 of g_joy_fire corresponds to Port 0 Fire 1 (Right joystick fire on PIA0 PA0).
    /// Bit 1 of g_joy_fire corresponds to Port 1 Fire 1 (Left joystick fire on PIA0 PA1).
    pub fn set_joystick_fire(&mut self, port: usize, pressed: bool) {
        if port > 1 {
            return;
        }
        if let Some(fire_addr) = self.symbols.joy_fire_addr
            && (0x2000_0000..0x2008_2000).contains(&fire_addr)
        {
            let sram_off = fire_addr - 0x2000_0000;
            let current = self.emu.bus.memory.sram_read8(sram_off);
            let mask = 1u8 << port;
            let updated = if pressed {
                current | mask
            } else {
                current & !mask
            };
            self.emu.bus.memory.sram_write8(sram_off, updated);
        }
    }

    /// Update joystick secondary fire button state (port: 0 [Right/primary] or 1 [Left], pressed: bool).
    ///
    /// Bit 2 of g_joy_fire corresponds to Port 0 Fire 2 (Right joystick second fire on PIA0 PA2, CoCo 3).
    /// Bit 3 of g_joy_fire corresponds to Port 1 Fire 2 (Left joystick second fire on PIA0 PA3, CoCo 3).
    pub fn set_joystick_fire2(&mut self, port: usize, pressed: bool) {
        if port > 1 {
            return;
        }
        if let Some(fire_addr) = self.symbols.joy_fire_addr
            && (0x2000_0000..0x2008_2000).contains(&fire_addr)
        {
            let sram_off = fire_addr - 0x2000_0000;
            let current = self.emu.bus.memory.sram_read8(sram_off);
            let mask = 4u8 << port;
            let updated = if pressed {
                current | mask
            } else {
                current & !mask
            };
            self.emu.bus.memory.sram_write8(sram_off, updated);
        }
    }

    /// Update joystick fire button by button index (0 = primary fire, 1 = second fire).
    pub fn set_joystick_fire_button(&mut self, port: usize, button: usize, pressed: bool) {
        match button {
            0 => self.set_joystick_fire(port, pressed),
            1 => self.set_joystick_fire2(port, pressed),
            _ => {}
        }
    }

    /// Release all joysticks (center all axes to 32767 and release all fire buttons).
    pub fn release_all_joysticks(&mut self) {
        for port in 0..2 {
            for axis in 0..2 {
                self.set_joystick_axis(port, axis, 32767);
            }
            self.set_joystick_fire(port, false);
            self.set_joystick_fire2(port, false);
        }
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
        if self.keyboard_ready_forced {
            return true;
        }
        // CoCo 3 (32 KB ROM at 0x2000_f200 matrix) requires ~210 6809 frames to complete memory test.
        // CoCo 2 (16 KB ROM at 0x2000_b304 matrix) requires ~150 6809 frames.
        let is_coco3 = self.symbols.kb_matrix_addr == 0x2000_f200;
        let ready_cycles = if is_coco3 {
            882_000_000u64 // 210 frames * 4,200,000 cycles
        } else {
            630_000_000u64 // 150 frames * 4,200,000 cycles
        };
        self.total_cycles >= ready_cycles / (self.turbo.max(1) as u64)
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
        self.check_fdc_motor_sync();
        Ok(())
    }

    /// Read the current $FF40 floppy disk controller latch from guest state in SRAM.
    pub fn get_fdc_latch(&self) -> Option<u8> {
        let addr = self.symbols.fdc_latch_addr?;
        if (0x2000_0000..0x2008_2000).contains(&addr) {
            Some(self.emu.bus.memory.sram_read8(addr - 0x2000_0000))
        } else {
            None
        }
    }

    /// Check whether the floppy disk drive spindle motor is currently energized ($FF40 bit 3 == 1).
    pub fn is_spindle_motor_on(&self) -> bool {
        (self.get_fdc_latch().unwrap_or(0) & 0x08) != 0
    }

    /// Read the currently selected floppy drive index (0..3) as tracked by firmware.
    pub fn get_fdc_drive(&self) -> Option<u32> {
        let addr = self.symbols.fdc_drive_addr?;
        if (0x2000_0000..0x2008_2000).contains(&addr) {
            Some(self.emu.bus.memory.sram_read32(addr - 0x2000_0000))
        } else {
            None
        }
    }

    /// Check for floppy drive spindle motor shutoff ($FF40 bit 3: 1 -> 0).
    ///
    /// When guest operating systems (Disk BASIC / NitrOS-9) complete disk activity,
    /// they clear bit 3 of the $FF40 latch. This 1 -> 0 transition triggers an immediate
    /// flush of dirty virtual FAT32 sectors and directory metadata back to host files.
    pub fn check_fdc_motor_sync(&mut self) -> bool {
        let current_motor_on = self.is_spindle_motor_on();
        let motor_shutoff = self.prev_fdc_motor_on && !current_motor_on;
        self.prev_fdc_motor_on = current_motor_on;

        if motor_shutoff {
            self.motor_off_syncs += 1;
            self.flush_storage()
        } else {
            false
        }
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

    /// Check whether the guest system is a Color Computer 3 (CoCo 3).
    pub fn is_coco3(&self) -> bool {
        self.symbols.kb_matrix_addr == 0x2000_f200
    }

    /// Check whether the guest CoCo 3 display is currently in 640-wide mode (e.g. WIDTH 80).
    pub fn is_wide_mode(&self) -> bool {
        if !self.is_coco3() || self.is_menu_active() {
            return false;
        }

        let gime_ptr_addr = self.symbols.gime3_ptr_addr.unwrap_or(0x2004_bbf0);
        if !(0x2000_0000..0x2008_2000).contains(&gime_ptr_addr) {
            return false;
        }

        let gime_addr = self.emu.bus.memory.sram_read32(gime_ptr_addr - 0x2000_0000);
        if !(0x2000_0000..0x2008_2000).contains(&gime_addr) {
            return false;
        }

        // VMODE ($FF98) at gime_addr + 0xFC + 8; VRES ($FF99) at gime_addr + 0xFC + 9
        let reg8 = self
            .emu
            .bus
            .memory
            .sram_read8(gime_addr + 0x104 - 0x2000_0000);
        let reg9 = self
            .emu
            .bus
            .memory
            .sram_read8(gime_addr + 0x105 - 0x2000_0000);

        let bp = (reg8 & 0x80) != 0;
        let hres = (reg9 >> 2) & 7;

        if !bp {
            // Text mode: (hres & 4) != 0 corresponds to 64 or 80 columns (resolution 2)
            (hres & 4) != 0
        } else {
            // Graphics mode: resolution = hres >> 1; resolution >= 2 corresponds to 640-wide graphics
            (hres >> 1) >= 2
        }
    }

    /// Check whether the active firmware uses 640×240 8bpp paletted scanout (VIDEO_640).
    pub fn is_paletted(&self) -> bool {
        self.symbols.pal565_addr.is_some()
    }

    /// Extract the 320×240 RGB565 frame from SRAM for rendering.
    pub fn extract_frame(&mut self, out: &mut [u16; FB_PIXELS]) -> bool {
        if let Some(pal_addr) = self.symbols.pal565_addr {
            let fb_addr = self.fb_addr.unwrap_or(self.symbols.fb_addr);
            let fb_offset = fb_addr.saturating_sub(0x2000_0000);
            let pal_offset = pal_addr.saturating_sub(0x2000_0000);
            let mem = &self.emu.bus.memory;

            let mut pal = [0u16; 256];
            for i in 0..256 {
                pal[i] = mem.sram_read16(pal_offset + (i as u32) * 2);
            }

            // Downsample 640x240 to 320x240 by sampling every second pixel horizontally
            for y in 0..FB_HEIGHT {
                let line_start = fb_offset + (y * FB_WIDTH_640) as u32;
                let out_line_start = y * FB_WIDTH;
                for x in 0..FB_WIDTH {
                    let idx = mem.sram_read8(line_start + (x * 2) as u32) as usize;
                    out[out_line_start + x] = pal[idx];
                }
            }
            return true;
        }

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

    /// Extract a 640×240 RGB565 frame for crisp 1:1 rendering.
    ///
    /// In 640x240 paletted mode (VIDEO_640), this method translates single-byte pixel indices
    /// in g_fb directly through g_pal565[256].
    /// In legacy 16bpp firmware, it decodes CoCo 3 VRAM in wide mode, or doubles 320-wide pixels horizontally.
    pub fn extract_frame_640(&mut self, out: &mut [u16]) -> bool {
        if out.len() < FB_PIXELS_640 {
            return false;
        }

        // If pal565_addr is detected, the firmware uses VIDEO_640 paletted scanout:
        // g_fb holds 640x240 single-byte color indices, mapped through g_pal565[256].
        if let Some(pal_addr) = self.symbols.pal565_addr {
            let fb_addr = self.fb_addr.unwrap_or(self.symbols.fb_addr);
            let fb_offset = fb_addr.saturating_sub(0x2000_0000);
            let pal_offset = pal_addr.saturating_sub(0x2000_0000);
            let mem = &self.emu.bus.memory;

            let mut pal = [0u16; 256];
            for i in 0..256 {
                pal[i] = mem.sram_read16(pal_offset + (i as u32) * 2);
            }

            for i in 0..(FB_PIXELS_640 / 4) {
                let val32 = mem.sram_read32(fb_offset + (i as u32) * 4);
                let b0 = (val32 & 0xFF) as usize;
                let b1 = ((val32 >> 8) & 0xFF) as usize;
                let b2 = ((val32 >> 16) & 0xFF) as usize;
                let b3 = ((val32 >> 24) & 0xFF) as usize;
                let base_idx = i * 4;
                out[base_idx] = pal[b0];
                out[base_idx + 1] = pal[b1];
                out[base_idx + 2] = pal[b2];
                out[base_idx + 3] = pal[b3];
            }
            return true;
        }

        if self.is_wide_mode() {
            let gime_ptr_addr = self.symbols.gime3_ptr_addr.unwrap_or(0x2004_bbf0);
            let c3bus_addr = self.symbols.c3bus_addr.unwrap_or(0x2001_0860);

            let gime_addr = self.emu.bus.memory.sram_read32(gime_ptr_addr - 0x2000_0000);
            let ram_lo = self.emu.bus.memory.sram_read32(c3bus_addr - 0x2000_0000);
            let ram_hi = self
                .emu
                .bus
                .memory
                .sram_read32(c3bus_addr + 4 - 0x2000_0000);

            let ram_lo_offset = ram_lo.saturating_sub(0x2000_0000);
            let ram_hi_offset = ram_hi.saturating_sub(0x2000_0000);

            // Read 64-entry RGB565 palette table from g_c3bus
            let mut rgb64 = [0u16; 64];
            for (i, entry) in rgb64.iter_mut().enumerate() {
                *entry = self
                    .emu
                    .bus
                    .memory
                    .sram_read16(c3bus_addr + 24 + (i as u32) * 2 - 0x2000_0000);
            }

            // Read 16 palette registers from gime->palette_reg
            let mut pal16 = [0u8; 16];
            for (i, entry) in pal16.iter_mut().enumerate() {
                *entry = self
                    .emu
                    .bus
                    .memory
                    .sram_read8(gime_addr + 0x148 + i as u32 - 0x2000_0000)
                    & 0x3F;
            }

            let reg8 = self
                .emu
                .bus
                .memory
                .sram_read8(gime_addr + 0x104 - 0x2000_0000);
            let reg9 = self
                .emu
                .bus
                .memory
                .sram_read8(gime_addr + 0x105 - 0x2000_0000);
            let reg10 = self
                .emu
                .bus
                .memory
                .sram_read8(gime_addr + 0x106 - 0x2000_0000);
            let reg13 = self
                .emu
                .bus
                .memory
                .sram_read8(gime_addr + 0x109 - 0x2000_0000) as u32;
            let reg14 = self
                .emu
                .bus
                .memory
                .sram_read8(gime_addr + 0x10A - 0x2000_0000) as u32;

            let bp = (reg8 & 0x80) != 0;
            let lpr = reg8 & 7;
            let lpf = (reg9 >> 5) & 3;
            let cres = reg9 & 3;

            let border_rgb = rgb64[(pal16[(reg10 & 15) as usize] & 63) as usize];
            let vram_base = (reg13 << 11) | (reg14 << 3);

            let sram = &self.emu.bus.memory;
            let read_ram = |addr: u32| -> u8 {
                let z = addr & 0x1FFFF;
                let sram_off = if (z & 0x10000) != 0 {
                    ram_hi_offset + (z & 0xFFFF)
                } else {
                    ram_lo_offset + (z & 0xFFFF)
                };
                if (sram_off as usize) < rp2350_emu::memory::SRAM_SIZE {
                    sram.sram_read8(sram_off)
                } else {
                    0
                }
            };

            if !bp {
                // CoCo 3 80-Column Text Mode
                let lines_per_char = match lpr {
                    0 => 1,
                    1 => 2,
                    2 => 8,
                    3 => 9,
                    4 => 10,
                    5 => 11,
                    _ => 8,
                };
                let active_lines = match lpf {
                    0 => 192,
                    1 => 199,
                    3 => 225,
                    _ => 192,
                };
                let num_rows = (active_lines / lines_per_char).min(25);
                let total_rendered_lines = num_rows * lines_per_char;
                let y_start = ((240usize).saturating_sub(total_rendered_lines)) / 2;

                // Fill border
                out.fill(border_rgb);

                let attr_mode = (cres & 1) != 0;

                for row in 0..num_rows {
                    for font_row in 0..lines_per_char {
                        let y = y_start + row * lines_per_char + font_row;
                        if y >= FB_HEIGHT {
                            break;
                        }
                        let row_y_off = y * FB_WIDTH_640;
                        for col in 0..80 {
                            let cell_offset = if attr_mode {
                                (row * 80 + col) * 2
                            } else {
                                row * 80 + col
                            };
                            let char_code =
                                (read_ram(vram_base + cell_offset as u32) & 0x7F) as usize;
                            let (fgi, bgi) = if attr_mode {
                                let a = read_ram(vram_base + cell_offset as u32 + 1);
                                (8 | ((a >> 3) & 7), a & 7)
                            } else {
                                (1, 0)
                            };

                            let fg_rgb = rgb64[(pal16[fgi as usize] & 63) as usize];
                            let bg_rgb = rgb64[(pal16[bgi as usize] & 63) as usize];

                            let font_line = if font_row < 12 {
                                crate::font_gime::FONT_GIME[char_code * 12 + font_row]
                            } else {
                                0
                            };

                            let px_base = row_y_off + col * 8;
                            for bit in 0..8 {
                                let is_set = (font_line & (1 << (7 - bit))) != 0;
                                out[px_base + bit] = if is_set { fg_rgb } else { bg_rgb };
                            }
                        }
                    }
                }
                return true;
            } else {
                // CoCo 3 640-Pixel Graphics Mode
                let active_lines = match lpf {
                    0 => 192,
                    1 => 199,
                    3 => 225,
                    _ => 192,
                };
                let y_start = ((240usize).saturating_sub(active_lines)) / 2;
                out.fill(border_rgb);

                if cres == 0 {
                    // 1-bit per pixel (640×192, 2 colors)
                    for line in 0..active_lines {
                        let y = y_start + line;
                        if y >= FB_HEIGHT {
                            break;
                        }
                        let row_y_off = y * FB_WIDTH_640;
                        let line_addr = vram_base + (line as u32) * 80;
                        let fg_rgb = rgb64[(pal16[1] & 63) as usize];
                        let bg_rgb = rgb64[(pal16[0] & 63) as usize];

                        for bx in 0..80 {
                            let b = read_ram(line_addr + bx as u32);
                            let px_base = row_y_off + bx * 8;
                            for bit in 0..8 {
                                let is_set = (b & (1 << (7 - bit))) != 0;
                                out[px_base + bit] = if is_set { fg_rgb } else { bg_rgb };
                            }
                        }
                    }
                    return true;
                } else if cres == 1 {
                    // 2-bit per pixel (640×192, 4 colors)
                    for line in 0..active_lines {
                        let y = y_start + line;
                        if y >= FB_HEIGHT {
                            break;
                        }
                        let row_y_off = y * FB_WIDTH_640;
                        let line_addr = vram_base + (line as u32) * 160;

                        for bx in 0..160 {
                            let b = read_ram(line_addr + bx as u32);
                            let px_base = row_y_off + bx * 4;
                            let p0 = rgb64[(pal16[((b >> 6) & 3) as usize] & 63) as usize];
                            let p1 = rgb64[(pal16[((b >> 4) & 3) as usize] & 63) as usize];
                            let p2 = rgb64[(pal16[((b >> 2) & 3) as usize] & 63) as usize];
                            let p3 = rgb64[(pal16[(b & 3) as usize] & 63) as usize];
                            out[px_base] = p0;
                            out[px_base + 1] = p1;
                            out[px_base + 2] = p2;
                            out[px_base + 3] = p3;
                        }
                    }
                    return true;
                }
            }
        }

        // Standard 320-wide fallback: double each pixel horizontally
        let mut fb320 = [0u16; FB_PIXELS];
        self.extract_frame(&mut fb320);
        for y in 0..FB_HEIGHT {
            let src_row = &fb320[y * FB_WIDTH..(y + 1) * FB_WIDTH];
            let dst_row = &mut out[y * FB_WIDTH_640..(y + 1) * FB_WIDTH_640];
            for x in 0..FB_WIDTH {
                let px = src_row[x];
                dst_row[x * 2] = px;
                dst_row[x * 2 + 1] = px;
            }
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
        self.emu
            .bus
            .memory
            .sram_read8(self.symbols.menu_active_addr - 0x2000_0000)
            != 0
    }

    /// Get current active menu mode (0 = Disks, 1 = Programs, 2 = Carts, 3 = Files, 4 = Info).
    pub fn get_menu_mode(&self) -> u32 {
        self.emu
            .bus
            .memory
            .sram_read32(self.symbols.menu_mode_addr - 0x2000_0000)
    }

    /// Read the active audio volume level (0..=15).
    pub fn get_volume(&self) -> u8 {
        if let Some(addr) = self.symbols.settings_addr {
            if (0x2000_0000..0x2008_2000).contains(&addr) {
                // In struct settings, byte offset 10 is volume
                return self.emu.bus.memory.sram_read8(addr + 10 - 0x2000_0000);
            }
        }
        self.prev_volume
    }

    /// Set the active audio volume level (0..=15) in SRAM and internal state.
    pub fn set_volume(&mut self, vol: u8) {
        let v = vol.min(15);
        if let Some(addr) = self.symbols.settings_addr {
            if (0x2000_0000..0x2008_2000).contains(&addr) {
                self.emu.bus.memory.sram_write8(addr + 10 - 0x2000_0000, v);
            }
        }
        if v > 0 {
            self.prev_volume = v;
            self.is_muted = false;
        } else {
            self.is_muted = true;
        }
    }

    /// Process an HID Consumer volume key code:
    /// 0xE9: Volume Up (+1)
    /// 0xEA: Volume Down (-1)
    /// 0xE2: Mute (toggle)
    pub fn adjust_volume(&mut self, keycode: u8) -> u8 {
        let current = self.get_volume();
        let new_vol = match keycode {
            0xE9 => {
                // Volume Up
                if self.is_muted {
                    self.is_muted = false;
                    self.prev_volume.max(1)
                } else {
                    current.saturating_add(1).min(15)
                }
            }
            0xEA => {
                // Volume Down
                if self.is_muted {
                    self.is_muted = false;
                    self.prev_volume.saturating_sub(1)
                } else {
                    current.saturating_sub(1)
                }
            }
            0xE2 => {
                // Mute toggle
                if self.is_muted {
                    self.is_muted = false;
                    self.prev_volume.max(1)
                } else {
                    self.prev_volume = current;
                    self.is_muted = true;
                    0
                }
            }
            _ => current,
        };
        self.set_volume(new_vol);
        new_vol
    }

    /// Drain complete serial telemetry lines received from UART0 console.
    pub fn drain_serial_lines(&mut self) -> Vec<String> {
        let mut log = self.serial_tx_log.lock().unwrap();
        let bytes = log.as_slice();
        let mut lines = Vec::new();
        let mut start = 0;
        for (i, &b) in bytes.iter().enumerate() {
            if b == b'\n' {
                let line_str = String::from_utf8_lossy(&bytes[start..=i]).trim().to_string();
                if !line_str.is_empty() {
                    lines.push(line_str);
                }
                start = i + 1;
            }
        }
        let remainder = if start < bytes.len() {
            bytes[start..].to_vec()
        } else {
            Vec::new()
        };
        *log = remainder;
        lines
    }

    /// Inject a USB HID keycode report into the firmware's TinyUSB host handler on Core 0.
    pub fn inject_hid_keycode(&mut self, keycode: u8) {
        // If keycode is an HID Consumer multimedia volume key (0xE9, 0xEA, 0xE2), adjust volume
        if keycode == 0xE9 || keycode == 0xEA || keycode == 0xE2 {
            let v = self.adjust_volume(keycode);
            eprintln!("[Audio] Volume adjusted to {} (keycode 0x{:02X})", v, keycode);
        }

        // Trampoline at TRAMP_ADDR: 'b .' (0xE7FE)
        self.emu.bus.write16(TRAMP_ADDR, 0xE7FE, 0);

        // Dedicated scratch HID report buffer located right below trampoline in upper SRAM
        let report_addr = TRAMP_ADDR - 0x10;

        // 1. Send Key Press report [modifier, reserved, keycode, 0, 0, 0, 0, 0]
        self.emu.bus.write8(report_addr, 0, 0); // modifier
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
                motor_off_syncs: self.motor_off_syncs,
            },
        }
    }

    /// Extract the active framebuffer and save it to a standard 24-bit BMP image.
    /// In 640-wide modes (such as VIDEO_640 paletted scanout, CoCo 3 WIDTH 80, or 640-pixel graphics), outputs a 640×240 BMP.
    /// In standard modes, outputs a 320×240 BMP.
    pub fn dump_framebuffer_bmp(&mut self, path: impl AsRef<Path>) -> std::io::Result<()> {
        if self.is_paletted() || self.is_wide_mode() {
            let mut fb = vec![0u16; FB_PIXELS_640];
            self.extract_frame_640(&mut fb);
            save_bmp(path, &fb, FB_WIDTH_640, FB_HEIGHT)
        } else {
            let mut fb = [0u16; FB_PIXELS];
            self.extract_frame(&mut fb);
            save_bmp(path, &fb, FB_WIDTH, FB_HEIGHT)
        }
    }

    /// Read the 8 hardware watchdog scratch registers (SCRATCH0..7 at 0x400D800C..=0x400D8028).
    pub fn read_watchdog_scratch(&mut self) -> [u32; 8] {
        let mut out = [0u32; 8];
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = self.emu.bus.read32(0x400D_8000 + 0x0C + (i as u32) * 4, 0);
        }
        out
    }

    /// Read the 8 hardware powman scratch registers (SCRATCH0..7 at 0x401000B0..=0x401000CC).
    pub fn read_powman_scratch(&mut self) -> [u32; 8] {
        let mut out = [0u32; 8];
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = self.emu.bus.read32(0x4010_0000 + 0xB0 + (i as u32) * 4, 0);
        }
        out
    }

    /// Read and parse persistent hardware watchdog and powman post-mortem crash diagnostics.
    pub fn read_watchdog_post_mortem(&mut self) -> Option<WatchdogPostMortem> {
        let wd = self.read_watchdog_scratch();
        let pw = self.read_powman_scratch();
        WatchdogPostMortem::parse(wd, pw)
    }

    /// Format watchdog post-mortem record if present.
    pub fn format_post_mortem(&mut self) -> Option<String> {
        self.read_watchdog_post_mortem().map(|pm| pm.describe())
    }
}

/// Watchdog Sentinel distinguishing valid freeze records from cold power-on state (`0x05330534`).
pub const WD_MAGIC: u32 = 0x05330534;
/// Powman Post-Mortem Sentinel confirming valid post-mortem record (`0x57445031` = 'WDP1').
pub const WDP_MAGIC: u32 = 0x57445031;
/// Sentinel indicating deliberate USB replug reboot (`0x51D15C09`).
pub const USB_REPLUG_MAGIC: u32 = 0x51D15C09;
/// Sentinel indicating deliberate machine switch reboot (`0x53574954` = 'SWIT').
pub const MACHINE_SWITCH_MAGIC: u32 = 0x53574954;

pub const WP_SETUP: u32 = 1;
pub const WP_LOOP: u32 = 2;
pub const WP_USB: u32 = 3;
pub const WP_KBD: u32 = 4;
pub const WP_EMU: u32 = 5;
pub const WP_RENDER: u32 = 6;
pub const WP_BLIT: u32 = 7;
pub const WP_AUDIO: u32 = 8;
pub const WP_PACE: u32 = 9;
pub const WP_TELEM: u32 = 10;
pub const WP_SOAKLOG: u32 = 11;

pub const WDP_NONE: u32 = 0;
pub const WDP_ARMED: u32 = 1;
pub const WDP_HANG: u32 = 2;
pub const WDP_FAULT: u32 = 3;

/// Translate execution phase enum constant to human-readable string.
pub fn phase_name(phase: u32) -> &'static str {
    match phase {
        WP_SETUP => "WP_SETUP",
        WP_LOOP => "WP_LOOP",
        WP_USB => "WP_USB",
        WP_KBD => "WP_KBD",
        WP_EMU => "WP_EMU",
        WP_RENDER => "WP_RENDER",
        WP_BLIT => "WP_BLIT",
        WP_AUDIO => "WP_AUDIO",
        WP_PACE => "WP_PACE",
        WP_TELEM => "WP_TELEM",
        WP_SOAKLOG => "WP_SOAKLOG",
        _ => "WP_UNKNOWN",
    }
}

/// Translate ARM exception/interrupt number to descriptive name.
pub fn irq_name(exc: u32) -> String {
    match exc {
        0 => "thread mode (no interrupt)".to_string(),
        3 => "HardFault".to_string(),
        11 => "SVCall".to_string(),
        14 => "PendSV".to_string(),
        15 => "SysTick".to_string(),
        1..=15 => "a system exception".to_string(),
        _ => {
            let irq = exc - 16;
            match irq {
                0..=3 => format!("TIMER0_IRQ_{}", irq),
                10..=13 => format!("DMA_IRQ_{}", irq - 10),
                14 => "USBCTRL_IRQ".to_string(),
                15 => "PIO0_IRQ_0".to_string(),
                16 => "PIO0_IRQ_1".to_string(),
                17 => "PIO1_IRQ_0".to_string(),
                18 => "PIO1_IRQ_1".to_string(),
                21 => "IO_IRQ_BANK0".to_string(),
                25 => "SIO_IRQ_FIFO".to_string(),
                26 => "SIO_IRQ_BELL".to_string(),
                33 => "UART0_IRQ".to_string(),
                _ => format!("IRQ {}", irq),
            }
        }
    }
}

/// Decoded hardware watchdog and powman post-mortem diagnostic record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WatchdogPostMortem {
    pub raw_watchdog: [u32; 8],
    pub raw_powman: [u32; 8],
    pub wd_magic: u32,
    pub phase: u32,
    pub phase_name: &'static str,
    pub heartbeat_frames: u32,
    pub cumulative_freeze_count: u32,
    pub last_freeze_phase: u32,
    pub usb_replug_magic: u32,
    pub fault_pc: u32,
    pub fault_lr: u32,
    pub kind: u32,
    pub core: u32,
    pub saves: u32,
    pub exception_num: u32,
    pub uptime_ms: u32,
    pub span_ms: u32,
    pub core1_lines: u32,
    pub guest_6809_pc: u16,
    pub usb_devices: u8,
    pub exc_return: u8,
    pub nvic_or_fault: [u32; 4],
}

impl WatchdogPostMortem {
    /// Parse raw scratch register arrays into structured diagnostic fields.
    pub fn parse(wd: [u32; 8], pw: [u32; 8]) -> Option<Self> {
        let wd_magic = wd[0];
        let pw_magic = pw[7];
        if wd_magic != WD_MAGIC && pw_magic != WDP_MAGIC {
            return None;
        }

        let phase = wd[1];
        let heartbeat_frames = wd[2];
        let cumulative_freeze_count = wd[3] >> 8;
        let last_freeze_phase = wd[3] & 0xFF;
        let usb_replug_magic = wd[4];
        let fault_pc = wd[5];
        let fault_lr = wd[6];
        let s7 = wd[7];
        let kind = s7 >> 24;
        let core = (s7 >> 20) & 0xF;
        let saves = (s7 >> 12) & 0xFF;
        let exception_num = s7 & 0x1FF;

        let uptime_ms = pw[0];
        let span_ms = pw[1] >> 16;
        let core1_lines = pw[1] & 0xFFFF;
        let guest_6809_pc = (pw[2] >> 16) as u16;
        let usb_devices = ((pw[2] >> 8) & 0xFF) as u8;
        let exc_return = (pw[2] & 0xFF) as u8;
        let nvic_or_fault = [pw[3], pw[4], pw[5], pw[6]];

        Some(Self {
            raw_watchdog: wd,
            raw_powman: pw,
            wd_magic,
            phase,
            phase_name: phase_name(phase),
            heartbeat_frames,
            cumulative_freeze_count,
            last_freeze_phase,
            usb_replug_magic,
            fault_pc,
            fault_lr,
            kind,
            core,
            saves,
            exception_num,
            uptime_ms,
            span_ms,
            core1_lines,
            guest_6809_pc,
            usb_devices,
            exc_return,
            nvic_or_fault,
        })
    }

    /// Format the diagnostic record into an authoritative post-mortem log matching silicon firmware output.
    pub fn describe(&self) -> String {
        let pw_magic = self.raw_powman[7];
        let kind = self.kind;

        if pw_magic != WDP_MAGIC || kind == WDP_NONE || kind > WDP_FAULT {
            if self.wd_magic == WD_MAGIC {
                let c1_status = if self.core1_lines > 0 { "alive" } else { "stopped" };
                return format!(
                    "[watchdog] post-mortem: core {} stuck at 0x{:08x} in {} (6809 PC=0x{:04X}, core 1 {})",
                    self.core, self.fault_pc, self.phase_name, self.guest_6809_pc, c1_status
                );
            }
            return "[watchdog] post-mortem: no record (a power-on, or a build before PIZERO-227)".to_string();
        }

        if kind == WDP_ARMED {
            return "[watchdog] post-mortem: the early warning never ran: core 0 had interrupts masked, was in a top-priority handler, or the bus stalled".to_string();
        }

        let irq_str = irq_name(self.exception_num);
        if kind == WDP_FAULT {
            let cfsr = self.nvic_or_fault[0];
            let hfsr = self.nvic_or_fault[1];
            let bfar = if (cfsr & (1 << 15)) != 0 {
                format!(" bfar={:08x}", self.nvic_or_fault[2])
            } else {
                String::new()
            };
            let mmfar = if (cfsr & (1 << 7)) != 0 {
                format!(" mmfar={:08x}", self.nvic_or_fault[3])
            } else {
                String::new()
            };
            format!(
                "[watchdog] post-mortem: HARD FAULT on core {} pc={:08x} lr={:08x} in {} cfsr={:08x} hfsr={:08x}{}{} exc_return=..{:02x} guest_pc={:04x} usb={} up={}ms",
                self.core, self.fault_pc, self.fault_lr, irq_str, cfsr, hfsr, bfar, mmfar,
                self.exc_return, self.guest_6809_pc, self.usb_devices, self.uptime_ms
            )
        } else {
            // WDP_HANG
            let c1_status = if self.saves < 2 {
                "unknown"
            } else if self.core1_lines > 0 {
                "running"
            } else {
                "STOPPED"
            };
            format!(
                "[watchdog] post-mortem: hang, core 0 at pc={:08x} lr={:08x} in {}; {} warnings over {} ms, core 1 {} (+{} lines); active={:05x}:{:08x} pending={:05x}:{:08x} exc_return=..{:02x}; guest_pc={:04x} usb={} up={}ms",
                self.fault_pc, self.fault_lr, irq_str, self.saves, self.span_ms, c1_status, self.core1_lines,
                self.nvic_or_fault[1], self.nvic_or_fault[0], self.nvic_or_fault[3], self.nvic_or_fault[2],
                self.exc_return, self.guest_6809_pc, self.usb_devices, self.uptime_ms
            )
        }
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
    pub motor_off_syncs: u64,
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
            "[telemetry] frame={:>6} cycles={:>10} | C0: PC=0x{:08X} wfe={:<5} | C1: PC=0x{:08X} wfe={:<5} | Aud: drained={:<8} prod={:<8} lapped={:<4} wmark={:<4} | SD: r={:<6} w={:<6} flush={:<4} motor_sync={:<4}",
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
            self.storage.motor_off_syncs,
        )
    }
}

/// Save a 16-bit RGB565 framebuffer to a standard 24-bit uncompressed BMP file.
pub fn save_bmp(
    path: impl AsRef<Path>,
    fb: &[u16],
    width: usize,
    height: usize,
) -> std::io::Result<()> {
    if fb.len() < width * height {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "Framebuffer buffer too small: expected {}, got {}",
                width * height,
                fb.len()
            ),
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

/// Structured serial telemetry record parsed from live firmware UART0 stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TelemetryRecord {
    /// Core 0 runtime and frame budget performance (`[run]`)
    Run(String),
    /// Audio ring buffer watermark and drain adjustments (`[aud]`)
    Audio(String),
    /// SN76489 Complex Sound Generator updates (`[csg]`)
    Csg(String),
    /// AY-3-8910 multi-chip sound bank updates (`[ay]`)
    Ay(String),
    /// Orchestra-90CC 8-bit stereo DAC writes (`[orch]`)
    Orch(String),
    /// USB Audio Class streaming delivery and pacing (`[uac]`)
    Uac(String),
    /// Applied configuration settings and volume level (`[settings]`)
    Settings(String),
    /// Unclassified bracketed telemetry record
    Other(String),
}

impl TelemetryRecord {
    /// Parse a single console line into a categorized telemetry record.
    pub fn parse_line(line: &str) -> Option<Self> {
        let trimmed = line.trim();
        if trimmed.starts_with("[run]") {
            Some(TelemetryRecord::Run(trimmed.to_string()))
        } else if trimmed.starts_with("[aud]") {
            Some(TelemetryRecord::Audio(trimmed.to_string()))
        } else if trimmed.starts_with("[csg]") {
            Some(TelemetryRecord::Csg(trimmed.to_string()))
        } else if trimmed.starts_with("[ay]") {
            Some(TelemetryRecord::Ay(trimmed.to_string()))
        } else if trimmed.starts_with("[orch]") {
            Some(TelemetryRecord::Orch(trimmed.to_string()))
        } else if trimmed.starts_with("[uac]") {
            Some(TelemetryRecord::Uac(trimmed.to_string()))
        } else if trimmed.starts_with("[settings]") {
            Some(TelemetryRecord::Settings(trimmed.to_string()))
        } else if trimmed.starts_with('[') && trimmed.contains(']') {
            Some(TelemetryRecord::Other(trimmed.to_string()))
        } else {
            None
        }
    }
}
