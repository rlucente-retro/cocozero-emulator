//! Simple FAT32 disk image builder from a host directory tree.
//!
//! Generates a valid MBR and FAT32 filesystem in memory that the SD card
//! emulator can serve to the firmware.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

const SECTOR_SIZE: usize = 512;
const SECTORS_PER_CLUSTER: usize = 8; // 4 KB clusters
const CLUSTER_SIZE: usize = SECTOR_SIZE * SECTORS_PER_CLUSTER;
const RESERVED_SECTORS: usize = 32;
const NUM_FATS: usize = 2;
const PARTITION_START_LBA: usize = 2048; // 1 MB alignment

#[derive(Debug, Clone)]
pub enum VEntry {
    File { name: String, data: Vec<u8> },
    Dir { name: String, entries: Vec<VEntry> },
}


pub fn build_virtual_fat32(host_root: &Path) -> Result<Vec<u8>, String> {
    let mut tree = read_host_tree(host_root)?;
    let has_coco = tree.iter().any(|e| match e {
        VEntry::Dir { name, .. } => name.eq_ignore_ascii_case("coco"),
        _ => false,
    });
    if !has_coco {
        let is_coco_dir = host_root.file_name().map_or(false, |n| n.to_string_lossy().eq_ignore_ascii_case("coco"));
        let has_roms = tree.iter().any(|e| match e {
            VEntry::Dir { name, .. } => name.eq_ignore_ascii_case("roms"),
            _ => false,
        });
        if is_coco_dir || has_roms {
            tree = vec![VEntry::Dir {
                name: "COCO".to_string(),
                entries: tree,
            }];
        }
    }
    build_image_from_tree(tree)
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

pub fn build_image_from_tree(root_entries: Vec<VEntry>) -> Result<Vec<u8>, String> {
    // 260 MB virtual disk size (532,480 sectors) to ensure cluster count
    // exceeds Microsoft / FatFS MAX_FAT16 (65,525 clusters).
    let total_sectors = 532480;
    let mut disk = vec![0u8; total_sectors * SECTOR_SIZE];

    // 1. MBR partition table at sector 0
    // Signature
    disk[510] = 0x55;
    disk[511] = 0xAA;
    // Partition entry 1 at 0x1BE:
    // Bootable (0x80), Type = 0x0C (FAT32 LBA)
    disk[0x1BE] = 0x80; // Active
    disk[0x1C2] = 0x0C; // FAT32 LBA
    let lba_start = (PARTITION_START_LBA as u32).to_le_bytes();
    disk[0x1C6..0x1CA].copy_from_slice(&lba_start);
    let part_sectors = ((total_sectors - PARTITION_START_LBA) as u32).to_le_bytes();
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
    let total_part_sectors = (total_sectors - PARTITION_START_LBA) as u32;
    bpb[32..36].copy_from_slice(&total_part_sectors.to_le_bytes()); // Large sector count

    let sectors_per_fat = 1024u32;
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
        fat.insert(current_cluster, 0x0FFF_FFFF); // End of chain for this dir

        let cluster_offset = (cluster2_lba + (current_cluster as usize - 2) * SECTORS_PER_CLUSTER)
            * SECTOR_SIZE;
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
                break; // Simplified: fit in 1 cluster for small trees
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
                        current_cluster,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_virtual_fat32_builder() {
        let entries = vec![
            VEntry::File {
                name: "SETTINGS.TXT".to_string(),
                data: b"video_mode=60\n".to_vec(),
            },
            VEntry::Dir {
                name: "COCO".to_string(),
                entries: vec![VEntry::File {
                    name: "BAS12.ROM".to_string(),
                    data: vec![0x39; 8192],
                }],
            },
        ];

        let img = build_image_from_tree(entries).expect("Should build valid image");
        assert_eq!(img.len(), 532480 * 512);

        // Check MBR signature
        assert_eq!(img[510], 0x55);
        assert_eq!(img[511], 0xAA);

        // Check VBR signature at 2048 * 512
        let vbr_off = 2048 * 512;
        assert_eq!(img[vbr_off + 510], 0x55);
        assert_eq!(img[vbr_off + 511], 0xAA);
        assert_eq!(&img[vbr_off + 82..vbr_off + 90], b"FAT32   ");
    }
}
