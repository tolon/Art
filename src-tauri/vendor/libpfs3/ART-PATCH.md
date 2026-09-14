# `libpfs3` 0.1.3+art.5 — ART's vendored copy

This directory is `libpfs3` 0.1.3 as published on crates.io, vendored into ART for
[ART-310](../../../docs/ISSUES.md). ART's build uses it through `[patch.crates-io]` in
`src-tauri/Cargo.toml`; the `libpfs3 = "=0.1.3"` pin there names the release it was taken from.

| | |
|---|---|
| Original | `https://static.crates.io/crates/libpfs3/libpfs3-0.1.3.crate`, SHA-256 `02f457ef99a09ddebf56e454c6a25dc3a6860a602c878489f132a4ca3eed4317` |
| Upstream source | `metaneutrons/pfs3` commit `33e9ff6ba8462cc4e434dfb6e2783d91b7dd5b14`, `crates/libpfs3` (the crate's `.cargo_vcs_info.json`) |
| Licence | LGPL-3.0-or-later. `LICENSE` is upstream's own file at that commit, unchanged; the full LGPL-3.0 text is `COPYING.LESSER`; the GPL-3.0 text it builds on is ART's `LICENSE` |
| Modified | 2026-09-13 and 2026-09-14, by ART: `src/format.rs`, `src/writer.rs`, `src/error.rs`, `src/ondisk/mod.rs` and `src/volume.rs`; each file's header says so; 2026-09-14, src/format.rs and src/writer.rs for ART-317; 2026-09-14, src/writer.rs, src/error.rs and src/volume.rs for ART-319 |
| Carried | `src/`, `README.md`, `Cargo.toml` (from `Cargo.toml.orig`: version `0.1.3+art.5`, `[dev-dependencies]` removed), `LICENSE`, `COPYING.LESSER` |
| Not carried | `tests/`: `GPL-3.0-only` headers, 9.3 MB of fixtures, and a dev-dependency (`sevenz-rust` 0.6) with RUSTSEC-2026-0245 and RUSTSEC-2026-0246. ART's own tests prove the patch (`src-tauri/src/core/preload/native.rs`) |

## Changes against 0.1.3

**`src/format.rs`** ([ART-310](../../../docs/ISSUES.md); research
`docs/superpowers/notes/2026-09-11-libpfs3-format-fix-research.md`, design
`docs/superpowers/specs/2026-09-13-art-310-libpfs3-format-fix-design.md`):

1. **SUPERINDEX mode writes the super index level.** One more reserved block is allocated after the
   bitmap index blocks and before the anode index block, which is pfs3aio's order. It is written as
   `SBLKID`, datestamp 1, seqnr 0, `index[0]` = the anode index block, and `rext.superindex[0]` names it.
   0.1.3 pointed `superindex[0]` at the anode index block. hst-imager could not mount such a volume, and
   `libpfs3` could not read its own root directory (`anode 5 not found`).
2. **Anodes 0–4 are reserved at every size**: `clustersize 0, blocknr 0xFFFFFFFF, next 0`, what pfs3aio's
   `AllocAnode` leaves. 0.1.3 left them `(0, 0, 0)`. hst-imager's first new directory then failed
   `ERROR_DISK_FULL`, and a later one was given reserved anode 1.

Written in this crate's own idiom from the on-disk layout, not translated from pfs3aio (BSD-4-Clause) or
any port of it. Reserved-area sizing, option flags, datestamps and everything else are 0.1.3's.

**`src/writer.rs`** ([ART-312](../../../docs/ISSUES.md)):

3. **An operation that allocates two anodes gets two different ones.** `get_anode_block_nr` resolves an
   anode block through the writer's own `read_reserved_raw` — `rootblock.indexblocks` or
   `rext.superindex`, then (SB, then) IB, then the entry — instead of through the volume's cache, which
   read the device. An index entry the same, still-uncommitted operation had set lived only in
   `pending_writes`, so 0.1.3 saw 0, created the anode block again and handed out the same anode number
   twice; the file then read back as its directory's continuation block, with no error.
   `pending_writes` and the commit order are unchanged. The SB read in `alloc_anode_block` still goes
   through the cache; it matters only when one operation allocates two new index blocks in SUPERINDEX
   mode.

Everything else in the writer is 0.1.3's.

**2026-09-14, the debt round** (plan `docs/superpowers/plans/2026-09-14-debt-3-pfs3.md`; pfs3aio read at
`tonioni/pfs3aio` `211f7f0`):

4. **A directory block's `parent` is the directory that holds it ([ART-313](../../../docs/ISSUES.md)).** The
   root directory's block is written with parent 0 (`format.rs`; pfs3aio `format.c:548`), and a continuation
   directory block copies the parent of the directory's existing blocks (`writer.rs` `add_dir_entry`; pfs3aio
   `directory.c:3176,3204`). 0.1.3 wrote the root with 5 and every continuation block with the directory's
   own anode. `libpfs3`'s reader never reads `parent`.
5. **Data blocks stay below the partition's end ([ART-315](../../../docs/ISSUES.md)).** `alloc_data_blocks`
   skips a bitmap bit at or past `disksize` (0.1.3: `disksize + bitmapstart`), `free_data_block` ignores a
   block at or past it, and `load_data_bitmap` sizes the bitmap from `disksize − bitmapstart`. pfs3aio refuses
   `blocknr >= numblocks` (`allocation.c:344`) and leaves the bits past the last block set, so on a volume
   formatted by pfs3aio or hst-imager 0.1.3 could hand out a block past the partition.
6. **A name the volume cannot find again is refused ([ART-314](../../../docs/ISSUES.md)).** The format writes
   `rext.fnsize` 107 (`FORMAT_FNSIZE`; 0.1.3 wrote 32), and every writer call that names an entry returns
   `Error::NameTooLong` for a name longer than `fnsize − 1` bytes (at most 107), before anything is allocated.
   0.1.3 stored up to 107 bytes whatever the volume's `fnsize`, and cut longer names silently. pfs3aio cuts
   every name to `fnsize − 1` on create and on lookup (`directory.c:1489-1490,721-722`) and compares lengths
   first (`assroutines.c:163`), so a longer name lists and never opens.
7. **The anode ceiling is pfs3aio's ([ART-311](../../../docs/ISSUES.md)), part 1.** `alloc_anode` searches
   every anode block the volume can address — 65 536 (pfs3aio's `UWORD` seqnr, `anodes.c:582`), fewer where
   the index levels end — instead of 256. In small mode `alloc_anode_block` makes a missing index block, up
   to `MAXSMALLINDEXNR` (pfs3aio `NewIndexBlock`, `anodes.c:740-761`), and `update_rootblock` writes the
   rootblock's `indexblocks` union; the volume's `AnodeReader` is rebuilt when an index root changes. 0.1.3
   held 253 × 84 − 6 = 21 246 anodes in small mode and 256 × 84 − 6 = 21 498 in SUPERINDEX mode.
8. **The anode ceiling is pfs3aio's ([ART-311](../../../docs/ISSUES.md)), part 2.** In SUPERINDEX mode
   `alloc_anode_block` makes a missing super block, up to `MAXSUPER` (pfs3aio `NewSuperBlock`,
   `anodes.c:851-870`), reads the super block through the writer's own pending writes instead of the cache,
   and numbers a new index block across the volume (`seqnr / index_per_block`, `anodes.c:592-595,767`; 0.1.3
   wrote its position inside its super block); `update_rootblock` writes `rext.superindex` when it changed
   (`update.c:240-256`). `alloc_anode` searches as pfs3aio's `AllocAnode` does (`anodes.c:366-471`): from the
   anode block it last allocated from, skipping blocks it found full (in memory only), starting over from 0
   before it makes a new block; freeing an anode makes its block searchable again. The roving start is not
   written to `rext.curranseqnr`.
9. **`FormatOptions::enable_deldir` makes pfs3aio's deldir ([ART-316](../../../docs/ISSUES.md)).** Two reserved
   blocks after the root directory, each `DD` with its seqnr, protection 5 and the rootblock's creation date;
   `rext.deldir[0..2]`, `deldirsize` 2, `deldirroving` 0; `MODE_DELDIR | MODE_SUPERDELDIR` (pfs3aio `format.c:249-255`,
   `directory.c:4442-4480,4572-4637`). 0.1.3 ignored the option. The writer's deldir path is fixed in item 10,
   and only then does ART format with the option on.
10. **The writer fills the deldir as pfs3aio does ([ART-318](../../../docs/ISSUES.md)).** Deleting a file takes
    the slot at `rext.deldirroving` and advances it modulo `deldirsize × 31`, frees the anodes (not the blocks) of a
    file still in that slot, writes the entry with a length byte before the name, dates the deldir block and
    `rext.dd_creation*`, and frees the deleted file's data blocks while keeping its anodes (pfs3aio
    `directory.c:1799-1830,4489-4564`). The name is at most 17 bytes, as the build pfs3aio's own makefile makes
    it (`makefile:21`, `LARGE_FILE_SIZE=0`, so `DELENTRYFNSIZE` 18, `blocks.h:100-105`) — its last two bytes where
    the struct has `fsizex` — and at most 15 on a `MODE_LARGEFILE` volume, where `fsizex` holds size bits 32-47.
    `DelDirEntry::parse` reads the length byte and treats `fsizex` as name, not size, behind a name longer than
    15 bytes (`directory.c:3688`); a deldir block holds 31 entries at every reserved block size (`blocks.h:611`);
    `undelete` refuses an entry whose blocks have been reused (`IsDelfileValid`, `directory.c:4092-4114`). 0.1.3
    took the first empty slot, wrote the raw name, kept the deleted file's blocks allocated and freed an evicted
    file's blocks. hst-amiga `6b45584` writes the raw name without a length byte (`DelDirEntryWriter.cs:14-20`);
    this follows pfs3aio, the Amiga's handler. Read, not run under pfs3aio.

**2026-09-14, the plan's final whole-branch review** ("With fixes", no Critical findings):

11. **`fsizex` is read as size only on a `MODE_LARGEFILE` volume (M2, `src/ondisk/mod.rs`, `src/volume.rs`,
    `src/writer.rs`).**
    `DelDirEntry::parse` now takes the volume's own `has_largefile()` flag and reads bytes 0x1E-0x1F as size
    bits 32-47 only when it is set — item 10's own gate (a name longer than 15 bytes) was independent of the
    volume's mode, so on an ordinary volume (every one ART formats) a reused slot's stale bytes past a short
    name could be read as a multi-terabyte size. `volume.rs`'s `list_deldir` and `writer.rs`'s `undelete` pass
    it through.
12. **`Writer::open` refuses an unsupported `reserved_blksize` (M3, `src/writer.rs`).** The writer indexes
    fixed offsets that only fit 1024, 2048 or 4096 bytes — the rootblock extension's superindex write reaches
    0x80 (`update_rootblock`), and a deldir block's fixed 31 entries reach byte 1024 (`move_to_deldir`,
    `DELENTRIES_PER_BLOCK`). `Volume` itself only requires `reserved_blksize >= 64`
    (`validate_rbs`); every value between 64 and 1023 reached those writes unrefused and would index past a
    smaller buffer under `panic = "abort"`. `Writer::open` now refuses any other value with a typed
    `Error::Corrupt`, before any of those writes. `Volume`'s own read paths (`list_deldir`, `undelete`'s deldir
    lookup) already bound every index to the buffer they read, so they are unchanged.
13. **The name-limit rule has one copy (M6, `src/format.rs`, `src/writer.rs`).** `format::pfs3_name_limit(fnsize)`
    is now the one place `fnsize - 1, capped at 107` is computed; `writer::Writer::max_name_bytes` and ART's
    `core::preload::native::pfs3_name_limit` both call it instead of each repeating the formula.
14. **Wording only (M5, `src/writer.rs`): "atomic commit" corrected.** The rootblock write pending writes are
    flushed *in place* before, not copy-on-write — a new super block named by the rootblock extension write can
    already be on disk while the on-disk reserved bitmap still marks it free, and the rootblock, written last,
    is a commit *point*, not the moment every earlier write becomes true at once. Not introduced by this
    review: every reserved block ART's writer touches has worked this way since 0.1.3; only the comment was
    wrong.

**`src/format.rs`, `src/writer.rs`** ([ART-317](../../../docs/ISSUES.md); design
`docs/superpowers/specs/2026-09-14-art-317-local-time-design.md`):

