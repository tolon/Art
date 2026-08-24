//! Building the distribution tree and the manifest that says what built it.
//!
//! `plan()` (Task 5) is a description; this is the one place that turns it
//! into files a user keeps. Everything downstream — booting the tree,
//! removing a component cleanly — depends on two things being true about
//! what lands here: every byte came from a `.uaem`-preserving copy of the
//! user's own media, and `distribution.json` at the root says, for every
//! file, which component put it there, which medium it came out of, and its
//! SHA-256. That manifest is not bookkeeping; it is the only record of what
//! an install actually did, because the media it was read from is not kept
//! around afterwards — it cannot be reconstructed later by re-reading
//! anything.
//!
//! ## Two ways in, one placer
//!
//! [`apply`] builds a whole tree from a plan; [`add_package`] puts one
//! package onto a tree that already exists. They are the same work over the
//! same [`super::plan::PlanItem`] shape, so they share one implementation
//! ([`TreeWriter`]) rather than two that would drift — and the test that
//! actually holds them together is
//! `producing_with_a_package_equals_adding_it_afterwards`, which builds the
//! same tree both ways and compares real bytes. Reading either path can
//! tell you it looks right; only running both can tell you they agree.
//!
//! **A file a package overwrote records both facts.** The manifest is the
//! only surviving account of where every byte came from, and replacing a
//! record with the winner's would delete the loser's half of that account.
//! So the new record names the package *and* carries what it displaced —
//! see [`FileRecord::overwrote`].
//!
//! ## `atomic_write`, never `guarded_write` — a ruled decision, not an omission
//!
//! Every write here goes through `core/safety`, so nothing is ever
//! half-written. None of them takes a **generational backup**, and a reader
//! who knows the rule ("`guarded_write` for a user's file") should meet the
//! reasoning here rather than assume it was forgotten.
//!
//! `core::safety::backup_file` puts its generations in a `.art-backup/`
//! directory **beside the file it backs up**. Beside a file in a
//! distribution tree means *inside the tree* — inside the AmigaOS system
//! volume this tree becomes, copied onto the card with everything else and
//! shipped as part of the operating system. That is worse than the thing
//! backups protect against.
//!
//! And the tree is not the original. It is derived, in seconds, from media
//! the user still has: re-running [`apply`] is the real answer to "undo",
//! which is the same reasoning `BackupPolicy::LARGE_IMAGE` already applies
//! to a multi-gigabyte HDF. What the tree owes instead is an honest account
//! of itself, and that is what [`FileRecord::overwrote`] keeps: the
//! component, medium, hash and size of every file a package displaced.
//!
//! ## `SAFE_CREATE`, before anything else is touched
//!
//! A distribution folder already there is somebody's work — possibly a
//! previous install this one would silently interleave with. `apply` refuses
//! the moment `root` exists, before it opens a single medium.
//!
//! ## Every destination through `safe_join`
//!
//! `item.to` came from a recipe rule, and a recipe is data a human typed
//! (`Component::rules` in `mod.rs`). `core/layout/apply.rs`'s own module doc
//! says it plainly: "a `../` in a text box is the same hole a `../` in a zip
//! is." The same discipline applies here, and for the same reason — a review
//! of that module once found a genuine escape from the staging root before
//! the guard was restored.
//!
//! ## Two decisions worth stating, not just making
//!
//! **Cancellation.** `core/layout/apply.rs` answers "how much landed?" with
//! `CoreError::CancelledPartway { files }` rather than a bare `Cancelled`,
//! because the two read the same on screen even though one of them left real
//! work behind. This module follows that lead, with one adjustment: the
//! threshold is *files*, not *files-or-directories*. An empty drawer created
//! moments before cancellation is not work a user needs told about — the
//! `CancelledPartway` message is literally "cancelled after writing N
//! file(s)", and a lone empty directory does not deserve to be a `0` in that
//! sentence. So `apply` only reaches for `CancelledPartway` once at least one
//! real file is durably on disk; a cancellation that only got as far as
//! creating a directory reports the plain `Cancelled` `core/layout/apply.rs`
//! itself falls back to when nothing landed at all.
//!
//! **`FileRecord::bytes`.** This is the size that was actually written —
//! `bytes.len()` on what `MediaSource::read` handed back — never
//! `PlanItem::bytes`, the plan's own estimate. The two should always agree,
//! since `media_paths` is exactly the plan's own promise to reopen the same
//! media it already measured; the one way they would not is a floppy that
//! changed on disk between `plan()` and `apply()`, which is also exactly the
//! situation `core/layout/apply.rs`'s own precedent for this question warns
//! about (`ApplyOutcome.bytes` there is "the real size copied, not the
//! plan's estimate", proven by a test that deliberately makes them disagree).
//! `apply` does not treat a mismatch as an error — the file that was
//! actually read is the one on disk and the one hashed, so recording
//! anything else would make the manifest describe bytes nobody wrote.
//!
//! ## The manifest is written last
//!
//! Every other write in this module can fail or be cancelled without the
//! tree lying about itself, because nothing reads `distribution.json` to
//! decide what is really there — until it exists. Writing it last, after
//! every item has landed, is what keeps a half-built tree from claiming to
//! be a whole one.
//!
//! ## `S:User-Startup`, composed after every file, before the manifest
//!
//! [`super::startup::merge_user_startup`] is a pure function; this is where
//! it gets called. Three decisions, each forced by something already true
//! about this module:
//!
//! **Where.** After the main loop, so every media-sourced file — including
//! a real `S/User-Startup` a `Subtree "S"` rule may have copied off the
//! release media itself — is already on disk to merge into. Before the
//! manifest, so a run that stops here still leaves no `distribution.json`
//! behind. It gets the same cancellation checkpoint as every item in the
//! loop above: checked once, before the single atomic write that composes
//! it, never mid-write — so cancelling here is indistinguishable, on disk,
//! from cancelling between any two ordinary items.
//!
//! **What lands in the manifest.** A composed file has no one component
//! that "put it there" the way [`FileRecord::component`] means for every
//! other file — it can carry lines from several. Recording it once, under
//! whichever component happened to write last, would be a lie of exactly
//! the shape this manifest exists to prevent (a file claiming a single
//! origin it does not have) — that is the wrong answer the brief warned
//! was "obviously unconsidered". So `apply` records it **once per
//! contributing component**, all four sharing the same `path`, `sha256`
//! and `bytes` — the file has exactly one real content, and every
//! contributor's entry describes that same content — with `media: ""`,
//! since nothing here came off a disk image. A future "remove this
//! component cleanly" flow can then find every component that touched
//! `S/User-Startup` by filtering `files` on the path, the same way it
//! would find any other file. This is also why `ApplyOutcome::files` and
//! `manifest.files.len()` are no longer guaranteed equal — the former
//! counts real files on disk, the latter counts *records*, and a composed
//! file with more than one contributor now makes the second the larger of
//! the two on purpose.
//!
//! **A missing `S` drawer.** Reachable whenever a component carries
//! `user_startup` lines without `workbench-base` (or an equivalent) also
//! being on — a hand-built plan can do this even though the shipped
//! recipe's `workbench-base` is `required` and always creates `S`. Handled
//! exactly the way every ordinary item in the loop above already handles a
//! missing parent: `create_dir_all` on the target's parent before writing.
//! Refusing here would single out one file for a rule nothing else in this
//! function follows, over a directory that is entirely ART's own to create.
//!
//! ## Latin-1, not UTF-8
//!
//! `core/adf/bcpl.rs` already documents this rule for one BCPL field at a
//! time; `S/User-Startup` needs the same rule applied to a whole text file.
//! AmigaDOS text is Latin-1 — one byte per character, the identity mapping
//! on code points `0..=255` — and `String::from_utf8` on a media-provided
//! starter file or a hand-edited one turns the first accented character
//! into `CoreError::Malformed`, failing the install on its very last step,
//! after every other file has already landed. This project's user is
//! Turkish and the shipped recipe carries a `Locale-TR` component, so that
//! is an ordinary byte, not a hypothetical one. [`latin1_decode`] cannot
//! fail — every byte value has a Latin-1 character — so the read side of
//! this step never rejects a file for what it says. [`latin1_encode`]
//! mirrors `write_bcpl_string`'s own choice for a character with no Latin-1
//! byte at all: it becomes `?` rather than failing, since ART's own
//! generated block content and every shipped component's lines are plain
//! ASCII today, and a mis-rendered character composed by some future
//! component is a smaller problem than an install that cannot finish.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::plan::{InstallPlan, PlanItem};
use super::source::MediaSource;
use super::startup::merge_user_startup;
use crate::core::archive::compress;
use crate::core::error::{CoreError, CoreResult};
use crate::core::hashing::{sha256_bytes, sha256_file};
use crate::core::jobs::ProgressSink;
use crate::core::volume::write::copy::sidecar_for;
use crate::core::volume::write::uaem::{render, sidecar_path};

/// One medium the plan actually read from, and the SHA-256 of the whole
/// image file — not of any one entry inside it. Removing a component later
/// needs to know it is looking at the same disk this install actually used;
/// the media itself is not kept around, so this is the only place that fact
/// is ever recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaRecord {
    pub volume_name: String,
    pub sha256: String,
}

/// One file in the finished tree, and where it came from.
///
/// Ordinarily one record per `path` — but `S/User-Startup` is composed, not
/// copied (see the module doc comment's "S:User-Startup" section), so it
/// gets one record **per contributing component**, all sharing the one
/// `path`, `sha256` and `bytes` the composed file actually has. `component`
/// still names a single component for every other file; for that one path
/// it names "one of possibly several".
// `rename_all` is a no-op for every field that existed before `host_path`
// (all one word), and makes `host_path` read as `hostPath` — the casing the
// rest of this manifest already uses (`DistributionManifest`, `Overwritten`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRecord {
    /// `/`-separated, relative to the distribution root — matches
    /// [`super::plan::PlanItem::to`] exactly, except for `S/User-Startup`.
    pub path: String,
    pub component: String,
    /// The volume name, not the medium's filename — matches
    /// [`super::plan::PlanItem::media`]. Empty for `S/User-Startup`, which
    /// came from no single medium.
    pub media: String,
    pub sha256: String,
    /// What was actually written, not [`super::plan::PlanItem::bytes`]'s
    /// estimate — see the module doc comment.
    pub bytes: u64,
    /// `HSPARWED` exactly as read off the source media (`MediaEntry::protection`
    /// — same convention throughout `core::volume`, `RWED` inverted). `None`
    /// for `S/User-Startup`: a composed file has no single medium's byte to
    /// carry over, and nothing in `apply` ever sets one on it (see the module
    /// doc comment's "S:User-Startup" section) — recording `Some(0)` there
    /// would assert an intent nobody actually stated. `#[serde(default)]` so a
    /// manifest written before this field existed deserializes as `None`
    /// rather than refusing to load — Task 10's `verify` reads exactly that as
    /// "this manifest never recorded an expected protection for this file",
    /// which is the truth.
    #[serde(default)]
    pub protection: Option<u32>,
    /// What this file replaced, when something wrote over a file another
    /// component (or package) had already put here — `None` for a file
    /// nothing overwrote, which is almost every file in a freshly built
    /// tree.
    ///
    /// **Why the manifest has to carry it.** `distribution.json` is the only
    /// surviving account of where every byte came from, and a package
    /// overwriting a base file destroys the previous account by
    /// construction: keeping only the winner would make the tree claim the
    /// package put the file there and say nothing at all about the base
    /// component whose copy it displaced — which is exactly the "a file
    /// claiming a single origin it does not have" failure this manifest
    /// exists to prevent, in the other direction. A future "remove this
    /// package cleanly" flow needs both halves; so does anyone asking why a
    /// file's version does not match the release they installed.
    ///
    /// One step deep, not a chain: a third writer's record carries what it
    /// displaced, which is the second writer's own record *including its
    /// `overwrote`*, so the history is not lost — it nests.
    ///
    /// `#[serde(default, skip_serializing_if)]` — a manifest written before
    /// this field existed reads back as "nothing was overwritten", which is
    /// the truth for every tree ART built before packages existed, and a
    /// tree of three thousand files does not carry three thousand `null`s.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overwrote: Option<Overwritten>,
    /// Where this file actually landed on the **host**, when that is not
    /// [`path`](Self::path) — ART-160.
    ///
    /// [`path`](Self::path) is always the AmigaDOS name, because that is what
    /// the finished volume must carry and what
    /// [`verify_volume`](super::verify::verify_volume) looks up on it. A
    /// Windows filesystem will not carry every AmigaDOS name: `AUX` is one of
    /// the 22 reserved device names and it is really on the owner's AmigaOS
    /// 3.9 disc at `Storage/DOSDrivers/AUX`, and a legal AmigaDOS
    /// `Prices: 1993` is refused outright. [`super::host_destination`]
    /// escapes those, and this is the record of it — without which the tree
    /// and the manifest would disagree about which file is which, and
    /// `core/preload` (which reads the Amiga name off the host filename)
    /// would put `_AUX` on the card.
    ///
    /// `None` for every file whose host name is its Amiga name, which is
    /// almost all of them — `#[serde(default, skip_serializing_if)]` so a
    /// tree of three thousand files does not carry three thousand `null`s,
    /// and a manifest written before this field existed reads back as "the
    /// host name is the Amiga name", which is what was true of it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_path: Option<String>,
}

/// [`FileRecord::host_path`] for a destination — `None` when the host carries
/// the AmigaDOS name unchanged, which is the ordinary case.
fn host_path_of(to: &str) -> Option<String> {
    let host = super::host_relative(to);
    (host != to).then_some(host)
}

/// What a [`FileRecord`] displaced — the previous record's own account of
/// the same path, kept so the manifest never loses where the bytes that
/// used to be there came from. See [`FileRecord::overwrote`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Overwritten {
    pub component: String,
    pub media: String,
    pub sha256: String,
    pub bytes: u64,
    /// What *that* file had itself overwritten, if anything — see
    /// [`FileRecord::overwrote`]'s "one step deep, not a chain" note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overwrote: Option<Box<Overwritten>>,
}

impl Overwritten {
    /// The record `previous` describes, in the shape a successor carries it.
    fn of(previous: &FileRecord) -> Self {
        Self {
            component: previous.component.clone(),
            media: previous.media.clone(),
            sha256: previous.sha256.clone(),
            bytes: previous.bytes,
            overwrote: previous.overwrote.clone().map(Box::new),
        }
    }
}

/// What lives at the distribution root's own `distribution.json`: which
/// components built this tree, off which media, and — file by file — where
/// each one came from. Removing a component cleanly reads this back; it is
/// the only record, because the media itself is gone by then.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DistributionManifest {
    pub release: String,
    pub built_from: Vec<MediaRecord>,
    pub files: Vec<FileRecord>,
    /// The Kickstart this tree was built for (G9). `#[serde(default)]` so a
    /// tree written by an older ART still reads back.
    #[serde(default)]
    pub paired_rom: Option<super::PairedRom>,
    /// Packages whose **own installer ran on the Amiga** against this tree
    /// and reported success — see [`super::chain`].
    ///
    /// A separate list rather than more [`FileRecord`]s, because it is a
    /// different kind of knowledge and saying otherwise would be a lie ART
    /// cannot back up: an Amiga Installer is a program ART did not write and
    /// cannot supervise per file, so nothing here knows which files it wrote
    /// or what they displaced. What is honestly known is that it ran and
    /// said it worked, and that is exactly what this records.
    ///
    /// [`super::chain::missing_prerequisites`] reads it together with the
    /// components named in [`files`](Self::files): a BoingBag cannot be
    /// placed from the host at all (ART-166), so without this the chain it
    /// is the second half of could never be satisfied and BoingBag 2 would
    /// be refused for ever.
    ///
    /// `#[serde(default)]` so a tree written before this existed reads back
    /// as "nothing has been installed on the Amiga", which is what was true
    /// of it.
    #[serde(default)]
    pub amiga_installed: Vec<AmigaInstallRecord>,
}

/// One package's Amiga-side install, as [`DistributionManifest`] records it.
///
/// Deliberately only the two facts ART can vouch for: which package, and the
/// AmigaDOS command line its own recipe named. No file list — see
/// [`DistributionManifest::amiga_installed`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AmigaInstallRecord {
    /// The package id, the same string [`super::package::Package::id`] and
    /// [`FileRecord::component`] use, so one lookup answers both.
    pub package: String,
    /// What actually ran, program and arguments joined by spaces — the line
    /// the generated AmigaDOS script carried.
    pub command: String,
}

/// The manifest's own file name, at the distribution root.
pub const MANIFEST_FILE_NAME: &str = "distribution.json";

/// Where `S:User-Startup` lives in the tree, `/`-separated — the one file
/// `apply` composes itself rather than copying from a `PlanItem`. See the
/// module doc comment's "S:User-Startup" section.
const USER_STARTUP_PATH: &str = "S/User-Startup";

/// What one call to [`apply`] actually did.
///
/// `rename_all = "camelCase"` (Task 12 fix round 1): every field is a single
/// word today, so this changes nothing on the wire yet — added anyway so a
/// future multi-word field cannot repeat `VerifyReport::not_checked`'s
/// mistake by omission.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyOutcome {
    pub root: PathBuf,
    pub files: u64,
    pub directories: u64,
    pub bytes: u64,
}

/// Decode raw bytes as Latin-1 — see the module doc comment's "Latin-1, not
/// UTF-8" section. Latin-1 is exactly the first 256 Unicode code points, so
/// this is a plain cast that can never fail, unlike `String::from_utf8`.
/// The identical rule `core/adf/bcpl.rs::read_bcpl_string` already applies
/// to one BCPL field; this applies it to a whole file.
fn latin1_decode(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

/// The write side of the same rule. A character above `U+00FF` has no
/// Latin-1 byte; `core/adf/bcpl.rs::write_bcpl_string` maps that case to
/// `?` rather than failing, and this follows the same choice.
fn latin1_encode(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| if (c as u32) <= 0xFF { c as u8 } else { b'?' })
        .collect()
}

/// The one placer both ways into this module go through.
///
/// **Two entry points, one placer** (Task 6). [`apply`] builds a tree from a
/// whole plan; [`add_package`] adds one package to a tree that already
/// exists. They are the same work over the same [`PlanItem`] shape, and a
/// second copy of "place these items and record what happened" is precisely
/// how the two would start disagreeing about what a package puts on a
/// volume — which is what
/// `producing_with_a_package_equals_adding_it_afterwards` exists to catch,
/// and what this struct exists so there is nothing left for it to catch.
struct TreeWriter<'a> {
    root: &'a Path,
    /// The manifest's file records — empty for [`apply`], the existing
    /// tree's own for [`add_package`].
    files: Vec<FileRecord>,
    /// Destination -> the index in `files` of the record describing what is
    /// there **now**. Keyed by [`super::destination_key`], the same key
    /// `plan::detect_collisions` pairs claimants by, so the two cannot
    /// disagree about what "the same destination" means (ART-124) — and,
    /// since that key folds case the way AmigaDOS and the host filesystem
    /// both do, so that a package spelling a path `C/ASSIGN` over a manifest
    /// record of `C/Assign` replaces that record instead of adding a second
    /// one describing bytes nobody wrote (F11).
    record_index: std::collections::HashMap<String, usize>,
    /// Destination -> the size *this run* last wrote there. Separate from
    /// `record_index` because [`ApplyOutcome`] describes this run and the
    /// manifest describes the tree: a package overwriting a base file is one
    /// file written now and one file in the tree, and seeding the run's own
    /// counters from an existing manifest would report the whole tree as
    /// this run's work.
    run_sizes: std::collections::HashMap<String, u64>,
    made_dirs: std::collections::HashSet<String>,
    outcome: ApplyOutcome,
}

impl<'a> TreeWriter<'a> {
    fn new(root: &'a Path, files: Vec<FileRecord>) -> Self {
        // First record wins the slot: a path can legitimately carry several
        // (`S/User-Startup`, one per contributing component), and `place`
        // collapses those the moment something writes over the path.
        let mut record_index = std::collections::HashMap::new();
        for (at, file) in files.iter().enumerate() {
            record_index
                .entry(super::destination_key(&file.path))
                .or_insert(at);
        }
        Self {
            root,
            files,
            record_index,
            run_sizes: std::collections::HashMap::new(),
            made_dirs: std::collections::HashSet::new(),
            outcome: ApplyOutcome {
                root: root.to_path_buf(),
                ..Default::default()
            },
        }
    }

    /// Which cancellation error to raise — see the module doc comment on why
    /// the threshold is *files*, not files-or-directories, and why it counts
    /// this run's writes rather than the tree's contents.
    fn cancelled(&self) -> CoreError {
        if self.outcome.files > 0 {
            CoreError::CancelledPartway {
                files: self.outcome.files,
            }
        } else {
            CoreError::Cancelled
        }
    }

    /// Every directory `to` needs that nothing has named yet. Ancestors no
    /// rule names — `Prefs/Presets` on the way to `Prefs/Presets/Backdrops`
    /// — are created by `create_dir_all` and were counted nowhere, which
    /// left `directories` short of the tree even once the double-counting
    /// was fixed (ART-124). Each distinct prefix is stat'd once, not once
    /// per entry inside it.
    fn count_missing_prefixes(&mut self, to: &str) {
        for (at, _) in to.match_indices('/') {
            let prefix = &to[..at];
            if self.made_dirs.insert(super::destination_key(prefix))
                && !self.root.join(prefix).is_dir()
            {
                self.outcome.directories += 1;
            }
        }
    }

    /// Record that `to` now holds `record`'s bytes, and account for it.
    ///
    /// A destination written over — by a declared `overrides` inside one
    /// run, or by a package landing on a tree that already exists — replaces
    /// its predecessor's record rather than adding a second one: the bytes on
    /// disk are this writer's, so the record describing them has to be too
    /// (ART-124). What it displaced travels on the new record
    /// ([`FileRecord::overwrote`]) rather than being dropped.
    fn record(&mut self, to: &str, mut record: FileRecord) {
        let size = record.bytes;
        let key = super::destination_key(to);
        match self.run_sizes.insert(key.clone(), size) {
            // This run already wrote here; the tree gains no file and the
            // byte count swaps rather than adds.
            Some(previous) => {
                self.outcome.bytes = self.outcome.bytes - previous + size;
            }
            None => {
                self.outcome.files += 1;
                self.outcome.bytes += size;
            }
        }

        match self.record_index.get(&key).copied() {
            Some(at) => {
                record.overwrote = Some(Overwritten::of(&self.files[at]));
                self.files[at] = record;
                // A path carrying more than one record (`S/User-Startup`,
                // one per contributing component) has just been overwritten
                // by a single real file: the other records describe bytes
                // nobody wrote any more, so they go, and the indices they
                // shifted are rebuilt.
                if self
                    .files
                    .iter()
                    .filter(|f| super::same_destination(&f.path, to))
                    .count()
                    > 1
                {
                    let mut seen = false;
                    self.files.retain(|f| {
                        if !super::same_destination(&f.path, to) {
                            return true;
                        }
                        let keep = !seen;
                        seen = true;
                        keep
                    });
                    self.record_index.clear();
                    for (at, file) in self.files.iter().enumerate() {
                        self.record_index
                            .entry(super::destination_key(&file.path))
                            .or_insert(at);
                    }
                }
            }
            None => {
                self.record_index.insert(key, self.files.len());
                self.files.push(record);
            }
        }
    }

