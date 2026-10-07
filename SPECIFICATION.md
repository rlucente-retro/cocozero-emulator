# CoCo Zero: System Specification & Technical Reference Manual

**Target Platform:** Waveshare RP2350-PiZero  
**Emulated System:** Tandy Color Computer 2 (CoCo 2) with selective CoCo 3 enhancements  
**Reference Implementation:** [ugufru/xroar-waveshare-rp2350-pizero](https://github.com/ugufru/xroar-waveshare-rp2350-pizero)  
**Document Version:** 1.1.0  
**Status:** Validated Technical Specification & System Reference  

---

## 1. Executive Summary & Device Concept

The **CoCo Zero** is a standalone, bare-metal hardware implementation of a Tandy Color Computer 2 (Dragon 32/64 compatible) built on a Raspberry Pi Zero form-factor microcontroller board—the **Waveshare RP2350-PiZero**. The device requires only a digital display (TV or monitor), a USB-C keyboard or gamepad, a standard 5V USB-C power source (or 3.7V Li-Po battery), and a FAT32-formatted microSD card containing Color BASIC ROMs.

The system delivers:
- **Locked 60.0 fps real-time emulation** of the Motorola MC6809E CPU, MC6883 Synchronous Address Multiplexer (SAM), dual MC6821 Peripheral Interface Adapters (PIAs), and MC6847 Video Display Generator (VDG).
- **Combined video and digital audio over a single mini HDMI/DVI connector** via custom PIO-driven TMDS encoding with DVI/HDMI Data Island packets.
- **Three-voice SN76489 Complex Sound Generator** (Games Master Cartridge sound chip) mixed with native 6-bit DAC audio.
- **USB Host input** for standard USB HID keyboards (with US-keycap matrix translation and auto-repeat) and gamepads (decoded into dual analog joysticks and mapped fire buttons).
- **Comprehensive on-screen display (OSD) overlay system** providing disk mounting, program launching, cartridge selection, on-screen settings text editing, and system diagnostics.
- **Configuration and autorun engine** driven by plain-text files on SD card (`settings.txt`, `autorun.txt`, and per-game override files).
- **Custom 3D-printable two-piece enclosure** faithfully styled after the ventilated chassis of an original Tandy Color Computer 2.

```
+--------------------------------------------------------------------------+
|                               CoCo Zero                                  |
|                                                                          |
|  +--------------------+    +--------------------+    +----------------+  |
|  |     Core 0 (Host)  |    |    Core 1 (Video)  |    |  MicroSD Card  |  |
|  |  * 6809/SAM/PIA/VDG|    |  * libdvi Scanout  |    |  * ROMs        |  |
|  |  * USB Host (PIO1) |    |  * Hardware TMDS   |    |  * DSK Images  |  |
|  |  * SD FatFS Access |    |  * Audio Islands   |    |  * BIN / CCC   |  |
|  |  * VDG Frame Blit  |    |  * SIO Encoder     |    |  * Settings    |  |
|  |  * Audio Resampler |    |  * Active DMA IRQ  |    |  * Screenshots |  |
|  +---------+----------+    +---------+----------+    +-------+--------+  |
|            |                         |                       |           |
|            +-----------+-------------+                       |           |
|                        |                                     |           |
|         Mini DVI / HDMI Video + Audio Out             MicroSD SPI1 Slot  |
|         USB-C Host Input (Keyboard / Pad)             USB-C Power/Serial |
+--------------------------------------------------------------------------+
```

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
| **Watchdog Timer** | Hardware Watchdog | 3000 ms timeout; preserves multi-phase failure diagnostics in scratch registers |
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

---

## 3. Video Subsystem Specification

### 3.1 Display Geometry & Framebuffer Architecture

A native 640×480 RGB565 framebuffer consumes $640 \times 480 \times 2 = 614,400\text{ bytes}$ (~600 KB), which exceeds the entire 520 KB SRAM capacity of the RP2350. To operate reliably within SRAM alongside emulation memory, audio buffers, and USB stacks, the CoCo Zero uses a hardware line-doubled and pixel-doubled scanout architecture:

1. **Internal Framebuffer Resolution:** $320 \times 240$ pixels, 16-bit RGB565 format ($320 \times 240 \times 2 = 153,600\text{ bytes}$ or ~150 KB).
2. **CoCo Native Raster:** The MC6847 VDG renders a native display of $256 \times 192$ pixels nibble-packed (2 pixels per byte = 24,576 bytes).
3. **Centering & Borders:** The $256 \times 192$ active raster is centered inside the $320 \times 240$ framebuffer at offset $(X=32, Y=24)$.
   - Horizontal border: 32 pixels left, 32 pixels right.
   - Vertical border: 24 lines top, 24 lines bottom.
   - The static border is painted once at system initialization and never re-cleared, eliminating 27,648 redundant pixel writes per frame.
4. **Hardware Scanout Scaling:**
   - Pixel doubling: 2 TMDS symbols per word (`DVI_SYMBOLS_PER_WORD = 2`).
   - Line doubling: Scanline repeat factor of 2 (`DVI_VERTICAL_REPEAT = 2`).
   - Monitor Active Visible Area: $640 \times 480$ pixels.
   - Visible CoCo Active Area on Monitor: $512 \times 384$ pixels centered with black border margins.

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

#### Complete Video Timing Matrix (`pizero_stream_60`)

| Timing Parameter | Horizontal (Pixels) | Vertical (Lines) | Notes |
|---|---|---|---|
| **Active Video** | 640 | 480 | Line doubled from 240 logical scanlines |
| **Front Porch** | 8 | 10 | Tight front porch to maximize back porch |
| **Sync Pulse** | 96 | 2 | Standard sync duration (negative polarity) |
| **Back Porch** | 56 | 33 | 28 TMDS words: fits 1 streaming audio data island |
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

---

## 4. Audio Subsystem Specification

### 4.1 Digital Audio Transport via TMDS Data Islands

The CoCo Zero features no analog audio jack or PWM low-pass filter. Instead, digital audio is transmitted over the digital video link through **HDMI/DVI Data Islands** embedded within the TMDS signal.

```
Scanline Structure:
[ Preamble: 4w ][ Guard: 1w ][ Data Island: 16w ][ Guard: 1w ][ Video Pre: 4w ][ Guard: 1w ][ Active Video: 320w ]
|<----------------------- Back Porch: 28 words (56 px) ------------------------------------>|
```

1. **Packet Structure & Parity:**
   - 32-byte Data Island packets containing 4-byte headers and 4 subpackets of 7 bytes each.
   - Encoded using TERC4 (Transition Minimized 4-to-10 bit encoding) and BCH error-correction parity (`dvi_data_island.c`).
2. **Vertical Blanking Transmissions:**
   - Auxiliary Video Information (AVI) InfoFrame.
   - Audio InfoFrame (2-channel, 48 kHz, 16-bit uncompressed LPCM).
   - Audio Clock Regeneration (ACR) packets: Regenerates 48 kHz audio clock at the display sink. Set to $CTS = 25200$, $N = 6144$ for 25.200 MHz pixel clock.
3. **Active Line Streaming Audio Delivery:**
   - To prevent buffer underrun/overrun warble, audio samples are metered onto active horizontal lines during the 56-pixel (28-word) back porch.
   - A 16.16 fixed-point accumulator (`g_meter_step`) releases **0 to 4 samples per active scanline**, dispersing the 800 samples/frame evenly across the 480 active physical lines.
   - Core 1 encodes each audio island in the DMA interrupt service routine (ISR) **one line ahead of the electron beam**, consuming ~4.8 µs per encode within a ~33 µs line window.
   - Rotating back-porch buffer pool (6 buffers) reclaims ~126 KB SRAM compared to legacy static pre-encoded banks.

### 4.2 Proportional Closed-Loop Audio Servo (`audio_servo.h`)

The audio producer (Core 0 emulation) runs on emulated MC6809 cycles ($14.31818\text{ MHz} / 16$), while the consumer (Core 1 DVI scanout) runs on the physical TMDS clock ($25.200\text{ MHz}$). Due to oscillator drift and fractional cycle rounding, the two clocks differ by approximately $0.02\%$ (~10 samples/second).

To prevent long-term buffer underrun (audio drops) or overrun (lost packets):
- The firmware executes a **proportional closed-loop servo** (`audio_servo_rate`) sampled each frame.
- Midpoint target: $\frac{\text{Capacity}}{2} = 4,096\text{ samples}$.
- Error calculation: $err = fill - target$.
- Rate adjustment: $adj = -err / 32$, clamped to $\pm 64\text{ samples/sec}$ (max 0.13% adjustment, $<0.025$ semitones, completely inaudible).
- This drives the steady-state frequency error to zero, stabilizing the ring level indefinitely without audio distortion.

### 4.3 Emulated Audio Sources & Synthesis

| Audio Source | Architecture | Hardware Register | Emulation Details |
|---|---|---|---|
| **Native 6-bit DAC** | Resistor ladder DAC | PIA1 Port A (`$FF20`) | Level tracked on every PIA write; resampled via integrate-and-dump filter into 48 kHz 16-bit signed mono stream. |
| **1-Bit Cassette Audio** | Single-bit flip-flop | PIA1 Port B Bit 3 (`$FF22`) | Cassette sound output mixed into DAC audio path. |
| **SN76489 CSG** | 4-channel sound generator | `$FF41` (or GMC cart `$FF40–$FF5F`) | 3 square wave tone generators + 1 noise generator (white/periodic). Integrated into 48 kHz stream; global volume attenuation (0–15, default 10). |

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
  - `Caps Lock` $\rightarrow$ CoCo Case Toggle (`Shift + 0`)
  - Numeric Keypad: Fully mapped to numbers and math operators.
  - Arrow Keys: Directly mapped to CoCo `UP`, `DOWN`, `LEFT`, `RIGHT`.
- **Keyboard Auto-Repeat:** Hardware-assisted auto-repeat (`key_repeat = on`, default 500 ms delay, 10 repeats/sec clamped to max 12/sec for BASIC input queue stability).
- **Serial CDC Keyboard (`serial_keyboard = on`):** Characters received over USB-CDC are translated and queued directly into the virtual keyboard matrix. *Note: Color BASIC input routines require a brief settling delay between Enter and the next character; scripts should prefix lines with a leading space.*

### 5.3 USB Gamepad Subsystem

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

---

## 6. Storage & Filesystem Subsystem Specification

### 6.1 MicroSD Interface & FAT32 Hierarchy

The on-board microSD slot connects to the RP2350's hardware SPI1 controller at a 12.5 MHz clock rate. The storage stack uses FatFs v3.x (`carlk3/no-OS-FatFS-SD-SDIO-SPI-RPi-Pico`). The card must be formatted as FAT32.

```
/coco/
  |-- roms/
  |     |-- bas12.rom       (Required: 8192 bytes, Color BASIC 1.2)
  |     |-- extbas11.rom    (Optional: 8192 bytes, Extended Color BASIC 1.1)
  |     +-- disk11.rom      (Optional: 8192 bytes, Disk BASIC 1.1 Cartridge)
  |-- dsk/                  (Floppy disk images: .dsk files)
  |-- bin/                  (Executable binaries: .bin files)
  |-- cart/                 (ROM Cartridges: .ccc files)
  |-- shots/                (Screenshots: SCR0001.PNG - SCR9999.PNG)
  |-- settings.txt          (System-wide settings file)
  |-- autorun.txt           (Boot script and auto-execution commands)
  +-- drives.txt            (Persisted floppy drive mount states)
```

#### ROM Resolution Logic & Boot Failure Splashes
The loader searches for ROMs first in `/coco/roms/<name>` and falls back to `/coco/<name>`:
- **`bas12.rom` Missing:** System halts and displays `MSG_NOROM` splash screen with instructions.
- **`bas12.rom` Size != 8192 Bytes:** System halts and displays `MSG_BADROM` splash indicating exact found file size.
- **`extbas11.rom` Missing:** System displays `MSG_CBONLY` notice ("COLOR BASIC ONLY") for 4.0 seconds, then boots into Color BASIC 1.2 with 16 KB RAM ($A000–$BFFF and $E000–$FFFF). Disk commands will not be available.
- **MicroSD Card Unreadable:** System displays `MSG_NOSD` splash screen.

### 6.2 Floppy Disk Controller (FDC) Emulation

The system emulates a Western Digital **WD2797 / WD1793** Floppy Disk Controller mapped at `$FF48–$FF4B`:
- **Virtual Drives:** 4 independent floppy disk drives (Drive 0, 1, 2, 3).
- **Disk Image Format:** Standard unadorned single-sided `.dsk` files (typically 35 tracks, 18 sectors/track, 256 bytes/sector = 161,280 bytes).
- **Drive Select Register ($FF40):** Monitored to detect guest drive switching.
- **Read-Only Operation:** Current firmware mounts `.dsk` images read-only; write attempts return write-protect errors.
- **Drive Persistence:** Drive assignments are automatically preserved across power cycles in `/coco/drives.txt`.

#### RS-DOS Directory Scanning (`rsdos_dir.h`)
When a disk is selected for launch in Drive 0:
- The firmware scans Track 17, Sectors 3 through 11 (8 directory entries of 32 bytes per sector).
- Evaluates file types: Type 0 = BASIC, Type 2 = Machine Language.
- Automatically generates the launch command (`RUN"NAME"\r` or `LOADM"NAME":EXEC\r`).
- Queues command with a 180-frame (~3.0 s) warmup delay to allow Disk BASIC to initialize before typing.

### 6.3 Cartridge & Binary Loading Engine

- **Direct Binary Loading (`.bin`):** Color BASIC `LOADM` binary parsing (`coco_boot_parse_loadm`). Loads multi-segment headers directly into RAM and executes via direct Program Counter (PC) jump without requiring Disk BASIC.
- **ROM Cartridges (`.ccc`):**
  - Standard Cartridges: 2 KB, 4 KB, 8 KB, and 16 KB ROMs mapped into `$C000–$DFFF` (mirrored through `$E000–$FEFF`). Small cartridges repeat through the address window.
  - Bank-Switched GMC Cartridges: 32 KB to 256 KB banked ROMs (16 KB per bank, up to 16 banks). Bank switching triggered by guest writes to even addresses in `$FF40–$FF5F`:
    $$\text{bank\_offset} = (D \ll 14) \ \& \ ((\text{len} - 1) \ \& \ \text{0x3C000})$$
    Odd addresses in `$FF40–$FF5F` route to the SN76489 sound chip.
- **Screenshot Writer (`png_write.h`):** Saves $320 \times 240$ 8-bit indexed PNG images using uncompressed (STORED) deflate blocks directly to `/coco/shots/SCR0001.PNG` through `SCR9999.PNG`. Consumes ~77 KB per image without requiring heap-heavy zlib compression buffers.

---

## 7. CoCo Machine Emulation Core

### 7.1 Emulation Architecture & Memory Map

The emulation core is adapted from [XRoar](https://www.6809.org.uk/xroar/) and organized into modular abstractions:

```
Address Range    Device / Function
$0000 - $7FFF    32 KB Lower RAM (Always RAM)
$8000 - $9FFF    Extended Color BASIC ROM (or RAM when SAM TY=1)
$A000 - $BFFF    Color BASIC ROM (or RAM when SAM TY=1)
$C000 - $DFFF    Cartridge ROM (Standard / GMC Banked, or RAM when SAM TY=1)
$E000 - $FEFF    ROM Mirror / Cartridge Mirror (or RAM when SAM TY=1)
$FF00 - $FF1F    PIA0: Keyboard Matrix, Joysticks, Sound Selector
$FF20 - $FF3F    PIA1: 6-bit Sound DAC, Cassette Audio, VDG Mode Control
$FF40 - $FF5F    Floppy Controller / GMC Bank Register / SN76489 Sound Chip
$FF90 - $FF95    CoCo 3 GIME Timer & Interrupt Controller
$FFB0 - $FFBF    CoCo 3 GIME Palette Registers (16 RGB565 entries)
$FFC0 - $FFDF    SAM Control Registers (Display Offset, VDG Mode, Page, Speed)
$FFE0 - $FFFF    Interrupt & Reset Vectors
```

- **SRAM Backing:** 64 KB of virtual CoCo RAM is backed by a static buffer in on-chip SRAM allocated via `psram_stub.c` (`rp_mem_malloc`).
- **CPU Clock Frequency:** Authentic Color Computer speed of **0.894886 MHz** (1.789772 MHz color burst divided by 2).
- **Cycles per Frame:** At 60.0 Hz, each frame period is $16,666.7\ \mu\text{s}$. The CPU advances:
  $$\text{CYCLES\_PER\_FRAME} = \frac{894,886\text{ Hz}}{60.0\text{ Hz}} \approx 14,915\text{ cycles}$$
- **Time Accounting:** Fixed emulated time budget per frame ($14,915 \times 16 = 238,640\text{ event ticks}$).
- **Core 0 Workload Budget:**
  - CoCo Emulation: ~11.0 ms
  - VDG Frame Render: ~0.5 ms
  - RGB565 Blit: ~1.6 ms
  - Total Frame Execution Time: ~13.1 ms (~78.6% of available 16.67 ms window, leaving >3.5 ms headroom for USB host, SD I/O, and pacing).

### 7.2 SAM High-Speed Modes (Double Speed) Policy

The MC6883 SAM incorporates high-speed address decode modes (`POKE 65495,0` for address-dependent 1.3×–1.5× speed; `POKE 65497,0` for full 2× speed).
- **Firmware Status:** The port accepts the POKE writes and updates internal SAM register bits.
- **Cycle Charging:** The memory dispatcher charges a flat **16 event ticks** per cycle regardless of the SAM speed bit. The machine runs at authentic 1× speed.
- **Architectural Rationale:** Honoring 2× speed would nearly double the Core 0 CPU load from ~11 ms to ~22 ms per frame. On a 16.67 ms frame budget, this would cause pacing overruns and drain the audio ring buffer. Faster emulation is slated for future core optimization passes (`docs/coco3-plan.md`).

---

## 8. User Interface & Software Features

### 8.1 On-Screen Display (OSD) Overlays

The OSD runs over a 32-column × 16-row character card rendered using the 6847T2 font. The running emulator pauses completely during overlay display.

| Key | Overlay Menu | Description & Functionality |
|---|---|---|
| **F12** | **`< DISKS >`** | Displays available `.dsk` files. Keys **0–3** insert/eject the selected disk into virtual drive 0, 1, 2, or 3 live. **ENTER** mounts disk in drive 0, cold-boots the system, and auto-executes the disk's first program. |
| **F9** | **`< PROGRAMS >`** | Displays `.bin` binary files in `/coco/bin/`. **ENTER** direct-loads the binary into RAM and jumps to the execution address. |
| **F10** | **`< CARTRIDGES >`** | Displays `.ccc` cartridge images in `/coco/cart/`. **ENTER** attaches the cartridge and performs a cold reset into the cartridge. |
| **F11** | **`< FILES >`** | Lists editable configuration files (`SETTINGS.TXT` and `AUTORUN.TXT`). **ENTER** opens the built-in text editor. |
| **F1** | **`< INFO >`** | Displays system diagnostic telemetry: firmware version, git commit, RP2350 chip ID and revision, board unique serial ID, system clock, uptime, free heap memory, and attached USB devices. |
| **F8** | *Artifact Cycle* | Directly cycles through NTSC artifact modes (`on` $\rightarrow$ `swapped` $\rightarrow$ `off`) and saves the setting for the current title. |
| **PrtScn** | *Screenshot* | Captures the active $320 \times 240$ RGB565 display buffer, compresses it to PNG format, and writes it to `/coco/shots/`. |

### 8.2 Built-In On-Screen Text Editor (`text_editor.cpp` / `text_edit.h`)

Accessible via **F11** or by pressing **TAB** on any highlighted game in the F9/F10/F12 lists:
- Flat 4,096-byte memory buffer (`TED_MAX = 4096`).
- Automatic conversion of Windows CRLF line endings to clean `\n`.
- Cursor positioning, vertical and horizontal viewport scrolling over the 32×16 card.
- **Ctrl-S**: Performs an atomic file write (`file.tmp` $\rightarrow$ `file.txt`) to protect against corruption during power removal.
- **ESC**: Exits editor (prompts if unsaved changes exist).

### 8.3 Configuration System (`settings.txt` & Per-Game `.TXT`)

Settings are plain text key-value pairs stored in `/coco/settings.txt`:
```ini
# /coco/settings.txt
sn76489           = on        # Enable SN76489 sound chip at $FF41
volume            = 10        # Global audio volume (0-15)
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
dpad              = joystick  # Gamepad D-pad mode: joystick / arrows
pad_start         = enter     # Gamepad button mapping
```

**Per-Game Custom Configurations:** Any game can have an accompanying `.TXT` file in the same directory (e.g. `/coco/bin/ORBIT.TXT` for `/coco/bin/ORBIT.BIN`). When launching, `settings.txt` is loaded first, followed by the game's `.TXT` file on top, allowing per-game button remapping, artifact phases, or custom palettes.

### 8.4 Autorun Automation (`autorun.txt`)

If `autorun = on`, `/coco/autorun.txt` is evaluated at boot:
- `# comment`: Comment line.
- `@CART <filename.ccc>`: Automatically mounts the specified cartridge and boots.
- `@DIRECT <filename.bin>`: Direct-loads binary into RAM and executes immediately.
- `<BASIC commands>`: Types commands line-by-line into the BASIC interpreter (e.g. `LOADM"GAME":EXEC`).
- Default (if `autorun.txt` is absent): Automatically mounts the first disk in `/coco/dsk/` and runs its first program.
- Bypass: Holding **Space** or **Break** during boot skips autorun and halts at the BASIC `OK` prompt.

---

## 9. System Reliability, Watchdog & Telemetry

### 9.1 Hardware Watchdog & Multi-Phase Crash Recovery

- **Timeout:** 3,000 ms hardware watchdog timer (`watchdog_enable(WATCHDOG_TIMEOUT_MS, true)`).
- **Execution Phase Tracking:** As Core 0 loops, it updates the watchdog scratch registers:
  - `scratch[0]`: `WD_MAGIC` (`0x5744434F` = `'WDCO'`)
  - `scratch[1]`: Execution phase (`WP_SETUP`=0x01, `WP_USB`=0x02, `WP_LOOP`=0x03, `WP_KBD`=0x04, `WP_EMU`=0x05, `WP_RENDER`=0x06, `WP_BLIT`=0x07, `WP_AUDIO`=0x08, `WP_PACE`=0x09)
  - `scratch[2]`: Elapsed frames prior to freeze
  - `scratch[3]`: `(g_freeze_count << 8) | (last_phase & 0xFF)`
  - `scratch[4]`: `USB_REPLUG_MAGIC` (`0x55534252` = `'USBR'`) deliberate reboot sentinel
- **Post-Crash Diagnostics:** On reboot, the firmware inspects the watchdog scratch registers. If a freeze occurred, the serial console logs the exact phase where Core 0 stalled, total elapsed frames, and cumulative session freeze count.

### 9.2 Telemetry Stream (`[run]` Serial Logs)

Every second, Core 0 outputs performance and status telemetry over the USB-CDC serial port at 115,200 baud:
```
[run] fps=60.0 cpu=11024us blit=1612us aud=0us free_ram=127KB usb=1 hid_rpts=120 freezes=0
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

The project is built using PlatformIO with the Earle F. Philhower, III Arduino-Pico core targeting the RP2350B (`board = waveshare_rp2350_pizero`):

| Build Environment | Video Mode | Audio Delivery | Sysclk / Pixel | Primary Purpose |
|---|---|---|---|---|
| **`pizero_stream_60`** | **640×480p @ 60 Hz** | **Streaming Data Islands** | **252 MHz / 25.2 MHz** | **Default Product Build:** Full 60 Hz video, streaming audio, USB host, GIME palette & timer. |
| **`pizero_stream`** | 640×480p @ ~52 Hz | Streaming Data Islands | 240 MHz / 24.0 MHz | Fallback build for legacy displays rejecting 25.2 MHz 60 Hz video. |
| **`pizero_usbdebug`** | 640×480p @ 60 Hz | Streaming Data Islands | 252 MHz / 25.2 MHz | Diagnostic: TinyUSB host debug logging to serial console. |
| **`pizero_padprobe`** | 640×480p @ 60 Hz | Streaming Data Islands | 252 MHz / 25.2 MHz | Diagnostic: Dumps raw gamepad HID report descriptors to serial. |
| **`pizero_videobench`** | 640×480p @ 60 Hz | Streaming Data Islands | 252 MHz / 25.2 MHz | Diagnostic: Benchmarks scanline TMDS encoding performance. |
| **`pizero_wavmeas`** | Disabled | WAV Stream via USB-CDC | 240 MHz | Diagnostic: Streams 48 kHz audio as base64 WAV over USB for frequency analysis. |
| **`pizero_wdtest`** | 640×480p @ ~52 Hz | Streaming Data Islands | 240 MHz | Diagnostic: Forces Core 0 lockup to verify watchdog recovery. |
| **`native`** | N/A | N/A | N/A | Host Unit Tests: 16 test suites (222 test cases) executed on host PC via Unity. |

### Critical Compiler & Toolchain Constraints

1. **`PICO_NO_HARDWARE` Macro Trap:** In the arduino-pico core, `PICO_NO_HARDWARE` is defined as `0`. Standard SDK macros like `__not_in_flash_func()` evaluate `#if defined(PICO_NO_HARDWARE)` as true, silently causing critical time-sensitive functions to remain in flash memory. All time-critical interrupt handlers and encoders must use explicit section attributes:
   ```c
   #define DVI_DI_RAMFUNC __attribute__((section(".time_critical.dvi_di")))
   ```
2. **PIO GPIO Windowing:** Because DVI TMDS lanes reside on GPIO 32–39 (above GPIO 31), PIO requires `-DPICO_PIO_USE_GPIO_BASE=1` and `pio_set_gpio_base(pio, 16)`.
3. **Include Ordering:** `#include <Arduino.h>` must always precede XRoar headers to prevent conflicts with C99 `_Bool`.
4. **Build Flag Caching:** PlatformIO SCons does not reliably invalidate cached compilation objects when flags are passed through `PLATFORMIO_BUILD_FLAGS` environment variables. Real environment profiles defined in `platformio.ini` extending `pizero_base` must be used instead.

---

## 12. Host Unit Test Coverage

The firmware architecture isolates pure emulation, parsing, and data manipulation logic into header-only modules tested under `platform = native`:
- `test_audio_servo`: Frequency error compensation and audio ring level servo.
- `test_cart_gmc`: Bank-switched Games Master Cartridge address decoding.
- `test_coco_palette`: RGB565 color palette lookup and GIME registers.
- `test_csg_sn76489`: SN76489 sound chip attenuation, tone, and noise generation.
- `test_data_island`: TERC4 encoding, BCH error correction, and InfoFrame generation.
- `test_dsk_catalog`: Directory scanning, sorting, and pagination.
- `test_gamepad`: USB HID report decoding for DualShock 4, XInput, and Android pads.
- `test_gime_timer`: CoCo 3 50 kHz timer and interrupt generation.
- `test_key_translate`: USB HID keycap to CoCo matrix chord translation.
- `test_overlay_keys`: OSD overlay menu navigation and state machine.
- `test_png_write`: MicroSD PNG screenshot compression and output.
- `test_rsdos_dir`: RS-DOS / Disk BASIC directory parser and executable detection.
- `test_settings`: Configuration parser and key-value serialization.
- `test_text_card`: 32×16 OSD character grid formatting and text wrapping.
- `test_text_edit`: On-screen text editor buffer manipulation and cursor movements.
- `test_vdg_pack`: MC6847 VDG pixel bit-packing and font rendering.
