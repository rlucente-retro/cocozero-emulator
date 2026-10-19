use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use cocozero_rp2350::fat32::{
    PARTITION_START_LBA, SECTOR_SIZE, TOTAL_SECTORS, VirtualFat32Storage,
};
use cocozero_rp2350::soc::CoCoZeroSoC;
use cocozero_rp2350::spi_sd::{SD_SECTOR_SIZE, SectorStorage};

/// Helper to create a unique temporary directory for testing.
struct TestTempDir {
    path: PathBuf,
}

impl TestTempDir {
    fn new(prefix: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}_{nanos}"));
        fs::create_dir_all(&path).expect("Failed to create test temp dir");
        Self { path }
    }

    fn path(&self) -> &PathBuf {
        &self.path
    }
}

impl Drop for TestTempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn test_virtual_fat32_storage_read_and_geometry() {
    let temp = TestTempDir::new("cocozero_storage_geom");
    let roms_dir = temp.path().join("roms");
    fs::create_dir_all(&roms_dir).unwrap();
    fs::write(roms_dir.join("bas12.rom"), vec![0x12; 8192]).unwrap();

    let mut storage =
        VirtualFat32Storage::from_directory(temp.path()).expect("Should build storage");

    assert_eq!(storage.num_sectors(), TOTAL_SECTORS as u64);
    assert!(!storage.is_dirty());
    assert_eq!(storage.dirty_count(), 0);

    // Read MBR (Sector 0)
    let mut mbr = [0u8; SD_SECTOR_SIZE];
    assert!(storage.read_sector(0, &mut mbr));
    assert_eq!(mbr[510], 0x55);
    assert_eq!(mbr[511], 0xAA);

    // Read VBR (Sector 2048)
    let mut vbr = [0u8; SD_SECTOR_SIZE];
    assert!(storage.read_sector(PARTITION_START_LBA as u64, &mut vbr));
    assert_eq!(vbr[510], 0x55);
    assert_eq!(vbr[511], 0xAA);
    assert_eq!(&vbr[82..90], b"FAT32   ");
}

#[test]
fn test_virtual_fat32_write_sector_and_host_flush() {
    let temp = TestTempDir::new("cocozero_storage_write");
    let roms_dir = temp.path().join("roms");
    fs::create_dir_all(&roms_dir).unwrap();
    fs::write(roms_dir.join("bas12.rom"), vec![0x39; 8192]).unwrap();
    fs::write(temp.path().join("settings.txt"), b"video_mode=60\n").unwrap();

    let mut storage =
        VirtualFat32Storage::from_directory(temp.path()).expect("Should build storage");

    // Locate the sector containing settings.txt
    let needle = b"video_mode=60\n";
    let data = storage.data();
    let pos = data
        .windows(needle.len())
        .position(|window| window == needle)
        .expect("settings.txt data must be present in image");

    let sector = (pos / SECTOR_SIZE) as u64;
    let byte_offset_in_sector = pos % SECTOR_SIZE;

    // Read sector, modify payload to video_mode=50\n, and write back
    let mut sector_buf = [0u8; SD_SECTOR_SIZE];
    assert!(storage.read_sector(sector, &mut sector_buf));
    let new_payload = b"video_mode=50\n";
    sector_buf[byte_offset_in_sector..byte_offset_in_sector + new_payload.len()]
        .copy_from_slice(new_payload);

    assert!(storage.write_sector(sector, &sector_buf));
    assert!(storage.is_dirty());
    assert_eq!(storage.dirty_count(), 1);

    // Flush modified storage to host
    assert!(storage.flush());
    assert!(!storage.is_dirty());
    assert_eq!(storage.dirty_count(), 0);

    // Verify host file was updated
    let host_content = fs::read(temp.path().join("settings.txt")).unwrap();
    assert_eq!(host_content, b"video_mode=50\n");
}

#[test]
fn test_virtual_fat32_case_insensitive_preservation() {
    let temp = TestTempDir::new("cocozero_storage_case");
    let dsk_dir = temp.path().join("dsk");
    fs::create_dir_all(&dsk_dir).unwrap();
    let host_file = dsk_dir.join("my_disk.dsk");
    fs::write(&host_file, vec![0xAA; 512]).unwrap();

    let mut storage =
        VirtualFat32Storage::from_directory(temp.path()).expect("Should build storage");

    // Modify file in virtual FAT32
    let pos = storage
        .data()
        .windows(512)
        .position(|w| w == [0xAA; 512])
        .expect("Payload must exist");

    let sector = (pos / SECTOR_SIZE) as u64;
    let new_data = [0xBBu8; SD_SECTOR_SIZE];
    assert!(storage.write_sector(sector, &new_data));
    assert!(storage.flush());

    // Verify host file was updated in-place with existing casing preserved
    let updated = fs::read(&host_file).unwrap();
    assert_eq!(updated, vec![0xBB; 512]);

    // Ensure no uppercase duplicate was created
    assert!(!dsk_dir.join("MY_DISK.DSK").is_symlink());
    if cfg!(target_os = "linux") {
        assert!(!dsk_dir.join("MY_DISK.DSK").exists());
    }
}

