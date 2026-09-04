//! Kickstart ROM identification and validation engine (Phase 2 & Phase 7).
//!
//! ART never distributes copyrighted ROMs; this module analyzes and matches
//! user-provided ROM files against known signatures, validates Kickstart
//! checksums, strips Cloanto encryption headers (`AMIROMTYPE1`), and provides
//! open-source AROS ROM fallback metadata.
//!
//! ## Three questions, asked in order (ART-104)
//!
//! 1. **What the ROM stores about itself.** Every Kickstart keeps a checksum
//!    24 bytes before its end, and that value is unique per build:
//!    `40.68 (A1200)` and `40.68 (A4000)` share a revision and differ here.
//!    [`remus::REMUS_ROMS`] maps it to a name and a machine list, generated
//!    from an independent database (`scripts/rom-table-check.py`) rather than
//!    hand-listed. **This is the only question that can name a machine.**
//! 2. **A catalogued SHA-256.** The older, hand-listed table. Kept because it
//!    answers for a few dumps the database does not carry, and because
//!    removing a working answer to make room for a better one is not a fix.
//!    Measured against the project's own 29 Kickstart dumps it matched none
//!    of them, which is what ART-104 was.
//! 3. **What the ROM says about its version.** From 2.0 onwards a ROM states
//!    its own revision, so an uncatalogued dump is still named — but a
//!    revision is shared across the per-machine builds, so this claims no
//!    machine, deliberately.

pub mod offer;
pub mod pairing;
pub mod place;
pub mod remus;

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::core::error::{CoreError, CoreResult};
use crate::core::hashing::sha256_bytes;

/// Cloanto ROM header prefix (11 bytes).
const CLOANTO_HEADER: &[u8] = b"AMIROMTYPE1";

/// Kickstart ROM info surfaced to the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RomInfo {
    pub name: String,
    pub version: String,
    pub revision: String,
    pub size_bytes: usize,
    pub sha256: String,
    pub crc32: String,
    pub is_cloanto: bool,
    /// Only meaningful when `is_cloanto`: whether the `rom.key` that decodes
    /// it was found beside it. False means every other field describes what
    /// ART could work out **without** reading the image — which is very
    /// little, and the name says so.
    #[serde(default)]
    pub key_available: bool,
    pub is_aros: bool,
    /// What ART can honestly say about this file's integrity — three answers,
    /// not two (ART-138).
    pub checksum: RomChecksum,
    pub compatible_models: Vec<String>,
    pub file_path: String,
    /// The Kickstart major this file identifies as, when one is known —
    /// `Some(37)` for a 2.04, `Some(40)` for a 3.1. Exists so a caller can
    /// apply a floor like WHDLoad's own stated minimum ("Kickstart 2.0
    /// (version 37)", <https://www.whdload.de/docs/en/need.html> — see
    /// `core::launch::WHDLOAD_MIN_KICKSTART_MAJOR`) without re-deriving a
    /// number out of `revision`'s text. **Not an identifier**: several
    /// models share a major, so `compatible_models` stays the authority on
    /// which machine a ROM suits — this only ever answers "how new".
    /// `None` when nothing here names a numeric major: an AROS replacement
    /// (`revision` is `"Built-in"`), an Amiga Forever dump with no `rom.key`
    /// beside it to decode, or a file that is not recognisably a Kickstart.
    #[serde(default)]
    pub major: Option<u16>,
    /// WHDLoad's own CRC-16/ARC over the **decoded** image, when ART could
    /// read one (ART-130).
    ///
    /// This is the number a WHDLoad slave declares to say which Kickstart it
    /// needs — not a filename, and not the CRC-32 above. It is computed here,
    /// where the decoded bytes already exist, so matching a title's request
    /// against a collection is arithmetic on data already in hand rather than
    /// a second pass that re-reads and re-decodes every ROM.
    ///
    /// **`None` is a real answer and not a zero.** A licensed Amiga Forever
    /// ROM without its `rom.key` beside it has no readable image, so there is
    /// nothing to checksum — and "you have this file but ART cannot read it"
    /// is a different sentence from "you do not have it".
    ///
    /// `#[serde(default)]`: a `RomInfo` serialised before this field existed
    /// must still deserialise.
    #[serde(default)]
    pub whdload_crc16: Option<u16>,
}

/// The verdict on a ROM file's stored checksum.
///
/// **A missing answer is not a failing one (ART-138).** The field this
/// replaced was a `bool`, so every file that is not a Kickstart — an
/// accelerator's boot ROM, a SCSI controller's, half of a split dump — came
/// back `false` and the screen said `CRC ERR` about it. That is a claim of
/// damage, and ART has no basis for it: the file is intact, it simply is not
/// a Kickstart and carries no Kickstart checksum to verify. The same rule
/// ART-104 applied to machine names, applied to faults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RomChecksum {
    /// The image is a Kickstart and its stored checksum verifies.
    Valid,
    /// The image is a Kickstart and its stored checksum does **not** verify —
    /// the one case where saying so is a claim about the file's integrity.
    Invalid,
    /// There was nothing to check: no Kickstart image here, or one ART cannot
    /// read (a licensed dump with no `rom.key` beside it).
    NotChecked,
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

