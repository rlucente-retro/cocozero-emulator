use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;
use clap::Parser;

use cocozero_rp2350::fat32::build_virtual_fat32;
use cocozero_rp2350::frontend::Frontend;

use cocozero_rp2350::soc::{CoCoZeroSoC, FB_PIXELS};
use cocozero_rp2350::spi_sd::{FileStorage, MemoryStorage, SectorStorage};
use cocozero_rp2350::uf2::{parse_uf2, FlashImage};

#[derive(Parser, Debug)]
#[command(
    name = "cocozero-rp2350",
    about = "Waveshare RP2350-PiZero hardware emulator for CoCo Zero firmware",
    version = "0.1.0"
)]
struct Args {
    /// Path to firmware UF2 binary (.uf2)
    #[arg(short, long)]
    uf2: Option<PathBuf>,

    /// Path to raw firmware binary (.bin)
    #[arg(short, long)]
    bin: Option<PathBuf>,

    /// Path to RP2350 Boot ROM binary (.bin)
    #[arg(long, default_value = "roms/rp2350/bootrom-combined.bin")]
    bootrom: PathBuf,

    /// Path to SD card image (.img) or host directory to mount (e.g. ./coco/)
    #[arg(short, long)]
    sd: Option<PathBuf>,

    /// Run in headless mode (no SDL2 window)
    #[arg(long)]
    headless: bool,

    /// Maximum frames to run before exiting (0 = run indefinitely)
    #[arg(long, default_value_t = 0)]
    max_frames: u64,

    /// Simulation turbo factor (1 = 252 MHz, 2 = 2x speedup / 126 MHz virtual frame quantum, 4 = 4x speedup, etc.)
    #[arg(long, default_value_t = 2)]
    turbo: u32,

    /// Override framebuffer SRAM address (e.g. 0x20010000)
    #[arg(long)]
    fb_addr: Option<String>,
}

