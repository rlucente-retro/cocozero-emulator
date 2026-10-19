//! Simple FAT32 disk image builder and extractor from a host directory tree.
//!
//! Generates a valid MBR and FAT32 filesystem in memory that the SD card
//! emulator can serve to the firmware, with bidirectional writeback
//! tracking and host directory synchronization.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::spi_sd::{SD_SECTOR_SIZE, SectorStorage};

pub const SECTOR_SIZE: usize = 512;
pub const SECTORS_PER_CLUSTER: usize = 8; // 4 KB clusters
pub const CLUSTER_SIZE: usize = SECTOR_SIZE * SECTORS_PER_CLUSTER;
pub const RESERVED_SECTORS: usize = 32;
pub const NUM_FATS: usize = 2;
pub const PARTITION_START_LBA: usize = 2048; // 1 MB alignment
pub const SECTORS_PER_FAT: usize = 1024;
pub const TOTAL_SECTORS: usize = 532_480; // 260 MB virtual disk size

#[derive(Debug, Clone)]
pub enum VEntry {
    File { name: String, data: Vec<u8> },
    Dir { name: String, entries: Vec<VEntry> },
}

/// Extracted file metadata and content parsed from a FAT32 filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedFile {
    pub path_components: Vec<String>,
    pub data: Vec<u8>,
}

/// Virtual FAT32 in-memory storage backed by a host directory tree.
///
/// Holds the synthetic FAT32 image in memory for ultra-fast single-cycle access,
/// tracks modified sectors, and synchronizes updated and new files back to the
/// host filesystem on flush or drop.
pub struct VirtualFat32Storage {
    data: Vec<u8>,
    host_root: PathBuf,
    wrapped_in_coco: bool,
    dirty_sectors: HashSet<usize>,
}

impl VirtualFat32Storage {
    /// Construct a virtual FAT32 image from the given host directory tree.
    pub fn from_directory<P: AsRef<Path>>(host_root: P) -> Result<Self, String> {
        let host_root = host_root.as_ref().to_path_buf();
        let mut tree = read_host_tree(&host_root)?;
        let has_coco = tree.iter().any(|e| match e {
            VEntry::Dir { name, .. } => name.eq_ignore_ascii_case("coco"),
            _ => false,
        });
        let mut wrapped_in_coco = false;
        if !has_coco {
            let is_coco_dir = host_root
                .file_name()
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("coco"));
            let has_roms = tree.iter().any(|e| match e {
                VEntry::Dir { name, .. } => name.eq_ignore_ascii_case("roms"),
                _ => false,
            });
            if is_coco_dir || has_roms {
                tree = vec![VEntry::Dir {
                    name: "COCO".to_string(),
                    entries: tree,
                }];
                wrapped_in_coco = true;
            }
        }
        let data = build_image_from_tree(tree)?;
        Ok(Self {
            data,
            host_root,
            wrapped_in_coco,
            dirty_sectors: HashSet::new(),
        })
    }

    /// Check if any sectors have been modified since the last flush.
    pub fn is_dirty(&self) -> bool {
        !self.dirty_sectors.is_empty()
    }

    /// Number of modified sectors pending writeback.
    pub fn dirty_count(&self) -> usize {
        self.dirty_sectors.len()
    }

    /// Access host root path.
    pub fn host_root(&self) -> &Path {
        &self.host_root
    }

    /// Check whether the root tree was wrapped in a synthetic /COCO/ directory.
    pub fn is_wrapped_in_coco(&self) -> bool {
        self.wrapped_in_coco
    }

    /// Raw disk byte slice.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Mutable raw disk byte slice.
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// Consume storage and return raw disk buffer.
    pub fn into_data(mut self) -> Vec<u8> {
        let _ = self.flush();
        std::mem::take(&mut self.data)
    }

    /// Manually mark a sector as modified.
    pub fn mark_sector_dirty(&mut self, sector: usize) {
        self.dirty_sectors.insert(sector);
    }

    /// Synchronize modified and newly created files in FAT32 back to the host filesystem.
    pub fn sync_to_host(&mut self) -> Result<(), String> {
        let extracted_files = extract_fat32_files(&self.data)?;

        for file in extracted_files {
            let components: &[String] = if self.wrapped_in_coco
                && !file.path_components.is_empty()
                && file.path_components[0].eq_ignore_ascii_case("coco")
            {
                &file.path_components[1..]
            } else {
                &file.path_components[..]
            };

            if components.is_empty() {
                continue;
            }

            let target_path = resolve_case_insensitive_path(&self.host_root, components);

            let needs_write = match fs::read(&target_path) {
                Ok(existing) => existing != file.data,
                Err(_) => true,
            };

            if needs_write {
                if let Some(parent) = target_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| {
                        format!("Failed to create directory {}: {e}", parent.display())
                    })?;
                }
                fs::write(&target_path, &file.data)
                    .map_err(|e| format!("Failed to write {}: {e}", target_path.display()))?;
            }
        }

        self.dirty_sectors.clear();
        Ok(())
    }
}

