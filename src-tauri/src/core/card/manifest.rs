//! What a card was built from, and whether it still is that (SD-1 · G7).
//!
//! The gap analysis asks for a manifest that describes a build well enough to
//! rebuild it, and for a check of a card against one. With G1 gone there is no
//! device to read back from — the artifact is a file ART just wrote, and the
//! manifest is the record of what went into it.
//!
//! ## It lives beside the image, not inside it
//!
//! `card.img` → `card.img.manifest.json`. Putting it on the FAT32 partition
//! would make it part of the thing it describes: a manifest carrying the boot
//! partition's own checksums, inside the boot partition, cannot be right about
//! itself. Beside the image it can describe every byte ART placed.
//!
//! ## The facts are read off the card, not remembered from the build
//!
//! [`describe_card`] opens the finished image and reads its partition table,
//! its areas and its RDBs back. Only the things an image cannot know — which
//! archive and which ROM the user supplied — come from the caller. A manifest
//! that recorded what ART *intended* would agree with a card ART wrote wrongly.
//!
//! ## What it does not check, and says so
//!
//! `core/fat32` writes a FAT32 and does not read one, so ART cannot re-read
//! the boot partition's files to hash them. Their hashes are recorded — from
//! the bytes ART placed — and [`verify_against_image`] reports them as
//! **unchecked** rather than passing over them in silence. `scripts/fat-oracle-check.py`
//! checks that half with 7-Zip, which is the project's rule anyway: a card is
//! verified by something that is not ART.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::core::card::read_card;
use crate::core::error::{CoreError, CoreResult};
use crate::core::hashing::sha256_bytes;
use crate::core::pistorm::hardware::{Emu68Line, PistormHardware};

/// The schema this ART writes and understands.
///
/// A manifest carrying a higher number is refused rather than half-read: the
/// fields ART does not know about are exactly the ones a newer build used to
/// describe something this one cannot check.
pub const MANIFEST_SCHEMA: u32 = 1;

/// How much of each Amiga area's start is hashed as "the RDB".
///
/// A fixed window rather than the RDB's own declared length, so describing and
/// verifying cannot disagree about where to stop. 256 KiB comfortably covers
/// an RDSK block, a hundred partition blocks and an embedded filesystem driver
/// — `pfs3aio` is about 59 KB — and everything past the written blocks is the
/// zeroes `set_len` left, which is part of what a build produced.
pub const RDB_WINDOW_BYTES: u64 = 256 * 1024;

/// One file ART placed on the boot partition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestFile {
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
}

/// Where the card came from. **Supplied by the caller** — an image cannot say
/// which archive on the user's disk it was unpacked from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFacts {
    /// The file name only, never the path: a manifest is shareable, and where
    /// somebody keeps their downloads is not part of what the card is.
    pub archive_name: String,
    pub archive_sha256: String,
    pub kickstart_name: Option<String>,
    pub kickstart_sha256: Option<String>,
    /// The name the Kickstart is written under **on the card** — what
    /// `config.txt`'s `initramfs` line points at. `kickstart_name` above is
    /// the source file's own name and `kickstart_sha256` the source file's
    /// hash; since ART-128 neither describes the bytes on the card, because a
    /// licensed Amiga Forever ROM is decoded on the way there. This field is
    /// how a reader finds the ROM among `boot_files`, whose hashes *are* of
    /// the bytes placed.
    ///
    /// `None` on a manifest written before this existed.
    #[serde(default)]
    pub kickstart_file: Option<String>,
    /// The major version that Kickstart states about itself, read once at
    /// build time.
    ///
    /// **Recorded because it cannot be recovered.** ART writes FAT32 and has
    /// no reader for it, so once the card exists the ROM's own bytes are out
    /// of reach — the same reason G8 answers the boot files from the manifest
    /// rather than by looking. `None` when the ROM states none (pre-2.0) or
    /// when the manifest predates this field.
    #[serde(default)]
    pub kickstart_stated_major: Option<u16>,
    /// The real types, not a rendered sentence: a manifest is data, and a
    /// sentence would have to be parsed back to rebuild anything from it.
    pub hardware: PistormHardware,
    pub line: Emu68Line,
    /// The file `config.txt` points the firmware at (ART-103).
    pub kernel_file: String,
}

