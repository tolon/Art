# ART-310: `libpfs3`'s format writes what pfs3aio writes

**Status:** the owner's four decisions taken 2026-09-13 (§ 2); this document is for the owner to approve
before the plan is written.
**Issue:** [ART-310](../../ISSUES.md) — route A, chosen by the owner 2026-09-11.
**Research:** `docs/superpowers/notes/2026-09-11-libpfs3-format-fix-research.md` (the experiment, both
fixes one variable at a time, pfs3aio's own source, upstream, licences, vendoring). Nothing in this
document is argued again; it is decided from there.

## 1. What is wrong, in one paragraph

`libpfs3` 0.1.3's `format_with_size` makes two mistakes, and 0.1.6 still makes both.

- **SUPERINDEX mode (above MAXSMALLDISK = 10 241 440 blocks):** it points `rext.superindex[0]` straight
  at the anode index block. The on-disk chain is `IB → AB` where every reader, `libpfs3`'s own included,
  expects `SB → IB → AB`. The volume is unmountable by hst-imager (`NullReferenceException`) and
  unreadable by `libpfs3` itself (`anode 5 not found`).
- **Every size:** it leaves anodes 0–4 as `(0, 0, 0)`, which pfs3aio's allocator reads as free. The
  first directory hst-imager makes fails `ERROR_DISK_FULL` (anode 0 is the allocator's failure value).
  A later one is given anode 1, a number pfs3aio reserves.

Measured on 2026-09-13 at 1 GiB and 6 GiB, five arms, every cell as predicted (research § 4).

## 2. Decisions (the owner's, 2026-09-13)

1. **Format only.** The `SB` level and anodes 0–4, nothing else.
   - `libpfs3`'s writer has two gaps of the same shape:
     - small mode never allocates a second index block (`writer.rs:1024`);
     - large mode never allocates a second `SB` (`writer.rs:980`).

     Neither is touched. They are filed as **ART-311** in the same round.
   - `core::card::sizing`, and its model of the small-mode cap, stays as it is.
2. **Vendor `src/` and ART's own tests.**
   - The vendored copy carries `src/`, the manifest, `README.md` and the licence text.
   - It does **not** carry `libpfs3`'s `tests/`, for three reasons: they declare `GPL-3.0-only`, their
     fixtures are 9.3 MB (an 8 MB HDF), and their dev-dependency `sevenz-rust` 0.6 carries
     RUSTSEC-2026-0245 (path traversal on extract) and RUSTSEC-2026-0246 (unmaintained).
   - The fix is proved by tests written in ART.
3. **`libpfs3 = "=0.1.3"` stays; the patched copy is `0.1.3+art.1` through `[patch.crates-io]`.**
   - `Cargo.lock`, `probe()` and the version constant all say `0.1.3+art.1`, so nothing claims an
     implementation nobody built.
   - Tried on 2026-09-13: it resolves, and clippy is clean (research § 6).
4. **Upstream: prepared here, opened by the owner.**
   - The patch and the pull-request text are prepared to `metaneutrons/pfs3`'s rules: a `fix/` branch,
     Conventional Commits, and no AI attribution trailers in commits or the PR body.
   - The owner opens the PR from their own account, after the real-pfs3aio mount (§ 7).

## 3. The change to `format.rs`

Against `libpfs3` 0.1.3, `src/format.rs`. Nothing outside this file changes in the crate except its
version.

- **Reserved-block allocation order becomes pfs3aio's:**
  1. rootblock extension
  2. bitmap blocks
  3. bitmap index blocks
  4. **`SB`**, in SUPERINDEX mode only
  5. anode index block
  6. anode block
  7. root directory block

  0.1.3 already allocates everything except `SB` in this order, so only `SB` is new. At 6 GiB this
  puts the root directory block where hst-imager's format puts it (block 3110). That was measured, not
  assumed.
- **The `SB` block**, in SUPERINDEX mode, is one reserved block:
  - `id = SBLKID` (`0x5342`)
  - `datestamp = 1`
  - `seqnr = 0`
  - `index[0]` = the anode index block

  `rext.superindex[0]` names it. In small mode nothing changes: `rootblock.indexblocks[0]` still names
  the index block.
- **Anodes 0–4** of the first anode block are written `clustersize = 0, blocknr = 0xFFFF_FFFF,
  next = 0`. That is exactly what pfs3aio's `AllocAnode` leaves (`anodes.c:459-461`) and what
  hst-imager's format was measured to write.
  - **By hand, not by an allocator:** `libpfs3`'s format has no anode allocator. The bytes are the same
    either way (research § 9.1).
- **Unchanged:**
  - reserved-area sizing (`calc_num_reserved` is already pfs3aio's)
  - option flags
  - datestamps, and `enable_deldir`
  - small-mode layout apart from the five markers
- **The module's leading doc comment** keeps its format-sequence list and names the two structures.
  Each changed site carries a one-line `ART-310` comment pointing at `ART-PATCH.md`, so the diff reads
  on its own.

The patch is written in `libpfs3`'s own idiom (`put_u16` / `put_u32` into a zeroed buffer) from the
on-disk layout. It is not translated from pfs3aio's C, hst-amiga's C# or AmigaDiskKit's Swift (licences:
research § 7).

## 4. Where it lives

```
src-tauri/vendor/libpfs3/
  Cargo.toml       from 0.1.3's Cargo.toml.orig: version "0.1.3+art.1", [dev-dependencies] removed
  README.md        unchanged
  LICENSE          metaneutrons/pfs3's LICENSE at 33e9ff6ba846, the commit 0.1.3 was packaged from
                   (.cargo_vcs_info.json), unchanged — it is a five-line pointer ("See
                   https://www.gnu.org/licenses/lgpl-3.0.txt for the full license text"), and the
                   .crate itself carries no licence file at all
  COPYING.LESSER   the full LGPL-3.0 text from gnu.org, because LGPL-3.0 § 4 has the licence text
                   accompany the library; the GPL-3.0 text it builds on is ART's own LICENSE
  ART-PATCH.md     what changed and why, the original .crate's SHA-256 (02f457ef…eed4317), the
                   upstream commit, the diff against 0.1.3, and the upstream status line
  src/             0.1.3's src/, with format.rs patched
```

`src-tauri/Cargo.toml`:

```toml
libpfs3 = "=0.1.3"                 # unchanged line — the pin test still reads it

[patch.crates-io]
libpfs3 = { path = "vendor/libpfs3" }
```

- **Not a workspace member.** `src-tauri/Cargo.toml` has no `[workspace]`. The plan checks that
  `cargo fmt --check` leaves the vendored source alone and that `cargo clippy --all-targets -D warnings`
  stays clean on it.
- **Sweeps.** `scratch-root-sweep.py` and `scratch-guard-sweep.py` walk `src-tauri/src` only.
  `control-byte-sweep.py` walks the whole tree, and 0.1.3's `src/` has no stray control byte or TAB.
- **`cargo deny`.** A path crate keeps its `LGPL-3.0-or-later` licence field, which is already allowed.
  `[sources]` concerns registries and git, not paths. The plan runs `cargo deny check` to confirm.
- **`[profile.dev.package.libpfs3] opt-level = 3`** names the package, not its source, so it applies to
  the vendored copy unchanged. The plan confirms the sizing suite's time is unchanged.

## 5. Tests, in ART

In `core::preload::native`'s tests, beside the PFS3 tests already there. Each is **seen red with the
`[patch]` line removed** (so against 0.1.3 from crates.io) before it is seen green, and each has a
mutation of the patch that fails it.

| Test | Asserts, by reading the image's own blocks | Red on 0.1.3 | Mutation that must fail it |
|---|---|---|---|
| `a_small_pfs3_format_reserves_anodes_zero_to_four` | small mode: `rootblock.indexblocks[0]` → `IB` → `AB`; anodes 0–4 are `(0, 0xFFFFFFFF, 0)`; anode 5 is the root directory | anodes 0–4 read `(0,0,0)` | write four markers, not five |
| `a_large_pfs3_format_writes_the_superblock_level` | past MAXSMALLDISK: `MODE_SUPERINDEX` set; `rext.superindex[0]` is an `SB` whose `index[0]` is an `IB` whose `index[0]` is an `AB`; anodes 0–4 reserved | `superindex[0]` is an `IB` | point `superindex[0]` at the `IB` again |
| `a_large_pfs3_volume_takes_its_own_writes` | past MAXSMALLDISK: `NativeFormatter::format_partition`, then `copy_in` of a small tree, then `libpfs3` lists and reads it back byte for byte | `anode 5 not found` | skip writing the `SB` block but keep the pointer |
| `probe_names_libpfs3` (existing, tightened) | `probe()` says `libpfs3 0.1.3+art.1` | says `0.1.3` | — |
| `the_pinned_version_constant_matches_cargo_toml` (existing, extended) | `Cargo.toml` still pins `=0.1.3`, still patches to `vendor/libpfs3`, and the vendored manifest's version equals `LIBPFS3_VERSION` | — | drop the `[patch]` line |

- **The large-mode tests need a partition past 10 241 440 blocks** (≈ 4.9 GiB) on a test card.
  - `core::hdf::create_hdf` writes the RDB's leading blocks and then extends the file with `set_len`
    (`hdf.rs:240-293`). It does not set NTFS's sparse flag.
  - The format writes only the partition's reserved area, about 50 MB at 6 GiB, near the partition's
    start. So the expected cost is that area's zero-fill, not 6 GB. That is an expectation, and **the
    first thing the plan measures** on the owner's machine, time and bytes on disk.
  - If it is too slow, the fallback is to format a `FileRegionMut` over a file the test creates itself,
    still through `format_with_size`.
  - Both tests stay in the full suite, never `#[ignore]`d.
- **Reading the blocks** goes through a small test-only helper that follows the rootblock, the extension
  and the index chain. It does not use `libpfs3`'s reader, so the test does not prove the writer against
  the reader it shares a crate with.

## 6. The outside checks

- **`scripts/pfs3-oracle-check.py`** today writes one 220 MB small-mode volume. It gains a
  **large-mode direction**: ART formats and fills a partition past MAXSMALLDISK through a second env-gated
  hook, then hst-imager `fs dir`, `fs mkdir`, `fs copy` it out and hashes it. The script then asks ART's
  read hook for the new directory's anode number and fails if it is 1–4. It stays local-only, as the
  script already is.
- **The experiment harness** of 2026-09-13 is not brought into the repository. The oracle is the
  repository's check.

## 7. What closes ART-310, and what lifts round 3's rule

ART-310 records a rule: *round 3 must not send a volume past MAXSMALLDISK to `NativeFormatter` while
ART-310 is open.* It is written in `plan_card_image`'s doc comment too (`core/card/sizing.rs:457-459`).

- **ART-310 moves to Fixed** when all of these hold on the branch:
  - § 5's tests have been seen red and then green;
  - their mutations have been seen failing;
  - § 6's oracle passes at both sizes on the owner's Windows machine;
  - the full `cargo test --lib` has run twice.
- **The rule in `plan_card_image`** is removed in the same round, and the doc comment says why.
- **Owed by a person, written into the Fixed entry rather than blocking it:** mount a patched 6 GiB
  volume under **real pfs3aio in WinUAE**, `dir` it, and make a directory. The upstream PR (§ 2.4) waits
  for this. *If the owner would rather the rule stay until the WinUAE mount, that is one sentence in the
  plan; the default above is the owner's to overturn.*

## 8. Documents the round touches

- **`docs/ISSUES.md`:**
  - ART-310 moves to Fixed, with the evidence and the owed mount;
  - **ART-311** is filed (the writer's two index-block gaps, with `pfs3_small_mode_anode_cap` named as
    what they cost).
- **`docs/licenses.md` and `THIRD_PARTY_LICENSES.md`:** `libpfs3` is named as *vendored and patched*
  (`0.1.3+art.1`), with where the patch is described.
- **`deny.toml`:** the comment beside `LGPL-3.0-or-later` says the crate is vendored.
- **`core/preload/native.rs`:** the module doc and `LIBPFS3_VERSION`'s doc comment.
- **`core/card/sizing.rs:457-459`:** the ART-310 rule.
- **`CHANGELOG.md` (Fixed):** *a PFS3 partition larger than about 4.9 GB that ART formatted could not be
  mounted, and on every size the Amiga's first new directory could fail.*
- **`docs/STATUS.md`** "Start here" and the snapshot, and a **`docs/session-log.md`** row.

## 9. Not in this round

- ART-311 (the writer's index-block gaps) and any change to `core::card::sizing`.
- 2048- and 4096-byte reserved blocks (partitions past ~104 GB). ART plans nothing there.
- ART-113 (non-ASCII names) and ART-116 (comments and dates), which are `libpfs3` writer limits too.
  They belong to the "PFS3 full write" work the owner chose from AmigaDiskKit, as its own round.
- Moving to `libpfs3` 0.1.6 (its `rust-version` is 1.94.1, past ART's MSRV).
