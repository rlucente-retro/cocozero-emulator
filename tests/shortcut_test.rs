use cocozero_rp2350::frontend::{Frontend, JoystickEvent};
use cocozero_rp2350::keyboard::{K_A, KeyboardMatrix};
use rp2350_emu::bus::Bus;
use sdl2::controller::{Axis, Button};
use sdl2::keyboard::Keycode;

#[test]
fn test_frontend_shortcuts_and_gamepad() {
    let mut frontend = Frontend::new(true).expect("Headless frontend should initialize");

    // 1. Cross-platform Function key and macOS Command/Control shortcuts
    frontend.handle_key_down(Keycode::D, false, true, false, false);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x45),
        "Cmd+D -> Disks menu"
    );

    frontend.handle_key_down(Keycode::P, false, true, false, false);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x42),
        "Cmd+P -> Programs menu"
    );

    frontend.handle_key_down(Keycode::C, false, true, false, false);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x43),
        "Cmd+C -> Cartridges menu"
    );

    frontend.handle_key_down(Keycode::F11, false, false, false, false);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x44),
        "F11 -> Files menu"
    );

    frontend.handle_key_down(Keycode::I, false, true, false, false);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x3A),
        "Cmd+I -> Info menu"
    );

    frontend.handle_key_down(Keycode::A, false, true, false, false);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x41),
        "Cmd+A -> Artifact toggle"
    );

    frontend.handle_key_down(Keycode::PrintScreen, false, false, false, false);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x46),
        "PrtScn -> Screenshot"
    );

    // 2. Navigation and action shortcuts when menu is active
    frontend.handle_key_down(Keycode::Up, false, false, true, false);
    assert_eq!(frontend.hid_key_queue.pop_front(), Some(0x52), "Menu Up");

    frontend.handle_key_down(Keycode::Down, false, false, true, false);
    assert_eq!(frontend.hid_key_queue.pop_front(), Some(0x51), "Menu Down");

    frontend.handle_key_down(Keycode::Left, false, false, true, false);
    assert_eq!(frontend.hid_key_queue.pop_front(), Some(0x50), "Menu Left");

    frontend.handle_key_down(Keycode::Right, false, false, true, false);
    assert_eq!(frontend.hid_key_queue.pop_front(), Some(0x4F), "Menu Right");

    frontend.handle_key_down(Keycode::PageUp, false, false, true, false);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x4B),
        "Menu PageUp"
    );

    frontend.handle_key_down(Keycode::PageDown, false, false, true, false);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x4E),
        "Menu PageDown"
    );

    frontend.handle_key_down(Keycode::Return, false, false, true, false);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x28),
        "Menu Select"
    );

    frontend.handle_key_down(Keycode::Escape, false, false, true, false);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x29),
        "Menu Cancel/Escape"
    );

    frontend.handle_key_down(Keycode::Tab, false, false, true, false);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x2B),
        "Menu Tab (Edit .TXT)"
    );

    frontend.handle_key_down(Keycode::W, false, false, true, false);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x1A),
        "Menu W (Toggle Write Lock)"
    );

    frontend.handle_key_down(Keycode::N, false, false, true, false);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x11),
        "Menu N (Format Blank Disk)"
    );

    frontend.handle_key_down(Keycode::R, false, false, true, false);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x15),
        "Menu R (Rename Disk)"
    );

    frontend.handle_key_down(Keycode::Num0, false, false, true, false);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x27),
        "Menu Drive 0"
    );

    frontend.handle_key_down(Keycode::Num1, false, false, true, false);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x1E),
        "Menu Drive 1"
    );

    frontend.handle_key_down(Keycode::Num2, false, false, true, false);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x1F),
        "Menu Drive 2"
    );

    frontend.handle_key_down(Keycode::Num3, false, false, true, false);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x20),
        "Menu Drive 3"
    );

    // Dialog typing in menu
    frontend.handle_key_down(Keycode::A, false, false, true, false);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x04),
        "Menu Dialog 'A'"
    );

    frontend.handle_key_down(Keycode::Z, false, false, true, false);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x1D),
        "Menu Dialog 'Z'"
    );

    frontend.handle_key_down(Keycode::Backspace, false, false, true, false);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x2A),
        "Menu Dialog Backspace"
    );

    frontend.handle_key_down(Keycode::Space, false, false, true, false);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x2C),
        "Menu Dialog Space"
    );

    // 3. Gamepad in Menu Active mode
    frontend.handle_controller_button_down(Button::DPadUp, true);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x52),
        "Gamepad Menu Up"
    );

    frontend.handle_controller_button_down(Button::DPadDown, true);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x51),
        "Gamepad Menu Down"
    );

    frontend.handle_controller_button_down(Button::DPadLeft, true);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x50),
        "Gamepad Menu Left"
    );

    frontend.handle_controller_button_down(Button::DPadRight, true);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x4F),
        "Gamepad Menu Right"
    );

    frontend.handle_controller_button_down(Button::A, true);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x28),
        "Gamepad Menu A (Select)"
    );

    frontend.handle_controller_button_down(Button::B, true);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x29),
        "Gamepad Menu B (Close)"
    );

    frontend.handle_controller_button_down(Button::X, true);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x2B),
        "Gamepad Menu X (Tab)"
    );

    frontend.handle_controller_button_down(Button::LeftShoulder, true);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x4B),
        "Gamepad Menu L1 (PageUp)"
    );

    frontend.handle_controller_button_down(Button::RightShoulder, true);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x4E),
        "Gamepad Menu R1 (PageDown)"
    );

    // 4. Gamepad in Normal Mode
    frontend.handle_controller_button_down(Button::Guide, false);
    assert_eq!(
        frontend.hid_key_queue.pop_front(),
        Some(0x45),
        "Gamepad Guide opens Disks menu"
    );

    frontend.handle_controller_button_down(Button::Start, false);
    assert_eq!(
        frontend.typed_chars.pop_front(),
        Some('\r'),
        "Gamepad Start -> Enter"
    );

    frontend.handle_controller_button_down(Button::Y, false);
    assert_eq!(
        frontend.typed_chars.pop_front(),
        Some(' '),
        "Gamepad Y -> Space"
    );

    // Primary fire buttons -> Port 0 fire
    frontend.handle_controller_button_down(Button::A, false);
    assert_eq!(
        frontend.drain_joystick_events(),
        vec![JoystickEvent::Fire {
            port: 0,
            pressed: true
        }],
        "Gamepad A -> Port 0 fire down"
    );

    frontend.handle_controller_button_up(Button::A, false);
    assert_eq!(
        frontend.drain_joystick_events(),
        vec![JoystickEvent::Fire {
            port: 0,
            pressed: false
        }],
        "Gamepad A -> Port 0 fire up"
    );

    frontend.handle_controller_button_down(Button::B, false);
    frontend.handle_controller_button_down(Button::RightShoulder, false);
    assert_eq!(
        frontend.drain_joystick_events(),
        vec![
            JoystickEvent::Fire {
                port: 0,
                pressed: true
            },
            JoystickEvent::Fire {
                port: 0,
                pressed: true
            }
        ],
        "Gamepad B and R1 -> Port 0 fire"
    );

    // Secondary fire buttons -> Port 1 fire
    frontend.handle_controller_button_down(Button::X, false);
    frontend.handle_controller_button_down(Button::LeftShoulder, false);
    assert_eq!(
        frontend.drain_joystick_events(),
        vec![
            JoystickEvent::Fire {
                port: 1,
                pressed: true
            },
            JoystickEvent::Fire {
                port: 1,
                pressed: true
            }
        ],
        "Gamepad X and L1 -> Port 1 fire"
    );

    // 5. Gamepad Analog Sticks
    frontend.handle_controller_axis_motion(Axis::LeftX, -32768, false);
    frontend.handle_controller_axis_motion(Axis::LeftX, 32767, false);
    frontend.handle_controller_axis_motion(Axis::LeftX, 100, false); // In deadzone (< 1536)
    frontend.handle_controller_axis_motion(Axis::LeftY, -32768, false);
    frontend.handle_controller_axis_motion(Axis::RightX, 32767, false);
    frontend.handle_controller_axis_motion(Axis::RightY, -32768, false);

    assert_eq!(
        frontend.drain_joystick_events(),
        vec![
            JoystickEvent::Axis {
                port: 0,
                axis: 0,
                value: 0
            },
            JoystickEvent::Axis {
                port: 0,
                axis: 0,
                value: 65535
            },
            JoystickEvent::Axis {
                port: 0,
                axis: 0,
                value: 32767 // Deadzone centered
            },
            JoystickEvent::Axis {
                port: 0,
                axis: 1,
                value: 0
            },
            JoystickEvent::Axis {
                port: 1,
                axis: 0,
                value: 65535
            },
            JoystickEvent::Axis {
                port: 1,
                axis: 1,
                value: 0
            },
        ],
        "Gamepad analog axis sweeps map correctly"
    );

    // 6. Verify render_frame handles both 640x240 and 320x240
    let fb640 = vec![0u16; 640 * 240];
    assert!(
        frontend.render_frame(&fb640).is_ok(),
        "Render 640x240 frame succeeds"
    );
    let fb320 = [0x07E0u16; 320 * 240];
    assert!(
        frontend.render_frame(&fb320).is_ok(),
        "Render 320x240 frame succeeds (expanded 2x)"
    );
}

