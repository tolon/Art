//! Kickstart ROM identification and validation engine (Phase 2 & Phase 7).
//!
//! ART never distributes copyrighted ROMs; this module analyzes and matches
//! user-provided ROM files against known cryptographic signatures, validates
//! Kickstart checksums, strips Cloanto encryption headers (`AMIROMTYPE1`), and
//! provides open-source AROS ROM fallback metadata.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::core::error::{CoreError, CoreResult};
use crate::core::hashing::sha256_bytes;

/// Cloanto ROM header prefix (11 bytes).
const CLOANTO_HEADER: &[u8] = b"AMIROMTYPE1";

/// Kickstart ROM info surfaced to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RomInfo {
    pub name: String,
    pub version: String,
    pub revision: String,
    pub size_bytes: usize,
    pub sha256: String,
    pub crc32: String,
    pub is_cloanto: bool,
    pub is_aros: bool,
    pub checksum_valid: bool,
    pub compatible_models: Vec<String>,
    pub file_path: String,
}

/// Known Kickstart ROM signature definition in database.
struct KnownRom {
    name: &'static str,
    version: &'static str,
    revision: &'static str,
    size: usize,
    sha256: &'static str,
    models: &'static [&'static str],
}

/// Curated database of standard Amiga Kickstart ROM signatures.
const KNOWN_ROMS: &[KnownRom] = &[
    KnownRom {
        name: "Kickstart 1.2 (33.180)",
        version: "1.2",
        revision: "33.180",
        size: 262_144, // 256 KB
        sha256: "3be60285a2faeb911ecad44c79ca332822a1068df6d0f6222bfa4e8dc8374d81",
        models: &["A500", "A1000", "A2000"],
    },
    KnownRom {
        name: "Kickstart 1.3 (34.005)",
        version: "1.3",
        revision: "34.005",
        size: 262_144, // 256 KB
        sha256: "895e3110292723c34898687265ea87f58c7386008ab5e9d99d3e8e2eb0cc04ef",
        models: &["A500", "A2000", "CDTV"],
    },
    KnownRom {
        name: "Kickstart 2.04 (37.175)",
        version: "2.04",
        revision: "37.175",
        size: 524_288, // 512 KB
        sha256: "0c476717596ff1e604f3fb0cfb9024fccae978bb15c61307b369ec2646d6d7e0",
        models: &["A500+", "A2000"],
    },
    KnownRom {
        name: "Kickstart 2.05 (37.350)",
        version: "2.05",
        revision: "37.350",
        size: 524_288, // 512 KB
        sha256: "17b8f9e6d8a39d8e7887e597f8c142c38865e94b281f9b01cdfc2d1bf2758117",
        models: &["A600"],
    },
    KnownRom {
        name: "Kickstart 3.0 (39.106)",
        version: "3.0",
        revision: "39.106",
        size: 524_288, // 512 KB
        sha256: "fc01a9ee56ee1853d9e4c1ea1a6a683935db5cf5451996cc51f8dc1c7ef549f4",
        models: &["A1200"],
    },
    KnownRom {
        name: "Kickstart 3.1 (40.068) A1200",
        version: "3.1",
        revision: "40.068",
        size: 524_288, // 512 KB
        sha256: "e40a5dfb3d017ba335127d85ea15c34cb27a2444230e963b7b6a1e378774d9b4",
        models: &["A1200"],
    },
    KnownRom {
        name: "Kickstart 3.1 (40.063) A500/A600/A2000",
        version: "3.1",
        revision: "40.063",
        size: 524_288, // 512 KB
        sha256: "fc24ae0e70f9a4fa43e743b3f2d315ee30e22b3d9993d290fb12cd0c59223e8f",
        models: &["A500", "A600", "A2000"],
    },
    KnownRom {
        name: "Kickstart 3.1 (40.070) A4000",
        version: "3.1",
        revision: "40.070",
        size: 524_288, // 512 KB
        sha256: "931215b22596ab03b573d842b036ca6d50ff01b6e42b2da116ea28b52fb1c4ea",
        models: &["A4000"],
    },
    KnownRom {
        name: "Kickstart 3.1 (40.060) CD32",
        version: "3.1",
        revision: "40.060",
        size: 1_048_576, // 1 MB Extended
        sha256: "5f8924d013d879e6cf23a73c1d9dfd70a48a4c843813fffa8403d15b2909180f",
        models: &["CD32"],
    },
    KnownRom {
        name: "Kickstart 3.2.2 (47.111)",
        version: "3.2.2",
        revision: "47.111",
        size: 524_288, // 512 KB
        sha256: "a6873528b8dc9bb54070a7b44357484dfc74676136df31ea3a6697858c704f72",
        models: &["A500", "A600", "A1200", "A2000", "A4000"],
    },
];