/// One primary partition, as the table records it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotFacts {
    pub index: usize,
    pub type_byte: u8,
    pub start_lba: u64,
    pub sector_count: u64,
}

/// One partition inside an Amiga area's RDB.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartitionFacts {
    pub drive_name: String,
    pub dostype: String,
    pub size_bytes: u64,
}

/// One Amiga disk on the card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AreaFacts {
    pub offset_bytes: u64,
    pub length_bytes: u64,
    /// SHA-256 of [`RDB_WINDOW_BYTES`] at `offset_bytes`.
    pub rdb_sha256: String,
    pub partitions: Vec<PartitionFacts>,
}

/// Everything ART records about a card it built.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardManifest {
    pub schema: u32,
    /// Which ART wrote it.
    pub art_version: String,
    /// Supplied by the caller: `core` has no clock.
    pub built_at: Option<String>,
    pub total_bytes: u64,
    /// SHA-256 of the 512-byte partition table.
    pub mbr_sha256: String,
    pub slots: Vec<SlotFacts>,
    pub source: SourceFacts,
    pub boot_files: Vec<ManifestFile>,
    pub areas: Vec<AreaFacts>,
    /// What was installed onto the Amiga volumes. Empty until SD-2 puts a
    /// system on one; the field exists so a manifest written today is the same
    /// shape as one written then.
    #[serde(default)]
    pub os: Vec<String>,
}

/// Something on the card that does not match its manifest.
///
/// Typed rather than a sentence: `CoreError`'s English reaches the UI
/// untranslated (ART-060), and this is a screen's text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ManifestFinding {
    /// Written by a newer ART. Refused rather than half-understood.
    SchemaTooNew {
        found: u32,
        understood: u32,
    },
    SizeChanged {
        expected: u64,
        found: u64,
    },
    PartitionTableChanged,
    AreaCountChanged {
        expected: usize,
        found: usize,
    },
    AreaMoved {
        area: usize,
        expected: u64,
        found: u64,
    },
    AreaResized {
        area: usize,
        expected: u64,
        found: u64,
    },
    RdbChanged {
        area: usize,
    },
    PartitionCountChanged {
        area: usize,
        expected: usize,
        found: usize,
    },
    PartitionChanged {
        area: usize,
        name: String,
    },
}

/// Something this check deliberately did not look at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum NotChecked {
    /// ART writes FAT32 and cannot read one, so the files on the boot
    /// partition were not re-read. Their hashes are in the manifest and
    /// `scripts/fat-oracle-check.py` checks them with 7-Zip.
    BootPartitionFiles { count: usize },
}

/// What a check found, and what it did not look at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestReport {
    pub findings: Vec<ManifestFinding>,
    pub not_checked: Vec<NotChecked>,
}

impl ManifestReport {
    /// Whether everything that *could* be checked agreed.
    pub fn matches(&self) -> bool {
        self.findings.is_empty()
    }
}

/// The facts read off a finished card.
struct CardFacts {
    total_bytes: u64,
    mbr_sha256: String,
    slots: Vec<SlotFacts>,
    areas: Vec<AreaFacts>,
}

fn read_facts(image: &Path) -> CoreResult<CardFacts> {
    let card = read_card(image)?;

    let mut file = std::fs::File::open(image)?;
    let mut sector = [0u8; 512];
    file.read_exact(&mut sector)?;

    let slots = card
        .mbr
        .as_ref()
        .map(|mbr| {
            mbr.partitions
                .iter()
                .map(|part| SlotFacts {
                    index: part.index,
                    type_byte: part.type_byte,
                    start_lba: part.start_lba,
                    sector_count: part.sector_count,
                })
                .collect()
        })
        .unwrap_or_default();

    let mut areas = Vec::with_capacity(card.areas.len());
    for area in &card.areas {
        let window = (RDB_WINDOW_BYTES).min(card.total_bytes.saturating_sub(area.offset_bytes));
        let mut buf = vec![0u8; window as usize];
        file.seek(SeekFrom::Start(area.offset_bytes))?;
        file.read_exact(&mut buf)?;

        areas.push(AreaFacts {
            offset_bytes: area.offset_bytes,
            length_bytes: area.length_bytes,
            rdb_sha256: sha256_bytes(&buf),
            partitions: area
                .rdb
                .partitions
                .iter()
                .map(|part| PartitionFacts {
                    drive_name: part.drive_name.clone(),
                    dostype: part.dostype_str.clone(),
                    size_bytes: part.size_bytes,
                })
                .collect(),
        });
    }

    Ok(CardFacts {
        total_bytes: card.total_bytes,
        mbr_sha256: sha256_bytes(&sector),
        slots,
        areas,
    })
}