15. **The caller may supply "now" ([ART-317](../../../docs/ISSUES.md)).** `FormatOptions::datestamp` sets the
    format's date: the rootblock's creation date, the extension's root date and each new deldir block's date,
    which pfs3aio's `NewDeldirBlock` copies from the rootblock (`directory.c:4477-4479`).
    `Writer::set_entry_date` sets the date of each new or rewritten directory entry (`build_dir_entry`,
    `update_dir_entry_size`), and the date a delete stamps on the deldir block and `rext.dd_creation*`
    (`move_to_deldir`; pfs3aio `AddToDeldir` `DateStamp()`, `directory.c:4556-4560`). A deldir entry's own date
    is still copied from the deleted entry, as pfs3aio does (`:4546-4548`). `None` keeps 0.1.3's
    `current_amiga_datestamp()`, which is UTC. AmigaDOS `DateStamp()` is local time with no zone; ART passes the
    local wall time. pfs3aio read at `211f7f0`, not run. Std-only: the crate still asks no clock but `SystemTime`.

**`src/writer.rs`, `src/error.rs`, `src/volume.rs`** ([ART-319](../../../docs/ISSUES.md); brief
`.superpowers/sdd/2026-09-14-art-319-writer-rollback/brief.md`; research
`D:\Projeler\Amiga\scratch-0913\art319-pfs3aio-research.md`, outside this repository):

16. **The writer returns to its last commit on error, and a commit that fails part-way locks it
    ([ART-319](../../../docs/ISSUES.md)).** Every public mutator (`write_file`, `create_dir`,
    `delete`, `set_volume_name`, `write_file_in`, `create_dir_in`, `create_softlink[_in]`,
    `create_hardlink`, `undelete`, `force_remove_entry`, `repair_blocksfree`,
    `repair_reserved_free`, `overwrite_file_in`, `update_dir_entry_protection`, `rename_in`,
    `delete_in`) now runs through `Writer::guarded`: on `Err`, unless the writer is already
    poisoned, it discards back to the last successful commit — `pending_writes` cleared; the
    rootblock, its extension, `vol.anodes`, `vol.bitmap` and `vol.cache` rebuilt from the device
    through the new `Volume::reload` (the same parse `from_device` does, shared rather than
    duplicated); `res_bitmap` and `data_bm` reloaded (`data_bm` cleared first —
    `load_data_bitmap` pushes); `anode_block_full` cleared; `rext_dirty` cleared. `datestamp`
    stays monotonic (kept, or the disk's own if higher); `anode_roving` (a hint) and `entry_date`
    (caller-set) are untouched. Because every metadata write of this writer goes through
    `write_reserved` into `pending_writes`, only reaching disk in `update_rootblock`'s
    `flush_pending`, the device already holds exactly the last commit for this to read back —
    unlike pfs3aio, which checks free space before allocating and defers every free to its own
    commit point (`allocation.c:242-243,600-608,730`) — the root block write, last, is that
    commit point (`update.c:265-270`) — and, on a failed commit, explicitly undoes the staged
    frees rather than re-reading disk (`UndoFreeList`, `update.c:311-328`; the deferred-free list
    itself, `allocation.c:765-799`, its intent stated in the comment at `allocation.c:741-745`) —
    this crate's writer has neither a check-first nor a deferred-free, so it discards instead. If
    the failure came from inside the commit itself —
    `update_rootblock`'s own rext write, `flush_pending`, or its rootblock cluster write, or
    `set_volume_name`'s direct rootblock write — the device may already be half-written (pending
    writes land in place, not copy-on-write, M5), so `update_rootblock` and `set_volume_name` set
    a `poisoned` flag instead of discarding; every later public mutator then refuses immediately,
    before touching anything, with the new `Error::CommitFailed`, mapped by ART's
    `core::preload::native::from_pfs3` to `CoreError::Pfs3WriterLocked` — its own ending, not a
    `Malformed`, so the sentence says to reopen and check the volume rather than implying the
    file itself is damaged. Out of scope, disclosed in `docs/ISSUES.md` rather than fixed:
    `overwrite_file_in` writes new data over a file's existing blocks before its metadata, so an
    error after that cannot be undone in memory; `rename_in` deletes an existing destination with
    its own commit first, so a failed rename leaves the destination deleted (the volume stays
    consistent, nothing is lost or duplicated). 0.1.3 published nothing until the caller invoked
    a persistence method of its own choosing and had no concept of a failed operation's in-memory
    state at all.

**2026-09-14, ART-319's own final review** ("With fixes", 2 Important, 5 Minor):

17. **Item 16's records over-claimed the user sentence, and its untested-mutator list was wrong
    ([ART-319](../../../docs/ISSUES.md); I1, I2).** `CoreError::Pfs3WriterLocked` /
    `ART-PFS3-WRITER-LOCKED` is what a *subsequent* call on an already-locked writer returns —
    never the call whose own commit failed, which returns its own original error unchanged
    (`guarded` returns `result`, not `Error::CommitFailed`, whenever the operation itself already
    set `poisoned`). ART does not produce it today: `copy_in_pfs3` returns at its first `?` on
    every writer call, so it never makes the second, now-refused call. The CHANGELOG's Unreleased
    line claiming a user would now see this sentence was wrong for the same reason and was
    dropped rather than corrected, since nothing user-visible changed; `docs/ISSUES.md` and the
    doc comments on `Error::CommitFailed` and `CoreError::Pfs3WriterLocked` say so instead. `set_volume_name`
    — its own rootblock-cluster write is a second, separate commit path `update_rootblock` does
    not cover — gained its own lock test, `a_failed_pfs3_set_volume_name_locks_the_writer`; item
    16's list of mutators with no behavioural discard/lock test of their own is corrected in
    `docs/ISSUES.md` to add `create_dir`/`create_dir_in`, `rename_in` and `overwrite_file_in` and
    drop `set_volume_name`, and to note that `repair_reserved_free` is exercised only as the
    second, already-locked call in `a_pfs3_commit_failing_part_way_locks_the_writer`, not for its
    own discard behaviour.
18. **The reload path that locks the writer was untested, and its sentence named only one cause
    ([ART-319](../../../docs/ISSUES.md); M1).** `pfs3_test_device::MemDevice` gained `fail_reads`
    (independent of `with_end`/`fail_from_write`, which affect only writes); a new test,
    `a_reload_that_cannot_read_the_device_locks_the_writer`, covers `discard_to_last_commit`'s own
    `poisoned = true` when its `Volume::reload` cannot even read the device back — reached even
    when the failed operation itself touched the device not at all (a refusal that fires before
    any I/O, e.g. `check_name_len`). Both `Error::CommitFailed` and `CoreError::Pfs3WriterLocked`
    read "a write to this PFS3 volume failed and ART could not confirm what is on the disk: reopen
    it (and check it) before writing to it again" — true of both causes that reach the lock,
    rather than the previous "this PFS3 volume's last write failed part-way through and may be
    half-written", which was wrong when the cause was an unreadable device during the discard and
    nothing had actually been half-written.
19. **Reopening means `Volume::from_device`, not `Writer::open` on a locked writer's own `Volume`
    ([ART-319](../../../docs/ISSUES.md); M2, documentation only).** `Writer::into_volume` on a
    poisoned writer returns a `Volume` that still holds the failed attempt's in-memory rootblock
    and other state; `Writer::open` puts no lock of its own on it, so its next commit would write
    those stale values. `Error::CommitFailed`, `CoreError::Pfs3WriterLocked` and
    `Writer::into_volume`'s own doc comment now say so. ART never reopens a locked writer this way
    today, so nothing here is tested.
20. **Two small corrections ([ART-319](../../../docs/ISSUES.md); M4).**
    `pfs3_test_device::MemDevice::refused()`'s doc now says a `fail_from_write` refusal records
    only the refused call's first block, not the specific out-of-range sector a `with_end`
    refusal records. `a_poisoned_pfs3_writers_error_reaches_the_user_as_a_readable_sentence` had
    never actually been seen red — it passed on its first run because the mapping it pins already
    existed when it was written — so its guard was proven by mutation instead: removing the
    `Error::CommitFailed => CoreError::Pfs3WriterLocked` arm from `from_pfs3` turns it red
    (`docs/ISSUES.md` has the line).

## Re-vendoring

After replacing this directory, run `cargo update -p libpfs3 --precise <version>` in `src-tauri`: Cargo
did not refresh the lock entry on its own when this copy was vendored (2026-09-13). Then
`core::preload::native`'s `the_pinned_version_constant_matches_cargo_toml` and `probe_names_libpfs3`
must pass.

## Upstream

Prepared, not offered. Branch `fix/format-superindex-reserved-anodes` (`6eb44df`, on `main` at `05f50b0`) in
a local clone of `metaneutrons/pfs3`: the same change to `crates/libpfs3/src/format.rs`, two tests in
`tests/format.rs` (seen red on `main`, green on the change), upstream's fmt, clippy, test and deny checks
clean, and a Conventional Commit with no AI attribution, as upstream's `CONTRIBUTING.md` requires. The
owner opens the pull request after a patched volume has been mounted under real pfs3aio. That branch
carries the format change only; the writer change (ART-312) is not prepared for upstream yet. The
2026-09-14 changes (items 4–9) are not prepared for upstream either.

## Diff against 0.1.3

