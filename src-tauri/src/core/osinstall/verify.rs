//! Reading the volume back and checking it against the manifest (SD-2 · G5,
//! §92's VERIFY step).
//!
//! Task 9 copies the distribution tree onto a real PFS3 or FFS volume,
//! through `NativeFormatter::copy_in`, without reading anything back. This
//! module is the read: for every [`FileRecord`] in the manifest that Task 5
//! wrote, does the file that name names actually exist on the finished
//! volume, at the size and with the protection bits `apply()` recorded?
//!
//! ## This is the weakest of the three witnesses, on purpose stated plainly
//!
//! ART writing PFS3 with `libpfs3` and then reading it back with `libpfs3` is
//! a reader and a writer that agree with each other and with nothing else.
//! That exact shape has already cost this project four times over: ART-032
//! through ART-035 (the RDB fields a reader and a writer both got wrong the
//! same way), ART-075 (Mode 2 vs XA), and ART-079 — 7z handing one archive
//! entry another entry's bytes, a corruption every fixture ART built for
//! *itself* sailed through, because the fixture and the code being tested
//! shared the same understanding of the format. `verify_volume` cannot be the
//! proof that a preload is good. It is one witness among three:
//!
//! 1. **This module.** Runs on every machine, in CI, with no external tool.
//! 2. **Task 11's `hst-imager` oracle.** A second implementation of the same
//!    formats, so a bug shared by ART's writer and reader stops being
//!    invisible.
//! 3. **WinUAE, and then a real Amiga.** The only witnesses that run the
//!    68000 code the volume actually claims to carry.
//!
//! `verify_volume` runs first because it is cheap and automatic, not because
//! it is trusted most. Its own report says so on the PFS3 side — see below.
//!
//! ## Three states, kept apart (G8)
//!
//! [`CheckState::NotChecked`] is not a weaker kind of pass. A `NotChecked`
//! verdict means ART did not — could not, honestly — confirm the claim, and
//! rendering that as a tick is exactly the claim §89 forbids: `NotChecked`
//! **never** counts toward [`VerifyReport::passed`], and every `NotChecked`
//! [`FileVerdict`] carries a [`FileVerdict::detail`] saying why, never a bare
//! `None`. This is the same rule `docs/security-model.md`'s G8 already
//! applies to the card's FAT32 boot partition, whose files ART writes and
//! cannot read back at all — `core/preload/mod.rs`'s own module doc calls
//! that "a not-checked in G8's report", and this module extends the same
//! honesty to the Amiga side, field by field rather than as one blanket
//! refusal, now that `libpfs3` gives ART *some* real (if weak) way to look.
//!
//! ## Decision 1 — what each family can honestly claim, field by field
//!
//! **FFS/OFS** (`DOS\0`..`DOS\7` that ART writes — see `core/volume/write`'s
//! own table for which of those). Presence, size and content are all read
//! through the free functions `core/volume/write/dir.rs::find_entry` and
//! `core/volume/write/file.rs::read_file`, which is a genuinely different
//! code path from the one that wrote the file: `add_file` allocates blocks
//! from the bitmap and lays out a header and data chain; `read_file` walks
//! that chain back from the header block it just looked up by name, with no
//! memory of what the writer did a moment before. A byte a bug swapped
//! between two files — the exact shape of ART-079 — shows up here as a
//! content-hash mismatch, not agreement. So FFS gets real `Pass`/`Fail` on
//! **presence, size, content hash and protection** — `Pass` only when every
//! one of those was actually read and matched, `Fail` the moment any one
//! disagrees, and the first thing this module's own tests prove is that a
//! wrong protection bit reaches `Fail`, never a silent `Pass`
//! (`a_file_whose_protection_bits_are_wrong_is_a_fail_not_a_pass`).
//!
//! **Read-only, all the way down to the OS file handle.** `verify_ffs_files`
//! opens the image through `core::volume::device::FileRegion`, never
//! `FileRegionMut`, and reads through the same free functions
//! `VolumeWriter` itself calls internally rather than through `VolumeWriter`
//! — generic over `BlockDevice`, never `BlockDeviceMut`, exactly the
//! decision `core/osinstall/source.rs` already made for install media (and
//! that module's own doc comment states why: refusing to even *look* at a
//! file because Windows will not hand out a write lock is a self-inflicted
//! wound). Fix round 2 caught this the hard way: fix round 1 fixed the
//! *symptom* — a dircache volume producing `Err` instead of a report — by
//! moving the `write_refusal` check ahead of `VolumeWriter::open`, but
//! `VolumeWriter::open` was still being reached for every ordinary volume,
//! and the `FileRegionMut::open` before it opens the underlying file for
//! writing whether or not anything ever mutates it. A write-protected image,
//! one on read-only media, or one already open elsewhere all failed with a
//! plain permission error — not the dircache symptom fix round 1 chased, but
//! the same shape of bug: something ART could perfectly well have read
//! turned into a failed run instead of a report. Proven directly by
//! `a_read_only_image_file_still_produces_a_report`, which sets the real
//! Windows read-only attribute on a finished volume and confirms
//! `verify_volume` still succeeds.
//!
//! **Not writing is a different question from not reading.**
//! `core/volume/write/mod.rs::write_refusal` refuses a dircache volume
//! (`DOS\4`/`DOS\5`) and a non-512-byte-block partition for *writing*, and
//! says so explicitly: "Read support is a separate question and stays
//! exactly as it was." This module still reuses `write_refusal` as its own
//! gate — unchanged since fix round 1 — because nothing here has verified
//! `find_entry`/`read_file` against a real dircache-formatted fixture yet;
//! widening what counts as checkable is a real, separate improvement this
//! round deliberately left alone, having already spent its scope on *how*
//! the volume is opened rather than *which* volumes it will open. A refusal
//! still becomes a whole-manifest `NotChecked` carrying that reason, never a
//! failed run.
//!
//! **PFS3** (`PFS\x`/`PDS\x`). `libpfs3::writer::Writer` and
//! `libpfs3::volume::Volume` are the *same* third-party crate — not two
//! modules ART wrote independently, the way FFS's writer and reader are.
//! `Volume::lookup` still walks the on-disk directory structure by name
//! rather than trusting anything the writer remembered, so presence and the
//! directory entry's own `fsize`/`protection` fields are worth reading and do
//! reach `Fail` when they disagree with the manifest — a file that plainly
//! never landed, or landed with the wrong bits, is not a maybe. **Content is
//! different.** Reading a file's bytes back means walking its anode chain
//! with the same library that built that chain, following the same
//! assumptions about where the data lives — exactly the shape that let
//! ART-079 hide behind fixtures ART wrote for itself. So content is never
//! hashed for PFS3 here (Decision 2, next section), and a PFS3 file whose
//! presence, size and protection all check out still lands on
//! `NotChecked`, not `Pass` — its bytes were never independently confirmed,
//! and the report says so in `detail` rather than pretending otherwise. A
//! PFS3 file only reaches `Fail` (a real, checked disagreement) or
//! `NotChecked` (everything checkable checked out, content did not); it
//! never reaches `Pass`.
//!
//! **Neither family** (`SFS\0`, an unrecognised `DosType`). ART cannot even
//! open a reader, so every record is `NotChecked`, the same way `DosFamily::Other`
//! already refuses to *write* one in `native.rs`.
//!
//! A record whose [`FileRecord::protection`] is `None` — only ever
//! `S/User-Startup`'s composed records, which never had one intended value
//! to begin with (see `apply.rs`'s own field doc comment) — has nothing
//! asserted about its protection, so nothing there can disagree with the
//! volume. That is not the same thing as "could not be checked"; it is "the
//! manifest made no claim", so protection is simply not part of that one
//! file's check, and it does not by itself hold a file back from `Pass`.
//!
//! One more shape, PFS3-only: a manifest can carry a `protection` value that
//! does not fit the single byte PFS3 actually stores (`pfs3_protection`'s
//! own checked narrowing can fail). That is not the volume's fault to be
//! blamed for with a `Fail`, but it is genuinely *not checked* either — and
//! the `NotChecked` detail says exactly that, rather than folding it into
//! the same "protection matched" sentence a real match gets. Fix round 1
//! caught the original version of this saying "matched" unconditionally,
//! including when protection was never asserted or never fit — the detail
//! text now varies with what was actually true.
//!
//! ## Decision 2 — is PFS3 content worth re-hashing at all?
//!
//! No, deliberately. `libpfs3::volume::Volume::read_file` *can* return the
//! bytes; the question is whether doing so and comparing a hash would mean
//! anything. It would not add a check independent of the one the writer
//! already trusted — a bug in how `libpfs3` places data on an anode chain
//! would very plausibly place it wrong in a way both `write_file_in` and
//! `read_file_data` agree on, because they share the same understanding of
//! where the data is. Attempting the hash anyway and calling a match `Pass`
//! would manufacture exactly the false confidence this module exists to
//! avoid — the module doc above's whole point. Reporting it as `NotChecked`
//! costs nothing (the field was never going to prove anything) and states
//! the true situation instead of a fabricated `Pass`. The independent
//! content check for PFS3 is Task 11's `hst-imager` oracle, which is a
//! *different* implementation of the format — genuinely the other witness
//! this weak one needs.
//!
//! ## Decision 3 — a file on the volume that is not in the manifest
//!
//! `verify_volume` only ever looks up the paths [`DistributionManifest::files`]
//! names; it never walks the volume's own directory tree looking for extras.
//! The manifest is `apply()`'s own record of what *it* put there (see
//! `apply.rs`'s module doc: "the only record of what an install actually
//! did"), not a claim about everything the volume holds — a user may add
//! their own files to the same partition before ever running verify, and
//! that is none of `verify_volume`'s business to flag as a problem. Auditing
//! the whole volume for strangers would also make this module something it
//! is not: a full disk scan through the same weak PFS3 witness Decision 2
//! just declined to trust for content, now asked to enumerate a tree it has
//! no independent way to confirm either. So an extra file is simply invisible
//! to this report — proven directly by
//! `an_extra_file_on_the_volume_that_is_not_in_the_manifest_is_simply_invisible_to_the_report`
//! below, which plants one and shows the report is unaffected either way.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::core::card::read_card;
use crate::core::error::CoreResult;
use crate::core::hashing::sha256_bytes;
use crate::core::osinstall::apply::{DistributionManifest, FileRecord};
use crate::core::preload::native::{
    area_for_slot, family_of, from_pfs3, partition_by_index, partition_region, pfs3_protection,
    DosFamily,
};
use crate::core::volume::device::FileRegion;
use crate::core::volume::write::layout::{self, BlockSet, PROTECT_OFFSET};
use crate::core::volume::write::{dir, file, uaem, write_refusal};
use crate::core::volume::{read_block_vec, BlockDevice, DosType, VolumeGeometry};

