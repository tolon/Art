//! Create new blank or formatted Amiga floppy disk images (880 KB DD).
//!
//! Generates a 901,120-byte standard AmigaDOS disk image with:
//! - Blocks 0..1: Bootblock with DOS signature and 1024-byte checksum. The root
//!   block is not stored here — a boot block has no such field — but it is
//!   always block 880 for a DD image (`DEFAULT_ROOT_BLOCK`).
//! - Block 880: Root block (T_HEADER, ST_ROOT, 72 empty hash buckets, volume name, dates, checksum).
//! - Block 881: Bitmap block (tracks blocks 0, 1, 880, 881 as used, rest free, checksum at offset 0).

use std::path::Path;

use super::bcpl::{write_bcpl_string, AmigaDate};
use super::blocks::{block_subtype, block_type, BLOCK_SIZE, HASH_TABLE_SIZE};
use super::bootblock::{BootBlock, FileSystemType, DEFAULT_ROOT_BLOCK};
use super::bootcode;
use super::checksum::block_checksum;

/// Boot blocks at the front of a floppy, excluded from the bitmap.
pub const FLOPPY_RESERVED: u32 = 2;
use super::{AdfImage, AdfInfo, DD_SIZE, DD_TOTAL_BLOCKS};
use crate::core::error::{CoreError, CoreResult};

/// Create an in-memory 880 KB ADF image buffer.
pub fn create_blank_adf(
    volume_name: &str,
    fs_type: FileSystemType,
    bootable: bool,
) -> CoreResult<Vec<u8>> {
    let clean_name = volume_name.trim();
    if clean_name.is_empty() {
        return Err(CoreError::InvalidInput(
            "volume name cannot be empty".into(),
        ));
    }
    if clean_name.len() > 30 {
        return Err(CoreError::InvalidInput(
            "volume name must not exceed 30 characters".into(),
        ));
    }

    let mut img = vec![0u8; DD_SIZE];

    // -----------------------------------------------------------------------
    // 1. Bootblock (Blocks 0 & 1 = 1024 bytes)
    // -----------------------------------------------------------------------
    let sig = match fs_type {
        FileSystemType::Ofs => b"DOS\x00",
        FileSystemType::Ffs => b"DOS\x01",
    };
    img[0..4].copy_from_slice(sig);
    // Offset 8 onwards is boot code, not a root-block pointer — a boot block
    // has no such field. Non-bootable images leave it zero, matching a real
    // AmigaDOS disk with no boot code installed.

    if bootable {
        // Kickstart jumps to offset 12 and requires the code there to return
        // D0 = 0 with A0 holding an address to jump to; a non-zero D0 raises
        // an alert and reboots. The bare RTS this used to write returned with
        // whatever was already in D0, so the machine did nothing (ART-063).
        let code = bootcode::boot_code();
        img[bootcode::BB_ENTRY..bootcode::BB_ENTRY + code.len()].copy_from_slice(&code);
    }

    // Compute and embed 1024-byte bootblock checksum at offset 4..8
    let boot_cks = BootBlock::compute_checksum(&img[..1024]);
    img[4..8].copy_from_slice(&boot_cks.to_be_bytes());

    // -----------------------------------------------------------------------
    // 2. Root Block (Block 880)
    // -----------------------------------------------------------------------
    let root_off = DEFAULT_ROOT_BLOCK as usize * BLOCK_SIZE;
    let root_slice = &mut img[root_off..root_off + BLOCK_SIZE];

    // Primary Type = T_HEADER (2) at offset 0
    root_slice[0..4].copy_from_slice(&block_type::HEADER.to_be_bytes());
    // ht_size = 72 at offset 12
    root_slice[12..16].copy_from_slice(&(HASH_TABLE_SIZE as u32).to_be_bytes());
    // Hash table at offset 24..312 is left all-zero (empty buckets)

    // bm_flag = -1 (0xFFFFFFFF = valid bitmap) at offset 312 (LW 78)
    root_slice[312..316].copy_from_slice(&(-1i32).to_be_bytes());
    // bm_pages[0] = 881 at offset 316 (LW 79)
    root_slice[316..320].copy_from_slice(&881u32.to_be_bytes());

    // Date (now)
    let now = get_current_amiga_date();
    root_slice[420..424].copy_from_slice(&now.days.to_be_bytes());
    root_slice[424..428].copy_from_slice(&now.mins.to_be_bytes());
    root_slice[428..432].copy_from_slice(&now.ticks.to_be_bytes());

    // Disk Volume Name at offset 432 (LW 108)
    write_bcpl_string(root_slice, 432, clean_name, 32);

    // Creation date at offset 472
    root_slice[472..476].copy_from_slice(&now.days.to_be_bytes());
    root_slice[476..480].copy_from_slice(&now.mins.to_be_bytes());
    root_slice[480..484].copy_from_slice(&now.ticks.to_be_bytes());

    // Secondary Type = ST_ROOT (1) at offset 508 (LW 127)
    root_slice[508..512].copy_from_slice(&block_subtype::ROOT.to_be_bytes());

    // Compute root block checksum at offset 20
    let root_cks = block_checksum(root_slice, 20);
    root_slice[20..24].copy_from_slice(&root_cks.to_be_bytes());

    // -----------------------------------------------------------------------
    // 3. Bitmap Block (Block 881)
    // -----------------------------------------------------------------------
    let bm_off = 881 * BLOCK_SIZE;
    let bm_slice = &mut img[bm_off..bm_off + BLOCK_SIZE];

    // Initialize all 1760 bits as 1 (free)
    // 1760 / 32 = 55 longwords of 0xFFFFFFFF
    for lw_i in 0..55 {
        let off = 4 + lw_i * 4;
        bm_slice[off..off + 4].copy_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
    }

    // Now mark used blocks (bit = 0 means USED in AmigaDOS bitmap):
    // Block 0: Bootblock sector 0 (bit 0 of longword 0) -> 0x7FFFFFFF
    // Block 1: Bootblock sector 1 (bit 1 of longword 0) -> 0x3FFFFFFF
    set_bitmap_bit(bm_slice, 0, false);
    set_bitmap_bit(bm_slice, 1, false);

    // Block 880: Root Block (880 / 32 = LW 27, bit 16)
    set_bitmap_bit(bm_slice, 880, false);
    // Block 881: Bitmap Block (881 / 32 = LW 27, bit 17)
    set_bitmap_bit(bm_slice, 881, false);

    // Compute bitmap block checksum at offset 0
    let bm_cks = block_checksum(bm_slice, 0);
    bm_slice[0..4].copy_from_slice(&bm_cks.to_be_bytes());

    Ok(img)
}