```diff
diff --git a/src/error.rs b/src/error.rs
index 48823c7..f0d728d 100644
--- a/src/error.rs
+++ b/src/error.rs
@@ -1,4 +1,11 @@
 //! Error types for libpfs3.
+//!
+//! Modified by ART on 2026-09-14 (ART-314): the `NameTooLong` variant, for a
+//! name the volume cannot store and find again; (ART-319) the `CommitFailed`
+//! variant, for a writer that has locked itself; on 2026-09-14, the final
+//! review (M1): `CommitFailed`'s sentence covers both causes that reach it,
+//! not only a failed commit. `ART-PATCH.md` in this crate's root says what
+//! and why.
 
 /// Result type alias using the PFS3 [`Error`].
 pub type Result<T> = std::result::Result<T, Error>;
@@ -30,9 +37,40 @@ pub enum Error {
     #[error("already exists: {0}")]
     AlreadyExists(String),
 
+    /// ART-314: a name longer than the volume can store and find again.
+    #[error("name too long: '{name}' is {len} bytes, this volume stores at most {max}")]
+    NameTooLong {
+        name: String,
+        len: usize,
+        max: usize,
+    },
+
     #[error("disk full: {0}")]
     DiskFull(String),
 
+    /// ART-319: the writer has locked itself, and every later mutating call
+    /// refuses immediately with this, before touching anything. Two causes
+    /// reach it: a commit itself failed part-way through — the rootblock
+    /// extension write, flushing the pending reserved-block writes, or the
+    /// rootblock cluster write itself, or `set_volume_name`'s own direct
+    /// write — where writes land in place, not copy-on-write, so the device
+    /// may already hold part of this operation's metadata while the
+    /// rootblock that would make it official does not; or an ordinary
+    /// operation failed for an unrelated reason and `discard_to_last_commit`'s
+    /// own reload could not even read the device back (M1, final review) —
+    /// with nothing safe left to fall back to, that locks the writer too,
+    /// even though nothing of this attempt was written. The sentence below
+    /// is written to be true of both causes, rather than naming only the
+    /// first (M1). Reopening means building a new `Volume` from the device
+    /// (`Volume::from_device` / `open*`), **not** `Writer::open` on the
+    /// `Volume` a poisoned writer's own `into_volume` hands back — that one
+    /// still holds this failed attempt's in-memory state (M2, final review).
+    #[error(
+        "a write to this PFS3 volume failed and ART could not confirm what is on the disk: \
+         reopen it (and check it) before writing to it again"
+    )]
+    CommitFailed,
+
     #[error("corrupt filesystem: {0}")]
     Corrupt(String),
 
diff --git a/src/format.rs b/src/format.rs
index b4acc35..0232248 100644
--- a/src/format.rs
+++ b/src/format.rs
@@ -3,14 +3,24 @@
 //! Creates a new PFS3 filesystem on a block device.
 //! Ported from pfs3aio/format.c and amitools PFSFormat.py.
 //!
+//! Modified by ART on 2026-09-13 (ART-310): the super index level and the
+//! reserved anodes 0-4; on 2026-09-14 (ART-313): the root directory's parent,
+//! (ART-314) names — `rext.fnsize` writes 107, not 32, (ART-316) the deldir;
+//! on 2026-09-14, the final review (M6): `pfs3_name_limit`, the one name-limit
+//! rule `writer::Writer` and ART's `core::preload::native` both call;
+//! on 2026-09-14 (ART-317): a caller-supplied datestamp;
+//! `ART-PATCH.md` in this crate's root says what and why.
+//!
 //! Format sequence:
 //! 1. Write boot block (PFS\1 magic)
 //! 2. Calculate reserved area size
 //! 3. Build rootblock + reserved bitmap
 //! 4. Allocate and write rootblock extension
 //! 5. Allocate and write bitmap index + bitmap blocks
-//! 6. Allocate and write anode index + anode block (with ANODE_ROOTDIR)
+//! 6. Allocate and write the super index block (SUPERINDEX mode only), the
+//!    anode index block and the anode block (anodes 0-4 reserved, ANODE_ROOTDIR)
 //! 7. Write root directory block (empty)
+//! 8. Write the two deldir blocks (enable_deldir only)
 
 use crate::error::{Error, Result};
 use crate::io::BlockDevice;
@@ -21,6 +31,12 @@ use crate::util::current_amiga_datestamp;
 pub struct FormatOptions {
     pub volume_name: String,
     pub enable_deldir: bool,
+    /// The format's datestamp as (days, minutes, ticks) since 1978-01-01: the
+    /// rootblock's creation date (0x0C), the extension's root date (0x10) and
+    /// each new deldir block's date (0x1A), which pfs3aio's `NewDeldirBlock`
+    /// copies from the rootblock. `None` stamps the current time as 0.1.3 did,
+    /// which is UTC. Added by ART for ART-317: AmigaDOS reads it as local time.
+    pub datestamp: Option<(u16, u16, u16)>,
 }
 
 impl Default for FormatOptions {
@@ -28,10 +44,29 @@ impl Default for FormatOptions {
         Self {
             volume_name: "Untitled".into(),
             enable_deldir: false,
+            datestamp: None,
         }
     }
 }
 
+/// ART-314: the `rext.fnsize` a format writes — the length limit, plus one, of
+/// every name on the volume. pfs3aio's own format writes 32 (`format.c:520`)
+/// and cuts every name to `fnsize - 1` bytes; hst-imager formats 107, the
+/// longest this crate's directory entries hold.
+pub const FORMAT_FNSIZE: u16 = 107;
+
+/// ART-314/M6 (final review): the longest name a PFS3 volume with this
+/// `fnsize` can store and find again — pfs3aio cuts every name to
+/// `fnsize - 1` bytes, on create and on lookup, before a compare that needs
+/// equal lengths (`directory.c:1489-1490,721-722`, `assroutines.c:163`), and
+/// this crate's own directory entries hold at most 107 whatever `fnsize`
+/// says. The one rule: `writer::Writer::check_name_len` and ART's
+/// `core::preload::native::pfs3_name_limit` both call it, rather than each
+/// repeating `saturating_sub(1).min(107)` on its own.
+pub fn pfs3_name_limit(fnsize: u16) -> usize {
+    usize::from(fnsize).saturating_sub(1).min(107)
+}
+
 /// Result of a successful format operation.
 #[derive(Debug)]
 pub struct FormatResult {
@@ -105,9 +140,14 @@ pub fn format_with_size(
     if supermode {
         options |= MODE_SUPERINDEX;
     }
+    // ART-316: pfs3aio's format turns the deldir on after making it
+    // (`format.c:249-255`, `MODE_DELDIR | MODE_SUPERDELDIR`).
+    if opts.enable_deldir {
+        options |= MODE_DELDIR | MODE_SUPERDELDIR;
+    }
 
-    // Timestamp (current time as Amiga datestamp)
-    let (cday, cmin, ctick) = current_amiga_datestamp();
+    // Timestamp (current time as Amiga datestamp, unless the caller supplied one)
+    let (cday, cmin, ctick) = opts.datestamp.unwrap_or_else(current_amiga_datestamp);
 
     // Index geometry — same formula as Rootblock::index_per_block()
     let index_per_block = (resblocksize / 4).saturating_sub(3);
@@ -146,13 +186,31 @@ pub fn format_with_size(
         bmi_blocknrs.push(firstreserved + idx * rescluster);
     }
 
-    // 4. Allocate anode index + anode block
+    // 4. Allocate the anode index path. In SUPERINDEX mode pfs3aio's first
+    //    AllocAnode goes through NewSuperBlock before NewIndexBlock, so the
+    //    super index block is allocated first.
+    let sb_blk = if supermode {
+        Some(firstreserved + alloc.alloc()? * rescluster)
+    } else {
+        None
+    };
     let anidx_blk = firstreserved + alloc.alloc()? * rescluster;
     let anode_blk = firstreserved + alloc.alloc()? * rescluster;
 
     // 5. Allocate root directory block
     let rootdir_blk = firstreserved + alloc.alloc()? * rescluster;
 
+    // 5b. ART-316: the deldir's two blocks, allocated after the root directory
+    //     as pfs3aio's SetDeldir(2) does (`format.c:249-253`, `directory.c:4614-4622`).
+    let deldir_blks: Vec<u32> = if opts.enable_deldir {
+        vec![
+            firstreserved + alloc.alloc()? * rescluster,
+            firstreserved + alloc.alloc()? * rescluster,
+        ]
+    } else {
+        Vec::new()
+    };
+
     // Build rootblock + reserved bitmap
     let rb_size = rblkcluster as usize * bs;
     let mut rb_data = vec![0u8; rb_size];
@@ -225,9 +283,21 @@ pub fn format_with_size(
     put_u16(&mut rext, 0x10, cday);
     put_u16(&mut rext, 0x12, cmin);
     put_u16(&mut rext, 0x14, ctick);
-    put_u16(&mut rext, 0x38, 32); // fnsize
-    if supermode {
-        put_u32(&mut rext, 0x40, anidx_blk); // superindex[0]
+    put_u16(&mut rext, 0x38, FORMAT_FNSIZE); // fnsize (ART-314)
+    if let Some(sb_blk) = sb_blk {
+        // superindex[0] names the super index block, never the anode index
+        // block: every reader walks SB -> IB -> AB.
+        put_u32(&mut rext, 0x40, sb_blk); // superindex[0]
+    }
+    // ART-316: pfs3aio's SetDeldir — deldirroving = old size × 31 = 0,
+    // deldirsize = 2, deldir[seqnr] = each block (`directory.c:4467,4627-4633`;
+    // offsets `blocks.h:444-456`).
+    if !deldir_blks.is_empty() {
+        put_u16(&mut rext, 0x34, 0); // deldirroving
+        put_u16(&mut rext, 0x36, deldir_blks.len() as u16); // deldirsize
+        for (seq, &blk) in deldir_blks.iter().enumerate() {
+            put_u32(&mut rext, 0x90 + seq * 4, blk); // deldir[seq]
+        }
     }
     write_reserved_blocks(dev, rext_blk as u64, &rext, rescluster, bs)?;
 
@@ -267,6 +337,17 @@ pub fn format_with_size(
         write_reserved_blocks(dev, bm_blknr as u64, &bm, rescluster, bs)?;
     }
 
+    // Write the super index block, as pfs3aio's NewSuperBlock leaves it:
+    // id SB, seqnr 0, index[0] = the anode index block.
+    if let Some(sb_blk) = sb_blk {
+        let mut sb = vec![0u8; resblocksize as usize];
+        put_u16(&mut sb, 0, SBLKID);
+        put_u32(&mut sb, 4, 1); // datestamp
+        put_u32(&mut sb, 8, 0); // seqnr
+        put_u32(&mut sb, 12, anidx_blk); // index[0] = anode index block
+        write_reserved_blocks(dev, sb_blk as u64, &sb, rescluster, bs)?;
+    }
+
     // Write anode index block
     let mut anidx = vec![0u8; resblocksize as usize];
     put_u16(&mut anidx, 0, IBLKID);
@@ -280,20 +361,44 @@ pub fn format_with_size(
     put_u16(&mut an, 0, ABLKID);
     put_u32(&mut an, 4, 1);
     put_u32(&mut an, 8, 0); // seqnr
+    // Anodes 0..ANODE_ROOTDIR-1 are reserved, as pfs3aio's format leaves them
+    // (AllocAnode: clustersize 0, blocknr 0xffffffff, next 0). An allocator
+    // ported from pfs3aio hands out any anode that reads (0, 0, 0).
+    for nr in 0..ANODE_ROOTDIR as usize {
+        let off = ANODE_BLOCK_HEADER_SIZE + nr * ANODE_SIZE;
+        put_u32(&mut an, off + 4, 0xFFFF_FFFF);
+    }
     let an_off = ANODE_BLOCK_HEADER_SIZE + ANODE_ROOTDIR as usize * ANODE_SIZE;
     put_u32(&mut an, an_off, 1); // clustersize = 1
     put_u32(&mut an, an_off + 4, rootdir_blk); // blocknr
     put_u32(&mut an, an_off + 8, 0); // next = EOF
     write_reserved_blocks(dev, anode_blk as u64, &an, rescluster, bs)?;
 
-    // Write root directory block (empty)
+    // Write root directory block (empty). ART-313: the root's own blocks carry
+    // parent 0 — pfs3aio's format.c:548 `MakeDirBlock(blocknr, anodenr, anodenr, 0, g)`,
+    // and GetParent treats 0 as "this is the root". 0.1.3 wrote ANODE_ROOTDIR here.
     let mut dir = vec![0u8; resblocksize as usize];
     put_u16(&mut dir, 0x00, DBLKID);
     put_u32(&mut dir, 0x04, 1); // datestamp
     put_u32(&mut dir, 0x0C, ANODE_ROOTDIR);
-    put_u32(&mut dir, 0x10, ANODE_ROOTDIR); // parent = self
+    put_u32(&mut dir, 0x10, 0); // parent: none, this is the root
     write_reserved_blocks(dev, rootdir_blk as u64, &dir, rescluster, bs)?;
 
+    // ART-316: each deldir block as pfs3aio's NewDeldirBlock leaves it
+    // (`directory.c:4466-4479`, layout `blocks.h:381-396`): id DD, seqnr,
+    // protection DELENTRY_PROT (5, `blocks.h:607`), the rootblock's creation date.
+    for (seq, &blk) in deldir_blks.iter().enumerate() {
+        let mut dd = vec![0u8; resblocksize as usize];
+        put_u16(&mut dd, 0x00, DELDIRID);
+        put_u32(&mut dd, 0x04, 1); // datestamp
+        put_u32(&mut dd, 0x08, seq as u32); // seqnr
+        put_u32(&mut dd, 0x16, 5); // protection
+        put_u16(&mut dd, 0x1A, cday);
+        put_u16(&mut dd, 0x1C, cmin);
+        put_u16(&mut dd, 0x1E, ctick);
+        write_reserved_blocks(dev, blk as u64, &dd, rescluster, bs)?;
+    }
+
     dev.flush()?;
 
     Ok(FormatResult {
diff --git a/src/ondisk/mod.rs b/src/ondisk/mod.rs
index 9c7259b..fe71c4a 100644
--- a/src/ondisk/mod.rs
+++ b/src/ondisk/mod.rs
@@ -4,6 +4,12 @@
 //! We parse manually via `byteorder` — never transmute raw buffers.
 //!
 //! Reference: pfs3aio/blocks.h, pfs3aio/struct.h
+//!
+//! Modified by ART on 2026-09-14 (ART-318): the deldir entry's name length byte,
+//! `fsizex` behind a long name, 31 entries per deldir block; on 2026-09-14, the
+//! final review (M2): `DelDirEntry::parse` takes the volume's own
+//! `MODE_LARGEFILE` flag and reads `fsizex` as size only on such a volume.
+//! `ART-PATCH.md` in this crate's root says what and why.
 
 mod direntry;
 mod rootblock;
@@ -229,7 +235,16 @@ pub struct DelDirEntry {
 
 impl DelDirEntry {
     /// Parse a deldir entry from 32 bytes. Returns None if slot is empty.
-    pub fn parse(data: &[u8]) -> Option<Self> {
+    ///
+    /// `largefile` is the volume's own `MODE_LARGEFILE` flag
+    /// (`Rootblock::has_largefile`). ART-318/M2 (final review, 2026-09-14):
+    /// `fsizex` holds size bits 32-47 only on a `MODE_LARGEFILE` volume —
+    /// pfs3aio's `GetDDFileSize` ignores it otherwise, whatever the name's
+    /// own length (`directory.c:3688`). Gating on the name length alone (the
+    /// name-length-based guess this replaced) would read a reused slot's
+    /// stale bytes at 0x1E-0x1F as a multi-terabyte size on every ordinary
+    /// volume ART formats.
+    pub fn parse(data: &[u8], largefile: bool) -> Option<Self> {
         if data.len() < 32 {
             return None;
         }
@@ -237,16 +252,21 @@ impl DelDirEntry {
         if anode == 0 {
             return None;
         }
+        // ART-318: a length byte, then the name (pfs3aio `directory.c:4196-4199`).
+        let name_len = usize::from(data[14]).min(DELENTRYFNSIZE - 1);
+        let fsizex = if largefile {
+            u16::from_be_bytes(data[30..32].try_into().unwrap())
+        } else {
+            0
+        };
         Some(Self {
             anode,
             fsize: u32::from_be_bytes(data[4..8].try_into().unwrap()),
             creation_day: u16::from_be_bytes(data[8..10].try_into().unwrap()),
             creation_minute: u16::from_be_bytes(data[10..12].try_into().unwrap()),
             creation_tick: u16::from_be_bytes(data[12..14].try_into().unwrap()),
-            filename: crate::util::latin1_to_string(&data[14..30])
-                .trim_end_matches('\0')
-                .to_string(),
-            fsizex: u16::from_be_bytes(data[30..32].try_into().unwrap()),
+            filename: crate::util::latin1_to_string(&data[15..15 + name_len]),
+            fsizex,
         })
     }
 
@@ -261,9 +281,24 @@ pub const DELDIR_HEADER_SIZE: usize = 32;
 /// Size of one deldir entry.
 pub const DELDIR_ENTRY_SIZE: usize = 32;
 
-/// Number of deldir entries that fit in one reserved block.
+/// Entries in a deldir block, whatever the reserved block size (pfs3aio `blocks.h:611`).
+pub const DELENTRIES_PER_BLOCK: usize = 31;
+/// The highest deldir block number; `rext.deldir` has `MAXDELDIR + 1` slots (`blocks.h:612`).
+pub const MAXDELDIR: usize = 31;
+/// ART-318: a deldir entry's name field, length byte included, in the build
+/// pfs3aio's own makefile makes (`makefile:21`, `LARGE_FILE_SIZE=0`;
+/// `blocks.h:100-105`): a name is at most 17 bytes, its last two where the
+/// struct has `fsizex`.
+pub const DELENTRYFNSIZE: usize = 18;
+/// The same in a `LARGE_FILE_SIZE` build, where `fsizex` holds size bits 32-47
+/// (`blocks.h:100-102,375-378`): a name is at most 15 bytes.
+pub const DELENTRYFNSIZE_LARGE_FILE: usize = 16;
+
+/// Number of deldir entries in one reserved block.
+/// ART-318: pfs3aio keeps 31 at every reserved block size; 0.1.3 computed 63 and 127 past 1024 bytes.
 pub fn deldir_entries_per_block(reserved_blksize: u16) -> usize {
-    (reserved_blksize as usize).saturating_sub(DELDIR_HEADER_SIZE) / DELDIR_ENTRY_SIZE
+    ((reserved_blksize as usize).saturating_sub(DELDIR_HEADER_SIZE) / DELDIR_ENTRY_SIZE)
+        .min(DELENTRIES_PER_BLOCK)
 }
 
 // ---- Big-endian write helpers ----
diff --git a/src/volume.rs b/src/volume.rs
index 757c2f9..2f3f6f3 100644
--- a/src/volume.rs
+++ b/src/volume.rs
@@ -1,4 +1,10 @@
 //! PFS3 volume: top-level read-only access to a PFS3 partition.
+//!
+//! Modified by ART on 2026-09-14, the final review (M2): `list_deldir` passes
+//! the volume's own `MODE_LARGEFILE` flag into `DelDirEntry::parse`; on
+//! 2026-09-14 (ART-319): `from_device`'s own parse is shared with `reload`,
+//! which `writer::Writer` uses to discard back to the last successful commit.
+//! `ART-PATCH.md` in this crate's root says what and why.
 
 use std::path::Path;
 
@@ -35,6 +41,23 @@ impl Volume {
 
     /// Open a PFS3 volume from an already-opened block device.
     pub fn from_device(dev: Box<dyn BlockDevice>) -> Result<Self> {
+        let (rootblock, rootblock_ext, anodes, bitmap, cache) = Self::parse_from(dev.as_ref())?;
+        Ok(Self {
+            dev,
+            cache,
+            rootblock,
+            rootblock_ext,
+            anodes,
+            bitmap,
+        })
+    }
+
+    /// ART-319: `from_device`'s own parse, taking a borrowed device instead
+    /// of consuming a `Box<dyn BlockDevice>` — shared by `from_device` and
+    /// `reload` rather than duplicated.
+    fn parse_from(
+        dev: &dyn BlockDevice,
+    ) -> Result<(Rootblock, Option<RootblockExt>, AnodeReader, BitmapReader, BlockCache)> {
         let mut buf = vec![0u8; 512];
         dev.read_block(ROOTBLOCK, &mut buf)?;
         let rb = Rootblock::parse(&buf)?;
@@ -52,7 +75,7 @@ impl Volume {
         Self::validate_rbs(&rootblock)?;
         let rootblock_ext = if rootblock.has_extension() {
             let rbs = rootblock.reserved_blksize;
-            let data = cache.read_reserved(dev.as_ref(), rootblock.extension as u64, rbs)?;
+            let data = cache.read_reserved(dev, rootblock.extension as u64, rbs)?;
             Some(RootblockExt::parse(data)?)
         } else {
             None
@@ -61,14 +84,25 @@ impl Volume {
         let anodes = AnodeReader::new(&rootblock, rootblock_ext.as_ref());
         let bitmap = BitmapReader::new(&rootblock);
 
-        Ok(Self {
-            dev,
-            cache,
-            rootblock,
-            rootblock_ext,
-            anodes,
-            bitmap,
-        })
+        Ok((rootblock, rootblock_ext, anodes, bitmap, cache))
+    }
+
+    /// ART-319: re-derive `rootblock`, `rootblock_ext`, `anodes`, `bitmap` and
+    /// `cache` from the device in place, without giving up ownership of
+    /// `dev` — the same parse `from_device` does, reused rather than
+    /// duplicated. `writer::Writer::discard_to_last_commit` calls this after
+    /// a mutating call fails: every metadata write of this crate's writer
+    /// goes through `pending_writes`, only reaching disk in
+    /// `update_rootblock`'s `flush_pending`, so the device already holds
+    /// exactly the last successful commit for this to read back.
+    pub(crate) fn reload(&mut self) -> Result<()> {
+        let (rootblock, rootblock_ext, anodes, bitmap, cache) = Self::parse_from(self.dev.as_ref())?;
+        self.rootblock = rootblock;
+        self.rootblock_ext = rootblock_ext;
+        self.anodes = anodes;
+        self.bitmap = bitmap;
+        self.cache = cache;
+        Ok(())
     }
 
     /// Open a PFS3 volume from a file.
@@ -380,6 +414,8 @@ impl Volume {
         };
         let rbs = self.rootblock.reserved_blksize;
         let entries_per_block = deldir_entries_per_block(rbs);
+        // M2 (final review): `fsizex` is size only on a MODE_LARGEFILE volume.
+        let largefile = self.rootblock.has_largefile();
         let mut result = Vec::new();
         for &blk in &rext.deldirblocks {
             if blk == 0 {
@@ -394,7 +430,8 @@ impl Volume {
             for i in 0..entries_per_block {
                 let off = DELDIR_HEADER_SIZE + i * DELDIR_ENTRY_SIZE;
                 if off + DELDIR_ENTRY_SIZE <= data.len()
-                    && let Some(entry) = DelDirEntry::parse(&data[off..off + DELDIR_ENTRY_SIZE])
+                    && let Some(entry) =
+                        DelDirEntry::parse(&data[off..off + DELDIR_ENTRY_SIZE], largefile)
                 {
                     result.push(entry);
                 }
diff --git a/src/writer.rs b/src/writer.rs
index fc692d6..7467ec4 100644
--- a/src/writer.rs
+++ b/src/writer.rs
@@ -6,6 +6,21 @@
 //! - Anode allocation and chain building
 //! - Directory entry creation and removal
 //! - Rootblock update
+//!
+//! Modified by ART on 2026-09-13 (ART-312): `get_anode_block_nr` sees this
+//! writer's own pending writes; on 2026-09-14 (ART-313): a continuation directory
+//! block's parent; (ART-315) the data bitmap's bounds; (ART-314) names — a name
+//! longer than `fnsize - 1` bytes is refused before anything is allocated;
+//! (ART-311) the anode search range, index blocks on demand, super blocks,
+//! the rootblock extension, the roving anode search; (ART-318) the deldir write path;
+//! on 2026-09-14, the final review: `Writer::open` refuses a `reserved_blksize`
+//! other than 1024/2048/4096 (M3); `max_name_bytes` calls the one name-limit
+//! rule now in `format::pfs3_name_limit` (M6);
+//! on 2026-09-14 (ART-317): a caller-supplied datestamp;
+//! on 2026-09-14 (ART-319): every public mutator discards back to the last
+//! successful commit on error, and a commit that fails part-way locks the
+//! writer;
+//! `ART-PATCH.md` in this crate's root says what and why.
 
 use crate::error::{Error, Result};
 use crate::ondisk::*;
@@ -28,8 +43,41 @@ pub struct Writer {
     // Mutable state
     res_bitmap: Vec<u32>,
     data_bm: Vec<(u32, Vec<u32>)>, // (blk_num, longs)
-    /// Pending reserved block writes, flushed atomically before rootblock update.
+    /// Pending reserved block writes, flushed in place before the rootblock
+    /// update — not copy-on-write (M5, final review). Each write lands on
+    /// disk as `flush_pending` runs it; the rootblock, written last, is where
+    /// this became true, not the point every earlier write becomes true at
+    /// once.
     pending_writes: Vec<(u32, Vec<u8>)>,
+    /// ART-311: per anode block, whether a search found no free anode in it —
+    /// pfs3aio's in-memory `anblkbitmap` (`anodes.c:949-960`), inverted. Never on disk.
+    anode_block_full: Vec<bool>,
+    /// ART-311: the anode block the last allocation came from — pfs3aio's
+    /// `curranseqnr` (`anodes.c:465`). In memory only: pfs3aio also saves it in
+    /// the rootblock extension (`update.c:247`) as a hint it recovers from
+    /// (`anodes.c:436-443`); this writer starts each session at 0.
+    anode_roving: u32,
+    /// ART-311: `rootblock_ext.superindex` changed; `update_rootblock` writes the extension.
+    rext_dirty: bool,
+    /// "Now" for this writer, as (days, minutes, ticks): the date of each new or
+    /// rewritten directory entry, and the date a delete stamps on the deldir
+    /// block and `rext.dd_creation*` (pfs3aio `AddToDeldir`, `directory.c:4556-4560`).
+    /// `None` is the current time, as 0.1.3 stamped it (UTC). Added by ART for
+    /// ART-317, whose caller sets local time before each operation. A deldir
+    /// entry's own date is not this: it is copied from the deleted entry.
+    entry_date: Option<(u16, u16, u16)>,
+    /// ART-319: set when a commit itself (`update_rootblock`, or
+    /// `set_volume_name`'s own direct write) failed part-way — the device may
+    /// already be half-written, since a pending write lands in place, not
+    /// copy-on-write (M5) — or when `discard_to_last_commit`'s own reload
+    /// could not even read the device back (M1, final review), leaving
+    /// nothing safe to fall back to. Once set, every later public mutating
+    /// call refuses immediately with `Error::CommitFailed`, before touching
+    /// anything; nothing clears it — the caller must reopen the volume, by
+    /// building a new `Volume` from the device, not by calling `Writer::open`
+    /// on the `Volume` `into_volume` hands back, which still holds this
+    /// failed attempt's in-memory state (M2, final review).
+    poisoned: bool,
 }
 
 impl Writer {
@@ -37,6 +85,19 @@ impl Writer {
     pub fn open(vol: Volume) -> Result<Self> {
         let rb = &vol.rootblock;
         let rbs = rb.reserved_blksize as u32;
+        // M3 (final review): the writer indexes fixed offsets that only fit
+        // these three sizes — the rootblock extension's superindex write
+        // reaches 0x80 (`update_rootblock`), and a deldir block's fixed 31
+        // entries (`DELENTRIES_PER_BLOCK`) reach byte 1024 (`move_to_deldir`).
+        // `Volume` itself only requires >= 64 (`validate_rbs`, read paths are
+        // bounds-checked); the writer needs the stronger refusal here, before
+        // any of those writes, rather than an out-of-bounds index under
+        // `panic = "abort"`.
+        if rbs != 1024 && rbs != 2048 && rbs != 4096 {
+            return Err(Error::Corrupt(format!(
+                "reserved_blksize {rbs} is not 1024, 2048 or 4096 — the writer cannot write this volume"
+            )));
+        }
         let rescluster = rbs / vol.block_size();
         let firstreserved = rb.firstreserved;
         let numreserved = (rb.lastreserved - firstreserved + 1) / rescluster;
@@ -57,6 +118,11 @@ impl Writer {
             res_bitmap: Vec::new(),
             data_bm: Vec::new(),
             pending_writes: Vec::new(),
+            anode_block_full: Vec::new(),
+            anode_roving: 0,
+            rext_dirty: false,
+            entry_date: None,
+            poisoned: false,
             vol,
         };
         w.load_reserved_bitmap()?;
@@ -64,38 +130,163 @@ impl Writer {
         Ok(w)
     }
 
-    /// Consume the writer and return the underlying volume.
+    /// Consume the writer and return the underlying volume. **M2 (final
+    /// review): if this writer is locked (`self.poisoned`), the `Volume`
+    /// returned here still holds the failed attempt's own in-memory
+    /// rootblock and other state** — `Writer::open` on it has no lock of its
+    /// own and its next commit would write those values. Reopening a locked
+    /// volume means building a fresh `Volume` from the device
+    /// (`Volume::from_device` / `open*`), not calling `Writer::open` on the
+    /// value this returns.
     pub fn into_volume(self) -> Volume {
         self.vol
     }
 
+    /// "Now" for this writer, as (days, minutes, ticks): the date of each new or
+    /// rewritten directory entry, and the date a delete stamps on the deldir
+    /// block and `rext.dd_creation*` (pfs3aio `AddToDeldir`, `directory.c:4556-4560`).
+    /// `None` is the current time, as 0.1.3 stamped it (UTC). Added by ART for
+    /// ART-317, whose caller sets local time before each operation. A deldir
+    /// entry's own date is not this: it is copied from the deleted entry.
+    pub fn set_entry_date(&mut self, date: Option<(u16, u16, u16)>) {
+        self.entry_date = date;
+    }
+
+    fn entry_datestamp(&self) -> (u16, u16, u16) {
+        self.entry_date.unwrap_or_else(crate::util::current_amiga_datestamp)
+    }
+
     fn next_datestamp(&mut self) -> u32 {
         self.datestamp += 1;
         self.datestamp
     }
 
+    /// ART-314: the longest name this volume can store and find again — M6
+    /// (final review): the one rule now lives in `format::pfs3_name_limit`,
+    /// which ART's `core::preload::native` calls too.
+    fn max_name_bytes(&self) -> usize {
+        crate::format::pfs3_name_limit(self.vol.fnsize())
+    }
+
+    /// ART-314: refuse a name the volume would store and never find again,
+    /// before anything is allocated. 0.1.3 stored up to 107 bytes and cut the rest.
+    fn check_name_len(&self, name: &str) -> Result<()> {
+        let max = self.max_name_bytes();
+        if name.len() > max {
+            return Err(Error::NameTooLong {
+                name: name.to_string(),
+                len: name.len(),
+                max,
+            });
+        }
+        Ok(())
+    }
+
+    /// ART-319: entry point for every public mutator, `op`. On `Err`, unless
+    /// the failure already locked the writer (`self.poisoned` — set by
+    /// `update_rootblock`/`set_volume_name`'s own commit write, because the
+    /// device may already be half-written), this discards back to the last
+    /// successful commit and returns the original error unchanged. Calling
+    /// this from within an already-guarded call (a path wrapper calling its
+    /// `_in` twin, `rename_in` calling `delete_in`) is harmless: a reload
+    /// twice reads the identical, still-current state a second time, and an
+    /// inner call that already committed (its own `update_rootblock` ran) is
+    /// simply what "the last commit" now is for the outer discard to reload.
+    /// Research (`D:\Projeler\Amiga\scratch-0913\art319-pfs3aio-research.md`,
+    /// pfs3aio `211f7f0`): pfs3aio publishes state only at its own commit
+    /// point, the root block write (`update.c:265-270`), and on a failed
+    /// commit explicitly reverses the in-memory bitmap changes it had staged
+    /// (`UndoFreeList`, `update.c:311-328`; the deferred-free list itself,
+    /// `allocation.c:765-799`, with its intent stated in the comment at
+    /// `allocation.c:741-745`) rather than re-reading the disk — this
+    /// crate's writer discards by re-reading instead, because every write of
+    /// this writer already goes through `pending_writes` and only reaches
+    /// disk at that same commit point (see `discard_to_last_commit` below).
+    fn guarded<T>(&mut self, op: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
+        if self.poisoned {
+            return Err(Error::CommitFailed);
+        }
+        let result = op(self);
+        if result.is_err() && !self.poisoned {
+            self.discard_to_last_commit();
+        }
+        result
+    }
+
+    /// ART-319: rebuild this writer's in-memory state from what the device
+    /// actually holds — the last successful commit. Every metadata write of
+    /// this writer goes through `write_reserved` into `pending_writes`, only
+    /// reaching disk in `update_rootblock`'s `flush_pending`; a failure
+    /// anywhere before that point (`DiskFull` mid `alloc_data_blocks`,
+    /// `move_to_deldir` staging an entry and then `free_data_blocks`
+    /// failing) has therefore changed nothing on disk, so reloading from the
+    /// device discards exactly the failed attempt's in-memory half-state and
+    /// nothing else — the same commit-point discipline pfs3aio's own
+    /// `UpdateDisk`/`UndoFreeList` keeps (`update.c:265-270,311-328`,
+    /// `allocation.c:741-799`; see `guarded`'s own comment above). `datestamp`
+    /// is kept, monotonic, rather than reloaded — the disk's own counter can
+    /// only be lower or equal; `anode_roving` (a hint) and `entry_date`
+    /// (caller-set) are left untouched. **If the device cannot even be
+    /// re-read here, there is nothing safe left to fall back to, so this
+    /// locks the writer instead of leaving it running on state it could not
+    /// refresh (M1, final review) — even when the operation that triggered
+    /// this discard never itself touched the device** (a refusal that fires
+    /// before any I/O, e.g. `check_name_len`).
+    fn discard_to_last_commit(&mut self) {
+        self.pending_writes.clear();
+        if self.vol.reload().is_err() {
+            self.poisoned = true;
+            return;
+        }
+        self.res_bitmap.clear();
+        self.data_bm.clear(); // load_data_bitmap pushes
+        if self.load_reserved_bitmap().is_err() || self.load_data_bitmap().is_err() {
+            self.poisoned = true;
+            return;
+        }
+        self.anode_block_full.clear();
+        self.rext_dirty = false;
+        self.datestamp = self.datestamp.max(self.vol.rootblock.datestamp);
+    }
+
     // ---- High-level API (path-based, for CLI) ----
 
     /// Write a file at the given path. Parent directories must exist.
     pub fn write_file(&mut self, path: &str, data: &[u8]) -> Result<()> {
+        self.guarded(|w| w.write_file_impl(path, data))
+    }
+
+    fn write_file_impl(&mut self, path: &str, data: &[u8]) -> Result<()> {
         let (parent_anode, filename) = self.split_path(path)?;
         self.write_file_in(parent_anode, &filename, data)
     }
 
     /// Create a directory at the given path.
     pub fn create_dir(&mut self, path: &str) -> Result<()> {
+        self.guarded(|w| w.create_dir_impl(path))
+    }
+
+    fn create_dir_impl(&mut self, path: &str) -> Result<()> {
         let (parent_anode, dirname) = self.split_path(path)?;
         self.create_dir_in(parent_anode, &dirname)
     }
 
     /// Delete a file or empty directory at the given path.
     pub fn delete(&mut self, path: &str) -> Result<()> {
+        self.guarded(|w| w.delete_impl(path))
+    }
+
+    fn delete_impl(&mut self, path: &str) -> Result<()> {
         let (parent_anode, name) = self.split_path(path)?;
         self.delete_in(parent_anode, &name)
     }
 
     /// Set the volume name (max 30 characters).
     pub fn set_volume_name(&mut self, name: &str) -> Result<()> {
+        self.guarded(|w| w.set_volume_name_impl(name))
+    }
+
+    fn set_volume_name_impl(&mut self, name: &str) -> Result<()> {
         let name_bytes = name.as_bytes();
         let len = name_bytes.len().min(30);
         self.vol.rootblock.diskname = name[..len].to_string();
@@ -114,16 +305,32 @@ impl Writer {
         cluster[RB_OFF_DISKNAME + 1..RB_OFF_DISKNAME + 1 + len].copy_from_slice(&name_bytes[..len]);
         let ds = self.next_datestamp();
         put_u32(&mut cluster, RB_OFF_DATESTAMP, ds);
-        self.vol
+        // ART-319: this writes the rootblock cluster directly — its own
+        // commit point, not routed through `update_rootblock` — so a failure
+        // here is treated the same way: the device may already hold a
+        // half-written sector, so this locks the writer rather than letting
+        // `guarded` discard and continue.
+        if let Err(e) = self
+            .vol
             .dev
-            .write_blocks(self.firstreserved as u64, rblkcluster, &cluster)?;
-        self.vol.dev.flush()
+            .write_blocks(self.firstreserved as u64, rblkcluster, &cluster)
+            .and_then(|()| self.vol.dev.flush())
+        {
+            self.poisoned = true;
+            return Err(e);
+        }
+        Ok(())
     }
 
     // ---- Anode-based API (for FUSE) ----
 
     /// Create a file in a directory identified by anode.
     pub fn write_file_in(&mut self, parent_anode: u32, name: &str, data: &[u8]) -> Result<()> {
+        self.guarded(|w| w.write_file_in_impl(parent_anode, name, data))
+    }
+
+    fn write_file_in_impl(&mut self, parent_anode: u32, name: &str, data: &[u8]) -> Result<()> {
+        self.check_name_len(name)?;
         // Check if file already exists — if so, overwrite it
         if let Ok((_, entry_data, pos)) = self.find_dir_entry(parent_anode, name) {
             let entry_type = entry_data[pos + 1] as i8;
@@ -144,6 +351,7 @@ impl Writer {
         name: &str,
         data: &[u8],
     ) -> Result<()> {
+        self.check_name_len(name)?;
         let bs = self.vol.block_size() as usize;
         let num_blocks = data.len().div_ceil(bs).max(1);
 
@@ -165,6 +373,11 @@ impl Writer {
 
     /// Create a directory in a parent identified by anode. Returns the new dir's anode number.
     pub fn create_dir_in(&mut self, parent_anode: u32, name: &str) -> Result<()> {
+        self.guarded(|w| w.create_dir_in_impl(parent_anode, name))
+    }
+
+    fn create_dir_in_impl(&mut self, parent_anode: u32, name: &str) -> Result<()> {
+        self.check_name_len(name)?;
         let dir_blk = self.alloc_reserved_block()?;
         let anodenr = self.alloc_anode(1, dir_blk, 0)?;
 
@@ -181,6 +394,10 @@ impl Writer {
 
     /// Create a softlink in a parent directory.
     pub fn create_softlink(&mut self, path: &str, target: &str) -> Result<()> {
+        self.guarded(|w| w.create_softlink_impl(path, target))
+    }
+
+    fn create_softlink_impl(&mut self, path: &str, target: &str) -> Result<()> {
         let (parent_anode, name) = self.split_path(path)?;
         self.create_softlink_in(parent_anode, &name, target)
     }
@@ -192,6 +409,16 @@ impl Writer {
         name: &str,
         target: &str,
     ) -> Result<()> {
+        self.guarded(|w| w.create_softlink_in_impl(parent_anode, name, target))
+    }
+
+    fn create_softlink_in_impl(
+        &mut self,
+        parent_anode: u32,
+        name: &str,
+        target: &str,
+    ) -> Result<()> {
+        self.check_name_len(name)?;
         let data = target.as_bytes();
         let bs = self.vol.block_size() as usize;
         let num_blocks = data.len().div_ceil(bs).max(1);
@@ -220,13 +447,22 @@ impl Writer {
 
     /// Create a hardlink in a parent directory.
     pub fn create_hardlink(&mut self, path: &str, target_anode: u32) -> Result<()> {
+        self.guarded(|w| w.create_hardlink_impl(path, target_anode))
+    }
+
+    fn create_hardlink_impl(&mut self, path: &str, target_anode: u32) -> Result<()> {
         let (parent_anode, name) = self.split_path(path)?;
+        self.check_name_len(&name)?;
         self.add_dir_entry(parent_anode, &name, ST_LINKFILE, target_anode, 0, 0)?;
         self.update_rootblock()
     }
 
     /// Undelete a file from the deldir by index. Writes it to `dest_path`.
     pub fn undelete(&mut self, deldir_idx: usize, dest_path: &str) -> Result<()> {
+        self.guarded(|w| w.undelete_impl(deldir_idx, dest_path))
+    }
+
+    fn undelete_impl(&mut self, deldir_idx: usize, dest_path: &str) -> Result<()> {
         // Read the deldir entry
         let rext = self
             .vol
@@ -256,7 +492,9 @@ impl Writer {
         let blk = deldirblocks[block_idx];
         let data = self.read_reserved_raw(blk)?;
         let off = DELDIR_HEADER_SIZE + slot_idx * DELDIR_ENTRY_SIZE;
-        let entry = DelDirEntry::parse(&data[off..off + DELDIR_ENTRY_SIZE])
+        // M2 (final review): `fsizex` is size only on a MODE_LARGEFILE volume.
+        let largefile = self.vol.rootblock.has_largefile();
+        let entry = DelDirEntry::parse(&data[off..off + DELDIR_ENTRY_SIZE], largefile)
             .ok_or_else(|| Error::NotFound("empty deldir slot".into()))?;
 
         // Check destination doesn't already exist
@@ -266,6 +504,23 @@ impl Writer {
 
         let old_anode = entry.anode;
 
+        // ART-318: a delete frees the file's blocks, so a later write may have
+        // taken them; pfs3aio refuses such an entry (`IsDelfileValid`,
+        // `directory.c:4092-4114`).
+        let chain = self
+            .vol
+            .anodes
+            .get_chain(old_anode, self.vol.dev.as_ref(), &mut self.vol.cache)?;
+        let reused = chain
+            .iter()
+            .any(|an| (0..an.clustersize).any(|i| !self.data_block_is_free(an.blocknr + i)));
+        if reused {
+            return Err(Error::NotFound(format!(
+                "deleted file '{}': its blocks have been reused",
+                entry.filename
+            )));
+        }
+
         // Read file data via the anode chain (still intact)
         let file_data = self.vol.read_file_data(old_anode, entry.file_size())?;
 
@@ -290,18 +545,30 @@ impl Writer {
     /// Force-remove a directory entry without touching anodes or data blocks.
     /// Used by check --repair for entries with broken anode chains.
     pub fn force_remove_entry(&mut self, parent_anode: u32, name: &str) -> Result<()> {
+        self.guarded(|w| w.force_remove_entry_impl(parent_anode, name))
+    }
+
+    fn force_remove_entry_impl(&mut self, parent_anode: u32, name: &str) -> Result<()> {
         self.remove_dir_entry(parent_anode, name)?;
         self.update_rootblock()
     }
 
     /// Repair: set the rootblock's blocksfree field.
     pub fn repair_blocksfree(&mut self, correct_free: u32) -> Result<()> {
+        self.guarded(|w| w.repair_blocksfree_impl(correct_free))
+    }
+
+    fn repair_blocksfree_impl(&mut self, correct_free: u32) -> Result<()> {
         self.vol.rootblock.blocksfree = correct_free;
         self.update_rootblock()
     }
 
     /// Repair: set the rootblock's reserved_free field.
     pub fn repair_reserved_free(&mut self, correct_free: u32) -> Result<()> {
+        self.guarded(|w| w.repair_reserved_free_impl(correct_free))
+    }
+
+    fn repair_reserved_free_impl(&mut self, correct_free: u32) -> Result<()> {
         self.vol.rootblock.reserved_free = correct_free;
         self.update_rootblock()
     }
@@ -314,6 +581,16 @@ impl Writer {
         name: &str,
         file_anode: u32,
         data: &[u8],
+    ) -> Result<()> {
+        self.guarded(|w| w.overwrite_file_in_impl(parent_anode, name, file_anode, data))
+    }
+
+    fn overwrite_file_in_impl(
+        &mut self,
+        parent_anode: u32,
+        name: &str,
+        file_anode: u32,
+        data: &[u8],
     ) -> Result<()> {
         let bs = self.vol.block_size() as usize;
         let new_blocks_needed = data.len().div_ceil(bs).max(1) as u32;
@@ -464,7 +741,18 @@ impl Writer {
 
     /// Clear a single anode slot (set all 3 fields to 0).
     fn clear_single_anode(&mut self, anodenr: u32) -> Result<()> {
-        self.write_anode_fields(anodenr, 0, 0, 0)
+        self.write_anode_fields(anodenr, 0, 0, 0)?;
+        // ART-311: its block has a free anode again — pfs3aio's FreeAnode sets
+        // the block's bit (`anodes.c:495-500`).
+        let seqnr = if self.vol.rootblock.is_splitted_anodes() {
+            anodenr >> 16
+        } else {
+            anodenr / self.anodes_per_block
+        };
+        if let Some(full) = self.anode_block_full.get_mut(seqnr as usize) {
+            *full = false;
+        }
+        Ok(())
     }
 
     /// Find a directory entry by name, returning (block_number, block_data, entry_offset).
@@ -565,7 +853,7 @@ impl Writer {
         }
 
         // Update datestamp
-        let (cday, cmin, ctick) = crate::util::current_amiga_datestamp();
+        let (cday, cmin, ctick) = self.entry_datestamp();
         put_u16(&mut data, pos + 10, cday);
         put_u16(&mut data, pos + 12, cmin);
         put_u16(&mut data, pos + 14, ctick);
@@ -580,6 +868,15 @@ impl Writer {
         dir_anode: u32,
         name: &str,
         protection: u8,
+    ) -> Result<()> {
+        self.guarded(|w| w.update_dir_entry_protection_impl(dir_anode, name, protection))
+    }
+
+    fn update_dir_entry_protection_impl(
+        &mut self,
+        dir_anode: u32,
+        name: &str,
+        protection: u8,
     ) -> Result<()> {
         let (blk, mut data, pos) = self.find_dir_entry(dir_anode, name)?;
         data[pos + 16] = protection;
@@ -595,6 +892,17 @@ impl Writer {
         dst_parent: u32,
         dst_name: &str,
     ) -> Result<()> {
+        self.guarded(|w| w.rename_in_impl(src_parent, src_name, dst_parent, dst_name))
+    }
+
+    fn rename_in_impl(
+        &mut self,
+        src_parent: u32,
+        src_name: &str,
+        dst_parent: u32,
+        dst_name: &str,
+    ) -> Result<()> {
+        self.check_name_len(dst_name)?;
         let entries = self.vol.list_dir_by_anode(src_parent)?;
         let entry = entries
             .iter()
@@ -627,6 +935,10 @@ impl Writer {
 
     /// Delete a file or empty directory by name in a parent directory.
     pub fn delete_in(&mut self, parent_anode: u32, name: &str) -> Result<()> {
+        self.guarded(|w| w.delete_in_impl(parent_anode, name))
+    }
+
+    fn delete_in_impl(&mut self, parent_anode: u32, name: &str) -> Result<()> {
         let entries = self.vol.list_dir_by_anode(parent_anode)?;
         let target = entries
             .iter()
@@ -642,9 +954,13 @@ impl Writer {
             self.free_anode_chain_reserved(target.anode)?;
             self.clear_anode_chain(target.anode)?;
         } else {
-            // Try to move to deldir instead of freeing
-            if !self.move_to_deldir(&target) {
-                self.free_data_blocks(target.anode)?;
+            // ART-318: pfs3aio's DeleteObject (`directory.c:1808-1830`). A file goes
+            // to the deldir first when there is one; the data blocks are freed
+            // either way, and the anodes are kept only for the deldir. Soft links
+            // never go there.
+            let kept = target.entry_type == ST_FILE && self.move_to_deldir(&target)?;
+            self.free_data_blocks(target.anode)?;
+            if !kept {
                 self.clear_anode_chain(target.anode)?;
             }
         }
@@ -652,86 +968,110 @@ impl Writer {
         self.update_rootblock()
     }
 
-    /// Move a deleted file entry to the deldir. Returns false if deldir not enabled.
-    fn move_to_deldir(&mut self, entry: &crate::ondisk::DirEntry) -> bool {
-        use crate::ondisk::*;
-        if !self.vol.rootblock.has_flag(MODE_DELDIR) {
-            return false;
+    /// ART-318: put a deleted file into the deldir as pfs3aio's `AllocDeldirSlot`
+    /// and `AddToDeldir` do (`directory.c:4489-4564`). The slot is
+    /// `rext.deldirroving`, which advances modulo `deldirsize × 31`; a slot whose
+    /// block is missing sends the roving pointer back to 0 and uses slot 0. A
+    /// file still in the slot loses its anodes — its blocks were freed at its own
+    /// delete. Returns `false` when the volume has no deldir. 0.1.3 took the first
+    /// empty slot, wrote the name without its length byte, freed an evicted
+    /// file's blocks a second time and swallowed every error.
+    fn move_to_deldir(&mut self, entry: &crate::ondisk::DirEntry) -> Result<bool> {
+        if !self.vol.rootblock.has_flag(MODE_DELDIR) || !self.vol.rootblock.has_extension() {
+            return Ok(false);
         }
-        let rext = match &self.vol.rootblock_ext {
-            Some(e) => e,
-            None => return false,
+        let ext_blk = self.vol.rootblock.extension;
+        let mut rext = self.read_reserved_raw(ext_blk)?;
+        let u16_at = |b: &[u8], at: usize| u16::from_be_bytes([b[at], b[at + 1]]);
+        let u32_at = |b: &[u8], at: usize| u32::from_be_bytes(b[at..at + 4].try_into().unwrap());
+        // rext: deldirroving 0x34, deldirsize 0x36, deldir[32] 0x90 (`blocks.h:444-456`).
+        let slots = usize::from(u16_at(&rext, 0x36)).min(MAXDELDIR + 1) * DELENTRIES_PER_BLOCK;
+        if slots == 0 {
+            return Ok(false);
+        }
+        let block_of =
+            |rext: &[u8], slot: usize| u32_at(rext, 0x90 + (slot / DELENTRIES_PER_BLOCK) * 4);
+        let roving = usize::from(u16_at(&rext, 0x34));
+        let (slot, next_roving) = if roving < slots && block_of(&rext, roving) != 0 {
+            (roving, (roving + 1) % slots)
+        } else {
+            (0, 0)
         };
-        let deldirblocks: Vec<u32> = rext
-            .deldirblocks
-            .iter()
-            .copied()
-            .filter(|&b| b != 0)
-            .collect();
-        if deldirblocks.is_empty() {
-            return false;
+        let dd_blk = block_of(&rext, slot);
+        if dd_blk == 0 {
+            return Ok(false);
         }
-
-        let rbs = self.vol.rootblock.reserved_blksize;
-        let entries_per_block = deldir_entries_per_block(rbs);
-
-        // Find a free slot (anode == 0) using roving pointer
-        for blk in &deldirblocks {
-            let data = match self.read_reserved_raw(*blk) {
-                Ok(d) => d,
-                Err(_) => continue,
-            };
-            if u16::from_be_bytes(data[0..2].try_into().unwrap()) != DELDIRID {
-                continue;
-            }
-
-            for i in 0..entries_per_block {
-                let off = DELDIR_HEADER_SIZE + i * DELDIR_ENTRY_SIZE;
-                if off + DELDIR_ENTRY_SIZE > data.len() {
-                    break;
-                }
-                let slot_anode = u32::from_be_bytes(data[off..off + 4].try_into().unwrap());
-                if slot_anode == 0 {
-                    // Found free slot — write the deldir entry
-                    let mut block_data = data;
-                    self.write_deldir_entry(&mut block_data, off, entry);
-                    let _ = self.write_reserved(*blk, &block_data);
-                    return true;
-                }
-            }
+        let mut data = self.read_reserved_raw(dd_blk)?;
+        if u16::from_be_bytes([data[0], data[1]]) != DELDIRID {
+            return Err(Error::Corrupt(format!(
+                "deldir block {dd_blk} is not a deldir block"
+            )));
         }
-
-        // Deldir full — evict oldest entry (first slot of first block)
-        let blk = deldirblocks[0];
-        let data = match self.read_reserved_raw(blk) {
-            Ok(d) => d,
-            Err(_) => return false,
-        };
-        let off = DELDIR_HEADER_SIZE;
-        let evict_anode = u32::from_be_bytes(data[off..off + 4].try_into().unwrap());
-        if evict_anode != 0 {
-            let _ = self.free_data_blocks(evict_anode);
-            let _ = self.clear_anode_chain(evict_anode);
+        let off = DELDIR_HEADER_SIZE + (slot % DELENTRIES_PER_BLOCK) * DELDIR_ENTRY_SIZE;
+        let evicted = u32_at(&data, off);
+        if evicted != 0 {
+            self.clear_anode_chain(evicted)?; // anodes only (`directory.c:4510-4518`)
         }
-        let mut block_data = data;
-        self.write_deldir_entry(&mut block_data, off, entry);
-        let _ = self.write_reserved(blk, &block_data);
-        true
+        self.write_deldir_entry(&mut data, off, entry);
+        // The deldir block's date and rext.dd_creation* are now (`directory.c:4556-4560`).
+        let (cday, cmin, ctick) = self.entry_datestamp();
+        put_u16(&mut data, 0x1A, cday);
+        put_u16(&mut data, 0x1C, cmin);
+        put_u16(&mut data, 0x1E, ctick);
+        put_u32(&mut data, 4, self.datestamp);
+        self.write_reserved(dd_blk, &data)?;
+        put_u16(&mut rext, 0x34, next_roving as u16);
+        put_u16(&mut rext, 0x88, cday);
+        put_u16(&mut rext, 0x8A, cmin);
+        put_u16(&mut rext, 0x8C, ctick);
+        self.write_reserved(ext_blk, &rext)?;
+        Ok(true)
     }
 
+    /// ART-318: pfs3aio's `deldirentry` (`blocks.h:368-379`) as `AddToDeldir` fills
+    /// it (`directory.c:4544-4550`): `anodenr`, `fsize`, the entry's own creation
+    /// date, a length byte and at most `DELENTRYFNSIZE - 1` name bytes. pfs3aio's
+    /// makefile builds with `LARGE_FILE_SIZE=0` (`makefile:21`), where that is 17
+    /// and a long name runs into what the struct calls `fsizex`; on a
+    /// `MODE_LARGEFILE` volume, which only a `LARGE_FILE_SIZE` build mounts as one
+    /// (`init.c:648`), the name stops at 15 and `fsizex` holds size bits 32-47
+    /// (`SetDDFileSize`, `directory.c:3704-3714`).
     fn write_deldir_entry(&self, block: &mut [u8], off: usize, entry: &crate::ondisk::DirEntry) {
         put_u32(block, off, entry.anode);
         put_u32(block, off + 4, entry.file_size() as u32);
         put_u16(block, off + 8, entry.creation_day);
         put_u16(block, off + 10, entry.creation_minute);
         put_u16(block, off + 12, entry.creation_tick);
-        let name_bytes = entry.name.as_bytes();
-        let len = name_bytes.len().min(16);
-        for b in &mut block[off + 14..off + 30] {
+        for b in &mut block[off + 14..off + DELDIR_ENTRY_SIZE] {
             *b = 0;
         }
-        block[off + 14..off + 14 + len].copy_from_slice(&name_bytes[..len]);
-        put_u16(block, off + 30, (entry.file_size() >> 32) as u16);
+        let largefile = self.vol.rootblock.has_largefile();
+        let fnsize = if largefile {
+            DELENTRYFNSIZE_LARGE_FILE
+        } else {
+            DELENTRYFNSIZE
+        };
+        let name = entry.name.as_bytes();
+        let len = name.len().min(fnsize - 1);
+        block[off + 14] = len as u8;
+        block[off + 15..off + 15 + len].copy_from_slice(&name[..len]);
+        if largefile {
+            put_u16(block, off + 30, (entry.file_size() >> 32) as u16);
+        }
+    }
+
+    /// ART-318: whether `blk` is a data block the bitmap holds free.
+    fn data_block_is_free(&self, blk: u32) -> bool {
+        if blk < self.bitmapstart || blk >= self.vol.rootblock.disksize {
+            return false;
+        }
+        let rel = blk - self.bitmapstart;
+        let per_block = self.index_per_block * 32;
+        let bit = rel % per_block;
+        self.data_bm
+            .get((rel / per_block) as usize)
+            .and_then(|(_, longs)| longs.get((bit / 32) as usize))
+            .is_some_and(|word| word & (0x8000_0000 >> (bit % 32)) != 0)
     }
 
     // ---- Data bitmap ----
@@ -739,9 +1079,11 @@ impl Writer {
     fn load_data_bitmap(&mut self) -> Result<()> {
         let no_bmb = {
             let bits_per_bmb = self.index_per_block * 32;
-            let ds = self.vol.rootblock.disksize;
+            // ART-315: the bitmap covers the data blocks, [bitmapstart, disksize),
+            // not the whole disk (format.rs sizes it the same way).
+            let data_blocks = self.vol.rootblock.disksize.saturating_sub(self.bitmapstart);
             // Cap at a reasonable maximum to prevent OOM on corrupt disksize
-            ds.div_ceil(bits_per_bmb).min(16384)
+            data_blocks.div_ceil(bits_per_bmb).min(16384)
         };
         for seq in 0..no_bmb {
             if let Some(blk) = self.get_bitmap_block_nr(seq)? {
@@ -778,7 +1120,10 @@ impl Writer {
                             .ok_or_else(|| {
                                 Error::Corrupt("block number overflow in bitmap".into())
                             })?;
-                        if data_blk >= self.vol.rootblock.disksize + self.bitmapstart {
+                        // ART-315: a data block is below the partition's end. pfs3aio
+                        // allocation.c:344 refuses `blocknr >= numblocks`; the bits past
+                        // it are left set by pfs3aio's and hst-imager's formats.
+                        if data_blk >= self.vol.rootblock.disksize {
                             continue; // skip out-of-range bitmap bits
                         }
                         longs[li] &= !(0x8000_0000 >> bit);
@@ -825,7 +1170,9 @@ impl Writer {
     }
 
     fn free_data_block(&mut self, blk: u32) -> Result<()> {
-        if blk < self.bitmapstart {
+        // ART-315: a block outside [bitmapstart, disksize) has no bit of its own;
+        // freeing one past the end would set a tail bit the allocator must never see.
+        if blk < self.bitmapstart || blk >= self.vol.rootblock.disksize {
             return Ok(());
         }
         let rel = blk - self.bitmapstart;
@@ -896,55 +1243,106 @@ impl Writer {
 
     // ---- Anode allocation ----
 
+    /// ART-311: pfs3aio's `AllocAnode` (`anodes.c:366-471`). The search starts
+    /// at the bitmap word holding the anode block the last allocation came from
+    /// (`:389`) and skips blocks already found full (`:395-416`). A missing
+    /// anode block is made where the search meets it — but a search that did
+    /// not start at 0 starts over from 0 first (`:418-419,436-450`). 0.1.3
+    /// rescanned 256 blocks from 0 for every anode.
     fn alloc_anode(&mut self, clustersize: u32, blocknr: u32, next: u32) -> Result<u32> {
         let split = self.vol.rootblock.is_splitted_anodes();
-        for seqnr in 0..256u32 {
-            let blk_num = self.get_anode_block_nr(seqnr)?;
-            if blk_num == 0 {
-                // No anode block at this seqnr — allocate one
-                let new_blk = self.alloc_anode_block(seqnr)?;
-                // Now use the first usable slot in the new block
-                let mut data = self.read_reserved_raw(new_blk)?;
-                let anodenr = if split {
-                    (seqnr << 16) | ANODE_USERFIRST
-                } else {
-                    seqnr * self.anodes_per_block + ANODE_USERFIRST
-                };
-                let base = ANODE_BLOCK_HEADER_SIZE + ANODE_USERFIRST as usize * ANODE_SIZE;
-                put_u32(&mut data, base, clustersize);
-                put_u32(&mut data, base + 4, blocknr);
-                put_u32(&mut data, base + 8, next);
-                put_u32(&mut data, 4, self.datestamp);
-                self.write_reserved(new_blk, &data)?;
-                return Ok(anodenr);
-            }
-            let mut data = self.read_reserved_raw(blk_num)?;
-            if u16::from_be_bytes(data[0..2].try_into().unwrap()) != ABLKID {
-                continue;
-            }
-            for offset in 0..self.anodes_per_block {
-                let anodenr = if split {
-                    (seqnr << 16) | offset
-                } else {
-                    seqnr * self.anodes_per_block + offset
-                };
-                if anodenr < ANODE_USERFIRST {
+        let limit = self.anode_block_limit();
+        if self.anode_block_full.len() < limit as usize {
+            self.anode_block_full.resize(limit as usize, false);
+        }
+        let mut start = (self.anode_roving / 32) * 32;
+        loop {
+            let mut seqnr = start;
+            while seqnr < limit {
+                if self.anode_block_full[seqnr as usize] {
+                    seqnr += 1;
                     continue;
                 }
-                let base = ANODE_BLOCK_HEADER_SIZE + offset as usize * ANODE_SIZE;
-                let cs = u32::from_be_bytes(data[base..base + 4].try_into().unwrap());
-                let bn = u32::from_be_bytes(data[base + 4..base + 8].try_into().unwrap());
-                if cs == 0 && bn == 0 {
+                let blk_num = self.get_anode_block_nr(seqnr)?;
+                if blk_num == 0 {
+                    if start != 0 {
+                        break; // start over from 0 before making a block
+                    }
+                    // No anode block at this seqnr — allocate one
+                    let new_blk = self.alloc_anode_block(seqnr)?;
+                    // Now use the first usable slot in the new block
+                    let mut data = self.read_reserved_raw(new_blk)?;
+                    let anodenr = if split {
+                        (seqnr << 16) | ANODE_USERFIRST
+                    } else {
+                        seqnr * self.anodes_per_block + ANODE_USERFIRST
+                    };
+                    let base = ANODE_BLOCK_HEADER_SIZE + ANODE_USERFIRST as usize * ANODE_SIZE;
                     put_u32(&mut data, base, clustersize);
                     put_u32(&mut data, base + 4, blocknr);
                     put_u32(&mut data, base + 8, next);
                     put_u32(&mut data, 4, self.datestamp);
-                    self.write_reserved(blk_num, &data)?;
+                    self.write_reserved(new_blk, &data)?;
+                    self.anode_roving = seqnr;
                     return Ok(anodenr);
                 }
+                let mut data = self.read_reserved_raw(blk_num)?;
+                if u16::from_be_bytes(data[0..2].try_into().unwrap()) == ABLKID {
+                    for offset in 0..self.anodes_per_block {
+                        let anodenr = if split {
+                            (seqnr << 16) | offset
+                        } else {
+                            seqnr * self.anodes_per_block + offset
+                        };
+                        if anodenr < ANODE_USERFIRST {
+                            continue;
+                        }
+                        let base = ANODE_BLOCK_HEADER_SIZE + offset as usize * ANODE_SIZE;
+                        let cs = u32::from_be_bytes(data[base..base + 4].try_into().unwrap());
+                        let bn = u32::from_be_bytes(data[base + 4..base + 8].try_into().unwrap());
+                        if cs == 0 && bn == 0 {
+                            put_u32(&mut data, base, clustersize);
+                            put_u32(&mut data, base + 4, blocknr);
+                            put_u32(&mut data, base + 8, next);
+                            put_u32(&mut data, 4, self.datestamp);
+                            self.write_reserved(blk_num, &data)?;
+                            self.anode_roving = seqnr;
+                            return Ok(anodenr);
+                        }
+                    }
+                }
+                // No free anode in this block (pfs3aio clears its bit, `anodes.c:414-416`).
+                self.anode_block_full[seqnr as usize] = true;
+                seqnr += 1;
+            }
+            if start == 0 {
+                return Err(Error::DiskFull("no free anode slots".into()));
             }
+            start = 0;
         }
-        Err(Error::DiskFull("no free anode slots".into()))
+    }
+
+    /// ART-311: the anode blocks this volume can address. pfs3aio passes an
+    /// anode block's number as a UWORD (`anodes.c:582`), so 65 536 at most; the
+    /// index levels bound it further — small mode's rootblock names
+    /// `MAXSMALLINDEXNR + 1` index blocks (`anodes.c:740`), SUPERINDEX mode's
+    /// extension `MAXSUPER + 1` super blocks (`anodes.c:851`). 0.1.3 searched 256.
+    fn anode_block_limit(&self) -> u32 {
+        let ipb = u64::from(self.index_per_block);
+        let by_index = if self.vol.rootblock.is_large() {
+            (MAXSUPER as u64 + 1) * ipb * ipb
+        } else {
+            (MAXSMALLINDEXNR as u64 + 1) * ipb
+        };
+        by_index.min(1 << 16) as u32
+    }
+
+    /// ART-311: the volume's `AnodeReader` copies the index roots when it is
+    /// built (`anode.rs:41-42`). After this writer names a new index or super
+    /// block it is rebuilt, so a chain walk later in this session resolves it.
+    fn refresh_anode_reader(&mut self) {
+        self.vol.anodes =
+            crate::anode::AnodeReader::new(&self.vol.rootblock, self.vol.rootblock_ext.as_ref());
     }
 
     /// Allocate a new anode block and register it in the index.
@@ -964,43 +1362,62 @@ impl Writer {
         let idx_off = seqnr % ipb;
 
         if self.vol.rootblock.is_large() {
-            // Large mode: superindex → index block → anode block
+            // Large mode: superindex → super block → index block → anode block
             let super_nr = seqnr / (ipb * ipb);
             let remainder = seqnr % (ipb * ipb);
             let idx_in_super = remainder / ipb;
             let off_in_idx = remainder % ipb;
 
-            let super_blk = self
+            let mut super_blk = self
                 .vol
                 .rootblock_ext
                 .as_ref()
                 .and_then(|e| e.superindex.get(super_nr as usize).copied())
                 .unwrap_or(0);
             if super_blk == 0 {
-                return Err(Error::DiskFull("no superindex slot available".into()));
+                // ART-311: a missing super block is made here, as pfs3aio's
+                // NewSuperBlock does (`anodes.c:851-870`), up to MAXSUPER; the
+                // rootblock extension names it when `update_rootblock` commits.
+                if super_nr as usize > MAXSUPER || self.vol.rootblock_ext.is_none() {
+                    return Err(Error::DiskFull("no superindex slot available".into()));
+                }
+                super_blk = self.alloc_reserved_block()?;
+                let mut sdata = vec![0u8; self.resblocksize as usize];
+                put_u16(&mut sdata, 0, SBLKID);
+                put_u32(&mut sdata, 4, self.datestamp);
+                put_u32(&mut sdata, 8, super_nr);
+                self.write_reserved(super_blk, &sdata)?;
+                if let Some(ext) = self.vol.rootblock_ext.as_mut() {
+                    if ext.superindex.len() <= super_nr as usize {
+                        ext.superindex.resize(super_nr as usize + 1, 0);
+                    }
+                    ext.superindex[super_nr as usize] = super_blk;
+                }
+                self.rext_dirty = true;
+                self.refresh_anode_reader();
             }
-            let idx_blk = {
-                let sdata = self.vol.cache.read_reserved(
-                    self.vol.dev.as_ref(),
-                    super_blk as u64,
-                    self.resblocksize as u16,
-                )?;
-                let off = INDEX_BLOCK_HEADER_SIZE + idx_in_super as usize * 4;
-                u32::from_be_bytes(sdata[off..off + 4].try_into().unwrap())
-            };
+            // ART-311: through this writer's own pending writes, like every index
+            // read since ART-312. A super block made above exists only there, and
+            // the reserved block it took may still hold a freed block's bytes.
+            let sdata = self.read_reserved_raw(super_blk)?;
+            let soff = INDEX_BLOCK_HEADER_SIZE + idx_in_super as usize * 4;
+            let idx_blk = u32::from_be_bytes(sdata[soff..soff + 4].try_into().unwrap());
             if idx_blk == 0 {
                 // Need to allocate an index block too
                 let new_idx = self.alloc_reserved_block()?;
                 let mut idata = vec![0u8; self.resblocksize as usize];
                 put_u16(&mut idata, 0, IBLKID);
                 put_u32(&mut idata, 4, self.next_datestamp());
-                put_u32(&mut idata, 8, idx_in_super);
+                // ART-311: an index block's seqnr is its number across the volume,
+                // `seqnr / indexperblock` (pfs3aio `anodes.c:592-595,767`). 0.1.3
+                // wrote its position inside its super block — the same only under
+                // super block 0.
+                put_u32(&mut idata, 8, seqnr / ipb);
                 let entry_off = INDEX_BLOCK_HEADER_SIZE + off_in_idx as usize * 4;
                 put_u32(&mut idata, entry_off, new_blk);
                 self.write_reserved(new_idx, &idata)?;
                 // Update super block
                 let mut sdata = self.read_reserved_raw(super_blk)?;
-                let soff = INDEX_BLOCK_HEADER_SIZE + idx_in_super as usize * 4;
                 put_u32(&mut sdata, soff, new_idx);
                 put_u32(&mut sdata, 4, self.datestamp);
                 self.write_reserved(super_blk, &sdata)?;
@@ -1012,8 +1429,14 @@ impl Writer {
                 self.write_reserved(idx_blk, &idata)?;
             }
         } else {
-            // Small mode: rootblock.indexblocks[idx_nr] → index block
-            let idx_blk = self
+            // Small mode: rootblock.indexblocks[idx_nr] -> index block. ART-311:
+            // a missing index block is made here, as pfs3aio's NewIndexBlock
+            // does (`anodes.c:740-761`), up to MAXSMALLINDEXNR; the rootblock
+            // names it when `update_rootblock` commits.
+            if idx_nr as usize > MAXSMALLINDEXNR {
+                return Err(Error::DiskFull("no index block slot available".into()));
+            }
+            let mut idx_blk = self
                 .vol
                 .rootblock
                 .indexblocks
@@ -1021,7 +1444,18 @@ impl Writer {
                 .copied()
                 .unwrap_or(0);
             if idx_blk == 0 {
-                return Err(Error::DiskFull("no index block slot available".into()));
+                idx_blk = self.alloc_reserved_block()?;
+                let mut idata = vec![0u8; self.resblocksize as usize];
+                put_u16(&mut idata, 0, IBLKID);
+                put_u32(&mut idata, 4, self.datestamp);
+                put_u32(&mut idata, 8, idx_nr); // seqnr (`anodes.c:767`)
+                self.write_reserved(idx_blk, &idata)?;
+                let slots = &mut self.vol.rootblock.indexblocks;
+                if slots.len() <= idx_nr as usize {
+                    slots.resize(idx_nr as usize + 1, 0);
+                }
+                slots[idx_nr as usize] = idx_blk;
+                self.refresh_anode_reader();
             }
             let mut idata = self.read_reserved_raw(idx_blk)?;
             let entry_off = INDEX_BLOCK_HEADER_SIZE + idx_off as usize * 4;
@@ -1097,6 +1531,7 @@ impl Writer {
                 .anodes
                 .get_chain(dir_anode, self.vol.dev.as_ref(), &mut self.vol.cache)?;
 
+        let mut dir_parent = None;
         for an in &chain {
             for i in 0..an.clustersize {
                 let blk = an.blocknr + i;
@@ -1104,6 +1539,11 @@ impl Writer {
                 if u16::from_be_bytes(data[0..2].try_into().unwrap()) != DBLKID {
                     continue;
                 }
+                // ART-313: every block of a directory carries the same parent;
+                // a new continuation block copies it (pfs3aio directory.c:3176,3204).
+                if dir_parent.is_none() {
+                    dir_parent = Some(u32::from_be_bytes(data[0x10..0x14].try_into().unwrap()));
+                }
                 // Find end of entries
                 let mut pos = DIR_BLOCK_HEADER_SIZE;
                 while pos < self.resblocksize as usize {
@@ -1123,13 +1563,17 @@ impl Writer {
                 }
             }
         }
-        // No space — allocate new dir block and extend chain
+        // No space — allocate new dir block and extend chain. Refused before
+        // anything is allocated when the directory has no block to copy from.
+        let parent = dir_parent.ok_or_else(|| {
+            Error::Corrupt(format!("directory {dir_anode} has no directory block"))
+        })?;
         let new_blk = self.alloc_reserved_block()?;
         let mut new_data = vec![0u8; self.resblocksize as usize];
         put_u16(&mut new_data, 0x00, DBLKID);
         put_u32(&mut new_data, 0x04, self.next_datestamp());
         put_u32(&mut new_data, 0x0C, dir_anode);
-        put_u32(&mut new_data, 0x10, dir_anode);
+        put_u32(&mut new_data, 0x10, parent);
         new_data[DIR_BLOCK_HEADER_SIZE..DIR_BLOCK_HEADER_SIZE + entry_bytes.len()]
             .copy_from_slice(&entry_bytes);
         self.write_reserved(new_blk, &new_data)?;
@@ -1172,7 +1616,7 @@ impl Writer {
         entry[1] = entry_type as u8;
         put_u32(&mut entry, 2, anode);
         put_u32(&mut entry, 6, fsize as u32);
-        let (cday, cmin, ctick) = crate::util::current_amiga_datestamp();
+        let (cday, cmin, ctick) = self.entry_datestamp();
         put_u16(&mut entry, 10, cday);
         put_u16(&mut entry, 12, cmin);
         put_u16(&mut entry, 14, ctick);
@@ -1200,11 +1644,52 @@ impl Writer {
 
     // ---- Rootblock update ----
 
+    /// The commit point: flushes every pending reserved-block write, then
+    /// writes the rootblock cluster last. ART-319: any error from in here
+    /// locks the writer — `update_rootblock_body`'s own writes land in place,
+    /// not copy-on-write (M5), so part of this operation's metadata (a
+    /// mutated dir block, a new bitmap value) can already be on disk while
+    /// the rootblock that would make it official is not; reloading and
+    /// continuing over that could silently accept the half state as though
+    /// it were the last commit, so this locks instead of guessing which of
+    /// the three writes inside failed.
     fn update_rootblock(&mut self) -> Result<()> {
+        match self.update_rootblock_body() {
+            Ok(()) => Ok(()),
+            Err(e) => {
+                self.poisoned = true;
+                Err(e)
+            }
+        }
+    }
+
+    fn update_rootblock_body(&mut self) -> Result<()> {
+        // ART-311: a new super block is named by the rootblock extension
+        // (`blocks.h:448`, superindex at 0x40); pfs3aio writes the extension
+        // before the rootblock (`update.c:240-256,265-270`). Only the
+        // superindex changes; every other extension field is left as read.
+        if self.rext_dirty {
+            let ext_blk = self.vol.rootblock.extension;
+            let mut rext = self.read_reserved_raw(ext_blk)?;
+            if let Some(ext) = &self.vol.rootblock_ext {
+                for (i, &blk) in ext.superindex.iter().enumerate().take(MAXSUPER + 1) {
+                    put_u32(&mut rext, 0x40 + i * 4, blk);
+                }
+            }
+            self.write_reserved(ext_blk, &rext)?;
+            self.rext_dirty = false;
+        }
+
         // Flush all pending reserved block writes first
         self.flush_pending()?;
 
-        // Write rootblock cluster (rootblock + reserved bitmap) last — atomic commit
+        // Write rootblock cluster (rootblock + reserved bitmap) last. M5
+        // (final review): this is a commit *point*, not an atomic commit —
+        // every pending write above already reached disk in place, and a new
+        // super block named by the extension write just above can already be
+        // on disk while the reserved bitmap on disk still marks it free.
+        // Nothing here is new to this round: every reserved block ART writes
+        // has worked this way since 0.1.3.
         let bs = self.vol.block_size() as usize;
         let rblkcluster = self.vol.rootblock.rblkcluster as u32;
         let cluster_size = rblkcluster as usize * bs;
@@ -1236,6 +1721,25 @@ impl Writer {
             }
         }
 
+        // ART-311: in small mode the rootblock names every anode index block
+        // (`blocks.h:133`, `idx.small.indexblocks`, after the five bitmap index
+        // numbers); pfs3aio sets it in NewIndexBlock (`anodes.c:759-760`) and
+        // writes the rootblock last (`update.c:265-270`). SUPERINDEX mode keeps
+        // bitmap index numbers there instead.
+        if !self.vol.rootblock.is_large() {
+            let base = RB_OFF_INDEX_UNION + (MAXSMALLBITMAPINDEX + 1) * 4;
+            for (i, &blk) in self
+                .vol
+                .rootblock
+                .indexblocks
+                .iter()
+                .enumerate()
+                .take(MAXSMALLINDEXNR + 1)
+            {
+                put_u32(&mut cluster, base + i * 4, blk);
+            }
+        }
+
         self.vol
             .dev
             .write_blocks(self.firstreserved as u64, rblkcluster, &cluster)?;
@@ -1286,10 +1790,44 @@ impl Writer {
         Ok(())
     }
 
+    /// The anode block at `seqnr`, or 0 when none is allocated yet.
+    ///
+    /// ART-312: resolved through `read_reserved_raw`, so an index entry this
+    /// writer set earlier in the same, still-uncommitted operation is seen.
+    /// Going through the volume's cache read the device, where that entry is
+    /// still 0 until `update_rootblock` flushes `pending_writes` — a second
+    /// allocation in one operation then created the same anode block again
+    /// and handed out the same anode number twice.
     fn get_anode_block_nr(&mut self, seqnr: u32) -> Result<u32> {
-        self.vol
-            .anodes
-            .resolve_anode_block(seqnr, self.vol.dev.as_ref(), &mut self.vol.cache)
+        let ipb = self.index_per_block;
+        let entry = |data: &[u8], nr: u32| -> u32 {
+            let off = INDEX_BLOCK_HEADER_SIZE + nr as usize * 4;
+            data.get(off..off + 4)
+                .map_or(0, |b| u32::from_be_bytes(b.try_into().unwrap()))
+        };
+        let idx_blk = if self.vol.rootblock.is_large() {
+            let super_blk = self
+                .vol
+                .rootblock_ext
+                .as_ref()
+                .and_then(|e| e.superindex.get((seqnr / (ipb * ipb)) as usize).copied())
+                .unwrap_or(0);
+            if super_blk == 0 {
+                return Ok(0);
+            }
+            entry(&self.read_reserved_raw(super_blk)?, (seqnr % (ipb * ipb)) / ipb)
+        } else {
+            self.vol
+                .rootblock
+                .indexblocks
+                .get((seqnr / ipb) as usize)
+                .copied()
+                .unwrap_or(0)
+        };
+        if idx_blk == 0 {
+            return Ok(0);
+        }
+        Ok(entry(&self.read_reserved_raw(idx_blk)?, seqnr % ipb))
     }
 
     fn get_bitmap_block_nr(&mut self, seqnr: u32) -> Result<Option<u32>> {
```