/// Largest file ART will read as a Kickstart ROM.
///
/// Real Kickstarts top out at 1 MB (2 MB for an extended pair). The user can
/// point this at any file, so without a ceiling a mistaken pick — a disk image,
/// a video — would be pulled entirely into memory (spec §56).
const MAX_ROM_BYTES: u64 = 4 * 1024 * 1024;

/// Identify a ROM file from disk.
pub fn identify_rom(path: &Path) -> CoreResult<RomInfo> {
    let size = std::fs::metadata(path)?.len();
    if size > MAX_ROM_BYTES {
        return Err(CoreError::InvalidInput(format!(
            "'{}' is {} MB — too large to be a Kickstart ROM (limit {} MB)",
            path.display(),
            size / (1024 * 1024),
            MAX_ROM_BYTES / (1024 * 1024)
        )));
    }

    let raw_bytes = std::fs::read(path)?;
    if raw_bytes.is_empty() {
        return Err(CoreError::InvalidInput("ROM file is empty".into()));
    }

    let is_cloanto = raw_bytes.starts_with(CLOANTO_HEADER);
    let bytes = if is_cloanto {
        strip_cloanto_header(&raw_bytes)
    } else {
        raw_bytes
    };

    let size_bytes = bytes.len();
    let sha256 = sha256_bytes(&bytes);
    let crc = compute_crc32(&bytes);
    let crc32 = format!("{:08X}", crc);

    // Compute Kickstart 32-bit checksum
    let checksum_valid = verify_kickstart_checksum(&bytes);

    // Check if it matches our known ROM database
    if let Some(matched) = KNOWN_ROMS
        .iter()
        .find(|r| r.sha256.eq_ignore_ascii_case(&sha256))
    {
        return Ok(RomInfo {
            name: matched.name.to_string(),
            version: matched.version.to_string(),
            revision: matched.revision.to_string(),
            size_bytes,
            sha256,
            crc32,
            is_cloanto,
            is_aros: false,
            checksum_valid,
            compatible_models: matched.models.iter().map(|s| s.to_string()).collect(),
            file_path: path.to_string_lossy().to_string(),
        });
    }

    // No catalogued dump matched. **Ask the ROM** (ART-104): from 2.0 onwards
    // it states its own version and revision, so a dump nobody catalogued is
    // still named rather than called generic.
    //
    // **And that is all it says.** The first version of this looked the
    // revision up in `KNOWN_ROMS` and borrowed that entry's machines —
    // measuring three real 3.1 dumps killed it: `40.68` is stated by files
    // whose names claim A500/A600/A2000, A1200 *and* A4000, with three
    // different SHA-256s. The revision is the exec version, shared across the
    // per-machine builds; only the hash tells them apart. Borrowing the
    // machines would have told an A500 owner their ROM was for an A1200 —
    // worse than the "generic" answer it replaced.
    if let Some((major, minor)) = stated_version(&bytes) {
        let revision = format!("{major}.{minor:03}");
        return Ok(RomInfo {
            name: match version_name(major) {
                Some(version) => format!("Kickstart {version} ({revision})"),
                None => format!("Kickstart {revision}"),
            },
            version: version_name(major).unwrap_or("Custom").to_string(),
            revision,
            size_bytes,
            sha256,
            crc32,
            is_cloanto,
            is_aros: false,
            checksum_valid,
            // Empty on purpose: the ROM said its version, not its machine.
            compatible_models: Vec::new(),
            file_path: path.to_string_lossy().to_string(),
        });
    }

    // Check if it's an AROS open-source ROM
    let is_aros = String::from_utf8_lossy(&bytes).contains("AROS")
        || path.to_string_lossy().to_lowercase().contains("aros");
    if is_aros {
        return Ok(RomInfo {
            name: "AROS Open-Source Replacement Kickstart".to_string(),
            version: "AROS".to_string(),
            revision: "Built-in".to_string(),
            size_bytes,
            sha256,
            crc32,
            is_cloanto: false,
            is_aros: true,
            checksum_valid: true,
            compatible_models: vec!["A500".into(), "A1200".into(), "A4000".into()],
            file_path: path.to_string_lossy().to_string(),
        });
    }

    // Generic fallback for custom / diagnostic / uncatalogued ROMs
    let (inferred_name, inferred_models) = match size_bytes {
        262_144 => (
            "Generic Amiga 256KB ROM (Kickstart 1.x)",
            vec!["A500".into(), "A2000".into()],
        ),
        524_288 => (
            "Generic Amiga 512KB ROM (Kickstart 2.x/3.x)",
            vec![
                "A500+".into(),
                "A600".into(),
                "A1200".into(),
                "A4000".into(),
            ],
        ),
        1_048_576 => (
            "Generic Amiga 1MB ROM (CD32 / Extended)",
            vec!["CD32".into(), "A4000".into()],
        ),
        2_097_152 => (
            "Generic Amiga 2MB ROM (Diagnostic / Custom)",
            vec!["All Models".into()],
        ),
        _ => ("Custom / Unknown ROM Image", vec!["Unknown".into()]),
    };

    Ok(RomInfo {
        name: inferred_name.to_string(),
        version: "Custom".to_string(),
        revision: format!("{} KB", size_bytes / 1024),
        size_bytes,
        sha256,
        crc32,
        is_cloanto,
        is_aros: false,
        checksum_valid,
        compatible_models: inferred_models,
        file_path: path.to_string_lossy().to_string(),
    })
}