    /// Settle the `.uaem` sidecar beside a file just written, and answer
    /// the protection the manifest must record for it.
    ///
    /// **The manifest and the sidecar have to say the same thing** — they
    /// are two halves of one claim about the file, and `verify_volume` reads
    /// the manifest while the card gets the sidecar. Returning the value
    /// that was actually settled on, rather than letting the caller record
    /// `entry.protection` independently, is what makes disagreeing between
    /// them impossible rather than merely unlikely.
    ///
    /// **A medium that states nothing does not erase what a medium that did
    /// state something said.** An archive carries no AmigaDOS protection,
    /// date or comment at all — `source_archive.rs`'s own module doc calls
    /// its values *declared defaults, never a reading*, and §89 forbids
    /// treating them as evidence. So when a package overwrites a file the
    /// release media placed, the sidecar already beside it is still the only
    /// statement anyone has made about that path, and it stands. Dropping it
    /// would take `--p-rwed` off `C/Assign` on a real BoingBag, and
    /// AmigaOS 3.2's `Startup-Sequence` runs `Resident C:Assign PURE` — the
    /// exact bit `uaem.rs`'s own module doc calls load-bearing.
    ///
    /// Before this, neither half was right: the stale sidecar was left
    /// saying `--p-rwed` while the manifest recorded the archive's default
    /// `0`, so the tree contradicted itself and the two entry points were
    /// wrong in the same way — which is why the equivalence test could not
    /// see it.
    ///
    /// A sidecar ART cannot parse is removed rather than kept: it no longer
    /// describes the file that is there, and ART cannot say what it claims.
    fn settle_sidecar(
        &self,
        target: &Path,
        entry: &crate::core::osinstall::source::MediaEntry,
    ) -> CoreResult<u32> {
        let beside = sidecar_path(target);

        // The medium stated something. It wins, and overwrites whatever was
        // there — this is the ordinary case for a floppy or a disc.
        //
        // Never itself copied as a file: it is written beside `target`,
        // under a name `uaem::sidecar_path` builds by appending `.uaem`
        // rather than replacing the extension. A medium that genuinely
        // carried a file literally called `X.uaem` next to `X` would still
        // collide with `X`'s own sidecar — vanishingly unlikely on real
        // Amiga media, but the code does not rule it out, so this comment
        // shouldn't claim more than it does.
        if let Some(sidecar) = sidecar_for(entry.protection, entry.date, &entry.comment) {
            crate::core::safety::atomic::atomic_write(&beside, render(&sidecar).as_bytes())?;
            return Ok(entry.protection);
        }

        // The medium stated nothing. Whatever stands beside the file is the
        // last thing anybody did state about this path.
        match std::fs::read_to_string(&beside) {
            Ok(text) => match crate::core::volume::write::uaem::parse(&text) {
                Ok(previous) => Ok(previous.protection),
                Err(_) => {
                    // Through `core/safety`, like every other write in this
                    // module — a removal is a write, and the one `std::fs`
                    // call that skipped the gate because it produced no
                    // bytes was still the most complete way for something to
                    // disappear. `CONFIG`'s five generations: a sidecar is
                    // tiny, irreplaceable and easy to get wrong, which is the
                    // policy's own description of itself.
                    crate::core::safety::guarded_remove(
                        &beside,
                        crate::core::safety::BackupPolicy::CONFIG,
                    )?;
                    Ok(entry.protection)
                }
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(entry.protection),
            Err(err) => Err(CoreError::Io(err)),
        }
    }

    /// Place `items` under the root, in order, reading each one's bytes out
    /// of `sources`.
    ///
    /// `sink.is_cancelled()` is checked between whole items and never inside
    /// one, so stopping always leaves whole files behind.
    fn place(
        &mut self,
        items: &[PlanItem],
        sources: &mut BTreeMap<String, Box<dyn MediaSource>>,
        sink: &dyn ProgressSink,
    ) -> CoreResult<()> {
        let total = items.len() as u64;
        for (done, item) in items.iter().enumerate() {
            // Between whole items, never inside one — see the module doc
            // comment on which of the two cancellation errors this reaches
            // for.
            if sink.is_cancelled() {
                return Err(self.cancelled());
            }
            sink.report(done as u64, Some(total), &item.to);

            // The *host* path, which is `item.to` for every destination a
            // Windows filesystem can carry verbatim and an escaped form for
            // the handful it cannot (`AUX`, `Prices: 1993`) — ART-160. The
            // AmigaDOS name stays in `item.to`, which is what every record,
            // key and map below is built from.
            let target = super::host_destination(self.root, &item.to)?;

            // Both branches below create ancestors, so this sits above both.
            self.count_missing_prefixes(&item.to);

            if item.is_dir {
                // Asked before creating: a drawer that was already in the
                // tree is not a drawer this run made, which only a run
                // adding to an existing tree can encounter (in a tree built
                // from nothing, anything on disk was put there by this run
                // and is already in `made_dirs`).
                let existed = target.is_dir();
                std::fs::create_dir_all(&target)?;
                if self.made_dirs.insert(super::destination_key(&item.to)) && !existed {
                    self.outcome.directories += 1;
                }
                continue;
            }

            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let source = sources.get_mut(&item.media).ok_or_else(|| {
                CoreError::InvalidInput(format!(
                    "'{}' names media '{}', which this plan never opened",
                    item.to, item.media
                ))
            })?;
            let raw = source.read(&item.from)?;
            // ART-228. The release's own Installer expands these on the way
            // in and drops the suffix; `plan` has already dropped the suffix
            // from `item.to`, so writing the packed bytes here would put a
            // file the Amiga cannot read under the name it will look for —
            // worse than leaving it alone.
            let bytes = if item.decompress {
                compress::decompress(&raw).map_err(|e| CoreError::Malformed {
                    format: "compress".into(),
                    detail: format!("'{}' on media '{}': {e}", item.from, item.media),
                })?
            } else {
                raw
            };
            let entry = source.entry(&item.from)?.ok_or_else(|| {
                CoreError::InvalidInput(format!(
                    "'{}' is no longer on media '{}'",
                    item.from, item.media
                ))
            })?;

            // `atomic_write`, not `guarded_write` — see the module doc
            // comment's own section on why a `.art-backup/` here would be
            // written onto the card as part of the operating system, and
            // what the tree keeps instead.
            crate::core::safety::atomic::atomic_write(&target, &bytes)?;

            // Only when there is something worth recording — see
            // `sidecar_for`'s own doc comment. Never itself copied as a
            // file: it is written beside `target`, under a name
            // `uaem::sidecar_path` builds by appending `.uaem` rather than
            // replacing the extension. A medium that genuinely carried a
            // file literally called `X.uaem` next to `X` would still collide
            // with `X`'s own sidecar — vanishingly unlikely on real Amiga
            // media, but the code does not rule it out, so this comment
            // shouldn't claim more than it does.
            let protection = self.settle_sidecar(&target, &entry)?;

            self.record(
                &item.to,
                FileRecord {
                    path: item.to.clone(),
                    host_path: host_path_of(&item.to),
                    component: item.component.clone(),
                    media: item.media.clone(),
                    sha256: sha256_bytes(&bytes),
                    bytes: bytes.len() as u64,
                    protection: Some(protection),
                    overwrote: None,
                },
            );
        }
        Ok(())
    }
}

/// Refuse a plan whose destinations would collide on the host — ART-160's
/// corollary. See [`super::host_name_collisions`] for why escaping without
/// this loses a file silently.
fn refuse_host_name_collisions(items: &[PlanItem]) -> CoreResult<()> {
    // Every destination, drawers included — see `host_name_collisions` for
    // why the `is_dir` flag it used to take was the wrong question.
    let destinations: Vec<String> = items.iter().map(|item| item.to.clone()).collect();
    let clashes = super::host_name_collisions(&destinations);
    if clashes.is_empty() {
        return Ok(());
    }
    let named: Vec<String> = clashes
        .iter()
        .map(|(host, first, second)| format!("'{first}' and '{second}' both become '{host}'"))
        .collect();
    Err(CoreError::SafetyRefused(format!(
        "{} destination(s) cannot be told apart once escaped for this filesystem: {} — \
         ART will not write one file where the media hold two",
        clashes.len(),
        some_of(&named)
    )))
}

/// `SAFE_CREATE`'s question, asked once so the engine and the screen cannot
/// answer it differently.
///
/// **What the rule is actually for:** ART never builds over somebody's data.
/// **An empty directory holds none.**
///
/// **ART-203.** It used to refuse anything that existed at all, and that made
/// the screen unusable rather than safe: a folder picker
/// (`open({ directory: true })`) can only return a folder that **exists** —
/// its "New folder" button creates one, and it exists from that moment — so
/// every destination a user could choose was refused. No distribution tree
/// has ever been built from the screen; every one came from the env-gated
/// test hook, which takes a path string and never sees a dialog.
///
/// The idiom is already in this codebase one module over:
/// `core::amigainstall::packagevol::unpack` refuses with *"already has
/// contents; a package is unpacked into an empty directory"*.
///
/// `read_dir().next().is_some()` is the test, so a folder holding **anything
/// at all** — a hidden file included — is still refused. Three outcomes, three
/// sentences: a file where a folder should be is not the same problem as a
/// folder with somebody's work in it, and telling a user the wrong one sends
/// them to the wrong fix.
pub fn refuse_unless_free(root: &Path) -> CoreResult<()> {
    if !root.exists() {
        return Ok(());
    }
    if !root.is_dir() {
        return Err(CoreError::SafetyRefused(format!(
            "'{}' is not a folder — a distribution tree is built into a folder, and this is a file",
            root.display()
        )));
    }
    let mut entries = std::fs::read_dir(root)?;
    if entries.next().is_some() {
        return Err(CoreError::SafetyRefused(format!(
            "'{}' already has something in it — a distribution tree is never built over one that is already there. Choose an empty folder, or a new one",
            root.display()
        )));
    }
    Ok(())
}

/// Build the distribution tree `plan` describes under `root`.
///
/// `SAFE_CREATE` first: `root` must be free — absent, or an **empty**
/// directory. Every medium named in `plan.media_paths` is then opened once,
/// read-only, and hashed whole — the media is never modified by anything below
/// this line. Items are placed one at a time, checking `sink.is_cancelled()`
/// between them and never inside one, so stopping always leaves whole files
/// behind. `distribution.json` is written only after every item has landed —
/// see the module doc comment.
/// Switch on what the media left on the shelf (`super::Activation`).
///
/// **Copies inside the tree, not from the media.** The source is a file
/// `TreeWriter::place` has just written, which is why an activation is not a
/// `PlanItem` and why `plan` checks the source is on the plan rather than
/// finding out here (`detect_missing_activations`).
///
/// Its own function so a test can ask it of a tree built by hand — `apply`
/// refuses a root that is not empty, so a test going through `apply` could
/// only ever activate files the fixture media happens to carry, and the
/// fixture carries no `.info` at all.
fn switch_on(
    root: &Path,
    activations: &[super::plan::PlannedActivation],
    outcome: &mut ApplyOutcome,
    files: &mut Vec<FileRecord>,
    total: u64,
    sink: &dyn ProgressSink,
) -> CoreResult<()> {
    for activation in activations {
        if sink.is_cancelled() {
            return Err(if outcome.files > 0 {
                CoreError::CancelledPartway {
                    files: outcome.files,
                }
            } else {
                CoreError::Cancelled
            });
        }
        sink.report(total, Some(total), &activation.to);

        // The icon is not optional for a commodity — AmigaOS starts what the
        // `WBStartup` icon says, not what the file is — but a driver without
        // one is merely untidy, so a missing icon is skipped rather than
        // refused. The file itself is not: `plan` promised it would be there.
        let pairs = [
            (activation.from.clone(), activation.to.clone()),
            (
                format!("{}.info", activation.from),
                format!("{}.info", activation.to),
            ),
        ];

        for (from, to) in pairs {
            let source = super::host_destination(root, &from)?;
            if !source.is_file() {
                continue;
            }
            let target = super::host_destination(root, &to)?;
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let bytes = std::fs::read(&source)?;
            crate::core::safety::atomic::atomic_write(&target, &bytes)?;

            // Recorded crediting the component that **asked** rather than the
            // medium the bytes came from: `distribution.json` is the only
            // account of that, and switching something back off needs it.
            // `media` is empty for the same reason `S/User-Startup`'s is.
            //
            // Accounted the way the composed file is: a destination already
            // written is a swap, not an addition (ART-124).
            let previous = files
                .iter()
                .find(|f| super::same_destination(&f.path, &to))
                .map(|f| f.bytes);
            files.retain(|f| !super::same_destination(&f.path, &to));
            match previous {
                Some(previous) => outcome.bytes = outcome.bytes - previous + bytes.len() as u64,
                None => {
                    outcome.files += 1;
                    outcome.bytes += bytes.len() as u64;
                }
            }
            files.push(FileRecord {
                path: to.clone(),
                host_path: host_path_of(&to),
                component: activation.component.clone(),
                media: String::new(),
                sha256: sha256_bytes(&bytes),
                bytes: bytes.len() as u64,
                protection: None,
                overwrote: None,
            });
        }
    }
    Ok(())
}

/// The thin wrapper, staging a nested package payload into the platform's own
/// temp directory. **The product never calls it** —
/// [`apply_staging_in`] is what the shell uses, because where ART stages work
/// it will throw away is the user's choice and not this module's (ART-196).
/// Kept for tests and for a future CLI shell that has not chosen a directory,
/// the same split `plan` / [`super::plan::plan_with_cache`] already has.
pub fn apply(plan: &InstallPlan, root: &Path, sink: &dyn ProgressSink) -> CoreResult<ApplyOutcome> {
    apply_staging_in(plan, root, &std::env::temp_dir(), sink)
}

/// [`apply`], unpacking any nested package payload under `scratch_root`.
///
/// Only a package declaring a `member` stages anything at all: install media
/// and a package whose files sit at its archive's own paths are both read in
/// place.
pub fn apply_staging_in(
    plan: &InstallPlan,
    root: &Path,
    scratch_root: &Path,
    sink: &dyn ProgressSink,
) -> CoreResult<ApplyOutcome> {
    // SAFE_CREATE. Nothing below this line touches `root` or any medium
    // until this has passed.
    refuse_unless_free(root)?;

    // A refused plan is not a smaller plan — `plan()` empties `items` and
    // `media_paths` the moment any refusal exists (see its own module doc),
    // so building one anyway would create `root` and write a
    // `distribution.json` with empty `files` and `built_from`: a manifest
    // asserting a complete, empty tree. That is requirement 5's failure —
    // a manifest lying about what it describes — arriving through a
    // different door, so it is refused here by the same rule.
    if !plan.refusals.is_empty() {
        return Err(CoreError::InvalidInput(format!(
            "this plan has {} unresolved refusal(s) and cannot be built",
            plan.refusals.len()
        )));
    }

    // Every medium the plan resolved, opened once — read-only, per
    // `source.rs`'s own module doc — and hashed by streaming
    // (`hashing::sha256_file`), never read whole into memory: CLAUDE.md's
    // own rule ("never read a whole user file into memory when only its
    // header is needed") applies with more force here than anywhere else in
    // this module — a floppy image is under 1 MB, but a real AmigaOS 3.9
    // disc is 469 MB, and `std::fs::read` used to pull the whole thing into
    // a `Vec` purely to feed `sha256_bytes`. `distribution.json` still ends
    // up saying exactly which physical image each component came out of.
    //
    // Opened through `scan::identify` — the same floppy-then-disc probe
    // `find_media` itself uses — rather than `AdfSource::open`
    // unconditionally (ART-153): `plan.media_paths` carries a path, never a
    // `MediaKind`, so `apply()` has to ask the same question `find_media`
    // already answered once, at plan time, on media that has since gone
    // stale (removed, replaced, or simply never was what the plan thinks it
    // is) is a real failure at apply time, not a skip — named by path so the
    // user knows which medium moved.
    let mut sources: BTreeMap<String, Box<dyn MediaSource>> = BTreeMap::new();
    let mut built_from = Vec::new();
    for (volume, path) in &plan.media_paths {
        // **Is this still the disc the plan was made against?** The name
        // check below asks whether it is still *a* Workbench3.2; this asks
        // whether it is *the* one the user was shown. See
        // `plan::MediaStamp` for what it catches and what it does not.
        if let Some(stamped) = plan.media_stamps.get(volume) {
            if let Some(now) = super::scan_cache::identity_of(path) {
                if now.size != stamped.size || now.mtime_nanos != stamped.mtime_nanos {
                    return Err(CoreError::InvalidInput(format!(
                        "'{}' has changed since this install was previewed — it is {} bytes now \
                         and was {}. The preview describes a different disc. Plan it \
                         again, and use Scan again if the folder holds more than one \
                         '{volume}'.",
                        path.display(),
                        now.size,
                        stamped.size
                    )));
                }
            }
        }

        let identified = super::scan::identify(path).ok_or_else(|| {
            CoreError::InvalidInput(format!(
                "'{}' no longer identifies as install media (expected volume '{volume}') — it \
                 may have been moved, replaced or removed since this plan was made",
                path.display()
            ))
        })?;
        // `identify` answers with the name read from **inside** the image,
        // so this is the question the sentence above only implied: the file
        // at that path still opens, *and* it is still the volume this plan
        // keyed it under. A disc swapped for another disc between `plan()`
        // and here would otherwise be read as if it were the first one, with
        // `built_from` recording the right name against the wrong medium.
        // `add_package` asks its own archive exactly this; the two now
        // agree.
        if identified.volume_name != *volume {
            return Err(CoreError::InvalidInput(format!(
                "'{}' now carries volume '{}', not '{volume}' — it was replaced since this plan was made",
                path.display(),
                identified.volume_name
            )));
        }
        let sha256 = sha256_file(path)?;
        built_from.push(MediaRecord {
            volume_name: volume.clone(),
            sha256,
        });
        sources.insert(volume.clone(), super::scan::open_media(&identified)?);
    }

    // The package half of the same question. A package archive is not
    // something `identify` can probe for — see `scan::PackageMedium`'s own
    // doc comment on why a package is not a third `MediaKind` — so it
    // travels on the plan already resolved, member and all, and is opened
    // through `open_package`. Hashed the same way, into the same
    // `built_from`: the archive on disk is the medium this tree's package
    // files came out of, and it is no more kept around afterwards than a
    // floppy is.
    for (media, medium) in &plan.package_media {
        // The same question asked of a package archive, whose identity is
        // its single top-level directory read from inside the file.
        let opened = super::scan::open_package_staging_in(medium, scratch_root)?;
        if opened.volume_name() != media {
            return Err(CoreError::InvalidInput(format!(
                "'{}' now carries '{}', not '{media}' — it was replaced since this plan was made",
                medium.path.display(),
                opened.volume_name()
            )));
        }
        let sha256 = sha256_file(&medium.path)?;
        built_from.push(MediaRecord {
            volume_name: media.clone(),
            sha256,
        });
        if sources.insert(media.clone(), opened).is_some() {
            // A package whose top-level directory happens to be spelled
            // exactly like a release volume in the same plan: `item.media`
            // could then reach either medium, and which one it got would
            // depend on nothing a reader could see. Refused rather than
            // resolved (§89) — no shipped combination produces it, and a
            // future one must not do so silently.
            return Err(CoreError::InvalidInput(format!(
                "'{media}' names both install media and a package in this plan — ART cannot \
                 tell which one a file should be read from"
            )));
        }
    }

    // Before a byte is written, and before any medium is opened: two
    // destinations that escape to one host name would silently become one
    // file (ART-160's corollary — see `host_name_collisions`).
    refuse_host_name_collisions(&plan.items)?;

    std::fs::create_dir_all(root)?;

    let total = plan.items.len() as u64;
    let mut writer = TreeWriter::new(root, Vec::new());
    writer.place(&plan.items, &mut sources, sink)?;
    // Taken apart rather than kept: everything below composes one file the
    // placer has no concept of, and doing that through the writer would put
    // `S/User-Startup`'s own rules inside the thing both entry points share.
    let TreeWriter {
        mut outcome,
        mut files,
        ..
    } = writer;

    // Switches, before the composed file below: a commodity that lands in
    // `WBStartup` here is on disk before anything writes a startup around it.
    switch_on(
        root,
        &plan.activations,
        &mut outcome,
        &mut files,
        total,
        sink,
    )?;

    // `S:User-Startup` — composed, not copied; see the module doc comment's
    // "S:User-Startup" section for all three decisions made here. Skipped
    // entirely when nothing has anything to add, so a plan with no
    // `user_startup` contributions (every shipped component today) never
    // touches the file at all.
    if !plan.user_startup.is_empty() {
        // Same cancellation checkpoint every item above already gets:
        // checked once, before the one atomic write this step performs,
        // never mid-write.
        if sink.is_cancelled() {
            return Err(if outcome.files > 0 {
                CoreError::CancelledPartway {
                    files: outcome.files,
                }
            } else {
                CoreError::Cancelled
            });
        }

        // Through `host_destination` like every other item, even though
        // `S/User-Startup` is a name every filesystem carries — one placer,
        // one rule (ART-160).
        let target = super::host_destination(root, USER_STARTUP_PATH)?;

        // A missing `S` drawer is created, exactly like every other item's
        // missing parent above — see the module doc comment.
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // A `Subtree "S"` rule earlier in `plan.items` may already have
        // copied a real `S/User-Startup` off the release media — the
        // release's own starter file. If so it is already on disk at
        // `target`, and it is read back as the starting point rather than
        // assumed absent, so composing the file never discards content a
        // plan item just wrote. Latin-1, not UTF-8 — see the module doc
        // comment's "Latin-1, not UTF-8" section; this read can never fail
        // on account of what the file says.
        let existing = match std::fs::read(&target) {
            Ok(bytes) => Some(latin1_decode(&bytes)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
            Err(err) => return Err(CoreError::Io(err)),
        };

        let merged = plan
            .user_startup
            .iter()
            .fold(existing, |acc, contribution| {
                Some(merge_user_startup(
                    acc.as_deref(),
                    &contribution.component,
                    &contribution.lines,
                ))
            });
        // `plan.user_startup` is non-empty (checked above), so the fold ran
        // at least once and always produces `Some`.
        let merged = merged.expect("at least one contribution was folded");

        // What actually reaches disk — Latin-1 bytes, not `merged`'s own
        // UTF-8 representation — so `bytes`/`sha256` below describe the
        // real file, the same rule `FileRecord::bytes`'s own doc comment
        // states for every other file this function writes.
        let merged_disk_bytes = latin1_encode(&merged);
        crate::core::safety::atomic::atomic_write(&target, &merged_disk_bytes)?;
        // Reported after the write actually lands, not before it starts —
        // an earlier version of this line ran ahead of the write itself,
        // claiming `done == total` while the one atomic write this step
        // performs had not happened yet.
        sink.report(total, Some(total), USER_STARTUP_PATH);

        let merged_bytes = merged_disk_bytes.len() as u64;
        let merged_sha256 = sha256_bytes(&merged_disk_bytes);

        // Drop whatever record the copy loop above made for this path (a
        // media-provided starter file, if there was one) before adding the
        // composed file's own records — the pre-merge sha256/bytes it
        // carried no longer describe what is actually on disk.
        let previous_bytes = files
            .iter()
            .find(|f| super::same_destination(&f.path, USER_STARTUP_PATH))
            .map(|f| f.bytes);
        files.retain(|f| !super::same_destination(&f.path, USER_STARTUP_PATH));
        match previous_bytes {
            Some(previous_bytes) => {
                outcome.bytes = outcome.bytes - previous_bytes + merged_bytes;
            }
            None => {
                outcome.files += 1;
                outcome.bytes += merged_bytes;
            }
        }

        for contribution in &plan.user_startup {
            files.push(FileRecord {
                path: USER_STARTUP_PATH.to_string(),
                host_path: host_path_of(USER_STARTUP_PATH),
                component: contribution.component.clone(),
                media: String::new(),
                sha256: merged_sha256.clone(),
                bytes: merged_bytes,
                // No single medium's byte to carry over — see the field's own
                // doc comment.
                protection: None,
                overwrote: None,
            });
        }
    }

    sink.report(total, Some(total), "done");

    // Last, deliberately — see the module doc comment. Everything above this
    // line can fail or be cancelled without the tree claiming to be whole;
    // this line is the only thing that makes the claim.
    let manifest = DistributionManifest {
        release: plan.release.clone(),
        built_from,
        files,
        paired_rom: plan.paired_rom.clone(),
        amiga_installed: Vec::new(),
    };
    write_manifest(root, &manifest)?;

    Ok(outcome)
}

/// Serialise `manifest` to the tree's own `distribution.json`.
///
/// `atomic_write`, not `guarded_write` — see the module doc comment's own
/// section. Here specifically: a `.art-backup/` beside `distribution.json`
/// is a directory at the **distribution root**, i.e. at `SYS:` on the
/// finished volume. What the manifest itself owes is that no version of it
/// is ever half-written, which is exactly what `atomic_write` guarantees.
fn write_manifest(root: &Path, manifest: &DistributionManifest) -> CoreResult<()> {
    let text = serde_json::to_string_pretty(manifest).map_err(|err| CoreError::Malformed {
        format: "distribution manifest".into(),
        detail: err.to_string(),
    })?;
    crate::core::safety::atomic::atomic_write(&root.join(MANIFEST_FILE_NAME), text.as_bytes())
}

/// Name what a set of refusals is actually about, for the one caller that
/// cannot hand them to the UI as values: [`add_package`] returns a
/// `CoreResult`, not a plan. Rendered in English like every other
/// `CoreError` message (ART-060); the typed refusals themselves stay the
/// plan path's business.
fn refusal_summary(refusals: &[super::RefusalReason]) -> String {
    use super::RefusalReason as R;
    refusals
        .iter()
        .map(|refusal| match refusal {
            R::MediaPathMissing { path, media, .. } => format!("'{path}' is not on '{media}'"),
            R::RuleKindMismatch {
                from,
                expected,
                found,
                ..
            } => format!("'{from}' is a {found:?} where a {expected:?} was expected"),
            other => format!("{other:?}"),
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Add one package to a distribution tree that already exists.
///
/// The second way into [`TreeWriter`]. `plan()`+[`apply`] build a tree with
/// its packages already in it; this puts one onto a tree that was built
/// without it, and the two must agree file for file — which is what
/// `producing_with_a_package_equals_adding_it_afterwards` proves rather than
/// asserts.
///
/// **`archive` is given, not looked up.** Which file this package is comes
/// from `scan::find_packages` + `scan::package_for`, at plan time, in front
/// of a user who can be told about an ambiguity; a second discovery pass in
/// here would be a second place for "which file is this package" to be
/// decided, and the two could answer differently on a folder holding
/// `BoingBag39-2.lha` and its eight language variants.
///
/// **A tree with no `distribution.json` is refused.** Without the manifest
/// ART cannot say what a file it is about to overwrite *was* — which
/// component put it there, off which medium, with which hash — so it could
/// neither record what it displaced ([`FileRecord::overwrote`]) nor let
/// `collide::preview` say anything true beforehand. The whole preview rests
/// on knowing, and a tree that cannot answer is not one to write into.
///
/// **What it does not do.** It does not compose `S:User-Startup`: that file
/// is a release plan's own last step, folded from the `user_startup` lines
/// of every switched-on component, and no shipped package carries any. A
/// package that placed a real file at `S/User-Startup` on a tree whose
/// manifest holds composed records for it would replace them here, where
/// `apply` would have re-composed afterwards — the one shape in which the
/// two entry points do not agree, stated rather than papered over, and
/// unreachable with anything ART ships today.
pub fn add_package(
    tree_root: &Path,
    package: &super::package::Package,
    archive: &Path,
    sink: &dyn ProgressSink,
) -> CoreResult<ApplyOutcome> {
    add_package_staging_in(tree_root, package, archive, &std::env::temp_dir(), sink)
}

/// [`add_package`], unpacking a nested payload under `scratch_root` (ART-196).
pub fn add_package_staging_in(
    tree_root: &Path,
    package: &super::package::Package,
    archive: &Path,
    scratch_root: &Path,
    sink: &dyn ProgressSink,
) -> CoreResult<ApplyOutcome> {
    // Before anything is opened, let alone written.
    let manifest_path = tree_root.join(MANIFEST_FILE_NAME);
    if !manifest_path.is_file() {
        return Err(CoreError::SafetyRefused(format!(
            "'{}' holds no {MANIFEST_FILE_NAME}, so ART cannot say what adding '{}' would \
             overwrite — a package is never applied to a tree that cannot account for itself",
            tree_root.display(),
            package.id
        )));
    }
    let mut manifest: DistributionManifest =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path)?).map_err(|err| {
            CoreError::Malformed {
                format: "distribution manifest".into(),
                detail: err.to_string(),
            }
        })?;

    // The archive the caller resolved has to actually be this package's:
    // `ArchiveSource` reads a package's identity from its single top-level
    // directory, never from the filename, so this is the same
    // "is this still the medium the plan meant?" question `apply` asks of
    // every floppy through `scan::identify`.
    let medium = super::scan::PackageMedium {
        path: archive.to_path_buf(),
        member: package.member.clone(),
    };
    let mut source = super::scan::open_package_staging_in(&medium, scratch_root)?;
    if source.volume_name() != package.media {
        return Err(CoreError::InvalidInput(format!(
            "'{}' carries '{}', not '{}' — this is not the archive package '{}' names",
            archive.display(),
            source.volume_name(),
            package.media,
            package.id
        )));
    }

    // A name already meaning something else in this tree would make
    // `built_from` ambiguous about which medium a file came out of.
    if let Some(clash) = manifest
        .files
        .iter()
        .find(|file| file.media == package.media && file.component != package.id)
    {
        return Err(CoreError::InvalidInput(format!(
            "'{}' already names the medium component '{}' was installed from in this tree — ART \
             will not add a package under a name that already means something else",
            package.media, clash.component
        )));
    }

    let mut refusals = Vec::new();
    let items = super::plan::expand_rules(&package.component, source.as_mut(), &mut refusals)?;
    if !refusals.is_empty() {
        // Refused before a byte is written, never partway through: every
        // rule this package declares has to resolve on the archive it was
        // given, or the archive is not the one the recipe was measured
        // against.
        return Err(CoreError::InvalidInput(format!(
            "'{}' cannot be added from '{}': {}",
            package.id,
            archive.display(),
            refusal_summary(&refusals)
        )));
    }

    // The same host-escaping check `apply` runs, for the same reason: a
    // package's own two files can collide with each other once escaped, and
    // Add writes through the identical placer.
    refuse_host_name_collisions(&items)?;

    // Nothing is overwritten silently — the round's central rule, and the
    // one Produce enforces through `plan::detect_collisions`. Add has to
    // reach the same verdict on the same facts, or the two entry points
    // disagree about whether an install is allowed at all.
    let (undeclared, unrecorded) = undeclared_overwrites(tree_root, package, &items, &manifest)?;
    if !undeclared.is_empty() {
        return Err(CoreError::SafetyRefused(format!(
            "'{}' would write over {} file(s) it never declared it may replace: {} — a package overwrites only what its own `overrides` names",
            package.id,
            undeclared.len(),
            some_of(&undeclared)
        )));
    }
    // A different problem and a different sentence: these files are in the
    // tree and not in `distribution.json`, so nothing says who put them there
    // and ART cannot tell what it would be replacing. Telling the user to
    // declare an override would be the wrong instruction — there is no
    // component to declare one over.
    if !unrecorded.is_empty() {
        return Err(CoreError::SafetyRefused(format!(
            "'{}' would write over {} file(s) that {MANIFEST_FILE_NAME} does not record: {} — ART cannot say what it would be replacing, so it replaces nothing",
            package.id,
            unrecorded.len(),
            some_of(&unrecorded)
        )));
    }

    let sha256 = sha256_file(archive)?;
    let mut sources: BTreeMap<String, Box<dyn MediaSource>> = BTreeMap::new();
    sources.insert(package.media.clone(), source);

    let mut writer = TreeWriter::new(tree_root, std::mem::take(&mut manifest.files));
    // **The manifest is updated whatever happened.** `apply` can leave no
    // manifest at all when it stops early, because a half-built tree that
    // claims nothing is honest. Add cannot: the tree already had a manifest,
    // files on disk have already changed, and leaving the old one in place
    // would make `distribution.json` describe bytes that are no longer
    // there — the one thing this file exists to prevent. So a cancellation
    // (or any other failure) still writes what actually landed, and only
    // then reports itself.
    let placed = writer.place(&items, &mut sources, sink);
    let TreeWriter { outcome, files, .. } = writer;
    manifest.files = files;

    // Re-added in place when this archive is already recorded (adding the
    // same package twice), appended otherwise — so `built_from`'s order is
    // the order media first contributed to the tree, in both entry points.
    let record = MediaRecord {
        volume_name: package.media.clone(),
        sha256,
    };
    match manifest
        .built_from
        .iter_mut()
        .find(|m| m.volume_name == package.media)
    {
        Some(existing) => *existing = record,
        None => manifest.built_from.push(record),
    }

    // Last, for the reason the module doc comment gives: until this line the
    // tree still describes itself as what it was, which is true right up
    // until it is not.
    let recorded = write_manifest(tree_root, &manifest);

    // The placer's own failure explains the run, so it is reported first; a
    // manifest that could not be written is the next-worst thing to know and
    // is never swallowed.
    placed?;
    recorded?;

    Ok(outcome)
}

/// Every destination in `items` that already holds a file `package` never
/// declared it may replace — Add's half of `plan::detect_collisions`.
///
/// The other claimant is not another item here; it is the tree, and
/// `distribution.json` is what says who put each file there. So the question
/// is the same one Produce asks and the answer comes from a different place:
/// a destination whose manifest record names some other component needs that
/// component in the package's own `overrides`.
///
/// Read from `package.component.overrides` directly rather than through
/// `package::by_id` (which `collide::declared_override` uses): the caller
/// already resolved this package, and looking it up again would be a second
/// place for "which package is this" to be answered — and would refuse a
/// package that is not in the shipped list at all.
///
/// **A file the manifest never recorded is undeclared too.** Nothing in the
/// tree claims it, so nothing could have declared an override over it; it is
/// most likely something the user put there themselves, which is exactly the
/// case where writing over it silently would be worst.
///
/// Directories are not claims — the same rule `detect_collisions` follows,
/// where a coinciding `Subtree` destination is a merge point rather than an
/// overwrite.
#[allow(clippy::type_complexity)]
fn undeclared_overwrites(
    tree_root: &Path,
    package: &super::package::Package,
    items: &[PlanItem],
    manifest: &DistributionManifest,
) -> CoreResult<(Vec<String>, Vec<String>)> {
    let mut undeclared = Vec::new();
    let mut unrecorded = Vec::new();
    for item in items.iter().filter(|item| !item.is_dir) {
        // The host path, not the AmigaDOS one — a file the tree carries
        // as `_AUX` is `Storage/DOSDrivers/AUX` to everything else here
        // (ART-160).
        let target = super::host_destination(tree_root, &item.to)?;
        if !target.is_file() {
            continue;
        }
        // `same_destination`, not `==`: the manifest records whatever the
        // release media spelled (`C/Assign` off the 3.9 disc's Joliet tree,
        // `C/ASSIGN` off its Primary one) and the package spells whatever its
        // own payload does. An exact match found no owner for all ~211 of a
        // real BoingBag's files and refused every one of them as undeclared —
        // see `destination_key`'s own doc comment.
        let owner = manifest
            .files
            .iter()
            .find(|file| super::same_destination(&file.path, &item.to))
            .map(|file| file.component.as_str());
        match owner {
            // This package rewriting its own file — adding the same package
            // twice, which is a replacement, not a surprise.
            Some(owner) if owner == package.id => {}
            Some(owner) if package.component.overrides.iter().any(|over| over == owner) => {}
            Some(_) => undeclared.push(item.to.clone()),
            None => unrecorded.push(item.to.clone()),
        }
    }
    Ok((undeclared, unrecorded))
}

/// At most this many paths are named in a refusal, with a count for the
/// rest.
///
/// A real BoingBag carries 211 files, so "name every one" is a wall of text
/// nobody reads and a log line nothing can hold. Enough to recognise the
/// shape of the problem, and then the number.
const REFUSAL_PATHS_SHOWN: usize = 5;

/// `paths`, capped — see [`REFUSAL_PATHS_SHOWN`].
fn some_of(paths: &[String]) -> String {
    if paths.len() <= REFUSAL_PATHS_SHOWN {
        return paths.join(", ");
    }
    format!(
        "{}, and {} more",
        paths[..REFUSAL_PATHS_SHOWN].join(", "),
        paths.len() - REFUSAL_PATHS_SHOWN
    )
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::jobs::NoProgress;
    use crate::core::osinstall::fixtures;
    use crate::core::osinstall::plan::{PlanItem, UserStartupContribution};
    use crate::core::osinstall::source::AdfSource;
    use crate::core::osinstall::source_cd::CdSource;

    /// Diagnostic-only, for `build_the_real_39_tree_when_asked`'s failure
    /// path: count what actually landed under `root` after a partial
    /// `apply()` run — a real, measured number rather than a guess about
    /// how far it got.
    fn count_tree(root: &Path) -> (u64, u64) {
        let mut files = 0u64;
        let mut dirs = 0u64;
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(read) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in read.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    dirs += 1;
                    stack.push(path);
                } else {
                    files += 1;
                }
            }
        }
        (files, dirs)
    }

