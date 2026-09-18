# Known Issues & Technical Debt

Every defect found in ART gets an ID here — fixed or not. The ID is stable and
never reused, so a commit message, a code comment or an error message can point
at one and stay meaningful.

Spec §68 requires user-facing errors to carry an identifier rather than an
opaque code. These IDs are the registry those identifiers come from.

**Format:** `ART-NNN` · severity · one-line claim · where · how it fails ·
what fixed it (with the test that proves it).

**Severity:**

| | Meaning |
|---|---|
| 🔴 **Critical** | Can destroy or corrupt user data, or crash the application. |
| 🟠 **High** | Wrong results, or a spec rule broken in a way users will notice. |
| 🟡 **Medium** | Incorrect behaviour with limited blast radius. |
| 🔵 **Low** | Hygiene, dead code, developer-facing friction. |

A ✅ beside the severity marks an entry found and fixed within the same
pass — filed and closed together rather than sitting in Open in between.

---

## Open

**ART-062** 🔵 **A handful of Turkish strings have been read on screen; the other ~2200 keys have not** (2308 leaf keys in both catalogues as of 2026-09-17 — count them, the figures written into this entry have been overtaken repeatedly). **Mechanical part done 2026-09-14** on `art-debt-2-0914` — the one string this table's original rows could still name and reach without a backend was measured, found clipped, and fixed; **stays open**, see "What remains" below.
`src/i18n/tr.json`, `src/i18n/en.json` · Every Turkish string landed this phase
was verified by `pnpm test`'s key-parity check and by reading the JSON — never
by opening the running application and looking at a screen. Several Turkish
strings are substantially longer than their English originals and sit in tight
controls, so the check that remains is visual, not automatable in general —
though a plain browser, off Tauri, can now measure the screens that need no
backend (below).

**This table had decayed.** Read against the live catalogue (`src/i18n/tr.json`, `src/i18n/en.json`) and the current
source on 2026-09-14, two of its four original rows named a key or a control that no longer exists:

| ART-062 row | Status, 2026-09-14 |
|---|---|
| `pistorm.saveSync` — "Save & Sync PiStorm SD" | **Gone.** No such key exists in either catalogue. The nearest surviving string is `gotek.saveSync` = "USB'ye Kaydet ve Eşitle" (EN "Save & Sync to USB"), on the **Gotek** screen, not PiStorm (`src/pages/GotekStudio.tsx:156`). It is gated behind `drivePath` — the button is not in the DOM until a USB folder is picked via a native Tauri file dialog — so a plain browser cannot reach it. Unmeasured. |
| `hardDisk.bootablePri` — "Bootable (Pri {{n}})" | **Real key** (`src/i18n/tr.json:640`, "Önyüklenebilir (Öncelik {{n}})"), but renders only when `p.bootable` is true on a real RDB partition (`src/pages/HardDiskStudio.tsx:492,690`), which needs the Rust core to have actually analyzed a card or disk image. Unmeasured — needs a live Tauri session with a loaded card. |
| `pistorm.profile.classic.badge` — "Cycle-Exact & Demos" | **Gone.** There is no "classic" PiStorm profile and no per-profile `.badge` field any more; `PistormStudio.tsx`'s `PROFILES` are now `["performance", "daily", "compatibility", "diagnostics"]`, each with only `.title`/`.description`. No key under `pistorm.profile.*.badge` and no string reading "Cycle-Exact"/"Demos" (or their Turkish) exists anywhere in either catalogue. |
| FileManager function-key "View" → "Görüntüle" | **Real and live.** `files.functionKeys.view`, measured 2026-09-14 in a real browser at 1280x800/1024x768/960x768, both languages: 0 overflow hits at any width. |
| FileManager function-key "Grid" → "Izgara" | **Wrong screen.** There is no grid/list function key in FileManager (`files.functionKeys` has only `view/edit/copy/move/rename/newFolder/delete/attributes`). The string lives at `collection.toolbar.gridView`, a **Collection Studio** toolbar button instead — measured there 2026-09-14, both languages, all three widths: 0 hits. |
| job status "Done" / "Failed" | **Unmeasurable off Tauri.** `JobBar` (`src/components/JobBar.tsx:90`) renders `null` with no running or notable job — nothing is in the tree to measure without a real background job, which needs the Rust core. |

**Browser measurement, 2026-09-14** (`scripts/tr-overflow-check.py`, headless Chrome against `pnpm dev`,
`D:\Projeler\Amiga\scratch-0913\art062-overflow.md` is the full working note): every top-level route in
`src/App.tsx`'s router (16 routes), at 1280x800, 1024x768 and 960x768 (the shell's own `minWidth`,
`src-tauri/tauri.conf.json`), Turkish against an English control in the same mounted tree — 48 (route x width)
runs, 0 crashes, 0 both-languages hits. **One real Turkish-only clip**, found and fixed: `files.pane.nothingOpen`
("Nothing open" → "Hiçbir şey açık değil", +83%) clipped 10px in `.tc-path-text` (`src/pages/FileManager.tsx:4680`),
**1024x768 only** — not 1280, and, non-monotonically, not 960 either (the pane has more room at 960 than at 1024;
not investigated further, recorded as measured). Shortened to **"Açık bir şey yok"** (the owner's choice; `en.json`
unchanged). **Re-run after the fix, same command:** 48 runs, 0 errors, 0 crashes, Turkish-only list empty at every
route and width. This sweep only reaches screens and controls that render with no backend — most of the 2308 keys,
and every row in the table above that needs a loaded disk image, a scanned drive or a running job, are outside what
it could exercise.

**What remains, for a person:** reading the Turkish catalogue itself (2308 keys, a handful read so far — see
below) and the two rows above still marked unmeasured (`hardDisk.bootablePri`, job status "Done"/"Failed"), both
of which need a real backend session rather than a plain browser.

**First real evidence, 2026-08-21.** The owner drove the release build, chose
an old title, and read the new WHDLoad refusal on screen **in Turkish**: *"…bu
başlatma bir A1200 Kickstart 3.x istiyor. Başka bir model için olan Kickstart
— örneğin A500/A600/A2000 için bir 3.1 — ne kadar yeni olursa olsun buna
uymaz."* Their verdict: *"gayet makul bir çözüm olmuş"*, and the launch then
worked.

That is one sentence, not the catalogue — but it is the first time any
Turkish string in ART has been read on screen by someone who speaks it, and
it was one of the hardest kind: a refusal that has to leave the reader knowing
what to do next. Essentially all of the rest remain unseen. (Since then the
owner has also read the Workbench menus of a Turkish tree ART built, which is
a different claim — that is AmigaOS rendering ART's *output*, not ART's own
interface.)

**ART-324** 🔵 **libpfs3's writer drops the high bits of a file size of 4 GiB or more on a largefile volume —
narrowed 2026-09-15: on every other volume, the kind ART formats, such a size is refused by name and `fsizex` is
neither read nor written, as pfs3aio does** — *found 2026-09-15 by the third debt round's final whole-branch review
(M6), by reading; filed, not fixed, by its fix wave; narrowed by the scoped re-review's follow-up 1, libpfs3
`0.1.3+art.11`, ART-PATCH item 26*
`src-tauri/vendor/libpfs3/src/writer.rs`, `src/ondisk/direntry.rs`, `src/ondisk/rootblock.rs` · **Done on
2026-09-15 (follow-up 1).** pfs3aio's `g->largefile` is `MODE_LARGEFILE` **and** `MODE_DIR_EXTENSION` (`init.c:648`,
`tonioni/pfs3aio` `211f7f0`); `Rootblock::has_largefile` looked at `MODE_LARGEFILE` alone and is that now. Without
it, `GetDEFileSize` ignores `fsizex` (`directory.c:3641-3652`), and `DirEntry::parse` now reads it as 0 — where
`file_size()` added it on any volume, the scoped re-review's concern (a), which `core/firstboot/cardread.rs` and
`core/osinstall/verify.rs` reach. `SetDEFileSize` sets `fsize` and nothing else there (`:3668-3671`), and
`update_dir_entry_size` no longer patches a `fsizex` word, nor `build_dir_entry` write one. pfs3aio refuses to write a
file past `MAXFILESIZE32` (0xffffffff, `blocks.h:627`; `WriteToFile`, `disk.c:797`; `SetEOF`, `disk.c:1130`); the
new public `Writer::check_file_size`, asked by `write_file_in_no_commit`, `create_softlink_in` and
`overwrite_file_in` before they allocate, refuses `'<name>' was not written: it is <n> bytes, and this PFS3 volume
holds files of at most 4294967295 bytes: it is not formatted for large files (MODE_LARGEFILE with
MODE_DIR_EXTENSION)`, and ART's `from_pfs3` hands it on as `ART-INPUT-INVALID`, not "malformed pfs3". **ART's copy
asks first (the third scoped re-review's Minor 1, 2026-09-15).** `copy_in_pfs3` read each host file whole
(`std::fs::read`) and only then did `write_file_in` refuse, so a file of 4 GiB or more was loaded into memory before
the refusal, which did not say what to do. It now opens the writer (which reads the bitmaps and writes nothing) before
its fit check and asks `check_file_size` of every file, from the size `collect_entries` took from the file's metadata,
before the fit check and before any host file is read; the refusal names each file and its size (up to 20, then a
count) and says what to do: `'<file>' (<n> bytes) in '<folder>' cannot be copied to '<drive>': a file on it can be at
most 4294967295 bytes, because this PFS3 partition is not formatted for large files, and ART's own PFS3 format does not
make large-file partitions. Nothing was copied. Leave that file out of '<folder>' and run the copy again.` —
`libpfs3::format` never sets `MODE_LARGEFILE` (`src/format.rs`, the option word and `FormatOptions`, checked), so
formatting again cannot help. It reaches the screen through `errors.verbatim`: no `ART-INPUT-INVALID` pattern in
`src/lib/errorText.ts` matches it, and no catalogue key changed. A file under 4 GiB is still read whole before it is
written. **Tests**
(`core::preload::native`): `a_pfs3_copy_refuses_a_file_of_4_gib_or_more_by_name_before_reading_it` (the seam is
`CopyEntry::size`, with a host file that does not exist, so a read before the refusal ends in "not found"; a small
volume, a 5 100 MiB volume, a byte under the limit as the control, and two files),
`a_pfs3_entrys_fsizex_is_part_of_its_size_only_on_a_largefile_volume` (one raw entry,
`fsize` 700 and `fsizex` 1, under three modes), `a_pfs3_overwrite_writes_no_fsizex_on_a_volume_that_is_not_largefile`,
`a_pfs3_file_of_4_gib_or_more_is_refused_by_name_on_a_volume_that_is_not_largefile`,
`a_pfs3_file_too_large_reaches_the_user_as_invalid_input_not_a_malformed_volume`;
`a_pfs3aio_entrys_extra_fields_read_back_as_pfs3aio_writes_them` now reads its `fsizex` entry on a largefile volume.
**Red first**, on the unchanged library: `left: [("LARGEFILE and DIR_EXTENSION", 4294967996, Some(4294967996)),
("DIR_EXTENSION only, as ART formats", 4294967996, Some(4294967996)), ("LARGEFILE without DIR_EXTENSION", 4294967996,
Some(4294967996))]`; the overwrite `left: Some([26, 253, …, 66, 105, 103, 0, 0, 0, 4, 0])` against `right: …, 0, 0, 1,
4, 0]` (the stray word patched to 0); and, against a `check_file_size` that returned `Ok(())` and a `from_pfs3`
without the arm, `left: [("ART's mode, 4 GiB", "Ok(())"), …]` and `left: ("ART-FORMAT-MALFORMED", "malformed pfs3:
'Huge' was not written: …")`. **Mutations** (`D:\Projeler\Amiga\scratch-0913\fu-mutate.py`; the files backed up to
`scratch-0913\*.fu-fixed`, grep-confirmed, each restored with `shutil.copyfile` and hash-identical): **M1a** (the
reader's gate off) → the reader test red on the two modes that are not largefile; **M1b** (`has_largefile` on
`MODE_LARGEFILE` alone) → its third mode alone; **M1c** (`update_dir_entry_size` patches on any volume) → the
overwrite test; **M1d** (the limit ignores the mode) → the "largefile, 4 GiB" arm alone; **M1e** (`>=`) → the "a
byte less" arm alone. **Survived, as predicted, judged:** **M1g** (`build_dir_entry`'s gate off), **M1h**
(`write_file_in_no_commit` no longer asks the limit) and **M1i** (a largefile entry without the word cut to 32 bits),
each green over all 131 PFS3 tests — no test can hand the writer 4 GiB. The limit itself is pinned through
`check_file_size`; each call site is one line, read, not tested. **Minor 1, red first** on the unchanged copy: `left:
[("small volume, 4 GiB", "ART-INPUT-INVALID: invalid input: '…copy-in-too-large-…' needs 4294967296 bytes but 'DH0'
only has 8018944 bytes free; 0 listed"), ("5 100 MiB volume, 4 GiB", "io NotFound; 0 listed"), ("5 100 MiB volume, a
byte less", "io NotFound; 0 listed")]` — the fit check answered with a byte count on the small volume, and on the large
one the copy read the file before anything refused it; the control was the same before and after. **Its mutations**
(`D:\Projeler\Amiga\scratch-0913\minors-mutate.py`; `native.rs` backed up to `scratch-0913\native.rs.minors-fixed`,
restored with `shutil.copyfile`, SHA-256 identical, green again): **m1** (the early check removed) → every size arm red,
the two small-volume arms with the fit check's sentence and the 5 100 MiB arm `io NotFound`; **m2** (the check moved
after the fit check, before the loop) → the two small-volume arms alone; **m3** (the plural wording made singular) →
the two-file arm alone. None survived. **Still open, on a largefile volume only** (ART
formats none): an entry without `fsizex` that must take high bits is refused `FileTooLarge` rather than grown and
moved, as `AddExtraFields` would (`:3764-3800`); the block counts are cast `as u32` (`write_file_in_no_commit`,
`create_softlink_in_impl`, `overwrite_file_in_impl`); and an overwrite that clears the high bits leaves a zero
`fsizex` word where `AddExtraFields` drops the word — the same size read back, not the same bytes. **Fix direction:**
grow and move the entry when `fsizex` must be added, and count blocks in `u64`.