#[test]
fn test_virtual_fat32_new_file_writeback() {
    let temp = TestTempDir::new("cocozero_storage_new_file");
    let roms_dir = temp.path().join("roms");
    fs::create_dir_all(&roms_dir).unwrap();
    fs::write(roms_dir.join("bas12.rom"), vec![0x39; 8192]).unwrap();

    let mut storage =
        VirtualFat32Storage::from_directory(temp.path()).expect("Should build storage");

    // Find the COCO directory cluster offset
    // In build_image_from_tree, COCO directory cluster is cluster 3
    // cluster 3 LBA = 4128 + (3 - 2) * 8 = 4136
    let cluster3_lba = 4128 + 8;
    let cluster3_off = cluster3_lba * SECTOR_SIZE;

    // Scan for first unused directory entry (all zeros) in cluster 3
    let mut unused_entry_offset = None;
    for offset in (cluster3_off..cluster3_off + 4096).step_by(32) {
        if storage.data()[offset] == 0x00 {
            unused_entry_offset = Some(offset);
            break;
        }
    }
    let entry_off = unused_entry_offset.expect("Unused entry slot in COCO directory");

    // Allocate cluster 4 for the new file (DRIVES.TXT)
    let new_file_cluster = 4u32;
    let cluster4_lba = 4128 + (4 - 2) * 8;
    let cluster4_off = cluster4_lba * SECTOR_SIZE;
    let file_data = b"drive0=game.dsk\n";

    // Write file payload into cluster 4
    let file_sector = (cluster4_off / SECTOR_SIZE) as u64;
    let mut sector_data = [0u8; SD_SECTOR_SIZE];
    sector_data[..file_data.len()].copy_from_slice(file_data);
    storage.write_sector(file_sector, &sector_data);

    // Update FAT1 and FAT2 for cluster 4 -> 0x0FFF_FFFF
    let fat1_off = 2080 * SECTOR_SIZE + (new_file_cluster as usize) * 4;
    let fat1_sec = (fat1_off / SECTOR_SIZE) as u64;
    let mut fat_buf = [0u8; SD_SECTOR_SIZE];
    storage.read_sector(fat1_sec, &mut fat_buf);
    let fat_byte_off = fat1_off % SECTOR_SIZE;
    fat_buf[fat_byte_off..fat_byte_off + 4].copy_from_slice(&0x0FFF_FFFFu32.to_le_bytes());
    storage.write_sector(fat1_sec, &fat_buf);

    let fat2_off = 3104 * SECTOR_SIZE + (new_file_cluster as usize) * 4;
    let fat2_sec = (fat2_off / SECTOR_SIZE) as u64;
    storage.read_sector(fat2_sec, &mut fat_buf);
    let fat2_byte_off = fat2_off % SECTOR_SIZE;
    fat_buf[fat2_byte_off..fat2_byte_off + 4].copy_from_slice(&0x0FFF_FFFFu32.to_le_bytes());
    storage.write_sector(fat2_sec, &fat_buf);

    // Construct 32-byte directory entry for DRIVES.TXT
    let mut entry = [0u8; 32];
    entry[0..11].copy_from_slice(b"DRIVES  TXT");
    entry[11] = 0x20; // Archive file
    entry[20..22].copy_from_slice(&((new_file_cluster >> 16) as u16).to_le_bytes());
    entry[26..28].copy_from_slice(&(new_file_cluster as u16).to_le_bytes());
    entry[28..32].copy_from_slice(&(file_data.len() as u32).to_le_bytes());

    let dir_sec = (entry_off / SECTOR_SIZE) as u64;
    let dir_byte_off = entry_off % SECTOR_SIZE;
    let mut dir_sec_buf = [0u8; SD_SECTOR_SIZE];
    storage.read_sector(dir_sec, &mut dir_sec_buf);
    dir_sec_buf[dir_byte_off..dir_byte_off + 32].copy_from_slice(&entry);
    storage.write_sector(dir_sec, &dir_sec_buf);

    // Flush and verify that drives.txt is created under host root
    assert!(storage.flush());
    let host_drives = temp.path().join("drives.txt");
    assert!(host_drives.exists(), "drives.txt must be created on host");
    let content = fs::read(&host_drives).unwrap();
    assert_eq!(content, file_data);
}

#[test]
fn test_soc_flush_storage_integration() {
    let temp = TestTempDir::new("cocozero_soc_storage");
    let roms_dir = temp.path().join("roms");
    fs::create_dir_all(&roms_dir).unwrap();
    fs::write(roms_dir.join("bas12.rom"), vec![0x12; 8192]).unwrap();
    fs::write(temp.path().join("settings.txt"), b"turbo=1\n").unwrap();

    let storage = VirtualFat32Storage::from_directory(temp.path()).expect("Should build storage");
    let mut soc = CoCoZeroSoC::new(Box::new(storage)).expect("Should create SoC");

    // Modify a sector through the SoC's SD card controller
    let needle = b"turbo=1\n";

    // Use direct backing write to simulate CMD24 sector modification
    {
        let mut sd = soc.sd_card.lock().unwrap();
        let mut buf = [0u8; SD_SECTOR_SIZE];
        // Find position in backing data
        let mut target_sec = 0u64;
        for s in 4128..4200 {
            if sd.backing.read_sector(s, &mut buf)
                && let Some(pos) = buf.windows(needle.len()).position(|w| w == needle)
            {
                buf[pos..pos + needle.len()].copy_from_slice(b"turbo=2\n");
                sd.backing.write_sector(s, &buf);
                target_sec = s;
                break;
            }
        }
        assert!(target_sec > 0, "Must have found and modified sector");
    }

    // Call flush_storage on SoC
    assert!(soc.flush_storage());

    // Verify host file was updated
    let updated = fs::read(temp.path().join("settings.txt")).unwrap();
    assert_eq!(updated, b"turbo=2\n");
}
