# CoCo Zero: RP2350B Hardware Emulator

An emulator for the **Raspberry Pi RP2350B** (Cortex-M33) microcontroller board running the bare-metal **CoCo Zero** firmware ([ugufru/xroar-waveshare-rp2350-pizero](https://github.com/ugufru/xroar-waveshare-rp2350-pizero)). It delivers an authentic, standalone **Tandy Color Computer 2 (CoCo 2)** experience with 60 FPS video, digital audio, virtual FAT32 SD storage, and full keyboard support.

---

## Quick Start

### 1. Install Prerequisites

Ensure you have **Rust** and **SDL2** installed on your system:

#### macOS
```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install SDL2
brew install sdl2 pkg-config
```

#### Ubuntu / Debian
```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install SDL2 and build tools
sudo apt-get update
sudo apt-get install -y libsdl2-dev pkg-config build-essential
```

### 2. Firmware Setup & Upstream Updates

The emulator runs bare-metal CoCo Zero firmware built from [ugufru/xroar-waveshare-rp2350-pizero](https://github.com/ugufru/xroar-waveshare-rp2350-pizero).

To incorporate recent builds or update to the latest upstream release, copy the compiled PlatformIO build artifacts from your local clone into the `roms/` directory:

```bash
# Copy the UF2 firmware flash image:
cp /path/to/xroar-waveshare-rp2350-pizero/.pio/build/pizero_stream_60/firmware.uf2 roms/cocozero.uf2

# Copy the ELF binary for automatic dynamic symbol extraction:
cp /path/to/xroar-waveshare-rp2350-pizero/.pio/build/pizero_stream_60/firmware.elf roms/cocozero.elf
```

> **Note on Dynamic Symbol Resolution:**
> The emulator automatically extracts physical symbol addresses (framebuffer base, keyboard matrix bitmasks, OSD overlay state, and TinyUSB HID callbacks) from `roms/cocozero.elf` at startup. This guarantees that upstream firmware rebuilds and memory relocations work immediately without modifying hardcoded addresses or recompiling the emulator. If no ELF file is provided, the emulator falls back to in-flash opcode and literal pool signature scanning.

### 3. Build and Run

Launch the emulator with real-time 60 FPS performance:

```bash
cargo run --release -- --turbo 6
```

By default, the emulator automatically loads:
- **Firmware:** `roms/cocozero.uf2` (with symbols from `roms/cocozero.elf`)
- **Boot ROM:** `roms/rp2350/bootrom-combined.bin`
- **Storage:** The local `./coco/` directory mounted as a virtual FAT32 MicroSD card.

To run the automated test suite:
```bash
cargo test --tests --release
```

---

## Storage & Directory Structure (`./coco`)

The emulator dynamically packages the `./coco/` host directory into a virtual FAT32 MicroSD card image at startup. Drop your software and media into the corresponding folders:

```text
coco/
├── roms/           # Color BASIC system ROMs (user-supplied)
│   ├── bas12.rom       # Color BASIC 1.2 (required)
│   ├── extbas11.rom    # Extended Color BASIC 1.1 (optional)
│   └── disk11.rom      # Disk BASIC 1.1 (optional)
├── dsk/            # Floppy disk images (.dsk) -> mount via F12
├── cart/           # ROM cartridges (.ccc / .rom) -> mount via F10
├── bin/            # Machine language binaries (.bin) -> load via F9 or LOADM
├── shots/          # Destination folder for captured screenshots (.png)
├── settings.txt    # System configuration settings
└── autorun.txt     # Optional boot script (auto-executed at startup)
```

> **Important Note on System ROMs:**
> Due to copyright restrictions, proprietary Color Computer ROM files are **not distributed** with this repository. You must provide your own ROM files in `coco/roms/` before starting the emulator:
> - `bas12.rom`: Tandy Color BASIC 1.2 (8192 bytes, required)
> - `extbas11.rom`: Extended Color BASIC 1.1 (8192 bytes, optional)
> - `disk11.rom`: Disk Extended Color BASIC 1.1 (8192 bytes, optional)

- **Disks (`coco/dsk/`):** Place your `.dsk` floppy images here. Press **F12** in the emulator to open the disk mount menu.
- **Cartridges (`coco/cart/`):** Place `.ccc` cartridge images here. Press **F10** to open the cartridge selector.
- **Programs (`coco/bin/`):** Place executable `.bin` files here. Press **F9** or use the BASIC `LOADM` command to run them.

---

## Features

- **RP2350B Dual Cortex-M33 Emulation:** Cycle-accurate execution of firmware with inter-core SIO FIFOs and atomic synchronization.
- **Dynamic Symbol Auto-Detection:** Automatically extracts physical symbol addresses from companion `.elf` binaries or in-flash signatures (framebuffer, keyboard matrix, menu OSD, TinyUSB HID callbacks, audio ring buffer).
- **Real-Time Digital Audio Pipeline:** 48 kHz stereo 16-bit digital audio streaming synchronized with the firmware producer ring buffer and low-latency SDL2 audio queues.
- **Bidirectional Virtual FAT32 Storage:** Maps `./coco` directory into a live FAT32 MicroSD card image with full writeback support for sector modifications, newly formatted disks, and save files.
- **Full Keyboard, Shortcut & Gamepad Support:** Native Color Computer keyboard matrix typing, cross-platform shortcuts (`Cmd` / `Ctrl`), and SDL2 gamepad navigation.
- **Diagnostic Telemetry & Automation:** Multi-core status tracking, WFE sleep ratios, audio buffer monitoring, headless frame dumps to BMP, and scripted command injection.

---

## Command-Line Options

```text
Usage: cocozero-rp2350 [OPTIONS]
```

| Option | Description | Default |
|---|---|---|
| `--turbo <N>` | **Virtual clock multiplier.** Scaling factor for emulation pacing (`1` = 252 MHz, `2` = 2×, `6` = real-time 60 FPS). | `2` |
| `-s, --sd <PATH>` | Host directory to mount as virtual FAT32 SD card, or path to a raw `.img` file. | `./coco` |
| `-u, --uf2 <PATH>` | Path to a custom firmware UF2 binary. | `roms/cocozero.uf2` |
| `-b, --bin <PATH>` | Path to a raw flash binary (`.bin`). | *None* |
| `--elf <PATH>` | Path to firmware ELF binary (`.elf`) for dynamic symbol extraction. | `roms/cocozero.elf` (if present) |
| `--bootrom <PATH>` | Path to RP2350 Boot ROM binary. | `roms/rp2350/bootrom-combined.bin` |
| `--fb-addr <ADDR>` | Override framebuffer SRAM address (e.g. `0x20022780`). | *Auto-detected* |
| `--headless` | Run in headless mode without opening an SDL2 GUI window. | `false` |
| `-f, --frames <N>` | Exit cleanly after `N` frames (`0` = run indefinitely; aliased as `--max-frames`). | `0` |
| `--type <TEXT>` | Enqueue ASCII text to be typed into Color BASIC once initialization is complete. | *None* |
| `--script <PATH>` | Path to a script file containing commands to execute upon Color BASIC readiness. | *None* |
| `--dump-frame <N>` | Dump framebuffer to a standard 24-bit BMP image at frame `N`. | *None* |
| `--dump-path <PATH>` | Destination path for dumped BMP file (defaults to `frame_<N>.bmp`). | `frame_<N>.bmp` |
| `--telemetry` | Enable periodic diagnostic telemetry reports. | `false` |
| `--telemetry-interval <N>`| Frame interval between periodic telemetry reports. | `60` |
| `--sync-interval <N>` | Frame interval for flushing dirty SD storage sectors to host disk. | `300` |
| `--audio-buffer-ms <N>` | SDL2 audio buffer queue latency budget in milliseconds. | `100` |
| `-h, --help` | Show command-line help. | |

### CLI & Automation Examples

```bash
# Standard interactive execution at 60 FPS
cargo run --release -- --turbo 6

# Headless visual regression test: dump frame 180 to a BMP image
cargo run --release -- --headless -f 185 --dump-frame 180 --dump-path screen.bmp

# Automated script execution: inject commands from a script file and print telemetry
cargo run --release -- --headless -f 200 --script tests/test_script.txt --telemetry

# Direct command typing: run a calculation and exit
cargo run --release -- --headless -f 180 --type "PRINT 100*42\n"
```

---

## Controls & Shortcuts

### Cross-Platform Menu Shortcuts

| Function | Host Key (PC) | Host Key (macOS) | Action |
|---|---|---|---|
| **Disks Menu** | **`F12`** | **`Cmd + D`** / `Ctrl + D` | OSD Floppy Disk Mount (`< DISKS >`) |
| **Programs Menu** | **`F9`** | **`Cmd + P`** / `Ctrl + P` | OSD Executable Binary Launcher (`< PROGRAMS >`) |
| **Cartridges Menu**| **`F10`** | **`Cmd + C`** / `Ctrl + C` | OSD ROM Cartridge Selector (`< CARTRIDGES >`) |
| **Files Menu** | **`F11`** | *N/A* | OSD Text Editor (`< FILES >`) |
| **Info / Telemetry**| **`F1`** | **`Cmd + I`** / `Ctrl + I` | OSD System Diagnostics & Telemetry (`< INFO >`) |
| **Artifact Toggle**| **`F8`** | **`Cmd + A`** / `Ctrl + A` | Cycle PMODE 4 NTSC Artifact Colors |
| **Screenshot** | **`PrintScreen`** | *N/A* | Capture display screenshot to `/coco/shots/` |

### Overlay Menu Actions & Navigation

When an on-screen menu overlay is open:
- **`Up` / `Down` / `PageUp` / `PageDown`**: Navigate file and cartridge lists.
- **`0` – `3`**: Insert or eject highlighted disk image into virtual drive 0, 1, 2, or 3.
- **`Enter` / `Return`**: Mount selection and cold-boot or launch executable.
- **`Escape`**: Close active menu overlay.
- **`W`**: Toggle write-protection lock on virtual floppy drive.
- **`N`**: Format a new blank `.dsk` disk image with prompted filename.
- **`R`**: Rename the highlighted disk image file.
- **`Tab`**: Open built-in on-screen text editor on highlighted title's `.TXT` configuration.

### Gamepad Controls (SDL2 Game Controller)

| Gamepad Button | Normal Emulation Mode | Menu Overlay Mode |
|---|---|---|
| **D-Pad Up / Down / Left / Right** | Arrow keys | List navigation & category switching |
| **A** | Physical Spacebar | Select / Mount / Start |
| **B / Back** | *Unmapped* | Close Menu (Escape) |
| **X** | *Unmapped* | Open Editor on `.TXT` (Tab) |
| **Y** | Spacebar (`' '`) | *Unmapped* |
| **Start** | Enter (`\r`) | *Unmapped* |
| **Guide / Home** | Open Disks Menu (`F12`) | Toggle Menu Closed |
| **L1 (Left Shoulder)** | *Unmapped* | Page Up |
| **R1 (Right Shoulder)** | Physical Spacebar | Page Down |

---

## Keyboard Matrix Mapping

Typed characters and physical key presses are mapped directly to the Color Computer keyboard matrix (8 columns × 7 rows):

| Host / PC Key | CoCo 2 Function | Notes |
|---|---|---|
| **`A` – `Z`** | Letters `A` – `Z` | Normal case in Color BASIC |
| **`0` – `9`** | Digits `0` – `9` | Numeric keypad also mapped |
| **`Enter` / `Return`** | `ENTER` | Executes command |
| **`Backspace` / `Delete`** | `LEFT` ($\leftarrow$) | Erases previous character |
| **`Escape`** | `BREAK` | Interrupts running program |
| **`Spacebar`** | `SPACE` | Space character / Primary game fire |
| **`Arrow Keys`** | `UP`, `DOWN`, `LEFT`, `RIGHT` | Navigation and game controls |
| **`Home`** | `CLEAR` | Clears the screen |
| **Symbols (`"`, `!`, `?`, `*`, `+`, etc.)** | Automatic Chords | Auto-pulses required chords (e.g. `"` pulses `Shift + 2`) |

---

## License

MIT License. Reference firmware and ROM files belong to their respective copyright holders.