    /// A plan `apply` can run against directly, without going through
    /// `plan()` — `apply` only ever consumes the `InstallPlan` struct, so a
    /// test of `apply` alone should not have to satisfy every rule `plan()`
    /// enforces along the way. Built by hand instead, the way `plan.rs`'s
    /// own hand-built-recipe tests already do for shapes the shipped recipe
    /// cannot produce.
    ///
    /// Two files, both real entries on one medium, `ModulesA1200_3.2` —
    /// matching the shipped recipe's own `modules-a1200` component and its
    /// `C/LoadModule` rule, so a test reading the manifest afterwards is
    /// checking a real, recognisable shape rather than an invented one.
    /// `C/LoadModule` carries protection `0x20` (`--p-rwed`) — the exact
    /// fixture `source.rs`'s own protection test uses, and the bit pattern
    /// the module doc comment on `uaem.rs` calls out as load-bearing:
    /// AmigaOS 3.2's `Startup-Sequence` runs `Resident C:Assign PURE` and
    /// fails without it. `C/Other` exists so a cancellation test has a
    /// second file to stop before reaching.
    fn planned() -> (InstallPlan, PathBuf) {
        // A fresh scratch directory every call, not a fixed tag: several of
        // this module's own tests call `planned()` and run in parallel
        // threads of the same test binary (same pid), and `fixtures::scratch`
        // keys only on tag + pid — a shared tag would let two tests race over
        // the same directory, which is exactly what happened before this
        // counter was added (see the report).
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = fixtures::scratch(&format!("apply-planned-{n}"));
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();
        fixtures::media(
            &folder,
            "ModulesA1200_3.2",
            "modules.adf",
            &[("C/LoadModule", b"cmd", 0x20), ("C/Other", b"more", 0x00)],
        );

        let mut media_paths = BTreeMap::new();
        media_paths.insert("ModulesA1200_3.2".to_string(), folder.join("modules.adf"));

        let items = vec![
            PlanItem {
                component: "modules-a1200".into(),
                media: "ModulesA1200_3.2".into(),
                from: "C/LoadModule".into(),
                to: "C/LoadModule".into(),
                is_dir: false,
                bytes: 3,
                decompress: false,
            },
            PlanItem {
                component: "modules-a1200".into(),
                media: "ModulesA1200_3.2".into(),
                from: "C/Other".into(),
                to: "C/Other".into(),
                is_dir: false,
                bytes: 4,
                decompress: false,
            },
        ];
        let total_bytes = items.iter().map(|i| i.bytes).sum();
        let total_files = items.iter().filter(|i| !i.is_dir).count() as u64;

        let plan = InstallPlan {
            release: "AmigaOS 3.2".into(),
            items,
            refusals: Vec::new(),
            packages: Vec::new(),
            package_media: BTreeMap::new(),
            total_bytes,
            total_files,
            components_on: vec!["modules-a1200".into()],
            paired_rom: None,
            media_paths,
            user_startup: Vec::new(),
            activations: Vec::new(),
            media_stamps: BTreeMap::new(),
        };
        (plan, dir)
    }

    fn media_folder(dir: &Path) -> PathBuf {
        dir.join("media")
    }

    /// A plan whose destinations a Windows filesystem will not carry
    /// verbatim — ART-160.
    ///
    /// `Storage/DOSDrivers/AUX` is not invented: it is on the owner's own
    /// AmigaOS 3.9 disc, and `AUX` is one of the 22 device names Windows has
    /// reserved since DOS. `Devs/Prices: 1993` is the other half of the same
    /// problem and the harder one — a colon is legal in an AmigaDOS filename
    /// and NTFS refuses it outright, so before this fix `apply()` did not
    /// write it under a wrong name, it failed with a raw OS error partway
    /// through building the tree.
    fn planned_with_host_hostile_names() -> (InstallPlan, PathBuf) {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = fixtures::scratch(&format!("apply-hostile-{n}"));
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();
        fixtures::media(
            &folder,
            "Workbench3.9",
            "wb.adf",
            &[("AUX", b"aux-driver", 0x00), ("Prices", b"listing", 0x00)],
        );

        let mut media_paths = BTreeMap::new();
        media_paths.insert("Workbench3.9".to_string(), folder.join("wb.adf"));

        let items = vec![
            PlanItem {
                component: "workbench-base".into(),
                media: "Workbench3.9".into(),
                from: "AUX".into(),
                to: "Storage/DOSDrivers/AUX".into(),
                is_dir: false,
                bytes: 10,
                decompress: false,
            },
            PlanItem {
                component: "workbench-base".into(),
                media: "Workbench3.9".into(),
                from: "Prices".into(),
                to: "Devs/Prices: 1993".into(),
                is_dir: false,
                bytes: 7,
                decompress: false,
            },
        ];
        let total_bytes = items.iter().map(|i| i.bytes).sum();
        let total_files = items.iter().filter(|i| !i.is_dir).count() as u64;

        let plan = InstallPlan {
            release: "AmigaOS 3.9".into(),
            items,
            refusals: Vec::new(),
            packages: Vec::new(),
            package_media: BTreeMap::new(),
            total_bytes,
            total_files,
            components_on: vec!["workbench-base".into()],
            paired_rom: None,
            media_paths,
            user_startup: Vec::new(),
            activations: Vec::new(),
            media_stamps: BTreeMap::new(),
        };
        (plan, dir)
    }

    /// **ART-160's corollary (F5): two names, one host file.**
    ///
    /// `windows_safe_name` maps every refused character onto the single
    /// replacement `_`, so `Prices: 1993` and `Prices? 1993` — two different,
    /// legal AmigaDOS filenames — escape to the same host name. `apply` wrote
    /// items in order and `atomic_write` replaces, so the second silently
    /// overwrote the first and the tree held one file where the media held
    /// two. Refused before anything is written.
    #[test]
    fn two_destinations_that_escape_to_one_host_name_are_refused() {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = fixtures::scratch(&format!("apply-hostclash-{n}"));
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();
        fixtures::media(
            &folder,
            "Workbench3.9",
            "wb.adf",
            &[("A", b"first", 0x00), ("B", b"second", 0x00)],
        );

        let mut media_paths = BTreeMap::new();
        media_paths.insert("Workbench3.9".to_string(), folder.join("wb.adf"));

        let items = vec![
            PlanItem {
                component: "workbench-base".into(),
                media: "Workbench3.9".into(),
                from: "A".into(),
                to: "Devs/Prices: 1993".into(),
                is_dir: false,
                bytes: 5,
                decompress: false,
            },
            PlanItem {
                component: "workbench-base".into(),
                media: "Workbench3.9".into(),
                from: "B".into(),
                to: "Devs/Prices? 1993".into(),
                is_dir: false,
                bytes: 6,
                decompress: false,
            },
        ];
        let plan = InstallPlan {
            release: "AmigaOS 3.9".into(),
            items,
            refusals: Vec::new(),
            packages: Vec::new(),
            package_media: BTreeMap::new(),
            total_bytes: 11,
            // Two items, two destinations — they escape to one *host* name,
            // which is the refusal this fixture exists for, but the plan's
            // own count is of AmigaDOS destinations.
            total_files: 2,
            components_on: vec!["workbench-base".into()],
            paired_rom: None,
            media_paths,
            user_startup: Vec::new(),
            activations: Vec::new(),
            media_stamps: BTreeMap::new(),
        };

        let root = dir.join("dist");
        let err = apply(&plan, &root, &NoProgress).unwrap_err();
        assert_eq!(err.code(), "ART-SAFETY-REFUSED");
        let msg = format!("{err}");
        assert!(msg.contains("Prices: 1993"), "{msg}");
        assert!(msg.contains("Prices? 1993"), "{msg}");
        assert!(
            msg.contains("Prices_ 1993"),
            "the host name they share: {msg}"
        );

        // Refused *before* anything was written — not a tree with one file in
        // it and an error afterwards.
        assert!(
            !root.exists(),
            "a refused plan must leave no tree behind at all"
        );
    }

    /// **F6, the same defect from the other side.** A reserved name and a
    /// genuine `_`-prefixed one collide too — and that pair is worse, because
    /// the survivor would then be copied onto the volume under the *other*
    /// one's AmigaDOS name.
    #[test]
    fn a_reserved_name_and_a_real_underscore_name_are_refused_together() {
        let items = vec![
            "Storage/DOSDrivers/AUX".to_string(),
            "Storage/DOSDrivers/_AUX".to_string(),
        ];
        let clashes = crate::core::osinstall::host_name_collisions(&items);
        assert_eq!(clashes.len(), 1, "{clashes:?}");
        assert_eq!(clashes[0].0, "Storage/DOSDrivers/_AUX");
    }

    /// **R1, the reviewer's own case.** The collision map used to be keyed
    /// **exact-case** while the comparison beside it folds ASCII case, so a
    /// pair differing only in case never met in the map: `apply` returned
    /// `Ok`, wrote one file and recorded two.
    ///
    /// `Devs/Prices: 1993` claims `Devs/Prices_ 1993` and `Devs/prices? 1993`
    /// claims `Devs/prices_ 1993` — two keys, one file on a case-insensitive
    /// filesystem. `destination_key` is what both sides use now.
    #[test]
    fn two_destinations_differing_only_in_case_still_collide() {
        let items = vec![
            "Devs/Prices: 1993".to_string(),
            "Devs/prices? 1993".to_string(),
        ];
        let clashes = crate::core::osinstall::host_name_collisions(&items);
        assert_eq!(
            clashes.len(),
            1,
            "the case fold must not hide it: {clashes:?}"
        );
        assert_eq!(clashes[0].1, "Devs/Prices: 1993");
        assert_eq!(clashes[0].2, "Devs/prices? 1993");
    }

    /// The same, end to end: the reviewer reproduced this through `apply`,
    /// so the test that closes it goes through `apply` too rather than only
    /// through the detector.
    #[test]
    fn a_case_differing_pair_is_refused_by_apply_not_written_once_and_recorded_twice() {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = fixtures::scratch(&format!("apply-hostcase-{n}"));
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();
        fixtures::media(
            &folder,
            "Workbench3.9",
            "wb.adf",
            &[("A", b"first", 0x00), ("B", b"second", 0x00)],
        );

        let mut media_paths = BTreeMap::new();
        media_paths.insert("Workbench3.9".to_string(), folder.join("wb.adf"));

        let items = vec![
            PlanItem {
                component: "workbench-base".into(),
                media: "Workbench3.9".into(),
                from: "A".into(),
                to: "Devs/Prices: 1993".into(),
                is_dir: false,
                bytes: 5,
                decompress: false,
            },
            PlanItem {
                component: "workbench-base".into(),
                media: "Workbench3.9".into(),
                from: "B".into(),
                to: "Devs/prices? 1993".into(),
                is_dir: false,
                bytes: 6,
                decompress: false,
            },
        ];
        let plan = InstallPlan {
            release: "AmigaOS 3.9".into(),
            items,
            refusals: Vec::new(),
            packages: Vec::new(),
            package_media: BTreeMap::new(),
            total_bytes: 11,
            // Two items, two destinations — they escape to one *host* name,
            // which is the refusal this fixture exists for, but the plan's
            // own count is of AmigaDOS destinations.
            total_files: 2,
            components_on: vec!["workbench-base".into()],
            paired_rom: None,
            media_paths,
            user_startup: Vec::new(),
            activations: Vec::new(),
            media_stamps: BTreeMap::new(),
        };

        let root = dir.join("dist");
        let err = apply(&plan, &root, &NoProgress).unwrap_err();
        assert_eq!(err.code(), "ART-SAFETY-REFUSED");
        assert!(!root.exists(), "nothing may be written for a refused plan");
    }

    /// The ordinary tree is unaffected: nothing escapes, so nothing clashes.
    #[test]
    fn destinations_that_escape_to_nothing_do_not_clash() {
        let items = vec![
            "C/LoadModule".to_string(),
            "C/Other".to_string(),
            "Libs/icon.library".to_string(),
        ];
        assert!(crate::core::osinstall::host_name_collisions(&items).is_empty());
    }

    /// Two spellings of the *same* destination are not a clash — that is the
    /// ordinary overwrite `FileRecord::overwrote` already records, and
    /// refusing it would break every package that legitimately replaces a
    /// base file (the 3.9 disc spells `C/ASSIGN`, a BoingBag `C/Assign`).
    #[test]
    fn the_same_destination_spelled_twice_is_not_a_clash() {
        let items = vec!["C/ASSIGN".to_string(), "C/Assign".to_string()];
        assert!(crate::core::osinstall::host_name_collisions(&items).is_empty());
    }

    /// **R1's corollary, and this test used to assert the opposite.** It was
    /// written as "two components claiming one drawer is not a clash", with
    /// `Storage/AUX` and `Storage/_AUX` as the example — but those are not
    /// one drawer claimed twice, they are **two different drawers becoming
    /// one**, and every file under both would land in it. `core/preload`
    /// resolves a host drawer to a single AmigaDOS name, so one whole subtree
    /// would arrive on the volume under the other's name.
    ///
    /// The rule it was reaching for is real and still holds — see the test
    /// below.
    #[test]
    fn two_different_drawers_that_escape_to_one_are_refused() {
        let items = vec!["Storage/AUX".to_string(), "Storage/_AUX".to_string()];
        let clashes = crate::core::osinstall::host_name_collisions(&items);
        assert_eq!(clashes.len(), 1, "{clashes:?}");
        assert_eq!(clashes[0].0, "Storage/_AUX");
    }

    /// Two components creating *the same* drawer is still fine — the case the
    /// test above was actually reaching for. `same_destination` answers it,
    /// which is why no `is_dir` flag is needed to tell the two apart.
    #[test]
    fn two_components_creating_the_same_drawer_is_not_a_clash() {
        let items = vec![
            "Storage/DOSDrivers".to_string(),
            "Storage/DOSDrivers".to_string(),
            "STORAGE/DOSDRIVERS".to_string(),
        ];
        assert!(crate::core::osinstall::host_name_collisions(&items).is_empty());
    }

    /// A drawer and a file that escape onto one name are a collision too —
    /// nothing about the kinds makes it safer.
    #[test]
    fn a_drawer_and_a_file_that_escape_to_one_name_are_refused() {
        let items = vec!["Devs/AUX".to_string(), "Devs/_AUX".to_string()];
        assert_eq!(
            crate::core::osinstall::host_name_collisions(&items).len(),
            1
        );
    }

    /// **ART-160.** The tree escapes what the host cannot carry, and the
    /// manifest keeps both names.
    #[test]
    fn a_name_windows_reserves_is_escaped_on_disk_and_recorded_in_the_manifest() {
        let (plan, dir) = planned_with_host_hostile_names();
        let root = dir.join("dist");
        apply(&plan, &root, &NoProgress).unwrap();

        // On disk under the escaped name...
        assert!(root
            .join("Storage")
            .join("DOSDrivers")
            .join("_AUX")
            .is_file());
        assert!(!root.join("Storage").join("DOSDrivers").join("AUX").exists());
        // ...and the colon one exists at all, which is the half that used to
        // fail with an OS error rather than land wrong.
        assert!(root.join("Devs").join("Prices_ 1993").is_file());

        let manifest: DistributionManifest =
            serde_json::from_str(&std::fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap())
                .unwrap();

        // `path` is always the AmigaDOS name — it is what has to reach the
        // volume, and what `verify_volume` looks up on it.
        let aux = manifest
            .files
            .iter()
            .find(|f| f.path == "Storage/DOSDrivers/AUX")
            .expect("the manifest records the Amiga name");
        assert_eq!(
            aux.host_path.as_deref(),
            Some("Storage/DOSDrivers/_AUX"),
            "and the host name it actually landed at, beside it"
        );

        let prices = manifest
            .files
            .iter()
            .find(|f| f.path == "Devs/Prices: 1993")
            .expect("the manifest records the Amiga name");
        assert_eq!(prices.host_path.as_deref(), Some("Devs/Prices_ 1993"));
    }

    /// The round trip that makes the escape safe: `core/preload` reads the
    /// Amiga name back off the manifest rather than off the host filename,
    /// so what reaches the card is `AUX`, not `_AUX`.
    ///
    /// Asked here, in the module that wrote the manifest, because the pairing
    /// is only correct if both halves agree — `core/preload`'s own tests pin
    /// the reader against hand-written records, and this pins the reader
    /// against a manifest `apply()` actually produced.
    #[test]
    fn the_amiga_name_is_recoverable_from_the_tree_apply_wrote() {
        let (plan, dir) = planned_with_host_hostile_names();
        let root = dir.join("dist");
        apply(&plan, &root, &NoProgress).unwrap();

        let names = crate::core::preload::amiga_names::AmigaNames::read(&root);
        assert_eq!(names.name_for("Storage/DOSDrivers/_AUX"), Some("AUX"));
        assert_eq!(names.name_for("Devs/Prices_ 1993"), Some("Prices: 1993"));
    }

    /// A tree with nothing to escape carries no `hostPath` at all — the field
    /// is absent from the JSON, not `null`, so a three-thousand-file manifest
    /// is unchanged by this feature existing.
    #[test]
    fn an_ordinary_tree_records_no_host_path() {
        let (plan, dir) = planned();
        let root = dir.join("dist");
        apply(&plan, &root, &NoProgress).unwrap();

        let text = std::fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap();
        assert!(!text.contains("hostPath"), "{text}");
        let manifest: DistributionManifest = serde_json::from_str(&text).unwrap();
        assert!(manifest.files.iter().all(|f| f.host_path.is_none()));
        assert!(crate::core::preload::amiga_names::AmigaNames::read(&root).is_empty());
    }

    #[test]
    fn the_tree_carries_a_uaem_sidecar_for_every_file_with_something_to_say() {
        let (plan, dir) = planned();
        let root = dir.join("dist");
        apply(&plan, &root, &NoProgress).unwrap();

        let sidecar = root.join("C").join("LoadModule.uaem");
        assert!(sidecar.exists());
        assert!(std::fs::read_to_string(&sidecar)
            .unwrap()
            .starts_with("--p-rwed"));
    }

    #[test]
    fn the_manifest_says_which_component_and_which_media_each_file_came_from() {
        let (plan, dir) = planned();
        let root = dir.join("dist");
        apply(&plan, &root, &NoProgress).unwrap();

        let manifest: DistributionManifest =
            serde_json::from_str(&std::fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap())
                .unwrap();
        let record = manifest
            .files
            .iter()
            .find(|f| f.path == "C/LoadModule")
            .unwrap();
        assert_eq!(record.component, "modules-a1200");
        assert_eq!(record.media, "ModulesA1200_3.2");
        // The actual digest of the known content, not merely its length — a
        // hash of the path, the sidecar text, or a placeholder string would
        // also happen to be 64 hex characters long. `FileRecord::bytes` is
        // covered on its own by `the_manifest_records_the_real_size_written_
        // not_the_plans_estimate` below, which is the one that can actually
        // fail for a wrong size (`plan.items[0].bytes` here is 3, which
        // happens to already be correct, so asserting it here again would
        // not prove anything that test doesn't already prove better).
        assert_eq!(record.sha256, sha256_bytes(b"cmd"));

        assert_eq!(manifest.release, "AmigaOS 3.2");
        let media_record = manifest
            .built_from
            .iter()
            .find(|m| m.volume_name == "ModulesA1200_3.2")
            .unwrap();
        let expected_media_hash = {
            let raw = std::fs::read(media_folder(&dir).join("modules.adf")).unwrap();
            sha256_bytes(&raw)
        };
        assert_eq!(media_record.sha256, expected_media_hash);
    }

    /// The `plan()` → `apply()` seam, exercised for real. Every other test
    /// in this module hand-builds an `InstallPlan` (see `planned()`'s own
    /// doc comment for why), and every one of those hand-built plans sets
    /// `is_dir: false` throughout — so none of them ever walked `apply`'s
    /// directory branch (`create_dir_all` + `outcome.directories += 1`) at
    /// all. That branch is the one a real plan hits *first*, on almost
    /// every component: `workbench-base`'s own rules are all `Subtree`, and
    /// `plan()` always emits a `Subtree` rule's own root directory before
    /// anything inside it (see `plan.rs`'s comment: "the subtree's own
    /// root, so an empty drawer still gets created"). This test runs the
    /// real `plan()` — via `fixtures::planned_with`, the same helper
    /// `plan.rs`'s own tests use — over the shipped recipe's
    /// `workbench-base` component, and checks the two things nothing else
    /// in this module checked: `ApplyOutcome` itself, and that the manifest
    /// agrees with it.
    #[test]
    fn a_real_plan_builds_a_tree_that_matches_the_plan_including_its_directories() {
        let (plan, dir) = fixtures::planned_with(&["workbench-base"], &["Workbench3.2"], Some(47));
        assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
        let root = dir.join("dist");

        let outcome = apply(&plan, &root, &NoProgress).unwrap();

        // Items and entries are the same number *here* because one component
        // alone writes no destination twice. Where that stops being true —
        // an `overrides` relationship — the report counts entries, and
        // `the_report_counts_what_the_tree_holds_not_what_the_plan_did` is
        // the test for it (ART-124).
        let expected_files = plan.items.iter().filter(|i| !i.is_dir).count() as u64;
        let expected_dirs = plan.items.iter().filter(|i| i.is_dir).count() as u64;
        assert_eq!(outcome.files, expected_files);
        assert_eq!(outcome.directories, expected_dirs);
        assert!(
            outcome.directories > 0,
            "workbench-base's rules are all Subtree — this plan must have \
             produced at least one directory item, or this test is not \
             exercising the branch it claims to"
        );

        // Every item lands, and as the right kind — proves the directory
        // branch actually created directories rather than, say, silently
        // treating every item as a file.
        for item in &plan.items {
            let target = root.join(&item.to);
            assert!(target.exists(), "'{}' was never created", item.to);
            assert_eq!(
                target.is_dir(),
                item.is_dir,
                "'{}' landed as the wrong kind",
                item.to
            );
        }

        let manifest: DistributionManifest =
            serde_json::from_str(&std::fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap())
                .unwrap();
        assert_eq!(
            manifest.files.len() as u64,
            outcome.files,
            "the manifest must name exactly the files ApplyOutcome counted"
        );
    }

    // ---- Is this still the disc the preview described? ----
    //
    // `apply` asks whether the file is still *a* `Workbench3.2`. These ask
    // whether it is *the* one. A different disc of the same name used to
    // build silently while the screen described the old one.

