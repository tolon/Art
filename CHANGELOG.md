# Changelog

All notable changes to Amiga Retro Toolkit are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Stopping an install now really stops it (2026-08-10)

#### Fixed
- **Cancelling a WHDLoad or Aminet install leaves the disk untouched.** It used
  to write whatever had been copied so far, then report the install as
  finished — so a game could end up on the disk without the one file that
  starts it, with nothing saying anything was missing. Cancelling now ends as
  "cancelled", and the image is exactly as it was.
- **"ART will not install this" no longer contradicts itself.** When an archive
  turns out not to be a WHDLoad package, the screen no longer offers to create
  a drawer with no name and write nothing into it, or complain that the package
  has no icon. And the advice under the refusal now fits the reason: copying it
  by hand is suggested only when that would actually help, rather than to
  someone whose disk is full.
- **The sidebar no longer cuts the page off on a short window.** With the
  window shorter than the full list of tools, the bottom of every page — and
  the bottom of its scrollbar — was clipped. The tool list now scrolls on its
  own instead of stretching the window.

### A write that would break a disk image is refused (2026-08-10)

#### Fixed
- **The finished image is checked before it replaces your file** — its
  bootblock, checksum, block count and root block. ART checked only the
  blocks an operation touched, so a change that got those four things wrong
  on the disk as a whole was written out anyway. Now they are checked on the
  whole result first, and if it does not hold up nothing is written — the
  file on disk is left exactly as it was, and the message says so. This does
  not yet check the free-space bitmap or a directory's hash chains, so a
  double allocation can still commit undetected.
- **HD floppies and hard disk images are measured against themselves.** ART's
  health check compared every image with a standard 880 KB floppy and flagged
  anything else as suspect. It now reads each image's own geometry, so a 1.76 MB
  floppy or a hard disk image is no longer reported as odd — and is not refused
  by the check above.

### The window scrolls, and ADF Studio opens real disks (2026-08-10)

#### Fixed
- **ADF Studio could not open a bootable disk.** It looked for the root block
  in a place the Amiga does not keep one — in the boot code itself — so any
  disk that could actually start a machine was refused as damaged. It now
  works out where the root block is, the way every other Amiga tool does.
- **1.76 MB disks now work**, and report their real size rather than half of
  it.
- **Long pages scroll.** The window was cutting content off at the bottom with
  no scrollbar at all.
- **The window scales.** Narrow it and the sidebar collapses to icons instead
  of squeezing the panes; widen it and content stays centred instead of
  clinging to the left.
- **Buttons that cannot be used now look it.** A greyed button is a button you
  know not to click. The folder names and the path bar in ADF Studio follow the
  same rule: while an operation is running they no longer look clickable.
- **"ART will not do this" no longer looks like a crash.** Being told an
  archive is not a WHDLoad package is an answer, not a fault, and it no longer
  arrives in red with an error code.
- **Installing a WHDLoad game or an Aminet package now refuses up front if it
  will not fit**, instead of discovering that partway through and leaving
  whatever landed — a game missing its `.slave` because it did not fit used to
  be a broken result with no warning.
- **A write could very rarely be reported as failed right after it succeeded.**
  Confirming a write by re-opening the file could race an external program
  (an antivirus scan, a search indexer) briefly locking it the moment ART let
  go. Confirmation now happens without releasing the file first, so that race
  is gone.

#### Changed
- **The second AmigaDOS writer is retired.** ADF Studio and the two-pane file
  manager now go through exactly one writer, so they can no longer disagree
  about what is on a disk. Everything it did — five previously-fixed defects
  among them — moved to the surviving writer with its tests; nothing was
  quietly dropped.

### Put a WHDLoad game on a hard disk, in one step (2026-08-09)

#### Added
- **Install WHDLoad** — a new screen. Point it at a `.lha` and a hard disk
  image, and it puts the game on the disk: the drawer, everything in it, and
  the icon that makes it show up on Workbench.
- It **checks before it writes**. What it found in the archive and how sure it
  is, which drawer it will create and where, how many blocks that needs and how
  many are free. The button is only live once all of that is settled.
- It **refuses rather than guesses**, and says which of these it is:
  the archive does not look like a WHDLoad package; it holds an `Install`
  script, so the game has not been installed yet and needs an Amiga to do it;
  a game of that name is already on the disk; it will not fit; it holds names
  AmigaDOS cannot store; or the archive did not unpack completely.