/// The Kickstart major encoded in a `major.minor` revision string, the shape
/// every `KNOWN_ROMS` entry's `revision` and every ROM's own stated version
/// both write it (`format!("{major}.{:03}", minor)`) — see
/// [`RomInfo::major`]. `None` for anything not that shape: `""`, `"Built-in"`
/// (AROS) or a size like `"256 KB"` (the generic fallback), none of which
/// this function is ever actually called on today but all of which it must
/// answer safely rather than panic on.
fn major_from_revision(revision: &str) -> Option<u16> {
    revision.split('.').next()?.parse().ok()
}

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

    // A licensed Amiga Forever ROM is the same Kickstart behind a header and
    // a repeating XOR. With the buyer's own key beside it, ART decodes it and
    // everything below — the stored checksum, the hash, the version — reads
    // the real image. Without the key there is nothing to identify, and the
    // one honest answer is to say which file this is and stop.
    let is_cloanto = raw_bytes.starts_with(CLOANTO_HEADER);
    let key = is_cloanto.then(|| key_beside(path)).flatten();
    let key_available = key.is_some();
    let bytes = match (is_cloanto, &key) {
        (true, Some(key)) => decode_cloanto(&strip_cloanto_header(&raw_bytes), key),
        (true, None) => {
            return Ok(RomInfo {
                name: "Amiga Forever ROM (encrypted, needs rom.key)".to_string(),
                version: "Custom".to_string(),
                revision: String::new(),
                size_bytes: raw_bytes.len().saturating_sub(CLOANTO_HEADER.len()),
                sha256: sha256_bytes(&raw_bytes),
                crc32: format!("{:08X}", compute_crc32(&raw_bytes)),
                is_cloanto,
                key_available,
                is_aros: false,
                checksum: RomChecksum::NotChecked,
                compatible_models: Vec::new(),
                file_path: path.to_string_lossy().to_string(),
                major: None,
                // Nothing to checksum: without the key there is no image.
                whdload_crc16: None,
            });
        }
        (false, _) => raw_bytes,
    };

    let size_bytes = bytes.len();
    let sha256 = sha256_bytes(&bytes);
    let crc = compute_crc32(&bytes);
    let crc32 = format!("{:08X}", crc);

    // What can honestly be said about this file's integrity (ART-138): a
    // Kickstart's stored checksum verifies or it does not, and anything that
    // is not a Kickstart image is not accused of either.
    let checksum = checksum_verdict(&bytes);

    // 1. What the ROM stores about itself — the only answer that can name a
    //    machine, and the one that tells same-revision builds apart.
    if let Some(matched) = stored_checksum(&bytes).and_then(catalogued) {
        let (version, revision) = match matched.major {
            0 => ("Custom", String::new()),
            major => (
                version_name(major).unwrap_or("Custom"),
                format!("{major}.{:03}", matched.minor),
            ),
        };
        return Ok(RomInfo {
            name: matched.name.to_string(),
            version: version.to_string(),
            revision,
            size_bytes,
            sha256,
            crc32,
            is_cloanto,
            key_available,
            is_aros: false,
            checksum,
            compatible_models: matched.models.iter().map(|s| s.to_string()).collect(),
            file_path: path.to_string_lossy().to_string(),
            // `0` is remus's own "no numbered major" case (`Custom` above) —
            // never a real Kickstart major, so it becomes `None` here too.
            major: (matched.major != 0).then_some(matched.major),
            // WHDLoad's own checksum over the image a slave would load.
            whdload_crc16: Some(crate::core::hashing::crc16_arc(&bytes)),
        });
    }

    // 2. A catalogued SHA-256.
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
            key_available,
            is_aros: false,
            checksum,
            compatible_models: matched.models.iter().map(|s| s.to_string()).collect(),
            file_path: path.to_string_lossy().to_string(),
            major: major_from_revision(matched.revision),
            // WHDLoad's own checksum over the image a slave would load.
            whdload_crc16: Some(crate::core::hashing::crc16_arc(&bytes)),
        });
    }

    // 3. No catalogued dump matched. **Ask the ROM** (ART-104): from 2.0 onwards
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
            key_available,
            is_aros: false,
            checksum,
            // Empty on purpose: the ROM said its version, not its machine.
            compatible_models: Vec::new(),
            file_path: path.to_string_lossy().to_string(),
            major: Some(major),
            whdload_crc16: Some(crate::core::hashing::crc16_arc(&bytes)),
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
            key_available: false,
            is_aros: true,
            checksum,
            compatible_models: vec!["A500".into(), "A1200".into(), "A4000".into()],
            file_path: path.to_string_lossy().to_string(),
            // AROS states no Kickstart major at all — nothing here to floor
            // against, so a caller enforcing WHDLoad's minimum must not
            // guess one on AROS's behalf.
            major: None,
            whdload_crc16: Some(crate::core::hashing::crc16_arc(&bytes)),
        });
    }

    // Generic fallback for custom / diagnostic / uncatalogued ROMs.
    //
    // **A size names a shape, not a machine (ART-104).** This used to hand
    // back a machine list derived from the file's length — a 256 KB image was
    // "A500, A2000", which it told the user about the CDTV extended ROM in
    // the project's own collection, and anything unrecognised was given the
    // model `"Unknown"`, a machine no Amiga ever was. `rom_suits` never acted
    // on either (it declines to answer when `version` is `Custom`, which is
    // always the case here), so nothing was refused wrongly — but the screen
    // showed the claim, and a claim ART cannot support is one it should not
    // make (§89). The name still comes from the size, because that much *is*
    // what the size says.
    //
    // **And the size only says it of a Kickstart (ART-138).** A 256 KB
    // accelerator ROM was called *Generic Amiga 256KB ROM (Kickstart 1.x)* —
    // the same unfounded claim in the other direction, now that ART can tell
    // a Kickstart image from a file that merely sits in the same folder.
    let inferred_name = if is_kickstart_image(&bytes) {
        match size_bytes {
            262_144 => "Generic Amiga 256KB ROM (Kickstart 1.x)".to_string(),
            524_288 => "Generic Amiga 512KB ROM (Kickstart 2.x/3.x)".to_string(),
            1_048_576 => "Generic Amiga 1MB ROM (CD32 / Extended)".to_string(),
            2_097_152 => "Generic Amiga 2MB ROM (Diagnostic / Custom)".to_string(),
            _ => "Custom / Unknown ROM Image".to_string(),
        }
    } else {
        format!("Not a Kickstart image ({} KB)", size_bytes / 1024)
    };

    Ok(RomInfo {
        name: inferred_name,
        version: "Custom".to_string(),
        revision: format!("{} KB", size_bytes / 1024),
        size_bytes,
        sha256,
        crc32,
        is_cloanto,
        key_available,
        is_aros: false,
        checksum,
        compatible_models: Vec::new(),
        file_path: path.to_string_lossy().to_string(),
        // Nothing here identified a Kickstart major — `revision` is a size,
        // not a version.
        major: None,
        whdload_crc16: Some(crate::core::hashing::crc16_arc(&bytes)),
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

/// One entry out of a Kickstart's own resident module table.
///
/// `version` is `rt_Version`, the module's major alone — a Resident carries
/// no minor field at all. The minor a caller actually wants lives only in
/// `id`'s free-text second word (`"exec 47.10 (21.01.2023)"`), which is why
/// [`resident_version`] parses `id` rather than reporting `version` twice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RomResident {
    /// `rt_Name` — the module's own name, e.g. `"exec.library"`.
    pub name: String,
    /// `rt_Version`, the major alone.
    pub version: u8,
    /// `rt_IdString`, e.g. `"exec 47.10 (21.01.2023)"`.
    pub id: String,
}

/// `0x4AFC` is the m68k `ILLEGAL` instruction and turns up in ordinary code
/// too, so it cannot be trusted alone — see [`residents`].
const RESIDENT_MATCH_WORD: u16 = 0x4AFC;

/// `sizeof(struct Resident)`: two words (`rt_MatchWord`, `rt_MatchTag` is a
/// long) … the fields through `rt_Init` add up to 26 bytes on a 32-bit m68k
/// with no padding, which is what every real Kickstart lays out.
const RESIDENT_SIZE: usize = 26;

/// Where a Kickstart image of this size maps in the Amiga's address space.
///
/// A `Resident`'s own pointers (`rt_MatchTag`, `rt_Name`, `rt_IdString`) are
/// absolute 68000 addresses, not offsets into the dump file — so reading one
/// back requires knowing where the file's byte 0 sits in memory, and that
/// depends on the image's size. An unrecognised size has no answer: refusing
/// beats guessing a base that would silently misread every pointer in the
/// file.
fn rom_base(len: usize) -> Option<u32> {
    match len {
        0x8_0000 => Some(0xF8_0000), // 512 KiB
        0x4_0000 => Some(0xFC_0000), // 256 KiB
        _ => None,
    }
}

fn resident_malformed(detail: impl Into<String>) -> CoreError {
    CoreError::Malformed {
        format: "Kickstart resident table".into(),
        detail: detail.into(),
    }
}

/// Read a NUL-terminated string out of the image, given an absolute pointer
/// into it — the one place a `Resident`'s pointer becomes a file offset.
///
/// Every caller goes through this rather than indexing `bytes` with a
/// pointer-derived offset directly, because the pointer comes from a file
/// ART did not write: a pointer below the image's own base, or past its end,
/// is a [`CoreError`] naming the problem, never a clamp and never a silent
/// truncation. The release profile aborts on an out-of-range index, so this
/// is the difference between a refusal and taking the whole application down
/// over one untrusted ROM.
fn string_at(bytes: &[u8], base: u32, pointer: u32) -> CoreResult<String> {
    let offset = pointer
        .checked_sub(base)
        .ok_or_else(|| resident_malformed("a resident points below the ROM base"))?
        as usize;
    let tail = bytes
        .get(offset..)
        .ok_or_else(|| resident_malformed("a resident points past the end of the ROM"))?;
    let end = tail.iter().position(|b| *b == 0).unwrap_or(tail.len());
    Ok(String::from_utf8_lossy(&tail[..end]).into_owned())
}

/// Walk a Kickstart image for its `Resident` modules (ART's own reader —
/// AmigaOS itself does this at boot, in `InitResident`/`InitCode`).
///
/// ## Why the self-pointer, and not the match word alone
///
/// `0x4AFC` is the m68k `ILLEGAL` instruction, and a 512 KiB image is mostly
/// ordinary 68000 code — that word occurs there by coincidence, repeatedly.
/// What AmigaOS's own loader actually trusts is `rt_MatchTag`, which every
/// real `Resident` sets to point **at its own `rt_MatchWord`**. A scan that
/// stops at the match word alone finds rubbish; requiring `rt_MatchTag ==
/// base + offset` is what turns the scan into a real one.
///
/// ## Why this exists instead of trusting the ROM header (design §5)
///
/// AmigaOS 3.2.2's Modules step doesn't ask a ROM file anything — it asks
/// the **running machine** for `exec.library`'s revision and for `strap`'s
/// version, and decides from those two numbers alone. ART has no running
/// machine, only the paired ROM file, so it has to read the same two facts
/// out of the image instead. The header (`stated_version`) does not carry
/// them: measured against the owner's own three A1200 Kickstarts —
///
/// | Paired Kickstart | header  | `exec.library` | `strap` | release does |
/// |---|---|---|---|---|
/// | 3.2 `kicka1200.rom`        | 47.96  | 47.7  | **45.1** | modules on, larger set  |
/// | 3.2.1 `A1200.47.102.rom`   | 47.102 | 47.8  | 47.2     | modules on, smaller set |
/// | 3.2.2 `A1200.47.111.rom`   | 47.111 | 47.10 | 47.2     | modules off             |
///
/// the header collapses the first two rows into one outcome, and would place
/// `Shell-Seg` and three library modules onto a 47.102 machine that the
/// release deliberately withholds them from. `exec.library`'s and `strap`'s
/// own resident entries carry the two numbers the release actually reads —
/// so this reads those instead of the header.
pub fn residents(bytes: &[u8]) -> CoreResult<Vec<RomResident>> {
    let base = rom_base(bytes.len())
        .ok_or_else(|| resident_malformed("not a recognised Kickstart image size"))?;

    let mut found = Vec::new();
    let mut offset = 0usize;
    while offset + RESIDENT_SIZE <= bytes.len() {
        let word = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]);
        if word == RESIDENT_MATCH_WORD {
            let tag = u32::from_be_bytes([
                bytes[offset + 2],
                bytes[offset + 3],
                bytes[offset + 4],
                bytes[offset + 5],
            ]);
            // The only thing separating a real Resident from an ILLEGAL
            // opcode that happens to sit in ordinary code: a real one's
            // rt_MatchTag points at itself.
            let self_pointer = base.checked_add(offset as u32);
            if self_pointer == Some(tag) {
                let version = bytes[offset + 11];
                let name_ptr = u32::from_be_bytes([
                    bytes[offset + 14],
                    bytes[offset + 15],
                    bytes[offset + 16],
                    bytes[offset + 17],
                ]);
                let id_ptr = u32::from_be_bytes([
                    bytes[offset + 18],
                    bytes[offset + 19],
                    bytes[offset + 20],
                    bytes[offset + 21],
                ]);
                let name = string_at(bytes, base, name_ptr)?;
                let id = string_at(bytes, base, id_ptr)?;
                found.push(RomResident { name, version, id });
            }
        }
        offset += 2;
    }
    Ok(found)
}