*The entry as filed on 2026-09-15, before follow-up 1:*
`src-tauri/vendor/libpfs3/src/writer.rs` · Pre-existing since 0.1.3; nothing this round changed. A PFS3 directory
entry holds a size's low 32 bits in `fsize` and bits 32–47 in the optional `fsizex` extra field, which is size only
on a `MODE_LARGEFILE` volume (ART-PATCH item 11). Read on the tree of 2026-09-15: **`update_dir_entry_size`** (used
by `overwrite_file_in`) writes the low 32 bits and patches `fsizex` only when the entry already carries that field
(`put_u32(&mut data, pos + 6, new_size as u32)`, then the extra-field walk), so a file grown past 4 GiB keeps only
its low 32 bits — the entry would have to grow, which that in-place patch cannot do; **`build_dir_entry`** writes
`fsizex` whenever the high bits are non-zero, **without** checking `MODE_LARGEFILE`, where pfs3aio refuses the write
instead (`WriteToFile`, `disk.c:797`: `newfileoffset > MAXFILESIZE32 && !g->largefile` → `ERROR_DISK_FULL`,
`tonioni/pfs3aio` `211f7f0`); the block counts are cast with `as u32` (`write_file_in_no_commit`,
`create_softlink_in_impl`, `overwrite_file_in_impl`); and `write_deldir_entry` keeps the high bits only on a
`MODE_LARGEFILE` volume. **Why it is not fixed in the fix wave:** the fix is not contained to the entry-size paths —
a missing `fsizex` needs the entry rebuilt and moved, not patched — and a test that is red first needs a file of
more than 4 GiB as an in-memory `&[u8]`, which the writer's API takes. **Unreachable from ART today:**
`copy_in_pfs3` reads each host file whole into memory and checks the volume's free space first, and ART formats no
`MODE_LARGEFILE` PFS3 volume. **Fix direction:** refuse a size above `u32::MAX` on a volume without
`MODE_LARGEFILE`, as pfs3aio does, and rebuild the entry when `fsizex` must be added.

**ART-328** 🔵 **libpfs3's writer stores a new entry's name as its UTF-8 bytes, and its reader decodes a name as
Latin-1** — *found 2026-09-15 while fixing ART-327, by reading; not measured*
`src-tauri/vendor/libpfs3/src/writer.rs` (`build_dir_entry`, `check_name_len`) · Pre-existing since 0.1.3.
`build_dir_entry` copies `name.as_bytes()` and `check_name_len` measures `name.len()`, both UTF-8, while
`util::latin1_to_string` decodes a stored name one byte a character, as an Amiga writes it. A name "ä" created through
the writer is stored `C3 A4` and lists as "Ã¤", on the Amiga too. The two rename paths now write Latin-1
(`rename_dir_entry_in_place` since the final review fix wave, M1; `moved_entry` since ART-327). Only `moved_entry`
falls back to the name's UTF-8 bytes for a character above U+00FF; `rename_dir_entry_in_place` refuses such a name
instead, `Error::Corrupt` (`writer.rs`, its `new_name_bytes` match). So a name ART-327 moves and one `create_dir_in`
makes are not written alike. *(Corrected 2026-09-15 by the scoped re-review's follow-up 5: this said both rename
paths fall back.)*
**Unreachable from ART:** `copy_in_pfs3` refuses a non-ASCII name before libpfs3 sees it
(`core/preload/native.rs`, "A non-ASCII name is refused before `libpfs3` ever sees it (ART-113)"), and for an ASCII
name the two encodings are the same bytes. **Fix direction:** encode Latin-1 in `build_dir_entry`, measure
`check_name_len` in those bytes, and refuse a character above U+00FF by name.

**ART-329** 🔵 **libpfs3's `rename_in` moves a directory without updating its directory blocks' parent, and does not
refuse a move into its own subtree** — *found 2026-09-15 reading pfs3aio's `RenameAndMove` for ART-327; not
measured*
`src-tauri/vendor/libpfs3/src/writer.rs` (`rename_in_impl`) · Pre-existing since 0.1.3. pfs3aio refuses a directory
renamed into a child of itself (`IsChildOf`, `ERROR_OBJECT_IN_USE`, `directory.c:2079-2095`) and, when a directory
changes parent, sets every one of its directory blocks' `parent` to the destination (`directory.c:2148-2169`,
`tonioni/pfs3aio` `211f7f0`). `rename_in_impl` does neither: ART-313 made each block of a directory carry the
directory that holds it, and a moved directory's blocks keep the old one; `rename_in(root, "A", a_b, "A")`, with
`a_b` the anode of "A/B", would put A's entry inside A's own subtree and leave nothing reachable from the root
naming it. libpfs3's own reader never reads `parent` (ART-PATCH item 4). **Unreachable from ART:** ART's product code
never renames on PFS3. **Fix direction:** refuse a move into the directory's own subtree before anything is staged,
and rewrite each directory block's `parent` in the rename's commit.

**ART-331** 🟡 **libpfs3 steps over a directory block that does not carry the `DB` id, silently, when it reads or
writes a directory** — *found 2026-09-15 while fixing ART-330, by reading; not measured*
`src-tauri/vendor/libpfs3/src/dir.rs` (`walk_entries`), `src/writer.rs` (`find_dir_entry`, `add_dir_entry_bytes`, the
delete-time link scan) · Pre-existing since 0.1.3; ART-330's fix kept it. A block in a directory's anode chain whose
id is not `DB` is skipped: its entries are neither listed nor found, so a lookup of a name in it answers "not there"
and a listing comes back short — ART-330's confident wrong sentence, from another cause. pfs3aio refuses the block:
`LoadDirBlock` returns NULL with `AFS_ERROR_DNV_WRONG_DIRID` (`directory.c:3590-3606`), and `SearchInDir` then fails
(`:753-756`; `tonioni/pfs3aio` `211f7f0`). ART's own test fixture leans on the skip — `pfs3_mutator_fixture`'s
"Broken" has its block's id zeroed so that adding an entry to it fails late. **Reachable on a damaged card:**
`core/firstboot/cardread.rs` and `core/osinstall/verify.rs` read PFS3 directories off real cards through
`Volume::lookup`, and would report such a card's file missing or its report "not booted". **Fix direction:** refuse
the block by name in both walks, as `Error::DamagedDirectory` does for a malformed entry, and give the fixture's late
failure another shape.

**ART-342** 🔵 **`core/whdload` and `core/gameindex` import each other** — *found 2026-09-17 by card round 3's research
(`.superpowers/sdd/2026-09-17-one-button-card-round-3/research-tree.md` § 1), by reading; not fixed*
`src-tauri/src/core/whdload/install.rs` imports `core::gameindex` (`:624`, `:713-716`, `:1238-1240`) while
`core/gameindex/readers/drawer.rs:24`, `lhadrawer.rs:67` and `whdhdf.rs:37` import `core::whdload` — the shape ART-339
was. **Nothing is broken by it**; neither module can be lifted into a crate alone. Card round 3 put its own WHDLoad work
in `core/cardos/` rather than deepen it. **Fix direction:** move `install.rs` (the volume installer, which needs the
catalogue) up out of `core/whdload`, leaving `core/whdload` the pure layout analysis both use.