- The readme that ships alongside a package is **not** installed — it is not
  part of the game — and the screen says so rather than leaving you to notice.
- Dropping a `.lha` on ART now offers this as the first suggestion.

### The Files screen writes (2026-08-09)

#### Added
- **Hard disk images are no longer read-only.** Copy into a partition, rename,
  move, delete, make folders, edit attributes — the same operations a floppy
  has always had, on any volume ART can read.
- **Function keys, the way a file manager should have them.** F3 view · F4 edit
  · F5 copy · F6 rename · F7 new folder · F8 delete · F9 attributes, on the
  keyboard and on a bar under the panes.
- **Copy between two disk images**, in either direction, with every file
  checked after it lands.
- **Before a folder copy, ART tells you what it will cost**: how many blocks,
  how much room is left, which names AmigaDOS cannot store and what it would
  call them instead, and which names are already taken. One report, one
  decision — instead of a copy that stops on file 37 and again on file 52.
- **Edit a file inside an image (F4).** It comes out to a working copy, your
  own editor opens it, and you put it back when you are done. Nothing is
  written back unless the file actually changed, so opening a file and closing
  it cannot touch the image. If a Windows editor saved Windows line endings
  into an Amiga text file, ART offers to convert them — and never does it
  without asking.
- **Protection bits and comments are editable** (Power User Mode), with a line
  explaining what each of the eight does. They are also preserved when you copy
  files out to your disk and back, through a small `.uaem` file next to each
  one — the same format WinUAE uses, so folders round-trip between the two.
- **Icons travel with what they belong to.** Renaming or deleting `Game` offers
  to do the same to `Game.info`, and a copy warns when a folder is going in
  without its icon — which is what makes a drawer invisible on Workbench.
- **Install an Aminet package into a hard disk partition**, not just a floppy.
- **Free space, the volume name and the filesystem** are always in the pane
  footer. A volume ART will not write to keeps a lock badge and says why on
  hover, rather than a pane that quietly refuses everything.

#### Changed
- **A large image is no longer copied to back it up.** Floppies and small
  hardfiles keep the pipeline they always had — read whole, verify, replace
  atomically. Anything above 16 MB is written in place, with a record of every
  block before it changes, so a rename on a 2 GB disk takes a moment instead of
  minutes.
- **If ART is interrupted mid-write, it says so the next time you open the
  image** and offers to put it back exactly as it was. Until you decide, that
  image is read-only.

#### Fixed
- **Names with accented characters were refused as too long.** ART counted the
  bytes a name takes in its own memory rather than the characters AmigaDOS
  stores, so a perfectly legal name like `Grüße vom Süden` was rejected — and
  the more accents, the worse the count.

### Whole folders, Aminet settings that stick (2026-08-09)

#### Added
- **Copy a whole folder into an ADF.** Subfolders are recreated on the disk,
  and you are told up front how many files and how much space it needs. Names
  a floppy cannot hold, symlinks and anything nested too deep are listed as
  skipped rather than quietly dropped.
- **"Show in Collection"** on a downloaded Aminet package indexes your download
  folder in the Collection screen.
- **The mirror list is editable** in Power User Mode: reorder it, change an
  address, add or remove one, or go back to the mirrors ART ships with. Mirrors
  are tried top to bottom, so putting the one nearest you first makes syncing
  faster.

#### Fixed
- **Your Aminet download folder is remembered.** It was kept only while the app
  was running, so every restart quietly went back to the default — downloads
  then landed somewhere other than the folder you had chosen.
- **A custom mirror order is remembered too**, for the same reason.

### Hard disk images can be browsed — and four compatibility bugs fixed (2026-08-09)

#### Fixed
These four made ART's disks unusable outside ART. All were found by checking
ART's output with `amitools`, an independent implementation; none was visible
to ART's own tests, because its reader and writer shared each mistake.

- **Every ADF ART wrote had invalid checksums** (ART-033). AmigaDOS uses one
  algorithm for the boot block and a different one for every other block; ART
  used the boot block's everywhere. AmigaOS and WinUAE rejected the result while
  ART called it healthy.
- **Files added to a disk were zero bytes on a real Amiga** (ART-034). The file
  header's size field was written two longwords early, into unused space.
