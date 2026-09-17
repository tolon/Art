//! Core error type.
//!
//! Kept separate from the Tauri command error (`crate::error::AppError`) so the
//! core engine can be compiled and tested without Tauri.

use std::path::PathBuf;

use thiserror::Error;

/// Why `core::card::sizing::plan_card_image` (or the card preparation's own
/// System re-check) could not lay a card out. Declared here rather than in
/// `core/card` (card round 3, M9): this module sits below every other `core/`
/// module and names a refusal without importing the module that makes it.
/// `core::card::sizing` re-exports it under its old path.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(
    tag = "refusal",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum SizingRefusal {
    DoesNotFit {
        needed: u64,
        available: u64,
        /// The requested partition with the most measured content — `None`
        /// when `content` is empty or none of it was measured.
        largest: Option<String>,
    },
    PartitionTooLarge {
        volume_name: String,
        bytes: u64,
    },
    CardTooSmall {
        card_gb: u32,
    },
    /// A partition's own measured content does not fit the size it is given
    /// — today only System's tree, whose partition size is fixed.
    PartitionContentDoesNotFit {
        volume_name: String,
        needed_blocks: u64,
        available_blocks: u64,
    },
    /// System's tree fits, and what the card would still add to it — WHDLoad
    /// and every Kickstart the collection can answer, each with its `.RTB` —
    /// does not (card round 3, I5). Its own sentence, because the user agrees
    /// to Kickstarts only at the build: "agree to fewer" names a step that
    /// does not exist yet, and what does work is taking the ROM files that
    /// answer them out of the folders ART scans. The partition is always
    /// System, so it is not a field — which also keeps `CoreError` inside
    /// clippy's `result_large_err` budget.
    SystemAdditionsDoNotFit {
        needed_blocks: u64,
        available_blocks: u64,
        /// The tree's own blocks, before anything was added.
        tree_blocks: u64,
        /// The WHDLoad that would be added, as it states itself.
        whdload: Option<String>,
        /// Every Kickstart name that would be added.
        kickstarts: Vec<String>,
    },
}

/// Which of a card build's places a free-space refusal is about — each has
/// its own next step: the image's folder is chosen on the card screen, the
/// scratch folder in Settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpacePlace {
    Image,
    Scratch,
    /// The image and the scratch folder are on one volume, summed.
    ImageAndScratch,
}

/// The most items a card refusal lists by name before it says "and N more".
pub const MAX_NAMED_IN_REFUSAL: usize = 20;

/// `items` joined, at most [`MAX_NAMED_IN_REFUSAL`] of them, then "and N more".
fn listed_capped(items: &[String]) -> String {
    let mut text = items
        .iter()
        .take(MAX_NAMED_IN_REFUSAL)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if items.len() > MAX_NAMED_IN_REFUSAL {
        text.push_str(&format!(
            ", and {} more",
            items.len() - MAX_NAMED_IN_REFUSAL
        ));
    }
    text
}

/// Why ART will not edit an RDB in place (ART-117). Every one is decided
/// before the first byte is written, and each has its own stable code, so a
/// user can act on it and a maintainer can find it (§68).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RdbEditRefusal {
    /// The strict walk could not account for a block (spec decision 5).
    Unaccounted { block: u32, check: String },
    /// `rdb_BadBlockList` names a list, which ART does not edit.
    BadBlocks { first: u32 },
    /// No room below both bounds, and `RDBBlocksHi` could not be raised
    /// (decisions 2 and 12).
    NoRoom {
        needed: u32,
        start: u32,
        free: u32,
        rdb_blocks_hi: u32,
        partition_block: Option<u64>,
        raise: RaiseRefused,
    },
    /// Not an Amiga hunk file, or not a whole number of longwords.
    NotExecutable { detail: String },
    /// Over the per-driver cap, read from metadata before the file is.
    DriverTooLarge { bytes: u64, cap: u64 },
    /// An append whose driver states no `$VER:`.
    DriverNoVersion,
    /// A replace whose file names another program than the card's driver,
    /// or where either side's `$VER:` names no program (spec decision 13).
    /// `None` is "no readable name": a file with no `$VER:` at all never gets
    /// here — that is decision 1's "no `$VER:` plans no step".
    DifferentDriver {
        card: Option<String>,
        file: Option<String>,
    },
    /// A dynamic VHD: a write could grow it, and the journal identifies an
    /// image by size.
    DynamicVhd,
    /// An unfinished operation's journal is beside the image.
    JournalPending { journal: PathBuf },
    /// A journal marked finished is beside the image: the previous operation
    /// was written and verified, and only its journal file was left (ART-117
    /// scoped re-review). The opposite next step to `JournalPending`'s.
    JournalFinished {
        description: String,
        journal: PathBuf,
    },
    /// No backup location was chosen.
    NoBackup,
    /// The chosen backup file already exists (SAFE_CREATE).
    BackupExists { path: PathBuf },
}

/// Why decision 12 could not raise `RDBBlocksHi`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RaiseRefused {
    /// This block above `RDBBlocksHi` is not all zero.
    NonZeroBlock(u32),
    /// The raised range would reach this block, where partitions begin by
    /// `bound`'s reckoning.
    PastPartitionArea { limit: u64, bound: PartitionBound },
    /// The raised range would end past what ART reads of a disk.
    PastWindow { window_blocks: u32 },
}

/// Which of decision 12's three answers to "where do partitions begin" was the
/// lowest, so the sentence can say which one stopped the raise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionBound {
    /// `LowCyl` through the PART's own DosEnvec.
    PartitionEnvec,
    /// `LowCyl` through the RDB's own heads and sectors.
    RdbGeometry,
    /// `rdb_LoCylinder × rdb_CylBlocks`.
    LoCylinder,
}

impl RdbEditRefusal {
    /// The stable id for this refusal.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unaccounted { .. } => "ART-RDB-EDIT-UNACCOUNTED",
            Self::BadBlocks { .. } => "ART-RDB-EDIT-BAD-BLOCKS",
            Self::NoRoom { .. } => "ART-RDB-EDIT-NO-ROOM",
            Self::NotExecutable { .. } => "ART-RDB-EDIT-NOT-EXECUTABLE",
            Self::DriverTooLarge { .. } => "ART-RDB-EDIT-DRIVER-TOO-LARGE",
            Self::DriverNoVersion => "ART-RDB-EDIT-DRIVER-NO-VERSION",
            Self::DifferentDriver { .. } => "ART-RDB-EDIT-DIFFERENT-DRIVER",
            Self::DynamicVhd => "ART-RDB-EDIT-DYNAMIC-VHD",
            Self::JournalPending { .. } => "ART-RDB-EDIT-JOURNAL-PENDING",
            Self::JournalFinished { .. } => "ART-RDB-EDIT-JOURNAL-FINISHED",
            Self::NoBackup => "ART-RDB-EDIT-NO-BACKUP",
            Self::BackupExists { .. } => "ART-RDB-EDIT-BACKUP-EXISTS",
        }
    }
}

