//! Is this image a card that will boot? (SD-1 · G8)
//!
//! **The last thing ART does before handing the file over.** With G1 gone
//! there is no flash-and-verify step after this one: the next thing that
//! touches the card is somebody else's imager, and then a real Amiga. So this
//! is the only gate between a build and a machine that does or does not come
//! up.
//!
//! ## Three kinds of answer, kept apart
//!
//! - **Checked**, by reading the image: the partition table's rules, each
//!   area's RDB, whether any partition names a filesystem the card does not
//!   carry.
//! - **Checked through the manifest**, because ART writes FAT32 and does not
//!   read one: whether the boot partition holds the four files the firmware
//!   needs. Answered from what the manifest recorded, not from the card, and
//!   the report says which of the two it is.
//! - **Not ART's to check at all**: whether the card was flashed, whether HDMI
//!   was plugged in before power, whether the Pi in the machine is the one the
//!   build was made for. These are listed as steps for the user rather than
//!   quietly omitted — the checklist is meant to be walked through at the
//!   machine (§50).
//!
//! Mixing the three would be the failure §89 forbids: a green tick that means
//! "ART did not look" reads exactly like one that means "ART looked and it is
//! right".

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::core::card::manifest::{verify_against_image, CardManifest, ManifestFinding};
use crate::core::card::{read_card, CardImage};
use crate::core::error::CoreResult;
use crate::core::mbr::{FIRST_PARTITION_LBA, MAX_AREAS, SECTOR_BYTES};

/// The four files a Raspberry Pi needs on the boot partition before any of
/// this is an Amiga.
///
/// `config.txt` and `cmdline.txt` ART writes itself; the kernel and the ROM
/// are named *by* `config.txt`, which is why they are looked up by the name
/// the manifest recorded rather than by a name ART chose (ART-103).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BootFileRole {
    Config,
    Cmdline,
    Kernel,
    Kickstart,
}

/// One thing that was, could not be, or did not need to be checked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum HealthCheck {
    /// The FAT32 partition is the first one. **This is SD-0's unit-0 rule**:
    /// the m68k side sees unit 0 as the whole card, MBR included, so an Amiga
    /// partition at byte zero is a card that eats its own partition table.
    BootPartitionFirst,
    /// At sector 2048, where the Pi's firmware and every modern imager put it.
    BootPartitionAligned { lba: u64 },
    /// One to three, because there are four primary slots and one is spent.
    AmigaAreaCount { count: usize },
    /// Each Amiga area starts on a 4 MiB boundary — the erase-block size flash
    /// of this size is built around.
    AreasAligned,
    /// No two partitions share a sector.
    NothingOverlaps,
    /// Nothing runs past the end of the file.
    EverythingInsideTheImage,
    /// The area has an RDB where one is expected.
    AreaHasRdb { area: usize },
    /// And its checksum is right.
    AreaRdbChecksum { area: usize },
    /// **ART-084, as a check.** A partition naming a filesystem no area on the
    /// card carries is one an Amiga ignores in silence.
    EveryPartitionCanMount { unmountable: usize },
    /// The card still matches the manifest written beside it (G7). The
    /// findings travel rather than a count: "one thing disagrees" is not an
    /// answer somebody can act on.
    ManifestAgrees { findings: Vec<ManifestFinding> },
    /// A file the firmware needs, looked up in the manifest by the name the
    /// build recorded.
    BootFile { role: BootFileRole, name: String },
}

/// What ART concluded about one check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CheckState {
    Pass,
    Fail,
    /// ART could not answer it. **Never rendered as a pass.**
    NotChecked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthItem {
    pub check: HealthCheck,
    pub state: CheckState,
}

/// Something only the person at the machine can do or confirm.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ManualStep {
    /// ART builds the image and never touches a card (§56).
    FlashTheCard,
    /// The VPU configures the HDMI port at boot and not after, so a screen
    /// plugged in later stays dark for that session.
    HdmiBeforePower,
    /// The build was made for one Pi; the machine has whichever is in it.
    PiModelMatches { pi: String },
    /// SD-1 builds the shape, not the system.
    VolumesNeedFormatting { count: usize },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthReport {
    pub items: Vec<HealthItem>,
    /// Always present, always separate from what ART checked.
    pub by_hand: Vec<ManualStep>,
}