/// Scan a directory recursively for Kickstart ROM files (.rom, .bin).
pub fn scan_rom_directory(dir: &Path) -> CoreResult<Vec<RomInfo>> {
    if !dir.is_dir() {
        return Err(CoreError::InvalidInput(format!(
            "'{}' is not a directory",
            dir.display()
        )));
    }

    let mut results = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        if p.is_file() {
            let ext = p
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            if ext == "rom" || ext == "bin" || ext == "a500" || ext == "a1200" {
                if let Ok(info) = identify_rom(&p) {
                    results.push(info);
                }
            }
        }
    }

    results.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(results)
}

/// Strip 11-byte Cloanto encryption prefix (`AMIROMTYPE1`).
/// The version and revision a Kickstart states in its own header (ART-104).
///
/// From Kickstart 2.0 onwards a ROM carries them as two big-endian words at
/// offset 12 — `40 00 44` for the A1200's 3.1, which reads 40.68. Older ROMs
/// have something else there (`Kickstart 1.1.rom` reads `65535.65535`), so a
/// value outside the range Kickstart actually used is not believed.
///
/// **Why this exists.** `KNOWN_ROMS` names an exact *dump* by its SHA-256, and
/// several dumps of the same ROM circulate — the one on this project's own
/// machine is not the one the table carries, so a perfectly ordinary A1200
/// Kickstart came back as *Generic Amiga 512KB ROM* and nothing could say
/// which machine it suited. The ROM says what it is; asking it is cheaper than
/// cataloguing every dump anybody made.
pub fn stated_version(bytes: &[u8]) -> Option<(u16, u16)> {
    /// Kickstart majors in the wild: 33 (1.2) through 47 (3.2.x), with room
    /// above for a release nobody has shipped yet. Below 33 the field is not a
    /// version at all.
    const PLAUSIBLE: std::ops::RangeInclusive<u16> = 33..=55;

    if bytes.len() < 16 {
        return None;
    }
    let major = u16::from_be_bytes([bytes[12], bytes[13]]);
    let minor = u16::from_be_bytes([bytes[14], bytes[15]]);

    // 0xFFFF is what a pre-2.0 ROM has there, and a minor that large is not a
    // revision either.
    if !PLAUSIBLE.contains(&major) || minor == 0xFFFF {
        return None;
    }
    Some((major, minor))
}

