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
//! ## `import_filesystem` refuses
//!
//! The trait method exists to embed a filesystem driver into an **already
//! built** card's RDB, in place, without disturbing the partitions already on
//! it. `core::card::build::create_rdb_layout` cannot do that: it builds an
//! RDB **from scratch**, laying out every partition's cylinders in order from
//! a `size_mb` figure. Reconstructing an existing area's boundaries from
//! `ParsedPartition` and feeding them back in is not just fragile — for many
//! `size_mb` values it is impossible. `create_rdb_layout`'s cylinder count is
//! `ceil(size_mb * 1_048_576 / bytes_per_cyl)`, where `bytes_per_cyl` is
//! `516_096`. Consecutive integer `size_mb` values step the resulting
//! cylinder count by roughly 2.03, so **most integer cylinder counts have no
//! `size_mb` that reproduces them** — there is no lossless round trip from
//! "this partition currently spans N cylinders" back to a whole-megabyte
//! request that regenerates exactly N. Rebuilding the RDB with a
//! reconstruction that lands even one cylinder off would silently move every
//! partition after the first, which is only harmless the moment nothing has
//! been formatted or filled yet on that area — a card returned to for a
//! second partition is exactly the case where it would not be. So this method
//! refuses, by name, rather than produce a plausible-looking corruption. A
//! card built with its filesystem driver already embedded
//! (`core/card/build.rs::AreaSpec::file_systems`), or `hst-imager`'s own
//! `import_filesystem`, are the paths that actually serve this case.
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

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::core::adf::bcpl::{write_bcpl_string, AmigaDate};
use crate::core::adf::blocks::{
    bit_position, block_subtype, block_type, BLOCKS_PER_BITMAP_BLOCK, HASH_TABLE_SIZE,
};
use crate::core::adf::bootblock::BootBlock;
use crate::core::adf::checksum::block_checksum;
use crate::core::card::{read_card, AmigaArea, CardImage};
use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;
use crate::core::preload::pfs3dev::ArtBlockDevice;
use crate::core::preload::{CopySummary, ToolVersion, VolumeFormatter};
use crate::core::rdb::ParsedPartition;
use crate::core::volume::device::FileRegionMut;
use crate::core::volume::write::{dir, uaem, write_refusal, FileMeta, VolumeWriter};
use crate::core::volume::{BlockDevice, BlockDeviceMut, DosType, VolumeGeometry};

/// The version pinned in `Cargo.toml`. There is no `CARGO_PKG_VERSION`-style
/// macro for a *dependency's* version, so this is kept in sync by hand — the
/// same trade-off ART already accepts for `ureq`'s exact `=3.2.1` pin
/// (CLAUDE.md). `libpfs3` is pinned exactly (`=0.1.3`) for the same reason:
/// `probe()` reports this constant as which implementation did the work, and
/// an unpinned `cargo update` drifting past it would make that report state
/// a version nobody actually built. `the_pinned_version_constant_matches_cargo_toml`
/// (below) is what turns "kept in sync by hand" into something a `cargo
/// update` cannot get away with silently.
const LIBPFS3_VERSION: &str = "0.1.3";

/// A [`VolumeFormatter`] backed by `libpfs3` and ART's own FFS writer.
/// Launches nothing; see the module docs for what each method actually does.
pub struct NativeFormatter;

impl VolumeFormatter for NativeFormatter {
    fn probe(&self) -> CoreResult<ToolVersion> {
        Ok(ToolVersion {
            raw: format!("libpfs3 {LIBPFS3_VERSION} (native, no external tool)"),
        })
    }