/// Describe a card ART has just built.
///
/// Everything but `source` is read back off the finished image, so the
/// manifest records what is *there* rather than what the builder meant.
pub fn describe_card(
    image: &Path,
    source: SourceFacts,
    boot_files: Vec<ManifestFile>,
    built_at: Option<String>,
) -> CoreResult<CardManifest> {
    let facts = read_facts(image)?;

    Ok(CardManifest {
        schema: MANIFEST_SCHEMA,
        art_version: env!("CARGO_PKG_VERSION").to_string(),
        built_at,
        total_bytes: facts.total_bytes,
        mbr_sha256: facts.mbr_sha256,
        slots: facts.slots,
        source,
        boot_files,
        areas: facts.areas,
        os: Vec::new(),
    })
}

/// Check a card against the manifest that describes it.
pub fn verify_against_image(manifest: &CardManifest, image: &Path) -> CoreResult<ManifestReport> {
    if manifest.schema > MANIFEST_SCHEMA {
        return Ok(ManifestReport {
            findings: vec![ManifestFinding::SchemaTooNew {
                found: manifest.schema,
                understood: MANIFEST_SCHEMA,
            }],
            not_checked: Vec::new(),
        });
    }

    let facts = read_facts(image)?;
    let mut findings = Vec::new();

    if facts.total_bytes != manifest.total_bytes {
        findings.push(ManifestFinding::SizeChanged {
            expected: manifest.total_bytes,
            found: facts.total_bytes,
        });
    }
    if facts.mbr_sha256 != manifest.mbr_sha256 || facts.slots != manifest.slots {
        findings.push(ManifestFinding::PartitionTableChanged);
    }

    if facts.areas.len() != manifest.areas.len() {
        findings.push(ManifestFinding::AreaCountChanged {
            expected: manifest.areas.len(),
            found: facts.areas.len(),
        });
    }

    for (index, (want, got)) in manifest.areas.iter().zip(facts.areas.iter()).enumerate() {
        if want.offset_bytes != got.offset_bytes {
            findings.push(ManifestFinding::AreaMoved {
                area: index,
                expected: want.offset_bytes,
                found: got.offset_bytes,
            });
        }
        if want.length_bytes != got.length_bytes {
            findings.push(ManifestFinding::AreaResized {
                area: index,
                expected: want.length_bytes,
                found: got.length_bytes,
            });
        }
        if want.rdb_sha256 != got.rdb_sha256 {
            findings.push(ManifestFinding::RdbChanged { area: index });
        }
        if want.partitions.len() != got.partitions.len() {
            findings.push(ManifestFinding::PartitionCountChanged {
                area: index,
                expected: want.partitions.len(),
                found: got.partitions.len(),
            });
            continue;
        }
        for (wanted, found) in want.partitions.iter().zip(got.partitions.iter()) {
            if wanted != found {
                findings.push(ManifestFinding::PartitionChanged {
                    area: index,
                    name: wanted.drive_name.clone(),
                });
            }
        }
    }

    Ok(ManifestReport {
        findings,
        not_checked: vec![NotChecked::BootPartitionFiles {
            count: manifest.boot_files.len(),
        }],
    })
}

/// Where a card's manifest lives: beside the image, named after it.
pub fn manifest_path_for(image: &Path) -> std::path::PathBuf {
    let mut name = image.as_os_str().to_os_string();
    name.push(".manifest.json");
    std::path::PathBuf::from(name)
}

