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

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::amigaprefs::{iff, wbpattern};
use crate::core::card::read_card;
use crate::core::error::{CoreError, CoreResult};
use crate::core::hashing::sha256_bytes;
use crate::core::osinstall::apply::{DistributionManifest, FileRecord};
use crate::core::osinstall::{resolve_ci_optional, strip_sys_prefix_ci};
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
/// exactly one verdict. Since Task 8, `files` also carries one further
/// verdict per checkable backdrop-path claim [`check_prefs_paths`] found in
/// the distribution tree's own prefs files — a check on the tree ART built,
/// independent of anything a manifest happens to record, so
/// `files.len() >= manifest.files.len()` rather than `==`; the manifest-only
/// invariant lives on as `files.len() - manifest.files.len()` equalling
/// `check_prefs_paths`'s own result length, which
/// `an_extra_file_on_the_volume_that_is_not_in_the_manifest_is_simply_invisible_to_the_report`
/// (below) still pins directly.
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
/// check every file `manifest` says `apply()` put there, plus — Task 8 —
/// whether every backdrop path `dist_root`'s own prefs files name actually
/// resolves inside that same distribution tree. See [`check_prefs_paths`]'s
/// own doc comment for what that check is and why it exists (the dist-3.2
/// orphan: a released tree whose `WBPattern.prefs` named two files that were
/// never in it, and nothing noticed for a month because nothing looked).
///
/// Structural failures — the image will not open, the slot or index does not
/// exist, the partition's own geometry cannot be computed — are a hard `Err`:
/// nothing here was verified, so there is no `VerifyReport` to hand back, the
/// same way `NativeFormatter::copy_in` refuses before writing anything rather
/// than reporting a doomed attempt as a summary. Once the volume itself opens,
/// every problem after that is a **verdict**, not an error: a missing file
/// does not stop the run, it becomes that one file's `Fail`.
///
/// `dist_root` is a wholly separate resource from `image` — a host folder,
/// not a card — so a problem reading *it* must not cost the caller the
/// volume-based verdicts already computed: an unexpected error out of
/// [`check_prefs_paths`] becomes one more `Fail` verdict appended to `files`,
/// never a hard `Err` that discards everything already found. `NotFound` is
/// not "unexpected" here — [`check_prefs_paths`] already turns "no prefs
/// directory at all" into its own `NotChecked` verdict rather than an `Err`.
pub fn verify_volume(
    image: &Path,
    slot: Option<usize>,
    index: usize,
    manifest: &DistributionManifest,
    dist_root: &Path,
) -> CoreResult<VerifyReport> {
    let card = read_card(image)?;
    let area = area_for_slot(&card, slot)?;
    let part = partition_by_index(area, index)?;
    let (offset, length, block_size) = partition_region(area, part)?;
    let dos = DosType::new(part.dostype.to_be_bytes());

    let mut files = match family_of(dos) {
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

    match check_prefs_paths(dist_root) {
        Ok(prefs_verdicts) => files.extend(prefs_verdicts),
        Err(err) => files.push(fail(
            PREFS_SYS_DIR_REL,
            format!("the distribution tree's own prefs paths could not be checked: {err}"),
        )),
    }

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
// Task 8 — every prefs path resolves
// ---------------------------------------------------------------------------
//
// A distribution tree can be a perfectly good FFS/PFS3 volume by every check
// above and still boot to a Workbench with no explanation, because nothing
// upstream ever asked whether a `PTRN` chunk's own claim is true. That is
// not hypothetical: the AmigaOS 3.2 tree ART itself built in August named
// `Sys:Prefs/Presets/Backdrops/default_pal.iff` and `.../pattern.iff` in its
// own `WBPattern.prefs`, and `Prefs/Presets/` held `Backdrops.info` and no
// `Backdrops` drawer at all. Nothing noticed for a month, because nothing
// looked. This section is that look.

/// Where AmigaOS's own `Env-Archive` prefs live inside a distribution tree,
/// resolved case-insensitively — see [`crate::core::osinstall::resolve_ci_optional`],
/// the same helper `core::appearance` resolves `WBPATTERN_REL`/`SCREENMODE_REL`
/// against.
const PREFS_SYS_DIR_REL: &str = "Prefs/Env-Archive/Sys";

/// Every backdrop path a distribution tree's own prefs files claim, checked
/// against the tree itself: for every `*.prefs` file under
/// [`PREFS_SYS_DIR_REL`], every `PTRN` chunk whose content is a picture
/// (never a pattern — see below), does the Amiga path it names actually
/// resolve under `tree`?
///
/// One [`FileVerdict`] per **resolvable claim**, not one per prefs file and
/// not one per chunk. A `PTRN` in [`wbpattern::Content::Pattern`] form names
/// no path at all — the release's own screen backdrop ships that way, a
/// `Depth=0`, 256-byte blank buffer — and is silently skipped: reporting a
/// verdict for it would mean inventing a claim the chunk never made.
///
/// A tree with no `*.prefs` file under [`PREFS_SYS_DIR_REL`] at all produced
/// nothing to check — not zero problems, *nothing looked at* — which is
/// [`CheckState::NotChecked`], never rendered as a pass (module doc's G8
/// section). Everything else this function finds is a **checked** claim, so
/// it lands on `Pass` or `Fail`, matching this module's own convention that
/// an error actually encountered while attempting a check (a `PTRN` chunk
/// that will not decode) is `Fail`, the same as `verify_ffs_one`'s "its path
/// could not be read" — `NotChecked` is reserved for a check ART never
/// attempted at all, by design (no prefs found, a path naming an assign
/// other than `Sys:`, which a distribution tree simply has no way to
/// resolve, or a `.prefs` file that is not IFF at all — see
/// [`iff::looks_like_iff_pref`]).
///
/// **`.prefs` is a filename convention on the Amiga, not a format
/// guarantee.** A real AmigaOS 3.9 tree's `Env-Archive/Sys` carries ViNCEd,
/// XTerm, AmiDock, StringSnip and DefIcons preferences beside the genuine IFF
/// ones — measured directly (Task 11's own prefs oracle over the owner's own
/// 3.9 material: 9 of 24 `.prefs` files there are third-party formats). This
/// function must not fail a tree for carrying one of those; see
/// [`iff::looks_like_iff_pref`] for how "not IFF at all" is told apart from "IFF,
/// and genuinely broken".
pub fn check_prefs_paths(tree: &Path) -> CoreResult<Vec<FileVerdict>> {
    let sys_dir = match resolve_ci_optional(tree, PREFS_SYS_DIR_REL)? {
        Some(dir) => dir,
        None => return Ok(vec![no_prefs_checked()]),
    };

    let mut prefs_paths: Vec<PathBuf> = match std::fs::read_dir(&sys_dir) {
        Ok(entries) => {
            let mut out = Vec::new();
            for entry in entries {
                let entry = entry?;
                let is_prefs_file = entry.file_type()?.is_file()
                    && entry
                        .path()
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| ext.eq_ignore_ascii_case("prefs"))
                        .unwrap_or(false);
                if is_prefs_file {
                    out.push(entry.path());
                }
            }
            out
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(err) => return Err(CoreError::Io(err)),
    };
    // Deterministic order, so the report is stable and reproducible run to
    // run — the same reason `backdrops_in_tree` sorts its own listing.
    prefs_paths.sort();

    if prefs_paths.is_empty() {
        return Ok(vec![no_prefs_checked()]);
    }

    let mut verdicts = Vec::new();
    for prefs_path in prefs_paths {
        let file_name = prefs_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| prefs_path.display().to_string());
        let prefs_rel = format!("{PREFS_SYS_DIR_REL}/{file_name}");

        let bytes = match std::fs::read(&prefs_path) {
            Ok(bytes) => bytes,
            Err(err) => {
                verdicts.push(fail(
                    &prefs_rel,
                    format!("'{prefs_rel}' could not be read: {err}"),
                ));
                continue;
            }
        };

        // `.prefs` is a filename convention on the Amiga, not a format
        // guarantee — measured on the owner's own AmigaOS 3.9 material: 9 of
        // 24 `.prefs` files there are ViNCEd, XTerm, AmiDock, StringSnip and
        // DefIcons formats, none of them IFF at all. Sniffing the two magic
        // markers *before* calling `iff::parse` is what keeps "this is not
        // ART's format" apart from "this is ART's format and it is broken" —
        // deciding by evidence rather than by which error `iff::parse`
        // happened to return.
        if !iff::looks_like_iff_pref(&bytes) {
            verdicts.push(FileVerdict {
                path: prefs_rel.clone(),
                state: CheckState::NotChecked,
                detail: Some(format!(
                    "'{prefs_rel}' is not an IFF FORM/PREF preferences file — likely a \
                     different program's own format that happens to share the .prefs \
                     extension; ART did not examine it"
                )),
            });
            continue;
        }

        let prefs = match iff::parse(&bytes) {
            Ok(prefs) => prefs,
            Err(err) => {
                // The sniff above already confirmed this file opens
                // FORM/PREF, so a parse failure here is a concrete, checked
                // problem — ART recognised the format and it is malformed
                // (truncated, a bad chunk size, ...) — not an incapability
                // decided in advance. `Fail`, matching `verify_ffs_one`, not
                // a `NotChecked` shrug.
                verdicts.push(fail(
                    &prefs_rel,
                    format!("'{prefs_rel}' does not parse as an IFF prefs file: {err}"),
                ));
                continue;
            }
        };

        for (chunk_index, chunk) in prefs.chunks().iter().enumerate() {
            if chunk.id != *b"PTRN" {
                continue;
            }
            let body = match prefs.body(chunk_index) {
                Ok(body) => body,
                Err(_) => {
                    // `chunk_index` was taken from this same `prefs.chunks()`
                    // a line above, so `body()` cannot refuse it — its
                    // fallible path exists only for a caller-computed index
                    // (see `iff::PrefsFile::body`'s own doc comment).
                    // Unreachable by ART's own arithmetic, not by a third
                    // party's behaviour, so a `debug_assert!` records the
                    // invariant rather than a runtime `Err` branch nothing
                    // can reach.
                    debug_assert!(
                        false,
                        "an index taken from this file's own chunks() cannot be out of range"
                    );
                    continue;
                }
            };
            let backdrop = match wbpattern::read_backdrop(body) {
                Ok(backdrop) => backdrop,
                Err(err) => {
                    verdicts.push(fail(
                        &prefs_rel,
                        format!("'{prefs_rel}' carries a PTRN chunk that does not parse: {err}"),
                    ));
                    continue;
                }
            };
            let amiga_path = match backdrop.content {
                // Names no path at all — see this function's own doc comment.
                wbpattern::Content::Pattern { .. } => continue,
                wbpattern::Content::Picture(path) => path,
            };
            verdicts.push(check_one_backdrop_path(tree, &prefs_rel, &amiga_path)?);
        }
    }
    Ok(verdicts)
}

// `looks_like_iff_pref` used to live here as its own copy of the same two
// magic-marker checks. Fix round 1 (Task 11's real-material oracle review)
// found that a private copy is how the oracle and this module quietly drift:
// the oracle's own inline check agreed with this one on every one of the 24
// real files measured, but not on a `FORM` of some *other* IFF type. The
// sniff now lives once, in `core::amigaprefs::iff::looks_like_iff_pref`
// (used below via the `iff` import already in scope), so both callers are
// the same function rather than two hand-kept-in-sync copies of it.

/// The one verdict for a tree with nothing under [`PREFS_SYS_DIR_REL`] to
/// check at all — no such directory, or a directory with no `*.prefs` file
/// in it. `NotChecked`, and the detail says exactly why, per G8: "ART did
/// not look" must never be rendered as a tick.
fn no_prefs_checked() -> FileVerdict {
    FileVerdict {
        path: PREFS_SYS_DIR_REL.to_string(),
        state: CheckState::NotChecked,
        detail: Some(format!(
            "the tree carries no '{PREFS_SYS_DIR_REL}' directory (or no *.prefs file inside \
             it), so no backdrop path could be checked"
        )),
    }
}

/// One `Content::Picture` claim: does `amiga_path` resolve inside `tree`?
///
/// The verdict names both ends, per the Task 8 brief's own rule: `path` is
/// the Amiga path the chunk actually named (so the missing file is never
/// left out), and `detail` opens with the prefs file that named it (so the
/// verdict is never just "a backdrop is missing" with no way to find which
/// prefs file said so) before saying why.
fn check_one_backdrop_path(
    tree: &Path,
    prefs_rel: &str,
    amiga_path: &str,
) -> CoreResult<FileVerdict> {
    // The assign itself is folded case-insensitively, matching every other
    // AmigaDOS name comparison in this function — real material carries
    // `SYS:` as well as `Sys:` (whole-branch review finding I3).
    let Some(rel) = strip_sys_prefix_ci(amiga_path) else {
        // A path naming a different assign (`Work:`, say) is not something a
        // distribution tree can resolve — a tree has no notion of what
        // `Work:` points at on the machine that eventually mounts it.
        // Reporting it as missing would be exactly the confident-wrong
        // sentence this module exists to avoid (CLAUDE.md, "the failure that
        // does not crash"): ART cannot know the claim is false, so it must
        // not say so. `NotChecked`, not `Fail`.
        return Ok(FileVerdict {
            path: amiga_path.to_string(),
            state: CheckState::NotChecked,
            detail: Some(format!(
                "named by '{prefs_rel}'; '{amiga_path}' names an assign other than 'Sys:', \
                 which a distribution tree has no way to resolve"
            )),
        });
    };

    match resolve_ci_optional(tree, rel)? {
        // ART-245: "was not found" is the honest sentence when nothing
        // resolves at all, and a different one when something does but is
        // the wrong kind — a directory sitting where the `PTRN` chunk named
        // a picture file. Endings stay distinct (CLAUDE.md, "the failure
        // that does not crash"): a user reading "not found" would go looking
        // for a file that in fact already exists, one level up.
        Some(resolved) if resolved.is_dir() => Ok(FileVerdict {
            path: amiga_path.to_string(),
            state: CheckState::Fail,
            detail: Some(format!(
                "named by '{prefs_rel}'; '{amiga_path}' resolves to a directory under the \
                 distribution tree ('{}'), not the picture file itself",
                tree.display()
            )),
        }),
        // The detail names what was actually examined rather than leaving it
        // to a document to say (whole-branch review finding I4): this walks
        // the **host distribution tree** `apply()` produced, not the
        // partition a card or volume write later copies it onto, so a
        // `Pass` here is not the same claim as a `Pass` this module derives
        // by actually reading a PFS3 volume elsewhere in this file. Naming
        // the tree makes that self-describing rather than something only
        // `CLAUDE.md` or a changelog entry states.
        Some(_) => Ok(FileVerdict {
            path: amiga_path.to_string(),
            state: CheckState::Pass,
            detail: Some(format!(
                "named by '{prefs_rel}'; found under the distribution tree ('{}')",
                tree.display()
            )),
        }),
        None => Ok(FileVerdict {
            path: amiga_path.to_string(),
            state: CheckState::Fail,
            detail: Some(format!(
                "named by '{prefs_rel}'; '{amiga_path}' was not found under this distribution \
                 tree ('{}')",
                tree.display()
            )),
        }),
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
    use crate::core::ScratchDir;
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
                host_path: None,
            }],
            paired_rom: None,
            amiga_installed: Vec::new(),
            layers: Vec::new(),
        }
    }

    /// An FFS volume carrying exactly what its manifest says: content,
    /// size and protection all genuinely agree. Every field this module
    /// can check on FFS is checkable here, which is what lets
    /// `every_file_in_the_manifest_is_found_with_its_size_and_its_bits`
    /// legitimately expect every file to `Pass` — not a PFS3 fixture, whose
    /// content is never confirmed at all (Decision 2), which would make
    /// that same expectation false by this module's own design.
    ///
    /// Returns the source `tree` too (not just the volume built from it) —
    /// every caller now also needs somewhere to pass as `verify_volume`'s
    /// `dist_root`, and this is the same tree that was actually copied in,
    /// rather than an unrelated stand-in.
    fn written_volume() -> (PathBuf, DistributionManifest, PathBuf) {
        let dir = scratch("written-volume");
        let content = b"cmd";
        let image = formatted_ffs_image(&dir);
        let tree = tree_with_load_module(&dir, content, 0x20);
        NativeFormatter
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();
        (image, manifest_for_load_module(content), tree)
    }

    /// The same manifest as `written_volume` — it expects `--p-rwed` — but
    /// the sidecar actually copied onto the volume drops the pure bit, so the
    /// volume and the manifest genuinely disagree about `C/LoadModule`'s
    /// protection. `written_volume`'s own content and size stay correct, so
    /// this isolates the one field under test.
    fn written_volume_with_the_pure_bit_dropped() -> (PathBuf, DistributionManifest, PathBuf) {
        let dir = scratch("pure-bit-dropped");
        let content = b"cmd";
        let image = formatted_ffs_image(&dir);
        let tree = tree_with_load_module(&dir, content, 0x00); // pure bit gone
        NativeFormatter
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();
        (image, manifest_for_load_module(content), tree)
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
        let (image, manifest, tree) = written_volume();

        let mut perms = std::fs::metadata(&image).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&image, perms).unwrap();

        let result = verify_volume(&image, None, 1, &manifest, &tree);

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
        let (image, manifest, tree) = written_volume();
        let report = verify_volume(&image, None, 1, &manifest, &tree).unwrap();
        assert_eq!(report.failed, 0, "{:?}", report.files);
        assert_eq!(report.passed, manifest.files.len());
    }

    #[test]
    fn a_missing_file_is_a_fail_and_says_which_one() {
        let (image, mut manifest, tree) = written_volume();
        manifest.files.push(FileRecord {
            path: "C/NeverWritten".into(),
            component: "workbench-base".into(),
            media: "Workbench3.2".into(),
            sha256: "0".repeat(64),
            bytes: 4,
            protection: None,
            overwrote: None,
            host_path: None,
        });
        let report = verify_volume(&image, None, 1, &manifest, &tree).unwrap();
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
        let (image, mut manifest, tree) = written_volume();
        manifest.files[0].sha256 = "0".repeat(64); // not b"cmd"'s real hash
        let report = verify_volume(&image, None, 1, &manifest, &tree).unwrap();
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
        let (image, manifest, tree) = written_volume_with_the_pure_bit_dropped();
        let report = verify_volume(&image, None, 1, &manifest, &tree).unwrap();
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
        let (image, manifest, tree) = written_volume();
        let report = verify_volume(&image, None, 1, &manifest, &tree).unwrap();
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

        let report = verify_volume(&image, None, 1, &manifest, &tree).unwrap();

        assert_eq!(
            report.passed, 0,
            "PFS3 content is never independently confirmed here"
        );
        assert_eq!(report.failed, 0);
        assert_eq!(
            report.not_checked, 2,
            "the PFS3 content itself, plus Task 8's own NotChecked for a tree with no prefs \
             files at all: {:?}",
            report.files
        );
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

        let report = verify_volume(&image, None, 1, &manifest, &dir).unwrap();

        assert_eq!(report.failed, 1);
        assert_eq!(
            report.not_checked, 1,
            "Task 8's own NotChecked for a tree with no prefs files at all: {:?}",
            report.files
        );
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

        let report = verify_volume(&image, None, 1, &manifest, &tree).unwrap();

        assert_eq!(report.failed, 1);
        assert_eq!(
            report.not_checked, 1,
            "Task 8's own NotChecked for a tree with no prefs files at all: {:?}",
            report.files
        );
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

        let report = verify_volume(&image, None, 1, &manifest, &tree).unwrap();

        assert_eq!(report.failed, 1);
        assert_eq!(
            report.not_checked, 1,
            "Task 8's own NotChecked for a tree with no prefs files at all: {:?}",
            report.files
        );
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

        let report = verify_volume(&image, None, 1, &manifest, &tree).unwrap();

        assert_eq!(report.failed, 0, "{:?}", report.files);
        assert_eq!(
            report.not_checked, 2,
            "the unfittable protection itself, plus Task 8's own NotChecked for a tree with no \
             prefs files at all: {:?}",
            report.files
        );
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

        let report = verify_volume(&image, None, 1, &manifest, &tree).unwrap();

        assert_eq!(
            report.not_checked, 2,
            "no recorded protection, plus Task 8's own NotChecked for a tree with no prefs \
             files at all: {:?}",
            report.files
        );
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

        let report = verify_volume(&image, None, 1, &manifest, &dir).unwrap();

        assert_eq!(report.passed, 0);
        assert_eq!(report.failed, 0);
        assert_eq!(
            report.not_checked, 2,
            "the unrecognised filesystem itself, plus Task 8's own NotChecked for a tree with \
             no prefs files at all: {:?}",
            report.files
        );
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

        let report = verify_volume(&image, None, 1, &manifest, &dir).unwrap();

        assert_eq!(report.passed, 0);
        assert_eq!(report.failed, 0);
        assert_eq!(
            report.not_checked, 2,
            "the dircache refusal itself, plus Task 8's own NotChecked for a tree with no \
             prefs files at all: {:?}",
            report.files
        );
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
                host_path: None,
            }],
            paired_rom: None,
            amiga_installed: Vec::new(),
            layers: Vec::new(),
        };

        let report = verify_volume(&image, None, 1, &manifest, &tree).unwrap();
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

        let report = verify_volume(&image, None, 1, &manifest, &tree).unwrap();

        assert_eq!(
            report.files.len(),
            // One verdict per manifest record, never one for a file the
            // manifest never named — plus, since Task 8, one further
            // verdict for `tree`'s own prefs check, which finds no prefs
            // file at all here and so contributes exactly one `NotChecked`.
            // That is `check_prefs_paths`'s own business, wholly unrelated
            // to "Unlisted" — this assertion pins the count rather than
            // silently drifting whenever `check_prefs_paths` finds
            // something to report.
            manifest.files.len() + 1,
            "{:?}",
            report.files
        );
        assert!(
            report.files.iter().all(|f| f.path != "Unlisted"),
            "the extra file must not appear in the report at all: {:?}",
            report.files
        );
        assert_eq!(report.passed, 1);
        assert_eq!(report.failed, 0);
    }

    // ---- Task 8: every prefs path resolves ----
    //
    // These call `check_prefs_paths` directly rather than building a full
    // card and volume — the claim under test is about the *tree*
    // `apply()`'s own recipe produces, not about anything a volume write or
    // read adds on top, and `verify_volumes_report_folds_in_the_trees_own_
    // prefs_check` below proves the fold-in into `VerifyReport` separately.
    //
    // Fixtures use `core::ScratchDir`, not `fixtures::scratch` — self-
    // removing on `Drop`, per this round's own convention (`core/appearance`
    // already uses it and CLAUDE.md names it directly), rather than a
    // trailing `remove_dir_all` that a panicking test would skip.

    fn write_wbpattern_picture(tree: &Path, amiga_path: &str) {
        let sys_dir = tree.join("Prefs").join("Env-Archive").join("Sys");
        std::fs::create_dir_all(&sys_dir).unwrap();
        let backdrop = wbpattern::Backdrop {
            reserved: [0u8; 16],
            which: wbpattern::Which::Root,
            placement: wbpattern::Placement::Scale,
            precision: wbpattern::Precision::Image,
            dither: wbpattern::Dither::Good,
            no_remap: false,
            other_flags: 0,
            revision: 0,
            content: wbpattern::Content::Picture(amiga_path.to_string()),
        };
        let body = wbpattern::write_backdrop(&backdrop).unwrap();
        let bytes =
            crate::core::amigaprefs::iff::tests_support::synthetic_prefs(&[(*b"PTRN", body)]);
        std::fs::write(sys_dir.join("WBPattern.prefs"), bytes).unwrap();
    }

    /// The release's own screen `PTRN`: `WBPF_PATTERN` set, `Depth=0`, a
    /// 256-byte blank buffer — measured in `wbpattern`'s own module doc.
    /// Names no path at all.
    fn write_wbpattern_pattern_only(tree: &Path) {
        let sys_dir = tree.join("Prefs").join("Env-Archive").join("Sys");
        std::fs::create_dir_all(&sys_dir).unwrap();
        let backdrop = wbpattern::Backdrop {
            reserved: [0u8; 16],
            which: wbpattern::Which::Screen,
            placement: wbpattern::Placement::Tile,
            precision: wbpattern::Precision::Default,
            dither: wbpattern::Dither::Default,
            no_remap: false,
            other_flags: 0,
            revision: 0,
            content: wbpattern::Content::Pattern {
                depth: 0,
                planes: vec![0u8; 256],
            },
        };
        let body = wbpattern::write_backdrop(&backdrop).unwrap();
        let bytes =
            crate::core::amigaprefs::iff::tests_support::synthetic_prefs(&[(*b"PTRN", body)]);
        std::fs::write(sys_dir.join("WBPattern.prefs"), bytes).unwrap();
    }

    /// A genuinely IFF `FORM`/`PREF` file, truncated mid-chunk: the `FORM`
    /// header and `PREF` type at the front are untouched (so
    /// `iff::looks_like_iff_pref` still says yes), but the bytes the `PTRN`
    /// chunk's own size field promises do not all follow — the same shape
    /// as a real truncated file, not an invented error. `iff::parse` must
    /// still refuse this one, and `check_prefs_paths` must still call that
    /// refusal `Fail`.
    fn write_truncated_wbpattern(tree: &Path) {
        let sys_dir = tree.join("Prefs").join("Env-Archive").join("Sys");
        std::fs::create_dir_all(&sys_dir).unwrap();
        let full = crate::core::amigaprefs::iff::tests_support::synthetic_prefs(&[(
            *b"PTRN",
            vec![1u8; 24],
        )]);
        let truncated = &full[..full.len() - 5];
        std::fs::write(sys_dir.join("WBPattern.prefs"), truncated).unwrap();
    }

    /// The dist-3.2 orphan, exactly: a `WBPattern.prefs` naming a backdrop
    /// that was never placed in the tree. The verdict must name both the
    /// prefs file and the missing Amiga path, so a user can act on it
    /// without opening a hex editor (Task 8 brief, rule 2).
    #[test]
    fn a_backdrop_the_tree_does_not_have_is_reported_by_both_names() {
        let scratch = ScratchDir::new("art-verify-prefs", "missing-backdrop");
        let tree = scratch.path();
        write_wbpattern_picture(tree, "Sys:Prefs/Presets/Backdrops/default_pal.iff");
        // No Backdrops drawer at all — `Prefs/Presets/` held only
        // `Backdrops.info` in the real tree this check exists to catch.

        let verdicts = check_prefs_paths(tree).unwrap();

        assert_eq!(verdicts.len(), 1, "{verdicts:?}");
        let verdict = &verdicts[0];
        assert_eq!(verdict.state, CheckState::Fail, "{verdict:?}");
        assert!(
            verdict.path.contains("default_pal.iff"),
            "the verdict must name the missing Amiga path: {verdict:?}"
        );
        let detail = verdict.detail.as_deref().unwrap_or("");
        assert!(
            detail.contains("WBPattern.prefs"),
            "the verdict must name the prefs file that claimed it: {detail}"
        );
        // ART-245: this is the "nothing at that path at all" sentence, and
        // it must stay distinct from the "wrong kind at that path" one
        // (`a_backdrop_at_the_right_name_but_the_wrong_kind_says_so` below).
        assert!(
            detail.contains("was not found"),
            "nothing exists at this path -- the verdict must say 'not found', not something a \
             wrong-kind detail would also satisfy: {detail}"
        );
    }

    /// ART-245: the name a `PTRN` chunk claims can resolve to something that
    /// exists but is the wrong kind -- a directory sitting where the picture
    /// file was supposed to be. "Was not found" would be false (something IS
    /// there) and would send a reader looking for a file that, one level up,
    /// already exists as a directory; the honest sentence names the kind.
    #[test]
    fn a_backdrop_at_the_right_name_but_the_wrong_kind_says_so() {
        let scratch = ScratchDir::new("art-verify-prefs", "wrong-kind-backdrop");
        let tree = scratch.path();
        write_wbpattern_picture(tree, "Sys:Prefs/Presets/Backdrops/default_pal.iff");
        // A directory sitting exactly where the picture file was named.
        let wrong_kind = tree
            .join("Prefs")
            .join("Presets")
            .join("Backdrops")
            .join("default_pal.iff");
        std::fs::create_dir_all(&wrong_kind).unwrap();

        let verdicts = check_prefs_paths(tree).unwrap();

        assert_eq!(verdicts.len(), 1, "{verdicts:?}");
        let verdict = &verdicts[0];
        assert_eq!(verdict.state, CheckState::Fail, "{verdict:?}");
        let detail = verdict.detail.as_deref().unwrap_or("");
        assert!(
            detail.contains("directory"),
            "the verdict must name what is actually wrong -- a directory, not a missing file: \
             {detail}"
        );
        assert!(
            !detail.contains("was not found"),
            "something IS there; 'was not found' is the sentence for the other case and would \
             be false here: {detail}"
        );
    }

    /// AmigaDOS is case-insensitive; the host is not. A tree holding
    /// `Default_Pal.iff` must satisfy a `PTRN` naming `default_pal.iff`.
    ///
    /// **Whole-branch review finding I4.** `check_prefs_paths` walks the
    /// **host distribution tree**, not a volume — `verify_volume` calls it
    /// with `dist_root`, never with the image it just wrote. A `Pass`
    /// verdict's own detail must say so, rather than leaving the distinction
    /// to a document (`CHANGELOG.md`/`docs/FEATURES.md` both used to say "on
    /// the volume", which is a different and stronger claim than what this
    /// function actually checked).
    #[test]
    fn a_backdrop_the_tree_does_have_passes_even_when_the_case_differs() {
        let scratch = ScratchDir::new("art-verify-prefs", "case-insensitive");
        let tree = scratch.path();
        write_wbpattern_picture(tree, "Sys:Prefs/Presets/Backdrops/default_pal.iff");
        let drawer = tree.join("Prefs").join("Presets").join("Backdrops");
        std::fs::create_dir_all(&drawer).unwrap();
        // Different case than the prefs file names.
        std::fs::write(drawer.join("Default_Pal.iff"), b"FORM....ILBM").unwrap();

        let verdicts = check_prefs_paths(tree).unwrap();

        assert_eq!(verdicts.len(), 1, "{verdicts:?}");
        assert_eq!(verdicts[0].state, CheckState::Pass, "{:?}", verdicts[0]);
        let detail = verdicts[0].detail.as_deref().unwrap_or("");
        assert!(
            detail.contains("distribution tree"),
            "a Pass verdict must name what was actually examined, not leave it to a document: \
             {detail}"
        );
    }

    /// A `PTRN` in `Content::Pattern` form names no path at all — the
    /// release's own screen chunk ships exactly this way — and must not be
    /// reported, positively or negatively: checking it would mean inventing
    /// a claim the chunk never made.
    #[test]
    fn a_pattern_chunk_names_no_path_and_is_not_checked() {
        let scratch = ScratchDir::new("art-verify-prefs", "pattern-only");
        let tree = scratch.path();
        write_wbpattern_pattern_only(tree);

        let verdicts = check_prefs_paths(tree).unwrap();

        assert!(
            verdicts.is_empty(),
            "a Content::Pattern PTRN names no path and must produce no verdict: {verdicts:?}"
        );
    }

    /// G8's three states: a tree with no prefs at all was never looked at,
    /// which is `CheckState::NotChecked` — never rendered as a tick, and
    /// never silently absent either (a `NotChecked` verdict has to say why).
    #[test]
    fn a_tree_with_no_prefs_at_all_is_not_checked_and_does_not_fail() {
        let scratch = ScratchDir::new("art-verify-prefs", "no-prefs");
        let tree = scratch.path();
        // Nothing written at all — not even a Prefs directory.

        let verdicts = check_prefs_paths(tree).unwrap();

        assert_eq!(verdicts.len(), 1, "{verdicts:?}");
        assert_eq!(
            verdicts[0].state,
            CheckState::NotChecked,
            "{:?}",
            verdicts[0]
        );
        assert!(
            verdicts[0].detail.is_some(),
            "a NotChecked verdict has to say why"
        );
    }

    /// The other decision the brief left to this task: a path naming an
    /// assign other than `Sys:` (`Work:`, say) is not something a
    /// distribution tree can resolve at all — ART has no idea what `Work:`
    /// points at on the machine that eventually mounts this tree. Reporting
    /// it as missing would be exactly the confident-wrong sentence
    /// CLAUDE.md's "the failure that does not crash" warns against, so this
    /// is `NotChecked`, never `Fail`.
    #[test]
    fn a_backdrop_naming_a_different_assign_is_not_checked_not_failed() {
        let scratch = ScratchDir::new("art-verify-prefs", "foreign-assign");
        let tree = scratch.path();
        write_wbpattern_picture(tree, "Work:MyBackdrops/custom.iff");

        let verdicts = check_prefs_paths(tree).unwrap();

        assert_eq!(verdicts.len(), 1, "{verdicts:?}");
        assert_eq!(
            verdicts[0].state,
            CheckState::NotChecked,
            "{:?}",
            verdicts[0]
        );
        let detail = verdicts[0].detail.as_deref().unwrap_or("");
        assert!(detail.contains("Work:"), "{detail}");
    }

    /// **Whole-branch review finding I3.** AmigaDOS assigns are
    /// case-insensitive like every other AmigaDOS name, and real material
    /// disagrees with a literal `"Sys:"` match: the round's own pre-flight
    /// scan recorded real `WBPattern.prefs` chunks reading
    /// `SYS:Prefs/Presets/Patterns/…` in upper case. Before the fix this
    /// fell into the `Work:`-shaped branch above and was reported
    /// `NotChecked` with a **false** sentence — `SYS:` names exactly the
    /// same assign as `Sys:`, so it must resolve and land on `Pass`/`Fail`
    /// like any other `Sys:` claim, not be waved off as unresolvable.
    #[test]
    fn an_uppercase_sys_assign_is_folded_the_same_as_the_measured_case() {
        let scratch = ScratchDir::new("art-verify-prefs", "uppercase-sys");
        let tree = scratch.path();
        write_wbpattern_picture(tree, "SYS:Prefs/Presets/Backdrops/default_pal.iff");
        let drawer = tree.join("Prefs").join("Presets").join("Backdrops");
        std::fs::create_dir_all(&drawer).unwrap();
        std::fs::write(drawer.join("default_pal.iff"), b"FORM....ILBM").unwrap();

        let verdicts = check_prefs_paths(tree).unwrap();

        assert_eq!(verdicts.len(), 1, "{verdicts:?}");
        assert_eq!(
            verdicts[0].state,
            CheckState::Pass,
            "an upper-case SYS: must resolve like Sys: does, not be waved off as a foreign \
             assign: {:?}",
            verdicts[0]
        );
    }

    /// **Real-material follow-up (Task 11's prefs oracle over the owner's
    /// own AmigaOS 3.9 material, not a fixture).** The original version of
    /// this test wrote plain text (`b"not an iff file at all"`) and asserted
    /// `Fail` — which was right under the old, pre-sniff code, but that same
    /// fixture does not open `FORM`/`PREF` at all, so under
    /// `iff::looks_like_iff_pref` it now correctly lands on the *other* branch
    /// (`NotChecked`, see `a_prefs_file_that_is_not_iff_at_all_is_not_checked_not_failed`
    /// below). This test was rewritten, not deleted or quietly patched to
    /// pass: it now provokes a *genuinely* IFF `FORM`/`PREF` file that is
    /// truncated mid-chunk, which is the fixture that actually distinguishes
    /// "ART recognised the format and it is broken" from "this was never
    /// ART's format" — the one thing `iff::looks_like_iff_pref` must not turn
    /// into a blanket suppression.
    #[test]
    fn a_prefs_file_that_is_form_pref_but_truncated_mid_chunk_is_still_a_fail() {
        let scratch = ScratchDir::new("art-verify-prefs", "truncated-form-pref");
        let tree = scratch.path();
        write_truncated_wbpattern(tree);

        let verdicts = check_prefs_paths(tree).unwrap();

        assert_eq!(verdicts.len(), 1, "{verdicts:?}");
        assert_eq!(verdicts[0].state, CheckState::Fail, "{:?}", verdicts[0]);
        let detail = verdicts[0].detail.as_deref().unwrap_or("");
        assert!(detail.contains("WBPattern.prefs"), "{detail}");
    }

    /// The defect itself, from Task 11's own oracle run against the owner's
    /// real AmigaOS 3.9 tree: `amidock.prefs` opens with the literal text
    /// `"AmiDock conf..."`, not `FORM` — one of 9 (of 24) real `.prefs`
    /// files there that are ViNCEd/XTerm/AmiDock/StringSnip/DefIcons
    /// formats, not IFF at all. Before `iff::looks_like_iff_pref`, this file
    /// reached `iff::parse`, failed, and came back `Fail` — a perfectly good
    /// tree reported as a failed verification, naming a file that is not
    /// ART's business and that nothing is wrong with. It must be
    /// `NotChecked`, and the detail must name the file.
    #[test]
    fn a_prefs_file_that_is_not_iff_at_all_is_not_checked_not_failed() {
        let scratch = ScratchDir::new("art-verify-prefs", "third-party-format");
        let tree = scratch.path();
        let sys_dir = tree.join("Prefs").join("Env-Archive").join("Sys");
        std::fs::create_dir_all(&sys_dir).unwrap();
        // The real shape, measured directly: amidock.prefs starts with this
        // text, not a FORM header.
        std::fs::write(sys_dir.join("amidock.prefs"), b"AmiDock config file V2.0\n").unwrap();

        let verdicts = check_prefs_paths(tree).unwrap();

        assert_eq!(verdicts.len(), 1, "{verdicts:?}");
        assert_eq!(
            verdicts[0].state,
            CheckState::NotChecked,
            "{:?}",
            verdicts[0]
        );
        let detail = verdicts[0].detail.as_deref().unwrap_or("");
        assert!(
            detail.contains("amidock.prefs"),
            "the verdict must name the file: {detail}"
        );
    }

    /// **Fix round 1's own defect, pinned as shipped behaviour, not merely
    /// inherited from `iff::looks_like_iff_pref`.** A `FORM` of some other
    /// IFF type — a misnamed `.prefs` that is really a picture — is the
    /// false-positive class this whole follow-up exists to close: before
    /// this fix, the prefs oracle's own round-trip hook checked only
    /// `bytes[0..4] != b"FORM"` and would have sent this fixture into
    /// `iff::parse`, recording it as a failure, while this function already
    /// correctly called it `NotChecked`. Same shape as
    /// `a_prefs_file_that_is_not_iff_at_all_is_not_checked_not_failed`
    /// above, but with a fixture that *does* open `FORM` — proving the
    /// distinction is `FORM` **and** `PREF` at offset 8, not `FORM` alone.
    #[test]
    fn a_form_that_is_not_pref_is_not_checked_not_failed() {
        let scratch = ScratchDir::new("art-verify-prefs", "form-not-pref");
        let tree = scratch.path();
        let sys_dir = tree.join("Prefs").join("Env-Archive").join("Sys");
        std::fs::create_dir_all(&sys_dir).unwrap();
        // "FORM" + size + "ILBM" - a misnamed .prefs that is really a picture.
        let mut bytes = b"FORM".to_vec();
        bytes.extend_from_slice(&4u32.to_be_bytes());
        bytes.extend_from_slice(b"ILBM");
        std::fs::write(sys_dir.join("notreally.prefs"), &bytes).unwrap();

        let verdicts = check_prefs_paths(tree).unwrap();

        assert_eq!(verdicts.len(), 1, "{verdicts:?}");
        assert_eq!(
            verdicts[0].state,
            CheckState::NotChecked,
            "{:?}",
            verdicts[0]
        );
        let detail = verdicts[0].detail.as_deref().unwrap_or("");
        assert!(
            detail.contains("notreally.prefs"),
            "the verdict must name the file: {detail}"
        );
    }

    /// A tree holding both kinds at once: the new branch must not swallow
    /// the real work. `amidock.prefs` (not IFF) gets its own `NotChecked`
    /// verdict and the genuine `WBPattern.prefs` beside it is still checked
    /// properly — its resolvable backdrop path still reaches `Pass`.
    #[test]
    fn a_tree_with_both_a_third_party_prefs_file_and_a_genuine_one_checks_each_correctly() {
        let scratch = ScratchDir::new("art-verify-prefs", "mixed-prefs");
        let tree = scratch.path();
        let sys_dir = tree.join("Prefs").join("Env-Archive").join("Sys");
        std::fs::create_dir_all(&sys_dir).unwrap();
        std::fs::write(sys_dir.join("amidock.prefs"), b"AmiDock config file V2.0\n").unwrap();
        write_wbpattern_picture(tree, "Sys:Prefs/Presets/Backdrops/default_pal.iff");
        let drawer = tree.join("Prefs").join("Presets").join("Backdrops");
        std::fs::create_dir_all(&drawer).unwrap();
        std::fs::write(drawer.join("default_pal.iff"), b"FORM....ILBM").unwrap();

        let verdicts = check_prefs_paths(tree).unwrap();

        assert_eq!(verdicts.len(), 2, "{verdicts:?}");
        let not_iff = verdicts
            .iter()
            .find(|v| v.path.contains("amidock.prefs"))
            .expect("the third-party prefs file must still get its own verdict");
        assert_eq!(not_iff.state, CheckState::NotChecked, "{not_iff:?}");

        let backdrop = verdicts
            .iter()
            .find(|v| v.path.contains("default_pal.iff"))
            .expect("the genuine WBPattern.prefs must still be checked, not swallowed");
        assert_eq!(backdrop.state, CheckState::Pass, "{backdrop:?}");
    }

    /// The integration the brief's own wording asks for: `verify_volume`'s
    /// report actually carries `check_prefs_paths`'s verdicts, not just the
    /// manifest's own file-by-file checks — the dist-3.2 orphan reproduced
    /// end to end, through the same call a real `osinstall_verify` makes.
    #[test]
    fn verify_volumes_report_folds_in_the_trees_own_prefs_check() {
        let (image, manifest, tree) = written_volume();
        write_wbpattern_picture(&tree, "Sys:Prefs/Presets/Backdrops/default_pal.iff");
        // Never actually placed in the tree — a real dist-3.2-shaped orphan.

        let report = verify_volume(&image, None, 1, &manifest, &tree).unwrap();

        assert!(
            report
                .files
                .iter()
                .any(|f| f.path.contains("default_pal.iff") && f.state == CheckState::Fail),
            "{:?}",
            report.files
        );
        assert_eq!(
            report.failed, 1,
            "the manifest's own file still passes; only the tree's own prefs claim fails: {:?}",
            report.files
        );
    }
}
