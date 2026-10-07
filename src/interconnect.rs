//! Interconnect bus wrapper for RP2350B SoC peripherals:
//! - Intercepts SIO TMDS hardware encoder registers (0xD00001C0..0xD00001E4)
//! - Intercepts PL022 SPI1 controller for MicroSD card (0x40084000..0x40084020)
//! - Captures UART0 serial telemetry (0x40070000)
//! - Monitors MicroSD software CS on GPIO 43

use std::sync::Arc;
use rp2350_emu::bus::Bus;
use rp2350_emu::core::CoreBus;
use rp2350_emu::threaded::CoreAtomics;

use crate::sio_tmds::SioTmds;
use crate::spi_sd::SpiSdCard;

pub const SIO_TMDS_BASE: u32 = 0xD000_01C0;
pub const SIO_TMDS_END: u32 = 0xD000_01E4;

pub const SPI1_BASE: u32 = 0x4008_4000;
pub const SPI1_DR: u32 = SPI1_BASE + 0x08;
pub const SPI1_SR: u32 = SPI1_BASE + 0x0C;

pub const UART0_BASE: u32 = 0x4007_0000;
pub const UART0_DR: u32 = UART0_BASE + 0x00;

pub struct CoCoBus<'a> {
    pub bus: &'a mut Bus,
    pub tmds: &'a mut SioTmds,
    pub sd: &'a mut SpiSdCard,
    pub spi1_rx: u32,
    pub spi1_has_rx: bool,
    pub uart_tx: &'a mut Vec<u8>,
}

impl<'a> CoCoBus<'a> {
    pub fn new(
        bus: &'a mut Bus,
        tmds: &'a mut SioTmds,
        sd: &'a mut SpiSdCard,
        uart_tx: &'a mut Vec<u8>,
    ) -> Self {
        Self {
            bus,
            tmds,
            sd,
            spi1_rx: 0xFF,
            spi1_has_rx: false,
            uart_tx,
        }
    }

    fn read_spi1(&mut self, addr: u32) -> u32 {
        let offset = addr & 0xFFF;
        match offset {
            0x08 => {
                // SSPDR
                let val = self.spi1_rx;
                self.spi1_has_rx = false;
                val
            }
            0x0C => {
                // SSPSR: bits: TFE(0), TNF(1), RNE(2), BSY(4)
                let rne = if self.spi1_has_rx { 0x04 } else { 0x00 };
                0x03 | rne // TFE (1) | TNF (2) | RNE
            }
            _ => self.bus.read32(addr, 0),
        }
    }

    fn write_spi1(&mut self, addr: u32, val: u32) {
        let offset = addr & 0xFFF;
        match offset {
            0x08 => {
                // SSPDR: Transmit byte to SD card
                let reply = self.sd.transfer_byte((val & 0xFF) as u8);
                self.spi1_rx = reply as u32;
                self.spi1_has_rx = true;
            }
            _ => {
                self.bus.write32(addr, val, 0);
            }
        }
    }

    fn check_gpio_cs(&mut self, addr: u32, val: u32) {
        // GPIO 43 is bit 11 in GPIO_HI_OUT registers (0xD000_0014..=0xD000_002C)
        let cs_bit = 1 << (43 - 32);
        match addr {
            0xD000_0014 => {
                // GPIO_HI_OUT direct write
                self.sd.set_cs((val & cs_bit) == 0);
            }
            0xD000_001C => {
                // GPIO_HI_OUT_SET: 1 sets pin high -> CS deasserted
                if (val & cs_bit) != 0 {
                    self.sd.set_cs(false);
                }
            }
            0xD000_0024 => {
                // GPIO_HI_OUT_CLR: 1 sets pin low -> CS asserted
                if (val & cs_bit) != 0 {
                    self.sd.set_cs(true);
                }
            }
            _ => {}
        }
    }
}

impl<'a> CoreBus for CoCoBus<'a> {
    #[inline(always)]
    fn read8(&mut self, addr: u32, core: u8) -> u8 {
        <Bus as CoreBus>::read8(self.bus, addr, core)
    }

    #[inline(always)]
    fn read16(&mut self, addr: u32, core: u8) -> u16 {
        <Bus as CoreBus>::read16(self.bus, addr, core)
    }

    #[inline(always)]
    fn read32(&mut self, addr: u32, core: u8) -> u32 {
        // 1. Intercept SIO TMDS registers (0xD00001C0..=0xD00001E4)
        if (SIO_TMDS_BASE..=SIO_TMDS_END).contains(&addr) {
            return self.tmds.read32(addr & 0xFFF);
        }

        // 2. Intercept SPI1 registers (0x40084000..=0x40084020)
        if addr >= SPI1_BASE && addr <= (SPI1_BASE + 0x20) {
            return self.read_spi1(addr);
        }

        <Bus as CoreBus>::read32(self.bus, addr, core)
    }

    #[inline(always)]
    fn write8(&mut self, addr: u32, val: u8, core: u8) {
        <Bus as CoreBus>::write8(self.bus, addr, val, core)
    }

    #[inline(always)]
    fn write16(&mut self, addr: u32, val: u16, core: u8) {
        <Bus as CoreBus>::write16(self.bus, addr, val, core)
    }