fn main() {
    let args = Args::parse();

    println!("========================================================");
    println!("  CoCo Zero - Waveshare RP2350-PiZero Hardware Emulator ");
    println!("  Target: Dual Cortex-M33 @ 252 MHz, DVI/TMDS, SPI1 SD  ");
    println!("========================================================");

    // 1. Prepare SD card storage
    let sd_storage: Box<dyn SectorStorage + Send> = match &args.sd {
        Some(path) if path.is_file() => {
            println!("Loading SD card image: {}", path.display());
            Box::new(FileStorage::open(path).expect("Failed to open SD image file"))
        }
        Some(path) if path.is_dir() => {
            println!("Building virtual FAT32 filesystem from: {}", path.display());
            let img = build_virtual_fat32(path).expect("Failed to build virtual FAT32");
            Box::new(MemoryStorage::from_bytes(img))
        }
        _ => {
            let default_dir = Path::new("coco");
            if default_dir.is_dir() {
                println!("Auto-mounting local ./coco directory as SD card...");
                let img = build_virtual_fat32(default_dir).expect("Failed to build virtual FAT32");
                Box::new(MemoryStorage::from_bytes(img))
            } else {
                println!("No SD card specified; initializing empty 64 MB virtual card");
                Box::new(MemoryStorage::new(64 * 1024 * 1024))
            }
        }
    };

    // 2. Initialize SoC
    println!("Initializing RP2350B SoC engine...");
    let mut soc = CoCoZeroSoC::new(sd_storage).expect("Failed to initialize SoC");

    if let Some(fb_str) = &args.fb_addr {
        let clean = fb_str.trim_start_matches("0x").trim_start_matches("0X");
        if let Ok(addr) = u32::from_str_radix(clean, 16) {
            println!("Forcing framebuffer address to 0x{:08X}", addr);
            soc.fb_addr = Some(addr);
        }
    }

    // 3. Load Boot ROM if present
    if args.bootrom.exists() {
        println!("Loading RP2350 Boot ROM: {}", args.bootrom.display());
        let rom_data = std::fs::read(&args.bootrom).expect("Failed to read Boot ROM");
        soc.load_bootrom(&rom_data);
    } else {
        println!("Notice: Boot ROM {} not found; using direct flash boot", args.bootrom.display());
    }

    // 4. Load Firmware (UF2 or Raw Binary)
    if let Some(uf2_path) = &args.uf2 {
        println!("Loading UF2 firmware: {}", uf2_path.display());
        let uf2_data = std::fs::read(uf2_path).expect("Failed to read UF2 file");
        let flash = parse_uf2(&uf2_data).expect("Failed to parse UF2 binary");
        println!(
            "Flash image loaded: 0x{:08X} - 0x{:08X} (Entry PC: {:?}, Initial SP: {:?})",
            flash.min_addr, flash.max_addr, flash.entry_point, flash.initial_sp
        );
        soc.load_firmware(&flash);
    } else if let Some(bin_path) = &args.bin {
        println!("Loading raw binary: {}", bin_path.display());
        let bin_data = std::fs::read(bin_path).expect("Failed to read binary file");
        let mut flash = FlashImage::new(16 * 1024 * 1024);
        flash.load_raw_bin(0, &bin_data);
        soc.load_firmware(&flash);
    } else {
        let default_uf2 = Path::new("roms/cocozero.uf2");
        if default_uf2.is_file() {
            println!("Auto-loading default firmware: {}", default_uf2.display());
            let uf2_data = std::fs::read(default_uf2).expect("Failed to read default UF2 file");
            let flash = parse_uf2(&uf2_data).expect("Failed to parse UF2 binary");
            println!(
                "Flash image loaded: 0x{:08X} - 0x{:08X} (Entry PC: {:?}, Initial SP: {:?})",
                flash.min_addr, flash.max_addr, flash.entry_point, flash.initial_sp
            );
            soc.load_firmware(&flash);
        } else {
            println!("Warning: No firmware (--uf2 or --bin) specified.");
            println!("Running Boot ROM idle/USB boot loop.");
        }
    }

    // 5. Initialize Frontend
    let mut frontend = Frontend::new(args.headless).expect("Failed to initialize SDL2 frontend");
    println!("SDL2 frontend initialized successfully.");
    if !args.headless {
        println!("Display window opened: {}x{} @ 60 Hz", 640, 480);
    }

    // 6. Main Emulation Loop
    println!("Starting RP2350B execution loop...");
    let mut fb_buffer = [0u16; FB_PIXELS];
    let mut frame_count = 0u64;
    let start_time = Instant::now();
    let mut last_fps_time = Instant::now();
    let mut fps_counter = 0;

    // Spawn background non-blocking stdin reader for terminal typing
    let (stdin_tx, stdin_rx) = std::sync::mpsc::channel::<char>();
    std::thread::spawn(move || {
        use std::io::Read;
        let mut stdin = std::io::stdin();
        let mut buf = [0u8; 1];
        while stdin.read_exact(&mut buf).is_ok() {
            let ch = buf[0] as char;
            let mapped = match ch {
                '\n' => '\r',
                c => c,
            };
            if stdin_tx.send(mapped).is_err() {
                break;
            }
        }
    });

    let mut basic_ready_announced = false;

    loop {
        // Poll frontend input events
        if !frontend.poll_events() {
            println!("\nExit requested by user.");
            break;
        }

        // Forward physical key events (arrows, space, etc.)
        for ev in frontend.drain_key_events() {
            match ev {
                cocozero_rp2350::frontend::InputKeyEvent::Down(k) => soc.key_down(k),
                cocozero_rp2350::frontend::InputKeyEvent::Up(k) => soc.key_up(k),
            }
        }

        // Forward typed characters from window into the CoCo Zero keyboard matrix
        while let Some(ch) = frontend.typed_chars.pop_front() {
            eprintln!("[Input] Forwarding window char to SoC: {:?}", ch);
            soc.type_char(ch);
        }

        // Forward typed characters from terminal stdin
        while let Ok(ch) = stdin_rx.try_recv() {
            eprintln!("[Input] Forwarding terminal stdin char to SoC: {:?}", ch);
            soc.type_char(ch);
        }

        // Also drain legacy serial_input_queue if any
        while let Some(ch) = frontend.serial_input_queue.pop() {
            soc.push_serial_char(ch);
        }

        // Advance simulation by 1 frame (divided by turbo factor)
        if let Err(e) = soc.step_frame_turbo(args.turbo) {
            eprintln!("\nSimulation error at frame {}: {}", frame_count, e);
            break;
        }

        if !basic_ready_announced && soc.is_keyboard_ready() {
            basic_ready_announced = true;
            println!("\n[System Ready] Color BASIC initialization complete! Ready for keyboard input.");
        }

        // Extract and render framebuffer
        soc.extract_frame(&mut fb_buffer);
        let _ = frontend.render_frame(&fb_buffer);

        frame_count += 1;
        fps_counter += 1;

        if last_fps_time.elapsed().as_secs() >= 1 {
            let elapsed = last_fps_time.elapsed().as_secs_f64();
            let fps = fps_counter as f64 / elapsed;
            print!("\r[Emulating] Frame: {:>6} | Virt-FPS: {:>5.1} | Core 0 PC: 0x{:08X} | Core 1 PC: 0x{:08X}",
                frame_count,
                fps,
                soc.emu.core(0).regs.pc(),
                soc.emu.core(1).regs.pc()
            );
            let _ = std::io::stdout().flush();
            fps_counter = 0;
            last_fps_time = Instant::now();
        }

        if args.max_frames > 0 && frame_count >= args.max_frames {
            println!("\nReached maximum frames ({}); exiting.", args.max_frames);
            break;
        }
    }

    let total_elapsed = start_time.elapsed().as_secs_f64();
    println!(
        "\nEmulation finished: {} frames in {:.2}s ({:.1} avg FPS)",
        frame_count,
        total_elapsed,
        frame_count as f64 / total_elapsed.max(0.001)
    );
}