/// Save a newly created ADF image to a file and return its [`AdfInfo`].
///
/// Creating a disk is a `SAFE_CREATE` operation: it writes a new file and never
/// replaces an existing one (spec §57). Overwriting here would silently destroy
/// a disk image the user had already made.
pub fn save_new_adf(
    path: &Path,
    volume_name: &str,
    fs_type: FileSystemType,
    bootable: bool,
) -> CoreResult<AdfInfo> {
    if path.exists() {
        return Err(CoreError::SafetyRefused(format!(
            "'{}' already exists; creating a disk never replaces one",
            path.display()
        )));
    }

    let bytes = create_blank_adf(volume_name, fs_type, bootable)?;
    crate::core::safety::atomic_write(path, &bytes)?;
    let img = AdfImage::open(path)?;
    img.info()
}

/// Set a bit in an AmigaDOS bitmap block (bit=1 means free, bit=0 means used).
pub fn set_bitmap_bit(bm_block: &mut [u8], block_num: usize, is_free: bool) {
    // A floppy's bitmap fits in one block, so only the position within it
    // matters here. `bit_position` owns the arithmetic — see ART-035 for why
    // this must not be written out a second time.
    let Some(position) =
        super::blocks::bit_position(block_num as u32, FLOPPY_RESERVED, DD_TOTAL_BLOCKS as u32)
    else {
        return;
    };
    if position.bitmap_index != 0 {
        return;
    }

    let off = position.byte_offset;
    let mut lw = u32::from_be_bytes([
        bm_block[off],
        bm_block[off + 1],
        bm_block[off + 2],
        bm_block[off + 3],
    ]);

    if is_free {
        lw |= position.mask;
    } else {
        lw &= !position.mask;
    }
    bm_block[off..off + 4].copy_from_slice(&lw.to_be_bytes());
}