impl SectorStorage for VirtualFat32Storage {
    fn num_sectors(&self) -> u64 {
        (self.data.len() / SD_SECTOR_SIZE) as u64
    }

    fn read_sector(&mut self, sector: u64, out: &mut [u8; SD_SECTOR_SIZE]) -> bool {
        let offset = (sector as usize) * SD_SECTOR_SIZE;
        if offset + SD_SECTOR_SIZE <= self.data.len() {
            out.copy_from_slice(&self.data[offset..offset + SD_SECTOR_SIZE]);
            true
        } else {
            false
        }
    }

    fn write_sector(&mut self, sector: u64, data: &[u8; SD_SECTOR_SIZE]) -> bool {
        let offset = (sector as usize) * SD_SECTOR_SIZE;
        if offset + SD_SECTOR_SIZE <= self.data.len() {
            self.data[offset..offset + SD_SECTOR_SIZE].copy_from_slice(data);
            self.dirty_sectors.insert(sector as usize);
            true
        } else {
            false
        }
    }

    fn flush(&mut self) -> bool {
        if self.dirty_sectors.is_empty() || self.data.is_empty() {
            return true;
        }
        match self.sync_to_host() {
            Ok(()) => true,
            Err(e) => {
                eprintln!("VirtualFat32Storage flush failed: {e}");
                false
            }
        }
    }
}

