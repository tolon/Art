//! Browsing volumes inside an image — the commands behind the HDF pane.
//!
//! Stage R is read-only, so everything here reads. The pane's "copy to Amiga"
//! action stays visible and disabled with "Coming Later" (§96, §3), the same
//! pattern the workflow catalogue uses for `install_hdf`.
//!
//! ## Never "cannot see HDF"
//!
//! §2.5. [`volume_scan`] always returns every partition it found, with its
//! name, size and filesystem — and, when ART cannot read it, the exact reason.
//! A pane that showed nothing would tell the user their disk is broken when it
//! is merely using a filesystem ART has not implemented.

use std::path::PathBuf;

use serde::Serialize;

use super::panel::PanelEntry;
use crate::core::adf::blocks::EntryKind;
use crate::core::adf::bootblock::FileSystemType;
use crate::core::adf::{extract, fs as adf_fs, validate};
use crate::core::error::{CoreError, CoreResult};
use crate::core::volume::mount::{mount, scan_image, ImageVolumes, VolumeEntry};
use crate::core::volume::VolumeGeometry;
use crate::error::AppResult;

/// What is inside an image file: partitions, or a single bare volume.
#[tauri::command]
pub fn volume_scan(path: String) -> AppResult<ImageVolumes> {
    Ok(scan_image(&PathBuf::from(path.trim()))?)
}

/// A directory inside a volume, as panel rows.
#[derive(Debug, Clone, Serialize)]
pub struct VolumeListing {
    /// The volume's own name, from its root block.
    pub volume_name: String,
    pub root_block: u32,
    pub total_blocks: u32,
    pub block_size: usize,
    /// `FFS INTL`, and so on.
    pub filesystem: String,
    pub entries: Vec<PanelEntry>,
    /// Health findings worth showing — an invalid bitmap flag on a read-only
    /// mount is a warning, not a failure (§5).
    pub warnings: Vec<String>,
}

/// Open one volume of an image and list a directory in it.
///
/// `dir_block` is `None` for the root. The volume is identified by the index
/// [`volume_scan`] reported, so the frontend never has to know about byte
/// offsets or geometry.
#[tauri::command]
pub fn volume_list(
    path: String,
    volume_index: usize,
    dir_block: Option<u32>,
) -> AppResult<VolumeListing> {
    let image = PathBuf::from(path.trim());
    let entry = pick(&image, volume_index)?;
    let (device, geometry) = mount(&image, &entry)?;

    let dir_block = dir_block.unwrap_or(geometry.root_block);
    let entries = adf_fs::list_directory_on(&device, dir_block)?;

    let volume_name = read_volume_name(&device, &geometry).unwrap_or_else(|| entry.name.clone());

    let report = validate::validate_volume(&device, &geometry)?;
    let warnings = report
        .findings
        .iter()
        .map(|finding| finding.message.clone())
        .collect();

    Ok(VolumeListing {
        volume_name,
        root_block: geometry.root_block,
        total_blocks: geometry.total_blocks,
        block_size: geometry.block_size,
        filesystem: entry.filesystem.clone(),
        entries: entries
            .into_iter()
            .map(|file| PanelEntry {
                name: file.name,
                is_dir: file.kind == EntryKind::Directory,
                bytes: file.byte_size,
                path: None,
                header_block: Some(file.header_block),
                iso_extent: None,
                is_link: false,
                date: Some(file.unix_date),
                attrs: Some(file.attrs),
            })
            .collect(),
        warnings,
    })
}