impl std::fmt::Display for RaiseRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonZeroBlock(block) => write!(f, "block {block} above it is not empty"),
            Self::PastPartitionArea { limit, bound } => match bound {
                PartitionBound::PartitionEnvec => write!(
                    f,
                    "the first partition begins at block {limit} by its own DosEnvec"
                ),
                PartitionBound::RdbGeometry => write!(
                    f,
                    "the first partition begins at block {limit} by the RDB's own heads and sectors"
                ),
                PartitionBound::LoCylinder => write!(
                    f,
                    "rdb_LoCylinder puts the partitionable area at block {limit}"
                ),
            },
            Self::PastWindow { window_blocks } => write!(
                f,
                "ART reads only the first {window_blocks} blocks of an Amiga disk"
            ),
        }
    }
}

impl std::fmt::Display for RdbEditRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unaccounted { block, check } => write!(
                f,
                "ART edits only an RDB it can account for block by block, and block {block} \
                 fails: {check}. Nothing was written."
            ),
            Self::BadBlocks { first } => write!(
                f,
                "this RDB carries a bad-block list (from block {first}), which ART does not \
                 edit. hst-imager can; back the card up first. Nothing was written."
            ),
            Self::NoRoom {
                needed,
                start,
                free,
                rdb_blocks_hi,
                partition_block,
                raise,
            } => {
                write!(
                    f,
                    "the driver needs {needed} blocks from block {start}, but only {free} are free \
                     below RDBBlocksHi {rdb_blocks_hi}"
                )?;
                if let Some(first) = partition_block {
                    write!(f, " and the first partition begins at block {first}")?;
                }
                write!(
                    f,
                    ", and RDBBlocksHi cannot be raised: {raise}. hst-imager rewrites the whole \
                     RDB and checks neither bound, so back the card up before using it. Nothing \
                     was written."
                )
            }
            Self::NotExecutable { detail } => write!(
                f,
                "the driver is not an Amiga executable: {detail}. An .lha archive has to be \
                 unpacked first — choose the file inside it. Nothing was written."
            ),
            Self::DriverTooLarge { bytes, cap } => write!(
                f,
                "the driver is {bytes} bytes, over the {cap}-byte limit ART and hst-imager set \
                 for a driver in an RDB. Nothing was written."
            ),
            Self::DriverNoVersion => write!(
                f,
                "the driver does not say what version it is ($VER:). AmigaOS keeps the higher of \
                 the version in the RDB and the one already loaded, so ART will not write one it \
                 guessed. Nothing was written."
            ),
            Self::DifferentDriver {
                card: Some(card),
                file: Some(file),
            } => write!(
                f,
                "the card's driver calls itself '{card}' and the file you chose calls itself \
                 '{file}'. ART replaces a driver only with a newer copy of the same one; hst-imager \
                 can replace it — back the card up first. Nothing was written."
            ),
            Self::DifferentDriver {
                card: None,
                file: Some(file),
            } => write!(
                f,
                "the card's driver does not say what it is ($VER:), so ART cannot tell whether \
                 '{file}' is a newer copy of it; hst-imager can replace it — back the card up \
                 first. Nothing was written."
            ),
            Self::DifferentDriver {
                card: Some(card),
                file: None,
            } => write!(
                f,
                "the card's driver calls itself '{card}', and the file you chose states a version \
                 but no program name ($VER:), so ART cannot tell whether it is a newer copy of the \
                 same driver; hst-imager can replace it — back the card up first. Nothing was \
                 written."
            ),
            Self::DifferentDriver {
                card: None,
                file: None,
            } => write!(
                f,
                "neither the card's driver nor the file you chose says which program it is \
                 ($VER:), so ART cannot tell whether the file is a newer copy of the card's \
                 driver; hst-imager can replace it — back the card up first. Nothing was written."
            ),
            Self::DynamicVhd => write!(
                f,
                "this card is a dynamic VHD. ART edits an RDB only in a raw card or HDF: a write \
                 can grow a dynamic VHD, and the undo journal identifies its image by size. \
                 Nothing was written."
            ),
            Self::JournalPending { journal } => write!(
                f,
                "an unfinished operation's journal is waiting beside this image ({}). Undo it \
                 first in the File Manager — ART will not write over it. Nothing was written.",
                journal.display()
            ),
            Self::JournalFinished {
                description,
                journal,
            } => write!(
                f,
                "the previous operation on this image ({description}) finished and was verified; \
                 only its undo journal was left behind, at '{}'. Do not undo it — that would take \
                 the finished change back out. Delete that file (the File Manager offers to), then \
                 run this again. Nothing was written.",
                journal.display()
            ),
            Self::NoBackup => write!(
                f,
                "choose where the RDB backup goes before this runs: ART copies the RDB area \
                 there before it changes a byte. Nothing was written."
            ),
            Self::BackupExists { path } => write!(
                f,
                "'{}' already exists, and ART never overwrites a file. Choose another name for \
                 the RDB backup. Nothing was written.",
                path.display()
            ),
        }
    }
}

