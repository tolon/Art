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
pub fn strip_cloanto_header(bytes: &[u8]) -> Vec<u8> {
    if bytes.starts_with(CLOANTO_HEADER) && bytes.len() > 11 {
        bytes[11..].to_vec()
    } else {
        bytes.to_vec()
    }
}

/// Verify standard Kickstart 32-bit checksum (sum of all 32-bit big-endian words with carry).
pub fn verify_kickstart_checksum(bytes: &[u8]) -> bool {
    if bytes.len() < 4 || bytes.len() % 4 != 0 {
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

    #[test]
    fn crc32_empty_and_known_string() {
        assert_eq!(compute_crc32(b""), 0);
        // Standard test vector: "123456789" -> 0xCBF43926
        assert_eq!(compute_crc32(b"123456789"), 0xCBF4_3926);
    }
}
