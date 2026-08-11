//! Format detection.
//!
//! The detection layer is the entry point of the `DROP → ANALYZE` pipeline.
//! It must be conservative: never claim certainty about a format it cannot
//! actually verify. Detection combines:
//!
//! 1. path (extension) as a *hint*
//! 2. file size as a *sanity check*
//! 3. magic bytes / signature where available
//!
//! Phase 0 implements detection by category (Amiga disk image vs. archive vs.
//! ROM vs. directory). Detailed format parsing (OFS vs FFS, RDB inspection,
//! LHA header parsing, ...) is deferred to later phases.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::core::error::{CoreError, CoreResult};

/// A coarse category of Amiga object. Used by the Workflow Engine to decide
/// which workflows are candidates.
///
/// Keep discriminants stable — they may be persisted/logged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FormatCategory {
    /// Floppy disk image (ADF, ADZ, DMS).
    FloppyImage,
    /// Hard disk image (HDF, HDZ).
    HardDiskImage,
    /// Optical disc image (ISO9660, raw CD track). Detection only — ART does
    /// not yet read the filesystem inside one (spec §10, §89).
    OpticalImage,
    /// Archive, typically LHA on Amiga.
    Archive,
    /// Kickstart ROM.
    Rom,
    /// A plain directory (dropped folder).
    Directory,
    /// Unknown / unsupported file.
    Unknown,
}

impl FormatCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FloppyImage => "floppy-image",
            Self::HardDiskImage => "harddisk-image",
            Self::OpticalImage => "optical-image",
            Self::Archive => "archive",
            Self::Rom => "rom",
            Self::Directory => "directory",
            Self::Unknown => "unknown",
        }
    }
}

/// The detected logical format of a single object.
///
/// `format_hint` is the best-effort concrete format string (e.g. `"adf"`),
/// while `category` is the coarse group used for workflow routing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Detection {
    /// Coarse category — drives workflow routing.
    pub category: FormatCategory,
    /// Best-effort concrete format id: `"adf"`, `"adz"`, `"dms"`, `"hdf"`,
    /// `"hdz"`, `"lha"`, `"rom"`, `"directory"`, or `"unknown"`.
    pub format_hint: String,
    /// 0.0–1.0. How confident detection is. Conservative: signature-backed
    /// detection may report high confidence; extension-only detection stays
    /// low/medium.
    pub confidence: f32,
    /// File size in bytes. `0` for directories.
    pub size: u64,
    /// Whether the object is a directory rather than a file.
    pub is_dir: bool,
}

impl Detection {
    pub fn unknown(size: u64, is_dir: bool) -> Self {
        Self {
            category: if is_dir {
                FormatCategory::Directory
            } else {
                FormatCategory::Unknown
            },
            format_hint: if is_dir { "directory" } else { "unknown" }.to_string(),
            confidence: if is_dir { 1.0 } else { 0.0 },
            size,
            is_dir,
        }
    }
}

/// Standard sizes (bytes).
///
/// Amiga DD floppy = 80 cylinders × 2 heads × 11 sectors × 512 bytes = 901120.
/// HD floppy (rare) doubles that.
pub mod sizes {
    pub const ADF_DD: u64 = 901_120;
    pub const ADF_HD: u64 = 1_802_240;
}

/// LHA (-lh5-) level 1 header magic: `−lz−` archive header byte layout.
/// The Amiga LHA variant begins with the same 2-byte magic `-l`.
const LHA_MAGIC: &[u8] = b"-l";

/// ISO9660 volume descriptor magic, "CD001", as it appears at the start of
/// the Primary Volume Descriptor (sector 16 of a 2048-byte-sector image).
const ISO_MAGIC: &[u8] = b"CD001";

/// Offset of "CD001" in a standard 2048-byte-sector ISO image: sector 16 is
/// 16 × 2048 = 0x8000, and the signature sits at offset 1 within that sector.
const ISO_PVD_OFFSET_2048: u64 = 0x8001;

/// Offset of "CD001" in a raw 2352-byte-sector CD track. Sector 16 begins at
/// 16 × 2352 = 0x9300; its 2048 bytes of user data start at offset 16 within
/// the sector (0x9310), and the signature sits one byte into that (0x9311).
/// Finding it here also proves the sector size for later readers.
const ISO_PVD_OFFSET_2352: u64 = 0x9311;