/// Errors produced by the Amiga core engine.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("path is not valid UTF-8")]
    NonUtf8Path,

    #[error("unsupported or unknown format: {0}")]
    UnsupportedFormat(String),

    #[error("malformed {format}: {detail}")]
    Malformed { format: String, detail: String },

    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// A destructive operation was refused to protect the original file.
    #[error("operation refused to protect data: {0}")]
    SafetyRefused(String),

    /// A distribution tree destination used `:` or `/` inside one of its own
    /// segments (ART-341). Both are reserved in AmigaDOS since DOS — `:`
    /// separates a device or volume name from the path that follows it, and
    /// `/` means "parent directory" rather than a path separator — so
    /// neither can appear *inside* a file or drawer name; a distribution
    /// tree that carried one only worked by accident, escaped for whatever
    /// host filesystem happened to receive it
    /// (AmigaOS Manual, *AmigaDOS: Working With AmigaDOS*, § Naming
    /// Conventions). Raised in `core::osinstall::apply` before a single byte
    /// is written, the same "before anything is written" shape as
    /// [`SafetyRefused`](Self::SafetyRefused) but its own variant so a
    /// caller can tell "the name itself is illegal" from "the escaped host
    /// names collide".
    #[error(
        "'{path}' cannot be an AmigaDOS name: a colon (:) or a slash (/) is reserved in \
         AmigaDOS file and drawer names. Rename it in the source and run again."
    )]
    AmigaNameReserved { path: String },

    #[error("not yet implemented: {0}")]
    NotImplemented(String),

    /// Every configured repository mirror failed. An online enhancement being
    /// unavailable is not a fault in the user's data (§60/§94) — ART keeps
    /// working, this operation does not.
    #[error("could not reach any configured mirror: {0}")]
    MirrorUnreachable(String),

    /// A download did not match what the repository catalog described. The
    /// file is discarded; nothing reaches the cache or an image.
    #[error("download does not match the catalog: {0}")]
    IntegrityMismatch(String),

    /// The user asked for the operation to stop. Not a fault — the UI should
    /// say "cancelled", not show an error.
    #[error("operation cancelled")]
    Cancelled,

    /// Cancelled, but some of the work is already durable on disk (ART-058).
    ///
    /// The whole-file strategy can be cancelled with nothing left behind: it
    /// works in a buffer and returns before the commit. The block-journal
    /// strategy — the one an image too large to hold in memory uses — cannot.
    /// Each file it copied is its own committed, journalled, verified
    /// operation, already durable in the file before the next one starts, and
    /// they are deliberately left in place rather than rolled back.
    ///
    /// Both used to come back as plain [`Cancelled`](Self::Cancelled), so
    /// somebody who stopped a large install part way through had no way to
    /// learn that some of it is on the volume. The count travels as a number
    /// rather than inside the sentence because the sentence is English and the
    /// UI's is not (§68).
    ///
    /// [`Cancelled`]: Self::Cancelled
    #[error("operation cancelled after writing {files} file(s)")]
    CancelledPartway { files: u64 },

    /// A multi-item operation that **failed** part way, with the items before
    /// the failure already on disk.
    ///
    /// The sibling of [`CancelledPartway`](Self::CancelledPartway), for the
    /// other way a run can stop short. Cancelling had a way to say "some of
    /// this landed" and failing did not, so the residue of a failed
    /// `core::layout::apply` was invisible: the next preview reported it as
    /// ordinary collisions, with nothing saying it was the wreckage of a
    /// previous run (ART-110).
    ///
    /// `placed` travels as a number and `item` as the destination that
    /// refused, so the UI can say both without parsing the sentence — which
    /// is English, and the UI's is not (§68).
    ///
    /// **`placed` counts whole items, and the named one is not among them**
    /// (F8 of the wave-C1 review). An item that failed part way — a tree copy
    /// that stopped halfway down, an unpack that got most of a drawer out —
    /// can have left some of itself at the destination, and no count here
    /// describes that. It is why `item` is carried at all: the number says
    /// what finished, and the name says where to look for what did not.
    #[error(
        "{reason} — this stopped at '{item}', and the {placed} item(s) placed before it are \
         still there"
    )]
    PartiallyApplied {
        placed: u64,
        item: String,
        reason: String,
    },

    /// A name `libpfs3` 0.1.3 cannot round-trip: `writer.rs` writes it with
    /// `name.as_bytes()` (UTF-8) and `ondisk/direntry.rs` reads it back with
    /// `util::latin1_to_string` (one stored byte, one decoded char) — the two
    /// agree only where UTF-8 and Latin-1 coincide, which is exactly the
    /// ASCII range. Outside it the name is corrupted the moment it is read
    /// back, on the real Amiga too, since AmigaDOS itself reads Latin-1.
    ///
    /// Raised by `core::preload::native::copy_in_pfs3`'s own pre-flight pass
    /// — see that module's doc comment (ART-113) — before any of `entries`
    /// reaches `libpfs3`, never partway through a real write. `paths` is
    /// bounded (see `copy_in_pfs3`'s own constant) and `more` is the count of
    /// whatever did not fit, because a refusal that only says "some names are
    /// bad" answers nothing the user can act on. PFS3 only: ART's own FFS
    /// writer (`core/volume/write`) encodes these names correctly, which is
    /// why the message names FFS as the way out. Remove this the day
    /// `libpfs3` gains a way to accept a pre-encoded byte string for a name
    /// instead of a Rust `String`.
    #[error("{}", non_ascii_pfs3_message(paths, *more))]
    NonAsciiPfs3Names { paths: Vec<String>, more: usize },

    /// A name longer than the PFS3 volume can store and find again (ART-314).
    ///
    /// pfs3aio cuts a new name to `fnsize - 1` bytes and cuts a searched-for
    /// name the same way before a compare that needs equal lengths
    /// (`directory.c:1489-1490,721-722`, `assroutines.c:163`), so a longer name
    /// would be listed on the Amiga and never open. ART's format writes
    /// `fnsize` 107; a volume formatted elsewhere may say less, and the limit is
    /// read from the volume. Raised by `core::preload::native` before anything
    /// reaches `libpfs3`, the same shape as
    /// [`NonAsciiPfs3Names`](Self::NonAsciiPfs3Names). **Not a fallback
    /// reason**: hst-imager formats `fnsize` 107 as well.
    #[error("{}", pfs3_names_too_long_message(paths, *more, *max_bytes))]
    Pfs3NamesTooLong {
        paths: Vec<String>,
        more: usize,
        max_bytes: usize,
    },

    /// A distribution tree whose `distribution.json` records at least one file
    /// stored under an escaped host name (`Storage/DOSDrivers/AUX` → `_AUX`)
    /// cannot be copied in by an external tool, because the tool copies the
    /// folder as it finds it and has no way to be told what a file's real
    /// AmigaDOS name is (ART-160).
    ///
    /// `NativeFormatter` handles this case — it reads the manifest and puts
    /// the AmigaDOS name back — so this is a capability gap in the same sense
    /// as [`NonAsciiPfs3Names`](Self::NonAsciiPfs3Names), only in the other
    /// direction: the *fallback* is the one that cannot,
    /// and the default can. Its own variant for the same reason that one has
    /// one — `commands/preload.rs` matches on it to say which tool ran a step
    /// and why — and raised before a single byte is written, never partway.
    ///
    /// `pairs` is `host name → AmigaDOS name`, because a refusal that only
    /// says "some names were escaped" tells the user nothing they can act on.
    #[error(
        "this distribution tree stores {} file(s) or drawer(s) under a name Windows forced \
         ART to change ({}), and hst-imager copies a folder exactly as it finds it — so the \
         Amiga would receive the escaped name. ART's own writer handles this; use it for \
         this step.",
        pairs.len(),
        pairs
            .iter()
            .map(|(host, amiga)| format!("{host} → {amiga}"))
            .collect::<Vec<_>>()
            .join(", ")
    )]
    EscapedNamesNeedNativeCopy { pairs: Vec<(String, String)> },

    /// Two sources for one card partition put the same name at its top —
    /// the same name to AmigaDOS, which compares case-insensitively
    /// (`core::preload::amiga_fold`). Copying both would merge one into the
    /// other or replace it; neither is something to do silently (card round 2).
    #[error(
        "'{first}' and '{second}' both put '{name}' at the top of this partition. Rename one, \
         or give them different partitions."
    )]
    SourceNamesCollide {
        name: String,
        first: String,
        second: String,
    },

    /// Two Amiga names inside one source that Windows would stage under the
    /// same escaped host name (`windows_safe_name`), so neither could be
    /// copied as itself (card round 2).
    ///
    /// `source_path`, not `source`: `thiserror` takes a field called `source`
    /// to be the error's cause and requires it to be an error type.
    ///
    /// `Box<str>` rather than `String`: four `String`s made this the largest
    /// variant and grew every `CoreError` by a word (clippy's
    /// `result_large_err` on `gameindex::igame`); the text is never edited.
    #[error(
        "'{first}' and '{second}' in '{source_path}' would both be staged as '{host}' on Windows, so \
         neither could be copied as itself. Rename one on the Amiga side first."
    )]
    EscapedNamesCollide {
        source_path: Box<str>,
        first: Box<str>,
        second: Box<str>,
        host: Box<str>,
    },

    /// One partition holds both names ART's own PFS3 writer cannot write
    /// (non-ASCII, ART-113) and escaped names only ART's own writer can put
    /// back (ART-160) — so neither writer can copy the whole partition, and
    /// the refusal names both lists (card round 2).
    #[error(
        "{}",
        names_no_writer_message(non_ascii, *non_ascii_more, escaped, *escaped_more)
    )]
    NamesNoWriterCanCopy {
        non_ascii: Vec<String>,
        /// Non-ASCII names past those listed.
        non_ascii_more: usize,
        escaped: Vec<String>,
        /// Escaped names past those listed.
        escaped_more: usize,
    },

    /// The tree's own `S/Startup-Sequence` never runs `S:User-Startup`, so the
    /// block ART would merge there could not execute. Writing it and saying
    /// "installed" would be a confident wrong sentence (spec §7).
    #[error(
        "{} never runs S:User-Startup, so a first-boot block placed there would never execute. \
         Both AmigaOS 3.2 and 3.9 ship a sequence that does; restore that line or use the \
         release's own Startup-Sequence.",
        file.display()
    )]
    FirstBootHookUnreachable { file: PathBuf },

    /// No `distribution.json` — this is not a tree ART built.
    #[error(
        "{} is not a distribution tree ART built (no distribution.json), so ART cannot say what a \
         first boot would run against.",
        tree.display()
    )]
    FirstBootNotATree { tree: PathBuf },

    /// A command the fixed first-boot scripts run is not in the tree's `C/`.
    /// Named so the user can fix it with one file rather than told "cannot".
    #[error("the tree has no C/{command}, which the first boot runs; copy it from the release's own C directory")]
    FirstBootNeedsCommand { command: String },

    /// The input is **well-formed** and larger than a bound ART sets for
    /// itself — ART-158.
    ///
    /// [`Malformed`](Self::Malformed) says "this file is not what it claims
    /// to be", and it was carrying two quite different meanings: a genuinely
    /// damaged ISO9660 disc, and a perfectly valid one holding more than
    /// `core::iso`'s walk caps (`MAX_WALK_ENTRIES` 100,000 entries,
    /// `MAX_WALK_DEPTH` 16 levels) will read. A user seeing
    /// `ART-FORMAT-MALFORMED` on the second one is being told their disc is
    /// broken when the truth is that ART stops early, and the two want
    /// opposite answers: one is "this medium cannot be used", the other is
    /// "ART's limit, which could be raised".
    ///
    /// `subject` names the limit that was hit in ART's own terms (a caller
    /// passes something like `"iso9660 walk"`), `detail` says what it is and
    /// what the consequence would have been. Nothing existing was renumbered
    /// to add this — only the two limit refusals in
    /// `core::osinstall::source_cd::CdSource::open` moved onto it.
    #[error("{subject}: {detail}")]
    LimitExceeded { subject: String, detail: String },

    /// An update package's own encrypted payload did not open with the key
    /// ART's recipe carries for it (ART-166, 2026-09-08).
    ///
    /// **Its own ending, not a `Malformed`.** The archive is not damaged and
    /// ART is not confused about the format: this is *"the file you have is
    /// not the build this key belongs to"*, and its next step is to check
    /// which copy of the package is in the folder — a different sentence and
    /// a different action from "this file is corrupt", which is what
    /// `ART-FORMAT-MALFORMED` tells someone. Collapsing the two is CLAUDE.md's
    /// "endings stay distinct" exactly.
    #[error(
        "the payload inside '{archive}' did not open with the password ART carries for this \
         package — the archive may be a different build of it. Check that the file in your \
         package folder is the published one; ART will not try any other key."
    )]
    PayloadPasswordRefused { archive: String },

    /// ART-319: `libpfs3::error::Error::CommitFailed` — the PFS3 writer that
    /// made this call is locked: either its own commit failed part-way
    /// through, or an earlier call's did (or an earlier call's recovery
    /// attempt could not even read the device back, M1) and this is a
    /// *later* call on that same, already-locked writer
    /// (`vendor/libpfs3/src/writer.rs::guarded`) — never the call whose
    /// commit actually failed, which returns its own original error
    /// unchanged. **Its own ending, not a `Malformed`**, for the same reason
    /// `PayloadPasswordRefused` is: the volume is not necessarily damaged,
    /// and the fix ("reopen it") is different from what a "this file is
    /// corrupt" sentence would tell someone to do. Reopening means building
    /// a new volume from the device, not reusing the one a locked writer's
    /// `into_volume` hands back (M2). **ART does not produce this today**
    /// (final review, I1): `copy_in_pfs3` (`core/preload/native.rs`) returns
    /// at its first `?` on every writer call, so it never makes the second,
    /// already-locked call that would surface it — see ART-319 in
    /// `docs/ISSUES.md`.
    #[error(
        "a write to this PFS3 volume failed and ART could not confirm what is on the disk: \
         reopen it (and check it) before writing to it again"
    )]
    Pfs3WriterLocked,

    /// ART-117: an in-place RDB edit refused before anything was written.
    #[error("{0}")]
    RdbEditRefused(RdbEditRefusal),

    /// ART-117: the RDB backup could not be written; the card was not touched.
    #[error("ART could not write the RDB backup to '{}': {detail}. The card was not touched.", path.display())]
    RdbBackupFailed { path: PathBuf, detail: String },

    /// ART-117: the backup exists, and the edit stopped before its first write.
    #[error(
        "the RDB edit stopped before its first write: {detail}. The card was not touched; the RDB \
         backup is at '{}'.",
        backup.display()
    )]
    RdbEditUntouched { backup: PathBuf, detail: String },

    /// ART-117: the edit was written and then failed or did not verify.
    /// `restored` says whether the journal put the card back.
    #[error("{}", rdb_edit_failed_message(backup, *restored, journal, detail))]
    RdbEditFailed {
        backup: PathBuf,
        restored: bool,
        journal: PathBuf,
        detail: String,
    },

    /// ART-117: the edit was written, verified and synced — the card holds it
    /// — and only its undo journal file was left behind: it could not be
    /// removed (final review I2), or it could neither be marked finished nor
    /// put back as it was, so no block was undone (scoped re-review). **Not a
    /// failed edit**, and its next step is the opposite of
    /// [`RdbEditFailed`](Self::RdbEditFailed)'s `restored: false`: undoing
    /// this journal would take a good edit back out.
    #[error(
        "the RDB edit was written and verified, and the card holds it — but its undo journal at \
         '{}' was left behind: {detail}. The card needs nothing more. Delete that file once ART \
         is closed: it holds the RDB as it was before this edit, so do not undo it in the File \
         Manager, which would take the new driver out again. The RDB backup is at '{}'.",
        journal.display(),
        backup.display()
    )]
    RdbEditJournalLeft {
        backup: PathBuf,
        journal: PathBuf,
        detail: String,
    },

    /// A `<image>.partial` was already on disk when a card build started
    /// (P3). ART removes only a file it created in this run — a `.partial`
    /// left from an earlier one might not have been looked at yet, and this
    /// refusal names it rather than silently building over or deleting it.
    #[error(
        "'{path}' is a half-built card image from an earlier run. ART does not remove a file it \
         did not create in this run: delete it yourself, or choose another image name, and \
         build again."
    )]
    PartialImageExists { path: String },

    /// No PFS3 driver was found in any material folder — a loose `pfs3aio`,
    /// nor an archive holding one — and the caller gave no `explicit` choice
    /// either (`core::cardos::driver::find_pfs3_driver`, R4). `searched`
    /// names every folder looked in, in order; `unreadable` names any file
    /// whose name looked like the driver but which ART could not open or
    /// read, so a refusal never reads as "nothing was there" when something
    /// was and ART simply could not use it.
    #[error("{}", pfs3_driver_not_found_message(searched, unreadable))]
    Pfs3DriverNotFound {
        searched: Vec<String>,
        unreadable: Vec<String>,
    },

    /// `core::cardos::whdload::find_whdload` found nothing anywhere ART
    /// looks, while the card holds at least one title that needs it (P6): a
    /// card whose games cannot start is exactly the confident wrong sentence
    /// CLAUDE.md warns about, so this is a refusal, not a silent card.
    /// `searched` names every material folder and hardfile ART looked in.
    #[error("{}", whdload_not_found_message(*titles, searched))]
    WhdloadNotFound {
        titles: usize,
        searched: Vec<String>,
    },

    /// `core::cardos::kickstarts::place_agreed` was asked to place a name
    /// `KickstartProposal` does not offer: absent from `items` entirely,
    /// present but not `Offer::Supplied`, or `Supplied` with a `Missing`
    /// `.RTB`. Refused before a single byte is written, for every agreed
    /// name in the batch, before any of them is placed.
    ///
    /// **This is what keeps the owner's 2026-08-21 rule true by
    /// construction rather than by discipline**: ART offers, and places
    /// only what the user agreed to *from that offer* — a caller cannot pass
    /// a name that never appeared on the screen and have it land anyway.
    #[error(
        "'{name}' cannot be placed: {why}. Prepare the card again and agree only to what the \
         proposal offers."
    )]
    KickstartNotProposed { name: String, why: String },

    /// One of a card partition's sources cannot be used
    /// (`core::cardos::prepare::measure_card`). `why` is the sentence for the
    /// source's `Unusable` reason, rendered by `core::cardos` — this module
    /// sits below `core/cardos` and does not import its types (ART-339).
    #[error(
        "'{source_path}' in {partition} cannot be used: {why}. Remove it from {partition} or fix \
         it."
    )]
    CardSourceUnusable {
        partition: String,
        source_path: String,
        why: String,
    },

    /// A card partition holds non-ASCII names and no hst-imager is set up:
    /// ART's own PFS3 writer cannot write them (ART-113), and the card
    /// path never writes them wrong silently.
    #[error("{}", card_names_need_hst_message(partition, paths, *more))]
    CardNamesNeedHstImager {
        partition: String,
        paths: Vec<String>,
        more: usize,
    },

    /// The card cannot be laid out: the sizing refusal, said per variant.
    #[error("{}", card_does_not_fit_message(.0))]
    CardDoesNotFit(SizingRefusal),

    /// A place the card build writes to has less free space than it needs.
    #[error("{}", not_enough_space_message(place, *needed, *available, *what))]
    NotEnoughSpace {
        place: String,
        needed: u64,
        available: u64,
        /// Which place it is — each has its own next step (card round 3, M7).
        what: SpacePlace,
    },

    /// The one-button card's last gate (`commands/cardos.rs`): the image ART
    /// just built and filled failed `core::card::health::check_image`.
    /// `checks` names each failed check as the health report does. The image
    /// is not given its final name; the caller says what became of it.
    #[error(
        "the card image failed {failures} of its checks ({checks}), so ART did not finish it. \
         Build it again; if it fails the same way, keep the operation log for a report."
    )]
    CardCheckFailed { failures: usize, checks: String },

    /// hst-imager cannot be used — none was given, the file is missing, or it
    /// does not run — and `partitions` hold names only it can write
    /// (ART-113). Said by name before anything is written (card round 3, I1).
    /// `path` is empty when no hst-imager was given at all.
    #[error("{}", hst_imager_unusable_message(partitions, path, why))]
    HstImagerUnusable {
        partitions: Vec<String>,
        path: String,
        why: String,
    },
}