    /// **The disc was swapped between the preview and the build.**
    ///
    /// Asserted by changing the file on disk after planning, which is the
    /// thing that actually happens — a check written against a hand-edited
    /// `media_stamps` would prove the comparison and not the stamping.
    #[test]
    fn a_medium_that_changed_since_the_preview_is_refused_by_name() {
        let (plan, dir) = fixtures::planned_with(&["workbench-base"], &["Workbench3.2"], Some(47));
        assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
        assert!(
            !plan.media_stamps.is_empty(),
            "the plan has to have recorded what it saw"
        );

        // Somebody puts a different disk of the same name in the folder.
        let medium = plan
            .media_paths
            .get("Workbench3.2")
            .expect("the plan resolved it")
            .clone();
        let mut bytes = std::fs::read(&medium).unwrap();
        bytes.extend_from_slice(&[0u8; 512]);
        std::fs::write(&medium, &bytes).unwrap();

        let root = dir.join("dist");
        let err = apply(&plan, &root, &NoProgress).expect_err("the swap must be refused");
        let text = err.to_string();
        assert!(
            text.contains(&medium.display().to_string()),
            "the refusal must name the medium: {text}"
        );
        assert!(
            text.contains("previewed"),
            "and say what is wrong with it: {text}"
        );
        assert!(
            !root.exists() || std::fs::read_dir(&root).unwrap().next().is_none(),
            "and nothing may have been written from the wrong disc"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The ordinary case still builds. A check that refused everything would
    /// pass the test above and be useless.
    #[test]
    fn an_unchanged_medium_builds_exactly_as_before() {
        let (plan, dir) = fixtures::planned_with(&["workbench-base"], &["Workbench3.2"], Some(47));
        let root = dir.join("dist");
        let outcome =
            apply(&plan, &root, &NoProgress).expect("nothing changed, so nothing refuses");
        assert!(outcome.files > 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A plan from an older ART carries no stamps at all, and must still
    /// build — `#[serde(default)]` gives it an empty map, and an empty map
    /// means "nothing was recorded", never "everything changed".
    #[test]
    fn a_plan_that_recorded_no_stamps_is_not_treated_as_a_changed_disc() {
        let (mut plan, dir) =
            fixtures::planned_with(&["workbench-base"], &["Workbench3.2"], Some(47));
        plan.media_stamps.clear();

        // Change the medium too, so the only thing letting this through is
        // the absent stamp rather than the file happening to match.
        let medium = plan.media_paths.get("Workbench3.2").unwrap().clone();
        let mut bytes = std::fs::read(&medium).unwrap();
        bytes.extend_from_slice(&[0u8; 512]);
        std::fs::write(&medium, &bytes).unwrap();

        let root = dir.join("dist");
        assert!(
            apply(&plan, &root, &NoProgress).is_ok(),
            "an old plan must not be refused for a check it never recorded"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- Activation: what the media leaves on the shelf, switched on ----
    //
    // AmigaOS reads `Devs/DOSDrivers` and `Devs/Monitors`, never `Storage/`.
    // Every tree ART built before 2026-08-23 had its drivers on the shelf and
    // none of them switched on, so a finished card had no CD drive and
    // exactly one screen mode. Found by reading `jit06/emu68-bootstrap`,
    // whose `library.sh` exists for this and nothing else.

    fn activation(
        component: &str,
        from: &str,
        to: &str,
    ) -> crate::core::osinstall::plan::PlannedActivation {
        crate::core::osinstall::plan::PlannedActivation {
            component: component.to_string(),
            name: to.rsplit('/').next().unwrap().to_string(),
            from: from.to_string(),
            to: to.to_string(),
        }
    }

    /// A tree with something on the shelf, so `switch_on` has a real file to
    /// move rather than a fixture's idea of one.
    fn tree_with_a_shelf(tag: &str) -> (PathBuf, PathBuf) {
        let dir = fixtures::scratch(&format!("activation-{tag}"));
        let root = dir.join("dist");
        std::fs::create_dir_all(root.join("Storage").join("Monitors")).unwrap();
        std::fs::create_dir_all(root.join("Tools").join("Commodities")).unwrap();
        std::fs::write(root.join("Storage/Monitors/NTSC"), b"monitor").unwrap();
        std::fs::write(root.join("Tools/Commodities/Blanker"), b"commodity").unwrap();
        std::fs::write(root.join("Tools/Commodities/Blanker.info"), b"icon").unwrap();
        (dir, root)
    }

    /// **The switch is thrown, into the drawer AmigaOS actually reads.**
    ///
    /// Asserted against the filesystem: the whole point is that a file exists
    /// where the operating system will look, and a check against the plan
    /// could not see that.
    #[test]
    fn an_activation_copies_the_file_where_amigados_will_look_for_it() {
        let (dir, root) = tree_with_a_shelf("monitor");
        let mut outcome = ApplyOutcome {
            root: root.clone(),
            files: 0,
            directories: 0,
            bytes: 0,
        };
        let mut files = Vec::new();

        switch_on(
            &root,
            &[activation(
                "storage",
                "Storage/Monitors/NTSC",
                "Devs/Monitors/NTSC",
            )],
            &mut outcome,
            &mut files,
            0,
            &NoProgress,
        )
        .unwrap();

        assert!(
            root.join("Storage/Monitors/NTSC").is_file(),
            "the shelf copy stays where it was"
        );
        assert!(
            root.join("Devs/Monitors/NTSC").is_file(),
            "nothing landed in Devs/Monitors — the tree is what it always was"
        );
        assert_eq!(
            std::fs::read(root.join("Devs/Monitors/NTSC")).unwrap(),
            b"monitor",
            "and it is the file, not an empty one"
        );

        assert_eq!(outcome.files, 1, "the tree gained exactly one file");
        assert_eq!(outcome.bytes, 7);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "Devs/Monitors/NTSC");
        assert_eq!(
            files[0].component, "storage",
            "credited to the component that asked, not the medium"
        );
        assert_eq!(
            files[0].media, "",
            "it came from inside the tree, not from a medium"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **The icon travels with it, and for a commodity that is not a
    /// nicety**: AmigaOS starts what the `WBStartup` icon says, not what the
    /// file is, so a commodity copied without its `.info` is a switch that
    /// looks thrown and is not.
    #[test]
    fn a_commodity_takes_its_icon_with_it() {
        let (dir, root) = tree_with_a_shelf("commodity");
        let mut outcome = ApplyOutcome {
            root: root.clone(),
            files: 0,
            directories: 0,
            bytes: 0,
        };
        let mut files = Vec::new();

        switch_on(
            &root,
            &[activation(
                "workbench-base",
                "Tools/Commodities/Blanker",
                "WBStartup/Blanker",
            )],
            &mut outcome,
            &mut files,
            0,
            &NoProgress,
        )
        .unwrap();

        assert!(root.join("WBStartup/Blanker").is_file());
        assert!(
            root.join("WBStartup/Blanker.info").is_file(),
            "without the icon, Workbench never runs it"
        );
        assert_eq!(outcome.files, 2, "the file and its icon");
        assert_eq!(
            files.iter().filter(|f| f.path.ends_with(".info")).count(),
            1,
            "and the manifest accounts for the icon too: {files:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A driver with no icon is merely untidy, so the missing `.info` is
    /// skipped rather than failing the whole install. The file itself is not
    /// optional — `plan` promised it would be there.
    #[test]
    fn a_missing_icon_is_skipped_and_not_an_error() {
        let (dir, root) = tree_with_a_shelf("no-icon");
        let mut outcome = ApplyOutcome {
            root: root.clone(),
            files: 0,
            directories: 0,
            bytes: 0,
        };
        let mut files = Vec::new();

        switch_on(
            &root,
            &[activation(
                "storage",
                "Storage/Monitors/NTSC",
                "Devs/Monitors/NTSC",
            )],
            &mut outcome,
            &mut files,
            0,
            &NoProgress,
        )
        .unwrap();

        assert!(root.join("Devs/Monitors/NTSC").is_file());
        assert!(!root.join("Devs/Monitors/NTSC.info").exists());
        assert_eq!(outcome.files, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Switching the same thing on twice is one file, not two records. A
    /// second run over a tree that already has it must not make the manifest
    /// claim the tree gained something it did not (ART-124's rule, applied
    /// here).
    #[test]
    fn switching_the_same_thing_on_twice_is_still_one_file() {
        let (dir, root) = tree_with_a_shelf("twice");
        let mut outcome = ApplyOutcome {
            root: root.clone(),
            files: 0,
            directories: 0,
            bytes: 0,
        };
        let mut files = Vec::new();
        let switches = [activation(
            "storage",
            "Storage/Monitors/NTSC",
            "Devs/Monitors/NTSC",
        )];

        switch_on(&root, &switches, &mut outcome, &mut files, 0, &NoProgress).unwrap();
        switch_on(&root, &switches, &mut outcome, &mut files, 0, &NoProgress).unwrap();

        assert_eq!(outcome.files, 1, "one file on disk, one in the count");
        assert_eq!(outcome.bytes, 7, "and its bytes counted once");
        assert_eq!(files.len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **ART-124 — the report has to describe the tree, not the work.**
    ///
    /// `outcome.files` and the manifest both counted *plan items*, and an
    /// override writes one destination twice by design (that is what an
    /// override is — ART-112 was a missing one). So a real 3.2 install
    /// announced 4047 files where 3950 existed, and `distribution.json` —
    /// whose own doc comment calls it "the only record… because the media
    /// itself is gone by then" — carried 94 paths twice, each claiming a
    /// different component put it there. One of those two claims was always
    /// false.
    ///
    /// Asserted against the **filesystem**, not against a number worked out
    /// by hand: a count derived from `plan.items` is exactly the count that
    /// was wrong, so it cannot be the thing that checks it.
    #[test]
    fn the_report_counts_what_the_tree_holds_not_what_the_plan_did() {
        // `classes` overrides `workbench-base` — the double-count — and
        // `backdrops` writes into `Prefs/Presets/Backdrops`, whose middle
        // directory no rule names: the two halves of ART-124 in one plan.
        let (plan, dir) = fixtures::planned_with(
            &["classes", "backdrops"],
            &["Workbench3.2", "Classes3.2", "Install3.2", "Backdrops3.2"],
            Some(47),
        );
        assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
        assert!(
            plan.items
                .iter()
                .any(|i| i.to.starts_with("Prefs/Presets/"))
                && !plan.items.iter().any(|i| i.to == "Prefs/Presets"),
            "this test needs a destination whose parent no rule names"
        );

        // The fixture has to actually contain the shape this is about.
        let mut claimed: std::collections::BTreeMap<&str, usize> =
            std::collections::BTreeMap::new();
        for item in plan.items.iter().filter(|i| !i.is_dir) {
            *claimed.entry(item.to.as_str()).or_default() += 1;
        }
        let overridden = claimed.values().filter(|n| **n > 1).count();
        assert!(
            overridden > 0,
            "this test needs a plan where one destination is written twice; \
             `classes` overrides `workbench-base` in the shipped recipe"
        );

        let root = dir.join("dist");
        let outcome = apply(&plan, &root, &NoProgress).unwrap();

        // What is actually there, counted the way the tree is counted: a
        // `.uaem` sidecar is metadata beside an entry, and the manifest is
        // the report itself, not one of the files it reports.
        fn walk(dir: &Path, files: &mut u64, dirs: &mut u64) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    *dirs += 1;
                    walk(&path, files, dirs);
                } else if !path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("uaem"))
                    && path.file_name().is_some_and(|n| n != MANIFEST_FILE_NAME)
                {
                    *files += 1;
                }
            }
        }
        let (mut on_disk_files, mut on_disk_dirs) = (0, 0);
        walk(&root, &mut on_disk_files, &mut on_disk_dirs);

        assert_eq!(
            outcome.files, on_disk_files,
            "the report claims {} files; the tree holds {on_disk_files}",
            outcome.files
        );
        assert_eq!(outcome.directories, on_disk_dirs);
        assert_eq!(
            outcome.files,
            claimed.len() as u64,
            "one count per destination, not per item"
        );

        let manifest: DistributionManifest =
            serde_json::from_str(&std::fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap())
                .unwrap();
        assert_eq!(manifest.files.len() as u64, outcome.files);
        let mut paths: Vec<&str> = manifest.files.iter().map(|f| f.path.as_str()).collect();
        paths.sort_unstable();
        let before = paths.len();
        paths.dedup();
        assert_eq!(
            paths.len(),
            before,
            "the manifest may name a path once: two records for one file means \
             one of them credits a component that did not write what is there"
        );

        // And the record kept is the one that won — the override, which is
        // the component that wrote last.
        let winner = manifest
            .files
            .iter()
            .find(|f| claimed.get(f.path.as_str()).is_some_and(|n| *n > 1))
            .expect("an overridden path is in the manifest");
        assert_eq!(
            winner.component, "classes",
            "the surviving record must credit the component whose bytes are on \
             disk, not the one it overrode"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **ART-205 — the plan predicts the tree, not the reading.**
    ///
    /// The byte half of [`the_report_counts_what_the_tree_holds_not_what_the_plan_did`]
    /// above. `outcome.files` was taught to count destinations rather than
    /// plan items by ART-124; `plan.total_bytes` went on summing every
    /// non-directory item, so a destination two components write — which is
    /// exactly what an `overrides` relationship *is* — was counted twice in
    /// the prediction and once on disk. Measured on the owner's own AmigaOS
    /// 3.9 disc: 17,579,966 predicted against 14,883,492 written, 18% over
    /// and systematic.
    ///
    /// Asserted against the **filesystem**, for ART-124's own reason: a
    /// number derived from `plan.items` is the number that was wrong, so it
    /// cannot be the thing that checks it.
    #[test]
    fn the_plan_predicts_the_bytes_the_tree_will_hold_not_the_bytes_it_reads() {
        // `classes` overrides `workbench-base` in the shipped 3.2 recipe —
        // the same fixture the file-count half uses.
        let (plan, dir) = fixtures::planned_with(
            &["classes", "backdrops"],
            &["Workbench3.2", "Classes3.2", "Install3.2", "Backdrops3.2"],
            Some(47),
        );
        assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);

        // The fixture has to actually contain the shape this is about,
        // otherwise the test is inert whatever the arithmetic does.
        let mut claimed: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        for item in plan.items.iter().filter(|i| !i.is_dir) {
            *claimed
                .entry(crate::core::osinstall::destination_key(&item.to))
                .or_default() += 1;
        }
        let overridden = claimed.values().filter(|n| **n > 1).count();
        assert!(
            overridden > 0,
            "this test needs a plan where one destination is written twice"
        );

        let root = dir.join("dist");
        let outcome = apply(&plan, &root, &NoProgress).unwrap();

        // What the tree really holds: every file's real length off the
        // filesystem, the sidecars and the manifest excluded the same way
        // the file count excludes them.
        fn weigh(dir: &Path, bytes: &mut u64, files: &mut u64) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    weigh(&path, bytes, files);
                } else if !path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("uaem"))
                    && path.file_name().is_some_and(|n| n != MANIFEST_FILE_NAME)
                {
                    *bytes += std::fs::metadata(&path).unwrap().len();
                    *files += 1;
                }
            }
        }
        let (mut on_disk_bytes, mut on_disk_files) = (0, 0);
        weigh(&root, &mut on_disk_bytes, &mut on_disk_files);

        assert_eq!(
            outcome.bytes, on_disk_bytes,
            "the report already describes the tree (ART-124); this is the baseline"
        );
        assert_eq!(
            plan.total_bytes, on_disk_bytes,
            "the plan predicted {} bytes; the tree holds {on_disk_bytes} \
             ({overridden} destination(s) written twice)",
            plan.total_bytes
        );
        assert_eq!(
            plan.total_files, on_disk_files,
            "the plan predicted {} files; the tree holds {on_disk_files}",
            plan.total_files
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The coordinator's review, Critical 1, closed end to end. Before this
    /// fix, exclusion was applied by editing the `InstallPlan` *object* on
    /// the frontend — which left `plan.media_paths` untouched, so a
    /// component the user excluded was still opened by `apply` and still
    /// recorded in `distribution.json`'s `built_from`, a manifest whose own
    /// doc comment calls it "the only record… because the media itself is
    /// gone by then". Now `InstallRequest::excluded` is subtracted inside
    /// `resolve_components_on`, so `plan()` itself never adds the excluded
    /// component's media to `media_paths` in the first place.
    ///
    /// Proved here through the real `plan()` → `apply()` seam, not by
    /// inspecting the plan alone: `ModulesA1200_3.2`'s media is built as a
    /// normal, valid disk and then **deleted from disk** before `apply`
    /// runs. Under the old, client-filtered shape this would have failed
    /// `apply` outright — reading a file that just moved is exactly the
    /// review's "a disk that moved between plan and build fails a job over
    /// a component the user turned off". Succeeding here proves `apply`
    /// never reached for it at all.
    #[test]
    fn excluding_a_component_means_apply_never_opens_or_records_its_media() {
        let dir = fixtures::scratch("apply-excluded-media-untouched");
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();

        let recipe = crate::core::osinstall::recipe::amigaos_32().unwrap();
        let wb = fixtures::entries_for(&recipe, "Workbench3.2");
        let wb_refs: Vec<(&str, &[u8], u32)> = wb
            .iter()
            .map(|(path, bytes, protection)| (path.as_str(), bytes.as_slice(), *protection))
            .collect();
        fixtures::media(&folder, "Workbench3.2", "wb.adf", &wb_refs);
        fixtures::required_media(&folder, &recipe, &["Workbench3.2"]);

        let modules = fixtures::entries_for(&recipe, "ModulesA1200_3.2");
        let modules_refs: Vec<(&str, &[u8], u32)> = modules
            .iter()
            .map(|(path, bytes, protection)| (path.as_str(), bytes.as_slice(), *protection))
            .collect();
        let modules_path =
            fixtures::media(&folder, "ModulesA1200_3.2", "modules.adf", &modules_refs);

        let request = crate::core::osinstall::plan::InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder,
            rom: Some(fixtures::fake_rom(&dir, 40)), // pre-V47: the condition holds
            chosen: vec!["workbench-base".to_string()],
            excluded: vec!["modules-a1200".to_string()],
            destination: dir.join("dist"),
            scan_cache: Default::default(),
        };
        let built_plan = crate::core::osinstall::plan::plan(&request, &recipe).unwrap();
        assert!(built_plan.refusals.is_empty(), "{:?}", built_plan.refusals);
        assert!(!built_plan
            .components_on
            .iter()
            .any(|c| c == "modules-a1200"));
        assert!(
            !built_plan.media_paths.contains_key("ModulesA1200_3.2"),
            "the excluded component's media must never enter media_paths: {:?}",
            built_plan.media_paths
        );

        std::fs::remove_file(&modules_path).unwrap();

        let root = dir.join("dist");
        let outcome = apply(&built_plan, &root, &NoProgress).unwrap();
        assert!(outcome.files > 0);

        let manifest: DistributionManifest =
            serde_json::from_str(&std::fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap())
                .unwrap();
        assert!(
            !manifest
                .built_from
                .iter()
                .any(|m| m.volume_name == "ModulesA1200_3.2"),
            "the excluded component's media must not be recorded as something \
             this tree was built from: {:?}",
            manifest.built_from
        );
    }

    /// Requirement 5's failure arriving through a different door: a plan
    /// `plan()` itself refused (empty `items`/`media_paths`, per its own
    /// module doc) must not be silently "built" into an empty tree with a
    /// manifest that claims completeness. `extras`'s media is deliberately
    /// absent, so `plan()` returns a real `MediaMissing` refusal.
    #[test]
    fn a_plan_with_refusals_is_refused_not_silently_built_empty() {
        let (plan, dir) = fixtures::planned_with(&["extras"], &["Workbench3.2"], Some(47));
        assert!(
            !plan.refusals.is_empty(),
            "sanity: this plan should have refused (extras's media is absent)"
        );
        let root = dir.join("dist");

        assert!(apply(&plan, &root, &NoProgress).is_err());
        assert!(
            !root.exists(),
            "nothing is built from a plan that never resolved"
        );
    }

    /// The negative half of "only when there is something worth recording":
    /// `fixtures::media` always stamps a file with the current wall-clock
    /// time (`write_entries` passes `date: None`, and `add_file` falls back
    /// to `amiga_now()`), so every file built through it carries a non-default
    /// date and always gets a sidecar — the test above never actually
    /// exercises the "nothing to record" branch of `sidecar_for`. This test
    /// builds a file the same way `source.rs`'s own
    /// `an_entry_carries_its_size_date_and_comment` does — straight through
    /// `VolumeWriter`, so the date can be pinned to `AmigaDate::default()`
    /// deliberately — with default protection, no comment and the Amiga
    /// epoch itself as its date, so there is genuinely nothing worth a
    /// `.uaem` for.
    #[test]
    fn a_file_with_nothing_worth_recording_gets_no_sidecar() {
        use crate::core::adf::bcpl::AmigaDate;
        use crate::core::volume::device::FileRegionMut;
        use crate::core::volume::write::{FileMeta, VolumeWriter};
        use crate::core::volume::{DosType, VolumeGeometry};

        let dir = fixtures::scratch("apply-no-sidecar");
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();
        let image = fixtures::media(&folder, "Plain", "plain.adf", &[]);
        let geometry = VolumeGeometry::floppy_dd(DosType::new(*b"DOS\x01"));
        {
            let mut device =
                FileRegionMut::open(&image, 0, geometry.total_bytes(), geometry.block_size)
                    .unwrap();
            let mut writer = VolumeWriter::open(&mut device, geometry, &image, 0).unwrap();
            writer
                .add_file(
                    0,
                    "Plain",
                    b"nothing special",
                    FileMeta {
                        protection: Some(0),
                        date: Some(AmigaDate::default()),
                    },
                )
                .unwrap();
        }

        let mut media_paths = BTreeMap::new();
        media_paths.insert("Plain".to_string(), image);
        let items = vec![PlanItem {
            component: "a".into(),
            media: "Plain".into(),
            from: "Plain".into(),
            to: "Plain".into(),
            is_dir: false,
            bytes: 16,
            decompress: false,
        }];
        let plan = InstallPlan {
            release: "Test".into(),
            items,
            refusals: Vec::new(),
            packages: Vec::new(),
            package_media: BTreeMap::new(),
            total_bytes: 16,
            total_files: 1,
            components_on: vec!["a".into()],
            paired_rom: None,
            media_paths,
            user_startup: Vec::new(),
            activations: Vec::new(),
            media_stamps: BTreeMap::new(),
        };

        let root = dir.join("dist");
        apply(&plan, &root, &NoProgress).unwrap();

        assert!(root.join("Plain").exists());
        assert!(
            !root.join("Plain.uaem").exists(),
            "default protection, no comment, and the Amiga epoch itself as the \
             date is nothing a sidecar needs to preserve"
        );
    }

    /// The mismatch itself: `PlanItem::bytes` is wrong on purpose, and the
    /// manifest must record what was actually read off the media, not the
    /// plan's stale guess — the same falsification `core/layout/apply.rs`'s
    /// own `a_file_lands_at_its_destination_and_the_source_is_untouched`
    /// uses for the identical question.
    #[test]
    fn the_manifest_records_the_real_size_written_not_the_plans_estimate() {
        let (mut plan, dir) = planned();
        plan.items[0].bytes = 999; // b"cmd" is really 3 bytes.
        let root = dir.join("dist");
        apply(&plan, &root, &NoProgress).unwrap();

        let manifest: DistributionManifest =
            serde_json::from_str(&std::fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap())
                .unwrap();
        let record = manifest
            .files
            .iter()
            .find(|f| f.path == "C/LoadModule")
            .unwrap();
        assert_eq!(record.bytes, 3, "the real size read, not the plan's 999");
    }

    /// `SAFE_CREATE`, and what it is actually for: **a distribution folder
    /// with something in it is somebody's work.**
    ///
    /// Strengthened past the brief's own `is_err()` — what was already there
    /// must come back out byte for byte. A version that only checked the
    /// return type could pass while happily writing into the folder first and
    /// failing later.
    #[test]
    fn an_existing_destination_with_anything_in_it_is_refused_never_written_into() {
        let (plan, dir) = planned();
        let root = dir.join("dist");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("someone-elses-work.txt"), b"do not touch").unwrap();

        assert!(apply(&plan, &root, &NoProgress).is_err());

        assert_eq!(
            std::fs::read_dir(&root).unwrap().count(),
            1,
            "nothing was written into the folder that was already there"
        );
        assert_eq!(
            std::fs::read(root.join("someone-elses-work.txt")).unwrap(),
            b"do not touch",
            "and what was there is byte for byte what it was"
        );
    }

    /// **ART-203.** An existing folder that is **empty** is accepted.
    ///
    /// The rule is that ART never builds over somebody's data, and an empty
    /// directory holds none. Refusing it made the screen unusable: the folder
    /// picker can only return a folder that *exists* — its "New folder" button
    /// creates one, and it exists from that moment — so every destination a
    /// user could choose was refused, and no tree was ever built from the
    /// screen at all.
    #[test]
    fn an_existing_empty_destination_is_accepted() {
        let (plan, dir) = planned();
        let root = dir.join("dist");
        std::fs::create_dir_all(&root).unwrap();

        apply(&plan, &root, &NoProgress).expect("an empty folder holds no data to protect");
        assert!(
            root.join(MANIFEST_FILE_NAME).is_file(),
            "and the tree really was built into it"
        );
    }

    /// A hidden file is still a file. "Empty" means the directory yields
    /// nothing at all, not "nothing that looks important".
    #[test]
    fn a_destination_holding_only_a_hidden_file_is_still_refused() {
        let (plan, dir) = planned();
        let root = dir.join("dist");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(".hidden"), b"x").unwrap();

        assert!(apply(&plan, &root, &NoProgress).is_err());
        assert!(root.join(".hidden").is_file());
    }

    /// A folder that is not there at all still works, which is the ordinary
    /// case and the one every other test in this file relies on.
    #[test]
    fn a_destination_that_does_not_exist_is_created() {
        let (plan, dir) = planned();
        let root = dir.join("brand-new");
        apply(&plan, &root, &NoProgress).unwrap();
        assert!(root.join(MANIFEST_FILE_NAME).is_file());
    }

    /// A path that exists but is a **file** is refused — building a tree
    /// "into" it is not a thing, and the sentence has to say which of the two
    /// problems it is.
    #[test]
    fn a_destination_that_is_a_file_is_refused() {
        let (plan, dir) = planned();
        let root = dir.join("not-a-folder");
        std::fs::write(&root, b"i am a file").unwrap();

        let err = apply(&plan, &root, &NoProgress).unwrap_err().to_string();
        assert!(
            err.contains("not a folder"),
            "the refusal says which problem it is: {err}"
        );
        assert_eq!(std::fs::read(&root).unwrap(), b"i am a file");
    }

    /// The rule G11 proved by measurement: removing `safe_join` genuinely
    /// wrote outside the staging root.
    #[test]
    fn a_destination_that_climbs_out_of_the_root_is_refused() {
        let (mut plan, dir) = planned();
        plan.items[0].to = "../escaped".into();
        let root = dir.join("dist");
        assert!(apply(&plan, &root, &NoProgress).is_err());
        assert!(!dir.join("escaped").exists());
    }

    #[test]
    fn the_media_is_byte_for_byte_unchanged_afterwards() {
        let (plan, dir) = planned();
        let before = fixtures::digest_of_folder(&media_folder(&dir));
        apply(&plan, &dir.join("dist"), &NoProgress).unwrap();
        assert_eq!(fixtures::digest_of_folder(&media_folder(&dir)), before);
    }

    /// Cancelling after the first file has landed gets `CancelledPartway`
    /// with the real count — not just a different-shaped error, which is all
    /// the brief's own version of this test checked (`matches!(err,
    /// CoreError::Cancelled)` alone cannot fail if `apply` always returns
    /// plain `Cancelled`, so it never actually proved anything "says how
    /// many landed"). This pins the count and proves the file really is on
    /// disk.
    #[test]
    fn a_cancelled_apply_stops_between_files_and_says_how_many_landed() {
        let (plan, dir) = planned();
        let root = dir.join("dist");
        let sink = fixtures::CancelAfter::new(1);

        let err = apply(&plan, &root, &sink).unwrap_err();

        assert!(
            matches!(err, CoreError::CancelledPartway { files: 1 }),
            "{err:?}"
        );
        assert!(root.join("C").join("LoadModule").exists());
        assert!(
            !root.join("C").join("Other").exists(),
            "the second item was never begun"
        );
    }

    /// The other half of the same decision: cancelled before any file has
    /// landed reports the plain `Cancelled` `core/layout/apply.rs` itself
    /// falls back to — there is no count worth a sentence about.
    #[test]
    fn a_cancellation_before_any_file_lands_reports_plain_cancelled() {
        let (plan, dir) = planned();
        let root = dir.join("dist");
        let sink = fixtures::CancelAfter::new(0);

        let err = apply(&plan, &root, &sink).unwrap_err();

        assert!(matches!(err, CoreError::Cancelled), "{err:?}");
    }

    /// The manifest-written-last ordering, proved directly rather than
    /// assumed from the code's own shape: stopping partway must never leave
    /// a `distribution.json` behind, cancelled or not — a manifest is a
    /// claim of completeness, and a half-built tree must never make it.
    #[test]
    fn a_cancelled_run_leaves_no_manifest_behind() {
        let (plan, dir) = planned();
        let root = dir.join("dist");
        let sink = fixtures::CancelAfter::new(1);

        assert!(apply(&plan, &root, &sink).is_err());
        assert!(
            !root.join(MANIFEST_FILE_NAME).exists(),
            "a run that was stopped partway must not claim a complete tree"
        );
    }

    // ---- Task 7: `S:User-Startup` ----

    /// `planned()`'s own plan never touches `S` at all — its two items are
    /// `C/LoadModule` and `C/Other` — so this is also the "no `S` drawer"
    /// case (decision 3): the drawer has to be created from nothing.
    #[test]
    fn a_contribution_composes_the_file_and_creates_a_missing_s_drawer() {
        let (mut plan, dir) = planned();
        plan.user_startup = vec![UserStartupContribution {
            component: "modules-a1200".into(),
            lines: vec!["Assign Foo: SYS:".into()],
        }];
        let root = dir.join("dist");

        let outcome = apply(&plan, &root, &NoProgress).unwrap();

        let content = std::fs::read_to_string(root.join("S").join("User-Startup")).unwrap();
        assert_eq!(
            content,
            ";BEGIN modules-a1200\nAssign Foo: SYS:\n;END modules-a1200\n"
        );

        // A real file that was not one of `plan.items` — `ApplyOutcome`
        // must count it too, not just the copied ones.
        assert_eq!(outcome.files, 3, "2 copied items + the composed file");
        assert_eq!(
            outcome.bytes,
            3 + 4 + content.len() as u64,
            "the composed file's real size, added once"
        );
    }

    /// The manifest side of the same run: one record, naming the
    /// contributing component, with `media` empty because nothing here came
    /// off a disk image, and the real sha256/bytes of the composed file —
    /// not the plan's `bytes` estimate, which does not exist for this file
    /// at all (it is not a `PlanItem`).
    #[test]
    fn the_manifest_names_the_contributing_component_with_no_media() {
        let (mut plan, dir) = planned();
        plan.user_startup = vec![UserStartupContribution {
            component: "modules-a1200".into(),
            lines: vec!["Assign Foo: SYS:".into()],
        }];
        let root = dir.join("dist");
        apply(&plan, &root, &NoProgress).unwrap();

        let content = std::fs::read_to_string(root.join("S").join("User-Startup")).unwrap();
        let manifest: DistributionManifest =
            serde_json::from_str(&std::fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap())
                .unwrap();
        let records: Vec<_> = manifest
            .files
            .iter()
            .filter(|f| f.path == "S/User-Startup")
            .collect();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].component, "modules-a1200");
        assert_eq!(records[0].media, "");
        assert_eq!(records[0].sha256, sha256_bytes(content.as_bytes()));
        assert_eq!(records[0].bytes, content.len() as u64);
    }

    /// Two contributing components: both blocks land, in the order
    /// `plan.user_startup` carries them (recipe order — `plan.rs`'s own
    /// concern, not this one's), and the manifest carries **two** records
    /// for the one path, both sharing the one file's real content — the
    /// property the module doc comment's "S:User-Startup" section commits
    /// to. Falsification: a version that pushed only the *last*
    /// contribution's record (the "obviously unconsidered" shape the brief
    /// warned about) would still pass every single-contribution test above,
    /// since they only ever supply one. This is the one test that shape
    /// fails.
    #[test]
    fn two_contributions_both_land_and_both_get_a_manifest_record() {
        let (mut plan, dir) = planned();
        plan.user_startup = vec![
            UserStartupContribution {
                component: "modules-a1200".into(),
                lines: vec!["Assign Alpha: SYS:".into()],
            },
            UserStartupContribution {
                component: "storage".into(),
                lines: vec!["Assign Beta: SYS:".into()],
            },
        ];
        let root = dir.join("dist");
        apply(&plan, &root, &NoProgress).unwrap();

        let content = std::fs::read_to_string(root.join("S").join("User-Startup")).unwrap();
        assert!(content.contains(";BEGIN modules-a1200\nAssign Alpha: SYS:\n;END modules-a1200\n"));
        assert!(content.contains(";BEGIN storage\nAssign Beta: SYS:\n;END storage\n"));

        // Review item 3: order is claimed everywhere (the field doc,
        // `plan.rs`'s comment, `apply`'s own fold) and, until now, tested
        // nowhere. `modules-a1200` is first in `plan.user_startup`, so its
        // block must land at a lower byte offset than `storage`'s — these
        // are `Assign` lines, and which one runs first changes what the
        // Amiga does.
        let modules_offset = content.find(";BEGIN modules-a1200").unwrap();
        let storage_offset = content.find(";BEGIN storage").unwrap();
        assert!(
            modules_offset < storage_offset,
            "modules-a1200 is first in plan.user_startup and must land first \
             in the file: modules at {modules_offset}, storage at {storage_offset}"
        );

        let manifest: DistributionManifest =
            serde_json::from_str(&std::fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap())
                .unwrap();
        let records: Vec<_> = manifest
            .files
            .iter()
            .filter(|f| f.path == "S/User-Startup")
            .collect();
        assert_eq!(records.len(), 2);
        let components: Vec<&str> = records.iter().map(|r| r.component.as_str()).collect();
        assert!(components.contains(&"modules-a1200"));
        assert!(components.contains(&"storage"));
        assert!(
            records.iter().all(|r| r.sha256 == records[0].sha256),
            "one real file has one real sha256, whichever component's record you read"
        );
    }

    /// A `Subtree "S"` rule can genuinely copy a real `S/User-Startup` off
    /// the release media before this step ever runs — the release's own
    /// starter file. That copy must be read back and edited in place, never
    /// treated as absent (which would silently discard it) or left as a
    /// stale manifest record with the pre-merge sha256/bytes.
    #[test]
    fn a_media_provided_starter_file_is_merged_into_not_discarded() {
        let (mut plan, dir) = planned();
        let folder = media_folder(&dir);
        let starter = b"; the release's own starter file\n".as_slice();
        fixtures::media(
            &folder,
            "Workbench3.2",
            "wb.adf",
            &[("S/User-Startup", starter, 0)],
        );
        plan.media_paths
            .insert("Workbench3.2".to_string(), folder.join("wb.adf"));
        plan.items.push(PlanItem {
            component: "workbench-base".into(),
            media: "Workbench3.2".into(),
            from: "S/User-Startup".into(),
            to: "S/User-Startup".into(),
            is_dir: false,
            bytes: starter.len() as u64,
            decompress: false,
        });
        plan.user_startup = vec![UserStartupContribution {
            component: "amissl".into(),
            lines: vec!["Assign AmiSSL: SYS:Libs/AmiSSL".into()],
        }];
        let root = dir.join("dist");

        let outcome = apply(&plan, &root, &NoProgress).unwrap();

        let content = std::fs::read_to_string(root.join("S").join("User-Startup")).unwrap();
        assert!(
            content.starts_with(std::str::from_utf8(starter).unwrap()),
            "the release's own starter text comes first and unchanged"
        );
        assert!(content.contains(";BEGIN amissl\nAssign AmiSSL: SYS:Libs/AmiSSL\n;END amissl\n"));

        let manifest: DistributionManifest =
            serde_json::from_str(&std::fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap())
                .unwrap();
        let records: Vec<_> = manifest
            .files
            .iter()
            .filter(|f| f.path == "S/User-Startup")
            .collect();
        assert_eq!(
            records.len(),
            1,
            "the workbench-base copy's own record must be gone, replaced by \
             the composed file's — not left behind alongside it"
        );
        assert_eq!(records[0].component, "amissl");
        assert_eq!(records[0].sha256, sha256_bytes(content.as_bytes()));

        // Still one real file on disk, not two — `outcome.files` counts
        // disk reality, and must not double-count the starter copy and the
        // composed result as though they were separate files.
        let copied_bytes: u64 = plan
            .items
            .iter()
            .filter(|i| !i.is_dir && i.to != "S/User-Startup")
            .map(|i| i.bytes)
            .sum();
        assert_eq!(outcome.files, 3, "C/LoadModule, C/Other, S/User-Startup");
        assert_eq!(outcome.bytes, copied_bytes + content.len() as u64);
    }

    /// The cancellation checkpoint this step gets is the same shape every
    /// item in the loop above already gets: checked once, before its one
    /// atomic write. Cancelling exactly here — after both of `planned()`'s
    /// two items have landed, before the composed file is ever written —
    /// must report `CancelledPartway { files: 2 }` and leave neither
    /// `S/User-Startup` nor `distribution.json` behind.
    #[test]
    fn cancelling_at_the_user_startup_step_writes_nothing_and_reports_two_files() {
        let (mut plan, dir) = planned();
        plan.user_startup = vec![UserStartupContribution {
            component: "modules-a1200".into(),
            lines: vec!["Assign Foo: SYS:".into()],
        }];
        let root = dir.join("dist");
        let sink = fixtures::CancelAfter::new(2);

        let err = apply(&plan, &root, &sink).unwrap_err();

        assert!(
            matches!(err, CoreError::CancelledPartway { files: 2 }),
            "{err:?}"
        );
        assert!(root.join("C").join("LoadModule").exists());
        assert!(root.join("C").join("Other").exists());
        assert!(
            !root.join("S").join("User-Startup").exists(),
            "the composed file is one atomic write — cancelling before it \
             starts must leave none of it behind"
        );
        assert!(!root.join(MANIFEST_FILE_NAME).exists());
    }

    /// The negative control for every test above: a plan with no
    /// `user_startup` contributions at all — the shape every shipped
    /// component produces today — must never touch `S` in any way. Without
    /// this, a version of `apply` that always wrote an (empty) `S/User-
    /// Startup` regardless of `plan.user_startup` would still pass every
    /// test above, since none of them check for the file's *absence*.
    #[test]
    fn no_contributions_means_the_file_is_never_touched_at_all() {
        let (plan, dir) = planned();
        assert!(plan.user_startup.is_empty(), "sanity");
        let root = dir.join("dist");

        apply(&plan, &root, &NoProgress).unwrap();

        assert!(!root.join("S").exists());
    }

    /// Review item 5: AmigaDOS text is Latin-1, not UTF-8, and this
    /// project's user is Turkish — an accented byte in a media-provided
    /// starter file is ordinary input, not a hypothetical one. `0xE7` alone
    /// is not valid UTF-8 (it is a lead byte that promises two continuation
    /// bytes that never come), so a `String::from_utf8` read of this exact
    /// byte is the one that used to fail the whole install on its very last
    /// step, after every other file had already landed. Latin-1 decodes it
    /// as `ç` — this must not fail at all.
    #[test]
    fn a_latin1_starter_file_does_not_fail_the_install() {
        let (mut plan, dir) = planned();
        let folder = media_folder(&dir);
        let starter: &[u8] = b"; caf\xE7 comment\n";
        fixtures::media(
            &folder,
            "Workbench3.2",
            "wb.adf",
            &[("S/User-Startup", starter, 0)],
        );
        plan.media_paths
            .insert("Workbench3.2".to_string(), folder.join("wb.adf"));
        plan.items.push(PlanItem {
            component: "workbench-base".into(),
            media: "Workbench3.2".into(),
            from: "S/User-Startup".into(),
            to: "S/User-Startup".into(),
            is_dir: false,
            bytes: starter.len() as u64,
            decompress: false,
        });
        plan.user_startup = vec![UserStartupContribution {
            component: "amissl".into(),
            lines: vec!["Assign AmiSSL: SYS:Libs/AmiSSL".into()],
        }];
        let root = dir.join("dist");

        // Must not fail — that is the whole point of the fix. Before it,
        // this returned `CoreError::Malformed` here.
        let outcome = apply(&plan, &root, &NoProgress).unwrap();

        let disk_bytes = std::fs::read(root.join("S").join("User-Startup")).unwrap();
        assert!(
            disk_bytes.starts_with(starter),
            "the release's own Latin-1 starter text survives byte for byte, \
             including the non-UTF-8 byte"
        );
        assert!(disk_bytes
            .windows(b";BEGIN amissl".len())
            .any(|w| w == b";BEGIN amissl"));

        // The manifest's own bytes/sha256 must describe what is really on
        // disk (Latin-1-encoded bytes), not the UTF-8 length of the Rust
        // `String` used internally to compose it — those two only coincide
        // here because the *new* block content happens to be plain ASCII;
        // the starter's own non-ASCII byte is what would expose a mismatch.
        let manifest: DistributionManifest =
            serde_json::from_str(&std::fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap())
                .unwrap();
        let record = manifest
            .files
            .iter()
            .find(|f| f.path == "S/User-Startup")
            .unwrap();
        assert_eq!(record.bytes, disk_bytes.len() as u64);
        assert_eq!(record.sha256, sha256_bytes(&disk_bytes));

        // `ApplyOutcome::bytes` must also agree with disk reality: the sum
        // of every ordinary copied item plus the composed file's real
        // (Latin-1-encoded) size.
        let copied_bytes: u64 = plan
            .items
            .iter()
            .filter(|i| !i.is_dir && i.to != USER_STARTUP_PATH)
            .map(|i| i.bytes)
            .sum();
        assert_eq!(outcome.bytes, copied_bytes + disk_bytes.len() as u64);
    }

    /// **Task 14 — the real run.** Drives the whole engine — `find_media`
    /// (through `plan()`'s own call), `plan()`, `apply()` — against the
    /// user's own AmigaOS 3.2 media and a real Kickstart ROM, never a
    /// synthetic fixture. Skipped cleanly unless all three environment
    /// variables are set, the same convention `core/preload/native.rs`'s
    /// oracle hooks (`ART_PFS3_WRITE_OUT`, `ART_PFS3_READ_IN`) already use:
    ///
    /// ```text
    /// ART_OSINSTALL_MEDIA="E:\amiga\Amigatolon\paketler\3.2\AmigaOs 3.2\ADF" ^
    /// ART_OSINSTALL_ROM="E:\amiga\Amigatolon\kickstart\Kickstart v3.1 rev 40.68 (1993)(Commodore)(A1200).rom" ^
    /// ART_OSINSTALL_DEST="E:\amiga\ProjeART\dist-3.2" ^
    /// cargo test run_the_real_engine_against_the_users_own_media_when_asked -- --nocapture --ignored
    /// ```
    ///
    /// (`--ignored` because this test is also marked `#[ignore]` below —
    /// belt and braces: even with the env vars unset by accident, a plain
    /// `cargo test` run must never touch anything outside the repo's own
    /// tempdir, which is this project's own standing fixture rule.)
    ///
    /// `chosen` names every component the shipped recipe marks reachable
    /// (`available: true` and no `condition`) except the two the engine
    /// decides for itself: `workbench-base` (`required`, always on) and
    /// `modules-a1200` (`condition: rom-older-than 47`), which is
    /// deliberately left **out** of `chosen` — the point of this run is to
    /// prove the condition switches it on by itself against the user's real
    /// V40 Kickstart, not to force it on the way `chosen` would.
    /// `update-3.2.1` is left out because it is `available: false` in the
    /// recipe (registered, not implemented — see `CLAUDE.md`'s "don't claim
    /// support that isn't implemented and tested"), so a real screen would
    /// never offer it. `backdrops` **is** chosen now: it stopped being a
    /// guess when the running system named its own path (ART-127), and a
    /// tree without it boots to a Preferences error about a missing
    /// wallpaper.
    #[test]
    #[ignore = "touches the user's real media and E:\\amiga\\ProjeART; run explicitly, see the doc comment"]
    fn run_the_real_engine_against_the_users_own_media_when_asked() {
        let (Ok(media), Ok(rom), Ok(dest)) = (
            std::env::var("ART_OSINSTALL_MEDIA"),
            std::env::var("ART_OSINSTALL_ROM"),
            std::env::var("ART_OSINSTALL_DEST"),
        ) else {
            return;
        };

        let chosen: Vec<String> = [
            "extras",
            "locale-base",
            "locale-de",
            "locale-dk",
            "locale-en",
            "locale-es",
            "locale-fr",
            "locale-gr",
            "locale-it",
            "locale-nl",
            "locale-no",
            "locale-pl",
            "locale-pt",
            "locale-ru",
            "locale-se",
            "locale-tr",
            "locale-uk",
            "fonts",
            "classes",
            "glowicons",
            "diskdoctor",
            "mmulibs",
            "hdtools",
            "storage",
            "keymaps",
            "backdrops",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let request = crate::core::osinstall::plan::InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: PathBuf::from(&media),
            rom: Some(PathBuf::from(&rom)),
            chosen,
            excluded: Vec::new(),
            destination: PathBuf::from(&dest),
            scan_cache: Default::default(),
        };

        let found = crate::core::osinstall::scan::find_media(&request.media_folder).unwrap();
        println!("find_media: {} volume(s) found", found.len());
        for entry in &found {
            println!("  {} -> {}", entry.volume_name, entry.path.display());
        }

        let recipe = crate::core::osinstall::recipe::amigaos_32().unwrap();
        let planned = crate::core::osinstall::plan::plan(&request, &recipe).unwrap();

        println!("release={}", planned.release);
        println!("components_on={:?}", planned.components_on);
        println!("refusals={:?}", planned.refusals);
        println!("total_bytes={}", planned.total_bytes);
        println!("items={}", planned.items.len());
        // ART-228: how many of them the medium ships compressed, printed so
        // a run records it rather than a reader having to count the tree.
        println!(
            "compressed items={}",
            planned.items.iter().filter(|i| i.decompress).count()
        );

        let modules_on = planned.components_on.iter().any(|id| id == "modules-a1200");
        println!("modules-a1200 on without being chosen: {modules_on}");

        assert!(
            planned.refusals.is_empty(),
            "the real plan refused: {:?}",
            planned.refusals
        );
        // **Asked of the ROM rather than assumed of it.** The hook was
        // written against the user's 3.1 (V40) dump and asserted
        // `modules-a1200` on, flatly — which made it fail when pointed at
        // their 3.2 (V47) ROM, where the component is correctly *off*. Both
        // ROMs are real material worth running against (the V47 one is what
        // boots the tree in WinUAE), so the assertion now states the rule the
        // condition actually encodes instead of one ROM's answer to it.
        let rom_major = crate::core::rom::stated_version(&std::fs::read(&rom).unwrap())
            .map(|(major, _)| major)
            .expect("the paired Kickstart states its own version");
        println!("rom stated major={rom_major}");
        assert_eq!(
            modules_on,
            rom_major < 47,
            "modules-a1200 must switch on by its own condition — unchosen — \
             exactly when the paired ROM's stated major is below 47, and stay \
             off otherwise. The ROM here states {rom_major}. If this fails, \
             something upstream (ROM read, Cloanto strip, condition_holds) is \
             wrong, which is the finding the brief expects this run to surface"
        );

        // Review fix round 1: these were printed, never asserted, so none of
        // them could regress. The real 36-ADF media set is fixed (it is the
        // user's own material, not expected to change), so the exact counts
        // are asserted rather than left as text a reader has to trust — if
        // this ever fails because the media folder genuinely changed, that
        // is itself worth knowing, not a flaky test to work around.
        // **One set of numbers per ROM, because the ROM changes the plan.**
        // A pre-V47 machine gets `modules-a1200` (LoadModule and the module
        // sets its own condition switched on); a V47 machine correctly does
        // not, and its tree is that much smaller. Pinning one ROM's answer as
        // *the* answer is what made this hook fail the first time it was
        // pointed at the user's 3.2 ROM — the material it was written against
        // was never the only real material.
        // **Both rows moved on 2026-08-24 and each half has its own reason.**
        //
        // The byte totals grew by exactly **15 934** without anything being
        // added, and that number is [ART-224](../../../docs/ISSUES.md)
        // measured on real material rather than off the ADFs. `glowicons`
        // used to be declared *above* `storage`, so the plain shelf icons won
        // the sixteen files the two disks share; declared below it, the
        // GlowIcons ones do. Ten monitor icons at 1 452 bytes against 476 is
        // 9 760, and the six DOSDrivers icons account for the other 6 174.
        // The fix was previously only measured by comparing the two ADFs; the
        // tree now says the same thing.
        //
        // The rest is [ART-226](../../../docs/ISSUES.md)'s `keymaps`
        // component, chosen above: twenty-two keymap files into
        // `Devs/Keymaps`, which until now held twenty-two icons and nothing
        // they pointed at.
        // **The byte totals grew again on 2026-08-24, and by seven megabytes.**
        // ART-228: the medium ships **3 263** of these files `compress`-format
        // `.Z`, and the release's own Installer expands them and drops the
        // suffix. ART does the same now, so the tree holds what a real
        // install holds rather than a drawer of packed files under names
        // nothing reads. That is also why `planned.total_bytes` and
        // `outcome.bytes` stop agreeing here — a `.Z` stream carries no
        // expanded length, so the plan cannot predict it without reading
        // every file, which is a price the live preview must not pay. The
        // numbers below are what `apply` wrote, measured.
        //
        // **`want_media` is not `want_components`, and 2026-08-24 is when
        // that stopped being the same number.** `built_from` carries one
        // record per *medium*; every component had its own disk until
        // ART-226's `keymaps` arrived, which comes off `Storage3.2` — the
        // disk `storage` already reads. One more component, no more media.
        // Asserting one count against both is the kind of conflation that
        // holds until it quietly does not.
        let (want_components, want_media, want_files, want_dirs, want_bytes) = if rom_major < 47 {
            (29, 28, 3976, 281, 19_839_113)
        } else {
            (28, 27, 3972, 278, 19_789_593)
        };
        assert_eq!(
            planned.components_on.len(),
            want_components,
            "components_on={:?}",
            planned.components_on
        );

        let root = PathBuf::from(&dest);
        let outcome = apply(&planned, &root, &NoProgress).unwrap();
        println!(
            "apply: files={} directories={} bytes={}",
            outcome.files, outcome.directories, outcome.bytes
        );
        assert_eq!(outcome.files, want_files);
        assert_eq!(outcome.directories, want_dirs);
        assert_eq!(outcome.bytes, want_bytes);

        let manifest_text = std::fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap();
        let manifest: DistributionManifest = serde_json::from_str(&manifest_text).unwrap();
        println!(
            "manifest: {} file(s) recorded from {} medium/media",
            manifest.files.len(),
            manifest.built_from.len()
        );
        assert_eq!(manifest.files.len(), want_files as usize);
        assert_eq!(manifest.built_from.len(), want_media);
    }

    /// Throwaway diagnostic, not part of the suite's real coverage — lists
    /// what is actually inside three of the user's real ADFs, to diagnose
    /// the refusals `run_the_real_engine_against_the_users_own_media_when_asked`
    /// found on the first real run. Same env-var-gated skip convention.
    #[test]
    #[ignore = "diagnostic only; touches the user's real media"]
    fn inspect_real_media_when_asked() {
        let Ok(media) = std::env::var("ART_OSINSTALL_MEDIA") else {
            return;
        };
        let folder = PathBuf::from(&media);
        let found = crate::core::osinstall::scan::find_media(&folder).unwrap();
        for volume in ["Storage3.2", "Classes3.2", "GlowIcons3.2"] {
            let entry = found.iter().find(|f| f.volume_name == volume).unwrap();
            let mut source = AdfSource::open(&entry.path).unwrap();
            let mut all = source.walk("").unwrap();
            all.sort_by(|a, b| a.path.cmp(&b.path));
            println!("--- {volume} ({}) ---", entry.path.display());
            for e in &all {
                println!("  {}{}", e.path, if e.is_dir { "/" } else { "" });
            }
        }
    }

    /// Throwaway diagnostic (Task 4): lists what the real 3.9 disc actually
    /// holds near its root, to find the real path the shipped recipe's
    /// `OS-Version3.9/Workbench3.5/*` rules got wrong. Same env-var gate as
    /// `build_the_real_39_tree_when_asked`, deleted once its answer is known.
    #[test]
    #[ignore = "diagnostic only; touches the user's real media"]
    fn inspect_real_39_disc_when_asked() {
        let Ok(iso) = std::env::var("ART_OS39_ISO") else {
            return;
        };
        let mut source = CdSource::open(&PathBuf::from(&iso)).unwrap();
        println!("volume_name={}", source.volume_name());
        let mut root = source.walk("").unwrap();
        root.sort_by(|a, b| a.path.cmp(&b.path));
        println!("--- OS-VERSION3.9/WORKBENCH3.9 direct children ---");
        for e in &root {
            if e.path.starts_with("OS-VERSION3.9/WORKBENCH3.9/") && e.path.matches('/').count() == 2
            {
                println!("  {}{}", e.path, if e.is_dir { "/" } else { "" });
            }
        }
        for candidate in [
            "OS-Version3.9",
            "OS-Version3.9/Workbench3.5",
            "Workbench3.5",
            "OS-VERSION3.9/WORKBENCH3.9",
            "OS-VERSION3.9/WORKBENCH3.9/C",
            "OS-VERSION3.9/WORKBENCH3.9/L",
        ] {
            match source.entry(candidate) {
                Ok(Some(e)) => println!("{candidate} -> present, is_dir={}", e.is_dir),
                Ok(None) => println!("{candidate} -> absent"),
                Err(e) => println!("{candidate} -> error: {e}"),
            }
        }

        println!("--- anywhere on the disc named L, REXXC or EXPANSION ---");
        for e in &root {
            let last = e.path.rsplit('/').next().unwrap_or("");
            if e.is_dir && (last == "L" || last == "REXXC" || last == "EXPANSION") {
                println!("  {}", e.path);
            }
        }

        println!("--- OS-VERSION3.9/WORKBENCH3.5 direct children ---");
        for e in &root {
            if e.path.starts_with("OS-VERSION3.9/WORKBENCH3.5/") && e.path.matches('/').count() == 2
            {
                println!("  {}{}", e.path, if e.is_dir { "/" } else { "" });
            }
        }
    }

    /// Throwaway diagnostic (Task 4, fix round 1): `apply()`'s real write
    /// against the disc failed with Windows error 123 (`InvalidFilename`) —
    /// this walks every name `workbench-base`'s rules actually resolve to
    /// under `OS-VERSION3.9/WORKBENCH3.5` and flags anything Windows'
    /// `CreateFile` would refuse: the reserved characters
    /// (`< > : " | ? *`), a trailing dot or space in any segment, or one of
    /// the reserved device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1-9`,
    /// `LPT1-9`), matching or not matching its extension.
    #[test]
    #[ignore = "diagnostic only; touches the user's real media"]
    fn find_windows_illegal_names_when_asked() {
        let Ok(iso) = std::env::var("ART_OS39_ISO") else {
            return;
        };
        let mut source = CdSource::open(&PathBuf::from(&iso)).unwrap();
        let all = source.walk("OS-VERSION3.9/WORKBENCH3.5").unwrap();

        const RESERVED: &[&str] = &[
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ];

        let mut flagged = 0;
        for e in &all {
            for segment in e.path.split('/') {
                let bad_char = segment
                    .chars()
                    .any(|c| "<>:\"|?*".contains(c) || (c as u32) < 32);
                let trailing = segment.ends_with('.') || segment.ends_with(' ');
                let stem = segment.split('.').next().unwrap_or(segment);
                let reserved = RESERVED.iter().any(|r| r.eq_ignore_ascii_case(stem));
                if bad_char || trailing || reserved {
                    flagged += 1;
                    println!(
                        "FLAGGED: {} (segment '{segment}': bad_char={bad_char} trailing={trailing} reserved={reserved})",
                        e.path
                    );
                }
            }
        }
        println!("{flagged} flagged segment(s) out of {} entries", all.len());
    }

    /// Throwaway diagnostic (Task 5, Step 1): the exact bytes behind the
    /// three `?`-bearing `.country` names ART-155 names. `CdSource`/
    /// `IsoImage::list` already decode a name before handing it back, so
    /// this walks the same directory *underneath* that decoding — reading
    /// the raw sector by hand and printing each flagged identifier's own
    /// bytes in hex beside what `decode_iso646` made of them — because
    /// Step 2's charset question can only be answered from the bytes the
    /// disc actually carries, never from ART's own already-lossy `?`.
    #[test]
    #[ignore = "diagnostic only; touches the user's real media"]
    fn read_the_raw_country_name_bytes_when_asked() {
        let Ok(iso) = std::env::var("ART_OS39_ISO") else {
            return;
        };
        let image = crate::core::iso::IsoImage::open(&PathBuf::from(&iso)).unwrap();

        // Descend by name. Every segment down to COUNTRIES-EURO itself is
        // plain ASCII (confirmed by ART-155's own report), so decoding loses
        // nothing before the leaf directory this diagnostic actually cares
        // about — matched case-insensitively because the disc's Primary
        // tree mixes case (`OS-VERSION3.9` beside `STORAGE`) rather than
        // sticking to strict ISO9660 Level 1 uppercase.
        let (mut extent, mut length) = image.root();
        for segment in [
            "OS-VERSION3.9",
            "WORKBENCH3.5",
            "STORAGE",
            "LOCALE",
            "COUNTRIES-EURO",
        ] {
            let entries = image.list(extent, length).unwrap();
            let found = entries
                .iter()
                .find(|e| e.name.eq_ignore_ascii_case(segment))
                .unwrap_or_else(|| {
                    panic!(
                        "{segment} not found; entries here: {:?}",
                        entries.iter().map(|e| &e.name).collect::<Vec<_>>()
                    )
                });
            assert!(found.is_dir, "{segment} is not a directory");
            extent = found.extent;
            length = found.bytes as u32;
        }

        // `extent`/`length` now name COUNTRIES-EURO itself. Read its raw
        // sectors and walk records by hand — the same record shape
        // `directory::parse_directory_extent` walks, but keeping the
        // identifier bytes instead of feeding them through `decode_iso646`,
        // which is exactly the step this diagnostic exists to look behind.
        use std::io::{Read, Seek, SeekFrom};
        let layout = image.layout();
        let sector = crate::core::iso::LOGICAL_SECTOR_SIZE as u64;
        let sectors = (length as u64).div_ceil(sector);
        let mut file = std::fs::File::open(&iso).unwrap();
        let mut buf = vec![0u8; (sectors * sector) as usize];
        file.seek(SeekFrom::Start(layout.data_offset_of(extent).unwrap()))
            .unwrap();
        file.read_exact(&mut buf).unwrap();

        let mut pos = 0usize;
        let mut printed = 0;
        while pos < buf.len() {
            let len = buf[pos] as usize;
            if len == 0 {
                // Padding to the next sector, not a record — same rule
                // `directory::parse_directory_extent` follows.
                pos = pos.div_ceil(2048) * 2048;
                continue;
            }
            let id_len = buf[pos + 32] as usize;
            let id = &buf[pos + 33..pos + 33 + id_len];
            let is_dot_or_dotdot = id_len == 1 && (id[0] == 0x00 || id[0] == 0x01);
            if !is_dot_or_dotdot {
                let decoded = crate::core::iso::descriptor::decode_iso646(id);
                if decoded.contains('?') {
                    let hex: Vec<String> = id.iter().map(|b| format!("{b:02x}")).collect();
                    println!("decoded={decoded:?} raw_hex=[{}]", hex.join(" "));
                    printed += 1;
                }
            }
            pos += len;
        }
        println!("{printed} entries with '?' found under COUNTRIES-EURO");
    }

    /// Throwaway investigation (Task 5, Step 5), kept as the evidence for
    /// **ART-156**: `build_the_real_39_tree_when_asked` wrote 54,094 fewer
    /// bytes than `plan()` predicted even though every one of the 663
    /// planned items became a real file or a real directory and every real
    /// *file*'s size matches its `PlanItem::bytes` exactly (checked below,
    /// per item, against the previous run's output — never deleted, so
    /// `ART_OS39_DEST` still holds it). The 75 directory items are the
    /// difference: `PlanItem::bytes` for a directory sourced from a CD is
    /// `IsoEntry::bytes` — the ISO9660 directory record's own declared
    /// extent length, a real, nonzero, sector-rounded number, not the `0`
    /// an `AdfSource` directory reports — but a directory becomes a plain
    /// host folder with no "content" of its own, so `plan::total_bytes`
    /// (`items.iter().map(|i| i.bytes).sum()`, unconditionally over every
    /// item) counts bytes for 75 directories that `apply()` correctly never
    /// writes anywhere. Not a naming defect and not `decode_iso646` — a
    /// distinct, pre-existing miscount in `core/osinstall/plan.rs` that
    /// this task's fix to ART-155 simply let `apply()` run far enough to
    /// expose for the first time.
    ///
    /// **Fixed 2026-08-20** (`plan::content_bytes`): `total_bytes` now sums
    /// file items only. This diagnostic is kept as the evidence it always
    /// was, and re-running it should now print `sum_dir_bytes=54094` with
    /// `total_bytes` equal to `sum_file_bytes` alone.
    #[test]
    #[ignore = "diagnostic only; touches the user's real media and a prior real run's output"]
    fn find_the_directory_byte_overcount_when_asked() {
        let (Ok(iso), Ok(dest)) = (
            std::env::var("ART_OS39_ISO"),
            std::env::var("ART_OS39_DEST"),
        ) else {
            return;
        };
        let iso_path = PathBuf::from(&iso);
        let media_folder = iso_path.parent().unwrap().to_path_buf();
        let request = crate::core::osinstall::plan::InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            // Inert here — `plan()` takes its `release` label from the
            // `recipe` argument below, never from the request — but naming
            // it accurately keeps this diagnostic honest about which recipe
            // it is actually planning against.
            release: "AmigaOS 3.9".to_string(),
            media_folder,
            rom: None,
            chosen: Vec::new(),
            excluded: Vec::new(),
            destination: PathBuf::from(&dest),
            scan_cache: Default::default(),
        };
        let recipe = crate::core::osinstall::recipe::amigaos_39().unwrap();
        let planned = crate::core::osinstall::plan::plan(&request, &recipe).unwrap();

        let root = PathBuf::from(&dest);
        let mut sum_file_bytes: u64 = 0;
        let mut sum_dir_bytes: u64 = 0;
        let mut mismatches = 0;
        for item in &planned.items {
            if item.is_dir {
                sum_dir_bytes += item.bytes;
                continue;
            }
            sum_file_bytes += item.bytes;
            match std::fs::metadata(root.join(&item.to)) {
                Ok(meta) if meta.len() == item.bytes => {}
                Ok(meta) => {
                    mismatches += 1;
                    println!(
                        "MISMATCH {} planned={} actual={}",
                        item.to,
                        item.bytes,
                        meta.len()
                    );
                }
                Err(err) => {
                    mismatches += 1;
                    println!("MISSING {} ({err})", item.to);
                }
            }
        }
        println!(
            "sum_file_bytes={sum_file_bytes} sum_dir_bytes={sum_dir_bytes} \
             total_bytes={} (= sum_file_bytes + sum_dir_bytes: {}) per-file mismatches={mismatches}",
            planned.total_bytes,
            sum_file_bytes + sum_dir_bytes == planned.total_bytes
        );
    }

    /// **Task 4 (amigaos-39 plan), the real run.** Drives the whole engine —
    /// `find_media` (through `plan()`'s own call), `plan()`, `apply()` —
    /// against the owner's own AmigaOS 3.9 CD image, never a synthetic
    /// fixture. Modelled exactly on
    /// `run_the_real_engine_against_the_users_own_media_when_asked` above:
    /// same env-var-gated skip, same doubled `#[ignore]` belt-and-braces, same
    /// shape of report.
    ///
    /// `ART_OS39_ISO` names the disc image itself; `find_media` is handed its
    /// **parent folder**, not a folder holding only this one file — the
    /// owner's real `iso/` folder also holds `AmigaOS3.2CD(ZaP).iso` and other
    /// discs beside it, on purpose (see the task dispatch), so this run is
    /// what proves the scan is not confused by the neighbour rather than
    /// assuming a tidy folder that does not exist on this machine.
    ///
    /// No ROM is supplied: the shipped 3.9 recipe's one component
    /// (`workbench-base`) is `required` and carries no `Condition`, so a plan
    /// against it needs nothing decided from a Kickstart.
    ///
    /// ```text
    /// cd src-tauri && ART_OS39_ISO="E:\amiga\Amigatolon\iso\AmigaOS39.iso" \
    ///   ART_OS39_DEST="E:\amiga\ProjeART\dist-3.9" \
    ///   cargo test build_the_real_39_tree_when_asked -- --nocapture --ignored
    /// ```
    ///
    /// **Measured against the owner's real 469 MiB disc (Task 4, fix round
    /// 1 — after ART-153 was fixed so `apply()` can open a disc at all):**
    /// `find_media` sees all 4 discs in the shared `iso/` folder and resolves
    /// `AmigaOS3.9` correctly, never confused by the neighbouring
    /// `AmigaOS3.2CD(ZaP)`; `plan()` succeeds clean — 1 component on
    /// (`workbench-base`), 0 refusals, 663 items, 6 108 319 planned bytes.
    ///
    /// At that point `apply()` reached 1,020 files and 71 directories before
    /// hitting **ART-155**: a real disc name it could not write as a literal
    /// Windows path segment — three `Storage/Locale/Countries-Euro/*.country`
    /// files whose accented letters `core/iso/descriptor.rs::decode_iso646`
    /// rendered as `?`, a character Windows refuses in a path.
    ///
    /// **Task 5 fixed ART-155's real cause** (`decode_iso646` now decodes a
    /// high-bit byte as ISO-8859-1 instead of `?` — see that function's own
    /// doc comment for the full case) **and corrected its other named
    /// cause**: a reserved DOS device name, `Storage/DOSDrivers/AUX`, was
    /// measured on this machine (Windows 11 Pro 26200) to write and list
    /// back just fine — it was never what failed. Re-run against the same
    /// disc, `apply()` now completes: 588 files, 75 directories, exactly the
    /// 663 planned items, every file's bytes matching its plan exactly. The
    /// one number that does *not* match is `planned.total_bytes` itself —
    /// 6,108,319 against 6,054,225 actually written — and that turned out to
    /// be a second, distinct, previously-unreachable defect: `total_bytes`
    /// sums `PlanItem::bytes` over directories too, and a CD-sourced
    /// directory's `bytes` is its ISO9660 extent length, not `0`. Filed as
    /// **ART-156** and fixed 2026-08-20, so the assertion below is now a
    /// direct `outcome.bytes == planned.total_bytes` rather than the
    /// file-items-only work-around it was written with.
    #[test]
    #[ignore = "touches the user's real media and E:\\amiga\\ProjeART; run explicitly, see the doc comment"]
    fn build_the_real_39_tree_when_asked() {
        let (Ok(iso), Ok(dest)) = (
            std::env::var("ART_OS39_ISO"),
            std::env::var("ART_OS39_DEST"),
        ) else {
            return;
        };

        let iso_path = PathBuf::from(&iso);
        let media_folder = iso_path
            .parent()
            .expect("ART_OS39_ISO names a file inside some folder")
            .to_path_buf();

        let found = crate::core::osinstall::scan::find_media(&media_folder).unwrap();
        println!(
            "find_media: {} volume(s) found in {}",
            found.len(),
            media_folder.display()
        );
        for entry in &found {
            println!(
                "  {:?} {} -> {}",
                entry.kind,
                entry.volume_name,
                entry.path.display()
            );
        }

        let request = crate::core::osinstall::plan::InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            // Inert here — see the same note on
            // `find_the_directory_byte_overcount_when_asked` above.
            release: "AmigaOS 3.9".to_string(),
            media_folder,
            rom: None,
            chosen: Vec::new(),
            excluded: Vec::new(),
            destination: PathBuf::from(&dest),
            scan_cache: Default::default(),
        };

        let recipe = crate::core::osinstall::recipe::amigaos_39().unwrap();
        let planned = crate::core::osinstall::plan::plan(&request, &recipe).unwrap();

        println!("release={}", planned.release);
        println!("components_on={:?}", planned.components_on);
        println!("refusals={:?}", planned.refusals);
        println!("total_bytes={}", planned.total_bytes);
        println!("items={}", planned.items.len());

        assert!(
            planned.refusals.is_empty(),
            "the real plan refused: {:?}",
            planned.refusals
        );
        // The 3.9 recipe carried one component when this hook was written and
        // carries two since ART-169 added `workbench-39`, the overlay that
        // turns a 3.5 tree into a 3.9 one. The assertion said "exactly one"
        // and so the hook could not pass at all — a real-material check nobody
        // could run is a check that is not there.
        assert_eq!(
            planned.components_on,
            vec!["workbench-base".to_string(), "workbench-39".to_string()],
            "3.9 is the base plus the overlay ART-169 added"
        );

        // ART-153 fixed (fix round 1): `apply()` now opens each medium
        // through `scan::identify`/`scan::open_media`, the same probe
        // `find_media` uses, so this reaches a real write against the disc
        // rather than failing at the first byte.
        let start = std::time::Instant::now();
        let root = PathBuf::from(&dest);
        match apply(&planned, &root, &NoProgress) {
            Ok(outcome) => {
                let elapsed = start.elapsed();
                println!(
                    "apply: files={} directories={} bytes={} in {:.2}s",
                    outcome.files,
                    outcome.directories,
                    outcome.bytes,
                    elapsed.as_secs_f64()
                );
                // **ART-156 is fixed, so this is now a direct comparison.**
                // It used to compare `outcome.bytes` against the sum of
                // *file* items only, because `total_bytes` ran over every
                // item and a CD-sourced directory's `PlanItem::bytes` is its
                // ISO9660 extent length rather than `0` — 6,108,319 predicted
                // against 6,054,225 written, a difference of exactly the 75
                // directory items' 54,094 bytes. `total_bytes` now means what
                // that work-around meant, so the work-around is gone and the
                // plan's own number is what gets checked.
                println!(
                    "plan.total_bytes={} vs apply.outcome.bytes={} (difference={})",
                    planned.total_bytes,
                    outcome.bytes,
                    outcome.bytes as i64 - planned.total_bytes as i64
                );
                assert_eq!(
                    outcome.bytes, planned.total_bytes,
                    "apply() wrote a different byte total than plan() predicted"
                );
                // The file half of the same prediction (ART-205). Directories
                // are deliberately not predicted — `apply` creates the
                // ancestors no rule names — so only these two are compared.
                assert_eq!(
                    outcome.files, planned.total_files,
                    "apply() wrote a different file count than plan() predicted"
                );

                // This disc is real, fixed material (the owner's own AmigaOS
                // 3.9 CD): these exact counts are pinned, not just checked
                // non-zero, the same way
                // `run_the_real_engine_against_the_users_own_media_when_asked`
                // pins its own real media's counts — a change here means the
                // media folder or the recipe genuinely changed, worth
                // knowing rather than a flaky assertion to loosen.
                //
                // **Re-measured 2026-08-22, and they had been stale since
                // ART-169.** 588/75/6,054,225 is the *one*-component 3.9
                // tree; the recipe has carried `workbench-39` over
                // `workbench-base` since then, and ART-206 corrected the
                // components assertion above without re-measuring these. The
                // numbers below are this run's own output against
                // `E:\amiga\Amigatolon\iso\AmigaOS39.iso`, 11.58 s on a
                // release build.
                assert_eq!(outcome.files, 1242, "files written");
                assert_eq!(outcome.directories, 105, "directories written");
                assert_eq!(outcome.bytes, 14_883_492, "file bytes written");

                // Fix round 1, review item 2: the counts and sums above would
                // pass just as well if every file landed empty. Name one
                // real file the tree must hold and check it has plausible
                // content — chosen with a Latin-1 letter in its own name
                // (`Ö`) specifically so this assertion pins ART-155's
                // decode_iso646 fix end to end (the name *and* the bytes
                // behind it), not only the aggregate counts above.
                let osterreich = root.join("Storage/LOCALE/COUNTRIES-EURO/\u{d6}STERREICH.COUNTRY");
                let content = std::fs::read(&osterreich)
                    .unwrap_or_else(|err| panic!("{} should exist: {err}", osterreich.display()));
                assert!(!content.is_empty(), "{} landed empty", osterreich.display());

                let manifest_text = std::fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap();
                let manifest: DistributionManifest = serde_json::from_str(&manifest_text).unwrap();
                println!(
                    "manifest: {} file(s) recorded from {} medium/media",
                    manifest.files.len(),
                    manifest.built_from.len()
                );
                assert_eq!(manifest.files.len(), outcome.files as usize);
                // `built_from` is one record per **medium**, not per
                // component — the two were the same number only while the
                // 3.9 recipe had a single component, and ART-169's overlay
                // reads the same disc, so it is 1 medium and 2 components
                // now. Compared against the plan's own media, which is what
                // the field actually records.
                assert_eq!(manifest.built_from.len(), planned.media_paths.len());
            }
            Err(err) => {
                // ART-153 and ART-155 are both fixed as of this task — a
                // failure reaching here is a *new*, as-yet-unfiled defect,
                // not either of those. `root` is left as `apply()` stopped
                // it (writes commit as they land, never rolled back), so a
                // real, measured partial count is possible even though the
                // run did not finish — matching spec §89 ("report what
                // happened, including what did not work").
                let (files_written, dirs_written) = count_tree(&root);
                println!(
                    "apply failed partway: files_written={files_written} directories_written={dirs_written}"
                );
                panic!(
                    "apply() failed against the real disc: {err}\n\n\
                     Neither ART-153 nor ART-155 (docs/ISSUES.md) — both are fixed as of this \
                     task, and this run's failure is something else. File it as a new ART-NNN \
                     with what actually happened, the same rigour ART-155 itself was filed \
                     with, rather than assuming it is one of the two closed causes above."
                );
            }
        }
    }

    /// **ART-159's two language components, against the disc they were read
    /// off.** Everything the recipe now claims about
    /// `Special-Locale/TÜRKÇE` and `Locale.Euro` was measured by walking the
    /// owner's own `AmigaOS39.iso` outside ART — this is the run that asks
    /// ART the same questions and checks it answers the same way.
    ///
    /// Three things only a real disc can settle, and each is asserted rather
    /// than printed:
    ///
    /// - **A non-ASCII `from` path resolves at all.** The Primary tree spells
    ///   the drawer `TÜRKÇE` (raw bytes `54 DC 52 4B C7 45`), and every
    ///   earlier rule in every shipped recipe is pure ASCII. A recipe path
    ///   that no medium matches is a `MediaPathMissing` refusal, so an empty
    ///   `refusals` here is the proof.
    /// - **Nothing collides.** `special-locale-turkish` declares no
    ///   `overrides`, and `plan::detect_collisions` refuses an undeclared
    ///   claim — so the same empty `refusals`, with all three optional
    ///   components on together, is what says the thirteen ISO-8859-9
    ///   families really do share no name with the base `Fonts` drawer.
    /// - **The euro country files win.** Nine names that `locale-base` and
    ///   `workbench-39` also place, same length, different bytes. This reads
    ///   both source variants straight off the disc and checks the tree got
    ///   the euro one — which is what proves recipe *order* and the
    ///   `overrides` declaration are both doing their job, the exact property
    ///   ART-224 found two components in the 3.2 recipe silently missing.
    ///
    /// Read-only with respect to the user's material: the disc is opened, the
    /// tree goes to `ART_159_DEST`.
    #[test]
    #[ignore = "needs the user's own AmigaOS 3.9 disc; set ART_159_ISO and ART_159_DEST"]
    fn build_the_real_39_language_components_when_asked() {
        let (Ok(iso), Ok(dest)) = (std::env::var("ART_159_ISO"), std::env::var("ART_159_DEST"))
        else {
            eprintln!(
                "skipped: set ART_159_ISO (the AmigaOS39.iso itself) and ART_159_DEST \
                 (an empty destination folder)"
            );
            return;
        };

        let iso_path = PathBuf::from(&iso);
        let media_folder = iso_path
            .parent()
            .expect("ART_159_ISO names a file inside some folder")
            .to_path_buf();
        let root = PathBuf::from(&dest);

        let request = crate::core::osinstall::plan::InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.9".to_string(),
            media_folder,
            rom: None,
            // All three optional components at once, on purpose: the euro
            // files have to meet `locale-base`'s copies for the override to
            // be exercised at all.
            chosen: vec![
                "locale-base".to_string(),
                "keymaps".to_string(),
                "special-locale-turkish".to_string(),
                "locale-euro".to_string(),
            ],
            excluded: Vec::new(),
            destination: root.clone(),
            scan_cache: Default::default(),
        };

        let recipe = crate::core::osinstall::recipe::amigaos_39().unwrap();
        let planned = crate::core::osinstall::plan::plan(&request, &recipe).unwrap();

        println!("=== ART-159: the plan ===");
        println!("components_on={:?}", planned.components_on);
        println!(
            "items={} bytes={}",
            planned.items.len(),
            planned.total_bytes
        );
        for refusal in &planned.refusals {
            println!("  refused: {refusal:?}");
        }
        assert!(
            planned.refusals.is_empty(),
            "a refusal here is either a `from` path the disc does not carry — including the \
             non-ASCII TÜRKÇE drawer — or an undeclared collision: {:?}",
            planned.refusals
        );
        assert_eq!(
            planned.components_on,
            vec![
                "workbench-base".to_string(),
                "locale-base".to_string(),
                "workbench-39".to_string(),
                "keymaps".to_string(),
                "special-locale-turkish".to_string(),
                "locale-euro".to_string(),
            ],
            "all six, in recipe order"
        );

        // Per component, from the plan itself — the count each one
        // contributes, so a rule that silently matched nothing shows up as a
        // zero rather than being lost in the total.
        let mut per_component: std::collections::BTreeMap<&str, usize> =
            std::collections::BTreeMap::new();
        for item in &planned.items {
            *per_component.entry(item.component.as_str()).or_default() += 1;
        }
        println!("plan items per component: {per_component:?}");
        assert_eq!(
            per_component.get("special-locale-turkish").copied(),
            Some(63),
            "13 .FONT descriptors + 37 size files + 13 family drawers"
        );
        // **10, not 9** — measured, and the expectation written first was
        // wrong rather than the code. A `Subtree` rule emits the drawer it
        // targets as an item of its own, so `Locale/Countries` is planned
        // alongside the nine `.country` files even though `locale-base`
        // plans it too. Harmless (creating a directory that exists is what
        // `apply` does all day) and worth stating, because "9" is the number
        // every other document in this round quotes.
        assert_eq!(
            per_component.get("locale-euro").copied(),
            Some(10),
            "nine .country files plus the Locale/Countries drawer the subtree rule targets"
        );

        let start = std::time::Instant::now();
        let outcome = apply(&planned, &root, &NoProgress).expect("build the tree");
        let elapsed = start.elapsed();
        println!(
            "=== ART-159: applied === files={} directories={} bytes={} in {:.2}s",
            outcome.files,
            outcome.directories,
            outcome.bytes,
            elapsed.as_secs_f64()
        );

        // **The thirteen families, on disk, checked by their exact names.**
        //
        // ART-225 is why this is written the hard way. Windows is
        // case-insensitive, so `fonts.join("courier-iso9.font").is_file()`
        // answers `true` for a file actually named `COURIER-ISO9.FONT` — the
        // very defect this now guards against, and the reason the first
        // version of this block passed while the running Amiga could see none
        // of these fonts. Every name below is compared against the directory's
        // own entries, character for character.
        //
        // The pairs are the disc's own spelling, inconsistencies and all.
        const DISC: [(&str, &str); 13] = [
            ("courier-iso9", "courier-iso9.font"),
            ("diamond-iso9", "diamond-iso9.font"),
            ("emerald-iso9", "emerald-iso9.font"),
            ("futurab-iso9", "FuturaB-ISO9.font"),
            ("garnet-iso9", "garnet-iso9.font"),
            ("personal-iso9", "Personal-ISO9.font"),
            ("times-iso9", "Times-ISO9.font"),
            ("topaz-iso9", "Topaz-ISO9.font"),
            ("topazt", "topazt.font"),
            ("xcourier-iso9", "XCourier-ISO9.font"),
            ("xen-iso9", "Xen-ISO9.font"),
            ("xen-wide-iso9", "Xen-Wide-ISO9.font"),
            ("xhelvetica-iso9", "XHelvetica-iso9.font"),
        ];

        let fonts = root.join("Fonts");
        let placed: std::collections::BTreeSet<String> = std::fs::read_dir(&fonts)
            .expect("the tree must carry a Fonts drawer")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| !n.ends_with(".uaem"))
            .collect();