#[test]
fn test_turbo_debounce_invariance() {
    let mut bus = Bus::new();

    // Verify debounce scaling across different turbo multipliers (1, 2, 4)
    for turbo in [1u32, 2u32, 4u32] {
        let mut kbd = KeyboardMatrix::new();
        kbd.type_char('A');

        // Initial sync_frame: character is popped and asserted into typed_matrix (Frame 1)
        kbd.sync_frame(&mut bus, turbo, true);

        let hold_frames = turbo + 1;
        let gap_frames = (turbo * 2 / 3).max(1);

        // At 60 Hz virtual time, hold duration equals (turbo + 1) frames
        // We already stepped frame 1, so frames 2..hold_frames are the remaining hold frames
        let (row, col) = cocozero_rp2350::keyboard::dscan_to_row_col(K_A);
        for f in 1..hold_frames {
            let bus_val = bus.read8(kbd.matrix_addr + col as u32, 0);
            assert_eq!(
                bus_val & (1 << row),
                0,
                "Key 'A' must be asserted at frame {} (turbo={})",
                f,
                turbo
            );
            kbd.sync_frame(&mut bus, turbo, true);
        }

        // Stepping the final hold frame clears typed_matrix and transitions to gap
        kbd.sync_frame(&mut bus, turbo, true);
        let bus_val = bus.read8(kbd.matrix_addr + col as u32, 0);
        assert_ne!(
            bus_val & (1 << row),
            0,
            "Key 'A' must be released during gap period (turbo={})",
            turbo
        );

        // Release gap duration equals gap_frames
        // Step all gap frames
        for _ in 0..gap_frames {
            kbd.sync_frame(&mut bus, turbo, true);
        }

        // After gap duration completes, keyboard is ready for next key
        kbd.type_char('B');
        kbd.sync_frame(&mut bus, turbo, true);
        let (b_row, b_col) = cocozero_rp2350::keyboard::dscan_to_row_col(K_A + 1);
        let b_val = bus.read8(kbd.matrix_addr + b_col as u32, 0);
        assert_eq!(
            b_val & (1 << b_row),
            0,
            "Next key 'B' must latch after invariant gap (turbo={})",
            turbo
        );
    }
}