- **The free-space bitmap was laid out the wrong way round** (ART-035). AmigaOS
  would have treated ART's occupied blocks as free and written over them.
- **RDB partitions reported no filesystem** (ART-032). DosType and boot priority
  were each one longword early.

If you created disks with an earlier build, re-create them: the images
themselves are wrong, not just how ART reads them.

#### Added
- **Hard disk images open in the Files screen.** An HDF shows its partitions;
  open one and browse it like a floppy, with folders and file sizes, and copy
  files out to your disk. Bare hardfiles without a partition table work too.
- Partitions ART cannot read — PFS3, SFS, long filenames — are still listed with
  their name, size and the exact reason, so a healthy disk never looks broken.
- A partition that claims more space than the file holds is read as far as it
  really goes, and says so.
- Copying *into* a hard disk image is not implemented and refuses with the
  reason rather than appearing to work.

### Files: a two-pane manager, and more control over downloads (2026-08-09)

#### Added
- **Files** — a new screen with two panes, Norton Commander style. Each pane
  independently shows a local folder, an ADF image or an HDF image. Copy
  between local folders and floppy images in both directions, by button or by
  dragging; make folders and delete files inside an image; and **drag a file
  out to Explorer**. Every write to an image goes through the existing
  backup-and-validate pipeline, so the previous image is always kept.
- The **HDF pane shows partitions, sizes and filesystem types — not files**,
  and says so. ART cannot read inside a hard disk image's partitions yet, and
  an empty file list would have implied the disk was empty.
- **Aminet: sorting and filters.** Order by best match, newest, oldest,
  largest, smallest or name. Narrow by name-only search, upload age, size range
  and file type.
- **Aminet: you choose where downloads go.** Pick a download folder, put each
  package in a subfolder of your choosing, and move it later. Files keep their
  real names; the verified copy still lives in the cache so nothing is
  downloaded twice.
- **Aminet: install a downloaded package into an ADF.** It refuses up front if
  it will not fit, with real numbers, rather than failing part-way. "Install to
  HDF" is shown as *coming later* rather than hidden.

#### Fixed
- The Aminet screen could be left waiting forever if a sync or download failed:
  the button stayed disabled and a readme spun indefinitely. It now watches the
  job's own outcome and shows the error with its ID.

### Aminet — Software Sources Engine, Stage A (2026-08-09)

#### Added
- **Aminet Studio** (spec §41.5), a new screen. Sync the Aminet catalogue once
  and search, browse by category and read package descriptions **entirely
  offline** — 85 000+ packages, no connection needed after the first sync.
  Downloading fetches into a cache that lives outside every disk image;
  installing remains a separate step you ask for.
- Every download goes through the §41.5.3 trust pipeline: size check against
  the catalogue, SHA-256, and structural validation with the existing LHA
  engine. A package that fails any gate is discarded rather than cached.
- Sync, download and readme all run as background jobs, so the window never
  waits on a mirror. Mirrors fail over in order, and the error names every
  attempt rather than saying "download failed".
- Package facts carry their provenance: a version read from a readme is
  labelled as such and rated lower than one from the index, and a `Requires:`
  line is shown as the readme's claim, not as something ART checked (§14, §34).
- Two error IDs: `ART-MIRROR-UNREACHABLE`, `ART-INTEGRITY-MISMATCH`.
- Power User Mode shows the mirror order, index path, cache location, download
  hashes and the sync's unreadable-line report. Beginner Mode hides all of it
  and works identically.

#### Fixed
- **LHA archives with level 2 or 3 headers could not be opened at all**
  (ART-031). That is the format most modern tools write and what Aminet hosts,
  so LHA Studio could not list or extract a typical Aminet download. Level 0
  and 1 archives are unaffected and their filenames are unchanged.
- Mirror failover could concatenate a failed mirror's partial response onto the
  next mirror's (ART-030), producing a file assembled from two sources that
  parsed and hashed cleanly. A truncated response is now refused outright.

#### Notes
- A sync that comes back too damaged to trust **keeps your existing catalogue**
  and says so, rather than replacing it with a short one.
- Tests: 180 → 326 (368 after the Files work).

### Background jobs & Beginner/Power mode (2026-08-09)