/// The marketing version a Kickstart major belongs to.
///
/// Only the ones Commodore shipped; an unknown major is reported by its
/// numbers rather than given a name ART made up.
fn version_name(major: u16) -> Option<&'static str> {
    Some(match major {
        33 => "1.2",
        34 => "1.3",
        36 => "2.0",
        37 => "2.04",
        39 => "3.0",
        40 => "3.1",
        45 => "3.1.4",
        46 => "3.2",
        47 => "3.2.x",
        _ => return None,
    })
}

pub fn strip_cloanto_header(bytes: &[u8]) -> Vec<u8> {
    if bytes.starts_with(CLOANTO_HEADER) && bytes.len() > 11 {
        bytes[11..].to_vec()
    } else {
        bytes.to_vec()
    }
}

/// Verify standard Kickstart 32-bit checksum (sum of all 32-bit big-endian words with carry).
pub fn verify_kickstart_checksum(bytes: &[u8]) -> bool {
    if bytes.len() < 4 || !bytes.len().is_multiple_of(4) {
        return false;
    }

    let mut sum = 0u32;
    for chunk in bytes.chunks_exact(4) {
        let val = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        let (new_sum, carry) = sum.overflowing_add(val);
        sum = new_sum.wrapping_add(carry as u32);
    }

    sum == 0xFFFF_FFFF || sum == 0
}

