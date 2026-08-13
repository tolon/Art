//! Building a PiStorm card image (SD-1 · G2).
//!
//! The other three quarters of the card were already here: [`plan_card`] and
//! [`write_mbr`] decide its shape and write its table, [`create_boot_partition`]
//! makes the FAT32 the Raspberry Pi boots from, and [`create_rdb_layout`] builds
//! an Amiga partition table. This is what puts them in one file in the right
//! order, and it is the first place in ART where all four exist at once.
//!
//! ## What is built, and what is not
//!
//! A card image with: a partition table, a formatted boot partition holding
//! whatever files the caller supplies, and an **RDB at the start of every Amiga
//! area**. The volumes those RDBs describe are *not* formatted — a PFS3 volume
//! is SD-2's work (G3) and an FFS one is not what a card this size wants. So
//! what comes out is a card an Amiga will boot into a partition table it can
//! see, with volumes it will offer to format. That is a real milestone and it
//! is not a finished appliance; nothing here says otherwise.
//!
//! ## Every RDB goes in at its own offset
//!
//! Each Amiga area is a separate disk to the m68k side, and its RDB's block
//! numbers are relative to the area's own start (`docs/sd2-card-layout.md`).
//! Writing one is therefore the same shape as ART-043, which was the mirror of
//! this on the reading side: a block number that is right for a volume and
//! wrong for the file it lives in.

use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

use crate::core::card::{read_card, CardImage};
use crate::core::error::{CoreError, CoreResult};
use crate::core::fat32::{create_boot_partition, BootFile, DEFAULT_LABEL};
use crate::core::jobs::ProgressSink;
use crate::core::mbr::{plan_card, write_mbr, CardLayout, SECTOR_BYTES};
use crate::core::rdb::{create_rdb_layout, FileSystemSpec, PartitionSpec};

/// One Amiga disk on the card, as it is to be built.
pub struct AreaSpec {
    /// How big this disk is, in bytes. **The last area may pass `0`** for
    /// "whatever is left" — see [`plan_card`], whose rule this is.
    pub size_bytes: u64,
    pub partitions: Vec<PartitionSpec>,
    /// Filesystem drivers to embed in this area's RDB.
    ///
    /// A `PDS\3` partition with no PFS3 driver anywhere on the card is one an
    /// Amiga ignores in silence (ART-084), and the drivers are the **card's**
    /// rather than the area's — a caller may legitimately put them in one area
    /// and reference them from another (ART-097). Nothing here enforces that
    /// either way; `CardImage::partitions_missing_driver` is what answers it,
    /// and the builder runs it before it hands the file over.
    pub file_systems: Vec<FileSystemSpec>,
}

/// A card to build.
pub struct CardSpec {
    pub total_bytes: u64,
    /// `0` for the 1.10 GiB both real cards carry.
    pub boot_bytes: u64,
    /// The FAT32 volume label.
    pub label: String,
    /// What goes on the boot partition: the Emu68 kernel, a Kickstart image,
    /// `config.txt`, `cmdline.txt`. Choosing them is the caller's job — this
    /// places what it is given.
    pub boot_files: Vec<BootFile>,
    /// One to three Amiga disks.
    pub areas: Vec<AreaSpec>,
}

impl CardSpec {
    /// A card with ART's defaults: the measured boot partition, ART's label.
    pub fn new(total_bytes: u64, areas: Vec<AreaSpec>) -> Self {
        Self {
            total_bytes,
            boot_bytes: 0,
            label: DEFAULT_LABEL.into(),
            boot_files: Vec::new(),
            areas,
        }
    }
}

/// What a finished build produced.
#[derive(Debug)]
pub struct BuiltCard {
    pub layout: CardLayout,
    /// The card read back out of the file that was just written, by the same
    /// reader that opens somebody else's card. A build that cannot be read is
    /// not a build.
    pub verified: CardImage,
}