#[test]
fn test_multi_source_keyboard_layering() {
    let mut bus = Bus::new();
    let mut kbd = KeyboardMatrix::new();
    let src_addr = 0x2004_b81cu32;
    let ready_addr = 0x2006_0450u32;
    kbd.src_addr = Some(src_addr);
    kbd.src_ready_addr = Some(ready_addr);

    // Initial state: ready_addr is 0 (unready)
    assert_eq!(bus.read8(ready_addr, 0), 0);

    // Frame 1: sync_frame initializes g_kb_src (all 24 bytes 0xFF) and marks ready=1
    kbd.sync_frame(&mut bus, 1, true);
    assert_eq!(bus.read8(ready_addr, 0), 1);
    for i in 0..24 {
        assert_eq!(bus.read8(src_addr + i, 0), 0xFF);
    }
    for c in 0..8 {
        assert_eq!(bus.read8(kbd.matrix_addr + c as u32, 0), 0xFF);
    }

    // Simulate an external input source (e.g. Serial Autotype in Layer 1: bytes 8..15)
    // asserting CoCo Space (dscan K_SPACE = 0x2F).
    let (sp_row, sp_col) =
        cocozero_rp2350::keyboard::dscan_to_row_col(cocozero_rp2350::keyboard::K_SPACE);
    let orig_l1 = bus.read8(src_addr + 8 + sp_col as u32, 0);
    bus.write8(src_addr + 8 + sp_col as u32, orig_l1 & !(1 << sp_row), 0);

    // Now, user types 'A' on physical USB keyboard (Layer 0)
    kbd.type_char('A');
    kbd.sync_frame(&mut bus, 1, true);

    let (a_row, a_col) =
        cocozero_rp2350::keyboard::dscan_to_row_col(cocozero_rp2350::keyboard::K_A);

    // Layer 0 (USB) must have 'A' asserted
    let l0_val = bus.read8(src_addr + a_col as u32, 0);
    assert_eq!(l0_val & (1 << a_row), 0, "Layer 0 (USB) must hold key 'A'");

    // Layer 1 (Serial) must still have Space asserted
    let l1_val = bus.read8(src_addr + 8 + sp_col as u32, 0);
    assert_eq!(
        l1_val & (1 << sp_row),
        0,
        "Layer 1 (Serial) must hold Space"
    );

    // The combined matrix g_kb_col_row_mask must have BOTH keys asserted
    let mask_a = bus.read8(kbd.matrix_addr + a_col as u32, 0);
    assert_eq!(mask_a & (1 << a_row), 0, "Combined matrix must assert 'A'");
    let mask_sp = bus.read8(kbd.matrix_addr + sp_col as u32, 0);
    assert_eq!(
        mask_sp & (1 << sp_row),
        0,
        "Combined matrix must assert Space"
    );

    // Now simulate Serial Autotype releasing Space in Layer 1
    bus.write8(src_addr + 8 + sp_col as u32, 0xFF, 0);
    kbd.sync_frame(&mut bus, 1, true);

    // Verify key isolation (PIZERO-163): Releasing Layer 1 MUST NOT drop Layer 0's held key 'A'!
    let l0_after = bus.read8(src_addr + a_col as u32, 0);
    assert_eq!(
        l0_after & (1 << a_row),
        0,
        "Layer 0 must still hold 'A' after Layer 1 release"
    );
    let mask_after = bus.read8(kbd.matrix_addr + a_col as u32, 0);
    assert_eq!(
        mask_after & (1 << a_row),
        0,
        "Combined matrix must still assert 'A' after Layer 1 release"
    );
    let sp_after = bus.read8(kbd.matrix_addr + sp_col as u32, 0);
    assert_ne!(
        sp_after & (1 << sp_row),
        0,
        "Space in Layer 1 must now be released"
    );
}