#### Added
- **Job Queue** (spec §54, §55). Long operations run on background threads and
  report progress the UI can watch, with a Stop button. Collection scanning and
  LHA extraction are jobs; a global job bar in the app shell keeps work visible
  from any screen. Cancellation is checked between whole units of work, so
  stopping never leaves a half-written file.
- **Beginner / Power User mode** (spec §47, §48) is now actually applied. In
  Beginner mode the raw-data studios (Hex Tools, PiStorm), the Advanced action
  group and block-level numbers are hidden. Nothing is disabled, and Settings
  explains what the switch changes.
- `CoreError::Cancelled` (`ART-CANCELLED`) so a stopped operation reads as
  cancelled rather than as a failure.

#### Changed
- `collection_scan` returns a job id and delivers its titles in a
  `collection-scan-result` event instead of blocking until the walk finishes.
- New `lha_extract_job` runs extraction in the background; the synchronous
  `lha_extract` remains for small archives.
- Tests: 169 → 180.

### Operation Log & error IDs (2026-08-09)

#### Added
- **Operation Log** (spec §53). Every operation that changes user data is now
  recorded: what was done, the source and destination, where the previous
  version was backed up, whether verification passed, and — on failure — the
  error ID. Stored as append-only JSON Lines beside the application log.
- Settings gains an **Operation Log** section listing recent operations and
  exporting the full history as readable text.
- **Error IDs** (spec §68). Every error carries a stable `ART-*` identifier
  (`ART-SAFETY-REFUSED`, `ART-FORMAT-MALFORMED`, …) shown to the user and stored
  in the log, so a failure can be quoted rather than described.
- `OperationOrigin` distinguishes user actions from workflow runs and, ahead of
  §45.5, from AI-generated plans.

#### Changed
- Command errors reaching the frontend now end with `Error ID: ART-…`.
- Tests: 157 → 169.

### Hard disk & collection audit (2026-08-09)

#### Fixed
- **Creating or opening a hard disk image allocated the whole thing in memory.**
  A 4 GB HDF needed 4 GB of RAM. Images are now created sparsely (only the RDB
  blocks are written, then the file is extended), and opening one reads a 1 MB
  header window instead of the entire file.
- **Creating an image could silently destroy an existing one.** Both the HDF and
  ADF creation paths called `fs::write`, replacing whatever was already there.
  Creation now refuses to overwrite and cleans up a partially written file.
- **RDSK blocks described a zero-capacity disk.** `HiCylinder` and `CylBlocks`
  were never written and other logical-drive fields were at the wrong offsets,
  so disks ART created were self-consistent but wrong for AmigaOS and every
  other Amiga tool. Verified against `amitools`.
- **Empty RDB block lists pointed at block 0.** `BadBlockList`,
  `FileSysHeaderList` and `DriveInit` now use the `-1` sentinel; `BlockBytes`
  is set to 512.
- **Oversized partitions were silently truncated** and could leave the partition
  chain pointing at an unwritten block. Impossible layouts are now refused with
  the sizes involved.
- RDB checksums honour the block's own `SummedLongs` instead of assuming 128.
- Folder scans are depth-limited and no longer follow symlinks — a cyclic
  Windows junction used to overflow the stack and close the application.
- A request for an absurdly small image aborted the process instead of erroring.
- ROM identification checks file size before reading.

#### Changed
- `create_rdb_image` → `create_rdb_layout`, returning the leading blocks plus
  the intended total size rather than a full image buffer.
- Tests: 143 → 157.

### Data-safety & correctness hardening (2026-08-09)

#### Added
- **`core/safety`** — the single gate every write now passes through.
  `atomic_write` (temp file → `sync_all` → rename) means a write can never
  leave a half-finished image; `backup_file` keeps generational copies under
  `.art-backup/` (3 for disk images, 5 for config files, off for large HDFs).
- `OverwritePolicy` for LHA extraction (`Skip` by default, plus `Overwrite`
  and `Rename`), so extracting over a folder no longer destroys existing files.
- `MutationOutcome` / `GotekSaveOutcome` / `PistormSaveOutcome` carry the backup
  path back to the UI, so the user is always told where the previous version went.
- Read-only mounting option for HDFs passed to WinUAE.