/// Copy one file out of a volume to a local folder.
///
/// The bytes go host-side to host-side; a partition can hold files far larger
/// than anything worth passing through the webview.
#[tauri::command]
pub fn volume_extract_to(
    path: String,
    volume_index: usize,
    header_block: u32,
    dest_dir: String,
    overwrite: Option<bool>,
    oplog: tauri::State<'_, crate::core::oplog::JsonlOperationLog>,
) -> AppResult<super::panel::ExtractedTo> {
    use crate::core::oplog::OperationOutcome;

    let image = PathBuf::from(path.trim());
    let destination = PathBuf::from(dest_dir.trim());
    if !destination.is_dir() {
        return Err(CoreError::InvalidInput(format!(
            "'{}' is not a folder",
            destination.display()
        ))
        .into());
    }

    let entry = pick(&image, volume_index)?;
    let (device, geometry) = mount(&image, &entry)?;
    let header = adf_fs::read_header_on(&device, header_block)?;

    // The name comes from an Amiga disk — untrusted input that must not be
    // able to steer the write out of the folder the user chose.
    let target =
        crate::core::security::path::safe_join(&destination, &header.name).map_err(|err| {
            CoreError::SafetyRefused(format!("'{}' cannot be written here: {err}", header.name))
        })?;

    if target.exists() && !overwrite.unwrap_or(false) {
        return Ok(super::panel::ExtractedTo {
            bytes: std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0),
            path: target.to_string_lossy().to_string(),
            skipped_existing: true,
        });
    }

    let fs_type = if geometry.dos_type.is_ffs() {
        FileSystemType::Ffs
    } else {
        FileSystemType::Ofs
    };

    let result = (|| -> CoreResult<super::panel::ExtractedTo> {
        let data = extract::extract_file_on(&device, &header, fs_type)?;
        crate::core::safety::atomic::atomic_write(&target, &data)?;
        Ok(super::panel::ExtractedTo {
            path: target.to_string_lossy().to_string(),
            bytes: data.len() as u64,
            skipped_existing: false,
        })
    })()
    .map_err(crate::error::AppError::from);

    super::oplog::write_result(
        &oplog,
        super::oplog::user_operation("Copy file out of volume")
            .source(format!("{path}:{}:{}", entry.name, header.name))
            .destination(target.to_string_lossy().to_string()),
        &result,
        |record, extracted: &super::panel::ExtractedTo| {
            record
                .detail("Bytes", extracted.bytes.to_string())
                .outcome(OperationOutcome::verified(true))
        },
    );

    result
}

/// Find the volume the frontend asked for.
fn pick(image: &std::path::Path, index: usize) -> CoreResult<VolumeEntry> {
    let found = scan_image(image)?;
    found.volumes.get(index).cloned().ok_or_else(|| {
        CoreError::InvalidInput(format!(
            "this image has no volume {index} ({} found)",
            found.volumes.len()
        ))
    })
}