/// Detect the logical format of a filesystem path.
///
/// Reads at most a few bytes of the file for signature checks; never loads
/// the whole image into memory, and never reads past a probe offset that
/// falls beyond the file's own length.
pub fn detect(path: &Path) -> CoreResult<Detection> {
    let meta = std::fs::metadata(path).map_err(|e| {
        // Surface a clearer error if the path doesn't exist at all.
        if e.kind() == std::io::ErrorKind::NotFound {
            CoreError::InvalidInput(format!("file not found: {}", path.to_string_lossy()))
        } else {
            CoreError::Io(e)
        }
    })?;

    if meta.is_dir() {
        return Ok(Detection {
            category: FormatCategory::Directory,
            format_hint: "directory".to_string(),
            confidence: 1.0,
            size: 0,
            is_dir: true,
        });
    }

    let size = meta.len();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();

    // --- Signature-backed checks first (highest confidence) ---------------
    //
    // Content decides the category; the extension below is only a fallback
    // hint for when no signature matches (a corrupted header, or a format
    // ART does not yet recognise by content). First match wins.
    if let Ok(head) = read_head(path, 4) {
        // AmigaDOS boot signature: "DOS" + a filesystem flags byte 0x00-0x07
        // (OFS/FFS × international × dircache/long-filenames). Floppy-sized
        // images are floppies; anything else with this signature is a
        // single-volume hard disk image.
        if head.len() == 4 && &head[0..3] == b"DOS" && head[3] <= 0x07 {
            return Ok(dos_signature_detection(size));
        }
        if head.as_slice() == b"RDSK" {
            return Ok(Detection {
                category: FormatCategory::HardDiskImage,
                format_hint: "rdb".to_string(),
                confidence: 0.95,
                size,
                is_dir: false,
            });
        }
    }

    // ISO9660 checks need to probe deep into the file — not the 4-byte head.
    if let Ok(sig) = probe_at(path, ISO_PVD_OFFSET_2048, ISO_MAGIC.len()) {
        if sig == ISO_MAGIC {
            return Ok(Detection {
                category: FormatCategory::OpticalImage,
                format_hint: "iso9660".to_string(),
                confidence: 0.95,
                size,
                is_dir: false,
            });
        }
    }
    if let Ok(sig) = probe_at(path, ISO_PVD_OFFSET_2352, ISO_MAGIC.len()) {
        if sig == ISO_MAGIC {
            return Ok(Detection {
                category: FormatCategory::OpticalImage,
                format_hint: "iso9660-raw".to_string(),
                confidence: 0.9,
                size,
                is_dir: false,
            });
        }
    }

    if let Ok(head) = read_head(path, 4) {
        if head.as_slice() == b"PFS\x03" || head.as_slice() == b"PDS\x03" {
            return Ok(Detection {
                category: FormatCategory::HardDiskImage,
                format_hint: "pfs3".to_string(),
                confidence: 0.9,
                size,
                is_dir: false,
            });
        }
        if head.as_slice() == b"SFS\x00" {
            return Ok(Detection {
                category: FormatCategory::HardDiskImage,
                format_hint: "sfs".to_string(),
                confidence: 0.9,
                size,
                is_dir: false,
            });
        }
        // LHA archive: starts with "-l" followed by compression method.
        if head.starts_with(LHA_MAGIC) {
            return Ok(Detection {
                category: FormatCategory::Archive,
                format_hint: "lha".to_string(),
                confidence: 0.95,
                size,
                is_dir: false,
            });
        }
    }

    // --- Size + extension hints (fallback only; weaker than a signature) --
    match ext.as_str() {
        "adf" => Ok(floppy_by_size("adf", size)),
        "adz" => Ok(floppy_by_size("adz", size)),
        "dms" => Ok(floppy_by_size("dms", size)),
        "hdf" => Ok(hdf_by_extension(size)),
        "hdz" => Ok(hdf_by_extension(size)),
        "rom" => Ok(rom_by_size(size)),
        "lha" | "lzh" => Ok(Detection {
            category: FormatCategory::Archive,
            format_hint: "lha".to_string(),
            confidence: 0.35,
            size,
            is_dir: false,
        }),
        _ => Ok(Detection::unknown(size, false)),
    }
}