/// Whether one claim about a file was confirmed, contradicted, or never
/// looked at. See the module doc comment — `NotChecked` is not a soft
/// `Pass`, and nothing in this module is allowed to treat it as one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CheckState {
    Pass,
    Fail,
    NotChecked,
}

/// What became of one [`FileRecord`].
///
/// `rename_all = "camelCase"` (Task 12 fix round 1): every field here happens
/// to be a single word today, so this was safe by luck rather than by
/// construction — a future field would not be. Explicit now, matching every
/// other type that crosses the command boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileVerdict {
    /// Matches [`FileRecord::path`] exactly, so the two line up by index or
    /// by lookup either way.
    pub path: String,
    pub state: CheckState,
    /// Why, when `state` is anything but a clean `Pass` with nothing to add.
    /// Mandatory reading whenever `state` is [`CheckState::NotChecked`] — see
    /// the module doc comment's G8 section.
    pub detail: Option<String>,
}

/// What reading the volume back found, one verdict per [`FileRecord`] in the
/// manifest — never more (Decision 3) and never fewer: every record gets
/// exactly one verdict, so `files.len() == manifest.files.len()` always.
///
/// `rename_all = "camelCase"` (Task 12 fix round 1): without it, `not_checked`
/// crossed the wire as `not_checked` while `src/lib/osinstall.ts` read
/// `notChecked` — always `undefined`, so `isVerified` always returned
/// `false` and any screen showing the count rendered nothing. Caught by
/// `commands::osinstall`'s own outbound wire-shape test, not by anything
/// here; see that module for why a Rust-only test could not have caught it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyReport {
    pub files: Vec<FileVerdict>,
    pub passed: usize,
    pub failed: usize,
    pub not_checked: usize,
}

