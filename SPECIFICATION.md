# CoCo Zero: System Specification & Technical Reference Manual

**Target Platform:** Waveshare RP2350-PiZero  
**Standard Definition:** CoCo Zero Specification Standard ([ugufru/coco-zero](https://github.com/ugufru/coco-zero))  
**Emulated Systems:** Unified Triple-Machine Architecture — CoCo Zero (Enhanced CoCo 2), Tandy Color Computer 2 (Stock CoCo 2), and Tandy Color Computer 3 (Stock 128 KB CoCo 3)  
**Reference Firmware:** [ugufru/xroar-waveshare-rp2350-pizero](https://github.com/ugufru/xroar-waveshare-rp2350-pizero) (Git commit `8daafd5`, PIZERO-256 / PIZERO-255)  
**Host Hardware Emulator:** `cocozero-rp2350` (Rust / SDL2 / `rp2350-emu`)  
**Document Version:** 2.0.0  
**Status:** Authoritative Technical Specification & System Reference  

---

## 1. Executive Summary & Device Concept

The **CoCo Zero** is a standalone, bare-metal hardware implementation of the Tandy Color Computer (CoCo) family built on a Raspberry Pi Zero form-factor microcontroller board—the **Waveshare RP2350-PiZero**. The device requires only a digital display (TV or monitor), a USB-C keyboard or gamepad, a standard 5V USB-C power source (or 3.7V Li-Po battery), and a FAT32-formatted microSD card containing Color BASIC ROMs.

In accordance with the **CoCo Zero Specification Standard** ([`coco-zero`](file:///Users/richardlucente/development/git/coco-zero/README.md)):
- **Form Factor:** Strict physical adherence to the Raspberry Pi Zero footprint (65.0 mm × 30.0 mm with 3.0 mm corner radius and four M2.5 mounting holes).
- **Zero OS Architecture:** There is no underlying operating system (no Linux kernel, RTOS, or hypervisor). The hardware boots directly into Color BASIC in under two seconds.
- **Passive Thermal Design:** Completely silent operation with zero active fans or forced cooling.
- **Locked Real-Time Execution:** Emulation of the Motorola MC6809E CPU, memory bus controllers, PIAs, and video generators operates at a locked **60.0 fps real-time cadence** at both standard (0.895 MHz) and double speed (1.79 MHz), with zero dropped video frames and continuous, gapless audio.

In addition to bare-metal microcontroller operation, this repository provides **`cocozero-rp2350`**, a full-system hardware emulator written in Rust that models the RP2350B microcontroller (dual Cortex-M33 cores, SIO TMDS encoder, SPI1 SD controller, Boot ROM, and UART0) executing the unmodified bare-metal CoCo Zero firmware.

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
|  |  * VDG/GIME Raster |    |  * Multicore FIFO  |    |  * Sector Read/|  |
|  |  * Audio Mix & Ring|    |  * Active DMA IRQ  |    |    Write (CMD) |  |
|  +---------+----------+    +---------+----------+    +-------+--------+  |
|            |                         |                       |           |
|            +-----------+-------------+                       |           |
|                        |                                     |           |
|  Physical Hardware:    Mini HDMI / DVI Out, USB Host, SPI1   MicroSD Slot|
|  Host Emulator:        SDL2 640x480 @ 60Hz, AudioQueue,      Virtual SD /|
|                        Dual-Tier Matrix/HID Input, UART0     ./coco/ Dir |
+--------------------------------------------------------------------------+
```

### 1.1 Unified Triple-Machine Architecture

The reference firmware (`pizero_stream_60`) consolidates three distinct machine personalities into a **single production binary** (PIZERO-135, PIZERO-242), configured via `machine = cocozero | coco2 | coco3` in `settings.txt` or a title's companion `.TXT` override file:

| Machine Profile | Base Emulated Hardware | Unique Characteristics & Built-In Peripherals |
|---|---|---|
| **`cocozero`** *(Default)* | Enhanced Tandy Color Computer 2 | Authentic CoCo 2 architecture augmented with: built-in multi-chip audio (SN76489 CSG, 4× AY-3-8910 bank, Orchestra-90CC stereo DACs), CoCo 3 palette registers (`$FFB0–$FFBF`) and 50 kHz timer (`$FF90–$FF95`), true lowercase 6847T2 font, and keyboard auto-repeat. |
| **`coco2`** | Stock Tandy Color Computer 2 | Authentic factory specification: classic 6847 font (lowercase rendered as inverted uppercase), switched audio multiplexer, no extra sound chips, no auto-repeat. |
| **`coco3`** | Stock Tandy Color Computer 3 | Full 128 KB CoCo 3: Super Extended Color BASIC 2.1 (`coco3.rom`), GIME chip memory management, 640-pixel video modes (32, 40, and 80-column text), hardware palettes, dual-joystick second fire buttons, CTRL/ALT/F1/F2 keys, and NitrOS-9 Level 2 support. |

**Dynamic Machine Switching:** Launching software with a companion `.TXT` specifying a different machine automatically initiates a clean board restart (taking ~2 seconds) into the requested architecture. Launching software without an override cleanly returns to the machine declared in `/coco/settings.txt`.

### 1.2 Four-Tier Architectural Hierarchy

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
| Core 0 (Emulation & I/O): 6809 MPU loop, VDG/GIME raster, TinyUSB Host, Audio mix, FatFs SD       |
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
| I/O & Peripherals: Dual Motorola MC6821 PIAs, MC6847 VDG, WD2797 FDC, Multi-Chip Audio Subsystem  |
| Software: Color BASIC 1.2 / Extended BASIC 1.1 / Super Extended BASIC 2.1 / NitrOS-9 Levels 1 & 2|
+---------------------------------------------------------------------------------------------------+
```

| Execution Tier | Execution Substrate | Primary Function / Scope | Key Implementation Components |
|---|---|---|---|
| **Tier 1: Physical Microcontroller** | Waveshare RP2350-PiZero hardware PCB | Physical silicon, voltage regulation, physical connectors (mini-HDMI, dual USB-C, microSD). | RP2350B SoC, ETA6096 charger, TMI3112H buck, GPIO routing. |
| **Tier 2: Bare-Metal Firmware** | Dual ARM Cortex-M33 @ 252 MHz (Bare-metal C/C++) | Native firmware bridging guest 8-bit machine to RP2350 peripherals. | Core 0 run loop, Core 1 `libdvi`, Pico-PIO-USB, TinyUSB, FatFs, SIO TMDS. |
| **Tier 3: Emulated Guest System** | Virtual 8-bit Tandy Color Computer architecture | Emulated 6809E hardware running vintage software (ROMs, disks, OS-9). | `xroar_core` (MC6809E, MC6883 SAM, MC6821 PIAs, MC6847 VDG, WD2797 FDC, GIME). |
| **Tier 4: Host Hardware Emulator** | Host PC (x86_64 / aarch64) in Rust (`cocozero-rp2350`) | Simulates the RP2350B SoC to execute the Tier 2 firmware on desktop workstations. | `rp2350-emu` (dual Cortex-M33, SIO, SPI1, Flash, SRAM, NVIC, UART0), SDL2 UI. |

---

## 2. Hardware Architecture & Board Specifications

### 2.1 Microcontroller Specifications (RP2350B)

| Parameter | Specification | Notes |
|---|---|---|
| **Microcontroller IC** | Raspberry Pi RP2350B | 48 GPIO variant (QFN-80 package) |
| **CPU Cores** | Dual ARM Cortex-M33 @ 252 MHz | Overclocked from stock 150 MHz; Hardware FPU & DSP |
| **Core Voltage ($V_{REG}$)** | 1.25 V | Programmed via `vreg_set_voltage(VREG_VOLTAGE_1_25)` for 252 MHz stability |
| **System Clock ($f_{SYS}$)** | 252.000 MHz | Driven from TMDS bit clock; integer multiple of 48 MHz USB clock ($252 / 48 = 5.250$) |
| **On-Chip SRAM** | 520 KB | Unified low-latency system SRAM (SRAM0–SRAM7 + Scratch X/Y) |
| **Flash Memory** | 16 MB QSPI NOR Flash | Winbond W25Q128JVSI (Quad SPI XIP) |
| **External PSRAM** | None (unpopulated footprint `U1`) | PCB footprint exists (CS on GPIO 47); firmware operates within 520 KB SRAM |
| **Watchdog Timer** | Hardware Watchdog | 3000 ms timeout; early-warning timer alarm and fault handlers preserve post-mortem context |
| **Silicon Revision** | RP2350 Rev A2 / B2 (Checked) | Silicon revision inspected at boot for errata handling (e.g. Errata E9 pull-down leakage) |

### 2.2 Power Delivery & Charging Subsystem

The board incorporates comprehensive power management supporting both USB-C power and single-cell Lithium-Ion/Polymer batteries:
- **Battery Management IC:** ETA Solutions **ETA6096** switching lithium battery charger (with charging status LED `STAT`).
- **Battery Connector (`J3`):** 2-pin 1.25mm pitch JST connector for 3.7V Li-Po cells.
- **System Buck Regulator:** **TMI3112H** high-efficiency 1.5 MHz synchronous step-down buck converter delivering a regulated **3.3V system rail** from either 5V VBUS or battery VBAT.
- **Core LDO / SMPS:** RP2350 internal switching regulator stepped up to **1.25V** core supply.
- **Power Consumption:** Typical current draw is approximately 140–180 mA @ 5V (~0.8 W) during active 60 fps emulation with digital audio.

### 2.3 Board Form Factor & Physical Layout

The Waveshare RP2350-PiZero matches the physical outline of a Raspberry Pi Zero, adhering strictly to the CoCo Zero standard ([`spec/form-factor.md`](file:///Users/richardlucente/development/git/coco-zero/spec/form-factor.md)):
- **Board Outline:** 65.0 mm × 30.0 mm rectangle with 3.0 mm corner radius.
- **Mounting Holes:** Four holes for M2.5 hardware, drilled $2.75 \pm 0.05\text{ mm}$.
- **Hole Centers:** Origin at board corner $(0, 0)$: $(3.5, 3.5)$, $(61.5, 3.5)$, $(3.5, 26.5)$, and $(61.5, 26.5)\text{ mm}$ (58.0 mm × 23.0 mm grid spacing).

Its physical port layout is:
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
| **GPIO 47** | `PSRAM_CS` | Output | QSPI / GPIO | Chip select for unpopulated external PSRAM footprint (`U1`) |

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
  - **Hardware TMDS Encoder:** Emulates RP2350 SIO hardware TMDS encoder registers (`TMDS_CTRL`, `TMDS_WDATA`, `TMDS_PEEK_SINGLE`), accelerating 8-bit palette-indexed scanline encoding in Core 1.
  - **Multicore FIFO:** Models the bi-directional 8-deep FIFO (`FIFO_ST`, `FIFO_WR`, `FIFO_RD`) with automatic NVIC cross-core IRQ pending generation (IRQ 15/16).
  - **GPIO Output Registers:** Monitors `GPIO_HI_OUT` bit 11 (GPIO 43) to track software Chip Select assertions for the MicroSD controller.
- **Memory Bus Dispatcher:** 520 KB unified on-chip SRAM (`0x2000_0000..0x2008_2000`) with mirror alias decoding, 16 MB QSPI NOR Flash (`0x1000_0000..0x1100_0000`), PPB, and Boot RAM.
- **Hardware UART0 Interception:** Intercepts byte writes to UART0 TX (`0x4003_4000`), printing characters directly to host terminal `stdout` and appending to an internal circular telemetry buffer (`serial_tx_log`).
- **Simulation Turbo Multiplier (`--turbo <N>`):**
  - Scales the per-frame virtual cycle slice: $\text{Cycles per frame} = \frac{252,000,000 / 60}{N}$.
  - *Performance Tradeoff:* Simulating 252,000,000 cycles/sec across dual cores in software can exceed real-time budgets on host CPUs. At `--turbo 6`, virtual frame execution achieves real-time 60.0 FPS on standard desktop and laptop processors.
  - *Timing Invariance:* Internal keyboard matrix debouncing, hold countdowns, and gap delays are dynamically scaled by $N$ to guarantee that every keystroke spans across at least one full 60 Hz vertical blank keyboard scan interrupt (> 4,200,000 cycles), preserving reliable virtual-time latching inside Color BASIC regardless of host simulation speed.

---

## 3. Video Subsystem Specification

### 3.1 Display Geometry & Framebuffer Architecture

A literal 640×480 RGB565 framebuffer consumes $640 \times 480 \times 2 = 614,400\text{ bytes}$ (~600 KB), which exceeds the entire 520 KB SRAM capacity of the RP2350. To operate reliably within SRAM alongside emulation memory, audio buffers, and USB stacks, the CoCo Zero firmware deploys an **8-bit paletted scanout architecture** (`VIDEO_640`, PIZERO-134, PIZERO-175):

1. **Framebuffer Format & Resolution:**
   - Single framebuffer: **640 × 240 pixels at 1 byte per pixel** ($640 \times 240 = 153,600\text{ bytes}$ or ~150 KB).
   - Each byte is an index into a 256-entry RGB565 palette table (`g_pal565[256]`).
   - Line doubling: `libdvi` scanout repeats each logical scanline twice (`DVI_VERTICAL_REPEAT = 2`), producing a physical display of **640 × 480p @ 60.0 Hz**.
2. **Master Palette Allocation (`g_pal565[256]`):**
   - **Indices `0–63`:** GIME master color palette (64 native CoCo 3 RGB colors).
   - **Indices `64–65`:** OSD card and overlay ink and paper (`CARD_INK` = `0x07E0` factory green, `CARD_PAPER` = `0x0000` black, PIZERO-256).
   - **Indices `80–95`:** CoCo 2 VDG 16-color palette (reflecting software `$FFB0–$FFBF` palette writes and `color_*` settings).
3. **CoCo 3 Video Mode Rendering:**
   - Full 640 horizontal resolution: GIME scanlines are rendered 1:1 into `g_fb`.
   - Text modes: 32, 40, and 80 columns (`WIDTH 40`, `WIDTH 80`). In 80-column mode, characters are rendered with full single-pixel clarity (no horizontal squashing or column dropping).
   - Graphics modes: HSCREEN 1, 2, 3, and 4 rendered directly to `g_fb`.
   - Port-Side Renderer (`port_render_scanline`, PIZERO-218): GIME rasterizer hooks horizontal sync, decodes active VRAM directly, and evaluates a line hash (`port_sum`). Scanlines matching the previous frame are skipped (passed as NULL), dramatically reducing CPU overhead during static screens.
   - Dynamic Beam Tearing: As on a physical CoCo 3, scanlines are rendered as the virtual beam moves, authentically reproducing mid-frame palette splits and HCLS color bands (PIZERO-217).
4. **CoCo 2 Video Mode Rendering:**
   - Native MC6847 VDG resolution: $256 \times 192$ pixels, nibble-packed (2 pixels/byte = 24,576 bytes).
   - Horizontal pixel doubling: `coco_boot_blit_vdg_pizero8()` expands each VDG pixel to 2 output bytes in `g_fb` (using index $80 + \text{color}$).
   - Centering: Active raster is centered at offset $(X=64\text{ bytes}, Y=24\text{ lines})$, producing a visible active area of **512 × 384 pixels** inside the 640 × 480 field, surrounded by a static black border.
   - The border is painted once at initialization and never re-cleared, eliminating 27,648 redundant writes per frame.

```
       +----------------------- 640 px (Physical Output) --------------------+
       |                                                                      |
       |  Top Border: 48 physical lines (24 logical scanlines)                |
       |  +----------------- 512 px (CoCo 2 Active Display) -------------+     |
       |  |                                                             |     |
480 px |  |  CoCo 2: 384 physical lines (192 logical scanlines)         |     |
(240 L)|  |  CoCo 3: Full 640 px width across active scanlines          |     |
       |  |                                                             |     |
       |  +-------------------------------------------------------------+     |
       |  Bottom Border: 48 physical lines (24 logical scanlines)              |
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
| **Front Porch** | 8 | 10 | Standard HDMI front porch |
| **Sync Pulse** | 92 | 2 | Reduced from 96 px to donate 4 px to back porch |
| **Back Porch** | 60 | 33 | Expanded from 56 px to provide 14 px control period |
| **Total Period** | 800 pixels | 525 lines | Product refresh: **60.000 Hz** exact ($25.200\text{ MHz} / (800 \times 525)$) |
| **Scanout Clock** | 25.200 MHz | — | TMDS bit clock = 252.000 MHz |

### 3.3 TMDS Encoding & Core 1 Scanout Engine

- **Hardware TMDS Encoder (`tmds_encode_pal8_sio`):** Core 1 reads the 640×240 8-bit buffer scanline-by-scanline, looks up pixel pairs in `g_pal565`, and feeds them to the RP2350 SIO hardware TMDS encoder block. Encoding consumes only ~23.7 µs per 640-pixel line (compared to ~57 µs for software table encoders), leaving ample margin within the 63.5 µs scanline window.
- **Single-Buffered Handoff:** In all shipping builds (`AV_DATA_ISLAND`), Core 0 renders directly into the single buffer `g_fb`, and Core 1 scans out continuously. This frees ~150 KB of SRAM required for audio data islands.
- **Overlay Rendering (PIZERO-256):** Cards and overlay menus (F9–F12, settings editor, splash screens) are rendered using dedicated palette slots `g_pal565[64..65]` set to factory green on black (`CARD_INK` = `0x07E0`, `CARD_PAPER` = `0x0000`). This ensures menu readability regardless of custom guest palette modifications.

### 3.4 Color Palettes & Fonts

#### Palette Support
- **MC6847 VDG Palettes:** Standard green and buff/white text modes; Semigraphics-4, 6, 8, 12, 24; PMODE 0–4 graphics.
- **GIME Palette Registers (`$FFB0–$FFBF`):** Transparently supported on CoCo 2 when enabled (`gime_palette = on`). CoCo 3 supports full 16-color register mapping from the 64-color master palette.
- **Palette Customization:** Default VDG colors can be customized via `color_<NAME> = #RRGGBB` in `settings.txt`.
- **NTSC Artifact Colors (`artifact_colors = on | off | swapped`):** Emulates composite artifact color fringing in PMODE 4 high-resolution graphics. Toggled live via **F8** (or **Cmd+A**).

#### Character Fonts
- `classic`: Authentic CoCo 1/2 font (inverted uppercase for lowercase characters).
- `6847t1`: Later CoCo 2B font supporting true lowercase via `POKE 65314,16`.
- `6847t2` *(Default)*: Enhanced font featuring authentic caret (`^`) and underscore (`_`) glyphs, and permanent lowercase support (`lowercase = on`).

### 3.5 Host Emulator Display Pipeline & Dynamic Symbol Resolution

In `cocozero-rp2350`, the display pipeline accommodates both 320-wide and 640-wide modes:
- **Dynamic Symbol Extraction:** Symbol addresses (`_ZL4g_fb`, `_ZL8g_pal565`, `_ZL7g_c3bus`, `_ZL7g_gime3`) are extracted from companion `.elf` binaries or in-flash signatures at startup.
- **Wide-Mode Auto-Detection:** The emulator inspects GIME registers `VMODE` ($FF98) and `VRES` ($FF99) in SRAM. When 80-column text or 640-pixel graphics are active, `extract_frame_640()` performs direct 1:1 scanline decoding from virtual VRAM. In standard 320-wide modes, it doubles pixels horizontally.
- **Presentation:** Framebuffers are presented via SDL2 streaming textures on a centered 640×480 window at 60 Hz.
- **Headless Mode (`--headless`):** Allows automated continuous integration and headless frame dumping to 24-bit BMP images (`--dump-frame <N>`).

---

## 4. Audio Subsystem Specification

### 4.1 Dual Audio Output Architecture

The CoCo Zero provides **two mutually exclusive digital audio destinations** (PIZERO-255). Sound goes to one output, never both at once:

| Audio Output Destination | Active Transport | Selection Criteria | Notes |
|---|---|---|---|
| **Video Cable (HDMI / DVI)** | TMDS Data Islands | Default mode (`usb_audio = off` or no USB DAC detected at power-on). | Digital audio rides within HDMI blanking intervals to TV/monitor speakers. |
| **USB Headphone DAC** | Isochronous USB OUT | USB DAC attached at power-on and `usb_audio = on` (default). | 48 kHz 16-bit stereo PCM streamed to USB-C dongle or hub headphone jack. HDMI audio carries silence. |

### 4.2 HDMI Data Island Transport

Audio transmitted over the mini-HDMI port uses **TERC4-encoded Data Islands** embedded within TMDS blanking intervals:

```
Scanline Structure (Active Line Back Porch):
[ Preamble: 4w ][ Guard: 1w ][ Data Island: 16w ][ Guard: 1w ][ Video Pre: 4w ][ Guard: 1w ][ Active Video: 320w ]
|<----------------------- Back Porch: 30 words (60 px) ------------------------------------>|
|<-- Audio Island: 18 words (36 px) -->|<------------- Video Preamble & Guard: 5w --------->|
                                       |<-- Control Period: 7 words (14 px) -->|
```

1. **IEC 60958 Compliance (PIZERO-223):** Sample subframes strictly format consumer LPCM: validity bit $V=0$, a 48 kHz / 16-bit channel status table transmitted bit-by-bit per frame, and the block-start flag ($B=1$) asserted on the first subpacket of each 192-frame block.
2. **523-Line Continuous Metering (`AV_VBLANK_AUDIO`, PIZERO-232):** Audio packets are metered across **all 523 non-sync lines** (480 active scanlines + 43 non-sync vertical blanking lines), eliminating the ~1.4 ms inter-frame silence gap of active-line-only delivery.
3. **Full 4-Sample Packets (`AV_AUDIO_FULL_PACKETS`):** The sample meter dispatches strictly **full 4-sample packets** once every ~2.4–2.6 lines, mirroring standard commercial HDMI transmitters.
4. **Vblank Active Block Delivery:** Vblank lines carry an audio island within their active block (never in the back porch, which disrupts sync on some monitors, PIZERO-34), followed by a 2-pixel trailing guard band (`AV_HDMI_STRICT_FRAMING`).
5. **Clock Regeneration (ACR):** Transmitted on every 8th vblank line (`AV_VBLANK_INFO_EVERY = 8`), generating ~840 ACR packets/sec at $CTS = 25200$, $N = 6144$ for 25.200 MHz pixel clock.
6. **Sample Rate Metering:** Accumulator (`g_meter_acc`) steps in exact 480ths of a sample (`step = AUDIO_SAMPLES_PER_FRAME = 800`), delivering **exactly 48,000 samples per second** with zero truncation drift.

### 4.3 Proportional Closed-Loop Audio Servo ([`audio_servo.h`](file:///Users/richardlucente/development/git/xroar-waveshare-rp2350-pizero/lib/coco_machine/src/audio_servo.h))

To eliminate clock drift between emulated 6809 cycles ($14.31818\text{ MHz} / 16$) and physical TMDS / USB clocks:
- Sampled once per frame across the 8,192-sample SPSC ring buffer.
- Target fill level: $\frac{\text{Capacity}}{2} = 4,096\text{ samples}$.
- Rate adjustment: $adj = -(fill - target) / 32$, clamped to $\pm 64\text{ samples/sec}$ (max 0.13% adjustment, $<0.025$ semitones, completely inaudible).
- Successfully maintains locked synchronization over multi-day soak tests without buffer skips or underruns.

### 4.4 Complete Audio Source Portfolio & Conditioning

The CoCo Zero emulates the full spectrum of Color Computer audio hardware:

| Audio Source | Hardware Address | Voices & Capabilities | Setting | `cocozero` | `coco2` | `coco3` |
|---|---|---|---|---|---|---|
| **6-Bit DAC** | PIA1 Port A (`$FF20` bits 2–7) | 1 voice; resistor ladder DAC; integrate-and-dump resampler | *Always On* | On | On | On |
| **Single-Bit Sound** | PIA1 Port B (`$FF22` bit 1) | 1 voice; cassette sound & direct CPU toggle | *Always On* | On | On | On |
| **SN76489 CSG** | `$FF41` (GMC cartridge) | 3 square wave tones + 1 noise channel; 4 MHz clock, 2 dB steps | `sn76489` | On | Off* | Off* |
| **AY-3-8910 (CoCo PSG)** | `$FF5D–$FF5F` | 3 tones + 1 noise generator + hardware envelope; 1 or 2 MHz clock | `coco_psg` | On | Off | Off |
| **4× AY-3-8910 (Symphony-12)**| PIA at `$FF60–$FF63` | 12 voices + 4 noise generators; 894,886 Hz clock; optional stereo | `symphony12` | On | Off | Off |
| **AY-3-8913 (Deluxe CoCo)** | `$FF38–$FF39` | 3 tones + 1 noise generator; 1.79 MHz clock | `deluxe_psg` | Off | Off | Off |
| **Orchestra-90CC** | `$FF7A` (L), `$FF7B` (R) | Dual 8-bit DACs; true stereo output; dual audio integrator | `orchestra90` | On | Off | Off |

*\*Note: Loading a Games Master Cartridge image (.ccc) automatically enables the SN76489 on any machine.*

#### CoCo Sound Multiplexer Emulation (`sound_sources = all | switched`)

On physical hardware, audio sources pass through an analog multiplexer chip:
- **Select Lines:** PIA0 CA2 (`$FF01` bit 3) and CB2 (`$FF03` bit 3) select the audio source (and simultaneously select the analog joystick comparator axis):
  - `CB2=0, CA2=0`: DAC (and Right X joystick axis).
  - `CB2=0, CA2=1`: Cassette input (and Right Y joystick axis).
  - `CB2=1, CA2=0`: Cartridge sound line (and Left X joystick axis).
  - `CB2=1, CA2=1`: No source (and Left Y joystick axis).
- **Sound Enable:** PIA1 CB2 (`$FF23` bit 3). When low, multiplexer output is disconnected.
- **Single-Bit Output:** PIA1 Port B bit 1 (`$FF22`). When configured as output, modulates overall line level.
- **Hardware Millivolt Levels:** Authentic output levels measured from Dragon/CoCo silicon:
  - *PB1 an input:* $\text{DAC} = 4500 \times (\text{DAC} / 63) + 200\text{ mV}$; $\text{Cassette} = 2050\text{ mV}$; $\text{Cartridge} = \text{Sound} \times 1.0$.
  - *PB1 low:* $\text{DAC} = 2840 \times (\text{DAC} / 63) + 180\text{ mV}$; $\text{Cassette} = 1600\text{ mV}$; $\text{Cartridge} = \text{Sound} \times 0.60 + 180\text{ mV}$.
  - *PB1 high:* $\text{DAC} = 3400 \times (\text{DAC} / 63) + 1300\text{ mV}$; $\text{Cassette} = 2350\text{ mV}$; $\text{Cartridge} = \text{Sound} \times 0.72 + 1300\text{ mV}$.
- **Operating Modes:**
  - `sound_sources = switched` *(Stock `coco2` and `coco3`)*: Authentic multiplexer switching. Cartridge chips are only heard when selected and enabled.
  - `sound_sources = all` *(Default `cocozero`)*: All sources (DAC, single-bit, SN76489, AY bank, Orchestra-90) mix simultaneously regardless of multiplexer selection and enable flags.

#### Joystick Probe Cancellation (`joystick_quiet = on | off`)
Reading an analog joystick in software involves writing trial values to the 6-bit DAC and reading the voltage comparator on PIA0 PA7. In `sound_sources = all` mode, these successive-approximation writes produce an audible buzzing sound (e.g. in *Double Back*).
- When `joystick_quiet = on` (default on `cocozero`), DAC writes immediately followed within 16 CPU cycles by a read of PIA0 Port A are identified as joystick SAR trials.
- The trial write is rolled back from the audio output stream, holding audio back by 1 sample (~21 µs) and rendering joystick reading completely silent while preserving comparator evaluation.

#### AY-3-8910 Bank & Symphony-12 Stereo
- Shared Chip 1: CoCo PSG, Symphony-12 chip 1, and Deluxe CoCo share the first physical AY instance.
- **Stereo Jumper (`symphony12_stereo = on`):** Chips 1 and 2 route to the Left channel; chips 3 and 4 route to the Right channel. When disabled, all four chips mix to center.
- **Selective Rendering Optimization (PIZERO-248):** AY chips step only active channels with non-zero volume (`ay_advance`, `ay_sync`). Silent or unmuted channels are caught up on register writes, reducing execution time from ~5.5 ms to ~1.6 ms per frame.

#### Signal Conditioning Pipeline
Samples pass through a three-stage conditioning pipeline before entering the 8,192-sample SPSC ring:
1. **Dynamic AY Mixing:** Multiple active AY chips on one channel are mixed using $\text{Sample} = \text{Sum} \times 2 / (n + 1)$, preventing multi-voice clipping.
2. **AC-Coupling DC Blocker (`audio_dc_run`):** Single-pole high-pass filter ($f_c \approx 5\text{ Hz}$) eliminating DC bias from resting DAC levels.
3. **Two-Pole Butterworth Low-Pass Filter (`audio_lp_run`):** Simulates CRT television acoustic rolloff (default 8,000 Hz, configurable via `audio_lowpass`).
4. **Gain Policy & Volume Control:** 6× amplification boost (+15.5 dB). Master volume scaled by `volume = 0..15` (default 10).

### 4.5 USB Audio Headphone DAC Subsystem (PIZERO-255)

The firmware includes a native USB Audio Class host driver ([`src/usb_audio.cpp`](file:///Users/richardlucente/development/git/xroar-waveshare-rp2350-pizero/src/usb_audio.cpp)):
- **Transport:** Isochronous OUT endpoint streaming added to vendored Pico-PIO-USB (`pio_usb_host_iso_out_stream`). Streams 48 stereo sample pairs (192 bytes) every 1 ms USB frame at 12 Mbps Full Speed.
- **Class Compliance:** Supports both **UAC1** (adaptive endpoints) and **UAC2** (synchronous endpoints) devices exposing 16-bit stereo PCM @ 48 kHz.
- **DAC Volume Initialization (`usb_audio_db`):** At device mount, the driver queries the Audio Feature Unit, sets DAC hardware attenuation to `usb_audio_db` (default -10 dB, range -60 to 0 dB), and un-mutes the device.
- **Keyboard Volume Keys ([`src/hid_consumer.h`](file:///Users/richardlucente/development/git/xroar-waveshare-rp2350-pizero/src/hid_consumer.h)):** Parses HID Consumer page reports (usage `0x0C`). Volume Up (`0xE9`), Volume Down (`0xEA`), and Mute (`0xE2`) dynamically adjust `volume` (0–15). Changes are automatically committed to `/coco/settings.txt` after 2 seconds of inactivity.

### 4.6 Host Emulator Audio Subsystem

In `cocozero-rp2350`:
- Digital audio is drained from `g_audio_ring` via `CoCoZeroSoC::extract_audio()`.
- Drains 48 kHz 16-bit samples, duplicates to stereo `[L, R]`, applies host DC blocking, and updates `g_audio_r` in firmware SRAM.
- Feeds SDL2 `AudioQueue<i16>` with a 100 ms maximum queue budget (`AUDIO_QUEUE_MAX_BYTES = 19,200 bytes`) to prevent latency runaway under `--turbo` acceleration.

---

## 5. Input & Peripheral Subsystem Specification

### 5.1 USB Host Subsystem (Pico-PIO-USB + Adafruit TinyUSB)

- **Controller:** Pico-PIO-USB operating on **PIO 1** (GPIO 28 `D+`, GPIO 29 `D-`), bit-banging USB 1.1 Full Speed (12 Mbps).
- **Host Stack:** Adafruit TinyUSB Host configured for external hubs (`CFG_TUH_HUB = 2`) and 8 concurrent HID interfaces (`CFG_TUH_HID = 8`).
- **Enumeration Buffer:** TinyUSB enumeration buffer expanded to 512 bytes (`CFG_TUH_ENUMERATION_BUFSIZE`) to reliably capture large composite USB audio and gamepad descriptors.
- **Hotplug Limitation (Errata E9):** Direct connection of devices requires attachment before power-on due to missing VBUS power-switching MOSFETs and RP2350 pull-down leakage. Devices connected through external USB hubs support live hot-plugging.

### 5.2 USB Keyboard & Matrix Translation

The CoCo keyboard is an $8 \times 7$ passive matrix scanned via PIA0 Port A (columns) and Port B (rows). The firmware translates USB HID keycodes to authentic matrix chords:

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

- **Symbol Chording (`kt_chord`):** Punctuation and symbols (`"`, `!`, `?`, `*`, `+`, etc.) automatically synthesize required Shift chords (e.g. `"` pulses `Shift + 2`).
- **CoCo 3 Native Keys (PIZERO-215):** On CoCo 3 (`machine = coco3`), USB `Ctrl` maps to native `CTRL` (`K_CTRL = 0x34`), USB `Alt` maps to native `ALT` (`K_ALT = 0x33`), and USB `F1` / `F2` map to native `F1` (`0x35`) and `F2` (`0x36`) on matrix row 6.
- **Relocated INFO Overlay:** Because F1 is a native CoCo 3 key, the `< INFO >` diagnostic overlay is relocated to the overlay carousel: accessible by pressing **F12** (Disks) followed by **Left Arrow**.
- **Case Toggles:**
  - *Color BASIC:* Caps Lock generates `Shift + 0`.
  - *NitrOS-9 / OS-9:* Console driver toggles case on `Ctrl + 0` (Home + 0 on USB keyboard).
- **Auto-Repeat (`key_repeat`):** Default `on` for `cocozero` profile; default `off` for stock `coco2` and `coco3`. Clamped to 12 repeats/sec.

### 5.3 Host Emulator Dual-Tier Input Subsystem ([`src/frontend.rs`](file:///Users/richardlucente/development/git/cocozero-emulator/src/frontend.rs) & [`src/keyboard.rs`](file:///Users/richardlucente/development/git/cocozero-emulator/src/keyboard.rs))

The host emulator satisfies both guest 6809 software and firmware OSD overlays:

```
Host SDL2 Window / Non-blocking Terminal Stdin
                     |
         +-----------+-----------+
         |                       |
[Normal Typing Mode]    [Function Keys & Menu Active]
         |                       |
         v                       v
Tier 1: Matrix Engine   Tier 2: TinyUSB HID Callback
  * SRAM Matrix Address   * Report at Scratch SRAM
    (0x2000B304/0x2000F200) * Trampoline at 0x2007FFE0
  * kt_chord() Table      * HID Callback Invocation
  * Virtual Hold (T + 1)  * Register Save / Restore
  * Virtual Gap (2T/3)    * Direct Menu Navigation
  * Boot Gated by Cycles  |
    (CoCo 2: 150f, CoCo 3: 210f)
         |                       |
         v                       v
Guest 6809 Color BASIC   Firmware OSD Overlays (F9-F12)
```

#### Tier 1: Direct Matrix Injection (`0x2000_B304` / `0x2000_F200`)
- Directly writes active-low bitmasks to `g_kb_col_row_mask` in SRAM.
- Enforces debouncing invariance across turbo speeds ($T+1$ hold frames, $\max(1, \lfloor 2T/3 \rfloor)$ gap frames).
- Gates keystroke delivery during boot memory sizing (~150 frames on CoCo 2; ~210 frames on CoCo 3).

#### Tier 2: TinyUSB HID Callback Injection (`inject_hid_keycode`)
- Synthesizes 8-byte HID reports and executes `tuh_hid_report_received_cb` on Core 0 via a scratch trampoline at `0x2007_FFE0`, preserving all registers (`R0–R14`, `xPSR`, `MSP`).
- Intercepts navigation keys (`Up`, `Down`, `Enter`, `Esc`, `Tab`, `W`, `N`, `R`, `0`–`3`) when an overlay is active (`MENU_ACTIVE_ADDR != 0`).

#### Function Keys & Cross-Platform Shortcuts
| Target Function | Host Key (PC) | Host Key (macOS) | HID Code | Description |
|---|---|---|---|---|
| **Disks Menu** | **F12** | **Cmd+D** / **Ctrl+D** | `0x45` | OSD Floppy Disk Mount (`< DISKS >`) |
| **Programs Menu** | **F9** | **Cmd+P** / **Ctrl+P** | `0x42` | OSD Binary Launcher (`< PROGRAMS >`) |
| **Cartridges Menu** | **F10** | **Cmd+C** / **Ctrl+C** | `0x43` | OSD Cartridge Selector (`< CARTRIDGES >`) |
| **Files Menu** | **F11** | *N/A* | `0x44` | OSD Text Editor (`< FILES >`) |
| **Info / Telemetry** | Overlay Nav | **F12 then Left** | `0x3A` | OSD System Diagnostics (`< INFO >`) |
| **Artifact Toggle** | **F8** | **Cmd+A** / **Ctrl+A** | `0x41` | Cycle PMODE 4 NTSC Artifact Colors |
| **Screenshot** | **PrintScreen** | *N/A* | `0x46` | Capture display screenshot to `/coco/shots/` |

### 5.4 USB Gamepad Subsystem

- **Supported Protocols:** Sony DualShock 4, GameSir XInput, GameSir Android HID.
- **Joystick Axis Mapping:** Left stick (and D-pad) drives **Right Joystick** (`JOYSTK(0)/(1)`); Right stick drives **Left Joystick** (`JOYSTK(2)/(3)`). Swapped via `joystick_swap = on`.
- **D-Pad Modes:** `dpad = joystick` (snapped to axes) or `dpad = arrows` (emulates 6809 arrow keys).
- **Buttons & CoCo 3 Second Fire (PIZERO-215):**
  - Face buttons: Bottom/Right = `fire` (Right stick fire); Left = `fire_right` (Left stick fire); Top = `space`.
  - Shoulder buttons: R1 = `fire`; L1 = `fire_right`.
  - Triggers (CoCo 3): R2 = `fire2` (Right joystick second fire); L2 = `fire2_right` (Left joystick second fire). Triggers evaluate analog byte thresholds ($\ge 128$) to eliminate phantom presses from noisy digital trigger bits.
  - Home: Toggles OSD menu overlay.
  - M button: Captures screenshot.
- **Overlay Navigation on Gamepad:** D-pad moves; A selects/mounts; B closes; X opens settings editor on highlighted game; L1/R1 page.

### 5.5 Multi-Source Keyboard Matrix Layering

To prevent key drops between concurrent physical and virtual input sources (PIZERO-163), the firmware maintains three independent $8 \times 8$ active-low matrix layers (`COCO_KEY_SOURCES = 3`):
- Layer 0: Physical USB HID Keyboard.
- Layer 1: USB CDC Serial / Script Autotyper.
- Layer 2: Gamepad Button-to-Key mappings.

The effective matrix presented to PIA0 is the continuous bitwise AND:
$$\text{g\_kb\_col\_row\_mask}[\text{col}] = \bigwedge_{s=0}^{2} \text{g\_kb\_src}[s][\text{col}]$$

---

## 6. Storage & Filesystem Subsystem Specification

### 6.1 MicroSD Interface & FAT32 Hierarchy

The on-board microSD slot connects via SPI1 at 12.5 MHz using FatFs:

```text
/coco/
├── roms/           # System ROMs (user supplied)
│   ├── bas12.rom       # Color BASIC 1.2 (8192 bytes, required for CoCo 2)
│   ├── extbas11.rom    # Extended Color BASIC 1.1 (8192 bytes, optional)
│   ├── disk11.rom      # Disk BASIC 1.1 (8192 bytes, optional)
│   └── coco3.rom       # Super Extended Color BASIC 2.1 (32768 bytes, required for CoCo 3)
├── dsk/            # Floppy disk images (.dsk) -> mount via F12
├── bin/            # Machine language executables (.bin) -> launch via F9
├── cart/           # ROM Cartridges (.ccc) -> mount via F10
├── shots/          # Destination folder for captured screenshots (.png)
├── log/            # Telemetry soak logs (soak-001.txt - soak-NNN.txt)
├── settings.txt    # System configuration settings
├── autorun.txt     # Optional boot script (auto-executed at startup)
└── drives.txt      # Persisted floppy drive mount states
```

#### System ROM Resolution & Error Splashes
- `bas12.rom` Missing / Bad Size: System displays `MSG_NOROM` or `MSG_BADROM` splash screen.
- `extbas11.rom` Missing: Displays `MSG_CBONLY` ("COLOR BASIC ONLY") for 4.0 s, then boots into Color BASIC 1.2 with 16 KB RAM.
- `coco3.rom` Missing when switching to CoCo 3: System displays an error notice and remains in CoCo 2 mode.

### 6.2 MicroSD SPI Controller & Virtual FAT32 Storage Engine ([`src/spi_sd.rs`](file:///Users/richardlucente/development/git/cocozero-emulator/src/spi_sd.rs) & [`src/fat32.rs`](file:///Users/richardlucente/development/git/cocozero-emulator/src/fat32.rs))

The host emulator provides two storage modes:
1. **Dynamic Virtual FAT32 Builder ([`src/fat32.rs`](file:///Users/richardlucente/development/git/cocozero-emulator/src/fat32.rs)):** When launched pointing to a directory (`--sd ./coco`), the emulator dynamically synthesizes a complete FAT32 filesystem in memory (`MemoryStorage`). It generates the Master Boot Record (MBR), FAT32 Volume Boot Record (VBR / BPB), FSInfo sector, two 32-bit FAT tables, the root directory cluster, subdirectories (`roms/`, `dsk/`, `cart/`, `bin/`, `shots/`, `log/`), contiguous cluster chains, and authentic 8.3 directory entries on the fly.
2. **File-Backed Storage (`FileStorage`):** When launched pointing to a raw disk image file (`--sd sdcard.img`), sectors are read and written directly to the host image file.

#### SPI1 SD Card Controller Protocol State Machine
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
  - *Contract:* The SD controller state machine strictly inhibits command parsing whenever the card is in a data-receiving state (`WritingSingleBlock` or `WritingMultiBlock`). Command parsing is only re-enabled after block commit and busy-signaling completion.
  - *Tradeoff Impact:* Without this disambiguation, FatFs directory rescans during OSD disk mounting mistake directory sector bytes for new commands, desynchronizing the SPI bus and causing disk cataloging to abort with false "NO DISK IMAGES FOUND" errors.

### 6.3 Floppy Disk Controller (WD2797 / WD1793) & Writable Disks

- **Four Virtual Drives:** Drives 0, 1, 2, and 3 mapped via `$FF40` drive latch and `$FF48–$FF4B` FDC registers.
- **Synchronous Sector Writes (PIZERO-66):** `SAVE`, `SAVEM`, `KILL`, and OS-9 writes commit directly to the `.dsk` file on microSD in $\le 2.1\text{ ms}$ per sector.
- **Spindle Motor Shutoff Commit:** Bit 3 of `$FF40` controls drive motor power. A high-to-low transition (`1 -> 0`) triggers an immediate cache flush and FAT commit (`coco_disk_idle_fn`).
- **Interactive Drive Management in `< DISKS >` (F12):**
  - **W:** Toggle write-protection lock on the highlighted drive.
  - **N:** Format a new blank 35-track single-sided `.dsk` image with initialized RS-DOS Track 17 directory and FAT.
  - **R:** Rename the highlighted disk image on the microSD card.
- **OS-9 Operating System Support:**
  - NitrOS-9 Level 1 boots on CoCo 2 via Disk BASIC `DOS` command.
  - NitrOS-9 Level 2 boots on CoCo 3 to an 80-column shell (PIZERO-236).

### 6.4 Cartridge & Binary Loading Engine

- Direct binary loading (`.bin`) via `LOADM` parser (`coco_boot_parse_loadm`).
- ROM Cartridges (`.ccc`): 2 KB to 16 KB simple mirrored ROMs; 32 KB to 256 KB bank-switched Games Master Cartridges (even addresses in `$FF40–$FF5F` switch banks; odd addresses route to the SN76489 sound chip).

---

## 7. CoCo Machine Emulation Core

### 7.1 System Memory Maps & Timing Cadence

#### CoCo 2 Standard Map ($TY=0$)
```text
$0000 - $7FFF: 32 KB Lower Dynamic RAM
$8000 - $9FFF: Extended Color BASIC ROM (8 KB)
$A000 - $BFFF: Standard Color BASIC ROM (8 KB)
$C000 - $DFFF: Cartridge ROM / Disk BASIC (8 KB)
$E000 - $FEFF: ROM / Cartridge Upper Mirror (8 KB)
$FF00 - $FF1F: MC6821 PIA0 (Keyboard, Joystick, Comparator, Mux Select)
$FF20 - $FF3F: MC6821 PIA1 (6-bit DAC, Single-bit Sound, VDG Control, Mux Enable)
$FF40 - $FF5F: WD2797 FDC / Cartridge Bank Switch / SN76489 CSG
$FF60 - $FF63: Symphony-12 4x AY-3-8910 PIA (CoCo Zero)
$FF7A - $FF7B: Orchestra-90CC Stereo DACs (CoCo Zero)
$FF90 - $FF95: GIME Timer & Interrupt Registers (CoCo Zero / CoCo 3)
$FFB0 - $FFBF: GIME Palette Registers (CoCo Zero / CoCo 3)
$FFC0 - $FFDF: MC6883 SAM Control Registers
$FFE0 - $FFFF: 6809 Hardware Interrupt / Reset Vectors
```

#### CoCo 2 All-RAM Map ($TY=1$)
- When the SAM $TY$ register is set (`POKE 65499,0` or write to `$FFDF`), the entire 64 KB address space from `$0000` through `$FEFF` is mapped to contiguous dynamic RAM.
- Only the upper 256 bytes (`$FF00–$FFFF`) remain reserved for I/O registers and hardware interrupt vectors.
- This mode is utilized by advanced operating systems like NitrOS-9 Level 1 and OS-9 to provide an unfragmented 64 KB user address space.

#### CoCo 3 GIME Paged MMU Map
- The CoCo 3 firmware models 128 KB of physical dynamic RAM partitioned into sixteen 8 KB blocks (`$00` through `$0F`).
- The 64 KB logical MPU address space is divided into eight 8 KB windows (`$0000–$1FFF`, `$2000–$3FFF`, ..., `$E000–$FFFF`), mapped dynamically through Task 0 (`$FFA0–$FFA7`) and Task 1 (`$FFA8–$FFAF`) MMU registers.
- Fastbus paged helper (`COCO_FASTBUS_PAGES`, PIZERO-213) executes memory mapping lookups in ~4 Cortex-M33 cycles.

#### SRAM Backing Contract
- 64 KB of virtual CoCo 2 RAM (or 128 KB for CoCo 3) is allocated as a static contiguous buffer in the RP2350's internal 520 KB SRAM via `psram_stub.c` (`rp_mem_malloc`).
- The buffer is zero-initialized at boot, mirroring the authentic cold-boot state of physical dynamic RAM.

#### Master Clock & Frame Cadence
- Physical Color Computer hardware derives all timings from a **14.3181818 MHz** master color burst crystal.
- The MC6883 SAM divides this master frequency by 16 to synthesize the standard MPU clock:
  $$f_{MPU} = \frac{14.3181818\text{ MHz}}{16} \approx \mathbf{0.894886\text{ MHz}} \quad (1117.5\text{ ns per cycle})$$
- At a 60.0 Hz video display refresh rate, the frame period is exactly $16,666.7\ \mu\text{s}$. The emulated CPU executes:
  $$\text{CYCLES\_PER\_FRAME} = \frac{894,886\text{ Hz}}{60.0\text{ Hz}} \approx \mathbf{14,915\text{ cycles}}$$
- **Fixed-Time Accounting:** The virtual machine enforces a fixed emulated time budget per frame:
  $$\text{BUDGET} = 14,915 \times 16\text{ ticks/cycle} = \mathbf{238,640\text{ event ticks/frame}}$$
- At 1.79 MHz double speed ($R=2$), the CPU executes 29,830 cycles per frame at 8 event ticks per cycle, charging exactly the identical $238,640$ event ticks per frame. This mathematical invariance guarantees that audio sample generation and vertical blank timing remain perfectly locked to 60.0 Hz regardless of CPU speed.

---

### 7.2 Motorola MC6809E Microprocessor Model

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

#### Fastbus Inlined Memory Architecture ([`coco_fastbus.h`](file:///Users/richardlucente/development/git/xroar-waveshare-rp2350-pizero/lib/xroar_core/include/coco_fastbus.h))
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

#### Addressing Modes & Indexed Postbyte Decoding
- **Direct Page Addressing:** Formed by `(REG_DP << 8) | offset`, followed by 1 NVMA cycle.
- **Extended Addressing:** Formed by 16-bit address operand, followed by 1 NVMA cycle.
- **Indexed Addressing Modes:** Evaluated via postbyte:
  - 5-bit signed offset (`postbyte & 0x80 == 0`): $EA = R + \text{sex5}(\text{offset})$, consumes 1 NVMA cycle ($R \in \{X, Y, U, S\}$).
  - Postbyte Bit 7 = 1 decoding matrix:

| Postbyte `[4:0]` | Addressing Mode | Direct / Non-Indirect | Indirect `[EA]` |
|---|---|---|---|
| `0000` | Auto-increment by 1 | `,R+` (+2 cycles) | *Illegal* |
| `0001` | Auto-increment by 2 | `,R++` (+3 cycles) | `[,R++]` (+6 cycles) |
| `0010` | Auto-decrement by 1 | `,-R` (+2 cycles) | *Illegal* |
| `0011` | Auto-decrement by 2 | `,--R` (+3 cycles) | `[,--R]` (+6 cycles) |
| `0100` | Zero offset | `,R` (+0 cycles) | `[,R]` (+3 cycles) |
| `0101` | Accumulator B offset | `B,R` (+1 cycle) | `[B,R]` (+4 cycles) |
| `0110` | Accumulator A offset | `A,R` (+1 cycle) | `[A,R]` (+4 cycles) |
| `1000` | 8-bit constant offset | `n,R` (+1 cycle) | `[n,R]` (+4 cycles) |
| `1001` | 16-bit constant offset | `nn,R` (+4 cycles) | `[nn,R]` (+7 cycles) |
| `1011` | Accumulator D offset | `D,R` (+4 cycles) | `[D,R]` (+7 cycles) |
| `1100` | 8-bit PC relative | `n,PCR` (+1 cycle) | `[n,PCR]` (+4 cycles) |
| `1101` | 16-bit PC relative | `nn,PCR` (+5 cycles) | `[nn,PCR]` (+8 cycles) |
| `1111` | Extended indirect | *N/A* | `[nnnn]` (+4 cycles) |

#### Undocumented Silicon Quirks
- **Illegal Postbyte `$07`:** Decoded as accumulator A offset (mirroring `$06`), as documented by David Flamand [dfffffff].
- **Illegal Postbyte `$0E`:** Forces effective address to `$FFFF` and charges 4 dummy NVMA cycles.
- **Stack Register Modification:** Auto-decrement on hardware stack `S` arms the NMI latch (`cpu->nmi_armed |= (REG_S != reg)`), matching physical silicon behavior where writing `S` enables NMI detection.
- **Undocumented Arithmetic:** Opcode `op_xdec` (illegal `DEC`) alters the Carry flag based on borrowing from zero; opcode `op_xclr` (illegal `CLR`) leaves the Carry flag untouched (documented by David Banks [hoglet67]).
- **Register-to-Register Transfer (`TFR` / `EXG`):** Transfers between mismatched 8-bit and 16-bit registers follow Tim Lindner's hardware analysis (e.g. transferring an 8-bit accumulator into a 16-bit pointer sign-extends or pads with `$FF`).

---

### 7.3 Interrupt Controller, Condition Codes, and Stacking Contracts

#### Interrupt Priority & Hierarchy
The MC6809E enforces a strict priority hierarchy among its hardware interrupts and software exceptions:
$$\text{RESET} > \text{NMI} > \text{SWI} > \text{FIRQ} > \text{IRQ} > \text{SWI2} > \text{SWI3}$$

#### NMI Arming Contract & Hardware Interlock
- **Silicon Behavior:** On cold power-on or hardware reset, the MC6809E's Non-Maskable Interrupt (`NMI`) line is strictly **disarmed** (`cpu->nmi_armed = 0`). NMI is only armed after the hardware stack pointer `S` is initialized (e.g. via `LDS #$0F00`), or after an interrupt stack push occurs.
- **Architectural Purpose:** This hardware interlock prevents spurious glitches on the cartridge or expansion port from causing an unrecoverable crash before the operating system has established a valid stack frame in RAM.

#### Condition Code Register (`CC`) Layout & Overflow Arithmetic
```
Bit:   7     6     5     4     3     2     1     0
     +-----+-----+-----+-----+-----+-----+-----+-----+
     |  E  |  F  |  H  |  I  |  N  |  Z  |  V  |  C  |
     +-----+-----+-----+-----+-----+-----+-----+-----+
```
- **Bit 7 ($E$ - Entire):** Flag indicating whether a full 12-byte stack frame ($E=1$) or fast 3-byte frame ($E=0$) was saved.
- **Bit 6 ($F$ - FIRQ Mask):** When set, fast interrupts are masked.
- **Bit 5 ($H$ - Half-Carry):** Set if carry occurred from bit 3 to bit 4 during 8-bit addition (`ADD`, `ADC`).
- **Bit 4 ($I$ - IRQ Mask):** When set, standard interrupts are masked.
- **Bit 3 ($N$ - Negative):** Set if sign bit (bit 7 for 8-bit, bit 15 for 16-bit) of result is set.
- **Bit 2 ($Z$ - Zero):** Set if result equals zero.
- **Bit 1 ($V$ - Two's-Complement Overflow):** Set if signed arithmetic resulted in overflow:
  - *8-bit Subtraction ($R = A - B$):* $V = (A_7 \oplus B_7) \cdot (A_7 \oplus R_7)$
  - *8-bit Addition ($R = A + B$):* $V = \overline{A_7 \oplus B_7} \cdot (A_7 \oplus R_7)$
  - *16-bit Operations ($ADDD, SUBD$):* Evaluated across bit 15:
    $$V_{SUB16} = (A_{15} \oplus B_{15}) \cdot (A_{15} \oplus R_{15})$$
    $$V_{ADD16} = \overline{A_{15} \oplus B_{15}} \cdot (A_{15} \oplus R_{15})$$
- **Bit 0 ($C$ - Carry / Borrow):** Set if an unsigned carry or borrow occurred.

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

### 7.4 MC6883 SAM & Hardware Double-Speed POKEs

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
  - At full 2× speed (`POKE 65497,0`), the VDG is starved of RAM access cycles, causing the video display on physical hardware to break down into severe visual noise ("screen hash" or "snow").
- **CoCo Zero Emulation Model & Justification:**
  - The firmware models rate switching as **flat, discrete quanta**: slow cycles charge exactly 16 event ticks, and fast cycles charge exactly 8 event ticks (`coco_fastbus.ticks` and `rom_ticks`).
  - *Empirical Timing Validation:* Over instruction execution sequences, physical 15/17/25-tick transitional variations average out to within $<1.8\%$ of the flat 8/16-tick model.
  - *Performance Trade-Off:* The flat model allows branchless, branch-predicted execution in `coco_fastbus`, avoiding expensive transitional phase state machines that would blow the 16.67 ms real-time frame budget on the Cortex-M33.
  - *Clean Scanout Advantage:* In CoCo Zero, display scanout is executed by Core 1 directly from the paletted framebuffer (`g_fb`). As a result, the display remains **100% clean, crisp, and readable** at full 2× speed, eliminating the screen hash of physical hardware.
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

#### PIA0 Operations (`$FF00–$FF1F`)
- **Port A (`$FF00`):**
  - Bits 0–6: Keyboard matrix column inputs (active low when key closed).
  - Bit 7: Analog voltage comparator input for joystick ADC.
  - Bit 0: Right Joystick Fire 1 button (active low).
  - Bit 1: Left Joystick Fire 1 button (active low).
  - Bit 2 (CoCo 3): Right Joystick Fire 2 button (active low).
  - Bit 3 (CoCo 3): Left Joystick Fire 2 button (active low).
- **Control Register A (`$FF01`):**
  - Bit 3 (CA2): Audio multiplexer select line / Joystick Y axis select.
  - CA1: VDG Horizontal Sync (`HS`, 15.75 kHz interrupt line).
- **Port B (`$FF02`):**
  - Drives 8 keyboard matrix rows (`PB0–PB7`, active low outputs).
- **Control Register B (`$FF03`):**
  - Bit 3 (CB2): Audio multiplexer select line / Joystick X axis select.
  - CB1: VDG Frame Sync (`FS`, ~60.0 Hz vertical blank interrupt line). Color BASIC hooks this vector to increment the real-time clock at `$0112–$0114` and poll keyboard status.

#### PIA1 Operations (`$FF20–$FF3F`)
- **Port A (`$FF20`):**
  - Bits 2–7: 6-bit R-2R ladder digital-to-analog converter (DAC). Intercepted by `audio_integ.h` to synthesize digital audio.
  - Bits 0–1: Cassette motor remote control / audio input biasing.
- **Control Register A (`$FF21`):**
  - CA1: Cassette data audio input zero-crossing detector.
- **Port B (`$FF22`):**
  - Bit 1: Single-bit sound output / cassette data audio write.
  - Bit 2: RS-232 serial output bit.
  - Bits 3–7: MC6847 VDG hardware control lines ($GM_0, GM_1, GM_2, A/G, CSS$), selecting alphanumeric vs graphic modes and color palettes.
- **Control Register B (`$FF23`):**
  - Bit 3 (CB2): Sound Enable (1 = sound multiplexer enabled, 0 = muted).
  - CB1: Cartridge Interrupt Request (FIRQ line).

#### Joystick Successive-Approximation ADC Conversion
Reading an analog joystick axis on a CoCo requires software successive approximation (SAR):
1. Software configures the multiplexer lines (PIA0 CA2 and CB2) to select the desired axis (Right X, Right Y, Left X, Left Y).
2. The 6809 outputs trial DAC values to PIA1 Port A (`$FF20` bits 2–7).
3. The DAC output is compared against the joystick potentiometer capacitor voltage via an analog comparator.
4. The 6809 reads the comparator result on PIA0 Port A bit 7 (`$FF00`).
5. *SAR Buzz Cancellation (`joystick_quiet = on`):* When enabled, DAC trial writes that are immediately followed within 16 CPU cycles by a read of PIA0 Port A are identified as SAR trial probes. The audio resampler rolls back the trial write, keeping audio output completely silent while preserving authentic comparator evaluation.

---

### 7.6 CoCo 3 Hardware Emulation Architecture

The unified firmware models a full 128 KB Tandy Color Computer 3 architecture:
- **System ROM:** Requires the 32 KB CoCo 3 ROM image (`coco3.rom`) located in `/coco/roms/coco3.rom` (Super Extended Color BASIC 2.1).
- **Physical Hardware Scope & 128 KB Memory Limit (PIZERO-111, PIZERO-228):**
  - The Waveshare RP2350-PiZero PCB features an unpopulated PSRAM pad, restricting the system strictly to the RP2350B's internal 520 KB SRAM.
  - CoCo 3 emulation on this board is strictly a **128 KB machine**. A 512 KB CoCo 3 requires 448 KB of additional RAM, which cannot fit within on-chip SRAM alongside the display framebuffer (~153.6 KB) and firmware infrastructure (firmware static + buffers ~400 KB = ~76.3% of 520 KB). Populating physical external PSRAM would be required for 512 KB support.
- **512 KB Software Execution & Memory Wrapping Mechanics:**
  - *Explicit Checks:* Software with built-in memory detection routines (e.g. *Golaxian*) queries available banks and cleanly reports `512K REQUIRED, 128K DETECTED` before halting.
  - *Blind Bank Mapping:* Titles that blindly map 512 KB memory banks (e.g. *Montezuma's Revenge*, *Sinistar preview*) map MMU pages outside the 128 KB boundary. On 128 KB hardware, 8 KB logical page indices wrap modulo 16 onto physical RAM. For example, in *Montezuma's Revenge*, loader routines set MMU slot `$FFA2` (`$4000–$5FFF`) to pages `$08`–`$11`; on 128 KB, page `$08` wraps to `$38`, aliasing Disk BASIC's workspace in `$0000–$1FFF` and causing disk load routines to abort with `?IE ERROR` or system lockups, matching authentic physical 128 KB CoCo 3 behavior.
- **GIME Chip Emulation (`tcc1014.c`):** Adapts the full XRoar GIME ASIC model to govern memory management, programmable interrupt timers, palette registers, and video generation.
- **MMU Paged Fast Path (`COCO_FASTBUS_PAGES`, PIZERO-213):** Employs an inlined page table that resolves physical 8 KB memory pages directly in the 6809 core, preserving high instruction throughput despite dynamic MMU remapping.
- **Port-Side GIME Scanline Renderer (PIZERO-218):**
  - Bypasses traditional frame rendering and blitting passes (`render` and `blit` = 0 µs).
  - During horizontal sync (`do_hs_fall`), active scanlines are decoded directly from VRAM into the $640 \times 240$ 8-bit paletted framebuffer (`g_fb`) using the GIME 64-entry color table (`coco3_bus_render_line`).
  - Scanline dirty-checking (`port_sum`) skips re-rendering unchanged scanlines, maintaining low video CPU overhead (~3.0 ms worst-case).
  - Authentic dynamic beam tearing is faithfully reproduced as the virtual beam traverses the screen.
- **Performance Characteristics (PIZERO-219, PIZERO-220):**
  - Runs at locked 60.0 fps at both 0.895 MHz and 1.789772 MHz (`POKE 65497,0`).
  - Full-screen high-speed animations (HCLS loops at 1.79 MHz) execute in ~12.9 ms median (15.2 ms worst-case), maintaining full 60.0 fps with zero late frames and 0% audio shortfall.
- **Native Keyboard & Second Fire Buttons (PIZERO-215):** CTRL (`0x34`), ALT (`0x33`), F1 (`0x35`), and F2 (`0x36`) are wired into matrix row 6. Gamepad triggers R2 and L2 drive native CoCo 3 second fire buttons (`fire2`, `fire2_right`) evaluated via analog thresholding ($\ge 128$).

---

## 8. User Interface & Software Features

### 8.1 On-Screen Display (OSD) Overlays

- Access lists via dedicated keys: **F12** (Disks), **F9** (Programs), **F10** (Cartridges), **F11** (Files).
- **`< INFO >` Screen:** Accessed from the Disks list (F12) by pressing **Left Arrow**. Displays firmware build, RP2350 chip serial, uptime, free RAM, and USB device status.
- **Factory Colors (PIZERO-256):** All cards and overlays render in factory green on black (`CARD_INK` = `0x07E0`, `CARD_PAPER` = `0x0000`), unaffected by guest software palette reprogramming.
- Navigation: Up/Down arrows move; PgUp/PgDn/Home/End jump; Enter selects/mounts; Esc dismisses; Tab opens text editor.

### 8.2 Built-In On-Screen Text Editor ([`text_editor.cpp`](file:///Users/richardlucente/development/git/xroar-waveshare-rp2350-pizero/src/text_editor.cpp) / [`text_edit.h`](file:///Users/richardlucente/development/git/xroar-waveshare-rp2350-pizero/src/text_edit.h))

- Press **F11** $\rightarrow$ `SETTINGS.TXT`, or highlight any game title in F9/F10/F12 and press **TAB** to create/edit its companion `.TXT` override file.
- Features cursor navigation, text insertion, Backspace/Delete, held key auto-repeat, and **Ctrl-S** atomic saving (writes to temporary file before replacing original to prevent corruption).

### 8.3 Configuration System & Machine Profiles

Settings follow a strict three-layer hierarchy:
1. **Machine Profile Defaults:** Declared by `machine = cocozero | coco2 | coco3`.
2. **Global Configuration:** `/coco/settings.txt` on microSD card.
3. **Per-Game Overrides:** Companion file in the same folder as the software (`STARCAST.TXT` for `STARCAST.DSK`).

#### Configuration Parameter Matrix
| Parameter | Permitted Values | Default | Purpose & Description |
|---|---|---|---|
| `machine` | `cocozero` / `coco2` / `coco3` | `cocozero` | Sets machine profile. Switching to/from CoCo 3 triggers an automatic board reboot. |
| `sn76489` | `on` / `off` | Profile (`on` CoCo Zero) | Enables SN76489 sound chip at `$FF41`. |
| `coco_psg` | `on` / `off` | Profile (`on` CoCo Zero) | Enables CoCo PSG AY-3-8910 cartridge at `$FF5D–$FF5F`. |
| `symphony12` | `on` / `off` | Profile (`on` CoCo Zero) | Enables Symphony-12 4× AY-3-8910 cartridge at `$FF60–$FF63`. |
| `symphony12_stereo` | `on` / `off` | `on` | Routes Symphony-12 chips 1–2 to Left and chips 3–4 to Right. |
| `orchestra90` | `on` / `off` | Profile (`on` CoCo Zero) | Enables Orchestra-90CC stereo DACs at `$FF7A` and `$FF7B`. |
| `deluxe_psg` | `on` / `off` | `off` | Enables Tandy Deluxe CoCo sound chip at `$FF38–$FF39`. |
| `sound_sources` | `all` / `switched` | Profile (`all` CoCo Zero) | `all` mixes every source concurrently; `switched` enforces authentic multiplexer routing. |
| `joystick_quiet` | `on` / `off` | Profile (`on` CoCo Zero) | Cancels DAC trial writes within 16 cycles of PIA0 PA read to silence joystick buzz. |
| `usb_audio` | `on` / `off` | `on` | Routes audio to USB headphone DAC if present at power-on. |
| `usb_audio_db` | `-60` to `0` | `-10` | Hardware attenuation in dB applied to USB headphone DAC at startup. |
| `volume` | `0` to `15` | `10` | Master audio volume (adjustable live via keyboard volume keys). |
| `audio_lowpass` | `1000`–`20000` / `on` / `off` | `8000` | 2-pole Butterworth low-pass filter cutoff frequency in Hz. |
| `artifact_colors` | `on` / `off` / `swapped` | `on` | PMODE 4 NTSC composite artifact color rendering (CoCo 2). |
| `gime_palette` | `on` / `off` | Profile (`on` CoCo Zero) | Enables `$FFB0–$FFBF` palette registers on CoCo 2. |
| `gime_timer` | `on` / `off` | Profile (`on` CoCo Zero) | Enables `$FF90–$FF95` 50 kHz timer on CoCo 2. |
| `font` | `classic` / `6847t1` / `6847t2` | Profile (`6847t2` CoCo Zero)| Display text font for CoCo 2. |
| `lowercase` | `on` / `off` | Profile (`on` CoCo Zero) | Holds true lowercase enabled on CoCo 2. |
| `key_repeat` | `on` / `off` | Profile (`on` CoCo Zero) | Enables hardware auto-repeat for USB keyboard. |
| `key_repeat_delay` | `100` to `2000` | `500` | Delay in milliseconds before held key begins repeating. |
| `key_repeat_rate` | `1` to `30` | `10` | Repeats per second (clamped to 12). |
| `autorun` | `on` / `off` | `on` | Auto-runs Drive 0 executable or `/coco/autorun.txt` at boot. |
| `reset_button` | `basic` / `autorun` | `basic` | Board RUN button resets to BASIC (`basic`) or re-executes autorun (`autorun`). |
| `serial_keyboard` | `on` / `off` | `on` | Injects characters received over USB serial CDC into keyboard buffer. |
| `joystick_swap` | `on` / `off` | `off` | Inverts Left and Right physical gamepad sticks. |
| `soak_log` | `on` / `off` | `off` | Writes minute-by-minute telemetry logs to `/coco/log/soak-NNN.txt`. |
| `dpad` | `joystick` / `arrows` | `joystick` | Maps gamepad D-pad to analog joystick axes or 6809 arrow keys. |
| `pad_<button>` | Action name | See §5.4 | Binds physical gamepad buttons to actions (`fire`, `fire2`, `space`, `enter`, etc.). |
| `color_<name>` | `#RRGGBB` | Factory default | Overrides CoCo 2 VDG palette color. |

### 8.4 Autorun Automation (`autorun.txt`)

- Directives:
  - `# <comment>`: Ignored.
  - `@CART <name.ccc>`: Mounts cartridge from `/coco/cart/` and cold-boots.
  - `@DIRECT <name.bin>`: Writes executable directly into RAM and jumps to entry point.
  - `<Text Line>`: Injected directly into BASIC prompt.
  - `~` Token: Inserts a **1.0-second delay** per token (e.g. `~~~~~~~~~~~~~~~` = 15 s delay for NitrOS-9 boot).
- Bypass: Holding **Space** or **Break / Esc** during boot skips autorun and enters BASIC prompt.

---

## 9. System Reliability, Watchdog & Telemetry

### 9.1 Hardware Watchdog & Post-Mortem Crash Diagnostics (PIZERO-227)

- **Hardware Watchdog:** 3,000 ms hardware watchdog timer enabled via `watchdog_enable(WATCHDOG_TIMEOUT_MS, true)`.
- **Execution Phase Tracking:** As Core 0 loops, it updates the hardware watchdog scratch registers to mark execution milestones:

| Watchdog Scratch Register | Name / Value | Description |
|---|---|---|
| `watchdog scratch[0]` | `WD_MAGIC` (`0x05330534u`) | Sentinel distinguishing valid freeze records from cold power-on state |
| `watchdog scratch[1]` | Phase Enum (`1..11`) | Active execution phase (see table below) |
| `watchdog scratch[2]` | Frame Heartbeat | Cumulative elapsed frame counter prior to freeze |
| `watchdog scratch[3]` | Freeze Tally & Phase | Packaged as `[31:8]=cumulative_freeze_count \| [7:0]=last_freeze_phase` |
| `watchdog scratch[4]` | `USB_REPLUG_MAGIC` (`0x51D15C09u`) | Deliberate reboot sentinel for USB re-enumeration |
| `watchdog scratch[5]` | Fault PC | Program Counter of interrupted code (resolvable via `arm-none-eabi-addr2line`) |
| `watchdog scratch[6]` | Fault LR | Link Register (return address) of interrupted code |
| `watchdog scratch[7]` | Fault State Flags | Encoded as `kind << 24 \| core << 20 \| saves << 12 \| exception_number` |

| Powman Scratch Register | Name / Value | Description |
|---|---|---|
| `powman scratch[0]` | Uptime | System uptime in milliseconds at the last save |
| `powman scratch[1]` | `(span_ms << 16) \| core1_lines` | Evaluates whether Core 1 DVI scanout continued running while Core 0 stalled |
| `powman scratch[2]` | `(guest_6809_pc << 16) \| (usb_device_count << 8) \| exc_return_byte` | Guest CPU PC and USB state |
| `powman scratch[3..6]` | Interrupt / Fault State | Core 0 NVIC state (`IABR[0..1]`, `ISPR[0..1]`) or Cortex-M33 fault registers (`CFSR`, `HFSR`, `BFAR`, `MMFAR`) |
| `powman scratch[7]` | `WDP_MAGIC` (`0x57445031u` = `'WDP1'`) | Sentinel confirming valid post-mortem record |

#### Execution Phase Constants (`watchdog scratch[1]`)
| Phase Constant | Value | Execution Subsystem |
|---|---|---|
| `WP_SETUP` | 1 | Board hardware and peripheral initialization |
| `WP_LOOP` | 2 | Main frame loop top |
| `WP_USB` | 3 | TinyUSB host task dispatch and event polling |
| `WP_KBD` | 4 | Keyboard matrix scanning and chord evaluation |
| `WP_EMU` | 5 | 6809 / GIME guest machine instruction execution |
| `WP_RENDER` | 6 | VDG / GIME scanline rendering |
| `WP_BLIT` | 7 | Framebuffer transfer to `g_fb` |
| `WP_AUDIO` | 8 | Audio sample generation, mixing, and ring buffer write |
| `WP_PACE` | 9 | Inter-frame pacing delay and VSYNC wait |
| `WP_TELEM` | 10 | Serial telemetry output formatting and transmission |
| `WP_SOAKLOG` | 11 | MicroSD soak log write and FAT commit |

#### Watchdog Post-Mortem Subsystem ([`wd_post.cpp`](file:///Users/richardlucente/development/git/xroar-waveshare-rp2350-pizero/src/wd_post.cpp) / [`wd_post.h`](file:///Users/richardlucente/development/git/xroar-waveshare-rp2350-pizero/src/wd_post.h))
To eliminate guesswork when Core 0 stalls, the firmware deploys an autonomous early-warning and fault-capture mechanism that saves real CPU state across watchdog resets:
1. **Early Warning Alarm (Timer 0):** A spare hardware timer alarm (`timer0`) running at the highest interrupt priority on Core 0 evaluates the watchdog countdown every 200 ms. If fewer than 1,000 ms remain before timeout, it interrupts execution and captures:
   - Interrupted PC (`scratch[5]`) and LR (`scratch[6]`).
   - System uptime, Core 1 scanout state, guest 6809 PC, and attached USB device count.
   - NVIC interrupt active/pending registers (`IABR`, `ISPR`) and ARM Cortex-M33 SCB fault status registers (`CFSR`, `HFSR`, `BFAR`, `MMFAR`).
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
- `fps`: Achieved physical video refresh rate (locked at 60.0 fps).
- `cpu`: Core 0 6809 emulation duration in microseconds (~6,200 µs at normal 1× speed; ~12,600 µs at 2× speed).
- `blit`: Framebuffer copy / scanline blitting duration.
- `slack`: Minimum spare headroom remaining in any single frame during that 1-second window.
- `late`: Number of frames that failed to complete within the 16,667 µs frame budget.
- `cmax`: Maximum Core 0 emulation duration encountered during that second.
- `wmax`: Maximum whole-frame workload (emulation + render + blit + USB + audio) during that second.
- `badfps`: Cumulative seconds since boot where frame rate dropped below 60.0 fps.

Additional subsystem telemetry streams output during active operation:
- `[aud]`: Audio ring buffer fill levels, drain rate, and closed-loop servo adjustments.
- `[csg]`: SN76489 register writes, voice updates, and dropped commands.
- `[ay]`: AY-3-8910 bank writes, active chip bitmasks, and heard-parts skipping tallies.
- `[orch]`: Orchestra-90CC sample write counts and integrator states.
- `[uac]`: USB Audio streaming packet delivery rate, isochronous frame pacing, and underruns.
- `[settings]`: Applied configuration parameters and volume changes.

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
- **Readiness Announcement:** When virtual cycles reach the Color BASIC boot readiness threshold (630,000,000 cycles on CoCo 2; 882,000,000 cycles on CoCo 3), the console announces:
  ```text
  [System Ready] Color BASIC initialization complete! Ready for keyboard input.
  ```

---

## 10. Physical Enclosure Specifications

The custom two-piece enclosure ([`hardware/case/pizero_case.scad`](file:///Users/richardlucente/development/git/xroar-waveshare-rp2350-pizero/hardware/case/README.md)) is parametric and rendered in OpenSCAD:
- **Dimensions:** 69.8 mm (Length) × 34.8 mm (Width) × 18.6 mm (Height).
- **Styling:** Modeled after the ventilated top chassis of the Tandy Color Computer 2.
- **Construction:** Two-piece split at the top plane of the PCB, eliminating 3D print overhangs.
- **Fasteners:** Four M2.5 countersunk screws (12 mm length) passing through board mounting holes into lid bosses.
- **Port Cutouts:** Mini HDMI, dual USB-C, microSD slot, battery JST header, with RUN and BOOT buttons accessible through ventilation slats.

---

## 11. Firmware Build Environments Matrix

### 11.1 Firmware PlatformIO Build Profiles

Built using PlatformIO with the Earle F. Philhower, III Arduino-Pico core (`board = waveshare_rp2350_pizero`):

| Build Environment | Video Mode | Audio Delivery | Sysclk / Pixel | Primary Purpose |
|---|---|---|---|---|
| **`pizero_stream_60`** | **640×480p @ 60 Hz** | **523-Line Streaming / USB Audio** | **252 MHz / 25.2 MHz** | **Default Product Build:** 8/92/60 wide back-porch timing, full 4-sample packets, 640×240 paletted video, unified triple-machine (`machine = cocozero \| coco2 \| coco3`). |
| **`pizero_stream`** | 640×480p @ ~52 Hz | Streaming Data Islands | 240 MHz / 24.0 MHz | Fallback build for legacy monitors rejecting 25.2 MHz 60 Hz video. |
| **`pizero_coco3`** | 640×480p @ 60 Hz | 523-Line Streaming / USB Audio | 252 MHz / 25.2 MHz | Alias extending `pizero_stream_60` kept for backward compatibility. |
| **`pizero_usbdump`** | 640×480p @ 60 Hz | Streaming Data Islands | 252 MHz / 25.2 MHz | Diagnostic: Dumps complete USB configuration descriptors, HID reports, and UAC formats. |
| **`pizero_usbtone`** | 640×480p @ 60 Hz | USB Audio Test Tone | 252 MHz / 25.2 MHz | Diagnostic: Streams 440 Hz test tone to USB headphone DAC. |
| **`pizero_cpuprof`** | 640×480p @ 60 Hz | Streaming Data Islands | 252 MHz / 25.2 MHz | Diagnostic: Profiles Cortex-M33 clocks per emulated 6809 bus cycle via DWT. |
| **`pizero_coco3_prof`** | 640×480p @ 60 Hz | Streaming Data Islands | 252 MHz / 25.2 MHz | Diagnostic: Profiles CoCo 3 clocks across GIME bus and port scanline renderer. |
| **`pizero_fdctrace`** | 640×480p @ 60 Hz | Streaming Data Islands | 252 MHz / 25.2 MHz | Diagnostic: Deep FDC, sector read/write checksums, and PC tracing over USB serial. |
| **`pizero_fdctrace_uart`** | 640×480p @ 60 Hz | Streaming Data Islands | 252 MHz / 25.2 MHz | Diagnostic: Routes FDC trace output over UART0 for host emulator capture. |
| **`pizero_padprobe`** | 640×480p @ 60 Hz | Streaming Data Islands | 252 MHz / 25.2 MHz | Diagnostic: Dumps raw gamepad HID report bytes to serial console. |
| **`pizero_videobench`** | 640×480p @ 60 Hz | Streaming Data Islands | 252 MHz / 25.2 MHz | Diagnostic: Benchmarks scanline TMDS encoding performance in SRAM. |
| **`pizero_wdtest`** | 640×480p @ ~52 Hz | Streaming Data Islands | 240 MHz | Diagnostic: Forces Core 0 freezes to verify watchdog recovery and post-mortem logs. |
| **`pizero_wavmeas`** | Disabled | WAV Stream via USB-CDC | 240 MHz | Diagnostic: Streams 48 kHz audio as base64 WAV over USB for frequency analysis. |
| **`native`** | N/A | N/A | N/A | Host Unit Tests: 24 test suites executed on host workstation via Unity. |

### 11.2 Critical Compiler & Toolchain Constraints

1. **`PICO_NO_HARDWARE` Trap:** The arduino-pico core defines `PICO_NO_HARDWARE = 0`. SDK macros like `__not_in_flash_func()` silently no-op in plain C files. Time-critical functions must use explicit section attributes:
   ```c
   #define DVI_DI_RAMFUNC __attribute__((section(".time_critical.dvi_di")))
   ```
2. **PIO GPIO Base:** Because TMDS lanes reside on GPIO 32–39 (above GPIO 31), PIO requires `-DPICO_PIO_USE_GPIO_BASE=1` and `pio_set_gpio_base(pio, 16)`.
3. **Include Ordering:** `#include <Arduino.h>` must always precede XRoar headers to prevent conflicts with C99 `_Bool`.
4. **Build Flag Invalidation:** Avoid `PLATFORMIO_BUILD_FLAGS` environment variables as SCons links stale objects; configure explicit profiles in `platformio.ini`.

---

## 12. Test Coverage & Verification Suites

### 12.1 Reference Firmware Unity Unit Tests (24 Suites)

Isolated logic verified under `platform = native` via Unity:
- `test_audio_integ`: Resampler integration catch-up over multi-cycle spans and 8/16-tick runs.
- `test_audio_lowpass`: Two-pole Butterworth low-pass filter and DC blocker frequency response.
- `test_audio_servo`: Frequency error compensation and audio ring level servo.
- `test_cart_gmc`: Bank-switched Games Master Cartridge address decoding.
- `test_coco_palette`: RGB565 color palette lookup and GIME registers.
- `test_coco_sound`: Multiplexer millivolt levels, sound enable, and joystick SAR trial probe cancellation.
- `test_coco3_boot`: Boots 128 KB CoCo 3 machine on host, validates GIME bus arbiter and MMU fast path.
- `test_csg_sn76489`: SN76489 sound chip attenuation, tone, and noise generation.
- `test_data_island`: TERC4 encoding, BCH error correction, and InfoFrame generation.
- `test_dsk_catalog`: Directory scanning, sorting, and pagination.
- `test_gamepad`: USB HID report decoding for DualShock 4, XInput, and Android pads.
- `test_gime_timer`: CoCo 3 50 kHz timer and interrupt generation.
- `test_hid_consumer`: USB Consumer page parsing for keyboard volume keys and DAC headset buttons.
- `test_key_translate`: USB HID keycap to CoCo matrix chord translation and CoCo 3 native keys.
- `test_mc6809_lockstep`: Runs reference 6809 core against optimized live core in lockstep, verifying instruction execution and registers.
- `test_overlay_keys`: OSD overlay menu navigation and state machine.
- `test_png_write`: MicroSD PNG screenshot compression and output.
- `test_psg_ay8910`: AY-3-8910 four-chip bank, clock rates, register decoding, and heard-parts skipping.
- `test_rsdos_dir`: RS-DOS / Disk BASIC directory parser and executable detection.
- `test_settings`: Configuration parser and key-value serialization.
- `test_text_card`: 32×16 OSD character grid formatting and factory green styling.
- `test_text_edit`: On-screen text editor buffer manipulation and cursor movements.
- `test_vdg_pack`: MC6847 VDG pixel bit-packing and font rendering.

### 12.2 Host Emulator Rust Automated Test Suite (`cargo test`)

Executed via `cargo test --tests --release`:
- [`tests/menu_test.rs`](file:///Users/richardlucente/development/git/cocozero-emulator/tests/menu_test.rs): End-to-end integration test validating virtual FAT32 construction, boot to BASIC, F12 HID keycode injection, `/coco/dsk/` directory scanning, overlay rendering, and menu dismissal.
- [`tests/keyboard_test.rs`](file:///Users/richardlucente/development/git/cocozero-emulator/tests/keyboard_test.rs): Validates boot detection, keyboard matrix chording, hold/gap timing, and BASIC typing.
- [`tests/hardware_test.rs`](file:///Users/richardlucente/development/git/cocozero-emulator/tests/hardware_test.rs): Validates RP2350 SIO TMDS hardware encoder registers.
- [`tests/screen_test.rs`](file:///Users/richardlucente/development/git/cocozero-emulator/tests/screen_test.rs): Validates SRAM framebuffer discovery and RGB565 extraction.
- [`tests/multicore_test.rs`](file:///Users/richardlucente/development/git/cocozero-emulator/tests/multicore_test.rs): Validates SIO FIFO inter-core messaging and Boot ROM Core 1 WFE parking.
- [`tests/storage_test.rs`](file:///Users/richardlucente/development/git/cocozero-emulator/tests/storage_test.rs): Validates FAT32 dynamic synthesis, sector read/write commands, and file writeback.
- [`tests/audio_test.rs`](file:///Users/richardlucente/development/git/cocozero-emulator/tests/audio_test.rs): Validates audio ring buffer drain, latency budget enforcement, and DC blocking.
- [`tests/shortcut_test.rs`](file:///Users/richardlucente/development/git/cocozero-emulator/tests/shortcut_test.rs): Validates cross-platform keyboard shortcuts (`Cmd+D`, `Cmd+P`, `Cmd+C`, `Cmd+I`, `Cmd+A`).
- [`tests/integration_test.rs`](file:///Users/richardlucente/development/git/cocozero-emulator/tests/integration_test.rs): Full-system headless execution verifying execution lifecycle from reset vector to BASIC prompt.
- [`tests/telemetry_test.rs`](file:///Users/richardlucente/development/git/cocozero-emulator/tests/telemetry_test.rs): Validates UART0 telemetry parsing and interval reporting.
- [`tests/stress_test.rs`](file:///Users/richardlucente/development/git/cocozero-emulator/tests/stress_test.rs): Multi-frame high-load stability and memory barrier stress testing.
- [`tests/trace_fault.rs`](file:///Users/richardlucente/development/git/cocozero-emulator/tests/trace_fault.rs): Validates ARM Cortex-M33 fault trapping and post-mortem register preservation.
- [`tests/demo_test.rs`](file:///Users/richardlucente/development/git/cocozero-emulator/tests/demo_test.rs): Automated script playback and visual validation test harness.

---

## 13. Emulator Command-Line Interface & Operational Reference

### 13.1 Command-Line Options Matrix

```text
Usage: cocozero-rp2350 [OPTIONS]
```

| Option | Flag | Type | Default | Description |
|---|---|---|---|---|
| `--turbo` | | `u32` | `2` | **Simulation speed multiplier.** Multiplies the virtual cycle quantum per frame. `--turbo 6` provides smooth 60.0 FPS real-time execution on typical desktop CPUs. |
| `--sd` | `-s` | `Path` | `./coco` | Path to host directory to package into virtual FAT32, or path to a raw `.img` SD disk image. |
| `--uf2` | `-u` | `Path` | `roms/cocozero.uf2` | Path to firmware UF2 binary. |
| `--bin` | `-b` | `Path` | *None* | Path to raw firmware binary (`.bin`). |
| `--elf` | | `Path` | `roms/cocozero.elf` | Companion ELF binary for dynamic symbol resolution. |
| `--bootrom` | | `Path` | `roms/rp2350/bootrom-combined.bin` | Path to RP2350 Boot ROM image. |
| `--fb-addr` | | `String` | *Auto-detected* | Explicit override for framebuffer SRAM address (e.g. `0x20022780`). |
| `--headless` | | `bool` | `false` | Run without an SDL2 GUI window (for automated testing and benchmarks). |
| `--frames` | `-f` | `u64` | `0` | Exit cleanly after $N$ frames (`0` = run indefinitely; aliased as `--max-frames`). |
| `--type` | | `String` | *None* | Enqueue ASCII text to type into Color BASIC once boot is complete. |
| `--script` | | `Path` | *None* | Path to script containing commands to execute upon boot. |
| `--dump-frame` | | `u64` | *None* | Dump framebuffer to a standard 24-bit BMP image at frame $N$. |
| `--dump-path` | | `Path` | `frame_<N>.bmp` | Destination path for dumped BMP file. |
| `--telemetry` | | `bool` | `false` | Enable periodic diagnostic telemetry reports to stdout. |
| `--telemetry-interval` | | `u64` | `60` | Frame interval between periodic telemetry reports. |
| `--sync-interval` | | `u64` | `300` | Frame interval for flushing dirty SD storage sectors to host disk. |
| `--audio-buffer-ms` | | `u32` | `100` | SDL2 audio buffer queue latency budget in milliseconds. |
| `--help` | `-h` | | | Print command-line help summary. |

### 13.2 Common Operational Examples

```bash
# Standard interactive execution at locked 60 FPS
cargo run --release -- --turbo 6

# Mount a custom games folder as the virtual SD card
cargo run --release -- --turbo 6 --sd /path/to/my_coco_folder

# Launch 128 KB Tandy Color Computer 3 with companion ELF symbols
cargo run --release -- --turbo 6 --uf2 roms/cocozero-coco3.uf2 --elf roms/cocozero-coco3.elf

# Automated visual regression test: dump frame 180 to a BMP image
cargo run --release -- --headless -f 185 --dump-frame 180 --dump-path screen.bmp

# Script injection: execute commands from a test script and output telemetry
cargo run --release -- --headless -f 200 --script tests/test_script.txt --telemetry

# Execute the complete automated regression test suite
cargo test --tests --release
```