    /// See the module docs: this cannot be done safely with
    /// `create_rdb_layout`'s shape, so it refuses rather than guess.
    fn import_filesystem(
        &self,
        _image: &Path,
        _slot: Option<usize>,
        _driver: &Path,
        _dostype: &str,
        _name: &str,
        _sink: &dyn ProgressSink,
    ) -> CoreResult<()> {
        Err(CoreError::NotImplemented(
            "NativeFormatter cannot embed a filesystem driver into an existing card's RDB in \
             place — create_rdb_layout only builds a partition table from scratch, and there is \
             no whole-megabyte size that reproduces every existing partition's cylinder \
             boundaries exactly. Build the card with the driver already embedded, or use \
             hst-imager's import_filesystem."
                .into(),
        ))
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
                    enable_deldir: false,
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
                format_ffs_volume(&mut region, &geometry, &checked_name)?;
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

    fn copy_in(
        &self,
        image: &Path,
        slot: Option<usize>,
        drive: &str,
        source: &Path,
        sink: &dyn ProgressSink,
    ) -> CoreResult<CopySummary> {
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
        let dos = DosType::new(part.dostype.to_be_bytes());

        let entries = collect_entries(source)?;

        match family_of(dos) {
            DosFamily::Pfs3 => copy_in_pfs3(
                image, offset, length, block_size, drive, source, &entries, sink,
            ),
            DosFamily::Ffs => copy_in_ffs(
                image,
                offset,
                length,
                block_size,
                dos,
                part.reserved,
                drive,
                source,
                &entries,
                sink,
            ),
            DosFamily::Other => Err(CoreError::UnsupportedFormat(format!(
                "{} is not a filesystem NativeFormatter can copy into",
                dos.label()
            ))),
        }
    }
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
/// offset zero, the same convention `core/preload/mod.rs::mbr_slot_of` uses in
/// the other direction.
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
    let now = current_amiga_date();
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

/// The clock, as an Amiga timestamp. Mirrors the private helper of the same
/// shape in `core/adf/create.rs`, which is not `pub` and belongs to a
/// fixed-geometry (floppy) creator this module does not otherwise depend on.
fn current_amiga_date() -> AmigaDate {
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(crate::core::adf::bcpl::AMIGA_EPOCH_UNIX);

    let secs_since_amiga = (now_unix - crate::core::adf::bcpl::AMIGA_EPOCH_UNIX).max(0);
    let days = (secs_since_amiga / 86_400) as u32;
    let rem_secs = secs_since_amiga % 86_400;
    let mins = (rem_secs / 60) as u32;
    let ticks = ((rem_secs % 60) * 50) as u32;

    AmigaDate { days, mins, ticks }
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

/// Flatten `source` into an ordered list: a directory always appears before
/// anything inside it, which is what lets `copy_in` look its parent's anode
/// or header block up in a map built as it goes, rather than re-walking the
/// volume for every entry.
///
/// A `.uaem` sidecar is never included as an entry of its own (§ binding
/// requirement 1) — it is read later, beside the file it describes.
fn collect_entries(source: &Path) -> CoreResult<Vec<CopyEntry>> {
    let mut out = Vec::new();
    collect_into(source, "", &mut out)?;
    Ok(out)
}

fn collect_into(dir: &Path, prefix: &str, out: &mut Vec<CopyEntry>) -> CoreResult<()> {
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

        let name = entry
            .file_name()
            .to_str()
            .ok_or(CoreError::NonUtf8Path)?
            .to_string();
        let relative = if prefix.is_empty() {
            name
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
            collect_into(&path, &relative, out)?;
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
) -> CoreResult<CopySummary> {
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

    let mut writer = libpfs3::writer::Writer::open(vol).map_err(from_pfs3)?;
    let mut summary = CopySummary::default();
    let mut anode_of: HashMap<String, u32> = HashMap::new();
    anode_of.insert(String::new(), libpfs3::ondisk::ANODE_ROOTDIR);

    let total = entries.len() as u64;
    for (done, entry) in entries.iter().enumerate() {
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
            summary.bytes += data.len() as u64;
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
            // libpfs3 0.1.3 exposes no way to set a directory entry's date —
            // only `update_dir_entry_protection`. The sidecar's date is
            // therefore read and then dropped here; FFS's `FileMeta` below
            // carries it because ART's own writer does have that setter.
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
) -> CoreResult<CopySummary> {
    let mut region = FileRegionMut::open(image, offset, length, block_size)?;
    let total_blocks = region.total_blocks();
    let geometry = VolumeGeometry::new(block_size, total_blocks, reserved, dos)?;

    // `VolumeWriter::open` requires an already-formatted, matching bootblock —
    // exactly the state `format_partition` leaves the volume in, and exactly
    // why `copy_in` never precedes `format_partition` in the plan (mod.rs).
    let mut writer = VolumeWriter::open(&mut region, geometry, image, offset)?;

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
            summary.bytes += data.len() as u64;
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
    use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec};

    /// Several tests share a tag (`"pds3"`, for instance) because they are
    /// building the same *kind* of fixture, not because they may share a
    /// directory: Cargo runs tests in parallel within one process, so the tag
    /// alone is not enough to keep two of them apart. A counter, not just the
    /// pid, is what actually guarantees a fresh directory per call.
    fn scratch(tag: &str) -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "art-preload-native-{tag}-{}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn card_with_partition(tag: &str, fs: AmigaHardDiskFs, size_mb: u32) -> PathBuf {
        let dir = scratch(tag);
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
        path
    }

    fn rdb_image_with_one_pds3_partition() -> PathBuf {
        card_with_partition("pds3", AmigaHardDiskFs::Pfs3DirectScsi, 8)
    }

    fn rdb_image_with_one_dos3_partition() -> PathBuf {
        card_with_partition("dos3", AmigaHardDiskFs::FfsDirCache, 8)
    }

    fn partition_offset(image: &Path) -> u64 {
        let card = read_card(image).unwrap();
        let part = &card.areas[0].rdb.partitions[0];
        card.areas[0].offset_bytes + part.byte_offset().unwrap()
    }

    fn formatted_pds3_image() -> PathBuf {
        let image = rdb_image_with_one_pds3_partition();
        NativeFormatter
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();
        image
    }

    /// A freshly formatted PFS3 partition of `mb` megabytes, on its own card.
    fn formatted_pds3_image_of(mb: u32) -> PathBuf {
        let path =
            card_with_partition(&format!("pds3-{mb}mb"), AmigaHardDiskFs::Pfs3DirectScsi, mb);
        NativeFormatter
            .format_partition(&path, None, 1, "Work", &NoProgress)
            .unwrap();
        path
    }

    fn tree_of_bytes(bytes: usize) -> PathBuf {
        let dir = scratch("big-tree");
        std::fs::write(dir.join("Big.bin"), vec![0xABu8; bytes]).unwrap();
        dir
    }

    /// Bytes free on a formatted PFS3 partition, read with `libpfs3`'s own
    /// reader — independent of `copy_in_pfs3`'s own arithmetic, which is
    /// exactly what a boundary test needs.
    fn pfs3_free_bytes(image: &Path) -> u64 {
        let vol = libpfs3::volume::Volume::open(image, partition_offset(image)).unwrap();
        u64::from(vol.free_blocks()) * u64::from(vol.block_size())
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
        let image = rdb_image_with_one_pds3_partition();
        NativeFormatter
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
        let image = formatted_pds3_image();
        let tree = fixtures::scratch("copy-in-protection");
        std::fs::create_dir_all(tree.join("C")).unwrap();
        std::fs::write(tree.join("C/Assign"), b"x").unwrap();
        std::fs::write(
            tree.join("C/Assign.uaem"),
            "--p-rwed 2021-04-13 02:43:13.68 \n",
        )
        .unwrap();

        NativeFormatter
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
        let image = formatted_pds3_image();
        let tree = fixtures::scratch("copy-in-sidecar-not-a-file");
        std::fs::create_dir_all(tree.join("C")).unwrap();
        std::fs::write(tree.join("C/Assign"), b"x").unwrap();
        std::fs::write(
            tree.join("C/Assign.uaem"),
            "--p-rwed 2021-04-13 02:43:13.68 \n",
        )
        .unwrap();

        NativeFormatter
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
        let image = formatted_pds3_image();
        let tree = fixtures::scratch("copy-in-summary");
        std::fs::create_dir_all(tree.join("C")).unwrap();
        std::fs::write(tree.join("C/Assign"), b"x").unwrap();

        let summary = NativeFormatter
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();
        assert_eq!(summary.files, 1);
        assert_eq!(summary.directories, 1);
    }

    #[test]
    fn it_stops_between_files_when_cancelled() {
        let image = formatted_pds3_image();
        let tree = fixtures::scratch("copy-in-cancel");
        std::fs::create_dir_all(tree.join("C")).unwrap();
        std::fs::write(tree.join("C/Assign"), b"x").unwrap();

        let err = NativeFormatter
            .copy_in(&image, None, "DH0", &tree, &fixtures::CancelAfter::new(1))
            .unwrap_err();
        assert!(matches!(err, CoreError::Cancelled), "{err}");

        // And nothing landed: cancellation stopped before "Assign" was written.
        let mut vol = libpfs3::volume::Volume::open(&image, partition_offset(&image)).unwrap();
        assert!(vol.list_dir("C").unwrap().is_empty());
    }

    #[test]
    fn a_tree_too_big_for_the_volume_is_refused_before_anything_is_written() {
        let image = formatted_pds3_image_of(4); // MB
        let tree = tree_of_bytes(8 * 1024 * 1024);
        let before = std::fs::read(&image).unwrap();

        let err = NativeFormatter
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
        let image = formatted_pds3_image_of(4);
        let free = pfs3_free_bytes(&image);
        let tree = tree_of_bytes(free as usize);

        NativeFormatter
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .expect("a tree that exactly fits must not be refused");
    }

    #[test]
    fn a_pfs3_tree_one_byte_over_the_limit_is_refused() {
        let image = formatted_pds3_image_of(4);
        let free = pfs3_free_bytes(&image);
        let tree = tree_of_bytes(free as usize + 1);
        let before = std::fs::read(&image).unwrap();

        let err = NativeFormatter
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
        let image = formatted_pds3_image_of(1);
        // One block per one-byte file, and comfortably more files than the
        // volume has free *blocks* — while their raw bytes (one each) are
        // nowhere near its free *bytes*. That gap is exactly what the old
        // check missed.
        let block_size = crate::core::volume::SECTOR_BYTES as u64;
        let file_count = pfs3_free_bytes(&image) / block_size + 100;

        let tree = fixtures::scratch("copy-in-many-small-pfs3");
        for i in 0..file_count {
            std::fs::write(tree.join(format!("F{i}")), b"x").unwrap();
        }
        let before = std::fs::read(&image).unwrap();

        let err = NativeFormatter
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
        let image = rdb_image_with_one_dos3_partition();
        NativeFormatter
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();
        let free = ffs_free_bytes(&image);
        let block_size = crate::core::volume::SECTOR_BYTES;
        let data_blocks = largest_single_ffs_file_data_blocks(free / block_size as u64, block_size);
        let tree = tree_of_bytes(data_blocks as usize * block_size);

        NativeFormatter
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .expect("a tree that exactly fits must not be refused");
    }

    #[test]
    fn an_ffs_tree_one_block_over_the_limit_is_refused() {
        let image = rdb_image_with_one_dos3_partition();
        NativeFormatter
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();
        let free = ffs_free_bytes(&image);
        let block_size = crate::core::volume::SECTOR_BYTES;
        let data_blocks = largest_single_ffs_file_data_blocks(free / block_size as u64, block_size);
        // One byte into the next data block: `budget_for` needs one more
        // data block (and, past the 72-block mark, sometimes one more
        // extension block too) — either way, strictly more than fits.
        let tree = tree_of_bytes(data_blocks as usize * block_size + 1);
        let before = std::fs::read(&image).unwrap();

        let err = NativeFormatter
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

    #[test]
    fn the_pinned_version_constant_matches_cargo_toml() {
        let cargo_toml = include_str!("../../../Cargo.toml");
        let expected = format!("libpfs3 = \"={LIBPFS3_VERSION}\"");
        assert!(
            cargo_toml.contains(&expected),
            "Cargo.toml's libpfs3 pin no longer matches LIBPFS3_VERSION \
             ({LIBPFS3_VERSION}) — update the constant (and what probe() \
             claims) together with the dependency bump"
        );
    }

    // ---- fix round 1, item 3: a directory's own sidecar must not be lost ----

    #[test]
    fn a_directorys_own_sidecar_is_applied_on_pfs3() {
        let image = formatted_pds3_image();
        let tree = fixtures::scratch("copy-in-dir-sidecar-pfs3");
        std::fs::create_dir_all(tree.join("C")).unwrap();
        std::fs::write(tree.join("C.uaem"), "--p-rwed 2021-04-13 02:43:13.68 \n").unwrap();

        NativeFormatter
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
        let image = rdb_image_with_one_dos3_partition();
        NativeFormatter
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();
        let tree = fixtures::scratch("copy-in-dir-sidecar-ffs");
        std::fs::create_dir_all(tree.join("C")).unwrap();
        std::fs::write(tree.join("C.uaem"), "--p-rwed 2021-04-13 02:43:13.68 \n").unwrap();

        NativeFormatter
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
        let image = rdb_image_with_one_dos3_partition();
        NativeFormatter
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();
        let tree = fixtures::scratch("ffs-copy-in-protection");
        std::fs::create_dir_all(tree.join("C")).unwrap();
        std::fs::write(tree.join("C/Assign"), b"x").unwrap();
        std::fs::write(
            tree.join("C/Assign.uaem"),
            "--p-rwed 2021-04-13 02:43:13.68 \n",
        )
        .unwrap();

        NativeFormatter
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
        let image = rdb_image_with_one_dos3_partition();
        NativeFormatter
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();
        let tree = fixtures::scratch("ffs-copy-in-sidecar-not-a-file");
        std::fs::create_dir_all(tree.join("C")).unwrap();
        std::fs::write(tree.join("C/Assign"), b"x").unwrap();
        std::fs::write(
            tree.join("C/Assign.uaem"),
            "--p-rwed 2021-04-13 02:43:13.68 \n",
        )
        .unwrap();

        NativeFormatter
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
        let image = rdb_image_with_one_pds3_partition();
        let err = NativeFormatter
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
        let image = formatted_pds3_image();
        let tree = fixtures::scratch("copy-in-populated-pfs3");
        std::fs::write(tree.join("First"), b"one").unwrap();
        NativeFormatter
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();

        let second = fixtures::scratch("copy-in-populated-pfs3-second");
        std::fs::write(second.join("Second"), b"two").unwrap();
        let err = NativeFormatter
            .copy_in(&image, None, "DH0", &second, &NoProgress)
            .unwrap_err();
        assert_eq!(err.code(), "ART-SAFETY-REFUSED", "{err}");
    }

    /// The FFS equivalent of the same requirement.
    #[test]
    fn copy_in_refuses_to_fill_an_already_populated_ffs_volume_again() {
        let image = rdb_image_with_one_dos3_partition();
        NativeFormatter
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();
        let tree = fixtures::scratch("copy-in-populated-ffs");
        std::fs::write(tree.join("First"), b"one").unwrap();
        NativeFormatter
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();

        let second = fixtures::scratch("copy-in-populated-ffs-second");
        std::fs::write(second.join("Second"), b"two").unwrap();
        let err = NativeFormatter
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
        let image = rdb_image_with_one_dos3_partition();
        NativeFormatter
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
        let image = rdb_image_with_one_dos3_partition();
        NativeFormatter
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
        let probed = NativeFormatter.probe().unwrap();
        assert!(probed.raw.contains("libpfs3"), "{}", probed.raw);
    }

    /// `import_filesystem` refuses by name rather than pretend — see the
    /// module docs for why `create_rdb_layout` cannot serve this case.
    #[test]
    fn import_filesystem_refuses_rather_than_guess() {
        let image = rdb_image_with_one_pds3_partition();
        let err = NativeFormatter
            .import_filesystem(
                &image,
                None,
                Path::new("pfs3aio.lha"),
                "PDS3",
                "pfs3aio",
                &NoProgress,
            )
            .unwrap_err();
        assert_eq!(err.code(), "ART-NOT-IMPLEMENTED", "{err}");
    }

    /// A DosType neither family claims — ART refuses rather than guessing.
    #[test]
    fn format_partition_refuses_a_filesystem_neither_family_claims() {
        let image = card_with_partition("sfs", AmigaHardDiskFs::Sfs0, 8);
        let err = NativeFormatter
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
        let image = PathBuf::from(&target);
        if let Some(parent) = image.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        crate::core::hdf::create_hdf(
            &image,
            220 * 1024 * 1024,
            true,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::Pfs3DirectScsi,
                size_mb: 200,
                bootable: true,
                boot_priority: 0,
                num_buffers: 0,
            }],
            &[],
        )
        .unwrap();

        NativeFormatter
            .format_partition(&image, None, 1, "Workbench", &NoProgress)
            .unwrap();

        // The literal bytes are named once and reused for both the write and
        // the JSON claim below, so a size or hash in the JSON can never drift
        // from what was actually handed to `copy_in`.
        let readme: &[u8] = b"hello from ART\n";
        let assign: &[u8] = b"ASSIGN\n";
        let startup: &[u8] = b"; a comment\n";
        let deep: &[u8] = b"deep\n";

        let tree = fixtures::scratch("pfs3-oracle-write");
        std::fs::create_dir_all(tree.join("C/Extra")).unwrap();
        std::fs::create_dir_all(tree.join("S")).unwrap();
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

        NativeFormatter
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
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
}
