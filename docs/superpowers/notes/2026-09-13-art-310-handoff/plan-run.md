# ART-310 plan-run log

## Task 2 red

Command:
```
bash ~/Belgeler/Projeler/art-experiments/art-linux-run.sh bash -c 'cd src-tauri && cargo test --lib -- a_small_pfs3_format_reserves_anodes_zero_to_four a_large_pfs3_format_writes_the_superblock_level a_large_pfs3_volume_takes_its_own_writes'
```

Result: `test result: FAILED. 0 passed; 3 failed; 0 ignored; 0 measured; 3307 filtered out; finished in 0.04s`

Panic messages, verbatim:

```
---- core::preload::native::tests::a_small_pfs3_format_reserves_anodes_zero_to_four stdout ----

thread 'core::preload::native::tests::a_small_pfs3_format_reserves_anodes_zero_to_four' (76406) panicked at src/core/preload/native.rs:1523:13:
assertion `left == right` failed: anode 0 must be reserved the way pfs3aio's AllocAnode leaves it
  left: (0, 0, 0)
 right: (0, 4294967295, 0)
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- core::preload::native::tests::a_large_pfs3_format_writes_the_superblock_level stdout ----

thread 'core::preload::native::tests::a_large_pfs3_format_writes_the_superblock_level' (76404) panicked at src/core/preload/native.rs:1542:9:
assertion `left == right` failed: superindex[0] must name a super index block, not the anode index block
  left: [[73, 66], [65, 66]]
 right: [[83, 66], [73, 66], [65, 66]]

---- core::preload::native::tests::a_large_pfs3_volume_takes_its_own_writes stdout ----

thread 'core::preload::native::tests::a_large_pfs3_volume_takes_its_own_writes' (76405) panicked at src/core/preload/native.rs:1569:14:
called `Result::unwrap()` on an `Err` value: Malformed { format: "pfs3", detail: "anode 5 not found" }
```

All three failed for exactly the reason the brief predicted: `anode 0` left `(0,0,0)`; the chain read `IB AB` (bytes 73,66=`IB`, 65,66=`AB`) instead of `SB IB AB`; and `copy_in` failed with `anode 5 not found`.

## Task 2 green

After applying `art-310-format-patch.py` to `src-tauri/vendor/libpfs3/src/format.rs`:

Command: `bash ~/Belgeler/Projeler/art-experiments/art-linux-run.sh bash -c 'cd src-tauri && cargo test --lib core::preload::native'`
Result: `test result: ok. 40 passed; 0 failed; 1 ignored; 0 measured; 3269 filtered out; finished in 0.70s`

The two large-mode tests (`a_large_pfs3_format_writes_the_superblock_level`, `a_large_pfs3_volume_takes_its_own_writes`) are both included in this 0.70 s total — well under the 60 s stop rule.

Command: `bash ~/Belgeler/Projeler/art-experiments/art-linux-run.sh bash -c 'cd src-tauri && cargo test --lib core::card::sizing'`
Result: `test result: ok. 24 passed; 0 failed; 0 measured; 3286 filtered out; finished in 11.17s` — same counts as Task 1, Step 8.

## Task 2 mutations

Committed the patch as `f52bf5e` first, then mutated `src-tauri/vendor/libpfs3/src/format.rs` (and, for M4,
`src-tauri/Cargo.toml`) one at a time, ran the Step 3 command, and restored with `git checkout --`.

Two of the four mutations (M2, M3) as literally described leave the pattern binding `sb_blk` in
`if let Some(sb_blk) = sb_blk { ... }` unused once the one named line is changed, and `libpfs3/src/lib.rs`
has `#![deny(warnings)]`, so the crate fails to *compile* rather than producing the test failure the table
predicts. This is incidental to Rust's shadowing/unused-variable lint, not to the semantic property the
mutation is meant to falsify (the pointer target for M2, the missing write for M3), so for those two only,
the pattern was additionally renamed to `_sb_blk` on that same line for the duration of the mutation, then
reverted along with everything else. M1 and M4 needed no such adjustment.