/// Helper to get current date as AmigaDate (days since 1978-01-01).
fn get_current_amiga_date() -> AmigaDate {
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(super::bcpl::AMIGA_EPOCH_UNIX);

    let secs_since_amiga = (now_unix - super::bcpl::AMIGA_EPOCH_UNIX).max(0);
    let days = (secs_since_amiga / 86_400) as u32;
    let rem_secs = secs_since_amiga % 86_400;
    let mins = (rem_secs / 60) as u32;
    let ticks = ((rem_secs % 60) * 50) as u32;

    AmigaDate { days, mins, ticks }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::adf::validate::HealthStatus;

    #[test]
    fn create_blank_ofs_adf_validates_healthy() {
        let bytes = create_blank_adf("MyDisk", FileSystemType::Ofs, false).unwrap();
        assert_eq!(bytes.len(), DD_SIZE);
        let adf = AdfImage::from_bytes(bytes).unwrap();
        let info = adf.info().unwrap();
        assert_eq!(info.volume_name, "MyDisk");
        assert_eq!(info.fs_type, FileSystemType::Ofs);
        assert!(info.checksum_valid);
        assert_eq!(info.directory_count, 0);
        assert_eq!(info.file_count, 0);
        // Used blocks: 0, 1, 880, 881 = 4 blocks = 2048 bytes
        assert_eq!(info.used_bytes, 4 * 512);

        let report = adf.validate().unwrap();
        assert_eq!(report.status, HealthStatus::Healthy);
    }

    #[test]
    fn create_blank_ffs_bootable_adf() {
        let bytes = create_blank_adf("WorkbenchCustom", FileSystemType::Ffs, true).unwrap();
        let adf = AdfImage::from_bytes(bytes).unwrap();
        let info = adf.info().unwrap();
        assert_eq!(info.volume_name, "WorkbenchCustom");
        assert_eq!(info.fs_type, FileSystemType::Ffs);
        assert!(info.bootable);
        assert!(info.checksum_valid);
    }

    #[test]
    fn rejects_empty_or_long_name() {
        assert!(create_blank_adf("", FileSystemType::Ofs, false).is_err());
        assert!(create_blank_adf(
            "ThisVolumeNameIsWayTooLongForAnAmigaDisk",
            FileSystemType::Ofs,
            false
        )
        .is_err());
    }

    /// Creating a disk is SAFE_CREATE: it must never land on an existing file.
    #[test]
    fn save_refuses_to_replace_an_existing_file() {
        let dir =
            std::env::temp_dir().join(format!("art-adf-create-{}", crate::core::test_scratch_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("Existing.adf");
        std::fs::write(&target, b"a disk the user already made").unwrap();

        let err = save_new_adf(&target, "New", FileSystemType::Ffs, false).unwrap_err();
        assert!(matches!(err, CoreError::SafetyRefused(_)), "got {err:?}");
        assert_eq!(
            std::fs::read(&target).unwrap(),
            b"a disk the user already made"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_writes_a_new_disk() {
        let dir =
            std::env::temp_dir().join(format!("art-adf-new-{}", crate::core::test_scratch_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("Fresh.adf");

        let info = save_new_adf(&target, "Fresh", FileSystemType::Ffs, false).unwrap();
        assert_eq!(info.volume_name, "Fresh");
        assert!(target.exists());

        std::fs::remove_dir_all(&dir).ok();
    }
}

#[cfg(test)]
mod oracle_export {
    /// Write an ADF for the amitools oracle (brief §4.2/§4.3).
    ///
    /// Not an assertion about ART: a hook so `xdftool` can be pointed at an
    /// image ART wrote. Set `ART_ADF_OUT` to a path and run the suite; the
    /// cross-check itself is a manual step, recorded in STATUS.
    ///
    /// The disk is formatted by *this* module and then filled by
    /// `core/volume/write`, which is the combination a user's disk really goes
    /// through — the writer's own export hooks start from
    /// `core::volume::fixture`, so without this one nothing checks that
    /// `create_blank_adf`'s output is a volume the writer and amitools agree
    /// about.
    #[test]
    fn export_adf_for_oracle_when_asked() {
        use crate::core::volume::device::FileRegionMut;
        use crate::core::volume::write::VolumeWriter;
        use crate::core::volume::{DosType, VolumeGeometry};

        let Ok(dest) = std::env::var("ART_ADF_OUT") else {
            return;
        };
        let dest = std::path::PathBuf::from(dest);
        let image =
            super::create_blank_adf("Work", crate::core::adf::FileSystemType::Ffs, false).unwrap();
        std::fs::write(&dest, image).unwrap();

        // Add a file the way the application does: through the one filesystem
        // writer ART has, so the oracle sees what a user's disk would contain.
        let geometry = VolumeGeometry::floppy_dd(DosType::new(*b"DOS\x01"));
        let mut device =
            FileRegionMut::open(&dest, 0, geometry.total_bytes(), geometry.block_size).unwrap();
        let mut writer = VolumeWriter::open(&mut device, geometry, &dest, 0).unwrap();
        writer
            .add_file(0, "Readme", b"hello from ART", Default::default())
            .unwrap();
    }

    /// Write a **bootable** disk for a human to try on a real Amiga.
    ///
    /// ART-063's fix cannot be proved by any test here. Whether Kickstart
    /// accepts our boot code is a question only Kickstart answers: the tests
    /// in `core::adf::bootcode` pin the instruction layout and the two
    /// relative displacements, which is the part a human gets wrong, but a
    /// disk can satisfy every one of them and still hang a real machine.
    ///
    /// Set `ART_BOOT_ADF_OUT` to a path and run the suite. Contents match the
    /// non-bootable disk in `test/` deliberately, so the only difference
    /// between the two is the boot block.
    #[test]
    fn export_bootable_adf_when_asked() {
        use crate::core::volume::device::FileRegionMut;
        use crate::core::volume::write::VolumeWriter;
        use crate::core::volume::{DosType, VolumeGeometry};

        let Ok(dest) = std::env::var("ART_BOOT_ADF_OUT") else {
            return;
        };
        let dest = std::path::PathBuf::from(dest);
        let image =
            super::create_blank_adf("Work", crate::core::adf::FileSystemType::Ffs, true).unwrap();
        std::fs::write(&dest, image).unwrap();

        let geometry = VolumeGeometry::floppy_dd(DosType::new(*b"DOS\x01"));
        let mut device =
            FileRegionMut::open(&dest, 0, geometry.total_bytes(), geometry.block_size).unwrap();
        let mut writer = VolumeWriter::open(&mut device, geometry, &dest, 0).unwrap();
        writer
            .add_file(0, "Readme", b"hello from ART", Default::default())
            .unwrap();
    }
}