/// A `DOS\0`..`DOS\7` boot signature at offset 0, categorised by size: the
/// two known floppy sizes are floppies, anything else is a single-volume
/// hard disk image (spec §41 leaves multi-partition RDB disks to the `RDSK`
/// check above, which runs first).
fn dos_signature_detection(size: u64) -> Detection {
    match size {
        sizes::ADF_DD => Detection {
            category: FormatCategory::FloppyImage,
            format_hint: "adf".to_string(),
            confidence: 0.97,
            size,
            is_dir: false,
        },
        sizes::ADF_HD => Detection {
            category: FormatCategory::FloppyImage,
            format_hint: "adf".to_string(),
            confidence: 0.95,
            size,
            is_dir: false,
        },
        _ => Detection {
            category: FormatCategory::HardDiskImage,
            format_hint: "hdf".to_string(),
            confidence: 0.9,
            size,
            is_dir: false,
        },
    }
}

/// Build a floppy-image detection from size, validating against known ADF
/// sizes. Extension-only fallback: no `DOS` signature was found at offset 0,
/// so this is weaker evidence than [`dos_signature_detection`].
fn floppy_by_size(format: &str, size: u64) -> Detection {
    let (ok, confidence) = match size {
        sizes::ADF_DD => (true, 0.6),
        sizes::ADF_HD => (true, 0.5),
        _ => (false, 0.3), // unusual size — still probably an ADF, but flag it
    };
    let _ = ok; // surfaced to the caller via confidence in Phase 1+ reporting
    Detection {
        category: FormatCategory::FloppyImage,
        format_hint: format.to_string(),
        confidence,
        size,
        is_dir: false,
    }
}

/// HDF detection by extension alone: no recognised signature (`DOS`, `RDSK`,
/// `PFS\3`, `PDS\3`, `SFS\0`) was found at offset 0, so this is only ever
/// reached as the weaker fallback — real RDB/header inspection is the
/// signature-backed path above.
fn hdf_by_extension(size: u64) -> Detection {
    Detection {
        category: FormatCategory::HardDiskImage,
        format_hint: "hdf".to_string(),
        confidence: 0.35,
        size,
        is_dir: false,
    }
}

/// ROM detection validates against common Kickstart ROM sizes.
fn rom_by_size(size: u64) -> Detection {
    // Common Kickstart ROM sizes: 256K, 512K, 1M.
    const ROM_SIZES: &[u64] = &[256 * 1024, 512 * 1024, 1024 * 1024];
    let matches_known = ROM_SIZES.contains(&size);
    Detection {
        category: FormatCategory::Rom,
        format_hint: "rom".to_string(),
        confidence: if matches_known { 0.7 } else { 0.4 },
        size,
        is_dir: false,
    }
}

/// Read up to `n` bytes from the start of a file.
fn read_head(path: &Path, n: usize) -> CoreResult<Vec<u8>> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut buf = vec![0u8; n];
    let read = f.read(&mut buf)?;
    buf.truncate(read);
    Ok(buf)
}