#[test]
fn test_joystick_axis_and_fire_integration() {
    use cocozero_rp2350::soc::CoCoZeroSoC;
    use cocozero_rp2350::spi_sd::MemoryStorage;
    use cocozero_rp2350::symbols::FirmwareSymbols;
    use std::path::Path;

    // 1. Verify symbol resolution on CoCo 2 ELF
    let c2_elf = Path::new("roms/cocozero.elf");
    if c2_elf.is_file() {
        let elf_bytes = std::fs::read(c2_elf).expect("Read cocozero.elf");
        let syms = FirmwareSymbols::from_elf(&elf_bytes).expect("Parse cocozero.elf");
        assert_eq!(
            syms.joy_axis_addr,
            Some(0x2000_b2f4),
            "CoCo 2 joy_axis_addr in ELF"
        );
        assert_eq!(
            syms.joy_fire_addr,
            Some(0x2006_0443),
            "CoCo 2 joy_fire_addr in ELF"
        );
    }

    // 2. Verify symbol resolution on CoCo 3 ELF
    let c3_elf = Path::new("roms/cocozero-coco3.elf");
    if c3_elf.is_file() {
        let elf_bytes = std::fs::read(c3_elf).expect("Read cocozero-coco3.elf");
        let syms = FirmwareSymbols::from_elf(&elf_bytes).expect("Parse cocozero-coco3.elf");
        assert_eq!(
            syms.joy_axis_addr,
            Some(0x2000_f1f0),
            "CoCo 3 joy_axis_addr in ELF"
        );
        assert_eq!(
            syms.joy_fire_addr,
            Some(0x2006_45cf),
            "CoCo 3 joy_fire_addr in ELF"
        );
    }

    // 3. Verify in-flash signature scanning on cocozero.uf2
    let uf2_path = Path::new("roms/cocozero.uf2");
    if uf2_path.is_file() {
        let uf2_data = std::fs::read(uf2_path).expect("Read cocozero.uf2");
        let flash = cocozero_rp2350::uf2::parse_uf2(&uf2_data).expect("Parse UF2");
        let syms =
            FirmwareSymbols::from_flash_signatures(&flash.memory, cocozero_rp2350::uf2::FLASH_BASE);
        assert_eq!(
            syms.joy_axis_addr,
            Some(0x2000_b2f4),
            "Flash signature scan for joy_axis_addr"
        );
        assert_eq!(
            syms.joy_fire_addr,
            Some(0x2006_0443),
            "Flash signature scan for joy_fire_addr"
        );
    }

    // 4. Test CoCoZeroSoC joystick methods against simulated RP2350 SRAM
    let storage = Box::new(MemoryStorage::new(1024 * 1024));
    let mut soc = CoCoZeroSoC::new(storage).expect("Init SoC");

    let axis_addr = soc.symbols.joy_axis_addr.expect("joy_axis_addr present");
    let fire_addr = soc.symbols.joy_fire_addr.expect("joy_fire_addr present");

    // Center state initially
    soc.release_all_joysticks();

    for port in 0..2 {
        for axis in 0..2 {
            let offset = ((port * 2 + axis) * 2) as u32;
            let val = soc
                .emu
                .bus
                .memory
                .sram_read16(axis_addr + offset - 0x2000_0000);
            assert_eq!(val, 32767, "Initial axis [{}][{}] centered", port, axis);
        }
    }
    assert_eq!(
        soc.emu.bus.memory.sram_read8(fire_addr - 0x2000_0000),
        0,
        "Initial fire buttons released"
    );

    // Update Port 0 (Right Joystick): full left (0), full down (65535)
    soc.set_joystick_axis(0, 0, 0);
    soc.set_joystick_axis(0, 1, 65535);

    let p0_x = soc.emu.bus.memory.sram_read16(axis_addr - 0x2000_0000);
    let p0_y = soc.emu.bus.memory.sram_read16(axis_addr + 2 - 0x2000_0000);
    assert_eq!(p0_x, 0, "Port 0 X set to 0");
    assert_eq!(p0_y, 65535, "Port 0 Y set to 65535");

    // Update Port 1 (Left Joystick): custom values
    soc.set_joystick_axis(1, 0, 12345);
    soc.set_joystick_axis(1, 1, 54321);

    let p1_x = soc.emu.bus.memory.sram_read16(axis_addr + 4 - 0x2000_0000);
    let p1_y = soc.emu.bus.memory.sram_read16(axis_addr + 6 - 0x2000_0000);
    assert_eq!(p1_x, 12345, "Port 1 X set to 12345");
    assert_eq!(p1_y, 54321, "Port 1 Y set to 54321");

    // Test Fire Buttons:
    // Assert Port 0 Fire (PA0)
    soc.set_joystick_fire(0, true);
    assert_eq!(
        soc.emu.bus.memory.sram_read8(fire_addr - 0x2000_0000),
        0x01,
        "Port 0 Fire asserted (bit 0)"
    );

    // Assert Port 1 Fire (PA1) concurrently
    soc.set_joystick_fire(1, true);
    assert_eq!(
        soc.emu.bus.memory.sram_read8(fire_addr - 0x2000_0000),
        0x03,
        "Both Fire buttons asserted (bits 0 and 1)"
    );

    // Release Port 0 Fire
    soc.set_joystick_fire(0, false);
    assert_eq!(
        soc.emu.bus.memory.sram_read8(fire_addr - 0x2000_0000),
        0x02,
        "Only Port 1 Fire remains asserted"
    );

    // Release all joysticks
    soc.release_all_joysticks();
    assert_eq!(
        soc.emu.bus.memory.sram_read8(fire_addr - 0x2000_0000),
        0x00,
        "Fire buttons cleared"
    );
    for port in 0..2 {
        for axis in 0..2 {
            let offset = ((port * 2 + axis) * 2) as u32;
            let val = soc
                .emu
                .bus
                .memory
                .sram_read16(axis_addr + offset - 0x2000_0000);
            assert_eq!(
                val, 32767,
                "Axis [{}][{}] re-centered after release",
                port, axis
            );
        }
    }
}