    #[inline(always)]
    fn write32(&mut self, addr: u32, val: u32, core: u8) {
        // 1. Intercept SIO TMDS registers (0xD00001C0..=0xD00001E4)
        if (SIO_TMDS_BASE..=SIO_TMDS_END).contains(&addr) {
            self.tmds.write32(addr & 0xFFF, val);
            return;
        }

        // 2. Intercept SPI1 registers
        if addr >= SPI1_BASE && addr <= (SPI1_BASE + 0x20) {
            self.write_spi1(addr, val);
            return;
        }

        // 3. Monitor UART0 DR for console telemetry
        if addr == UART0_DR {
            self.uart_tx.push((val & 0xFF) as u8);
        }

        // 4. Monitor SIO GPIO for MicroSD software CS
        if (0xD000_0014..=0xD000_002C).contains(&addr) {
            self.check_gpio_cs(addr, val);
        }

        <Bus as CoreBus>::write32(self.bus, addr, val, core)
    }

    #[inline(always)]
    fn set_active_pc(&mut self, pc: u32, core: u8) {
        <Bus as CoreBus>::set_active_pc(self.bus, pc, core)
    }

    #[inline(always)]
    fn bus_fault(&self, core: u8) -> bool {
        <Bus as CoreBus>::bus_fault(self.bus, core)
    }

    #[inline(always)]
    fn bus_fault_addr(&self, core: u8) -> u32 {
        <Bus as CoreBus>::bus_fault_addr(self.bus, core)
    }

    #[inline(always)]
    fn clear_bus_fault(&mut self, core: u8) {
        <Bus as CoreBus>::clear_bus_fault(self.bus, core)
    }

    #[inline(always)]
    fn set_burst_mode(&mut self, on: bool) {
        <Bus as CoreBus>::set_burst_mode(self.bus, on)
    }

    #[inline(always)]
    fn add_extra_wait_states(&mut self, n: u32) {
        <Bus as CoreBus>::add_extra_wait_states(self.bus, n)
    }

    #[inline(always)]
    fn take_extra_wait_states(&mut self) -> u32 {
        <Bus as CoreBus>::take_extra_wait_states(self.bus)
    }

    #[inline(always)]
    fn atomics(&self) -> &Arc<CoreAtomics> {
        <Bus as CoreBus>::atomics(self.bus)
    }

    #[inline(always)]
    fn gpio_read_out(&self) -> u32 {
        <Bus as CoreBus>::gpio_read_out(self.bus)
    }

    #[inline(always)]
    fn gpio_write_out(&mut self, val: u32) {
        <Bus as CoreBus>::gpio_write_out(self.bus, val)
    }

    #[inline(always)]
    fn gpio_set_out(&mut self, mask: u32) {
        <Bus as CoreBus>::gpio_set_out(self.bus, mask)
    }

    #[inline(always)]
    fn gpio_clear_out(&mut self, mask: u32) {
        <Bus as CoreBus>::gpio_clear_out(self.bus, mask)
    }

    #[inline(always)]
    fn gpio_xor_out(&mut self, mask: u32) {
        <Bus as CoreBus>::gpio_xor_out(self.bus, mask)
    }

    #[inline(always)]
    fn gpio_read_oe(&self) -> u32 {
        <Bus as CoreBus>::gpio_read_oe(self.bus)
    }

    #[inline(always)]
    fn gpio_write_oe(&mut self, val: u32) {
        <Bus as CoreBus>::gpio_write_oe(self.bus, val)
    }

    #[inline(always)]
    fn gpio_set_oe(&mut self, mask: u32) {
        <Bus as CoreBus>::gpio_set_oe(self.bus, mask)
    }

    #[inline(always)]
    fn gpio_clear_oe(&mut self, mask: u32) {
        <Bus as CoreBus>::gpio_clear_oe(self.bus, mask)
    }

    #[inline(always)]
    fn gpio_xor_oe(&mut self, mask: u32) {
        <Bus as CoreBus>::gpio_xor_oe(self.bus, mask)
    }

    #[inline(always)]
    fn gpio_read_in(&self) -> u32 {
        <Bus as CoreBus>::gpio_read_in(self.bus)
    }

    #[inline(always)]
    fn extra_wait_states(&self) -> u32 {
        <Bus as CoreBus>::extra_wait_states(self.bus)
    }

    #[inline(always)]
    fn reset_extra_wait_states(&mut self) {
        <Bus as CoreBus>::reset_extra_wait_states(self.bus)
    }

    #[inline(always)]
    fn last_fetch_addr(&self) -> u32 {
        <Bus as CoreBus>::last_fetch_addr(self.bus)
    }

    #[inline(always)]
    fn set_last_fetch_addr(&mut self, addr: u32) {
        <Bus as CoreBus>::set_last_fetch_addr(self.bus, addr)
    }

    #[inline(always)]
    fn mmio_trace_enabled(&self) -> bool {
        <Bus as CoreBus>::mmio_trace_enabled(self.bus)
    }

    #[inline(always)]
    fn emit_mmio_trace(&mut self, rw: char, size: u32, addr: u32, val: u32, core: u8) {
        <Bus as CoreBus>::emit_mmio_trace(self.bus, rw, size, addr, val, core)
    }
}