impl Drop for VirtualFat32Storage {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

/// Helper to build raw virtual FAT32 bytes from a host directory path.
pub fn build_virtual_fat32(host_root: &Path) -> Result<Vec<u8>, String> {
    VirtualFat32Storage::from_directory(host_root).map(VirtualFat32Storage::into_data)
}

fn read_host_tree(path: &Path) -> Result<Vec<VEntry>, String> {
    let mut entries = Vec::new();
    if !path.exists() {
        return Ok(entries);
    }

    let read_dir = fs::read_dir(path)
        .map_err(|e| format!("Failed to read directory {}: {}", path.display(), e))?;

    for entry in read_dir {
        let entry = entry.map_err(|e| e.to_string())?;
        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        let file_name = entry.file_name().to_string_lossy().to_string();

        if file_name.starts_with('.') {
            continue;
        }

        if file_type.is_dir() {
            let sub = read_host_tree(&entry.path())?;
            entries.push(VEntry::Dir {
                name: file_name,
                entries: sub,
            });
        } else if file_type.is_file() {
            let data = fs::read(entry.path())
                .map_err(|e| format!("Failed to read {}: {}", entry.path().display(), e))?;
            entries.push(VEntry::File {
                name: file_name,
                data,
            });
        }
    }
    Ok(entries)
}

fn to_short_name(name: &str) -> [u8; 11] {
    let mut short = [b' '; 11];
    let parts: Vec<&str> = name.split('.').collect();
    let base = parts[0].to_uppercase();
    let base_bytes = base.as_bytes();
    let base_len = base_bytes.len().min(8);
    short[0..base_len].copy_from_slice(&base_bytes[0..base_len]);

    if parts.len() > 1 {
        let ext = parts[parts.len() - 1].to_uppercase();
        let ext_bytes = ext.as_bytes();
        let ext_len = ext_bytes.len().min(3);
        short[8..8 + ext_len].copy_from_slice(&ext_bytes[0..ext_len]);
    }
    short
}

/// Build a raw 260 MB FAT32 disk image byte array from a directory tree structure.
pub fn build_image_from_tree(root_entries: Vec<VEntry>) -> Result<Vec<u8>, String> {
    let mut disk = vec![0u8; TOTAL_SECTORS * SECTOR_SIZE];

    // 1. MBR partition table at sector 0
    disk[510] = 0x55;
    disk[511] = 0xAA;
    // Partition entry 1 at 0x1BE:
    // Bootable (0x80), Type = 0x0C (FAT32 LBA)
    disk[0x1BE] = 0x80; // Active
    disk[0x1C2] = 0x0C; // FAT32 LBA
    let lba_start = (PARTITION_START_LBA as u32).to_le_bytes();
    disk[0x1C6..0x1CA].copy_from_slice(&lba_start);
    let part_sectors = ((TOTAL_SECTORS - PARTITION_START_LBA) as u32).to_le_bytes();
    disk[0x1CA..0x1CE].copy_from_slice(&part_sectors);

    // 2. FAT32 Boot Record at PARTITION_START_LBA
    let bpb_offset = PARTITION_START_LBA * SECTOR_SIZE;
    let bpb = &mut disk[bpb_offset..bpb_offset + SECTOR_SIZE];
    bpb[0..3].copy_from_slice(&[0xEB, 0x58, 0x90]); // Jump instruction
    bpb[3..11].copy_from_slice(b"MSDOS5.0");
    bpb[11..13].copy_from_slice(&(SECTOR_SIZE as u16).to_le_bytes()); // Bytes per sector (512)
    bpb[13] = SECTORS_PER_CLUSTER as u8; // Sectors per cluster (8)
    bpb[14..16].copy_from_slice(&(RESERVED_SECTORS as u16).to_le_bytes()); // Reserved sectors (32)
    bpb[16] = NUM_FATS as u8; // Number of FATs (2)
    bpb[21] = 0xF8; // Media descriptor
    bpb[24..26].copy_from_slice(&63u16.to_le_bytes()); // Sectors per track
    bpb[26..28].copy_from_slice(&255u16.to_le_bytes()); // Number of heads
    bpb[28..32].copy_from_slice(&(PARTITION_START_LBA as u32).to_le_bytes()); // Hidden sectors (preceding partition)
    let total_part_sectors = (TOTAL_SECTORS - PARTITION_START_LBA) as u32;
    bpb[32..36].copy_from_slice(&total_part_sectors.to_le_bytes()); // Large sector count

    let sectors_per_fat = SECTORS_PER_FAT as u32;
    bpb[36..40].copy_from_slice(&sectors_per_fat.to_le_bytes()); // Sectors per FAT
    bpb[44..48].copy_from_slice(&2u32.to_le_bytes()); // Root cluster = 2
    bpb[66] = 0x29; // Extended boot signature
    bpb[67..71].copy_from_slice(&0x12345678u32.to_le_bytes()); // Volume ID
    bpb[71..82].copy_from_slice(b"COCOZERO   "); // Volume label
    bpb[82..90].copy_from_slice(b"FAT32   "); // FS type
    bpb[510] = 0x55;
    bpb[511] = 0xAA;

    // FAT and Data Area geometry
    let fat1_lba = PARTITION_START_LBA + RESERVED_SECTORS;
    let fat2_lba = fat1_lba + sectors_per_fat as usize;
    let cluster2_lba = fat2_lba + sectors_per_fat as usize;

    let mut fat_table: BTreeMap<u32, u32> = BTreeMap::new();
    fat_table.insert(0, 0x0FFF_FFF8);
    fat_table.insert(1, 0x0FFF_FFFF);

    // Recursively write directory tree into clusters
    #[allow(clippy::too_many_arguments)]
    fn allocate_dir(
        entries: &[VEntry],
        is_root: bool,
        parent_cluster: u32,
        current_cluster: u32,
        next_free_cluster: &mut u32,
        fat: &mut BTreeMap<u32, u32>,
        disk: &mut [u8],
        cluster2_lba: usize,
    ) {
        let mut curr_dir_cluster = current_cluster;
        let mut cluster_offset =
            (cluster2_lba + (curr_dir_cluster as usize - 2) * SECTORS_PER_CLUSTER) * SECTOR_SIZE;
        let mut dir_bytes = vec![0u8; CLUSTER_SIZE];
        let mut entry_idx = 0;

        if !is_root {
            // "." entry
            let mut dot = [0u8; 32];
            dot[0..11].copy_from_slice(b".          ");
            dot[11] = 0x10; // Directory
            let cl_lo = (current_cluster as u16).to_le_bytes();
            let cl_hi = ((current_cluster >> 16) as u16).to_le_bytes();
            dot[20..22].copy_from_slice(&cl_hi);
            dot[26..28].copy_from_slice(&cl_lo);
            dir_bytes[0..32].copy_from_slice(&dot);
            entry_idx += 1;

            // ".." entry
            let mut dotdot = [0u8; 32];
            dotdot[0..11].copy_from_slice(b"..         ");
            dotdot[11] = 0x10;
            let pcl_lo = (parent_cluster as u16).to_le_bytes();
            let pcl_hi = ((parent_cluster >> 16) as u16).to_le_bytes();
            dotdot[20..22].copy_from_slice(&pcl_hi);
            dotdot[26..28].copy_from_slice(&pcl_lo);
            dir_bytes[32..64].copy_from_slice(&dotdot);
            entry_idx += 1;
        }

        for entry in entries {
            if entry_idx * 32 + 32 > CLUSTER_SIZE {
                // Flush full cluster and chain to next
                disk[cluster_offset..cluster_offset + CLUSTER_SIZE].copy_from_slice(&dir_bytes);
                let next_dir_cl = *next_free_cluster;
                *next_free_cluster += 1;
                fat.insert(curr_dir_cluster, next_dir_cl);
                curr_dir_cluster = next_dir_cl;
                cluster_offset = (cluster2_lba
                    + (curr_dir_cluster as usize - 2) * SECTORS_PER_CLUSTER)
                    * SECTOR_SIZE;
                dir_bytes.fill(0);
                entry_idx = 0;
            }

            let mut rec = [0u8; 32];
            match entry {
                VEntry::File { name, data } => {
                    rec[0..11].copy_from_slice(&to_short_name(name));
                    rec[11] = 0x20; // Archive / file
                    let file_size = data.len() as u32;
                    rec[28..32].copy_from_slice(&file_size.to_le_bytes());

                    if file_size > 0 {
                        let start_cl = *next_free_cluster;
                        *next_free_cluster += 1;
                        let cl_lo = (start_cl as u16).to_le_bytes();
                        let cl_hi = ((start_cl >> 16) as u16).to_le_bytes();
                        rec[20..22].copy_from_slice(&cl_hi);
                        rec[26..28].copy_from_slice(&cl_lo);

                        // Write file clusters
                        let mut remaining = &data[..];
                        let mut curr_cl = start_cl;
                        while !remaining.is_empty() {
                            let take = remaining.len().min(CLUSTER_SIZE);
                            let cl_off = (cluster2_lba
                                + (curr_cl as usize - 2) * SECTORS_PER_CLUSTER)
                                * SECTOR_SIZE;
                            disk[cl_off..cl_off + take].copy_from_slice(&remaining[..take]);
                            remaining = &remaining[take..];

                            if !remaining.is_empty() {
                                let next_cl = *next_free_cluster;
                                *next_free_cluster += 1;
                                fat.insert(curr_cl, next_cl);
                                curr_cl = next_cl;
                            } else {
                                fat.insert(curr_cl, 0x0FFF_FFFF);
                            }
                        }
                    }
                }
                VEntry::Dir { name, entries } => {
                    rec[0..11].copy_from_slice(&to_short_name(name));
                    rec[11] = 0x10; // Directory
                    let sub_cl = *next_free_cluster;
                    *next_free_cluster += 1;
                    let cl_lo = (sub_cl as u16).to_le_bytes();
                    let cl_hi = ((sub_cl >> 16) as u16).to_le_bytes();
                    rec[20..22].copy_from_slice(&cl_hi);
                    rec[26..28].copy_from_slice(&cl_lo);

                    allocate_dir(
                        entries,
                        false,
                        curr_dir_cluster,
                        sub_cl,
                        next_free_cluster,
                        fat,
                        disk,
                        cluster2_lba,
                    );
                }
            }
            dir_bytes[entry_idx * 32..(entry_idx + 1) * 32].copy_from_slice(&rec);
            entry_idx += 1;
        }

        fat.insert(curr_dir_cluster, 0x0FFF_FFFF);
        disk[cluster_offset..cluster_offset + CLUSTER_SIZE].copy_from_slice(&dir_bytes);
    }

    let mut next_cluster = 3u32;
    allocate_dir(
        &root_entries,
        true,
        0,
        2,
        &mut next_cluster,
        &mut fat_table,
        &mut disk,
        cluster2_lba,
    );

    // Flush FAT table to FAT1 and FAT2
    let fat1_offset = fat1_lba * SECTOR_SIZE;
    let fat2_offset = fat2_lba * SECTOR_SIZE;
    for (&cluster, &next) in &fat_table {
        let entry_offset = (cluster as usize) * 4;
        let bytes = next.to_le_bytes();
        disk[fat1_offset + entry_offset..fat1_offset + entry_offset + 4].copy_from_slice(&bytes);
        disk[fat2_offset + entry_offset..fat2_offset + entry_offset + 4].copy_from_slice(&bytes);
    }

    Ok(disk)
}

/// Extract all files and directories from a raw FAT32 disk image.
pub fn extract_fat32_files(disk: &[u8]) -> Result<Vec<ExtractedFile>, String> {
    if disk.len() < PARTITION_START_LBA * SECTOR_SIZE + SECTOR_SIZE {
        return Err("Disk image is too small to contain a FAT32 partition".to_string());
    }

    // Determine partition start (check MBR at sector 0)
    let partition_lba = if disk.len() >= 512 && disk[510] == 0x55 && disk[511] == 0xAA {
        let lba = u32::from_le_bytes(
            disk[0x1C6..0x1CA]
                .try_into()
                .map_err(|e| format!("Slice conversion error: {e}"))?,
        ) as usize;
        if lba > 0 && lba * SECTOR_SIZE < disk.len() {
            lba
        } else {
            PARTITION_START_LBA
        }
    } else {
        PARTITION_START_LBA
    };

    let bpb_off = partition_lba * SECTOR_SIZE;
    if bpb_off + SECTOR_SIZE > disk.len() {
        return Err("BPB sector offset out of bounds".to_string());
    }
    if disk[bpb_off + 510] != 0x55 || disk[bpb_off + 511] != 0xAA {
        return Err("Invalid boot record signature".to_string());
    }

    let bytes_per_sector = u16::from_le_bytes([disk[bpb_off + 11], disk[bpb_off + 12]]) as usize;
    if bytes_per_sector != 512 {
        return Err(format!("Unsupported bytes per sector: {bytes_per_sector}"));
    }

    let sectors_per_cluster = disk[bpb_off + 13] as usize;
    if sectors_per_cluster == 0 || !sectors_per_cluster.is_power_of_two() {
        return Err(format!(
            "Invalid sectors per cluster: {sectors_per_cluster}"
        ));
    }

    let reserved_sectors = u16::from_le_bytes([disk[bpb_off + 14], disk[bpb_off + 15]]) as usize;
    let num_fats = disk[bpb_off + 16] as usize;
    if num_fats == 0 {
        return Err("Invalid number of FATs (0)".to_string());
    }

    let sectors_per_fat = u32::from_le_bytes(
        disk[bpb_off + 36..bpb_off + 40]
            .try_into()
            .map_err(|e| format!("Slice conversion error: {e}"))?,
    ) as usize;
    if sectors_per_fat == 0 {
        return Err("Invalid sectors per FAT (0)".to_string());
    }

    let root_cluster = u32::from_le_bytes(
        disk[bpb_off + 44..bpb_off + 48]
            .try_into()
            .map_err(|e| format!("Slice conversion error: {e}"))?,
    );
    if root_cluster < 2 {
        return Err(format!("Invalid root cluster: {root_cluster}"));
    }

    let cluster_size = bytes_per_sector * sectors_per_cluster;
    let fat1_lba = partition_lba + reserved_sectors;
    let fat1_offset = fat1_lba * bytes_per_sector;
    let data_start_lba = fat1_lba + num_fats * sectors_per_fat;

    let mut out_files = Vec::new();
    let mut visited_dir_clusters = HashSet::new();

    traverse_dir(
        disk,
        fat1_offset,
        data_start_lba,
        bytes_per_sector,
        sectors_per_cluster,
        cluster_size,
        root_cluster,
        Vec::new(),
        &mut out_files,
        &mut visited_dir_clusters,
    )?;

    Ok(out_files)
}

#[allow(clippy::too_many_arguments)]
fn traverse_dir(
    disk: &[u8],
    fat1_offset: usize,
    data_start_lba: usize,
    bytes_per_sector: usize,
    sectors_per_cluster: usize,
    cluster_size: usize,
    start_cluster: u32,
    current_path: Vec<String>,
    out_files: &mut Vec<ExtractedFile>,
    visited_dir_clusters: &mut HashSet<u32>,
) -> Result<(), String> {
    let mut curr_cluster = start_cluster;
    let mut lfn_parts: BTreeMap<u8, Vec<u16>> = BTreeMap::new();

    while (2..0x0FFF_FFF8).contains(&curr_cluster) {
        if !visited_dir_clusters.insert(curr_cluster) {
            break; // Directory cycle detected
        }
        let cl_lba = data_start_lba + (curr_cluster as usize - 2) * sectors_per_cluster;
        let cl_offset = cl_lba * bytes_per_sector;
        if cl_offset + cluster_size > disk.len() {
            return Err("Cluster offset exceeds disk boundary".to_string());
        }

        let cl_data = &disk[cl_offset..cl_offset + cluster_size];
        let mut end_of_dir = false;

        for chunk in cl_data.as_chunks::<32>().0 {
            let first_byte = chunk[0];
            if first_byte == 0x00 {
                end_of_dir = true;
                break;
            }
            if first_byte == 0xE5 {
                lfn_parts.clear();
                continue;
            }

            let attr = chunk[11];
            if attr == 0x0F {
                let ord = chunk[0];
                if (ord & 0x40) != 0 {
                    lfn_parts.clear();
                }
                let seq = ord & 0x1F;
                let mut u16_chars = Vec::with_capacity(13);
                for i in (1..11).step_by(2) {
                    u16_chars.push(u16::from_le_bytes([chunk[i], chunk[i + 1]]));
                }
                for i in (14..26).step_by(2) {
                    u16_chars.push(u16::from_le_bytes([chunk[i], chunk[i + 1]]));
                }
                for i in (28..32).step_by(2) {
                    u16_chars.push(u16::from_le_bytes([chunk[i], chunk[i + 1]]));
                }
                lfn_parts.insert(seq, u16_chars);
                continue;
            }

            if (attr & 0x08) != 0 {
                lfn_parts.clear();
                continue;
            }

            let name = if !lfn_parts.is_empty() {
                let mut u16_all = Vec::new();
                for chars in lfn_parts.values() {
                    for &ch in chars {
                        if ch == 0x0000 || ch == 0xFFFF {
                            break;
                        }
                        u16_all.push(ch);
                    }
                }
                lfn_parts.clear();
                String::from_utf16_lossy(&u16_all)
            } else {
                let mut base_bytes = chunk[0..8].to_vec();
                if base_bytes[0] == 0x05 {
                    base_bytes[0] = 0xE5;
                }
                let ntres = chunk[12];
                let mut base = String::from_utf8_lossy(&base_bytes).trim_end().to_string();
                let mut ext = String::from_utf8_lossy(&chunk[8..11])
                    .trim_end()
                    .to_string();
                if (ntres & 0x08) != 0 {
                    base = base.to_ascii_lowercase();
                }
                if (ntres & 0x10) != 0 {
                    ext = ext.to_ascii_lowercase();
                }
                if ext.is_empty() {
                    base
                } else {
                    format!("{base}.{ext}")
                }
            };

            if name == "." || name == ".." || name.is_empty() {
                continue;
            }

            let cl_hi = u16::from_le_bytes([chunk[20], chunk[21]]) as u32;
            let cl_lo = u16::from_le_bytes([chunk[26], chunk[27]]) as u32;
            let entry_cluster = (cl_hi << 16) | cl_lo;

            if (attr & 0x10) != 0 {
                // Subdirectory
                if entry_cluster >= 2 {
                    let mut sub_path = current_path.clone();
                    sub_path.push(name);
                    traverse_dir(
                        disk,
                        fat1_offset,
                        data_start_lba,
                        bytes_per_sector,
                        sectors_per_cluster,
                        cluster_size,
                        entry_cluster,
                        sub_path,
                        out_files,
                        visited_dir_clusters,
                    )?;
                }
            } else {
                // Regular file
                let file_size = u32::from_le_bytes(
                    chunk[28..32]
                        .try_into()
                        .map_err(|e| format!("Slice conversion error: {e}"))?,
                ) as usize;
                let file_data = read_file_data(
                    disk,
                    fat1_offset,
                    data_start_lba,
                    bytes_per_sector,
                    sectors_per_cluster,
                    cluster_size,
                    entry_cluster,
                    file_size,
                )?;
                let mut file_path = current_path.clone();
                file_path.push(name);
                out_files.push(ExtractedFile {
                    path_components: file_path,
                    data: file_data,
                });
            }
        }

        if end_of_dir {
            break;
        }

        let fat_off = fat1_offset + (curr_cluster as usize) * 4;
        if fat_off + 4 > disk.len() {
            break;
        }
        curr_cluster = u32::from_le_bytes(
            disk[fat_off..fat_off + 4]
                .try_into()
                .map_err(|e| format!("Slice conversion error: {e}"))?,
        ) & 0x0FFF_FFFF;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn read_file_data(
    disk: &[u8],
    fat1_offset: usize,
    data_start_lba: usize,
    bytes_per_sector: usize,
    sectors_per_cluster: usize,
    cluster_size: usize,
    start_cluster: u32,
    file_size: usize,
) -> Result<Vec<u8>, String> {
    if file_size == 0 || start_cluster < 2 {
        return Ok(Vec::new());
    }

    let mut data = Vec::with_capacity(file_size);
    let mut curr_cl = start_cluster;
    let mut visited = HashSet::new();

    while (2..0x0FFF_FFF8).contains(&curr_cl) && data.len() < file_size {
        if !visited.insert(curr_cl) {
            return Err("Cluster cycle detected while reading file".to_string());
        }
        let cl_lba = data_start_lba + (curr_cl as usize - 2) * sectors_per_cluster;
        let cl_off = cl_lba * bytes_per_sector;
        if cl_off + cluster_size > disk.len() {
            return Err("Cluster offset exceeds disk size".to_string());
        }
        let needed = file_size - data.len();
        let take = needed.min(cluster_size);
        data.extend_from_slice(&disk[cl_off..cl_off + take]);

        let fat_off = fat1_offset + (curr_cl as usize) * 4;
        if fat_off + 4 > disk.len() {
            break;
        }
        curr_cl = u32::from_le_bytes(
            disk[fat_off..fat_off + 4]
                .try_into()
                .map_err(|e| format!("Slice conversion error: {e}"))?,
        ) & 0x0FFF_FFFF;
    }

    Ok(data)
}

fn resolve_case_insensitive_path(base: &Path, components: &[String]) -> PathBuf {
    let mut current = base.to_path_buf();
    for comp in components {
        let mut matched = None;
        if let Ok(entries) = fs::read_dir(&current) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.eq_ignore_ascii_case(comp) {
                    matched = Some(name_str.into_owned());
                    break;
                }
            }
        }
        match matched {
            Some(existing_name) => current.push(existing_name),
            None => {
                if comp.eq_ignore_ascii_case("drives.txt")
                    || comp.eq_ignore_ascii_case("settings.txt")
                    || comp.eq_ignore_ascii_case("autorun.txt")
                {
                    current.push(comp.to_ascii_lowercase());
                } else {
                    current.push(comp);
                }
            }
        }
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_virtual_fat32_builder_and_extractor() {
        let entries = vec![
            VEntry::File {
                name: "SETTINGS.TXT".to_string(),
                data: b"video_mode=60\n".to_vec(),
            },
            VEntry::Dir {
                name: "COCO".to_string(),
                entries: vec![
                    VEntry::File {
                        name: "BAS12.ROM".to_string(),
                        data: vec![0x39; 8192],
                    },
                    VEntry::File {
                        name: "EMPTY.DAT".to_string(),
                        data: Vec::new(),
                    },
                ],
            },
        ];

        let img = build_image_from_tree(entries).expect("Should build valid image");
        assert_eq!(img.len(), TOTAL_SECTORS * SECTOR_SIZE);

        // Check MBR signature
        assert_eq!(img[510], 0x55);
        assert_eq!(img[511], 0xAA);

        // Check VBR signature at 2048 * 512
        let vbr_off = PARTITION_START_LBA * SECTOR_SIZE;
        assert_eq!(img[vbr_off + 510], 0x55);
        assert_eq!(img[vbr_off + 511], 0xAA);
        assert_eq!(&img[vbr_off + 82..vbr_off + 90], b"FAT32   ");

        // Extract files back from image
        let extracted = extract_fat32_files(&img).expect("Should extract files");
        assert_eq!(extracted.len(), 3);

        let settings = extracted
            .iter()
            .find(|f| f.path_components == vec!["SETTINGS.TXT"])
            .expect("SETTINGS.TXT must be found");
        assert_eq!(settings.data, b"video_mode=60\n");

        let rom = extracted
            .iter()
            .find(|f| f.path_components == vec!["COCO", "BAS12.ROM"])
            .expect("BAS12.ROM must be found");
        assert_eq!(rom.data, vec![0x39; 8192]);

        let empty = extracted
            .iter()
            .find(|f| f.path_components == vec!["COCO", "EMPTY.DAT"])
            .expect("EMPTY.DAT must be found");
        assert!(empty.data.is_empty());
    }
}