        let mut sizes = 0usize;
        for (drawer, descriptor) in DISC {
            assert!(
                placed.contains(drawer),
                "the Fonts drawer holds no entry spelled exactly '{drawer}' — it holds {:?}",
                placed
                    .iter()
                    .filter(|n| n.eq_ignore_ascii_case(drawer))
                    .collect::<Vec<_>>()
            );
            assert!(
                placed.contains(descriptor),
                "the Fonts drawer holds no entry spelled exactly '{descriptor}'. ART-225: \
                 diskfont.library matches the '.font' suffix case-sensitively, so a \
                 descriptor spelled any other way is invisible to the running system, and \
                 Windows' case-insensitive filesystem will not tell you. Entries that differ \
                 only in case: {:?}",
                placed
                    .iter()
                    .filter(|n| n.eq_ignore_ascii_case(descriptor))
                    .collect::<Vec<_>>()
            );
            let bytes = std::fs::read(fonts.join(descriptor))
                .unwrap_or_else(|e| panic!("{descriptor}: {e}"));
            assert!(
                !bytes.is_empty(),
                "{descriptor} landed empty — a font AmigaOS would list and fail to open"
            );
            sizes += std::fs::read_dir(fonts.join(drawer))
                .unwrap()
                .flatten()
                .count();
        }
        println!("Turkish fonts: 13 families, {sizes} entries under them");
        // 37 real sizes, plus one `.uaem` sidecar each.
        assert_eq!(sizes, 74, "37 size files and their 37 .uaem sidecars");