| # | Mutation | Failed | Message | Restored cleanly |
|---|---|---|---|---|
| M1 | `for nr in 0..ANODE_ROOTDIR as usize` → `for nr in 1..ANODE_ROOTDIR as usize` | `a_small_pfs3_format_reserves_anodes_zero_to_four`, `a_large_pfs3_format_writes_the_superblock_level` (as predicted); `a_large_pfs3_volume_takes_its_own_writes` passed (anode 0 being free does not affect this test's path) | small: `assertion `left == right` failed: anode 0 must be reserved the way pfs3aio's AllocAnode leaves it / left: (0, 0, 0) / right: (0, 4294967295, 0)`; large-format: `assertion `left == right` failed: anode 0 must be reserved in SUPERINDEX mode too / left: (0, 0, 0) / right: (0, 4294967295, 0)` | yes |
| M2 | `put_u32(&mut rext, 0x40, sb_blk);` → `put_u32(&mut rext, 0x40, anidx_blk);` (plus the incidental `_sb_blk` rename noted above) | `a_large_pfs3_format_writes_the_superblock_level`, `a_large_pfs3_volume_takes_its_own_writes` (as predicted); small test passed | large-format: `assertion `left == right` failed: superindex[0] must name a super index block, not the anode index block / left: [[73, 66], [65, 66]] / right: [[83, 66], [73, 66], [65, 66]]`; writes: `Malformed { format: "pfs3", detail: "anode 5 not found" }` | yes |
| M3 | delete `write_reserved_blocks(dev, sb_blk as u64, &sb, rescluster, bs)?;` (plus the incidental `_sb_blk` rename noted above) | `a_large_pfs3_format_writes_the_superblock_level`, `a_large_pfs3_volume_takes_its_own_writes` (as predicted); small test passed | large-format: `the anode pointer chain reached "\0\0" after ["\0\0"]` (the SB block was never written, so its slot on disk is all zero); writes: `Malformed { format: "pfs3", detail: "anode 5 not found" }` | yes |
| M4 | deleted `[patch.crates-io]` / `libpfs3 = { path = "vendor/libpfs3" }` from `src-tauri/Cargo.toml` | all four: the three ART-310 tests and `the_pinned_version_constant_matches_cargo_toml` (as predicted) | pinned-version: `Cargo.toml must patch libpfs3 to the vendored copy (ART-310) — without it the build silently goes back to 0.1.3's broken format`; the three ART-310 tests failed with the same red messages as Task 2 Step 3 | yes (`git checkout -- src-tauri/Cargo.toml src-tauri/Cargo.lock`) |

Every "Must fail" test failed for every mutation; no survivors. `git status --short` is empty after the
last restore.

## Task 3 oracle, fix

Step 2, hooks build and stay silent without their env:

```
bash ~/Belgeler/Projeler/art-experiments/art-linux-run.sh bash -c 'cd src-tauri && cargo test --lib core::preload::native'
```

Result: `test result: ok. 42 passed; 0 failed; 1 ignored; 0 measured; 3269 filtered out; finished in 0.69s`
— matches the brief's expectation exactly.

Step 4, the oracle on the fix:

```
mkdir -p /home/tolon/Belgeler/Projeler/art-experiments/tmp/oracle
bash ~/Belgeler/Projeler/art-experiments/art-linux-run.sh env \
  ART_HST_IMAGER=$HOME/.local/share/art-tools/hst-imager-1.6.616/hst.imager \
  ART_PFS3_DRIVER=$HOME/.local/share/art-tools/pfs3aio-3.1/pfs3aio \
  ART_SCRATCH=/home/tolon/Belgeler/Projeler/art-experiments/tmp/oracle \
  python3 scripts/pfs3-oracle-check.py
```

Full output:

```
hst-imager: /home/tolon/.local/share/art-tools/hst-imager-1.6.616/hst.imager
pfs3 driver: /home/tolon/.local/share/art-tools/pfs3aio-3.1/pfs3aio
scratch: /home/tolon/Belgeler/Projeler/art-experiments/tmp/oracle

ART writes, hst-imager reads:
  art-write.hdf: 230178816 bytes long, 73728 bytes on disk
  ok   ART wrote a PFS3 volume
  ok   Readme is a file
  ok   Readme is 15 bytes (hst-imager says 15)
  ok   Readme carries ----RWED (hst-imager says ----RWED)
  ok   C is a dir
  ok   C carries ----RWED (hst-imager says ----RWED)
  ok   C/Assign is a file
  ok   C/Assign is 7 bytes (hst-imager says 7)
  ok   C/Assign carries --P-RWED (hst-imager says --P-RWED)
  ok   C/Startup-Sequence is a file
  ok   C/Startup-Sequence is 12 bytes (hst-imager says 12)
  ok   C/Startup-Sequence carries -S--RWED (hst-imager says -S--RWED)
  ok   C/Extra is a dir
  ok   C/Extra carries ----RWED (hst-imager says ----RWED)
  ok   C/Extra/Deep.txt is a file
  ok   C/Extra/Deep.txt is 5 bytes (hst-imager says 5)
  ok   C/Extra/Deep.txt carries ----RWED (hst-imager says ----RWED)
  ok   S is a dir
  ok   S carries ----RWED (hst-imager says ----RWED)
  ok   DOSDrivers is a dir
  ok   DOSDrivers carries ----RWED (hst-imager says ----RWED)
  ok   DOSDrivers/AUX is a file
  ok   DOSDrivers/AUX is 14 bytes (hst-imager says 14)
  ok   DOSDrivers/AUX carries ----RWED (hst-imager says ----RWED)
  ok   nothing extra is on the volume
  ok   hst-imager extracted the volume ART wrote back to disk
  ok   Readme's extracted bytes hash to what NativeFormatter was given
  ok   C/Assign's extracted bytes hash to what NativeFormatter was given
  ok   C/Startup-Sequence's extracted bytes hash to what NativeFormatter was given
  ok   C/Extra/Deep.txt's extracted bytes hash to what NativeFormatter was given
  ok   hst-imager made a directory on the volume ART formatted
  ok   hst-imager's new directory has an anode pfs3aio does not reserve (anode 15; 0-4 are reserved, ART-310)

ART writes past MAXSMALLDISK (SUPERINDEX mode), hst-imager reads:
  art-write-large.hdf: 5452554240 bytes long, 1335296 bytes on disk
  ok   ART wrote a PFS3 volume
  ok   Readme is a file
  ok   Readme is 15 bytes (hst-imager says 15)
  ok   Readme carries ----RWED (hst-imager says ----RWED)
  ok   C is a dir
  ok   C carries ----RWED (hst-imager says ----RWED)
  ok   C/Assign is a file
  ok   C/Assign is 7 bytes (hst-imager says 7)
  ok   C/Assign carries --P-RWED (hst-imager says --P-RWED)
  ok   C/Startup-Sequence is a file
  ok   C/Startup-Sequence is 12 bytes (hst-imager says 12)
  ok   C/Startup-Sequence carries -S--RWED (hst-imager says -S--RWED)
  ok   C/Extra is a dir
  ok   C/Extra carries ----RWED (hst-imager says ----RWED)
  ok   C/Extra/Deep.txt is a file
  ok   C/Extra/Deep.txt is 5 bytes (hst-imager says 5)
  ok   C/Extra/Deep.txt carries ----RWED (hst-imager says ----RWED)
  ok   S is a dir
  ok   S carries ----RWED (hst-imager says ----RWED)
  ok   DOSDrivers is a dir
  ok   DOSDrivers carries ----RWED (hst-imager says ----RWED)
  ok   DOSDrivers/AUX is a file
  ok   DOSDrivers/AUX is 14 bytes (hst-imager says 14)
  ok   DOSDrivers/AUX carries ----RWED (hst-imager says ----RWED)
  ok   nothing extra is on the volume
  ok   hst-imager extracted the volume ART wrote back to disk
  ok   Readme's extracted bytes hash to what NativeFormatter was given
  ok   C/Assign's extracted bytes hash to what NativeFormatter was given
  ok   C/Startup-Sequence's extracted bytes hash to what NativeFormatter was given
  ok   C/Extra/Deep.txt's extracted bytes hash to what NativeFormatter was given
  ok   hst-imager made a directory on the volume ART formatted
  ok   hst-imager's new directory has an anode pfs3aio does not reserve (anode 15; 0-4 are reserved, ART-310)

hst-imager writes, ART reads:
  ok   hst-imager built and filled a PFS3 volume
  ok   ART read the volume hst-imager wrote
  ok   the volume name reads back (Workbench)
  ok   Readme is a file
  ok   Readme is 34 bytes (ART says 34)
  ok   Readme hashes to what hst-imager was given
  ok   Readme carries ----RWED (ART says ----RWED)
  ok   Devs is a directory
  ok   Devs carries ----RWED (ART says ----RWED)
  ok   Devs/Assign is a file
  ok   Devs/Assign is 38 bytes (ART says 38)
  ok   Devs/Assign hashes to what hst-imager was given
  ok   Devs/Assign carries --P-RWED (ART says --P-RWED)
  ok   nothing extra is on the volume

2 file(s) skipped — Windows/MS-DOS reserved device name(s) (CON, PRN, AUX, NUL, COM1-9, LPT1-9, with or without an extension). `hst-imager fs copy` cannot extract these to an NTFS path on this OS; that is a Windows limitation in the oracle's own tool, not a defect in the volume ART wrote (ART-114):
  - DOSDrivers/AUX
  - DOSDrivers/AUX

ART and hst-imager agree, both directions — names, sizes, bytes, and protection bits.
(2 file(s) with Windows-reserved names skipped for extraction, see above — not a failure.)
```

Both anode lines name anode 15, above 4. `art-write-large.hdf` is 5 452 554 240 bytes long — matches the
brief's expected value for 10 565 whole cylinders of the 5 200 MiB asked. Exit code 0.

## Task 3 oracle, 0.1.3

Deleted `[patch.crates-io]` / `libpfs3 = { path = "vendor/libpfs3" }` from `src-tauri/Cargo.toml`. A plain
`cargo check --lib` through the gate script picked up crates.io `libpfs3 v0.1.3` on its own (`Locking 1
package to latest compatible version / Adding libpfs3 v0.1.3`); no manual `cargo update -p libpfs3
--precise 0.1.3` was needed.

Ran Step 4's command again. Full output (exit code 1):

```
hst-imager: /home/tolon/.local/share/art-tools/hst-imager-1.6.616/hst.imager
pfs3 driver: /home/tolon/.local/share/art-tools/pfs3aio-3.1/pfs3aio
scratch: /home/tolon/Belgeler/Projeler/art-experiments/tmp/oracle

ART writes, hst-imager reads:
  art-write.hdf: 230178816 bytes long, 73728 bytes on disk
[14:45:26 INF] Hst Imager v1.6.616 (05/26/2026 21:36:05)
[14:45:26 INF] Henrik Nørfjand Stengaard
[14:45:26 INF] [CMD] fs mkdir art-write.hdf/rdb/dh0/HstMade
[14:45:26 INF] Creating directory path: 'art-write.hdf/rdb/dh0/HstMade'
[14:45:26 ERR] Failed to execute command 'Hst.Imager.Core.Commands.FsCommands.FsMkDirCommand'
System.IO.IOException: ERROR_DISK_FULL
   at Hst.Amiga.FileSystems.Pfs3.Directory.NewDir(objectinfo parent, String dirname, globaldata g)
   at Hst.Amiga.FileSystems.Pfs3.Pfs3Volume.CreateDirectory(String dirName)
   at Hst.Imager.Core.Commands.FsCommands.FsMkDirCommand.CreateRdbDirectory(Media media, String[] parts)
   at Hst.Imager.Core.Commands.FsCommands.FsMkDirCommand.CreateRdbDirectory(Media media, String[] parts)
   at Hst.Imager.Core.Commands.FsCommands.FsMkDirCommand.CreateDiskMediaDirectory(MediaResult resolvedMedia)
   at Hst.Imager.Core.Commands.FsCommands.FsMkDirCommand.Execute(CancellationToken token)
   at Hst.Imager.ConsoleApp.CommandHandler.Execute(CommandBase command)


  ok   ART wrote a PFS3 volume
  ok   Readme is a file
  ok   Readme is 15 bytes (hst-imager says 15)
  ok   Readme carries ----RWED (hst-imager says ----RWED)
  ok   C is a dir
  ok   C carries ----RWED (hst-imager says ----RWED)
  ok   C/Assign is a file
  ok   C/Assign is 7 bytes (hst-imager says 7)
  ok   C/Assign carries --P-RWED (hst-imager says --P-RWED)
  ok   C/Startup-Sequence is a file
  ok   C/Startup-Sequence is 12 bytes (hst-imager says 12)
  ok   C/Startup-Sequence carries -S--RWED (hst-imager says -S--RWED)
  ok   C/Extra is a dir
  ok   C/Extra carries ----RWED (hst-imager says ----RWED)
  ok   C/Extra/Deep.txt is a file
  ok   C/Extra/Deep.txt is 5 bytes (hst-imager says 5)
  ok   C/Extra/Deep.txt carries ----RWED (hst-imager says ----RWED)
  ok   S is a dir
  ok   S carries ----RWED (hst-imager says ----RWED)
  ok   DOSDrivers is a dir
  ok   DOSDrivers carries ----RWED (hst-imager says ----RWED)
  ok   DOSDrivers/AUX is a file
  ok   DOSDrivers/AUX is 14 bytes (hst-imager says 14)
  ok   DOSDrivers/AUX carries ----RWED (hst-imager says ----RWED)
  ok   nothing extra is on the volume
  ok   hst-imager extracted the volume ART wrote back to disk
  ok   Readme's extracted bytes hash to what NativeFormatter was given
  ok   C/Assign's extracted bytes hash to what NativeFormatter was given
  ok   C/Startup-Sequence's extracted bytes hash to what NativeFormatter was given
  ok   C/Extra/Deep.txt's extracted bytes hash to what NativeFormatter was given
  FAIL hst-imager made a directory on the volume ART formatted

ART writes past MAXSMALLDISK (SUPERINDEX mode), hst-imager reads:

running 1 test
core::preload::native::tests::build_large_pfs3_volume_for_oracle_when_asked --- FAILED

failures:

failures:
    core::preload::native::tests::build_large_pfs3_volume_for_oracle_when_asked

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 3311 filtered out; finished in 0.03s



thread 'core::preload::native::tests::build_large_pfs3_volume_for_oracle_when_asked' (83352) panicked at src/core/preload/native.rs:2371:14:
called `Result::unwrap()` on an `Err` value: Malformed { format: "pfs3", detail: "anode 5 not found" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
error: test failed, to rerun pass `--lib`

  FAIL ART wrote a PFS3 volume

hst-imager writes, ART reads:
  ok   hst-imager built and filled a PFS3 volume
  ok   ART read the volume hst-imager wrote
  ok   the volume name reads back (Workbench)
  ok   Readme is a file
  ok   Readme is 34 bytes (ART says 34)
  ok   Readme hashes to what hst-imager was given
  ok   Readme carries ----RWED (ART says ----RWED)
  ok   Devs is a directory
  ok   Devs carries ----RWED (ART says ----RWED)
  ok   Devs/Assign is a file
  ok   Devs/Assign is 38 bytes (ART says 38)
  ok   Devs/Assign hashes to what hst-imager was given
  ok   Devs/Assign carries --P-RWED (ART says --P-RWED)
  ok   nothing extra is on the volume

1 file(s) skipped — Windows/MS-DOS reserved device name(s) (CON, PRN, AUX, NUL, COM1-9, LPT1-9, with or without an extension). `hst-imager fs copy` cannot extract these to an NTFS path on this OS; that is a Windows limitation in the oracle's own tool, not a defect in the volume ART wrote (ART-114):
  - DOSDrivers/AUX

2 check(s) failed:
  - hst-imager made a directory on the volume ART formatted
  - ART wrote a PFS3 volume

ART and hst-imager disagree about a PFS3 volume. That means the volume is wrong, not the reader — libpfs3 backs both ART's writer and its reader, so only an outside implementation like hst-imager can tell the two apart.
```

Small section: matches the brief's expected "either fails, or gets an anode ≤ 4" — hst-imager's `fs mkdir`
itself failed with `ERROR_DISK_FULL` (its allocator, a port of pfs3aio's, cannot find a free anode on a
volume where 0-4 are not reserved), so the check that fails is "hst-imager made a directory on the volume
ART formatted", not the anode check itself (the script returns before reaching it, by design, when `mkdir`
fails).