/// The `(major, minor)` revision a named resident's own ID string states —
/// `exec.library`'s `"exec 47.10 (21.01.2023)"` reads `(47, 10)`.
///
/// Deliberately not `rt_Version`: that field carries the major alone, and
/// the minor the AmigaOS 3.2.2 Modules step actually compares lives only in
/// this free-text string's second word. `name` is matched against the ID
/// string's **first** word (`"exec"`, `"strap"`) — the same short name the
/// release's own installer script asks about — not against `rt_Name`, which
/// carries the longer library name (`"exec.library"`).
///
/// `None` covers three different things ART cannot tell apart from the
/// caller's side, all of which mean the same thing to a [`Condition`] built
/// on this: no resident of that name, an ID string that does not parse, or
/// an image this reader refuses outright. A malformed string yields `None`
/// rather than a partial number — a caller comparing versions must never see
/// a `major` with no `minor` behind it dressed up as `(major, 0)`.
///
/// [`Condition`]: crate::core::osinstall::Condition
pub fn resident_version(bytes: &[u8], name: &str) -> Option<(u16, u16)> {
    let table = residents(bytes).ok()?;
    resident_revision(&table, name)
}

/// The same lookup as [`resident_version`], over a table already read —
/// shared with `core::osinstall::plan::resident_older_than` so a
/// [`Condition`]'s own comparison and the version this module hands the UI
/// parse the exact same ID-string grammar rather than two copies drifting
/// apart.
///
/// [`Condition`]: crate::core::osinstall::Condition
pub(crate) fn resident_revision(residents: &[RomResident], name: &str) -> Option<(u16, u16)> {
    residents.iter().find_map(|resident| {
        let mut words = resident.id.split_whitespace();
        if words.next()? != name {
            return None;
        }
        let mut parts = words.next()?.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        Some((major, minor))
    })
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

/// Undo Amiga Forever's encoding: the payload XORed with the key, repeating.
///
/// **Read from an implementation, not remembered** — `amitools`' own
/// `rom.Loader` does exactly this, and it is what every emulator that accepts
/// these files does. The key is the buyer's; ART holds none and ships none.
pub fn decode_cloanto(payload: &[u8], key: &[u8]) -> Vec<u8> {
    if key.is_empty() {
        return payload.to_vec();
    }
    payload
        .iter()
        .enumerate()
        .map(|(at, byte)| byte ^ key[at % key.len()])
        .collect()
}

/// The `rom.key` that decodes a ROM, looked for **beside the ROM itself**.
///
/// That is where Amiga Forever puts it and where `amitools` looks, so a user
/// who exports their ROMs gets a working answer with nothing to configure.
/// A key that is not there is not an error here: the caller says so instead of
/// guessing at the bytes.
fn key_beside(rom: &Path) -> Option<Vec<u8>> {
    let key = rom.parent()?.join("rom.key");
    let bytes = std::fs::read(key).ok()?;
    (!bytes.is_empty()).then_some(bytes)
}

/// A ROM's bytes as an Amiga would read them: the header gone and the image
/// decoded when it is a licensed Amiga Forever dump with its key beside it
/// (ART-128), and the file as-is otherwise.
///
/// The one place that answers "what is actually in this ROM", so nothing has
/// to repeat the header-and-key dance to read a version out of one.
pub fn decoded_image(path: &Path) -> CoreResult<Vec<u8>> {
    let raw = std::fs::read(path)?;
    if !raw.starts_with(CLOANTO_HEADER) {
        return Ok(raw);
    }
    match key_beside(path) {
        Some(key) => Ok(decode_cloanto(&strip_cloanto_header(&raw), &key)),
        None => Err(CoreError::InvalidInput(format!(
            "'{}' is an encrypted Amiga Forever ROM and its 'rom.key' is not beside it",
            path.display()
        ))),
    }
}

pub fn strip_cloanto_header(bytes: &[u8]) -> Vec<u8> {
    if bytes.starts_with(CLOANTO_HEADER) && bytes.len() > 11 {
        bytes[11..].to_vec()
    } else {
        bytes.to_vec()
    }
}

/// Whether this image is shaped like a Kickstart at all — the question that
/// has to be answered **before** its checksum means anything (ART-138).
///
/// Two structural marks, both put there by Commodore's build and neither
/// affected by damage to the code between them:
///
/// - it opens with `$11`, then a `JMP` (`$4EF9`) into the ROM. The second byte
///   varies with the image's size (`$11`, `$14`, `$16` all occur), so it is
///   not part of the test;
/// - it ends with the eight bytes `00 1C 00 1D 00 1E 00 1F`, the tail of the
///   table that follows the checksum a Kickstart stores 24 bytes before its
///   end.
///
/// **Measured, not assumed.** Over the 76 files in this project's own ROM
/// folder plus the AmigaOS 3.2 / 3.2.1 / 3.2.2 releases and an Amiga Forever
/// export — some 150 files in total, Kickstart 0.7 through 47.111 — these two
/// marks and a verifying checksum agree exactly: every image carrying both
/// summed correctly, and no accelerator, SCSI or split half-image carried
/// either. The two files that opened like a ROM but carried no tail (a CDTV
/// extended v1.0 dump and the A1000 bootstrap) are the honest "nothing to
/// check" case: they keep no checksum where a Kickstart keeps one.
pub fn is_kickstart_image(bytes: &[u8]) -> bool {
    const TAIL: &[u8] = &[0x00, 0x1C, 0x00, 0x1D, 0x00, 0x1E, 0x00, 0x1F];

    bytes.len() >= 32
        && bytes.len().is_multiple_of(4)
        && bytes[0] == 0x11
        && bytes[2..4] == [0x4E, 0xF9]
        && bytes.ends_with(TAIL)
}

/// The checksum verdict for an image, refusing to answer where there is no
/// question (ART-138).
pub fn checksum_verdict(bytes: &[u8]) -> RomChecksum {
    if !is_kickstart_image(bytes) {
        return RomChecksum::NotChecked;
    }
    if verify_kickstart_checksum(bytes) {
        RomChecksum::Valid
    } else {
        RomChecksum::Invalid
    }
}

/// Verify standard Kickstart 32-bit checksum (sum of all 32-bit big-endian words with carry).
pub fn verify_kickstart_checksum(bytes: &[u8]) -> bool {
    if bytes.len() < 4 || !bytes.len().is_multiple_of(4) {
        return false;
    }

    let mut sum = 0u32;
    for chunk in bytes.as_chunks::<4>().0 {
        let val = u32::from_be_bytes(*chunk);
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

/// The dump a stored checksum names, if the generated table carries it.
///
/// A linear scan over 154 entries, once per identification — nanoseconds, and
/// not worth a map built at startup. It lives here rather than in `remus.rs`
/// because that file is generated and holds data only: a function in it would
/// be lost the next time `scripts/rom-table-check.py --emit` runs, and the
/// verifier compares the file whole.
fn catalogued(stored: u32) -> Option<&'static remus::RemusRom> {
    remus::REMUS_ROMS
        .iter()
        .find(|rom| rom.stored_checksum == stored)
}

/// The checksum a Kickstart keeps 24 bytes before its end.
///
/// **Read, never computed.** ART already computes a Kickstart checksum to
/// answer whether the image is intact (`verify_kickstart_checksum`); this is
/// the *stored* longword that computation is checked against, and it is what
/// the Remus database keys on. `None` for anything too short to hold one —
/// the A1000 bootstrap in the project's own collection is 8 KB and has no such
/// field.
fn stored_checksum(bytes: &[u8]) -> Option<u32> {
    let at = bytes.len().checked_sub(24)?;
    let slice = bytes.get(at..at + 4)?;
    Some(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [`major_from_revision`] feeds [`RomInfo::major`] for the
    /// `KNOWN_ROMS`-by-hash case — every real revision string parses, and
    /// the shapes that are not a `major.minor` string (an AROS entry's
    /// `"Built-in"`, a Cloanto-without-key's `""`, a generic fallback's
    /// `"256 KB"`) come back `None` rather than panicking or misreading a
    /// leading digit run as a major.
    #[test]
    fn major_from_revision_reads_the_leading_number_and_nothing_else() {
        assert_eq!(major_from_revision("40.068"), Some(40));
        assert_eq!(major_from_revision("34.005"), Some(34));
        assert_eq!(major_from_revision("47.111"), Some(47));
        assert_eq!(major_from_revision(""), None);
        assert_eq!(major_from_revision("Built-in"), None);
        assert_eq!(major_from_revision("256 KB"), None);
    }

    #[test]
    fn strip_cloanto_rom_header() {
        let mut raw = CLOANTO_HEADER.to_vec();
        raw.extend_from_slice(&[0x11, 0x22, 0x33, 0x44]);
        let stripped = strip_cloanto_header(&raw);
        assert_eq!(stripped, vec![0x11, 0x22, 0x33, 0x44]);
    }

    /// An image shaped like a Kickstart: Commodore's opening `$11xx 4EF9` and
    /// the tail its build leaves at the end. Everything between is zero, which
    /// is what makes these fixtures legal to ship (ART owns no ROM).
    fn kickstart_shaped(size: usize) -> Vec<u8> {
        let mut bytes = vec![0u8; size];
        bytes[0..4].copy_from_slice(&[0x11, 0x14, 0x4E, 0xF9]);
        let at = bytes.len() - 8;
        bytes[at..].copy_from_slice(&[0x00, 0x1C, 0x00, 0x1D, 0x00, 0x1E, 0x00, 0x1F]);
        bytes
    }

    /// The same, with the checksum a Kickstart stores 24 bytes before its end
    /// set to the value that makes the image sum correctly.
    fn kickstart_that_sums(size: usize) -> Vec<u8> {
        let mut bytes = kickstart_shaped(size);
        let at = bytes.len() - 24;
        bytes[at..at + 4].copy_from_slice(&[0, 0, 0, 0]);
        let mut sum = 0u32;
        for chunk in bytes.as_chunks::<4>().0 {
            let val = u32::from_be_bytes(*chunk);
            let (next, carry) = sum.overflowing_add(val);
            sum = next.wrapping_add(carry as u32);
        }
        bytes[at..at + 4].copy_from_slice(&(0xFFFF_FFFFu32 - sum).to_be_bytes());
        bytes
    }

    /// A synthetic ROM that states `major.minor` where a real one does.
    fn rom_stating(major: u16, minor: u16, size: usize) -> Vec<u8> {
        let mut bytes = kickstart_shaped(size);
        bytes[12..14].copy_from_slice(&major.to_be_bytes());
        bytes[14..16].copy_from_slice(&minor.to_be_bytes());
        bytes
    }

    /// A 512 KB image carrying `stored` where a Kickstart keeps its own
    /// checksum — 24 bytes before the end — and nothing else that identifies
    /// it. ART ships no ROM, so a catalogued dump is stood in for by the one
    /// field the identification actually reads.
    fn rom_with_stored_checksum(stored: u32) -> Vec<u8> {
        let mut bytes = kickstart_shaped(524_288);
        let at = bytes.len() - 24;
        bytes[at..at + 4].copy_from_slice(&stored.to_be_bytes());
        bytes
    }

    /// The same bytes as Amiga Forever ships them: the header, then the image
    /// XORed with the key, repeating. Matches `amitools`' own loader, which is
    /// where this shape was read rather than remembered.
    fn encrypted(plain: &[u8], key: &[u8]) -> Vec<u8> {
        let mut out = CLOANTO_HEADER.to_vec();
        out.extend(
            plain
                .iter()
                .enumerate()
                .map(|(at, byte)| byte ^ key[at % key.len()]),
        );
        out
    }

    fn write(dir: &std::path::Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("art-rom-{tag}-{}", crate::core::test_scratch_id()));
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
        assert_eq!(
            info.major,
            Some(40),
            "the ROM's own stated major (ART-148's floor reads exactly this)"
        );

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

    /// **ART-104, the half `stated_version` could not reach.** A revision
    /// does not name a machine — 40.68 is stated by the A1200 build and the
    /// A4000 build alike — so a dump identified only by what it says about
    /// itself leaves `compatible_models` empty, and `rom_suits` has nothing to
    /// compare against. Measured before this fix: **none** of the 29 Kickstart
    /// dumps in the project's own collection matched the ten hand-listed
    /// hashes, so that check had never once fired for real material.
    ///
    /// The dump is now identified by the checksum it stores about itself
    /// (`size - 24`), against a table derived from the Remus split database
    /// (`scripts/rom-table-check.py`). Two same-revision builds store
    /// different values, which is exactly the distinction that was missing.
    #[test]
    fn a_catalogued_dump_is_named_and_placed_by_the_checksum_it_stores() {
        let dir = scratch("stored-checksum");

        // The two real 40.68 builds, by the values the database holds for
        // them. Nothing else about these fixtures differs — same size, same
        // stated revision — so the machine can only have come from the
        // checksum.
        let a1200 = write(&dir, "a.rom", &rom_with_stored_checksum(0x87BA_7A3E));
        let a4000 = write(&dir, "b.rom", &rom_with_stored_checksum(0x45C3_145E));

        let one = identify_rom(&a1200).unwrap();
        let other = identify_rom(&a4000).unwrap();

        assert_eq!(one.name, "Kickstart 40.68 (A1200)");
        assert_eq!(one.compatible_models, vec!["A1200".to_string()]);
        assert_eq!(one.version, "3.1", "derived from the major, as ever");
        assert_eq!(one.revision, "40.068");
        assert_eq!(one.major, Some(40));

        assert_eq!(other.name, "Kickstart 40.68 (A4000)");
        assert_eq!(other.compatible_models, vec!["A4000".to_string()]);
        assert_eq!(other.major, Some(40));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The table names machines only where the database named machines. An
    /// entry like `Kickstart 40.68 (AmigaForever)` describes a distribution,
    /// not a model, and claims none — empty means "ART says nothing", never
    /// "suits nothing" (`rom_suits` reads it as the former).
    #[test]
    fn an_entry_that_names_no_machine_claims_none() {
        let dir = scratch("no-machine-claim");
        let path = write(&dir, "af.rom", &rom_with_stored_checksum(0x44C3_115E));

        let info = identify_rom(&path).unwrap();

        assert_eq!(info.name, "Kickstart 40.68 (AmigaForever)");
        assert!(
            info.compatible_models.is_empty(),
            "{:?}",
            info.compatible_models
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

    /// **The measurement ART-104 was filed over, as a hook rather than a
    /// claim.** Points at a folder of real Kickstart dumps — the user's own,
    /// which ART does not ship and never will — and prints what
    /// `identify_rom` now says about each. Before the Remus table it named 0
    /// of 29 and could claim a machine for none of them.
    ///
    /// ```text
    /// cd src-tauri
    /// ART_ROM_DIR="E:\amiga\Amigatolon\kickstart" \
    ///   cargo test identify_the_real_rom_collection_when_asked -- --nocapture
    /// ```
    /// **ART-104's mirror.** The fallback names a ROM by its size, and used
    /// to name machines by it too — telling the user a 256 KB CDTV extended
    /// ROM suited an A500 and an A2000. The size is kept; the machines are
    /// not, because nothing measured them.
    /// **A licensed Amiga Forever ROM is a first-class input, not a mystery
    /// blob.** Its bytes are the same Kickstart, kept behind an
    /// `AMIROMTYPE1` header and a repeating XOR against the buyer's own
    /// `rom.key`. ART reads the key from beside the ROM — the layout Amiga
    /// Forever ships and the one `amitools`' own loader looks in — and then
    /// identifies the image exactly as it would a bare dump.
    ///
    /// Built from a *synthetic* ROM and a synthetic key, because ART ships no
    /// ROM and never will: the fixture carries the stored checksum the table
    /// holds for `Kickstart 40.68 (A1200)`, so recovering that name proves the
    /// decryption produced the original bytes and not merely different ones.
    #[test]
    fn a_cloanto_rom_is_decoded_with_the_key_beside_it_and_then_identified() {
        let dir = scratch("cloanto-keyed");
        let plain = rom_with_stored_checksum(0x87BA_7A3E);
        let key = b"a rom key of no particular length".to_vec();
        std::fs::write(dir.join("rom.key"), &key).unwrap();
        let path = write(&dir, "amiga-os-310-a1200.rom", &encrypted(&plain, &key));

        let info = identify_rom(&path).unwrap();

        assert!(info.is_cloanto, "the header says what it is");
        assert_eq!(info.name, "Kickstart 40.68 (A1200)");
        assert_eq!(info.compatible_models, vec!["A1200".to_string()]);
        assert_eq!(
            info.size_bytes,
            plain.len(),
            "the size reported is the ROM's, not the file's — the header is not \
             part of the image"
        );
        assert_eq!(
            info.sha256,
            crate::core::hashing::sha256_bytes(&plain),
            "and the hash is of the decoded image, so it can be compared with \
             a bare dump of the same ROM"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Without the key, ART says what it has rather than describing the
    /// ciphertext. It used to call this *Generic Amiga 512KB ROM* — the same
    /// answer it gives an unknown dump — which reads as "probably fine, just
    /// uncatalogued" when the truth is "ART cannot read this at all, and it
    /// will not boot anything as it stands".
    #[test]
    fn a_cloanto_rom_with_no_key_is_named_as_one_rather_than_guessed_at() {
        let dir = scratch("cloanto-keyless");
        let key = b"whatever".to_vec();
        let path = write(
            &dir,
            "amiga-os-310-a1200.rom",
            &encrypted(&rom_with_stored_checksum(0x87BA_7A3E), &key),
        );

        let info = identify_rom(&path).unwrap();

        assert!(info.is_cloanto);
        assert!(!info.key_available, "no rom.key sits beside it");
        assert_eq!(info.name, "Amiga Forever ROM (encrypted, needs rom.key)");
        assert!(
            info.compatible_models.is_empty(),
            "nothing can be claimed about bytes ART cannot read: {:?}",
            info.compatible_models
        );
        assert_eq!(
            info.version, "Custom",
            "and it is not passed off as a version ART worked out"
        );
        // **ART-130.** `None`, never `Some(0)`. A zero here is a number, and
        // `core::rom::offer` matches on this field — so a "checksum" ART never
        // computed would silently answer a title whose slave happens to
        // declare 0, and the user would be offered a ROM nobody had read.
        assert_eq!(
            info.whdload_crc16, None,
            "there is no image to checksum without the key"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_size_names_the_shape_and_not_the_machine() {
        let dir = scratch("size-only");
        // 256 KB, shaped like a Kickstart, stating no version and matching
        // nothing catalogued.
        let path = write(&dir, "odd.rom", &kickstart_shaped(262_144));

        let info = identify_rom(&path).unwrap();

        assert_eq!(info.name, "Generic Amiga 256KB ROM (Kickstart 1.x)");
        assert!(
            info.compatible_models.is_empty(),
            "a length is not evidence of a machine: {:?}",
            info.compatible_models
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **ART-138.** An accelerator's boot ROM is not a Kickstart, and ART has
    /// no basis whatsoever for a claim about its integrity. The file below is
    /// shaped like the ones that provoked this — `A2630_390282-06.bin` and its
    /// 39 companions in the project's own ROM folder, every one of which the
    /// screen labelled `CRC ERR`.
    #[test]
    fn a_rom_that_is_not_a_kickstart_is_not_accused_of_a_bad_checksum() {
        let dir = scratch("not-a-kickstart");
        let mut bytes = vec![0u8; 32_768];
        bytes[0..4].copy_from_slice(&[0x13, 0xF9, 0xF8, 0x60]);
        let path = write(&dir, "A2630_390282-06.bin", &bytes);

        let info = identify_rom(&path).unwrap();

        assert_eq!(
            info.checksum,
            RomChecksum::NotChecked,
            "the file is intact; there is simply no Kickstart checksum in it"
        );
        assert_eq!(
            info.name, "Not a Kickstart image (32 KB)",
            "and it is not passed off as a generic Kickstart either"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The other half of the same rule: where a Kickstart **does** keep a
    /// checksum, a body that no longer sums to it is a real finding and ART
    /// still says so.
    #[test]
    fn a_kickstart_whose_body_changed_still_reports_a_bad_checksum() {
        let dir = scratch("damaged-kickstart");
        let sound = kickstart_that_sums(262_144);
        assert_eq!(checksum_verdict(&sound), RomChecksum::Valid);

        let mut damaged = sound.clone();
        damaged[4096] ^= 0x01;
        let path = write(&dir, "damaged.rom", &damaged);

        let info = identify_rom(&path).unwrap();

        assert_eq!(
            info.checksum,
            RomChecksum::Invalid,
            "one flipped bit in the body, and both structural marks intact — exactly the case the label is for"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The two marks are structural, so damage to the code between them does
    /// not make ART forget what kind of file it is looking at.
    #[test]
    fn a_kickstart_is_recognised_by_its_opening_and_its_tail() {
        let rom = kickstart_shaped(262_144);
        assert!(is_kickstart_image(&rom));

        let mut no_header = rom.clone();
        no_header[0] = 0x00;
        assert!(!is_kickstart_image(&no_header));

        let mut no_tail = rom.clone();
        let at = no_tail.len() - 1;
        no_tail[at] = 0x00;
        assert!(!is_kickstart_image(&no_tail));

        assert!(
            !is_kickstart_image(&[0x11, 0x14, 0x4E, 0xF9]),
            "and something far too short to hold either is not one"
        );
    }

    /// A licensed dump with no `rom.key` beside it is a Kickstart ART cannot
    /// read at all — which is a reason to say nothing about its checksum, not
    /// a reason to fail it (ART-138 meeting ART-128).
    #[test]
    fn an_encrypted_rom_with_no_key_says_nothing_about_its_checksum() {
        let dir = scratch("cloanto-checksum");
        let path = write(
            &dir,
            "amiga-os-310-a1200.rom",
            &encrypted(&kickstart_that_sums(524_288), b"whatever"),
        );

        let info = identify_rom(&path).unwrap();

        assert!(!info.key_available);
        assert_eq!(info.checksum, RomChecksum::NotChecked);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn identify_the_real_rom_collection_when_asked() {
        let Ok(dir) = std::env::var("ART_ROM_DIR") else {
            return;
        };
        let mut named = 0;
        let mut placed = 0;
        let mut total = 0;
        let (mut sums, mut accused, mut unchecked) = (0, 0, 0);
        let mut entries: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        entries.sort();
        for path in entries {
            let Ok(info) = identify_rom(&path) else {
                continue;
            };
            total += 1;
            if info.version != "Custom" && !info.revision.is_empty() {
                named += 1;
            }
            if !info.compatible_models.is_empty() {
                placed += 1;
            }
            match info.checksum {
                RomChecksum::Valid => sums += 1,
                RomChecksum::Invalid => accused += 1,
                RomChecksum::NotChecked => unchecked += 1,
            }
            println!(
                "  {:<58} -> {} [{}] {:?}",
                path.file_name().unwrap().to_string_lossy(),
                info.name,
                if info.compatible_models.is_empty() {
                    "no machine claimed".to_string()
                } else {
                    info.compatible_models.join(", ")
                },
                info.checksum
            );
        }
        println!(
            "named={named} placed={placed} total={total} checksum: valid={sums} invalid={accused} not-checked={unchecked}"
        );
        assert!(total > 0, "'{dir}' held no ROM ART could read at all");
    }

    #[test]
    fn crc32_empty_and_known_string() {
        assert_eq!(compute_crc32(b""), 0);
        // Standard test vector: "123456789" -> 0xCBF43926
        assert_eq!(compute_crc32(b"123456789"), 0xCBF4_3926);
    }

    /// A 512 KiB image with one hand-built `Resident` at a known offset.
    fn rom_with_resident(offset: usize, name: &str, version: u8, id: &str) -> Vec<u8> {
        const BASE: u32 = 0xF8_0000;
        let mut rom = vec![0u8; 512 * 1024];
        let name_at = offset + 64;
        let id_at = offset + 128;
        rom[offset..offset + 2].copy_from_slice(&0x4AFCu16.to_be_bytes());
        rom[offset + 2..offset + 6].copy_from_slice(&(BASE + offset as u32).to_be_bytes());
        rom[offset + 11] = version;
        rom[offset + 14..offset + 18].copy_from_slice(&(BASE + name_at as u32).to_be_bytes());
        rom[offset + 18..offset + 22].copy_from_slice(&(BASE + id_at as u32).to_be_bytes());
        rom[name_at..name_at + name.len()].copy_from_slice(name.as_bytes());
        rom[id_at..id_at + id.len()].copy_from_slice(id.as_bytes());
        rom
    }

    #[test]
    fn a_resident_is_found_by_its_own_self_pointer() {
        let rom = rom_with_resident(0x400, "exec.library", 47, "exec 47.10 (21.01.2023)");
        let found = residents(&rom).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "exec.library");
        assert_eq!(found[0].version, 47);
        assert_eq!(found[0].id, "exec 47.10 (21.01.2023)");
    }

    #[test]
    fn a_match_word_whose_tag_points_elsewhere_is_not_a_resident() {
        // 0x4AFC is the m68k ILLEGAL instruction and occurs in ordinary code.
        // Only the self-pointer separates a real Resident from a coincidence.
        let mut rom = rom_with_resident(0x400, "exec.library", 47, "exec 47.10 (x)");
        rom[0x800..0x802].copy_from_slice(&0x4AFCu16.to_be_bytes());
        rom[0x802..0x806].copy_from_slice(&0xF8_0000u32.to_be_bytes()); // points at 0, not itself
        assert_eq!(residents(&rom).unwrap().len(), 1);
    }

    #[test]
    fn resident_version_reads_the_revision_out_of_the_id_string() {
        let rom = rom_with_resident(0x400, "exec.library", 47, "exec 47.10 (21.01.2023)");
        assert_eq!(resident_version(&rom, "exec"), Some((47, 10)));
        assert_eq!(resident_version(&rom, "strap"), None);
    }

    #[test]
    fn a_name_pointer_outside_the_image_is_refused_not_read() {
        let mut rom = rom_with_resident(0x400, "exec.library", 47, "exec 47.10 (x)");
        rom[0x400 + 14..0x400 + 18].copy_from_slice(&0xFFFF_FFFEu32.to_be_bytes());
        assert!(
            residents(&rom).is_err(),
            "a pointer outside the image is a refusal"
        );
    }

    #[test]
    fn an_image_of_an_unexpected_size_has_no_base_and_is_refused() {
        assert!(residents(&vec![0u8; 1234]).is_err());
    }

    /// **Not run by default.** `cargo test -- --ignored` or a direct name
    /// runs it, and only `ART_ROM` set makes it do anything — this is the
    /// hook the design's §5 table (reproduced on [`residents`]'s own doc
    /// comment) came from, and running it again is how that table is kept
    /// honest rather than trusted. Read-only: it never copies, moves or
    /// modifies the file it names.
    #[test]
    #[ignore = "needs the owner's own 3.2-family Kickstarts"]
    fn read_the_real_roms_residents_when_asked() {
        let Ok(path) = std::env::var("ART_ROM") else {
            return;
        };
        let bytes = std::fs::read(path).unwrap();
        println!(
            "ART_ROM_RESULT header={:?} exec={:?} strap={:?}",
            stated_version(&bytes),
            resident_version(&bytes, "exec"),
            resident_version(&bytes, "strap"),
        );
        assert!(resident_version(&bytes, "exec").is_some());
    }
}