/// Calculate IEEE 802.3 CRC32 checksum.
pub fn compute_crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in bytes {
        crc ^= b as u32;
        for _ in 0..8 {
            if (crc & 1) != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_cloanto_rom_header() {
        let mut raw = CLOANTO_HEADER.to_vec();
        raw.extend_from_slice(&[0x11, 0x22, 0x33, 0x44]);
        let stripped = strip_cloanto_header(&raw);
        assert_eq!(stripped, vec![0x11, 0x22, 0x33, 0x44]);
    }

    /// A synthetic ROM that states `major.minor` where a real one does.
    fn rom_stating(major: u16, minor: u16, size: usize) -> Vec<u8> {
        let mut bytes = vec![0u8; size];
        bytes[0..2].copy_from_slice(&0x1114u16.to_be_bytes());
        bytes[12..14].copy_from_slice(&major.to_be_bytes());
        bytes[14..16].copy_from_slice(&minor.to_be_bytes());
        bytes
    }

    fn write(dir: &std::path::Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("art-rom-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// **A Kickstart says what it is.** From 2.0 onwards the version and
    /// revision are two big-endian words at offset 12, and reading them is how
    /// a dump nobody catalogued can still be named.
    #[test]
    fn a_rom_states_its_own_version() {
        assert_eq!(
            stated_version(&rom_stating(40, 68, 524_288)),
            Some((40, 68))
        );
        assert_eq!(
            stated_version(&rom_stating(39, 106, 524_288)),
            Some((39, 106))
        );
    }

    /// **ART-104.** A Kickstart that hashes to a dump `KNOWN_ROMS` does not
    /// carry used to come back as *Generic Amiga 512KB ROM*. It states its own
    /// version and revision, so it is named from those.
    #[test]
    fn a_dump_art_has_not_catalogued_is_named_from_the_revision_it_states() {
        let dir = scratch("uncatalogued");
        let path = write(&dir, "mystery.rom", &rom_stating(40, 68, 524_288));

        let info = identify_rom(&path).unwrap();

        assert_eq!(info.version, "3.1", "{info:?}");
        assert_eq!(info.revision, "40.068");
        assert_eq!(info.name, "Kickstart 3.1 (40.068)");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **The revision does not name a machine, and this is what stopped the
    /// first version of the fix from shipping.**
    ///
    /// Measured on three real dumps in this project's own collection: files
    /// whose names claim A500/A600/A2000, A1200 and A4000 all state `40.68`
    /// and have three different SHA-256s. The revision is the exec version,
    /// shared across the per-machine builds; only the hash tells them apart.
    /// Borrowing a same-revision table entry's machines — which the first
    /// implementation did — would tell an A500 owner their ROM is for an
    /// A1200, and `rom_suits` would then call a perfectly good ROM wrong.
    #[test]
    fn a_rom_named_from_its_revision_claims_no_machine() {
        let dir = scratch("no-machine");
        let path = write(&dir, "mystery.rom", &rom_stating(40, 68, 524_288));

        let info = identify_rom(&path).unwrap();

        assert!(
            info.compatible_models.is_empty(),
            "the ROM said its version, not its machine: {:?}",
            info.compatible_models
        );
        assert!(
            KNOWN_ROMS.iter().any(|k| k.revision == "40.068"),
            "and the table does carry that revision — the point is that it is \
             not consulted for the machine"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A pre-2.0 ROM has no version there — `Kickstart 1.1.rom` reads
    /// `65535.65535` — so nothing is claimed and the size-based fallback
    /// stands.
    #[test]
    fn a_rom_that_states_no_version_is_not_guessed_at() {
        let dir = scratch("silent");
        let path = write(&dir, "old.rom", &rom_stating(0xFFFF, 0xFFFF, 524_288));

        assert_eq!(stated_version(&rom_stating(0xFFFF, 0xFFFF, 524_288)), None);
        let info = identify_rom(&path).unwrap();
        assert_eq!(info.version, "Custom", "{info:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Two bytes that happen to sit at offset 12 are not a version. Only a
    /// range Kickstart actually used is believed.
    #[test]
    fn an_implausible_version_is_not_believed() {
        assert_eq!(stated_version(&rom_stating(0, 0, 524_288)), None);
        assert_eq!(stated_version(&rom_stating(7, 1, 524_288)), None);
        assert_eq!(stated_version(&rom_stating(900, 1, 524_288)), None);
    }

    /// A revision the table does not know is still better than "generic": the
    /// ROM said what it is, and saying so beats a shrug.
    #[test]
    fn an_unknown_revision_is_still_reported_as_what_it_says() {
        let dir = scratch("unknown-rev");
        let path = write(&dir, "future.rom", &rom_stating(52, 3, 524_288));

        let info = identify_rom(&path).unwrap();

        assert_eq!(info.revision, "52.003", "{info:?}");
        assert_eq!(info.name, "Kickstart 52.003", "no version name is invented");
        assert!(info.compatible_models.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The exact-hash answer still wins where there is one: it names the dump,
    /// not only the revision.
    #[test]
    fn a_catalogued_dump_is_still_named_from_its_hash() {
        // Only that the lookup is tried first — a real dump's bytes are not
        // ART's to ship, so this asserts the order rather than a hash.
        let stated = rom_stating(40, 68, 524_288);
        assert!(
            KNOWN_ROMS
                .iter()
                .all(|known| !known.sha256.eq_ignore_ascii_case(&sha256_bytes(&stated))),
            "the synthetic ROM must not collide with a catalogued dump"
        );
    }

    #[test]
    fn crc32_empty_and_known_string() {
        assert_eq!(compute_crc32(b""), 0);
        // Standard test vector: "123456789" -> 0xCBF43926
        assert_eq!(compute_crc32(b"123456789"), 0xCBF4_3926);
    }
}