/// The sentence for [`CoreError::HstImagerUnusable`].
fn hst_imager_unusable_message(partitions: &[String], path: &str, why: &str) -> String {
    let tool = if path.is_empty() {
        "no hst.imager.exe was given to this build".to_string()
    } else {
        format!("'{path}' cannot be used as hst-imager ({why})")
    };
    format!(
        "{} hold names only hst-imager can write (ART-113), and {tool}. Point ART at a working \
         hst.imager.exe in Settings, or rename those names to ASCII, and prepare the card again.",
        listed_capped(partitions)
    )
}

/// The sentence for [`CoreError::NotEnoughSpace`], with the next step for
/// the place it is about.
fn not_enough_space_message(place: &str, needed: u64, available: u64, what: SpacePlace) -> String {
    let choose = match what {
        SpacePlace::Image => "choose another folder for the card image",
        SpacePlace::Scratch => "choose another scratch folder in Settings",
        SpacePlace::ImageAndScratch => {
            "choose another folder for the card image, or another scratch folder in Settings"
        }
    };
    format!(
        "'{place}' has {available} bytes free and this card needs {needed} there. Free some \
         space on that drive, or {choose}, and prepare again."
    )
}

/// The sentence for [`CoreError::CardNamesNeedHstImager`].
fn card_names_need_hst_message(partition: &str, paths: &[String], more: usize) -> String {
    let mut listed = paths.join(", ");
    if more > 0 {
        listed.push_str(&format!(", and {more} more"));
    }
    format!(
        "{partition} holds names ART's own PFS3 writer cannot write yet (ART-113): {listed}. \
         Point ART at hst-imager in Settings, or rename them."
    )
}

