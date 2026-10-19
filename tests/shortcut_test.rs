use cocozero_rp2350::frontend::{Frontend, InputKeyEvent};
use cocozero_rp2350::keyboard::{K_A, KeyboardMatrix};
use rp2350_emu::bus::Bus;
use sdl2::controller::Button;
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

    frontend.handle_controller_button_down(Button::A, false);
    assert_eq!(
        frontend.drain_key_events(),
        vec![InputKeyEvent::Down(Keycode::Space)],
        "Gamepad A -> Physical Space down"
    );

    frontend.handle_controller_button_up(Button::A, false);
    assert_eq!(
        frontend.drain_key_events(),
        vec![InputKeyEvent::Up(Keycode::Space)],
        "Gamepad A -> Physical Space up"
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
