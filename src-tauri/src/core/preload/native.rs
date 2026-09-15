//! A [`VolumeFormatter`] that launches nothing (SD-2 · G5).
//!
//! `pfs3dev.rs` proved the composition — `ArtBlockDevice` over a
//! [`FileRegionMut`] satisfies `libpfs3`'s own `BlockDevice` — and `core/card`
//! and `core/volume/write` already exist. This is what wires all three
//! together behind [`VolumeFormatter`], so a preload no longer needs
//! `hst-imager` on the machine. See `core/preload/mod.rs` for how the two
//! implementations of that trait now relate.
//!
//! ## Two families, one trait
//!
//! [`format_partition`](VolumeFormatter::format_partition) and
//! [`copy_in`](VolumeFormatter::copy_in) both branch on the partition's
//! `DosType`: `PFS\x`/`PDS\x` goes to `libpfs3`, `DOS\0`..`DOS\7` to ART's own
//! writer (`core/volume/write`). Anything else — `SFS\0`, an unrecognised
//! type — is refused by name; `NativeFormatter` does not guess.
//!
//! `libpfs3` here is ART's vendored copy, `src-tauri/vendor/libpfs3`
//! (`0.1.3+art.9`): its format and writer differ from 0.1.3 — the super index
//! level and reserved anodes 0–4 (ART-310), one anode per allocation (ART-312),
//! directory `parent`s (ART-313), names against `fnsize` (ART-314), the data
//! bitmap's bounds (ART-315), the deldir, formatted on and written as pfs3aio
//! writes it (ART-316, ART-318), pfs3aio's anode
//! ceiling (ART-311), a caller-supplied datestamp (ART-317), and the writer
//! returning to its last commit on error, locking rather than continuing when
//! a commit itself fails part-way (ART-319), overwriting a file copy-on-write and
//! renaming over an existing entry in one commit (ART-319's gaps, 2026-09-15), and a
//! case-only rename no longer deleting the file it renames (ART-322, 2026-09-15);
//! `ART-PATCH.md` there lists each. The writer's other limits below (ART-113, ART-116)
//! still hold.
//!
//! ## Embedding a driver is not a formatter's job (ART-117)
//!
//! Until 2026-09-14 `import_filesystem` sat on this trait and refused every
//! card, because the only RDB writer ART had rebuilt a table from scratch on
//! 16/63 geometry — true of a rebuild, and not of an edit that never touches a
//! PART block or a cylinder number. Appending or replacing a driver in a card's
//! existing RDB is `core::preload::embed` now: a strict walk, block selection
//! above everything live, a user-chosen backup, four journalled stages and a
//! read-back. It launches nothing and needs no formatter.
//!
//! ## `copy_in`: progress and cancellation
//!
//! The source tree is flattened into an ordered list before anything is
//! written — parent directories always precede their children, so a
//! directory's PFS3 anode or FFS header block is known by the time its first
//! child is reached. `sink.is_cancelled()` is checked once per list entry,
//! before that entry's own work starts and after the previous entry's
//! `sink.report` call — the same shape `core/osinstall/apply.rs` already
//! uses. A cancellation is reported as a plain [`CoreError::Cancelled`],
//! matching Task 9's own test: unlike `apply`'s `CancelledPartway`, nothing
//! here needs to say how much landed, because every file and folder already
//! written is exactly what a subsequent `format_partition` will discard.
//!
//! ## A non-ASCII name is refused before `libpfs3` ever sees it (ART-113)
//!
//! `libpfs3` 0.1.3's `writer.rs` writes an entry's name with `name.as_bytes()`
//! — UTF-8, because that is what a Rust `String` holds — and `ondisk/direntry.rs`
//! reads a stored name back with `util::latin1_to_string`, which maps each
//! stored byte to one char. The two agree only in the ASCII range, where UTF-8
//! and Latin-1 happen to coincide byte for byte; outside it (`türkçe`,
//! `español`, `français`) the round trip is wrong, and it would be wrong on a
//! real Amiga too, since AmigaDOS itself reads Latin-1. This is a `libpfs3`
//! 0.1.3 limitation, not something `core/` can repair through the crate's own
//! public API — a Rust `String` cannot carry pre-encoded Latin-1 bytes — so it
//! is refused by name instead: [`non_ascii_entries`] walks the already-flattened
//! entry list before [`copy_in_pfs3`] opens the volume at all, and
//! [`CoreError::NonAsciiPfs3Names`] carries every offending path it finds
//! (bounded — see [`MAX_NAMED_NON_ASCII`]) plus a count of the rest. Remove
//! this check the day `libpfs3` accepts a pre-encoded byte string for a name
//! instead of a `String`. FFS is unaffected — ART's own writer
//! (`core/volume/write`) encodes these names correctly — so this check runs
//! only on the PFS3 branch.
//!
//! ## A comment or a date `libpfs3` cannot carry is counted, not hidden (ART-116)
//!
//! `libpfs3` 0.1.3 exposes `update_dir_entry_protection` and nothing else for
//! a directory entry — no setter for a comment or a date — so [`copy_in_pfs3`]
//! applies a sidecar's protection bits and has nowhere to put the other two.
//! The FFS branch ([`copy_in_ffs`]) keeps all three, through `FileMeta`. Since
//! there is no fix available (the same `libpfs3` limitation as above), the
//! loss is made visible instead of silent: [`CopySummary::comments_lost`] and
//! [`CopySummary::dates_lost`] count every entry whose sidecar carried a
//! non-empty comment, or a date other than [`AmigaDate::default`], that could
//! not be written — the same "is this actually worth mentioning" rule
//! `core/volume/write/copy.rs::sidecar_for` already applies when it decides
//! whether a sidecar is worth writing at all. This is information for the
//! caller to report, not a refusal — G5 verified end to end on PFS3 without
//! either field, and nothing here blocks that.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::core::adf::bcpl::{write_bcpl_string, AmigaDate};
use crate::core::adf::blocks::{
    bit_position, block_subtype, block_type, BLOCKS_PER_BITMAP_BLOCK, HASH_TABLE_SIZE,
};
use crate::core::adf::bootblock::BootBlock;
use crate::core::adf::checksum::block_checksum;
use crate::core::card::{read_card, AmigaArea, CardImage};
use crate::core::clock::AmigaClock;
use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;
use crate::core::preload::amiga_names::AmigaNames;
use crate::core::preload::pfs3dev::ArtBlockDevice;
use crate::core::preload::{CopySummary, ToolVersion, VolumeFormatter};
use crate::core::rdb::ParsedPartition;
use crate::core::safety::backup::BACKUP_DIR;
use crate::core::volume::device::FileRegionMut;
use crate::core::volume::write::{dir, uaem, write_refusal, FileMeta, VolumeWriter};
use crate::core::volume::{BlockDevice, BlockDeviceMut, DosType, VolumeGeometry};

/// The version of the `libpfs3` ART builds: the vendored copy in
/// `src-tauri/vendor/libpfs3` (ART-310) — crates.io's 0.1.3 with ART's patches,
/// `+art.9`. There is no `CARGO_PKG_VERSION`-style macro for a *dependency's*
/// version, so this is kept in sync by hand, the same trade-off ART already
/// accepts for `ureq`'s exact `=3.2.1` pin (CLAUDE.md). `probe()` reports this
/// constant as which implementation did the work, and
/// `the_pinned_version_constant_matches_cargo_toml` (below) reads the pin, the
/// `[patch.crates-io]` line and the vendored manifest, so the constant cannot
/// drift from what was actually built.
const LIBPFS3_VERSION: &str = "0.1.3+art.9";

/// A [`VolumeFormatter`] backed by `libpfs3` and ART's own FFS writer.
/// Launches nothing; see the module docs for what each method actually does.
/// Every date it writes comes from `clock` (ART-317).
pub struct NativeFormatter {
    clock: &'static dyn AmigaClock,
}

impl NativeFormatter {
    pub const fn new(clock: &'static dyn AmigaClock) -> Self {
        Self { clock }
    }

    /// UTC, for tests whose subject is not the date.
    #[cfg(test)]
    pub const UTC: Self = Self::new(&crate::core::clock::UtcClock);
}

/// An [`AmigaDate`] as libpfs3's (days, minutes, ticks). PFS3 stores days as
/// a `u16`, which lasts until 2157, so a later day clamps rather than wraps.
fn pfs3_datestamp(date: AmigaDate) -> (u16, u16, u16) {
    (
        u16::try_from(date.days).unwrap_or(u16::MAX),
        date.mins as u16,
        date.ticks as u16,
    )
}

/// The one product path to a libpfs3 [`Writer`](libpfs3::writer::Writer)
/// (ART-317, third debt round's survivor (c)). A `Writer` opened without
/// calling `set_entry_date` falls back to `current_amiga_datestamp`
/// (`vendor/libpfs3/src/writer.rs`), which stamps UTC — so every writer
/// opened through this helper is stamped with `clock`'s own date before it
/// is handed back, and `core::independence::
/// libpfs3_writer_open_is_named_only_by_the_pfs3_writer_helper` keeps this
/// the only place that calls `Writer::open` outside a test. A caller that
/// writes for longer than an instant, such as `copy_in_pfs3`'s loop, still
/// refreshes the date per entry with its own `set_entry_date` call — this
/// helper only guarantees the writer is never left holding no date at all.
fn open_pfs3_writer(
    vol: libpfs3::volume::Volume,
    clock: &dyn AmigaClock,
) -> CoreResult<libpfs3::writer::Writer> {
    let mut writer = libpfs3::writer::Writer::open(vol).map_err(from_pfs3)?;
    writer.set_entry_date(Some(pfs3_datestamp(clock.amiga_now())));
    Ok(writer)
}

impl VolumeFormatter for NativeFormatter {
    fn probe(&self) -> CoreResult<ToolVersion> {
        Ok(ToolVersion {
            raw: format!("libpfs3 {LIBPFS3_VERSION} (native, no external tool)"),
        })
    }

    fn format_partition(
        &self,
        image: &Path,
        slot: Option<usize>,
        index: usize,
        volume: &str,
        sink: &dyn ProgressSink,
    ) -> CoreResult<()> {
        // The engine must not depend on a screen having asked (Task 9's brief) —
        // and hst-imager's own answer would arrive after the partition was gone.
        let checked_name = dir::check_name(volume)?;

        let card = read_card(image)?;
        let area = area_for_slot(&card, slot)?;
        let part = partition_by_index(area, index)?;
        let (offset, length, block_size) = partition_region(area, part)?;
        let dos = DosType::new(part.dostype.to_be_bytes());

        sink.report(
            0,
            Some(1),
            &format!("Formatting {} as {checked_name}", part.drive_name),
        );

        match family_of(dos) {
            DosFamily::Pfs3 => {
                let region = FileRegionMut::open(image, offset, length, block_size)?;
                let total_blocks = region.total_blocks();
                let device = ArtBlockDevice::new(region);
                let opts = libpfs3::format::FormatOptions {
                    volume_name: checked_name,
                    // ART-316/318: pfs3aio's two-block deldir, as the Amiga's own
                    // format makes it (the owner's decision, 2026-09-14).
                    enable_deldir: true,
                    datestamp: Some(pfs3_datestamp(self.clock.amiga_now())),
                };
                libpfs3::format::format_with_size(&device, total_blocks as u64, &opts)
                    .map_err(from_pfs3)?;
            }
            DosFamily::Ffs => {
                let mut region = FileRegionMut::open(image, offset, length, block_size)?;
                let total_blocks = region.total_blocks();
                let geometry = VolumeGeometry::new(block_size, total_blocks, part.reserved, dos)?;
                if let Some(reason) = write_refusal(&geometry) {
                    return Err(CoreError::UnsupportedFormat(reason));
                }
                format_ffs_volume(
                    &mut region,
                    &geometry,
                    &checked_name,
                    self.clock.amiga_now(),
                )?;
            }
            DosFamily::Other => {
                return Err(CoreError::UnsupportedFormat(format!(
                    "{} is not a filesystem NativeFormatter can format",
                    dos.label()
                )));
            }
        }

        sink.report(1, Some(1), "done");
        Ok(())
    }

    /// **ART-122.** Everything `copy_in` decides before it opens the volume,
    /// and nothing after it. The two share [`plan_copy`] so the answer cannot
    /// drift from what the copy itself would do — a `can_copy_in` that said
    /// yes to a copy `copy_in` then refuses would be worse than not asking,
    /// because the partition is formatted in between.
    fn can_copy_in(
        &self,
        image: &Path,
        slot: Option<usize>,
        drive: &str,
        source: &Path,
    ) -> CoreResult<()> {
        let planned = plan_copy(image, slot, drive, source)?;
        match family_of(planned.dos) {
            DosFamily::Pfs3 => match non_ascii_refusal(&planned.entries).or_else(|| {
                // ART-314: against the volume `format_partition` is about to write.
                long_name_refusal(
                    &planned.entries,
                    pfs3_name_limit(libpfs3::format::FORMAT_FNSIZE),
                )
            }) {
                Some(err) => Err(err),
                None => Ok(()),
            },
            DosFamily::Ffs => Ok(()),
            DosFamily::Other => Err(unsupported_family(planned.dos)),
        }
    }

    fn copy_in(
        &self,
        image: &Path,
        slot: Option<usize>,
        drive: &str,
        source: &Path,
        sink: &dyn ProgressSink,
    ) -> CoreResult<CopySummary> {
        let PlannedCopy {
            offset,
            length,
            block_size,
            dos,
            reserved,
            entries,
        } = plan_copy(image, slot, drive, source)?;

        match family_of(dos) {
            DosFamily::Pfs3 => copy_in_pfs3(
                image, offset, length, block_size, drive, source, &entries, sink, self.clock,
            ),
            DosFamily::Ffs => copy_in_ffs(
                image, offset, length, block_size, dos, reserved, drive, source, &entries, sink,
                self.clock,
            ),
            DosFamily::Other => Err(unsupported_family(dos)),
        }
    }
}

/// What both [`NativeFormatter::can_copy_in`] and
/// [`NativeFormatter::copy_in`] work out before either writes or refuses.
struct PlannedCopy {
    offset: u64,
    length: u64,
    block_size: usize,
    dos: DosType,
    /// The partition's own reserved-block count, which only the FFS branch
    /// needs — carried here so `copy_in` does not read the card a second time
    /// to fetch one field.
    reserved: u32,
    entries: Vec<CopyEntry>,
}

fn plan_copy(
    image: &Path,
    slot: Option<usize>,
    drive: &str,
    source: &Path,
) -> CoreResult<PlannedCopy> {
    if !source.is_dir() {
        return Err(CoreError::InvalidInput(format!(
            "'{}' is not a folder",
            source.display()
        )));
    }

    let card = read_card(image)?;
    let area = area_for_slot(&card, slot)?;
    let part = partition_by_drive(area, drive)?;
    let (offset, length, block_size) = partition_region(area, part)?;

    Ok(PlannedCopy {
        offset,
        length,
        block_size,
        dos: DosType::new(part.dostype.to_be_bytes()),
        reserved: part.reserved,
        entries: collect_entries(source)?,
    })
}

fn unsupported_family(dos: DosType) -> CoreError {
    CoreError::UnsupportedFormat(format!(
        "{} is not a filesystem NativeFormatter can copy into",
        dos.label()
    ))
}

// ---------------------------------------------------------------------------
// Which implementation a partition's DosType wants
// ---------------------------------------------------------------------------

/// `pub(crate)`: [`crate::core::osinstall::verify`] routes on the same
/// families this module writes to, and duplicating the `DosType` decision
/// there would be a second place ART-043-style offset arithmetic could drift
/// from this one.
pub(crate) enum DosFamily {
    Pfs3,
    Ffs,
    Other,
}

pub(crate) fn family_of(dos: DosType) -> DosFamily {
    if dos.family() == b"PFS" || dos.family() == b"PDS" {
        DosFamily::Pfs3
    } else if dos.is_dos() && dos.flavour() <= 7 {
        DosFamily::Ffs
    } else {
        DosFamily::Other
    }
}

pub(crate) fn from_pfs3(err: libpfs3::error::Error) -> CoreError {
    match err {
        libpfs3::error::Error::DiskFull(detail) => {
            CoreError::InvalidInput(format!("not enough room on this PFS3 volume: {detail}"))
        }
        libpfs3::error::Error::Io(io) => CoreError::Io(io),
        libpfs3::error::Error::NameTooLong { name, max, .. } => CoreError::Pfs3NamesTooLong {
            paths: vec![name],
            more: 0,
            max_bytes: max,
        },
        libpfs3::error::Error::CommitFailed => CoreError::Pfs3WriterLocked,
        other => CoreError::Malformed {
            format: "pfs3".into(),
            detail: other.to_string(),
        },
    }
}

/// `uaem`'s sidecars carry `HSPARWED` as a `u32` (`core/volume/write/uaem.rs`);
/// `libpfs3` stores the same eight bits, in the same order, as a `u8` — the
/// two encodings agree bit for bit with `libpfs3::util::amiga_protection_string`,
/// which inverts `RWED` exactly the way `uaem::format_bits` does. So the
/// narrowing is a truncation, and a **checked** one: anything set above bit 7
/// means the sidecar is not describing what this code thinks it is, and the
/// safe direction is to refuse rather than silently drop those bits.
pub(crate) fn pfs3_protection(protection: u32) -> CoreResult<u8> {
    u8::try_from(protection).map_err(|_| CoreError::Malformed {
        format: "uaem".into(),
        detail: format!(
            "protection bits {protection:#x} do not fit the byte AmigaDOS actually stores"
        ),
    })
}

// ---------------------------------------------------------------------------
// Finding a partition on a card
// ---------------------------------------------------------------------------

/// Which Amiga area a `slot` names. `None` is the plain-HDF case: one area at
/// offset zero, the same convention `AmigaArea::mbr_slot` (`core/card/mod.rs`)
/// carries in the other direction (ART-321).
pub(crate) fn area_for_slot(card: &CardImage, slot: Option<usize>) -> CoreResult<&AmigaArea> {
    match slot {
        None => card.areas.first().ok_or_else(|| CoreError::Malformed {
            format: "card".into(),
            detail: "this image has no Amiga disk on it".into(),
        }),
        Some(n) => {
            let mbr = card.mbr.as_ref().ok_or_else(|| {
                CoreError::InvalidInput(format!(
                    "slot {n} was given but this image has no partition table"
                ))
            })?;
            let entry = mbr
                .amiga_areas()
                .into_iter()
                .find(|p| p.slot_number() == n)
                .ok_or_else(|| {
                    CoreError::InvalidInput(format!("this card has no Amiga disk in slot {n}"))
                })?;
            card.areas
                .iter()
                .find(|a| a.offset_bytes == entry.start_bytes())
                .ok_or_else(|| CoreError::Malformed {
                    format: "card".into(),
                    detail: format!("slot {n} is in the partition table but its RDB did not read"),
                })
        }
    }
}

pub(crate) fn partition_by_index(area: &AmigaArea, index: usize) -> CoreResult<&ParsedPartition> {
    index
        .checked_sub(1)
        .and_then(|i| area.rdb.partitions.get(i))
        .ok_or_else(|| {
            CoreError::InvalidInput(format!(
                "this disk has {} partition(s); there is no partition {index}",
                area.rdb.partitions.len()
            ))
        })
}

fn partition_by_drive<'a>(area: &'a AmigaArea, drive: &str) -> CoreResult<&'a ParsedPartition> {
    area.rdb
        .partitions
        .iter()
        .find(|p| p.drive_name.eq_ignore_ascii_case(drive))
        .ok_or_else(|| {
            CoreError::InvalidInput(format!("this disk has no partition named '{drive}'"))
        })
}

/// The partition's file-absolute byte offset, length and block size — never
/// the partition's own, volume-relative numbers on their own (ART-043's
/// mistake, from the reading side).
pub(crate) fn partition_region(
    area: &AmigaArea,
    part: &ParsedPartition,
) -> CoreResult<(u64, u64, usize)> {
    let rel_offset = part.byte_offset().ok_or_else(|| CoreError::Malformed {
        format: "card".into(),
        detail: format!(
            "'{}' has geometry ART cannot compute an offset from",
            part.drive_name
        ),
    })?;
    let length = part.byte_length().ok_or_else(|| CoreError::Malformed {
        format: "card".into(),
        detail: format!(
            "'{}' has geometry ART cannot compute a length from",
            part.drive_name
        ),
    })?;
    let offset = area
        .offset_bytes
        .checked_add(rel_offset)
        .ok_or_else(|| CoreError::Malformed {
            format: "card".into(),
            detail: "partition offset overflows".into(),
        })?;
    let block_size = usize::try_from(part.block_bytes()).map_err(|_| CoreError::Malformed {
        format: "card".into(),
        detail: "block size does not fit".into(),
    })?;
    Ok((offset, length, block_size))
}

// ---------------------------------------------------------------------------
// Formatting a blank FFS/OFS partition — ART's own writer
// ---------------------------------------------------------------------------

/// Writes the smallest set of blocks that makes an unformatted partition read
/// as an **empty** AmigaDOS volume: the boot block, the root block, and its
/// bitmap block(s) and extension chain. Everything past that structured
/// region is left exactly as it was — an unallocated data block only has to
/// be marked free, not zeroed, which is the same thing a real AmigaDOS Format
/// does.
///
/// The layout mirrors `core/adf/create.rs::create_blank_adf` (fixed at floppy
/// geometry) and `core/volume/fixture.rs::make_ffs_volume` (test-only,
/// building a whole in-memory image). This is the one that writes through a
/// [`BlockDeviceMut`] at whatever geometry the partition actually has,
/// without ever materialising the volume in memory — the same reason
/// `create_rdb_layout` returns only its leading blocks rather than a whole
/// image.
///
/// ## No journal underneath this either
///
/// These are raw [`BlockDeviceMut::write_block`] calls, not
/// `core/volume/journal.rs::Journalled` writes — the same choice
/// `pfs3dev.rs`'s module doc already makes for PFS3, and for the same
/// reason: a format's contents are forfeit the moment the user's confirmed
/// choice runs it, so there is nothing here worth journalling.
///
/// What an interrupted format leaves behind is worth being specific about,
/// because `VolumeWriter::open`'s ART-049 check only compares the
/// bootblock's four-byte signature against the geometry it was opened
/// with — and this function writes that signature **first**. An I/O failure
/// between the boot block write and the root block write leaves a bootblock
/// that already claims the new `DosType`, sitting over whatever the root
/// block held before:
///
/// - On a partition that was never formatted, that is a block of zeros.
///   `VolumeWriter::open` accepts it (the signature matches), but every
///   subsequent read of block 0 refuses by name — `dir::is_directory` sees a
///   block that is not a `T_HEADER`, not silently an empty directory.
/// - On a partition being *re*formatted, the previous filesystem's root
///   block may still be there, structurally valid. `copy_in`'s
///   already-populated refusal then catches it and refuses to fill what
///   still looks, correctly, like somebody else's volume.
///
/// Neither outcome is data loss beyond what the reformat already asked for,
/// and neither is corruption with no reason given. G5 always formats before
/// it fills, so an interrupted format is simply reformatted from scratch on
/// the next attempt — exactly as an interrupted PFS3 format already is.
fn format_ffs_volume(
    device: &mut dyn BlockDeviceMut,
    geometry: &VolumeGeometry,
    volume_name: &str,
    now: AmigaDate,
) -> CoreResult<()> {
    let bs = geometry.block_size;
    let total_blocks = geometry.total_blocks;
    let reserved = geometry.reserved;
    let root_block = geometry.root_block;

    // ---- boot block: blocks 0 and 1, 1024 bytes together ----
    let mut boot = vec![0u8; bs * 2];
    boot[0..4].copy_from_slice(&geometry.dos_type.0);
    let boot_cks = BootBlock::compute_checksum(&boot);
    boot[4..8].copy_from_slice(&boot_cks.to_be_bytes());
    device.write_block(0, &boot[..bs])?;
    device.write_block(1, &boot[bs..bs * 2])?;

    // ---- where the bitmap and its extension chain land ----
    const MAX_BM_PAGES: usize = 25;
    let pages_per_extension = (bs / 4).saturating_sub(1).max(1);

    let described = total_blocks.saturating_sub(reserved) as usize;
    let bitmap_count = described.div_ceil(BLOCKS_PER_BITMAP_BLOCK).max(1);
    let bitmap_blocks: Vec<u32> = (0..bitmap_count as u32)
        .map(|i| root_block + 1 + i)
        .collect();

    let overflow = bitmap_blocks.len().saturating_sub(MAX_BM_PAGES);
    let ext_count = overflow.div_ceil(pages_per_extension);
    let ext_blocks: Vec<u32> = (0..ext_count as u32)
        .map(|i| root_block + 1 + bitmap_count as u32 + i)
        .collect();

    let highest = ext_blocks
        .last()
        .or(bitmap_blocks.last())
        .copied()
        .unwrap_or(root_block);
    if highest >= total_blocks {
        return Err(CoreError::InvalidInput(format!(
            "'{volume_name}' has no room for its own bitmap: this volume is {total_blocks} \
             blocks and formatting it needs at least {}",
            highest + 1
        )));
    }

    // ---- root block ----
    let mut root = vec![0u8; bs];
    put_i32(&mut root, 0, block_type::HEADER);
    put_u32(&mut root, 12, HASH_TABLE_SIZE as u32);
    // bm_flag: -1 means the bitmap is valid. Left at 0 the volume reads as
    // "needs validating", which real tools refuse to trust.
    put_i32(&mut root, 312, -1);
    for (i, blk) in bitmap_blocks.iter().enumerate().take(MAX_BM_PAGES) {
        put_u32(&mut root, 316 + i * 4, *blk);
    }
    if let Some(first_ext) = ext_blocks.first() {
        put_u32(&mut root, 416, *first_ext);
    }
    put_u32(&mut root, 420, now.days);
    put_u32(&mut root, 424, now.mins);
    put_u32(&mut root, 428, now.ticks);
    write_bcpl_string(&mut root, 432, volume_name, 32);
    put_u32(&mut root, 472, now.days);
    put_u32(&mut root, 476, now.mins);
    put_u32(&mut root, 480, now.ticks);
    put_i32(&mut root, 508, block_subtype::ROOT);
    let root_cks = block_checksum(&root, 20);
    root[20..24].copy_from_slice(&root_cks.to_be_bytes());
    device.write_block(root_block, &root)?;

    // ---- bitmap blocks ----
    let used: Vec<u32> = std::iter::once(root_block)
        .chain(bitmap_blocks.iter().copied())
        .chain(ext_blocks.iter().copied())
        .collect();

    for (index, &blk) in bitmap_blocks.iter().enumerate() {
        let mut bm = vec![0u8; bs];
        for byte in bm[4..].iter_mut() {
            *byte = 0xFF; // everything free, until marked used below
        }
        for &u in &used {
            if let Some(pos) = bit_position(u, reserved, total_blocks) {
                if pos.bitmap_index == index {
                    let lw = u32::from_be_bytes(
                        bm[pos.byte_offset..pos.byte_offset + 4].try_into().unwrap(),
                    );
                    let lw = lw & !pos.mask;
                    bm[pos.byte_offset..pos.byte_offset + 4].copy_from_slice(&lw.to_be_bytes());
                }
            }
        }
        let bm_cks = block_checksum(&bm, 0);
        bm[0..4].copy_from_slice(&bm_cks.to_be_bytes());
        device.write_block(blk, &bm)?;
    }

    // ---- bitmap extension blocks: no checksum of their own ----
    for (index, &ext) in ext_blocks.iter().enumerate() {
        let mut data = vec![0u8; bs];
        let start = MAX_BM_PAGES + index * pages_per_extension;
        for slot in 0..pages_per_extension {
            match bitmap_blocks.get(start + slot) {
                Some(page) => put_u32(&mut data, slot * 4, *page),
                None => break,
            }
        }
        let next = ext_blocks.get(index + 1).copied().unwrap_or(0);
        put_u32(&mut data, bs - 4, next);
        device.write_block(ext, &data)?;
    }

    Ok(())
}

