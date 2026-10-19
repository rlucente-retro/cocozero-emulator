# CoCo Zero: System Specification & Technical Reference Manual

**Target Platform:** Waveshare RP2350-PiZero  
**Emulated System:** Tandy Color Computer 2 (CoCo 2) with SAM double-speed and selective CoCo 3 enhancements (plus experimental 128 KB CoCo 3 firmware)  
**Reference Firmware:** [ugufru/xroar-waveshare-rp2350-pizero](https://github.com/ugufru/xroar-waveshare-rp2350-pizero)  
**Host Hardware Emulator:** `cocozero-rp2350` (Rust / SDL2 / `rp2350-emu`)  
**Document Version:** 1.3.0  
**Status:** Validated Technical Specification & System Reference  

---

## 1. Executive Summary & Device Concept

The **CoCo Zero** is a standalone, bare-metal hardware implementation of a Tandy Color Computer 2 (Dragon 32/64 compatible) built on a Raspberry Pi Zero form-factor microcontroller board—the **Waveshare RP2350-PiZero**. The device requires only a digital display (TV or monitor), a USB-C keyboard or gamepad, a standard 5V USB-C power source (or 3.7V Li-Po battery), and a FAT32-formatted microSD card containing Color BASIC ROMs.

In addition to bare-metal microcontroller operation, this repository provides **`cocozero-rp2350`**, a full-system hardware emulator written in Rust that models the RP2350B microcontroller (dual Cortex-M33, SIO TMDS, SPI1 SD, Boot ROM, and UART0) executing the unmodified bare-metal CoCo Zero firmware.

The system delivers:
- **Locked 60.0 fps real-time emulation** of the Motorola MC6809E CPU, MC6883 Synchronous Address Multiplexer (SAM), dual MC6821 Peripheral Interface Adapters (PIAs), and MC6847 Video Display Generator (VDG).
- **Hardware-honored SAM double-speed POKEs:** Supports address-dependent high speed (`POKE 65495,0`, ~1.62× speedup) and full double speed (`POKE 65497,0`, 2.00× speedup at 1.79 MHz) at a locked 60.0 fps with clean, noise-free display scanout (avoiding the screen hash of physical CoCo 2 hardware).
- **Experimental 128 KB CoCo 3 platform (`pizero_coco3`):** Dedicated firmware build booting Super Extended Color BASIC (`coco3.rom`) with full vendored GIME memory management, MMU paged fast path, and port-side scanline rendering at both 0.895 MHz and 1.79 MHz at locked 60 fps.
- **Combined video and digital audio over a single mini HDMI/DVI connector** via custom PIO-driven TMDS encoding with 8/92/60 line blanking (providing a compliant 14 px control period) and DVI/HDMI Data Island packets metered across all 523 non-sync lines.
- **High-fidelity audio subsystem:** Strict IEC 60958 subframe formatting, 6× audio gain boost (+15.5 dB), AC-coupling DC blocker, and a configurable 2-pole Butterworth low-pass filter (default 8 kHz) mixing 6-bit DAC audio, 1-bit cassette sound, and a three-voice SN76489 Complex Sound Generator (GMC chip).
- **Synchronous writable `.DSK` floppy storage:** Full write support (`SAVE`, `SAVEM`, `KILL`, and NitrOS-9 Level 1 OS-9 boot) writing directly to `.dsk` files on microSD, with interactive drive write-locking (**W**), new blank formatted disk creation (**N**), and disk renaming (**R**).
- **USB Host input** for standard USB HID keyboards (with US-keycap matrix translation, auto-repeat, and cross-platform function/Command key shortcuts) and gamepads.
- **Comprehensive on-screen display (OSD) overlay system** providing disk mounting, program launching, cartridge selection, integrated text editing (**TAB** on any highlighted game), artifact color toggling (**F8**), and system diagnostics.
- **Configuration and autorun engine** driven by plain-text files on SD card (`settings.txt`, `autorun.txt`, and per-game override files), including `~` delay tokens and automated standalone soak logging (`soak_log = on`).
- **Virtual FAT32 storage engine** capable of dynamically packaging a host directory into an in-memory FAT32 microSD image or mounting raw `.img` files, with full SPI read/write command support (`CMD0`–`CMD59`).
- **Custom 3D-printable two-piece enclosure** faithfully styled after the ventilated chassis of an original Tandy Color Computer 2.

```
+--------------------------------------------------------------------------+
|                  CoCo Zero / RP2350B Execution Model                     |
|                                                                          |
|  +--------------------+    +--------------------+    +----------------+  |
|  |     Core 0 (Host)  |    |    Core 1 (Video)  |    |  MicroSD SPI1  |  |
|  |  * 6809/SAM/PIA/VDG|    |  * libdvi Scanout  |    |  * ROMs / DSK  |  |
|  |  * USB Host / Matrix|   |  * Hardware TMDS   |    |  * FatFS Access|  |
|  |  * OSD Menu Engine |    |  * Audio Islands   |    |  * Virtual     |  |
|  |  * SD FatFS Access |    |  * SIO Encoder     |    |    FAT32 Img   |  |
|  |  * VDG Frame Blit  |    |  * Multicore FIFO  |    |  * Sector Read/|  |
|  |  * Audio Resampler |    |  * Active DMA IRQ  |    |    Write (CMD) |  |
|  +---------+----------+    +---------+----------+    +-------+--------+  |
|            |                         |                       |           |
|            +-----------+-------------+                       |           |
|                        |                                     |           |
|  Physical Hardware:    Mini DVI/HDMI Out, USB Host, SPI1     MicroSD Slot|
|  Host Emulator:        SDL2 640x480 @ 60Hz, AudioQueue,      Virtual SD /|
|                        Dual-Tier Matrix/HID Input, UART0     ./coco/ Dir |
+--------------------------------------------------------------------------+
```

### 1.1 Four-Tier System Architectural Hierarchy

A rigorous understanding of the CoCo Zero platform requires distinguishing between four distinct execution tiers:

```
+---------------------------------------------------------------------------------------------------+
| TIER 4: HOST MICROCONTROLLER EMULATOR (cocozero-rp2350)                                           |
| Host OS (macOS / Linux / Windows) -> Rust Application -> rp2350-emu + SDL2 Presentation           |
| Emulates: RP2350B SoC (Dual Cortex-M33, SIO TMDS, SPI1 SD, Boot ROM, UART0, 520 KB RAM, Flash)     |
+---------------------------------------------------------------------------------------------------+
                                                  | executes compiled UF2/ELF ARM binary
                                                  v
+---------------------------------------------------------------------------------------------------+
| TIER 2: BARE-METAL FIRMWARE (xroar-waveshare-rp2350-pizero)                                      |
| Core 0 (Emulation & I/O): 6809 MPU loop, VDG blit pass, TinyUSB Host, Audio resampler, FatFs SD   |
| Core 1 (Display & Scanout): libdvi rasterization, SIO TMDS encode, HDMI Audio Data Island DMA ISR |
+---------------------------------------------------------------------------------------------------+
                                                  | runs on / targets
                                                  v
+---------------------------------------------------------------------------------------------------+
| TIER 1: PHYSICAL MICROCONTROLLER BOARD (Waveshare RP2350-PiZero)                                 |
| Silicon: Raspberry Pi RP2350B (QFN-80, 48 GPIO, 520 KB on-chip SRAM, 16 MB QSPI NOR Flash)        |
| Board: Mini-HDMI (GPIO 32-39), USB-C Host (GPIO 28-29), MicroSD SPI1 (GPIO 30,31,40,43)          |
+---------------------------------------------------------------------------------------------------+
                                                  ^
                                                  | virtual hardware modeled by Tier 2
+---------------------------------------------------------------------------------------------------+
| TIER 3: EMULATED GUEST SYSTEM (Tandy Color Computer 2 / 3)                                       |
| Microprocessor: Motorola MC6809E CPU @ 0.895 MHz / 1.79 MHz (quadrature clocks E & Q)            |
| Address Multiplexer: Motorola MC6883 SAM (or GIME TCC1014 for CoCo 3)                            |
| I/O & Peripherals: Dual Motorola MC6821 PIAs, MC6847 VDG, WD2797 FDC, SN76489 CSG                 |
| Software: Color BASIC 1.2 / Extended BASIC 1.1 / Disk BASIC 1.1 / NitrOS-9 Level 1               |
+---------------------------------------------------------------------------------------------------+
```

| Execution Tier | Execution Substrate | Primary Function / Scope | Key Implementation Components |
|---|---|---|---|
| **Tier 1: Physical Microcontroller** | Waveshare RP2350-PiZero hardware PCB | Physical silicon, voltage regulation, physical connectors (mini-HDMI, dual USB-C, microSD). | RP2350B SoC, ETA6096 charger, TMI3112H buck, GPIO routing. |
| **Tier 2: Bare-Metal Firmware** | Dual ARM Cortex-M33 @ 252 MHz (Bare-metal C/C++) | Native firmware bridging guest 8-bit machine to RP2350 peripherals. | Core 0 run loop, Core 1 `libdvi`, Pico-PIO-USB, TinyUSB, FatFs, SIO TMDS. |
| **Tier 3: Emulated Guest System** | Virtual 8-bit Tandy Color Computer architecture | Emulated 6809E hardware running vintage software (ROMs, disks, OS-9). | `xroar_core` (MC6809E, MC6883 SAM, MC6821 PIAs, MC6847 VDG, WD2797 FDC). |
| **Tier 4: Host Hardware Emulator** | Host PC (x86_64 / aarch64) in Rust (`cocozero-rp2350`) | Simulates the RP2350B SoC to execute the Tier 2 firmware on desktop workstations. | `rp2350-emu` (dual Cortex-M33, SIO, SPI1, Flash, SRAM, NVIC, UART0), SDL2 UI. |

---

## 2. Hardware Architecture & Board Specifications

### 2.1 Microcontroller Specifications (RP2350B)

| Parameter | Specification | Notes |
|---|---|---|
| **Microcontroller IC** | Raspberry Pi RP2350B | 48 GPIO variant (QFN-80 package) |
| **CPU Cores** | Dual ARM Cortex-M33 @ 252 MHz | Overclocked from stock 150 MHz; Hardware FPU & DSP |
| **Core Voltage ($V_{REG}$)** | 1.25 V | Programmed via `vreg_set_voltage(VREG_VOLTAGE_1_25)` for 252 MHz stability |
| **System Clock ($f_{SYS}$)** | 252.000 MHz | Driven from TMDS bit clock; integer multiple of 48 MHz USB clock ($252/48 = 5.25$) |
| **On-Chip SRAM** | 520 KB | Unified low-latency system SRAM (SRAM0–SRAM7 + Scratch X/Y) |
| **Flash Memory** | 16 MB QSPI NOR Flash | Winbond W25Q128JVSI (Quad SPI XIP) |
| **External PSRAM** | None (unpopulated PCB footprint) | System operates exclusively within the 520 KB on-chip SRAM |
| **Watchdog Timer** | Hardware Watchdog | 3000 ms timeout; early-warning interrupt & fault handler preserve post-mortem context in persistent scratch registers (PIZERO-227) |
| **Silicon Revision** | RP2350 Rev A2 / B2 (Chip ID checked) | Chip revision inspected at boot for errata handling (e.g. E9 pull-down leakage) |

### 2.2 Power Delivery & Charging Subsystem

The board incorporates comprehensive power management supporting both USB-C power and single-cell Lithium-Ion/Polymer batteries:
- **Battery Management IC:** ETA Solutions **ETA6096** switching lithium battery charger (with charging status LED `STAT`).
- **Battery Connector (`J3`):** 2-pin 1.25mm pitch JST connector for 3.7V Li-Po cells.
- **System Buck Regulator:** **TMI3112H** high-efficiency 1.5 MHz synchronous step-down buck converter delivering a regulated **3.3V system rail** from either 5V VBUS or battery VBAT.
- **Core LDO / SMPS:** RP2350 internal switching regulator stepped up to **1.25V** core supply.
- **Power Consumption:** Measured typical current draw is approximately 140–180 mA @ 5V (~0.8 W) during active 60 fps emulation with digital audio.

### 2.3 Board Form Factor & Physical Layout

The Waveshare RP2350-PiZero matches the physical outline of a Raspberry Pi Zero (65.0 mm × 30.0 mm PCB, 58.0 mm × 23.0 mm M2.5 mounting hole grid). Its physical port layout is:
- **Port `J4` (Power & Debug):** USB Type-C connector wired to the RP2350 native USB 1.1 controller (pins 66/67). Provides 5V power, UF2 / picotool flashing, and bidirectional USB-CDC serial communications (115200 baud).
- **Port `J2` (USB Host):** Dedicated USB Type-C connector with 5.1 kΩ pulldowns wired to GPIO 28 (`D+`) and GPIO 29 (`D-`) through 27 Ω series damping resistors, driven as a full-speed USB 1.1 host by Pico-PIO-USB.
- **Display Port `J1`:** Mini HDMI connector (Type C HDMI) carrying DVI digital video signals directly from GPIOs 32–39.
- **Storage `U5`:** Push-pull MicroSD card slot wired to SPI1.
- **Buttons:**
  - `RUN`: Hardware reset button (pulls RUN low, tracked by RP2350 `POWMAN_CHIP_RESET_HAD_RUN_LOW_BITS`).
  - `BOOT`: Hardware BOOTSEL entry button (sampled on reset).
- **Status Indicators:** One WS2812 RGB LED attached to GPIO 2; one red power LED (`D1`); one charge status LED (`STAT`).
- **Debug Header (`J6`):** 3-pin 2.54mm pitch header exposing SWDIO, SWCLK, and GND for ARM Serial Wire Debug.

### 2.4 Comprehensive Pinout & GPIO Matrix

The RP2350B features 48 GPIO pins (GPIO 0 to GPIO 47). The pin assignments for the CoCo Zero firmware are specified below:

| GPIO Pin | Function Name | Direction | Peripheral / Subsystem | Description |
|---|---|---|---|---|
| **GPIO 0** | `UART0_TX` | Output | UART0 | Primary hardware serial transmit (115200 baud) |
| **GPIO 1** | `UART0_RX` | Input | UART0 | Primary hardware serial receive |
| **GPIO 2** | `WS2812_LED` | Output | PIO / PWM | On-board status addressable RGB LED |
| **GPIO 6** | `I2C0_SDA` | Bidirectional | I2C0 | I2C bus serial data |
| **GPIO 7** | `I2C0_SCL` | Output | I2C0 | I2C bus serial clock |
| **GPIO 22** | `SD_CD` | Input | GPIO | MicroSD card detect switch (hardware present; card polled at mount) |
| **GPIO 28** | `USB_HOST_DP` | Bidirectional | PIO 1 (Pico-PIO-USB) | USB Host D+ signal (USB 1.1 Full Speed, 12 Mbps) |
| **GPIO 29** | `USB_HOST_DM` | Bidirectional | PIO 1 (Pico-PIO-USB) | USB Host D− signal (always `HOST_PIN_DP + 1`) |
| **GPIO 30** | `SD_SCK` | Output | SPI1 | MicroSD clock (12.5 MHz, 12 mA drive strength) |
| **GPIO 31** | `SD_MOSI` | Output | SPI1 | MicroSD master-out-slave-in (2 mA drive strength) |
| **GPIO 32** | `DVI_D2_P` | Output | PIO 0 (`libdvi`) | DVI TMDS Data Lane 2 positive (+) [Red] |
| **GPIO 33** | `DVI_D2_N` | Output | PIO 0 (`libdvi`) | DVI TMDS Data Lane 2 negative (−) [Red] |
| **GPIO 34** | `DVI_D1_P` | Output | PIO 0 (`libdvi`) | DVI TMDS Data Lane 1 positive (+) [Green] |
| **GPIO 35** | `DVI_D1_N` | Output | PIO 0 (`libdvi`) | DVI TMDS Data Lane 1 negative (−) [Green] |
| **GPIO 36** | `DVI_D0_P` | Output | PIO 0 (`libdvi`) | DVI TMDS Data Lane 0 positive (+) [Blue / Sync] |
| **GPIO 37** | `DVI_D0_N` | Output | PIO 0 (`libdvi`) | DVI TMDS Data Lane 0 negative (−) [Blue / Sync] |
| **GPIO 38** | `DVI_CLK_P` | Output | PIO 0 / PWM | DVI TMDS Clock Lane positive (+) |
| **GPIO 39** | `DVI_CLK_N` | Output | PIO 0 / PWM | DVI TMDS Clock Lane negative (−) |
| **GPIO 40** | `SD_MISO` | Input | SPI1 | MicroSD master-in-slave-out (pull-up disabled) |
| **GPIO 43** | `SD_CS` | Output | GPIO | MicroSD software chip select (active low, 2 mA drive) |
| **GPIO 44** | `DDC_SDA` | Bidirectional | I2C | Video display DDC / EDID data (reserved) |
| **GPIO 45** | `DDC_SCL` | Output | I2C | Video display DDC / EDID clock (reserved) |

### 2.5 40-Pin Expansion Header (`J5`)

The board features a 40-pin 2.54mm pitch dual-row expansion header matching the Raspberry Pi Zero mechanical form factor. GPIO 0 through GPIO 27 are broken out according to standard Raspberry Pi numbering. GPIO 30 through GPIO 47 are reserved internally on the PCB for SD SPI, DVI TMDS, and USB host.

> [!IMPORTANT]
> **HSTX Serializer Incompatibility:** The RP2350 silicon incorporates a dedicated hardware high-speed serializer (HSTX). However, HSTX outputs are hardwired strictly to **GPIO 12 through GPIO 19**. Because the Waveshare RP2350-PiZero wires the mini video connector to **GPIO 32 through GPIO 39**, HSTX is electrically unreachable. Consequently, video generation must be executed entirely via PIO state machines using `libdvi` with `pio_set_gpio_base(pio, 16)`.

### 2.6 Host RP2350B Hardware Emulator Architecture (`cocozero-rp2350`)

The host emulator (`cocozero-rp2350`) operates at **Tier 4** of the architecture. It models the physical RP2350B microcontroller SoC through modular Rust crates (`rp2350-emu` and `cocozero-rp2350`), executing the unaltered compiled ARM Cortex-M33 firmware binary (`cocozero.uf2` or companion ELF):

> [!IMPORTANT]
> **Emulation Boundary Clarification:** The host emulator does **not** interpret or execute MC6809E machine code directly. Instead, it emulates the ARM Cortex-M33 dual-core processor, the RP2350 system bus, and its on-chip hardware peripherals. The Motorola MC6809E CPU, SAM address multiplexer, VDG, and PIAs exist purely as emulated guest structures inside the compiled C/C++ firmware running within the simulated Cortex-M33 Core 0 execution engine.

- **Dual Cortex-M33 Execution Engine:** Dual cores executed in round-robin quantum slices (`step_quantum = 512` cycles). Cores feature complete Thumb-2 instruction sets, MPU, NVIC interrupt controller, SysTick timers, and inter-core memory barrier synchronization (`DMB`, `DSB`, `ISB`).
  - *Granularity Tradeoff:* A 512-cycle quantum balances host CPU execution efficiency against fine-grained inter-core FIFO and SIO interaction fidelity.
- **Boot ROM & Multicore Handshake:**
  - Loads authentic RP2350 Boot ROM (`roms/rp2350/bootrom-combined.bin`).
  - Core 1 initiates in a low-power `WFE` (Wait For Event) wait state until Core 0 executes the official Pico SDK multicore launch sequence via SIO FIFO.
  - *Security Canary Decision:* The emulator pre-populates atomic canary salt registers (`rcp_salt_set(0, 0xCAFE_BABE)`, `rcp_salt_set(1, 0xDEAD_BEEF)`). Without this initialization, Core 1's Boot ROM monitor rejects Core 0's multicore wakeup signal and remains permanently in `WFE`, halting video and scanout generation.
  - When booting UF2 directly without Boot ROM, the emulator initializes Core 0 vector registers directly from the Flash image vector table (`initial_sp` and `entry_point`).
- **SIO (Single-Cycle I/O) Subsystem Emulation:**
  - **Hardware TMDS Encoder:** Emulates RP2350 SIO hardware TMDS encoder registers (`TMDS_CTRL`, `TMDS_WDATA`, `TMDS_PEEK_SINGLE`), accelerating 3-lane RGB565 scanline encoding in Core 1.
  - **Multicore FIFO:** Models the bi-directional 8-deep FIFO (`FIFO_ST`, `FIFO_WR`, `FIFO_RD`) with automatic NVIC cross-core IRQ pending generation (IRQ 15/16).
  - **GPIO Output Registers:** Monitors `GPIO_HI_OUT` bit 11 (GPIO 43) to track software Chip Select assertions for the MicroSD controller.
- **Memory Bus Dispatcher:** 520 KB unified on-chip SRAM (`0x2000_0000..0x2008_2000`) with mirror alias decoding, 16 MB QSPI NOR Flash (`0x1000_0000..0x1100_0000`), PPB, and Boot RAM.
- **Hardware UART0 Interception:** Intercepts byte writes to UART0 TX (`0x4003_4000`), printing characters directly to host terminal `stdout` and appending to an internal circular telemetry buffer (`serial_tx_log`).
- **Framebuffer Tap:** Extracts the $320 \times 240$ RGB565 framebuffer directly from RP2350 SRAM (`_ZL4g_fb`) each frame quantum, presenting it via SDL2 at 60 Hz. This bypasses host-side TMDS decoding while allowing Core 1's TMDS hardware encoder to execute faithfully to preserve multi-core synchronization and DMA timing.
- **Simulation Turbo Multiplier (`--turbo <N>`):**
  - Scales the per-frame virtual cycle slice: $\text{Cycles per frame} = \frac{252,000,000 / 60}{N}$.
  - *Performance Tradeoff:* Simulating 252,000,000 cycles/sec across dual cores in software can exceed real-time budgets on host CPUs. At `--turbo 6`, virtual frame execution achieves real-time 60.0 FPS on standard desktop and laptop processors.
  - *Timing Invariance:* Internal keyboard matrix debouncing, hold countdowns, and gap delays are dynamically multiplied by $N$ ($4 \times T$ hold, $2 \times T$ gap), preserving correct virtual-time latching inside Color BASIC regardless of host simulation speed.

---

## 3. Video Subsystem Specification

### 3.1 Display Geometry & Framebuffer Architecture

A native 640×480 RGB565 framebuffer consumes $640 \times 480 \times 2 = 614,400\text{ bytes}$ (~600 KB), which exceeds the entire 520 KB SRAM capacity of the RP2350. To operate reliably within SRAM alongside emulation memory, audio buffers, and USB stacks, the CoCo Zero uses a hardware line-doubled and pixel-doubled scanout architecture:

1. **Internal Framebuffer Resolution:** $320 \times 240$ pixels, 16-bit RGB565 format ($320 \times 240 \times 2 = 153,600\text{ bytes}$ or ~150 KB).
2. **CoCo 2 VDG Display Pipeline:** The MC6847 VDG renders a native display of $256 \times 192$ pixels nibble-packed (2 pixels per byte = 24,576 bytes).
   - Centering: The active raster is centered inside the $320 \times 240$ framebuffer at offset $(X=32, Y=24)$.
   - Borders: 32 pixels left/right, 24 lines top/bottom. The static border is painted once at initialization and never re-cleared, eliminating 27,648 redundant pixel writes per frame.
3. **CoCo 3 Direct Scanline Render Pipeline (`pizero_coco3`):** In the CoCo 3 firmware, the intermediate VDG buffer and blit passes are eliminated. The port-side GIME scanline renderer (`port_render_scanline`, PIZERO-218) hooks horizontal sync, decodes VRAM directly into $320 \times 240$ RGB565 scanlines via the 64-entry GIME palette table (`coco3_bus_render_line`), and skips scanlines whose VRAM and mode state hash (`port_sum`) matches the previous frame.
4. **Hardware Scanout Scaling:**
   - Pixel doubling: 2 TMDS symbols per word (`DVI_SYMBOLS_PER_WORD = 2`).
   - Line doubling: Scanline repeat factor of 2 (`DVI_VERTICAL_REPEAT = 2`).
   - Monitor Active Visible Area: $640 \times 480$ pixels.
   - Visible CoCo Active Area on Monitor: $512 \times 384$ pixels centered with black border margins (or full 320-pixel active field in CoCo 3 modes).

```
       +----------------------- 640 px (320 px doubled) ---------------------+
       |                                                                      |
       |  Top Border: 48 physical lines (24 logical lines)                    |
       |  +----------------- 512 px (256 px doubled) -------------------+     |
       |  |                                                             |     |
       |  |                                                             |     |
480 px |  |  CoCo 2 Active Display: 384 physical lines                  |     |
(240 L)|  |  (192 logical scanlines)                                    |     |
       |  |                                                             |     |
       |  |                                                             |     |
       |  +-------------------------------------------------------------+     |
       |  Bottom Border: 48 physical lines (24 logical lines)                 |
       |                                                                      |
       +----------------------------------------------------------------------+
```

### 3.2 Video Timing & Clock Reconciliation

The standard CEA-861 640×480p @ 60 Hz video mode calls for a 25.175 MHz pixel clock ($10 \times \text{pixel clock} = 251.75\text{ MHz}$ TMDS bit clock). Concurrently, the Pico-PIO-USB software host stack requires an exact integer or fractional divider from sysclk to synthesize a 48 MHz USB bit clock.

By setting the system clock to **252.000 MHz** and operating with a **25.200 MHz pixel clock**, both subsystems achieve optimal timing:
- **USB Divider:** $252\text{ MHz} / 48\text{ MHz} = 5.250$ (represented with zero jitter in the PIO 16.8 fractional clock divider).
- **Line Geometry:** Total horizontal period of 800 pixel clocks, total vertical lines of 525.
- **Refresh Rate:** $\frac{25,200,000}{800 \times 525} = \mathbf{60.000\text{ Hz}}$ exact.

#### Complete Video Timing Matrix (`pizero_stream_60`, `AV_HDMI_WIDE_BP`)

The product build incorporates revised horizontal blanking parameters (`8/92/60`, PIZERO-233) to strictly enforce HDMI electrical and protocol compliance:
- **HDMI Control Period Compliance:** The HDMI specification mandates a minimum **12-pixel control period** between the trailing guard band of an audio data island and the video preamble. At the legacy 56-pixel back porch, the available control window was only 10 pixels (1 word + 8 px video preamble), causing clock recovery failure and audio distortion on sensitive monitors and televisions.
- **Pulse Borrowing:** Expanding the back porch to 60 pixels provides a compliant **14-pixel control period**. The 4 extra pixels are borrowed from the horizontal sync pulse (reducing sync from 96 px to 92 px), keeping total line duration invariant at exactly 800 pixels ($60.000\text{ Hz}$). *(Note: Borrowing from the front porch was rejected as 4/96/60 caused sync loss on MS2109 HDMI capture devices).*

| Timing Parameter | Horizontal (Pixels) | Vertical (Lines) | Notes |
|---|---|---|---|
| **Active Video** | 640 | 480 | Line doubled from 240 logical scanlines |
| **Front Porch** | 8 | 10 | Preserves tight front porch for capture card stability |
| **Sync Pulse** | 92 | 2 | Reduced by 4 px to provide back-porch timing margin |
| **Back Porch** | 60 | 33 | 30 TMDS words: 14 px control period before video preamble |
| **Total** | 800 | 525 | Frame period: 16.667 ms ($16,666.7\ \mu\text{s}$) |

### 3.3 TMDS Encoding & Core 1 Scanout Engine

- **Lane Pin Allocation (`pico_sock_cfg`):**
  - State Machine 0 $\rightarrow$ TMDS Data Lane 0 (Blue / Sync) on GPIO 36 (+), GPIO 37 (−)
  - State Machine 1 $\rightarrow$ TMDS Data Lane 1 (Green) on GPIO 34 (+), GPIO 35 (−)
  - State Machine 2 $\rightarrow$ TMDS Data Lane 2 (Red) on GPIO 32 (+), GPIO 33 (−)
  - PWM Slice $\rightarrow$ TMDS Clock on GPIO 38 (+), GPIO 39 (−)
- **SIO Hardware TMDS Acceleration:** Core 1 uses the RP2350 SIO (Single-cycle I/O) hardware TMDS encoder (`tmds_encode_16bpp_sio_doubled`), encoding all 3 color lanes simultaneously in ~12.6 µs per line.
- **Software Encoder Fallback:** libdvi table encoder (`tmds_encode_data_channel_16bpp` ×3) available via `video_encoder = software` in `settings.txt` (~25.2 µs per line).
- **Single-Buffering Architecture:** Due to SRAM constraints when allocating audio data island pools, production builds define `AV_DATA_ISLAND` and single-buffer the framebuffer (`g_front = g_fb`). Core 0 blits directly to `g_fb` while Core 1 scans out, avoiding starvation glitches at the cost of negligible tearing during blits.

### 3.4 Color Palettes & VDG Display Modes

#### Palette Support
1. **Stock MC6847 VDG Palette:** Authentic 16-color composite palette mapping green, yellow, blue, red, white, cyan, magenta, orange, black, and background shades.
2. **NTSC Artifact Color Simulation:** In PMODE 4 high-resolution graphics ($256 \times 192$ monochrome), composite television color subcarrier phase interference creates color artifacts:
   - `artifact_colors = on`: Blue and orange artifact rendering.
   - `artifact_colors = swapped`: Inverted color phase (orange and blue reversed).
   - `artifact_colors = off`: Clean monochrome black-and-white rendering.
   - Interactive toggle via **F8** key cycles through modes and writes the active selection to disk.
3. **CoCo 3 GIME Palette Registers ($FFB0–$FFBF):** 16-entry programmable palette registers fully emulated. Software writes to `$FFB0–$FFBF` update the RGB565 hardware lookup table live.
   - Format: Interleaved 6-bit RGB (`bit 5: R1`, `bit 4: G1`, `bit 3: B1`, `bit 2: R0`, `bit 1: G0`, `bit 0: B0`).
   - Authentic non-linear DAC transfer curve: intensities scaled to 8-bit $\{0, 117, 188, 235\}$ (representing $0.000, 0.460, 0.736, 0.920$).
4. **User Palette Customization:** Any palette entry can be overridden in `settings.txt` or `NAME.TXT` using `#RRGGBB` hex values (e.g. `color_green = #1ED01E`).

#### Character Fonts
- **`classic`:** Original MC6847 character ROM (uppercase letters only; lowercase displayed as inverted uppercase; `^` displayed as up-arrow; `_` displayed as left-arrow).
- **`6847t1`:** CoCo 2B character set supporting true lowercase when enabled via software write `POKE 65314,16`.
- **`6847t2` (Default):** Enhanced character set providing true lowercase permanently, true ASCII caret `^`, true underscore `_`, and full braces `{`, `|`, `}`, `~`.
- **`lowercase = on` (Default):** Overrides Color BASIC's automatic prompt resets, maintaining lowercase display active even after `PRINT` statements and prompt redrawing.

### 3.5 Host Emulator Display Pipeline & Dynamic Symbol Resolution

The host emulator bridges RP2350 SRAM video generation to modern host display servers using SDL2:
- **Dynamic Symbol Resolution:** To accommodate upstream firmware rebuilds without recompiling the emulator, the engine automatically extracts critical symbol addresses at startup via `FirmwareSymbols`:
  1. *Companion ELF Analysis:* If a companion ELF file exists (`roms/cocozero.elf` or `--elf <PATH>`), the emulator parses the 32-bit ELF symbol table (`.symtab`), extracting `_ZL4g_fb`, `_ZL17g_kb_col_row_mask`, `_ZL5g_ovk`, and `tuh_hid_report_received_cb` in < 1 ms.
  2. *In-Flash Signature Scanning:* If only raw UF2 flash data is supplied, the engine scans the flash image for invariant opcode and literal pool signatures.
  3. *Fallback / User Override:* If symbols cannot be discovered, defaults are used (`0x2002_2900`), and users can override explicitly via `--fb-addr <ADDR>`.
- **Extraction Mechanism:** Each simulated frame quantum, `extract_frame()` reads 32-bit words from SRAM offset, unpacking them into a host-side array of $320 \times 240$ 16-bit RGB565 pixels (153,600 bytes).
- **SDL2 Presentation:** The extracted frame locks an SDL2 streaming texture (`PixelFormatEnum::RGB565`), presenting it on a centered $640 \times 480$ window at 60 Hz.
- **Headless Mode (`--headless`):** Initializes an invisible window and bypasses frame presentation, enabling rapid continuous integration testing and automated regression benchmarks.

---

## 4. Audio Subsystem Specification

### 4.1 Digital Audio Transport via TMDS Data Islands

The CoCo Zero features no analog audio jack or PWM low-pass filter. Instead, digital audio is transmitted over the digital video link through **HDMI/DVI Data Islands** embedded within the TMDS signal.

```
Scanline Structure (Active Line Back Porch):
[ Preamble: 4w ][ Guard: 1w ][ Data Island: 16w ][ Guard: 1w ][ Video Pre: 4w ][ Guard: 1w ][ Active Video: 320w ]
|<----------------------- Back Porch: 30 words (60 px) ------------------------------------>|
|<-- Audio Island: 18 words (36 px) -->|<------------- Video Preamble & Guard: 5w --------->|
                                       |<-- Control Period: 7 words (14 px) -->|
```

1. **Packet Structure, Parity & IEC 60958 Framing:**
   - 32-byte Data Island packets containing 4-byte headers and 4 subpackets of 7 bytes each.
   - Encoded using TERC4 (Transition Minimized 4-to-10 bit encoding) and BCH error-correction parity (`dvi_data_island.c`).
   - **IEC 60958 Compliance (PIZERO-223):** Sample packets strictly format consumer audio subframes: validity bit $V=0$, a 48 kHz / 16-bit channel status table transmitted bit-by-bit per frame, and the block-start flag ($B=1$) asserted on the first subpacket of each 192-frame block. (Legacy firmware sent $V=1$ without channel status, which caused modern TVs to treat audio as untrusted or fallback 44.1 kHz).
2. **Horizontal & Vertical Delivery Architecture (`AV_VBLANK_AUDIO`, PIZERO-232):**
   - **523-Line Continuous Delivery:** Audio delivery is metered across **all 523 non-sync lines** (480 active scanlines + 43 non-sync vertical blanking lines). This eliminates the ~1.4 ms inter-frame silence gap of active-line-only transmission that caused audio dropouts and buzzing on sensitive sinks.
   - **Full 4-Sample Packets (`AV_AUDIO_FULL_PACKETS`):** Rather than fragmenting samples into 1–2 sample packets across every line, the meter dispatches strictly **full 4-sample packets** once every ~2.4–2.6 lines, mirroring standard commercial HDMI audio transmitters.
   - **Active Lines:** Audio islands are carried within the 60-pixel (30-word) horizontal back porch.
   - **Vblank Lines:** Vblank lines carry an audio island within their active block (never in the vblank back porch, which disrupts sync on some monitors, PIZERO-34).
   - **Strict Framing (`AV_HDMI_STRICT_FRAMING`):** A 2-pixel trailing guard band is appended after every vblank data island.
3. **InfoFrames & Clock Regeneration:**
   - Auxiliary Video Information (AVI) InfoFrame (declaring 640×480p60 VIC 1 or 0).
   - Audio InfoFrame (2-channel, 48 kHz, 16-bit uncompressed LPCM).
   - Audio Clock Regeneration (ACR) packets: Transmitted on every 8th vblank line (`AV_VBLANK_INFO_EVERY = 8`), generating ~840 ACR packets/sec at $CTS = 25200$, $N = 6144$ for 25.200 MHz pixel clock.
4. **Exact Rate Metering:**
   - The sample accumulator (`g_meter_acc`) steps in exact 480ths of a sample (`step = AUDIO_SAMPLES_PER_FRAME = 800`), delivering **exactly 48,000 samples per second** (eliminating the 47,999.7 sample truncation of 16.16 fixed-point math).
   - Core 1 encodes each audio island in the DMA interrupt service routine (ISR) **one line ahead of the electron beam**, consuming ~4.8 µs per encode within a ~33 µs line window.
   - Rotating back-porch buffer pool (6 buffers) reclaims ~126 KB SRAM compared to legacy static pre-encoded banks.

### 4.2 Proportional Closed-Loop Audio Servo (`audio_servo.h`)

The audio producer (Core 0 emulation) runs on emulated MC6809 cycles ($14.31818\text{ MHz} / 16$), while the consumer (Core 1 DVI scanout) runs on the physical TMDS clock ($25.200\text{ MHz}$). Due to oscillator drift and fractional cycle rounding, the two clocks differ by approximately $0.02\%$ (~10 samples/second).

To prevent long-term buffer underrun (audio drops) or overrun (lost packets):
- The firmware executes a **proportional closed-loop servo** (`audio_servo_rate`) sampled each frame.
- Midpoint target: $\frac{\text{Capacity}}{2} = 4,096\text{ samples}$ within the 8,192-sample SPSC ring.
- Error calculation: $err = fill - target$.
- Rate adjustment: $adj = -err / 32$, clamped to $\pm 64\text{ samples/sec}$ (max 0.13% adjustment, $<0.025$ semitones, completely inaudible).
- This drives the steady-state frequency error to zero, stabilizing the ring level indefinitely without audio distortion. In extended soak testing (PIZERO-168), the servo maintained lock over 26.7 hours with **0 buffer skips or underruns**.

### 4.3 Emulated Audio Sources & Signal Conditioning

| Audio Source | Architecture | Hardware Register | Emulation Details |
|---|---|---|---|
| **Native 6-bit DAC** | Resistor ladder DAC | PIA1 Port A (`$FF20`) | Level tracked on every PIA write; resampled via integrate-and-dump filter into 48 kHz 16-bit signed mono stream (`AUDIO_DAC_GAIN = 360`). |
| **1-Bit Cassette Audio** | Single-bit flip-flop | PIA1 Port B Bit 3 (`$FF22`) | Cassette sound output mixed into DAC audio path. |
| **SN76489 CSG** | 4-channel sound generator | `$FF41` (or GMC cart `$FF40–$FF5F`) | 3 square wave tone generators + 1 noise generator (white/periodic). Integrated into 48 kHz stream (`AUDIO_GAIN_X = 6`); volume attenuation (0–15, default 10). |

#### Output Signal Shaping & Filtering Pipeline
Before audio samples enter the 8,192-sample SPSC ring buffer, they pass through a dedicated three-stage audio conditioning pipeline (`audio_emit`, `lib/coco_machine/src/audio_lowpass.h`):
1. **AC-Coupling DC Blocker (`audio_dc_run`, PIZERO-223):** A single-pole high-pass filter ($f_c \approx 5\text{ Hz}$) that removes DC offsets caused by static DAC levels. Like a physical CoCo 2's analog capacitor-coupled output, this centers waveforms around zero and prevents a parked DAC value from permanently stealing dynamic range.
2. **Two-Pole Butterworth Low-Pass Filter (`audio_lp_run`, PIZERO-212):** Emulates the acoustic rolloff of an authentic CRT television speaker. Configurable via `audio_lowpass` in `settings.txt` (default `8000` Hz; range `1000`–`20000` Hz or `off`). Eliminates the harsh, fatiguing edge of discrete 6-bit DAC voltage transitions while preserving clear musical harmonics.
3. **Amplification & Headroom Policy (PIZERO-223, PIZERO-230):**
   - Output gain has been increased by **6× (+15.5 dB)** relative to early firmware builds (tripled in PIZERO-223, doubled in PIZERO-230).
   - At the default volume (`volume = 10`), a full-scale CoCo DAC swing produces a peak output of approximately $-3\text{ dBFS}$.
   - Volumes 11 through 15 provide additional digital gain for quiet titles, with a caveat that complex multi-voice music (such as simultaneous SN76489 and 6-bit DAC playback) may clip at maximum volume settings.

### 4.4 Host Emulator Audio Subsystem

In the host emulator, digital audio is routed to host speakers through the SDL2 audio pipeline:
- **Audio Queue Specification:** Configured for 48,000 Hz sample rate, 2-channel stereo, 16-bit signed integer format (`AudioFormat::S16LSB`), and a 1,024-sample hardware buffer.
- **Latency & Overflow Protection:** The host queue enforces a bounded maximum size ($48,000 \times 2 \times 2\text{ bytes}$) to discard stale frames and maintain low audio latency across long execution sessions.

---

## 5. Input & Peripheral Subsystem Specification

### 5.1 USB Host Subsystem (Pico-PIO-USB + Adafruit TinyUSB)

- **Controller Implementation:** Pico-PIO-USB running on **PIO 1** (leaving PIO 0 dedicated to `libdvi`), bit-banging USB 1.1 Full Speed (12 Mbps) on GPIO 28 (`D+`) and GPIO 29 (`D-`).
- **Software Stack:** Adafruit TinyUSB Host library configured for up to 2 chained external USB hubs (`CFG_TUH_HUB = 2`) and 8 concurrent HID interfaces (`CFG_TUH_HID = 8`).
- **Protocol:** Enforces USB HID Boot Protocol (`HID_PROTOCOL_BOOT`) at device attachment.

#### Hardware Limitation: Direct Hot-Plugging & Errata E9
On the Waveshare RP2350-PiZero, the USB Host port's 5V VBUS rail is hardwired directly to the system 5V input without a GPIO-controlled power MOSFET switch:
- When a device is unplugged directly from the board, the data lines remain pulled high into the J/FS (Full-Speed idle) state.
- Because RP2350 Rev 3 silicon disables the Pico-PIO-USB Errata E9 pull-down workaround, PIO-USB cannot detect a Single-Ended Zero (SE0) disconnect event.
- It continues polling the idle bus and floods ~180 identical phantom HID reports per second.
- **Operational Requirement:** Keyboards and gamepads plugged directly into the board must be attached **before power-on**.
- **Hub Exception:** When connected through an external powered USB hub, hot-plugging is fully supported because the external hub chip manages its own downstream port power and reports standard port-status changes.
- **Chained Hub Race Condition:** Certain complex USB-C hubs with two cascaded hub ICs trigger a known TinyUSB race condition during enumeration; simple 4-port hubs operate reliably.

### 5.2 USB Keyboard & Matrix Translation

The Tandy Color Computer 2 keyboard is a passive $8 \times 8$ matrix scanned via PIA0 Port A and Port B. Keycaps on a modern USB keyboard do not match the physical layout of a CoCo keyboard. The CoCo Zero implements full **keycap-to-chord translation**:

```
USB Key Event (HID Usage + Shift State)
                |
                v
       kt_translate() Look-up
                |
                v
  CoCo DSCAN Keycode + Forced SHIFT (KEEP / ON / OFF)
                |
                v
  Injected into coco_machine Virtual Key Matrix
```

- **Symbol Chord Mapping:** Typing symbols like `!`, `"`, `#`, `$`, `%`, `&`, `'`, `(`, `)`, `*`, `+`, `<`, `=`, `>`, `?` automatically generates the exact CoCo chord (e.g. `Shift + 2` generates `"`, `Shift + :` generates `*`).
- **Special Key Equivalents:**
  - `Enter` $\rightarrow$ CoCo `ENTER`
  - `Backspace` / `Delete` $\rightarrow$ CoCo `LEFT` (erases character)
  - `Esc` $\rightarrow$ CoCo `BREAK`
  - `Home` $\rightarrow$ CoCo `CLEAR`
  - `Caps Lock` $\rightarrow$ CoCo Case Toggle (`Shift + 0` in Color BASIC)
  - *NitrOS-9 / OS-9 Case Toggle:* OS-9's console driver begins in uppercase and toggles case via `CTRL-0` (where `CTRL` is CoCo `CLEAR`). On USB keyboards, press **Home + 0**. (Note: Caps Lock generates Color BASIC's `Shift + 0` toggle, which OS-9 interprets as a literal digit `'0'`).
  - Numeric Keypad: Fully mapped to numbers and math operators.
  - Arrow Keys: Directly mapped to CoCo `UP`, `DOWN`, `LEFT`, `RIGHT`.