        // **And every one of those names is the medium's own**, asked of the
        // disc rather than trusted from this array. This is the assertion that
        // makes ART-225's whole class impossible on real material: a recipe
        // author who retypes a destination instead of copying it fails here
        // even if the spelling they invented happens to look plausible.
        {
            let mut disc = CdSource::open(&iso_path).expect("open the disc");
            let listing: std::collections::BTreeSet<String> = disc
                .walk("OS-VERSION3.9/SPECIAL-LOCALE")
                .expect("the disc's Special-Locale drawer")
                .into_iter()
                .filter_map(|e| e.path.rsplit('/').next().map(str::to_string))
                .collect();
            for (drawer, descriptor) in DISC {
                assert!(
                    listing.contains(drawer),
                    "the disc spells no entry exactly '{drawer}'; the recipe invented it"
                );
                assert!(
                    listing.contains(descriptor),
                    "the disc spells no entry exactly '{descriptor}'; the recipe invented it"
                );
            }
            println!("all 26 names are the disc's own spelling");
        }

        // And the base fonts are all still there, untouched. The measured
        // claim behind "no overrides" is that these two sets are disjoint, so
        // this is the half a collision would break.
        for base in ["topaz.font", "times.font", "courier.font", "diamond.font"] {
            assert!(
                placed.contains(base),
                "{base} went missing — the base Fonts drawer must be untouched"
            );
        }

