# Project Status

**The single source of truth for where ART actually is.**

Every other document describes intent (the spec, `roadmap.md`) or mechanics
(`architecture.md`, `CLAUDE.md`). This one describes reality. When they
disagree, this file wins — and if this file disagrees with the code, fix this
file.

Update it at the end of any session that changes what works. Keep it short:
each round's own story is one row in the [session log](session-log.md), its
reasoning is under `docs/superpowers/` and `.superpowers/sdd/`, and the defect
register is [ISSUES.md](ISSUES.md). Nothing here is retold from those.

- Known defects and technical debt: [ISSUES.md](ISSUES.md)
- Feature-by-feature implementation state: [FEATURES.md](FEATURES.md)

---

## Snapshot

Every row is one current fact and the measurement behind it. A claim here is
only valid if the command that proves it was actually run — do not carry a
PASS forward on faith.

| | |
|---|---|
| **Last updated** | 2026-09-16 — **First boot phase 3 is merged and pushed.** `art-firstboot-phase-3` merged to `main` `--no-ff` as `4b72d7b` on the owner's word (15 commits on top of `f359a14`; no conflict, the merged tree equals the branch head `13645ac`), pushed on the owner's word, branch deleted; CI run 35064080515 on it completed `success`. The scoped re-review of the fix wave (`939586b..13645ac`) said **ready to merge**: all six Minors ADDRESSED, no new finding. Still owed, and named in *Start here*: the owner's own hand-answered rehearsal (design §8.3). Before that, 2026-09-16 — **First boot phase 3's final whole-branch review, and its six Minors fixed in one wave.** The review (`f359a14..939586b`, `.superpowers/sdd/2026-09-15-firstboot-phase-3/final-review.md`) said **ready to merge**: 0 Critical, 0 Important, 6 Minor. On the owner's word all six were fixed on `art-firstboot-phase-3` (still unmerged; wave base `939586b`): the restart sentence names the effect rather than completion, the `ART_Set_Input` write makes its own drawer, the `Written.removed` comment says where a removal is actually told, the `ART_Set_ScreenMode` marker takes no backup, `askPrefs`'s "absent means ticked" is one exported helper, and the wizard's rows resolve case-insensitively. Red first for each behavioural fix and every new guard mutated: M2's test is red under the reorder that hid the missing `create_dir_all`, M4's with `BackupPolicy::CONFIG` back, M5's with `?? false`. **M6's mutation survived and is disclosed**: NTFS folds case itself, so on the only supported host that fix changes no behaviour and its test is a control there. `pnpm lint` clean; `pnpm test` `Test Files 112 passed (112)`, `Tests 1721 passed (1721)`; `cd src-tauri && cargo test --lib`: `test result: ok. 3541 passed; 0 failed; 62 ignored; 0 measured; 0 filtered out` twice (389.13 s, 381.68 s), `TMP`/`TEMP` on `E:\amiga\ProjeART\build\tmp`; fmt and clippy clean; control-byte, scratch-root, scratch-guard and contrast sweeps clean. `cargo deny check` was not run (`Cargo.lock` unchanged) and the wave did not re-run `tr-overflow-check.py` — it touched one English sentence and one Turkish one, both in the card's report panel. Report `.superpowers/sdd/2026-09-15-firstboot-phase-3/fix-wave-report.md`. Before that, 2026-09-15 — **First boot phase 3's close (task 11): the whole suite twice, every gate CI runs, and the round's documents.** On `art-firstboot-phase-3` (not merged, base `4a9722b`, ten tasks). `pnpm lint` clean; `pnpm test` `Test Files 112 passed (112)`, `Tests 1720 passed (1720)`; `cd src-tauri && cargo test --lib`: `test result: ok. 3538 passed; 0 failed; 62 ignored; 0 measured; 0 filtered out` twice (418.34 s, 376.47 s), `TMP`/`TEMP` on `E:\amiga\ProjeART\build\tmp`; fmt and clippy clean; `Cargo.lock` unchanged, so `cargo deny check` was not run; control-byte, scratch-root, scratch-guard and contrast sweeps all clean; `tr-overflow-check.py` (headless Chrome over `pnpm dev`, 16 routes × 3 widths × tr/en) 0 Turkish-only hits — it reaches only screens with no backend, so the second tick's own windows line (which needs a tree) was not measured by it. Report `.superpowers/sdd/2026-09-15-firstboot-phase-3/phase-3-report.md`. **Owed:** the owner's own closing measurement (spec §8.3, Task 12) and the owner's ruling on an amitools finding (below). Before that, 2026-09-15 — **0.9.4 released.** `v0.9.4` on `e91f22e`, release run 34969262720 success, published on the owner's word with one NSIS and one MSI. Before that, 2026-09-15 — **The third debt round merged and pushed; 0.9.4 prepared.** `art-debt-3-0915` merged to `main` `--no-ff` as `e060e76` and pushed, both on the owner's word; the version fields raised to 0.9.4 and CHANGELOG's `[0.9.4]` section cut. Before that, 2026-09-15 — **The follow-ups' scoped re-review's three Minors fixed on `art-debt-3-0915` (unmerged); libpfs3 still `0.1.3+art.11`.** The re-review (`caceffb..11f820f`, `followups-rereview.md`) said *ready to merge*. Minor 1 ([ART-324](ISSUES.md#open)): ART's PFS3 copy asks `check_file_size` of every file before its fit check and before it reads one, and refuses a file of 4 GiB or more by name, saying to leave it out — it used to read the file whole first. Minor 2: ART-330's record names `firstboot.report.readFailed`. Minor 3: `volume.rs`'s module doc names `Rootblock::has_largefile`; ART-PATCH item 26 amended, comment only. Red first (the fit check's byte count on a small volume, `io NotFound` on a 5 100 MiB one); mutations m1–m3 red, restored hash-identical, none surviving. `cd src-tauri && cargo test --lib`: `test result: ok. 3495 passed; 0 failed; 60 ignored; 0 measured; 0 filtered out` twice (397.87 s, 421.30 s); fmt and clippy clean; ART-PATCH diff byte-identical (165 205 bytes); control-byte, scratch-guard and scratch-root sweeps clean; `pnpm lint` clean. `Cargo.lock` unchanged, so `cargo deny check` was not run; the writer and the format are unchanged, so the PFS3 oracle was not run. Report `.superpowers/sdd/2026-09-15-debt-3-round/minors-report.md`. **Next:** the merge, the push and the 0.9.4 build on the owner's word. Before that, 2026-09-15 — **The second scoped re-review's six follow-ups fixed on `art-debt-3-0915` (unmerged); libpfs3 `0.1.3+art.11`, ART-PATCH item 26.** `fsizex` is part of a size only on a largefile volume (`MODE_LARGEFILE` with `MODE_DIR_EXTENSION`, pfs3aio `init.c:648`), and a file of 4 GiB or more is refused by name elsewhere ([ART-324](ISSUES.md#open) narrowed to largefile volumes); a malformed directory entry is refused naming the directory, block and offset, so a damaged drawer no longer reads as a missing file in the install check or as "not booted" in the first-boot report ([ART-330](ISSUES.md#fixed)); the extra-field fallback and the Latin-1 fold's edges are tested; the `Writer::open` guard sees brace-rooted `use` trees; ART-328's wording corrected. Filed open: [ART-331](ISSUES.md#open) (a directory block without the `DB` id skipped silently). Red first `test result: FAILED. 3 passed; 6 failed`, and the two size tests red against a stub; 20 mutations and one fixture experiment, three survivors judged (no test can hand the writer 4 GiB). `cd src-tauri && cargo test --lib`: `test result: ok. 3494 passed; 0 failed; 60 ignored; 0 measured; 0 filtered out` twice (415.89 s, 399.36 s); fmt, clippy and `cargo deny check` clean; ART-PATCH diff byte-identical (165 053 bytes); `pfs3-oracle-check.py` exit 0, both directions agree; control-byte and scratch-guard sweeps clean; `pnpm lint` clean. Report `.superpowers/sdd/2026-09-15-debt-3-round/followups-report.md`. **Next:** a scoped re-review of the follow-ups, then the merge, the push and the 0.9.4 build on the owner's word. Before that, 2026-09-15 — **ART-325, ART-326, ART-327 and the scoped re-review's items 2, 4 and 5 fixed on `art-debt-3-0915` (unmerged); libpfs3 `0.1.3+art.10`, ART-PATCH item 25.** Extra fields read and written in pfs3aio's layout everywhere (ART-325, reachable through the first-boot report's and install check's PFS3 sizes); no second entry under an existing name, compared as pfs3aio compares (ART-326); `rename_in` moves the entry's own bytes and updates a moved link's chain as `MoveLink` does (ART-327); directory-block walks bounded, a panic before (item 5); the link check's refusals name what stopped them (item 4); the `Writer::open` guard sees globs, `<T>::open(`, `extern crate` aliases and skips comments and strings (item 2). pfs3aio read at `211f7f0`. Seven tests red first (`FAILED. 11 passed; 7 failed`), green after; 16 mutations red and restored hash-identical, none surviving. `cd src-tauri && cargo test --lib`: `test result: ok. 3485 passed; 0 failed; 60 ignored; 0 measured; 0 filtered out` twice (442.43 s, 429.03 s); fmt, clippy and `cargo deny check` clean; ART-PATCH diff byte-identical (141 294 bytes); `pfs3-oracle-check.py` exit 0, both directions agree; control-byte and scratch-guard sweeps clean. Filed open, unreachable: [ART-328](ISSUES.md#open), [ART-329](ISSUES.md#open). Report `.superpowers/sdd/2026-09-15-debt-3-round/art325-327-report.md`. **Next:** one scoped re-review, then the merge, the push and the 0.9.4 build on the owner's word. Before that, 2026-09-15 — **ART-323 fixed on `art-debt-3-0915` (unmerged); libpfs3 `0.1.3+art.9`.** Deleting a hard link removes only its entry. A link pfs3aio made, an object that links still name, and `create_hardlink` are each refused by name before anything is staged. pfs3aio read at `211f7f0`: `DeleteLink`, `RemapLinks`, `CreateLink`, `GetExtraFields`; hst-amiga's reader confirms the extra-field layout. Five tests red first, green after; seven mutations red, restored hash-identical, one survivor judged. `cd src-tauri && cargo test --lib`: `test result: ok. 3478 passed; 0 failed; 60 ignored; 0 measured; 0 filtered out` twice (425.44 s, 388.78 s); fmt, clippy and `cargo deny check` clean; `pfs3-oracle-check.py` exit 0; control-byte sweep clean. Filed open, by reading: [ART-325](ISSUES.md#open) (the crate's own extra-field parser), [ART-326](ISSUES.md#open) (a second entry under an existing name), [ART-327](ISSUES.md#open) (`rename_in` drops a moved entry's comment, date and extra fields). Detail in [ISSUES.md](ISSUES.md#fixed) (ART-323), report `.superpowers/sdd/2026-09-15-debt-3-round/art323-report.md`. **Next:** a scoped re-review, then the merge, the push and the 0.9.4 build on the owner's word. Before that, 2026-09-15 — **the third debt round's final whole-branch review fixed on `art-debt-3-0915` (unmerged), one fix wave; libpfs3 `0.1.3+art.8`.** I1 `rename_in` identifies the source by its name too and refuses a hard link as the destination (ART-322); I2 the `Writer::open` guard resolves every `use`/alias naming across `src/` and allows only the helper's own call (ART-317); M1 a Latin-1 case-only rename renames in place; M2 copy-on-write refuses a bitmap offering the old file's own block; M3/M5 records corrected and pfs3aio's overwrite path cited (ART-319); M4 item 5 recorded (CI actions → Node 24 majors, `f8dab92`, unproven until CI runs); M6 filed as [ART-324](ISSUES.md#open); M7 "Start here" trimmed. New, open: [ART-323](ISSUES.md#open). `cd src-tauri && cargo test --lib`: `test result: ok. 3475 passed; 0 failed; 60 ignored; 0 measured; 0 filtered out` twice (411.31 s, 397.22 s); fmt, clippy, `cargo deny check`, `pnpm lint`, control-byte and scratch-guard sweeps clean; `pfs3-oracle-check.py` exit 0. Report `.superpowers/sdd/2026-09-15-debt-3-round/fix-wave-report.md`. **Next:** a scoped re-review, then merge, push and the 0.9.4 build on the owner's word. Before that, 2026-09-15 — **ART-117 M5 closed on `art-debt-3-0915` (unmerged), item 4 of the third debt round: ART's three `$VER:` readers are one.** `core::amigaver::read_loose(data) -> LooseVersion { name: Option<String>, version: Option<(u16, u16)> }` shares one marker search and one 200-byte window; `rdb::version_from_ver_string` and `rdbedit::program_name_from_ver_string` are now thin calls into it (`.version`, `.name`), keeping their own signatures. `amigaver::read`, the strict reader, is untouched — its control-byte rejection is exactly what `read_loose` does not carry, and every current caller (`core::osinstall::collide::read_own_marker`, `core::amigainstall::packagevol::stated_version`, `core::osinstall::apply::read_stated_version`) still gets it; M5's own claim that this rejection was "what the game index depends on" did not hold on a grep of every caller (`core::gameindex` calls `amigaver` nowhere) and is corrected in place in [ISSUES.md](ISSUES.md#fixed). Characterization tests were written against today's `rdb::version_from_ver_string` and `rdbedit::program_name_from_ver_string` first, run green on the old code, then kept green through the refactor: NUL run before the name, control bytes in the name, a version with no name, extra whitespace, no marker — `core::rdb::tests::{a_nul_run_between_the_marker_and_the_name_does_not_block_the_version, control_bytes_in_the_name_do_not_stop_the_version_being_found, a_version_with_no_program_name_still_reads, extra_whitespace_between_the_marker_and_the_version_is_tolerated}`, `core::rdbedit::driver_tests::{control_bytes_in_the_name_are_returned_verbatim_not_rejected, extra_whitespace_between_the_marker_and_the_name_is_tolerated}`, plus `core::amigaver::tests::a_nul_run_before_the_name_is_rejected_by_the_strict_reader` pinning `read`'s own strictness and `read_loose`'s own suite. Two mutations, backed up by absolute path into `D:\Projeler\Amiga\scratch-0913\`, grep-confirmed, seen red and restored with `shutil.copyfile`: `read_loose` made strict about NUL — `core::rdbedit::driver_tests::the_program_name_is_the_first_token_after_ver` red (`80 passed; 1 failed`; the DifferentDriver tests themselves stayed green, disclosed); restored `81 passed`. The strict accessor's `is_plausible_name` made to accept everything — the new NUL-rejection test red alongside `binary_noise_with_a_coincidental_number_shape_is_rejected` and `reads_the_id_string_a_library_with_no_ver_marker_carries` (`23 passed; 3 failed`); restored `26 passed`. `cd src-tauri && cargo test --lib`: `test result: ok. 3469 passed; 0 failed; 60 ignored; 0 measured; 0 filtered out` (406.57 s, once, `TMP`/`TEMP` on `E:`); fmt and clippy clean. No frontend file changed, so `pnpm test`/`pnpm lint` were not re-run. Detail in [ISSUES.md](ISSUES.md#fixed) (ART-117 M5), report `.superpowers/sdd/2026-09-15-debt-3-round/item4-report.md`. **Next:** item 6 of the third debt round — final review, then merging and the 0.9.4 build are the owner's word. Before that, 2026-09-15 — **ART-322 fixed on `art-debt-3-0915` (unmerged), item 3b of the third debt round; libpfs3 `0.1.3+art.7`.** `Writer::rename_in`'s destination lookup used `name_eq_ci`, PFS3's case-insensitive name compare, so a case-only rename in the same directory (`rename_in(root, "File", root, "FILE")`) found the source itself as "an existing destination" and deleted it — reporting `Ok` while the file was gone; an identical-name rename hit the same path and was worse. `rename_in_impl` now checks, before deleting a found destination, whether it is the very entry being renamed (same parent anode, same anode number); when it is, an identical name is a no-op success and any other case-only change is rewritten in place by the new `rename_dir_entry_in_place`, keeping the entry's anode, size, protection, dates and comment — a same-directory destination differing only in case from a genuinely different entry is still replaced, unchanged. pfs3aio's own `RenameAndMove` (`directory.c:2064-2076,2130`, `tonioni/pfs3aio` `211f7f0`) was read this time and confirms the direction. Two tests red first (`left: 0 right: 1`, the file gone; `left: 72 right: 65`, an identical-name rename wrote to the device), green after (`test result: ok. 28 passed; 0 failed`); the mutation (disabling the same-entry check) reproduced both lines and was restored. `cd src-tauri && cargo test --lib`: `test result: ok. 3453 passed; 0 failed; 60 ignored; 0 measured; 0 filtered out` (388.60 s); fmt, clippy and `cargo deny check` clean; `pfs3-oracle-check.py` exit 0; control-byte sweep clean. No frontend file changed. Detail in [ISSUES.md](ISSUES.md#fixed) (ART-322), report `.superpowers/sdd/2026-09-15-debt-3-round/item3b-report.md`. **Next:** the third debt round's review; merging is the owner's word. Before that, 2026-09-15 — **ART-319's disclosed gaps closed on `art-debt-3-0915` (unmerged), item 3 of the third debt round; libpfs3 `0.1.3+art.6`.** `Writer::overwrite_file_in` is copy-on-write: the new content goes only into free blocks, and the head anode rewritten in place, the old blocks freed and the old chain's other anodes cleared all land in the one `update_rootblock` commit. An error before it leaves the old file on disk. The limit: the whole new content needs free space of its own, or the call is refused `DiskFull` before a write. `Writer::rename_in` over an existing destination is one commit (`delete_in_no_commit`, shared with `delete_in`, so the deldir behaviour is identical). Every public mutator now has discard and lock tests of its own (`src-tauri/src/core/preload/native.rs`). Four tests red on the old writer first; mutations M1–M6 each seen red and restored green, and the two survivors judged, in [ISSUES.md](ISSUES.md#fixed) (ART-319). New, open: [ART-322](ISSUES.md#open), a case-only PFS3 rename deletes the file and returns `Ok`, measured on the writer before and after this item, unreachable from ART. `cd src-tauri && cargo test --lib`: `test result: ok. 3450 passed; 0 failed; 60 ignored; 0 measured; 0 filtered out` (388.93 s) and `test result: ok. 3450 passed; 0 failed; 60 ignored; 0 measured; 0 filtered out` (452.50 s); fmt, clippy and `cargo deny check` clean; `pfs3-oracle-check.py` exit 0, "ART and hst-imager agree, both directions" (2 Windows-reserved names skipped, ART-114); control-byte and scratch-guard sweeps clean. No frontend file changed, so `pnpm test`/`pnpm lint` were not re-run. Report `.superpowers/sdd/2026-09-15-debt-3-round/item3-report.md`. **Next:** the third debt round's review; merging is the owner's word. Before that, 2026-09-15 — **ART-317's two open gaps guarded on `art-debt-3-0915` (unmerged), item 2 of the third debt round.** (b) `amiga_from_wall`/`system_now_unix` were `pub` and unguarded outside `core::clock`/`tools::local_time`, so a product call bypassing `AmigaClock::offset_at` entirely would make a UTC date with no offset and no guard would see it — closed by `core::independence::only_clock_and_local_time_name_amiga_from_wall_or_system_now_unix`, which walks all of `src/` (not only `core/`) and allow-lists only `core/clock.rs` and `tools/local_time.rs`; `core/volume/write/layout.rs`'s two calls are inside `#[cfg(test)]` and needed no entry. (c) a libpfs3 `Writer` opened without `set_entry_date` falls back to UTC (`vendor/libpfs3/src/writer.rs`) and the independence guards did not read `vendor/` — closed by a new `core::preload::native::open_pfs3_writer(vol, clock)`, the one product path to a libpfs3 `Writer`, which stamps the clock's date before handing the writer back, and by `core::independence::libpfs3_writer_open_is_named_only_by_the_pfs3_writer_helper` (allow-listing the helper and `core/card/sizing.rs`'s own test-only call). `copy_in_pfs3` now calls the helper in place of a bare `Writer::open`. TDD: both guards seen red with an injected product call (a temporary function in `core/card/sizing.rs` for (b), `core/dirsize.rs` for (c)) and green after restore; `core::preload::native::tests::open_pfs3_writer_stamps_the_clocks_local_date_before_any_write` was red when the helper's own `set_entry_date` call was removed (UTC day 17789 vs the expected local day 17546) and green restored. Backups by absolute path into `D:\Projeler\Amiga\scratch-0913\`, grep-confirmed; restored with `shutil.copyfile`, never `git checkout --`. `cd src-tauri && cargo test --lib`: `test result: ok. 3425 passed; 0 failed; 60 ignored; 0 measured; 0 filtered out` (once, 401.66 s, `TMP`/`TEMP` on `E:`); fmt, clippy clean; `scratch-guard-sweep.py` clean; no frontend file changed by this fix, so `pnpm test`/`pnpm lint` were not re-run. Detail in [ISSUES.md](ISSUES.md#fixed) (ART-317). **Next:** item 3 of the third debt round (ART-319's disclosed gaps: `rename_in` in one commit, copy-on-write `overwrite_file_in`, rollback/lock tests for the nine untested mutators). Before that, 2026-09-15 — **ART-321 fixed on `art-debt-3-0915` (unmerged), item 1 of the third debt round: `mbr_slot_of` no longer names the wrong MBR slot when an earlier Amiga area is unreadable.** `AmigaArea` (`core/card/mod.rs`) now carries its own `mbr_slot: Option<usize>`, set in `read_card_from` from the MBR entry its base came from, so the slot survives an earlier `0x76` area being skipped as unreadable; `core/preload/mod.rs`'s two call sites read `area.mbr_slot` directly and the position-deriving `mbr_slot_of` is removed. TDD: `core::preload::tests::a_format_uses_the_readable_areas_own_mbr_slot_when_an_earlier_area_is_unreadable` (a card with an unreadable first `0x76` area and a real second one) was red (`slot Some(2)`, the wrong area's own slot) before the fix and green (`slot Some(3)`) after; the mutation (reverting to a position-derived slot) reproduced the identical red line and was restored via `shutil.copyfile`. TS `AmigaArea` gained the matching `mbr_slot` field; every fixture updated. `cd src-tauri && cargo test --lib`: `test result: ok. 3422 passed; 0 failed; 60 ignored; 0 measured; 0 filtered out` (once, 361.85 s, `TMP`/`TEMP` on `E:`); fmt, clippy clean; `pnpm lint` clean; `scratch-guard-sweep.py` clean. Detail in [ISSUES.md](ISSUES.md#fixed) (ART-321). **Next:** item 2 of the third debt round. Before that, 2026-09-15 — **the second debt round is merged: `art-debt-2-0914` into `main` `--no-ff` as `23681a6`, on the owner's word, and pushed on the owner's word** (ART-250, ART-062's mechanical part, ART-117, ART-320, rustls 0.23.45). Before that, 2026-09-15 — **ART-117 scoped re-review fixed on `art-debt-2-0914`: a journal left after a finished, verified RDB edit is marked finished and is never offered for undoing.** The edit writes `FINISHED_MARK` into the journal header before removing it, or rolls back if it cannot; `open_rdb` says `ART-RDB-EDIT-JOURNAL-FINISHED`, recovery refuses to roll it back, the File Manager offers deletion only (jsdom-tested); a mark that can neither be written nor taken out ends `JOURNAL-LEFT`, never "undo it". `cd src-tauri && cargo test --lib`: `test result: ok. 3421 passed; 0 failed; 60 ignored; 0 measured; 0 filtered out` twice (443 s, 412 s, `TMP`/`TEMP` on `E:`); fmt, clippy clean; `pnpm test` `112` / `1687`, `pnpm lint` clean; control-byte, scratch-guard, scratch-root sweeps clean. Detail in [ISSUES.md](ISSUES.md#fixed) (ART-117). Before that, 2026-09-15 — **ART-117's final whole-branch review fixed on `art-debt-2-0914` (unmerged): one fix wave for its 3 Important and 9 of its 12 Minor findings.** A replace is refused when either side's `$VER:` names no program (I1); a verified edit whose journal file cannot be removed has its own ending, `ART-RDB-EDIT-JOURNAL-LEFT`, instead of "undo failed" (I2); a format or copy that fails, or a cancel, after the RDB edit keeps the edit in the stop, the operation log and the result panel (I3); appends are planned before replaces, and every RDB edit before any format (M1, M2); M4, M6–M10 done, M3 and M11 corrected in the records, M12 filed open as [ART-321](ISSUES.md#open), M5 disclosed by the controller's ruling. The round's disclosed read-back survivor is now caught. `cd src-tauri && cargo test --lib`: `test result: ok. 3411 passed; 0 failed; 60 ignored; 0 measured; 0 filtered out` twice (431 s, 399 s, `TMP`/`TEMP` on `E:`); fmt, clippy and `cargo deny check` (`advisories ok, bans ok, licenses ok, sources ok`) clean; `pnpm test` `Test Files 111 passed (111)`, `Tests 1685 passed (1685)`, `pnpm lint` clean; control-byte, scratch-guard, scratch-root and contrast sweeps clean. Detail in [ISSUES.md](ISSUES.md#fixed) (ART-117) and `.superpowers/sdd/2026-09-14-art-117-rdb-embed/fix-wave-report.md`. **Next:** push and watch CI (ART-320's override is proved only there), then merging is the owner's word. Before that, 2026-09-14 — **ART-117 fixed on `art-debt-2-0914` (unmerged, Task 12 of `.superpowers/sdd/2026-09-14-art-117-rdb-embed/`, records and full verification): ART embeds or replaces a filesystem driver in a foreign card's existing RDB in place, without hst-imager.** `core/rdbedit.rs` walks an RDB strictly and refuses any block it cannot account for; `core/preload/embed.rs` allocates above everything live (raising `RDBBlocksHi` only over zero blocks), backs the RDB area up to a file the user chooses, writes four synced journalled stages with the one-sector link last, and reads every block back before committing. `VolumeFormatter::import_filesystem` and `CoreError::ForeignRdbEmbedNotSupported` are gone — hst-imager no longer embeds; a refusal names it and the user decides. The 2026-08-21 "leave it" decision is corrected in place in [ISSUES.md](ISSUES.md#fixed): the 16-head/63-sector argument is about *rebuilding* an RDB, not *appending* to one. Twelve tasks, each with a red test before the code and mutation testing after; three disclosed survivors (no per-stage sync, the backup's read-back, `ready_to_run()?` redundant with `embed::run`'s own refusal — none testable in-process, none a design gap). `cd src-tauri && cargo test --lib`: `test result: ok. 3398 passed; 0 failed; 60 ignored; 0 measured; 0 filtered out` (twice, `TMP`/`TEMP` on `E:`; fmt and clippy clean). `pnpm test`: `Test Files 111 passed (111)`, `Tests 1683 passed (1683)`; `pnpm lint` clean. `scripts/control-byte-sweep.py`, `scripts/scratch-root-sweep.py`, `scripts/scratch-guard-sweep.py` and `scripts/contrast-check.py --quiet` all clean; `scripts/scratch-counter-sweep.py` shows its one known, non-blocking finding (ART-235) and nothing new; `scripts/oracle-check.py` (amitools installed) both directions clean, including a driver ART embedded read back by amitools byte for byte. `cargo deny check` failed at Task 12 on one `rustls` advisory reached through `ureq`, unrelated to any ART-117 change; *corrected 2026-09-15:* `31566bf` bumped `rustls` to 0.23.45 in `Cargo.lock` for RUSTSEC-2026-0285, and `cargo deny check` then gave `advisories ok, bans ok, licenses ok, sources ok`. **New finding, filed [ART-320](ISSUES.md#open), not fixed:** `src-tauri/.cargo/config.toml` (ART-184) forces `TMP`/`TEMP` to `D:/tmp/art-tests` with `force = true` for every `cargo test`, overriding the shell's own `TMP` — so every suite run on this branch has put scratch on `D:` (1.5 GB there now), against the owner's rule that test scratch goes on `E:\amiga\ProjeART\build\tmp`; Task 11's own hook cannot pass its own TMP-on-`E:` pre-flight under `cargo test -- --ignored` because of this, and ran the compiled test binary directly instead (below). Moving the forced path is not a one-line fix — `E:` does not exist on the CI runner — so this is left open for the owner's decision. **Owed by the owner:** mounting the edited copy (already produced and checked by Task 11 against two independent oracles) in WinUAE or on a PiStorm and asking the loaded handler for `version full`. Full detail in [ISSUES.md](ISSUES.md#fixed), spec `docs/superpowers/specs/2026-09-14-art-117-rdb-embed-design.md`, research `docs/superpowers/notes/2026-09-14-art-117-rdb-embed-research.md`. **Next:** merging is the owner's word. Before that, 2026-09-14 — **ART-062's mechanical part done on `art-debt-2-0914` (unmerged, entry stays open): the one Turkish string a plain browser could reach and measure was clipped, and is fixed.** A headless-Chrome sweep of all 16 top-level routes x 3 widths (1280x800, 1024x768, 960x768 — the shell's own `minWidth`) x tr/en (`scripts/tr-overflow-check.py`, new, modelled on `zoom-check.py`/ART-099) found exactly one Turkish-only clip: `files.pane.nothingOpen` ("Nothing open" → "Hiçbir şey açık değil", +83%) clipped 10px in `.tc-path-text`, 1024x768 only. Shortened to "Açık bir şey yok" (the owner's choice); `en.json` unchanged; no test pinned the old string. Re-run after the fix: 48 (route x width) runs, 0 errors, 0 crashes, Turkish-only list empty at every width. The same pass also found ART-062's own table had decayed — two of its four original rows name a key or control that no longer exists — and corrected it in place; full detail in [ISSUES.md](ISSUES.md#open). `pnpm lint` clean; `pnpm test` `110` files / `1671` tests (unchanged — a value edit, not a key change); `scripts/control-byte-sweep.py` and `scripts/contrast-check.py --quiet` both clean; no Rust file touched, so no cargo run. **Still open:** reading the Turkish catalogue itself (2262 keys), and `hardDisk.bootablePri` / job status "Done"/"Failed", both of which need a live Tauri session a plain browser cannot fake. Before that, 2026-09-14 — **ART-250 fixed on `art-debt-2-0914` (unmerged): ToolTypes are Latin-1, byte-exact.** `tooltypes()` decoded and `set_tooltypes()` encoded with UTF-8; real AmigaDOS ToolTypes text is ISO-8859-1, and a NewIcon `IM1=`/`IM2=` pixel-encoding byte above 0x7F is essentially never valid standalone UTF-8, so the old code turned it into `U+FFFD` and could not reproduce it on write — measured on 69 of the owner's 798 real icons (`lossy_tooltypes=69`). Both ends now decode/encode as Latin-1 (a private `latin1_decode`/`latin1_encode` pair, the same identity cast `core/adf/bcpl.rs` uses, ART-074's precedent); `set_tooltypes` refuses a character above `U+00FF` by name rather than substituting, since it has no production caller today. Three new tests, each red before the fix and green after; three mutations, each red and restored. `cargo test --lib` `test result: ok. 3326 passed; 0 failed; 59 ignored` twice; fmt and clippy clean; `control-byte-sweep.py` clean. Oracle against the owner's `E:\amiga\Amigatolon\os39` (798 real icons), both directions: before `checked=798 failed=0 no_drawer_data2=0 lossy_tooltypes=69` (matches the prior figure exactly); after `checked=798 failed=0 no_drawer_data2=0`, the bucket empty. Full detail in [ISSUES.md](ISSUES.md#fixed). Before that, 2026-09-14 — **ART-319 merged: `art-319-writer-rollback` into `main` `--no-ff` as `f916301`, on the owner's word, and pushed on the owner's word.** Before that, 2026-09-14 — **ART-319 fixed on `art-319-writer-rollback`: the PFS3 writer returns to its last commit on error, and a commit that fails part-way locks it** — libpfs3 `0.1.3+art.5`; `cd src-tauri && cargo test --lib` `test result: ok. 3321 passed; 0 failed; 59 ignored` twice; fmt, clippy and `cargo deny check` clean; the three sweeps below clean; `pfs3-oracle-check.py` exit 0. Full detail in [ISSUES.md](ISSUES.md#fixed) and `.superpowers/sdd/2026-09-14-art-319-writer-rollback/`. Before that, 2026-09-14 — **the debt round is merged: `art-debt-0914` into `main` `--no-ff` as `7725afa`, on the owner's word, and pushed on the owner's word.** Before that, 2026-09-14 — plan 5 fixed on `art-debt-0914`: **ART-317, every Amiga date ART
makes from an instant is now local wall time.** Every writer, copy source and reader takes a
`core::clock::AmigaClock`; the product's is `tools::local_time::LOCAL_TIME` (`chrono::Local`, the offset in
force on the date being converted, not on now); libpfs3 is `0.1.3+art.4` (`FormatOptions::datestamp`,
`Writer::set_entry_date`). `VolumeWriter::open` and `create_blank_adf` are `#[cfg(test)]`, so the
compiler refuses them in product code; `AMIGA_EPOCH_UNIX` arithmetic outside `clock.rs`/`adf/bcpl.rs`
fails `core_makes_amiga_dates_only_through_the_clock`, and `UtcClock` named in `commands/`/`tools/`
outside a test fails `commands_and_tools_never_name_utc_clock_outside_a_test` (fix wave, 2026-09-14).
`.uaem` dates, `volume_attributes`'s `date_text` and
Amiga→Amiga dates pass through untouched. `SCAN_CACHE_SCHEMA` 2 → 3 — a pre-fix disc listing is a miss, so
every install disc is read again once. Full report `.superpowers/sdd/2026-09-14-debt-5-art-317-local-time/`.
Before that, 2026-09-14 — plan 4's final whole-branch review ("With fixes", Minor findings only) fixed
on `art-debt-0914`: ART-301's job-title migration closed out — `job-title-keys.test.ts` now refuses a production
`JobTitle::new(` in the two mechanism files (`commands/jobs.rs`, `core/jobs/mod.rs`) outside their own `mod
tests`, its `.count(` detector no longer mistakes a bare `Iterator::count()` for the builder's own `.count(n)`,
a new check refuses a `.text()` value named after an i18next `TOptions` key (`context`, `lng`, `ns`, …),
`JobTitle`'s unneeded public `Deserialize` derive is dropped (nothing on the Rust side deserializes a job title;
the field is now a plain `&'static str` rather than a `Cow`) with its doc comment corrected, and
`docs/architecture.md`'s job-title sentence now says Rust *code* names a key, naming the distro registry's JSON
keys as the other case; unmerged. Before that, 2026-09-14 — plan 3's final whole-branch review ("With fixes", no Critical findings) fixed
on `art-debt-0914`: ART-318's naming corrected (ART-318 15→17/15), ART-319 widened to every writer operation an
error can leave part-way committed (M4), and five code findings fixed (M2 `DelDirEntry::parse` gated on the
volume's own `MODE_LARGEFILE`, M3 `Writer::open` refuses an unsupported `reserved_blksize`, M5 the "atomic
commit" wording corrected, M6 the name-limit rule shares one function, M9 a direct guard closes ART-311's Task 5
survivor); unmerged. Before that, 2026-09-14 — debt round on `art-debt-0914` (plans 1–2: records, ART-302, ART-300), unmerged. Before that, 2026-09-13 — ART-310 fixed on `art-310-libpfs3-format` (vendored, patched `libpfs3` 0.1.3+art.1), merged to `main` `--no-ff` as `37c7538` and pushed on the owner's word. Before that, 2026-09-11 — `art-one-button-card` (round 1 of the one-button card work: card sizing, ART-308, ART-309) merged to `main` `--no-ff` as `803ec5f` on the owner's word and pushed on the owner's word. Before that, 2026-09-10 — ART-278/279 (`art-278-runaway`) and the four-tab rewrite (`art-four-tabs`, five rounds) both merged to `main` `--no-ff` on the owner's word; the first is pushed, the second is not. Per-round detail is the [session log](session-log.md) |
| **Version** | **0.9.4**, released 2026-09-15 (tag `v0.9.4`, annotated, on `e91f22e`; release run 34969262720 — success; published on the owner's word at 2026-09-15 12:55:17 UTC with `Amiga.Retro.Toolkit_0.9.4_x64-setup.exe` 5,725,112 B and `Amiga.Retro.Toolkit_0.9.4_x64_en-US.msi` 7,118,848 B). Its notes are CHANGELOG's `[0.9.4]` section: the third debt round. Before that **0.9.3**, released 2026-09-14 (tag `v0.9.3`, annotated, on `62f0f81`; release run 34782632119 — success; published on the owner's word at 2026-09-13 21:26:21 UTC with `Amiga.Retro.Toolkit_0.9.3_x64-setup.exe` 5,672,205 B and `Amiga.Retro.Toolkit_0.9.3_x64_en-US.msi` 7,045,120 B). Its notes are CHANGELOG's `[0.9.3]` section: ART-312's fix. Before that **0.9.2**, tagged 2026-09-13 (`v0.9.2`, annotated, on `d698522`; release run 34779524060 — success; the release is **published** with both installers attached: `Amiga.Retro.Toolkit_0.9.2_x64-setup.exe` 5,671,852 B, `Amiga.Retro.Toolkit_0.9.2_x64_en-US.msi` 7,045,120 B). Its notes are CHANGELOG's `[0.9.2]` section, with a *Known issues* part naming ART-312 and ART-311. Before that **0.9.1**, released 2026-09-09 (tag `v0.9.1`, release run 34331989539 — success, published 2026-09-09 09:21 UTC with the MSI and the NSIS installer). The number lives in three files (`package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`) and `release.yml` refuses a tag that disagrees with any of them, or a version with no CHANGELOG section |
| **`main` / `origin`** | `origin/main` is `4b72d7b` — `art-firstboot-phase-3` merged `--no-ff` on 2026-09-16 on the owner's word (first boot phase 3: the wizard and the reboot; 15 commits on top of `f359a14`; no conflict, the merged tree equals the branch head `13645ac`, where `cargo test --lib` ran `3541 passed; 0 failed; 62 ignored` twice and `pnpm test` 112 files / 1721 tests) and pushed on the owner's word; branch deleted (it was never pushed); CI run 35064080515 on it completed `success`. This STATUS commit sits on top of it locally until it is pushed. Before that `origin/main` is `e91f22e` — *Release 0.9.4*, pushed on the owner's word with the tag `v0.9.4`; CI run 34969258448 on it completed `success`; this STATUS commit sits on top of it locally until it is pushed. Before that `origin/main` is `e060e76` — `art-debt-3-0915` merged `--no-ff` on 2026-09-15 on the owner's word (the third debt round: 21 commits on top of `8b44277`; no conflict, the merged tree equals the branch head `870a0fd`, where `cargo test --lib` ran `3495 passed; 0 failed; 60 ignored` twice) and pushed on the owner's word; branch deleted (it was never pushed); CI run 34960932926 on it completed `success` (2026-09-15, 22 min; the Node 24 action majors ran on `windows-latest`). The 0.9.4 release commit sits on top of it locally until it is pushed. Before that `main` is `23681a6` — `art-debt-2-0914` merged `--no-ff` on 2026-09-15 on the owner's word (the second debt round: ART-250, ART-062's mechanical part, ART-117, ART-320, rustls 0.23.45; 28 commits on top of `85148f9`; no conflict, the merged tree equals the branch head `68f9e3c`; `cargo test --lib` on it `3421 passed; 0 failed; 60 ignored`) and pushed on the owner's word as `e7e667c`; CI run 34912500886 on it completed `success` (2026-09-15, 20 min) — Rust clippy, Rust tests and cargo-deny included, so ART-320's `--config env.TMP` override (value + force) works on the runner. This STATUS commit sits on top of it. Before that `main` is `f916301` — `art-319-writer-rollback` merged `--no-ff` on 2026-09-14 on the owner's word (ART-319, 4 commits on top of `eeb3d62`; no conflict, the merged tree equals the branch head `11579cf`; `cargo test --lib` on it `3323 passed; 0 failed; 59 ignored`) and pushed on the owner's word together with this STATUS commit; CI run 34848587004 on `85148f9` (the ART-319 merge's STATUS commit) completed `success`. Before that `main` is `7725afa` — `art-debt-0914` merged `--no-ff` on 2026-09-14 on the owner's word (the debt round's five plans, 52 commits; the merge's first parent is `bc479b6`, *docs: 0.9.3 released*; no conflict, `main` had not moved; the merged tree is identical to the branch head `adebdf6`) and pushed on the owner's word as `b07bed3`; CI run 34835570453 on it completed `success` (2026-09-14, 23 min). This STATUS commit sits on top of it. Before that `origin/main` is `62f0f81` — *Release 0.9.3*, pushed on the owner's word with the tag `v0.9.3`; CI run 34782630378 on it completed `success`, and CI run 34782070016 on the ART-312 merge `28f2830` completed `success`. This STATUS commit sits on top of it locally until it is pushed. Before that `origin/main` is `28f2830` — `art-312-anode-reuse` merged `--no-ff` on 2026-09-13 on the owner's word and pushed on the owner's word together with `70ab0ac` (*docs: 0.9.2 released*); the merged tree is the branch's, where `cargo test --lib` ran 3258 / 0 / 58 ignored twice. The 0.9.3 release commit sits on top of it locally until it is pushed. Before that `origin/main` is `d698522` — *Release 0.9.2*, pushed on the owner's word with the tag `v0.9.2`; this STATUS commit sits on top of it locally until it is pushed; CI run 34779521678 on it completed `success`. Before that Identical at this STATUS commit, on top of `37c7538` — `art-310-libpfs3-format` merged `--no-ff` on 2026-09-13 on the owner's word (ART-310; its 11 commits plus the merge, on top of `8f33ab8`; one STATUS conflict, each side having changed a different row) and pushed on the owner's word; CI run 34777983363 on it (`225f3af`) completed `success`; no package built. The merged tree's `src-tauri` and `scripts` are the branch's, where `cargo test --lib` ran 3257 / 0 / 58 ignored twice. Before that identical at `0fa4142`, on top of `803ec5f` — `art-one-button-card` merged `--no-ff` on 2026-09-11 on the owner's word (its 16 commits plus the merge, on top of the STATUS commit `a85f68f`), branch deleted, and pushed on the owner's word as `0fa4142`; CI run 34632539453 on it completed `success`; no package built. Merged tree: full `cargo test --lib` 3252 / 0 / 58 ignored, Vitest 109 / 1658. Before that identical at `415233d` — `art-307-pistorm-a1200-rom` merged `--no-ff` on 2026-09-11 (ART-307) and pushed on the owner's word together with `af4026b` and `2fe104a`; CI run 34579155661 on it completed `success`; no package built for it (the owner did not ask). Before that identical at `09d52fb` — `art-305-emu68-any-release` merged `--no-ff` on 2026-09-11 (ART-305 by the owner's ruling, ART-306) and pushed on the owner's word together with `f8d7b1d`; CI run 34539089579 on it completed `success`. The package `main-09d52fb` is in `E:/amiga/ProjeART/build`. Before that identical at `ae030ff` — `art-304-shared-top-level` merged `--no-ff` on 2026-09-11 (ART-304) and pushed on the owner's word together with `b82b0dd`; CI run 34534960913 on it completed `success`. The package `main-ae030ff` is in `E:/amiga/ProjeART/build`. Before that identical at `ed448d7` — `art-303-unresolved-gate` merged `--no-ff` as `706de9f` on 2026-09-10 (ART-303, the owner's ruling) and pushed on the owner's word; CI run 34530659628 on it completed `success`. Before that `origin/main` was `c501542`: `art-files-columns` merged `--no-ff` as `cdcc54b` and pushed on the owner's word; CI run 34528010629 on it completed `success`. Before that identical at `e2633a6` — `art-owner-findings-0910` merged `--no-ff` as `be7a32f` on 2026-09-10 (ART-297, ART-298, ART-299) and pushed on the owner's word together with `4c84e60`; CI run 34522603545 on it completed `success`. The package `main-e2633a6` is in `E:/amiga/ProjeART/build`. Before that identical at `f354f46` — `art-291-clear-on-removal` merged `--no-ff` as `1596fe4` on 2026-09-10 (ART-291 fixed by the owner's ruling) and pushed on the owner's word; CI run 34510704111 on it completed `success`. Before that `origin/main` was `60cfe12` — `art-291-292-295` merged `--no-ff` as `ad786ff` on 2026-09-10 (ART-292, ART-295, ART-296, and ART-291's fix reverted) and pushed on the owner's word; CI run 34506640236 on it completed `success`. Before that `origin/main` was `a826441` — ART-285 (`c2bcdda`), ART-294 (`6698859`) and ART-293 (`8398564`) merged `--no-ff` on 2026-09-10 and pushed on the owner's word; CI run 34491510093 on it completed `success`. Before that `origin/main` was `916b611`, pushed the same day with the four-tabs and ART-283 merges; CI run 34475354297 on it completed `success`. Before that `0737c2d` (ART-283 merged `--no-ff`, on top of `a01851c`) — three merges on 2026-09-10, both `--no-ff` on the owner's word, both branches deleted: `9234368` (`art-278-runaway`, ART-278/279) and `f9602c3` (`art-four-tabs`, simplification round A + the four-tab rewrite, 58 commits, merged **before** spec § 7's drive). `origin/main` is `dd10a11` (the ART-278 merge plus its STATUS line, pushed 2026-09-10); the four-tabs and ART-283 merges were pushed as `916b611` on 2026-09-10. Merged tree re-measured: full `cargo test --lib` 3197 / 0 / 53 ignored, Vitest 107 / 1611, lint, control-byte and contrast sweeps clean |
| **Tests — Rust** | `cd src-tauri && cargo test --lib` on `art-debt-3-0915` at `870a0fd` (the follow-ups' Minors; the tree `main` has at `e060e76`), 2026-09-15: `test result: ok. 3495 passed; 0 failed; 60 ignored; 0 measured; 0 filtered out` twice (397.87 s, 421.30 s; `TMP`/`TEMP` on `E:`). Before that, at `11f820f` (the six follow-ups): `3494 passed; 0 failed; 60 ignored` three times. Before that, `cd src-tauri && cargo test --lib` on `art-debt-3-0915`, 2026-09-15 (ART-323 fixed, libpfs3 `0.1.3+art.9`; 5 new tests in `core::preload::native`, 2 `create_hardlink` discard/lock tests replaced by one): `test result: ok. 3478 passed; 0 failed; 60 ignored; 0 measured; 0 filtered out; finished in 425.44s` and `test result: ok. 3478 passed; 0 failed; 60 ignored; 0 measured; 0 filtered out; finished in 388.78s` (twice, `TMP`/`TEMP` on `E:`; fmt, clippy and `cargo deny check` clean). Before that, `cd src-tauri && cargo test --lib` on `art-debt-3-0915`, 2026-09-15 (the round's final review fix wave, libpfs3 `0.1.3+art.8`; 5 new tests in `core::preload::native`, 1 new fixture test in `core::independence`): `test result: ok. 3475 passed; 0 failed; 60 ignored; 0 measured; 0 filtered out` (411.31 s) and `test result: ok. 3475 passed; 0 failed; 60 ignored; 0 measured; 0 filtered out` (397.22 s) (twice, `TMP`/`TEMP` on `E:`; fmt, clippy and `cargo deny check` clean). Before that, `cd src-tauri && cargo test --lib` on `art-debt-3-0915`, 2026-09-15 (ART-117 M5 closed — item 4: `3469 passed; 0 failed; 60 ignored`, once). Before that, `cd src-tauri && cargo test --lib` on `art-debt-3-0915`, 2026-09-15 (ART-322 fixed — item 3b of the third debt round, libpfs3 `0.1.3+art.7`; 3 new tests in `core::preload::native`): `test result: ok. 3453 passed; 0 failed; 60 ignored; 0 measured; 0 filtered out` (388.60 s, once, `TMP`/`TEMP` on `E:`; fmt, clippy and `cargo deny check` clean). Before that, `cd src-tauri && cargo test --lib` on `art-debt-3-0915`, 2026-09-15 (ART-319's disclosed gaps closed — item 3 of the third debt round, libpfs3 `0.1.3+art.6`; 25 new tests in `core::preload::native`): `test result: ok. 3450 passed; 0 failed; 60 ignored; 0 measured; 0 filtered out` (388.93 s) and `test result: ok. 3450 passed; 0 failed; 60 ignored; 0 measured; 0 filtered out` (452.50 s) (twice, `TMP`/`TEMP` on `E:`; fmt, clippy and `cargo deny check` clean). Before that, `cd src-tauri && cargo test --lib` on `art-debt-3-0915`, 2026-09-15 (ART-317's survivors (b) and (c) guarded — item 2 of the third debt round, 3 new tests: two `core::independence` guards and `core::preload::native::tests::open_pfs3_writer_stamps_the_clocks_local_date_before_any_write`): `test result: ok. 3425 passed; 0 failed; 60 ignored; 0 measured; 0 filtered out` (once, 401.66 s, `TMP`/`TEMP` on `E:`; fmt and clippy clean). Before that, `cd src-tauri && cargo test --lib` on `art-debt-3-0915`, 2026-09-15 (ART-321 fixed — item 1 of the third debt round): `test result: ok. 3422 passed; 0 failed; 60 ignored; 0 measured; 0 filtered out` (once, 361.85 s, `TMP`/`TEMP` on `E:`; fmt and clippy clean). Before that, `cd src-tauri && cargo test --lib` on `art-debt-2-0914`, 2026-09-15 (ART-117's final-review fix wave, 13 new tests): `test result: ok. 3411 passed; 0 failed; 60 ignored; 0 measured; 0 filtered out` (twice, 431 s and 399 s, `TMP`/`TEMP` on `E:`; fmt, clippy and `cargo deny check` clean). Before that, `cd src-tauri && cargo test --lib` on `art-debt-2-0914`, 2026-09-14 (ART-117 fixed — in-place RDB embed and replace, Task 12's own close-out verification): `test result: ok. 3398 passed; 0 failed; 60 ignored; 0 measured; 0 filtered out` (twice, `TMP`/`TEMP` on `E:`; fmt and clippy clean; `cargo deny check` failed then on one `rustls` advisory unrelated to this round — cleared by `31566bf`, see Lint row). Before that, on `art-debt-2-0914`, 2026-09-14 (ART-250 fixed — ToolTypes are Latin-1, byte-exact): `test result: ok. 3326 passed; 0 failed; 59 ignored; 0 measured; 0 filtered out` (twice, `TMP`/`TEMP` on `E:`; fmt, clippy and `scripts/control-byte-sweep.py` clean). Before that, on `art-319-writer-rollback`, 2026-09-14 (ART-319 fixed, libpfs3 `0.1.3+art.5`): `test result: ok. 3321 passed; 0 failed; 59 ignored; 0 measured; 0 filtered out` (twice, `TMP`/`TEMP` on `E:`; fmt, clippy and `cargo deny check` clean). Before that, `cd src-tauri && cargo test --lib` on `art-debt-0914` after plan 5's final-review fix wave (`a3b879d`), 2026-09-14: `test result: ok. 3317 passed; 0 failed; 59 ignored` (twice, `TMP`/`TEMP` on `E:`; fmt and clippy clean). Before that, `cd src-tauri && cargo test --lib` on `art-debt-0914` (plan 5, ART-317 fixed — local time everywhere, vendored crate `0.1.3+art.4`), 2026-09-14: `test result: ok. 3316 passed; 0 failed; 59 ignored; 0 measured; 0 filtered out` (twice, identical counts, `TMP`/`TEMP` on `E:`; fmt, clippy and `cargo deny check` clean — `advisories ok, bans ok, licenses ok, sources ok`). Before that, `cd src-tauri && cargo test --lib` on `art-debt-0914` after ART-301, 2026-09-14: `test result: ok. 3298 passed; 0 failed; 58 ignored; 0 measured; 0 filtered out` (twice, `TMP`/`TEMP` on `E:`; fmt and clippy clean). Before that, `cd src-tauri && cargo test --lib` on `art-debt-0914` (plan 3's final review fix wave — M2, M3, M6 and the M9 guard each add a test), 2026-09-14: `test result: ok. 3292 passed; 0 failed; 58 ignored; 0 measured; 0 filtered out` (twice, `TMP`/`TEMP` on `E:`; fmt, clippy and `cargo deny check` clean). Before that, `cd src-tauri && cargo test --lib` on `art-debt-0914` (task 8, the PFS3 round's close-out — ART-311, 313, 314, 315, 316, 318 fixed, ART-319 filed, vendored crate `0.1.3+art.3`), 2026-09-14: `test result: ok. 3289 passed; 0 failed; 58 ignored; 0 measured; 0 filtered out` (twice, `TMP`/`TEMP` on `E:`; fmt, clippy and `cargo deny check` clean). Before that, `cd src-tauri && cargo test --lib` on `art-debt-0914` (task 5, ART-302/ART-300 close-out), 2026-09-14: `test result: ok. 3266 passed; 0 failed; 58 ignored; 0 measured; 0 filtered out` (twice, `TMP`/`TEMP` on `E:`; fmt and clippy clean). Before that, `cd src-tauri && cargo test --lib` on `art-312-anode-reuse`, 2026-09-13: `test result: ok. 3258 passed; 0 failed; 58 ignored; 0 measured; 0 filtered out` (twice, `TMP`/`TEMP` on `E:`; clippy clean). Before that, `cd src-tauri && cargo test --lib` on `main` (`225f3af`, the ART-310 merge plus its STATUS commit), 2026-09-13: `test result: ok. 3257 passed; 0 failed; 58 ignored; 0 measured; 0 filtered out` (twice, `TMP`/`TEMP` on `E:`). Before that, `cd src-tauri && cargo test --lib` on `art-310-libpfs3-format` (`e58abbb`), 2026-09-13: `test result: ok. 3257 passed; 0 failed; 58 ignored; 0 measured; 0 filtered out` (twice, the owner's Windows machine, `TMP`/`TEMP` on `E:`; fmt, clippy and deny clean); module runs on Linux through the gate script. Before that, the **full** suite on the merged `main` (`803ec5f`), 2026-09-11: `3252 passed; 0 failed; 58 ignored` (`TMP`/`TEMP` on `E:`). Before that, on `art-one-button-card` after the final review's fixes, 2026-09-11: `3252 passed; 0 failed; 58 ignored` (run twice, `TMP`/`TEMP` on `E:`; fmt and clippy clean). The six new tests are in `core::card::sizing`: the ceiling clamp, the floor at `PFS3_MAX_BYTES`, two Work-split guards, and two that pin the ceiling and `built_bytes` to the RDB writer. Before that, on the same branch, 2026-09-11: `3246 passed; 0 failed; 58 ignored` (run twice, `TMP`/`TEMP` on `E:`; fmt and clippy clean) — round 1 of the one-button card work (task 7): ART-308 and ART-309 fixed, the M10 mutation's survivor closed with a new test, and `[profile.dev.package.libpfs3] opt-level = 3` added to `src-tauri/Cargo.toml`, which cut the `core::card::sizing` module from 101.05 s to 11.21 s and the whole suite from ~120 s to ~35 s with no test dropped or shrunk. Before that, on `art-307-pistorm-a1200-rom`, 2026-09-11: `3225 passed; 0 failed; 58 ignored` (run twice; fmt and clippy clean). Before that, on `art-305-emu68-any-release`, 2026-09-11: `3224 passed; 0 failed; 58 ignored` (run twice, `TMP`/`TEMP` on `E:`; fmt and clippy clean). Before that, on `art-304-shared-top-level`, 2026-09-11: `3218 passed; 0 failed; 58 ignored` (run twice, `TMP`/`TEMP` on `E:`; fmt and clippy clean) — ART-304's eight new tests, one of them the ignored owner's-material test. Before that, on `art-owner-findings-0910` (merged into an unmoved `main` as `be7a32f`), 2026-09-10: `3210 passed; 0 failed; 57 ignored` (run twice, `TMP`/`TEMP` on `E:`; clippy clean). The four new ignored are the three `measure_*` harnesses and the Turkish real-material test. Before that, on `art-291-292-295` (merged into an unmoved `main` as `ad786ff`), 2026-09-10: `3205 passed; 0 failed; 53 ignored` (run twice; clippy clean). Before that, on `art-293-plan-root` (merged into an unmoved `main` as `8398564`), 2026-09-10: `3205 passed; 0 failed; 53 ignored` (run twice). Before that, on `art-294-rehearsal-ceiling` (merged into an unmoved `main` as `6698859`), 2026-09-10: `3204 passed; 0 failed; 53 ignored` (run twice). Before that, on `art-285-install-media` (merged into an unmoved `main` as `c2bcdda`), 2026-09-10: `3200 passed; 0 failed; 53 ignored` (run twice). Before that, on the merged `main` (`f9602c3`, which adds no Rust over `9234368`), 2026-09-10: `3197 passed; 0 failed; 53 ignored` once; on `art-278-runaway`, 2026-09-10: `3197 passed; 0 failed; 53 ignored (run twice, 2026-09-10)`. On the 0.9.1 release head, 2026-09-09: `test result: ok. 3181 passed; 0 failed; 53 ignored` (run twice, ART-059). On `art-281-scratch` (merged as `9fffc52`; the merge adds no line the branch head lacked — the branch was cut at `ad61a7a` and `main` had not moved), 2026-09-10: `test result: ok. 3188 passed; 0 failed; 53 ignored; 0 measured; 0 filtered out` (run twice). Two of the difference from 3181 are ART-281's own guard tests; the rest came with what `main` gained between the release head and `ad61a7a`, where the branch was cut, and was not attributed here |
| **Tests — frontend** | `pnpm test` on `main` (`e060e76`, the third debt round merged), 2026-09-15: `Test Files 112 passed (112)`, `Tests 1687 passed (1687)`; `pnpm lint` clean on the branch head. Before that, `pnpm test` on `art-debt-2-0914`, 2026-09-15 (ART-117's final-review fix wave — the stopped-run result panel, the unnamed-driver notes, five new catalogue keys): `Test Files 111 passed (111)`, `Tests 1685 passed (1685)`, `pnpm lint` clean. Before that, `pnpm test` on `art-debt-2-0914`, 2026-09-14 (ART-117 fixed, Task 12's own close-out verification; Task 10 was the last task to touch a frontend file, adding the RDB-backup field, the replace preview and both catalogues): `Test Files 111 passed (111)`, `Tests 1683 passed (1683)`, `pnpm lint` clean. Before that, `pnpm test` on `art-debt-0914` (plan 5, ART-317 — no frontend file changed by this plan; `git log --oneline -- src/` still ends at `6dcae37`, ART-301's own last commit), 2026-09-14: `110` files / `1671` tests passed, `pnpm lint` clean — 2 more than the row below's `1669`: `git log -S"1669" -- docs/STATUS.md` shows the `1669` row was written by `f44cba1` (ART-301 fixed); the next commit, `ae0f97f` ("close plan 4's final-review findings — tests and JobTitle"), added exactly two tests to `src/i18n/job-title-keys.test.ts`, and its own docs commit (`330b1e5`) never updated the count. `1671` has been correct since `ae0f97f`. Before that, `pnpm test` on `art-debt-0914` after ART-301, 2026-09-14: `110` files / `1669` tests passed, `pnpm lint` clean. Before that, `pnpm test` on `main` (`225f3af`), 2026-09-13: `109` files / `1658` tests passed, `pnpm lint` clean. Before that, `pnpm test` on the merged `main` (`803ec5f`), 2026-09-11: `109` files / `1658` tests passed. Before that, on `art-one-button-card`, 2026-09-11: `109` files / `1658` tests passed, `pnpm lint` clean — round 1's task 7, no frontend test count change over task 6 (the mutation script restored every file byte for byte). Before that, on `art-307-pistorm-a1200-rom`, 2026-09-11: `109` files / `1655` tests passed, `pnpm lint` clean (two catalogue sentences reworded, no test count change). Before that, on `art-305-emu68-any-release`, 2026-09-11: `Test Files 109 passed (109)`, `Tests 1655 passed (1655)`; `pnpm lint` clean. Before that, on `art-files-columns` (merged as `cdcc54b`), 2026-09-10: `Test Files 109 passed (109)`, `Tests 1654 passed (1654)` — the columns' model, menu and six screen tests. Before that, on `art-owner-findings-0910` (merged as `be7a32f`), 2026-09-10: `Test Files 107 passed (107)`, `Tests 1632 passed (1632)`. Before that, on `art-291-clear-on-removal` (merged as `1596fe4`): `Tests 1628 passed (1628)` — the four-tab rewrite's 1609, ART-278's two, ART-283's three, ART-285's five, ART-294's two, ART-292's three, ART-291's four. On the 0.9.1 release head, 2026-09-09: 91 / 1388 |
| **Lint, format, clippy** | Clean on `art-debt-3-0915` at `870a0fd`, 2026-09-15 (the tree merged as `e060e76`): `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `pnpm lint`; `cargo deny check` last at `11f820f` (`advisories ok, bans ok, licenses ok, sources ok`), `Cargo.lock` unchanged since. Before that, Clean on `art-debt-3-0915`, 2026-09-15 (ART-323 fixed): `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo deny check` (`advisories ok, bans ok, licenses ok, sources ok`). The vendored crate is outside `cargo fmt`; default `rustfmt` wraps one new `get_chain` call in `writer.rs` differently, the same layout the file's existing ART-318 code has, left alone. No frontend file changed, so `pnpm lint` was not re-run. Before that, Clean on `art-debt-3-0915`, 2026-09-15 (the round's final review fix wave): `cargo fmt --check` (after `cargo fmt` rewrapped three new test lines), `cargo clippy --all-targets -- -D warnings`, `cargo deny check` (`advisories ok, bans ok, licenses ok, sources ok`), `pnpm lint`. Before that, Clean on `art-debt-3-0915`, 2026-09-15 (ART-322 fixed, item 3b): `cargo fmt --check` (after `cargo fmt` rewrapped the new test code in `native.rs`; the vendored `writer.rs` change needed no rewrap), `cargo clippy --all-targets -- -D warnings`, `cargo deny check` (`advisories ok, bans ok, licenses ok, sources ok`). No frontend file changed, so `pnpm lint` was not re-run. Before that, Clean on `art-debt-3-0915`, 2026-09-15 (ART-319's disclosed gaps closed, item 3): `cargo fmt --check` (after `cargo fmt` rewrapped the new test code), `cargo clippy --all-targets -- -D warnings`, `cargo deny check` (`advisories ok, bans ok, licenses ok, sources ok`). The vendored crate is outside `cargo fmt`; default `rustfmt` differs from its `writer.rs` in three places, none in code this item touched, and they were left alone. No frontend file changed, so `pnpm lint` was not re-run. Before that, Clean on `art-debt-3-0915`, 2026-09-15 (ART-317 survivors (b)/(c) guarded, item 2): `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` — no frontend file changed by this fix, so `pnpm lint` was not re-run. Before that, Clean on `art-debt-3-0915`, 2026-09-15 (ART-321 fixed, item 1): `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`; `pnpm lint` clean. Before that, Clean on `art-debt-2-0914`, 2026-09-15 (ART-117's final-review fix wave): `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` (after boxing the two stop types for `clippy::result_large_err`), `cargo deny check` (`advisories ok, bans ok, licenses ok, sources ok`), `pnpm lint`. Before that, on `art-debt-2-0914`, 2026-09-14 (ART-117's own close-out verification, Task 12): `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean; `pnpm lint` clean. `cargo deny check` failed at that point on one `rustls` advisory via `ureq`, unrelated to any change this branch made; *corrected 2026-09-15:* `31566bf` bumped `rustls` to 0.23.45 in `Cargo.lock` for RUSTSEC-2026-0285, after which `cargo deny check` gave `advisories ok, bans ok, licenses ok, sources ok`. Before that, Clean on `art-debt-2-0914`, 2026-09-14 (ART-062's mechanical-part close-out): `pnpm lint` (`tsc --noEmit` twice) — no Rust file changed by this fix, so `cargo fmt`/`clippy`/`deny` were not re-run. Before that, Clean on `art-debt-2-0914`, 2026-09-14 (ART-250's own close-out verification): `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` — no frontend file changed by this fix, so `pnpm lint` and `cargo deny check` were not re-run. Before that, Clean on `art-319-writer-rollback`, 2026-09-14 (ART-319's own close-out verification): `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo deny check` (`advisories ok, bans ok, licenses ok, sources ok`) — no frontend file changed by this fix, so `pnpm lint` was not re-run. Before that, Clean on `art-debt-0914`, 2026-09-14 (plan 5, ART-317's own close-out verification): `pnpm lint`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo deny --manifest-path src-tauri/Cargo.toml check` (`advisories ok, bans ok, licenses ok, sources ok`). Before that, Clean on `art-debt-0914`, 2026-09-14 (plan 4's final review fix wave — ART-301 job-title findings): `pnpm lint`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`. Before that, Clean on `art-debt-0914`, 2026-09-14 (plan 3's final review fix wave): `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo deny check` (`advisories ok, bans ok, licenses ok, sources ok`). Before that, Clean on `art-debt-0914`, 2026-09-14 (plan 2's fix wave, ART-302/ART-300 final review): `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`. Before that, Clean on `art-310-libpfs3-format` before its merge and on `main` after it, 2026-09-13: `pnpm lint`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` (the merged `src-tauri` is the branch's). Before that, Clean on the merged `main`, 2026-09-07: `pnpm lint` (run unpiped — a pipe reports `tail`'s status, not `tsc`'s), `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` |
| **Sweeps** | **Re-run 2026-09-15 on `art-debt-3-0915` (ART-323 fixed), clean:** `scripts/control-byte-sweep.py` (`clean - 7 file(s) allow-listed for AmigaDOS DosType data and 7 for deliberate alignment; no stray control bytes and no lost line continuations anywhere else`, re-run after the last doc edit); `scripts/pfs3-oracle-check.py` (`ART_PFS3_DRIVER=E:\amiga\Amigatolon\hstimager\pfs3aio`) exit 0, "ART and hst-imager agree, both directions" (2 Windows-reserved names skipped, ART-114); `scratch-guard-sweep.py`, `scratch-root-sweep.py` and `contrast-check.py` not re-run (the new tests use `MemDevice`, no scratch; no frontend file changed). Before that, **Re-run 2026-09-15 on `art-debt-3-0915` (the round's final review fix wave), clean:** `scripts/control-byte-sweep.py` (`clean - 7 file(s) allow-listed for AmigaDOS DosType data and 7 for deliberate alignment; no stray control bytes and no lost line continuations anywhere else`, re-run after the last doc edit) and `scripts/scratch-guard-sweep.py` (`clean — 149 guard sources (by return type), 1873 call sites, 3 hand-built paths, 9 platform-root arguments (12 exempt)`); `pfs3-oracle-check.py` exit 0; `scratch-root-sweep.py` and `contrast-check.py` not re-run (no staging site and no frontend file changed). Before that, **Re-run 2026-09-15 on `art-debt-3-0915` (ART-322 fixed, item 3b), clean:** `scripts/control-byte-sweep.py` (`clean - 7 file(s) allow-listed for AmigaDOS DosType data and 7 for deliberate alignment; no stray control bytes and no lost line continuations anywhere else`); `scratch-guard-sweep.py`, `scratch-root-sweep.py` and `contrast-check.py` not re-run (no new scratch-guard site and no frontend file changed). Before that, **Re-run 2026-09-15 on `art-debt-3-0915` (ART-319's disclosed gaps, item 3), both clean:** `scripts/scratch-guard-sweep.py` (`clean — 149 guard sources (by return type), 1873 call sites, 3 hand-built paths, 9 platform-root arguments (12 exempt)`) and `scripts/control-byte-sweep.py` (re-run after the last doc edit: `clean - 7 file(s) allow-listed for AmigaDOS DosType data and 7 for deliberate alignment; no stray control bytes and no lost line continuations anywhere else`); `scratch-root-sweep.py` and `contrast-check.py` not re-run (no staging site and no frontend file changed). Before that, **Re-run 2026-09-15 on `art-debt-2-0914` (ART-117's final-review fix wave), all clean:** `scripts/control-byte-sweep.py`, `scripts/scratch-guard-sweep.py` (`clean — 149 guard sources (by return type), 1864 call sites, 3 hand-built paths, 9 platform-root arguments (12 exempt)`), `scripts/scratch-root-sweep.py` (5 named exceptions) and `scripts/contrast-check.py --quiet` (all 105 pairs, both themes). Before that, **Re-run 2026-09-14 on `art-debt-2-0914` (ART-117's close-out, Task 12), all clean:** `scripts/control-byte-sweep.py`, `scripts/scratch-root-sweep.py`, `scripts/scratch-guard-sweep.py` (`clean — 148 guard sources (by return type), 1853 call sites, 3 hand-built paths, 9 platform-root arguments (12 exempt)`, unchanged from Task 11) and `scripts/contrast-check.py --quiet` (all 105 pairs, both themes); `scripts/scratch-counter-sweep.py` shows its one known finding (ART-235, non-blocking) and nothing new. Before that, **Re-run 2026-09-14 on `art-debt-2-0914` (ART-062's mechanical-part close-out), both clean:** `scripts/control-byte-sweep.py` — "7 file(s) allow-listed for AmigaDOS DosType data and 7 for deliberate alignment; no stray control bytes and no lost line continuations anywhere else"; `scripts/contrast-check.py --quiet` — all 105 pairs, both themes. Also new this round: `scripts/tr-overflow-check.py`, a headless-Chrome sweep for Turkish text clipping (ART-062), not in CI — 48 (route x width) runs against `pnpm dev`, 0 errors, 0 crashes, Turkish-only list empty. Before that, **Re-run 2026-09-14 on `art-debt-2-0914` (ART-250's close-out): `scripts/control-byte-sweep.py` clean** — "7 file(s) allow-listed for AmigaDOS DosType data and 7 for deliberate alignment; no stray control bytes and no lost line continuations anywhere else" (the other sweeps were not re-run; this fix touched no scratch-root, scratch-guard or contrast-relevant code). Before that, **Re-run 2026-09-14 on `art-319-writer-rollback` (ART-319's close-out), all clean:** `scripts/control-byte-sweep.py`,
`scripts/scratch-root-sweep.py` and `scripts/scratch-guard-sweep.py` (`contrast-check.py` not re-run — no frontend
file changed by this fix). Earlier: **Re-run 2026-09-14 on `art-debt-0914` (plan 5, ART-317's close-out), all clean:**
`scripts/control-byte-sweep.py` — "7 file(s) allow-listed for AmigaDOS DosType data and 7 for deliberate
alignment; no stray control bytes and no lost line continuations anywhere else"; `scripts/scratch-root-sweep.py`
— 5 named exceptions; `scripts/scratch-guard-sweep.py` — 146 guard sources (by return type), 1814 call sites,
3 hand-built paths, 9 platform-root arguments (12 exempt); `scripts/contrast-check.py --quiet` — all 105 pairs
clear their threshold, both themes. Earlier: Re-run 2026-09-10 on `art-291-292-295`: `scripts/scratch-root-sweep.py` clean with **5** named exceptions — ART-295 removed the three osinstall whole-file ones — and now reads test code item by item; `scripts/scratch-guard-sweep.py` clean, 12 exempt, `--self-test` 20/20. Earlier: Re-run 2026-09-07 on the merged `main` (control-byte) and on the branch head `e46062a` (the rest), all clean: `scripts/control-byte-sweep.py` — clean, and now also catches a stray mid-line TAB inside tracked text, not only one that swallows a line continuation ([ART-271](ISSUES.md#fixed)); `scripts/scratch-root-sweep.py` — clean, 8 named exceptions; `scripts/contrast-check.py --quiet` — **105/105** pairs, both themes, on the merged `main` (the Windows 11 round replaced the script's six `color-mix` badge pairs with nine flat-tint pairs it actually draws — 113 before). `scripts/scratch-counter-sweep.py`: `already had a counter: 25`, `needing a counter: 0` — the false positive on a pid hashed with the thread id is gone ([ART-235](ISSUES.md#fixed)). **Re-run on `art-281-scratch`, 2026-09-10**, all clean: control-byte, scratch-root (8 named exceptions), and the counter sweep now `sites total: 10`, `production (never touched): 8`, `already had a counter: 2`, `needing a counter: 0` — 25 became 2 because [ART-281](ISSUES.md#fixed) moved those names inside `ScratchDir::pair`, where one production site now builds them all. New and blocking in CI: `scripts/scratch-guard-sweep.py` — `clean — 69 helpers, 765 call sites, 3 hand-built paths, 4 platform-root arguments (7 exempt)` |
| **Build** | `pnpm tauri build` on `main` at `62f0f81` (0.9.3), 2026-09-13: exit 0 in 350 s; `_x64-setup.exe` 5,670,486 B and `_x64_en-US.msi` 7,929,856 B, copied to `E:\amiga\ProjeART\build\` as `Amiga Retro Toolkit_0.9.3_x64-setup_main-62f0f81.exe` and `…_x64_en-US_main-62f0f81.msi`, SHA-256 identical to the bundle. Before that, a package from `art-312-anode-reuse` at `dc3cdd6` (0.9.2 version string) in the same folder. Before that, `pnpm tauri build` on `main` at `d698522` (0.9.2), 2026-09-13: exit 0 in 363 s; `_x64-setup.exe` 5,670,026 B and `_x64_en-US.msi` 7,929,856 B, copied to `E:\amiga\ProjeART\build\` as `Amiga Retro Toolkit_0.9.2_x64-setup_main-d698522.exe` and `…_x64_en-US_main-d698522.msi`, SHA-256 identical to the bundle. Before that, `pnpm tauri build` on `main` at `09d52fb` (the ART-305/306 merge), 2026-09-11 01:52: exit 0, `_x64-setup.exe` 5,670,328 B, copied to `E:\amiga\ProjeART\build\` as `Amiga Retro Toolkit_0.9.1_x64-setup_main-09d52fb.exe`, SHA-256 `8fb27ed4…` identical to the bundle, **not code-signed**, handed to the owner to write a card from their 1.1-beta Emu68. Before that, `pnpm tauri build` on `main` at `ae030ff` (the ART-304 merge), 2026-09-11 01:03: exit 0, `_x64-setup.exe` 5,672,827 B, copied to `E:\amiga\ProjeART\build\` as `Amiga Retro Toolkit_0.9.1_x64-setup_main-ae030ff.exe`, SHA-256 `ae5093ef…` identical to the bundle, **not code-signed**, handed to the owner to drive on `E:/amiga/Amigatolon/sonuclar`. Before that, `pnpm tauri build` on `main` at `f354f46`, 2026-09-10 21:01: exit 0, `Amiga Retro Toolkit_0.9.1_x64_en-US.msi` (7,901,184 B) and `_x64-setup.exe` (5,653,806 B); the setup copied to `E:\amiga\ProjeART\build\` with a `_main-f354f46` suffix, SHA-256 identical to the bundle, **not code-signed**, not yet driven. Before that, `pnpm tauri build` on `art-win11-look` (`991c485`), 2026-09-07 22:53 — `Amiga Retro Toolkit_0.9.0_x64_en-US.msi` (7,782,400 B) and `_x64-setup.exe` (5,554,795 B), copied to `E:\amiga\ProjeART\build\` with a `_win11-look` suffix, **not code-signed**, which the README says rather than leaving to SmartScreen. The last build of `main` itself is the 0.9.0 bundle of 2026-09-04 |
| **i18n** | **2262** leaf strings in each of `en.json` and `tr.json`, counted 2026-09-14 on `art-debt-0914` (ART-301 added 47). Before that, **2215** leaf strings in each of `en.json` and `tr.json`, counted 2026-09-13 on `main`. Before that, `src/i18n/en.json` and `tr.json`: **2201** leaf keys each, counted 2026-09-10 on the merged `main` (2170 on 2026-09-09 on `art-091-fixes` (2 082 on 2026-09-07; the intake rounds added, the BoingBag cleanup removed twenty-one). Parity — key sets, empty values, interpolation variables — is enforced by `pnpm test`, so count them rather than quoting this |
| **Open defects** | **5** on `main` (`e060e76`), counted 2026-09-15 after the third debt round's merge — ART-062, [ART-324](ISSUES.md#open), [ART-328](ISSUES.md#open), [ART-329](ISSUES.md#open) and [ART-331](ISSUES.md#open). Counted by the `**ART-` entries between `## Open` and `## Fixed` in docs/ISSUES.md. Before that, **2** on `art-debt-2-0914`, counted 2026-09-15 after the ART-117 final review's fix wave — ART-062 and [ART-321](ISSUES.md#open) (`mbr_slot_of` can name the wrong MBR slot, filed from the review's M12, found by reading); ART-320 was fixed after the count below. Counted with `awk '/^## Open/{f=1} /^## Fixed/{f=0} f' docs/ISSUES.md | grep -c '^\*\*ART-'`. Before that, **2** on `art-debt-2-0914`, counted 2026-09-14, ART-117 moved to Fixed and [ART-320](ISSUES.md#open) filed open (the forced-TMP finding) — ART-062 and 320. Counted with `awk '/^## Open/{f=1} /^## Fixed/{f=0} f' docs/ISSUES.md | grep -c '^\*\*ART-'`. Before that, **2** on `art-debt-2-0914`, counted 2026-09-14, ART-250 moved to Fixed — ART-062 and 117. Counted with `awk '/^## Open/{f=1} /^## Fixed/{f=0} f' docs/ISSUES.md | grep -c '^\*\*ART-'`. Before that, **3** on `art-319-writer-rollback`, counted 2026-09-14, ART-319 moved to Fixed — ART-062, 117 and 250. Counted with `awk '/^## Open/{f=1} /^## Fixed/{f=0} f' docs/ISSUES.md | grep -c '^\*\*ART-'`. Before that, **4** on `art-debt-0914`, counted 2026-09-14, ART-317 moved to Fixed (plan 5) — ART-062, 117, 250 and 319. Counted with `awk '/^## Open/{f=1} /^## Fixed/{f=0} f' docs/ISSUES.md | grep -c '^\*\*ART-'`. Before that, **5** on `art-debt-0914`, counted 2026-09-14, ART-301 moved to Fixed — ART-062, 117, 250, 317 and 319. Before that, **6** on `art-debt-0914`, counted 2026-09-14 after plan 3's final-review fix wave — ART-062, 117, 250, 301, 317 and 319. Task 8 moved ART-311, 313, 314, 315 and 316 to Fixed and filed ART-318 fixed and ART-319 open in the same close-out; the fix wave widened ART-319's scope (M4) and fixed six review findings (M2, M3, M5, M6, M9 guard, plus the docs-only I1/I2/M1) without filing a new ID. Counted with `awk '/^## Open/{f=1} /^## Fixed/{f=0} f' docs/ISSUES.md | grep -c '^\*\*ART-'`. Before that, **10** on `art-debt-0914`, counted 2026-09-14 after Task 8's own close-out — ART-062, 117, 250, 301, 311, 313, 314, 315, 316 and 317. ART-300 and ART-302 fixed this session (task 5); ART-118 (closed as superseded) and ART-313 through ART-317 (filed by concurrent PFS3 research on this branch) were already reflected in `docs/ISSUES.md` when this session began. Before that, **8** on `art-312-anode-reuse`, counted 2026-09-13 — ART-062, 117, 118, 250, 300, 301, 302 and 311 (ART-312 fixed there, not yet merged). Before that, **9** on `main` at `225f3af`, counted 2026-09-13 — ART-062, 117, 118, 250, 300, 301, 302, 311 and 312 (ART-310 fixed; ART-311 and ART-312 filed by the ART-310 round). Before that, **7** on `main` at `706de9f`, counted 2026-09-10 — ART-303 moved to Fixed by the owner's ruling; before it **8** on `main` at `cdcc54b` — ART-303 filed from the owner's drive of `main-e2633a6`; before it **7** on `main` at `be7a32f`, counted 2026-09-10 with the command below — ART-297, ART-298 and ART-299 filed and fixed in the same round, ART-300, ART-301 and ART-302 filed open; before it **4** on `main` at `1596fe4` — ART-291 moved to Fixed by the owner's ruling; before it **5** at `ad786ff`: ART-292 and ART-295 moved to Fixed, ART-296 filed and fixed in the same round, ART-291 still open after its proposed fix was measured wrong and reverted; before it **7** at `8398564`: ART-293 moved to Fixed and its second half filed as ART-295; before it **7** at `6698859`: ART-294 moved to Fixed; before it **8** at `c2bcdda`: ART-285 moved to Fixed; before it **9** at `0737c2d`: ART-283 moved to Fixed; before it **10** at `f9602c3`: the four-tabs merge brought ART-291 and ART-292 (both 🔵); before it **8** at `9234368`: ART-278 and ART-279 moved to Fixed, ART-294 filed (9 on `main` at `2b5096e`). Before that **5** on `main`, counted 2026-09-07 evening with `awk '/^## Open/{f=1} /^## Fixed/{f=0} f' docs/ISSUES.md \| grep -c '^\*\*ART-'` — [ART-261](ISSUES.md#fixed) closed the same evening when the owner installed the antivirus exclusions and the full suite printed its summary twice. Thirteen closed across the debt-clearing wave (19 open before it, 6 after, 5 now): ART-271, ART-235, ART-260, ART-245, ART-247, ART-246, ART-274, ART-251, ART-248, ART-241, ART-243, ART-244, ART-242. Before the wave: 19 |
| **Feature rows** | **216 marked rows** in [FEATURES.md](FEATURES.md) — **159 green, 26 amber, 21 not started, 6 stubs, 4 deferred to v2** — counted 2026-09-07, one marker per table row, taken from the first cell in that row that is exactly a State marker. **State the method with the number**: counting marker *cells* instead gives a different figure, and an earlier count of "233 rows" left behind no method and cannot now be reproduced |
| **amitools oracle** | 53 checks, both directions (`scripts/oracle-check.py`, blocking in CI) — including a filesystem driver ART embedded in an RDB and `rdbtool` extracted back out byte-for-byte |
| **Kickstart table** | 154 dumps (`core/rom/remus.rs::REMUS_ROMS`, counted 2026-09-07), generated from amitools' Remus split database and re-verified against it on every CI run (`scripts/rom-table-check.py`, [ART-104](ISSUES.md#fixed)). Licensed Amiga Forever ROMs are first-class input ([ART-128](ISSUES.md#fixed)): decoded with the `rom.key` beside them, then identified like any dump |
| **Install-media table** | 186 rows, 186 distinct MD5s, adopted from `rootrootde/emu68hatcher` (MIT). `scripts/media-table-check.py` against the owner's own AmigaOS 3.2 ADFs, 2026-09-06: **35 verified, 151 unverified, 0 conflicting**; the Rust twin agrees. Both fail when a directory verifies nothing ([ART-265](ISSUES.md#fixed)) |
| **7-Zip oracles** | The card's FAT32 boot partition read back by 7-Zip — type, geometry, label, names and every file's bytes (`scripts/fat-oracle-check.py`); and 4 disc fixtures — Joliet, ISO9660-only, raw Mode 1, raw Mode 2/XA — names, sizes, SHA-256 per file (`scripts/iso-oracle-check.py`). Neither in CI |
| **hst-imager PFS3 oracle** | Both directions, local only (`scripts/pfs3-oracle-check.py`): ART writes a volume through `NativeFormatter` and `hst-imager fs dir -r` reads it back; `hst-imager` formats and fills one and ART reads it back through `libpfs3`, SHA-256 per file plus protection strings |
| **ILBM / prefs oracle** | `scripts/ilbm-oracle-check.py` — ffmpeg agrees with ART's ByteRun1 encoder pixel-for-pixel on **8 of 8** fixtures. Against the owner's own AmigaOS 3.9 material, **15 of 24** `.prefs` files are genuine `FORM PREF` containers and all 15 round-trip byte-for-byte through the real rebuilder (`replace_bodies(&[])`); the other 9 are third-party non-IFF formats, reported apart |
| **Icon oracle** | `scripts/icon-oracle-check.py` round-trips real `.info` files through `core/amigaicon`'s reader **and its four writers**, against the owner's own `E:\amiga\Amigatolon\os39` (798 real icons): **2026-09-14, on `art-debt-2-0914` (ART-250 fixed — ToolTypes are Latin-1, byte-exact): `checked=798 failed=0 no_drawer_data2=0`, the `lossy_tooltypes` bucket gone (0 of 798, was 69) because `tooltypes`/`set_tooltypes` now agree with AmigaOS about the charset.** Re-run both directions: the pre-fix code (`git show HEAD:...`) reproduced the prior `lossy_tooltypes=69` exactly, and the fixed code round-trips all 798 byte-identically, including the 69 that used to fall into that bucket. Before that: `checked=798 failed=0 no_drawer_data2=0 lossy_tooltypes=69`. Its first real run found [ART-249](ISSUES.md#fixed), a writer offset landing inside the wrong struct that no unit test could see |
| **cargo-deny** | advisories, bans, licences, sources — all ok |
| **MSRV** | 1.93 (raised from 1.77 on 2026-08-12, for a maintained 7z decoder) |
| **Published** | <https://github.com/tolon/Art> — public, `main`, **GPL-3.0-or-later**. [v0.9.0](https://github.com/tolon/Art/releases/tag/v0.9.0) is released with both installers attached (NSIS 5.1 MB, MSI 6.3 MB), built by `release.yml` from the tagged commit after CI went green on it. The built `.exe` was launched and answered before the draft was published |
| **Real hardware** | **Bare metal, 2026-08-12**: `test/art-bootable-test.adf` booted a real **A500/A500+** (Kickstart 3.9) from a **Gotek** to an AmigaDOS CLI. Photographed. In emulation, 2026-08-16: a PFS3 volume ART formatted and filled booted a licensed Kickstart 3.1 to `hello from ART`, and a full AmigaOS 3.2 tree ART built booted a licensed V47 A1200 ROM to a clean Workbench. **No card or hard disk ART built has reached real hardware**, and physical magnetic media is still untouched — a Gotek is not a mechanical drive |
| **Seen on a screen** | The ADF, hard-disk, PiStorm, WHDLoad, Files and Settings screens have been opened and driven by a person (2026-08-12 onwards); the Collection's Play path ran real titles on 2026-08-18/21. **[ART-062](ISSUES.md)** narrows to the screens still unopened (Aminet, Collection, Gotek, ROM, WinUAE, Tools) and to the Turkish catalogue, of which a handful of strings out of 2262 have been read by someone who speaks it. A headless browser, 2026-09-14, mounted every top-level route in both languages and measured layout (not meaning) — that is not the same claim as a person reading the screen, but it did find and fix ART-062's one measurable clip (`files.pane.nothingOpen`); see [ISSUES.md](ISSUES.md#open) |

Reproduce the numbers above:

```bash
pnpm lint                                              # TypeScript (run unpiped)
pnpm test                                              # frontend unit tests (i18n parity, phrase keys)
cd src-tauri && cargo fmt --check                      # formatting
cd src-tauri && cargo clippy --all-targets -- -D warnings
cd src-tauri && cargo test --lib                       # the full suite, twice — ART-059 (ART-261 is closed)
pip install amitools && python scripts/oracle-check.py # independent cross-check
python scripts/iso-oracle-check.py                     # the disc reader vs 7-Zip (needs 7z; not in CI)
python scripts/fat-oracle-check.py                     # the card's boot partition vs 7-Zip (needs 7z; not in CI)
python scripts/pfs3-oracle-check.py                    # PFS3, both directions, vs hst-imager (needs hst.imager.exe; not in CI)
python scripts/catalogue-check.py                      # every shipped bundle path vs Aminet itself (not in CI, it leaves the machine)
python scripts/zoom-check.py                           # the shell's widths, in a real browser (needs `pnpm dev`)
python scripts/tr-overflow-check.py                     # Turkish text clipping vs English, per route and width
                                                       #   (ART-062; needs `pnpm dev`)
python scripts/osbuilder-strip-check.py                # the OS Builder's step strip: per kind, both languages,
                                                       #   three Application Sizes (needs `pnpm dev`)
python scripts/contrast-check.py --quiet               # every colour pair in both themes, against WCAG (in CI)
python scripts/control-byte-sweep.py                   # no stray BEL/BS/VT/FF/ESC in tracked text (ART-216; in CI)
python scripts/scratch-root-sweep.py                   # every production staging site goes through the chosen
                                                       #   scratch root, not %TEMP% (ART-196; in CI)
python scripts/scratch-counter-sweep.py                # test scratch names unique *within one process*
                                                       #   (ART-059/164/173; not in CI — one known false
                                                       #   positive, ART-235)
python scripts/rom-table-check.py                      # the Kickstart table vs amitools' Remus data (ART-104; in CI)
python scripts/vhd-oracle-check.py                     # the dynamic VHD writer vs Microsoft's Get-VHD
                                                       #   (needs the Hyper-V PowerShell module; not in CI)
cd src-tauri && cargo deny check                       # licences and advisories
pnpm tauri build                                       # full bundle (slow)
```

**The `#[ignore]`d hooks are the other half**, and they are where ART meets
material nobody here wrote. `cargo test -- --ignored` lists them; each is
env-gated on the owner's own disks and none is in CI.

```bash
# The OS-install engine against the owner's own AmigaOS 3.2 ADFs.
cd src-tauri && ART_OSINSTALL_MEDIA="E:\amiga\Amigatolon\paketler\3.2\AmigaOs 3.2\ADF" \
  ART_OSINSTALL_ROM="E:\amiga\Amigatolon\kickstart\Kickstart v3.1 rev 40.68 (1993)(Commodore)(A1200).rom" \
  ART_OSINSTALL_DEST="E:\amiga\ProjeART\dist-3.2" \
  cargo test run_the_real_engine_against_the_users_own_media_when_asked -- --nocapture --ignored

# Puts an existing tree onto a card; it does not build one. Point it at
# dist-3.2 above and it reproduces ART-113's non-ASCII PFS3 refusal exactly.
cd src-tauri && ART_OSINSTALL_DEST="E:\amiga\ProjeART\dist-3.2-witness" \
  ART_CARD_OUT="E:\amiga\ProjeART\dist-3.2-witness-card.hdf" \
  cargo test build_the_real_dist_tree_onto_a_card_when_asked -- --nocapture --ignored

# The only one that carries the whole tree through the path the product
# actually runs — `run_with_fallback`, native first. `ART_HST` is optional:
# without it the run measures the native path alone and ends on the refusal a
# user with no hst-imager would see. Delete the output image first; SAFE_CREATE
# refuses an existing one.
cd src-tauri && ART_OSINSTALL_DEST="E:\amiga\ProjeART\dist-3.2" \
  ART_CARD_OUT="E:\amiga\ProjeART\dist-3.2-fallback.hdf" \
  ART_PFS3="E:\amiga\Amigatolon\hstimager\pfs3aio" \
  ART_HST="E:\amiga\Amigatolon\hstimager\hst.imager.exe" \
  cargo test carry_the_real_dist_tree_through_the_fallback_path_when_asked -- --nocapture --ignored

# ART-117: replace the PFS3 driver on a byte copy of the owner's card. Make the copy first — it is written; the
# original is only read, and the test asserts its reserved range and mtime unchanged. The driver must state a newer
# $VER: than the card's 19.2. The backup and the post-edit range are kept in
# ART_RDB_EMBED_OUT (an existing folder; nothing in it is overwritten).
# TMP/TEMP no longer need setting by hand (ART-320, fixed: src-tauri/.cargo/config.toml forces them onto
# E:\amiga\ProjeART\build\tmp with force = true), so this hook's own TMP-on-E: pre-flight now passes under a plain
# `cargo test --lib -- --ignored`; the compiled-binary workaround ART-320 needed before the fix is gone.
cd src-tauri && \
  ART_CARD_IN="E:\amiga\Amigatolon\caffeine\CaffeineOS_Storm_9317.img" \
  ART_RDB_EMBED_COPY="E:\amiga\ProjeART\build\tmp\caffeine-copy.img" \
  ART_RDB_EMBED_DRIVER="E:\amiga\ProjeART\build\tmp\pfs3aio-newer" \
  ART_RDB_EMBED_OUT="E:\amiga\ProjeART\art117-owner" \
  cargo test --lib core::preload::embed::tests::replace_the_driver_on_a_copy_of_the_owners_card_when_asked \
  -- --exact --nocapture --ignored

# ART-159's two language components, against the disc they were read off.
# Died mid-run four times in four on 2026-09-06 — exit 0, no `test result:`
# line, always at 561 of 2391 files. ART-261 named the cause (the scanner
# reacting to `art_lib`); the exclusions landed 2026-09-07 evening and the
# full suite now completes, so a clean run of THIS hook is possible and still
# owed — it has not been re-run since.
cd src-tauri && ART_159_ISO="E:\amiga\Amigatolon\iso\AmigaOS39.iso" \
  ART_159_DEST="E:\amiga\ProjeART\art159-tree" \
  cargo test --release build_the_real_39_language_components_when_asked -- --nocapture --ignored

# Builds AmigaOS 3.2.2 from the owner's own base and update media and then
# **asks the tree what it is** — `Prefs/Env-Archive/Versions/Release` is
# written by the release, so the claim comes from a file Hyperion wrote rather
# than from ART's own dropdown. Measured 2026-09-05: 4 052 files, 295 drawers,
# 20 667 671 bytes. Re-measured 2026-09-07 after ART-251 gave `workbench-base`
# two more `Subtree` rules: 4066 files, 295 drawers, 20706449 bytes (+14 files
# over the earlier number) — the tree still says `Release 3.2.2`. The ROM
# matters —
# `kicka1200.rom` is 47.96, so both update Modules components switch on and the
# base's own stays off. Unpack `Update3.2.2.lha` yourself; ART never reads it.
cd src-tauri && ART_322_BASE="E:\amiga\Amigatolon\paketler\3.2\AmigaOs 3.2\ADF" \
  ART_322_UPDATE="E:\amiga\ProjeART\layer-research\Update3.2.2\ADFs" \
  ART_322_ROM="E:\amiga\Amigatolon\paketler\3.2\AmigaOs 3.2\ROM\kicka1200.rom" \
  ART_322_DEST="E:\amiga\ProjeART\dist-3.2.2" \
  cargo test build_the_real_322_tree_when_asked -- --nocapture --ignored

# The icon oracle's Rust half. `scripts/icon-oracle-check.py` extracts the
# `.info` files and drives this; run it directly only when debugging that script.
cd src-tauri && ART_ICON_DIR="<a folder of extracted .info files>" \
  cargo test round_trip_every_icon_in_a_folder_when_asked -- --nocapture --ignored

# The install-media hash table's outside check — rom-table-check.py's sibling.
# Read-only: it only ever opens a file to hash it. `--expect-verified N` holds
# it to a number the CALLER states; 35 is this machine's measurement against
# this machine's disks and is deliberately never hardcoded, because a user with
# a 3.1 set must not meet a failing check.
python scripts/media-table-check.py "E:\amiga\Amigatolon\paketler\3.2\AmigaOs 3.2\ADF" \
  --expect-verified 35

# The same run, rewriting ART's own record of which rows this machine has
# confirmed — core/osinstall/media_hashes_confirmed.json, which the screen's
# "checked against a real disk here" sentence reads. `--against` is that
# sentence, so word it as a claim ART will put on screen. It appends rather
# than replacing (ART-267).
python scripts/media-table-check.py "E:\amiga\Amigatolon\paketler\3.2\AmigaOs 3.2\ADF" \
  --emit-confirmed --against "the ART author's own AmigaOS 3.2 install set, 35 ADFs"

# The Rust twin of the same check, through core's own row_for/rows rather than
# the script's independent re-implementation.
cd src-tauri && ART_MEDIA_DIR="E:\amiga\Amigatolon\paketler\3.2\AmigaOs 3.2\ADF" \
  ART_MEDIA_EXPECT_VERIFIED=35 \
  cargo test every_real_disk_in_a_folder_is_looked_up_through_cores_own_code_when_asked -- --nocapture --ignored

# The only test in ART that opens WinUAE against a real first-boot tree. Never
# touches the tree it is pointed at — `stage_with` copies it, first boot is
# written into the copy, and the copy is what boots; a failure keeps the copy
# and prints where it is. TMP/TEMP must be on E: (std::env::temp_dir()
# defaults to C:, and the rules forbid writing there); every run stages
# beside the tree it is given, so point ART_FIRSTBOOT_TREE at a copy under
# E:\amiga\ProjeART\fb3-trees\.
cd src-tauri && TMP=E:/amiga/ProjeART/build/tmp TEMP=E:/amiga/ProjeART/build/tmp \
  ART_FIRSTBOOT_TREE=E:/amiga/ProjeART/fb3-trees/art5 \
  ART_FIRSTBOOT_ROM="E:/amiga/Amigatolon/paketler/3.2/AmigaOs 3.2/ROM/kicka1200.rom" \
  ART_WINUAE="C:/Program Files/WinUAE/winuae64.exe" \
  cargo test rehearse_the_real_tree_when_asked -- --ignored --nocapture

# Phase 3, Task 10: the reboot on a real 3.2 tree that has C/Reboot — proves
# the wrapper waits 3s and reboots, and the next boot finishes done all.
cd src-tauri && TMP=E:/amiga/ProjeART/build/tmp TEMP=E:/amiga/ProjeART/build/tmp \
  ART_FIRSTBOOT_TREE=E:/amiga/ProjeART/fb3-trees/art5 \
  ART_FIRSTBOOT_ROM="E:/amiga/Amigatolon/paketler/3.2/AmigaOs 3.2/ROM/kicka1200.rom" \
  ART_WINUAE="C:/Program Files/WinUAE/winuae64.exe" ART_FIRSTBOOT_EXPECT_REBOOT=restarted \
  cargo test --lib rehearse_a_reboot_on_the_real_tree_when_asked -- --ignored --nocapture

# Phase 3, Task 10: the same reboot step on a real 3.9 tree with no
# C/Reboot — proves the wrapper logs "reboot unavailable" and carries on in
# the same boot.
cd src-tauri && TMP=E:/amiga/ProjeART/build/tmp TEMP=E:/amiga/ProjeART/build/tmp \
  ART_FIRSTBOOT_TREE=E:/amiga/ProjeART/fb3-trees/sonuclar \
  ART_FIRSTBOOT_ROM="E:/amiga/Amigatolon/kickstart/Kickstart v3.1 rev 40.68 (1993)(Commodore)(A1200).rom" \
  ART_WINUAE="C:/Program Files/WinUAE/winuae64.exe" ART_FIRSTBOOT_EXPECT_REBOOT=unavailable \
  cargo test --lib rehearse_a_reboot_on_the_real_tree_when_asked -- --ignored --nocapture

# Phase 3, Task 10: the wizard on a real tree with nobody at the keyboard —
# proves Locale opens and the rehearsal ends TimedOut at its deadline naming
# Locale as the waiting window. ART_FIRSTBOOT_DEADLINE_SECS defaults to 120;
# raise it (Task 12 uses 900) to answer the windows by hand instead.
cd src-tauri && TMP=E:/amiga/ProjeART/build/tmp TEMP=E:/amiga/ProjeART/build/tmp \
  ART_FIRSTBOOT_TREE=E:/amiga/ProjeART/fb3-trees/art5 \
  ART_FIRSTBOOT_ROM="E:/amiga/Amigatolon/paketler/3.2/AmigaOs 3.2/ROM/kicka1200.rom" \
  ART_WINUAE="C:/Program Files/WinUAE/winuae64.exe" \
  cargo test --lib rehearse_the_wizard_on_the_real_tree_when_asked -- --ignored --nocapture

# The real Aminet — every shipped mirror asked separately, because failover
# stops at the first that answers and a dead one in position two is invisible
# in an ordinary sync. Leaves the machine; never in CI.
cd src-tauri && cargo test live_aminet -- --ignored --nocapture

# ART-244's own measurement: `readers::lhadrawer`'s claim that reading an
# archive's drawers "seeks header to header rather than decompressing" is
# cheap, run against the owner's own 663 MB, 893-drawer archive rather than a
# fixture. Prints the count and elapsed time; asserts neither. Measured
# 2026-09-07: 3118 ms cold, ~1600-1631 ms warm, for one archive alone — which
# is what motivated `refresh_root`'s Update-mode archive cache (size+mtime,
# the same shape the file walk already had).
cd src-tauri && ART_LHA_ARCHIVE="E:\amiga\Amigatolon\paketler\WHDLoadDemos100.lha" \
  cargo test --lib real_archive_scan_is_fast -- --ignored --nocapture
```

---

## Picking up next session

*One block, kept current.* Every round used to add its own "Start here" and
leave the previous one below it; on 2026-09-04 four such blocks were collapsed
into this one, because the oldest still contradicted the newest. **Update this
block — do not stack another on top of it.**

### Start here (2026-09-16)

*Trimmed 2026-09-15 by the third debt round's final review fix wave to the live state. The per-round narrative
that used to stack here is in the [session log](session-log.md), per-defect detail in [ISSUES.md](ISSUES.md), and
the old block in git history.*

**First boot phase 3 is merged to `main` `--no-ff` as `4b72d7b` and pushed, both on the owner's word
(2026-09-16; branch `art-firstboot-phase-3` deleted, base `4a9722b`; CI run 35064080515 success), ten tasks: the wizard's
`90-prefs` (Locale and Input in the foreground, ScreenMode through `Run`, each skipped when ART already set it),
the step wrapper's own reboot (`C:Wait 3` + `C:Reboot` when the tree has `C/Reboot`, else logging `reboot
unavailable` and carrying on), and the OS Builder's second tick for it.** Measured under WinUAE on the owner's own
real trees (Task 10, each run once): the 3.2 tree (`C/Reboot` present) requests a reboot, restarts, and finishes
`done all` in 19.40 s; the 3.9 tree (`C/Reboot` absent) logs `reboot unavailable` and finishes `done all` in the
same boot in 22.91 s; asked for the wizard instead, both trees open Locale and, with nobody at the keyboard, time
out at the 120 s deadline naming Locale as the waiting window (120.48 s, 121.01 s). Task 11 closed the round —
documents, the whole suite twice, the sweeps — report
`.superpowers/sdd/2026-09-15-firstboot-phase-3/phase-3-report.md`. **The final whole-branch review** (opus,
`f359a14..939586b`, `.superpowers/sdd/2026-09-15-firstboot-phase-3/final-review.md`) said **ready to merge**: 0
Critical, 0 Important, 6 Minor. On the owner's word (2026-09-16) all six were fixed in one wave — the restart
sentence now names the effect rather than completion, the `ART_Set_Input` write makes its own drawer, the
`Written.removed` comment says where a removal is actually told, the `ART_Set_ScreenMode` marker takes no backup,
`askPrefs`'s "absent means ticked" is one exported helper, and the wizard's rows resolve case-insensitively like
the rest of the tree code — report `fix-wave-report.md` in the same folder. **Owed:** the owner's own closing measurement,
writing first boot on 3.2 and 3.9 and answering the Locale/Input/ScreenMode windows by hand (spec §8.3, Task 12),
which alone can move the FEATURES row off 🟡. **Settled 2026-09-16, no ART id on the owner's word:** amitools
`xdftool`-formatted FFS images fail the Kickstart validator after an unclean reboot ("Block 1146049281 out of
range", `0x444F5301` read as a block number), and ART's own formatter was then measured beside it — one variable
(the formatter), 3.2 ROM, 20160 × 512 blocks, 3 runs per arm: ART's `NativeFormatter` 3/3 clean, ART formatted and
filled 3/3 clean (the guest read ART's files on both boots), AmigaOS `Format` 3/3 clean, `xdftool` 0/3; without the
reboot all three clean. Not measured: 3.1/3.9 ROMs, other sizes, `DOS\3`, a real card. Report
`.superpowers/sdd/2026-09-16-art-ffs-validator/experiment.md`; the fixture rule is in
[testing.md](testing.md#real-material-and-the-ignored-hooks).

**The third debt round, `art-debt-3-0915`, is merged to `main` `--no-ff` as `e060e76` and pushed, both on the
owner's word (2026-09-15); 0.9.4 carries it.** Its items: ART-321 (the MBR slot carried
by each area), ART-317's survivors (b) and (c) guarded, ART-319's disclosed gaps closed (copy-on-write
`overwrite_file_in`, `rename_in` in one commit, every mutator tested), ART-322 (a case-only PFS3 rename no longer
deletes the file), ART-117 M5 (the three `$VER:` readers are one), and item 5 — CI's GitHub Actions moved to their
Node 24 majors (`f8dab92`: `actions/checkout` v4→v7, `setup-node` v4→v7, `setup-python` v5→v7, `upload-artifact`
v4→v7, `pnpm/action-setup` v4→v6, `softprops/action-gh-release` v2→v3). **The final whole-branch review** (opus,
`8b44277..89e386d`, `.superpowers/sdd/2026-09-15-debt-3-round/final-review.md`) said *With fixes*: 0 Critical,
2 Important, 7 Minor. **Its fix wave landed on the branch** (`fix-wave-report.md` beside it; libpfs3
`0.1.3+art.8`): I1, `rename_in` identifies the source by its name too and refuses a hard link as the destination
(ART-322); I2, the `Writer::open` guard resolves `use`/alias/grouped/`type` namings across all of `src/` and allows
only the helper's own call (ART-317); M1, a case-only rename of a Latin-1 name renames in place; M2, copy-on-write
refuses a bitmap that offers the old file's own block; M3/M5, the reachability records corrected and pfs3aio's
overwrite path read and cited (ART-319); M4, this record; M6, filed as ART-324; M7, this trim. **Found by the fix
wave:** [ART-323](ISSUES.md#fixed), libpfs3 deleting a hard link by freeing the file it names — **now fixed on the
branch** (libpfs3 `0.1.3+art.9`, ART-PATCH item 24). A link's delete removes only its entry; a link pfs3aio made,
an object links still name, and `create_hardlink` are refused by name. **The scoped re-review** (`89e386d..a9fd8c3`)
found five items; on the owner's decision **ART-325, ART-326 and ART-327 are fixed on the branch with them**
(libpfs3 `0.1.3+art.10`, ART-PATCH item 25, report `art325-327-report.md`): extra fields in pfs3aio's layout
everywhere — [ART-325](ISSUES.md#fixed) was reachable after all, through the first-boot report's and the install
check's PFS3 sizes; no second entry under an existing name; `rename_in` keeps the entry's own bytes and updates a
moved link's chain; directory-block walks bounded (item 5, a panic before); the link check's refusals name what
stopped them (item 4); the `Writer::open` guard sees globs, `<T>::open(`, `extern crate` aliases and skips comments
and strings (item 2). **The second scoped re-review** (`a9fd8c3..caceffb`) said *ready to merge* with six follow-ups;
on the owner's decision **all six are fixed on the branch** (libpfs3 `0.1.3+art.11`, ART-PATCH item 26, report
`followups-report.md`): `fsizex` is part of a size only on a largefile volume, and a file of 4 GiB or more is refused
by name elsewhere (1, [ART-324](ISSUES.md#open) narrowed to largefile volumes); a damaged PFS3 drawer is reported as
damaged, not as a missing file or "not booted" (6, [ART-330](ISSUES.md#fixed)); the extra-field fallback and the
Latin-1 fold's edges are tested (2, 3); the guard sees brace-rooted `use` trees (4); ART-328's wording (5). **The
scoped re-review of the follow-ups** (`caceffb..11f820f`, `followups-rereview.md`) said *ready to merge* with three
Minors; on the owner's decision **all three are fixed on the branch** (report `minors-report.md`): ART's PFS3 copy asks
`check_file_size` of every file before its fit check and before it reads one, and refuses a file of 4 GiB or more by
name, saying to leave it out (1, [ART-324](ISSUES.md#open)); ART-330's record names `firstboot.report.readFailed`
(2); `volume.rs`'s module doc names `Rootblock::has_largefile` (3, ART-PATCH item 26 amended, comment only, still
`0.1.3+art.11`). Still open: [ART-324](ISSUES.md#open) on a largefile volume, which ART never formats; [ART-328](ISSUES.md#open) and
[ART-329](ISSUES.md#open), unreachable; [ART-331](ISSUES.md#open), a directory block without the `DB` id skipped
silently — reachable only on a damaged card.

**Item 5 is proven.** `ci.yml`'s first run after the push, CI run 34960932926 on `e060e76`, completed `success`
(2026-09-15, 22 min; the Node 24 action majors ran on `windows-latest`), and `release.yml`'s, release run
34969262720 on `v0.9.4`, completed `success` and attached one NSIS and one MSI.

**0.9.4 is released** (2026-09-15, published on the owner's word). **Next**, on the owner's word (2026-09-16): round 2 of the one-button card
(`core/card/content.rs`, its own plan). First boot's phase 4 was checked against the tree first and **most of its
reason had gone**: its `run` of BoingBag 3.9-1/3.9-2 is superseded by ART-166's fix (both placed from Windows since
2026-09-08), and its `unpack` of the Turkish locale by ART-168's (names decoded as Latin-1 since 2026-08-20;
`locale-39-turkish` is host-placed). What is still Amiga-only is `boingbags-39-3-4` (`needs-installer-script`, its
`Install` unmeasured) and `euro-update` (`needs-fixfonts`); phase 4 waits, narrowed to those two. Phase 3 is merged
and awaits only the owner's closing measurement (Task 12).

**Still owed by a person** (each as last recorded; none is code):

- **A card flashed and an A500 booted** — nothing SD-1 or SD-2 built has reached real hardware; the project's 1.0
  bar. Also the owner's 2026-09-11 card (`E:/amiga/Amigatolon/Kartlar/1card.img`) booted on the PiStorm.
- **ART-117:** both PDS partitions of the edited card copy mounted in WinUAE or on a PiStorm, the loaded handler
  asked for `version full`.
- **PFS3 under the real handler:** a volume ART wrote, and a patched volume past MAXSMALLDISK, mounted under real
  pfs3aio in WinUAE; a file deleted and undeleted on a real Amiga from a PFS3 partition ART formatted.
- **ART-062:** the Turkish catalogue read on screen; `hardDisk.bootablePri` and job status "Done"/"Failed" need a
  live Tauri session. **ART-301:** with Turkish chosen, start a job and read the bar.
- **The OS Builder, driven** (four-tab spec § 7 and round 5's five additions, listed in
  `.superpowers/sdd/2026-09-09-four-tabs/round-5-report.md`): BoingBag 1, BoingBag 2, the Locale update and the
  Türkçe catalogs ticked from a clean destination, one press, five phase lines, then the tree booted under WinUAE
  and asked `version full`; the update-mode run on an existing 3.9 tree; `scripts/osbuilder-strip-check.py` in both
  languages; the WinUAE studio's install section and the rehearsal driven, shutting the window yourself.
- **The Files screen's columns** (2026-09-08 simplification spec § 5.10): hide Ext, widen Date, restart and see
  both kept; 28 px text; narrow a pane; Reset.

**Owner decisions still open:** the unreferenced trial images under `E:\amiga\ProjeART` (about 80 GB, nothing
deleted). `D:\tmp\art-tests` is settled: deleted on the owner's word on 2026-09-15 (719 directories, 1.56 GB).

**Deliberately open, disclosed:** `scripts/control-byte-sweep.py` walks the real filesystem, not `git ls-files`
(its header says why); the WinUAE studio asks `osinstall_describe_tree` twice per destination (four-tabs round 5
report § 8).

### Where the work stands

One paragraph per live area. Per-feature state is [FEATURES.md](FEATURES.md),
per-defect state is [ISSUES.md](ISSUES.md), per-round narrative is the
[session log](session-log.md). None of it is retold here.

- **The PiStorm card path** — SD-0, SD-1, SD-2 and SD-4 are built, SD-3's two
  named gaps are merged, SD-5 is part-built. The gaps still open are one line
  each in the stage table below. What is left in SD-1 is **not code**: a card
  flashed and an A500 booted, which is also the project's 1.0 bar.
- **The OS Builder** — builds a distribution tree from the owner's own
  AmigaOS 3.2, 3.2.2 (layered base + update) and 3.9 media, carries wallpaper,
  screen mode, shell defaults, network config and arranged drawer icons, and
  identifies install media by content hash. Its remaining recipe gap is
  [ART-251](ISSUES.md) (no rule for `Utilities` or `WBStartup`).
- **First boot on the Amiga** — phases 1 and 2 built (dispatcher, the
  `10-hardware`/`20-aux`/`30-datatypes` steps, the `S/FirstBoot.log` report,
  reading that report back off a card's FAT, FFS or PFS3), and phase 3 — the
  wizard and the reboot — merged 2026-09-16 and awaiting the owner's
  hand-answered rehearsal. Phase 4 (packages) is not built and is narrowed to
  `boingbags-39-3-4` and `euro-update` (2026-09-16); the Pi3/Pi4 branch of `10-hardware` is unmeasured because it needs
  a real card.
- **Aminet and the package catalogue** — Stage A and Stage B are both built
  and tested, catalogue sync through install-to-HDF and the update view. The
  AI layer (§45.5) is deliberately deferred to v2, the owner's decision of
  2026-08-24.
- **The Emu68 Hatcher intake** (`rootrootde/emu68hatcher`, MIT) — scoped at six
  rounds of taking what is worth taking. Five have landed on `main`: prefs and
  wallpaper (1), drawer icons (2), refusal evidence (3), media identification
  by hash (4), and first boot phases 1–3 (5). Round 5's own phase 4 waits, narrowed;
  the one-button card's round 2 is the next code work.
- **The 2026-09-04 work list**
  ([superpowers/specs/2026-09-04-work-list.md](superpowers/specs/2026-09-04-work-list.md))
  — items 3, 5 and 7 are closed. **Every item left on it is the owner's**:
  items 1 and 4 need a person, item 2 closed with ART-118, (a card, a screen driven by hand, a real
  catalogue looked at) and item 6 is blocked on the owner bringing a real
  distribution image.
- **v0.9.0 is out and in the community's hands.** Reading what came back is
  still the first thing a session does — issues on the repository, and whatever
  the owner was told directly. A report that went *well* counts: the ten gaps
  the README lists are unverified in both directions. Nothing had come back as
  of 2026-09-05: no issues, three release downloads.

### How a release goes out now

Recorded because it did not exist before 2026-09-04 and nobody should have to
work it out twice. Raise the version in **all three** files
(`package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`), close
the CHANGELOG's `[Unreleased]` section under the new number, push, **wait for
CI to go green on that commit**, then push a `vX.Y.Z` tag.
`.github/workflows/release.yml` does the rest and refuses rather than shipping
something wrong: it stops if the tag and the three version fields disagree,
and it stops if the CHANGELOG has no section for that version. It publishes a
**draft**, so a person presses the last button — and before pressing it, launch
the built `.exe` once. "It compiled" is not the same claim as "it opens".

### Blocked on a person, not on code

1. **A card flashed and an A500 booted.** Nothing SD-1 or SD-2 has built has
   reached real hardware. The software side is complete; what is missing is
   physical — a microSD card, a USB reader and an HDMI cable, the last plugged
   in **before** power or the VPU never configures the port and there is no
   RTG that session.

It needs no design decision or more code first.

---

## What ART is, and the ceiling that comes with it

**Every established Amiga distribution builder runs the install inside an
emulator** — HstWB Installer, AmiKit, AmigaSYS and ClassicWB all do, using the
user's own OS files. ART is not in that family: `amitools`'s `xdftool` does
what ART does — host-side placement plus a metadata sidecar (`.xdfmeta`, ART's
`.uaem`) — so ART sits in the **tooling** family. That has a ceiling, and it
is a live constraint rather than a defect:

- **[ART-166](ISSUES.md)** (**fixed 2026-09-08**) — this paragraph used to
  say a BoingBag's payload is a password-encrypted ZIP whose password lives in
  an Amiga executable, so a host-side placer cannot read it, and that the
  owner had ruled out writing a bypass. The owner reversed that ruling on
  2026-09-08, and the ceiling was lower than it looked: **Emu68 Hatcher** and
  **Emu68-Imager** (both MIT) publish the key, and the round-3 hashed
  snapshots show the `Updater` simply copies its payload onto the tree — of
  BoingBag 3.9-1's 210 payload files it wrote 166 and the other 44 were
  already byte-identical. ART now places both BoingBags from Windows, and the
  result hashes to the Updater's own tree (4 030 files; the only differences
  are one step ART deliberately does more of, and ART's own manifest).
- **The Amiga side is no longer hypothetical.** `core/amigainstall` runs a
  package's own installer under WinUAE, and `core/firstboot` runs a tree's own
  setup on the real machine at first boot. What that path can and cannot do
  yet is the FEATURES rows for those two, not this paragraph.

Two rules came out of the same round and outlive it, both now in CLAUDE.md:
**ask the artefact what it is** — a booted system answers `version full`,
where a copyright line, a directory name and a file size are each consistent
with several answers (this is how a 3.5 tree was recorded as 3.9 for most of a
day, [ART-169](ISSUES.md#fixed)) — and **a file with no `$VER:` marker is
invisible to a collision classifier**, so any "did the update take?" check
built on collision classes alone will be confidently wrong about the one file
that matters ([ART-170](ISSUES.md#fixed)).

---

## Stage plan

This supersedes the phase numbering in `roadmap.md` for scheduling.
`roadmap.md` still defines *what each phase contains*; this defines *what order
the remaining work happens in and why*.

### Closed stages

Each of these is finished. The detail is the [session log](session-log.md) row
for its date and the FEATURES rows it produced; nothing about them is retold
here.

| Stage | Status | Closed | Where the detail lives |
|---|---|---|---|
| **Stage 1** — Data safety (`core/safety`, atomic writes, generational backups, bounds-checked blocks, the AmigaDOS hash fix) | ✅ | 2026-08-09 | [session log](session-log.md) · [ART-021…ART-029](ISSUES.md#fixed) |
| **Stage 2** — Workflow Engine (`core/workflow/builtin.rs`, `Navigate`/`Execute`, the drop panel) | ✅ | 2026-08-09 | [session log](session-log.md) · [architecture.md](architecture.md) |
| **Stage 3** — Systematic audit (HDF/RDB held nine defects, three critical; sparse HDF creation) | ✅ | 2026-08-09 | [ISSUES.md](ISSUES.md#fixed) |
| **Stage 4** — Shared infrastructure (jobs, oplog, UX modes — the hard prerequisites Stage 5 names) | ✅ | 2026-08-09 | [session log](session-log.md) |
| **Stage W** — Writing into volumes (`core/volume/write/`: OFS, FFS, INTL at any geometry) | ✅ | 2026-08-09 | [FEATURES.md](FEATURES.md) |
| **§82** — One-click WHDLoad install, end to end | ✅ | 2026-08-09 | [FEATURES.md](FEATURES.md) |
| **Phase 0a** — Live bugs fixed, one filesystem writer | ✅ | 2026-08-10 | [session log](session-log.md) |
| **Phase 0b** — Dead code removed, the interface speaks Turkish | ✅ | 2026-08-10 | [session log](session-log.md) |
| **Phase 1a** — The Commander (real selection, batch operations, sorting, filter) | ✅ | 2026-08-11 | [session log](session-log.md) |
| **Phase 2a** — Content-first detection and containers | ✅ | 2026-08-12 | [plan](superpowers/plans/2026-08-11-phase-2a-content-detection-and-optical.md) |
| **Phase 2b** — The Files screen, done right | ✅ | 2026-08-12 | [plan](superpowers/plans/2026-08-12-phase-2b-commander-ui.md) · [brief](brief-files-commander-ui.md) |
| **Stage 5 / §41.5** — Aminet: catalog sync, search, download, readme, Collection, install to HDF, update view, and the 14-set package-bundle catalogue | ✅ | 2026-08-09, extended to 2026-08-24 | [FEATURES.md](FEATURES.md) · [design-software-sources.md](design-software-sources.md). Bundles are download-only in phase 1 — nothing installs onto an Amiga volume yet; `net/live_aminet.rs` (2026-08-24) is the re-runnable check against the real mirrors |
| **Stage 5 / §45.5** — the AI workflow layer | 🅥2 | deferred 2026-08-24 | [design-ai-layer.md](design-ai-layer.md) — the owner's decision: the only planned feature that adds no capability ART lacks, so it waits until the ground under it has been driven |

Two caveats from Phase 2b that nobody has closed since: **the light theme has
never been looked at** (its tokens derive from the dark ones by role), and
**session restore is only half verified** — the write half reaches
`settings.json` and has tests (`src/lib/paneSession.ts`), but nothing has
closed and reopened ART to watch the read half. Both sit under
[ART-062](ISSUES.md).

### 🟡 PiStorm Image Builder — SD-0 … SD-5

**The project's largest feature and, per the owner, its point.** SD-0, SD-1,
SD-2 and SD-4 are built. SD-3's two named gaps are both built and merged —
G14 (prefs, wallpaper, screen mode, shell defaults, `core/amiganet/`'s network
half) and G16 (multiboot) — and SD-5 is part-built. Only the gaps still open
are listed below; a built gap is a [FEATURES.md](FEATURES.md) row, not a line
here.

Gap analysis: [sd-appliance-gap-analysis.md](sd-appliance-gap-analysis.md).
Prior-art teardown: [sd0-prior-art.md](sd0-prior-art.md). Card layout measured
off two real cards: [sd2-card-layout.md](sd2-card-layout.md).

| Gap | Stage | What is still missing (checked against the tree 2026-09-07) |
|---|---|---|
| **SD-1 exit** | SD-1 | Not code. Every gap in SD-1 is a ✅ row in [FEATURES.md](FEATURES.md); what is left is a card ART built, flashed, and an A500 booted from it. This is the 1.0 bar |
| **G10** | SD-2 | Built and closed 2026-09-05 (`core/gameindex/igame.rs`, `igamewrite.rs`). The one outstanding claim: **no `igame.data` ART wrote has been read by iGame on a real Amiga.** The AGS half was assessed 2026-08-25 and the recommendation is not to build it — `core/artwork` handles PNG and JPEG and not IFF, so the launcher would show a collection with no pictures |
| **G11** | SD-2 | Built (`core/layout/`, the `/layout` screen). **Not yet driven in `pnpm tauri dev` against real material, and no staging tree it built has been carried onto a card** |
| **SD-5 planner** | SD-5 | All five entries in the distro registry are `available: false` (`grep -c '"available": false' src-tauri/src/core/distro/*.json` → 5), so nothing yet plans a card from a named distribution. The capacity half is built — `core/card/capacity.rs` refuses an FFS partition past the 4 GB a pre-v46 Kickstart can address, which is a corrupted drive rather than an inconvenience |

Four decisions behind it, worth knowing before reading the code they produced:

- **ART never touches physical media.** Everything is built into a sparse image
  file through the existing tested paths, and there the job ends: the user
  flashes it with the imager they already have. §56's raw-device guard stays in
  the spec as the reason ART does not do this, rather than as a problem ART has
  to solve. This deleted the original G1 and G12 outright.
- **v1 targets Emu68 only.** The classic Linux/Musashi route is a different
  build and is out of scope in writing, not by omission.
- **Which Amiga is a parameter, not an assumption.** What varies by machine is
  data the build already carries — the Kickstart, the Emu68 config, the OS
  release, the partition geometry — not a code path. The hardware in the room
  is a classic PiStorm on a Pi 3A+, verified on an A500 and an A500+.
- **A distribution is configured in ART, not afterwards on the Amiga.**
  Wallpaper, WiFi, prefs and Startup-Sequence are build inputs (G14), each
  **edited in place rather than regenerated** — the rule §39/§40 already impose
  on `FF.CFG` and `cmdline.txt`, applied to AmigaOS. The WiFi passphrase is
  deliberately not remembered and stays out of the oplog, the manifest and any
  AI prompt.

---

## Phase completion criteria

Carried over from `roadmap.md`; a stage is not done until all of these hold.

- Build: PASS
- Tests: PASS
- No critical errors
- No obvious data-loss risk
- UI remains responsive
- Documentation updated (this file, [ISSUES.md](ISSUES.md), [FEATURES.md](FEATURES.md))
- CHANGELOG updated

---

## The machine, and where things live

- `E:\amiga\Amigatolon` — the owner's own material. **Read from, never written
  to.** Inventoried 2026-08-13: A1200 Kickstarts in both families (3.1 rev
  40.68, 3.2's `kicka1200.rom`, 3.2.1's `A1200.47.102.rom`), the full AmigaOS
  3.2 ADF set with its CD and the 3.2.1/3.2.2 updates, the 3.9 ISO with both
  BoingBags, every release from 1.0 to 3.1 as ADFs, and both real PiStorm
  distributions (CaffeineOS 9317, MultibootOS 2.2) as images. WinUAE, 7-Zip
  and amitools are installed.
- `E:\amiga\ProjeART` — where trial output goes. **Not C:, not D:** (the
  owner's rule, 2026-08-14), and not F: either, which is a 499 MB drive a
  300 MB oracle image will not fit on. `ART_SCRATCH` overrides it in the
  scripts. It holds `card.img` (a card ART built from the real release),
  `dist-3.2\` (the real AmigaOS 3.2 tree — read from, not rebuilt idly, since
  rebuilding costs the same real-media run), and `screen-test.img` +
  `preload-tree\`, staged for a screen run. That card's one Amiga partition is
  `SDH0`, FFS, 0.90 GiB, in MBR slot 2 — so a correct preload plan has **two**
  steps and no driver-embed step, because Kickstart carries FFS. If the plan
  shows three, something is wrong before anything is formatted.
- The test suite's own scratch goes to `E:/amiga/ProjeART/build/tmp`, forced
  by the machine-local, gitignored `src-tauri/.cargo/config.toml`
  ([ART-184](ISSUES.md#fixed), [ART-320](ISSUES.md#fixed)); CI never has that
  file (or an `E:` drive) at all, so `cargo test` there sets both `value` and
  `force` itself on the command line. The old `D:\tmp\art-tests` (≈1.49 GiB,
  5 594 items) is left in place, not deleted — that is a separate decision.
- Rebuilding the card is the fastest way to see the whole engine work at once:

  ```bash
  cd src-tauri
  ART_CARD_ZIP="E:\amiga\Amigatolon\Emu68\Emu68-pistorm.zip" \
  ART_CARD_ROM="E:\amiga\Amigatolon\kickstart\Kickstart v3.1 rev 40.68 (1993)(Commodore)(A1200).rom" \
  ART_CARD_OUT="E:\amiga\ProjeART\card.img" \
    cargo test build_real_card_when_asked -- --nocapture
  ```

  It prints the payload it chose, where each partition landed, and reads the
  finished card back. Delete `card.img` first — `build_card` refuses to write
  over one that is already there.

## What the owner brings, and what each thing settles

Each of these changed a decision rather than merely being nice to know.

- **The target board is settled by hardware, not preference**: an A500 with a
  classic PiStorm on a Raspberry Pi 3A+, plus a Gotek — which is
  `PistormHardware::default()` already. Consequences that are facts rather
  than parameters: the kernel archive is `Emu68-pistorm.zip` on the stable
  line (**not** the same file as on the 1.1 alpha — [ART-091](ISSUES.md#fixed)),
  the SD driver is `brcm-sdhc.device`, and `genet.device` is irrelevant (the
  3A+ has no ethernet; its WiFi is `wifipi.device`).
- **More than one real Amiga.** The rule is unchanged — a claim records
  *which* machine proved it — but there is more than one machine to prove
  things on.
- **Licensed Amiga Forever, desktop and mobile.** There is no Kickstart
  licence problem to design around, and the Cloanto-headered dumps are
  available — which `core/rom` already strips (`AMIROMTYPE1`). It is also why
  [ART-104](ISSUES.md#fixed) mattered: a licensed collection carries dumps
  whose checksums are not the ones a table copied from anywhere else will hold.
- **The owner wrote two Amiga-side programs and both are meant to ship in
  ART-built distributions**: **tolunnet**, a TCP/IP stack in Roadshow's place,
  and **tolunwifi**. Source at `D:\Projeler\tolunnet`. A stack the owner
  wrote is theirs to distribute, so ART Baseline can carry one outright rather
  than as a fetch task — which is what SD-0 concluded for anything not clearly
  redistributable. G14's network half is **built** against that program's own
  source (`core/amiganet/`), so nothing was reverse-engineered and Roadshow is
  not assumed as the default stack.

---

## Session log

Moved to [session-log.md](session-log.md) on 2026-09-04, and it is history: a
row describes the tree on the day it was written and is never rewritten.

**Add a row there, at the top, when work lands.** Nothing is duplicated back
here on purpose — two copies of a log is how this file's own "Last updated"
cell once ended up a day behind it.