/// The sentence for [`CoreError::CardDoesNotFit`], one per refusal.
fn card_does_not_fit_message(refusal: &SizingRefusal) -> String {
    match refusal {
        SizingRefusal::DoesNotFit {
            needed,
            available,
            largest,
        } => {
            let advice = match largest {
                Some(name) => format!(
                    "{name} holds the most; move some of it to another card, or choose a larger \
                     card."
                ),
                None => "Choose a larger card.".to_string(),
            };
            format!(
                "These partitions do not fit this card: they need {needed} bytes and the card \
                 has {available}. {advice}"
            )
        }
        SizingRefusal::PartitionTooLarge { volume_name, bytes } => format!(
            "{volume_name} would need {bytes} bytes, more than one PFS3 partition can hold. \
             Split its sources across two partitions."
        ),
        SizingRefusal::CardTooSmall { card_gb } => format!(
            "A {card_gb} GB card is too small to hold System and the RDB. Choose a larger card."
        ),
        SizingRefusal::PartitionContentDoesNotFit {
            volume_name,
            needed_blocks,
            available_blocks,
        } => format!(
            "{volume_name}'s tree needs {needed_blocks} blocks of data and {volume_name} has room \
             for {available_blocks}. Take something out of the tree and prepare again."
        ),
        SizingRefusal::SystemAdditionsDoNotFit {
            needed_blocks,
            available_blocks,
            tree_blocks,
            whdload,
            kickstarts,
        } => {
            let mut added = Vec::new();
            if let Some(whdload) = whdload {
                added.push(whdload.clone());
            }
            if !kickstarts.is_empty() {
                added.push(format!(
                    "the Kickstarts {} with their .RTB files",
                    listed_capped(kickstarts)
                ));
            }
            let advice = if kickstarts.is_empty() {
                "Take something out of the tree and prepare again."
            } else {
                "ART counts every Kickstart your ROM files can answer, because you choose which \
                 to place only when you build. Take the ROM files that answer some of them out of \
                 your material and Kickstart folders, take out the titles that need them, or take \
                 something out of the tree, and prepare again."
            };
            format!(
                "System needs {needed_blocks} blocks of data with {} added, and has room for \
                 {available_blocks}; its tree alone needs {tree_blocks}. {advice}",
                added.join(" and ")
            )
        }
    }
}