fn put_u32(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

fn put_i32(buf: &mut [u8], offset: usize, value: i32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

// ---------------------------------------------------------------------------
// Walking the source tree
// ---------------------------------------------------------------------------

/// One thing to create on the volume. `relative` uses `/` throughout, the way
/// both `libpfs3`'s path API and AmigaDOS itself do — never a host separator.
struct CopyEntry {
    relative: String,
    host_path: PathBuf,
    is_dir: bool,
    /// File size in bytes. `0`, and unused, for a directory.
    size: u64,
}

fn parent_key(relative: &str) -> &str {
    relative.rsplit_once('/').map_or("", |(parent, _)| parent)
}

fn leaf_name(relative: &str) -> &str {
    relative.rsplit_once('/').map_or(relative, |(_, name)| name)
}

/// How many offending paths [`CoreError::NonAsciiPfs3Names`] names directly
/// before folding the rest into a count — enough to be useful on a real
/// install (ART-113 found 24 non-ASCII directories on one real tree), never
/// enough to make the refusal itself as large as the tree it is refusing.
const MAX_NAMED_NON_ASCII: usize = 20;

/// Every entry in `entries` — file or directory — whose own name (the final
/// path segment, exactly what `create_dir_in`/`write_file_in` are handed) is
/// not pure ASCII. See the module doc comment's "ART-113" section for why
/// that name cannot round-trip through `libpfs3` 0.1.3. Directories are
/// checked too, not only files: a directory's own name goes through the
/// identical `name.as_bytes()` write path, so `español` the *drawer* is just
/// as broken as `español` the file, even if every file inside it happens to
/// be pure ASCII.
fn non_ascii_entries(entries: &[CopyEntry]) -> Vec<&str> {
    entries
        .iter()
        .filter(|entry| !leaf_name(&entry.relative).is_ascii())
        .map(|entry| entry.relative.as_str())
        .collect()
}

/// The ART-113 refusal itself, or `None` when there is nothing to refuse.
///
/// One place, because two callers now need the same answer: `copy_in_pfs3`
/// refuses with it before opening the volume, and
/// [`NativeFormatter::can_copy_in`] answers with it before the partition is
/// formatted at all (ART-122). Two copies of this decision would be two
/// chances for the pre-flight to say yes where the copy says no — with a
/// destructive format in between.
fn non_ascii_refusal(entries: &[CopyEntry]) -> Option<CoreError> {
    let offending = non_ascii_entries(entries);
    if offending.is_empty() {
        return None;
    }
    let more = offending.len().saturating_sub(MAX_NAMED_NON_ASCII);
    Some(CoreError::NonAsciiPfs3Names {
        paths: offending
            .into_iter()
            .take(MAX_NAMED_NON_ASCII)
            .map(str::to_string)
            .collect(),
        more,
    })
}

/// ART-314: the longest name a PFS3 volume with this `fnsize` can store and
/// find again — M6 (final review): the one rule lives in
/// `libpfs3::format::pfs3_name_limit`, which the vendored writer's own
/// `check_name_len` calls too, so there is one rule instead of two copies.
fn pfs3_name_limit(fnsize: u16) -> usize {
    libpfs3::format::pfs3_name_limit(fnsize)
}

/// The ART-314 refusal, or `None`: every entry whose own name is longer than
/// `max_bytes`, bounded the way [`non_ascii_refusal`] bounds its list. Bytes
/// equal characters here, because non-ASCII names are refused before this.
fn long_name_refusal(entries: &[CopyEntry], max_bytes: usize) -> Option<CoreError> {
    let offending: Vec<&str> = entries
        .iter()
        .filter(|entry| leaf_name(&entry.relative).len() > max_bytes)
        .map(|entry| entry.relative.as_str())
        .collect();
    if offending.is_empty() {
        return None;
    }
    let more = offending.len().saturating_sub(MAX_NAMED_NON_ASCII);
    Some(CoreError::Pfs3NamesTooLong {
        paths: offending
            .into_iter()
            .take(MAX_NAMED_NON_ASCII)
            .map(str::to_string)
            .collect(),
        more,
        max_bytes,
    })
}

/// Flatten `source` into an ordered list: a directory always appears before
/// anything inside it, which is what lets `copy_in` look its parent's anode
/// or header block up in a map built as it goes, rather than re-walking the
/// volume for every entry.
///
/// A `.uaem` sidecar is never included as an entry of its own (§ binding
/// requirement 1) — it is read later, beside the file it describes.
///
/// `CopyEntry::relative` is the **AmigaDOS** path, which is the host path for
/// every folder a user assembled and for almost every file of a distribution
/// tree. The exceptions are the names a Windows filesystem will not carry —
/// `Storage/DOSDrivers/AUX` off the owner's real AmigaOS 3.9 disc is stored
/// as `_AUX` — and [`AmigaNames`] reads the tree's own `distribution.json` to
/// put those back (ART-160). A folder with no manifest renames nothing, so
/// this costs one failed `read_to_string` per copy and changes nothing else.
fn collect_entries(source: &Path) -> CoreResult<Vec<CopyEntry>> {
    let mut out = Vec::new();
    let names = AmigaNames::read(source);
    collect_into(source, "", "", &names, &mut out)?;
    Ok(out)
}

fn collect_into(
    dir: &Path,
    host_prefix: &str,
    prefix: &str,
    names: &AmigaNames,
    out: &mut Vec<CopyEntry>,
) -> CoreResult<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<std::io::Result<_>>()?;
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case(uaem::UAEM_EXTENSION))
        {
            continue;
        }

        let host_name = entry
            .file_name()
            .to_str()
            .ok_or(CoreError::NonUtf8Path)?
            .to_string();

        // C6 (final whole-branch review): `.art-backup` is ART's own
        // host-side safety net (`core::safety::backup`), never Amiga
        // content — it must not reach the card any more than a `.uaem`
        // sidecar does. Icon writes no longer create these at all
        // (`core::appearance` uses `BackupPolicy::NONE` for them, C6's other
        // half), but a wallpaper or shell-defaults write still can, and
        // excluding it here means neither route can leak one onto a real
        // PiStorm card regardless of which produced it.
        if host_name == BACKUP_DIR {
            continue;
        }
        let host_relative = if host_prefix.is_empty() {
            host_name.clone()
        } else {
            format!("{host_prefix}/{host_name}")
        };
        // The Amiga name, which is the host name unless the tree recorded
        // that it had to store this node under a different one.
        let name = names.name_for(&host_relative).unwrap_or(&host_name);
        let relative = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}/{name}")
        };

        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            out.push(CopyEntry {
                relative: relative.clone(),
                host_path: path.clone(),
                is_dir: true,
                size: 0,
            });
            collect_into(&path, &host_relative, &relative, names, out)?;
        } else if file_type.is_file() {
            let size = entry.metadata()?.len();
            out.push(CopyEntry {
                relative,
                host_path: path,
                is_dir: false,
                size,
            });
        }
        // Anything else (a symlink, a device node) is not something a real
        // Amiga install tree contains; it is silently skipped rather than
        // refused, the same way a stray `.uaem` orphan is.
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Copying into a PFS3 volume
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn copy_in_pfs3(
    image: &Path,
    offset: u64,
    length: u64,
    block_size: usize,
    volume_label: &str,
    source: &Path,
    entries: &[CopyEntry],
    sink: &dyn ProgressSink,
    clock: &'static dyn AmigaClock,
) -> CoreResult<CopySummary> {
    // ART-113: refused before the volume is even opened, let alone written
    // to — see the module doc comment and `non_ascii_entries`'s own.
    if let Some(refusal) = non_ascii_refusal(entries) {
        return Err(refusal);
    }

    let region = FileRegionMut::open(image, offset, length, block_size)?;
    let device = ArtBlockDevice::new(region);
    let mut vol = libpfs3::volume::Volume::from_device(Box::new(device)).map_err(from_pfs3)?;

    // Binding requirement 4: PFS3 has no journal, so filling an already
    // populated volume without formatting it first has no crash safety.
    // Format-then-fill is only safe because the user's confirmed choice to
    // format already forfeits whatever was there.
    let existing = vol.list_dir("").map_err(from_pfs3)?;
    if !existing.is_empty() {
        return Err(CoreError::SafetyRefused(format!(
            "'{volume_label}' already has files on it. NativeFormatter only fills a volume \
             immediately after formatting it — format '{volume_label}' first."
        )));
    }

    // ART-314: by name, against what this volume says it can store, before the
    // writer opens — nothing has been written yet.
    if let Some(refusal) = long_name_refusal(entries, pfs3_name_limit(vol.fnsize())) {
        return Err(refusal);
    }

    // Binding requirement 5: the fit check, before the first byte, with real
    // numbers — and a real bound, not a byte sum that fails open. PFS3 draws
    // file data from `blocksfree` in whole blocks (`write_file_in_no_commit`'s
    // own `div_ceil(bs).max(1)`, matched exactly here), so a raw byte total
    // under-counts every file that does not end on a block boundary — the
    // gap this check used to have. Directory and file *entries* — anodes,
    // dir blocks — are a **separate** pool (`reserved_free`, not
    // `blocksfree`), so they cannot be folded into the same byte comparison;
    // checked on their own instead, one reserved block per entry, which is
    // never less than PFS3 actually needs (most entries reuse an
    // already-allocated anode block and consume none).
    let bs = u64::from(vol.block_size());
    let data_blocks_needed: u64 = entries
        .iter()
        .filter(|e| !e.is_dir)
        .map(|e| e.size.div_ceil(bs).max(1))
        .sum();
    let data_bytes_needed = data_blocks_needed * bs;
    let free_bytes = u64::from(vol.free_blocks()) * bs;
    if data_bytes_needed > free_bytes {
        return Err(CoreError::InvalidInput(format!(
            "'{}' needs {data_bytes_needed} bytes but '{volume_label}' only has {free_bytes} \
             bytes free",
            source.display()
        )));
    }
    let reserved_needed = entries.len() as u64;
    let reserved_free = u64::from(vol.rootblock.reserved_free);
    if reserved_needed > reserved_free {
        return Err(CoreError::InvalidInput(format!(
            "'{}' needs room for {reserved_needed} new file(s) and folder(s), but \
             '{volume_label}' only has {reserved_free} reserved block(s) free",
            source.display()
        )));
    }

    let mut writer = open_pfs3_writer(vol, clock)?;
    let mut summary = CopySummary::default();
    let mut anode_of: HashMap<String, u32> = HashMap::new();
    anode_of.insert(String::new(), libpfs3::ondisk::ANODE_ROOTDIR);

    let total = entries.len() as u64;
    for (done, entry) in entries.iter().enumerate() {
        writer.set_entry_date(Some(pfs3_datestamp(clock.amiga_now())));
        // Between whole files, never mid-write (§54; module docs above).
        if sink.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        sink.report(done as u64, Some(total), &entry.relative);

        let parent =
            *anode_of
                .get(parent_key(&entry.relative))
                .ok_or_else(|| CoreError::Malformed {
                    format: "volume".into(),
                    detail: format!("'{}' has no parent directory tracked", entry.relative),
                })?;
        let name = leaf_name(&entry.relative);

        if entry.is_dir {
            writer.create_dir_in(parent, name).map_err(from_pfs3)?;
            let siblings = writer.vol.list_dir_by_anode(parent).map_err(from_pfs3)?;
            let anode = siblings
                .iter()
                .find(|e| e.name.eq_ignore_ascii_case(name))
                .map(|e| e.anode)
                .ok_or_else(|| CoreError::Malformed {
                    format: "volume".into(),
                    detail: format!("'{}' was created and is not listed back", entry.relative),
                })?;
            anode_of.insert(entry.relative.clone(), anode);
            summary.directories += 1;
        } else {
            let data = std::fs::read(&entry.host_path)?;
            writer
                .write_file_in(parent, name, &data)
                .map_err(from_pfs3)?;
            summary.files += 1;
            summary.bytes = summary.bytes.map(|n| n + data.len() as u64);
        }

        // Binding requirement 1: apply the sidecar, never copy it as a file
        // (already excluded in `collect_entries`) — for a directory's own
        // sidecar exactly as for a file's; `update_dir_entry_protection`
        // patches the named entry in its parent's listing and does not care
        // which kind of entry that is.
        let sidecar = uaem::sidecar_path(&entry.host_path);
        if sidecar.is_file() {
            let text = std::fs::read_to_string(&sidecar)?;
            let parsed = uaem::parse(&text)?;
            let protection = pfs3_protection(parsed.protection)?;
            // libpfs3 0.1.3 exposes no way to set a directory entry's date or
            // its comment — only `update_dir_entry_protection`. Both are read
            // here and then dropped; FFS's `FileMeta` below carries both,
            // because ART's own writer does have those setters. ART-116:
            // counted rather than silently lost, using the same "is this
            // actually worth mentioning" rule `sidecar_for` already applies
            // when deciding whether a sidecar is worth writing at all — an
            // empty comment or the Amiga epoch itself as the date is nothing
            // this copy could have carried over anyway.
            if !parsed.comment.is_empty() {
                summary.comments_lost += 1;
            }
            if parsed.date != AmigaDate::default() {
                summary.dates_lost += 1;
            }
            writer
                .update_dir_entry_protection(parent, name, protection)
                .map_err(from_pfs3)?;
        }
    }

    Ok(summary)
}