/// Read up to `len` bytes starting at `offset` in a file, seeking there
/// first rather than reading through it — a signature offset can be well
/// past 0x8000 and the file itself can be gigabytes.
///
/// A short read (offset lands past end of file, or the file has fewer than
/// `len` bytes left) is not an error: it returns whatever bytes exist,
/// possibly empty, so the caller's equality check against a known magic
/// simply fails to match rather than panicking or misreporting. `len` is
/// always a small caller-supplied constant (a signature length), never a
/// value read from the file itself.
fn probe_at(path: &Path, offset: u64, len: usize) -> CoreResult<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};

    let mut f = std::fs::File::open(path)?;
    let file_len = f.metadata()?.len();
    if offset >= file_len {
        return Ok(Vec::new());
    }

    f.seek(SeekFrom::Start(offset))?;
    let mut buf = vec![0u8; len];
    let read = f.read(&mut buf)?;
    buf.truncate(read);
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn tmp() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "art-detect-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn detects_directory() {
        let d = tmp();
        let det = detect(&d).unwrap();
        assert_eq!(det.category, FormatCategory::Directory);
        assert!(det.is_dir);
        assert_eq!(det.confidence, 1.0);
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn detects_adf_by_size_and_extension() {
        let d = tmp();
        let p = d.join("disk.adf");
        let mut f = fs::File::create(&p).unwrap();
        // Write exactly ADF_DD zero bytes — no DOS signature, so this
        // exercises the extension-only fallback, not the signature path.
        let chunk = vec![0u8; 8192];
        let mut remaining = sizes::ADF_DD;
        while remaining > 0 {
            let n = chunk.len().min(remaining as usize);
            f.write_all(&chunk[..n]).unwrap();
            remaining -= n as u64;
        }
        let det = detect(&p).unwrap();
        assert_eq!(det.category, FormatCategory::FloppyImage);
        assert_eq!(det.format_hint, "adf");
        // Extension-only evidence is weaker than a content signature (see
        // `an_img_holding_a_floppy_is_a_floppy_not_an_unknown`, which hits
        // the signature path and expects >= 0.9), but still confident given
        // the exact known ADF size.
        assert!(det.confidence >= 0.5, "got {}", det.confidence);
        assert!(det.confidence < 0.9, "got {}", det.confidence);
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn detects_lha_by_signature() {
        let d = tmp();
        let p = d.join("game.lha");
        fs::write(&p, b"-lh5-").unwrap();
        let det = detect(&p).unwrap();
        assert_eq!(det.category, FormatCategory::Archive);
        assert_eq!(det.format_hint, "lha");
        assert!(det.confidence >= 0.9);
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn detects_unknown_for_unrecognised_file() {
        let d = tmp();
        let p = d.join("random.bin");
        fs::write(&p, b"not an amiga file").unwrap();
        let det = detect(&p).unwrap();
        assert_eq!(det.category, FormatCategory::Unknown);
        assert_eq!(det.format_hint, "unknown");
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn missing_file_is_invalid_input() {
        let p = std::path::PathBuf::from("Z:/no/such/file.adf");
        let err = detect(&p).unwrap_err();
        assert!(matches!(err, CoreError::InvalidInput(_)), "got: {err}");
    }

    // --- Content-first signature detection ---------------------------------
    //
    // These are what make this a content-first detector rather than a longer
    // extension list: the extension lies (or is absent — Gotek/PiStorm often
    // hand ART bare `.img`) and detection must still get it right by reading
    // the bytes.

    /// Write `bytes` at `offset` in a fresh file, backfilling the gap with
    /// zeros. Used to build synthetic fixtures without materialising huge
    /// buffers in memory (a raw ISO fixture needs 0x9311+ bytes).
    fn write_at(path: &std::path::Path, offset: u64, bytes: &[u8]) {
        use std::io::{Seek, SeekFrom};
        let mut f = fs::File::create(path).unwrap();
        f.seek(SeekFrom::Start(offset)).unwrap();
        f.write_all(bytes).unwrap();
    }

    #[test]
    fn an_img_holding_a_floppy_is_a_floppy_not_an_unknown() {
        let d = tmp();
        let p = d.join("disk.img");
        let mut f = fs::File::create(&p).unwrap();
        f.write_all(b"DOS\0").unwrap();
        let chunk = vec![0u8; 8192];
        let mut remaining = sizes::ADF_DD - 4;
        while remaining > 0 {
            let n = chunk.len().min(remaining as usize);
            f.write_all(&chunk[..n]).unwrap();
            remaining -= n as u64;
        }
        drop(f);

        let det = detect(&p).unwrap();
        assert_eq!(det.category, FormatCategory::FloppyImage);
        assert!(det.confidence >= 0.9, "got {}", det.confidence);
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn an_adf_that_is_really_an_iso_is_reported_as_an_iso() {
        let d = tmp();
        let p = d.join("disk.adf");
        write_at(&p, 0x8001, b"CD001");

        let det = detect(&p).unwrap();
        assert_eq!(det.category, FormatCategory::OpticalImage);
        assert_eq!(det.format_hint, "iso9660");
        assert!(det.confidence >= 0.9, "got {}", det.confidence);
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_raw_track_iso_is_detected_at_0x9311() {
        let d = tmp();
        let p = d.join("disk.iso");
        write_at(&p, 0x9311, b"CD001");

        let det = detect(&p).unwrap();
        assert_eq!(det.category, FormatCategory::OpticalImage);
        assert_eq!(det.format_hint, "iso9660-raw");
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_file_shorter_than_the_signature_offset_is_not_a_panic() {
        let d = tmp();
        let p = d.join("tiny.iso");
        fs::write(&p, vec![0u8; 100]).unwrap();

        // Must not panic and must not misreport — a 100-byte file cannot
        // possibly contain a signature at offset 0x8001.
        let det = detect(&p).unwrap();
        assert_ne!(det.category, FormatCategory::OpticalImage);
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn an_empty_file_is_not_a_panic() {
        let d = tmp();
        let p = d.join("empty.iso");
        fs::write(&p, []).unwrap();

        let det = detect(&p).unwrap();
        assert_eq!(det.category, FormatCategory::Unknown);
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn detects_rdb_hard_disk_by_signature() {
        let d = tmp();
        let p = d.join("disk.hdf");
        fs::write(&p, b"RDSK").unwrap();

        let det = detect(&p).unwrap();
        assert_eq!(det.category, FormatCategory::HardDiskImage);
        assert_eq!(det.format_hint, "rdb");
        assert!(det.confidence >= 0.9, "got {}", det.confidence);
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn detects_pfs3_hard_disk_by_signature() {
        let d = tmp();
        let p = d.join("disk.hdf");
        fs::write(&p, b"PFS\x03").unwrap();

        let det = detect(&p).unwrap();
        assert_eq!(det.category, FormatCategory::HardDiskImage);
        assert_eq!(det.format_hint, "pfs3");
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn detects_sfs_hard_disk_by_signature() {
        let d = tmp();
        let p = d.join("disk.hdf");
        fs::write(&p, b"SFS\x00").unwrap();

        let det = detect(&p).unwrap();
        assert_eq!(det.category, FormatCategory::HardDiskImage);
        assert_eq!(det.format_hint, "sfs");
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_dos_signature_at_harddisk_size_is_a_harddisk_not_a_floppy() {
        let d = tmp();
        let p = d.join("disk.img");
        let mut f = fs::File::create(&p).unwrap();
        f.write_all(b"DOS\x01").unwrap();
        let chunk = vec![0u8; 8192];
        // Larger than either known floppy size.
        let mut remaining = sizes::ADF_HD + chunk.len() as u64 - 4;
        while remaining > 0 {
            let n = chunk.len().min(remaining as usize);
            f.write_all(&chunk[..n]).unwrap();
            remaining -= n as u64;
        }
        drop(f);

        let det = detect(&p).unwrap();
        assert_eq!(det.category, FormatCategory::HardDiskImage);
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn probe_at_returns_empty_past_end_of_file_without_panicking() {
        let d = tmp();
        let p = d.join("tiny.bin");
        fs::write(&p, vec![0u8; 10]).unwrap();

        let result = probe_at(&p, 0x9311, 5).unwrap();
        assert!(result.is_empty());

        let empty = d.join("empty.bin");
        fs::write(&empty, []).unwrap();
        let result = probe_at(&empty, 0, 5).unwrap();
        assert!(result.is_empty());

        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn rom_known_size_has_higher_confidence() {
        let d = tmp();
        let p = d.join("kick.rom");
        let mut f = fs::File::create(&p).unwrap();
        let chunk = vec![0u8; 8192];
        let mut remaining = 512 * 1024u64;
        while remaining > 0 {
            let n = chunk.len().min(remaining as usize);
            f.write_all(&chunk[..n]).unwrap();
            remaining -= n as u64;
        }
        let det = detect(&p).unwrap();
        assert_eq!(det.category, FormatCategory::Rom);
        assert!(det.confidence >= 0.7);
        fs::remove_dir_all(&d).ok();
    }
}
