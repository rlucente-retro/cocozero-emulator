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

## Command-Line Options

```text
Usage: cocozero-rp2350 [OPTIONS]
```

| Option | Description | Default |
|---|---|---|
| `--turbo <N>` | **Virtual clock multiplier.** Use `--turbo 6` for real-time 60 FPS. | `1` |
| `-s, --sd <PATH>` | Host directory to mount as virtual SD card, or path to a raw `.img` file. | `./coco` |
| `-u, --uf2 <PATH>` | Path to a custom firmware UF2 binary. | `roms/cocozero.uf2` |
| `-b, --bin <PATH>` | Path to a raw flash binary (`.bin`). | *None* |
| `--elf <PATH>` | Path to firmware ELF binary (`.elf`) for dynamic symbol extraction. | `roms/cocozero.elf` (if present) |
| `--fb-addr <ADDR>` | Override framebuffer SRAM address (e.g. `0x20022900`). | *Auto-detected* |
| `--bootrom <PATH>` | Path to RP2350 Boot ROM binary. | `roms/rp2350/bootrom-combined.bin` |
| `--headless` | Run without an SDL2 GUI window (for benchmarks / scripting). | `false` |
| `--max-frames <N>` | Exit after `N` frames (`0` = run indefinitely). | `0` |
| `-h, --help` | Show command-line help. | |

### Examples

```bash
# Run with smooth 60 FPS emulation (standard)
cargo run --release -- --turbo 6

# Mount a custom games directory as the SD card
cargo run --release -- --turbo 6 --sd /path/to/my_coco_folder

# Run headless smoke test for 180 frames (~3 simulated seconds)
cargo run --release -- --headless --max-frames 180 --turbo 6
```

---

## Keyboard Controls

Typed characters and physical key presses are mapped directly to the Color Computer keyboard matrix:

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

### CoCo Zero Function Keys

- **`F1`**: Toggle On-Screen Diagnostics & Info Overlay
- **`F8`**: Cycle PMODE 4 NTSC Artifact Colors (*on / swapped / off*)
- **`F9`**: Programs Menu (`coco/bin/`)
- **`F10`**: Cartridges Menu (`coco/cart/`)
- **`F12`**: Floppy Disks Menu (`coco/dsk/`)

---

## License

MIT License. Reference firmware and ROM files belong to their respective copyright holders.
