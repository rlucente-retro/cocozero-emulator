use clap::Parser;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use cocozero_rp2350::fat32::VirtualFat32Storage;
use cocozero_rp2350::frontend::Frontend;

use cocozero_rp2350::soc::{CoCoZeroSoC, FB_PIXELS_640};
use cocozero_rp2350::spi_sd::{FileStorage, MemoryStorage, SectorStorage};
use cocozero_rp2350::uf2::{FlashImage, parse_uf2};

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

    /// Optional path to firmware ELF binary (.elf) for dynamic symbol extraction
    #[arg(long)]
    elf: Option<PathBuf>,

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
    #[arg(
        short = 'f',
        long = "frames",
        alias = "max-frames",
        default_value_t = 0
    )]
    frames: u64,

    /// Simulation turbo factor (1 = 252 MHz, 2 = 2x speedup / 126 MHz virtual frame quantum, 4 = 4x speedup, etc.)
    #[arg(long, default_value_t = 2)]
    turbo: u32,

    /// Override framebuffer SRAM address (e.g. 0x20010000)
    #[arg(long)]
    fb_addr: Option<String>,

    /// Enqueue ASCII text to be typed into Color BASIC once initialization is complete
    #[arg(long = "type")]
    type_str: Option<String>,

    /// Path to a text script file to be typed into Color BASIC once initialization is complete
    #[arg(long = "script")]
    script: Option<PathBuf>,

    /// Dump framebuffer to a 24-bit BMP file at the specified frame number
    #[arg(long)]
    dump_frame: Option<u64>,

    /// Destination file path for dumped framebuffer BMP (default: frame_<N>.bmp)
    #[arg(long)]
    dump_path: Option<PathBuf>,

    /// Enable periodic diagnostic telemetry reports
    #[arg(long)]
    telemetry: bool,

    /// Interval in frames between periodic telemetry reports (default: 60)
    #[arg(long, default_value_t = 60)]
    telemetry_interval: u64,

    /// Periodic SD card storage flush interval in frames (default: 300)
    #[arg(long, default_value_t = 300)]
    sync_interval: u64,

    /// Audio buffer latency budget in milliseconds (default: 100)
    #[arg(long, default_value_t = 100)]
    audio_buffer_ms: u32,
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
            println!(
                "Mounting directory as virtual FAT32 SD card: {}",
                path.display()
            );
            Box::new(
                VirtualFat32Storage::from_directory(path).expect("Failed to build virtual FAT32"),
            )
        }
        _ => {
            let default_dir = Path::new("coco");
            if default_dir.is_dir() {
                println!("Auto-mounting local ./coco directory as virtual FAT32 SD card...");
                Box::new(
                    VirtualFat32Storage::from_directory(default_dir)
                        .expect("Failed to build virtual FAT32"),
                )
            } else {
                println!("No SD card specified; initializing empty 64 MB virtual card");
                Box::new(MemoryStorage::new(64 * 1024 * 1024))
            }
        }
    };

    // 2. Initialize SoC
    println!("Initializing RP2350B SoC engine...");
    let mut soc = CoCoZeroSoC::new(sd_storage).expect("Failed to initialize SoC");

    // 3. Load Boot ROM if present
    if args.bootrom.exists() {
        println!("Loading RP2350 Boot ROM: {}", args.bootrom.display());
        let rom_data = std::fs::read(&args.bootrom).expect("Failed to read Boot ROM");
        soc.load_bootrom(&rom_data);
    } else {
        println!(
            "Notice: Boot ROM {} not found; using direct flash boot",
            args.bootrom.display()
        );
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
        let companion_elf = uf2_path.with_extension("elf");
        let effective_elf = args.elf.as_deref().or_else(|| {
            if companion_elf.is_file() {
                Some(companion_elf.as_path())
            } else {
                None
            }
        });
        soc.load_firmware_with_elf(&flash, effective_elf);
    } else if let Some(bin_path) = &args.bin {
        println!("Loading raw binary: {}", bin_path.display());
        let bin_data = std::fs::read(bin_path).expect("Failed to read binary file");
        let mut flash = FlashImage::new(16 * 1024 * 1024);
        flash.load_raw_bin(0, &bin_data);
        soc.load_firmware_with_elf(&flash, args.elf.as_deref());
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
            soc.load_firmware_with_elf(&flash, args.elf.as_deref());
        } else {
            println!("Warning: No firmware (--uf2 or --bin) specified.");
            println!("Running Boot ROM idle/USB boot loop.");
        }
    }

    if let Some(fb_str) = &args.fb_addr {
        let clean = fb_str.trim_start_matches("0x").trim_start_matches("0X");
        if let Ok(addr) = u32::from_str_radix(clean, 16) {
            println!(
                "Explicit CLI override: forcing framebuffer address to 0x{:08X}",
                addr
            );
            soc.fb_addr = Some(addr);
        }
    }

    // 5. Initialize Frontend
    let mut frontend = Frontend::new(args.headless).expect("Failed to initialize SDL2 frontend");
    frontend.set_audio_buffer_ms(args.audio_buffer_ms);
    println!("SDL2 frontend initialized successfully.");
    if !args.headless {
        println!("Display window opened: {}x{} @ 60 Hz", 640, 480);
    }

    // 6. Main Emulation Loop
    println!("Starting RP2350B execution loop...");
    let mut fb_buffer = vec![0u16; FB_PIXELS_640];
    let mut frame_count = 0u64;
    let start_time = Instant::now();
    let mut last_fps_time = Instant::now();
    let mut fps_counter = 0;
    let mut had_error = false;

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

    let target_frame_duration = std::time::Duration::from_nanos(1_000_000_000 / 60);
    let mut basic_ready_announced = false;

    loop {
        let frame_start = Instant::now();

        // Poll frontend input events (pass current menu active state)
        let is_menu_active = soc.is_menu_active();
        if !frontend.poll_events(is_menu_active) {
            println!("\nExit requested by user.");
            break;
        }

        // Drain HID keycodes for menu operation (F1..F12, navigation, drive selection)
        while let Some(hid_code) = frontend.hid_key_queue.pop_front() {
            soc.inject_hid_keycode(hid_code);
        }

        // Drain joystick events (analog axes and fire buttons)
        for ev in frontend.drain_joystick_events() {
            match ev {
                cocozero_rp2350::frontend::JoystickEvent::Axis { port, axis, value } => {
                    soc.set_joystick_axis(port, axis, value);
                }
                cocozero_rp2350::frontend::JoystickEvent::Fire { port, pressed } => {
                    soc.set_joystick_fire(port, pressed);
                }
            }
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
            had_error = true;
            break;
        }

        if !basic_ready_announced && soc.is_keyboard_ready() {
            basic_ready_announced = true;
            println!(
                "\n[System Ready] Color BASIC initialization complete! Ready for keyboard input."
            );

            // Inject scripted commands from file if specified
            if let Some(script_path) = &args.script {
                match std::fs::read_to_string(script_path) {
                    Ok(content) => {
                        println!(
                            "[Script] Injecting commands from {}:",
                            script_path.display()
                        );
                        for line in content.lines() {
                            let trimmed = line.trim_end();
                            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                                println!("  > {}", trimmed);
                                soc.type_str(trimmed);
                                soc.type_char('\r');
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "[Script] Failed to read script file {}: {}",
                            script_path.display(),
                            e
                        );
                    }
                }
            }

            // Inject directly specified --type text if provided
            if let Some(text) = &args.type_str {
                println!("[Script] Injecting command text: {:?}", text);
                let unescaped = text.replace("\\n", "\n").replace("\\r", "\r");
                for ch in unescaped.chars() {
                    let mapped = match ch {
                        '\n' => '\r',
                        c => c,
                    };
                    soc.type_char(mapped);
                }
            }
        }

        // Extract and render framebuffer
        soc.extract_frame_640(&mut fb_buffer);
        let _ = frontend.render_frame(&fb_buffer);

        // Screen dump capture if requested at current frame
        if let Some(target) = args.dump_frame
            && frame_count == target
        {
            let out_path = args
                .dump_path
                .clone()
                .unwrap_or_else(|| PathBuf::from(format!("frame_{}.bmp", target)));
            match soc.dump_framebuffer_bmp(&out_path) {
                Ok(()) => println!(
                    "\n[Screen Dump] Captured frame {} to {}",
                    target,
                    out_path.display()
                ),
                Err(e) => eprintln!("\n[Screen Dump] Failed to dump frame {}: {}", target, e),
            }
        }

        // Extract and push audio samples to frontend
        let audio_samples = soc.extract_audio();
        if !audio_samples.is_empty() {
            frontend.push_audio(&audio_samples);
        }

        frame_count += 1;
        fps_counter += 1;

        // Periodic sync to host every sync_interval frames if dirty
        if args.sync_interval > 0 && frame_count.is_multiple_of(args.sync_interval) {
            soc.flush_storage();
        }

        // Periodic diagnostic telemetry logging if enabled
        if args.telemetry && frame_count.is_multiple_of(args.telemetry_interval) {
            println!("{}", soc.telemetry());
        }

        if last_fps_time.elapsed().as_secs() >= 1 {
            let elapsed = last_fps_time.elapsed().as_secs_f64();
            let fps = fps_counter as f64 / elapsed;
            print!(
                "\r[Emulating] Frame: {:>6} | Virt-FPS: {:>5.1} | Core 0 PC: 0x{:08X} | Core 1 PC: 0x{:08X}",
                frame_count,
                fps,
                soc.emu.core(0).regs.pc(),
                soc.emu.core(1).regs.pc()
            );
            let _ = std::io::stdout().flush();
            fps_counter = 0;
            last_fps_time = Instant::now();
        }

        if args.frames > 0 && frame_count >= args.frames {
            println!(
                "\nReached maximum frames ({}); exiting cleanly.",
                args.frames
            );
            break;
        }

        if !args.headless {
            let elapsed = frame_start.elapsed();
            if elapsed < target_frame_duration {
                std::thread::sleep(target_frame_duration - elapsed);
            }
        }
    }

    let total_elapsed = start_time.elapsed().as_secs_f64();
    println!(
        "\nEmulation finished: {} frames in {:.2}s ({:.1} avg FPS)",
        frame_count,
        total_elapsed,
        frame_count as f64 / total_elapsed.max(0.001)
    );

    if args.telemetry {
        println!("\n--- Final Diagnostic Telemetry ---");
        println!("{}", soc.telemetry());
    }

    // Flush any pending SD card sector writes to host storage on exit
    soc.flush_storage();

    if had_error {
        std::process::exit(1);
    }
}