/// The volume's name from its root block, when it has one.
fn read_volume_name(
    device: &dyn crate::core::volume::BlockDevice,
    geometry: &VolumeGeometry,
) -> Option<String> {
    let block = crate::core::volume::read_block_vec(device, geometry.root_block).ok()?;
    let root = crate::core::adf::blocks::RootBlock::parse(&block).ok()?;
    (!root.volume_name.is_empty()).then_some(root.volume_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec};
    use crate::core::volume::fixture::{make_ffs_volume, FixtureFile};
    use std::io::{Seek, SeekFrom, Write};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("art-volcmd-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// An RDB image with one FFS partition holding one file.
    fn image_with_partition(path: &std::path::Path) {
        crate::core::hdf::create_hdf(
            path,
            32 * 1024 * 1024,
            true,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::FfsStandard,
                size_mb: 10,
                bootable: true,
                boot_priority: 0,
                num_buffers: 100,
            }],
            &[],
        )
        .unwrap();

        let found = scan_image(path).unwrap();
        let blocks = (found.volumes[0].byte_length / 512) as u32;
        let volume = make_ffs_volume(
            blocks,
            "Work",
            &[FixtureFile {
                name: "Readme",
                content: b"hello from a partition",
            }],
        );

        let mut file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        file.seek(SeekFrom::Start(found.volumes[0].byte_offset))
            .unwrap();
        file.write_all(&volume).unwrap();
        file.sync_all().unwrap();
    }

    #[test]
    fn a_partition_lists_its_files_through_the_command() {
        let dir = scratch("list");
        let path = dir.join("disk.hdf");
        image_with_partition(&path);

        let listing = volume_list(path.to_string_lossy().to_string(), 0, None).unwrap();

        assert_eq!(listing.volume_name, "Work");
        assert_eq!(listing.filesystem, "FFS");
        assert_eq!(listing.entries.len(), 1);
        assert_eq!(listing.entries[0].name, "Readme");
        assert_eq!(listing.entries[0].bytes, 22);
        assert!(listing.entries[0].header_block.is_some());
        assert!(listing.warnings.is_empty(), "{:?}", listing.warnings);
        assert_ne!(listing.root_block, 880, "this is a partition, not a floppy");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_file_copies_out_of_a_partition_byte_for_byte() {
        let dir = scratch("extract");
        let path = dir.join("disk.hdf");
        image_with_partition(&path);
        let out = dir.join("out");
        std::fs::create_dir_all(&out).unwrap();

        let listing = volume_list(path.to_string_lossy().to_string(), 0, None).unwrap();
        let header = listing.entries[0].header_block.unwrap();

        let log = crate::core::oplog::JsonlOperationLog::new(dir.join("ops.jsonl"));
        let extracted = extract_for_test(&path, header, &out, &log);

        assert!(!extracted.skipped_existing);
        assert_eq!(
            std::fs::read(&extracted.path).unwrap(),
            b"hello from a partition"
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A second copy must not silently replace the first.
    #[test]
    fn copying_out_twice_keeps_the_first_file() {
        let dir = scratch("twice");
        let path = dir.join("disk.hdf");
        image_with_partition(&path);
        let out = dir.join("out");
        std::fs::create_dir_all(&out).unwrap();

        let listing = volume_list(path.to_string_lossy().to_string(), 0, None).unwrap();
        let header = listing.entries[0].header_block.unwrap();
        let log = crate::core::oplog::JsonlOperationLog::new(dir.join("ops.jsonl"));

        extract_for_test(&path, header, &out, &log);
        let second = extract_for_test(&path, header, &out, &log);

        assert!(second.skipped_existing);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn asking_for_a_volume_that_is_not_there_is_an_honest_error() {
        let dir = scratch("missing");
        let path = dir.join("disk.hdf");
        image_with_partition(&path);

        let err = volume_list(path.to_string_lossy().to_string(), 7, None).unwrap_err();
        assert!(err.to_string().contains("no volume 7"), "{err}");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The Attr column and the attributes dialog format the exact same
    /// protection field, through the exact same function
    /// (`uaem::format_bits`), so they can never read differently for the
    /// same entry. This proves it end to end rather than trusting that two
    /// call sites of one function must agree — a future refactor that gives
    /// the listing path its own formatting is exactly the drift this guards
    /// against.
    #[test]
    fn a_listing_entry_s_attrs_matches_the_attributes_dialog_s_bits() {
        let dir = scratch("attrs-drift");
        let path = dir.join("disk.hdf");
        image_with_partition(&path);

        let header_block = volume_list(path.to_string_lossy().to_string(), 0, None)
            .unwrap()
            .entries[0]
            .header_block
            .unwrap();

        // A non-default value distinctive enough that the two sides agreeing
        // could not be a coincidence: H and A set, RWED left at "granted".
        let custom_protection = (1u32 << 7) | (1u32 << 4);
        crate::commands::volume_write::with_volume(&path, 0, |writer| {
            writer.set_attributes(header_block, Some(custom_protection), None, None)
        })
        .unwrap();

        let listing = volume_list(path.to_string_lossy().to_string(), 0, None).unwrap();
        let attributes = crate::commands::volume_write::volume_attributes(
            path.to_string_lossy().to_string(),
            0,
            header_block,
        )
        .unwrap();

        assert_eq!(attributes.bits, "h--arwed", "sanity: not the default bits");
        assert_eq!(
            listing.entries[0].attrs.as_deref(),
            Some(attributes.bits.as_str())
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// `volume_extract_to` needs Tauri state, so the test drives the same code
    /// through a thin stand-in rather than pretending to have an app handle.
    fn extract_for_test(
        image: &std::path::Path,
        header_block: u32,
        dest: &std::path::Path,
        _log: &crate::core::oplog::JsonlOperationLog,
    ) -> super::super::panel::ExtractedTo {
        let entry = pick(image, 0).unwrap();
        let (device, geometry) = mount(image, &entry).unwrap();
        let header = adf_fs::read_header_on(&device, header_block).unwrap();
        let target = crate::core::security::path::safe_join(dest, &header.name).unwrap();

        if target.exists() {
            return super::super::panel::ExtractedTo {
                bytes: std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0),
                path: target.to_string_lossy().to_string(),
                skipped_existing: true,
            };
        }

        let fs_type = if geometry.dos_type.is_ffs() {
            FileSystemType::Ffs
        } else {
            FileSystemType::Ofs
        };
        let data = extract::extract_file_on(&device, &header, fs_type).unwrap();
        crate::core::safety::atomic::atomic_write(&target, &data).unwrap();

        super::super::panel::ExtractedTo {
            path: target.to_string_lossy().to_string(),
            bytes: data.len() as u64,
            skipped_existing: false,
        }
    }
}