        // **ART-226: the keymaps, and the one name this test refuses to
        // spell.** Twenty-one of the twenty-two are ASCII and are pinned;
        // the twenty-second is the disc's Turkish one, whose bytes are the
        // whole reason this component uses a `Subtree` rule instead of a
        // typed list. Asserting a spelling for it here would be retyping it
        // in a second place — ART-225 in a test rather than in a recipe — so
        // what is asserted is its *shape* (exactly one non-ASCII pair) and
        // the run prints what it actually was.
        const ASCII_KEYMAPS: [&str; 21] = [
            "1251Q_US_RUS",
            "1251_GB1_RUS",
            "1251_GB_RUS",
            "br",
            "br2",
            "br3-ABNT2",
            "cat",
            "cdn",
            "ch1",
            "ch2",
            "d",
            "dk",
            "e",
            "f",
            "gb",
            "i",
            "n",
            "po",
            "s",
            "si",
            "usa2",
        ];
        let keymaps_dir = root.join("Devs").join("Keymaps");
        let placed_keymaps: std::collections::BTreeSet<String> = std::fs::read_dir(&keymaps_dir)
            .expect("ART-226: the tree must carry Devs/Keymaps")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| !n.ends_with(".uaem"))
            .collect();

        for name in ASCII_KEYMAPS {
            assert!(
                placed_keymaps.contains(name),
                "Devs/Keymaps holds no entry spelled exactly '{name}'"
            );
            assert!(
                placed_keymaps.contains(&format!("{name}.info")),
                "Devs/Keymaps holds no icon for '{name}' — Emergency-Boot on this same disc \
                 is a real bootable system and its Devs/Keymaps has one per keymap"
            );
        }
        let non_ascii: Vec<&String> = placed_keymaps.iter().filter(|n| !n.is_ascii()).collect();
        println!(
            "keymaps: {} entries, non-ASCII: {non_ascii:?}",
            placed_keymaps.len()
        );
        assert_eq!(
            non_ascii.len(),
            2,
            "the disc's Turkish keymap and its icon, carried through by the medium's own \
             spelling rather than retyped (ART-225); got {non_ascii:?}"
        );
        assert_eq!(placed_keymaps.len(), 44, "22 keymaps and their 22 icons");

        // The euro countries, asked of the disc rather than of a pinned hash:
        // read both source variants and check which one the tree holds.
        let mut source = CdSource::open(&iso_path).expect("open the disc");
        const EURO: [&str; 9] = [
            "\u{d6}STERREICH.COUNTRY",
            "BELGIE.COUNTRY",
            "BELGIQUE.COUNTRY",
            "DEUTSCHLAND.COUNTRY",
            "ESPA\u{d1}A.COUNTRY",
            "FRANCE.COUNTRY",
            "ITALIA.COUNTRY",
            "NEDERLAND.COUNTRY",
            "PORTUGAL.COUNTRY",
        ];
        let mut replaced = 0usize;
        for name in EURO {
            let euro = source
                .read(&format!("OS-VERSION3.9/LOCALE.EURO/COUNTRIES/{name}"))
                .unwrap_or_else(|e| panic!("the disc's euro {name}: {e}"));
            let base = source
                .read(&format!("OS-VERSION3.9/LOCALE/COUNTRIES/{name}"))
                .unwrap_or_else(|e| panic!("the disc's base {name}: {e}"));
            assert_ne!(
                euro, base,
                "{name}: the two sources are the same file, so this component places nothing \
                 new and the recipe's own note is wrong"
            );
            let placed = std::fs::read(root.join("Locale/Countries").join(name))
                .unwrap_or_else(|e| panic!("{name} should be in the tree: {e}"));
            assert_eq!(
                placed, euro,
                "{name}: the tree holds the base copy, so `locale-euro`'s override lost — \
                 either its `overrides` list or its position in the recipe"
            );
            replaced += 1;
        }
        println!("euro countries: {replaced} of 9 replaced the base copy");
        assert_eq!(replaced, 9);
    }

    /// **ART-226 question 2, against the owner's own archive.** Builds a 3.9
    /// tree with the `locale-39` package selected and checks what actually
    /// lands.
    ///
    /// The measurement this exists for is the one the owner asked about: the
    /// CD's shelf carries 22 keymaps and this archive carries **49**, so a
    /// tree with the package on should hold the larger set — Turkish among
    /// them — in the drawer AmigaOS loads from, not on a shelf.
    ///
    /// Read-only with respect to the user's material: the disc and the
    /// archive are opened, the tree goes to `ART_L39_DEST`.
    #[test]
    #[ignore = "needs the user's own AmigaOS 3.9 disc and Locale3_9.lha; set ART_L39_ISO, ART_L39_PACKAGES and ART_L39_DEST"]
    fn install_the_locale_update_when_asked() {
        let (Ok(iso), Ok(packages), Ok(dest)) = (
            std::env::var("ART_L39_ISO"),
            std::env::var("ART_L39_PACKAGES"),
            std::env::var("ART_L39_DEST"),
        ) else {
            return;
        };

        let iso_path = PathBuf::from(&iso);
        let media_folder = iso_path.parent().unwrap().to_path_buf();
        let root = PathBuf::from(&dest);

        let request = crate::core::osinstall::plan::InstallRequest {
            packages: vec!["locale-39".to_string()],
            package_folder: Some(PathBuf::from(&packages)),
            release: "AmigaOS 3.9".to_string(),
            media_folder,
            rom: None,
            chosen: vec!["locale-base".to_string(), "keymaps".to_string()],
            excluded: Vec::new(),
            destination: root.clone(),
            scan_cache: Default::default(),
        };

        let recipe = crate::core::osinstall::recipe::amigaos_39().unwrap();
        let planned = crate::core::osinstall::plan::plan(&request, &recipe).expect("plan");
        println!("components_on={:?}", planned.components_on);
        for refusal in &planned.refusals {
            println!("  refused: {refusal:?}");
        }
        assert!(planned.refusals.is_empty(), "{:?}", planned.refusals);

        let from_package = planned
            .items
            .iter()
            .filter(|i| i.component == "locale-39")
            .count();
        println!("locale-39 plan items: {from_package}");

        let outcome = apply(&planned, &root, &NoProgress).expect("build the tree");
        println!(
            "applied: files={} directories={} bytes={}",
            outcome.files, outcome.directories, outcome.bytes
        );

        // The point of the exercise, on disk.
        let keymaps: std::collections::BTreeSet<String> =
            std::fs::read_dir(root.join("Devs").join("Keymaps"))
                .expect("Devs/Keymaps")
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| !n.ends_with(".uaem") && !n.ends_with(".info"))
                .collect();
        println!("Devs/Keymaps holds {} keymaps", keymaps.len());
        let non_ascii: Vec<&String> = keymaps.iter().filter(|n| !n.is_ascii()).collect();
        println!("  non-ASCII among them: {non_ascii:?}");
        assert!(
            keymaps.len() > 22,
            "the archive carries 49 where the CD shelf carries 22; got {}",
            keymaps.len()
        );
        assert_eq!(
            non_ascii.len(),
            1,
            "the Turkish keymap, carried by the medium's own spelling; got {non_ascii:?}"
        );
        for expected in ["czech", "slovak", "hrvatska", "srpski_D"] {
            assert!(
                keymaps.contains(expected),
                "'{expected}' is in the archive and not on the CD shelf, so its presence is \
                 what says the package's keymaps really arrived"
            );
        }
    }

    // ---- Task 6: produce with a package, or add one afterwards ----------

    /// Every file in a tree, path -> bytes, with the manifest left out.
    ///
    /// `distribution.json` records *when* an install happened and in how
    /// many steps, which legitimately differs between the two paths;
    /// everything it says about the tree's contents is checked separately
    /// below. Every other file, `.uaem` sidecars included, must match
    /// exactly.
    fn tree_contents(
        root: &std::path::Path,
    ) -> std::collections::BTreeMap<String, Option<Vec<u8>>> {
        let mut out = std::collections::BTreeMap::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                let rel = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                if path.is_dir() {
                    // Recorded as `None` rather than skipped (fix round 2,
                    // F5): a drawer the two paths disagree about — one
                    // creates an empty `Storage/Monitors`, the other does
                    // not — is a real difference between the trees, and an
                    // Amiga volume built from one of them would differ. A
                    // files-only map could not see it.
                    out.insert(rel, None);
                    stack.push(path);
                    continue;
                }
                if rel == MANIFEST_FILE_NAME {
                    continue;
                }
                out.insert(rel, Some(std::fs::read(&path).unwrap()));
            }
        }
        out
    }

    fn read_manifest(root: &Path) -> DistributionManifest {
        serde_json::from_str(&std::fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap())
            .unwrap()
    }

    /// What the manifest says about where every byte came from: for each
    /// path, every record's component, its medium, and what it overwrote.
    ///
    /// Deliberately not the whole [`FileRecord`] — `sha256` and `bytes`
    /// describe the file's content, which `tree_contents` already compares
    /// byte for byte, and `protection` likewise reaches disk as a `.uaem`
    /// sidecar. What only the manifest can answer is *provenance*, and that
    /// is what this projects.
    ///
    /// **Sorted per path, and that is a real equivalence, not a blind spot**
    /// (fix round 2, F5). More than one record for one path happens in
    /// exactly one place — `S/User-Startup`, one record per contributing
    /// component (see the module doc comment) — and those records describe
    /// one file's one content; which contributor is listed first says
    /// nothing about the tree. Ordering *between* paths is a different
    /// question and is not sorted away: the caller asserts
    /// `left.files == right.files` outright, so a difference in the
    /// manifest's own record order fails the test rather than hiding here.
    #[allow(clippy::type_complexity)]
    fn sources_by_path(
        manifest: &DistributionManifest,
    ) -> std::collections::BTreeMap<String, Vec<(String, String, Option<Overwritten>)>> {
        let mut out: std::collections::BTreeMap<
            String,
            Vec<(String, String, Option<Overwritten>)>,
        > = std::collections::BTreeMap::new();
        for file in &manifest.files {
            out.entry(file.path.clone()).or_default().push((
                file.component.clone(),
                file.media.clone(),
                file.overwrote.clone(),
            ));
        }
        for records in out.values_mut() {
            records.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
        }
        out
    }

    /// A media folder and a package folder, the two kept apart the way the
    /// owner keeps them apart.
    fn package_dirs(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = fixtures::scratch(&format!("apply-packages-{tag}-{n}"));
        let media = dir.join("media");
        let packages = dir.join("packages");
        std::fs::create_dir(&media).unwrap();
        std::fs::create_dir(&packages).unwrap();
        fixtures::package_test_media(&media);
        (dir, media, packages)
    }

    fn install_request(
        media: &Path,
        packages_folder: &Path,
        destination: &Path,
        packages: &[&str],
    ) -> crate::core::osinstall::plan::InstallRequest {
        crate::core::osinstall::plan::InstallRequest {
            release: "Test OS".to_string(),
            media_folder: media.to_path_buf(),
            rom: None,
            chosen: Vec::new(),
            excluded: Vec::new(),
            packages: packages.iter().map(|s| s.to_string()).collect(),
            package_folder: Some(packages_folder.to_path_buf()),
            destination: destination.to_path_buf(),
            scan_cache: Default::default(),
        }
    }

    /// The catalogue every equivalence test below plans against — both
    /// synthetic packages, so one helper serves the one-package and the
    /// two-package forms.
    fn test_catalogue() -> Vec<crate::core::osinstall::package::Package> {
        vec![
            fixtures::package_test_package(),
            fixtures::package_test_package_two(),
        ]
    }

    fn planned_over(request: &crate::core::osinstall::plan::InstallRequest) -> InstallPlan {
        let plan = crate::core::osinstall::plan::plan_over(
            request,
            &fixtures::package_test_recipe(),
            &test_catalogue(),
        )
        .unwrap();
        assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
        plan
    }

    /// Two trees must be the same tree: every path, every byte, every
    /// drawer, and everything `distribution.json` says about where the bytes
    /// came from.
    ///
    /// Shared by both equivalence tests so the one-package and two-package
    /// forms are held to exactly the same standard — a weaker comparison in
    /// one of them would be a hole with a test sitting over it.
    fn assert_trees_agree(left: &Path, right: &Path) {
        let a = tree_contents(left);
        let b = tree_contents(right);

        // Name what differs rather than asserting equality over a wall of
        // bytes: the first line of a failure should be a path.
        let only_left: Vec<&String> = a.keys().filter(|k| !b.contains_key(*k)).collect();
        let only_right: Vec<&String> = b.keys().filter(|k| !a.contains_key(*k)).collect();
        assert!(
            only_left.is_empty(),
            "only in the built tree: {only_left:?}"
        );
        assert!(
            only_right.is_empty(),
            "only in the added tree: {only_right:?}"
        );
        for (path, left_bytes) in &a {
            assert_eq!(left_bytes, &b[path], "{path} differs between the two paths");
        }

        // The manifest's account of the tree must agree even though its
        // timestamps do not: every file records the same source component
        // and the same thing overwritten.
        let left_manifest = read_manifest(left);
        let right_manifest = read_manifest(right);
        assert_eq!(
            sources_by_path(&left_manifest),
            sources_by_path(&right_manifest)
        );
        // Outright, records and order included (fix round 2, F5): whatever
        // `sources_by_path` sorts or projects away is caught here instead of
        // going unexamined.
        assert_eq!(left_manifest.files, right_manifest.files);
        // And the same media, in the same order — a package archive is a
        // medium this tree came out of exactly as a floppy is.
        assert_eq!(left_manifest.built_from, right_manifest.built_from);
    }

    /// `produce(base + A)` and `add(produce(base), A)` must give the same
    /// tree, byte for byte. Two entry points into one placer that disagree
    /// mean one of them is wrong, and nothing short of comparing the
    /// results says which.
    ///
    /// The package overwrites `C/LoadModule`, a file the base component
    /// already wrote — without that, this would pass while proving nothing
    /// about the case the whole round is about.
    #[test]
    fn producing_with_a_package_equals_adding_it_afterwards() {
        let (dir, media, packages) = package_dirs("equivalence");
        let archive = fixtures::package_test_archive(&packages, "pack.zip");
        let left = dir.join("produced");
        let right = dir.join("added");

        // produce(base + package)
        let with_package = install_request(&media, &packages, &left, &["test-package"]);
        let planned = planned_over(&with_package);
        apply(&planned, &left, &NoProgress).unwrap();

        // add(produce(base), package)
        let base_only = install_request(&media, &packages, &right, &[]);
        let base_plan = planned_over(&base_only);
        apply(&base_plan, &right, &NoProgress).unwrap();
        add_package(
            &right,
            &fixtures::package_test_package(),
            &archive,
            &NoProgress,
        )
        .unwrap();

        // The premise, checked rather than assumed: the package really did
        // land on a file the base had already written.
        assert_eq!(
            std::fs::read(left.join("C").join("LoadModule")).unwrap(),
            b"package LoadModule",
            "the fixture must overwrite a base file, or this test proves nothing"
        );

        assert_trees_agree(&left, &right);
    }

    /// Spec §2's own form: `produce(base + A + B) == add(produce(base + A), B)`.
    ///
    /// **The one-package form above is the weaker statement** (fix round 2,
    /// F4). It never adds onto a tree that already holds a package, so it
    /// never exercises the case where the file being overwritten was put
    /// there by *another package* rather than by the release — which is both
    /// the shape a real user meets (BoingBag 3.9-1, then 3.9-2) and the shape
    /// where Add's own rules have to reach the same verdict Produce reaches.
    ///
    /// `test-package-two` writes `C/LoadModule` for the third time and
    /// declares `overrides: ["base-c", "test-package"]`, so both claimants
    /// are named; the test asserts the third writer's bytes actually won,
    /// because an equivalence over two trees that both failed to apply B
    /// would pass while proving nothing.
    #[test]
    fn producing_with_two_packages_equals_adding_the_second_afterwards() {
        let (dir, media, packages) = package_dirs("equivalence-two");
        fixtures::package_test_archive(&packages, "pack.zip");
        let second = fixtures::package_test_archive_two(&packages, "pack2.zip");
        let left = dir.join("produced");
        let right = dir.join("added");

        // produce(base + A + B)
        let both = install_request(
            &media,
            &packages,
            &left,
            &["test-package", "test-package-two"],
        );
        let planned = planned_over(&both);
        apply(&planned, &left, &NoProgress).unwrap();

        // add(produce(base + A), B)
        let first_only = install_request(&media, &packages, &right, &["test-package"]);
        let first_plan = planned_over(&first_only);
        apply(&first_plan, &right, &NoProgress).unwrap();
        add_package(
            &right,
            &fixtures::package_test_package_two(),
            &second,
            &NoProgress,
        )
        .unwrap();

        // B really did land, over A's own copy of the same path.
        assert_eq!(
            std::fs::read(left.join("C").join("LoadModule")).unwrap(),
            b"second package LoadModule",
            "the second package must win the path both earlier writers claimed"
        );
        assert!(
            right.join("C").join("OnlyPack").is_file(),
            "A's own file survives"
        );
        assert!(
            right.join("C").join("OnlyPack2").is_file(),
            "B's own file lands"
        );

        assert_trees_agree(&left, &right);

        // What B displaced was A's file, not the base's — the manifest keeps
        // the chain rather than flattening it to the original.
        let manifest = read_manifest(&right);
        let record = manifest
            .files
            .iter()
            .find(|f| f.path == fixtures::OVERWRITTEN_PATH)
            .unwrap();
        assert_eq!(record.component, "test-package-two");
        let displaced = record.overwrote.as_ref().expect("B overwrote A");
        assert_eq!(displaced.component, "test-package");
        assert_eq!(
            displaced
                .overwrote
                .as_ref()
                .expect("and A had overwritten the base")
                .component,
            "base-c"
        );
    }

    /// The other half of the equivalence: the package's own files are
    /// there, the base's untouched files survive, and the manifest says
    /// which is which — so "the two agree" is not two identically wrong
    /// trees.
    #[test]
    fn a_package_replaces_what_it_carries_and_leaves_the_rest_alone() {
        let (dir, media, packages) = package_dirs("replaces");
        let archive = fixtures::package_test_archive(&packages, "pack.zip");
        let root = dir.join("dist");

        let base_only = install_request(&media, &packages, &root, &[]);
        apply(&planned_over(&base_only), &root, &NoProgress).unwrap();
        assert_eq!(
            std::fs::read(root.join("C").join("LoadModule")).unwrap(),
            b"base LoadModule"
        );

        let outcome = add_package(
            &root,
            &fixtures::package_test_package(),
            &archive,
            &NoProgress,
        )
        .unwrap();

        assert_eq!(
            std::fs::read(root.join("C").join("LoadModule")).unwrap(),
            b"package LoadModule"
        );
        assert_eq!(
            std::fs::read(root.join("C").join("OnlyPack")).unwrap(),
            b"package only"
        );
        assert_eq!(
            std::fs::read(root.join("C").join("OnlyBase")).unwrap(),
            b"base only",
            "a file the package does not carry is not this package's to touch"
        );
        // Two files written by this run — not the whole tree.
        assert_eq!(outcome.files, 2);

        let manifest = read_manifest(&root);
        let untouched = manifest
            .files
            .iter()
            .find(|f| f.path == "C/OnlyBase")
            .unwrap();
        assert_eq!(untouched.component, "base-c");
        assert!(untouched.overwrote.is_none());
    }

    /// The manifest stays a true account of where every byte came from: an
    /// overwritten file records the package **and** what it displaced.
    #[test]
    fn the_manifest_records_the_package_and_what_it_overwrote() {
        let (dir, media, packages) = package_dirs("overwrote");
        let archive = fixtures::package_test_archive(&packages, "pack.zip");
        let root = dir.join("dist");

        let base_only = install_request(&media, &packages, &root, &[]);
        apply(&planned_over(&base_only), &root, &NoProgress).unwrap();
        let before = read_manifest(&root);
        let base_record = before
            .files
            .iter()
            .find(|f| f.path == fixtures::OVERWRITTEN_PATH)
            .unwrap()
            .clone();

        add_package(
            &root,
            &fixtures::package_test_package(),
            &archive,
            &NoProgress,
        )
        .unwrap();

        let manifest = read_manifest(&root);
        let records: Vec<&FileRecord> = manifest
            .files
            .iter()
            .filter(|f| f.path == fixtures::OVERWRITTEN_PATH)
            .collect();
        assert_eq!(
            records.len(),
            1,
            "one file on disk is one record, never two claims"
        );
        assert_eq!(records[0].component, "test-package");
        assert_eq!(records[0].media, "TestPack");
        assert_eq!(records[0].sha256, sha256_bytes(b"package LoadModule"));

        let overwrote = records[0]
            .overwrote
            .as_ref()
            .expect("what a package replaced is never dropped from the manifest");
        assert_eq!(overwrote.component, "base-c");
        assert_eq!(overwrote.media, "TestBase");
        assert_eq!(overwrote.sha256, base_record.sha256);
        assert_eq!(overwrote.bytes, base_record.bytes);
    }

    /// Without the manifest ART cannot say what it is overwriting, and the
    /// whole preview rests on knowing.
    #[test]
    fn adding_a_package_to_a_tree_with_no_manifest_is_refused() {
        let (dir, media, packages) = package_dirs("no-manifest");
        let archive = fixtures::package_test_archive(&packages, "pack.zip");
        let root = dir.join("dist");

        let base_only = install_request(&media, &packages, &root, &[]);
        apply(&planned_over(&base_only), &root, &NoProgress).unwrap();
        std::fs::remove_file(root.join(MANIFEST_FILE_NAME)).unwrap();
        let before = tree_contents(&root);

        let err = add_package(
            &root,
            &fixtures::package_test_package(),
            &archive,
            &NoProgress,
        )
        .unwrap_err();
        assert!(matches!(err, CoreError::SafetyRefused(_)), "got {err:?}");

        assert_eq!(
            tree_contents(&root),
            before,
            "a refused add writes nothing at all"
        );
    }

    /// `add_package` takes the archive the caller already resolved — so the
    /// one thing it must still check is that the archive really is this
    /// package's, which `ArchiveSource` answers from inside the file rather
    /// than from its name.
    #[test]
    fn adding_a_package_from_an_archive_that_is_not_its_own_is_refused() {
        let (dir, media, packages) = package_dirs("wrong-archive");
        let root = dir.join("dist");

        let base_only = install_request(&media, &packages, &root, &[]);
        apply(&planned_over(&base_only), &root, &NoProgress).unwrap();

        let other = packages.join("something-else.zip");
        std::fs::write(
            &other,
            crate::core::archive::zip::tests::make_zip_with(&[
                ("SomethingElse/C/", b"" as &[u8]),
                ("SomethingElse/C/LoadModule", b"not this package"),
            ]),
        )
        .unwrap();
        let before = tree_contents(&root);

        let err = add_package(
            &root,
            &fixtures::package_test_package(),
            &other,
            &NoProgress,
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("SomethingElse") && err.contains("TestPack"),
            "got {err}"
        );
        assert_eq!(tree_contents(&root), before);
    }

    /// A rule the archive cannot satisfy is refused before a byte is
    /// written — never partway through, leaving a tree half-updated and a
    /// manifest that still describes the old one.
    #[test]
    fn a_package_whose_rule_does_not_resolve_is_refused_before_anything_lands() {
        let (dir, media, packages) = package_dirs("bad-rule");
        let root = dir.join("dist");

        let base_only = install_request(&media, &packages, &root, &[]);
        apply(&planned_over(&base_only), &root, &NoProgress).unwrap();
        let before = tree_contents(&root);

        // An archive that answers to the right name and holds nothing the
        // package's `C` rule can resolve.
        let empty = packages.join("empty.zip");
        std::fs::write(
            &empty,
            crate::core::archive::zip::tests::make_zip_with(&[
                ("TestPack/Libs/", b"" as &[u8]),
                ("TestPack/Libs/x.library", b"lib"),
            ]),
        )
        .unwrap();

        let err = add_package(
            &root,
            &fixtures::package_test_package(),
            &empty,
            &NoProgress,
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("test-package") && err.contains('C'),
            "got {err}"
        );
        assert_eq!(tree_contents(&root), before);
    }

    /// Cancellation stops between whole items, never inside one — and then
    /// the manifest is brought into line with what actually landed.
    ///
    /// **This is the opposite of `apply`'s rule, deliberately** (fix round
    /// 2, F6). A cancelled `apply` leaves no `distribution.json` at all,
    /// which is honest: a half-built tree that claims nothing cannot lie. A
    /// cancelled Add cannot do that — the tree already had a manifest, files
    /// on disk have already changed, and leaving the old one standing would
    /// make `distribution.json` describe bytes that are no longer there.
    /// That is the single thing this file exists to prevent, so the manifest
    /// is written before the cancellation is reported.
    #[test]
    fn a_cancelled_add_leaves_the_manifest_describing_what_actually_landed() {
        let (dir, media, packages) = package_dirs("cancel");
        let archive = fixtures::package_test_archive(&packages, "pack.zip");
        let root = dir.join("dist");

        let base_only = install_request(&media, &packages, &root, &[]);
        apply(&planned_over(&base_only), &root, &NoProgress).unwrap();

        // The package's items are `C` (a drawer), then its two files. Two
        // reports in, one file has landed and one has not.
        let err = add_package(
            &root,
            &fixtures::package_test_package(),
            &archive,
            &fixtures::CancelAfter::new(2),
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                CoreError::Cancelled | CoreError::CancelledPartway { .. }
            ),
            "got {err:?}"
        );

        // Every file the manifest names holds exactly the bytes it claims —
        // whatever the run got through, the record and the tree agree.
        let manifest = read_manifest(&root);
        for record in &manifest.files {
            let bytes =
                std::fs::read(root.join(record.path.replace('/', "\\"))).unwrap_or_else(|e| {
                    panic!(
                        "the manifest names '{}', which is not there: {e}",
                        record.path
                    )
                });
            assert_eq!(
                sha256_bytes(&bytes),
                record.sha256,
                "'{}' does not hold what the manifest says it holds",
                record.path
            );
            assert_eq!(bytes.len() as u64, record.bytes, "{}", record.path);
        }

        // And it is not simply the old manifest: at least one file the
        // package carries landed and is recorded as the package's.
        assert!(
            manifest.files.iter().any(|f| f.component == "test-package"),
            "the cancelled run wrote a file, so the manifest must say so: {:?}",
            manifest.files
        );
    }

    /// **Nothing is overwritten silently** — the rule Produce enforces
    /// through `plan::detect_collisions`, and which Add ignored entirely
    /// until fix round 2 (F1): the same package, the same tree, refused by
    /// one entry point and written by the other.
    ///
    /// The equivalence test could not see it because its fixture *declares*
    /// the override, so the refusing branch was never taken. This test takes
    /// it, on both paths at once, and asserts they reach the same verdict.
    #[test]
    fn a_package_that_never_declared_the_override_is_refused_by_both_paths() {
        let (dir, media, packages) = package_dirs("undeclared-add");
        let archive = fixtures::package_test_archive(&packages, "pack.zip");
        let root = dir.join("dist");

        let mut undeclaring = fixtures::package_test_package();
        undeclaring.component.overrides.clear();

        // Produce refuses it.
        let produce = crate::core::osinstall::plan::plan_over(
            &install_request(&media, &packages, &dir.join("produced"), &["test-package"]),
            &fixtures::package_test_recipe(),
            std::slice::from_ref(&undeclaring),
        )
        .unwrap();
        assert!(
            produce.refusals.iter().any(|r| matches!(
                r,
                crate::core::osinstall::RefusalReason::DestinationCollision { .. }
            )),
            "{:?}",
            produce.refusals
        );

        // Add must reach the same verdict on the same facts.
        let base_only = install_request(&media, &packages, &root, &[]);
        apply(&planned_over(&base_only), &root, &NoProgress).unwrap();
        let before = tree_contents(&root);

        let err = add_package(&root, &undeclaring, &archive, &NoProgress).unwrap_err();
        assert!(matches!(err, CoreError::SafetyRefused(_)), "got {err:?}");
        assert!(
            err.to_string().contains(fixtures::OVERWRITTEN_PATH),
            "the refusal names the file it would have written over: {err}"
        );
        assert_eq!(
            tree_contents(&root),
            before,
            "a refused add writes nothing at all"
        );
    }

    /// A file the manifest never recorded is undeclared too — most likely
    /// something the user put there themselves, which is exactly the case
    /// where writing over it silently would be worst.
    #[test]
    fn a_package_landing_on_a_file_the_manifest_never_recorded_is_refused() {
        let (dir, media, packages) = package_dirs("unrecorded");
        let archive = fixtures::package_test_archive(&packages, "pack.zip");
        let root = dir.join("dist");

        let base_only = install_request(&media, &packages, &root, &[]);
        apply(&planned_over(&base_only), &root, &NoProgress).unwrap();
        // Nothing in `distribution.json` claims this one.
        std::fs::write(root.join("C").join("OnlyPack"), b"the user's own file").unwrap();

        let err = add_package(
            &root,
            &fixtures::package_test_package(),
            &archive,
            &NoProgress,
        )
        .unwrap_err();
        assert!(matches!(err, CoreError::SafetyRefused(_)), "got {err:?}");
        let text = err.to_string();
        assert!(text.contains("C/OnlyPack"), "got {text}");
        // The instruction has to be the one that would help: there is no
        // component to declare an override over, so saying so would be
        // wrong (fix round 3).
        assert!(
            text.contains(MANIFEST_FILE_NAME) && !text.contains("overrides"),
            "an unrecorded file is not an undeclared override: {text}"
        );
        assert_eq!(
            std::fs::read(root.join("C").join("OnlyPack")).unwrap(),
            b"the user's own file"
        );
    }

    // ---- fix round 3: a destination is compared as AmigaDOS compares it ---

    /// Every file `distribution.json` names really holds what it says it
    /// holds. A manifest that describes bytes nobody wrote is the one thing
    /// this file exists to prevent, and it is the shape F11 produced on the
    /// Produce path.
    fn assert_manifest_matches_disk(root: &Path) {
        let manifest = read_manifest(root);
        for record in &manifest.files {
            let on_disk = root.join(record.path.replace('/', "\\"));
            let bytes = std::fs::read(&on_disk).unwrap_or_else(|e| {
                panic!(
                    "{MANIFEST_FILE_NAME} names '{}', which is not there: {e}",
                    record.path
                )
            });
            assert_eq!(
                sha256_bytes(&bytes),
                record.sha256,
                "'{}' does not hold what the manifest says it holds",
                record.path
            );
        }
    }

    /// The real spellings, not invented ones: the Joliet-less `AmigaOS39.iso`
    /// yields `C/ASSIGN` and BoingBag 3.9-1's ZIP payload yields `C/Assign`,
    /// so **every one of that package's 211 files** is this case.
    ///
    /// Destinations used to be compared with `==` while everything around
    /// them resolved case-insensitively, and the two entry points failed in
    /// opposite, equally bad ways: Add refused all 211 as undeclared
    /// overwrites *despite* the declared `overrides`, and Produce wrote them
    /// silently and left a manifest naming a file whose `sha256` matched
    /// nothing on disk.
    fn case_dirs(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = fixtures::scratch(&format!("apply-case-{tag}-{n}"));
        let media = dir.join("media");
        let packages = dir.join("packages");
        std::fs::create_dir(&media).unwrap();
        std::fs::create_dir(&packages).unwrap();
        // The disc's own all-caps spelling.
        fixtures::media(
            &media,
            "TestBase",
            "base.adf",
            &[
                ("C/ASSIGN", b"disc Assign", 0x20),
                ("C/OnlyBase", b"base only", 0),
            ],
        );
        // The package payload's mixed-case one.
        std::fs::write(
            packages.join("pack.zip"),
            crate::core::archive::zip::tests::make_zip_with(&[
                ("TestPack/C/Assign", b"package Assign" as &[u8]),
                ("TestPack/C/OnlyPack", b"package only"),
            ]),
        )
        .unwrap();
        (dir, media, packages)
    }

    #[test]
    fn a_destination_spelled_in_another_case_is_the_same_destination_to_both_paths() {
        let (dir, media, packages) = case_dirs("both");
        let archive = packages.join("pack.zip");
        let left = dir.join("produced");
        let right = dir.join("added");

        // Produce: the collision is seen, the declared override resolves it,
        // and the plan does not refuse.
        let with_package = install_request(&media, &packages, &left, &["test-package"]);
        let planned = planned_over(&with_package);
        apply(&planned, &left, &NoProgress).unwrap();

        // Add: must **not** refuse — the manifest records `C/ASSIGN` and the
        // package writes `C/Assign`, which is the same file.
        let base_only = install_request(&media, &packages, &right, &[]);
        apply(&planned_over(&base_only), &right, &NoProgress).unwrap();
        add_package(
            &right,
            &fixtures::package_test_package(),
            &archive,
            &NoProgress,
        )
        .expect("the package declares `overrides: [base-c]`, and this is that file");

        for root in [&left, &right] {
            // One file, not two: `C` holds `Assign`, `OnlyBase` and
            // `OnlyPack`, plus their sidecars.
            let names: std::collections::BTreeSet<String> = std::fs::read_dir(root.join("C"))
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .filter(|n| !n.ends_with(".uaem"))
                .collect();
            assert_eq!(
                names,
                ["Assign", "OnlyBase", "OnlyPack"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                "the two spellings are one file, in {}",
                root.display()
            );

            // One record for it, and it is the package's.
            let manifest = read_manifest(root);
            let records: Vec<&FileRecord> = manifest
                .files
                .iter()
                .filter(|f| f.path.eq_ignore_ascii_case("C/Assign"))
                .collect();
            assert_eq!(records.len(), 1, "{:?}", manifest.files);
            assert_eq!(records[0].component, "test-package");
            assert_eq!(
                records[0].overwrote.as_ref().map(|o| o.component.as_str()),
                Some("base-c"),
                "and it records what it replaced"
            );

            // The whole point: the manifest describes the tree.
            assert_manifest_matches_disk(root);
        }

        assert_trees_agree(&left, &right);
    }

    /// The undeclared case still refuses when the spellings differ — the
    /// case fix must not have turned the collision check off, only made it
    /// find the right owner.
    #[test]
    fn a_differently_cased_destination_is_still_refused_when_undeclared() {
        let (dir, media, packages) = case_dirs("undeclared");
        let archive = packages.join("pack.zip");
        let root = dir.join("dist");

        let mut undeclaring = fixtures::package_test_package();
        undeclaring.component.overrides.clear();

        let produce = crate::core::osinstall::plan::plan_over(
            &install_request(&media, &packages, &dir.join("produced"), &["test-package"]),
            &fixtures::package_test_recipe(),
            std::slice::from_ref(&undeclaring),
        )
        .unwrap();
        assert!(
            produce.refusals.iter().any(|r| matches!(
                r,
                crate::core::osinstall::RefusalReason::DestinationCollision { .. }
            )),
            "a collision that only differs in case is still a collision: {:?}",
            produce.refusals
        );

        let base_only = install_request(&media, &packages, &root, &[]);
        apply(&planned_over(&base_only), &root, &NoProgress).unwrap();
        let err = add_package(&root, &undeclaring, &archive, &NoProgress).unwrap_err();
        assert!(matches!(err, CoreError::SafetyRefused(_)), "got {err:?}");
    }

    /// F1's remaining branch: a package writing over **its own** earlier
    /// file. It works by construction (`component.id == package.id`), which
    /// is exactly the kind of thing that stops working when somebody
    /// rearranges the match arms.
    #[test]
    fn adding_the_same_package_twice_replaces_its_own_files_rather_than_refusing() {
        let (dir, media, packages) = package_dirs("re-add");
        let archive = fixtures::package_test_archive(&packages, "pack.zip");
        let root = dir.join("dist");

        let base_only = install_request(&media, &packages, &root, &[]);
        apply(&planned_over(&base_only), &root, &NoProgress).unwrap();
        add_package(
            &root,
            &fixtures::package_test_package(),
            &archive,
            &NoProgress,
        )
        .unwrap();

        // Again. Nothing about the tree has changed except that the package
        // is now the owner of the files it is about to write.
        let outcome = add_package(
            &root,
            &fixtures::package_test_package(),
            &archive,
            &NoProgress,
        )
        .expect("a package may replace its own earlier files");
        assert_eq!(outcome.files, 2);

        let manifest = read_manifest(&root);
        let records: Vec<&FileRecord> = manifest
            .files
            .iter()
            .filter(|f| f.path == fixtures::OVERWRITTEN_PATH)
            .collect();
        assert_eq!(records.len(), 1, "one file is one record");
        assert_eq!(records[0].component, "test-package");
        assert_eq!(
            records[0].overwrote.as_ref().map(|o| o.component.as_str()),
            Some("test-package"),
            "the second write replaced the first, and says so"
        );
        // `built_from` gains no second entry for one archive.
        assert_eq!(
            manifest
                .built_from
                .iter()
                .filter(|m| m.volume_name == "TestPack")
                .count(),
            1
        );
        assert_manifest_matches_disk(&root);
    }

    /// A refusal about a real BoingBag would otherwise print 211 paths.
    #[test]
    fn a_refusal_names_a_few_paths_and_then_counts_the_rest() {
        let (dir, media, packages) = package_dirs("many");
        let root = dir.join("dist");

        // Eight files the base tree does not have, written straight into the
        // tree so nothing in `distribution.json` claims them.
        let base_only = install_request(&media, &packages, &root, &[]);
        apply(&planned_over(&base_only), &root, &NoProgress).unwrap();

        let mut rows: Vec<(String, Vec<u8>)> = Vec::new();
        for n in 0..8 {
            let name = format!("Cmd{n}");
            std::fs::write(root.join("C").join(&name), b"the user's own").unwrap();
            rows.push((format!("TestPack/C/{name}"), b"package".to_vec()));
        }
        let refs: Vec<(&str, &[u8])> = rows
            .iter()
            .map(|(n, b)| (n.as_str(), b.as_slice()))
            .collect();
        let archive = packages.join("many.zip");
        std::fs::write(
            &archive,
            crate::core::archive::zip::tests::make_zip_with(&refs),
        )
        .unwrap();

        let err = add_package(
            &root,
            &fixtures::package_test_package(),
            &archive,
            &NoProgress,
        )
        .unwrap_err()
        .to_string();

        assert!(
            err.contains("8 file(s)"),
            "the count is always exact: {err}"
        );
        assert!(err.contains("and 3 more"), "and the list is not: {err}");
        assert_eq!(
            err.matches("C/Cmd").count(),
            REFUSAL_PATHS_SHOWN,
            "exactly {REFUSAL_PATHS_SHOWN} paths are named: {err}"
        );
        assert!(
            err.contains(MANIFEST_FILE_NAME),
            "an unrecorded file is a different problem from an undeclared \
             override, and the sentence has to say which: {err}"
        );
    }

    /// **The sidecar and the manifest are two halves of one claim** (fix
    /// round 2, F2). An archive states no AmigaDOS protection, date or
    /// comment — `source_archive.rs` calls its values declared defaults,
    /// never a reading — so a package overwriting a file the release media
    /// placed leaves the release's own statement standing, and the manifest
    /// records *that*, not the archive's zero.
    ///
    /// Before this, `C/LoadModule.uaem` said `--p-rwed` while the manifest
    /// recorded `protection: 0`: the tree contradicting itself, on both
    /// paths identically, which is why the equivalence test was green over
    /// it. Asserting the sidecar's **content** rather than its presence is
    /// the difference.
    ///
    /// Not cosmetic: `--p-rwed`'s `p` is the pure bit, and AmigaOS 3.2's
    /// `Startup-Sequence` runs `Resident C:Assign PURE`.
    #[test]
    fn an_overwritten_files_sidecar_and_manifest_agree_about_its_protection() {
        let (dir, media, packages) = package_dirs("sidecar");
        let archive = fixtures::package_test_archive(&packages, "pack.zip");
        let left = dir.join("produced");
        let right = dir.join("added");

        let with_package = install_request(&media, &packages, &left, &["test-package"]);
        apply(&planned_over(&with_package), &left, &NoProgress).unwrap();

        let base_only = install_request(&media, &packages, &right, &[]);
        apply(&planned_over(&base_only), &right, &NoProgress).unwrap();
        add_package(
            &right,
            &fixtures::package_test_package(),
            &archive,
            &NoProgress,
        )
        .unwrap();

        for root in [&left, &right] {
            let sidecar =
                std::fs::read_to_string(root.join("C").join(format!("LoadModule.{}", "uaem")))
                    .expect("the overwritten file keeps the statement made about it");
            assert!(
                sidecar.starts_with("--p-rwed"),
                "the pure bit survives the overwrite: {sidecar:?}"
            );
            let parsed = crate::core::volume::write::uaem::parse(&sidecar).unwrap();
            assert_eq!(parsed.protection, 0x20);

            let manifest = read_manifest(root);
            let record = manifest
                .files
                .iter()
                .find(|f| f.path == fixtures::OVERWRITTEN_PATH)
                .unwrap();
            assert_eq!(record.component, "test-package");
            assert_eq!(
                record.protection,
                Some(parsed.protection),
                "the manifest and the sidecar must say the same thing"
            );
        }
    }

    /// The other direction, so the rule above is "the medium's statement
    /// wins when it makes one", not "sidecars are never rewritten": a
    /// medium that does state metadata overwrites whatever stood there.
    #[test]
    fn a_medium_that_states_metadata_overwrites_the_previous_sidecar() {
        let (dir, media, packages) = package_dirs("sidecar-wins");
        let root = dir.join("dist");
        let base_only = install_request(&media, &packages, &root, &[]);
        apply(&planned_over(&base_only), &root, &NoProgress).unwrap();

        // A second medium that *does* carry protection bits, writing the
        // same destination.
        let other = fixtures::media(
            &media,
            "Restater",
            "restater.adf",
            &[("C/LoadModule", b"restated", 0x42)],
        );
        let mut sources: BTreeMap<String, Box<dyn MediaSource>> = BTreeMap::new();
        sources.insert(
            "Restater".into(),
            Box::new(AdfSource::open(&other).unwrap()),
        );

        let items = vec![PlanItem {
            component: "restater".into(),
            media: "Restater".into(),
            from: "C/LoadModule".into(),
            to: fixtures::OVERWRITTEN_PATH.into(),
            is_dir: false,
            bytes: 8,
            decompress: false,
        }];
        let mut writer = TreeWriter::new(&root, read_manifest(&root).files);
        writer.place(&items, &mut sources, &NoProgress).unwrap();

        let sidecar = std::fs::read_to_string(root.join("C").join("LoadModule.uaem")).unwrap();
        assert!(sidecar.starts_with("-s--rw-d"), "got {sidecar:?}");
        let record = writer
            .files
            .iter()
            .find(|f| f.path == fixtures::OVERWRITTEN_PATH)
            .unwrap();
        assert_eq!(record.protection, Some(0x42));
    }

    // ---- Task 8: the owner's own packages, onto the owner's own tree ------

    /// One package's collision preview, counted by class.
    ///
    /// `Collision::Identical` has no counter because
    /// `collide::preview` never returns one — it is excluded there by design
    /// (see that type's own doc comment), so a field for it here would
    /// always read `0` and say nothing.
    #[derive(Debug, Default, PartialEq, Eq)]
    struct CollisionTally {
        upgrade: usize,
        downgrade: usize,
        same_version: usize,
        unversioned: usize,
        declared: usize,
    }

    fn tally(reports: &[crate::core::osinstall::collide::CollisionReport]) -> CollisionTally {
        use crate::core::osinstall::collide::Collision;
        let mut out = CollisionTally::default();
        for report in reports {
            if report.declared {
                out.declared += 1;
            }
            match report.collision {
                Collision::Identical => unreachable!("preview() excludes Identical by design"),
                Collision::Upgrade { .. } => out.upgrade += 1,
                Collision::Downgrade { .. } => out.downgrade += 1,
                Collision::SameVersion { .. } => out.same_version += 1,
                Collision::Unversioned { .. } => out.unversioned += 1,
            }
        }
        out
    }

    /// Every non-directory file `package` would place, written out as real
    /// files so `collide::preview` has a `bytes_at` it can read.
    ///
    /// Deliberately a small local copy of what
    /// `commands/osinstall.rs::extract_package_items` does rather than a
    /// call into it: `commands/` sits above `core/` and a `core/` test must
    /// not reach up into it (the core-independence rule). What is shared is
    /// the part that matters — `expand_rules`, the same function `plan()`
    /// and the real preview both call — so this hook cannot resolve a
    /// package's rules differently from the way the product does.
    ///
    /// Returns a `CoreResult` rather than unwrapping: against real material
    /// a package's *bytes* can be unreadable even when its listing resolves
    /// perfectly (ART-166), and a run that panicked here would report one
    /// package's defect and measure neither of the other two.
    fn extract_for_preview(
        package: &crate::core::osinstall::package::Package,
        archive: &Path,
        scratch: &Path,
    ) -> CoreResult<Vec<(String, String, PathBuf)>> {
        let medium = crate::core::osinstall::scan::PackageMedium {
            path: archive.to_path_buf(),
            member: package.member.clone(),
        };
        let mut source = crate::core::osinstall::scan::open_package(&medium)?;
        let mut refusals = Vec::new();
        let items = crate::core::osinstall::plan::expand_rules(
            &package.component,
            source.as_mut(),
            &mut refusals,
        )?;
        assert!(
            refusals.is_empty(),
            "'{}' does not resolve against '{}': {refusals:?} — the archive is right and the \
             recipe is wrong",
            package.id,
            archive.display()
        );
        let mut out = Vec::new();
        for (n, item) in items.iter().filter(|item| !item.is_dir).enumerate() {
            let bytes = source.read(&item.from)?;
            let at = scratch.join(n.to_string());
            std::fs::write(&at, &bytes)?;
            out.push((item.to.clone(), item.component.clone(), at));
        }
        Ok(out)
    }

    /// Which real archive this run should hand [`add_package`] for `package`,
    /// and how that was decided.
    ///
    /// `scan::package_for` is asked first and its answer is always printed —
    /// that answer is one of this run's findings (ART-167: eight of the
    /// owner's archives carry the top-level directory `LocaleUpdate`, and
    /// two carry `BoingBag3.9-2`, so `package_for` correctly refuses both by
    /// name). When it cannot resolve one, the run falls back to an archive
    /// named outright by an environment variable, which is exactly what
    /// [`add_package`]'s own contract allows: "`archive` is given, not
    /// looked up". That keeps the *ambiguity* a measured finding instead of
    /// a dead end, without inventing a resolution rule ART does not have.
    fn archive_for_real_run(
        package: &crate::core::osinstall::package::Package,
        found: &[crate::core::osinstall::scan::FoundPackage],
        named: Option<&str>,
    ) -> Option<PathBuf> {
        use crate::core::osinstall::scan::MediaMatch;
        match crate::core::osinstall::scan::package_for(
            found,
            &package.media,
            package.distinguished_by.as_deref(),
        ) {
            MediaMatch::Found(archive) => {
                println!("  package_for('{}') -> Found", package.media);
                Some(archive.path.clone())
            }
            MediaMatch::Missing => {
                println!("  package_for('{}') -> Missing", package.media);
                named.map(PathBuf::from)
            }
            MediaMatch::Ambiguous(candidates) => {
                println!(
                    "  package_for('{}') -> Ambiguous, {} candidates:",
                    package.media,
                    candidates.len()
                );
                for candidate in &candidates {
                    println!("      {}", candidate.path.display());
                }
                match named {
                    Some(path) => {
                        println!("      run continues with the explicitly named {path}");
                        Some(PathBuf::from(path))
                    }
                    None => None,
                }
            }
        }
    }

    /// **Task 8, the real run.** Builds the base AmigaOS 3.9 tree from the
    /// owner's own disc — with `locale-base` switched on, which
    /// `build_the_real_39_tree_when_asked` does not do — and then adds the
    /// owner's own three real packages to it in dependency order, one at a
    /// time, previewing each against the tree as it actually stands before
    /// writing it.
    ///
    /// ```text
    /// cd src-tauri && ART_OS39_ISO="E:\amiga\Amigatolon\iso\AmigaOS39.iso" \
    ///   ART_OS39_PACKAGES="E:\amiga\Amigatolon\paketler" \
    ///   ART_OS39_PACKAGE_DEST="E:\amiga\ProjeART\dist-3.9-bb" \
    ///   ART_PKG_BOINGBAG_39_2="E:\amiga\Amigatolon\paketler\BoingBag39-2.lha" \
    ///   ART_PKG_LOCALE_TURKISH="E:\amiga\Amigatolon\paketler\BoingBag39-2-turkce.lha" \
    ///   cargo test --release apply_the_real_packages_when_asked -- --nocapture --ignored
    /// ```
    ///
    /// **Release build, deliberately.** The base tree alone takes 20 s
    /// debug against 6.2 s released; a timing reported off a debug run is a
    /// number nobody can use.
    ///
    /// The package folder is the owner's real one — fifty-eight items,
    /// including a 171 MB `.rar` and a 248 MB `.7z` — not a folder holding
    /// only the three archives that have recipes. That `find_packages`
    /// skips what it cannot open rather than failing the whole scan is a
    /// claim only a run against that folder can check, and it holds: 27 of
    /// the 58 identified, in 1.6 s, the `.rar` and the `.7z` among them.
    ///
    /// **Every package is attempted, and a failure is recorded rather than
    /// panicked on.** Two of the three fail against the owner's real
    /// material for two entirely different reasons (ART-166, ART-167); a
    /// run that stopped at the first would have measured neither the second
    /// nor the one that works.
    ///
    /// **Measured on 2026-08-19, release build, against the owner's own
    /// material** (see `.superpowers/sdd/2026-08-19-content-layer/task-8-report.md`
    /// for the verbatim output):
    ///
    /// | | files | drawers | bytes | elapsed | upgrade / downgrade / same / unversioned |
    /// |---|---|---|---|---|---|
    /// | base 3.9, before ART-169 (`workbench-base` + `locale-base`) | 1257 | 156 | 10,003,017 | 10.10 s | — |
    /// | base 3.9, after ART-169 (`…` + `workbench-39`) | 1879 | 181 | 18,813,726 | 17.92 s | — |
    /// | BoingBag 3.9-1 | — | — | — | — | never read — **ART-166** |
    /// | Türkçe catalogs | 36 | 3 | 161,534 | 0.15 s | 0 / 0 / 0 / 0 — **ART-168** |
    /// | BoingBag 3.9-2 | — | — | — | — | never read — **ART-166** |
    ///
    /// Re-run unchanged after ART-169's fix landed (the base row above gains
    /// `workbench-39`, which is `required: true` and so always on): every
    /// package result is byte-for-byte the same, which is what keeps the two
    /// rounds' findings separable.
    ///
    /// This test **fails on purpose** while ART-166 and ART-167 stand: it is
    /// the measurement, and a measurement that passed would be claiming the
    /// owner's own packages reach the tree when two of them do not. The boot
    /// the same tree produced was **ART-169**, now fixed — see
    /// `layer_the_real_39_overlay_when_asked` below.
    ///
    /// **ART-168 is fixed** (`core::lha::entry_path` decodes Latin-1), so the
    /// U+FFFD sweep below should now report none and the Türkçe row's
    /// `0 / 0 / 0 / 0` collision count should turn into a real overlap
    /// against the disc's own `TÜRKÇE` — which is ART-172, the re-measurement
    /// this fix unblocks. The table above is left as measured on 2026-08-19;
    /// it is a record of that run, not a prediction of the next one.
    #[test]
    #[ignore = "touches the user's real media and E:\\amiga\\ProjeART; run explicitly, see the doc comment"]
    fn apply_the_real_packages_when_asked() {
        let (Ok(iso), Ok(packages_at), Ok(dest)) = (
            std::env::var("ART_OS39_ISO"),
            std::env::var("ART_OS39_PACKAGES"),
            std::env::var("ART_OS39_PACKAGE_DEST"),
        ) else {
            return;
        };

        let iso_path = PathBuf::from(&iso);
        let media_folder = iso_path
            .parent()
            .expect("ART_OS39_ISO names a file inside some folder")
            .to_path_buf();
        let package_folder = PathBuf::from(&packages_at);
        let root = PathBuf::from(&dest);

        // ---- the base tree, with `locale-base` on -------------------------
        //
        // `locale-turkish` declares `requires_components: ["locale-base"]`
        // (ART-162): without it, thirty-six catalogs land in a drawer no
        // running system can open. `build_the_real_39_tree_when_asked`
        // leaves it off, so this hook cannot reuse that tree even if one
        // were lying around.
        let request = crate::core::osinstall::plan::InstallRequest {
            packages: Vec::new(),
            package_folder: Some(package_folder.clone()),
            release: "AmigaOS 3.9".to_string(),
            media_folder,
            rom: None,
            chosen: vec!["locale-base".to_string()],
            excluded: Vec::new(),
            destination: root.clone(),
            scan_cache: Default::default(),
        };
        let recipe = crate::core::osinstall::recipe::amigaos_39().unwrap();
        let planned = crate::core::osinstall::plan::plan(&request, &recipe).unwrap();
        assert!(
            planned.refusals.is_empty(),
            "the real plan refused: {:?}",
            planned.refusals
        );
        println!(
            "BASE plan: components_on={:?} items={} total_bytes={}",
            planned.components_on,
            planned.items.len(),
            planned.total_bytes
        );

        let start = std::time::Instant::now();
        let base = apply(&planned, &root, &NoProgress)
            .unwrap_or_else(|err| panic!("the base tree failed to build: {err}"));
        println!(
            "BASE apply: files={} directories={} bytes={} elapsed={:.2}s",
            base.files,
            base.directories,
            base.bytes,
            start.elapsed().as_secs_f64()
        );

        // ---- what is actually in the owner's package folder ---------------
        let scan_start = std::time::Instant::now();
        let found = crate::core::osinstall::scan::find_packages(&package_folder).unwrap();
        println!(
            "find_packages: {} archive(s) identified out of {} entries in {} in {:.2}s",
            found.len(),
            std::fs::read_dir(&package_folder).unwrap().count(),
            package_folder.display(),
            scan_start.elapsed().as_secs_f64()
        );
        for entry in &found {
            println!("  {} -> {}", entry.media, entry.path.display());
        }

        let catalogue = crate::core::osinstall::package::packages().unwrap();
        let ordered = crate::core::osinstall::package::order(&[
            "boingbag-39-1".to_string(),
            "boingbag-39-2".to_string(),
            "locale-turkish".to_string(),
        ])
        .unwrap();
        println!("order={ordered:?}");

        let mut failures: Vec<String> = Vec::new();

        for id in &ordered {
            let package = catalogue
                .iter()
                .find(|p| &p.id == id)
                .expect("order() only returns ids from this catalogue");
            println!("--- {} ('{}') ---", package.id, package.media);

            let named = match id.as_str() {
                "boingbag-39-1" => std::env::var("ART_PKG_BOINGBAG_39_1").ok(),
                "boingbag-39-2" => std::env::var("ART_PKG_BOINGBAG_39_2").ok(),
                "locale-turkish" => std::env::var("ART_PKG_LOCALE_TURKISH").ok(),
                _ => None,
            };
            let Some(archive) = archive_for_real_run(package, &found, named.as_deref()) else {
                failures.push(format!(
                    "'{id}': no archive could be resolved for media '{}'",
                    package.media
                ));
                continue;
            };
            println!("  archive = {}", archive.display());

            // PREVIEW, against the tree as it actually stands right now —
            // before this package is written, and after every earlier one
            // already was.
            let scratch = fixtures::scratch(&format!("real-package-{id}"));
            let extracted = match extract_for_preview(package, &archive, &scratch) {
                Ok(extracted) => extracted,
                Err(err) => {
                    println!("  EXTRACT FAILED: {err}");
                    failures.push(format!("'{id}': its payload cannot be read: {err}"));
                    let _ = std::fs::remove_dir_all(&scratch);
                    continue;
                }
            };
            let incoming: Vec<crate::core::osinstall::collide::Incoming> = extracted
                .iter()
                .map(
                    |(to, component, bytes_at)| crate::core::osinstall::collide::Incoming {
                        to: to.clone(),
                        component: component.clone(),
                        bytes_at,
                    },
                )
                .collect();
            let reports = crate::core::osinstall::collide::preview(&root, &incoming).unwrap();
            let counts = tally(&reports);
            println!(
                "  {id} preview: incoming_files={} rows={} upgrade={} downgrade={} \
                 same-version={} unversioned={} declared={} (identical excluded by design)",
                incoming.len(),
                reports.len(),
                counts.upgrade,
                counts.downgrade,
                counts.same_version,
                counts.unversioned,
                counts.declared
            );
            for report in &reports {
                if matches!(
                    report.collision,
                    crate::core::osinstall::collide::Collision::Downgrade { .. }
                ) {
                    println!("    DOWNGRADE {} {:?}", report.path, report.collision);
                }
            }
            let _ = std::fs::remove_dir_all(&scratch);

            let start = std::time::Instant::now();
            match add_package(&root, package, &archive, &NoProgress) {
                Ok(outcome) => println!(
                    "  {id} apply: files={} directories={} bytes={} elapsed={:.2}s",
                    outcome.files,
                    outcome.directories,
                    outcome.bytes,
                    start.elapsed().as_secs_f64()
                ),
                Err(err) => {
                    println!("  ADD FAILED: {err}");
                    failures.push(format!("'{id}': add_package refused: {err}"));
                    continue;
                }
            }

            // A BoingBag that reports no upgrade at all has not been
            // applied — the brief's own rule, asserted rather than left to
            // a reader of the numbers above.
            if id.starts_with("boingbag") && counts.upgrade == 0 {
                failures.push(format!(
                    "'{id}' reported zero upgrades: it landed on nothing the base tree placed, \
                     which means it was not applied to this system at all"
                ));
            }
        }

        let manifest = read_manifest(&root);
        println!(
            "manifest: {} file record(s) from {} medium/media",
            manifest.files.len(),
            manifest.built_from.len()
        );
        for medium in &manifest.built_from {
            println!("  built_from {}", medium.volume_name);
        }

        // **ART-168.** A name ART could not decode is a name ART invented:
        // U+FFFD is not a character any Amiga archive contains, so every
        // path segment carrying one is a file or drawer whose real name was
        // thrown away on the way in. It cannot be caught by counting — the
        // Turkish pack's own numbers (36 files, 3 drawers, 161,534 bytes)
        // are exactly the same whether its catalogs land in `TÜRKÇE` or
        // beside it in a drawer nothing will ever open — so this walks the
        // finished tree and names them.
        let mut undecodable: Vec<String> = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(read) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in read.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path.clone());
                }
                if entry.file_name().to_string_lossy().contains('\u{fffd}') {
                    undecodable.push(path.display().to_string());
                }
            }
        }
        undecodable.sort();
        println!(
            "undecodable names in the finished tree: {}",
            undecodable.len()
        );
        for name in undecodable.iter().take(10) {
            println!("  {name}");
        }
        if !undecodable.is_empty() {
            failures.push(format!(
                "{} path(s) in the tree carry U+FFFD — a name ART could not decode and \
                 replaced rather than a name any archive holds (first: {})",
                undecodable.len(),
                undecodable[0]
            ));
        }

        assert!(
            failures.is_empty(),
            "the owner's own packages did not all reach the tree:\n  {}",
            failures.join("\n  ")
        );
    }

    // ---- Task 8 fix round 1: ART-169, the 3.9 overlay ---------------------

    /// **ART-169's measurement, and the run that proves the layer landed.**
    ///
    /// `workbench-39` is an overlay over `workbench-base`, not a replacement
    /// for it — see that component's own `_why` notes in
    /// `recipes/amigaos-3.9.json`. This hook builds the tree **twice** from
    /// one real `plan()`:
    ///
    /// - **before** — every item whose component is not `workbench-39`, which
    ///   is exactly the tree ART built before this fix;
    /// - **after** — the whole plan, the real product, which is what gets
    ///   booted.
    ///
    /// Between the two it classifies every file the overlay would land on the
    /// *before* tree, through `collide::classify` — the same function
    /// `collide::preview` calls once it has both sides' bytes in hand.
    /// `preview` itself is not used, and that is a finding rather than a
    /// shortcut — **ART-170**: its `declared` column resolves the incoming
    /// component id through `package::by_id`, so it can only ever be asked
    /// about a **package**. Asked about a recipe component it refuses by
    /// name. The classification is identical; only the `declared` column is
    /// missing, and for a layer inside one recipe `detect_collisions` has
    /// already enforced the same thing at plan time.
    ///
    /// Filtering one real plan rather than planning twice is deliberate:
    /// `workbench-39` is `required: true`, so `resolve_components_on` will
    /// not let `excluded` turn it off (by design — a required component is
    /// not a preference). Hand-building an `InstallPlan` from items a real
    /// `plan()` produced is the same shape `planned()` above already uses,
    /// and every item in the *before* tree is a real item off the real disc.
    ///
    /// ```text
    /// cd src-tauri && ART_OS39_ISO="E:\amiga\Amigatolon\iso\AmigaOS39.iso" \
    ///   ART_OS39_LAYER_BEFORE="E:\amiga\ProjeART\dist-3.9-l0" \
    ///   ART_OS39_LAYER_AFTER="E:\amiga\ProjeART\dist-3.9-l1" \
    ///   cargo test --release layer_the_real_39_overlay_when_asked -- --nocapture --ignored
    /// ```
    #[test]
    #[ignore = "touches the user's real media and E:\\amiga\\ProjeART; run explicitly, see the doc comment"]
    fn layer_the_real_39_overlay_when_asked() {
        use crate::core::osinstall::collide::{classify, Collision};

        let (Ok(iso), Ok(before_at), Ok(after_at)) = (
            std::env::var("ART_OS39_ISO"),
            std::env::var("ART_OS39_LAYER_BEFORE"),
            std::env::var("ART_OS39_LAYER_AFTER"),
        ) else {
            return;
        };

        let iso_path = PathBuf::from(&iso);
        let media_folder = iso_path
            .parent()
            .expect("ART_OS39_ISO names a file inside some folder")
            .to_path_buf();
        let before = PathBuf::from(&before_at);
        let after = PathBuf::from(&after_at);

        let request = crate::core::osinstall::plan::InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.9".to_string(),
            media_folder,
            rom: None,
            chosen: vec!["locale-base".to_string()],
            excluded: Vec::new(),
            destination: after.clone(),
            scan_cache: Default::default(),
        };
        let recipe = crate::core::osinstall::recipe::amigaos_39().unwrap();
        let planned = crate::core::osinstall::plan::plan(&request, &recipe).unwrap();
        assert!(
            planned.refusals.is_empty(),
            "the real plan refused: {:?}",
            planned.refusals
        );
        println!(
            "plan: components_on={:?} items={} total_bytes={}",
            planned.components_on,
            planned.items.len(),
            planned.total_bytes
        );
        assert!(
            planned.components_on.iter().any(|id| id == "workbench-39"),
            "workbench-39 is required — a plan without it is not this recipe"
        );

        let overlay: Vec<PlanItem> = planned
            .items
            .iter()
            .filter(|item| item.component == "workbench-39")
            .cloned()
            .collect();
        let base_items: Vec<PlanItem> = planned
            .items
            .iter()
            .filter(|item| item.component != "workbench-39")
            .cloned()
            .collect();
        println!(
            "items: base={} overlay={} (overlay files={} drawers={})",
            base_items.len(),
            overlay.len(),
            overlay.iter().filter(|i| !i.is_dir).count(),
            overlay.iter().filter(|i| i.is_dir).count()
        );

        // ---- the tree as it stood before ART-169 --------------------------
        let mut before_plan = planned.clone();
        before_plan.items = base_items;
        before_plan.components_on = planned
            .components_on
            .iter()
            .filter(|id| id.as_str() != "workbench-39")
            .cloned()
            .collect();
        let start = std::time::Instant::now();
        let base = apply(&before_plan, &before, &NoProgress)
            .unwrap_or_else(|err| panic!("the before tree failed to build: {err}"));
        println!(
            "BEFORE apply: files={} directories={} bytes={} elapsed={:.2}s",
            base.files,
            base.directories,
            base.bytes,
            start.elapsed().as_secs_f64()
        );

        // ---- what the layer would do to it --------------------------------
        let mut source = crate::core::osinstall::scan::open_media(
            &crate::core::osinstall::scan::identify(&iso_path)
                .expect("the disc must identify as media"),
        )
        .unwrap();
        let (mut upgrade, mut downgrade, mut same_version, mut unversioned) = (0usize, 0, 0, 0);
        let (mut identical, mut brand_new) = (0usize, 0usize);
        let mut downgrades: Vec<String> = Vec::new();
        let mut upgrades: Vec<String> = Vec::new();
        // Named in full, not sampled (fix round 2, F5). An `Unversioned` row
        // is a real overwrite the classifier could not put a number on, and
        // `Libs/WORKBENCH.LIBRARY` — the file `Version FULL` reads — turned
        // out to be one of them: the boot proved the decisive change and the
        // classifier could not, because that library carries no `$VER:`.
        // A count alone would have hidden that.
        let mut unversioned_names: Vec<String> = Vec::new();
        for item in overlay.iter().filter(|item| !item.is_dir) {
            let existing = match std::fs::read(before.join(&item.to)) {
                Ok(bytes) => bytes,
                Err(_) => {
                    brand_new += 1;
                    continue;
                }
            };
            let incoming = source.read(&item.from).unwrap();
            match classify(&existing, &incoming, &item.to) {
                Collision::Identical => identical += 1,
                Collision::Upgrade { from, to } => {
                    upgrade += 1;
                    if upgrades.len() < 8 {
                        upgrades.push(format!("{} {from} -> {to}", item.to));
                    }
                }
                Collision::Downgrade { from, to } => {
                    downgrade += 1;
                    downgrades.push(format!("{} {from} -> {to}", item.to));
                }
                Collision::SameVersion { .. } => same_version += 1,
                Collision::Unversioned {
                    from_bytes,
                    to_bytes,
                } => {
                    unversioned += 1;
                    unversioned_names.push(format!("{} {from_bytes} -> {to_bytes} bytes", item.to));
                }
            }
        }
        println!(
            "LAYER collisions: upgrade={upgrade} downgrade={downgrade} same-version={same_version} \
             unversioned={unversioned} · identical={identical} (excluded by preview's own rule) \
             · new files (no existing destination)={brand_new}"
        );
        for line in &upgrades {
            println!("  UPGRADE {line}");
        }
        for line in &downgrades {
            println!("  DOWNGRADE {line}");
        }
        unversioned_names.sort();
        for line in &unversioned_names {
            println!("  UNVERSIONED {line}");
        }

        // A layer reporting no upgrade at all has not been applied — the same
        // rule this task's package hook applies to a BoingBag, and the one
        // number that cannot be satisfied by a layer that silently landed
        // nowhere.
        assert!(
            upgrade > 0,
            "the 3.9 overlay reported zero upgrades: it landed on nothing the 3.5 layer placed, \
             which means it is not an overlay of it at all"
        );

        // ---- the real product ---------------------------------------------
        let start = std::time::Instant::now();
        let full = apply(&planned, &after, &NoProgress)
            .unwrap_or_else(|err| panic!("the layered tree failed to build: {err}"));
        println!(
            "AFTER apply: files={} directories={} bytes={} elapsed={:.2}s",
            full.files,
            full.directories,
            full.bytes,
            start.elapsed().as_secs_f64()
        );
        println!(
            "delta: files {:+} directories {:+} bytes {:+}",
            full.files as i64 - base.files as i64,
            full.directories as i64 - base.directories as i64,
            full.bytes as i64 - base.bytes as i64
        );

        // ART-169's own evidence, inverted: the command whose absence the boot
        // console reported must now be in the tree, and it must have come off
        // the overlay rather than from nowhere.
        let load_mon_drvs = after.join("C").join("LoadMonDrvs");
        assert!(
            load_mon_drvs.exists(),
            "{} is what ART-169 was: the Startup-Sequence's first command",
            load_mon_drvs.display()
        );

        let manifest = read_manifest(&after);
        let from_overlay = manifest
            .files
            .iter()
            .filter(|f| f.component == "workbench-39")
            .count();
        println!(
            "manifest: {} file record(s), {from_overlay} of them from workbench-39",
            manifest.files.len()
        );
        assert_eq!(
            manifest.files.len(),
            full.files as usize,
            "one record per file in the tree (ART-124)"
        );
    }
}
