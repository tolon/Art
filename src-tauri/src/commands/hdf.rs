//! HDF & RDB Hard Disk Tauri commands.

use std::path::PathBuf;

use tauri::State;

use super::oplog::{user_operation, write_result};
use crate::core::error::{CoreError, CoreResult};
use crate::core::hdf::{create_hdf, open_hdf, HdfInfo};
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::core::rdb::{version_from_ver_string, FileSystemSpec, PartitionSpec};
use crate::error::{AppError, AppResult};

/// Open and inspect an HDF image.
#[tauri::command]
pub fn hdf_open(path: String) -> AppResult<HdfInfo> {
    let info = open_hdf(&PathBuf::from(path))?;
    Ok(info)
}

/// A filesystem driver the user has chosen to embed (G4's writing half).
///
/// The **path** is here rather than the bytes because `core/` reads no user
/// path of its own — this is the adapter's job, and reading it here is what
/// keeps `create_rdb_layout` a pure function over bytes.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct FileSystemInput {
    /// The driver on the user's disk — `pfs3aio`, typically.
    pub path: String,
    /// Four characters: `PDS3`, `SFS0`.
    pub dos_type: String,
    /// Left out when ART should read the version out of the driver's own
    /// `$VER:` string, which is the usual case and the more reliable one.
    #[serde(default)]
    pub version: Option<u16>,
    #[serde(default)]
    pub revision: Option<u16>,
}

/// The most a filesystem driver may be.
///
/// A real one is tens of kilobytes — `pfs3aio` is 58 KB. The whole driver has
/// to fit in the reserved area at the front of the disk alongside the
/// partition table, so this is a bound on *reading an unchecked file into
/// memory* (§56) long before it is a bound on anything an Amiga would accept.
const MAX_FILE_SYSTEM_BYTES: u64 = 2 * 1024 * 1024;

/// Turn the screen's driver choices into what the RDB writer needs.
///
/// `pub(crate)` and `CoreResult` rather than `AppResult` since 2026-08-23:
/// the card builder embeds drivers too now, and its own inner half returns
/// `CoreResult`. Every failure here is already a `CoreError`; the command
/// layer converts at its own edge, as it does everywhere else.
pub(crate) fn read_file_systems(inputs: &[FileSystemInput]) -> CoreResult<Vec<FileSystemSpec>> {
    let mut specs = Vec::new();
    for input in inputs {
        let path = PathBuf::from(&input.path);
        let size = std::fs::metadata(&path)
            .map_err(|e| {
                CoreError::Io(std::io::Error::new(
                    e.kind(),
                    format!("{}: {e}", input.path),
                ))
            })?
            .len();
        if size == 0 || size > MAX_FILE_SYSTEM_BYTES {
            return Err(CoreError::InvalidInput(format!(
                "'{}' is {size} bytes; a file system driver must be between 1 byte and {} MB",
                input.path,
                MAX_FILE_SYSTEM_BYTES / (1024 * 1024)
            )));
        }

        // A DOS type is three characters and a **number**, not four
        // characters: `PDS3` is `P`, `D`, `S`, `0x03`. Writing the ASCII `'3'`
        // there instead gives `0x50445333`, which no Amiga recognises as
        // anything — and the partition would then name a filesystem this very
        // driver does not claim to be.
        let bytes = input.dos_type.as_bytes();
        if bytes.len() != 4 || !bytes[3].is_ascii_digit() {
            return Err(CoreError::InvalidInput(format!(
                "'{}' is not a DOS type; it must be three characters and a digit, like PDS3",
                input.dos_type
            )));
        }
        let dos_type = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3] - b'0']);

        let data = std::fs::read(&path)?;

        // The driver's own word first; the user's only when it stays silent.
        // A version of 0.0 would lose to whatever AmigaOS already has loaded,
        // so there is no safe default to fall back on — ART asks (spec §89).
        let (version, revision) = match (input.version, input.revision) {
            (Some(version), Some(revision)) => (version, revision),
            _ => version_from_ver_string(&data).ok_or_else(|| {
                CoreError::InvalidInput(format!(
                    "'{}' does not say what version it is. AmigaOS keeps the higher of the \
                     version in the disk and the one already loaded, so ART will not guess \
                     one — state the driver's version.",
                    input.path
                ))
            })?,
        };

        specs.push(FileSystemSpec {
            dos_type,
            version,
            revision,
            data,
        });
    }
    Ok(specs)
}

/// Create a new HDF image file.
#[tauri::command]
pub fn hdf_create(
    path: String,
    total_bytes: u64,
    is_rdb: bool,
    partitions: Vec<PartitionSpec>,
    file_systems: Vec<FileSystemInput>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<HdfInfo> {
    let result: AppResult<HdfInfo> = read_file_systems(&file_systems)
        .map_err(AppError::from)
        .and_then(|specs| {
            create_hdf(
                &PathBuf::from(&path),
                total_bytes,
                is_rdb,
                &partitions,
                &specs,
            )
            .map_err(Into::into)
        });

    write_result(
        &oplog,
        user_operation("Create hard disk image")
            .destination(&path)
            .detail("Size", format!("{} MB", total_bytes / (1024 * 1024)))
            .detail("Layout", if is_rdb { "RDB partitioned" } else { "Plain" })
            .detail("Partitions", partitions.len().to_string())
            .detail("File systems", file_systems.len().to_string()),
        &result,
        |record, info: &HdfInfo| {
            record.outcome(OperationOutcome::verified(info.rdb_checksum_valid))
        },
    );

    result
}
