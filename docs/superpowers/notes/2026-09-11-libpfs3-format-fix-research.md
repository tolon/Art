# `libpfs3`'s format, and what route A has to change (research, 2026-09-13)

Why this exists: [ART-310](../../ISSUES.md) found that `libpfs3` 0.1.3's FORMAT writes two structures
wrong, and on 2026-09-11 the owner chose route A — *patch `libpfs3`'s format (write the `SB` level, mark
anodes 0–4) and vendor the patched crate into ART, offering the patch upstream*. ART-310 names this file
as the research that comes before the design. The file name keeps the date ART-310 gave it; the work was
done on 2026-09-13. It describes the sources **on the day they were read**; re-check before building on
it.

It does four things: re-runs ART-310's experiment independently (a different machine, OS, and pfs3aio
binary), with the two fixes as separate one-variable arms; checks both fixes against pfs3aio's own
source; checks whether anything upstream has moved; and records what vendoring would actually meet in
this repository. It is not the design.

## 0. Sources, as read on 2026-09-13

| Source | Version read | How |
|---|---|---|
| `libpfs3` | **0.1.3** (ART's pin), `.crate` SHA-256 `02f457ef…eed4317` — the checksum `src-tauri/Cargo.lock` carries; **0.1.6** (newest, 2026-09-07), `74b49c9f…cc80` | downloaded from `static.crates.io`, read and built |
| `metaneutrons/pfs3` (libpfs3's repository) | `main` at `05f50b06c3cd` (2026-09-08) | GitHub API: issues, pull requests, `CONTRIBUTING.md`, `LICENSE` |
| pfs3aio source | `tonioni/pfs3aio` `211f7f06aa29` (2026-08-07) | `format.c`, `anodes.c`, `allocation.c`, `blocks.h`, `LICENSE` |
| pfs3aio binary | **3.1**, Aminet `disk/misc/pfs3aio.lha` (`4837919c…f104`), `pfs3aio` 59 120 B (`185fae78…6047`) | the handler hst-imager embedded as the RDB's `PDS\3` driver |
| hst-imager | **1.6.616** (`a91fa4c`), `console_linux_x64` (`aa5b4608…4ff1`) — the same release ART-310's Windows run used | run |
| hst-amiga | `henrikstengaard/hst-amiga` `6b4558418083` (2026-07-02) | `Pfs3Formatter.cs`, `anodes.cs` — structure only |
| AmigaDiskKit | `thomas-luebker/AmigaDiskKit` `5114d1a70d6d` (2026-07-29) | `PFS3Format.swift`, `PFS3Core.swift`, `PFS3Constants.swift` |

No Amiga emulator is installed on the machine this ran on, so **nothing here is real pfs3aio mounting
a volume** — hst-imager is still the only outside implementation that touched these images (§ 7).

## 1. The two defects, in `libpfs3` 0.1.3's own lines

- **The `SB` level.** In SUPERINDEX mode `format.rs:230` writes the anode *index* block's number straight
  into `rext.superindex[0]`. The reader resolves a large-mode anode as `superindex → SB → IB → AB`
  (`anode.rs:103-125`), so it reads the `IB`'s first entry as if it named an index block. The writer's
  large-mode branch expects the same three levels (`writer.rs:951-1013`).
- **Anodes 0–4.** The format writes one anode — `ANODE_ROOTDIR` (5) — into the first anode block
  (`format.rs:278-287`) and leaves 0–4 as `(0, 0, 0)`, which is what every PFS3 allocator reads as *free*.
  `libpfs3`'s own allocator never sees it: it skips any anode number below `ANODE_USERFIRST` (6)
  (`writer.rs:931`).

**Not fixed upstream.** `libpfs3` 0.1.6 still writes `superindex[0] = anidx_blk` (`format.rs:235`) and
still writes only anode 5 (`format.rs:288-291`). Between 0.1.3 and 0.1.6 the `src/` diff is clippy-style
rewriting (inline format arguments, `let … else`, digit separators, an `else` un-nested) and the removal
of `#![deny(warnings)]` from `lib.rs`; no line changes what is written to disk. The repository has 23
issues and pull requests, all closed and all release, CI or dependency work; none mentions the format.
0.1.6 also raises the crate's `rust-version` to **1.94.1**, past ART's MSRV of 1.93. 0.1.3 declares no
`rust-version` (edition 2024), and ART already builds it.

## 2. What pfs3aio itself does

`format.c`'s `FDSFormat` (lines 225-253), after `MakeRootBlock`, `MakeVolumeData`, the rootblock
extension and `InitModules(volume, TRUE)`:

```c
MakeBitmap (g);
do {
    i = AllocAnode (0, g);
} while (i<ANODE_ROOTDIR-1);
MakeRootDir (g);
```

- **Anodes 0–4 are reserved by allocation, not written by hand.** `AllocAnode` (`anodes.c:366-475`) marks
  the slot it returns as `clustersize = 0, blocknr = 0xffffffff, next = 0` (`anodes.c:459-461`). The loop
  runs until it has handed out 4, so it reserves 0, 1, 2, 3 and 4. `MakeRootDir` then takes the next
  anode, which is 5. `FreeAnode` refuses to free these numbers (`anodes.c:483`, *"don't kill reserved
  anodes"*).
- **The `SB` level is made by the first allocation.** While formatting, `MakeAnodeBitmap` starts from
  `maxanseqnr = 1` (`anodes.c:971-973`). So the first `AllocAnode` finds no anode block, and
  `big_NewAnodeBlock(0)` runs (`anodes.c:582-628`). It asks `GetIndexBlock(0)`, which finds nothing, and
  calls `NewIndexBlock(0)` (`anodes.c:717-772`). In supermode that asks `GetSuperBlock(0)`, which finds
  nothing, and calls `NewSuperBlock(0)` (`anodes.c:844-890`). `NewSuperBlock` allocates a reserved block,
  stores it in `rblkextension->blk.superindex[0]` and gives it the id `SBLKID`. The chain on disk is
  therefore `superindex[0] → SB → IB → AB`, each allocated in that order after the bitmap blocks. In
  small mode `NewIndexBlock` writes `rootblk->idx.small.indexblocks[seqnr]` instead, so there is no `SB`.
- **The mount side needs the same shape.** When not formatting, `MakeAnodeBitmap` walks
  `superindex[s]` → `GetSuperBlock` → the last non-zero `index[i]` → `GetIndexBlock`
  (`anodes.c:977-993`). `GetSuperBlock` rejects a block whose id is not `SBLKID` (`anodes.c:820-832`).
  That is the crash hst-imager's C# port shows on a `libpfs3` large-mode volume (§ 4).

hst-amiga's `Pfs3Formatter.FormatPartition` has the same reservation loop (`Pfs3Formatter.cs:108-114`), and
its `anodes.cs` has the same `NewSuperBlock` / `NewIndexBlock` / `AllocAnode` structure. AmigaDiskKit's
Swift port of hst-amiga does too (`PFS3Format.swift:85-94`, `PFS3Core.swift:547-568, 658-743, 808-850`).

**One place where these ports are not a reference: reserved-area sizing.** pfs3aio's current
`CalcNumReserved` (`format.c`, the `taken = 32; for (i = 2048; …)` loop — revision 11.29, *"Bigger
reserved area (2x)"*) is what `libpfs3`'s `calc_num_reserved` ports. [ART-122](../../ISSUES.md) worked it
through by hand and found the two agree. hst-amiga and AmigaDiskKit still carry the older `schijf`
threshold table (`Pfs3Formatter.cs:305-339`, `PFS3Format.swift:22-25, 152-167`). Route A should
therefore **not** copy sizing from either port, and the experiment below shows sizing is not part of the
defect.

## 3. The experiment, decided before it ran

**One variable per arm: who formats DH0, and which fix the formatter carries.** Everything else is held
fixed:

- hst-imager builds every image: `blank <size>`, then `format … Rdb PDS3 --file-system-path pfs3aio`. So
  the RDB, the embedded pfs3aio 3.1 driver and the partition geometry are identical in every arm.
- The `libpfs3` arms then reformat DH0 in place through `format_with_size`.
- The fixes are two environment switches on a **copy** of `libpfs3` 0.1.3's `format.rs` (27 changed
  lines):
  - `EXP_SB` allocates one more reserved block before the anode index block, writes it as `SBLKID` with
    `index[0] = IB`, and points `superindex[0]` at it.
  - `EXP_ANODES` writes `blocknr = 0xFFFFFFFF` into anodes 0–4 of the first anode block.

  This copy is the experiment, not the patch.
- Sizes: `1gb` gives 2 094 624 blocks (small mode) and `6gb` gives 12 580 848 (SUPERINDEX mode, past
  MAXSMALLDISK = 10 241 440). These are 3 cylinders fewer than ART-310's 2 097 648 and 12 583 872, at
  both sizes. The control arm here is this run's own `hst` arm, not ART-310's numbers.

**Probes, in this order in every arm:**
- **A:** `hst fs dir`
- **B:** hst's **first** `mkdir`
- **C:** `libpfs3` writes a directory and 300 files (content a pure function of the path). That is enough
  to need several anode blocks.
- **D:** `libpfs3` lists and hashes everything.
- **E:** `hst fs copy` extracts the 300 files, and they are compared byte for byte.
- **F:** hst's `mkdir` after `libpfs3`'s writes
- **G:** `hst fs dir`
- **H:** the anode number each directory received

Before the probes, the image's own blocks are read raw: the pointer, the chain it leads to, and anodes 0–5.

**Expected, if ART-310 is right:**

| Arm | 1 GiB (small mode) | 6 GiB (SUPERINDEX mode) |
|---|---|---|
| `hst` (control) | every probe passes | every probe passes |
| `orig` | hst lists; hst's first `mkdir` fails | hst cannot mount; `libpfs3` fails on its own volume |
| `sb` | same as `orig` (there is no `SB` in small mode) | mounts; hst's first `mkdir` still fails |
| `anodes` | every probe passes | still unmountable |
| `both` | every probe passes | every probe passes |

## 4. What it measured

Run twice. Run 1's structure probe tested `MODE_SIZEFIELD` (0x10) where it meant `MODE_SUPERINDEX` (0x80),
so its small-mode structure rows were wrong and are not used. The probe results A–G were the same in
both runs. Run 2 is below.

| Arm | Size | Chain on disk | Anodes 0–4 | A dir | B first mkdir | C fill 300 | D read+hash | E hst extract | F mkdir after | H hst's `HstDir2` anode |
|---|---|---|---|---|---|---|---|---|---|---|
| hst | 1 GiB | `indexblocks[0]`→IB→AB | reserved | ok | ok | ok | 300, 0 bad | 300, 0 bad | ok | 196672 (seq 3) |
| orig | 1 GiB | IB→AB | **free** | ok | **`ERROR_DISK_FULL`** | ok | 300, 0 bad | 300, 0 bad | ok | **1** |
| sb | 1 GiB | IB→AB | **free** | ok | **`ERROR_DISK_FULL`** | ok | 300, 0 bad | 300, 0 bad | ok | **1** |
| anodes | 1 GiB | IB→AB | reserved | ok | ok | ok | 300, 0 bad | 300, 0 bad | ok | 196672 (seq 3) |
| both | 1 GiB | IB→AB | reserved | ok | ok | ok | 300, 0 bad | 300, 0 bad | ok | 196672 (seq 3) |
| hst | 6 GiB | `superindex[0]`→**SB→IB→AB** | reserved | ok | ok | ok | 300, 0 bad | 300, 0 bad | ok | 196672 (seq 3) |
| orig | 6 GiB | `superindex[0]`→**IB→AB** | **free** | **`NullReferenceException`** | fails | **`anode 5 not found`** | fails | fails | fails | — |
| sb | 6 GiB | SB→IB→AB | **free** | ok | **`ERROR_DISK_FULL`** | ok | 300, 0 bad | 300, 0 bad | ok | **1** |
| anodes | 6 GiB | IB→AB | reserved | **`NullReferenceException`** | fails | **`anode 5 not found`** | fails | fails | fails | — |
| both | 6 GiB | SB→IB→AB | reserved | ok | ok | ok | 300, 0 bad | 300, 0 bad | ok | 196672 (seq 3) |

**Every cell matched the expectation written down before the run.** In more detail:

- **Each fix is necessary, and together they are sufficient, at both sizes, against this outside
  implementation.** `sb` alone mounts the large volume but leaves the first-write failure. `anodes` alone
  cures small mode and does nothing for large mode.
- **ART-310's second claim is now measured, not only reasoned.** On the `orig` and `sb` volumes, hst's
  directory made *after* `libpfs3` had written is given **anode 1**, a number pfs3aio reserves. On every
  volume with anodes 0–4 reserved, the same directory is given 196672 (seqnr 3, offset 64), exactly as on
  hst's own volume.
- **The fixed format lands where hst's does.** At 6 GiB, `both` puts anode 5's directory block at block
  3110, as hst's format does; `orig` puts it at 3108, two blocks earlier, because it allocates no `SB`. At
  1 GiB all five arms put it at 524. Reserved-area sizing was not changed (24 032 and 103 040 reserved
  blocks in every `libpfs3` arm).
- **The error texts reproduce ART-310's Windows run exactly**: `System.IO.IOException: ERROR_DISK_FULL` on
  the first `mkdir`, and `System.NullReferenceException` on every command against a two-level large
  volume.

**`libpfs3`'s own suite with both fixes on.** Unmodified 0.1.3 passes its 207 tests. With `EXP_SB=1
EXP_ANODES=1` it passes the same 207. No existing test asserts the old layout.

**The third limit is untouched by either fix, as ART-310 predicted.** On a 1 GiB `both` volume,
`libpfs3` wrote 20 655 files into one directory and stopped with `disk full: no index block slot
available` (`writer.rs:1024`). That is the small-mode writer cap `core::card::sizing::
pfs3_small_mode_anode_cap` models. Its large-mode twin is in the same function: when `superindex[n]` is
unset, the writer returns `no superindex slot available` (`writer.rs:980`) instead of allocating an `SB`
the way pfs3aio's `NewIndexBlock` does. That is reached only past 253² anode blocks, far beyond any card
ART plans, but it is the same shape as the small-mode gap.

## 5. Eliminations

- **ART-122's reserved-bitmap disagreement is not the cause of hst's failed first write.** ART-122 found
  ART's reserved area larger than hst-imager's and showed ART's arithmetic is pfs3aio's. Here the
  `anodes` arm keeps `libpfs3`'s sizing, and so still disagrees with hst's by the same kind of margin, yet
  hst's first `mkdir` succeeds. The failure follows anodes 0–4 alone. ART-310's statement that it *"is the
  root cause ART-122 worked around"* stands, now with the one-variable arm behind it.
- **An `SB` block alone does not repair large mode.** The `sb` arm mounts but is not safe to write from an
  Amiga-side allocator (F: anode 1). ART-310's control (only the anode markers) crashed; this run's `sb`
  arm adds the other half.
- **Upgrading `libpfs3` is not route A's shortcut.** 0.1.6 carries both defects (§ 1).

## 6. What vendoring meets in this repository

Measured or read here, for the design to decide from, not decided.

- **The version pin.** `native.rs` reports `LIBPFS3_VERSION` through `probe()`, and
  `the_pinned_version_constant_matches_cargo_toml` checks `Cargo.toml` for `libpfs3 = "=0.1.3"`. A patched
  crate that still calls itself `0.1.3` would make `probe()` claim an implementation nobody built. Tried in
  a throwaway crate: `libpfs3 = "=0.1.3"` together with
  `[patch.crates-io] libpfs3 = { path = "<vendored>" }`, where the vendored `Cargo.toml` says
  `version = "0.1.3+art.1"`, resolves, builds, and locks as `0.1.3+art.1`. Cargo ignores build metadata
  when matching `=0.1.3`. Whether ART keeps that line and moves the constant to `0.1.3+art.1`, or writes
  the dependency as a plain `path`, is the design's choice.
- **Clippy.** `cargo clippy --all-targets -- -D warnings` on that throwaway crate checked the vendored
  `libpfs3` as a local path crate and passed. The lint cap that silences registry crates is not what
  kept it clean. ART's CI step (`ci.yml:65`) would lint the vendored source too, so it has to stay
  clippy-clean at ART's toolchain.
- **What to vendor.** `libpfs3`'s `src/` files carry no SPDX headers, and the repository `LICENSE` and the
  crate metadata say **LGPL-3.0-or-later**. Its `tests/*.rs` still carry `SPDX-License-Identifier:
  GPL-3.0-only` (copyright 2025). GPL-3.0-only is not GPL-3.0-or-later, so vendoring the tests would bring
  a licence ART's inventory does not list. The tests also build their scratch paths from
  `std::env::temp_dir()` directly, which is what `scripts/scratch-guard-sweep.py` exists to catch in
  ART's own code. The sweep's scope was not checked against a vendor directory. Whether the vendored copy
  carries `tests/`, and how ART then keeps them running, is for the design.
- **`deny.toml`** already allows `LGPL-3.0-or-later` for `libpfs3` (`deny.toml:36-41`); a path crate
  keeps the same licence field. `docs/licenses.md` and `THIRD_PARTY_LICENSES.md` would need the vendored,
  patched copy named as such.
- **ART's own suite cannot run on this machine.** `cargo test --lib --no-run` on Linux stops on four
  errors, all in test-only Windows code (`commands/osinstall.rs:4771`, `core/appearance/mod.rs:1863`,
  `OpenOptionsExt::custom_flags` / `share_mode`). The project builds for `x86_64-pc-windows-msvc` only,
  by design. The patch's `libpfs3`-level evidence can be produced anywhere; ART's `cargo test --lib`
  cannot be produced here.
- **Upstream's rules**, for when the patch is offered (`CONTRIBUTING.md` at `05f50b06c3cd`):
  - branch `fix/<topic>`
  - Conventional Commits, and the squash-merged PR title decides the version
  - toolchain from `rust-toolchain.toml`, which pins channel **1.98.1**. *Corrected 2026-09-13:* this
    line first said 1.94.1, which is the crate's `rust-version` (its MSRV), not the toolchain the
    repository builds with
  - lefthook + gitleaks
  - **"No AI attribution trailers, in commits or in the pull request body."**

## 7. Licences behind the reference code

- **pfs3aio** is BSD-4-Clause (`LICENSE`: Copyright (c) 2011 Michiel Pelt, with the advertising clause).
  The FSF lists the four-clause BSD licence as incompatible with the GPL.
- **hst-amiga** (MIT) and **AmigaDiskKit** (Apache-2.0) are both ports descending from pfs3aio. So are
  `libpfs3`'s formatter (*"Ported from pfs3aio/format.c and amitools PFSFormat.py"*) and ART's own
  `core::card::sizing`, which ports `libpfs3`'s `calc_num_reserved`.
- This note does not settle what that chain means. It records it, because route A is a patch **in the
  same lineage**. What the fix needs from pfs3aio is on-disk layout, not code: *anodes 0–4 hold
  `(0, 0xFFFFFFFF, 0)`* and *`superindex[0]` names an `SB` block whose `index[0]` names the first `IB`*.
  The experiment's patch was written in `libpfs3`'s own idiom (`put_u16`/`put_u32` into a zeroed buffer),
  not translated from C, C# or Swift.

## 8. Not proven, and owed

- **Real pfs3aio has not mounted any of these volumes.** hst-imager is a C# port of pfs3aio, not pfs3aio
  on 68k code under Kickstart. ART-310's "owed" line stands: mount a `libpfs3`-formatted partition under
  real pfs3aio in WinUAE — now **both** a 0.1.3 one (expected to fail at 6 GiB) and a patched one
  (expected to mount, and to create a directory whose anode is not 1–4).
- **No 2048- or 4096-byte reserved-block size was tried** (partitions past ~104 GB). ART plans nothing
  past `PFS3_CEILING_BYTES`, which is inside the 1024-byte range.
- **One file system, one handler version.** pfs3aio 3.1 from Aminet was the embedded driver. hst-imager
  formats with its own port whatever the driver is, so the driver matters only to a real Amiga (above).

## 9. Questions this puts to the design

1. **Anode reservation: by hand or by allocator?** pfs3aio reserves 0–4 by calling `AllocAnode`.
   `libpfs3`'s format has no allocator and writes blocks directly; the experiment wrote the five markers.
   The bytes on disk are the same either way (§ 4, anodes 0–4 read back as hst writes them).
2. **Reserved-block allocation order.** The experiment allocates `SB` just before `IB`, which is pfs3aio's
   order, and at 6 GiB lands the root directory where hst's format does. Holding this with a test
   (an image-level comparison against a known layout) is cheap.
3. **Lift the writer's two index-block gaps in the same vendored copy, or not?** The small-mode one is what
   `core::card::sizing` currently plans around (ART-310's third limit). Lifting it changes sizing and its
   tests, so it is scope the owner decides, not a side effect of the format fix.
4. **Tests in `libpfs3`, tests in ART, or both?** At minimum: the 6 GiB `superindex[0]` id is `SB`; anodes
   0–4 read `(0, ~0, 0)` at both sizes; a large-mode `libpfs3` volume accepts its own writes. The
   hst-imager round trip belongs in `scripts/pfs3-oracle-check.py` (local, not CI), which today exercises
   a 220 MB small-mode volume only.
5. **What round 3 may do once this lands.** ART-310's rule, *"never `NativeFormatter` past MAXSMALLDISK
   while this entry is open"*, is written into `plan_card_image`'s doc and ART-310 itself. Closing ART-310
   should state what evidence closes it: the oracle past MAXSMALLDISK, and the real-pfs3aio mount above.

The experiment's harness (`Cargo.toml`, `src/main.rs`, `run.py`), the `format.rs` diff, both runs' JSON
and every probe's full hst-imager log are kept out of the repository, like ART-310's own experiment, at
`~/Belgeler/Projeler/art-experiments/2026-09-13-art-310/` on the Linux machine this ran on (748 K, images
deleted after each arm).