- **Keyboard Auto-Repeat:** Hardware-assisted auto-repeat (`key_repeat = on`, default 500 ms delay, 10 repeats/sec clamped to max 12/sec for BASIC input queue stability).
- **Serial CDC Keyboard (`serial_keyboard = on`):** Characters received over USB-CDC are translated and queued directly into the virtual keyboard matrix. *Note: Color BASIC input routines require a brief settling delay between Enter and the next character; scripts should prefix lines with a leading space.*

### 5.3 Host Emulator Dual-Tier Input Subsystem (`frontend.rs` & `keyboard.rs`)

Because the host emulator executes the unaltered bare-metal RP2350 firmware, input must satisfy two distinct consumer layers: (1) guest 6809 Color BASIC running inside XRoar, which polls the virtual hardware keyboard matrix, and (2) the firmware's on-screen display (OSD) overlay menus, which listen for USB HID reports from TinyUSB host callbacks. The emulator implements a **dual-tier input architecture**:

```
Host SDL2 Window / Non-blocking Terminal Stdin
                     |
         +-----------+-----------+
         |                       |
[Normal Typing Mode]    [Function Keys & Menu Active]
         |                       |
         v                       v
Tier 1: Matrix Engine   Tier 2: TinyUSB HID Callback
  * SRAM 0x2000B304       * Report at 0x2001C18C
  * kt_chord() Table      * Trampoline at 0x2007FFE0
  * Virtual Hold (4f*T)   * HID Callback 0x1001F460
  * Virtual Gap (2f*T)    * Register Save / Restore
  * Boot Gated (630M cyc) * Direct Menu Navigation
         |                       |
         v                       v
Guest 6809 Color BASIC   Firmware OSD Overlays (F1-F12)
```