/// The sentence for [`CoreError::NonAsciiPfs3Names`] — pulled out of the
/// `#[error(...)]` attribute because it has to branch on whether `more` is
/// nonzero, which a single format string cannot do cleanly.
fn non_ascii_pfs3_message(paths: &[String], more: usize) -> String {
    let mut msg = format!(
        "{} non-ASCII name(s) cannot be written to a PFS3 volume by this version of libpfs3 \
         (it writes names as UTF-8 and reads them back as Latin-1, so any non-ASCII name is \
         corrupted): {}",
        paths.len() + more,
        paths.join(", ")
    );
    if more > 0 {
        msg.push_str(&format!(", and {more} more"));
    }
    msg.push_str(". Use FFS for this volume instead.");
    msg
}

/// The sentence for [`CoreError::NamesNoWriterCanCopy`].
fn names_no_writer_message(
    non_ascii: &[String],
    non_ascii_more: usize,
    escaped: &[String],
    escaped_more: usize,
) -> String {
    let listed = |names: &[String], more: usize| {
        let mut text = names.join(", ");
        if more > 0 {
            text.push_str(&format!(", and {more} more"));
        }
        text
    };
    format!(
        "This partition holds names the native PFS3 writer cannot write, because they are not \
         ASCII ({}; ART-113), and names Windows forced ART to change, which hst-imager cannot \
         put back ({}; ART-160) — so neither writer can copy all of it. Rename the first set to \
         ASCII, or put the sources holding them on a different partition from the second.",
        listed(non_ascii, non_ascii_more),
        listed(escaped, escaped_more)
    )
}

/// The sentence for [`CoreError::Pfs3NamesTooLong`].
fn pfs3_names_too_long_message(paths: &[String], more: usize, max_bytes: usize) -> String {
    let mut msg = format!(
        "{} name(s) are longer than the {max_bytes} bytes this PFS3 volume can store and find \
         again — the Amiga would list them and never open them: {}",
        paths.len() + more,
        paths.join(", ")
    );
    if more > 0 {
        msg.push_str(&format!(", and {more} more"));
    }
    msg.push_str(". Shorten them before copying.");
    msg
}

/// The sentence for [`CoreError::Pfs3DriverNotFound`] — the folders searched
/// are always said; the archives ART could not read are said only when
/// there were any (most searches never meet one).
fn pfs3_driver_not_found_message(searched: &[String], unreadable: &[String]) -> String {
    let mut msg = format!(
        "No PFS3 driver was found. ART looked for 'pfs3aio' or 'pfs3aio.lha' in: {}. Put \
         pfs3aio.lha (Aminet disk/misc/pfs3aio) in one of your material folders, or choose the \
         driver file yourself.",
        listed_capped(searched)
    );
    if !unreadable.is_empty() {
        msg.push_str(&format!(
            " ART could not read: {}.",
            listed_capped(unreadable)
        ));
    }
    msg
}

/// The sentence for [`CoreError::WhdloadNotFound`].
fn whdload_not_found_message(titles: usize, searched: &[String]) -> String {
    format!(
        "{titles} WHDLoad title(s) are on this card and no WHDLoad was found, so they would not \
         start. ART looked in: {}. Put WHDLoad_usr.lha (from whdload.de) in one of your material \
         folders and prepare again.",
        listed_capped(searched)
    )
}

/// The sentence for [`CoreError::RdbEditFailed`]: two endings, two next steps.
fn rdb_edit_failed_message(
    backup: &std::path::Path,
    restored: bool,
    journal: &std::path::Path,
    detail: &str,
) -> String {
    if restored {
        format!(
            "the RDB edit failed or did not verify, and ART put every block it wrote back as it \
             was: {detail}. The RDB backup is at '{}'.",
            backup.display()
        )
    } else {
        format!(
            "the RDB edit failed and undoing it failed too: {detail}. The undo journal is at '{}' \
             — undo it in the File Manager before using the card. The RDB backup is at '{}'.",
            journal.display(),
            backup.display()
        )
    }
}

impl CoreError {
    /// A short, stable identifier for this class of failure.
    ///
    /// Spec §68: an error the user can act on needs a name, not an opaque code
    /// like `0x80004005`. This is what appears at the end of a message and in
    /// the operation log, so a user can quote it and a maintainer can find it.
    ///
    /// These strings are part of ART's user-facing surface — treat them as
    /// stable once released.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "ART-IO",
            Self::NonUtf8Path => "ART-PATH-ENCODING",
            Self::UnsupportedFormat(_) => "ART-FORMAT-UNSUPPORTED",
            Self::Malformed { .. } => "ART-FORMAT-MALFORMED",
            Self::InvalidInput(_) => "ART-INPUT-INVALID",
            Self::SafetyRefused(_) => "ART-SAFETY-REFUSED",
            Self::AmigaNameReserved { .. } => "ART-AMIGA-NAME-RESERVED",
            Self::NotImplemented(_) => "ART-NOT-IMPLEMENTED",
            Self::MirrorUnreachable(_) => "ART-MIRROR-UNREACHABLE",
            Self::IntegrityMismatch(_) => "ART-INTEGRITY-MISMATCH",
            Self::Cancelled => "ART-CANCELLED",
            Self::CancelledPartway { .. } => "ART-CANCELLED-PARTWAY",
            Self::PartiallyApplied { .. } => "ART-APPLY-PARTIAL",
            Self::NonAsciiPfs3Names { .. } => "ART-PFS3-NON-ASCII-NAME",
            Self::Pfs3NamesTooLong { .. } => "ART-PFS3-NAME-TOO-LONG",
            Self::EscapedNamesNeedNativeCopy { .. } => "ART-ESCAPED-NAME-NEEDS-NATIVE",
            Self::SourceNamesCollide { .. } => "ART-CARD-SOURCE-COLLISION",
            Self::EscapedNamesCollide { .. } => "ART-ESCAPED-NAME-COLLISION",
            Self::NamesNoWriterCanCopy { .. } => "ART-NAMES-NO-WRITER",
            Self::FirstBootHookUnreachable { .. } => "ART-FIRSTBOOT-HOOK-UNREACHABLE",
            Self::FirstBootNotATree { .. } => "ART-FIRSTBOOT-NOT-A-TREE",
            Self::FirstBootNeedsCommand { .. } => "ART-FIRSTBOOT-NEEDS-COMMAND",
            Self::LimitExceeded { .. } => "ART-LIMIT-EXCEEDED",
            Self::PayloadPasswordRefused { .. } => "ART-PAYLOAD-PASSWORD",
            Self::Pfs3WriterLocked => "ART-PFS3-WRITER-LOCKED",
            Self::RdbEditRefused(refusal) => refusal.code(),
            Self::RdbBackupFailed { .. } => "ART-RDB-BACKUP-FAILED",
            Self::RdbEditUntouched { .. } => "ART-RDB-EDIT-UNTOUCHED",
            Self::RdbEditFailed { restored: true, .. } => "ART-RDB-EDIT-ROLLED-BACK",
            Self::RdbEditFailed {
                restored: false, ..
            } => "ART-RDB-EDIT-ROLLBACK-FAILED",
            Self::RdbEditJournalLeft { .. } => "ART-RDB-EDIT-JOURNAL-LEFT",
            Self::PartialImageExists { .. } => "ART-CARD-PARTIAL-EXISTS",
            Self::Pfs3DriverNotFound { .. } => "ART-PFS3-DRIVER-NOT-FOUND",
            Self::WhdloadNotFound { .. } => "ART-WHDLOAD-NOT-FOUND",
            Self::KickstartNotProposed { .. } => "ART-KICKSTART-NOT-PROPOSED",
            Self::CardSourceUnusable { .. } => "ART-CARD-SOURCE-UNUSABLE",
            Self::CardNamesNeedHstImager { .. } => "ART-CARD-NAMES-NEED-HST",
            Self::CardDoesNotFit(_) => "ART-CARD-DOES-NOT-FIT",
            Self::NotEnoughSpace { .. } => "ART-NOT-ENOUGH-SPACE",
            Self::CardCheckFailed { .. } => "ART-CARD-CHECK-FAILED",
            Self::HstImagerUnusable { .. } => "ART-HST-IMAGER-UNUSABLE",
        }
    }

    /// The message a user should see: what went wrong, then how to refer to it.
    ///
    /// Technical detail belongs in the log, not in front of the user (§68).
    pub fn user_message(&self) -> String {
        format!("{self}\n\nError ID: {}", self.code())
    }
}

