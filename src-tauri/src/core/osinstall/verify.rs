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
//! through `VolumeWriter::find`/`read_file`, which is a genuinely different
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
use sha2::{Digest, Sha256};

use crate::core::card::read_card;
use crate::core::error::CoreResult;
use crate::core::osinstall::apply::{DistributionManifest, FileRecord};
use crate::core::preload::native::{
    area_for_slot, family_of, from_pfs3, partition_by_index, partition_region, pfs3_protection,
    DosFamily,
};
use crate::core::volume::device::FileRegionMut;
use crate::core::volume::write::{uaem, VolumeWriter};
use crate::core::volume::{BlockDevice, DosType, VolumeGeometry};

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

// ---------------------------------------------------------------------------
// FFS/OFS — ART's own reader, a genuinely different path from the writer
// ---------------------------------------------------------------------------

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
    let mut region = FileRegionMut::open(image, offset, length, block_size)?;
    let total_blocks = region.total_blocks();
    let geometry = VolumeGeometry::new(block_size, total_blocks, reserved, dos)?;
    let writer = VolumeWriter::open(&mut region, geometry, image, offset)?;

    Ok(manifest
        .files
        .iter()
        .map(|record| verify_ffs_one(&writer, record))
        .collect())
}

/// Walk `path` one `/`-separated segment at a time from the root (block `0`
/// in `VolumeWriter`'s own spelling), the same way `native.rs::copy_in_ffs`
/// builds `block_of` while writing — just reading instead of creating.
fn find_ffs_path(writer: &VolumeWriter, path: &str) -> CoreResult<Option<u32>> {
    let mut current = 0u32;
    for segment in path.split('/') {
        match writer.find(current, segment)? {
            Some(entry) => current = entry.block,
            None => return Ok(None),
        }
    }
    Ok(Some(current))
}

fn verify_ffs_one(writer: &VolumeWriter, record: &FileRecord) -> FileVerdict {
    let block = match find_ffs_path(writer, &record.path) {
        Ok(Some(block)) => block,
        Ok(None) => {
            return fail(&record.path, "not found on the volume");
        }
        Err(err) => {
            return fail(&record.path, format!("its path could not be read: {err}"));
        }
    };

    let bytes = match writer.read_file(block) {
        Ok(bytes) => bytes,
        Err(err) => {
            return fail(
                &record.path,
                format!("its content could not be read back: {err}"),
            );
        }
    };
    let attrs = match writer.attributes(block) {
        Ok(attrs) => attrs,
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
    let actual_sha256 = hex_sha256(&bytes);
    if actual_sha256 != record.sha256 {
        problems.push("its content does not match the manifest's sha256".to_string());
    }
    if let Some(expected) = record.protection {
        if attrs.protection != expected {
            problems.push(format!(
                "protection is {}, the manifest says {}",
                uaem::format_bits(attrs.protection),
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

/// Reason a `NotChecked` PFS3 verdict always carries — everything checkable
/// on this family checked out, but the content bytes are the one thing this
/// module will not claim to have confirmed. See Decision 2.
const PFS3_CONTENT_NOT_CHECKED: &str = "presence, size and protection matched, but PFS3 has no \
     reader in ART other than the library that wrote it — its content was not re-hashed here. \
     See Task 11's independent hst-imager oracle.";

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
    if let Some(expected) = record.protection {
        match pfs3_protection(expected) {
            Ok(expected_u8) if entry.protection != expected_u8 => {
                problems.push(format!(
                    "protection is {}, the manifest says {}",
                    libpfs3::util::amiga_protection_string(entry.protection),
                    libpfs3::util::amiga_protection_string(expected_u8)
                ));
            }
            // Fits and matches: nothing to add. Does not fit a PFS3 byte at
            // all: the manifest's own expectation is not something this
            // volume could ever have stored, which is not the volume's
            // fault to be blamed for — left unasserted rather than failed.
            Ok(_) | Err(_) => {}
        }
    }

    if !problems.is_empty() {
        return fail(&record.path, problems.join("; "));
    }

    FileVerdict {
        path: record.path.clone(),
        state: CheckState::NotChecked,
        detail: Some(PFS3_CONTENT_NOT_CHECKED.to_string()),
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

    /// A card with one partition of `fs`, sized `mb` megabytes.
    fn card_with_partition(dir: &Path, fs: AmigaHardDiskFs, mb: u32) -> PathBuf {
        let path = dir.join("card.hdf");
        crate::core::hdf::create_hdf(
            &path,
            32 * 1024 * 1024,
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
                sha256: hex_sha256(content),
                bytes: content.len() as u64,
                protection: Some(0x20), // --p-rwed: what apply() actually recorded
            }],
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
            ..Default::default()
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

    /// The pure bit is why this check exists at all.
    #[test]
    fn a_file_whose_protection_bits_are_wrong_is_a_fail_not_a_pass() {
        let (image, manifest) = written_volume_with_the_pure_bit_dropped();
        let report = verify_volume(&image, None, 1, &manifest).unwrap();
        assert!(report
            .files
            .iter()
            .any(|f| f.path == "C/LoadModule" && f.state == CheckState::Fail));
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
                sha256: hex_sha256(content),
                bytes: content.len() as u64,
                protection: None,
            }],
        };

        let report = verify_volume(&image, None, 1, &manifest).unwrap();
        assert_eq!(report.passed, 1, "{:?}", report.files);
    }

    /// Decision 3: a file the volume carries that the manifest never
    /// mentioned is not this report's business, in either direction — not a
    /// `Fail` (the manifest never claimed the volume was *only* what it
    /// wrote) and not a phantom extra verdict either.
    #[test]
    fn an_extra_file_on_the_volume_that_is_not_in_the_manifest_is_simply_invisible_to_the_report() {
        let (image, manifest) = written_volume();
        // The tree `written_volume` copied in also carries `C.uaem`'s
        // sibling directory `C` itself, which is not a manifest record —
        // already proves nothing crashes over a directory. Copy a second,
        // wholly unrecorded file onto the same tree before reformatting, to
        // prove the point with an ordinary file too.
        let dir = scratch("extra-file");
        let image2 = formatted_ffs_image(&dir);
        let tree = tree_with_load_module(&dir, b"cmd", 0x20);
        std::fs::write(tree.join("Unlisted"), b"nobody told the manifest").unwrap();
        NativeFormatter
            .copy_in(&image2, None, "DH0", &tree, &NoProgress)
            .unwrap();
        let manifest2 = manifest_for_load_module(b"cmd");

        let report = verify_volume(&image2, None, 1, &manifest2).unwrap();
        assert_eq!(
            report.files.len(),
            manifest2.files.len(),
            "one verdict per manifest record, never one for a file the manifest never named"
        );
        assert_eq!(report.passed, 1);
        assert_eq!(report.failed, 0);

        // The unmodified fixture from Step 1, unaffected by any of this.
        let _ = verify_volume(&image, None, 1, &manifest).unwrap();
    }
}