#[test]
fn test_coco3_wide_mode_and_640_display() {
    use cocozero_rp2350::soc::{CoCoZeroSoC, FB_PIXELS_640, FB_WIDTH_640};
    use cocozero_rp2350::spi_sd::MemoryStorage;
    use std::path::Path;

    let storage = Box::new(MemoryStorage::new(1024 * 1024));
    let mut soc = CoCoZeroSoC::new(storage).expect("Init SoC");

    // 1. By default, CoCoZeroSoC initializes with CoCo 2 symbols
    assert!(!soc.is_coco3(), "Default SoC is CoCo 2");
    assert!(!soc.is_wide_mode(), "CoCo 2 is never wide mode");

    // 2. Switch to CoCo 3 configuration
    soc.symbols.kb_matrix_addr = 0x2000_f200;
    soc.symbols.c3bus_addr = Some(0x2001_0860);
    soc.symbols.gime3_ptr_addr = Some(0x2004_bbf0);
    assert!(soc.is_coco3(), "SoC now recognized as CoCo 3");

    // 3. Set up simulated GIME data structures in SRAM
    let gime_ptr_addr = 0x2004_bbf0;
    let gime_struct_addr = 0x2003_0000;
    let c3bus_addr = 0x2001_0860;
    let ram_lo_addr = 0x2005_0000;
    let ram_hi_addr = 0x2006_0000;

    // Pointer to gime in SRAM
    soc.emu
        .bus
        .memory
        .sram_write32(gime_ptr_addr - 0x2000_0000, gime_struct_addr);

    // c3bus structure in SRAM: ram_lo (+0), ram_hi (+4), rgb64 (+24)
    soc.emu
        .bus
        .memory
        .sram_write32(c3bus_addr - 0x2000_0000, ram_lo_addr);
    soc.emu
        .bus
        .memory
        .sram_write32(c3bus_addr + 4 - 0x2000_0000, ram_hi_addr);

    // Fill rgb64 table in c3bus: Color 0 = Black, Color 1 = White (0xFFFF), Color 2 = Green (0x07E0)
    soc.emu
        .bus
        .memory
        .sram_write16(c3bus_addr + 24 - 0x2000_0000, 0x0000);
    soc.emu
        .bus
        .memory
        .sram_write16(c3bus_addr + 26 - 0x2000_0000, 0xFFFF);
    soc.emu
        .bus
        .memory
        .sram_write16(c3bus_addr + 28 - 0x2000_0000, 0x07E0);

    // Set GIME palette registers (offset 0x148)
    // pal16[0] = 0 (Black), pal16[1] = 1 (White), pal16[10] = 2 (Border: Green)
    soc.emu
        .bus
        .memory
        .sram_write8(gime_struct_addr + 0x148 - 0x2000_0000, 0);
    soc.emu
        .bus
        .memory
        .sram_write8(gime_struct_addr + 0x148 + 1 - 0x2000_0000, 1);
    soc.emu
        .bus
        .memory
        .sram_write8(gime_struct_addr + 0x148 + 10 - 0x2000_0000, 2);

    // Set GIME registers at offset 0xFC:
    // reg8 ($FF98, VMODE) at offset +0x104: Text mode (bp = 0), lines per row (lpr = 2 -> 8 lines per row)
    soc.emu
        .bus
        .memory
        .sram_write8(gime_struct_addr + 0x104 - 0x2000_0000, 0x02);
    // reg9 ($FF99, VRES) at offset +0x105: HRES = 4 (80 columns: (4 << 2) = 0x10)
    soc.emu
        .bus
        .memory
        .sram_write8(gime_struct_addr + 0x105 - 0x2000_0000, 0x10);
    // reg10 ($FF9A, BRDR) at offset +0x106: border color palette 10
    soc.emu
        .bus
        .memory
        .sram_write8(gime_struct_addr + 0x106 - 0x2000_0000, 10);
    // reg13 ($FF9D, VOFF MSB) and reg14 ($FF9E, VOFF LSB): VRAM offset = 0
    soc.emu
        .bus
        .memory
        .sram_write8(gime_struct_addr + 0x109 - 0x2000_0000, 0);
    soc.emu
        .bus
        .memory
        .sram_write8(gime_struct_addr + 0x10A - 0x2000_0000, 0);

    // Verify wide mode is now true
    assert!(
        soc.is_wide_mode(),
        "80-column text mode triggers is_wide_mode"
    );

    // If menu is active, is_wide_mode must be false to let overlay display
    soc.emu
        .bus
        .memory
        .sram_write8(soc.symbols.menu_active_addr - 0x2000_0000, 1);
    assert!(
        !soc.is_wide_mode(),
        "Menu overlay deactivates wide mode for menu render"
    );
    soc.emu
        .bus
        .memory
        .sram_write8(soc.symbols.menu_active_addr - 0x2000_0000, 0);
    assert!(soc.is_wide_mode(), "Wide mode active after menu closes");

    // 4. Populate VRAM row 0 with known characters: 'A' (ASCII 65) across all 80 columns
    let ram_lo_offset = ram_lo_addr - 0x2000_0000;
    for col in 0..80 {
        soc.emu
            .bus
            .memory
            .sram_write8(ram_lo_offset + col as u32, 65);
    }

    // 5. Test extract_frame_640
    let mut fb640 = vec![0u16; FB_PIXELS_640];
    assert!(soc.extract_frame_640(&mut fb640));

    // Glyph for 'A' in FONT_GIME: row 0 is top of 'A'
    // Characters are drawn starting at row 0 at y_start = (240 - 192)/2 = 24.
    // In scanline 24 (y = 24), verify character pixels:
    // Every 8th column is a character cell. In each character cell, pixels match FONT_GIME['A'*12].
    let font_a_row0 = cocozero_rp2350::font_gime::FONT_GIME[65 * 12];
    for col in 0..80 {
        let base_x = col * 8;
        for bit in 0..8 {
            let px = fb640[24 * FB_WIDTH_640 + base_x + bit];
            let expected_white = (font_a_row0 & (1 << (7 - bit))) != 0;
            if expected_white {
                assert_eq!(
                    px, 0xFFFF,
                    "Pixel bit {} of col {} matches 'A' glyph",
                    bit, col
                );
            } else {
                assert_eq!(px, 0x0000, "Background bit {} of col {} is Black", bit, col);
            }
        }
    }

    // 6. Test BMP dump in 640-wide mode produces correct 640x240 dimensions
    let tmp_bmp = Path::new("tests/tmp_wide_test.bmp");
    soc.dump_framebuffer_bmp(tmp_bmp)
        .expect("Dump 640-wide BMP");
    assert!(tmp_bmp.is_file(), "BMP file created");
    let bmp_bytes = std::fs::read(tmp_bmp).expect("Read BMP");
    let width = u32::from_le_bytes(bmp_bytes[18..22].try_into().unwrap());
    let height = u32::from_le_bytes(bmp_bytes[22..26].try_into().unwrap());
    assert_eq!(width, 640, "BMP width is 640");
    assert_eq!(height, 240, "BMP height is 240");
    let _ = std::fs::remove_file(tmp_bmp);
}