/// Build a card image at `dest`.
///
/// **`SAFE_CREATE`**: an existing file is refused rather than replaced. A card
/// image is tens of gigabytes of somebody's afternoon, and "it was already
/// there" is not a reason to lose it.
///
/// The file is created *sparse* where the filesystem underneath allows it —
/// `set_len` on NTFS costs nothing and takes no space — so building a 128 GB
/// card writes the few megabytes that are actually structure.
pub fn build_card(
    dest: &Path,
    spec: &CardSpec,
    progress: &dyn ProgressSink,
) -> CoreResult<BuiltCard> {
    if dest.exists() {
        return Err(CoreError::SafetyRefused(format!(
            "'{}' already exists — ART will not build over a card that is already there",
            dest.display()
        )));
    }

    let shares: Vec<u64> = spec.areas.iter().map(|area| area.size_bytes).collect();
    let layout = plan_card(spec.total_bytes, spec.boot_bytes, &shares)?;

    // Steps, for the bar: the table, the boot partition, then one per area,
    // then reading it back.
    let steps = (spec.areas.len() + 3) as u64;
    let mut done = 0u64;
    let mut step = |message: &str| {
        progress.report(done, Some(steps), message);
        done += 1;
    };

    step("Creating the image");
    let mut file = std::fs::File::create_new(dest)?;
    file.set_len(layout.total_sectors * SECTOR_BYTES)?;

    step("Writing the partition table");
    file.write_all(&write_mbr(&layout))?;

    step("Creating the boot partition");
    create_boot_partition(
        &mut file,
        layout.boot.start_bytes(),
        layout.boot.length_bytes(),
        &spec.label,
        &spec.boot_files,
    )?;

    for (index, area) in spec.areas.iter().enumerate() {
        step(&format!("Preparing Amiga disk {}", index + 1));
        if progress.is_cancelled() {
            // Between whole units of work (§54). The half-built file is ART's
            // own — it did not exist a moment ago — so it goes with the
            // cancellation rather than being left as a card-shaped thing
            // somebody might later try to boot.
            drop(file);
            let _ = std::fs::remove_file(dest);
            return Err(CoreError::Cancelled);
        }

        let placed = &layout.areas[index];
        let rdb = create_rdb_layout(placed.length_bytes(), &area.partitions, &area.file_systems)?;

        // The one thing this module exists to get right: at the area's own
        // offset, with block numbers that are relative to it.
        file.seek(SeekFrom::Start(placed.start_bytes()))?;
        file.write_all(&rdb.blocks)?;
    }

    file.sync_all()?;
    drop(file);

    step("Checking what was built");
    let verified = read_card(dest)?;
    if verified.areas.len() != spec.areas.len() {
        return Err(CoreError::Malformed {
            format: "card".into(),
            detail: format!(
                "the card was written with {} Amiga disks and reads back with {}",
                spec.areas.len(),
                verified.areas.len()
            ),
        });
    }

    Ok(BuiltCard { layout, verified })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::jobs::NoProgress;
    use crate::core::rdb::AmigaHardDiskFs;

    const GIB: u64 = 1024 * 1024 * 1024;

    /// The smallest card these tests can use: the boot partition is 1.10 GiB
    /// and an Amiga disk has to fit after it. Deliberately small — every one
    /// of these builds a sparse file, and a test suite that reserves tens of
    /// gigabytes on somebody's system drive to prove a partition table is a
    /// test suite nobody runs twice.
    const SMALLEST: u64 = 2 * GIB;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("art-cardbuild-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn work_partition(name: &str, size_mb: u32) -> PartitionSpec {
        PartitionSpec {
            drive_name: name.into(),
            fs_type: AmigaHardDiskFs::FfsStandard,
            size_mb,
            bootable: true,
            boot_priority: 0,
            num_buffers: 0,
        }
    }

    /// The whole of G2 in one test: a card comes out of ART with a partition
    /// table, a bootable FAT32 partition holding the files it was given, and an
    /// RDB at the start of every Amiga disk — and ART's own card reader, the
    /// one that opens CaffeineOS and MultibootOS, reads it back.
    #[test]
    fn a_built_card_reads_back_as_a_card() {
        let dir = scratch("whole");
        let dest = dir.join("card.img");

        let built = build_card(
            &dest,
            &CardSpec {
                total_bytes: SMALLEST,
                boot_bytes: 0,
                label: "ART CARD".into(),
                boot_files: vec![
                    BootFile {
                        name: "config.txt".into(),
                        bytes: b"initramfs kick.rom\n".to_vec(),
                    },
                    BootFile {
                        name: "cmdline.txt".into(),
                        bytes: b"sd.unit0=ro\n".to_vec(),
                    },
                ],
                areas: vec![AreaSpec {
                    size_bytes: 0,
                    partitions: vec![work_partition("SDH0", 512)],
                    file_systems: Vec::new(),
                }],
            },
            &NoProgress,
        )
        .unwrap();

        // The table.
        assert_eq!(built.layout.areas.len(), 1);
        assert_eq!(built.layout.boot.start_bytes(), 1_048_576);
        // The same offset both real cards put their first Amiga disk at.
        assert_eq!(built.layout.areas[0].start_bytes(), 1_178_599_424);

        // Read back by the reader that opens real cards.
        assert!(built.verified.mbr.is_some(), "it is a card, not an HDF");
        assert_eq!(built.verified.areas.len(), 1);
        assert_eq!(built.verified.areas[0].offset_bytes, 1_178_599_424);
        assert_eq!(built.verified.areas[0].rdb.partitions.len(), 1);
        assert_eq!(built.verified.areas[0].rdb.partitions[0].drive_name, "SDH0");
        assert!(
            built.verified.partitions_missing_driver().is_empty(),
            "FFS is Kickstart's own and needs no driver in the RDB"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Two Amiga disks, each with its own RDB at its own offset. This is the
    /// shape MultibootOS has, and the shape that made ART-095 a bug: a reader
    /// looking at byte zero finds the partition table, not an Amiga disk.
    #[test]
    fn every_amiga_disk_gets_its_own_table_at_its_own_offset() {
        let dir = scratch("two-areas");
        let dest = dir.join("card.img");

        let built = build_card(
            &dest,
            &CardSpec::new(
                4 * GIB,
                vec![
                    AreaSpec {
                        size_bytes: GIB,
                        partitions: vec![work_partition("SDH0", 512)],
                        file_systems: Vec::new(),
                    },
                    AreaSpec {
                        size_bytes: 0,
                        partitions: vec![work_partition("ADH0", 512)],
                        file_systems: Vec::new(),
                    },
                ],
            ),
            &NoProgress,
        )
        .unwrap();

        assert_eq!(built.verified.areas.len(), 2);
        assert!(
            built.verified.areas[1].offset_bytes > built.verified.areas[0].offset_bytes,
            "the second disk starts after the first"
        );
        assert_eq!(built.verified.areas[0].rdb.partitions[0].drive_name, "SDH0");
        assert_eq!(built.verified.areas[1].rdb.partitions[0].drive_name, "ADH0");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The FAT32 side survives the RDBs being written after it — the areas are
    /// past its end, and `Region` is what makes that true rather than luck.
    #[test]
    fn the_boot_partition_still_reads_after_the_amiga_disks_are_written() {
        let dir = scratch("boot-survives");
        let dest = dir.join("card.img");

        build_card(
            &dest,
            &CardSpec {
                total_bytes: SMALLEST,
                boot_bytes: 0,
                label: "ART CARD".into(),
                boot_files: vec![BootFile {
                    name: "cmdline.txt".into(),
                    bytes: b"sd.unit0=ro\n".to_vec(),
                }],
                areas: vec![AreaSpec {
                    size_bytes: 0,
                    partitions: vec![work_partition("SDH0", 512)],
                    file_systems: Vec::new(),
                }],
            },
            &NoProgress,
        )
        .unwrap();

        let file = std::fs::File::open(&dest).unwrap();
        let mut region = crate::core::fat32::Region::new(
            file,
            1_048_576,
            crate::core::mbr::DEFAULT_BOOT_SECTORS * SECTOR_BYTES,
        );
        let fs = fatfs::FileSystem::new(&mut region, fatfs::FsOptions::new()).unwrap();
        let names: Vec<String> = fs
            .root_dir()
            .iter()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert!(names.contains(&"cmdline.txt".to_string()), "{names:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Build a card from **real** material, for a human to look at.
    ///
    /// Every test above uses bytes ART made up. This one takes a folder — an
    /// unpacked `Emu68-pistorm.zip`, a Kickstart named `kick.rom`, a
    /// `config.txt` — and lays it on a real card image, because the question
    /// "will a Raspberry Pi boot this" is not one a synthetic fixture can even
    /// be asked. The same reason `export_adf_for_oracle_when_asked` exists.
    ///
    /// ```text
    /// ART_CARD_PAYLOAD=E:\amiga\ProjeART\emu68  ART_CARD_OUT=E:\amiga\ProjeART\card.img \
    ///   cargo test build_real_card_when_asked -- --nocapture
    /// ```
    ///
    /// `ART_CARD_GB` sets the size; it defaults to 2, which is the smallest
    /// card that holds the 1.10 GiB boot partition and an Amiga disk after it.
    #[test]
    fn build_real_card_when_asked() {
        let (Ok(payload), Ok(dest)) = (
            std::env::var("ART_CARD_PAYLOAD"),
            std::env::var("ART_CARD_OUT"),
        ) else {
            return;
        };
        let size_gb: u64 = std::env::var("ART_CARD_GB")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(2);

        // Every file under the payload folder, with its path relative to it —
        // `overlays/emu68.dtbo` stays `overlays/emu68.dtbo`.
        fn collect(root: &Path, at: &Path, into: &mut Vec<BootFile>) {
            for entry in std::fs::read_dir(at).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if path.is_dir() {
                    collect(root, &path, into);
                } else {
                    into.push(BootFile {
                        name: path
                            .strip_prefix(root)
                            .unwrap()
                            .to_string_lossy()
                            .replace('\\', "/"),
                        bytes: std::fs::read(&path).unwrap(),
                    });
                }
            }
        }

        let mut boot_files = Vec::new();
        collect(Path::new(&payload), Path::new(&payload), &mut boot_files);
        println!("payload: {} files", boot_files.len());
        for file in &boot_files {
            println!("  {} ({} bytes)", file.name, file.bytes.len());
        }

        let built = build_card(
            Path::new(&dest),
            &CardSpec {
                total_bytes: size_gb * GIB,
                boot_bytes: 0,
                label: "ART CARD".into(),
                boot_files,
                areas: vec![AreaSpec {
                    size_bytes: 0,
                    partitions: vec![work_partition("SDH0", 512)],
                    file_systems: Vec::new(),
                }],
            },
            &NoProgress,
        )
        .unwrap();

        println!(
            "card: {} bytes, boot at {}, {} Amiga disk(s), first at {}",
            built.layout.total_sectors * SECTOR_BYTES,
            built.layout.boot.start_bytes(),
            built.verified.areas.len(),
            built.verified.areas[0].offset_bytes
        );
        for (index, part) in built.verified.areas[0].rdb.partitions.iter().enumerate() {
            println!(
                "  partition {index}: {} {} {} bytes",
                part.drive_name, part.dostype_str, part.size_bytes
            );
        }
    }

    /// `SAFE_CREATE`: a card that is already there is not built over.
    #[test]
    fn an_existing_file_is_refused_rather_than_replaced() {
        let dir = scratch("exists");
        let dest = dir.join("card.img");
        std::fs::write(&dest, b"somebody's afternoon").unwrap();

        let err = build_card(
            &dest,
            &CardSpec::new(
                SMALLEST,
                vec![AreaSpec {
                    size_bytes: 0,
                    partitions: vec![work_partition("SDH0", 512)],
                    file_systems: Vec::new(),
                }],
            ),
            &NoProgress,
        )
        .unwrap_err();

        assert_eq!(err.code(), "ART-SAFETY-REFUSED", "{err}");
        assert_eq!(
            std::fs::read(&dest).unwrap(),
            b"somebody's afternoon",
            "and the file is exactly as it was"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A card too small is refused before anything is created, and leaves no
    /// half-built file behind.
    #[test]
    fn a_card_too_small_leaves_nothing_behind() {
        let dir = scratch("too-small");
        let dest = dir.join("card.img");

        assert!(build_card(
            &dest,
            &CardSpec::new(
                512 * 1024 * 1024,
                vec![AreaSpec {
                    size_bytes: 0,
                    partitions: vec![work_partition("SDH0", 512)],
                    file_systems: Vec::new(),
                }],
            ),
            &NoProgress,
        )
        .is_err());
        assert!(!dest.exists(), "nothing was created");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