// ---------------------------------------------------------------------------
// Copying into an FFS/OFS volume
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn copy_in_ffs(
    image: &Path,
    offset: u64,
    length: u64,
    block_size: usize,
    dos: DosType,
    reserved: u32,
    volume_label: &str,
    source: &Path,
    entries: &[CopyEntry],
    sink: &dyn ProgressSink,
    clock: &'static dyn AmigaClock,
) -> CoreResult<CopySummary> {
    let mut region = FileRegionMut::open(image, offset, length, block_size)?;
    let total_blocks = region.total_blocks();
    let geometry = VolumeGeometry::new(block_size, total_blocks, reserved, dos)?;

    // `VolumeWriter::open_with_clock` requires an already-formatted, matching
    // bootblock — exactly the state `format_partition` leaves the volume in,
    // and exactly why `copy_in` never precedes `format_partition` in the plan
    // (mod.rs).
    let mut writer = VolumeWriter::open_with_clock(&mut region, geometry, image, offset, clock)?;

    let existing = writer.list(0)?;
    if !existing.is_empty() {
        return Err(CoreError::SafetyRefused(format!(
            "'{volume_label}' already has files on it. NativeFormatter only fills a volume \
             immediately after formatting it — format '{volume_label}' first."
        )));
    }

    // Binding requirement 5: the real bound, not a raw byte sum. FFS keeps
    // headers, data and extension blocks in the *same* bitmap `free_bytes`
    // already reports, so `file::budget_for` — the exact formula
    // `add_file`'s own allocator uses — gives an exact block count per file,
    // not merely a conservative one; `make_dir` always allocates exactly one
    // block per directory.
    let mut blocks_needed: u64 = 0;
    for entry in entries {
        blocks_needed += if entry.is_dir {
            1
        } else {
            crate::core::volume::write::file::budget_for(entry.size, &geometry)?.total() as u64
        };
    }
    let bytes_needed = blocks_needed * block_size as u64;
    let free_bytes = writer.free_bytes()?;
    if bytes_needed > free_bytes {
        return Err(CoreError::InvalidInput(format!(
            "'{}' needs {bytes_needed} bytes but '{volume_label}' only has {free_bytes} bytes \
             free",
            source.display()
        )));
    }

    let mut summary = CopySummary::default();
    let mut block_of: HashMap<String, u32> = HashMap::new();
    block_of.insert(String::new(), 0u32); // 0 is VolumeWriter's own spelling of the root

    let total = entries.len() as u64;
    for (done, entry) in entries.iter().enumerate() {
        if sink.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        sink.report(done as u64, Some(total), &entry.relative);

        let parent =
            *block_of
                .get(parent_key(&entry.relative))
                .ok_or_else(|| CoreError::Malformed {
                    format: "volume".into(),
                    detail: format!("'{}' has no parent directory tracked", entry.relative),
                })?;
        let name = leaf_name(&entry.relative);

        if entry.is_dir {
            let outcome = writer.make_dir(parent, name)?;
            let block = outcome.block.ok_or_else(|| CoreError::Malformed {
                format: "volume".into(),
                detail: format!("'{}' was created with no block of its own", entry.relative),
            })?;
            block_of.insert(entry.relative.clone(), block);
            summary.directories += 1;

            // Binding requirement 1: a directory's own sidecar, applied the
            // same as a file's — `make_dir` takes no metadata itself, so
            // `set_attributes` is what carries it onto the just-created
            // header block.
            let sidecar = uaem::sidecar_path(&entry.host_path);
            if sidecar.is_file() {
                let text = std::fs::read_to_string(&sidecar)?;
                let parsed = uaem::parse(&text)?;
                writer.set_attributes(block, Some(parsed.protection), None, Some(parsed.date))?;
            }
        } else {
            let data = std::fs::read(&entry.host_path)?;
            let sidecar = uaem::sidecar_path(&entry.host_path);
            let meta = if sidecar.is_file() {
                let text = std::fs::read_to_string(&sidecar)?;
                let parsed = uaem::parse(&text)?;
                FileMeta {
                    protection: Some(parsed.protection),
                    date: Some(parsed.date),
                }
            } else {
                FileMeta::default()
            };
            writer.add_file(parent, name, &data, meta)?;
            summary.files += 1;
            summary.bytes = summary.bytes.map(|n| n + data.len() as u64);
        }
    }

    Ok(summary)
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::jobs::NoProgress;
    use crate::core::osinstall::fixtures;
    use crate::core::preload::pfs3_test_device::MemDevice;
    use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec};

    /// Several tests share a tag (`"pds3"`, for instance) because they are
    /// building the same *kind* of fixture, not because they may share a
    /// directory: Cargo runs tests in parallel within one process, so the tag
    /// alone is not enough to keep two of them apart. The local counter this
    /// used to append is now inside [`crate::core::test_scratch_id`], which
    /// every `ScratchDir` name carries, so one call still cannot collide with
    /// another (ART-281).
    fn scratch(tag: &str) -> (crate::core::ScratchDir, PathBuf) {
        crate::core::ScratchDir::pair("art-preload-native", tag)
    }

    /// **ART-160.** `collect_entries` takes the AmigaDOS name off the tree's
    /// own `distribution.json` when the host had to store a file under a
    /// different one, so what is copied onto the card is `AUX`, not `_AUX`.
    ///
    /// The whole path is checked, not only the leaf: a renamed *drawer* has
    /// to translate for everything beneath it, and a walk that translated the
    /// leaf while keeping the escaped parent would place the file in a drawer
    /// no Amiga names.
    #[test]
    fn an_escaped_host_name_is_copied_under_its_amiga_name() {
        let (_guard, dir) = scratch("amiga-names");
        let tree = dir.join("dist");
        std::fs::create_dir_all(tree.join("Storage").join("_CON")).unwrap();
        std::fs::write(tree.join("Storage").join("_CON").join("_AUX"), b"driver").unwrap();
        std::fs::write(
            tree.join("distribution.json"),
            br#"{"release":"AmigaOS 3.9","builtFrom":[],"files":[
                 {"path":"Storage/CON/AUX","hostPath":"Storage/_CON/_AUX",
                  "component":"workbench-base","media":"Workbench3.9",
                  "sha256":"","bytes":6}]}"#,
        )
        .unwrap();

        let entries = collect_entries(&tree).unwrap();
        let relatives: Vec<&str> = entries.iter().map(|e| e.relative.as_str()).collect();
        assert!(
            relatives.contains(&"Storage/CON/AUX"),
            "the Amiga name, whole path: {relatives:?}"
        );
        assert!(
            relatives.contains(&"Storage/CON"),
            "including the drawer: {relatives:?}"
        );
        assert!(
            !relatives.iter().any(|r| r.contains('_')),
            "nothing escaped may survive into the copy: {relatives:?}"
        );
        // `distribution.json` itself is still copied — it is a real file of
        // the tree, and the card carries it.
        assert!(relatives.contains(&"distribution.json"));
    }

    /// A folder a user assembled has no manifest, so nothing is translated
    /// and a genuine `_AUX` stays `_AUX`.
    #[test]
    fn a_folder_without_a_manifest_is_copied_verbatim() {
        let (_guard, dir) = scratch("no-manifest");
        let tree = dir.join("src");
        std::fs::create_dir_all(&tree).unwrap();
        std::fs::write(tree.join("_AUX"), b"mine").unwrap();

        let entries = collect_entries(&tree).unwrap();
        assert_eq!(
            entries
                .iter()
                .map(|e| e.relative.as_str())
                .collect::<Vec<_>>(),
            vec!["_AUX"]
        );
    }

    /// **C6 (final whole-branch review).** `.art-backup` — ART's own
    /// host-side safety net, never Amiga content — must not reach the card
    /// any more than a `.uaem` sidecar does. Built with a nested drawer's
    /// own backup folder too, since `guarded_write` places one beside
    /// whatever it just backed up, at any depth in the tree.
    #[test]
    fn an_art_backup_drawer_is_never_copied_onto_the_card() {
        let (_guard, dir) = scratch("art-backup-excluded");
        let tree = dir.join("dist");
        std::fs::create_dir_all(tree.join(BACKUP_DIR)).unwrap();
        std::fs::write(
            tree.join(BACKUP_DIR).join("Prefs.info.12345-000000000.bak"),
            b"old bytes",
        )
        .unwrap();
        std::fs::create_dir_all(tree.join("Prefs").join(BACKUP_DIR)).unwrap();
        std::fs::write(
            tree.join("Prefs")
                .join(BACKUP_DIR)
                .join("Env-Archive.info.12345-000000001.bak"),
            b"old bytes too",
        )
        .unwrap();
        std::fs::write(tree.join("Prefs.info"), b"current").unwrap();

        let entries = collect_entries(&tree).unwrap();
        let relatives: Vec<&str> = entries.iter().map(|e| e.relative.as_str()).collect();
        assert!(
            !relatives.iter().any(|r| r.contains(BACKUP_DIR)),
            "no .art-backup drawer, at any depth, may reach the card: {relatives:?}"
        );
        assert!(
            relatives.contains(&"Prefs.info"),
            "the real file beside it is still copied: {relatives:?}"
        );
    }

    fn card_with_partition(
        tag: &str,
        fs: AmigaHardDiskFs,
        size_mb: u32,
    ) -> (crate::core::ScratchDir, PathBuf) {
        let (_guard, dir) = scratch(tag);
        let path = dir.join("card.hdf");
        crate::core::hdf::create_hdf(
            &path,
            32 * 1024 * 1024,
            true,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: fs,
                size_mb,
                bootable: true,
                boot_priority: 0,
                num_buffers: 0,
            }],
            &[],
        )
        .unwrap();
        (_guard, path)
    }

    fn rdb_image_with_one_pds3_partition() -> (crate::core::ScratchDir, PathBuf) {
        card_with_partition("pds3", AmigaHardDiskFs::Pfs3DirectScsi, 8)
    }

    fn rdb_image_with_one_dos3_partition() -> (crate::core::ScratchDir, PathBuf) {
        card_with_partition("dos3", AmigaHardDiskFs::FfsDirCache, 8)
    }

    fn partition_offset(image: &Path) -> u64 {
        let card = read_card(image).unwrap();
        let part = &card.areas[0].rdb.partitions[0];
        card.areas[0].offset_bytes + part.byte_offset().unwrap()
    }

    fn formatted_pds3_image() -> (crate::core::ScratchDir, PathBuf) {
        let (_guard, image) = rdb_image_with_one_pds3_partition();
        NativeFormatter::UTC
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();
        (_guard, image)
    }

    /// A freshly formatted PFS3 partition of `mb` megabytes, on its own card.
    fn formatted_pds3_image_of(mb: u32) -> (crate::core::ScratchDir, PathBuf) {
        let (_guard, path) =
            card_with_partition(&format!("pds3-{mb}mb"), AmigaHardDiskFs::Pfs3DirectScsi, mb);
        NativeFormatter::UTC
            .format_partition(&path, None, 1, "Work", &NoProgress)
            .unwrap();
        (_guard, path)
    }

    fn tree_of_bytes(bytes: usize) -> (crate::core::ScratchDir, PathBuf) {
        let (_guard, dir) = scratch("big-tree");
        std::fs::write(dir.join("Big.bin"), vec![0xABu8; bytes]).unwrap();
        (_guard, dir)
    }

    /// Bytes free on a formatted PFS3 partition, read with `libpfs3`'s own
    /// reader — independent of `copy_in_pfs3`'s own arithmetic, which is
    /// exactly what a boundary test needs.
    fn pfs3_free_bytes(image: &Path) -> u64 {
        let vol = libpfs3::volume::Volume::open(image, partition_offset(image)).unwrap();
        u64::from(vol.free_blocks()) * u64::from(vol.block_size())
    }

    /// A PFS3 partition past MAXSMALLDISK (10 241 440 blocks), the size at which
    /// a format selects SUPERINDEX mode: 5 100 MiB is 10 362 cylinders,
    /// 10 444 896 blocks. The image is extended with `set_len` (`create_hdf`),
    /// and the format writes only the reserved area near the partition's start.
    fn formatted_large_pds3_image() -> (crate::core::ScratchDir, PathBuf) {
        let (_guard, dir) = scratch("pds3-large");
        let path = dir.join("card.hdf");
        crate::core::hdf::create_hdf(
            &path,
            5_200 * 1024 * 1024,
            true,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::Pfs3DirectScsi,
                size_mb: 5_100,
                bootable: true,
                boot_priority: 0,
                num_buffers: 0,
            }],
            &[],
        )
        .unwrap();
        NativeFormatter::UTC
            .format_partition(&path, None, 1, "Work", &NoProgress)
            .unwrap();
        (_guard, path)
    }

    /// **ART-310.** What a PFS3 format left on disk, read straight off the
    /// image — not through `libpfs3`'s reader, which shares a crate with the
    /// writer under test. Follows the rootblock to the first anode block:
    /// `rootblock.indexblocks[0]` in small mode, `rext.superindex[0]` in
    /// SUPERINDEX mode.
    struct AnodeChain {
        /// `MODE_SUPERINDEX` (0x80) is set in the rootblock's options.
        supermode: bool,
        /// The two-byte id of every block from that pointer down to the first
        /// `AB`, in order: `IB AB` in small mode, `SB IB AB` in SUPERINDEX mode.
        ids: Vec<[u8; 2]>,
        /// Anodes 0–5 of the first anode block, as `(clustersize, blocknr, next)`.
        anodes: Vec<(u32, u32, u32)>,
    }

    fn anode_chain(image: &Path) -> AnodeChain {
        use std::io::{Read, Seek, SeekFrom};
        const MODE_SUPERINDEX: u32 = 0x80;
        let offset = partition_offset(image);
        let mut file = std::fs::File::open(image).unwrap();
        let mut sectors = |sector: u32, len: usize| -> Vec<u8> {
            file.seek(SeekFrom::Start(offset + u64::from(sector) * 512))
                .unwrap();
            let mut buf = vec![0u8; len];
            file.read_exact(&mut buf).unwrap();
            buf
        };
        let be16 = |b: &[u8], at: usize| u16::from_be_bytes([b[at], b[at + 1]]);
        let be32 = |b: &[u8], at: usize| u32::from_be_bytes(b[at..at + 4].try_into().unwrap());

        // Rootblock at sector 2: options 0x04, reserved_blksize 0x40,
        // extension 0x58, and the index union from 0x60 (small mode:
        // bitmapindex[0..=4], then indexblocks[0..]).
        let root = sectors(2, 512);
        let supermode = be32(&root, 0x04) & MODE_SUPERINDEX != 0;
        let resblk = usize::from(be16(&root, 0x40));
        let mut next = if supermode {
            let ext = sectors(be32(&root, 0x58), resblk);
            be32(&ext, 0x40) // rext.superindex[0]
        } else {
            be32(&root, 0x60 + 5 * 4) // rootblock.indexblocks[0]
        };
        let mut ids = Vec::new();
        loop {
            let block = sectors(next, resblk);
            let id = [block[0], block[1]];
            ids.push(id);
            if &id == b"AB" {
                // Anode block: a 16-byte header, then 12-byte anodes.
                let anodes = (0..6)
                    .map(|k| {
                        let at = 16 + 12 * k;
                        (be32(&block, at), be32(&block, at + 4), be32(&block, at + 8))
                    })
                    .collect();
                return AnodeChain {
                    supermode,
                    ids,
                    anodes,
                };
            }
            assert!(
                (&id == b"SB" || &id == b"IB") && ids.len() < 3,
                "the anode pointer chain reached {:?} after {:?}",
                String::from_utf8_lossy(&id),
                ids.iter()
                    .map(|i| String::from_utf8_lossy(i).into_owned())
                    .collect::<Vec<_>>()
            );
            next = be32(&block, 12); // index[0], after a 12-byte header
        }
    }

    fn be16(b: &[u8], at: usize) -> u16 {
        u16::from_be_bytes([b[at], b[at + 1]])
    }

    fn be32(b: &[u8], at: usize) -> u32 {
        u32::from_be_bytes(b[at..at + 4].try_into().unwrap())
    }

    /// **ART-313.** Every directory block in a PFS3 partition's reserved area,
    /// read straight off the image as `(anodenr, parent)`: `anodenr` (0x0C) is
    /// the directory's own first anode on every one of its blocks, and `parent`
    /// (0x10) is what pfs3aio's `GetParent` reads. Not through `libpfs3`'s
    /// reader, which never reads `parent` at all.
    fn dir_block_parents(image: &Path) -> Vec<(u32, u32)> {
        use std::io::{Read, Seek, SeekFrom};
        let offset = partition_offset(image);
        let mut file = std::fs::File::open(image).unwrap();
        let mut sectors = |sector: u32, len: usize| -> Vec<u8> {
            file.seek(SeekFrom::Start(offset + u64::from(sector) * 512))
                .unwrap();
            let mut buf = vec![0u8; len];
            file.read_exact(&mut buf).unwrap();
            buf
        };
        // Rootblock: lastreserved 0x34, firstreserved 0x38, reserved_blksize 0x40.
        let root = sectors(2, 512);
        let lastreserved = be32(&root, 0x34);
        let resblk = usize::from(be16(&root, 0x40));
        let rescluster = (resblk / 512) as u32;
        let mut found = Vec::new();
        let mut blk = be32(&root, 0x38);
        while blk + rescluster - 1 <= lastreserved {
            let block = sectors(blk, resblk);
            if &block[0..2] == b"DB" {
                found.push((be32(&block, 0x0C), be32(&block, 0x10)));
            }
            blk += rescluster;
        }
        found
    }

    /// **ART-318.** One used deldir entry as pfs3aio's handler reads it
    /// (`blocks.h:368-379`): the name is the `name_len` bytes after the length
    /// byte at 0x0E.
    #[derive(Debug, PartialEq)]
    struct RawDelEntry {
        slot: usize,
        anodenr: u32,
        fsize: u32,
        date: Vec<u8>,
        name_len: u8,
        name: Vec<u8>,
        fsizex: u16,
    }

    /// **ART-318.** The deldir straight off the image: `rext.deldirroving`, and
    /// every entry with a nonzero `anodenr` in every block `rext.deldir[..deldirsize]`
    /// names, 31 to a block (`blocks.h:611`). Not through `libpfs3`'s reader.
    fn raw_deldir(image: &Path) -> (u16, Vec<RawDelEntry>) {
        let offset = partition_offset(image) as usize;
        let raw = std::fs::read(image).unwrap();
        let sector = |n: u32, len: usize| {
            let at = offset + n as usize * 512;
            raw[at..at + len].to_vec()
        };
        let root = sector(2, 512);
        let resblk = usize::from(be16(&root, 0x40));
        let ext = sector(be32(&root, 0x58), resblk);
        let mut entries = Vec::new();
        for b in 0..usize::from(be16(&ext, 0x36)) {
            let dd = sector(be32(&ext, 0x90 + b * 4), resblk);
            assert_eq!(&dd[0..2], b"DD", "deldir[{b}] must be a deldir block");
            for e in 0..31 {
                let at = 32 + e * 32;
                let anodenr = be32(&dd, at);
                if anodenr == 0 {
                    continue;
                }
                let len = usize::from(dd[at + 14]).min(17);
                entries.push(RawDelEntry {
                    slot: b * 31 + e,
                    anodenr,
                    fsize: be32(&dd, at + 4),
                    date: dd[at + 8..at + 14].to_vec(),
                    name_len: dd[at + 14],
                    name: dd[at + 15..at + 15 + len].to_vec(),
                    fsizex: be16(&dd, at + 30),
                });
            }
        }
        (be16(&ext, 0x34), entries)
    }

    /// A PFS3 volume of `total_blocks` formatted in memory by the vendored
    /// `libpfs3`, with the deldir off (Task 7 turns it on for `NativeFormatter`;
    /// these fill and bounds tests do not depend on it).
    fn formatted_in_memory(dev: &MemDevice, total_blocks: u64) -> libpfs3::format::FormatResult {
        libpfs3::format::format_with_size(
            dev,
            total_blocks,
            &libpfs3::format::FormatOptions {
                volume_name: "Work".into(),
                enable_deldir: false,
                datestamp: None,
            },
        )
        .unwrap()
    }

    /// Write `dirs` directories of `per_dir` files, every file with its own
    /// content, through one `libpfs3` writer; drop it; reopen the volume from
    /// the device — which knows only what reached it — and return every file
    /// that does not read back as its own bytes, with the time the writes took.
    fn fill_and_read_back(
        dev: &MemDevice,
        dirs: usize,
        per_dir: usize,
    ) -> (Vec<String>, std::time::Duration) {
        let mut written = Vec::with_capacity(dirs * per_dir);
        let started = std::time::Instant::now();
        {
            let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            for d in 0..dirs {
                let dir = format!("D{d:03}");
                w.create_dir(&dir).unwrap_or_else(|e| panic!("{dir}: {e}"));
                for f in 0..per_dir {
                    let name = format!("{dir}/F{f:05}");
                    let content = format!("{name}-unique-content");
                    w.write_file(&name, content.as_bytes())
                        .unwrap_or_else(|e| panic!("{name}: {e}"));
                    written.push((name, content));
                }
            }
        }
        let took = started.elapsed();
        let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
        let wrong = written
            .into_iter()
            .filter(|(name, content)| {
                vol.read_file(name).ok().as_deref() != Some(content.as_bytes())
            })
            .map(|(name, _)| name)
            .collect();
        (wrong, took)
    }

    /// The pieces needed to reopen a formatted `DOS\3` partition's volume for
    /// verification, without going through `NativeFormatter` a second time.
    fn ffs_region(image: &Path) -> (FileRegionMut, VolumeGeometry, u64) {
        let card = read_card(image).unwrap();
        let part = &card.areas[0].rdb.partitions[0];
        let offset = card.areas[0].offset_bytes + part.byte_offset().unwrap();
        let length = part.byte_length().unwrap();
        let block_size = part.block_bytes() as usize;
        let dos = DosType::new(part.dostype.to_be_bytes());
        let region = FileRegionMut::open(image, offset, length, block_size).unwrap();
        let total_blocks = region.total_blocks();
        let geometry = VolumeGeometry::new(block_size, total_blocks, part.reserved, dos).unwrap();
        (region, geometry, offset)
    }

    fn ffs_free_bytes(image: &Path) -> u64 {
        let (mut region, geometry, offset) = ffs_region(image);
        VolumeWriter::open(&mut region, geometry, image, offset)
            .unwrap()
            .free_bytes()
            .unwrap()
    }

    /// The most data blocks a **single** FFS file can occupy while its own
    /// `file::budget_for` total (data + extension + one header block) still
    /// fits in `target_total_blocks`. A single file's overhead is not just
    /// rounding to a block — every 72 data blocks (`pointers_per_block` at
    /// 512 bytes) needs one more extension block to hold their pointers — so
    /// "the volume's whole free space, in one file" and "the volume's whole
    /// free space, in many small files" land at different byte counts. This
    /// mirrors `budget_for`'s own formula rather than reimplementing a
    /// different one, so the boundary it finds is the same boundary
    /// `copy_in_ffs`'s real check uses.
    fn largest_single_ffs_file_data_blocks(target_total_blocks: u64, block_size: usize) -> u64 {
        let per_header = crate::core::volume::write::layout::pointers_per_block(block_size) as u64;
        let mut best = 0u64;
        let mut data_blocks = 0u64;
        loop {
            let extension_blocks = data_blocks
                .saturating_sub(per_header)
                .div_ceil(per_header.max(1));
            let total = data_blocks + extension_blocks + 1; // +1: the header block
            if total > target_total_blocks {
                break;
            }
            best = data_blocks;
            data_blocks += 1;
        }
        best
    }

    // ---- Step 1's given tests ----

    #[test]
    fn it_formats_a_pfs3_partition_and_reads_the_volume_name_back() {
        let (_guard, image) = rdb_image_with_one_pds3_partition();
        NativeFormatter::UTC
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();

        // libpfs3's own reader, not ART's — the independent half of the check.
        let vol = libpfs3::volume::Volume::open(&image, partition_offset(&image)).unwrap();
        assert_eq!(vol.name(), "Work");
    }

    // The brief's own `it_formats_an_ffs_partition_with_arts_own_writer` used
    // to live here, asserting `scan_image(&image).unwrap().volumes[0].name
    // == "DH0"`. `scan_image` reports the RDB's own drive name regardless of
    // whether the partition was ever formatted, so that assertion could not
    // fail for the property it claimed to prove — see
    // `format_ffs_writes_the_requested_volume_name_into_the_root_block` and
    // `a_freshly_formatted_ffs_volume_accepts_a_real_write` below, which
    // replace it with checks that actually exercise the write.

    #[test]
    fn copy_in_carries_the_protection_bits_out_of_the_uaem_sidecars() {
        let (_guard, image) = formatted_pds3_image();
        let (_guard, tree) = fixtures::scratch("copy-in-protection");
        std::fs::create_dir_all(tree.join("C")).unwrap();
        std::fs::write(tree.join("C/Assign"), b"x").unwrap();
        std::fs::write(
            tree.join("C/Assign.uaem"),
            "--p-rwed 2021-04-13 02:43:13.68 \n",
        )
        .unwrap();

        NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();

        let mut vol = libpfs3::volume::Volume::open(&image, partition_offset(&image)).unwrap();
        let entry = vol
            .list_dir("C")
            .unwrap()
            .into_iter()
            .find(|e| e.name == "Assign")
            .unwrap();
        assert_eq!(
            libpfs3::util::amiga_protection_string(entry.protection),
            "--p-rwed"
        );
    }

    #[test]
    fn a_sidecar_is_applied_and_never_copied_as_a_file_of_its_own() {
        let (_guard, image) = formatted_pds3_image();
        let (_guard, tree) = fixtures::scratch("copy-in-sidecar-not-a-file");
        std::fs::create_dir_all(tree.join("C")).unwrap();
        std::fs::write(tree.join("C/Assign"), b"x").unwrap();
        std::fs::write(
            tree.join("C/Assign.uaem"),
            "--p-rwed 2021-04-13 02:43:13.68 \n",
        )
        .unwrap();

        NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();

        let mut vol = libpfs3::volume::Volume::open(&image, partition_offset(&image)).unwrap();
        assert!(vol
            .list_dir("C")
            .unwrap()
            .iter()
            .all(|e| !e.name.ends_with(".uaem")));
    }

    #[test]
    fn copy_in_reports_what_it_moved() {
        let (_guard, image) = formatted_pds3_image();
        let (_guard, tree) = fixtures::scratch("copy-in-summary");
        std::fs::create_dir_all(tree.join("C")).unwrap();
        std::fs::write(tree.join("C/Assign"), b"x").unwrap();

        let summary = NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();
        assert_eq!(summary.files, 1);
        assert_eq!(summary.directories, 1);
    }

    // ---- ART-310: the format writes what pfs3aio writes ----

    /// **ART-310, every size.** pfs3aio's format reserves anodes 0–4 by
    /// allocating them (`AllocAnode` leaves `clustersize 0, blocknr 0xffffffff,
    /// next 0`); `libpfs3` 0.1.3 left them `(0, 0, 0)`, which every allocator
    /// ported from pfs3aio reads as free — hst-imager's first new directory then
    /// failed `ERROR_DISK_FULL`, and a later one was given reserved anode 1.
    #[test]
    fn a_small_pfs3_format_reserves_anodes_zero_to_four() {
        let (_guard, image) = formatted_pds3_image();
        let chain = anode_chain(&image);
        assert!(!chain.supermode, "an 8 MB partition must be small mode");
        assert_eq!(chain.ids, vec![*b"IB", *b"AB"]);
        for (nr, anode) in chain.anodes.iter().take(5).enumerate() {
            assert_eq!(
                *anode,
                (0, 0xFFFF_FFFF, 0),
                "anode {nr} must be reserved the way pfs3aio's AllocAnode leaves it"
            );
        }
        assert_eq!(
            chain.anodes[5].0, 1,
            "anode 5 is the root directory, one block"
        );
    }

    /// **ART-310, SUPERINDEX mode.** Every reader — pfs3aio's `GetSuperBlock`,
    /// hst-imager's port, `libpfs3`'s own `resolve_anode_block` — walks
    /// `superindex[0] -> SB -> IB -> AB`. `libpfs3` 0.1.3 pointed
    /// `superindex[0]` at the index block itself: hst-imager could not mount
    /// the volume and `libpfs3` could not read its own root directory.
    #[test]
    fn a_large_pfs3_format_writes_the_superblock_level() {
        let (_guard, image) = formatted_large_pds3_image();
        let chain = anode_chain(&image);
        assert!(
            chain.supermode,
            "a 5 100 MiB partition must be SUPERINDEX mode"
        );
        assert_eq!(
            chain.ids,
            vec![*b"SB", *b"IB", *b"AB"],
            "superindex[0] must name a super index block, not the anode index block"
        );
        for (nr, anode) in chain.anodes.iter().take(5).enumerate() {
            assert_eq!(
                *anode,
                (0, 0xFFFF_FFFF, 0),
                "anode {nr} must be reserved in SUPERINDEX mode too"
            );
        }
    }

    /// **ART-310, the consequence.** A volume past MAXSMALLDISK that
    /// `NativeFormatter` formatted takes `copy_in`'s writes and gives back the
    /// same bytes. On 0.1.3 the first write failed `anode 5 not found`.
    #[test]
    fn a_large_pfs3_volume_takes_its_own_writes() {
        let (_guard, image) = formatted_large_pds3_image();
        let (_guard, tree) = fixtures::scratch("large-pfs3-writes");
        std::fs::create_dir_all(tree.join("C")).unwrap();
        std::fs::write(tree.join("C/Assign"), b"assign\n").unwrap();
        std::fs::write(tree.join("Readme"), b"hello from ART\n").unwrap();

        let summary = NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();
        assert_eq!((summary.files, summary.directories), (2, 1));

        let mut vol = libpfs3::volume::Volume::open(&image, partition_offset(&image)).unwrap();
        assert_eq!(vol.read_file("C/Assign").unwrap(), b"assign\n");
        assert_eq!(vol.read_file("Readme").unwrap(), b"hello from ART\n");
    }

    /// **ART-312.** One write that allocates two anodes — the file's own,
    /// which starts a fresh anode block, and its directory's, because the new
    /// entry spills the directory into a new block — must get two different
    /// anode numbers. `libpfs3` 0.1.3's writer resolved the second allocation
    /// through the device, where the first's index update was still only
    /// pending, and handed the same number out twice: the file then read back
    /// as its directory's continuation block, with no error anywhere.
    ///
    /// The layout is the one the Windows machine's ART-310 run measured the
    /// collision with (10 directories of 200 files, first seen at
    /// `D009/F00070`); `copy_in` makes the same `create_dir_in` /
    /// `write_file_in` calls per entry. Every file's content is its own, and
    /// every file is read back from a reopened volume, so a collision cannot
    /// hide behind identical contents or an unchecked file.
    #[test]
    fn a_pfs3_write_that_allocates_two_anodes_gets_two_different_ones() {
        let (_guard, image) = formatted_pds3_image_of(24);
        let offset = partition_offset(&image);
        let mut written = Vec::new();
        {
            let vol = libpfs3::volume::Volume::open_rw(&image, offset).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            for d in 0..10 {
                let dir = format!("D{d:03}");
                w.create_dir(&dir).unwrap();
                for f in 0..200 {
                    let name = format!("{dir}/F{f:05}");
                    let content = format!("{name}-unique-content");
                    w.write_file(&name, content.as_bytes())
                        .unwrap_or_else(|e| panic!("{name}: {e}"));
                    written.push((name, content));
                }
            }
            drop(w.into_volume());
        }

        let mut vol = libpfs3::volume::Volume::open(&image, offset).unwrap();
        let wrong: Vec<&str> = written
            .iter()
            .filter(|(name, content)| {
                vol.read_file(name).ok().as_deref() != Some(content.as_bytes())
            })
            .map(|(name, _)| name.as_str())
            .collect();
        assert!(
            wrong.is_empty(),
            "{} of {} files do not read back as their own content: {wrong:?}",
            wrong.len(),
            written.len()
        );
    }

    /// **ART-313.** pfs3aio's directory blocks name the directory that holds
    /// theirs: the root's own blocks carry `parent` 0 (`format.c:548`), a
    /// directory in the root carries 5 (`directory.c:1652-1655,1707-1708`),
    /// and a continuation block copies the parent of the block it grew from
    /// (`directory.c:3176,3204`). 0.1.3 wrote the root with 5 and gave every
    /// continuation block the directory's own anode. 60-byte names make an
    /// 82-byte entry, 12 to a 1024-byte block, so 20 of them give the root, `D`
    /// and `D/E` a second block each.
    #[test]
    fn pfs3_directory_blocks_carry_their_containing_directorys_anode_as_parent() {
        let (_guard, image) = formatted_pds3_image();
        let offset = partition_offset(&image);
        let long = |prefix: &str, i: usize| format!("{prefix}{i:0>59}");
        {
            let vol = libpfs3::volume::Volume::open_rw(&image, offset).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            w.create_dir("D").unwrap();
            w.create_dir("D/E").unwrap();
            for i in 0..20 {
                w.write_file(&long("R", i), b"r").unwrap();
                w.write_file(&format!("D/{}", long("F", i)), b"d").unwrap();
                w.write_file(&format!("D/E/{}", long("G", i)), b"e")
                    .unwrap();
            }
            drop(w.into_volume());
        }

        let mut vol = libpfs3::volume::Volume::open(&image, offset).unwrap();
        let d = vol.lookup("D").unwrap().unwrap().anode;
        let e = vol.lookup("D/E").unwrap().unwrap().anode;
        let blocks = dir_block_parents(&image);
        let root = libpfs3::ondisk::ANODE_ROOTDIR;
        for (dir, expected, label) in [(root, 0, "root"), (d, root, "D"), (e, d, "D/E")] {
            let parents: Vec<u32> = blocks
                .iter()
                .filter(|(anodenr, _)| *anodenr == dir)
                .map(|(_, parent)| *parent)
                .collect();
            assert!(
                parents.len() >= 2,
                "{label}: expected a continuation block, found {} block(s)",
                parents.len()
            );
            assert!(
                parents.iter().all(|&p| p == expected),
                "{label} (anode {dir}): every directory block's parent must be {expected}, read {parents:?}"
            );
        }
    }

    /// **ART-315.** pfs3aio and hst-imager leave the data bitmap's tail bits —
    /// the bits past the partition's last block — set, i.e. free; only a block
    /// below the partition's end may be handed out (pfs3aio `allocation.c:344`).
    /// A write asking for one block more than the volume holds must be refused
    /// as disk full, and nothing may reach past the partition. 48 000 blocks is
    /// small mode with 46 590 data blocks: the last bitmap block uses 6 110 of
    /// its 8 096 bits.
    #[test]
    fn a_pfs3_allocation_never_hands_out_a_block_past_the_partition() {
        const TOTAL: u64 = 48_000;
        let dev = MemDevice::with_end(TOTAL);
        let data_blocks = formatted_in_memory(&dev, TOTAL).data_blocks as u32;
        let root = dev.read(2, 512);
        let resblk = usize::from(be16(&root, 0x40));
        let bits_per_bmb = (resblk as u32 / 4 - 3) * 32;
        let last_seq = (data_blocks - 1) / bits_per_bmb;
        // bitmapindex[0] at 0x60 -> the last bitmap block.
        let bmi = dev.read(u64::from(be32(&root, 0x60)), resblk);
        let bm_blk = u64::from(be32(&bmi, 12 + last_seq as usize * 4));
        let mut bm = dev.read(bm_blk, resblk);
        let used_bits = data_blocks - last_seq * bits_per_bmb;
        assert!(
            used_bits < bits_per_bmb,
            "the last bitmap block must have tail bits"
        );
        for bit in used_bits..bits_per_bmb {
            let at = 12 + (bit / 32) as usize * 4;
            let word = be32(&bm, at) | (0x8000_0000 >> (bit % 32));
            bm[at..at + 4].copy_from_slice(&word.to_be_bytes());
        }
        dev.patch(bm_blk, &bm);

        let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
        let mut w = libpfs3::writer::Writer::open(vol).unwrap();
        let one_too_many = vec![0x5Au8; (data_blocks as usize + 1) * 512];
        let err = w.write_file("Big", &one_too_many).unwrap_err();
        assert!(
            matches!(err, libpfs3::error::Error::DiskFull(_)),
            "expected disk full, got: {err}"
        );
        assert_eq!(
            dev.refused(),
            Vec::<u64>::new(),
            "a write reached past the partition's end ({TOTAL})"
        );
    }

    /// **ART-315, the free side.** Freeing a data block past the partition's
    /// end must change nothing: its bitmap bit is exactly a tail bit the
    /// allocator must never find set, and counting it into `blocksfree` claims
    /// a block that does not exist. File `A`'s one extent (anode 6, the first
    /// user anode, in the format's anode block) is pointed at block `TOTAL`
    /// raw, then `A` is deleted.
    #[test]
    fn freeing_a_pfs3_block_past_the_partition_changes_nothing() {
        const TOTAL: u64 = 48_000;
        let dev = MemDevice::with_end(TOTAL);
        formatted_in_memory(&dev, TOTAL);
        {
            let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            w.write_file("A", b"one block").unwrap();
        }
        let root = dev.read(2, 512);
        let resblk = usize::from(be16(&root, 0x40));
        // rootblock.indexblocks[0] (0x60 + 5 × 4) -> IB -> index[0] -> AB.
        let ib = dev.read(u64::from(be32(&root, 0x60 + 5 * 4)), resblk);
        let ab_blk = u64::from(be32(&ib, 12));
        let mut ab = dev.read(ab_blk, resblk);
        let at = 16 + 6 * 12;
        assert_eq!(be32(&ab, at), 1, "anode 6 must be A's one-block extent");
        ab[at + 4..at + 8].copy_from_slice(&(TOTAL as u32).to_be_bytes());
        dev.patch(ab_blk, &ab);

        let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
        let free_before = vol.free_blocks();
        let mut w = libpfs3::writer::Writer::open(vol).unwrap();
        w.delete("A").unwrap();
        assert_eq!(
            w.into_volume().free_blocks(),
            free_before,
            "blocksfree counted block {TOTAL}, past the partition, as freed"
        );

        let root = dev.read(2, 512);
        let bitmapstart = be32(&root, 0x34) + 1;
        let rel = TOTAL as u32 - bitmapstart;
        let bits_per_bmb = (resblk as u32 / 4 - 3) * 32;
        let bmi = dev.read(u64::from(be32(&root, 0x60)), resblk);
        let bm = dev.read(
            u64::from(be32(&bmi, 12 + (rel / bits_per_bmb) as usize * 4)),
            resblk,
        );
        let bit = rel % bits_per_bmb;
        assert_eq!(
            be32(&bm, 12 + (bit / 32) as usize * 4) & (0x8000_0000 >> (bit % 32)),
            0,
            "the bitmap bit for block {TOTAL}, past the partition, was set free"
        );
    }

    /// **ART-314, the vendored crate.** ART's format writes `fnsize` 107, as
    /// hst-imager's does, and the writer refuses a name longer than the
    /// `fnsize - 1` = 106 bytes pfs3aio can find again — instead of storing it
    /// and leaving it unopenable — before anything is written. 106 bytes is
    /// written and read back.
    #[test]
    fn a_pfs3_name_longer_than_fnsize_minus_one_is_refused_before_anything_is_written() {
        let (_guard, image) = formatted_pds3_image();
        let offset = partition_offset(&image) as usize;
        let before = std::fs::read(&image).unwrap();
        // Rootblock at partition sector 2: extension 0x58; rext.fnsize at 0x38.
        let ext = be32(&before[offset + 2 * 512..], 0x58) as usize;
        assert_eq!(
            be16(&before[offset + ext * 512..], 0x38),
            107,
            "rext.fnsize"
        );

        let too_long = "T".repeat(107);
        {
            let vol = libpfs3::volume::Volume::open_rw(&image, offset as u64).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            let err = w.write_file(&too_long, b"x").unwrap_err();
            assert!(
                matches!(
                    err,
                    libpfs3::error::Error::NameTooLong {
                        len: 107,
                        max: 106,
                        ..
                    }
                ),
                "write_file: {err}"
            );
            let err = w.create_dir(&too_long).unwrap_err();
            assert!(
                matches!(err, libpfs3::error::Error::NameTooLong { .. }),
                "create_dir: {err}"
            );
        }
        assert!(
            std::fs::read(&image).unwrap() == before,
            "a refused name changed the image"
        );

        let longest = "L".repeat(106);
        {
            let vol = libpfs3::volume::Volume::open_rw(&image, offset as u64).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            w.write_file(&longest, b"kept").unwrap();
        }
        let mut vol = libpfs3::volume::Volume::open(&image, offset as u64).unwrap();
        assert_eq!(vol.read_file(&longest).unwrap(), b"kept");
    }

    /// **ART-314, ART's side.** A tree holding a name longer than the volume
    /// can store is refused by name — in `can_copy_in`, so the partition is
    /// never formatted for a copy that would stop partway, and in `copy_in`
    /// before the volume is written.
    #[test]
    fn a_pfs3_name_too_long_for_the_volume_is_refused_by_name_before_anything_is_written() {
        let (_guard, image) = formatted_pds3_image();
        let before = std::fs::read(&image).unwrap();
        let (_guard, tree) = fixtures::scratch("copy-in-long-name");
        let long = format!("{}.info", "N".repeat(102)); // 107 bytes
        std::fs::create_dir_all(tree.join("Drawer")).unwrap();
        std::fs::write(tree.join("Drawer").join(&long), b"x").unwrap();
        std::fs::write(tree.join("Short"), b"y").unwrap();

        let err = NativeFormatter::UTC
            .can_copy_in(&image, None, "DH0", &tree)
            .unwrap_err();
        let CoreError::Pfs3NamesTooLong {
            paths,
            more,
            max_bytes,
        } = err
        else {
            panic!("can_copy_in: expected Pfs3NamesTooLong, got {err}");
        };
        assert_eq!(
            (paths, more, max_bytes),
            (vec![format!("Drawer/{long}")], 0, 106)
        );

        let err = NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap_err();
        assert!(
            matches!(&err, CoreError::Pfs3NamesTooLong { max_bytes: 106, .. }),
            "copy_in: {err}"
        );
        assert!(
            format!("{err}").contains(&format!("Drawer/{long}")),
            "{err}"
        );
        assert_eq!(err.code(), "ART-PFS3-NAME-TOO-LONG");
        assert!(
            std::fs::read(&image).unwrap() == before,
            "nothing may be written"
        );
    }

    /// **ART-311, small mode.** 22 000 files in 110 directories — with the
    /// directories and their continuation blocks, ~22 700 anodes — go past the
    /// one anode index block the format makes (253 × 84 − 6 = 21 246 anodes).
    /// pfs3aio makes the next index block on demand and names it in the
    /// rootblock (`anodes.c:740-761`); every file must read back from a volume
    /// reopened off the device, which sees only what the rootblock names.
    /// 48 000 blocks: small mode, 46 590 data blocks, 1 408 reserved.
    #[test]
    fn a_small_pfs3_volume_takes_more_anodes_than_one_index_block_holds() {
        const TOTAL: u64 = 48_000;
        let dev = MemDevice::with_end(TOTAL);
        formatted_in_memory(&dev, TOTAL);
        let (wrong, took) = fill_and_read_back(&dev, 110, 200);
        println!("ART-311 small mode: 22 000 files written in {took:?}");
        assert!(
            wrong.is_empty(),
            "{} of 22 000 files do not read back as their own content, first: {:?}",
            wrong.len(),
            &wrong[..wrong.len().min(10)]
        );
        let root = dev.read(2, 512);
        assert_ne!(
            be32(&root, 0x60 + 5 * 4 + 4),
            0,
            "rootblock.indexblocks[1] must name the second index block"
        );
        assert_eq!(dev.refused(), Vec::<u64>::new());
    }

    /// **ART-311, SUPERINDEX mode.** The same 22 000 files on a volume past
    /// MAXSMALLDISK, past the 256 anode blocks the old search stopped at
    /// (256 × 84 − 6 = 21 498 anodes). 10 444 896 blocks is the partition
    /// `formatted_large_pds3_image` makes, held sparse in memory.
    #[test]
    fn a_superindex_pfs3_volume_takes_more_anodes_than_256_anode_blocks_hold() {
        const TOTAL: u64 = 10_444_896;
        let dev = MemDevice::with_end(TOTAL);
        formatted_in_memory(&dev, TOTAL);
        let (wrong, took) = fill_and_read_back(&dev, 110, 200);
        println!("ART-311 SUPERINDEX mode: 22 000 files written in {took:?}");
        assert!(
            wrong.is_empty(),
            "{} of 22 000 files do not read back as their own content, first: {:?}",
            wrong.len(),
            &wrong[..wrong.len().min(10)]
        );
        assert_eq!(dev.refused(), Vec::<u64>::new());
    }

    /// **ART-311, a second super block.** At 1024-byte reserved blocks one
    /// super block addresses 253² = 64 009 anode blocks, so the 16-bit anode
    /// block range reaches `superindex[1]`. Filling that many is not a test, so
    /// the volume is made to look full: the format's super block names its one
    /// index block 253 times, that index block names its one anode block 253
    /// times, and anodes 6–83 are taken. The next allocation must make super
    /// block 1 (pfs3aio `NewSuperBlock`, `anodes.c:844-870`) holding an index
    /// block numbered 253 across the volume (`anodes.c:592-595,767`), and name
    /// it in the rootblock extension (`blocks.h:448`, `update.c:240-256`). The
    /// free reserved blocks it takes hold stale bytes first, as on a used
    /// volume, so a super block read from the device rather than from the
    /// writer's own pending write cannot pass.
    #[test]
    fn a_superindex_pfs3_volume_makes_its_second_super_block() {
        const TOTAL: u64 = 10_444_896;
        let dev = MemDevice::with_end(TOTAL);
        formatted_in_memory(&dev, TOTAL);
        let root = dev.read(2, 512);
        let resblk = usize::from(be16(&root, 0x40));
        let rescluster = (resblk / 512) as u64;
        let ext_blk = u64::from(be32(&root, 0x58));
        let sb0 = u64::from(be32(&dev.read(ext_blk, resblk), 0x40));
        let mut sb = dev.read(sb0, resblk);
        let ib0 = be32(&sb, 12);
        let mut ib = dev.read(u64::from(ib0), resblk);
        let ab0 = be32(&ib, 12);
        let mut ab = dev.read(u64::from(ab0), resblk);
        for k in 6..84 {
            let at = 16 + k * 12;
            ab[at..at + 4].copy_from_slice(&1u32.to_be_bytes());
            ab[at + 4..at + 8].copy_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
        }
        for i in 0..253 {
            sb[12 + i * 4..16 + i * 4].copy_from_slice(&ib0.to_be_bytes());
            ib[12 + i * 4..16 + i * 4].copy_from_slice(&ab0.to_be_bytes());
        }
        dev.patch(u64::from(ab0), &ab);
        dev.patch(u64::from(ib0), &ib);
        dev.patch(sb0, &sb);
        // Stale bytes in the first eight free reserved blocks, the ones
        // `alloc_reserved_block` hands out next (it scans the bitmap from 0).
        let firstreserved = u64::from(be32(&root, 0x38));
        let cluster = dev.read(2, usize::from(be16(&root, 0x42)) * 512);
        let (mut stale, mut idx) = (0, 0u64);
        while stale < 8 {
            let word = be32(&cluster, 512 + 12 + (idx / 32) as usize * 4);
            if word & (0x8000_0000 >> (idx % 32)) != 0 {
                dev.patch(firstreserved + idx * rescluster, &vec![0xEEu8; resblk]);
                stale += 1;
            }
            idx += 1;
            assert!(idx < 1 << 20, "no free reserved blocks found");
        }

        {
            let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            w.write_file("Past64009", b"in the second super block")
                .unwrap();
        }

        let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
        let entry = vol.lookup("Past64009").unwrap().unwrap();
        assert_eq!(
            entry.anode >> 16,
            64_009,
            "the file's anode is in anode block 64 009"
        );
        assert_eq!(
            vol.read_file("Past64009").unwrap(),
            b"in the second super block"
        );
        let ext = dev.read(ext_blk, resblk);
        let sb1 = be32(&ext, 0x44);
        assert_ne!(sb1, 0, "rext.superindex[1] must name the new super block");
        let sb1 = dev.read(u64::from(sb1), resblk);
        assert_eq!(
            (&sb1[0..2], be32(&sb1, 8)),
            (&b"SB"[..], 1),
            "super block 1: id, seqnr"
        );
        let ib1 = dev.read(u64::from(be32(&sb1, 12)), resblk);
        assert_eq!(
            (&ib1[0..2], be32(&ib1, 8)),
            (&b"IB"[..], 253),
            "its index block: id, seqnr across the volume"
        );
        assert_eq!(dev.refused(), Vec::<u64>::new());
    }

    /// **ART-316.** `enable_deldir` makes the deldir pfs3aio's format makes:
    /// `MODE_DELDIR | MODE_SUPERDELDIR` (`format.c:249-255`); two `DD` blocks
    /// with seqnr 0 and 1, protection `DELENTRY_PROT` 5 at 0x16 and the
    /// rootblock's creation date at 0x1A (`directory.c:4466-4479`,
    /// `blocks.h:381-396`), named by `rext.deldir[0..2]` at 0x90, `deldirsize`
    /// 2 at 0x36 and `deldirroving` 0 at 0x34 (`directory.c:4627-4633`,
    /// `blocks.h:431-456`), both taken out of the reserved area. Without the
    /// option the format is unchanged. Both modes.
    #[test]
    fn a_pfs3_format_with_the_deldir_makes_pfs3aios_two_deldir_blocks() {
        for total in [48_000u64, 10_444_896] {
            let format = |deldir: bool| {
                let dev = MemDevice::with_end(total);
                libpfs3::format::format_with_size(
                    &dev,
                    total,
                    &libpfs3::format::FormatOptions {
                        volume_name: "Work".into(),
                        enable_deldir: deldir,
                        datestamp: None,
                    },
                )
                .unwrap();
                dev
            };
            let (off, on) = (format(false), format(true));
            let (root_off, root) = (off.read(2, 512), on.read(2, 512));
            let resblk = usize::from(be16(&root, 0x40));
            let (ext_off, ext) = (
                off.read(u64::from(be32(&root_off, 0x58)), resblk),
                on.read(u64::from(be32(&root, 0x58)), resblk),
            );

            assert_eq!(
                be32(&root_off, 0x04) & (8 | 256),
                0,
                "{total}: off, no deldir flags"
            );
            assert_eq!(
                (be16(&ext_off, 0x36), be32(&ext_off, 0x90)),
                (0, 0),
                "{total}: off, no deldir"
            );
            assert_eq!(
                be32(&root, 0x04) & (8 | 256),
                8 | 256,
                "{total}: MODE_DELDIR | MODE_SUPERDELDIR"
            );
            assert_eq!(
                be32(&root, 0x3C) + 2,
                be32(&root_off, 0x3C),
                "{total}: the deldir takes two reserved blocks"
            );
            assert_eq!(
                (be16(&ext, 0x34), be16(&ext, 0x36), be32(&ext, 0x98)),
                (0, 2, 0),
                "{total}: deldirroving, deldirsize, deldir[2]"
            );
            let firstreserved = be32(&root, 0x38);
            let cluster = on.read(2, usize::from(be16(&root, 0x42)) * 512);
            for seq in 0..2u32 {
                let blk = be32(&ext, 0x90 + seq as usize * 4);
                assert_ne!(blk, 0, "{total}: deldir[{seq}]");
                let dd = on.read(u64::from(blk), resblk);
                assert_eq!(&dd[0..2], b"DD", "{total}: deldir[{seq}] id");
                assert_eq!(
                    (be32(&dd, 0x08), be32(&dd, 0x16)),
                    (seq, 5),
                    "{total}: deldir[{seq}] seqnr, protection"
                );
                assert_eq!(
                    &dd[0x1A..0x20],
                    &root[0x0C..0x12],
                    "{total}: deldir[{seq}] creation date is the rootblock's"
                );
                let idx = (blk - firstreserved) / (resblk as u32 / 512);
                assert_eq!(
                    be32(&cluster, 512 + 12 + (idx / 32) as usize * 4)
                        & (0x8000_0000 >> (idx % 32)),
                    0,
                    "{total}: deldir[{seq}] must be marked used in the reserved bitmap"
                );
            }
            let mut vol = libpfs3::volume::Volume::from_device(Box::new(on.clone())).unwrap();
            assert!(
                vol.list_deldir().unwrap().is_empty(),
                "{total}: an empty deldir"
            );
        }
    }

    /// **ART-316/318, cards.** `NativeFormatter` formats PFS3 with pfs3aio's
    /// deldir on (the owner's decision, 2026-09-14), so a file deleted on the
    /// Amiga lands where it can be undeleted.
    #[test]
    fn native_pfs3_format_turns_the_deldir_on() {
        let (_guard, image) = formatted_pds3_image();
        let offset = partition_offset(&image) as usize;
        let raw = std::fs::read(&image).unwrap();
        let root = &raw[offset + 2 * 512..];
        assert_eq!(
            be32(root, 0x04) & (8 | 256),
            8 | 256,
            "MODE_DELDIR | MODE_SUPERDELDIR"
        );
        let ext = &raw[offset + be32(root, 0x58) as usize * 512..];
        assert_eq!(be16(ext, 0x36), 2, "deldirsize");
    }

    /// **ART-318.** A deleted file goes into the deldir as pfs3aio's
    /// `DeleteObject` puts it there (`directory.c:1799-1830,4489-4564`): the slot
    /// at `deldirroving`, which then advances; `anodenr`, `fsize`, the file's own
    /// date, a length byte and at most `DELENTRYFNSIZE - 1` name bytes — 17 in
    /// the build pfs3aio's `makefile:21` makes (`LARGE_FILE_SIZE=0`,
    /// `blocks.h:100-105`), the last two in what the struct calls `fsizex`; the
    /// file's data blocks freed and its anodes kept. `libpfs3`'s reader lists it
    /// by that name and at its real size, not reading those name bytes as size
    /// (`GetDDFileSize`, `directory.c:3688`).
    #[test]
    fn a_deleted_pfs3_file_goes_into_pfs3aios_deldir_entry() {
        let (_guard, image) = formatted_pds3_image();
        let offset = partition_offset(&image);
        let name = "A-long-deleted-name.txt"; // 23 bytes
        let (anode, date, free_before) = {
            let vol = libpfs3::volume::Volume::open_rw(&image, offset).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            w.write_file("Short", b"s").unwrap();
            let free_before = w.vol.free_blocks();
            w.write_file(name, &[0xC3u8; 700]).unwrap(); // two data blocks
            let e = w.vol.lookup(name).unwrap().unwrap();
            let date = [e.creation_day, e.creation_minute, e.creation_tick]
                .iter()
                .flat_map(|v| v.to_be_bytes())
                .collect::<Vec<u8>>();
            w.delete(name).unwrap();
            (e.anode, date, free_before)
        };

        let (roving, entries) = raw_deldir(&image);
        assert_eq!(
            entries,
            vec![RawDelEntry {
                slot: 0,
                anodenr: anode,
                fsize: 700,
                date,
                name_len: 17,
                name: b"A-long-deleted-na".to_vec(),
                fsizex: u16::from_be_bytes(*b"na"),
            }],
            "the deldir entry, raw"
        );
        assert_eq!(roving, 1, "deldirroving advances past the slot it gave out");

        let mut vol = libpfs3::volume::Volume::open(&image, offset).unwrap();
        assert_eq!(
            vol.free_blocks(),
            free_before,
            "the file's two data blocks are free again"
        );
        let chain = vol.get_anode_chain(anode).unwrap();
        assert_eq!(
            chain.iter().map(|a| a.clustersize).sum::<u32>(),
            2,
            "its anodes are kept for undelete"
        );
        let listed: Vec<(String, u64)> = vol
            .list_deldir()
            .unwrap()
            .into_iter()
            .map(|e| (e.filename.clone(), e.file_size()))
            .collect();
        assert_eq!(
            listed,
            vec![("A-long-deleted-na".to_string(), 700)],
            "the reader's name, and its size without the name bytes in `fsizex`"
        );
    }

    /// **ART-318.** A two-block deldir holds 62 files; the 63rd delete takes slot
    /// 0 again, where `deldirroving` wrapped to, and the file it evicts loses its
    /// anodes — only its anodes, its blocks having been freed at its own delete
    /// (`AllocDeldirSlot`, `directory.c:4497-4518`).
    #[test]
    fn the_pfs3_deldir_roves_and_the_63rd_delete_evicts_slot_zero() {
        let (_guard, image) = formatted_pds3_image();
        let offset = partition_offset(&image);
        let mut anodes = Vec::new();
        {
            let vol = libpfs3::volume::Volume::open_rw(&image, offset).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            for i in 0..63 {
                let name = format!("F{i:02}");
                w.write_file(&name, name.as_bytes()).unwrap();
                anodes.push(w.vol.lookup(&name).unwrap().unwrap().anode);
                w.delete(&name).unwrap();
            }
        }

        let (roving, entries) = raw_deldir(&image);
        let at = |slot: usize| entries.iter().find(|e| e.slot == slot).unwrap();
        assert_eq!(entries.len(), 62, "a full deldir");
        assert_eq!(
            (at(0).anodenr, at(0).name.as_slice()),
            (anodes[62], &b"F62"[..]),
            "slot 0 holds the 63rd deleted file"
        );
        assert_eq!(
            at(1).name.as_slice(),
            &b"F01"[..],
            "slot 1 still holds the second"
        );
        assert_eq!(roving, 1, "deldirroving wrapped to 0 and advanced past it");

        let mut vol = libpfs3::volume::Volume::open(&image, offset).unwrap();
        let evicted = &vol.get_anode_chain(anodes[0]).unwrap()[0];
        assert_eq!(
            (evicted.clustersize, evicted.blocknr, evicted.next),
            (0, 0, 0),
            "F00's anode is freed when its slot is taken"
        );
    }

    /// **ART-318.** A deleted file undeletes while its freed blocks are
    /// untouched, and is refused once a later write has taken them — pfs3aio's
    /// `IsDelfileValid` (`directory.c:4092-4114`), needed because a delete now
    /// frees the blocks.
    #[test]
    fn a_deleted_pfs3_file_undeletes_until_its_blocks_are_reused() {
        let (_guard, image) = formatted_pds3_image();
        let offset = partition_offset(&image);
        let vol = libpfs3::volume::Volume::open_rw(&image, offset).unwrap();
        let mut w = libpfs3::writer::Writer::open(vol).unwrap();

        w.write_file("Keep", b"keep me").unwrap();
        w.delete("Keep").unwrap();
        w.undelete(0, "Kept").unwrap();
        assert_eq!(w.vol.read_file("Kept").unwrap(), b"keep me");

        w.write_file("Lose", b"lose me").unwrap();
        w.delete("Lose").unwrap(); // slot 1: roving moved on
        w.write_file("Taker", b"takes the freed block").unwrap();
        let err = w.undelete(1, "Lost").unwrap_err();
        assert!(
            matches!(&err, libpfs3::error::Error::NotFound(m) if m.contains("reused")),
            "{err}"
        );
        assert!(
            w.vol.lookup("Lost").unwrap().is_none(),
            "a refused undelete writes nothing"
        );
    }

    /// **ART-318.** A deldir block holds pfs3aio's 31 entries at every reserved
    /// block size (`blocks.h:611`). Every test volume here has 1024-byte
    /// reserved blocks, where the computed count is 31 anyway, so this is what
    /// guards 2048 and 4096 (0.1.3 computed 63 and 127).
    #[test]
    fn a_pfs3_deldir_block_holds_31_entries_at_every_reserved_block_size() {
        assert_eq!(
            [1024u16, 2048, 4096].map(libpfs3::ondisk::deldir_entries_per_block),
            [31, 31, 31]
        );
    }

    /// **M2 (final review).** `DelDirEntry::parse` must read `fsizex` as the
    /// size bits 32-47 only on a `MODE_LARGEFILE` volume — pfs3aio's
    /// `GetDDFileSize` ignores it otherwise (`directory.c:3688`), whatever the
    /// name's own length. Every volume ART formats is not `MODE_LARGEFILE`.
    /// The writer never puts bytes at the entry's 0x1E-0x1F on such a volume,
    /// but a reused slot on a long-used card can carry stale bytes there from
    /// an earlier, longer name; those bytes must never be read as size.
    #[test]
    fn pfs3_deldir_fsizex_is_ignored_off_a_non_largefile_volume() {
        let (_guard, image) = formatted_pds3_image();
        let offset = partition_offset(&image);
        {
            let vol = libpfs3::volume::Volume::open_rw(&image, offset).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            w.write_file("A", b"x").unwrap();
            w.delete("A").unwrap();
        }
        // Slot 0: the first deldir block's first entry, DELDIR_HEADER_SIZE (32)
        // into the block; the entry's own fsizex is at its relative 0x1E.
        {
            use std::io::{Seek, SeekFrom, Write};
            let raw = std::fs::read(&image).unwrap();
            let off = offset as usize;
            let root = &raw[off + 2 * 512..off + 2 * 512 + 512];
            let resblk = usize::from(be16(root, 0x40));
            let ext_blk = be32(root, 0x58) as usize;
            let ext = &raw[off + ext_blk * 512..off + ext_blk * 512 + resblk];
            let dd_blk = be32(ext, 0x90) as usize;
            let entry_off = off as u64 + dd_blk as u64 * 512 + 32 /* header */ + 0x1E;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .open(&image)
                .unwrap();
            file.seek(SeekFrom::Start(entry_off)).unwrap();
            file.write_all(&[0xFFu8, 0xFF]).unwrap();
        }

        let mut vol = libpfs3::volume::Volume::open(&image, offset).unwrap();
        let listed: Vec<(String, u64)> = vol
            .list_deldir()
            .unwrap()
            .into_iter()
            .map(|e| (e.filename.clone(), e.file_size()))
            .collect();
        assert_eq!(
            listed,
            vec![("A".to_string(), 1)],
            "stray bytes past a short name were read as size on a non-LARGEFILE volume"
        );
    }

    /// **M3 (final review).** `Writer::open` indexes fixed offsets that only
    /// fit a 1024/2048/4096-byte reserved block — the rootblock extension's
    /// superindex write reaches 0x80 (`update_rootblock`), a deldir block's
    /// entries reach byte 1024 (`move_to_deldir`, `DELENTRIES_PER_BLOCK`
    /// fixed at 31). A `reserved_blksize` between 64 (`Volume`'s own floor)
    /// and 1023 must be refused before any of those writes, not panic under
    /// `panic = "abort"`.
    #[test]
    fn pfs3_writer_open_refuses_an_unsupported_reserved_blksize() {
        let (_guard, image) = formatted_pds3_image();
        let offset = partition_offset(&image);
        {
            use std::io::{Seek, SeekFrom, Write};
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .open(&image)
                .unwrap();
            // Rootblock reserved_blksize at 0x40 (u16).
            file.seek(SeekFrom::Start(offset + 2 * 512 + 0x40)).unwrap();
            file.write_all(&512u16.to_be_bytes()).unwrap();
        }
        let vol = libpfs3::volume::Volume::open_rw(&image, offset).unwrap();
        match libpfs3::writer::Writer::open(vol) {
            Ok(_) => panic!("expected a refusal for reserved_blksize 512"),
            Err(err) => assert!(
                matches!(err, libpfs3::error::Error::Corrupt(ref m) if m.contains("512")),
                "expected a typed refusal naming the unsupported reserved_blksize, got: {err}"
            ),
        }
    }

    // ---- ART-113: a non-ASCII name is refused before anything is written ----

    /// The exact real-world shape ART-113 found: a file whose AmigaDOS name
    /// carries an accented character. Must refuse before the volume is
    /// touched at all — proved here by hashing the whole image byte for
    /// byte, not merely by checking the error's type, which a version that
    /// refused only *after* writing something would still pass if it were
    /// checked less strictly.
    #[test]
    fn a_non_ascii_pfs3_file_name_is_refused_before_anything_is_written() {
        let (_guard, image) = formatted_pds3_image();
        let before = std::fs::read(&image).unwrap();
        let (_guard, tree) = fixtures::scratch("copy-in-non-ascii-file");
        std::fs::write(tree.join("türkçe"), b"data").unwrap();

        let err = NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap_err();

        assert!(matches!(err, CoreError::NonAsciiPfs3Names { .. }), "{err}");
        assert!(format!("{err}").contains("türkçe"), "{err}");
        assert_eq!(
            std::fs::read(&image).unwrap(),
            before,
            "nothing was written — the volume was never even opened"
        );
    }

    /// The shape that actually mattered on the real tree: 24 of the 969
    /// excluded files/directories in ART-113's own measurement were
    /// **directories** (`Locale/Countries/türkçe`, and everything under it).
    /// A directory's name goes through `create_dir_in`, not
    /// `write_file_in` — a check that only ever looked at file entries would
    /// let this one straight through to `libpfs3` and miss the exact case
    /// that motivated the fix. The one file inside the bad directory is
    /// itself pure ASCII, so this also proves the parent's own name is what
    /// gets checked, not merely everything nested under it.
    #[test]
    fn a_non_ascii_pfs3_directory_name_is_refused_even_when_its_contents_are_ascii() {
        let (_guard, image) = formatted_pds3_image();
        let before = std::fs::read(&image).unwrap();
        let (_guard, tree) = fixtures::scratch("copy-in-non-ascii-dir");
        std::fs::create_dir_all(tree.join("español")).unwrap();
        std::fs::write(tree.join("español").join("Readme"), b"data").unwrap();

        let err = NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap_err();

        let CoreError::NonAsciiPfs3Names { paths, more } = err else {
            panic!("expected NonAsciiPfs3Names, got {err}");
        };
        assert_eq!(paths, vec!["español".to_string()]);
        assert_eq!(more, 0);
        assert_eq!(
            std::fs::read(&image).unwrap(),
            before,
            "nothing was written"
        );
    }

    /// The bound, falsified directly: more offending names than
    /// `MAX_NAMED_NON_ASCII` must still name exactly that many and fold the
    /// rest into `more`, rather than either naming all of them (an
    /// unbounded, arbitrarily long refusal) or silently truncating without
    /// saying how many were left out.
    #[test]
    fn more_offending_names_than_the_bound_are_folded_into_a_count() {
        let (_guard, image) = formatted_pds3_image();
        let (_guard, tree) = fixtures::scratch("copy-in-non-ascii-many");
        let total = MAX_NAMED_NON_ASCII + 5;
        for i in 0..total {
            std::fs::write(tree.join(format!("é{i}")), b"x").unwrap();
        }

        let err = NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap_err();

        let CoreError::NonAsciiPfs3Names { paths, more } = err else {
            panic!("expected NonAsciiPfs3Names, got {err}");
        };
        assert_eq!(paths.len(), MAX_NAMED_NON_ASCII, "{paths:?}");
        assert_eq!(more, 5, "the 5 that did not fit the bound");
    }

    /// FFS is unaffected — ART's own writer encodes these names correctly,
    /// so the same tree that PFS3 refuses must copy in cleanly here. Without
    /// this, a version of the check that (by a routing bug) also ran on the
    /// FFS branch would still pass every PFS3-only test above.
    #[test]
    fn the_same_non_ascii_name_copies_in_fine_on_ffs() {
        let (_guard, image) = rdb_image_with_one_dos3_partition();
        NativeFormatter::UTC
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();
        let (_guard, tree) = fixtures::scratch("copy-in-non-ascii-ffs");
        std::fs::write(tree.join("türkçe"), b"data").unwrap();

        let summary = NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .expect("FFS has no such encoding mismatch to refuse");
        assert_eq!(summary.files, 1);
    }

    // ---- ART-116: a dropped comment or date is counted, not hidden ----

    /// A sidecar carrying both a real comment and a real (non-epoch) date —
    /// `libpfs3` 0.1.3 has a setter for neither, so both must be counted.
    /// Falsification: a version that only ever set `comments_lost` (or only
    /// `dates_lost`) would still pass a test that checked one field alone;
    /// this checks both from the one sidecar in one assertion each.
    #[test]
    fn copy_in_pfs3_counts_a_dropped_comment_and_a_dropped_date() {
        let (_guard, image) = formatted_pds3_image();
        let (_guard, tree) = fixtures::scratch("copy-in-lost-metadata");
        std::fs::write(tree.join("Assign"), b"x").unwrap();
        std::fs::write(
            tree.join("Assign.uaem"),
            "--p-rwed 2021-04-13 02:43:13.68 a real comment\n",
        )
        .unwrap();

        let summary = NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();

        assert_eq!(summary.comments_lost, 1);
        assert_eq!(summary.dates_lost, 1);
    }

    /// The negative control: a sidecar that only carries protection bits
    /// (empty comment, the Amiga epoch as its date — `sidecar_for`'s own
    /// "nothing worth recording" case for those two fields) must not be
    /// counted as having lost either. Without this, a version that counted
    /// every sidecar unconditionally would still pass the test above.
    #[test]
    fn copy_in_pfs3_does_not_count_a_sidecar_with_no_comment_or_date_to_lose() {
        let (_guard, image) = formatted_pds3_image();
        let (_guard, tree) = fixtures::scratch("copy-in-nothing-lost");
        std::fs::write(tree.join("Assign"), b"x").unwrap();
        std::fs::write(
            tree.join("Assign.uaem"),
            "--p-rwed 1978-01-01 00:00:00.00 \n",
        )
        .unwrap();

        let summary = NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();

        assert_eq!(summary.comments_lost, 0);
        assert_eq!(summary.dates_lost, 0);
    }

    /// The FFS branch keeps both fields — see `FileMeta` in `copy_in_ffs` —
    /// so it must never report either as lost, even for the exact sidecar
    /// that trips both counters on PFS3 above. Without this, a bug that
    /// counted on both branches (rather than only where the loss is real)
    /// would still pass every PFS3-only test above.
    #[test]
    fn copy_in_ffs_never_counts_anything_lost() {
        let (_guard, image) = rdb_image_with_one_dos3_partition();
        NativeFormatter::UTC
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();
        let (_guard, tree) = fixtures::scratch("copy-in-ffs-nothing-lost");
        std::fs::write(tree.join("Assign"), b"x").unwrap();
        std::fs::write(
            tree.join("Assign.uaem"),
            "--p-rwed 2021-04-13 02:43:13.68 a real comment\n",
        )
        .unwrap();

        let summary = NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();

        assert_eq!(summary.comments_lost, 0);
        assert_eq!(summary.dates_lost, 0);
    }

    #[test]
    fn it_stops_between_files_when_cancelled() {
        let (_guard, image) = formatted_pds3_image();
        let (_guard, tree) = fixtures::scratch("copy-in-cancel");
        std::fs::create_dir_all(tree.join("C")).unwrap();
        std::fs::write(tree.join("C/Assign"), b"x").unwrap();

        let err = NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &tree, &fixtures::CancelAfter::new(1))
            .unwrap_err();
        assert!(matches!(err, CoreError::Cancelled), "{err}");

        // And nothing landed: cancellation stopped before "Assign" was written.
        let mut vol = libpfs3::volume::Volume::open(&image, partition_offset(&image)).unwrap();
        assert!(vol.list_dir("C").unwrap().is_empty());
    }

    #[test]
    fn a_tree_too_big_for_the_volume_is_refused_before_anything_is_written() {
        let (_guard, image) = formatted_pds3_image_of(4); // MB
        let (_guard, tree) = tree_of_bytes(8 * 1024 * 1024);
        let before = std::fs::read(&image).unwrap();

        let err = NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap_err();
        assert!(
            format!("{err}").contains("8388608"),
            "the refusal carries the real numbers: {err}"
        );
        assert_eq!(
            std::fs::read(&image).unwrap(),
            before,
            "nothing was written"
        );
    }

    // ---- fix round 1, item 1: the fit check must not fail open ----
    //
    // The original check summed raw file bytes against whole-block free
    // space, so a tree that was merely close to full — needing one more
    // block than it had, rather than one more byte — passed the check and
    // then failed partway through the real copy. These two pin the corrected
    // arithmetic at its actual boundary, in bytes, on a real formatted
    // volume, not a synthetic number.

    #[test]
    fn a_pfs3_tree_that_exactly_fills_the_volume_is_not_refused() {
        let (_guard, image) = formatted_pds3_image_of(4);
        let free = pfs3_free_bytes(&image);
        let (_guard, tree) = tree_of_bytes(free as usize);

        NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .expect("a tree that exactly fits must not be refused");
    }

    #[test]
    fn a_pfs3_tree_one_byte_over_the_limit_is_refused() {
        let (_guard, image) = formatted_pds3_image_of(4);
        let free = pfs3_free_bytes(&image);
        let (_guard, tree) = tree_of_bytes(free as usize + 1);
        let before = std::fs::read(&image).unwrap();

        let err = NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap_err();
        assert_eq!(err.code(), "ART-INPUT-INVALID", "{err}");
        assert_eq!(
            std::fs::read(&image).unwrap(),
            before,
            "nothing was written"
        );
    }

    /// The exact scenario fix round 1's review found: many small files whose
    /// *raw* byte total is tiny but whose *block-rounded* total is not —
    /// PFS3 allocates one whole data block per file regardless of how small
    /// it is. The old check summed raw bytes and would have accepted this
    /// tree outright (a few kilobytes against a formatted volume); the fixed
    /// one refuses it before anything is written.
    #[test]
    fn many_small_pfs3_files_are_refused_by_their_rounded_size_not_their_raw_bytes() {
        let (_guard, image) = formatted_pds3_image_of(1);
        // One block per one-byte file, and comfortably more files than the
        // volume has free *blocks* — while their raw bytes (one each) are
        // nowhere near its free *bytes*. That gap is exactly what the old
        // check missed.
        let block_size = crate::core::volume::SECTOR_BYTES as u64;
        let file_count = pfs3_free_bytes(&image) / block_size + 100;

        let (_guard, tree) = fixtures::scratch("copy-in-many-small-pfs3");
        for i in 0..file_count {
            std::fs::write(tree.join(format!("F{i}")), b"x").unwrap();
        }
        let before = std::fs::read(&image).unwrap();

        let err = NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap_err();
        assert_eq!(err.code(), "ART-INPUT-INVALID", "{err}");
        assert_eq!(
            std::fs::read(&image).unwrap(),
            before,
            "nothing was written"
        );
    }

    #[test]
    fn an_ffs_tree_that_exactly_fills_the_volume_is_not_refused() {
        let (_guard, image) = rdb_image_with_one_dos3_partition();
        NativeFormatter::UTC
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();
        let free = ffs_free_bytes(&image);
        let block_size = crate::core::volume::SECTOR_BYTES;
        let data_blocks = largest_single_ffs_file_data_blocks(free / block_size as u64, block_size);
        let (_guard, tree) = tree_of_bytes(data_blocks as usize * block_size);

        NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .expect("a tree that exactly fits must not be refused");
    }

    #[test]
    fn an_ffs_tree_one_block_over_the_limit_is_refused() {
        let (_guard, image) = rdb_image_with_one_dos3_partition();
        NativeFormatter::UTC
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();
        let free = ffs_free_bytes(&image);
        let block_size = crate::core::volume::SECTOR_BYTES;
        let data_blocks = largest_single_ffs_file_data_blocks(free / block_size as u64, block_size);
        // One byte into the next data block: `budget_for` needs one more
        // data block (and, past the 72-block mark, sometimes one more
        // extension block too) — either way, strictly more than fits.
        let (_guard, tree) = tree_of_bytes(data_blocks as usize * block_size + 1);
        let before = std::fs::read(&image).unwrap();

        let err = NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap_err();
        assert_eq!(err.code(), "ART-INPUT-INVALID", "{err}");
        assert_eq!(
            std::fs::read(&image).unwrap(),
            before,
            "nothing was written"
        );
    }

    // ---- fix round 1, item 2: the version pin cannot silently drift ----
    // ---- ART-310: and it names the vendored, patched copy ----

    #[test]
    fn the_pinned_version_constant_matches_cargo_toml() {
        let cargo_toml = include_str!("../../../Cargo.toml");
        assert!(
            cargo_toml.contains("libpfs3 = \"=0.1.3\""),
            "Cargo.toml must still pin the crates.io release the vendored copy was taken from"
        );
        // Two separate checks, not one string with a newline in it: a Windows
        // checkout may carry CRLF.
        assert!(
            cargo_toml.contains("[patch.crates-io]")
                && cargo_toml.contains("libpfs3 = { path = \"vendor/libpfs3\" }"),
            "Cargo.toml must patch libpfs3 to the vendored copy (ART-310) — without it the \
             build silently goes back to 0.1.3's broken format"
        );
        let vendored = include_str!("../../../vendor/libpfs3/Cargo.toml");
        let expected = format!("version = \"{LIBPFS3_VERSION}\"");
        assert!(
            vendored.contains(&expected),
            "the vendored libpfs3's version no longer matches LIBPFS3_VERSION \
             ({LIBPFS3_VERSION}) — update the constant (and what probe() claims) together \
             with the vendored copy"
        );
    }

    // ---- fix round 1, item 3: a directory's own sidecar must not be lost ----

    #[test]
    fn a_directorys_own_sidecar_is_applied_on_pfs3() {
        let (_guard, image) = formatted_pds3_image();
        let (_guard, tree) = fixtures::scratch("copy-in-dir-sidecar-pfs3");
        std::fs::create_dir_all(tree.join("C")).unwrap();
        std::fs::write(tree.join("C.uaem"), "--p-rwed 2021-04-13 02:43:13.68 \n").unwrap();

        NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();

        let mut vol = libpfs3::volume::Volume::open(&image, partition_offset(&image)).unwrap();
        let entry = vol
            .list_dir("")
            .unwrap()
            .into_iter()
            .find(|e| e.name == "C")
            .unwrap();
        assert_eq!(
            libpfs3::util::amiga_protection_string(entry.protection),
            "--p-rwed"
        );
    }

    #[test]
    fn a_directorys_own_sidecar_is_applied_on_ffs() {
        let (_guard, image) = rdb_image_with_one_dos3_partition();
        NativeFormatter::UTC
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();
        let (_guard, tree) = fixtures::scratch("copy-in-dir-sidecar-ffs");
        std::fs::create_dir_all(tree.join("C")).unwrap();
        std::fs::write(tree.join("C.uaem"), "--p-rwed 2021-04-13 02:43:13.68 \n").unwrap();

        NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();

        let (mut region, geometry, offset) = ffs_region(&image);
        let writer = VolumeWriter::open(&mut region, geometry, &image, offset).unwrap();
        let c_dir = writer.find(0, "C").unwrap().unwrap();
        let attrs = writer.attributes(c_dir.block).unwrap();
        assert_eq!(uaem::format_bits(attrs.protection), "--p-rwed");
    }

    // ---- fix round 1, item 4: requirement 1's FFS counterpart ----

    #[test]
    fn ffs_copy_in_carries_the_protection_bits_out_of_the_uaem_sidecars() {
        let (_guard, image) = rdb_image_with_one_dos3_partition();
        NativeFormatter::UTC
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();
        let (_guard, tree) = fixtures::scratch("ffs-copy-in-protection");
        std::fs::create_dir_all(tree.join("C")).unwrap();
        std::fs::write(tree.join("C/Assign"), b"x").unwrap();
        std::fs::write(
            tree.join("C/Assign.uaem"),
            "--p-rwed 2021-04-13 02:43:13.68 \n",
        )
        .unwrap();

        NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();

        // ART's own reader, not libpfs3 — this is the FFS branch.
        let (mut region, geometry, offset) = ffs_region(&image);
        let writer = VolumeWriter::open(&mut region, geometry, &image, offset).unwrap();
        let c_dir = writer.find(0, "C").unwrap().unwrap();
        let assign = writer.find(c_dir.block, "Assign").unwrap().unwrap();
        let attrs = writer.attributes(assign.block).unwrap();
        assert_eq!(uaem::format_bits(attrs.protection), "--p-rwed");
    }

    #[test]
    fn ffs_a_sidecar_is_applied_and_never_copied_as_a_file_of_its_own() {
        let (_guard, image) = rdb_image_with_one_dos3_partition();
        NativeFormatter::UTC
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();
        let (_guard, tree) = fixtures::scratch("ffs-copy-in-sidecar-not-a-file");
        std::fs::create_dir_all(tree.join("C")).unwrap();
        std::fs::write(tree.join("C/Assign"), b"x").unwrap();
        std::fs::write(
            tree.join("C/Assign.uaem"),
            "--p-rwed 2021-04-13 02:43:13.68 \n",
        )
        .unwrap();

        NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();

        let (mut region, geometry, offset) = ffs_region(&image);
        let writer = VolumeWriter::open(&mut region, geometry, &image, offset).unwrap();
        let c_dir = writer.find(0, "C").unwrap().unwrap();
        let listing = writer.list(c_dir.block).unwrap();
        assert!(listing.iter().all(|e| !e.name.ends_with(".uaem")));
    }

    // ---- coverage for binding requirements the given tests do not reach ----

    /// Requirement 2's checked conversion, exercised directly rather than
    /// only through `uaem::parse` — which, by construction, can never itself
    /// produce a value above `0xFF`. An `as u8` cast in place of `try_from`
    /// would pass every test above (their sidecars are always in range) and
    /// only this one would catch it.
    #[test]
    fn the_pfs3_protection_narrowing_refuses_what_does_not_fit_a_byte() {
        assert_eq!(pfs3_protection(0xFF).unwrap(), 0xFF);
        assert!(pfs3_protection(0x1_00).is_err());
        assert!(pfs3_protection(u32::MAX).is_err());
    }

    /// Requirement 3: the engine validates the name itself, not a screen.
    #[test]
    fn format_partition_refuses_a_name_amigados_cannot_store() {
        let (_guard, image) = rdb_image_with_one_pds3_partition();
        let err = NativeFormatter::UTC
            .format_partition(&image, None, 1, "Not/Allowed", &NoProgress)
            .unwrap_err();
        assert_eq!(err.code(), "ART-INPUT-INVALID", "{err}");
    }

    /// Requirement 4, for the case none of the given tests reach: a *second*
    /// `copy_in` against a volume that already has content from the first —
    /// with no `format_partition` between them — must be refused rather than
    /// half-supported. A version of `copy_in` that skipped the `list_dir`
    /// check would pass every test above (each formats fresh) and only fail
    /// here.
    #[test]
    fn copy_in_refuses_to_fill_an_already_populated_pfs3_volume_again() {
        let (_guard, image) = formatted_pds3_image();
        let (_guard, tree) = fixtures::scratch("copy-in-populated-pfs3");
        std::fs::write(tree.join("First"), b"one").unwrap();
        NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();

        let (_guard, second) = fixtures::scratch("copy-in-populated-pfs3-second");
        std::fs::write(second.join("Second"), b"two").unwrap();
        let err = NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &second, &NoProgress)
            .unwrap_err();
        assert_eq!(err.code(), "ART-SAFETY-REFUSED", "{err}");
    }

    /// The FFS equivalent of the same requirement.
    #[test]
    fn copy_in_refuses_to_fill_an_already_populated_ffs_volume_again() {
        let (_guard, image) = rdb_image_with_one_dos3_partition();
        NativeFormatter::UTC
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();
        let (_guard, tree) = fixtures::scratch("copy-in-populated-ffs");
        std::fs::write(tree.join("First"), b"one").unwrap();
        NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();

        let (_guard, second) = fixtures::scratch("copy-in-populated-ffs-second");
        std::fs::write(second.join("Second"), b"two").unwrap();
        let err = NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &second, &NoProgress)
            .unwrap_err();
        assert_eq!(err.code(), "ART-SAFETY-REFUSED", "{err}");
    }

    /// `scan_image` reports the RDB's own drive name, not the AmigaDOS volume
    /// label — so `it_formats_an_ffs_partition_with_arts_own_writer` cannot
    /// by itself prove the label this module writes into the root block is
    /// right. This reads the root block directly.
    #[test]
    fn format_ffs_writes_the_requested_volume_name_into_the_root_block() {
        let (_guard, image) = rdb_image_with_one_dos3_partition();
        NativeFormatter::UTC
            .format_partition(&image, None, 1, "MyDisk", &NoProgress)
            .unwrap();

        let card = read_card(&image).unwrap();
        let part = &card.areas[0].rdb.partitions[0];
        let offset = card.areas[0].offset_bytes + part.byte_offset().unwrap();
        let block_size = part.block_bytes() as usize;
        let total_blocks = (part.byte_length().unwrap() / block_size as u64) as u32;
        let root_block = VolumeGeometry::root_block_for(total_blocks);

        let bytes = std::fs::read(&image).unwrap();
        let root_off = (offset as usize) + root_block as usize * block_size;
        let name =
            crate::core::adf::bcpl::read_bcpl_string(&bytes[root_off..root_off + block_size], 432)
                .unwrap();
        assert_eq!(name, "MyDisk");
    }

    /// A stronger proof than "the RDB still parses": a fresh FFS format is
    /// genuinely usable by ART's own writer for a real operation, not just
    /// structurally present.
    #[test]
    fn a_freshly_formatted_ffs_volume_accepts_a_real_write() {
        let (_guard, image) = rdb_image_with_one_dos3_partition();
        NativeFormatter::UTC
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();

        let (mut region, geometry, offset) = ffs_region(&image);
        let mut writer = VolumeWriter::open(&mut region, geometry, &image, offset).unwrap();

        assert!(writer.list(0).unwrap().is_empty());
        let outcome = writer
            .add_file(0, "Test.txt", b"hello", FileMeta::default())
            .unwrap();
        assert!(outcome.verified);
        assert_eq!(writer.read_file(outcome.block.unwrap()).unwrap(), b"hello");
    }

    /// `probe` names which implementation did the work.
    #[test]
    fn probe_names_libpfs3() {
        let probed = NativeFormatter::UTC.probe().unwrap();
        assert_eq!(probed.raw, "libpfs3 0.1.3+art.9 (native, no external tool)");
    }

    /// A DosType neither family claims — ART refuses rather than guessing.
    #[test]
    fn format_partition_refuses_a_filesystem_neither_family_claims() {
        let (_guard, image) = card_with_partition("sfs", AmigaHardDiskFs::Sfs0, 8);
        let err = NativeFormatter::UTC
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap_err();
        assert_eq!(err.code(), "ART-FORMAT-UNSUPPORTED", "{err}");
    }

    // -----------------------------------------------------------------------
    // Task 11: the PFS3 oracle, both directions (`scripts/pfs3-oracle-check.py`)
    //
    // A reader and a writer that agree only with each other is the shape
    // ART-032 … ART-035, ART-075 and ART-079 all were. `libpfs3` is both the
    // writer this module drives and the reader `core/osinstall/verify.rs`
    // uses, so ART cannot close that gap on its own — these two hooks are
    // what let `hst-imager`, a C# implementation sharing no code with ART,
    // stand in as the outside witness.
    // -----------------------------------------------------------------------

    /// **ART writes, `hst-imager` reads.** Builds a PFS3 volume end to end
    /// through `NativeFormatter` — the same `format_partition` then
    /// `copy_in` calls G5 makes — and prints a JSON description of every
    /// entry so `hst-imager` can be checked against a claim rather than
    /// against ART's own opinion of what it wrote: `fs dir -r` for name,
    /// kind, size and protection, and `fs copy` extracting the volume back
    /// out for the bytes themselves (the JSON's `sha256` is computed here
    /// from the exact literal each file was written from, so what the
    /// script hashes on the extracted side is checked against ART's write
    /// input, not against anything ART read back through its own reader).
    ///
    /// Protection bits are the point, not an extra: `C/Assign` carries the
    /// Pure bit (`--p-rwed`) and `C/Startup-Sequence` the Script bit
    /// (`-s--rwed`) — Task 11's brief calls out the Pure bit specifically,
    /// because AmigaOS 3.2's `Startup-Sequence` runs `Resident C:Assign
    /// PURE` and that bit arriving is the reason this whole phase exists. The
    /// attribute strings are `hst-imager`'s own spelling — `HSPARWED`,
    /// uppercase for granted — which is `uaem::format_bits`'s lowercase
    /// output uppercased; the two encodings agree bit for bit
    /// (`pfs3_protection`'s doc comment), confirmed against a real
    /// `hst-imager 1.6.616` run rather than assumed.
    ///
    /// ```text
    /// ART_PFS3_WRITE_OUT=... cargo test build_pfs3_volume_for_oracle_when_asked -- --nocapture
    /// ```
    #[test]
    fn build_pfs3_volume_for_oracle_when_asked() {
        let Ok(target) = std::env::var("ART_PFS3_WRITE_OUT") else {
            return;
        };
        write_pfs3_volume_for_oracle(&PathBuf::from(&target), 220 * 1024 * 1024, 200);
    }

    /// **ART-310.** The same volume and the same JSON claim as
    /// `build_pfs3_volume_for_oracle_when_asked`, on a partition past
    /// MAXSMALLDISK (5 100 MiB, 10 444 896 blocks) — SUPERINDEX mode, the mode
    /// `libpfs3` 0.1.3's format wrote wrong.
    ///
    /// ```text
    /// ART_PFS3_WRITE_OUT_LARGE=... cargo test build_large_pfs3_volume_for_oracle_when_asked -- --nocapture
    /// ```
    #[test]
    fn build_large_pfs3_volume_for_oracle_when_asked() {
        let Ok(target) = std::env::var("ART_PFS3_WRITE_OUT_LARGE") else {
            return;
        };
        write_pfs3_volume_for_oracle(&PathBuf::from(&target), 5_200 * 1024 * 1024, 5_100);
    }

    /// **ART-310, the oracle's anode question.** Prints the anode number of one
    /// entry on a PFS3 volume, so `pfs3-oracle-check.py` can ask whether
    /// hst-imager — whose allocator is a port of pfs3aio's — handed a new
    /// directory one of the numbers pfs3aio reserves (0–4).
    ///
    /// ```text
    /// ART_PFS3_ANODE_IN=<image> ART_PFS3_ANODE_PATH=<path> cargo test pfs3_anode_for_oracle_when_asked -- --nocapture
    /// ```
    #[test]
    fn pfs3_anode_for_oracle_when_asked() {
        let (Ok(source), Ok(path)) = (
            std::env::var("ART_PFS3_ANODE_IN"),
            std::env::var("ART_PFS3_ANODE_PATH"),
        ) else {
            return;
        };
        let image = PathBuf::from(&source);
        let mut vol = libpfs3::volume::Volume::open(&image, partition_offset(&image)).unwrap();
        let entry = vol
            .lookup(&path)
            .unwrap()
            .unwrap_or_else(|| panic!("{path} is not on the volume"));
        println!("anode={}", entry.anode);
    }

    /// The body both write hooks share: an RDB image of `disk_bytes` with one
    /// PDS\3 partition of `partition_mb`, formatted and filled through the same
    /// two calls G5 makes, then the JSON of every entry it believes it wrote.
    fn write_pfs3_volume_for_oracle(image: &Path, disk_bytes: u64, partition_mb: u32) {
        if let Some(parent) = image.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        crate::core::hdf::create_hdf(
            image,
            disk_bytes,
            true,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::Pfs3DirectScsi,
                size_mb: partition_mb,
                bootable: true,
                boot_priority: 0,
                num_buffers: 0,
            }],
            &[],
        )
        .unwrap();

        NativeFormatter::UTC
            .format_partition(image, None, 1, "Workbench", &NoProgress)
            .unwrap();

        // The literal bytes are named once and reused for both the write and
        // the JSON claim below, so a size or hash in the JSON can never drift
        // from what was actually handed to `copy_in`.
        let readme: &[u8] = b"hello from ART\n";
        let assign: &[u8] = b"ASSIGN\n";
        let startup: &[u8] = b"; a comment\n";
        let deep: &[u8] = b"deep\n";
        // ART-114: a Windows/MS-DOS reserved device basename, exercised for
        // real rather than left as an untested skip-path in the oracle
        // script. Not invented — `AUX` is the real case: `Storage3.2.adf` and
        // `GlowIcons3.2.adf` each carry a genuine `DOSDrivers/AUX` serial-port
        // DOSDriver definition (see ART-113's own doc comment for where those
        // two ADFs come from). `hst-imager fs copy` cannot extract this name
        // back out to NTFS — that is what the oracle script's own
        // `path_has_reserved_component` exists to recognise as a known,
        // explained skip rather than an unexplained shortfall — but ART
        // itself writes and lists it on the volume with nothing special
        // about it: the reserved-name problem is Windows extraction's, not
        // this volume's.
        let aux: &[u8] = b"DOSDriver AUX\n";

        let (_guard, tree) = fixtures::scratch("pfs3-oracle-write");
        std::fs::create_dir_all(tree.join("C/Extra")).unwrap();
        std::fs::create_dir_all(tree.join("S")).unwrap();
        std::fs::create_dir_all(tree.join("DOSDrivers")).unwrap();
        std::fs::write(tree.join("Readme"), readme).unwrap();
        std::fs::write(tree.join("C/Assign"), assign).unwrap();
        std::fs::write(
            tree.join("C/Assign.uaem"),
            "--p-rwed 2021-04-13 02:43:13.68 kept by ART\n",
        )
        .unwrap();
        std::fs::write(tree.join("C/Startup-Sequence"), startup).unwrap();
        std::fs::write(
            tree.join("C/Startup-Sequence.uaem"),
            "-s--rwed 2021-04-13 02:43:13.68\n",
        )
        .unwrap();
        std::fs::write(tree.join("C/Extra/Deep.txt"), deep).unwrap();
        std::fs::write(tree.join("DOSDrivers/AUX"), aux).unwrap();

        NativeFormatter::UTC
            .copy_in(image, None, "DH0", &tree, &NoProgress)
            .unwrap();

        // What a real `hst-imager fs dir -r` is expected to show — verified
        // by hand against `hst-imager 1.6.616` on this exact tree before
        // this assertion was written, not guessed at: forward slashes in
        // nested paths, `----RWED` for a file with no sidecar, `--P-RWED`
        // for the Pure bit, `-S--RWED` for the Script bit. `sha256` is only
        // meaningful for files; directories omit it.
        let entries = serde_json::json!([
            {"path": "Readme", "kind": "file", "size": readme.len(), "attributes": "----RWED",
             "sha256": crate::core::hashing::sha256_bytes(readme)},
            {"path": "C", "kind": "dir", "attributes": "----RWED"},
            {"path": "C/Assign", "kind": "file", "size": assign.len(), "attributes": "--P-RWED",
             "sha256": crate::core::hashing::sha256_bytes(assign)},
            {"path": "C/Startup-Sequence", "kind": "file", "size": startup.len(), "attributes": "-S--RWED",
             "sha256": crate::core::hashing::sha256_bytes(startup)},
            {"path": "C/Extra", "kind": "dir", "attributes": "----RWED"},
            {"path": "C/Extra/Deep.txt", "kind": "file", "size": deep.len(), "attributes": "----RWED",
             "sha256": crate::core::hashing::sha256_bytes(deep)},
            {"path": "S", "kind": "dir", "attributes": "----RWED"},
            {"path": "DOSDrivers", "kind": "dir", "attributes": "----RWED"},
            {"path": "DOSDrivers/AUX", "kind": "file", "size": aux.len(), "attributes": "----RWED",
             "sha256": crate::core::hashing::sha256_bytes(aux)},
        ]);
        println!("json={entries}");
    }

    /// **`hst-imager` writes, ART reads** — the other half, and the one that
    /// catches the hard bugs (Task 11's brief): ART-079 gave every file
    /// exactly the right *length* and another file's bytes, so this prints a
    /// SHA-256 per entry rather than a length.
    ///
    /// Reads through the same path `core/osinstall/verify.rs` uses —
    /// `read_card` for the RDB, then `libpfs3::volume::Volume::open` for the
    /// filesystem — so what is being proved is that ART's reader can make
    /// sense of a volume an independent writer produced, not merely that it
    /// agrees with itself.
    ///
    /// ```text
    /// ART_PFS3_READ_IN=... cargo test read_foreign_pfs3_for_oracle_when_asked -- --nocapture
    /// ```
    #[test]
    fn read_foreign_pfs3_for_oracle_when_asked() {
        let Ok(source) = std::env::var("ART_PFS3_READ_IN") else {
            return;
        };
        let image = PathBuf::from(&source);

        // The RDB is read with ART's own parser regardless of who wrote it —
        // a plain image from `hst.imager blank` + `format Rdb PDS3` has its
        // RDB at byte zero, the same `slot: None` convention `mod.rs` and
        // `format_partition` already use for a plain HDF.
        let card = read_card(&image).unwrap();
        let area = area_for_slot(&card, None).unwrap();
        let part = partition_by_index(area, 1).unwrap();
        let (offset, _length, _block_size) = partition_region(area, part).unwrap();

        let mut vol = libpfs3::volume::Volume::open(&image, offset).unwrap();
        println!("volume={}", vol.name());

        fn walk(vol: &mut libpfs3::volume::Volume, prefix: &str) {
            let mut entries = vol.list_dir(prefix).unwrap();
            entries.sort_by(|a, b| a.name.cmp(&b.name));
            for entry in entries {
                let path = if prefix.is_empty() {
                    entry.name.clone()
                } else {
                    format!("{prefix}/{}", entry.name)
                };
                let attrs = uaem::format_bits(u32::from(entry.protection)).to_uppercase();
                if entry.is_dir() {
                    println!("entry={path}|dir|-|-|{attrs}");
                    walk(vol, &path);
                } else {
                    let data = vol.read_file(&path).unwrap();
                    let hash = crate::core::hashing::sha256_bytes(&data);
                    println!("entry={path}|file|{}|{hash}|{attrs}", data.len());
                }
            }
        }
        walk(&mut vol, "");
    }

    /// **Task 14 — the real run, step 2.** Puts the real `dist-3.2` tree
    /// `core::osinstall::apply` built from the user's own media
    /// (`run_the_real_engine_against_the_users_own_media_when_asked` in
    /// `core/osinstall/apply.rs`) onto **a fresh card image ART itself
    /// creates here** — never the user's own hardware card, and nothing
    /// under `E:\amiga\Amigatolon` is opened for writing. `hst.imager fs
    /// dir` and `python scripts/pfs3-oracle-check.py` are the independent
    /// witnesses named in the brief; this hook only builds the volume and
    /// prints where it landed, so a human (or the report) can point both at
    /// it afterwards.
    ///
    /// Same `format_partition` then `copy_in` shape the two oracle hooks
    /// above already use, and the same shape G5 uses in production —
    /// `copy_in`'s `source` is "a folder on the PC whose tree goes in"
    /// (`PreloadPartition::content`'s own doc comment), so the whole
    /// `dist-3.2` folder, `distribution.json` included, goes in exactly as
    /// a screen driving Preload at that folder would send it. The manifest
    /// riding along as an inert extra file on the volume is a real,
    /// observed consequence worth naming in the report, not a defect this
    /// hook works around.
    ///
    /// **`SAFE_CREATE`**, same as `core/card/build.rs`'s own `write_card`:
    /// `ART_CARD_OUT` is refused if it already exists rather than replaced —
    /// a test hook is not exempt from the convention it exercises, and
    /// silently deleting whatever an env var happens to name was review
    /// fix round 1's finding here.
    ///
    /// ```text
    /// ART_OSINSTALL_DEST="E:\amiga\ProjeART\dist-3.2-witness" \
    /// ART_CARD_OUT="E:\amiga\ProjeART\dist-3.2-witness-card.hdf" \
    /// cargo test build_the_real_dist_tree_onto_a_card_when_asked -- --nocapture --ignored
    /// ```
    ///
    /// `dist-3.2-witness` — not `dist-3.2` itself — because [ART-113](../../../../docs/ISSUES.md)
    /// blocks the full tree (point this at `dist-3.2` directly to reproduce
    /// that failure). The reduced tree was built by a one-off pass, not a
    /// committed script — see ART-113's own entry for the method — so it
    /// must already exist on disk before this test can put it on a card.
    #[test]
    #[ignore = "touches a real dist tree on disk and writes a multi-MB image; run explicitly"]
    fn build_the_real_dist_tree_onto_a_card_when_asked() {
        let (Ok(dist), Ok(card_out)) = (
            std::env::var("ART_OSINSTALL_DEST"),
            std::env::var("ART_CARD_OUT"),
        ) else {
            return;
        };
        let dist_root = PathBuf::from(&dist);
        let image = PathBuf::from(&card_out);
        if let Some(parent) = image.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        assert!(
            !image.exists(),
            "'{}' already exists — SAFE_CREATE: remove it yourself first, \
             or point ART_CARD_OUT somewhere new",
            image.display()
        );

        // 128 MB total / 100 MB partition: the real (unreduced) tree
        // measured ~12.3 MB on disk (`total_bytes` in
        // `run_the_real_engine_against_the_users_own_media_when_asked`),
        // comfortably inside a 100 MB PFS3 partition with room for
        // anode/bitmap overhead and future growth without re-measuring
        // this number.
        crate::core::hdf::create_hdf(
            &image,
            128 * 1024 * 1024,
            true,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::Pfs3DirectScsi,
                size_mb: 100,
                bootable: true,
                boot_priority: 0,
                num_buffers: 0,
            }],
            &[],
        )
        .unwrap();

        NativeFormatter::UTC
            .format_partition(&image, None, 1, "Workbench", &NoProgress)
            .unwrap();

        let summary = NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &dist_root, &NoProgress)
            .unwrap();

        // Machine-readable (`key=value`), the same convention the two PFS3
        // oracle hooks above use for their own `json=`/`volume=`/`entry=`
        // lines — a human or a future script can parse this rather than
        // re-deriving or hard-coding the counts.
        println!(
            "card={} files={} directories={} bytes={}",
            image.display(),
            summary.files,
            summary.directories,
            summary
                .bytes
                .map_or("not answered".into(), |n: u64| n.to_string())
        );

        assert!(summary.files > 0, "the real tree must not copy in empty");
    }

    static PLUS_THREE: crate::core::clock::FixedClock = crate::core::clock::FixedClock {
        now: 1_768_478_400,
        offset: 10_800,
    };

    /// ART-317 on PFS3: the root date the format writes and the entry date
    /// `copy_in` writes are the clock's local time (15:00 at 12:00 UTC, UTC+3).
    #[test]
    fn a_pfs3_format_and_copy_in_carry_the_clocks_local_time() {
        let (_guard, image) = rdb_image_with_one_pds3_partition();
        let formatter = NativeFormatter::new(&PLUS_THREE);
        formatter
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();
        let (_guard, tree) = fixtures::scratch("pfs3-local-time");
        std::fs::write(tree.join("Readme"), b"hello\n").unwrap();
        formatter
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();

        let mut vol = libpfs3::volume::Volume::open(&image, partition_offset(&image)).unwrap();
        let local = (17_546u16, 900u16, 0u16);
        let rext = vol.rootblock_ext.as_ref().expect("a rootblock extension");
        assert_eq!(rext.root_date, local, "root date");
        let entry = vol
            .list_dir("")
            .unwrap()
            .into_iter()
            .find(|e| e.name == "Readme")
            .unwrap();
        assert_eq!(
            (
                entry.creation_day,
                entry.creation_minute,
                entry.creation_tick
            ),
            local,
            "entry date"
        );
        let rb = &vol.rootblock;
        assert_eq!(
            (rb.creation_day, rb.creation_minute, rb.creation_tick),
            local,
            "rootblock date"
        );
        // format_partition turns the deldir on (ART-316); a new deldir block carries the
        // rootblock's date (pfs3aio NewDeldirBlock, `directory.c:4477-4479`).
        let raw = std::fs::read(&image).unwrap();
        let at = partition_offset(&image) as usize;
        let sector =
            |n: u32, len: usize| raw[at + n as usize * 512..at + n as usize * 512 + len].to_vec();
        let root = sector(2, 512);
        let resblk = usize::from(be16(&root, 0x40));
        let ext = sector(be32(&root, 0x58), resblk);
        let dd = sector(be32(&ext, 0x90), resblk);
        assert_eq!(
            (be16(&dd, 0x1A), be16(&dd, 0x1C), be16(&dd, 0x1E)),
            local,
            "deldir block date at format"
        );
    }

    /// ART-317, third debt round's survivor (c): `open_pfs3_writer` itself —
    /// not `copy_in_pfs3`'s per-entry `set_entry_date` refresh — is what is
    /// under test here, so this writes through the writer immediately after
    /// `open_pfs3_writer` hands it back, with no `set_entry_date` call of its
    /// own in between. Without the helper's own stamp, `write_file` would
    /// fall through to `entry_datestamp`'s `current_amiga_datestamp` default,
    /// which is UTC (`vendor/libpfs3/src/writer.rs`).
    #[test]
    fn open_pfs3_writer_stamps_the_clocks_local_date_before_any_write() {
        const TOTAL: u64 = 48_000;
        let dev = MemDevice::with_end(TOTAL);
        formatted_in_memory(&dev, TOTAL);
        let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();

        let mut writer = open_pfs3_writer(vol, &PLUS_THREE).unwrap();
        writer.write_file("A", b"one block").unwrap();
        let mut vol = writer.into_volume();

        let entry = vol
            .list_dir("")
            .unwrap()
            .into_iter()
            .find(|e| e.name == "A")
            .unwrap();
        assert_eq!(
            (
                entry.creation_day,
                entry.creation_minute,
                entry.creation_tick
            ),
            (17_546u16, 900u16, 0u16),
            "open_pfs3_writer must stamp PLUS_THREE's local date (15:00 at 12:00 UTC, \
             UTC+3) before the first write, not UTC's fallback"
        );
    }

    /// ART-317 on the PFS3 deldir. pfs3aio `directory.c` at 211f7f0 (read, not run):
    /// `AddToDeldir` copies the deleted entry's own date into the deldir entry
    /// (`:4546-4548`) and stamps the deldir block and `rext.dd_creation*` with
    /// `DateStamp()` (`:4556-4560`). So the entry keeps the file's date (wall time,
    /// never shifted), and the block and the extension carry the delete's own
    /// local time, from `Writer::set_entry_date`.
    #[test]
    fn a_pfs3_delete_keeps_the_files_date_and_stamps_the_deldir_with_the_clock() {
        let (_guard, image) = formatted_pds3_image();
        let offset = partition_offset(&image);
        let written = (17_546u16, 900u16, 0u16); // 2026-01-15 15:00, when the file was written
        let deleted = (17_727u16, 900u16, 0u16); // 2026-07-15 15:00, when it was deleted
        {
            let vol = libpfs3::volume::Volume::open_rw(&image, offset).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            w.set_entry_date(Some(written));
            w.write_file("Gone", b"bye").unwrap();
            w.set_entry_date(Some(deleted));
            w.delete("Gone").unwrap();
        }

        let (_, entries) = raw_deldir(&image);
        let entry = entries
            .iter()
            .find(|e| e.name == b"Gone")
            .expect("Gone is in the deldir");
        assert_eq!(
            (
                be16(&entry.date, 0),
                be16(&entry.date, 2),
                be16(&entry.date, 4)
            ),
            written,
            "deldir entry: the file's own date"
        );

        let raw = std::fs::read(&image).unwrap();
        let at = offset as usize;
        let sector =
            |n: u32, len: usize| raw[at + n as usize * 512..at + n as usize * 512 + len].to_vec();
        let root = sector(2, 512);
        let resblk = usize::from(be16(&root, 0x40));
        let ext = sector(be32(&root, 0x58), resblk);
        let dd = sector(be32(&ext, 0x90), resblk);
        assert_eq!(
            (be16(&dd, 0x1A), be16(&dd, 0x1C), be16(&dd, 0x1E)),
            deleted,
            "deldir block date"
        );
        assert_eq!(
            (be16(&ext, 0x88), be16(&ext, 0x8A), be16(&ext, 0x8C)),
            deleted,
            "rext.dd_creation"
        );
    }

    /// ART-317 on FFS: the formatted root and a copied file.
    #[test]
    fn an_ffs_format_and_copy_in_carry_the_clocks_local_time() {
        let (_guard, image) = rdb_image_with_one_dos3_partition();
        let formatter = NativeFormatter::new(&PLUS_THREE);
        formatter
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();
        let (_guard, tree) = fixtures::scratch("ffs-local-time");
        std::fs::write(tree.join("Readme"), b"hello\n").unwrap();
        // A sidecar date is wall time already: it must land unshifted (12:00), not 15:00.
        std::fs::write(tree.join("Old"), b"old\n").unwrap();
        std::fs::write(tree.join("Old.uaem"), b"----rwed 2026-01-15 12:00:00.00 \n").unwrap();
        formatter
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();

        let card = read_card(&image).unwrap();
        let area = &card.areas[0];
        let part = &area.rdb.partitions[0];
        let (offset, length, block_size) = partition_region(area, part).unwrap();
        let mut region = FileRegionMut::open(&image, offset, length, block_size).unwrap();
        let dos = DosType::new(part.dostype.to_be_bytes());
        let geometry =
            VolumeGeometry::new(block_size, region.total_blocks(), part.reserved, dos).unwrap();

        let mut root = vec![0u8; block_size];
        region.read_block(geometry.root_block, &mut root).unwrap();
        let word = |o: usize| u32::from_be_bytes(root[o..o + 4].try_into().unwrap());
        assert_eq!(
            (word(420), word(424), word(428)),
            (17_546, 900, 0),
            "root date"
        );

        let writer =
            VolumeWriter::open_with_clock(&mut region, geometry, &image, offset, &PLUS_THREE)
                .unwrap();
        let block = writer.find(0, "Readme").unwrap().unwrap().block;
        assert_eq!(
            writer.attributes(block).unwrap().date,
            AmigaDate {
                days: 17_546,
                mins: 900,
                ticks: 0
            },
            "file date"
        );
        let old = writer.find(0, "Old").unwrap().unwrap().block;
        assert_eq!(
            writer.attributes(old).unwrap().date,
            AmigaDate {
                days: 17_546,
                mins: 720,
                ticks: 0
            },
            ".uaem date, unshifted"
        );
    }

    // ---- ART-319: the writer returns to its last commit on error ----

    /// **ART-319.** `alloc_data_blocks` clears bitmap bits for a
    /// `write_file_in` that does not fit, one bitmap block at a time, staging
    /// every touched block into `pending_writes` before it runs out and
    /// returns `DiskFull` — without reducing `blocksfree`. Unless that
    /// partial attempt is discarded, the next commit flushes those stale
    /// writes: the on-disk bitmap then says fewer blocks are free than
    /// `blocksfree` claims, and a later allocation can hand out a block a
    /// still-listed file already uses. Research (`art319-pfs3aio-research.md`):
    /// pfs3aio checks `alloc_available` **before** touching the bitmap
    /// (`allocation.c:242-243`), and even its own unwind-on-partial-failure
    /// only *defers* the free to the next commit (`allocation.c:600-608,730`)
    /// rather than mutating the bitmap directly — this crate's writer has
    /// neither a check-first nor a deferred-free, so it discards the failed
    /// attempt instead.
    #[test]
    fn a_failed_pfs3_write_discards_its_partial_allocation() {
        const TOTAL: u64 = 48_000;
        let dev = MemDevice::with_end(TOTAL);
        formatted_in_memory(&dev, TOTAL);

        let free_before = {
            let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            vol.free_blocks()
        };

        {
            let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            let one_too_many = vec![0x5Au8; (free_before as usize + 1) * 512];
            let err = w.write_file("Big", &one_too_many).unwrap_err();
            assert!(
                matches!(err, libpfs3::error::Error::DiskFull(_)),
                "expected disk full, got: {err}"
            );

            // A committing write after the failed one.
            w.write_file("Small", b"kept").unwrap();
        }

        let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
        let bitmap_free = vol.bitmap_count_free().unwrap();
        assert_eq!(
            vol.rootblock.blocksfree, bitmap_free,
            "blocksfree disagrees with the on-disk data bitmap after a failed allocation"
        );
        assert_eq!(
            vol.rootblock.blocksfree,
            free_before - 1,
            "must be exactly the free count before the failed write, minus the small file's \
             one block"
        );
        assert_eq!(vol.read_file("Small").unwrap(), b"kept");
    }

    /// **ART-319.** `delete_in` stages a deldir entry (`move_to_deldir`)
    /// before `free_data_blocks` walks the file's own anode chain; if that
    /// chain is corrupt, `move_to_deldir`'s write already sits in
    /// `pending_writes` when the error comes back. Unless it is discarded,
    /// the next commit flushes it — the file ends up listed both in its
    /// directory and in the deldir, and a later eviction of that deldir slot
    /// frees a file still reachable from its directory. Research: pfs3aio's
    /// own `DeleteObject` has the identical shape — `AllocDeldirSlot`/
    /// `AddToDeldir` write directly, with no rollback if a later step fails
    /// (`directory.c:1816-1830`) — the research notes it found no live
    /// trigger for that *in pfs3aio's own delete path as written* (`to ==
    /// NULL` skips the one fallible step); this crate's writer can hit it
    /// regardless, because a corrupt anode chain makes `free_data_blocks`
    /// itself fail.
    #[test]
    fn a_failed_pfs3_delete_stages_no_deldir_entry() {
        const TOTAL: u64 = 48_000;
        let dev = MemDevice::with_end(TOTAL);
        libpfs3::format::format_with_size(
            &dev,
            TOTAL,
            &libpfs3::format::FormatOptions {
                volume_name: "Work".into(),
                enable_deldir: true,
                datestamp: None,
            },
        )
        .unwrap();
        {
            let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            w.write_file("Gone", b"one block").unwrap();
        }

        // "Gone" is the format's first user anode, 6 — root dir uses 0-5 —
        // one extent, the same reading
        // `freeing_a_pfs3_block_past_the_partition_changes_nothing` does.
        // Corrupt its `next` field into a one-anode cycle: `get_chain` trips
        // over it before any block of the chain is freed.
        let root = dev.read(2, 512);
        let resblk = usize::from(be16(&root, 0x40));
        let ib = dev.read(u64::from(be32(&root, 0x60 + 5 * 4)), resblk);
        let ab_blk = u64::from(be32(&ib, 12));
        let mut ab = dev.read(ab_blk, resblk);
        let at = 16 + 6 * 12;
        assert_eq!(be32(&ab, at), 1, "anode 6 must be Gone's one-block extent");
        ab[at + 8..at + 12].copy_from_slice(&6u32.to_be_bytes()); // next = self
        dev.patch(ab_blk, &ab);

        {
            let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            let err = w.delete("Gone").unwrap_err();
            assert!(
                matches!(&err, libpfs3::error::Error::InvalidPartition(msg) if msg.contains("cycle")),
                "expected the failure from free_data_blocks's own anode-cycle check, got: {err}"
            );

            // Another committing operation, after the failed delete.
            w.write_file("Other", b"kept").unwrap();
        }

        let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
        assert!(
            vol.list_deldir().unwrap().is_empty(),
            "a failed delete left an entry in the deldir"
        );
        assert!(
            vol.list_dir("").unwrap().iter().any(|e| e.name == "Gone"),
            "the failed delete's file is missing from its own directory"
        );
    }

    /// **ART-319.** If the commit's own write fails — here,
    /// `update_rootblock`'s final rootblock-cluster write, its one write for
    /// an operation with nothing pending — the device may already be
    /// half-written (M5: writes land in place, not copy-on-write), so the
    /// writer locks rather than reloading and continuing over an unknown
    /// state. Every later mutating call then refuses immediately, with a
    /// typed lock error, before it can touch the device at all.
    #[test]
    fn a_pfs3_commit_failing_part_way_locks_the_writer() {
        const TOTAL: u64 = 48_000;
        let dev = MemDevice::with_end(TOTAL);
        formatted_in_memory(&dev, TOTAL);

        let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
        let mut w = libpfs3::writer::Writer::open(vol).unwrap();

        // Arm the device to refuse the very next write — the rootblock
        // cluster write inside `update_rootblock`, `repair_blocksfree`'s one
        // write, since it has no pending reserved-block writes and a clean
        // extension.
        let before = dev.write_count();
        dev.fail_from_write(before + 1);

        let err = w.repair_blocksfree(123).unwrap_err();
        assert!(
            matches!(err, libpfs3::error::Error::BlockOutOfRange(_)),
            "expected the commit's own write to be refused, got: {err}"
        );
        let after_first = dev.write_count();
        assert_eq!(
            after_first,
            before + 1,
            "the commit's own write did not even reach the device"
        );

        let err2 = w.repair_reserved_free(7).unwrap_err();
        assert!(
            matches!(err2, libpfs3::error::Error::CommitFailed),
            "the next operation must return the lock error, got: {err2}"
        );
        assert_eq!(
            dev.write_count(),
            after_first,
            "the device received a write after the writer should have locked"
        );
    }

    /// **ART-319 (I2, final review).** `set_volume_name` writes the
    /// rootblock cluster directly — its own commit point, not routed
    /// through `update_rootblock` — so a failure there had no test of its
    /// own lock behaviour (unlike `repair_blocksfree` above, which goes
    /// through `update_rootblock`). The locking code already exists
    /// (`set_volume_name_impl`'s own `self.poisoned = true;`), so this test
    /// passes unchanged; its guard is proven by mutation instead (ISSUES).
    #[test]
    fn a_failed_pfs3_set_volume_name_locks_the_writer() {
        const TOTAL: u64 = 48_000;
        let dev = MemDevice::with_end(TOTAL);
        formatted_in_memory(&dev, TOTAL);

        let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
        let mut w = libpfs3::writer::Writer::open(vol).unwrap();

        // Arm the device to refuse the very next write — `set_volume_name`'s
        // own rootblock-cluster write, its one write.
        let before = dev.write_count();
        dev.fail_from_write(before + 1);

        let err = w.set_volume_name("NewName").unwrap_err();
        assert!(
            matches!(err, libpfs3::error::Error::BlockOutOfRange(_)),
            "expected set_volume_name's own write to be refused, got: {err}"
        );
        let after_first = dev.write_count();
        assert_eq!(
            after_first,
            before + 1,
            "set_volume_name's own write did not even reach the device"
        );

        let err2 = w.repair_blocksfree(7).unwrap_err();
        assert!(
            matches!(err2, libpfs3::error::Error::CommitFailed),
            "the next operation must return the lock error, got: {err2}"
        );
        assert_eq!(
            dev.write_count(),
            after_first,
            "the device received a write after the writer should have locked"
        );
    }

    /// **ART-319 (M1, final review).** An operation that fails before any
    /// device I/O at all — a name over the volume's own limit,
    /// `check_name_len`, checked before `find_dir_entry` or any
    /// allocation — still runs through `guarded`'s discard, which rebuilds
    /// the writer's state from the device (`Volume::reload`). If the
    /// device has gone unreadable by the time that reload runs, there is
    /// nothing safe left to fall back to, so the writer must lock instead
    /// of carrying on over state it could not refresh — even though this
    /// particular failed call touched the device not at all. The locking
    /// code already exists (`discard_to_last_commit`'s own
    /// `self.poisoned = true;` on a failed `self.vol.reload()`), so this
    /// test passes unchanged; its guard is proven by mutation instead
    /// (ISSUES).
    #[test]
    fn a_reload_that_cannot_read_the_device_locks_the_writer() {
        const TOTAL: u64 = 48_000;
        let dev = MemDevice::with_end(TOTAL);
        formatted_in_memory(&dev, TOTAL);

        let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
        let mut w = libpfs3::writer::Writer::open(vol).unwrap();

        // Armed only now — after `Writer::open`'s own reads, so the writer
        // opened cleanly and this is the device going unreadable partway
        // through the session, not from the start.
        dev.fail_reads();

        let before = dev.write_count();
        let long_name = "x".repeat(200);
        let err = w
            .write_file_in(libpfs3::ondisk::ANODE_ROOTDIR, &long_name, b"data")
            .unwrap_err();
        assert!(
            matches!(err, libpfs3::error::Error::NameTooLong { .. }),
            "expected the name-length refusal, before any device I/O, got: {err}"
        );
        assert_eq!(
            dev.write_count(),
            before,
            "the failed write must not itself have reached the device"
        );

        let err2 = w.repair_blocksfree(7).unwrap_err();
        assert!(
            matches!(err2, libpfs3::error::Error::CommitFailed),
            "the reload's own read failure must lock the writer, got: {err2}"
        );
        assert_eq!(
            dev.write_count(),
            before,
            "a locked writer must refuse before touching the device"
        );
    }

    /// **ART-319.** The lock error reaches the user as a readable sentence
    /// that says what to do — reopen and check the volume — not
    /// "malformed pfs3: ...", which would point someone at the wrong
    /// diagnosis (a damaged file, rather than a session that failed
    /// mid-commit).
    #[test]
    fn a_poisoned_pfs3_writers_error_reaches_the_user_as_a_readable_sentence() {
        let err = from_pfs3(libpfs3::error::Error::CommitFailed);
        assert!(matches!(err, CoreError::Pfs3WriterLocked), "{err}");
        assert_eq!(err.code(), "ART-PFS3-WRITER-LOCKED");
        let text = err.to_string().to_lowercase();
        assert!(text.contains("reopen"), "{err}");
        assert!(!text.contains("malformed"), "{err}");
    }

    // ---- ART-319's disclosed gaps (third debt round, item 3): copy-on-write
    // ---- overwrite, rename in one commit, the untested mutators

    /// `len` bytes that differ from block to block, starting from `seed`.
    fn pfs3_bytes(seed: u8, len: usize) -> Vec<u8> {
        (0..len)
            .map(|i| seed.wrapping_add((i % 251) as u8))
            .collect()
    }

    /// Everything a reopened `Volume` says about a PFS3 volume's last
    /// commit: the reserved area byte for byte — with the rootblock's own
    /// datestamp zeroed, which every commit bumps — and, as sentences, the
    /// free counts, every entry reachable from the root with its content,
    /// and the deldir. Data blocks outside every file are not in it: a
    /// discarded call may have written its data into blocks that are still
    /// free, which the last commit never mentions.
    fn pfs3_committed_state(dev: &MemDevice) -> (Vec<u8>, Vec<String>) {
        let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
        let rb = vol.rootblock.clone();
        let mut raw = dev.read(0, (rb.lastreserved as usize + 1) * 512);
        let ds = rb.firstreserved as usize * 512 + libpfs3::ondisk::RB_OFF_DATESTAMP;
        raw[ds..ds + 4].fill(0);
        let mut facts = vec![format!(
            "blocksfree {} (bitmap {:?}), reserved_free {} (bitmap {:?})",
            rb.blocksfree,
            vol.bitmap_count_free().map_err(|e| e.to_string()),
            rb.reserved_free,
            vol.reserved_count_free().map_err(|e| e.to_string()),
        )];
        let mut dirs = vec![(String::new(), libpfs3::ondisk::ANODE_ROOTDIR)];
        while let Some((path, anode)) = dirs.pop() {
            let entries = match vol.list_dir_by_anode(anode) {
                Ok(entries) => entries,
                Err(e) => {
                    facts.push(format!("{path}/ unreadable: {e}"));
                    continue;
                }
            };
            for e in entries {
                let full = format!("{path}/{}", e.name);
                let content = if e.is_dir() {
                    dirs.push((full.clone(), e.anode));
                    Ok(Vec::new())
                } else {
                    vol.read_file_data(e.anode, e.file_size())
                        .map_err(|x| x.to_string())
                };
                facts.push(format!(
                    "{full} type {} anode {} size {} protection {} date {}/{}/{} content {content:?}",
                    e.entry_type,
                    e.anode,
                    e.file_size(),
                    e.protection,
                    e.creation_day,
                    e.creation_minute,
                    e.creation_tick
                ));
            }
        }
        facts.push(format!(
            "deldir {:?}",
            vol.list_deldir()
                .map(|d| d
                    .iter()
                    .map(|x| format!(
                        "{} anode {} size {} date {}/{}/{}",
                        x.filename,
                        x.anode,
                        x.file_size(),
                        x.creation_day,
                        x.creation_minute,
                        x.creation_tick
                    ))
                    .collect::<Vec<_>>())
                .map_err(|e| e.to_string())
        ));
        (raw, facts)
    }

    /// Whether data block `blk` is free in the on-disk data bitmap (a set
    /// bit is free), read raw rather than through `libpfs3`.
    fn pfs3_data_block_free_on_disk(dev: &MemDevice, blk: u32) -> bool {
        let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
        let rb = &vol.rootblock;
        let rbs = usize::from(rb.reserved_blksize);
        let bits_per_bitmap_block = (rbs as u32 / 4 - 3) * 32;
        let rel = blk - (rb.lastreserved + 1);
        let bmi = dev.read(u64::from(rb.bitmapindex[0]), rbs);
        let seq = (rel / bits_per_bitmap_block) as usize;
        let bm = dev.read(u64::from(be32(&bmi, 12 + seq * 4)), rbs);
        let bit = rel % bits_per_bitmap_block;
        be32(&bm, 12 + (bit / 32) as usize * 4) & (0x8000_0000 >> (bit % 32)) != 0
    }

    /// Anode `nr`'s three fields `(clustersize, blocknr, next)` as the
    /// device holds them, read raw from a small-mode volume's first anode
    /// block — the reading `a_failed_pfs3_delete_stages_no_deldir_entry` uses.
    fn pfs3_raw_anode(dev: &MemDevice, nr: u32) -> (u32, u32, u32) {
        let root = dev.read(2, 512);
        let resblk = usize::from(be16(&root, 0x40));
        assert!(
            (nr as usize) < (resblk - 16) / 12,
            "anode {nr} is past the first anode block"
        );
        let ib = dev.read(u64::from(be32(&root, 0x60 + 5 * 4)), resblk);
        let ab = dev.read(u64::from(be32(&ib, 12)), resblk);
        let at = 16 + nr as usize * 12;
        (be32(&ab, at), be32(&ab, at + 4), be32(&ab, at + 8))
    }

    /// The anode numbers and the one directory block the mutator tests use.
    struct Pfs3Fixture {
        /// "File", two blocks of `pfs3_bytes(1, 700)`.
        file: u32,
        /// "Dir", holding "Src".
        dir: u32,
        /// The sector "Dir"'s one directory block starts at.
        dir_block: u64,
        /// "Broken": a directory whose one block no longer carries the `DB`
        /// id, so adding an entry to it fails in `add_dir_entry` — *after*
        /// the caller allocated what the entry needs.
        broken: u32,
    }

    const PFS3_FIXTURE_DATE: (u16, u16, u16) = (17_000, 600, 0);

    /// A 48 000-block PFS3 volume holding "File", "Dir/Src", "Dst" and
    /// "Broken"; with the deldir on, also "Gone", deleted into deldir slot 0.
    fn pfs3_mutator_fixture(dev: &MemDevice, deldir: bool) -> Pfs3Fixture {
        const TOTAL: u64 = 48_000;
        libpfs3::format::format_with_size(
            dev,
            TOTAL,
            &libpfs3::format::FormatOptions {
                volume_name: "Work".into(),
                enable_deldir: deldir,
                datestamp: Some(PFS3_FIXTURE_DATE),
            },
        )
        .unwrap();
        {
            let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            w.set_entry_date(Some(PFS3_FIXTURE_DATE));
            w.write_file("File", &pfs3_bytes(1, 700)).unwrap();
            w.create_dir("Dir").unwrap();
            w.write_file("Dir/Src", b"source bytes").unwrap();
            w.write_file("Dst", b"destination bytes").unwrap();
            w.create_dir("Broken").unwrap();
            if deldir {
                w.write_file("Gone", b"deleted bytes").unwrap();
                w.delete("Gone").unwrap();
            }
        }
        let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
        assert_eq!(
            vol.rootblock.reserved_blksize, 1024,
            "the fixture's arithmetic"
        );
        let mut anode_of = |path: &str| vol.lookup(path).unwrap().unwrap().anode;
        let (file, dir, broken) = (anode_of("File"), anode_of("Dir"), anode_of("Broken"));
        let dir_block = u64::from(vol.get_anode_chain(dir).unwrap()[0].blocknr);
        let broken_block = u64::from(vol.get_anode_chain(broken).unwrap()[0].blocknr);
        let mut block = dev.read(broken_block, 1024);
        assert_eq!(be16(&block, 0), libpfs3::ondisk::DBLKID);
        block[0..2].fill(0);
        dev.patch(broken_block, &block);
        Pfs3Fixture {
            file,
            dir,
            dir_block,
            broken,
        }
    }

    /// The discard half of ART-319, for one mutator: `op` fails part-way
    /// with the failure `expected` names; `on_disk` checks the device right
    /// then; then the same writer commits once more — which would publish
    /// whatever in-memory state the failed call left behind — and the
    /// reopened volume must be exactly the last commit before the call.
    fn assert_pfs3_failure_leaves_the_last_commit(
        dev: &MemDevice,
        op: impl FnOnce(&mut libpfs3::writer::Writer) -> libpfs3::error::Result<()>,
        expected: impl Fn(&libpfs3::error::Error) -> bool,
        on_disk: impl FnOnce(&MemDevice),
    ) {
        let (raw_before, facts_before) = pfs3_committed_state(dev);
        let committed_free = libpfs3::volume::Volume::from_device(Box::new(dev.clone()))
            .unwrap()
            .rootblock
            .blocksfree;
        let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
        let mut w = libpfs3::writer::Writer::open(vol).unwrap();
        let err = op(&mut w).expect_err("the injected failure must fail the call");
        assert!(expected(&err), "not the injected failure: {err}");
        on_disk(dev);
        dev.fail_from_write(u64::MAX); // disarm anything `op` armed
        w.repair_blocksfree(committed_free)
            .unwrap_or_else(|e| panic!("the writer must commit again after a discard: {e}"));
        drop(w);
        let (raw_after, facts_after) = pfs3_committed_state(dev);
        assert_eq!(
            facts_after, facts_before,
            "the failed call's in-memory state reached the next commit"
        );
        if let Some(at) = raw_before.iter().zip(&raw_after).position(|(a, b)| a != b) {
            panic!(
                "the failed call's in-memory state reached the next commit: the reserved area \
                 differs from the last commit, first at sector {}",
                at / 512
            );
        }
    }

    /// The lock half of ART-319, for one mutator: the device refuses the
    /// first write after `op`'s own `data_writes` data-block writes — its
    /// commit's first write — and the next call must refuse with
    /// `CommitFailed` without reaching the device.
    fn assert_pfs3_commit_failure_locks(
        dev: &MemDevice,
        data_writes: u64,
        op: impl FnOnce(&mut libpfs3::writer::Writer) -> libpfs3::error::Result<()>,
    ) {
        let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
        let mut w = libpfs3::writer::Writer::open(vol).unwrap();
        let before = dev.write_count();
        dev.fail_from_write(before + data_writes + 1);
        let err = op(&mut w).expect_err("the refused commit must fail the call");
        assert!(
            matches!(err, libpfs3::error::Error::BlockOutOfRange(_)),
            "expected the commit's own write to be refused, got: {err}"
        );
        let after = dev.write_count();
        assert_eq!(
            after,
            before + data_writes + 1,
            "the call stops at the refused write"
        );
        let err2 = w.repair_reserved_free(0).unwrap_err();
        assert!(
            matches!(err2, libpfs3::error::Error::CommitFailed),
            "the next operation must return the lock error, got: {err2}"
        );
        assert_eq!(
            dev.write_count(),
            after,
            "the device received a write after the writer should have locked"
        );
    }

    fn is_corrupt(e: &libpfs3::error::Error) -> bool {
        matches!(e, libpfs3::error::Error::Corrupt(_))
    }

    fn is_not_found(e: &libpfs3::error::Error) -> bool {
        matches!(e, libpfs3::error::Error::NotFound(_))
    }

    const PFS3_ROOT: u32 = libpfs3::ondisk::ANODE_ROOTDIR;

    /// **Item 3 (ART-319's first disclosed gap).** An overwrite that fails
    /// after its new data is written but before its commit — here the
    /// directory entry it names is not there, which `update_dir_entry_size`
    /// finds only after the data and the anode chain are done — must leave
    /// the old content on disk. 0.1.3 wrote the new data over the file's
    /// own blocks first, so `guarded`'s discard restored the metadata over
    /// blocks that already held the new bytes.
    #[test]
    fn a_failed_pfs3_overwrite_keeps_the_old_content_on_disk() {
        let dev = MemDevice::new();
        let fx = pfs3_mutator_fixture(&dev, false);
        let d = dev.clone();
        assert_pfs3_failure_leaves_the_last_commit(
            &dev,
            |w| w.overwrite_file_in(PFS3_ROOT, "NoSuchName", fx.file, &pfs3_bytes(9, 700)),
            is_not_found,
            |_| {
                let mut vol = libpfs3::volume::Volume::from_device(Box::new(d)).unwrap();
                assert!(
                    vol.read_file("File").unwrap() == pfs3_bytes(1, 700),
                    "a failed overwrite destroyed the old content"
                );
            },
        );
    }

    /// **Item 3.** A successful overwrite: the new content, under the same
    /// anode number (the method's contract), in blocks the old file did not
    /// use; the old blocks free in the on-disk bitmap, and `blocksfree`
    /// agreeing with it.
    #[test]
    fn a_pfs3_overwrite_writes_new_blocks_and_frees_the_old_ones_in_its_commit() {
        let dev = MemDevice::new();
        let fx = pfs3_mutator_fixture(&dev, false);
        let (old_blocks, free_before) = {
            let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let chain = vol.get_anode_chain(fx.file).unwrap();
            let blocks: Vec<u32> = chain
                .iter()
                .flat_map(|a| (0..a.clustersize).map(move |i| a.blocknr + i))
                .collect();
            (blocks, vol.rootblock.blocksfree)
        };
        assert_eq!(old_blocks.len(), 2);
        {
            let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            w.overwrite_file_in(PFS3_ROOT, "File", fx.file, &pfs3_bytes(9, 1300))
                .unwrap();
        }
        let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
        assert!(vol.read_file("File").unwrap() == pfs3_bytes(9, 1300));
        assert_eq!(vol.lookup("File").unwrap().unwrap().anode, fx.file);
        let new_blocks: Vec<u32> = vol
            .get_anode_chain(fx.file)
            .unwrap()
            .iter()
            .flat_map(|a| (0..a.clustersize).map(move |i| a.blocknr + i))
            .collect();
        assert_eq!(new_blocks.len(), 3);
        for blk in &old_blocks {
            assert!(
                pfs3_data_block_free_on_disk(&dev, *blk),
                "old block {blk} is not free after the overwrite (new blocks {new_blocks:?})"
            );
        }
        for blk in &new_blocks {
            assert!(
                !pfs3_data_block_free_on_disk(&dev, *blk),
                "new block {blk} is free"
            );
        }
        let bitmap_free = vol.bitmap_count_free().unwrap();
        assert_eq!(vol.rootblock.blocksfree, bitmap_free);
        assert_eq!(vol.rootblock.blocksfree, free_before + 2 - 3);
    }

    /// **Item 3.** The old chain's other anodes are freed in the same
    /// commit — a file of two extents overwritten by one block keeps only
    /// its head anode, and the other reads `(0, 0, 0)` on disk.
    #[test]
    fn a_pfs3_overwrite_frees_the_old_chains_other_anodes() {
        const TOTAL: u64 = 48_000;
        let dev = MemDevice::new();
        formatted_in_memory(&dev, TOTAL);
        {
            let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            w.write_file("A", b"a").unwrap();
            w.write_file("B", b"b").unwrap();
            w.delete("A").unwrap();
            // A's freed block, then two after B's: two extents.
            w.write_file("File", &pfs3_bytes(3, 1500)).unwrap();
        }
        let (file, old_chain) = {
            let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let file = vol.lookup("File").unwrap().unwrap().anode;
            (file, vol.get_anode_chain(file).unwrap())
        };
        assert!(
            old_chain.len() >= 2,
            "the fixture must make a fragmented file"
        );
        {
            let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            w.overwrite_file_in(PFS3_ROOT, "File", file, b"one block now")
                .unwrap();
        }
        let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
        assert_eq!(vol.read_file("File").unwrap(), b"one block now");
        let chain = vol.get_anode_chain(file).unwrap();
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].nr, file);
        for an in &old_chain[1..] {
            assert_eq!(
                pfs3_raw_anode(&dev, an.nr),
                (0, 0, 0),
                "the old chain's anode {} leaked",
                an.nr
            );
        }
    }

    /// **Item 3.** Copy-on-write needs free space for the whole new
    /// content: an overwrite that would fit only by reusing the file's own
    /// block is refused with `DiskFull`, before a single write, and the
    /// file keeps its old content. 0.1.3 overwrote in place, so it fitted.
    #[test]
    fn a_pfs3_overwrite_needs_free_space_for_the_whole_new_content() {
        const TOTAL: u64 = 48_000;
        let dev = MemDevice::new();
        formatted_in_memory(&dev, TOTAL);
        {
            let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            w.write_file("File", b"one block").unwrap();
        }
        let (file, free) = {
            let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            (
                vol.lookup("File").unwrap().unwrap().anode,
                vol.rootblock.blocksfree,
            )
        };
        // Every free block plus the file's own one.
        let new = vec![0x33u8; free as usize * 512 + 1];
        {
            let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            let before = dev.write_count();
            let err = w
                .overwrite_file_in(PFS3_ROOT, "File", file, &new)
                .expect_err("copy-on-write has no room for the whole new content");
            assert!(
                matches!(err, libpfs3::error::Error::DiskFull(_)),
                "expected disk full, got: {err}"
            );
            assert_eq!(dev.write_count(), before, "a refused overwrite wrote");
        }
        let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
        assert_eq!(vol.read_file("File").unwrap(), b"one block");
        assert_eq!(vol.rootblock.blocksfree, free);
        assert_eq!(vol.bitmap_count_free().unwrap(), free);
    }

    /// **Item 3 (ART-319's second disclosed gap).** A rename over an
    /// existing destination that fails after the destination's delete —
    /// here an I/O error reading the source's directory block a second
    /// time, which `remove_dir_entry` does after the delete and the new
    /// entry — must leave the destination where it was. 0.1.3 deleted it
    /// with `delete_in`, its own commit, first.
    #[test]
    fn a_failed_pfs3_rename_over_an_existing_file_keeps_the_destination() {
        let dev = MemDevice::new();
        let fx = pfs3_mutator_fixture(&dev, false);
        let (armed, checked) = (dev.clone(), dev.clone());
        assert_pfs3_failure_leaves_the_last_commit(
            &dev,
            |w| {
                // Read 1: the source listing. Read 2: `remove_dir_entry`.
                armed.fail_read_of(fx.dir_block, 2);
                w.rename_in(fx.dir, "Src", PFS3_ROOT, "Dst")
            },
            |e| matches!(e, libpfs3::error::Error::Io(io) if io.to_string().contains("fail_read_of")),
            |_| {
                let mut vol = libpfs3::volume::Volume::from_device(Box::new(checked)).unwrap();
                assert_eq!(
                    vol.read_file("Dst").ok().as_deref(),
                    Some(&b"destination bytes"[..]),
                    "a failed rename deleted its destination"
                );
                assert_eq!(vol.read_file("Dir/Src").unwrap(), b"source bytes");
            },
        );
    }

    /// **Item 3.** In one commit, a rename over an existing file still
    /// sends that file to the deldir exactly as `delete_in` does: the same
    /// volume, renamed over "Dst" on one device and "Dst" deleted then
    /// renamed onto on another, reads the same — tree, free counts, deldir.
    #[test]
    fn a_pfs3_rename_over_an_existing_file_sends_it_to_the_deldir_as_delete_does() {
        let (over, apart) = (MemDevice::new(), MemDevice::new());
        let fx = pfs3_mutator_fixture(&over, true);
        pfs3_mutator_fixture(&apart, true);
        for (dev, delete_first) in [(&over, false), (&apart, true)] {
            let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            w.set_entry_date(Some(PFS3_FIXTURE_DATE));
            if delete_first {
                w.delete_in(PFS3_ROOT, "Dst").unwrap();
            }
            w.rename_in(fx.dir, "Src", PFS3_ROOT, "Dst").unwrap();
        }
        let (_, facts) = pfs3_committed_state(&over);
        assert_eq!(facts, pfs3_committed_state(&apart).1);
        let deldir = facts.last().unwrap();
        assert!(deldir.contains("Dst anode"), "{deldir}");
        assert!(
            facts
                .iter()
                .any(|f| f.starts_with("/Dst ") && f.contains("[115, 111, 117")),
            "the renamed file is not at /Dst: {facts:#?}"
        );
    }

    /// **ART-322.** `rename_in(root, "File", root, "FILE")` changes only a
    /// name's case in the same directory. PFS3 names compare case-
    /// insensitively (`name_eq_ci`), so the destination lookup finds the
    /// source itself as "an existing destination" — the defect deleted it
    /// (data blocks freed) and reported `Ok`. The fixed writer must recognise
    /// the destination as the very entry being renamed and rename it in
    /// place: the file survives under exactly one, case-insensitively
    /// matching name, with its content, anode, protection and creation date
    /// unchanged. pfs3aio's own `RenameAndMove` never deletes in this case
    /// either (`directory.c:2064-2076,2130`, `tonioni/pfs3aio` `211f7f0`):
    /// `FindObject` on the destination name only refuses `ERROR_OBJECT_EXISTS`
    /// when the found entry's `direntry` differs from the source's — "%9.1
    /// the same name IS allowed (rename 'hello' to 'Hello')" — and when it is
    /// the same entry, falls through to `ChangeDirEntry`, which moves it
    /// rather than deleting and recreating it.
    #[test]
    fn a_pfs3_case_only_rename_keeps_the_file_its_content_and_its_anode_and_lists_it_under_the_new_case(
    ) {
        let dev = MemDevice::new();
        let fx = pfs3_mutator_fixture(&dev, false);
        {
            let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            // A non-default protection, so "protection survives" is not
            // trivially true of every freshly created file.
            w.update_dir_entry_protection(PFS3_ROOT, "File", 0x0d)
                .unwrap();
        }
        let before = {
            let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            vol.lookup("File").unwrap().unwrap()
        };
        {
            let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            w.rename_in(PFS3_ROOT, "File", PFS3_ROOT, "FILE").unwrap();
        }
        let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
        let entries = vol.list_dir_by_anode(PFS3_ROOT).unwrap();
        assert_eq!(
            entries
                .iter()
                .filter(|e| e.name.eq_ignore_ascii_case("FILE"))
                .count(),
            1,
            "a case-only rename must leave exactly one entry, not the \
             deleted-and-recreated (or deleted-and-gone) result: {entries:#?}"
        );
        assert!(
            entries.iter().any(|e| e.name == "FILE"),
            "the entry must list under the new case: {entries:#?}"
        );
        let after = vol.lookup("FILE").unwrap().unwrap();
        assert_eq!(after.name, "FILE");
        assert_eq!(
            after.anode, fx.file,
            "the renamed entry must keep its anode"
        );
        assert_eq!(after.protection, before.protection);
        assert_eq!(
            (
                after.creation_day,
                after.creation_minute,
                after.creation_tick
            ),
            (
                before.creation_day,
                before.creation_minute,
                before.creation_tick
            ),
            "a case-only rename must not touch the entry's own creation date"
        );
        assert_eq!(after.comment, before.comment);
        assert_eq!(
            vol.read_file("FILE").unwrap(),
            pfs3_bytes(1, 700),
            "a case-only rename must not touch the file's content"
        );
    }

    /// **ART-322.** A rename to the identical name, same case, is a no-op
    /// success: the destination is once again the source itself, so nothing
    /// is deleted, and here nothing is written at all — the committed state
    /// (reserved area, tree, free counts, deldir) is byte-for-byte what it
    /// was. pfs3aio's own `RenameAndMove` was read (`directory.c:2064-2076`)
    /// but not traced past `ChangeDirEntry` for this exact sub-case, so this
    /// is ART's own choice of ending, not a claim about pfs3aio's.
    #[test]
    fn a_pfs3_rename_to_the_identical_name_is_a_no_op_success() {
        let dev = MemDevice::new();
        pfs3_mutator_fixture(&dev, false);
        let before_writes = dev.write_count();
        let (raw_before, facts_before) = pfs3_committed_state(&dev);
        {
            let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            w.rename_in(PFS3_ROOT, "File", PFS3_ROOT, "File").unwrap();
        }
        assert_eq!(
            dev.write_count(),
            before_writes,
            "a rename to the identical name wrote to the device"
        );
        let (raw_after, facts_after) = pfs3_committed_state(&dev);
        assert_eq!(facts_after, facts_before);
        assert_eq!(raw_after, raw_before);
    }

    /// **ART-322, the case that must keep replacing.** A same-directory
    /// rename whose destination name differs only in case from a *different*
    /// entry (a different anode) is not the same-entry case: the destination
    /// is still replaced, exactly as before this fix.
    #[test]
    fn a_pfs3_rename_over_a_different_entry_whose_name_differs_only_in_case_still_replaces_it() {
        let dev = MemDevice::new();
        let fx = pfs3_mutator_fixture(&dev, false);
        let dst_anode_before = {
            let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            vol.lookup("Dst").unwrap().unwrap().anode
        };
        {
            let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            // "dst" differs from the existing "Dst" only in case, but "File"
            // is a different entry (a different anode): ART-322's
            // same-entry check must not fire here.
            w.rename_in(PFS3_ROOT, "File", PFS3_ROOT, "dst").unwrap();
        }
        let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
        let entries = vol.list_dir_by_anode(PFS3_ROOT).unwrap();
        assert_eq!(
            entries
                .iter()
                .filter(|e| e.name.eq_ignore_ascii_case("dst"))
                .count(),
            1,
            "{entries:#?}"
        );
        let after = vol.lookup("dst").unwrap().unwrap();
        assert_eq!(after.anode, fx.file, "the renamed File must be at dst");
        assert_ne!(
            after.anode, dst_anode_before,
            "the old Dst's anode must not remain"
        );
        assert_eq!(vol.read_file("dst").unwrap(), pfs3_bytes(1, 700));
        assert!(vol.lookup("File").unwrap().is_none());
    }

    /// Sets data block `blk`'s bit — free — in the on-disk bitmap, and
    /// nothing else: `blocksfree` is left as it was, so the volume is
    /// deliberately inconsistent. The raw reading
    /// `pfs3_data_block_free_on_disk` uses, written back.
    fn pfs3_mark_data_block_free_on_disk(dev: &MemDevice, blk: u32) {
        let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
        let rb = &vol.rootblock;
        let rbs = usize::from(rb.reserved_blksize);
        let bits_per_bitmap_block = (rbs as u32 / 4 - 3) * 32;
        let rel = blk - (rb.lastreserved + 1);
        let bmi = dev.read(u64::from(rb.bitmapindex[0]), rbs);
        let seq = (rel / bits_per_bitmap_block) as usize;
        let bm_sector = u64::from(be32(&bmi, 12 + seq * 4));
        let mut bm = dev.read(bm_sector, rbs);
        let bit = rel % bits_per_bitmap_block;
        let at = 12 + (bit / 32) as usize * 4;
        let word = be32(&bm, at) | (0x8000_0000 >> (bit % 32));
        bm[at..at + 4].copy_from_slice(&word.to_be_bytes());
        dev.patch(bm_sector, &bm);
    }

    /// **Final review fix wave, I1.** A hard link stores its target's anode
    /// in its own entry (`create_hardlink`), so ART-322's same-entry check —
    /// same parent, same anode, and nothing about the name — took "Link" for
    /// "File" itself: `rename_in(root, "File", root, "Link")` rewrote "File"
    /// to "Link" in place and returned `Ok` with two entries named "Link".
    /// Neither direction may be treated as the source renaming onto itself,
    /// and neither may replace the destination either: this writer's delete
    /// of a hard link frees the file it names (ART-323), and the link and the
    /// file are one file. pfs3aio's own `RenameAndMove` refuses any found
    /// destination that is not the source's own direntry with
    /// `ERROR_OBJECT_EXISTS` (`directory.c:2064-2076`, `tonioni/pfs3aio`
    /// `211f7f0`) — and a link's direntry is its own. Refused, nothing written.
    #[test]
    fn a_pfs3_rename_onto_a_hard_link_to_the_same_file_is_refused_and_writes_nothing() {
        let dev = MemDevice::new();
        let fx = pfs3_mutator_fixture(&dev, false);
        pfs3_append_raw_entries(
            &dev,
            PFS3_ROOT,
            &[pfs3_raw_entry(LINKFILE, fx.file, 0, "Link", 0)],
        );
        let before_writes = dev.write_count();
        let (raw_before, facts_before) = pfs3_committed_state(&dev);
        for (src, dst) in [("File", "Link"), ("Link", "File")] {
            let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            let result = w.rename_in(PFS3_ROOT, src, PFS3_ROOT, dst);
            assert!(
                matches!(result, Err(libpfs3::error::Error::AlreadyExists(_))),
                "renaming {src} onto {dst}, a hard link to the same file, must be refused as \
                 already existing, got: {result:?}; root now: {:#?}",
                pfs3_committed_state(&dev).1
            );
        }
        assert_eq!(
            dev.write_count(),
            before_writes,
            "a refused rename wrote to the device"
        );
        let (raw_after, facts_after) = pfs3_committed_state(&dev);
        assert_eq!(facts_after, facts_before);
        assert!(raw_after == raw_before, "the reserved area changed");
    }

    /// **Final review fix wave, I1, a different-length link name.** The same
    /// misfire with a name of another length reached
    /// `rename_dir_entry_in_place`'s length check and called a healthy volume
    /// "corrupt filesystem". It is the same refusal as the same-length case.
    #[test]
    fn a_pfs3_rename_onto_a_longer_named_hard_link_to_the_same_file_is_refused_not_called_corrupt()
    {
        let dev = MemDevice::new();
        let fx = pfs3_mutator_fixture(&dev, false);
        pfs3_append_raw_entries(
            &dev,
            PFS3_ROOT,
            &[pfs3_raw_entry(LINKFILE, fx.file, 0, "LongerLink", 0)],
        );
        let (_, facts_before) = pfs3_committed_state(&dev);
        {
            let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            let result = w.rename_in(PFS3_ROOT, "File", PFS3_ROOT, "LongerLink");
            assert!(
                matches!(result, Err(libpfs3::error::Error::AlreadyExists(_))),
                "expected already-exists, got: {result:?}"
            );
        }
        assert_eq!(pfs3_committed_state(&dev).1, facts_before);
    }

    /// **Final review fix wave, I1, a link to another file.** Once the
    /// same-entry check compares names, a hard link to a *different* file is
    /// an ordinary found destination — and deleting it through
    /// `delete_in_no_commit` frees the data and anodes of the file it names
    /// (ART-323), while that file's own entry still points at them. Refused
    /// too, and "Dst" keeps its content.
    #[test]
    fn a_pfs3_rename_onto_a_hard_link_to_another_file_is_refused_and_that_file_keeps_its_content() {
        let dev = MemDevice::new();
        pfs3_mutator_fixture(&dev, false);
        let dst = {
            let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            vol.lookup("Dst").unwrap().unwrap().anode
        };
        pfs3_append_raw_entries(
            &dev,
            PFS3_ROOT,
            &[pfs3_raw_entry(LINKFILE, dst, 0, "DstLink", 0)],
        );
        {
            let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            let result = w.rename_in(PFS3_ROOT, "File", PFS3_ROOT, "DstLink");
            assert!(
                matches!(result, Err(libpfs3::error::Error::AlreadyExists(_))),
                "expected already-exists, got: {result:?}"
            );
        }
        let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
        assert_eq!(
            vol.read_file("Dst").ok().as_deref(),
            Some(&b"destination bytes"[..]),
            "renaming onto a link to Dst destroyed Dst"
        );
        assert_eq!(vol.read_file("File").unwrap(), pfs3_bytes(1, 700));
    }

    const FILE: i8 = libpfs3::ondisk::ST_FILE;
    const LINKFILE: i8 = libpfs3::ondisk::ST_LINKFILE;
    const LINKDIR: i8 = libpfs3::ondisk::ST_LINKDIR;

    /// One PFS3 directory entry, raw, with the fixture's date. `link` is
    /// the `link` extra field, laid out as pfs3aio's `AddExtraFields`
    /// writes it (`directory.c:3764-3800`, `tonioni/pfs3aio` `211f7f0`):
    /// the non-zero 16-bit words of the field in reverse order, then the
    /// flags word — one bit per word, bit 0 the high word — as the entry's
    /// last two bytes. hst-amiga reads it back the same way, from the end
    /// (`DirEntryReader.ReadExtraFields`, `henrikstengaard/hst-amiga`
    /// `6b45584`). With `link` 0 this is byte for byte what 0.1.3's
    /// `create_hardlink` wrote for a link (its `build_dir_entry`: flags 0).
    fn pfs3_raw_entry(entry_type: i8, anode: u32, fsize: u32, name: &str, link: u32) -> Vec<u8> {
        let mut e = vec![0u8; 18];
        e[1] = entry_type as u8;
        e[2..6].copy_from_slice(&anode.to_be_bytes());
        e[6..10].copy_from_slice(&fsize.to_be_bytes());
        e[10..12].copy_from_slice(&PFS3_FIXTURE_DATE.0.to_be_bytes());
        e[12..14].copy_from_slice(&PFS3_FIXTURE_DATE.1.to_be_bytes());
        e[14..16].copy_from_slice(&PFS3_FIXTURE_DATE.2.to_be_bytes());
        e[17] = name.len() as u8;
        e.extend_from_slice(name.as_bytes());
        e.push(0); // comment length
        if e.len() % 2 == 1 {
            e.push(0);
        }
        let (hi, lo) = ((link >> 16) as u16, link as u16);
        let mut flags = 0u16;
        if lo != 0 {
            e.extend_from_slice(&lo.to_be_bytes());
            flags |= 2;
        }
        if hi != 0 {
            e.extend_from_slice(&hi.to_be_bytes());
            flags |= 1;
        }
        e.extend_from_slice(&flags.to_be_bytes());
        e[0] = e.len() as u8;
        e
    }

    /// Appends raw entries after the last entry of `dir`'s first directory
    /// block, on the device — a committed state no writer call made.
    fn pfs3_append_raw_entries(dev: &MemDevice, dir: u32, entries: &[Vec<u8>]) {
        let blk = {
            let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            u64::from(vol.get_anode_chain(dir).unwrap()[0].blocknr)
        };
        let mut block = dev.read(blk, 1024);
        let mut pos = libpfs3::ondisk::DIR_BLOCK_HEADER_SIZE;
        while block[pos] != 0 {
            pos += usize::from(block[pos]);
        }
        for e in entries {
            block[pos..pos + e.len()].copy_from_slice(e);
            pos += e.len();
        }
        assert!(pos < 1024, "the raw entries do not fit the block");
        dev.patch(blk, &block);
    }

    /// "Links/Obj" and "Links/Lnk", a hard link to it, as pfs3aio's
    /// `CreateLink` leaves them (`directory.c:2672-2748`): the link's entry
    /// is `ST_LINKFILE`, its anode is a link node of its own — clustersize
    /// the object's directory, blocknr the link's directory, next 0 — and
    /// its `link` field is the object's anode; the object's own `link`
    /// field is the head of its chain of links, that node. `head` false
    /// leaves the object's field 0 (what 0.1.3's `rename_in`, which rebuilds
    /// an entry without its extra fields, leaves); `link_entry` false leaves
    /// the chain naming a node whose entry is gone.
    struct Pfs3aioLink {
        dir: u32,
        node: u32,
    }

    fn pfs3aio_linked_file(dev: &MemDevice, head: bool, link_entry: bool) -> Pfs3aioLink {
        {
            let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            w.set_entry_date(Some(PFS3_FIXTURE_DATE));
            w.create_dir("Links").unwrap();
            w.write_file("Links/Obj", b"linked bytes").unwrap();
        }
        let (dir, obj, dir_block) = {
            let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let dir = vol.lookup("Links").unwrap().unwrap().anode;
            let obj = vol.lookup("Links/Obj").unwrap().unwrap().anode;
            let blk = u64::from(vol.get_anode_chain(dir).unwrap()[0].blocknr);
            (dir, obj, blk)
        };
        // A free anode for the link node, in the first anode block — the
        // layout `pfs3_raw_anode` reads.
        let root = dev.read(2, 512);
        let resblk = usize::from(be16(&root, 0x40));
        let ib = dev.read(u64::from(be32(&root, 0x60 + 5 * 4)), resblk);
        let ab_blk = u64::from(be32(&ib, 12));
        let mut ab = dev.read(ab_blk, resblk);
        let node = (6..(resblk - 16) / 12)
            .find(|&i| ab[16 + i * 12..28 + i * 12].iter().all(|&b| b == 0))
            .expect("a free anode") as u32;
        let at = 16 + node as usize * 12;
        ab[at..at + 4].copy_from_slice(&dir.to_be_bytes());
        ab[at + 4..at + 8].copy_from_slice(&dir.to_be_bytes());
        dev.patch(ab_blk, &ab);
        let size = b"linked bytes".len() as u32;
        let mut block = dev.read(dir_block, 1024);
        block[libpfs3::ondisk::DIR_BLOCK_HEADER_SIZE..].fill(0);
        dev.patch(dir_block, &block);
        let mut entries = vec![pfs3_raw_entry(
            FILE,
            obj,
            size,
            "Obj",
            if head { node } else { 0 },
        )];
        if link_entry {
            entries.push(pfs3_raw_entry(LINKFILE, node, size, "Lnk", obj));
        }
        pfs3_append_raw_entries(dev, dir, &entries);
        Pfs3aioLink { dir, node }
    }

    fn has_links_sentence(name: &str, links: &str) -> String {
        format!(
            "'{name}' has hard links ({links}): this writer cannot hand it over to one of them as \
             pfs3aio does, so it did not delete it — delete the links first"
        )
    }

    /// **ART-323.** 0.1.3's hard link stores the anode of the file it names
    /// in its own entry, and `delete_in` took any entry that is not a
    /// directory for a file: it freed that anode's data blocks and cleared
    /// its anodes, so "Dst" stayed listed, read back empty, and its block was
    /// free for the next write — reported as a successful delete of
    /// something else. Deleting a link removes the link's entry and nothing
    /// else, as pfs3aio's `DeleteObject` sends a link to `DeleteLink`, which
    /// never frees the object (`directory.c:1778-1783,3835-3895`). Reopened
    /// from the device: Dst's content, its block in the bitmap, its anode
    /// and the free count are all as they were; and Dst itself, its link
    /// gone, deletes as any file does.
    #[test]
    fn deleting_a_pfs3_hard_link_removes_only_its_entry_and_the_file_it_names_keeps_its_blocks() {
        let dev = MemDevice::new();
        pfs3_mutator_fixture(&dev, false);
        let (dst, dst_block, free_before) = {
            let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let dst = vol.lookup("Dst").unwrap().unwrap().anode;
            let blk = vol.get_anode_chain(dst).unwrap()[0].blocknr;
            (dst, blk, vol.rootblock.blocksfree)
        };
        pfs3_append_raw_entries(
            &dev,
            PFS3_ROOT,
            &[pfs3_raw_entry(LINKFILE, dst, 0, "DstLink", 0)],
        );
        let anode_before = pfs3_raw_anode(&dev, dst);
        {
            let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            w.delete_in(PFS3_ROOT, "DstLink").unwrap();
        }
        let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
        assert!(vol.lookup("DstLink").unwrap().is_none(), "the link is gone");
        assert_eq!(
            vol.read_file("Dst").ok().as_deref(),
            Some(&b"destination bytes"[..]),
            "deleting a hard link to Dst emptied Dst"
        );
        assert!(
            !pfs3_data_block_free_on_disk(&dev, dst_block),
            "Dst's block {dst_block} is free in the bitmap after deleting a link to it"
        );
        assert_eq!(pfs3_raw_anode(&dev, dst), anode_before, "Dst's anode");
        assert_eq!(vol.rootblock.blocksfree, free_before, "blocksfree");
        drop(vol);
        {
            let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            w.delete_in(PFS3_ROOT, "Dst").unwrap();
        }
        assert!(
            pfs3_data_block_free_on_disk(&dev, dst_block),
            "Dst, its link gone, must delete as any file does"
        );
    }

    /// **ART-323, a link pfs3aio made.** Its entry's anode is its own link
    /// node, and deleting it means taking that node out of the object's
    /// chain of links — rewriting the object's `link` field when the link
    /// is the head, or the previous node's `next` — before freeing the node
    /// (`DeleteLink`, `directory.c:3835-3895`). This writer does not do
    /// that, so it refuses by name before anything is staged, and the
    /// volume is exactly its last commit. 0.1.3 read the link node as a data
    /// chain, cleared it and returned `Ok`, leaving the object's chain
    /// naming a node that was free.
    #[test]
    fn deleting_a_pfs3aio_hard_link_is_refused_by_name_and_leaves_the_last_commit() {
        let dev = MemDevice::new();
        pfs3_mutator_fixture(&dev, false);
        let lx = pfs3aio_linked_file(&dev, true, true);
        assert_pfs3_failure_leaves_the_last_commit(
            &dev,
            |w| w.delete_in(lx.dir, "Lnk"),
            |e| {
                e.to_string()
                    == "'Lnk' is a hard link pfs3aio made, and this writer cannot take it out of \
                        its object's chain of links, so it did not delete it — delete it on the \
                        Amiga"
            },
            |_| {},
        );
        let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
        assert_eq!(vol.read_file("Links/Obj").unwrap(), b"linked bytes");
    }

    /// **ART-323, the object a link names.** A file or a directory that a
    /// hard link (0.1.3's shape) still names is refused by name, and the
    /// volume is its last commit: deleting it would free what the link
    /// names. pfs3aio instead promotes the first link to be the object
    /// (`RemapLinks`, `directory.c:1795-1799,3903-3965`), which this writer
    /// does not do. 0.1.3 deleted it and left the link naming freed anodes.
    #[test]
    fn deleting_a_pfs3_file_or_directory_a_hard_link_names_is_refused_by_name() {
        for (target, link, link_type) in [
            ("Dst", "DstLink", LINKFILE),
            ("Empty", "EmptyLink", LINKDIR),
        ] {
            let dev = MemDevice::new();
            pfs3_mutator_fixture(&dev, false);
            {
                let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
                let mut w = libpfs3::writer::Writer::open(vol).unwrap();
                w.set_entry_date(Some(PFS3_FIXTURE_DATE));
                w.create_dir("Empty").unwrap();
            }
            let anode = {
                let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
                vol.lookup(target).unwrap().unwrap().anode
            };
            pfs3_append_raw_entries(
                &dev,
                PFS3_ROOT,
                &[pfs3_raw_entry(link_type, anode, 0, link, 0)],
            );
            let expected = has_links_sentence(target, &format!("'{link}'"));
            assert_pfs3_failure_leaves_the_last_commit(
                &dev,
                |w| w.delete_in(PFS3_ROOT, target),
                |e| e.to_string() == expected,
                |_| {},
            );
            if target == "Dst" {
                let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
                assert_eq!(vol.read_file("Dst").unwrap(), b"destination bytes");
            }
        }
    }

    /// **ART-323, a file pfs3aio linked.** Refused by name in each of three
    /// states: its chain and its link both there (the link is found and
    /// named); its own `link` field gone, the link still naming it (found
    /// by the link's `link` field); its chain there, the link's entry gone
    /// (named by the chain — pfs3aio would discard such a node and delete,
    /// `directory.c:3922-3934`; this writer refuses rather than guess what a
    /// chain it cannot walk safely holds).
    #[test]
    fn deleting_a_pfs3aio_linked_file_is_refused_whether_its_chain_or_its_link_names_it() {
        for (head, link_entry) in [(true, true), (false, true), (true, false)] {
            let dev = MemDevice::new();
            pfs3_mutator_fixture(&dev, false);
            let lx = pfs3aio_linked_file(&dev, head, link_entry);
            let links = if link_entry {
                "'Links/Lnk'".to_string()
            } else {
                format!("a pfs3aio link chain from anode {}", lx.node)
            };
            let expected = has_links_sentence("Obj", &links);
            assert_pfs3_failure_leaves_the_last_commit(
                &dev,
                |w| w.delete_in(lx.dir, "Obj"),
                |e| e.to_string() == expected,
                |_| {},
            );
            let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            assert_eq!(
                vol.read_file("Links/Obj").unwrap(),
                b"linked bytes",
                "head {head}, link entry {link_entry}"
            );
        }
    }

    /// **Final review fix wave, M1.** An Amiga stores a name's bytes as
    /// Latin-1, one byte a character, and libpfs3 reads them that way: "Äa"
    /// is `C4 61`. A case-only rename to "ÄA" measured and wrote the new
    /// name's UTF-8 bytes (`C3 84 41`), three against two, and refused a
    /// healthy volume as "corrupt filesystem". It must rename in place, to
    /// `C4 41`, keeping the entry's anode and content.
    #[test]
    fn a_pfs3_case_only_rename_of_a_latin1_name_rewrites_its_latin1_bytes_in_place() {
        let dev = MemDevice::new();
        pfs3_mutator_fixture(&dev, false);
        {
            let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            w.write_file("Xa", b"latin-1 bytes").unwrap();
        }
        let root_block = {
            let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            u64::from(vol.get_anode_chain(PFS3_ROOT).unwrap()[0].blocknr)
        };
        // The name as an Amiga would have written it: `C4 61`, not `58 61`.
        let mut block = dev.read(root_block, 1024);
        let at = block
            .windows(3)
            .position(|w| w == [2, b'X', b'a'])
            .expect("the entry's name length byte and name");
        block[at + 1] = 0xC4;
        dev.patch(root_block, &block);
        let anode = {
            let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            vol.lookup("\u{c4}a").unwrap().unwrap().anode
        };
        {
            let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            w.rename_in(PFS3_ROOT, "\u{c4}a", PFS3_ROOT, "\u{c4}A")
                .unwrap();
        }
        let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
        let entries = vol.list_dir_by_anode(PFS3_ROOT).unwrap();
        let renamed: Vec<_> = entries
            .iter()
            .filter(|e| e.name.eq_ignore_ascii_case("\u{c4}A"))
            .collect();
        assert_eq!(renamed.len(), 1, "{entries:#?}");
        assert_eq!(renamed[0].name, "\u{c4}A");
        assert_eq!(
            renamed[0].anode, anode,
            "the renamed entry must keep its anode"
        );
        assert!(
            dev.read(root_block, 1024)
                .windows(3)
                .any(|w| w == [2, 0xC4, b'A']),
            "the new name must be stored as Latin-1 `C4 41`"
        );
        assert_eq!(
            vol.read_file_data(anode, renamed[0].file_size()).unwrap(),
            b"latin-1 bytes"
        );
    }

    /// **Final review fix wave, M2.** Copy-on-write takes its new blocks from
    /// the committed bitmap, which must not offer the old file's own blocks.
    /// A corrupt bitmap that marks one of them free let `alloc_data_blocks`
    /// hand it out: the new content went over the old file on the device
    /// before any commit, and the commit then freed a block the new chain
    /// uses. Refused as corruption, before a single write, and the old
    /// content stays.
    #[test]
    fn a_pfs3_overwrite_refuses_a_bitmap_that_offers_the_old_files_own_block() {
        let dev = MemDevice::new();
        let fx = pfs3_mutator_fixture(&dev, false);
        let old_blocks: Vec<u32> = {
            let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            vol.get_anode_chain(fx.file)
                .unwrap()
                .iter()
                .flat_map(|a| (0..a.clustersize).map(move |i| a.blocknr + i))
                .collect()
        };
        pfs3_mark_data_block_free_on_disk(&dev, old_blocks[0]);
        {
            let vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
            let mut w = libpfs3::writer::Writer::open(vol).unwrap();
            let before = dev.write_count();
            let result = w.overwrite_file_in(PFS3_ROOT, "File", fx.file, &pfs3_bytes(9, 700));
            assert!(
                matches!(result, Err(libpfs3::error::Error::Corrupt(_))),
                "a bitmap offering the old file's own block {} must be refused as corruption, \
                 got: {result:?}",
                old_blocks[0]
            );
            assert_eq!(dev.write_count(), before, "a refused overwrite wrote");
        }
        let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
        assert!(
            vol.read_file("File").unwrap() == pfs3_bytes(1, 700),
            "the old content was overwritten on the device"
        );
    }

    #[test]
    fn a_pfs3_overwrite_commit_failure_locks_the_writer() {
        let dev = MemDevice::new();
        let fx = pfs3_mutator_fixture(&dev, false);
        assert_pfs3_commit_failure_locks(&dev, 3, |w| {
            w.overwrite_file_in(PFS3_ROOT, "File", fx.file, &pfs3_bytes(9, 1300))
        });
    }

    #[test]
    fn a_pfs3_rename_commit_failure_locks_the_writer() {
        let dev = MemDevice::new();
        let fx = pfs3_mutator_fixture(&dev, false);
        assert_pfs3_commit_failure_locks(&dev, 0, |w| w.rename_in(fx.dir, "Src", PFS3_ROOT, "Dst"));
    }

    // The mutators ART-319 wrapped in `guarded` without a test of their own.
    // Each failure below happens after the call allocated or staged
    // something, except where the mutator has no such point (said there).

    /// `create_dir`: the new directory's reserved block and anode are taken
    /// and its block staged before `add_dir_entry` finds "Broken" has no
    /// directory block.
    #[test]
    fn a_failed_pfs3_create_dir_leaves_the_last_commit() {
        let dev = MemDevice::new();
        pfs3_mutator_fixture(&dev, false);
        assert_pfs3_failure_leaves_the_last_commit(
            &dev,
            |w| w.create_dir("Broken/Sub"),
            is_corrupt,
            |_| {},
        );
    }

    #[test]
    fn a_pfs3_create_dir_commit_failure_locks_the_writer() {
        let dev = MemDevice::new();
        pfs3_mutator_fixture(&dev, false);
        assert_pfs3_commit_failure_locks(&dev, 0, |w| w.create_dir("NewDir"));
    }

    #[test]
    fn a_failed_pfs3_create_dir_in_leaves_the_last_commit() {
        let dev = MemDevice::new();
        let fx = pfs3_mutator_fixture(&dev, false);
        assert_pfs3_failure_leaves_the_last_commit(
            &dev,
            |w| w.create_dir_in(fx.broken, "Sub"),
            is_corrupt,
            |_| {},
        );
    }

    #[test]
    fn a_pfs3_create_dir_in_commit_failure_locks_the_writer() {
        let dev = MemDevice::new();
        pfs3_mutator_fixture(&dev, false);
        assert_pfs3_commit_failure_locks(&dev, 0, |w| w.create_dir_in(PFS3_ROOT, "NewDir"));
    }

    /// `create_softlink`: the target's data block is allocated and written,
    /// and its anode taken, before `add_dir_entry` fails in "Broken".
    #[test]
    fn a_failed_pfs3_create_softlink_leaves_the_last_commit() {
        let dev = MemDevice::new();
        pfs3_mutator_fixture(&dev, false);
        assert_pfs3_failure_leaves_the_last_commit(
            &dev,
            |w| w.create_softlink("Broken/Link", "File"),
            is_corrupt,
            |_| {},
        );
    }

    #[test]
    fn a_pfs3_create_softlink_commit_failure_locks_the_writer() {
        let dev = MemDevice::new();
        pfs3_mutator_fixture(&dev, false);
        assert_pfs3_commit_failure_locks(&dev, 1, |w| w.create_softlink("Link", "File"));
    }

    #[test]
    fn a_failed_pfs3_create_softlink_in_leaves_the_last_commit() {
        let dev = MemDevice::new();
        let fx = pfs3_mutator_fixture(&dev, false);
        assert_pfs3_failure_leaves_the_last_commit(
            &dev,
            |w| w.create_softlink_in(fx.broken, "Link", "File"),
            is_corrupt,
            |_| {},
        );
    }

    #[test]
    fn a_pfs3_create_softlink_in_commit_failure_locks_the_writer() {
        let dev = MemDevice::new();
        pfs3_mutator_fixture(&dev, false);
        assert_pfs3_commit_failure_locks(&dev, 1, |w| {
            w.create_softlink_in(PFS3_ROOT, "Link", "File")
        });
    }

    /// **ART-323.** `create_hardlink` wrote a link as 0.1.3 shaped it: the
    /// linked file's anode in the link's own entry, no link node, no chain.
    /// pfs3aio's `CreateLink` gives the link a node of its own, puts the
    /// object's anode in the link's `link` field and adds the node to the
    /// object's chain (`directory.c:2672-2748`) — so a link this writer made
    /// was not a pfs3aio link. It is refused by name before anything is
    /// read or written; the refusal discards back to the last commit, and
    /// the writer is not locked. This replaces the discard and lock tests
    /// `create_hardlink` had, which needed it to reach its commit. ART's
    /// product code never creates a link.
    #[test]
    fn pfs3_create_hardlink_is_refused_by_name_and_writes_nothing() {
        let dev = MemDevice::new();
        let fx = pfs3_mutator_fixture(&dev, false);
        let before = dev.write_count();
        assert_pfs3_failure_leaves_the_last_commit(
            &dev,
            |w| w.create_hardlink("Hard", fx.file),
            |e| {
                e.to_string()
                    == "hard link 'Hard' was not created: this writer cannot write pfs3aio's \
                        hard-link format (a link anode and the linked object's chain of links)"
            },
            |d| {
                assert_eq!(
                    d.write_count(),
                    before,
                    "a refused link wrote to the device"
                )
            },
        );
        let mut vol = libpfs3::volume::Volume::from_device(Box::new(dev.clone())).unwrap();
        assert!(vol.lookup("Hard").unwrap().is_none());
    }

    /// `undelete`: the deleted file's data is read, a new block allocated
    /// and written and its anode taken, before `add_dir_entry` fails in
    /// "Broken".
    #[test]
    fn a_failed_pfs3_undelete_leaves_the_last_commit() {
        let dev = MemDevice::new();
        pfs3_mutator_fixture(&dev, true);
        assert_pfs3_failure_leaves_the_last_commit(
            &dev,
            |w| w.undelete(0, "Broken/Back"),
            is_corrupt,
            |_| {},
        );
    }

    #[test]
    fn a_pfs3_undelete_commit_failure_locks_the_writer() {
        let dev = MemDevice::new();
        pfs3_mutator_fixture(&dev, true);
        assert_pfs3_commit_failure_locks(&dev, 1, |w| w.undelete(0, "Back"));
    }

    /// `force_remove_entry` has no point between staging and its commit: its
    /// one staged write is its last step, so the only failure before the
    /// commit is the refusal before it. What this pins is that a refused
    /// call leaves the last commit and a writer that still commits.
    #[test]
    fn a_refused_pfs3_force_remove_entry_leaves_the_last_commit() {
        let dev = MemDevice::new();
        pfs3_mutator_fixture(&dev, false);
        assert_pfs3_failure_leaves_the_last_commit(
            &dev,
            |w| w.force_remove_entry(PFS3_ROOT, "NoSuchName"),
            is_not_found,
            |_| {},
        );
    }

    #[test]
    fn a_pfs3_force_remove_entry_commit_failure_locks_the_writer() {
        let dev = MemDevice::new();
        pfs3_mutator_fixture(&dev, false);
        assert_pfs3_commit_failure_locks(&dev, 0, |w| w.force_remove_entry(PFS3_ROOT, "Dst"));
    }

    /// `update_dir_entry_protection`: the same shape as `force_remove_entry`
    /// — one staged write, its last step before the commit.
    #[test]
    fn a_refused_pfs3_update_dir_entry_protection_leaves_the_last_commit() {
        let dev = MemDevice::new();
        pfs3_mutator_fixture(&dev, false);
        assert_pfs3_failure_leaves_the_last_commit(
            &dev,
            |w| w.update_dir_entry_protection(PFS3_ROOT, "NoSuchName", 0x0F),
            is_not_found,
            |_| {},
        );
    }

    #[test]
    fn a_pfs3_update_dir_entry_protection_commit_failure_locks_the_writer() {
        let dev = MemDevice::new();
        pfs3_mutator_fixture(&dev, false);
        assert_pfs3_commit_failure_locks(&dev, 0, |w| {
            w.update_dir_entry_protection(PFS3_ROOT, "File", 0x0F)
        });
    }

    /// `repair_reserved_free` sets one in-memory field and commits; its only
    /// failure is the commit's. Until now it was exercised only as the second,
    /// already-locked call.
    #[test]
    fn a_pfs3_repair_reserved_free_commit_failure_locks_the_writer() {
        let dev = MemDevice::new();
        pfs3_mutator_fixture(&dev, false);
        assert_pfs3_commit_failure_locks(&dev, 0, |w| w.repair_reserved_free(7));
    }
}