/// Read `index`'s partition on `image` (see `core::card` for what `slot` and
/// `index` mean — one MBR slot, one partition inside that disk's own RDB) and
/// check every file `manifest` says `apply()` put there.
///
/// Structural failures — the image will not open, the slot or index does not
/// exist, the partition's own geometry cannot be computed — are a hard `Err`:
/// nothing here was verified, so there is no `VerifyReport` to hand back, the
/// same way `NativeFormatter::copy_in` refuses before writing anything rather
/// than reporting a doomed attempt as a summary. Once the volume itself opens,
/// every problem after that is a **verdict**, not an error: a missing file
/// does not stop the run, it becomes that one file's `Fail`.
pub fn verify_volume(
    image: &Path,
    slot: Option<usize>,
    index: usize,
    manifest: &DistributionManifest,
) -> CoreResult<VerifyReport> {
    let card = read_card(image)?;
    let area = area_for_slot(&card, slot)?;
    let part = partition_by_index(area, index)?;
    let (offset, length, block_size) = partition_region(area, part)?;
    let dos = DosType::new(part.dostype.to_be_bytes());

    let files = match family_of(dos) {
        DosFamily::Ffs => verify_ffs_files(
            image,
            offset,
            length,
            block_size,
            dos,
            part.reserved,
            manifest,
        )?,
        DosFamily::Pfs3 => verify_pfs3_files(image, offset, manifest)?,
        DosFamily::Other => manifest
            .files
            .iter()
            .map(|record| FileVerdict {
                path: record.path.clone(),
                state: CheckState::NotChecked,
                detail: Some(format!(
                    "{} is not a filesystem ART can open to verify",
                    dos.label()
                )),
            })
            .collect(),
    };

    Ok(summarize(files))
}

fn summarize(files: Vec<FileVerdict>) -> VerifyReport {
    let passed = files.iter().filter(|f| f.state == CheckState::Pass).count();
    let failed = files.iter().filter(|f| f.state == CheckState::Fail).count();
    let not_checked = files
        .iter()
        .filter(|f| f.state == CheckState::NotChecked)
        .count();
    VerifyReport {
        files,
        passed,
        failed,
        not_checked,
    }
}

// ---------------------------------------------------------------------------
// FFS/OFS — ART's own reader, a genuinely different path from the writer
// ---------------------------------------------------------------------------

/// Everything here is read-only end to end, on purpose, down to the OS file
/// handle — the exact decision `core/osinstall/source.rs` already made and
/// this project already endorsed for install media (that module's own doc
/// comment: "opening one means opening the underlying file **for write**
/// (`FileRegionMut`) even though nothing here ever calls a mutating
/// method... a user's install floppy image is exactly the kind of file that
/// gets archived read-only, and refusing to even *look* at it because
/// Windows will not hand out a write lock would be a self-inflicted wound").
/// A verifier has even less business asking for write access than a media
/// reader does — it is fundamentally a read, never a write, of an image the
/// user may have made read-only, put on read-only media, or already have
/// open elsewhere. `FileRegion` and the free functions `dir`/`file` are
/// built on (the very same ones `VolumeWriter` itself calls internally) are
/// generic over `BlockDevice`, never `BlockDeviceMut` — nothing below needs
/// a write handle at all. Fix round 2 caught this: fix round 1 moved the
/// `write_refusal` check ahead of `VolumeWriter::open`, which fixed a
/// dircache volume's hard `Err`, but `VolumeWriter::open` was still being
/// reached at all — and `FileRegionMut::open`, before it, opens the file for
/// write whether or not anything ever calls a mutating method.
#[allow(clippy::too_many_arguments)]
fn verify_ffs_files(
    image: &Path,
    offset: u64,
    length: u64,
    block_size: usize,
    dos: DosType,
    reserved: u32,
    manifest: &DistributionManifest,
) -> CoreResult<Vec<FileVerdict>> {
    let region = FileRegion::open(image, offset, length, block_size)?;
    let total_blocks = region.total_blocks();
    let geometry = VolumeGeometry::new(block_size, total_blocks, reserved, dos)?;

    // See fix round 1's own comment (module doc, Decision 1) for why a
    // refusal becomes a whole-manifest `NotChecked` rather than a failed
    // run. `write_refusal` is reused exactly as it stood before this round —
    // this round only changes *how* the volume is opened (read-only, never
    // through `VolumeWriter`), not which DosTypes a report can be produced
    // for. Widening that — dircache reading genuinely "stays on" per
    // CLAUDE.md's own table, unlike writing — is a real, separate
    // improvement, deliberately left for later: nothing here has verified
    // `dir`/`file`'s free functions against a real dircache-formatted
    // fixture, and this round is about the file handle, not about growing
    // what counts as checkable.
    if let Some(reason) = write_refusal(&geometry) {
        return Ok(manifest
            .files
            .iter()
            .map(|record| FileVerdict {
                path: record.path.clone(),
                state: CheckState::NotChecked,
                detail: Some(format!(
                    "ART's own volume reader will not open this partition: {reason}"
                )),
            })
            .collect());
    }

    let set = BlockSet::new(geometry.block_size);
    Ok(manifest
        .files
        .iter()
        .map(|record| verify_ffs_one(&region, &set, &geometry, record))
        .collect())
}