#### Tier 1: Direct Keyboard Matrix Injection (`0x2000_B304`)
- **Matrix Interface Specification:** Injects active-low column/row bitmasks into `g_kb_col_row_mask` (8 columns × 7 rows) at SRAM address `0x2000_B304`, directly driving the MC6821 PIA0 keyboard scanning interface.
- **Chording Translation Contract (`kt_chord`):** Maps ASCII character sequences to native CoCo `dscan` matrix intersections, synthesizing automatic Shift chords for standard punctuation and symbols (`!`, `"`, `#`, `$`, `%`, `&`, `'`, `(`, `)`, `*`, `+`, `<`, `=`, `>`, `?`, `[`, `]`, `\`, `_`).
- **Critical Timing Decision — Debounce Invariance:** Color BASIC's keyboard polling loop executes software debouncing that discards single-frame key pulses. Keystrokes are held asserted for 4 virtual frames (~66.7 ms at 60 Hz) and separated by a 2 virtual frame release gap (~33.3 ms). To guarantee reliable key latching under all emulation speeds, both durations are dynamically scaled by `--turbo` ($4 \times T$ and $2 \times T$), keeping virtual machine debouncing strictly invariant.
- **Boot Readiness Tradeoff:** During the initial ~2.5 seconds of system boot, Color BASIC wipes and resets zero-page memory and input buffers. Keystroke dequeuing is gated until simulated cycle count reaches 630,000,000 cycles, preventing early keystrokes from being destroyed.
- **Input Stream Multiplexing:** Accepts input concurrently from both the graphical SDL2 window (`TextInput` with `KeyDown` fallback) and a non-blocking background host terminal `stdin` thread.

#### Tier 2: TinyUSB HID Callback Injection (`inject_hid_keycode`)
The firmware's on-screen display (OSD) overlay system does not poll the keyboard matrix; it listens strictly for USB HID keyboard reports dispatched from TinyUSB host callbacks.
- **Interface Contract:** Synthesizes standard 8-byte USB HID keyboard reports (`[modifier, reserved, keycode, ...]`) in dedicated upper scratch SRAM (`TRAMP_ADDR - 0x10 = 0x2007_FFD0`) and invokes the firmware's TinyUSB HID report callback at `0x1001_F460` (or dynamically discovered address) directly within Core 0.
- **State Preservation Contract:** HID report delivery executes atomically with full register state preservation (`R0–R14`, `xPSR`, `MSP`), ensuring zero disruption to running 6809 emulation or multi-core CPU state.
- **Dynamic Routing Decision & State Isolation:**
  - The emulator actively monitors the firmware's menu activation flag (`MENU_ACTIVE_ADDR = 0x2000_C8D0`).
  - *Normal Mode (`MENU_ACTIVE_ADDR == 0`):* Alphanumeric and symbol keys route exclusively to Tier 1 (Matrix Engine) to drive Color BASIC.
  - *Menu Active Mode (`MENU_ACTIVE_ADDR != 0`):* Host navigation keys (`Up`, `Down`, `PageUp`, `PageDown`, `Left`, `Right`, `Return`, `Escape`, `Tab`) and drive keys (`0`–`3`) are intercepted and routed to Tier 2 (HID Callback). This prevents menu navigation keystrokes from leaking into the Color BASIC input queue and guarantees immediate menu responsiveness.

#### Function Keys & Cross-Platform Shortcuts
To accommodate macOS and compact laptop keyboards where physical Function keys are captured by the operating system for brightness or volume:

| Target Function | Standard Key | macOS / PC Shortcut | HID Code | Target Subsystem |
|---|---|---|---|---|
| **Disks Menu** | **F12** | **Cmd+D** / **Ctrl+D** | `0x45` | OSD Floppy Disk Mount (`< DISKS >`) |
| **Programs Menu** | **F9** | **Cmd+P** / **Ctrl+P** | `0x42` | OSD Binary Launcher (`< PROGRAMS >`) |
| **Cartridges Menu** | **F10** | **Cmd+C** / **Ctrl+C** | `0x43` | OSD Cartridge Selector (`< CARTRIDGES >`) |
| **Files Menu** | **F11** | *N/A* | `0x44` | OSD Text Editor (`< FILES >`) |
| **Info / Telemetry** | **F1** | **Cmd+I** / **Ctrl+I** | `0x3A` | OSD System Diagnostics (`< INFO >`) |
| **Artifact Toggle** | **F8** | **Cmd+A** / **Ctrl+A** | `0x41` | PMODE 4 NTSC Color Cycle |
| **Edit Game Config** | **TAB** | *N/A* | `0x2B` | Open Text Editor on highlighted item's `.TXT` |

#### Active Overlay Navigation Routing
When an OSD menu overlay is open (`MENU_ACTIVE_ADDR = 0x2000_C8D0 != 0`):
- Navigation keys are automatically intercepted and routed to Tier 2 HID reports:
  - `Up` (`0x52`), `Down` (`0x51`), `PageUp` (`0x4B`), `PageDown` (`0x4E`)
  - `Left` (`0x50`), `Right` (`0x4F`)
  - `Return` / `KpEnter` (`0x28`), `Escape` (`0x29`), `Tab` (`0x2B`)
- Drive selection keys: `0`, `1`, `2`, `3` on main keyboard or keypad are routed to HID keycodes `0x27`, `0x1E`, `0x1F`, `0x20` to mount or unmount floppy drives live.
- Drive lock toggle: `W` (`0x1A`) toggles write-protection lock on the selected drive.
- Disk management: `N` (`0x11`) prompts to format a new blank disk; `R` (`0x15`) renames the selected disk image.

### 5.4 USB Gamepad Subsystem

The firmware auto-detects and decodes multiple standard gamepad protocols:
1. **Sony DualShock 4** (VID `0x054C`, PID `0x09CC`)
2. **GameSir XInput / Auto Mode** (VID `0x3537`, PID `0x1093`)
3. **GameSir Android HID Mode** (VID `0x3537`, PID `0x1094`)

#### Gamepad Joystick & Button Mapping

- **Analog Sticks:**
  - Left Stick (and D-pad): Mapped to **Right Joystick** (`JOYSTK(0)` and `JOYSTK(1)` on PIA0 Port A Bit 0), which is the primary joystick read by almost all CoCo games.
  - Right Stick: Mapped to **Left Joystick** (`JOYSTK(2)` and `JOYSTK(3)` on PIA0 Port A Bit 1).
  - Swapping: Can be inverted via `joystick_swap = on`.
- **D-Pad Modes:** Configurable via `dpad = joystick` (default) or `dpad = arrows` (presses CoCo arrow keys).
- **Default Button Map:**
  - Bottom Face (A / Cross): `fire` (Right stick fire)
  - Right Face (B / Circle): `fire`
  - Left Face (X / Square): `fire_right` (Left stick fire)
  - Top Face (Y / Triangle): `space` (CoCo Spacebar)
  - Shoulder R1 / L1: `fire` / `fire_right`
  - Start: `enter` (CoCo Enter key)
  - Home: Opens and closes the OSD Overlay menus.
  - M Button (GameSir): Captures screenshot to SD card.
- **Overlay Menu Controls on Gamepad:** When an OSD menu is open, the D-pad navigates (left/right switches menu categories), **A** selects/starts the highlighted game, **B** closes the menu, **L1/R1** page up/down, and **X** opens the built-in text editor on the highlighted game's `.TXT` configuration file.

---

## 6. Storage & Filesystem Subsystem Specification

### 6.1 MicroSD Interface & FAT32 Hierarchy

The on-board microSD slot connects to the RP2350's hardware SPI1 controller at a 12.5 MHz clock rate. The storage stack uses FatFs v3.x (`carlk3/no-OS-FatFS-SD-SDIO-SPI-RPi-Pico`). The card must be formatted as FAT32.

```
/coco/
  |-- roms/
  |     |-- bas12.rom       (Required for CoCo 2: 8192 bytes, Color BASIC 1.2)
  |     |-- extbas11.rom    (Optional: 8192 bytes, Extended Color BASIC 1.1)
  |     |-- disk11.rom      (Optional: 8192 bytes, Disk BASIC 1.1 Cartridge)
  |     +-- coco3.rom       (Required for CoCo 3 build: 32768 bytes, Super Extended BASIC)
  |-- dsk/                  (Floppy disk images: .dsk files)
  |-- bin/                  (Executable binaries: .bin files)
  |-- cart/                 (ROM Cartridges: .ccc files)
  |-- shots/                (Screenshots: SCR0001.PNG - SCR9999.PNG)
  |-- log/                  (Soak telemetry logs: soak-001.txt - soak-NNN.txt)
  |-- settings.txt          (System-wide settings file)
  |-- autorun.txt           (Boot script and auto-execution commands)
  +-- drives.txt            (Persisted floppy drive mount states)
```

#### ROM Resolution Logic & Boot Failure Splashes
The loader searches for ROMs first in `/coco/roms/<name>` and falls back to `/coco/<name>`:
- **`bas12.rom` Missing (CoCo 2):** System halts and displays `MSG_NOROM` splash screen with instructions.
- **`bas12.rom` Size != 8192 Bytes:** System halts and displays `MSG_BADROM` splash indicating exact found file size.
- **`extbas11.rom` Missing:** System displays `MSG_CBONLY` notice ("COLOR BASIC ONLY") for 4.0 seconds, then boots into Color BASIC 1.2 with 16 KB RAM ($A000–$BFFF and $E000–$FFFF). Disk commands will not be available.
- **`coco3.rom` Missing (CoCo 3 `pizero_coco3` build):** System halts and displays instructions requiring the 32 KB CoCo 3 ROM image.
- **MicroSD Card Unreadable:** System displays `MSG_NOSD` splash screen.

### 6.2 MicroSD SPI Controller & Virtual FAT32 Storage Engine (`spi_sd.rs` & `fat32.rs`)

The host emulator provides two interchangeable storage backends:
1. **Dynamic Virtual FAT32 Builder (`src/fat32.rs`):** When launched pointing to a directory (`--sd ./coco`), the emulator dynamically synthesizes a complete FAT32 filesystem in memory (`MemoryStorage`). It generates the Master Boot Record (MBR), FAT32 Volume Boot Record (VBR / BPB), FSInfo sector, two 32-bit FAT tables, the root directory cluster, subdirectories (`roms/`, `dsk/`, `cart/`, `bin/`, `shots/`, `log/`), contiguous cluster chains, and authentic 8.3 directory entries on the fly.
2. **File-Backed Storage (`FileStorage`):** When launched pointing to a raw disk image file (`--sd sdcard.img`), sectors are read and written directly to the host image file.

#### SPI1 SD Card Controller Emulation
The firmware communicates with the SD card via SPI1 using software Chip Select on GPIO 43. The emulator implements a complete SD Card SPI-mode protocol state machine (`SpiSdCard`):

| SPI Command | Name | Arguments | Card Response | Description |
|---|---|---|---|---|
| **CMD0** | `GO_IDLE_STATE` | `0x00000000` | R1 (`0x01`) | Puts card into SPI idle state |
| **CMD8** | `SEND_IF_COND` | `0x000001AA` | R7 (`0x01`, `0xAA`) | Checks voltage range & pattern echo |
| **CMD9** | `SEND_CSD` | `0x00000000` | R1 (`0x00`) + Token `0xFE` + 16B | CSD v2.0 (SDHC capacity calculated from sectors) |
| **CMD10** | `SEND_CID` | `0x00000000` | R1 (`0x00`) + Token `0xFE` + 16B | Card identification ("COCO0", serial, CRC) |
| **CMD12** | `STOP_TRANSMISSION` | `0x00000000` | Stuff `0xFF`, R1 (`0x00`) | Terminates streaming multi-block read |
| **CMD13** | `SEND_STATUS` | `0x00000000` | R2 (`[0x00, 0x00]`) | Returns 2-byte card operational status |
| **CMD16** | `SET_BLOCKLEN` | `512` | R1 (`0x00`) | Sets block length (512 bytes) |
| **CMD17** | `READ_SINGLE_BLOCK` | Sector index | R1 (`0x00`) + `0xFE` + 512B + CRC | Single sector payload streamed from backing store |
| **CMD18** | `READ_MULTIPLE_BLOCK`| Sector index | R1 (`0x00`) + streaming blocks | Continuous multi-block sector stream |
| **CMD24** | `WRITE_SINGLE_BLOCK`| Sector index | R1 (`0x00`), `0x05`, `0xFF` | Single sector write with data token & busy release |
| **CMD25** | `WRITE_MULTIPLE_BLOCK`| Sector index | R1 (`0x00`), `0x05`, `0xFF` | Streaming multi-sector write terminated by `0xFD` |
| **CMD55 + ACMD41**| `SD_SEND_OP_COND` | `0x40000000` | R1 (`0x00`) | Activates card initialization, transitions to Ready |
| **CMD58** | `READ_OCR` | `0x00000000` | R1 (`0x00`) + OCR 4B | OCR register (CCS bit = 1, SDHC/SDXC supported) |
| **CMD59** | `CRC_ON_OFF` | `0x00000000` | R1 (`0x00`) | Enables or disables CRC checking |

#### Sector Write Contract & Critical Protocol Tradeoff
- **Single-Block Write Contract (`CMD24`):** Following CMD24, the card expects Start Block Token `0xFE`, captures 512 sector payload bytes and 2 CRC bytes, commits the sector to backing storage, and returns Data Accepted Token `0x05` followed by Ready `0xFF`.
- **Multi-Block Write Contract (`CMD25`):** Streams consecutive sectors delimited by Start Block Token `0xFC`, commits each sector, auto-increments the sector address, and cleanly terminates upon receiving Stop Tran Token `0xFD`.
- **Card Status Query Contract (`CMD13`):** Returns standard 2-byte R2 status indicating operational readiness (`0x0000`).
- **Critical Protocol Decision — SPI Stream Disambiguation:**
  - *Context:* In SD SPI mode, standard command framing looks for bytes matching `01xxxxxx` (`(mosi & 0xC0) == 0x40`). However, sector write payloads (especially directory sectors containing uppercase ASCII filenames) frequently contain bytes that match this mask.
  - *Contract:* The SD controller state machine strictly inhibits command framing whenever the card is in a data-receiving state (`WritingSingleBlock` or `WritingMultiBlock`). Command parsing is only re-enabled after block commit and busy-signaling completion.
  - *Tradeoff Impact:* Without this disambiguation, FatFs directory rescans during OSD disk mounting mistake directory sector bytes for new commands, desynchronizing the SPI bus and causing disk cataloging to abort with false "NO DISK IMAGES FOUND" errors.

### 6.3 Floppy Disk Controller (FDC) Emulation & Writable Disk Images

The system emulates a Western Digital **WD2797 / WD1793** Floppy Disk Controller mapped at `$FF48–$FF4B`:
- **Virtual Drives:** 4 independent floppy disk drives (Drive 0, 1, 2, 3).
- **Disk Image Format:** Standard unadorned single-sided `.dsk` files (typically 35 tracks, 18 sectors/track, 256 bytes/sector = 161,280 bytes).
- **Drive Select Register ($FF40):** Monitored to track guest drive selection and motor control.
- **Synchronous Sector Write Engine (PIZERO-66, PIZERO-194):** Guest disk writes (`SAVE`, `SAVEM`, `KILL`, and OS-9 filesystem operations) write directly through to the backing `.dsk` file on the microSD card. Writes occur synchronously: individual 256-byte sector updates complete in $\le 2.1\text{ ms}$, and FAT flush/sync operations complete in $\le 6.0\text{ ms}$, executing seamlessly within the Core 0 frame pacing budget without requiring asynchronous deferred queues.
- **Interactive Drive Write-Locking (PIZERO-193):** Pressing **W** on any highlighted drive in the `< DISKS >` OSD menu toggles read-only write protection on that virtual drive. Any `.dsk` file with a read-only filesystem flag on the microSD card is automatically locked read-only.
- **Blank Formatted Disk Creation (PIZERO-196):** Pressing **N** in the `< DISKS >` OSD menu prompts the user to enter a disk name and immediately creates an authentic 35-track single-sided blank formatted `.dsk` image with initialized RS-DOS Track 17 directory and FAT structures.
- **Disk Image Renaming (PIZERO-196):** Pressing **R** in the `< DISKS >` OSD menu allows renaming the highlighted `.dsk` file directly on the microSD card.
- **NitrOS-9 Level 1 OS-9 Boot Support (PIZERO-190, 191, 194, 199):** The floppy subsystem supports booting NitrOS-9 Level 1 operating system disks using Disk BASIC's `DOS` command. Full filesystem write operations and command execution persist reliably across power cycles.
- **Drive Persistence:** Drive assignments are automatically preserved across power cycles in `/coco/drives.txt`.

#### RS-DOS Directory Scanning (`rsdos_dir.h`)
When a disk is selected for launch in Drive 0:
- The firmware scans Track 17, Sectors 3 through 11 (8 directory entries of 32 bytes per sector).
- Evaluates file types: Type 0 = BASIC, Type 2 = Machine Language.
- Automatically generates the launch command (`RUN"NAME"\r` or `LOADM"NAME":EXEC\r`).
- Queues command with a 180-frame (~3.0 s) warmup delay to allow Disk BASIC to initialize before typing.

### 6.4 Cartridge & Binary Loading Engine

- **Direct Binary Loading (`.bin`):** Color BASIC `LOADM` binary parsing (`coco_boot_parse_loadm`). Loads multi-segment headers directly into RAM and executes via direct Program Counter (PC) jump without requiring Disk BASIC.
- **ROM Cartridges (`.ccc`):**
  - Standard Cartridges: 2 KB, 4 KB, 8 KB, and 16 KB ROMs mapped into `$C000–$DFFF` (mirrored through `$E000–$FEFF`). Small cartridges repeat through the address window.
  - Bank-Switched GMC Cartridges: 32 KB to 256 KB banked ROMs (16 KB per bank, up to 16 banks). Bank switching triggered by guest writes to even addresses in `$FF40–$FF5F`:
    $$\text{bank\_offset} = (D \ll 14) \ \& \ ((\text{len} - 1) \ \& \ \text{0x3C000})$$
    Odd addresses in `$FF40–$FF5F` route to the SN76489 sound chip.
- **Screenshot Writer (`png_write.h`):** Saves $320 \times 240$ 8-bit indexed PNG images using uncompressed (STORED) deflate blocks directly to `/coco/shots/SCR0001.PNG` through `SCR9999.PNG`. Consumes ~77 KB per image without requiring heap-heavy zlib compression buffers.
- **Autorun Script Delay Tokens (`~`):** In `autorun.txt`, the `~` character introduces a 1.0-second delay per token, allowing operating systems like NitrOS-9 time to complete device driver initialization before typing subsequent commands.

---

## 7. CoCo Machine Emulation Core

### 7.1 Four-Tier Architecture & System Memory Map

The CoCo Zero emulation core is organized according to the Four-Tier Architectural Model defined in Section 1.1. The guest 8-bit computer system (Tier 3) runs entirely within the Cortex-M33 Core 0 execution loop of the bare-metal firmware (Tier 2), which is hosted on either the physical RP2350B microcontroller (Tier 1) or inside the Rust hardware emulator (Tier 4).

The guest emulation core is adapted from [XRoar](https://www.6809.org.uk/xroar/) (developed by Ciaran Anscomb) and models the hardware architecture of the Tandy Color Computer 2 (CoCo 2) and the Tandy Color Computer 3 (CoCo 3):

```
+-------------------------------------------------------------------------+
|                  CoCo 2 System Memory Architecture                      |
|                                                                         |
|  Address Range    Device / Mapping Mode        Normal Map   All-RAM (TY) |
|  $0000 - $7FFF    32 KB Lower Dynamic RAM      RAM          RAM         |
|  $8000 - $9FFF    Extended Color BASIC ROM     ROM (8 KB)   RAM         |
|  $A000 - $BFFF    Standard Color BASIC ROM     ROM (8 KB)   RAM         |
|  $C000 - $DFFF    Cartridge ROM / Disk BASIC   ROM (8 KB)   RAM         |
|  $E000 - $FEFF    ROM Mirror / Cart Mirror     ROM (8 KB)   RAM         |
|  $FF00 - $FF1F    MC6821 PIA0 (Kbd / Joy / HS) I/O Ports    I/O Ports   |
|  $FF20 - $FF3F    MC6821 PIA1 (DAC / VDG / FS) I/O Ports    I/O Ports   |
|  $FF40 - $FF5F    WD2797 FDC / SN76489 CSG     I/O Ports    I/O Ports   |
|  $FF90 - $FF95    CoCo 3 GIME Timers & IRQs    Optional     Optional    |
|  $FFB0 - $FFBF    CoCo 3 GIME Palette (16 RGB) Optional     Optional    |
|  $FFC0 - $FFDF    MC6883 SAM Control Registers SAM Regs     SAM Regs    |
|  $FFE0 - $FFFF    6809 Interrupt/Reset Vectors Vectors      Vectors     |
+-------------------------------------------------------------------------+
```

#### Memory Mapping Operating Modes
1. **Standard CoCo 2 Operating Map ($TY=0$):**
   - The lower 32 KB ($0000–$7FFF) is continuously mapped to dynamic RAM.
   - Addresses $8000–$BFFF map the 16 KB Color BASIC system ROM (Extended BASIC 1.1 at $8000–$9FFF; Color BASIC 1.2 at $A000–$BFFF).
   - Addresses $C000–$DFFF map external cartridge ROM (such as Disk BASIC 1.1 or games).
   - Addresses $E000–$FEFF mirror the cartridge/BASIC ROM window.
   - Hardware I/O and control registers occupy the top 256 bytes ($FF00–$FFFF).
2. **All-RAM Operating Mode ($TY=1$):**
   - When the SAM $TY$ register is set (`POKE 65499,0` or write to `$FFDF`), the entire 64 KB address space from `$0000` through `$FEFF` is mapped to contiguous dynamic RAM.
   - Only the upper 256 bytes (`$FF00–$FFFF`) remain reserved for I/O registers and hardware interrupt vectors.
   - This mode is utilized by advanced operating systems like NitrOS-9 Level 1 and OS-9 to provide an unfragmented 64 KB user address space.
3. **CoCo 3 GIME Paged MMU Map (`pizero_coco3`):**
   - The CoCo 3 firmware models 128 KB of physical dynamic RAM partitioned into sixteen 8 KB blocks (`$00` through `$0F`).
   - The 64 KB logical MPU address space is divided into eight 8 KB windows (`$0000–$1FFF`, `$2000–$3FFF`, ..., `$E000–$FFFF`), mapped dynamically through Task 0 (`$FFA0–$FFA7`) and Task 1 (`$FFA8–$FFAF`) MMU registers.
4. **SRAM Backing Contract:**
   - 64 KB of virtual CoCo 2 RAM (or 128 KB for CoCo 3) is allocated as a static contiguous buffer in the RP2350's internal 520 KB SRAM via `psram_stub.c` (`rp_mem_malloc`).
   - The buffer is zero-initialized at boot, mirroring the authentic cold-boot state of physical dynamic RAM.
5. **Master Clock & Frame Cadence:**
   - Physical Color Computer hardware derives all timings from a **14.3181818 MHz** master color burst crystal.
   - The MC6883 SAM divides this master frequency by 16 to synthesize the standard MPU clock:
     $$f_{MPU} = \frac{14.3181818\text{ MHz}}{16} \approx \mathbf{0.894886\text{ MHz}} \quad (1117.5\text{ ns per cycle})$$
   - At a 60.0 Hz video display refresh rate, the frame period is exactly $16,666.7\ \mu\text{s}$. The emulated CPU executes:
     $$\text{CYCLES\_PER\_FRAME} = \frac{894,886\text{ Hz}}{60.0\text{ Hz}} \approx \mathbf{14,915\text{ cycles}}$$
   - **Fixed-Time Accounting:** The virtual machine enforces a fixed emulated time budget per frame:
     $$\text{BUDGET} = 14,915 \times 16\text{ ticks/cycle} = \mathbf{238,640\text{ event ticks/frame}}$$

---

### 7.2 Motorola MC6809E Microprocessor Emulation Model

#### Silicon Architecture: MC6809 vs MC6809E
A widespread misconception is that the Tandy Color Computer uses the standard Motorola MC6809 microprocessor. In reality, the CoCo 1, CoCo 2, and CoCo 3 exclusively employ the **Motorola MC6809E** ("External Clock") variant. The MC6809E differs fundamentally from the standalone MC6809:

```
          Motorola MC6809                      Motorola MC6809E
       +--------------------+               +--------------------+
       |                    |               |                    |
   XTAL| 38               34|E             E| 34                5|LIC
  EXTAL| 39               33|Q             Q| 35                6|AVMA
       |                    |               |                  33|BUSY
  RESET| 37                2|NMI       RESET| 37               39|TSC
       |   Internal Clock   |               |   External Clock   |
       |     Oscillator     |               |  Quadrature Inputs |
       |                    |               |                    |
       +--------------------+               +--------------------+
```

1. **Quadrature External Clocks ($E$ and $Q$):**
   - The MC6809 contains an internal clock oscillator driven by an external crystal across pins `XTAL` and `EXTAL`, outputting clock phases $E$ and $Q$.
   - The MC6809E **lacks an internal oscillator**. Pins 34 and 35 are high-impedance inputs driven by external quadrature clock signals $E$ (Enable) and $Q$ (Quadrature) synthesized by the MC6883 SAM.
   - **Phase Relationship:** Clock $Q$ leads clock $E$ by 90° (one quarter cycle). The MPU places valid addresses on the bus during the rising edge of $Q$; data is sampled and latched on the falling edge of $E$.
   - **Interrupt Sampling:** Interrupt inputs (`NMI`, `FIRQ`, `IRQ`) and `HALT` are sampled on the **falling edge of $Q$** during every bus cycle (including internal dummy cycles).
2. **Dedicated Bus Control Signals:**
   - **`LIC` (Last Instruction Cycle, Pin 5):** Driven high by the MPU during the final execution cycle of every instruction, providing lookahead for DMA arbiters.
   - **`AVMA` (Advanced Valid Memory Access, Pin 6):** Asserts high one bus cycle in advance of a valid memory access, allowing the SAM to prepare DRAM bank multiplexing.
   - **`BUSY` (Pin 33):** Asserts high during indivisible read-modify-write and 16-bit operations, signaling that external bus masters must not intervene.
   - **`TSC` (Three-State Control, Pin 39):** When pulled high by the SAM, forces address lines and the $R/\overline{W}$ line into a high-impedance state, allowing interleaved VDG video RAM accesses.
   - **`BA` (Bus Available, Pin 37) & `BS` (Bus Status, Pin 36):** Encodes the MPU bus state:
     - `BA=0, BS=0`: Normal instruction execution.
     - `BA=0, BS=1`: Interrupt or Reset vector fetch acknowledge.
     - `BA=1, BS=0`: `SYNC` instruction acknowledge (waiting for interrupt).
     - `BA=1, BS=1`: `HALT` or `CWAI` acknowledge (bus granted to external master).

#### Authoritative Literature & Primary Citations
The 6809 core implementation in `xroar-waveshare-rp2350-pizero` (`mc6809.c`, `mc6809_common.c`) is derived from Ciaran Anscomb's XRoar and incorporates empirical research and citations from authoritative primary sources:
1. **Motorola Semiconductor:** *MC6809E 8-Bit Microprocessor Data Sheet* (1983). Provides the foundational state transition model (Figure 14, "MPU Flow Chart") and pin definitions.
2. **Atsushi [atjs]:** *MC6809 Cycle-By-Cycle Performance* (`6809cyc.txt`). Authoritative cycle breakdown for every instruction, documenting intermediate dummy read addresses and stack sequencing.
3. **Ciaran Anscomb:** *Dragon Update: Illegal Op-codes* (February 1994). Primary documentation of undefined instruction behaviors on Dragon and CoCo hardware.
4. **Darren Atkinson [darrena]:** *Motorola 6809 & Hitachi 6309 Programmer's Reference* (2009). Exhaustive register semantics, flag interactions, and cycle timings.
5. **David Banks [hoglet67]:** *Undocumented 6809 Behaviours* (BBC Micro 6809 Second Processor project). Empirical silicon verification of undocumented instructions, including carry flag behavior on illegal `DEC` (`op_xdec`) and `CLR` (`op_xclr`).
6. **David Flamand [dfffffff]:** *The Comprehensive Document of 6809 Undocumented and Undefined Behavior* (GitLab `6809_undoc`). Exhaustive analysis of invalid indexed postbytes, page prefix fallthrough, and bus artifacts.
7. **Tim Lindner [tlindner]:** *6809 Exchange and Transfer Opcodes* (MacMESS / MAME CoCo driver). Dissects 8-bit to 16-bit register pairings during `EXG` and `TFR` instructions.
8. **MAME Project (Aaron Giles et al.):** Reference implementation of condition code $V$ (two's-complement overflow) calculation logic across 16-bit subtractions and additions.
9. **Stewart Orchard:** *Research into SAM VDG Mode Transitions*. Documents physical SAM silicon glitches and transitional phase shifts during video mode reprogramming.

#### Cycle-By-Cycle Execution State Machine
The core executes as a discrete state machine directly modeling Figure 14 of the Motorola MC6809E datasheet via `enum mc6809_state`:
- `mc6809_state_reset` $\rightarrow$ Clears DP to `$00`, sets interrupt mask bits `CC_I` and `CC_F`, disarms NMI, and transitions to `reset_check_halt`.
- `mc6809_state_reset_check_halt` $\rightarrow$ Checks `HALT`; if asserted, executes NVMA dummy cycles; if deasserted, executes `take_interrupt(cpu, 0, MC6809_INT_VEC_RESET)`.
- `mc6809_state_label_a` $\rightarrow$ Checks `HALT` at instruction completion.
- `mc6809_state_label_b` $\rightarrow$ Priority interrupt dispatch: evaluates `nmi_active`, then `firq_active` (if unmasked by `CC_F`), then `irq_active` (if unmasked by `CC_I`).
- `mc6809_state_next_instruction` $\rightarrow$ Fetches opcode byte from `REG_PC++`, resolves instruction pages (`$10` and `$11` page prefixes), and branches to execution handlers.
- `mc6809_state_dispatch_irq` $\rightarrow$ Acknowledges active interrupt, sets condition code masks, and branches to vector handler.
- `mc6809_state_sync` $\rightarrow$ Implements `SYNC` instruction idle state until an interrupt line transitions active.
- `mc6809_state_cwai_check_halt` $\rightarrow$ Implements `CWAI` waiting state with stacked registers.
- `mc6809_state_hcf` $\rightarrow$ Halt and Catch Fire state triggered by illegal opcodes, generating continuous dummy bus cycles.

#### Non-Valid Memory Access (NVMA) Cycle Mechanics
During internal execution operations (indexed addressing calculations, 16-bit register arithmetic, stack pointer adjustments, and relative branch evaluation), the MC6809E performs internal work without reading or writing data.
- **Physical Hardware Behavior:** On real CoCo hardware, the MPU deasserts $AVMA$. The address bus floats or defaults to `$FFFF`, and the CPU performs a dummy read cycle. Because `$FFFF` falls into the reset vector window of Color BASIC ROM, this dummy cycle always reads the high byte of the reset vector (`rom[0x3FFF]`).
- **Firmware Fast-Path Optimization (`PIZERO-131`):**
  - Pure software emulators invoke full memory bus decoding on every dummy cycle, which constitutes ~35% of all executed cycles.
  - In `xroar-waveshare-rp2350-pizero`, `coco_mem_nvma()` short-circuits this path entirely: it sets `cpu->D = coco_fastbus.rom[0x3FFF]`, charges `coco_fastbus.rom_ticks` (8 or 16 event ticks), and calls `mem_cycle_tail()` only when an event deadline has arrived.
  - When `coco_fastbus_advance()` succeeds, the dummy cycle advances `event_current_tick += ticks` in ~3 Cortex-M33 instructions without function calls or bus lookups.

#### Fastbus Inlined Memory Architecture (`coco_fastbus.h`)
To achieve locked 60 fps at 1.79 MHz on an embedded ARM microcontroller, memory accesses are divided into fast-path direct memory operations and slow-path peripheral operations:
1. **Flat Window Fast-Path (CoCo 2):**
   - Memory accesses below `ram_top` ($0000–$7FFF, or $0000–$FEFF in All-RAM mode) read and write directly to the SRAM RAM pointer (`coco_fastbus.ram[a]`).
   - Memory accesses between `ram_top` and `rom_top` ($8000–$BFFF) read directly from ROM (`coco_fastbus.rom[a & 0x3FFF]`).
   - Memory accesses between `rom_top` and `cart_top` ($C000–$DFFF) read directly from cartridge ROM (`coco_fastbus.cart[a & cart_mask]`).
   - Only accesses above `cart_top` ($FF00–$FFFF: PIAs, SAM, FDC, GIME) invoke the full out-of-line `coco_mem_cycle()` dispatcher.
2. **Paged Fast-Path (CoCo 3, `COCO_FASTBUS_PAGES`, PIZERO-213):**
   - Implements an 8-entry logical-to-physical pointer table (`rd[8]` and `wr[8]`).
   - Memory accesses evaluate `a < page_top && (p = rd[a >> 13])`, reading `p[a & 0x1FFF]` directly in ~4 ARM cycles.
3. **Deadline Time-Advancement (`coco_fastbus_advance`):**
   - Core 0 computes a `deadline` representing the earliest event tick where an external action is required (VDG raster line event, audio sample boundary, or frame budget expiration).
   - If `event_current_tick + ticks < deadline`, the fastbus helper simply adds `ticks` to `event_current_tick` and returns immediately.
   - Only when virtual time reaches `deadline` does the engine invoke `coco_mem_tick()`, running the event queue and checking interrupt latches.
4. **Throughput Impact:** These fastbus optimizations reduced host execution overhead from ~180 Cortex-M33 clocks per emulated 6809 cycle to **~101 clocks per cycle** (>43% speedup).

#### Addressing Modes & Undocumented Silicon Quirks
- **Direct Page Addressing:** Formed by `(REG_DP << 8) | offset`, followed by 1 NVMA cycle.
- **Extended Addressing:** Formed by 16-bit address operand, followed by 1 NVMA cycle.
- **Indexed Addressing Modes:** Evaluated via postbyte:
  - 5-bit signed offset (`postbyte & 0x80 == 0`): $EA = R + \text{sex5}(\text{offset})$, consumes 1 NVMA cycle.
  - Constant offsets: 8-bit (+1 cycle), 16-bit (+4 cycles).
  - Accumulator offsets: A (+1 cycle), B (+1 cycle), D (+4 cycles).
  - Auto-increment / decrement: $,R+$ (+2 cycles), $,R++$ (+3 cycles), $,-R$ (+2 cycles), $,--R$ (+3 cycles).
  - Program Counter relative: 8-bit (+1 cycle), 16-bit (+5 cycles).
  - Extended Indirect: $[address]$ (+4 cycles).
- **Undocumented Instruction & Postbyte Handling:**
  - *Illegal Postbyte `$07`:* Decoded as accumulator A offset (mirroring `$06`), as documented by Flamand [dfffffff].
  - *Illegal Postbyte `$0E`:* Forces effective address to `$FFFF` and charges 4 dummy NVMA cycles.
  - *Stack Register Modification:* Auto-decrement on hardware stack `S` arms the NMI latch (`cpu->nmi_armed |= (REG_S != reg)`), matching physical silicon behavior where writing `S` enables NMI detection.
  - *Undocumented Arithmetic:* Opcode `op_xdec` (illegal `DEC`) alters the Carry flag based on borrowing from zero; opcode `op_xclr` (illegal `CLR`) leaves the Carry flag untouched (documented by David Banks [hoglet67]).
  - *Register-to-Register Transfer (`TFR` / `EXG`):* Transfers between mismatched 8-bit and 16-bit registers follow Tim Lindner's hardware analysis (e.g. transferring an 8-bit accumulator into a 16-bit pointer sign-extends or pads with `$FF`).

---

### 7.3 Interrupt Controller, Latching Semantics, and Stacking Contracts

#### Interrupt Priority & Hierarchy
The MC6809E enforces a strict priority hierarchy among its hardware interrupts and software exceptions:

$$\mathbf{RESET} > \mathbf{NMI} > \mathbf{SWI} > \mathbf{FIRQ} > \mathbf{IRQ} > \mathbf{SWI2} > \mathbf{SWI3}$$

| Vector Name | Priority | Vector Address | Stacking Frame | Condition Code Mask Asserted | Triggering Mode |
|---|---|---|---|---|---|
| **RESET** | 1 (Highest) | `$FFFE–$FFFF` | None | `CC_F = 1`, `CC_I = 1`, `DP = $00` | Level / Power-On |
| **NMI** | 2 | `$FFFC–$FFFD` | Full (12 bytes, $E=1$) | `CC_F = 1`, `CC_I = 1` | Negative-Edge Triggered |
| **SWI** | 3 | `$FFFA–$FFFB` | Full (12 bytes, $E=1$) | `CC_F = 1`, `CC_I = 1` | Software Instruction |
| **FIRQ** | 4 | `$FFF6–$FFF7` | Fast (3 bytes, $E=0$) | `CC_F = 1`, `CC_I = 1` | Level-Sensitive (Active Low) |
| **IRQ** | 5 | `$FFF8–$FFF9` | Full (12 bytes, $E=1$) | `CC_I = 1` | Level-Sensitive (Active Low) |
| **SWI2** | 6 | `$FFF4–$FFF5` | Full (12 bytes, $E=1$) | None | Software Instruction |
| **SWI3** | 7 (Lowest) | `$FFF2–$FFF3` | Full (12 bytes, $E=1$) | None | Software Instruction |

#### Sampling Point & Cycle Latching Semantics
On physical MC6809E silicon, interrupt pins are sampled on the **falling edge of the $Q$ clock** during every bus cycle.
- In the firmware, `cpu->nmi_latch`, `cpu->firq_latch`, and `cpu->irq_latch` are updated during every bus transaction in `fetch_byte_notrace`, `store_byte`, and `nvma_cycle`:
  ```c
  cpu->nmi_latch |= (cpu->nmi_armed && cpu->nmi);
  cpu->firq_latch = cpu->firq;
  cpu->irq_latch  = cpu->irq;
  ```
- At instruction boundaries (`mc6809_state_label_b`), the latched state is committed to `nmi_active`, `firq_active`, and `irq_active`.

#### NMI Arming Contract & Spurious Trigger Prevention
On physical hardware, a negative edge on the `NMI` pin is latched by an internal flip-flop.
- **Arming Requirement:** On reset or power-up, NMI is strictly **disarmed** (`cpu->nmi_armed = 0`). NMI is only armed after the hardware stack pointer `S` is initialized (e.g. via `LDS #$0F00`), or after an interrupt stack push occurs.
- **Architectural Purpose:** This hardware interlock prevents spurious glitches on the cartridge or expansion port from causing an unrecoverable crash before the operating system has established a valid stack frame in RAM.

#### Stacking Differences & Entire Flag ($E$) Semantics
The 6809 provides two distinct hardware interrupt stacking mechanisms:
1. **Full Stacking (`IRQ`, `NMI`, `SWI`):**
   - Sets the Entire flag in the Condition Code register: `REG_CC |= CC_E` ($E=1$).
   - Pushes 12 bytes onto the hardware stack `S` in descending order:
     $$\text{Push Order: } PC_{lo}, PC_{hi}, U_{lo}, U_{hi}, Y_{lo}, Y_{hi}, X_{lo}, X_{hi}, DP, B, A, CC$$
   - Executes 1 NVMA cycle + 2 dummy reads + 12 stack writes + 1 NVMA cycle + 2 vector fetch reads + 1 NVMA cycle = **19 execution cycles total**.
2. **Fast Stacking (`FIRQ`):**
   - Designed for high-speed I/O service routines.
   - Clears the Entire flag in the Condition Code register: `REG_CC &= ~CC_E` ($E=0$).
   - Pushes only 3 bytes onto the hardware stack `S`:
     $$\text{Push Order: } PC_{lo}, PC_{hi}, CC$$
   - Registers `A`, `B`, `DP`, `X`, `Y`, and `U` remain unstacked and untouched.
   - Executes 1 NVMA cycle + 2 dummy reads + 3 stack writes + 1 NVMA cycle + 2 vector fetch reads + 1 NVMA cycle = **10 execution cycles total**.
3. **Return From Interrupt (`RTI`):**
   - Pulls the stacked `CC` register from stack `S` and inspects bit 7 (`CC_E`):
     - If `CC_E == 0` (Fast Frame): Pulls only `PC` from stack `S` (6 cycles total).
     - If `CC_E == 1` (Full Frame): Pulls registers `A`, `B`, `DP`, `X`, `Y`, `U`, `PC` from stack `S` (15 cycles total).

#### Wait Instructions (`SYNC` & `CWAI`)
- **`SYNC` (Synchronize with Interrupt):** The MPU halts instruction processing and enters state `mc6809_state_sync`, executing dummy NVMA cycles while asserting `BA=1, BS=0`. When any interrupt line transitions active (even if masked in `CC`), the MPU exits the sync state. If the interrupt is unmasked, it vectors to the interrupt service routine; if masked, execution resumes immediately with the next sequential instruction.
- **`CWAI` (Clear and Wait for Interrupt):** Takes an immediate 8-bit operand, performs `REG_CC &= operand`, sets `CC_E = 1`, stacks all registers onto `S` (12 bytes), and waits in low-power state (`BA=1, BS=1`). When an unmasked interrupt arrives, the MPU acknowledges immediately and vectors to the service routine without the latency of stack pushes.

---

### 7.4 MC6883 Synchronous Address Multiplexer (SAM) & Rate Switching

The Motorola **MC6883** (or **SN74LS783**) Synchronous Address Multiplexer (SAM) is the central memory controller and timing generator of the Color Computer 2:
- Decodes the 6809's 16-bit address bus into Dynamic RAM row/column multiplexed addresses (`RAS0`, `RAS1`, `CAS`), ROM chip selects, and peripheral I/O selects.
- Generates the $E$ and $Q$ MPU clock phases from the 14.31818 MHz master clock.
- Clock-steps the MC6847 VDG display raster via horizontal sync (`HS`) and vertical frame sync (`FS`).

```
+--------------------------------------------------------------------------+
|                  MC6883 SAM Control Register Address Map                 |
|                                                                          |
|  Address Range    Function / Bits              Clear ($0)    Set ($1)    |
|  $FFC0 - $FFC1    $V_0$: VDG Mode Select 0     Clear Bit     Set Bit     |
|  $FFC2 - $FFC3    $V_1$: VDG Mode Select 1     Clear Bit     Set Bit     |
|  $FFC4 - $FFC5    $V_2$: VDG Mode Select 2     Clear Bit     Set Bit     |
|  $FFC6 - $FFD3    $F_0 - F_6$: Display Offset  Clear Bit     Set Bit     |
|  $FFD4 - $FFD5    $P_1$: RAM Page Select       Page 0 (Low)  Page 1 (Hi) |
|  $FFD6 - $FFD7    $R_0$: MPU Rate Bit 0        Slow (1x)     Fast (2x)   |
|  $FFD8 - $FFD9    $R_1$: MPU Rate Bit 1        Standard      Address-Dep |
|  $FFDA - $FFDB    $M_0$: Memory Size Bit 0     Clear Bit     Set Bit     |
|  $FFDC - $FFDD    $M_1$: Memory Size Bit 1     Clear Bit     Set Bit     |
|  $FFDE - $FFDF    $TY$: Memory Map Type        Normal Map    All-RAM Map |
+--------------------------------------------------------------------------+
```

#### Hardware Rate Switching Modes (`POKE 65494..65497`)
The SAM MPU rate bits ($R_1, R_0$) control the clock frequency delivered to the MC6809E:
1. **Standard 1× Speed ($R=0$): `POKE 65494,0` or `POKE 65496,0`:**
   - Every bus cycle charges **16 event ticks** (0.894886 MHz).
   - Standard execution speed; standard Color BASIC boot mode.
2. **Address-Dependent High Speed ($R=1$): `POKE 65495,0`:**
   - Accesses to ROM, Cartridge, and dummy NVMA cycles charge **8 event ticks** (1.789772 MHz).
   - Accesses to dynamic RAM and PIA0 ($FF00–$FF1F) charge **16 event ticks** (0.894886 MHz).
   - Delivers a measured **~1.62× real-time speedup** on Color BASIC execution (where interpreter bytecode executes from ROM while variables reside in RAM).
3. **Full Double Speed ($R=2$): `POKE 65497,0`:**
   - Every bus cycle charges **8 event ticks** (**full 2.00× speedup** at 1.789772 MHz).
   - All instructions execute in exactly half their standard duration.

#### Silicon Physical Reality vs Emulation Modeling Trade-Off
A critical architectural analysis reveals a fundamental difference between physical SAM silicon and the CoCo Zero emulation model:
- **Silicon Hardware Reality:**
  - On physical CoCo 2 hardware, the SAM interleaves dynamic RAM cycles between the MPU and the MC6847 VDG. The VDG fetches display bytes during the first half of each 0.895 MHz clock cycle, while the MPU accesses memory during the second half.
  - When the MPU is accelerated to 1.79 MHz ($R=1$ or $R=2$), there are insufficient clock phases to satisfy both the VDG and the MPU. In address-dependent mode, the SAM must stretch or slip the clock phases to avoid collisions, introducing variable transitional cycles of **15, 17, or 25 master clock ticks**.
  - At full 2× speed ($POKE 65497,0$), the VDG is starved of RAM access cycles, causing the video display on physical hardware to break down into severe visual noise ("screen hash" or "snow").
- **CoCo Zero Emulation Model & Justification:**
  - The firmware models rate switching as **flat, discrete quanta**: slow cycles charge exactly 16 event ticks, and fast cycles charge exactly 8 event ticks (`coco_fastbus.ticks` and `rom_ticks`).
  - *Empirical Timing Validation:* Over instruction execution sequences, physical 15/17/25-tick transitional variations average out to within $<1.8\%$ of the flat 8/16-tick model.
  - *Performance Trade-Off:* The flat model allows branchless, branch-predicted execution in `coco_fastbus`, avoiding expensive transitional phase state machines that would blow the 16.67 ms real-time frame budget on the Cortex-M33.
  - *Clean Scanout Advantage:* In CoCo Zero, display scanout is executed by Core 1 directly from the RGB565 framebuffer (`g_fb`). As a result, the display remains **100% clean, crisp, and readable** at full 2× speed, eliminating the screen hash of physical hardware.
  - *Audio Pacing Invariance:* The frame budget remains fixed at $238,640$ event ticks per frame. Because audio samples are synthesized per event tick, the frame cadence produces exactly 800 samples/frame, guaranteeing zero audio pitch distortion or buffer underrun.

---

### 7.5 Dual MC6821 Peripheral Interface Adapters (PIAs)

The Color Computer 2 incorporates two Motorola **MC6821** Peripheral Interface Adapters (PIA0 and PIA1) governing keyboard input, analog joysticks, audio DAC, cassette, and VDG control:

```
+--------------------------------------------------------------------------+
|                  MC6821 PIA0 / PIA1 Register Assignments                 |
|                                                                          |
|  Address Range    Register    Primary Subsystem Connected                |
|  $FF00            PIA0 Port A Keyboard Column Inputs / Joy Comparator    |
|  $FF01            PIA0 CRA    Control A / VDG HS Interrupt (CA1)         |
|  $FF02            PIA0 Port B Keyboard Row Drive Outputs (PB0 - PB7)     |
|  $FF03            PIA0 CRB    Control B / VDG FS 60Hz Interrupt (CB1)    |
|  $FF20            PIA1 Port A 6-bit Sound DAC (PA2 - PA7)                |
|  $FF21            PIA1 CRA    Control A / Cassette Data Input (CA1)      |
|  $FF22            PIA1 Port B Cassette Out / Single-Bit Sound / VDG Mode |
|  $FF23            PIA1 CRB    Control B / Cartridge Interrupt (CB1)      |
+--------------------------------------------------------------------------+
```

1. **PIA0 Operations ($FF00–$FF1F):**
   - **Keyboard Matrix Scanning:** Port B outputs drive the 8 keyboard rows active-low (`PB0–PB7`). Port A inputs read the 8 keyboard columns (`PA0–PA7`). When a key is pressed, its corresponding column bit pulls low.
   - **Joystick ADC Conversion:** Port A bit 0 (`PA0`) reads the output of an analog voltage comparator. The 6809 executes a software successive-approximation analog-to-digital converter (SAR ADC) by writing test values to the 6-bit DAC (PIA1 Port A) and reading the comparator on `PA0`.
   - **Vertical Sync Interrupt (CB1):** The MC6847 VDG's vertical Frame Sync (`FS`, ~60.0 Hz) is connected to PIA0 `CB1`. Each falling edge asserts the PIA0 CB1 interrupt flag, raising an `IRQ` to the 6809. Color BASIC hooks this vector to increment its software real-time clock at `$0112–$0114` and poll the keyboard.
   - **Horizontal Sync Interrupt (CA1):** The VDG's Horizontal Sync (`HS`, 15.75 kHz) is connected to PIA0 `CA1`.
2. **PIA1 Operations ($FF20–$FF3F):**
   - **6-Bit Resistor Ladder DAC:** Port A bits 2 through 7 (`PA2–PA7`) drive a 6-bit R-2R ladder digital-to-analog converter ($FF20). Writes to this register are intercepted by `audio_integ.h` to synthesize 48 kHz digital audio.
   - **Audio Source Multiplexer:** PIA1 control lines select the active audio source routed to the television RF modulator (DAC audio, cassette audio, or cartridge audio).
   - **VDG Mode Control Lines:** Port B bits 3 through 7 drive the MC6847 VDG hardware mode pins ($A/G$, $GM_0$, $GM_1$, $GM_2$, $CSS$), governing text vs graphics selection and color palettes.
   - **Single-Bit Sound:** PIA1 Port B bit 3 drives a 1-bit sound output flip-flop used for simple beeps and cassette tape modulation.

---

### 7.6 Experimental 128 KB CoCo 3 Emulation Architecture (`pizero_coco3`)

The repository includes a dedicated build environment, **`pizero_coco3`** (PIZERO-133), providing full bare-metal emulation of a 128 KB Tandy Color Computer 3:
- **System ROM:** Requires the 32 KB CoCo 3 ROM image (`coco3.rom`) located in `/coco/roms/coco3.rom` (Super Extended Color BASIC).
- **Memory Architecture:** Models 128 KB of unified system RAM partitioned into sixteen 8 KB blocks mapped via the CoCo 3 MMU.
- **GIME Chip Emulation (`tcc1014.c`):** Adapts the full XRoar GIME ASIC model to govern memory management, programmable interrupt timers, palette registers, and video generation.
- **MMU Paged Fast Path (`COCO_FASTBUS_PAGES`, PIZERO-213):** Employs an inlined page table that resolves physical 8 KB memory pages directly in the 6809 core, preserving high instruction throughput despite dynamic MMU remapping.
- **Port-Side GIME Scanline Renderer (PIZERO-218):**
  - Bypasses traditional frame rendering and blitting passes (`render` and `blit` = 0 µs).
  - During horizontal sync (`do_hs_fall`), active scanlines are decoded directly from VRAM into the $320 \times 240$ RGB565 framebuffer using the GIME 64-entry color table (`coco3_bus_render_line`).
  - Scanline dirty-checking (`port_sum`) skips re-rendering unchanged scanlines, maintaining low video CPU overhead (~3.0 ms worst-case).
- **Performance Characteristics (PIZERO-219, PIZERO-220):**
  - Runs at locked 60.0 fps at both 0.895 MHz and 1.789772 MHz (`POKE 65497,0`).
  - Full-screen high-speed animations (HCLS loops at 1.79 MHz) execute in ~12.9 ms median (15.2 ms worst-case), maintaining full 60.0 fps with zero late frames and 0% audio shortfall.
- **Video Modes & Status:** Supports 32- and 40-column text modes and 256- and 320-pixel graphics (HSCREEN 0–2). 640-pixel wide modes (80-column text) are currently downscaled to 320 pixels (PIZERO-134); ALT, CTRL, F1, and F2 keys are pending mapping (PIZERO-215).

---

## 8. User Interface & Software Features

### 8.1 On-Screen Display (OSD) Overlays

The OSD runs over a 32-column × 16-row character card rendered using the 6847T2 font. The running emulator pauses completely during overlay display.

| Key | macOS / PC Shortcut | Overlay Menu | Description & Functionality |
|---|---|---|---|
| **F12** | **Cmd+D** / **Ctrl+D** | **`< DISKS >`** | Displays available `.dsk` files in `/coco/dsk/`. Keys **0–3** insert/eject the selected disk into virtual drive 0, 1, 2, or 3 live. **ENTER** mounts disk in drive 0, cold-boots the system, and auto-executes the disk's first program. Key **W** toggles drive write-protection lock. Key **N** prompts for name and formats a new blank `.dsk`. Key **R** renames the highlighted disk. Key **TAB** opens the built-in text editor on that disk's `.TXT` settings. |
| **F9** | **Cmd+P** / **Ctrl+P** | **`< PROGRAMS >`** | Displays `.bin` binary files in `/coco/bin/`. **ENTER** direct-loads the binary into RAM and jumps to the execution address. **TAB** edits that program's `.TXT` settings. |
| **F10** | **Cmd+C** / **Ctrl+C** | **`< CARTRIDGES >`** | Displays `.ccc` cartridge images in `/coco/cart/`. **ENTER** attaches the cartridge and performs a cold reset into the cartridge. **TAB** edits that cartridge's `.TXT` settings. |
| **F11** | *N/A* | **`< FILES >`** | Lists editable configuration files (`SETTINGS.TXT` and `AUTORUN.TXT`). **ENTER** opens the built-in text editor. |
| **F1** | **Cmd+I** / **Ctrl+I** | **`< INFO >`** | Displays system diagnostic telemetry: firmware version, git commit, RP2350 chip ID and revision, board unique serial ID, system clock, uptime, free heap memory, and attached USB devices. |
| **F8** | **Cmd+A** / **Ctrl+A** | *Artifact Cycle* | Directly cycles through NTSC artifact modes (`on` $\rightarrow$ `swapped` $\rightarrow$ `off`) and automatically saves the setting to that title's `.TXT` configuration file (or `settings.txt`). |
| **PrtScn** | *N/A* | *Screenshot* | Captures the active $320 \times 240$ RGB565 display buffer, compresses it to PNG format, and writes it to `/coco/shots/`. |

### 8.2 Built-In On-Screen Text Editor (`text_editor.cpp` / `text_edit.h`)

Accessible via **F11** or by pressing **TAB** on any highlighted game in the F9/F10/F12 lists:
- Flat 4,096-byte memory buffer (`TED_MAX = 4096`).
- Automatic conversion of Windows CRLF line endings to clean `\n`.
- Cursor positioning, vertical and horizontal viewport scrolling over the 32×16 card.
- **Ctrl-S**: Performs an atomic file write (`file.tmp` $\rightarrow$ `file.txt`) to protect against corruption during power removal. Takes effect immediately.
- **ESC**: Exits editor (prompts if unsaved changes exist).

### 8.3 Configuration System (`settings.txt` & Per-Game `.TXT`)

Settings are plain text key-value pairs stored in `/coco/settings.txt`:
```ini
# /coco/settings.txt
sn76489           = on        # Enable SN76489 sound chip at $FF41
volume            = 10        # Global audio volume (0-15; 6x gain boost over early builds)
audio_lowpass     = 8000      # 2-pole Butterworth low-pass filter (1000-20000, on=8000, off)
artifact_colors   = on        # PMODE 4 NTSC color artifacts (on / off / swapped)
gime_palette      = on        # CoCo 3 palette registers at $FFB0-$FFBF
gime_timer        = on        # CoCo 3 timer/interrupts at $FF90-$FF95
autorun           = on        # Automatically run disk or autorun.txt on boot
reset_button      = basic     # RUN button action: 'basic' (prompt) or 'autorun'
serial_keyboard   = on        # Accept serial keystrokes over USB-CDC
joystick_swap     = off       # Swap left/right gamepad sticks
font              = 6847t2    # Font set: classic / 6847t1 / 6847t2
lowercase         = on        # True lowercase display
key_repeat        = on        # Keyboard auto-repeat
key_repeat_delay  = 500       # Repeat delay in milliseconds
key_repeat_rate   = 10        # Repeats per second (1-12)
video_encoder     = hardware  # TMDS encoder: hardware (SIO) / software
soak_log          = off       # Standalone minute-by-minute soak telemetry to /coco/log/
dpad              = joystick  # Gamepad D-pad mode: joystick / arrows
pad_start         = enter     # Gamepad button mapping
```

**Per-Game Custom Configurations:** Any game can have an accompanying `.TXT` file in the same directory (e.g. `/coco/bin/ORBIT.TXT` for `/coco/bin/ORBIT.BIN`). When launching, `settings.txt` is loaded first, followed by the game's `.TXT` file on top, allowing per-game button remapping, artifact phases, or custom palettes. Pressing **TAB** in any OSD list instantly opens the editor for that title's `.TXT` file.

### 8.4 Autorun Automation (`autorun.txt`)

If `autorun = on`, `/coco/autorun.txt` is evaluated at boot:
- `# comment`: Comment line.
- `@CART <filename.ccc>`: Automatically mounts the specified cartridge and boots.
- `@DIRECT <filename.bin>`: Direct-loads binary into RAM and executes immediately.
- `<BASIC commands>`: Types commands line-by-line into the BASIC interpreter (e.g. `LOADM"GAME":EXEC`).
- `~`: Pause delay token. Each `~` pauses typing by **1.0 second**, allowing disk operating systems (such as NitrOS-9 booting via `DOS`) sufficient time to complete initialization before commands are executed.
- Default (if `autorun.txt` is absent): Automatically mounts the first disk in `/coco/dsk/` and runs its first program.
- Bypass: Holding **Space** or **Break** during boot skips autorun and halts at the BASIC `OK` prompt.

---

## 9. System Reliability, Watchdog & Telemetry

### 9.1 Hardware Watchdog & Post-Mortem Crash Diagnostics (PIZERO-227)

- **Timeout:** 3,000 ms hardware watchdog timer (`watchdog_enable(WATCHDOG_TIMEOUT_MS, true)`).
- **Execution Phase Tracking:** As Core 0 loops, it updates the hardware watchdog scratch registers:
  - `scratch[0]`: `WD_MAGIC` (`0x05330534u`) sentinel distinguishing valid freeze records from power-on noise.
  - `scratch[1]`: Execution phase enum (`WP_SETUP`=1, `WP_LOOP`=2, `WP_USB`=3, `WP_KBD`=4, `WP_EMU`=5, `WP_RENDER`=6, `WP_BLIT`=7, `WP_AUDIO`=8, `WP_PACE`=9, `WP_TELEM`=10, `WP_SOAKLOG`=11).
  - `scratch[2]`: Heartbeat / elapsed frame counter prior to freeze.
  - `scratch[3]`: Persistent freeze tally: `[31:8]=cumulative_freeze_count | [7:0]=last_freeze_phase`.
  - `scratch[4]`: `USB_REPLUG_MAGIC` (`0x51D15C09u`) deliberate reboot sentinel for USB re-enumeration.

#### Watchdog Post-Mortem Subsystem (`wd_post.cpp` / `wd_post.h`)
To eliminate guesswork when Core 0 stalls, the firmware deploys an autonomous early-warning and fault-capture mechanism that saves real CPU state across watchdog resets:
1. **Early Warning Interrupt:** A spare hardware timer alarm (`timer0`) running at the highest interrupt priority on Core 0 evaluates the watchdog countdown every 200 ms. If fewer than 1,000 ms remain before timeout, it interrupts execution and captures:
   - `watchdog scratch[5]`: PC of the interrupted code (resolvable via `arm-none-eabi-addr2line`).
   - `watchdog scratch[6]`: LR (Link Register).
   - `watchdog scratch[7]`: State flags (`kind << 24 | core << 20 | saves << 12 | exception_number`).
   - `powman scratch[0]`: System uptime in milliseconds at the last save.
   - `powman scratch[1]`: `(span_ms << 16) | core1_lines` (verifies whether Core 1 DVI scanout continued running while Core 0 stalled).
   - `powman scratch[2]`: `(guest_6809_pc << 16) | (usb_device_count << 8) | exc_return_byte`.
   - `powman scratch[3..6]`: Core 0 NVIC interrupt state (`IABR[0..1]` active interrupts and `ISPR[0..1]` pending interrupts), or ARM Cortex-M33 SCB fault status registers (`CFSR`, `HFSR`, `BFAR`, `MMFAR`).
   - `powman scratch[7]`: `WDP_MAGIC` (`0x57445031u` = `'WDP1'`).
2. **Dedicated HardFault Vector:** A custom HardFault handler captures CPU exception frames directly into the post-mortem scratch registers, allowing crashes (null pointers, bus faults) to be distinguished from execution deadlocks.
3. **Post-Boot Reporting:** Upon rebooting from a freeze, `wd_post_describe()` formats the persistent context into a structured console log:
   ```text
   [watchdog] post-mortem: core 0 stuck at 0x1000b240 in WP_PACE (6809 PC=0xA042, core 1 alive)
   ```

### 9.2 Telemetry Streams (`[run]` & `[aud]` Serial Logs)

Every second, Core 0 outputs performance and status telemetry over the USB-CDC serial port at 115,200 baud:
```text
[run] fps=60.0 cpu=6210us blit=1612us aud=0us free_ram=102KB usb=1 hid_rpts=120 freezes=0 slack=7600us late=0 cmax=6300us wmax=9100us badfps=0
```
- `cpu`: Core 0 6809 emulation duration in microseconds (~6,200 µs at normal 1× speed; ~12,600 µs at 2× speed).
- `slack`: Minimum spare headroom remaining in any single frame during that 1-second window.
- `late`: Number of frames that failed to complete within the 16,667 µs frame budget.
- `cmax`: Maximum Core 0 emulation duration encountered during that second.
- `wmax`: Maximum whole-frame workload (emulation + render + blit + USB + audio) during that second.
- `badfps`: Cumulative seconds since boot where frame rate dropped below 60.0 fps.

#### Standalone MicroSD Soak Logging (`soak_log = on`, PIZERO-201)
When `soak_log = on` is configured in `settings.txt`:
- The system automatically creates a timestamped log file `/coco/log/soak-NNN.txt` on the microSD card at power-on.
- The log header records firmware version, build environment, reset reason (power-on vs watchdog recovery), and the `[watchdog] post-mortem:` diagnostic summary if recovering from a freeze.
- Every minute, the board flushes synchronized `[run]` and `[aud]` telemetry records to disk (~8 ms per write).
- Timekeeping is maintained by the RP2350 always-on timer, seeded at power-on from the newest file modification timestamp on the card.
- Allows long-term burn-in and stability validation (such as 26.7-hour unattended soak runs) using only an external 5V USB-C phone charger.

### 9.3 Host Emulator Telemetry & Console Stream

The emulator captures the RP2350's hardware UART0 controller, bridging internal firmware logs and runtime diagnostics to the host terminal:
- **Live Console Output:** Every byte written to UART0 TX is printed directly to host `stdout` and appended to a synchronized internal log buffer (`serial_tx_log`).
- **Dynamic Status Line:** Every simulated second, the emulator renders a real-time progress update over `stdout`:
  ```text
  [Emulating] Frame:   1520 | Virt-FPS:  60.1 | Core 0 PC: 0x10006BAC | Core 1 PC: 0x100220D8
  ```
- **Readiness Announcement:** When virtual cycles reach the Color BASIC boot readiness threshold (630,000,000 cycles), the console announces:
  ```text
  [System Ready] Color BASIC initialization complete! Ready for keyboard input.
  ```

---

## 10. Physical Enclosure Specifications

The custom enclosure (`hardware/case/pizero_case.scad`) is fully parametric and rendered in OpenSCAD:

| Feature | Specification |
|---|---|
| **External Dimensions** | 69.8 mm (Length) × 34.8 mm (Width) × 18.6 mm (Height) |
| **Aesthetic Style** | Tandy Color Computer 2 slatted ventilation grille |
| **Construction** | 2-piece split (Base tray + Top lid) |
| **Split Datum** | Split at the top plane of the PCB (eliminates 3D print overhangs/bridging) |
| **Fasteners** | 4 × M2.5 countersunk screws (12 mm length), self-tapping into top lid bosses |
| **Port Cutouts** | Mini HDMI, Dual USB-C, MicroSD slot, battery JST header |
| **Plug Pockets** | Shallow 0.8 mm overmould relief pockets on USB-C and HDMI ports |
| **Button Access** | RUN and BOOT buttons reachable through top ventilation slots |

---

## 11. Firmware Build Environments Matrix

### 11.1 Firmware PlatformIO Build Profiles

The project is built using PlatformIO with the Earle F. Philhower, III Arduino-Pico core targeting the RP2350B (`board = waveshare_rp2350_pizero`):

| Build Environment | Video Mode | Audio Delivery | Sysclk / Pixel | Primary Purpose |
|---|---|---|---|---|
| **`pizero_stream_60`** | **640×480p @ 60 Hz** | **523-Line Streaming Islands** | **252 MHz / 25.2 MHz** | **Default Product Build:** 8/92/60 wide back-porch timing, full 4-sample packets, strict framing, USB host, GIME palette & timer, SAM double speed. |
| **`pizero_stream`** | 640×480p @ ~52 Hz | Streaming Data Islands | 240 MHz / 24.0 MHz | Fallback build for legacy displays rejecting 25.2 MHz 60 Hz video. |
| **`pizero_coco3`** | **640×480p @ 60 Hz** | **523-Line Streaming Islands** | **252 MHz / 25.2 MHz** | **128 KB CoCo 3 Build:** Boots Super Extended Color BASIC (`coco3.rom`), full GIME MMU, direct scanline rendering at 0.89 and 1.79 MHz. |
| **`pizero_cpu179`** | 640×480p @ 60 Hz | Streaming Data Islands | 252 MHz / 25.2 MHz | Benchmark: Runs 2× 6809 cycles per frame to baseline 1.79 MHz CPU load. |
| **`pizero_cpuprof`** | 640×480p @ 60 Hz | Streaming Data Islands | 252 MHz / 25.2 MHz | Diagnostic: Profiles Cortex-M33 clocks per emulated 6809 bus cycle via DWT. |
| **`pizero_coco3_prof`** | 640×480p @ 60 Hz | Streaming Data Islands | 252 MHz / 25.2 MHz | Diagnostic: Profiles CoCo 3 clocks across GIME bus and port scanline renderer. |
| **`pizero_fdctrace`** | 640×480p @ 60 Hz | Streaming Data Islands | 252 MHz / 25.2 MHz | Diagnostic: Deep FDC, sector read/write checksum, and PC tracing over USB serial. |
| **`pizero_fdctrace_uart`** | 640×480p @ 60 Hz | Streaming Data Islands | 252 MHz / 25.2 MHz | Diagnostic: Routes FDC trace output over UART0 (Serial1) for host emulator capture. |
| **`pizero_usbdebug`** | 640×480p @ 60 Hz | Streaming Data Islands | 252 MHz / 25.2 MHz | Diagnostic: TinyUSB host debug logging to serial console. |
| **`pizero_padprobe`** | 640×480p @ 60 Hz | Streaming Data Islands | 252 MHz / 25.2 MHz | Diagnostic: Dumps raw gamepad HID report descriptors to serial. |
| **`pizero_videobench`** | 640×480p @ 60 Hz | Streaming Data Islands | 252 MHz / 25.2 MHz | Diagnostic: Benchmarks scanline TMDS encoding performance in SRAM. |
| **`pizero_wavmeas`** | Disabled | WAV Stream via USB-CDC | 240 MHz | Diagnostic: Streams 48 kHz audio as base64 WAV over USB for frequency analysis. |
| **`pizero_wavmeas_boot`** | Disabled | WAV Stream via USB-CDC | 240 MHz | Diagnostic: Boots normally and records 20 s audio on serial trigger (Ctrl-W). |
| **`pizero_chord`** | 640×480p @ 60 Hz | Synthetic C-Major Chord | 252 MHz / 25.2 MHz | Diagnostic: Core 1 synthesized chord (sines/squares) to isolate HDMI audio link. |
| **`pizero_vic1` / `aif`** | 640×480p @ 60 Hz | Streaming Data Islands | 252 MHz / 25.2 MHz | Diagnostics: Validates CEA VIC 1 and Audio InfoFrame stream referral. |
| **`pizero_drive8` / `slew2`**| 640×480p @ 60 Hz | Streaming Data Islands | 252 MHz / 25.2 MHz | Diagnostics: Evaluates 8 mA drive and fast-slew TMDS pad configurations. |
| **`pizero_wdtest`** | 640×480p @ ~52 Hz | Streaming Data Islands | 240 MHz | Diagnostic: Forces Core 0 lockup to verify watchdog recovery and post-mortem logs. |
| **`native`** | N/A | N/A | N/A | Host Unit Tests: 20 test suites (254 test cases) executed on host PC via Unity. |

### 11.2 Host Emulator Build Environment (`cocozero-rp2350`)

The host emulator is implemented in Rust (2024 edition) and built via Cargo:
- **Workspace Architecture:**
  - `cocozero-rp2350`: Top-level binary application and integration library (`src/`).
  - `crates/rp2350-emu`: Standalone RP2350B hardware microcontroller emulation core (`crates/rp2350-emu/`).
- **Dependencies:**
  - `sdl2` (v0.38): Window management, RGB565 streaming texture rendering, event pump, and 48 kHz audio queue.
  - `clap` (v4.5): Command-line argument parsing with derive macros.
  - `byteorder` (v1.5) & `crc` (v3.2): Binary serialization and CRC checksum calculation.
- **Build Configurations:**
  - Development: `cargo build` / `cargo test`
  - Production (Optimized): `cargo build --release` (`opt-level = 3`, `lto = "thin"`, `codegen-units = 1`, `panic = "abort"`).

### 11.3 Critical Compiler & Toolchain Constraints (Firmware)

1. **`PICO_NO_HARDWARE` Macro Trap:** In the arduino-pico core, `PICO_NO_HARDWARE` is defined as `0`. Standard SDK macros like `__not_in_flash_func()` evaluate `#if defined(PICO_NO_HARDWARE)` as true, silently causing critical time-sensitive functions to remain in flash memory. All time-critical interrupt handlers and encoders must use explicit section attributes:
   ```c
   #define DVI_DI_RAMFUNC __attribute__((section(".time_critical.dvi_di")))
   ```
2. **PIO GPIO Windowing:** Because DVI TMDS lanes reside on GPIO 32–39 (above GPIO 31), PIO requires `-DPICO_PIO_USE_GPIO_BASE=1` and `pio_set_gpio_base(pio, 16)`.
3. **Include Ordering:** `#include <Arduino.h>` must always precede XRoar headers to prevent conflicts with C99 `_Bool`.
4. **Build Flag Caching:** PlatformIO SCons does not reliably invalidate cached compilation objects when flags are passed through `PLATFORMIO_BUILD_FLAGS` environment variables. Real environment profiles defined in `platformio.ini` extending `pizero_base` must be used instead.

---

## 12. Test Coverage & Verification Suites

### 12.1 Reference Firmware Unity Unit Tests

The C/C++ firmware architecture isolates pure emulation, parsing, and data manipulation logic into header-only modules tested under `platform = native` via Unity (20 test suites, 254 test cases):
- `test_audio_integ`: Resampler integration catch-up over multi-cycle spans and mixed 8/16-tick runs.
- `test_audio_lowpass`: Two-pole Butterworth low-pass filter and DC blocker frequency response and stability.
- `test_audio_servo`: Frequency error compensation and audio ring level servo.
- `test_cart_gmc`: Bank-switched Games Master Cartridge address decoding.
- `test_coco_palette`: RGB565 color palette lookup and GIME registers.
- `test_coco3_boot`: Boots 128 KB CoCo 3 machine on host, validates GIME bus arbiter and MMU paged fast path.
- `test_csg_sn76489`: SN76489 sound chip attenuation, tone, and noise generation.
- `test_data_island`: TERC4 encoding, BCH error correction, and InfoFrame generation.
- `test_dsk_catalog`: Directory scanning, sorting, and pagination.
- `test_gamepad`: USB HID report decoding for DualShock 4, XInput, and Android pads.
- `test_gime_timer`: CoCo 3 50 kHz timer and interrupt generation.
- `test_key_translate`: USB HID keycap to CoCo matrix chord translation.
- `test_mc6809_lockstep`: Runs frozen reference 6809 core against optimized live core in lockstep, verifying opcode execution, indexed modes, and registers.
- `test_overlay_keys`: OSD overlay menu navigation and state machine.
- `test_png_write`: MicroSD PNG screenshot compression and output.
- `test_rsdos_dir`: RS-DOS / Disk BASIC directory parser and executable detection.
- `test_settings`: Configuration parser and key-value serialization.
- `test_text_card`: 32×16 OSD character grid formatting and text wrapping.
- `test_text_edit`: On-screen text editor buffer manipulation and cursor movements.
- `test_vdg_pack`: MC6847 VDG pixel bit-packing and font rendering.

### 12.2 Host Emulator Rust Automated Test Suite (`cargo test`)

The host emulator incorporates an extensive automated test suite executed via `cargo test --tests --release`:
- **`tests/menu_test.rs`:** End-to-end integration test validating virtual FAT32 filesystem construction, boot to Color BASIC, F12 HID keycode injection, `/coco/dsk/` directory scanning (`ZORK1.DSK` verified with count = 1), menu overlay display rendering, Up/Down navigation, F1 Info mode activation, and Escape menu dismissal.
- **`tests/keyboard_test.rs`:** Validates Color BASIC boot detection, virtual keyboard matrix chording, hold/gap timing countdowns, and command execution in Color BASIC.
- **`tests/hardware_test.rs`:** Validates RP2350 SIO TMDS CPU execution and line doubling registers.
- **`tests/screen_test.rs`:** Validates SRAM framebuffer discovery and RGB565 extraction.
- **`tests/integration_test.rs`:** Validates full RP2350 SoC initialization and dual-core firmware execution.
- **`tests/multicore_test.rs`:** Validates SIO FIFO inter-core messaging, concurrent FIFO exchanges, and Boot ROM Core 1 WFE parking.
- **`tests/demo_test.rs`:** Validates UF2 image parsing and entry execution.
- **`tests/trace_fault.rs`:** Validates CPU fault tracing and instruction stepping.
- **Core Unit Tests (`src/lib.rs`):**
  - `test_spi_sd_initialization_sequence`: Validates CMD0, CMD8, CMD55, and ACMD41 SPI state transitions.
  - `test_spi_sd_block_read`: Validates CMD17 single-block read, `0xFE` token, 512-byte payload, and CRC16.
  - `test_spi_sd_block_write`: Validates CMD24 single-block write, data tokens, sector commit to storage, and `0x05` response.
  - `test_virtual_fat32_builder`: Validates in-memory FAT32 volume generation from a host directory hierarchy.
  - `test_sio_tmds_rgb565_doubled`: Validates SIO hardware TMDS pixel encoding algorithms.
  - `test_uf2_block_parsing`: Validates Microsoft UF2 flash block decoding.

---

## 13. Emulator Command-Line Interface & Operational Reference

The `cocozero-rp2350` binary exposes a flexible CLI for interactive execution, development, and automated scripting:

```text
Usage: cocozero-rp2350 [OPTIONS]
```

### 13.1 Command-Line Options Matrix

| Option | Flag | Type | Default | Description |
|---|---|---|---|---|
| `--turbo` | | `u32` | `2` | **Simulation speed multiplier.** Multiplies the virtual cycle quantum per frame. `--turbo 6` provides smooth 60.0 FPS real-time execution on typical desktop CPUs. |
| `--sd` | `-s` | `Path` | `./coco` | Path to host directory to package into virtual FAT32, or path to a raw `.img` SD disk image. |
| `--uf2` | `-u` | `Path` | `roms/cocozero.uf2` | Path to firmware UF2 binary. |
| `--bin` | `-b` | `Path` | *None* | Path to raw firmware binary (`.bin`). |
| `--bootrom` | | `Path` | `roms/rp2350/bootrom-combined.bin` | Path to RP2350 Boot ROM image. |
| `--headless` | | `bool` | `false` | Run without an SDL2 GUI window (for automated testing and benchmarks). |
| `--max-frames` | | `u64` | `0` | Terminate execution after $N$ frames (`0` = run indefinitely). |
| `--fb-addr` | | `String` | `0x20022744` | Explicit override for framebuffer SRAM address (e.g. `0x20010000`). |
| `--help` | `-h` | | | Print command-line help summary. |

### 13.2 Common Operational Examples

```bash
# Standard interactive execution with real-time 60 FPS performance
cargo run --release -- --turbo 6

# Mount a custom external games directory as the virtual SD card
cargo run --release -- --turbo 6 --sd /path/to/my_coco_folder

# Headless smoke test executing 300 frames (~5 simulated seconds)
cargo run --release -- --headless --max-frames 300 --turbo 6

# Execute the complete automated regression test suite
cargo test --tests --release
```