impl HealthReport {
    /// Whether anything ART could check came back wrong.
    ///
    /// `NotChecked` is deliberately not a failure and deliberately not a pass:
    /// it is what the report exists to *say*.
    pub fn ok(&self) -> bool {
        !self.items.iter().any(|item| item.state == CheckState::Fail)
    }

    pub fn failures(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.state == CheckState::Fail)
            .count()
    }
}

fn state(ok: bool) -> CheckState {
    if ok {
        CheckState::Pass
    } else {
        CheckState::Fail
    }
}

/// Check a built image, and — when there is one — the manifest beside it.
///
/// `manifest` is optional because a card ART did not build has none, and that
/// is a `NotChecked`, not a failure.
pub fn check_image(
    image: &Path,
    manifest: Option<&CardManifest>,
    pi: &str,
) -> CoreResult<HealthReport> {
    let card = read_card(image)?;
    let mut items = Vec::new();

    structural(&card, &mut items);
    from_manifest(image, manifest, &mut items)?;

    let by_hand = vec![
        ManualStep::FlashTheCard,
        ManualStep::HdmiBeforePower,
        ManualStep::PiModelMatches { pi: pi.into() },
        ManualStep::VolumesNeedFormatting {
            count: card
                .areas
                .iter()
                .map(|area| area.rdb.partitions.len())
                .sum(),
        },
    ];

    Ok(HealthReport { items, by_hand })
}

fn structural(card: &CardImage, items: &mut Vec<HealthItem>) {
    let mut push = |check: HealthCheck, ok: bool| {
        items.push(HealthItem {
            check,
            state: state(ok),
        })
    };

    let slots: Vec<_> = card
        .mbr
        .as_ref()
        .map(|mbr| mbr.partitions.clone())
        .unwrap_or_default();

    // The first slot in *table order* — the one at the lowest sector — has to
    // be the FAT32. Asking "is any of them FAT32" would pass a card whose
    // Amiga area sits at byte zero, which is the arrangement that eats the
    // partition table.
    let first = slots.iter().min_by_key(|part| part.start_lba);
    push(
        HealthCheck::BootPartitionFirst,
        first.is_some_and(|part| part.type_byte == 0x0C || part.type_byte == 0x0B),
    );
    push(
        HealthCheck::BootPartitionAligned {
            lba: first.map(|part| part.start_lba).unwrap_or(0),
        },
        first.is_some_and(|part| part.start_lba == FIRST_PARTITION_LBA),
    );

    push(
        HealthCheck::AmigaAreaCount {
            count: card.areas.len(),
        },
        !card.areas.is_empty() && card.areas.len() <= MAX_AREAS,
    );

    const ALIGN: u64 = 4 * 1024 * 1024;
    push(
        HealthCheck::AreasAligned,
        card.areas
            .iter()
            .all(|area| area.offset_bytes.is_multiple_of(ALIGN)),
    );

    // Sorted by start, then each has to end where the next begins or earlier.
    let mut spans: Vec<(u64, u64)> = slots
        .iter()
        .map(|part| {
            (
                part.start_lba * SECTOR_BYTES,
                (part.start_lba + part.sector_count) * SECTOR_BYTES,
            )
        })
        .collect();
    spans.sort_unstable();
    push(
        HealthCheck::NothingOverlaps,
        spans.windows(2).all(|pair| pair[0].1 <= pair[1].0),
    );
    push(
        HealthCheck::EverythingInsideTheImage,
        spans.iter().all(|(_, end)| *end <= card.total_bytes),
    );

    for (index, area) in card.areas.iter().enumerate() {
        push(
            HealthCheck::AreaHasRdb { area: index },
            !area.rdb.partitions.is_empty() || area.rdb.checksum_valid,
        );
        push(
            HealthCheck::AreaRdbChecksum { area: index },
            area.rdb.checksum_valid,
        );
    }

    let unmountable = card.partitions_missing_driver().len();
    push(
        HealthCheck::EveryPartitionCanMount { unmountable },
        unmountable == 0,
    );
}

