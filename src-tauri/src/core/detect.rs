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
/// # The wire strings are spelled out, not derived
///
/// These names cross to the frontend, so they are a contract with
/// `src/types/index.ts`. `rename_all` cannot express them: `"lowercase"` gives
/// `floppyimage`, and `"kebab-case"` gives `hard-disk-image` where the
/// contract says `harddisk-image`. Both silently disagree with
/// [`FormatCategory::as_str`], which is what the Rust side uses — a `===`
/// against `"floppy-image"` in TypeScript would type-check and never match at
/// runtime. `serde_name_matches_as_str` pins the two together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FormatCategory {
    /// Floppy disk image (ADF, ADZ, DMS).
    #[serde(rename = "floppy-image")]
    FloppyImage,
    /// Hard disk image (HDF, HDZ).
    #[serde(rename = "harddisk-image")]
    HardDiskImage,
    /// Optical disc image (ISO9660, raw CD track). Detection only — ART does
    /// not yet read the filesystem inside one (spec §10, §89).
    #[serde(rename = "optical-image")]
    OpticalImage,
    /// Archive, typically LHA on Amiga.
    #[serde(rename = "archive")]
    Archive,
    /// A Commodore 8-bit disk, tape or program file (spec addendum §10.5).
    ///
    /// Its own category rather than [`FormatCategory::FloppyImage`], and the
    /// reason is not tidiness: a D64 routed to the floppy workflows would be
    /// offered ADF Studio, disk validation and **"copy to Gotek"** — writing a
    /// 1541 image onto a Gotek as though it were an Amiga floppy. The format
    /// within the category is `format_hint`: `d64`, `d71`, `d81`, `t64`,
    /// `tap`, `prg`, `crt`.
    #[serde(rename = "commodore-8bit")]
    Commodore8Bit,
    /// Kickstart ROM.
    #[serde(rename = "rom")]
    Rom,
    /// A plain directory (dropped folder).
    #[serde(rename = "directory")]
    Directory,
    /// Unknown / unsupported file.
    #[serde(rename = "unknown")]
    Unknown,
}

impl FormatCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FloppyImage => "floppy-image",
            Self::HardDiskImage => "harddisk-image",
            Self::OpticalImage => "optical-image",
            Self::Archive => "archive",
            Self::Commodore8Bit => "commodore-8bit",
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

/// An LHA header's compression-method field, `-lh5-` and friends, sits at
/// **offset 2**: the header length and its checksum come first. It is five
/// bytes — a dash, two letters naming the family, a level digit, a dash.
///
/// ART-076: this used to be matched at offset 0, which no LHA tool has ever
/// written, so a real archive was recognised only by its extension and one
/// renamed to `.dat` was not recognised at all.
const LHA_METHOD_OFFSET: usize = 2;

/// The families that appear in that field: `-lh?-` (the LZSS/Huffman
/// generations plus `-lhd-` for a directory), `-lz?-` (the older LArc
/// methods) and `-pm?-` (PMarc, occasionally seen on Amiga disks).
const LHA_FAMILIES: [&[u8; 2]; 3] = [b"lh", b"lz", b"pm"];

/// ZIP's local-file header, `PK\x03\x04`. An empty archive starts with the
/// end-of-central-directory record (`PK\x05\x06`) and a spanned one with
/// `PK\x07\x08`; all three are ZIPs.
const ZIP_MAGIC_PREFIX: &[u8; 2] = b"PK";

/// 7z: `7z` then `BC AF 27 1C`.
const SEVENZ_MAGIC: &[u8; 6] = b"7z\xBC\xAF\x27\x1C";

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