/// Walk `path` one `/`-separated segment at a time from the volume's own
/// root block. Never `0` as a stand-in for it — that convenience belongs to
/// `VolumeWriter::resolve_directory`, which nothing here calls any more;
/// `dir::find_entry` wants a real block number, the same way
/// `source.rs::AdfSource::resolve` already starts from `geometry.root_block`
/// rather than `0`.
fn find_ffs_path(
    device: &FileRegion,
    set: &BlockSet,
    geometry: &VolumeGeometry,
    path: &str,
) -> CoreResult<Option<u32>> {
    let mut current = geometry.root_block;
    for segment in path.split('/') {
        match dir::find_entry(device, set, geometry, current, segment)? {
            Some(entry) => current = entry.block,
            None => return Ok(None),
        }
    }
    Ok(Some(current))
}

fn verify_ffs_one(
    device: &FileRegion,
    set: &BlockSet,
    geometry: &VolumeGeometry,
    record: &FileRecord,
) -> FileVerdict {
    let block = match find_ffs_path(device, set, geometry, &record.path) {
        Ok(Some(block)) => block,
        Ok(None) => {
            return fail(&record.path, "not found on the volume");
        }
        Err(err) => {
            return fail(&record.path, format!("its path could not be read: {err}"));
        }
    };

    let bytes = match file::read_file(device, set, geometry, block) {
        Ok(bytes) => bytes,
        Err(err) => {
            return fail(
                &record.path,
                format!("its content could not be read back: {err}"),
            );
        }
    };
    // The same fields `source.rs::AdfSource::entry_at` reads, at the same
    // offsets, off the same raw header block — straight through
    // `layout::get_u32`, not `VolumeWriter::attributes`, which needs a
    // `BlockDeviceMut` for no reason a read ever has.
    let protection = match read_block_vec(device, block)
        .and_then(|header| layout::get_u32(&header, PROTECT_OFFSET))
    {
        Ok(protection) => protection,
        Err(err) => {
            return fail(
                &record.path,
                format!("its attributes could not be read back: {err}"),
            );
        }
    };

    let mut problems = Vec::new();
    if bytes.len() as u64 != record.bytes {
        problems.push(format!(
            "size is {} bytes, the manifest says {}",
            bytes.len(),
            record.bytes
        ));
    }
    let actual_sha256 = sha256_bytes(&bytes);
    if actual_sha256 != record.sha256 {
        problems.push("its content does not match the manifest's sha256".to_string());
    }
    if let Some(expected) = record.protection {
        if protection != expected {
            problems.push(format!(
                "protection is {}, the manifest says {}",
                uaem::format_bits(protection),
                uaem::format_bits(expected)
            ));
        }
    }

    if problems.is_empty() {
        pass(&record.path)
    } else {
        fail(&record.path, problems.join("; "))
    }
}

// ---------------------------------------------------------------------------
// PFS3 — the weak witness. See the module doc comment's Decisions 1 and 2.
// ---------------------------------------------------------------------------

fn verify_pfs3_files(
    image: &Path,
    offset: u64,
    manifest: &DistributionManifest,
) -> CoreResult<Vec<FileVerdict>> {
    let mut vol = libpfs3::volume::Volume::open(image, offset).map_err(from_pfs3)?;
    Ok(manifest
        .files
        .iter()
        .map(|record| verify_pfs3_one(&mut vol, record))
        .collect())
}

/// The tail every PFS3 `NotChecked` detail carries, whatever else it says:
/// content is never re-hashed on this family. See Decision 2.
const PFS3_CONTENT_NOT_CHECKED_TAIL: &str = "PFS3 has no reader in ART other than the library \
     that wrote it, so its content was not re-hashed here. See Task 11's independent \
     hst-imager oracle.";

fn verify_pfs3_one(vol: &mut libpfs3::volume::Volume, record: &FileRecord) -> FileVerdict {
    let entry = match vol.lookup(&record.path) {
        Ok(Some(entry)) => entry,
        Ok(None) => return fail(&record.path, "not found on the volume"),
        Err(err) => {
            return fail(&record.path, format!("its path could not be read: {err}"));
        }
    };

    let mut problems = Vec::new();

    if entry.file_size() != record.bytes {
        problems.push(format!(
            "size is {} bytes, the manifest says {}",
            entry.file_size(),
            record.bytes
        ));
    }

    // What became of the protection field, kept apart from `problems` — a
    // mismatch already fails this file below, but "matched", "not
    // asserted" and "the manifest's own expectation does not fit a PFS3
    // byte" are three different truths, and the `NotChecked` detail this
    // function may still return must say which one actually happened
    // rather than a single sentence that quietly overclaims the other two
    // (fix round 1, item 2). `None` means "matched, nothing to add";
    // `Some(note)` replaces "protection matched" in the final detail.
    let protection_note: Option<String> = match record.protection {
        None => Some("the manifest recorded no expected protection for this file".to_string()),
        Some(expected) => match pfs3_protection(expected) {
            Ok(expected_u8) => {
                if entry.protection != expected_u8 {
                    problems.push(format!(
                        "protection is {}, the manifest says {}",
                        libpfs3::util::amiga_protection_string(entry.protection),
                        libpfs3::util::amiga_protection_string(expected_u8)
                    ));
                }
                None
            }
            Err(_) => Some(format!(
                "protection was not checked — the manifest's expected protection \
                 ({expected:#x}) does not fit the single byte PFS3 actually stores, so \
                 there was nothing on the volume to compare it against"
            )),
        },
    };

    if !problems.is_empty() {
        return fail(&record.path, problems.join("; "));
    }

    let detail = match protection_note {
        None => format!("presence, size and protection matched. {PFS3_CONTENT_NOT_CHECKED_TAIL}"),
        Some(note) => format!("presence and size matched; {note}. {PFS3_CONTENT_NOT_CHECKED_TAIL}"),
    };

    FileVerdict {
        path: record.path.clone(),
        state: CheckState::NotChecked,
        detail: Some(detail),
    }
}