pub type CoreResult<T> = Result<T, CoreError>;

#[cfg(test)]
mod tests {
    use super::*;

    /// **The sentences the frontend rebuilds in Turkish, pinned from this
    /// side** (ART-060).
    ///
    /// `src/lib/errorText.ts` recognises two of ART's sentences and rebuilds
    /// them from their parts, because a free-text English sentence cannot be
    /// translated — only rebuilt. That recognition is a regex over wording,
    /// and wording gets reworded.
    ///
    /// So both ends pin the same string. Change the sentence and **this**
    /// test fails, pointing at the pattern that has to change with it; the
    /// mirror image lives in `errorText.test.ts`. Neither side can drift
    /// quietly, which is the only property that makes the recogniser
    /// approach honest.
    ///
    /// The producers are `osinstall::apply::refuse_unless_free` and
    /// `amigainstall::packagevol::unpack`. They are called here rather than
    /// having their text copied, so this pins what actually ships.
    #[test]
    fn the_sentences_the_frontend_recognises_are_pinned_here() {
        // 1. An occupied destination. The frontend keys on
        //    `'…' already has something in it`.
        let dir = crate::core::ScratchDir::new("art-error-pin", "occupied");
        std::fs::write(dir.join("something"), b"x").unwrap();
        let err = crate::core::osinstall::apply::refuse_unless_free(dir.path())
            .expect_err("a folder with something in it is refused");
        let text = err.user_message();

        assert!(
            text.starts_with("operation refused to protect data: '"),
            "the frontend keys on this prefix: {text}"
        );
        assert!(
            text.contains("' already has something in it"),
            "the frontend keys on this phrase: {text}"
        );
        assert!(
            text.ends_with("\n\nError ID: ART-SAFETY-REFUSED"),
            "and on this trailer: {text}"
        );

        // 2. **The package-archive sentence.** `errorText.ts`'s second
        //    recogniser keys on `carries no '…' drawer, so it is not the
        //    archive this package's installer lives in; it holds …`, and until
        //    2026-08-24 nothing on this side said so.
        //
        //    It was not unguarded — rewording it failed
        //    `packagevol::tests::an_unrecognised_archive_still_lists_what_it_held`,
        //    which was measured rather than assumed. But that test is named for
        //    something else and its failure names nothing, so somebody
        //    rewording the sentence would fix the literal there and never learn
        //    that a Turkish sentence had just reverted to English. This
        //    module's own claim is that changing either end fails a test
        //    **pointing at the other**; for this sentence it did not.
        let said = crate::core::amigainstall::packagevol::wrong_archive_sentence(
            std::path::Path::new("E:\\dl\\BoingBag39-1-UAE.lha"),
            "BoingBag3.9-1",
            "BoingBag3.9-1-UAE, BoingBag3.9-1-UAE.info",
            &[],
        );
        assert!(
            said.contains("' carries no '"),
            "src/lib/errorText.ts keys on this phrase: {said}"
        );
        assert!(
            said.contains(
                "drawer, so it is not the archive this package's installer lives in; it holds "
            ),
            "src/lib/errorText.ts keys on this phrase: {said}"
        );

        // 3. The trailer's exact shape, which is what `parseError` splits on.
        //    One place writes it, and this is the check that it stays.
        assert_eq!(
            CoreError::NonUtf8Path.user_message(),
            "path is not valid UTF-8\n\nError ID: ART-PATH-ENCODING",
            "the `\\n\\nError ID: ` trailer is the frontend's only structural \
             dependency on ART's formatting"
        );
    }