Large section: did **not** fail the way the brief predicted (hst-imager's listing raising a
`NullReferenceException`). Instead it failed one step earlier: `write_pfs3_volume_for_oracle`'s own
`copy_in` call, running against unpatched 0.1.3, panicked with `Malformed { format: "pfs3", detail: "anode
5 not found" }` while ART was still writing the volume — before hst-imager ever touched it. This is the
same panic message Task 2's mutation table recorded for M2 and M3 (`plan-run.md`'s "Task 2 mutations"
section above) on the same large-format path, so it is a real, reproducible consequence of the missing SB
level in SUPERINDEX mode, just caught one step earlier than the brief anticipated (by ART's own writer,
not by hst-imager's reader). Recorded as observed, per instructions, rather than changed to force the
predicted `NullReferenceException`.

Restored with `git checkout -- src-tauri/Cargo.toml src-tauri/Cargo.lock`. `git status --short` afterward
showed only `scripts/pfs3-oracle-check.py` and `src-tauri/src/core/preload/native.rs` modified — this
task's two files.

## Task 4 Windows

The owner's Windows machine, 2026-09-13, on `art-310-libpfs3-format` at `e58abbb` with a clean tree (the
branch fetched from `origin`; the machine's own older ART-310 branch was renamed `art-310-windows` first).
From `src-tauri`, `TMP`/`TEMP` = `E:\amiga\ProjeART\build\tmp`, each step's output in
`E:\amiga\ProjeART\build\tmp\art310-task4\`:

| Step | Exit | Line |
|---|---|---|
| `cargo fmt --check` | 0 | — |
| `cargo clippy --all-targets -- -D warnings` | 0 | — |
| `cargo test --lib` (1) | 0 | `test result: ok. 3257 passed; 0 failed; 58 ignored; 0 measured; 0 filtered out; finished in 41.70s` |
| `cargo test --lib` (2) | 0 | `test result: ok. 3257 passed; 0 failed; 58 ignored; 0 measured; 0 filtered out; finished in 37.14s` |
| `cargo deny check` | 0 | `advisories ok, bans ok, licenses ok, sources ok` |

3257 is the plan's prediction (3252 + 5). The Linux-only clippy errors of Ruling 5 did not appear here.