/// Read a manifest off disk.
pub fn read_manifest(path: &Path) -> CoreResult<CardManifest> {
    let text = std::fs::read_to_string(path)?;
    serde_json::from_str(&text).map_err(|err| CoreError::Malformed {
        format: "card manifest".into(),
        detail: format!(
            "'{}' is not a manifest ART understands: {err}",
            path.display()
        ),
    })
}

/// Render a manifest for writing. Pretty-printed: it is meant to be read, and
/// to diff cleanly between two builds.
pub fn render_manifest(manifest: &CardManifest) -> CoreResult<String> {
    serde_json::to_string_pretty(manifest).map_err(|err| CoreError::Malformed {
        format: "card manifest".into(),
        detail: err.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::card::build::{build_card, AreaSpec, CardSpec};
    use crate::core::fat32::BootFile;
    use crate::core::jobs::NoProgress;
    use crate::core::pistorm::hardware::{AmigaTarget, PiModel, PistormVariant};
    use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec};

    const GIB: u64 = 1024 * 1024 * 1024;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("art-manifest-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    pub(super) fn source() -> SourceFacts {
        SourceFacts {
            archive_name: "Emu68-pistorm.zip".into(),
            archive_sha256: "a".repeat(64),
            kickstart_name: Some("kick.rom".into()),
            kickstart_sha256: Some("b".repeat(64)),
            kickstart_file: None,
            kickstart_stated_major: None,
            hardware: PistormHardware {
                amiga: AmigaTarget::A500,
                variant: PistormVariant::Classic,
                pi: PiModel::Pi3APlus,
            },
            line: Emu68Line::Stable,
            kernel_file: "Emu68-pistorm.gz".into(),
        }
    }

    pub(super) fn manifest() -> CardManifest {
        CardManifest {
            schema: MANIFEST_SCHEMA,
            art_version: env!("CARGO_PKG_VERSION").into(),
            built_at: None,
            total_bytes: 2 * 1024 * 1024 * 1024,
            mbr_sha256: "c".repeat(64),
            slots: Vec::new(),
            source: source(),
            boot_files: vec![ManifestFile {
                name: "kick.rom".into(),
                bytes: 524_288,
                sha256: "d".repeat(64),
            }],
            areas: Vec::new(),
            os: Vec::new(),
        }
    }

    fn boot_files() -> Vec<ManifestFile> {
        vec![ManifestFile {
            name: "config.txt".into(),
            bytes: 12,
            sha256: sha256_bytes(b"arm_64bit=1\n"),
        }]
    }

    fn build(dest: &Path, total: u64, drive: &str) {
        build_card(
            dest,
            &CardSpec {
                total_bytes: total,
                boot_bytes: 0,
                label: "ART CARD".into(),
                boot_files: vec![BootFile {
                    name: "config.txt".into(),
                    bytes: b"arm_64bit=1\n".to_vec(),
                }],
                areas: vec![AreaSpec {
                    size_bytes: 0,
                    partitions: vec![PartitionSpec {
                        drive_name: drive.into(),
                        fs_type: AmigaHardDiskFs::FfsStandard,
                        size_mb: 512,
                        bootable: true,
                        boot_priority: 0,
                        num_buffers: 0,
                    }],
                    file_systems: Vec::new(),
                }],
            },
            &NoProgress,
        )
        .unwrap();
    }

    /// The manifest describes the card that is *there*: its table, where each
    /// Amiga disk landed, and what its RDB says.
    #[test]
    fn a_manifest_describes_the_card_that_was_built() {
        let dir = scratch("describe");
        let image = dir.join("card.img");
        build(&image, 2 * GIB, "SDH0");

        let manifest = describe_card(&image, source(), boot_files(), None).unwrap();

        assert_eq!(manifest.schema, MANIFEST_SCHEMA);
        assert_eq!(manifest.total_bytes, 2 * GIB);
        assert_eq!(manifest.slots.len(), 2, "FAT32 and one Amiga area");
        assert_eq!(manifest.slots[0].type_byte, 0x0C);
        assert_eq!(manifest.slots[1].type_byte, 0x76);
        assert_eq!(manifest.areas.len(), 1);
        assert_eq!(manifest.areas[0].partitions.len(), 1);
        assert_eq!(manifest.areas[0].partitions[0].drive_name, "SDH0");
        assert_eq!(manifest.mbr_sha256.len(), 64);
        assert!(manifest.os.is_empty(), "SD-2's field, empty until then");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The point of the whole thing.
    #[test]
    fn a_manifest_verifies_the_card_it_describes() {
        let dir = scratch("verify");
        let image = dir.join("card.img");
        build(&image, 2 * GIB, "SDH0");

        let manifest = describe_card(&image, source(), boot_files(), None).unwrap();
        let report = verify_against_image(&manifest, &image).unwrap();

        assert!(report.matches(), "{:?}", report.findings);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **What it does not look at, in the report rather than in a comment.**
    /// ART writes FAT32 and cannot read one, so the boot partition's files are
    /// recorded and left unchecked — said out loud, never passed over.
    #[test]
    fn the_boot_partitions_files_are_reported_as_unchecked() {
        let dir = scratch("unchecked");
        let image = dir.join("card.img");
        build(&image, 2 * GIB, "SDH0");

        let manifest = describe_card(&image, source(), boot_files(), None).unwrap();
        let report = verify_against_image(&manifest, &image).unwrap();

        assert_eq!(
            report.not_checked,
            vec![NotChecked::BootPartitionFiles { count: 1 }]
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// One byte of the partition table, and the check says which part changed.
    #[test]
    fn one_byte_changed_in_the_partition_table_is_named() {
        use std::io::Write as _;

        let dir = scratch("tamper");
        let image = dir.join("card.img");
        build(&image, 2 * GIB, "SDH0");
        let manifest = describe_card(&image, source(), boot_files(), None).unwrap();

        // A byte inside the first entry's CHS fields: the table's bytes change
        // and `read_card` still parses it, which is the case a checksum has to
        // catch and a re-read of the parsed values would not.
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .open(&image)
            .unwrap();
        file.seek(SeekFrom::Start(446 + 1)).unwrap();
        file.write_all(&[0x01]).unwrap();
        file.sync_all().unwrap();
        drop(file);

        let report = verify_against_image(&manifest, &image).unwrap();

        assert!(!report.matches());
        assert!(
            report
                .findings
                .contains(&ManifestFinding::PartitionTableChanged),
            "{:?}",
            report.findings
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **Why the RDB is hashed as a window rather than compared field by
    /// field.** A byte written into the reserved area past the blocks ART
    /// wrote leaves the RDB parsing exactly as before — every partition, every
    /// name, every size still reads back right — so a check that compared the
    /// parsed values would pass. The window catches it.
    #[test]
    fn a_byte_written_into_the_reserved_area_is_caught() {
        use std::io::Write as _;

        let dir = scratch("rdb-tamper");
        let image = dir.join("card.img");
        build(&image, 2 * GIB, "SDH0");
        let manifest = describe_card(&image, source(), boot_files(), None).unwrap();
        let area = manifest.areas[0].offset_bytes;

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .open(&image)
            .unwrap();
        file.seek(SeekFrom::Start(area + 100 * 1024)).unwrap();
        file.write_all(&[0xA5]).unwrap();
        file.sync_all().unwrap();
        drop(file);

        let report = verify_against_image(&manifest, &image).unwrap();

        assert_eq!(
            report.findings,
            vec![ManifestFinding::RdbChanged { area: 0 }],
            "and nothing else: the RDB still parses the same"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A manifest belongs to one card. Another card's is refused with the
    /// difference named, not accepted because the shape happens to match.
    #[test]
    fn another_cards_manifest_is_refused() {
        let dir = scratch("other");
        let mine = dir.join("mine.img");
        let theirs = dir.join("theirs.img");
        build(&mine, 2 * GIB, "SDH0");
        build(&theirs, 4 * GIB, "WORK");

        let manifest = describe_card(&mine, source(), boot_files(), None).unwrap();
        let report = verify_against_image(&manifest, &theirs).unwrap();

        assert!(!report.matches());
        assert!(
            report.findings.iter().any(|f| matches!(
                f,
                ManifestFinding::SizeChanged { .. } | ManifestFinding::AreaResized { .. }
            )),
            "{:?}",
            report.findings
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A manifest from a newer ART is refused whole rather than half-read: the
    /// fields this one does not know are the ones it cannot check.
    #[test]
    fn a_newer_schema_is_refused_rather_than_half_understood() {
        let dir = scratch("schema");
        let image = dir.join("card.img");
        build(&image, 2 * GIB, "SDH0");

        let mut manifest = describe_card(&image, source(), boot_files(), None).unwrap();
        manifest.schema = MANIFEST_SCHEMA + 1;

        let report = verify_against_image(&manifest, &image).unwrap();

        assert_eq!(
            report.findings,
            vec![ManifestFinding::SchemaTooNew {
                found: MANIFEST_SCHEMA + 1,
                understood: MANIFEST_SCHEMA,
            }]
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **G9.** A manifest lists the boot files and, until now, gave no way to
    /// tell which of them is the Kickstart: `kickstart_name` is the *source*
    /// file's name on the user's PC, and after ART-128 not even the same
    /// bytes. The on-card name is what a reader needs.
    #[test]
    fn the_manifest_says_which_boot_file_is_the_kickstart() {
        let mut facts = source();
        facts.kickstart_file = Some("kick.rom".into());
        let rendered = render_manifest(&CardManifest {
            source: facts,
            ..manifest()
        })
        .unwrap();
        let read: CardManifest = serde_json::from_str(&rendered).unwrap();

        assert_eq!(read.source.kickstart_file.as_deref(), Some("kick.rom"));
    }

    /// A manifest written before these fields existed still reads.
    ///
    /// **Both** of G9's additions come out, not one: every manifest ART has
    /// written to date lacks both, so testing one key and leaving the other
    /// exercised half the change. The subject is the shape of the file — a
    /// `SourceFacts` written before G9 reads back whole, with both new facts
    /// absent rather than wrong.
    ///
    /// **What this test does *not* pin, measured rather than assumed:** the
    /// `#[serde(default)]` attributes on those fields. Deleting either one
    /// leaves this passing, because serde's derive already treats a missing
    /// `Option<T>` as `None`. The attributes state the intent; they are not
    /// what keeps older manifests readable. Change either field to a
    /// non-`Option` type and the attribute starts carrying real weight — and
    /// this test is then the thing that measures it.
    #[test]
    fn an_older_manifest_without_the_field_still_reads() {
        let mut value = serde_json::to_value(manifest()).unwrap();
        let source = value["source"].as_object_mut().unwrap();
        source.remove("kickstart_file");
        source.remove("kickstart_stated_major");
        let read: CardManifest = serde_json::from_value(value).unwrap();

        assert_eq!(read.source.kickstart_file, None);
        assert_eq!(read.source.kickstart_stated_major, None);
    }

    /// It has to survive a trip through the file it is written to.
    #[test]
    fn a_manifest_round_trips_through_json() {
        let dir = scratch("json");
        let image = dir.join("card.img");
        build(&image, 2 * GIB, "SDH0");

        let manifest =
            describe_card(&image, source(), boot_files(), Some("2026-08-14".into())).unwrap();
        let path = manifest_path_for(&image);
        std::fs::write(&path, render_manifest(&manifest).unwrap()).unwrap();

        assert_eq!(
            path.file_name().unwrap().to_string_lossy(),
            "card.img.manifest.json"
        );
        assert_eq!(read_manifest(&path).unwrap(), manifest);

        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// A shared fixture for other test modules that need a valid [`CardManifest`]
/// without building a card. Calls straight into `mod tests`'s own
/// `manifest()`/`source()` rather than holding a second hand-typed literal —
/// one definition, two callers, so a field added to either side cannot drift
/// out of step with the other.
#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;

    pub(crate) fn sample_manifest() -> CardManifest {
        super::tests::manifest()
    }
}