fn from_manifest(
    image: &Path,
    manifest: Option<&CardManifest>,
    items: &mut Vec<HealthItem>,
) -> CoreResult<()> {
    let Some(manifest) = manifest else {
        // No manifest is not a failure: a card ART did not build has none.
        // Every answer that would have come from one is `NotChecked`, said out
        // loud rather than left off the list.
        items.push(HealthItem {
            check: HealthCheck::ManifestAgrees {
                findings: Vec::new(),
            },
            state: CheckState::NotChecked,
        });
        for role in [
            BootFileRole::Config,
            BootFileRole::Cmdline,
            BootFileRole::Kernel,
            BootFileRole::Kickstart,
        ] {
            items.push(HealthItem {
                check: HealthCheck::BootFile {
                    role,
                    name: String::new(),
                },
                state: CheckState::NotChecked,
            });
        }
        return Ok(());
    };

    let report = verify_against_image(manifest, image)?;
    items.push(HealthItem {
        state: state(report.findings.is_empty()),
        check: HealthCheck::ManifestAgrees {
            findings: report.findings.clone(),
        },
    });

    // A schema ART cannot read means nothing under it was compared either.
    if report
        .findings
        .iter()
        .any(|f| matches!(f, ManifestFinding::SchemaTooNew { .. }))
    {
        return Ok(());
    }

    let has = |name: &str| manifest.boot_files.iter().any(|file| file.name == name);
    let kickstart = manifest.source.kickstart_name.as_deref();

    for (role, name, required) in [
        (BootFileRole::Config, "config.txt".to_string(), true),
        (BootFileRole::Cmdline, "cmdline.txt".to_string(), true),
        (
            BootFileRole::Kernel,
            manifest.source.kernel_file.clone(),
            true,
        ),
        // The ROM's name on the card is `config.txt`'s `initramfs` line, not
        // the file name on the user's disk — and a card with no ROM was built
        // on purpose, so it is `NotChecked` rather than a failure.
        (
            BootFileRole::Kickstart,
            manifest
                .boot_files
                .iter()
                .map(|file| file.name.clone())
                .find(|name| name.eq_ignore_ascii_case("kick.rom"))
                .unwrap_or_default(),
            kickstart.is_some(),
        ),
    ] {
        items.push(HealthItem {
            state: if !required {
                CheckState::NotChecked
            } else {
                state(!name.is_empty() && has(&name))
            },
            check: HealthCheck::BootFile { role, name },
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::card::build::{build_card, AreaSpec, CardSpec};
    use crate::core::card::manifest::{describe_card, ManifestFile, SourceFacts};
    use crate::core::fat32::BootFile;
    use crate::core::hashing::sha256_bytes;
    use crate::core::jobs::NoProgress;
    use crate::core::mbr::{write_mbr, CardLayout, MbrPartition, PartitionKind};
    use crate::core::pistorm::hardware::{
        AmigaTarget, Emu68Line, PiModel, PistormHardware, PistormVariant,
    };
    use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec};

    const GIB: u64 = 1024 * 1024 * 1024;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("art-health-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn build(dest: &Path, fs: AmigaHardDiskFs) {
        build_card(
            dest,
            &CardSpec {
                total_bytes: 2 * GIB,
                boot_bytes: 0,
                label: "ART CARD".into(),
                boot_files: vec![
                    BootFile {
                        name: "config.txt".into(),
                        bytes: b"kernel=Emu68-pistorm.gz\n".to_vec(),
                    },
                    BootFile {
                        name: "cmdline.txt".into(),
                        bytes: b"sd.unit0=ro\n".to_vec(),
                    },
                    BootFile {
                        name: "Emu68-pistorm.gz".into(),
                        bytes: b"kernel".to_vec(),
                    },
                    BootFile {
                        name: "kick.rom".into(),
                        bytes: vec![0xAB; 1024],
                    },
                ],
                areas: vec![AreaSpec {
                    size_bytes: 0,
                    partitions: vec![PartitionSpec {
                        drive_name: "SDH0".into(),
                        fs_type: fs,
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

    fn manifest_for(image: &Path, with_rom: bool) -> CardManifest {
        describe_card(
            image,
            SourceFacts {
                archive_name: "Emu68-pistorm.zip".into(),
                archive_sha256: "a".repeat(64),
                kickstart_name: with_rom.then(|| "A1200.rom".to_string()),
                kickstart_sha256: with_rom.then(|| "b".repeat(64)),
                hardware: PistormHardware {
                    amiga: AmigaTarget::A500,
                    variant: PistormVariant::Classic,
                    pi: PiModel::Pi3APlus,
                },
                line: Emu68Line::Stable,
                kernel_file: "Emu68-pistorm.gz".into(),
            },
            vec![
                ManifestFile {
                    name: "config.txt".into(),
                    bytes: 24,
                    sha256: sha256_bytes(b"kernel=Emu68-pistorm.gz\n"),
                },
                ManifestFile {
                    name: "cmdline.txt".into(),
                    bytes: 12,
                    sha256: sha256_bytes(b"sd.unit0=ro\n"),
                },
                ManifestFile {
                    name: "Emu68-pistorm.gz".into(),
                    bytes: 6,
                    sha256: sha256_bytes(b"kernel"),
                },
                ManifestFile {
                    name: "kick.rom".into(),
                    bytes: 1024,
                    sha256: sha256_bytes(&[0xAB; 1024]),
                },
            ],
            None,
        )
        .unwrap()
    }

    /// By variant, ignoring the numbers each carries.
    ///
    /// **Not `discriminant`**: every `BootFile` shares one, so the kickstart
    /// check would answer with the config file's state and two tests here
    /// would pass for the wrong reason.
    fn find(report: &HealthReport, want: &HealthCheck) -> CheckState {
        let same = |a: &HealthCheck, b: &HealthCheck| match (a, b) {
            (HealthCheck::BootFile { role: x, .. }, HealthCheck::BootFile { role: y, .. }) => {
                x == y
            }
            _ => std::mem::discriminant(a) == std::mem::discriminant(b),
        };
        report
            .items
            .iter()
            .find(|item| same(&item.check, want))
            .unwrap_or_else(|| panic!("no such check: {want:?}"))
            .state
    }

    /// A card ART built, with its manifest, passes everything.
    #[test]
    fn a_card_art_built_passes_every_check() {
        let dir = scratch("good");
        let image = dir.join("card.img");
        build(&image, AmigaHardDiskFs::FfsStandard);
        let manifest = manifest_for(&image, true);

        let report = check_image(&image, Some(&manifest), "pi3-a-plus").unwrap();

        assert!(report.ok(), "{:?}", report.items);
        assert_eq!(report.failures(), 0);
        assert!(
            report
                .items
                .iter()
                .all(|item| item.state == CheckState::Pass),
            "nothing should be unanswerable for a card ART just built: {:?}",
            report.items
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **What ART cannot check is always on the report**, never omitted
    /// because it happens to be fine. The checklist is meant to be walked
    /// through at the machine.
    #[test]
    fn the_steps_only_a_human_can_take_are_always_listed() {
        let dir = scratch("manual");
        let image = dir.join("card.img");
        build(&image, AmigaHardDiskFs::FfsStandard);

        let report = check_image(&image, None, "pi3-a-plus").unwrap();

        assert!(report.by_hand.contains(&ManualStep::FlashTheCard));
        assert!(report.by_hand.contains(&ManualStep::HdmiBeforePower));
        assert!(report.by_hand.contains(&ManualStep::PiModelMatches {
            pi: "pi3-a-plus".into()
        }));
        assert!(report
            .by_hand
            .contains(&ManualStep::VolumesNeedFormatting { count: 1 }));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// No manifest is not a failure, and it is not a pass either.
    #[test]
    fn a_card_with_no_manifest_reports_those_checks_as_unanswered() {
        let dir = scratch("no-manifest");
        let image = dir.join("card.img");
        build(&image, AmigaHardDiskFs::FfsStandard);

        let report = check_image(&image, None, "pi3-a-plus").unwrap();

        assert!(report.ok(), "unanswered is not failed");
        assert_eq!(
            find(
                &report,
                &HealthCheck::ManifestAgrees {
                    findings: Vec::new()
                }
            ),
            CheckState::NotChecked
        );
        assert_eq!(
            find(
                &report,
                &HealthCheck::BootFile {
                    role: BootFileRole::Kernel,
                    name: String::new()
                }
            ),
            CheckState::NotChecked,
            "and the boot files with it — they are answered from the manifest"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ART-084 as a gate: a PFS3 partition with no driver anywhere on the card
    /// is one an Amiga ignores in silence, and this is the last chance to say
    /// so before the file is handed over.
    #[test]
    fn a_partition_naming_a_filesystem_the_card_lacks_fails() {
        let dir = scratch("unmountable");
        let image = dir.join("card.img");
        build(&image, AmigaHardDiskFs::Pfs3Standard);

        let report = check_image(&image, None, "pi3-a-plus").unwrap();

        assert!(!report.ok());
        assert_eq!(
            find(
                &report,
                &HealthCheck::EveryPartitionCanMount { unmountable: 0 }
            ),
            CheckState::Fail
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **SD-0's unit-0 rule, as a check.** `plan_card` cannot express an Amiga
    /// area at byte zero, so this hand-writes the table `plan_card` refuses —
    /// which is the only way to prove the check would catch a card built by
    /// something else.
    #[test]
    fn an_amiga_area_before_the_boot_partition_fails() {
        use std::io::{Seek, SeekFrom, Write};

        let dir = scratch("unit0");
        let image = dir.join("card.img");
        build(&image, AmigaHardDiskFs::FfsStandard);

        // The real card's areas, with the two slots' *positions* swapped: the
        // Amiga disk first, the FAT32 after it.
        let card = read_card(&image).unwrap();
        let real = card.mbr.as_ref().unwrap();
        let boot = real.partitions[0];
        let amiga = real.partitions[1];
        let swapped = CardLayout {
            total_sectors: card.total_bytes / SECTOR_BYTES,
            boot: MbrPartition {
                index: 1,
                kind: PartitionKind::Fat32,
                type_byte: 0x0C,
                bootable: true,
                start_lba: amiga.start_lba,
                sector_count: boot.sector_count,
            },
            areas: vec![MbrPartition {
                index: 2,
                kind: PartitionKind::AmigaRdb,
                type_byte: 0x76,
                bootable: false,
                start_lba: FIRST_PARTITION_LBA,
                sector_count: amiga.sector_count,
            }],
        };

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .open(&image)
            .unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        file.write_all(&write_mbr(&swapped)).unwrap();
        file.sync_all().unwrap();
        drop(file);

        let report = check_image(&image, None, "pi3-a-plus").unwrap();

        assert_eq!(
            find(&report, &HealthCheck::BootPartitionFirst),
            CheckState::Fail,
            "an Amiga area at the front is a card that eats its own table"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A manifest that no longer describes the card fails the report, not just
    /// the manifest check on its own.
    #[test]
    fn a_manifest_that_disagrees_fails_the_report() {
        let dir = scratch("stale");
        let image = dir.join("card.img");
        build(&image, AmigaHardDiskFs::FfsStandard);

        let mut manifest = manifest_for(&image, true);
        manifest.total_bytes += 512;

        let report = check_image(&image, Some(&manifest), "pi3-a-plus").unwrap();

        assert!(!report.ok());
        assert_eq!(
            find(
                &report,
                &HealthCheck::ManifestAgrees {
                    findings: Vec::new()
                }
            ),
            CheckState::Fail
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A card built with no ROM is a card that will not boot — but it was
    /// asked for, so the ROM check is unanswered rather than failed, and the
    /// warning that it will not boot is the build's to make.
    #[test]
    fn a_card_with_no_rom_leaves_the_kickstart_check_unanswered() {
        let dir = scratch("no-rom");
        let image = dir.join("card.img");
        build(&image, AmigaHardDiskFs::FfsStandard);
        let manifest = manifest_for(&image, false);

        let report = check_image(&image, Some(&manifest), "pi3-a-plus").unwrap();

        assert!(report.ok());
        assert_eq!(
            find(
                &report,
                &HealthCheck::BootFile {
                    role: BootFileRole::Kickstart,
                    name: String::new()
                }
            ),
            CheckState::NotChecked
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