#### Fixed
- **ADF hash function was AmigaDOS-incompatible.** `name_hash` omitted the
  `& 0x7ff` mask applied after every character, so entries were written to the
  wrong hash buckets. Images ART produced were readable by ART but not by
  AmigaDOS, WinUAE or any other Amiga tool. Now matches `adfGetHashValue`, with
  reference values pinned by tests, and honours the volume's international flag.
- **ADF edits overwrote the original in place with no backup.** The mutation
  path is now `read → mutate → validate → backup → atomic commit`; a failed or
  corrupting mutation leaves the on-disk image untouched.
- **Invalid block numbers from the UI crashed the whole application.** Bare
  indexing in `mutate.rs` panicked, and the release profile aborts on panic.
  All block access is now bounds-checked through `blocks::block_slice*`.
- `rename_entry` silently continued when an entry was missing from its parent's
  hash chain, linking it in twice and corrupting the directory.
- Files and directories could be created with names already in use.
- A file header could be passed where a directory was expected, corrupting it.
- Hash and file-extension chains could loop forever on a malformed image.
- **FlashFloppy `FF.CFG` was regenerated from scratch**, discarding every
  hand-tuned setting ART does not manage (spec §39). It is now edited in place.
- **PiStorm `cmdline.txt` was regenerated from scratch**, dropping `root=` and
  leaving the SD card unbootable. `config.txt` and `cmdline.txt` are now merged.
- HD floppy geometry never reported: the two size checks in `analysis.rs` were
  the same number (`901_120` and `880 * 1024`). Caught by clippy, hidden by CI.
- Multiple HDFs generated bare `hardfile=` lines with no device names, making
  every drive after the first unreachable. Now emits `hardfile2=` per device.
- WinUAE detection used hard-coded `D:\WinUAE` paths; it now reads the real
  Program Files locations from the environment.
- Media paths containing line breaks could inject arbitrary `.uae` directives.
- Zip-bomb guard could be bypassed by an archive declaring a size that overflowed
  the running total (`checked_add`), and aborted extractions left truncated files.
- A fixed `art_launch.uae` temp name made concurrent launches clobber each other.

#### Changed
- CI no longer runs clippy with `continue-on-error` — it hid the geometry bug.
- `lib.rs` narrowed its blanket `allow` from four lints to `dead_code` only.
- Tests: 90 → 133, all passing. Clippy: clean at `-D warnings`.

### Phase 0 — Foundation (2026-08-08)

#### Added
- Tauri 2 + React 19 + TypeScript + Vite application shell.
- Platform-independent Rust core engine with module skeletons:
  `adf`, `hdf`, `lha`, `rdb`, `rom`, `binary`, `analysis`, `recovery`,
  `compatibility`, `hashing`, `conversion`, `validation`.
- Format detection (`core/detect`) — extension + size + signature based,
  with confidence levels. Supports ADF, ADZ, DMS, HDF, HDZ, LHA, ROM,
  directories.
- SHA256 hashing (`core/hashing`) — streaming, memory-safe for large images.
- Workflow Engine (`core/workflow`) — trait-based `Workflow` + registry +
  engine that turns a detected object into a ranked `Plan` of candidate
  workflows. Ships two built-in workflows: Inspect, Compute SHA256.
- Universal Drag & Drop Manager — single global webview listener; dropped
  paths are forwarded to the Workflow Engine for analysis.
- SQLite database (`tauri-plugin-sql`) with initial migration
  (`settings`, `recent_files`, `jobs` tables).
- Structured logging (`tauri-plugin-log`) — stdout (dev) + log dir (release).
- JSON key/value settings (`tauri-plugin-store`) — theme, UX mode, language,
  paths.
- i18n architecture (`react-i18next`) — English locale, ready for more.
- Dark/light theme with subtle Amiga-inspired accent.
- Dashboard with drop target, recent files, quick actions.
- Settings page (appearance, general, paths).
- "Coming Later" placeholders for all not-yet-implemented modules.
- Windows CI pipeline (GitHub Actions): type-check, fmt, clippy, test, build.
- `cargo-deny` license/advisory policy (`deny.toml`).
- Documentation: architecture, product-vision, roadmap, format-support-matrix,
  security-model, drag-drop-workflows, testing, licenses.

#### Build
- `pnpm tauri build` produces MSI + NSIS installers for Windows x64.

#### Tests
- 14 Rust unit tests covering detection, hashing, and the workflow registry.