// ---------------------------------------------------------------------------

fn pass(path: &str) -> FileVerdict {
    FileVerdict {
        path: path.to_string(),
        state: CheckState::Pass,
        detail: None,
    }
}

fn fail(path: &str, detail: impl Into<String>) -> FileVerdict {
    FileVerdict {
        path: path.to_string(),
        state: CheckState::Fail,
        detail: Some(detail.into()),
    }
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::jobs::NoProgress;
    use crate::core::osinstall::fixtures;
    use crate::core::preload::{native::NativeFormatter, VolumeFormatter};
    use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec};
    use std::path::PathBuf;

    /// A counter, not just `tag`: several tests below call helpers like
    /// `written_volume()` that share a tag, and Cargo runs tests in parallel
    /// threads of the same process (same pid) — `fixtures::scratch` alone
    /// keys only on tag + pid, so two tests sharing a tag would race over the
    /// same directory. The same fix `apply.rs`'s own `planned()` already
    /// applies, for the exact same reason.
    fn scratch(tag: &str) -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        fixtures::scratch(&format!("verify-{tag}-{n}"))
    }

    /// A card with one partition of `fs`, sized `mb` megabytes. The backing
    /// file is `mb` MB plus `RDB_HEADROOM_MB` — `create_rdb_layout` refuses
    /// anything under 10 MB whole regardless of the one partition's own
    /// size (`core/rdb.rs`: "Hard disk image size must be at least 10 MB"),
    /// so an 8 MB partition still needs a card at least 10 MB, and the RDB's
    /// own reserved cylinders want a little room past the partition itself.
    /// Every caller in this file asks for 8 MB, so this stays comfortably
    /// inside that floor without carrying an unexplained 32 MB fixed size.
    const RDB_HEADROOM_MB: u64 = 2;

    fn card_with_partition(dir: &Path, fs: AmigaHardDiskFs, mb: u32) -> PathBuf {
        let path = dir.join("card.hdf");
        crate::core::hdf::create_hdf(
            &path,
            (mb as u64 + RDB_HEADROOM_MB) * 1024 * 1024,
            true,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: fs,
                size_mb: mb,
                bootable: true,
                boot_priority: 0,
                num_buffers: 0,
            }],
            &[],
        )
        .unwrap();
        path
    }

    /// A freshly formatted, empty FFS partition, ready for `copy_in`.
    fn formatted_ffs_image(dir: &Path) -> PathBuf {
        let image = card_with_partition(dir, AmigaHardDiskFs::FfsStandard, 8);
        NativeFormatter
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();
        image
    }

    /// A freshly formatted, empty PFS3 partition, ready for `copy_in`.
    fn formatted_pfs3_image(dir: &Path) -> PathBuf {
        let image = card_with_partition(dir, AmigaHardDiskFs::Pfs3DirectScsi, 8);
        NativeFormatter
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();
        image
    }

    /// `C/LoadModule`, `--p-rwed` on the media it was measured off in
    /// `apply.rs`'s own fixture — the exact bit AmigaOS 3.2's
    /// `Startup-Sequence` needs. `content` is what actually lands in the tree
    /// and, from there, on the volume; `sidecar_protection` is what the
    /// `.uaem` beside it claims — deliberately a separate knob, so a test can
    /// make the volume disagree with the manifest without touching the
    /// manifest at all.
    fn tree_with_load_module(dir: &Path, content: &[u8], sidecar_protection: u32) -> PathBuf {
        let tree = dir.join("tree");
        std::fs::create_dir_all(tree.join("C")).unwrap();
        std::fs::write(tree.join("C/LoadModule"), content).unwrap();
        std::fs::write(
            tree.join("C/LoadModule.uaem"),
            uaem::render(&uaem::Sidecar {
                protection: sidecar_protection,
                date: Default::default(),
                comment: String::new(),
            }),
        )
        .unwrap();
        tree
    }

    fn manifest_for_load_module(content: &[u8]) -> DistributionManifest {
        DistributionManifest {
            release: "AmigaOS 3.2".into(),
            built_from: Vec::new(),
            files: vec![FileRecord {
                path: "C/LoadModule".into(),
                component: "modules-a1200".into(),
                media: "ModulesA1200_3.2".into(),
                sha256: sha256_bytes(content),
                bytes: content.len() as u64,
                protection: Some(0x20), // --p-rwed: what apply() actually recorded
                overwrote: None,
            }],
            paired_rom: None,
        }
    }

    /// An FFS volume carrying exactly what its manifest says: content,
    /// size and protection all genuinely agree. Every field this module
    /// can check on FFS is checkable here, which is what lets
    /// `every_file_in_the_manifest_is_found_with_its_size_and_its_bits`
    /// legitimately expect every file to `Pass` — not a PFS3 fixture, whose
    /// content is never confirmed at all (Decision 2), which would make
    /// that same expectation false by this module's own design.
    fn written_volume() -> (PathBuf, DistributionManifest) {
        let dir = scratch("written-volume");
        let content = b"cmd";
        let image = formatted_ffs_image(&dir);
        let tree = tree_with_load_module(&dir, content, 0x20);
        NativeFormatter
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();
        (image, manifest_for_load_module(content))
    }

    /// The same manifest as `written_volume` — it expects `--p-rwed` — but
    /// the sidecar actually copied onto the volume drops the pure bit, so the
    /// volume and the manifest genuinely disagree about `C/LoadModule`'s
    /// protection. `written_volume`'s own content and size stay correct, so
    /// this isolates the one field under test.
    fn written_volume_with_the_pure_bit_dropped() -> (PathBuf, DistributionManifest) {
        let dir = scratch("pure-bit-dropped");
        let content = b"cmd";
        let image = formatted_ffs_image(&dir);
        let tree = tree_with_load_module(&dir, content, 0x00); // pure bit gone
        NativeFormatter
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();
        (image, manifest_for_load_module(content))
    }

    /// Fix round 2's own finding: `verify_ffs_files` used to open the image
    /// with a write handle (`FileRegionMut`, via `VolumeWriter::open`)
    /// regardless of fix round 1's `write_refusal` reordering — so a card
    /// image that is write-protected on disk, on read-only media, or held
    /// open elsewhere by something else still failed with a permission
    /// error instead of producing a report, exactly the failure Task 2 (and
    /// this project's endorsement of it, in `source.rs`) already ruled out
    /// for install media. Windows honours `set_readonly` as the real
    /// `FILE_ATTRIBUTE_READONLY` bit — the same one a user's write-protected
    /// SD card image would carry — so this sets it on a genuinely finished
    /// volume and confirms `verify_volume` still reports normally rather
    /// than failing to open the file at all. The permission is restored
    /// afterwards so the scratch directory can still be cleaned up (a
    /// leftover read-only file would make a future `remove_dir_all` over
    /// the same tag fail silently rather than actually clear it).
    #[test]
    fn a_read_only_image_file_still_produces_a_report() {
        let (image, manifest) = written_volume();

        let mut perms = std::fs::metadata(&image).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&image, perms).unwrap();

        let result = verify_volume(&image, None, 1, &manifest);

        // Restore write access before asserting, so a failed assertion does
        // not also leave a read-only file behind in the scratch directory.
        let mut perms = std::fs::metadata(&image).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        std::fs::set_permissions(&image, perms).unwrap();

        let report = result.expect("a read-only image must still produce a report, not an Err");
        assert_eq!(report.failed, 0, "{:?}", report.files);
        assert_eq!(report.passed, manifest.files.len());
    }

    // ---- Step 1's given tests ----

    #[test]
    fn every_file_in_the_manifest_is_found_with_its_size_and_its_bits() {
        let (image, manifest) = written_volume();
        let report = verify_volume(&image, None, 1, &manifest).unwrap();
        assert_eq!(report.failed, 0, "{:?}", report.files);
        assert_eq!(report.passed, manifest.files.len());
    }

    #[test]
    fn a_missing_file_is_a_fail_and_says_which_one() {
        let (image, mut manifest) = written_volume();
        manifest.files.push(FileRecord {
            path: "C/NeverWritten".into(),
            component: "workbench-base".into(),
            media: "Workbench3.2".into(),
            sha256: "0".repeat(64),
            bytes: 4,
            protection: None,
            overwrote: None,
        });
        let report = verify_volume(&image, None, 1, &manifest).unwrap();
        assert_eq!(report.failed, 1);
        assert!(report
            .files
            .iter()
            .any(|f| f.path == "C/NeverWritten" && f.state == CheckState::Fail));
    }

    /// FFS is the family this module claims re-hashes real bytes, not a
    /// number the writer merely remembers — proved by making the manifest's
    /// own recorded sha256 wrong for content that is genuinely, correctly on
    /// the volume, and confirming that disagreement is caught. Without this,
    /// a version of `verify_ffs_one` that skipped the hash comparison
    /// entirely would still pass every other test in this file — none of
    /// them corrupt *content* specifically.
    #[test]
    fn content_that_disagrees_with_the_manifests_sha256_is_a_fail() {
        let (image, mut manifest) = written_volume();
        manifest.files[0].sha256 = "0".repeat(64); // not b"cmd"'s real hash
        let report = verify_volume(&image, None, 1, &manifest).unwrap();
        assert_eq!(report.failed, 1);
        assert_eq!(report.files[0].state, CheckState::Fail);
        assert!(
            report.files[0]
                .detail
                .as_deref()
                .unwrap_or("")
                .contains("sha256"),
            "{:?}",
            report.files[0].detail
        );
    }

    /// The pure bit is why this check exists at all. Also checks *why* it
    /// failed, not just that it did (fix round 1, item 5) — the sibling
    /// content test already does this; a regression that failed this file
    /// for the wrong reason (a bogus size mismatch, say) would otherwise
    /// still turn this test green.
    #[test]
    fn a_file_whose_protection_bits_are_wrong_is_a_fail_not_a_pass() {
        let (image, manifest) = written_volume_with_the_pure_bit_dropped();
        let report = verify_volume(&image, None, 1, &manifest).unwrap();
        let verdict = report
            .files
            .iter()
            .find(|f| f.path == "C/LoadModule")
            .expect("C/LoadModule has a verdict");
        assert_eq!(verdict.state, CheckState::Fail);
        assert!(
            verdict
                .detail
                .as_deref()
                .unwrap_or("")
                .contains("protection"),
            "{:?}",
            verdict.detail
        );
    }

    /// §89 and G8's three states. ART reads the volume, not the file's bytes
    /// against their recorded hash on a volume it cannot fully re-read — and
    /// what it did not look at must never render as a tick.
    #[test]
    fn what_was_not_checked_is_its_own_state_and_never_a_pass() {
        let (image, manifest) = written_volume();
        let report = verify_volume(&image, None, 1, &manifest).unwrap();
        assert_eq!(
            report.passed + report.failed + report.not_checked,
            report.files.len(),
            "every file lands in exactly one of the three states"
        );
        assert!(
            report
                .files
                .iter()
                .all(|f| f.state != CheckState::NotChecked || f.detail.is_some()),
            "a not-checked verdict has to say why"
        );
    }

    // ---- This module's own additions: the properties `written_volume`
    // (FFS, everything checkable) cannot exercise on its own. ----

    /// The PFS3 half of Decision 1 and 2, made concrete: presence, size and
    /// protection all genuinely agree, and the verdict is still not `Pass` —
    /// content was never re-hashed, and that has to show.
    #[test]
    fn a_correct_pfs3_file_is_not_checked_not_passed() {
        let dir = scratch("pfs3-not-checked");
        let content = b"cmd";
        let image = formatted_pfs3_image(&dir);
        let tree = tree_with_load_module(&dir, content, 0x20);
        NativeFormatter
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();
        let manifest = manifest_for_load_module(content);

        let report = verify_volume(&image, None, 1, &manifest).unwrap();

        assert_eq!(
            report.passed, 0,
            "PFS3 content is never independently confirmed here"
        );
        assert_eq!(report.failed, 0);
        assert_eq!(report.not_checked, 1);
        let verdict = &report.files[0];
        assert_eq!(verdict.state, CheckState::NotChecked);
        assert!(
            verdict.detail.as_deref().unwrap_or("").contains("PFS3"),
            "{:?}",
            verdict.detail
        );
    }

    /// The other half: PFS3 still catches a real, checkable disagreement —
    /// `Fail`, not a shrug. Missing entirely is the simplest such
    /// disagreement, and reuses no FFS machinery at all.
    #[test]
    fn a_pfs3_file_missing_from_the_volume_is_a_fail_not_a_shrug() {
        let dir = scratch("pfs3-missing");
        let image = formatted_pfs3_image(&dir);
        let manifest = manifest_for_load_module(b"cmd"); // never copied in

        let report = verify_volume(&image, None, 1, &manifest).unwrap();

        assert_eq!(report.failed, 1);
        assert_eq!(report.not_checked, 0);
        assert_eq!(report.files[0].state, CheckState::Fail);
    }

    /// Fix round 1, item 3: the module doc claims PFS3 reaches `Fail` on a
    /// size disagreement too, not only on outright absence — pinned
    /// directly rather than left asserted-but-untested.
    #[test]
    fn a_pfs3_file_whose_size_disagrees_with_the_manifest_is_a_fail() {
        let dir = scratch("pfs3-wrong-size");
        let content = b"cmd";
        let image = formatted_pfs3_image(&dir);
        let tree = tree_with_load_module(&dir, content, 0x20);
        NativeFormatter
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();
        let mut manifest = manifest_for_load_module(content);
        manifest.files[0].bytes = 999; // content is really 3 bytes

        let report = verify_volume(&image, None, 1, &manifest).unwrap();

        assert_eq!(report.failed, 1);
        assert_eq!(report.not_checked, 0);
        let verdict = &report.files[0];
        assert_eq!(verdict.state, CheckState::Fail);
        assert!(
            verdict.detail.as_deref().unwrap_or("").contains("size"),
            "{:?}",
            verdict.detail
        );
    }

    /// The other field the module doc claims and, before this round, never
    /// tested for PFS3: a protection disagreement is a `Fail`, the same as
    /// FFS's own pure-bit test — this is that test's PFS3 twin.
    #[test]
    fn a_pfs3_file_whose_protection_disagrees_with_the_manifest_is_a_fail() {
        let dir = scratch("pfs3-wrong-protection");
        let content = b"cmd";
        let image = formatted_pfs3_image(&dir);
        // The volume genuinely carries --p-rwed (0x20) ...
        let tree = tree_with_load_module(&dir, content, 0x20);
        NativeFormatter
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();
        // ... but the manifest expects the default, unprotected bits.
        let mut manifest = manifest_for_load_module(content);
        manifest.files[0].protection = Some(0x00);

        let report = verify_volume(&image, None, 1, &manifest).unwrap();

        assert_eq!(report.failed, 1);
        assert_eq!(report.not_checked, 0);
        let verdict = &report.files[0];
        assert_eq!(verdict.state, CheckState::Fail);
        assert!(
            verdict
                .detail
                .as_deref()
                .unwrap_or("")
                .contains("protection"),
            "{:?}",
            verdict.detail
        );
    }

    /// Fix round 1, item 2: an expected protection that does not fit the
    /// single byte PFS3 actually stores must be surfaced as its own,
    /// distinct reason — never folded into "protection matched", which
    /// would be a straightforward lie about a field that was never
    /// compared to anything at all.
    #[test]
    fn a_pfs3_expected_protection_that_does_not_fit_a_byte_is_surfaced_not_matched() {
        let dir = scratch("pfs3-unfittable-protection");
        let content = b"cmd";
        let image = formatted_pfs3_image(&dir);
        let tree = tree_with_load_module(&dir, content, 0x20);
        NativeFormatter
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();
        let mut manifest = manifest_for_load_module(content);
        manifest.files[0].protection = Some(0x1_0000); // does not fit a u8

        let report = verify_volume(&image, None, 1, &manifest).unwrap();

        assert_eq!(report.failed, 0, "{:?}", report.files);
        assert_eq!(report.not_checked, 1);
        let detail = report.files[0].detail.as_deref().unwrap_or("");
        assert!(detail.contains("not checked"), "{detail}");
        assert!(detail.contains("does not fit"), "{detail}");
        assert!(
            !detail.contains("protection matched"),
            "must not claim a match for a field that was never compared: {detail}"
        );
    }

    /// The PFS3 twin of `a_record_with_no_recorded_protection_can_still_
    /// pass_on_ffs`, but checking the *wording* rather than the state — on
    /// PFS3 the overall verdict is `NotChecked` either way (content is never
    /// re-hashed), so the only thing that can regress silently here is the
    /// detail text quietly claiming a match that was never attempted.
    #[test]
    fn a_pfs3_file_with_no_recorded_protection_says_so_rather_than_claiming_a_match() {
        let dir = scratch("pfs3-no-protection-recorded");
        let content = b"cmd";
        let image = formatted_pfs3_image(&dir);
        let tree = tree_with_load_module(&dir, content, 0x20);
        NativeFormatter
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();
        let mut manifest = manifest_for_load_module(content);
        manifest.files[0].protection = None;

        let report = verify_volume(&image, None, 1, &manifest).unwrap();

        assert_eq!(report.not_checked, 1);
        let detail = report.files[0].detail.as_deref().unwrap_or("");
        assert!(detail.contains("no expected protection"), "{detail}");
        assert!(
            !detail.contains("protection matched"),
            "nothing was asserted, so nothing can have 'matched': {detail}"
        );
    }

    /// Fix round 1, item 3's last leg: `DosFamily::Other` really does reach
    /// `NotChecked` for every record, not just in prose. `Sfs0` is neither
    /// `DOS` nor `PFS`/`PDS`, so it routes here without needing a formatted
    /// volume at all — `family_of` only looks at the RDB's own DosType.
    #[test]
    fn an_unrecognised_filesystem_is_not_checked_for_every_file() {
        let dir = scratch("unrecognised-fs");
        let image = card_with_partition(&dir, AmigaHardDiskFs::Sfs0, 8);
        let manifest = manifest_for_load_module(b"cmd");

        let report = verify_volume(&image, None, 1, &manifest).unwrap();

        assert_eq!(report.passed, 0);
        assert_eq!(report.failed, 0);
        assert_eq!(report.not_checked, 1);
        assert!(report.files[0].detail.is_some());
    }

    /// Fix round 1, item 1: a volume ART will not *write* to (dircache,
    /// here) must still produce a report — `NotChecked` for every file,
    /// with the real reason — rather than turning the whole run into a hard
    /// `Err` that hands the caller nothing. `DOS\5` (`FFS INTL + dircache`)
    /// has no `AmigaHardDiskFs` variant of its own in this codebase (its
    /// `FfsDirCache` name is actually `DOS\3`, plain FFS INTL), so this
    /// reaches for `Custom` with the real dircache flavour byte directly.
    #[test]
    fn a_dircache_volume_is_not_checked_rather_than_a_failed_run() {
        let dir = scratch("dircache");
        const DOS5_FFS_DIRCACHE: u32 = 0x444F_5305; // "DOS\5"
        let image = card_with_partition(&dir, AmigaHardDiskFs::Custom(DOS5_FFS_DIRCACHE), 8);
        let manifest = manifest_for_load_module(b"cmd");

        let report = verify_volume(&image, None, 1, &manifest).unwrap();

        assert_eq!(report.passed, 0);
        assert_eq!(report.failed, 0);
        assert_eq!(report.not_checked, 1);
        let detail = report.files[0].detail.as_deref().unwrap_or("");
        assert!(detail.contains("dircache"), "{detail}");
    }

    /// A record with no recorded protection (`S/User-Startup`'s own shape,
    /// per `apply.rs`) makes no claim to disagree with, so it does not hold
    /// an otherwise-correct FFS file back from `Pass` — "nothing asserted" is
    /// not "something unchecked". See the module doc comment's Decision 1.
    #[test]
    fn a_record_with_no_recorded_protection_can_still_pass_on_ffs() {
        let dir = scratch("no-protection-recorded");
        let content = b"; composed\n";
        let image = formatted_ffs_image(&dir);
        let tree = dir.join("tree");
        std::fs::create_dir_all(tree.join("S")).unwrap();
        std::fs::write(tree.join("S/User-Startup"), content).unwrap();
        // Deliberately no .uaem sidecar: `copy_in` falls back to
        // `FileMeta::default()`, exactly as it does for a real composed
        // `S/User-Startup` (see `apply.rs`'s own module doc).
        NativeFormatter
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();

        let manifest = DistributionManifest {
            release: "AmigaOS 3.2".into(),
            built_from: Vec::new(),
            files: vec![FileRecord {
                path: "S/User-Startup".into(),
                component: "amissl".into(),
                media: String::new(),
                sha256: sha256_bytes(content),
                bytes: content.len() as u64,
                protection: None,
                overwrote: None,
            }],
            paired_rom: None,
        };

        let report = verify_volume(&image, None, 1, &manifest).unwrap();
        assert_eq!(report.passed, 1, "{:?}", report.files);
    }

    /// Decision 3: a file the volume carries that the manifest never
    /// mentioned is not this report's business, in either direction — not a
    /// `Fail` (the manifest never claimed the volume was *only* what it
    /// wrote) and not a phantom extra verdict either. Fix round 1, item 6:
    /// the previous version of this test asserted nothing about the extra
    /// file itself (`let _ = …unwrap()`); this one names it and checks its
    /// absence directly, so a future version of `verify_ffs_files` that
    /// *did* start walking the whole volume — silently turning this into a
    /// full-disk audit, exactly what Decision 3 argues against — would fail
    /// it rather than sail through unnoticed.
    #[test]
    fn an_extra_file_on_the_volume_that_is_not_in_the_manifest_is_simply_invisible_to_the_report() {
        let dir = scratch("extra-file");
        let image = formatted_ffs_image(&dir);
        let tree = tree_with_load_module(&dir, b"cmd", 0x20);
        std::fs::write(tree.join("Unlisted"), b"nobody told the manifest").unwrap();
        NativeFormatter
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();
        let manifest = manifest_for_load_module(b"cmd");

        let report = verify_volume(&image, None, 1, &manifest).unwrap();

        assert_eq!(
            report.files.len(),
            manifest.files.len(),
            "one verdict per manifest record, never one for a file the manifest never named"
        );
        assert!(
            report.files.iter().all(|f| f.path != "Unlisted"),
            "the extra file must not appear in the report at all: {:?}",
            report.files
        );
        assert_eq!(report.passed, 1);
        assert_eq!(report.failed, 0);
    }
}