    #[test]
    fn every_variant_has_a_distinct_code() {
        let errors = [
            CoreError::Io(std::io::Error::other("x")),
            CoreError::NonUtf8Path,
            CoreError::UnsupportedFormat("x".into()),
            CoreError::Malformed {
                format: "adf".into(),
                detail: "x".into(),
            },
            CoreError::InvalidInput("x".into()),
            CoreError::SafetyRefused("x".into()),
            CoreError::AmigaNameReserved { path: "x".into() },
            CoreError::NotImplemented("x".into()),
            CoreError::MirrorUnreachable("x".into()),
            CoreError::IntegrityMismatch("x".into()),
            CoreError::Cancelled,
            CoreError::CancelledPartway { files: 1 },
            CoreError::NonAsciiPfs3Names {
                paths: vec!["x".into()],
                more: 0,
            },
            CoreError::Pfs3NamesTooLong {
                paths: vec!["x".into()],
                more: 0,
                max_bytes: 106,
            },
            CoreError::EscapedNamesNeedNativeCopy {
                pairs: vec![("Storage/DOSDrivers/_AUX".into(), "AUX".into())],
            },
            CoreError::SourceNamesCollide {
                name: "Demos".into(),
                first: "a".into(),
                second: "b".into(),
            },
            CoreError::EscapedNamesCollide {
                source_path: "x.lha".into(),
                first: "AUX".into(),
                second: "_AUX".into(),
                host: "_AUX".into(),
            },
            CoreError::NamesNoWriterCanCopy {
                non_ascii: vec!["français".into()],
                non_ascii_more: 0,
                escaped: vec!["AUX".into()],
                escaped_more: 0,
            },
            CoreError::FirstBootHookUnreachable {
                file: "S/Startup-Sequence".into(),
            },
            CoreError::FirstBootNotATree { tree: "x".into() },
            CoreError::FirstBootNeedsCommand {
                command: "Sort".into(),
            },
            CoreError::LimitExceeded {
                subject: "iso9660 walk".into(),
                detail: "x".into(),
            },
            CoreError::RdbEditRefused(RdbEditRefusal::Unaccounted {
                block: 1,
                check: "x".into(),
            }),
            CoreError::RdbEditRefused(RdbEditRefusal::BadBlocks { first: 1 }),
            CoreError::RdbEditRefused(RdbEditRefusal::NoRoom {
                needed: 1,
                start: 1,
                free: 0,
                rdb_blocks_hi: 1,
                partition_block: None,
                raise: RaiseRefused::NonZeroBlock(2),
            }),
            CoreError::RdbEditRefused(RdbEditRefusal::NotExecutable { detail: "x".into() }),
            CoreError::RdbEditRefused(RdbEditRefusal::DriverTooLarge { bytes: 1, cap: 0 }),
            CoreError::RdbEditRefused(RdbEditRefusal::DriverNoVersion),
            CoreError::RdbEditRefused(RdbEditRefusal::DifferentDriver {
                card: None,
                file: Some("pfs3aio".into()),
            }),
            CoreError::RdbEditRefused(RdbEditRefusal::DynamicVhd),
            CoreError::RdbEditRefused(RdbEditRefusal::JournalPending {
                journal: "x".into(),
            }),
            CoreError::RdbEditRefused(RdbEditRefusal::JournalFinished {
                description: "x".into(),
                journal: "x".into(),
            }),
            CoreError::RdbEditRefused(RdbEditRefusal::NoBackup),
            CoreError::RdbEditRefused(RdbEditRefusal::BackupExists { path: "x".into() }),
            CoreError::RdbBackupFailed {
                path: "x".into(),
                detail: "x".into(),
            },
            CoreError::RdbEditUntouched {
                backup: "x".into(),
                detail: "x".into(),
            },
            CoreError::RdbEditFailed {
                backup: "x".into(),
                restored: true,
                journal: "j".into(),
                detail: "x".into(),
            },
            CoreError::RdbEditFailed {
                backup: "x".into(),
                restored: false,
                journal: "j".into(),
                detail: "x".into(),
            },
            CoreError::RdbEditJournalLeft {
                backup: "x".into(),
                journal: "j".into(),
                detail: "x".into(),
            },
            CoreError::PartialImageExists { path: "x".into() },
            CoreError::Pfs3DriverNotFound {
                searched: vec!["x".into()],
                unreadable: vec![],
            },
            CoreError::WhdloadNotFound {
                titles: 1,
                searched: vec!["x".into()],
            },
            CoreError::KickstartNotProposed {
                name: "x".into(),
                why: "x".into(),
            },
            CoreError::CardSourceUnusable {
                partition: "x".into(),
                source_path: "x".into(),
                why: "x".into(),
            },
            CoreError::CardNamesNeedHstImager {
                partition: "x".into(),
                paths: vec!["x".into()],
                more: 0,
            },
            CoreError::CardDoesNotFit(SizingRefusal::CardTooSmall { card_gb: 1 }),
            CoreError::NotEnoughSpace {
                place: "x".into(),
                needed: 2,
                available: 1,
                what: SpacePlace::Image,
            },
            CoreError::CardCheckFailed {
                failures: 1,
                checks: "x".into(),
            },
            CoreError::HstImagerUnusable {
                partitions: vec!["x".into()],
                path: "x".into(),
                why: "x".into(),
            },
        ];

        let mut codes: Vec<&str> = errors.iter().map(|e| e.code()).collect();
        let total = codes.len();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), total, "error codes must be distinct");
        assert!(codes.iter().all(|c| c.starts_with("ART-")));
    }

    #[test]
    fn user_message_carries_the_id() {
        let e = CoreError::SafetyRefused("the original was not modified".into());
        let msg = e.user_message();
        assert!(msg.contains("the original was not modified"));
        assert!(msg.contains("Error ID: ART-SAFETY-REFUSED"));
    }

    /// Every offending path named, the true total (bounded names plus the
    /// rest), and the one piece of advice the user can act on — FFS. A
    /// version that dropped `more` from the sentence, or reported only
    /// `paths.len()` instead of the real total, would still pass a looser
    /// assertion than this one.
    #[test]
    fn a_non_ascii_pfs3_refusal_names_every_bounded_path_and_the_rest_as_a_count() {
        let err = CoreError::NonAsciiPfs3Names {
            paths: vec!["Locale/Countries/türkçe".into(), "Locale/español".into()],
            more: 22,
        };
        let msg = format!("{err}");
        assert!(msg.contains("Locale/Countries/türkçe"), "{msg}");
        assert!(msg.contains("Locale/español"), "{msg}");
        assert!(
            msg.contains("24"),
            "the real total (2 named + 22 more): {msg}"
        );
        assert!(msg.contains("22 more"), "{msg}");
        assert!(msg.contains("FFS"), "the actionable advice: {msg}");
    }

    /// Card round 3, M8: a refusal lists at most 20 places and counts the
    /// rest, whatever the search covered.
    #[test]
    fn a_card_refusal_lists_at_most_twenty_places_and_counts_the_rest() {
        let searched: Vec<String> = (1..=25).map(|n| format!("E:\\games\\t{n}.hdf")).collect();
        let whd = CoreError::WhdloadNotFound {
            titles: 25,
            searched: searched.clone(),
        }
        .to_string();
        assert!(whd.contains("t20.hdf") && !whd.contains("t21.hdf"), "{whd}");
        assert!(whd.contains("and 5 more"), "{whd}");

        let drv = CoreError::Pfs3DriverNotFound {
            searched: searched.clone(),
            unreadable: searched,
        }
        .to_string();
        assert!(drv.contains("t20.hdf") && !drv.contains("t21.hdf"), "{drv}");
        assert_eq!(drv.matches("and 5 more").count(), 2, "{drv}");
    }

    /// Card round 3, I5: the tree alone and the tree with what the card adds
    /// are two sentences, and only the second sends the user to the ROM files.
    #[test]
    fn system_s_two_refusals_give_their_own_next_step() {
        let tree = CoreError::CardDoesNotFit(SizingRefusal::PartitionContentDoesNotFit {
            volume_name: "System".into(),
            needed_blocks: 900,
            available_blocks: 800,
        })
        .to_string();
        assert!(tree.contains("tree"), "{tree}");
        assert!(!tree.contains("Kickstart"), "{tree}");

        let added = CoreError::CardDoesNotFit(SizingRefusal::SystemAdditionsDoNotFit {
            needed_blocks: 900,
            available_blocks: 800,
            tree_blocks: 700,
            whdload: Some("WHDLoad 20.0".into()),
            kickstarts: vec!["kick34005.A500".into()],
        })
        .to_string();
        assert!(added.starts_with("System needs 900"), "{added}");
        assert!(
            added.contains("WHDLoad 20.0") && added.contains("kick34005.A500"),
            "{added}"
        );
        assert!(added.contains("ROM files"), "{added}");
        assert!(!added.contains("agree to fewer"), "{added}");
    }

    /// Card round 3, M7: the scratch folder is a Settings choice, and a
    /// free-space refusal about it says so.
    #[test]
    fn a_free_space_refusal_names_where_its_place_is_chosen() {
        let msg = |what| {
            CoreError::NotEnoughSpace {
                place: "E:\\scratch".into(),
                needed: 2,
                available: 1,
                what,
            }
            .to_string()
        };
        assert!(
            msg(SpacePlace::Scratch).contains("scratch folder in Settings"),
            "{}",
            msg(SpacePlace::Scratch)
        );
        assert!(!msg(SpacePlace::Image).contains("Settings"));
        assert!(msg(SpacePlace::Image).contains("folder for the card image"));
    }

    /// The other half: no `more` at all must not claim there is any.
    #[test]
    fn a_non_ascii_pfs3_refusal_with_nothing_left_over_says_nothing_left_over() {
        let err = CoreError::NonAsciiPfs3Names {
            paths: vec!["français".into()],
            more: 0,
        };
        let msg = format!("{err}");
        assert!(!msg.contains("more"), "{msg}");
    }
}
