use cocozero_rp2350::fat32::build_virtual_fat32;
use cocozero_rp2350::soc::{CoCoZeroSoC, FB_HEIGHT, FB_PIXELS, FB_WIDTH};
use cocozero_rp2350::spi_sd::MemoryStorage;
use cocozero_rp2350::uf2::parse_uf2;
use std::path::Path;

#[test]
fn test_cocozero_framebuffer_output() {
    if !Path::new("coco/roms/bas12.rom").exists() {
        eprintln!("Skipping test_cocozero_framebuffer_output: coco/roms/bas12.rom not present (user-supplied ROM required)");
        return;
    }

    let sd_path = Path::new("coco");
    let sd_bytes = build_virtual_fat32(sd_path).expect("Failed to build virtual FAT32");
    let storage = Box::new(MemoryStorage::from_bytes(sd_bytes));

    let mut soc = CoCoZeroSoC::new(storage).expect("Failed to create SoC");
    soc.load_bootrom(&std::fs::read("roms/rp2350/bootrom-combined.bin").expect("bootrom"));
    soc.fb_addr = Some(0x2002_2744);

    let uf2_data = std::fs::read("roms/cocozero.uf2").expect("Failed to read cocozero.uf2");
    let flash = parse_uf2(&uf2_data).expect("Failed to parse UF2");
    soc.load_firmware(&flash);

    // Run for up to 180 frames (allow 6809 boot, RAM test, and prompt to display)
    let mut fb = [0u16; FB_PIXELS];
    for f in 0..180 {
        soc.step_frame().expect("Should step frame");
        if f % 15 == 0 {
            println!("Frame {}: c0 PC={:#x}, c1 PC={:#x}, wfe0={}, wfe1={}",
                f, soc.emu.core(0).regs.pc(), soc.emu.core(1).regs.pc(),
                soc.emu.core(0).is_wfe_waiting(), soc.emu.core(1).is_wfe_waiting());
        }
    }
    soc.extract_frame(&mut fb);

    // Count distinct pixel colors
    let mut colors = std::collections::HashSet::new();
    for &p in fb.iter() {
        colors.insert(p);
    }
    println!("Distinct colors in framebuffer after 180 frames: {}", colors.len());
    println!("Sample colors: {:?}", colors.iter().take(5).collect::<Vec<_>>());

    let history = soc.sd_card.lock().unwrap().cmd_history.clone();
    println!("SD Command History ({} commands): {:?}", history.len(), history.iter().take(20).collect::<Vec<_>>());
    let tx_bytes = soc.serial_tx_log.lock().unwrap().clone();
    println!("Total UART output ({} bytes):\n{}", tx_bytes.len(), String::from_utf8_lossy(&tx_bytes));

    // Verify Color Computer 2 screen rendered:
    assert!(colors.contains(&2016), "Framebuffer must contain authentic Color Computer green (0x07E0)");
    assert!(colors.contains(&0), "Framebuffer must contain black text/border pixels (0x0000)");
    assert!(colors.contains(&31), "Framebuffer must contain blue cursor pixels (0x001F)");

    // Write BMP images so we can view the rendered output!
    write_bmp("tests/frame_60.bmp", &fb, FB_WIDTH, FB_HEIGHT);
    write_bmp("tests/frame_180.bmp", &fb, FB_WIDTH, FB_HEIGHT);
}

fn write_bmp(path: &str, fb: &[u16], width: usize, height: usize) {
    let mut bmp = Vec::new();
    let row_padding = (4 - (width * 3) % 4) % 4;
    let image_size = (width * 3 + row_padding) * height;
    let file_size = 54 + image_size;

    // BMP Header
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&(file_size as u32).to_le_bytes());
    bmp.extend_from_slice(&0u32.to_le_bytes()); // Reserved
    bmp.extend_from_slice(&54u32.to_le_bytes()); // Data offset

    // DIB Header (BITMAPINFOHEADER)
    bmp.extend_from_slice(&40u32.to_le_bytes()); // Header size
    bmp.extend_from_slice(&(width as i32).to_le_bytes());
    bmp.extend_from_slice(&(height as i32).to_le_bytes()); // Bottom-up
    bmp.extend_from_slice(&1u16.to_le_bytes()); // Planes
    bmp.extend_from_slice(&24u16.to_le_bytes()); // Bits per pixel (24-bit RGB)
    bmp.extend_from_slice(&0u32.to_le_bytes()); // Compression (BI_RGB)
    bmp.extend_from_slice(&(image_size as u32).to_le_bytes());
    bmp.extend_from_slice(&2835u32.to_le_bytes()); // Horizontal resolution (72 DPI)
    bmp.extend_from_slice(&2835u32.to_le_bytes()); // Vertical resolution (72 DPI)
    bmp.extend_from_slice(&0u32.to_le_bytes()); // Colors
    bmp.extend_from_slice(&0u32.to_le_bytes()); // Important colors

    // Pixel data (bottom-up in standard BMP)
    for y in (0..height).rev() {
        for x in 0..width {
            let pixel565 = fb[y * width + x];
            let r = (((pixel565 >> 11) & 0x1F) as u32 * 255 / 31) as u8;
            let g = (((pixel565 >> 5) & 0x3F) as u32 * 255 / 63) as u8;
            let b = ((pixel565 & 0x1F) as u32 * 255 / 31) as u8;
            bmp.push(b); // B
            bmp.push(g); // G
            bmp.push(r); // R
        }
        for _ in 0..row_padding {
            bmp.push(0);
        }
    }

    std::fs::write(path, bmp).expect("Failed to write BMP");
    println!("Saved rendered framebuffer to {}", path);
}