**ART-343** 🟡 **Nothing checks that a prepared card's tree and sources are still what is on disk when it is
built** — *found 2026-09-17 by card round 3's final whole-branch review
(`.superpowers/sdd/2026-09-17-one-button-card-round-3/final-review.md`, M5), by reading; filed by the fix wave as
design; narrowed 2026-09-17 by the residual fix round, which fixed the ROM half*
`src-tauri/src/commands/cardos.rs` (`card_os_prepare` → `card_os_build`), `core/cardos/prepare.rs`
(`PreparedCard`) · Between prepare and build the OS Builder may rewrite `tree/`, and folder sources are copied in
place from the user's own folders, so System's re-check and every partition's size can be stale: the build then
fails in its Partitions phase **after the image exists**. **Why filed, not fixed:** it needs a design — what a
fingerprint of a large tree costs, whether a folder's mtime is trustworthy on every filesystem ART reads, and
whether a change is refused or re-prepared — and the round's rule is that a fix wave does not build unreviewed
design. **Fix direction:** record each source's size and modification time and the tree's fingerprint in
`PreparedCard`; refuse in the build's preflight (which already runs before anything is written, card round 3 I3)
when any differs, naming what changed and "prepare again". **The ROM half is fixed** (card round 3's residual
round, re-review's split-out of M5): `core/cardos/kickstarts.rs::check_agreed` re-reads each agreed Kickstart's
source through `identify_rom` and compares its WHDLoad CRC-16, and its size when the offer stated one, with the
offer; a changed, missing or unreadable file is `CoreError::KickstartSourceChanged`
(`ART-KICKSTART-SOURCE-CHANGED`), a `Refused { Whdload }` ending before the tree or the image is written, naming
the file, both checksums and "prepare the card again". Tests:
`an_agreed_kickstart_whose_file_changed_since_the_proposal_is_refused_by_name` (both arms: unchanged control,
changed bytes, a stated size that differs, the file gone) and, through the build,
`a_kickstart_changed_since_prepare_is_refused_before_the_tree_or_the_image`; the comparison, the size check and the
call each mutated and seen red. **Round 4's screen (2026-09-18) renders the ROM half's refusal** —
`ART-KICKSTART-SOURCE-CHANGED` is one of the 28 rows `src/lib/errorText.ts`'s `CARD_RECOGNISERS` answers in the
user's own language (Task 11), and the Build tab's own button turns `cardRun.run` into `cardRun.runAgain` the
moment the run is not running — pressing it opens a fresh session, re-prepares and rebuilds, which is round 4's
answer to Q9's "the screen says what a re-prepare costs and offers it on one press": the refusal names the file
and both checksums, and the one button that follows it does the "prepare again" the sentence asks for. **Still
open**: the tree/sources half above — nothing on the screen fingerprints a folder source or the tree between
prepare and build, so a change there is still only caught by the Partitions-phase failure this entry describes,
not refused earlier by name.

**ART-344** 🔵 **A card session's scratch folder is left behind when the app exits without closing it, and a
panicking job leaves the session busy** — *found 2026-09-17 by card round 3's final whole-branch review
(`final-review.md`, M6), by reading; filed by the fix wave as design, not built*
`src-tauri/src/commands/cardos.rs` (`CardOsSessions`), `src-tauri/src/lib.rs` · A session folder under the scratch
root (staging can be several GB) is removed by `card_os_close` or by the build's every ending. If the app exits
without either, nothing removes it: Tauri does not drop managed state on exit, and `OwnedScratch`'s `Drop` backstop
therefore never runs. With `panic = "abort"` a panic in a job does the same; in a dev build a panic leaves the
session `busy` forever, so it cannot be closed. `OwnedScratch::create_in` names a leftover folder when a later run
reuses its name (card round 3, M15), but nothing finds and offers to remove the rest. **Why filed, not fixed:** an
exit hook that deletes several GB while the window closes, and a startup sweep of `card-os-*` folders from earlier
processes, are both design choices (what to remove without asking, what to show). **Fix direction:** drain
`CardOsSessions` on `RunEvent::Exit` and report what could not be removed to the log; list leftover `card-os-*`
folders under the scratch root at startup and offer them for removal, never removing one silently.
**Round 4's screen (2026-09-18) narrows the exposure (Q9):** `card_os_open` is called only when the user presses
the Build tab's run button — `src/lib/useCardOsRun.ts`, never on mount — so a session cannot leak by merely
opening the tab; mutation-checked (`opens no session until the button is pressed`, Task 10). Every path through
the run (succeeded, refused, failed, the user's own Stop, giving up at the Kickstart agreement) calls
`card_os_close` exactly once. **Still open, as filed:** no exit hook and no startup sweep were built this round —
a session left by a real crash or a killed process is still found only by hand.
**Round 4's fix wave (2026-09-18) removed the `session.tree` half of this** (the final review's C3): the run's
scratch tree no longer goes anywhere persisted — `src/lib/cardRunTree.ts`, a store with no key — so no crash can
leave a scratch path in the build session for a later run to read. The leftover *folder* is what remains open.

**ART-345** 🟠 **In the one-button card flow the Machine tab can never show a size, a total or an overflow** —
*found 2026-09-18 by card round 4's final whole-branch review (`final-review.md`, I5), by reading; filed by the
fix wave as a flow question, not built*
`src/components/osbuilder/CardSection.tsx` (`tree`, the measure request, `cardSection.needTree`),
`src-tauri/src/commands/cardos.rs` (`card_os_measure`) · `CardSection` measures against a system tree, and in the
one-button flow the card's tree exists **only inside a run** — while the Machine tab is unreachable, because the
strip chips and the whole sidebar are dead under `useRunLock`. So unless the user has previously completed a
*folder* build, which a card user has no reason to do, every row reads *"henüz ölçülmedi"*, the heading cost reads
*"…ölçülebilecek bir sistem ağacı olduğunda"*, and § 3's live total, per-row size and overflow line never appear.
The refusal that names the overflowing partition — task 8's headline — is unreachable before the run too, which
is the one moment it would be worth anything. **Why filed, not fixed:** `card_os_measure` needs the tree only for
System, so a request could carry `tree: ""` and answer the user's own partitions while saying System is not yet
measured — but "measured except for the operating system" is a *screen* question (what the total then means,
whether the overflow line may be drawn at all, what the three registers say), and the owner decides what the
Machine tab claims. The alternative, a measure-only session opened on demand, is a second session lifetime beside
the run's. **Fix direction:** decide with the owner which of the two, then either let `card_os_measure` take an
empty tree and have `CardSection` say plainly which part of the answer is still missing, or open a staging-free
session for the measurement alone. Either way the Machine tab must be able to answer *"does this fit"* before the
build, which is the reason `card_os_measure` exists.

**ART-346** 🟡 **A card run's report is deleted from the screen if the Build tab's gate turns on while it runs** —
*found 2026-09-18 by card round 4's final whole-branch review (`final-review.md`, I4); the session-leak half was
fixed by the fix wave, this half filed*
`src/components/osbuilder/BuildTab.tsx` (`gate ? … : cardMode ? <CardRun/> : …`) · The whole card run is inside
`CardRun`, which is rendered only in the `gate === null` branch, unlike the folder run whose `run.reports` are
rendered outside the gate. The gate reads live values — `summary.ticked.loading`, `ticked.unresolved`,
`cardBlocker` over the plan and the media evidence, and `cardImage` — so any of them changing mid-run (a media
scan landing, a re-plan, a tick resolving) unmounts `CardRun`: every phase row and every refusal on screen
disappears and is replaced by the blocker's own sentence, with nothing saying a run happened. **The session no
longer leaks:** the fix wave rejects the pending agreement on unmount, so `runSequence`'s `finally` runs and
`card_os_close` is called (`is closed when the screen goes away while the run waits at the agreement`,
`src/lib/useCardOsRun.test.tsx`). **Why the rest is filed:** moving the card's rows outside the gate branch means
deciding what the screen shows when a gate and a finished run are both true — the blocker's sentence, the report,
or both, and in which order — and the folder lane's answer (report below, always) was never argued for the card,
whose rows carry the endings of a run that wrote a card image. **Fix direction:** render the card's reports and
its `.infobar` endings outside the `gate` ternary the way `!cardMode && run.reports.length > 0` already does, and
add a case that flips a gate input while a run is in flight and asserts the rows survive.

Missing features are not defects — see [FEATURES.md](FEATURES.md) for what is
not built yet, and [STATUS.md](STATUS.md) for what is scheduled.

Every module with working logic has now been audited. The remaining `core`
modules are stubs that only return `NotImplemented` (`recovery.rs`,
`conversion.rs`, `binary.rs`, `validation.rs`) or hold types with no logic
(`compatibility.rs`) — see [FEATURES.md](FEATURES.md) for their planned state.

Two areas were reviewed and found sound, and are recorded here so nobody
re-audits them without reason:

- `core/analysis.rs` — the hex reader clamps both offset and length, and the
  signature scan guards its window.
- `core/profile.rs` — preset data only, no parsing of untrusted input.

---

## Fixed

Fixed and closed entries live in full, verbatim, in
[ISSUES-archive.md](ISSUES-archive.md). The index below lists each one, newest
first, as `ID · title · when it was fixed` (the date as the entry states it).

- [ART-341](ISSUES-archive.md) · `core/osinstall` treated `:` as legal in an AmigaDOS name and kept such a name in the manifest, so a tree carrying one failed partway through the copy, after the format — fixed: `:` and an empty path segment are refused before anything is written · fixed 2026-09-17
- [ART-340](ISSUES-archive.md) · A card source refused after unpacking began left its staging folder part-filled, with nothing owning its removal — fixed: a product scratch guard (`OwnedScratch`) that a card-OS session holds and removes on every ending · fixed 2026-09-17
- [ART-339](ISSUES-archive.md) · `core/card` and `core/preload` imported each other, against CLAUDE.md's inward-layering rule — fixed: `content.rs` moved to `core/cardos/`, above both · fixed 2026-09-17
- [ART-338](ISSUES-archive.md) · ART's own LZX reader refused a match that reaches back before a merged group's first byte, which real archives do — fixed: that part of the window reads as zeros, as the Amiga archiver's does · fixed 2026-09-17
- [ART-337](ISSUES-archive.md) · ART's native FFS copy dropped a `.uaem` sidecar's comment, for files and drawers, without counting the loss · fixed 2026-09-17
- [ART-336](ISSUES-archive.md) · hst-imager's fallback copy ignored every `.uaem` sidecar, silently — protection bits, dates and comments were dropped and nothing said so · fixed 2026-09-17
- [ART-335](ISSUES-archive.md) · ART's native PFS3 copy stamped every entry "now" and dropped a `.uaem`'s comment, though the vendored writer could already take a date — ART-116's "not fixable" had gone stale · fixed 2026-09-17
- [ART-334](ISSUES-archive.md) · `ExtractReport.renamed` held leaf-only display strings, so a name escaped out of an HDF could not be restored to its Amiga name · fixed 2026-09-17
- [ART-333](ISSUES-archive.md) · Extracting from an OFS/FFS volume wrote no `.uaem` sidecar for a drawer, so a drawer's protection bits, comment and date were lost · fixed 2026-09-17
- [ART-332](ISSUES-archive.md) · The archive gate carried no Amiga protection bits, comments or dates, for any format · fixed 2026-09-17
- [ART-330](ISSUES-archive.md) · libpfs3's reader stopped at a malformed directory entry and answered "not there" for every name behind it — so ART's first-boot report said "not booted", and its install check "not found on the volume", for a card whose drawer was damaged — fixed: a walk that meets a malformed entry is refused, naming the directory, the block and the offset · fixed 2026-09-15
- [ART-325](ISSUES-archive.md) · libpfs3 read a directory entry's extra fields in a layout neither pfs3aio nor hst-amiga uses — fixed: every reader and writer of them goes through one implementation of pfs3aio's layout · fixed 2026-09-15
- [ART-326](ISSUES-archive.md) · libpfs3's writer added a second directory entry under a name that already exists — fixed: each creating call refuses the name, compared as pfs3aio compares names · fixed 2026-09-15
- [ART-327](ISSUES-archive.md) · libpfs3's `rename_in` rebuilt the entry it moves, dropping its comment, its date and its extra fields — fixed: the moved entry is the source's own bytes with the new name, and a moved link's chain is updated as `MoveLink` does · fixed 2026-09-15
- [ART-323](ISSUES-archive.md) · libpfs3's writer deleted a hard link by freeing the file it names — fixed: a link's delete removes only its entry, and what the writer cannot do as pfs3aio does is refused by name · fixed 2026-09-15
- [ART-322](ISSUES-archive.md) · A PFS3 rename that changes only a name's case deleted the file and reported success — fixed by treating the destination as the source when it is one · fixed 2026-09-15
- [ART-321](ISSUES-archive.md) · `mbr_slot_of` could name the wrong MBR slot when an earlier Amiga area is unreadable — fixed by carrying each area's own slot · fixed 2026-09-14
- [ART-320](ISSUES-archive.md) · `cargo test` forced `TMP`/`TEMP` onto `D:`, against the owner's own rule — moved to `E:`, CI overrides it with the runner's own temp directory · fixed 2026-09-14
- [ART-117](ISSUES-archive.md) · `import_filesystem` refused a foreign card's existing RDB — ART now embeds and replaces a driver in place · fixed 2026-09-14
- [ART-250](ISSUES-archive.md) · `tooltypes()`'s lossy UTF-8 decode could not byte-for-byte round-trip a NewIcon `IM1=`/`IM2=` tool type · fixed 2026-09-14
- [ART-319](ISSUES-archive.md) · An error part-way through a PFS3 writer operation leaves pending writes and in-memory index/superindex/deldir state for the next commit · fixed 2026-09-14
- [ART-317](ISSUES-archive.md) · Every Amiga date ART stamps from the host clock or a host file's modification time is UTC; the Amiga reads it as local time · found 2026-09-11, fix date not stated
- [ART-301](ISSUES-archive.md) · The job bar's titles were English sentences composed in Rust · fixed 2026-09-14
- [ART-318](ISSUES-archive.md) · `libpfs3`'s writer put a deleted file into the deldir in a layout pfs3aio does not read · fixed 2026-09-14
- [ART-311](ISSUES-archive.md) · `libpfs3`'s writer caps the anodes a PFS3 volume can hold: at most 21 246 in small mode and 21 498 in SUPERINDEX mode, whatever the volume's size · fixed 2026-09-14
- [ART-313](ISSUES-archive.md) · PFS3 directories ART writes carry the wrong `parent`: the Amiga's handler cannot find a file's parent directory · fixed 2026-09-14
- [ART-314](ISSUES-archive.md) · A PFS3 name longer than 31 bytes lists on the Amiga but cannot be opened by name · fixed 2026-09-14
- [ART-315](ISSUES-archive.md) · `libpfs3`'s data allocator accepts a block number up to `bitmapstart` past the partition · fixed 2026-09-14
- [ART-316](ISSUES-archive.md) · `libpfs3`'s `FormatOptions.enable_deldir` is never read · fixed 2026-09-14
- [ART-302](ISSUES-archive.md) · The first tab question of a session still read two same-size discs whole, every start · fixed 2026-09-14
- [ART-300](ISSUES-archive.md) · The refusal for an older package added over a newer one did not name the order · fixed 2026-09-14
- [ART-118](ISSUES-archive.md) · The OS Builder's install screen has never been driven in a real browser past its headings — jsdom now covers what a browser could not, the crash itself is still unresolved · found 2026-08-15, fix date not stated
- [ART-312](ISSUES-archive.md) · `libpfs3`'s writer can hand one anode number out twice in one operation, and a file then silently reads another block's bytes · fixed 2026-09-13
- [ART-310](ISSUES-archive.md) · `libpfs3` 0.1.3's format is wrong: a PFS3 partition over ~4.88 GiB is unmountable, and at every size anodes 0–4 are left unreserved · fixed 2026-09-13
- [ART-307](ISSUES-archive.md) · The PiStorm screens called the A1200 Kickstart wrong for an A500, and the card plan said such a machine "usually does not come up" — the ROM Emu68's guides recommend on every model · found 2026-09-11, fix date not stated
- [ART-309](ISSUES-archive.md) · The last partition of an RDB ART writes can end past the area it lives in · fixed 2026-09-11
- [ART-308](ISSUES-archive.md) · A card image was bigger than the card it is named for: "64" built 64 GiB, and a 64 GB card holds about 64 × 10⁹ bytes · fixed 2026-09-11
- [ART-306](ISSUES-archive.md) · Two card-plan warnings reached the screen with their names as `undefined`, and one took the wrong sentence · found 2026-09-11, fix date not stated
- [ART-305](ISSUES-archive.md) · The card builder refused any Emu68 archive but the one ART's table names — a prerelease, a nightly, a build the user is testing · found 2026-09-11, fix date not stated
- [ART-304](ISSUES-archive.md) · Contribution could never go on after BoingBag 3.9-2: two packages whose archives share a top-level name were taken for two copies of one · found 2026-09-11, fix date not stated
- [ART-303](ISSUES-archive.md) · Build runs a ticked update whose prerequisite is ticked but unresolved, and the core refuses it · found 2026-09-10, fix date not stated
- [ART-297](ISSUES-archive.md) · The OS Builder froze the window: every tab question re-hashed whole install discs, on the thread the window answers on · found 2026-09-10, fix date not stated
- [ART-298](ISSUES-archive.md) · Locale 3.9's Turkish slice was refused on the owner's tree — and the tick list had offered it · found 2026-09-10, fix date not stated
- [ART-299](ISSUES-archive.md) · After a run the Build tab said "no update ticked" and "replaces 0 files" while it was still finding out — and offered Build over that · found 2026-09-10, fix date not stated
- [ART-291](ISSUES-archive.md) · A `packages.folder` seeded from an older ART steers the archive dialogs even after the folder leaves the material list · fixed 2026-09-10
- [ART-296](ISSUES-archive.md) · The BoingBag preview staged its payload on the system drive, not under the chosen scratch root · fixed 2026-09-10
- [ART-295](ISSUES-archive.md) · Five production wrappers exist so tests need not name a root, and `scratch-guard-sweep.py` cannot see a test call to one · fixed 2026-09-10
- [ART-292](ISSUES-archive.md) · Browser back and forward are outside the run lock · fixed 2026-09-10
- [ART-293](ISSUES-archive.md) · A whole suite run still leaves one 794-byte scan-cache file under the scratch root · fixed 2026-09-10
- [ART-294](ISSUES-archive.md) · The first-boot rehearsal has no growth ceiling · fixed 2026-09-10
- [ART-285](ISSUES-archive.md) · "23 install disks found" counts game and CD32 discs as install disks · fixed 2026-09-10
- [ART-283](ISSUES-archive.md) · Opening the Files screen rewrites a remembered tab's location · fixed 2026-09-10
- [ART-278](ISSUES-archive.md) · A hung Amiga-side installer writes unbounded output into the staged copy, and nothing in ART bounds it or notices it · fixed 2026-09-10
- [ART-279](ISSUES-archive.md) · The `TimedOut` next step tells the user to watch the emulator window, which is wrong advice for an installer that is hung rather than waiting · fixed 2026-09-10
- [ART-281](ISSUES-archive.md) · The unit suite's scratch directories were never removed: `D:\tmp\art-tests` held 263 484 directories and 764 GB · fixed 2026-09-10
- [ART-289](ISSUES-archive.md) · The packages step's preview refused two copies of BoingBag 1 that the readout had already resolved · fixed 2026-09-09
- [ART-290](ISSUES-archive.md) · Component ticks lived in two stores, and the session's copy went stale after the first change · fixed 2026-09-09
- [ART-288](ISSUES-archive.md) · The headline feature of 0.9.1 refused on the owner's own material: the host placement resolved a package's archive by identity alone, and their folder holds two builds of BoingBag 3.9-1 · found 2026-09-09, fix date not stated
- [ART-287](ISSUES-archive.md) · "Checking what this would replace…" stayed on screen above the preview's own refusal · found 2026-09-09, fix date not stated
- [ART-286](ISSUES-archive.md) · "Checking what this would replace…" never resolves: the panel renders the host-placement preview for a row it never asks about · found 2026-09-09, fix date not stated
- [ART-166](ISSUES-archive.md) · Both BoingBag payload archives are password-encrypted ZIPs, so neither BoingBag recipe can place a single file · found 2026-08-19, fix date not stated
- [ART-282](ISSUES-archive.md) · Ticking `locale-turkish` on the Packages step, on a tree BoingBag 3.9-2 had not been run on, read the engine's own bookkeeping out at the owner · found 2026-09-08, fix date not stated
- [ART-284](ISSUES-archive.md) · Two screens gave two answers about one file: the source step's readout resolved BoingBag 3.9-1 to the owner's chosen archive, the updates chain called it ambiguous and blocked the row · fixed 2026-09-09
- [ART-280](ISSUES-archive.md) · ART's BoingBag 3.9-2 run never applied `XAD-Update`, so `xadmaster.library` stayed at 9.1 where every other 3.9 builder leaves it at 10+ · found 2026-09-08, fix date not stated
- [ART-277](ISSUES-archive.md) · A stale package selection carried the wrong archive into a request, and the refusal quoted an internal overlay path instead of naming either package: `'…BoingBag39-2.lha' is not this package's update archive: it carries none of 'BoingBag3.9-1-UAE/BoingBag3.9-1'; it holds BoingBag3.9-2, BoingBag3.9-2.info (ART-INPUT-INVALID)` · date not stated
- [ART-276](ISSUES-archive.md) · Two packages sharing one medium tripped the wrong clash check: `'Locale3.9' already names the medium component 'locale-39' was installed from in this tree` · fixed 2026-09-08
- [ART-275](ISSUES-archive.md) · The commander had no cursor keys: Up/Down, Home/End, PageUp/PageDown moved nothing, and the ini's one custom shortcut (Ctrl+Space) was not wired · fixed 2026-09-08
- [ART-261](ISSUES-archive.md) · `cargo test --lib` reports exit 0 with no `test result:` line whenever `commands::artwork` runs, and passes cleanly without it · found 2026-09-06, fix date not stated
- [ART-242](ISSUES-archive.md) · WHDLoad's one-click install writes and joins directly in `commands/whdload.rs`, with no `core`-level "install a pack" function to call instead · found 2026-09-05, fix date not stated
- [ART-241](ISSUES-archive.md) · Field hints, errors and outcome text sit beside a control, not associated with it · found 2026-09-05, fix date not stated
- [ART-248](ISSUES-archive.md) · `appearance_apply` runs the whole wallpaper pipeline synchronously on the command thread, with no progress and no cancel · found 2026-09-05, fix date not stated
- [ART-244](ISSUES-archive.md) · Update mode re-reads every archive candidate on every refresh, on an argued rather than measured basis · found 2026-09-05, fix date not stated
- [ART-243](ISSUES-archive.md) · An archive updated in place accumulates ghost records no Rescan can clear · found 2026-09-05, fix date not stated
- [ART-251](ISSUES-archive.md) · The AmigaOS 3.2 recipe has no rule for `Utilities` or `WBStartup`, so a tree ART builds has neither · found 2026-09-06, fix date not stated
- [ART-274](ISSUES-archive.md) · `core/winuae.rs` spawns an external process from inside `core/`, the exact shape the trait rule exists to prevent · found 2026-09-07, fix date not stated
- [ART-246](ISSUES-archive.md) · `verify_volume`'s prefs check has an untested failure path: a permission error walking the tree folds into one `Fail` row with no test provoking it · found 2026-09-05, fix date not stated
- [ART-247](ISSUES-archive.md) · A PNG decode arm the `png` crate's own `EXPAND` transform should make unreachable has no test of its own · found 2026-09-05, fix date not stated
- [ART-245](ISSUES-archive.md) · A missing backdrop and a wrong-type match at the same name read as the same sentence · found 2026-09-05, fix date not stated
- [ART-260](ISSUES-archive.md) · `setLayerIdentified({})` writes a fresh object where its two neighbours guard with `prev => prev`, and nothing wakes it yet · found 2026-09-06, fix date not stated
- [ART-235](ISSUES-archive.md) · The test-scratch sweep reports a site that is not a defect · found 2026-09-04, fix date not stated
- [ART-271](ISSUES-archive.md) · `control-byte-sweep.py` does not look for the one control byte CLAUDE.md's own incident report names first · found 2026-09-06, fix date not stated
- [ART-273](ISSUES-archive.md) · The first-boot dispatcher tried to delete itself on `done all`, and AmigaDOS refused: the file was still being executed · found 2026-09-07, fix date not stated
- [ART-272](ISSUES-archive.md) · The fixed first-boot scripts were written from recalled AmigaDOS and could not run past their first step under a real shell · found 2026-09-07, fix date not stated
- [ART-270](ISSUES-archive.md) · Two new MD5 tests leaked their scratch directory on a panic, and so did the two sha256 tests they copied the pattern from · found 2026-09-06, fix date not stated
- [ART-269](ISSUES-archive.md) · A doc comment named one cause for a state that has two · found 2026-09-06, fix date not stated
- [ART-268](ISSUES-archive.md) · Two layers pointed at one folder identified it twice and rendered every line twice under the same React key · found 2026-09-06, fix date not stated
- [ART-267](ISSUES-archive.md) · `--emit-confirmed` replaced ART's own confirmation record instead of adding to it, discarding an earlier run's material the moment a second one was made · found 2026-09-06, fix date not stated
- [ART-266](ISSUES-archive.md) · "151 of the table's 186 rows" was two literal digits in two catalogues, tied to nothing that would fail if the data changed · found 2026-09-06, fix date not stated
- [ART-265](ISSUES-archive.md) · Both media-table checks passed with 0 verified, so neither could fail on the thing it exists to check · found 2026-09-06, fix date not stated
- [ART-264](ISSUES-archive.md) · One unreadable folder discarded every folder already identified, and the screen said the whole pass failed · found 2026-09-06, fix date not stated
- [ART-263](ISSUES-archive.md) · Pressing Stop on the media-identification pass said ART had failed · found 2026-09-06, fix date not stated
- [ART-262](ISSUES-archive.md) · The module the hash round reversed still argued, in ART's own voice, that ART does not hash · found 2026-09-06, fix date not stated
- [ART-259](ISSUES-archive.md) · ART-257's own evidence check ran after the branch it was meant to reach through, so it never ran for the one folder it exists for · found 2026-09-06, fix date not stated
- [ART-258](ISSUES-archive.md) · `sameRelease` called every disk in the folder "the right media for this release" · found 2026-09-06, fix date not stated
- [ART-257](ISSUES-archive.md) · A layered release got no evidence line at all, and the union alone would have made it say the wrong thing · found 2026-09-06, fix date not stated
- [ART-256](ISSUES-archive.md) · The OS Builder's evidence line described one media folder while the plan had been built from several · found 2026-09-06, fix date not stated
- [ART-255](ISSUES-archive.md) · The unidentified-folder sentence stated a reason that is false in three of the four states that reach it · found 2026-09-06, fix date not stated
- [ART-254](ISSUES-archive.md) · A release switch could bring ART-253's false sentence back through a different door — the evidence was never asked which release it was about · found 2026-09-06, fix date not stated
- [ART-253](ISSUES-archive.md) · `wrongMediaFolder` told a user with the *right* media folder that none of its disks were ones the release asks for — two of its five conditions could never fire · found 2026-09-06, fix date not stated
- [ART-252](ISSUES-archive.md) · `rendered_size` tested `layout().trailing.start` for an appended ColorIcon's `FORM` tag, but a present `DrawerData2` sits there instead — every container icon with both fell back to the `Gadget` size this module exists to stop trusting · found 2026-09-06, fix date not stated
- [ART-249](ISSUES-archive.md) · `set_show_all_files` wrote `dd_Flags` at a fixed offset that lands inside `DrawerData`'s own `NewWindow` struct, overwriting drawer window geometry instead of the Show-mode flag · found 2026-09-06, fix date not stated
- [ART-236](ISSUES-archive.md) · A component's `removes` deletes the file and leaves its `.uaem` sidecar behind · found 2026-09-05, fix date not stated
- [ART-237](ISSUES-archive.md) · An `aria-label` sits on a bare `div` in the OS Builder's media step · found 2026-09-05, fix date not stated
- [ART-238](ISSUES-archive.md) · Nothing guarded the 3.2.2 recipe's `overrides: ["modules-a1200"]` · found 2026-09-05, fix date not stated
- [ART-239](ISSUES-archive.md) · A cross-layer exclusive-group conflict could never be refused · found 2026-09-05, fix date not stated
- [ART-240](ISSUES-archive.md) · Every "Remove" button for an added media folder carried the same accessible name · found 2026-09-05, fix date not stated
- [ART-226](ISSUES-archive.md) · Every tree ART builds has an empty `Devs/Keymaps`, so whatever language the user chose they can only type on an American keyboard — and for Turkish the keymap is not on the CD at all · found 2026-08-24, fix date not stated
- [ART-231](ISSUES-archive.md) · The WiFi credentials were written to a drawer no Amiga opens · fixed 2026-08-24
- [ART-230](ISSUES-archive.md) · A comment in the card builder said its images were free, and they cost their full size · fixed 2026-08-24
- [ART-229](ISSUES-archive.md) · An AmigaOS 3.1 folder was announced as the user's AmigaOS 3.2 folder · fixed 2026-08-24
- [ART-130](ISSUES-archive.md) · A game can name the Kickstart it needs, and nothing offers to supply it · date not stated
- [ART-234](ISSUES-archive.md) · Four alphabets got their catalogs and none of their letters: the AmigaOS 3.2 recipe placed no `Support/Fonts` at all · found 2026-08-24, fix date not stated
- [ART-233](ISSUES-archive.md) · The second Amiga disk's size arrived as an unknown field and became "take the rest", and nothing failed · found 2026-08-24, fix date not stated
- [ART-232](ISSUES-archive.md) · The Hard Disk studio's "which partition did I pick" ring is 2.52:1 on two of its five colours · found 2026-08-24, fix date not stated
- [ART-228](ISSUES-archive.md) · Every AmigaOS 3.2 tree ART builds carries 3 263 files the release's own Installer would have decompressed and renamed — the whole help system, and for a Turkish user the fonts and catalogs too · found 2026-08-24, fix date not stated
- [ART-227](ISSUES-archive.md) · ART runs a BoingBag's `Updater` exactly the way the one readable distribution builder does, and then stops — the fix-ups that builder performs *after* it are not done, and one of them is what activates BoingBag 2's ROM update · found 2026-08-24, fix date not stated
- [ART-225](ISSUES-archive.md) · Thirteen font descriptors were placed as `X.FONT` and the running Amiga could see none of them — `diskfont.library` matches the `.font` suffix case-sensitively · found 2026-08-23, fix date not stated
- [ART-159](ISSUES-archive.md) · Two of spec §5's three predicted hazards for AmigaOS 3.9 — `SetPatch`/the boot sequence, and the three language-variant trees — went untouched by every task on the branch and were recorded nowhere · found 2026-08-19, fix date not stated
- [ART-224](ISSUES-archive.md) · Two AmigaOS 3.2 components declared an override over `storage` and were declared above it, so both overrides silently did nothing — and one of them cost sixteen icons a user had asked for · found 2026-08-23, fix date not stated
- [ART-187](ISSUES-archive.md) · A cancelled Amiga-side install leaves the last phase line on screen under a badge that says nothing about it · found 2026-08-21, fix date not stated
- [ART-184](ISSUES-archive.md) · The test fixtures leak a scratch directory per run, for ever, and filled a 2 TB drive · found 2026-08-20, fix date not stated
- [ART-183](ISSUES-archive.md) · A misspelled key in a release recipe is still dropped in silence · date not stated
- [ART-179](ISSUES-archive.md) · Twenty-eight catalogue keys nothing renders · found 2026-08-20, fix date not stated
- [ART-172](ISSUES-archive.md) · The content layer's spec §8.4 hazard — a language pack colliding with the base `Locale` — was never exercised either, and the run that looked like it did was measuring a mangled name · date not stated
- [ART-171](ISSUES-archive.md) · The content layer's spec §8.3 hazard — `WBStartup` and `Devs` arriving on a tree for the first time — was never exercised, because no package file ever reached a tree · date not stated
- [ART-060](ISSUES-archive.md) · Rust-side error sentences do not translate · date not stated
- [ART-223](ISSUES-archive.md) · The same Kickstart was asked for three times · fixed 2026-08-23
- [ART-222](ISSUES-archive.md) · A different disc of the same name built silently while the screen described the old one · found 2026-08-23, fix date not stated
- [ART-221](ISSUES-archive.md) · Every tree ART built had its drivers on the shelf and none of them switched on · found 2026-08-23, fix date not stated
- [ART-220](ISSUES-archive.md) · The storage overlay block inherited whatever filter the user's `config.txt` left open · found 2026-08-23, fix date not stated
- [ART-219](ISSUES-archive.md) · Twelve sentences the user reads carried a run of fourteen spaces in the middle · found 2026-08-23, fix date not stated
- [ART-218](ISSUES-archive.md) · The card ART built had one partition, which is the one shape neither working card has · fixed 2026-08-23
- [ART-217](ISSUES-archive.md) · The card builder could only write the filesystem nobody else uses · date not stated
- [ART-215](ISSUES-archive.md) · A card ART builds would stop honouring its own storage settings the day the user updates Emu68 · found 2026-08-23, fix date not stated
- [ART-216](ISSUES-archive.md) · A committed doc block told the reader to run a command against a path with a BEL byte in it · found 2026-08-23, fix date not stated
- [ART-196](ISSUES-archive.md) · ART wrote its scratch to the system drive and the user could not move it · fixed 2026-08-22
- [ART-205](ISSUES-archive.md) · The predicted size of a distribution was larger than the one that landed, whenever a component overrode another · found 2026-08-22, fix date not stated
- [ART-214](ISSUES-archive.md) · The download folder was named for one of its two users, and its hint described a folder ART does not choose · found 2026-08-22, fix date not stated
- [ART-213](ISSUES-archive.md) · A cold-cache download over an occupied library slot was reported as `Downloaded`, and the same gap let one narrower case through even after that was fixed · fixed 2026-08-22
- [ART-212](ISSUES-archive.md) · A package chosen for another release still drove a run plan · found 2026-08-22, fix date not stated
- [ART-211](ISSUES-archive.md) · The release the user picked and the release the build carried were two variables · found 2026-08-22, fix date not stated
- [ART-210](ISSUES-archive.md) · Everything one release's screen had computed stayed on screen after switching to another · found 2026-08-22, fix date not stated
- [ART-209](ISSUES-archive.md) · A 3.2 build was offered AmigaOS 3.9's BoingBags · found 2026-08-22, fix date not stated
- [ART-208](ISSUES-archive.md) · Sixteen true refusals that together said something false · found 2026-08-22, fix date not stated
- [ART-207](ISSUES-archive.md) · A media folder outlived the release it belonged to · found 2026-08-22, fix date not stated
- [ART-206](ISSUES-archive.md) · A real-material check that could not pass at all · found 2026-08-22, fix date not stated
- [ART-203](ISSUES-archive.md) · A distribution tree cannot be built from the screen at all: the folder picker can only return a folder that exists, and `apply` refuses every folder that exists · found 2026-08-22, fix date not stated
- [ART-204](ISSUES-archive.md) · `config.txt` is a conditional-section format and ART's merge treats it as a flat one, so a real Emu68 config comes out with three of its four boards unbootable · found 2026-08-22, fix date not stated
- [ART-202](ISSUES-archive.md) · The answer to a button at the bottom of the panel renders at the top of it, so pressing it looks like nothing happened · found 2026-08-22, fix date not stated
- [ART-201](ISSUES-archive.md) · The preview describes a run ART already knows cannot happen, because it never opens the archive it would run from · found 2026-08-22, fix date not stated
- [ART-200](ISSUES-archive.md) · ART names an archive, the user fetches it, and ART refuses it with a sentence that does not say it belongs in the other field · found 2026-08-22, fix date not stated
- [ART-199](ISSUES-archive.md) · A step reports itself ready on a folder that is not a distribution tree, so the refusal arrives on the button instead of at the field · found 2026-08-22, fix date not stated
- [ART-197](ISSUES-archive.md) · The tree ART has just built is not carried to the step that needs it, so the user is asked to go and find ART's own output · found 2026-08-21, fix date not stated
- [ART-198](ISSUES-archive.md) · A sentence promises an official update and offers an unofficial one as its example, in both catalogues · found 2026-08-21, fix date not stated
- [ART-178](ISSUES-archive.md) · `useRemembered` hands back a fresh array identity when the persisted value lands, so every effect that depends on one runs twice with an identical request · fixed 2026-08-21
- [ART-195](ISSUES-archive.md) · The OS Builder started a preview on every render, not on every user action — 2,149 preview jobs in one session, and pressing Stop appeared to start another · found 2026-08-21, fix date not stated
- [ART-194](ISSUES-archive.md) · The OS Builder walked the whole medium on every preview, though the machinery to avoid it already existed · date not stated
- [ART-152](ISSUES-archive.md) · ART sized a WHDLoad launch's machine from the catalogue's own chipset — the Amiga the *game* was written for, not the Amiga WHDLoad runs on. Closed by one named, known-good WHDLoad machine profile instead of the per-title `ws_ExpMem` reading it was filed suggesting · date not stated
- [ART-193](ISSUES-archive.md) · A BoingBag's `Updater` started, printed nothing, opened nothing and never returned — because ART's script had never run the tree's own `AddDataTypes` · found 2026-08-21, fix date not stated
- [ART-192](ISSUES-archive.md) · The run built no `ENV:`, so a real installer stopped on a System Request nobody could answer · found 2026-08-21, fix date not stated
- [ART-191](ISSUES-archive.md) · `LIBS:` carried only the tree's `Libs`, so `resource.library` could not initialise and a BoingBag `Updater` refused to start · found 2026-08-21, fix date not stated
- [ART-190](ISSUES-archive.md) · The re-run guard read a marker written above `SetPatch`, and `SetPatch` reboots — so the installer was never invoked at all, and the run was on its way to being reported as a timeout · found 2026-08-21, fix date not stated
- [ART-189](ISSUES-archive.md) · ART's generated boot never ran the tree's own `SetPatch`, so an AmigaOS 3.9 tree met a 3.1 ROM · found 2026-08-21, fix date not stated
- [ART-188](ISSUES-archive.md) · A return code of 900 aborted ART's own script before it could report, so an installer that refused was on its way to being reported as a timeout · found 2026-08-21, fix date not stated
- [ART-186](ISSUES-archive.md) · Nothing enforced the BoingBag order, and nothing refused the `Updater` that cannot run under an emulator · date not stated
- [ART-185](ISSUES-archive.md) · Nothing mounted the package, so the installer would never have started — and ART would have reported that it ran and refused · found 2026-08-21, fix date not stated
- [ART-182](ISSUES-archive.md) · A blocking-CI flake: sixteen tests shared one staging namespace · date not stated
- [ART-181](ISSUES-archive.md) · Every user file in ART was written through a temp name that two threads could share · fixed 2026-08-20
- [ART-180](ISSUES-archive.md) · The dead-key allow-list could not tell an excuse from a stale one · found 2026-08-20, fix date not stated
- [ART-080](ISSUES-archive.md) · ART cannot delete a file on the user's own disk, so nothing can be moved *off* a host folder · date not stated
- [ART-081](ISSUES-archive.md) · A single file cannot be moved between two images, because the primitive underneath addresses a directory · fixed 2026-08-20
- [ART-175](ISSUES-archive.md) · The OS Builder can preview what a package would replace and still cannot preview what switching a recipe component on would replace · fixed 2026-08-20
- [ART-143](ISSUES-archive.md) · A hand-attached picture is not re-materialised if the artwork cache is deleted · fixed 2026-08-20
- [ART-144](ISSUES-archive.md) · Five minors deferred across collection-wave-c's own review rounds, folded into one entry — all five now closed · fixed 2026-08-18
- [ART-119](ISSUES-archive.md) · Five minors deferred from Task 13's review, folded into one entry — all five now closed · fixed 2026-08-15
- [ART-174](ISSUES-archive.md) · Two more breakpoints ask the real viewport a question about the zoomed layout · fixed 2026-08-20
- [ART-093](ISSUES-archive.md) · ART cannot fetch an Emu68 kernel update; it can only tell you which one you need · date not stated
- [ART-176](ISSUES-archive.md) · F5 between two images means two different things for one entry and for several · found 2026-08-20, fix date not stated
- [ART-177](ISSUES-archive.md) · A layout apply still cannot be resumed — the residue is reported, and there is no way to carry on from it · found 2026-08-20, fix date not stated
- [ART-069](ISSUES-archive.md) · No frontend test renders `FileManager.tsx` · date not stated
- [ART-110](ISSUES-archive.md) · A partial layout apply cannot be resumed, and the screen stays busy · found 2026-08-15, fix date not stated
- [ART-108](ISSUES-archive.md) · Nothing you drop can reach the layout screen · found 2026-08-15, fix date not stated
- [ART-109](ISSUES-archive.md) · `core/layout`'s WHDLoad tests never use LHA, and its `outside` test does not discriminate · found 2026-08-15, fix date not stated
- [ART-106](ISSUES-archive.md) · A WHDLoad icon's destination is invisible to collision analysis · found 2026-08-15, fix date not stated
- [ART-073](ISSUES-archive.md) · `delete_many`'s all-or-nothing guarantee only holds for the whole-file strategy · date not stated
- [ART-065](ISSUES-archive.md) · Volume→local multi-select is several concurrent operations, not one · date not stated
- [ART-064](ISSUES-archive.md) · Volume→volume multi-select refuses rather than batching · date not stated
- [ART-050](ISSUES-archive.md) · The §92 pre-flight gate does not check bitmap consistency or hash-chain integrity · date not stated
- [ART-078](ISSUES-archive.md) · An AmigaOS CD's protection bits and file comments are lost, because Rock Ridge and the Amiga `AS` entry are not read · date not stated
- [ART-161](ISSUES-archive.md) · The same disc is fully walked three to four times per install, because `scan::identify` opens a `CdSource` to read one string and then drops it · found 2026-08-19, fix date not stated
- [ART-170](ISSUES-archive.md) · `collide::preview` can only be asked about a · found 2026-08-19, fix date not stated
- [ART-157](ISSUES-archive.md) · The recipe format cannot state a Kickstart *minimum*, only a maximum, so AmigaOS 3.9's real requirement (V40 or newer) goes unstated and unchecked · found 2026-08-19, fix date not stated
- [ART-167](ISSUES-archive.md) · Eight of the owner's archives claim the top-level directory `LocaleUpdate` and two claim `BoingBag3.9-2`, so `scan::package_for` correctly refuses two of the three shipped packages and nothing in the product can pick between the candidates · found 2026-08-19, fix date not stated
- [ART-087](ISSUES-archive.md) · Space marks a row but does not compute a directory's size · fixed 2026-08-20
- [ART-101](ISSUES-archive.md) · The sidebar's collapse never fires under Application Size · fixed 2026-08-20
- [ART-107](ISSUES-archive.md) · `scan::gather` drops silently at the depth cap, and counts an overlapping input twice · fixed 2026-08-20
- [ART-105](ISSUES-archive.md) · `size()` is written three times · fixed 2026-08-20
- [ART-158](ISSUES-archive.md) · `CoreError::Malformed` covered two different failure classes for an ISO9660 disc — a corrupt structure, and a disc merely larger than `CdSource`'s own walk limits · fixed 2026-08-20
- [ART-173](ISSUES-archive.md) · `core::cbm`'s and `core::detect`'s test scratch directories can be shared by two threads, so one test reads another's fixture — measured at 4 failures in 40 runs · fixed 2026-08-20
- [ART-115](ISSUES-archive.md) · A `core::iso` test flake, seen three times across this session, never diagnosed · fixed 2026-08-20
- [ART-156](ISSUES-archive.md) · `plan()`'s `total_bytes` counts a CD-sourced directory's own ISO9660 extent length as if it were file content, so it overstates what `apply()` actually writes to disk · fixed 2026-08-20
- [ART-160](ISSUES-archive.md) · `osinstall::apply()` writes host filenames without going through `windows_safe_name`, and the one machine that measured a reserved device name is not every machine · fixed 2026-08-20
- [ART-168](ISSUES-archive.md) · An LHA entry name's non-ASCII bytes are replaced with U+FFFD rather than decoded, so a real Amiga drawer name becomes a name no Amiga can see · fixed 2026-08-20
- [ART-164](ISSUES-archive.md) · `core::iso`'s test scratch directory can be shared by two threads, so *any* test in the module can read another's fixture — first measured at about one full-suite run in thirty, and re-measured at four different tests failing this way · found 2026-08-19, fix date not stated
- [ART-169](ISSUES-archive.md) · `workbench-base` placed only the disc's `Workbench3.5` half, never its `Workbench3.9` overlay, so the tree ART called "AmigaOS 3.9" booted as Workbench 44.5 with a Startup-Sequence that failed on its first command · found 2026-08-19, fix date not stated
- [ART-165](ISSUES-archive.md) · Every `on*` subscription wrapper's fire-and-forget `.then((fn) => { unlisten = fn })` pattern (or an async-IIFE variant of the same shape) could leak a live Tauri listener, surface an unhandled promise rejection, or both — the product-code defect ART-163's own test symptom was standing in front of · found 2026-08-19, fix date not stated
- [ART-163](ISSUES-archive.md) · `pnpm test` exited non-zero while every test passed, because ten jsdom-rendered tests never mocked the Tauri `listen()` shim their components subscribed through, and each one left an unhandled promise rejection behind · found 2026-08-19, fix date not stated
- [ART-162](ISSUES-archive.md) · The AmigaOS 3.9 recipe placed nothing from the disc's own `Locale` drawer, which made the same task's `locale-turkish` package inert — no `.language`/`.country` file existed anywhere on the built tree for `locale.library` to select *any* non-English locale by, Turkish included · found 2026-08-19, fix date not stated
- [ART-155](ISSUES-archive.md) · A real AmigaOS 3.9 disc names files `apply()` could not write as literal Windows path segments — three accented letters ART's own ISO9660 reader turned into `?`, a character Windows refuses in a path · fixed 2026-08-19
- [ART-154](ISSUES-archive.md) · `apply()` hashed a whole medium into memory just to record its SHA-256 — 469 MB for the real AmigaOS 3.9 disc, against CLAUDE.md's own rule that ART never reads a whole user file into memory · fixed 2026-08-19
- [ART-153](ISSUES-archive.md) · `apply()` cannot build a distribution tree from disc media — it opens every medium `plan.media_paths` names through `AdfSource::open` unconditionally, never `scan::open_media` · found 2026-08-19, fix date not stated
- [ART-151](ISSUES-archive.md) · A WHDLoad launch got the stock A500 profile completely unmodified — 512 KB Chip, 512 KB Slow, no Fast RAM at all — and WHDLoad itself refused to load the game for want of memory · fixed 2026-08-18
- [ART-149](ISSUES-archive.md) · A fix built on a correct mechanism and a wrong inference changed the bare-hardfile geometry from `sectors=32` to `sectors=1`, and it broke the very titles it meant to fix — reverted, and the correct geometry restored with a measurement to settle it for good · fixed 2026-08-18
- [ART-148](ISSUES-archive.md) · A WHDLoad title's machine was chosen with no floor on the Kickstart it boots, so a name-sorted ROM-folder scan could hand it one older than WHDLoad itself requires · fixed 2026-08-18
- [ART-147](ISSUES-archive.md) · A self-booting WHDLoad hardfile was catalogued as an unpacked drawer, sending Play looking for a system volume the file never needed — and shipping the fix broke every catalogue that already existed · fixed 2026-08-18
- [ART-146](ISSUES-archive.md) · `hardfile2=` forced bare-image geometry onto every hard drive image, including a VHD container — WinUAE reported "Not a DOS disk in unit 0" · fixed 2026-08-18
- [ART-145](ISSUES-archive.md) · The one-click WHDLoad launch never got past the CLI: the generated startup-sequence could not run its own first line · fixed 2026-08-18
- [ART-142](ISSUES-archive.md) · A comma in a mounted folder's path shifts every field after it in the generated WinUAE configuration · date not stated
- [ART-141](ISSUES-archive.md) · Play mounted an `.rp9`'s hardfile as the zip package itself, writable · date not stated
- [ART-140](ISSUES-archive.md) · The palette was chosen by eye, and the light theme put 2.20:1 text inside its own success badge · fixed 2026-08-18
- [ART-138](ISSUES-archive.md) · ROM Manager said `CRC ERR` about ROMs it simply did not recognise · found 2026-08-18, fix date not stated
- [ART-139](ISSUES-archive.md) · Aminet's text inputs rendered white in the dark theme · found 2026-08-18, fix date not stated
- [ART-137](ISSUES-archive.md) · 99 of 758 records reported a Kickstart image whose name was 68000 machine code — `ws_kickname` is a list when `ws_kickcrc` is `$ffff` · found 2026-08-18, fix date not stated
- [ART-136](ISSUES-archive.md) · The ADF path assumed TOSEC filenames; of 847 real ADFs, none are TOSEC — so one game became five entries and artwork matched 3 % · found 2026-08-18, fix date not stated
- [ART-135](ISSUES-archive.md) · One politeness rule for every host, and four pictures fetched for the one the screen shows: a one-minute job took forty · found 2026-08-17, fix date not stated
- [ART-134](ISSUES-archive.md) · The artwork index was written only at the end of an hour-long run, so an interruption orphaned every picture it had fetched · found 2026-08-17, fix date not stated
- [ART-133](ISSUES-archive.md) · `window.confirm` asked nothing, so thirteen confirmations never fired — four of them in front of a delete · found 2026-08-17, fix date not stated
- [ART-132](ISSUES-archive.md) · Three things the Collection screen got wrong, all found in the first minute anyone looked at it · found 2026-08-17, fix date not stated
- [ART-131](ISSUES-archive.md) · A bare hardfile whose filesystem is smaller than the file would not mount — 1456 of the user's 1697 WHDLoad hardfiles · found 2026-08-17, fix date not stated
- [ART-129](ISSUES-archive.md) · The ROM pairing check stayed silent for the pairing it exists to warn about — twice over · fixed 2026-08-17
- [ART-128](ISSUES-archive.md) · A licensed Amiga Forever ROM went onto the card encrypted, and the card could not boot · fixed 2026-08-17
- [ART-104](ISSUES-archive.md) · ART's ROM database matched none of the user's 29 Kickstart dumps · fixed 2026-08-16
- [ART-124](ISSUES-archive.md) · `apply()` reported how many plan items it ran, not what the tree holds — and the manifest carried 94 paths twice · found 2026-08-16, fix date not stated
- [ART-125](ISSUES-archive.md) · A fallback copy reported zero bytes, and the screen printed that as a fact · found 2026-08-16, fix date not stated
- [ART-126](ISSUES-archive.md) · Every RDB filesystem ART has ever embedded was ignored by AmigaOS: `PatchFlags` named the wrong field · fixed 2026-08-16
- [ART-127](ISSUES-archive.md) · The tree G5 builds could not start Workbench: two libraries missing, and the wallpapers left off on an assumption · fixed 2026-08-16
- [ART-122](ISSUES-archive.md) · `hst-imager` cannot write into a volume `NativeFormatter` just formatted — the first copy dies `ERROR_DISK_FULL`, and it dies *after* the destructive format has already run · fixed 2026-08-16
- [ART-123](ISSUES-archive.md) · A failed `hst-imager` command reported a stack frame instead of what went wrong · fixed 2026-08-16
- [ART-121](ISSUES-archive.md) · ART-120's own fix wave, reviewed — four findings, folded into one entry · fixed 2026-08-16
- [ART-120](ISSUES-archive.md) · `NativeFormatter` was unreachable from the application — every preload a user ran still shelled out to `hst-imager` · fixed 2026-08-16
- [ART-116](ISSUES-archive.md) · ART's PFS3 writer carries protection bits but drops a `.uaem`'s comment and date; the FFS branch keeps both · found 2026-08-16, fix date not stated
- [ART-113](ISSUES-archive.md) · `libpfs3` 0.1.3 writes an entry's name as UTF-8 and reads it back as Latin-1 — any non-ASCII AmigaDOS name fails to copy in · found 2026-08-16, fix date not stated
- [ART-114](ISSUES-archive.md) · `hst-imager`'s `fs copy` extraction silently drops any entry whose name matches a Windows/MS-DOS reserved device basename, and the oracle that depends on it reported the drop as an unexplained shortfall · fixed 2026-08-16
- [ART-112](ISSUES-archive.md) · `glowicons` did not declare an override over `classes`, so a real card refused to build · fixed 2026-08-16
- [ART-111](ISSUES-archive.md) · The `storage` component's rules named a `Storage/` drawer the real disk does not have · fixed 2026-08-16
- [ART-099](ISSUES-archive.md) · Application Size cut the right-hand edge off every screen · fixed 2026-08-14
- [ART-103](ISSUES-archive.md) · ART wrote `kernel=Emu68.img` over the release's own line, and the card would not boot · fixed 2026-08-14
- [ART-102](ISSUES-archive.md) · `fatfs` writes two things wrong in every directory it creates · date not stated
- [ART-043](ISSUES-archive.md) · A partition inside a small image was written at the wrong offset · fixed 2026-08-13
- [ART-066](ISSUES-archive.md) · `archives_plan_install` unpacked the whole batch on the Tauri command thread · fixed 2026-08-13
- [ART-058](ISSUES-archive.md) · A cancelled block-journal copy did not tell the user files had already landed · fixed 2026-08-13
- [ART-070](ISSUES-archive.md) · `refresh(side)` moved keyboard focus to the pane it refreshed · fixed 2026-08-13
- [ART-068](ISSUES-archive.md) · The filter box told "empty" from "no match" by comparing entry counts · fixed 2026-08-13
- [ART-067](ISSUES-archive.md) · A batch archive install could not be stopped mid-archive · fixed 2026-08-13
- [ART-049](ISSUES-archive.md) · `create.rs`'s oracle hook and `VolumeWriter::open` agreed by hand, not by a check · fixed 2026-08-13
- [ART-085](ISSUES-archive.md) · A studio forgot the image it had open the moment you left the screen · fixed 2026-08-13
- [ART-096](ISSUES-archive.md) · ART wrote `MaxTransfer` and `Mask` as zero, and 100 buffers · fixed 2026-08-13
- [ART-100](ISSUES-archive.md) · The PiStorm screen went grey without saying why · fixed 2026-08-13
- [ART-098](ISSUES-archive.md) · CI's licence gate could never pass, and the build and the installer never ran · fixed 2026-08-13
- [ART-097](ISSUES-archive.md) · A card may carry several RDBs, and ART models one — so it would report fifteen working partitions as broken · fixed 2026-08-13
- [ART-095](ISSUES-archive.md) · ART cannot open a real PiStorm card image at all · fixed 2026-08-13
- [ART-094](ISSUES-archive.md) · Overwriting a write-protected file is not checked either · fixed 2026-08-13
- [ART-092](ISSUES-archive.md) · A named PiStorm firmware set cannot be deleted from ART · fixed 2026-08-13
- [ART-091](ISSUES-archive.md) · ART named an Emu68 archive that has never existed, and the name that does exist means a different board in each release line · fixed 2026-08-13
- [ART-090](ISSUES-archive.md) · The PiStorm screen offered controls Emu68 does not have, and wrote tokens it does not read · fixed 2026-08-12
- [ART-089](ISSUES-archive.md) · Session restore could not work, and destroyed the session it was meant to restore · fixed 2026-08-12
- [ART-088](ISSUES-archive.md) · The volume writer deletes a delete-protected entry without noticing the bit · fixed 2026-08-13
- [ART-086](ISSUES-archive.md) · Every path in Settings had to be typed by hand · fixed 2026-08-12
- [ART-084](ISSUES-archive.md) · An HDF created as PFS3 or SFS is a DosType with no filesystem behind it, and an Amiga cannot mount it · fixed 2026-08-12
- [ART-083](ISSUES-archive.md) · The New HDF wizard capped disk size at 8 GB, and nothing in the engine asked it to · fixed 2026-08-12
- [ART-082](ISSUES-archive.md) · The Files panes filled the window; their listings did not · fixed 2026-08-12
- [ART-072](ISSUES-archive.md) · Selection collision checks compare names case-sensitively, so `Docs` and `docs` are not caught · fixed 2026-08-13
- [ART-071](ISSUES-archive.md) · A selection of only symlinks copies nothing and reports success · fixed 2026-08-13
- [ART-061](ISSUES-archive.md) · `formatAge` is always plural in English · fixed 2026-08-13
- [ART-079](ISSUES-archive.md) · A 7z archive from any real tool gave one entry another entry's bytes · date not stated
- [ART-077](ISSUES-archive.md) · The file manager ignored the object a workflow sent it, so "Open in the file manager" opened nothing · date not stated
- [ART-076](ISSUES-archive.md) · Content-first detection never actually recognised an LHA, and its test could not tell · date not stated
- [ART-075](ISSUES-archive.md) · A raw CD image in Mode 2 Form 1 would be misread, and two layers would be wrong together · date not stated
- [ART-074](ISSUES-archive.md) · An accented filename came back corrupted · date not stated
- [ART-047](ISSUES-archive.md) · Dead code that clippy cannot see · date not stated
- [ART-048](ISSUES-archive.md) · A source comment still described a module that no longer exists · date not stated
- [ART-051](ISSUES-archive.md) · `FEATURES.md` carries raw control bytes and git treats it as binary · date not stated
- [ART-063](ISSUES-archive.md) · ART could not write a disk an Amiga would boot from · date not stated
- [ART-059](ISSUES-archive.md) · A flaky test could fail CI at random · date not stated
- [ART-037](ISSUES-archive.md) · ADF Studio could not open any bootable ADF · date not stated
- [ART-038](ISSUES-archive.md) · HD ADFs reported half their capacity · date not stated
- [ART-039](ISSUES-archive.md) · Disabled controls were indistinguishable from active ones · date not stated
- [ART-040](ISSUES-archive.md) · Content was clipped instead of scrolled, and nothing scaled · date not stated
- [ART-044](ISSUES-archive.md) · WHDLoad and Aminet installs could land a package partially with no warning · date not stated
- [ART-045](ISSUES-archive.md) · A transient re-read after a successful write could report it as failed · date not stated
- [ART-052](ISSUES-archive.md) · A cancelled install committed half a package and reported success · date not stated
- [ART-053](ISSUES-archive.md) · `VolumeWriter::all_bytes()` allocated from an unchecked block count · date not stated
- [ART-046](ISSUES-archive.md) · A doc comment claims a guarantee the public API does not give · date not stated
- [ART-054](ISSUES-archive.md) · The WHDLoad refusal panel contradicted itself · date not stated
- [ART-055](ISSUES-archive.md) · The install pre-flight guard had no test that would fail if it were deleted · date not stated
- [ART-056](ISSUES-archive.md) · The sidebar clipped the page on any window shorter than its own nav list · date not stated
- [ART-057](ISSUES-archive.md) · Two more controls looked enabled while disabled · date not stated
- [ART-042](ISSUES-archive.md) · A write that produced an invalid volume was committed anyway · date not stated
- [ART-041](ISSUES-archive.md) · Validation measured every image against a DD floppy · date not stated
- [ART-036](ISSUES-archive.md) · Names with accented characters were refused as too long · date not stated
- [ART-033](ISSUES-archive.md) · Every ADF ART wrote had invalid block checksums · date not stated
- [ART-034](ISSUES-archive.md) · Files ART added to a disk were zero bytes on a real Amiga · date not stated
- [ART-035](ISSUES-archive.md) · The free-space bitmap was laid out the wrong way round · date not stated
- [ART-032](ISSUES-archive.md) · RDB partitions described the wrong DosType and boot priority · date not stated
- [ART-030](ISSUES-archive.md) · Mirror failover concatenated a dead mirror's bytes onto the next one's · date not stated
- [ART-031](ISSUES-archive.md) · LHA archives with level 2 or 3 headers could not be opened at all · date not stated
- [ART-021](ISSUES-archive.md) · Creating or opening an HDF allocated the whole image in memory · date not stated
- [ART-022](ISSUES-archive.md) · Creating an image silently destroyed an existing one · date not stated
- [ART-023](ISSUES-archive.md) · A tiny requested size aborted the application · date not stated
- [ART-024](ISSUES-archive.md) · The RDSK block described a zero-capacity disk to AmigaOS · date not stated
- [ART-025](ISSUES-archive.md) · Empty RDB block lists pointed at block 0 · date not stated
- [ART-026](ISSUES-archive.md) · Oversized partitions were silently truncated · date not stated
- [ART-027](ISSUES-archive.md) · RDB checksums ignored the block's own `SummedLongs` · date not stated
- [ART-028](ISSUES-archive.md) · A folder scan could be sent into unbounded recursion · date not stated
- [ART-029](ISSUES-archive.md) · Any file could be read whole as a "ROM" · date not stated
- [ART-001](ISSUES-archive.md) · ADF edits overwrote the original with no backup and no atomicity · date not stated
- [ART-002](ISSUES-archive.md) · LHA extraction silently overwrote existing files · date not stated
- [ART-003](ISSUES-archive.md) · FlashFloppy `FF.CFG` was regenerated from scratch · date not stated
- [ART-004](ISSUES-archive.md) · PiStorm `cmdline.txt` was regenerated, leaving the SD card unbootable · date not stated
- [ART-005](ISSUES-archive.md) · Config files were replaced without a backup · date not stated
- [ART-006](ISSUES-archive.md) · Aborted extractions left truncated files behind · date not stated
- [ART-007](ISSUES-archive.md) · An invalid block number from the UI killed the whole application · date not stated
- [ART-008](ISSUES-archive.md) · Malformed images could hang the UI forever · date not stated
- [ART-009](ISSUES-archive.md) · The ADF hash function was not AmigaDOS-compatible · date not stated
- [ART-010](ISSUES-archive.md) · International volumes hashed names with the wrong case folding · date not stated
- [ART-011](ISSUES-archive.md) · `rename_entry` corrupted directories when the chain was inconsistent · date not stated
- [ART-012](ISSUES-archive.md) · Duplicate names could be created in one directory · date not stated
- [ART-013](ISSUES-archive.md) · A file header could be used as a directory · date not stated
- [ART-014](ISSUES-archive.md) · The zip-bomb guard could be bypassed by integer overflow · date not stated
- [ART-015](ISSUES-archive.md) · Media paths could inject arbitrary WinUAE directives · date not stated
- [ART-016](ISSUES-archive.md) · Only the first of several HDFs was reachable · date not stated
- [ART-017](ISSUES-archive.md) · WinUAE detection used hard-coded drive letters · date not stated
- [ART-018](ISSUES-archive.md) · Concurrent launches clobbered each other's configuration · date not stated
- [ART-019](ISSUES-archive.md) · HD floppy geometry was never reported · date not stated
- [ART-020](ISSUES-archive.md) · CI hid real correctness errors · date not stated

---

## Adding an entry

1. Take the next free `ART-NNN`.
2. State the defect as a claim, not a symptom — what is wrong, not what looked odd.
3. Name the file, and say **how it fails for a user**. If you cannot describe a
   way it hurts someone, it is a preference, not a defect.
4. Cite the spec section when a rule is broken.
5. When fixing it, add a regression test and name it here. A fix without a test
   is not fixed — it is untested.
6. Once it is fixed or closed, move the whole entry, unchanged, to the top of
   [ISSUES-archive.md](ISSUES-archive.md) and add its one-line index row at the
   top of [Fixed](#fixed) above. Open holds only what is still open.