/// Offset of "CD001" in a raw 2352-byte track whose sectors are **Mode 2/XA
/// Form 1**. Those carry an 8-byte subheader after the header, so their user
/// data begins at offset 24 rather than 16: 0x9300 + 24 + 1 = 0x9319.
///
/// Probing this separately is what stops ART being wrong twice over. The
/// reader takes its data offset from the layout detection reports, so a disc
/// recognised as Mode 1 when it is really Mode 2 would be misread by
/// detection and reader together — the shape of ART-032…035, recorded as
/// ART-075. CD32 and mixed-mode discs are written this way.
const ISO_PVD_OFFSET_2352_XA: u64 = 0x9319;

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
    if let Ok(sig) = probe_at(path, ISO_PVD_OFFSET_2352_XA, ISO_MAGIC.len()) {
        if sig == ISO_MAGIC {
            return Ok(Detection {
                category: FormatCategory::OpticalImage,
                format_hint: "iso9660-raw-xa".to_string(),
                confidence: 0.9,
                size,
                is_dir: false,
            });
        }
    }

    // --- Commodore 8-bit ---------------------------------------------------
    //
    // Tapes and cartridges carry a signature; disks carry nothing at all.
    if let Ok(head) = read_head(path, 16) {
        // Order matters here, and it is not alphabetical: all three of these
        // begin `C64`, and the T64 check is only those three bytes because
        // tools disagreed about the rest of its description field. The
        // specific magics have to be asked first, or every tape dump and
        // every cartridge is reported as a T64.
        if head.len() >= TAP_MAGIC.len() && &head[0..TAP_MAGIC.len()] == TAP_MAGIC {
            return Ok(commodore("tap", 0.95, size));
        }
        if head.len() >= CRT_MAGIC.len() && &head[0..CRT_MAGIC.len()] == CRT_MAGIC {
            return Ok(commodore("crt", 0.95, size));
        }
        if head.len() >= T64_MAGIC.len() && &head[0..T64_MAGIC.len()] == T64_MAGIC {
            return Ok(commodore("t64", 0.95, size));
        }
    }
    if let Some(detection) = commodore_disk_by_size(path, size) {
        return Ok(detection);
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
    }

    // Archives, all three by signature. A longer head than the four bytes
    // above, because LHA's evidence starts two bytes in and 7z's runs to six.
    if let Ok(head) = read_head(path, 8) {
        if is_lha_header(&head) {
            return Ok(Detection {
                category: FormatCategory::Archive,
                format_hint: "lha".to_string(),
                confidence: 0.95,
                size,
                is_dir: false,
            });
        }
        if head.len() >= 4 && &head[0..2] == ZIP_MAGIC_PREFIX && matches!(head[2], 3 | 5 | 7) {
            return Ok(Detection {
                category: FormatCategory::Archive,
                format_hint: "zip".to_string(),
                confidence: 0.95,
                size,
                is_dir: false,
            });
        }
        if head.len() >= 6 && &head[0..6] == SEVENZ_MAGIC {
            return Ok(Detection {
                category: FormatCategory::Archive,
                format_hint: "7z".to_string(),
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

/// The first three bytes of a T64's 32-byte description field. Tools wrote
/// several wordings — "C64 tape image file", "C64S tape file" — so only the
/// part they agree on is checked.
const T64_MAGIC: &[u8] = b"C64";

/// A raw tape dump says so in full.
const TAP_MAGIC: &[u8] = b"C64-TAPE-RAW";

/// A cartridge image, padded to sixteen bytes.
const CRT_MAGIC: &[u8] = b"C64 CARTRIDGE";

/// One detection for a Commodore 8-bit object.
fn commodore(hint: &str, confidence: f32, size: u64) -> Detection {
    Detection {
        category: FormatCategory::Commodore8Bit,
        format_hint: hint.to_string(),
        confidence,
        size,
        is_dir: false,
    }
}

/// A `.d64` and its relatives have **no header and no signature**: the file is
/// the sectors, starting at track 1 sector 0. Size is the only thing to go on,
/// and it is enough to be worth acting on because the sizes are exact — every
/// one is a whole number of sectors for a real drive, and none collides with
/// an Amiga image.
///
/// The header sector is then read to *raise* confidence rather than to gate:
/// a 1541 writes `A` as its DOS version at offset 2 of track 18 sector 0, a
/// 1581 writes `D` at the same offset of track 40 sector 0. A disk whose
/// header says something else is still that size and still very probably that
/// disk, so it is reported at the confidence size alone deserves.
fn commodore_disk_by_size(path: &Path, size: u64) -> Option<Detection> {
    use crate::core::cbm::geometry::{Drive, Geometry};

    let geometry = Geometry::from_len(size).ok()?;
    let hint = match geometry.drive {
        Drive::D64 => "d64",
        Drive::D71 => "d71",
        Drive::D81 => "d81",
    };

    let expected_dos = match geometry.drive {
        Drive::D64 | Drive::D71 => b'A',
        Drive::D81 => b'D',
    };
    let header_at = geometry.offset_of(geometry.directory_track(), 0).ok()?;
    let confirmed = probe_at(path, header_at + 2, 1)
        .map(|byte| byte == [expected_dos])
        .unwrap_or(false);

    Some(commodore(hint, if confirmed { 0.9 } else { 0.7 }, size))
}

/// True when `head` carries an LHA compression-method field where one belongs.
///
/// `-lh5-` at offset 2: dash, family, level digit, dash. Checking the closing
/// dash as well as the opening one is what keeps this from matching arbitrary
/// bytes that happen to start `-l`.
fn is_lha_header(head: &[u8]) -> bool {
    const FIELD_LEN: usize = 5;
    if head.len() < LHA_METHOD_OFFSET + FIELD_LEN {
        return false;
    }
    let field = &head[LHA_METHOD_OFFSET..LHA_METHOD_OFFSET + FIELD_LEN];
    field[0] == b'-'
        && field[4] == b'-'
        && LHA_FAMILIES.iter().any(|f| &field[1..3] == f.as_slice())
        && field[3].is_ascii_alphanumeric()
}

/// Read up to `n` bytes from the start of a file.
///
/// Public so `core::archive` can decide which backend opens a file from the
/// same bytes detection reads, rather than growing a second head-reader that
/// could drift from this one.
pub fn read_head(path: &Path, n: usize) -> CoreResult<Vec<u8>> {
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

    /// The wire strings and `as_str` must never drift: the frontend's
    /// `FormatCategory` union in `src/types/index.ts` is written from
    /// `as_str`, but what actually arrives is what serde produced. They
    /// disagreed for every variant until this was pinned — serde said
    /// `floppyimage` while everything else said `floppy-image`. Nothing
    /// compared them at runtime yet, so nothing failed; the first `===` in
    /// TypeScript would have type-checked and never matched.
    #[test]
    fn serde_name_matches_as_str() {
        for category in [
            FormatCategory::FloppyImage,
            FormatCategory::HardDiskImage,
            FormatCategory::OpticalImage,
            FormatCategory::Archive,
            FormatCategory::Rom,
            FormatCategory::Directory,
            FormatCategory::Unknown,
        ] {
            let json = serde_json::to_string(&category).unwrap();
            let wire = json.trim_matches('"');
            assert_eq!(
                wire,
                category.as_str(),
                "{category:?} goes over the wire as {wire:?}, but as_str says {:?}",
                category.as_str()
            );
            // And it round-trips, so a persisted or logged value still reads.
            let back: FormatCategory = serde_json::from_str(&json).unwrap();
            assert_eq!(back, category);
        }
    }

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

    /// ART-076. A real LHA carries its `-lh5-` method field at offset **2**,
    /// after the header length and its checksum — never at offset 0. This
    /// test used to write `-lh5-` at the start of the file, which is not a
    /// thing any LHA tool produces, so it passed while content-first
    /// detection of the format ART was built for did not work at all: a
    /// genuine `.lha` was recognised only by its extension, and one renamed to
    /// `.dat` was `unknown`. The fixture is now a real archive, from the same
    /// builder the LHA tests use.
    #[test]
    fn detects_lha_by_signature() {
        let d = tmp();
        let p = d.join("game.dat"); // the extension deliberately says nothing
        fs::write(
            &p,
            crate::core::lha::tests::make_lha_with(&[("hi.txt", b"hi")]),
        )
        .unwrap();
        let det = detect(&p).unwrap();
        assert_eq!(det.category, FormatCategory::Archive);
        assert_eq!(det.format_hint, "lha");
        assert!(det.confidence >= 0.9);
        fs::remove_dir_all(&d).ok();
    }

    /// And the shape that used to pass is not an archive at all.
    #[test]
    fn a_method_field_at_offset_zero_is_not_an_lha() {
        let d = tmp();
        let p = d.join("bogus.dat");
        fs::write(&p, b"-lh5-not-an-archive").unwrap();
        assert_ne!(detect(&p).unwrap().category, FormatCategory::Archive);
        fs::remove_dir_all(&d).ok();
    }

    /// A D64 has no signature at all — the file is the sectors. Size decides,
    /// and the header sector is read to raise confidence rather than to gate.
    #[test]
    fn detects_a_c64_disk_by_size_and_confirms_it_by_its_header() {
        let d = tmp();

        // A real fixture, so the header byte is where a drive would put it.
        let p = d.join("game.d64");
        fs::write(
            &p,
            crate::core::cbm::d64::fixture::D64Builder::new(35).build(),
        )
        .unwrap();
        let det = detect(&p).unwrap();
        assert_eq!(det.category, FormatCategory::Commodore8Bit);
        assert_eq!(det.format_hint, "d64");
        assert!(
            det.confidence >= 0.9,
            "header confirmed: {}",
            det.confidence
        );

        // The same size with nothing recognisable in the header: still the
        // right answer, reported at the confidence size alone earns.
        let plain = d.join("mystery.dat");
        fs::write(&plain, vec![0u8; 174_848]).unwrap();
        let det = detect(&plain).unwrap();
        assert_eq!(det.category, FormatCategory::Commodore8Bit);
        assert_eq!(det.format_hint, "d64");
        assert!(det.confidence < 0.9, "size only: {}", det.confidence);

        // And the 40-track variant amendment A3 added.
        let big = d.join("speeddos.d64");
        fs::write(&big, vec![0u8; 196_608]).unwrap();
        assert_eq!(detect(&big).unwrap().format_hint, "d64");

        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn detects_the_other_commodore_disk_sizes() {
        let d = tmp();
        for (bytes, hint) in [(349_696usize, "d71"), (819_200, "d81")] {
            let p = d.join(format!("{hint}.img"));
            fs::write(&p, vec![0u8; bytes]).unwrap();
            let det = detect(&p).unwrap();
            assert_eq!(det.category, FormatCategory::Commodore8Bit, "{hint}");
            assert_eq!(det.format_hint, hint);
        }
        fs::remove_dir_all(&d).ok();
    }

    /// The tape and cartridge formats do carry signatures, and each is
    /// identified as itself — `tap` and `crt` are identify-only, which is a
    /// property of the formats rather than a gap (§10, §89).
    #[test]
    fn detects_the_signed_commodore_formats() {
        let d = tmp();
        for (magic, hint) in [
            (b"C64 tape image file".to_vec(), "t64"),
            (b"C64-TAPE-RAW".to_vec(), "tap"),
            (b"C64 CARTRIDGE   ".to_vec(), "crt"),
        ] {
            let p = d.join(format!("{hint}.bin"));
            let mut bytes = magic;
            bytes.resize(512, 0);
            fs::write(&p, &bytes).unwrap();
            let det = detect(&p).unwrap();
            assert_eq!(det.category, FormatCategory::Commodore8Bit, "{hint}");
            assert_eq!(det.format_hint, hint);
            assert!(det.confidence >= 0.9, "{hint}");
        }
        fs::remove_dir_all(&d).ok();
    }

    /// An Amiga floppy is not a Commodore 8-bit disk, whatever the sizes look
    /// like next to each other: 901,120 is not one of the accepted sizes, and
    /// the `DOS` signature wins in any case.
    #[test]
    fn an_adf_is_not_mistaken_for_a_c64_disk() {
        let d = tmp();
        let p = d.join("disk.adf");
        let mut bytes = vec![0u8; sizes::ADF_DD as usize];
        bytes[0..4].copy_from_slice(b"DOS\x00");
        fs::write(&p, &bytes).unwrap();

        assert_eq!(detect(&p).unwrap().category, FormatCategory::FloppyImage);
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn detects_zip_by_signature() {
        let d = tmp();
        let p = d.join("pack.dat");
        fs::write(
            &p,
            crate::core::archive::zip::tests::make_zip_with(&[("readme.txt", b"hi")]),
        )
        .unwrap();
        let det = detect(&p).unwrap();
        assert_eq!(det.category, FormatCategory::Archive);
        assert_eq!(det.format_hint, "zip");
        assert!(det.confidence >= 0.9);
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn detects_7z_by_signature() {
        let d = tmp();
        let p = d.join("pack.dat");
        fs::write(
            &p,
            crate::core::archive::sevenz::tests::make_7z_with(&[("readme.txt", b"hi")]),
        )
        .unwrap();
        let det = detect(&p).unwrap();
        assert_eq!(det.category, FormatCategory::Archive);
        assert_eq!(det.format_hint, "7z");
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

    /// ART-075: a Mode 2/XA Form 1 disc carries an 8-byte subheader, so its
    /// user data starts at 24 rather than 16 and `CD001` lands eight bytes
    /// further on. Probing only 0x9311 does not merely miss it — detection and
    /// the reader take the data offset from the *same* assumption, so a disc
    /// that did somehow get through would be misread by both together. CD32
    /// and mixed-mode discs are where this appears.
    #[test]
    fn a_raw_mode2_xa_track_is_detected_at_0x9319() {
        let d = tmp();
        let p = d.join("cd32.iso");
        write_at(&p, 0x9319, b"CD001");

        let det = detect(&p).unwrap();
        assert_eq!(det.category, FormatCategory::OpticalImage);
        assert_eq!(det.format_hint, "iso9660-raw-xa");
        fs::remove_dir_all(&d).ok();
    }

    /// And the two raw offsets do not collide: a Mode 1 disc is still Mode 1,
    /// not an XA disc that happens to have a byte there.
    #[test]
    fn a_mode1_raw_track_is_not_reported_as_xa() {
        let d = tmp();
        let p = d.join("mode1.iso");
        write_at(&p, 0x9311, b"CD001");

        assert_eq!(detect(&p).unwrap().format_hint, "iso9660-raw");
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
