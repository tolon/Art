# One-button card, round 2 — LZX and restoring Amiga names

*2026-09-16, branch `art-card-round-2` (HEAD eb04326). Research only: no production code touched.
Describes the tree and the crates on this day. Follows
[2026-09-16-card-round-2-research.md](2026-09-16-card-round-2-research.md), whose findings 4 and 5
this note builds on and partly overturns (its "LZX is a refusal" is replaced by the owner's
decision below).*

## Question

The owner decided on 2026-09-16: **(a)** LZX archives are supported this round; **(b)** a name
escaped for Windows while extracting a WHDLoad HDF (`ExtractReport.renamed`, `AUX → _AUX`) gets
its real Amiga name back on the PFS3 card; **(c)** a multi-title archive goes in as its tree.

- **A.** What is Amiga LZX, which decoder can ART use (licence, maintenance, merged groups,
  store/LZX modes, CRC, attributes), what do the owner's real `.lzx` files look like, where does a
  backend plug into `core/archive`, and what oracle can check it?
- **B.** How does ART record an escaped Amiga name today, does the PFS3 copy read it, and what must
  `content.rs` write so the card gets `AUX` rather than `_AUX`?

Scratch for everything run: `E:\amiga\ProjeART\build\tmp\card-r2\lzx\` (probes, dumps, a
compiled `unlzx.exe`, `unar` 1.8.1, `compare.py`). Owner files were only read.

## Findings — A. LZX

### A1. What the format is

- LZX is the Amiga archiver by Jonathan Forbes and Tomi Poutanen, © 1995 Data Compression
  Technologies; shareware, with a free keyfile released in 1997; Forbes then sold the algorithm to
  Microsoft, where it became the CAB/CHM LZX codec
  (<https://en.wikipedia.org/wiki/LZX>, the program's own guide
  <https://www.amiga-stuff.com/text/archivers/LZX.guide>, <http://xavprods.free.fr/lzx/>).
- **Differs from Microsoft LZX:** Amiga LZX has a fixed 64 KiB window and **merges** files into
  groups compressed as one stream; Microsoft's window varies 32 KiB–2 MiB
  (<https://en.wikipedia.org/wiki/LZX>). So `lzxd` (Microsoft LZXD) is not a candidate.
- **Container, as three independent sources state it** (XADMaster `XADLZXParser.m:52-66`,
  <https://github.com/MacPaw/XADMaster/blob/master/XADLZXParser.m>; `unlzx.c:1080` and its header
  loop; amiga-lzx `src/archive/reader.rs`): a 10-byte info header `LZX…`, then a chain of records —
  31 fixed bytes (attributes, unpacked size u32 LE, packed size u32 LE, os, method, merged flag,
  comment length, version, **date u32**, data CRC-32 LE, header CRC-32 LE, name length), the raw
  name, the raw comment, then the packed bytes. A record whose packed size is 0 belongs to the
  group closed by the next record with a non-zero packed size; that record carries the whole
  group's stream. Methods: 0 = stored, 2 = LZX. Names use `/` and are Latin-1. **There are no
  directory records** — drawers are implied by the paths.
- **The date is where the sources disagree** — see A3.

### A2. Candidate decoders

| Candidate | Licence | State | Language | Merged groups | Store / LZX | CRC | Attributes · comment · date | Bounds |
|---|---|---|---|---|---|---|---|---|
| **`newtua-amiga` 0.1.1** (<https://lib.rs/crates/newtua-amiga>, repo `new-the-unarchiver/newtua-formats`) | LGPL-3.0-or-later (in `deny.toml` allow list) | 2 releases, 2026-07-13 / 07-18, one author; a port of XADMaster | Rust, `#![forbid(unsafe_code)]`; depends on `newtua-common` | yes (`lzx.rs:305-380`) | both | data CRC checked; **header CRC ignored** (`lzx.rs:325`) | protection, comment, raw date public; **decoded date wrong** (A3) | whole archive copied into memory (`open`: `data.to_vec()`, `lzx.rs:236`); `read_entry` **re-decodes the whole group for every member** (`lzx.rs:246-285`); `Vec::with_capacity(out_len)` from the header's size (`lzx.rs:555`); no limit parameter |
| **`amiga-lzx` 0.1.5** (<https://github.com/bitplane/amiga-lzx>) | `WTFPL` per Cargo.toml, README says "WTFPL with warranty clause"; no licence file in the `.crate`. FSF: WTFPL is GPL-compatible (<https://www.gnu.org/licenses/license-list.html>). **Not in `deny.toml`'s allow list** | 5 releases 2026-04-13 → **2026-09-16 (today)**, 215 downloads, one author; README: "LLM assisted clean-ish-room implementation", decompressor documented from `unlzx.c` | Rust, no `unsafe`; deps `bitflags`, `thiserror` | yes, one pass per group (`reader.rs` `next_entry`) | both | data **and header** CRC | protection bits, comment, date (piecewise year table, A3) | streaming reader, but `vec![0u8; compressed_size]` and `Vec::with_capacity(expected)` from header values (`reader.rs:106`, `decoder.rs:223`); no list-without-decode, no limit. **Has an encoder** (`ArchiveWriter`) |
| **`unlzx.c`** 1.1 (Aminet `misc/unix/unlzx.c`; copy at <https://github.com/nhoudelot/unlzx>) | Aminet readme (<https://aminet.net/misc/unix/unlzx.c.readme>) states **no licence**; the GitHub fork's `LICENSE` is the Unlicense, which is the forker's statement, not the author's | 1998/2001, unmaintained | C, 1 374 lines; its own header: "Most of the decrunch functions encourage overruns in the buffers" | yes | both | data + header | protection, comment, date read **big-endian** (`unlzx.c:1080`) | none — by design |
| **XADMaster / `unar`** (<https://github.com/MacPaw/XADMaster>) | LGPL-2.1-or-later (`XADLZXParser.m` header) | maintained by MacPaw | Objective-C; Windows CLI 1.8.1 from <https://cdn.theunarchiver.com/downloads/unarWindows.zip> ("slightly outdated") | yes | both | data | protection, comment; date read **little-endian** (`XADLZXParser.m:63`) | n/a — external tool |
| **7-Zip 26.03** (installed) | — | — | — | — | — | — | — | **does not read Amiga LZX**: `7z i` lists `Cab`, `Chm`, `Lzh` and no LZX handler; `7z l boingbag1.lzx` → `ERROR: Cannot open the file as archive` |
| Python | — | — | — | — | — | — | — | PyPI `unlzx`, `lzx`, `amiga-lzx`, `pylzx` all 404 (checked 2026-09-16). crates.io search `lzx`/`unlzx` finds only the two Rust crates above plus Microsoft-LZX crates (`lzxd`, `lzxc`, `libchm`) |

### A3. Run on the owner's material

`find /e/amiga -maxdepth 5 -iname '*.lzx'` → 4 files (one a duplicate):
`Amigatolon\paketler\boingbag1.lzx` (3 657 786 B), `Amigatolon\amikitdev\WiFi_WPA_for_AmiKit_PiStorm.lzx`
(1 701 318 B, also in `paketler`), `Amigatolon\amikitdev\AmiKit-DevPack.lzx` (151 823 749 B).
First bytes of boingbag1: `4c5a 5800 …` then `BoingBag_1/Disk.info` at 0x2b — no directory record.

Listing (probe `lzxprobe` on newtua-amiga + an independent raw header walk; `unlzx.exe -v`):

| Archive | Entries | Unpacked | Merged groups (largest) | Methods | Non-ASCII names | Comments | Longest name |
|---|---|---|---|---|---|---|---|
| boingbag1.lzx | 998 | 11 468 044 | 68 (87 files) | 974 × LZX, 24 × stored | **36** (Latin-1: `e7` ç, `fc` ü, `ea` ê, `f1` ñ, `b6` ¶ — e.g. `Locale/Catalogs/français/…`, `Locale/countries/türkiye.country`, `AslSizePatch.¶`) | 0 | 95 B |
| WiFi_WPA_for_AmiKit_PiStorm.lzx | 54 | 2 823 168 | 6 (29 files) | 54 × LZX | 0 | 2 (`C/WirelessManager` "Prism2", `Devs/Networks/wifipi.device` "Now with 5Ghz and WPA2 support") | 52 B |
| AmiKit-DevPack.lzx (listed only) | 25 674 | 424 493 406 | 540 (767 files) | 25 459 × LZX, 215 × stored | **47** | 19 | **104 B** (PFS3 limit is 106, `native.rs:259-262`) |

`unlzx -v` of the WiFi archive (literal, abridged):

```
    1269      n/a 19:11:24 21-apr-2024 ----rwed "C/WaitUntilConnected.txt"
    ... 5 more ...
    9408     2496 Merged
  196480      n/a 19:12:04 28-aug-2013 ----rwed "C/WirelessManager"
: "Prism2"
 2823168  1698166 54 files
```

**Bytes — three decoders agree, one crashed.** Full extraction of boingbag1 and WiFi by
`newtua-amiga`, `amiga-lzx` and `unar.exe -e ISO-8859-1`, compared by `compare.py` (SHA-256 per
file, names NFC):

```
newtua  vs unar : boingbag1 A 998 B 998 identical 998 differ 0 · wifi A 54 B 54 identical 54 differ 0
amiga-lzx vs unar: boingbag1 A 998 B 998 identical 998 differ 0 · wifi A 54 B 54 identical 54 differ 0
```

All 1 052 entries also passed newtua's per-entry CRC-32 against the value the Amiga archiver
stored. Timing, boingbag1: newtua decode-only 2.39 s (per-member group re-decode); amiga-lzx decode
**and** write 1.73 s. `unlzx.c` built with MSVC (`cl /O2`, scratch patch: `_mkdir`, a getopt shim,
and a fix for `if(node == malloc(...))` at fork line 934, a `==`-for-`=` bug in the GitHub copy)
lists correctly but **extracts with "crc bad" and a segfault** on both archives — a 64-bit/MSVC port
problem not pursued; it is not usable as an oracle without more work.

**Dates — three readings, no two agree:**

| Entry | XADMaster / unar / newtua (u32 **LE**) | amiga-lzx (BE, piecewise year) | unlzx.c (BE, `+1970`) |
|---|---|---|---|
| boingbag1 `BoingBag_1/Disk.info` | `1990-16-18 11:43` (month 16: impossible) | 1999-12-24 10:34:23 | 1999-12-24 10:34:23 |
| WiFi `C/WaitUntilConnected.txt` | 1995-01-27 14:54 | **2030**-04-21 19:11:24 | 2024-04-21 19:11:24 |
| WiFi `C/WirelessManager` | 1995-09-00 29:31 (hour 29) | — | 2013-08-28 19:12:04 |

So: the date is **big-endian** (XADMaster's LE read is wrong on real archives — `unar -l` shows
the same wrong 1995 dates); boingbag1 (BoingBag 1 is a Christmas 1999 release, `Christmas ReadMe`)
is 1999 by both BE readings; the 2024 WiFi archive is right only by `+1970` — amiga-lzx's piecewise
table (verified by its author against dr.Titus's `Test_LZX.lzx`, `tests/issue_5_dates.rs`) is right
for one patched LZX and wrong for whatever wrote this archive. **The year field's meaning depends on
which LZX build wrote it and the bytes do not say which.**

### A4. ART's constraints and where a backend plugs in

- `core/` crate list is CLAUDE.md's; a new crate means CLAUDE.md + `THIRD_PARTY_LICENSES.md` in the
  same commit and `cargo deny check` passing. `deny.toml` (repository root, not `src-tauri/`)
  allows MIT, Apache-2.0 (+LLVM exception), BSD-2/3, ISC, Unicode-3.0/DFS-2016, Zlib, CC0-1.0,
  MPL-2.0, GPL-3.0-or-later, CDLA-Permissive-2.0, LGPL-3.0-or-later; `allow-git = []`. WTFPL is
  **not** allowed; LGPL-3.0-or-later is. `[graph] all-features = true` and no `exclude-dev`, so a
  dev-dependency is audited too.
- The trait: `core/archive/mod.rs:60-98` `ArchiveBackend { format, entries, read(index, limit),
  read_selected(wanted, limit, sink) }`. The doc demands the bound **inside** the decompression
  loop ("Returning more than `limit` is a backend defect; every backend is tested for it",
  `mod.rs` doc on the trait, `:52-59`). `read_selected` is overridden by 7z for solid blocks (`sevenz.rs:110`) — LZX
  merged groups are the same shape and need the same override (one decode per group, not per
  member).
- `ArchiveEntry` (`mod.rs:43-50`) is `{ name, is_dir, declared_bytes }` — **no protection, comment
  or date**. `grep protection|comment src/core/archive/*.rs` finds none in code: the gate carries
  no Amiga attributes for any format today, so LZX's protection bits and comments are dropped
  unless the entry type grows them and the gate writes `.uaem` sidecars.
- Dispatch: `mod.rs:110-140` `open_with_password` matches `detect(path).format_hint` —
  `"lha"`, `"zip"`, `"7z"`; `detect.rs` has signatures for LHA (`:336`, `-lh?-` at offset 2) and 7z
  (`:152`, `:354`) and none for `LZX` (grep `lzx` in `detect.rs`: nothing). An LZX backend needs a
  `"lzx"` hint there (`LZX` at offset 0) and an arm in `open_with_password` (password refused as
  for LHA/7z).
- Bounds, confirmed in `archive/extract.rs`: `MAX_TOTAL_OUTPUT = 2 GiB` (`:39`),
  `MAX_ENTRY_OUTPUT = 256 MiB` (`:47`), `MAX_ENTRIES = 100_000` (`:50`, checked `:210`); total
  budget `:244`, per-entry declared size `:251`. For LZX the member bound is not enough: a group's
  decode buffer is the **sum** of its members, so the backend must refuse a group whose declared
  total exceeds `MAX_ENTRY_OUTPUT` (or a group limit) before allocating, and must never allocate
  from a header value (both crates do, A2).
- The gate does not escape Windows-reserved names: `extract.rs:265` goes through `safe_join`,
  whose only refusals are absolute, `..`, prefix and empty (`security/path.rs:19-29`). An LZX entry
  named `AUX` or containing `:` is therefore not handled by the archive route the way
  `volume/write/copy.rs::host_target` handles it (not tested today; see "does not prove").

### A5. Recommended route, cost, oracle, fixtures

**Route: ART's own decoder, `core/archive/lzx.rs` behind `ArchiveBackend`** — no new crate.
Reasons, each from above: the trait requires a limit inside the decode loop and neither crate has
one (both allocate from header sizes, A2); merged groups need a one-pass `read_selected` like 7z,
which newtua's `read_entry` cannot give (per-member re-decode) and amiga-lzx's `next_entry` gives
without a list-only pass; newtua's date decode is wrong on real files; amiga-lzx's licence needs a
`deny.toml` change and is a same-day release of a one-author, self-described LLM-assisted crate;
`unlzx.c` has no stated licence from its author and is written to overrun buffers.

- **Cost:** the reference implementations measure the size: amiga-lzx's decode side
  (`decoder.rs`, `huffman/decode.rs`, `huffman/pretree.rs`, `bitio/reader.rs`, `archive/reader.rs`,
  `archive/datetime.rs`, `crc32.rs`, `constants.rs`) is **2 291 lines incl. tests**; newtua's
  `lzx.rs` is **1 702 lines incl. ~600 of tests**; `unlzx.c` is 1 374. Expect **~900–1 200 lines
  including tests** for container + codec + backend + detect arm, reusing ART's existing CRC-32 if
  one exists (not checked). Porting from XADMaster/newtua (LGPL) into GPL-3.0-or-later ART is
  licence-compatible, but writing from the format description and checking against the tools is
  cleaner provenance.
- **Named fallback if the plan wants less code:** `newtua-amiga` as a dependency (licence already
  allowed), wrapped: ART walks the headers itself first (the 31-byte walk in `lzxprobe` is ~20
  lines) to refuse oversized groups and to read the date big-endian, caps the archive file size it
  will read into memory, and accepts the per-member re-decode cost. It still needs CLAUDE.md's crate
  list, `THIRD_PARTY_LICENSES.md`, and two crates (`newtua-amiga`, `newtua-common`) audited.
- **Oracle (not ART's code), re-runnable:** `scripts/lzx-oracle-check.py DIR` in the shape of
  `iso-oracle-check.py` — for each `.lzx` in DIR, extract with ART (an `#[ignore]` hook or a small
  bin) and with `unar.exe -e ISO-8859-1` (`lsar -j` for the listing), compare names + SHA-256 per
  file, and count entries, groups, non-ASCII names. Needs `unar` — not in CI, like 7-Zip. Two
  XADMaster-lineage answers agreeing (newtua, unar) is one lineage; amiga-lzx (unlzx.c lineage)
  agreeing too is what made today's byte result a cross-check. The **archive's own stored CRC-32s**
  are a second, decoder-independent check and belong in ART's `read`.
- **Dates:** no oracle can settle the year field (A3). Recommend the plan does **not** carry LZX
  dates as a claim: the native PFS3 copy stamps "now" anyway and counts `dates_lost`
  (`native.rs:1024`, `:1084-1086`). If a date is carried (FFS, hst-imager `.uaem`), read it
  big-endian and say which year rule was applied.
- **Synthetic fixtures without Amiga software:** an encoder exists — `amiga-lzx`'s `ArchiveWriter`
  (and `amiga-lzx-cli`'s `lzx c`) — but using it in tests makes it a dev-dependency and needs WTFPL
  in `deny.toml`. Without it: the test writes a **stored-mode** archive by hand (10-byte header +
  31-byte records + name + bytes, including a merged group — newtua's `tests/lzx_oracle.rs`
  `LzxBuilder` is exactly this, ~60 lines) and a **method-2** stream built bit by bit (an LZX block
  of literals only, as newtua's `lzx.rs` unit tests do with a `BitWriter`). Both are generated at
  runtime; no copyrighted content ships. Real LZX-compressed material stays in the `#[ignore]` hook
  and the oracle script over the owner's files.

## Findings — B. Restoring Amiga names

### B1. How ART records an escaped name today — and the gap

- **Writer of the escape:** `core/volume/write/copy.rs:708-751` `windows_safe_name` (`<>:"/\|?*`
  and controls → `_`, trailing dots/spaces → `_`, the 22 DOS device stems get a leading `_`).
  `host_target` (`copy.rs:833-845`) pushes `format!("{name} → {safe}")` into
  `ExtractReport.renamed` (`copy.rs:794`). **It records the leaf only, as a display string** — not
  the directory it is in — so `renamed` alone cannot tell `content.rs` which host path to map.
  The escape is not injective (`A:B` and `A_B` both become `A_B`; the second then lands in
  `skipped` under `OverwritePolicy::Skip`, `copy.rs:863-868`).
- **Reader:** `core/preload/amiga_names.rs` (ART-160). `AmigaNames::read(source)`
  (`:83-95`) reads **only** `<source>/distribution.json` → `files[].path` / `files[].hostPath`, and
  builds a per-node map (`from_records`, `:103-122`, private). Absent or unparsable → empty map.
  The only producer of that manifest is `core/osinstall` (`osinstall/mod.rs:670`
  `host_destination`; `apply.rs:2915-2991` test).
- `.uaem` sidecars: written by `write_one_file` (`copy.rs:1015-1036`) **for files only** —
  `extract_dir` creates directories with no sidecar (`copy.rs:1107-1117`), so a drawer's own
  protection bits and date do not survive an HDF extraction.

### B2. Does the copy consult it?

- **Native PFS3/FFS:** yes, through `collect_entries` (`native.rs:842-846`) →
  `collect_into` (`:848-…`), which maps each host-relative path with `names.name_for` and builds
  `CopyEntry.relative` from the Amiga name (`:892-898`); `.uaem` files are skipped as entries
  (`:860-865`) and applied beside their file: `copy_in_pfs3` sets protection only
  (`update_dir_entry_protection`, `:1087-1089`) and counts comment/date as lost (`:1080-1086`) —
  the writer's entry date is `clock.amiga_now()` per entry (`:1024`); `copy_in_ffs` applies
  protection, comment and date (`:1189-1200`). `plan_copy` calls `collect_entries(source)` once per
  source root (`native.rs:340`).
- **hst-imager fallback:** it cannot rename, so it **refuses** when the map is non-empty —
  `tools/hst_imager.rs:270-288` → `CoreError::EscapedNamesNeedNativeCopy`, before the tool runs.
  `copy_args` (`hst_imager.rs:166-175`) passes no `--uaemetadata` option (Emu68-Imager passes
  `--uaemetadata UaeFsDb`, previous note § 7); what hst-imager does with `.uaem` files by default
  was not checked.
- **Ran:** `cargo test --lib escaped` (TMP/TEMP on E:) →
  `test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 3594 filtered out; finished in
  0.07s`, including `core::preload::native::tests::an_escaped_host_name_is_copied_under_its_amiga_name`
  (`native.rs:1238-1278`), `tools::hst_imager::tests::a_tree_with_escaped_names_is_refused_before_the_tool_runs`,
  `core::preload::amiga_names::tests::a_drawer_and_the_file_in_it_can_both_be_escaped` and
  `core::volume::write::copy::tests::an_amiga_name_ntfs_refuses_is_escaped`. The native test proves
  the **entry list**, not a written PFS3 volume read back; no test drives an HDF extraction's
  `renamed` into a copy.

### B3. What `content.rs` must write, and what is missing

Nothing reads `ExtractReport.renamed` today, and `AmigaNames` has exactly one input format. For
`AUX` to reach the card:

1. **The extraction must say where each escaped node is.** Either `host_target` records a
   structured pair (`amiga relative path`, `host relative path`) — `ExtractReport.renamed` is a
   `Vec<String>` of leaf display strings, serialised to the frontend (`#[derive(Serialize)]`,
   `copy.rs:787`), so a new field beside it rather than a changed one — or `content.rs` rebuilds the
   pairs by listing the volume again. The first is one change at the one place that escapes.
2. **The record must reach `collect_entries` for that source root, without landing on the card.**
   Writing `distribution.json` into staging would work for `AmigaNames::read` today, but
   `collect_entries` copies `distribution.json` as a real file (asserted at `native.rs:1275-1277`):
   a WHDLoad drawer would carry a stray manifest onto the card, and two staged sources in one
   partition would collide on it. The plan needs either (a) an ART-private record name excluded in
   `collect_into` like `BACKUP_DIR` (`native.rs:882`) and read by `AmigaNames::read` as a second
   input, or (b) an in-memory `AmigaNames` carried with the source in the copy step
   (`from_records` made `pub(crate)`). (b) only works if staging and the copy share a process run;
   the preload plan crosses the command boundary (`PreloadPartition.content`, previous note § 7), so
   (a) is the one that survives.
3. **Keep the hst-imager refusal fed by the same record.** `hst_imager.rs:279` calls
   `AmigaNames::read(source)`; if the new record is not read there, the fallback silently copies
   `_AUX`. This matters because boingbag1 and AmiKit-DevPack both carry non-ASCII names that force
   the fallback (ART-113): a source with **both** a non-ASCII name and an escaped name is refused by
   native (non-ASCII) **and** by hst-imager (escaped) — the refusal must name both causes.
4. **Refuse the non-injective case before staging.** Two Amiga names in one drawer that escape to
   the same host name cannot both be staged; today the second becomes a `skipped` line. The content
   step should refuse the HDF by name (or not claim it complete — `ExtractReport::is_complete`
   already returns false on `skipped`, `copy.rs:800-803`).
5. **Archive route (LZX/LHA/ZIP/7z) has the same need.** The archive gate does not escape at all
   (A4); if the plan escapes there too, it must write the same record.

## Eliminations

- *7-Zip reads Amiga LZX* — eliminated: `7z i` has no LZX handler; `7z l boingbag1.lzx` fails.
- *Python has an LZX package* — eliminated for the names tried (PyPI 404 ×4).
- *`unlzx.c` is a ready Windows oracle* — eliminated today: MSVC x64 build extracts with "crc bad"
  and segfaults (list mode works).
- *`unlzx.c` is public domain* — not established: the author's Aminet readme states no licence; the
  Unlicense is the GitHub fork's own file.
- *XADMaster/newtua dates are right* — eliminated: LE read gives month 16 / hour 29 on real files.
- *amiga-lzx's piecewise year is right for every archive* — eliminated: it gives 2030 for a 2024
  archive that `+1970` reads as 2024.
- *LZX archives contain directory records* — eliminated on 3 real archives (0 of 26 726 records).
- *`ExtractReport.renamed` is enough to restore names* — eliminated: leaf-only display strings.
- *Staging a `distribution.json` is harmless* — eliminated: `collect_entries` copies it to the card.
- *The archive gate escapes Windows-reserved names* — eliminated by reading `safe_join`'s error
  set; not run.

## What the plan must do

1. Add `core/archive/lzx.rs` — ART's own Amiga LZX decoder behind `ArchiveBackend`, with a `"lzx"`
   signature in `detect.rs` and an arm in `open_with_password`; no new crate (or, if the plan
   chooses less code, `newtua-amiga` wrapped as in A5 with CLAUDE.md + `THIRD_PARTY_LICENSES.md` in
   the same commit).
2. Override `read_selected` to decode each merged group once; refuse a group whose declared total
   exceeds the per-entry bound before allocating; never size a buffer from a header value; bound
   the loop by `limit`; check header CRC and data CRC.
3. Treat names as Latin-1 (byte → char); treat LZX as having no directory entries (synthesise
   drawers or let the gate create parents).
4. Decide the attribute question for the whole gate: `ArchiveEntry` has no protection/comment/date.
   Either extend it and write `.uaem` sidecars (so WiFi's `C/WirelessManager` keeps its bits and
   comment), or state in the report that archive content lands with default bits.
5. Do not claim LZX dates; if carried, BE and a named year rule.
6. Tests: hand-built stored archive with a merged group, hand-built method-2 literal block, CRC
   mismatch, truncated record, oversized group refused, limit honoured; `#[ignore]` hook over the
   owner's `boingbag1.lzx` and WiFi archive.
7. `scripts/lzx-oracle-check.py` — ART vs `unar -e ISO-8859-1`, names + SHA-256 + counts; not in
   CI.
8. Measure the LZX content with preload's rules: boingbag1 has 36 non-ASCII names and AmiKit-DevPack
   47 (→ ART-113 fallback), DevPack's longest name is 104 bytes (limit 106).
9. Make escaped names travel: a structured `(amiga path, host path)` record from `host_target`
   (new field beside `renamed`), written by `content.rs` into staging under an ART-private name that
   `collect_into` excludes and `AmigaNames::read` reads — for the native copy **and** the hst-imager
   refusal.
10. Refuse, by name, an HDF whose names collide after escaping, and a source that is both
    non-ASCII (native refuses) and escaped (hst refuses).
11. Write `.uaem` for directories in `extract_dir` if drawer bits are to survive, or record that
    they do not.

## What this does not prove

- Byte agreement is on **two** archives (1 052 entries); AmiKit-DevPack (25 674 entries, 767-file
  group) was only listed, not decoded, so neither time nor memory for large groups is measured.
- Two of the three agreeing decoders share XADMaster's lineage; the third (amiga-lzx) says it was
  written from `unlzx.c` documentation. No Amiga-side LZX was run.
- The date finding is three entries read by hand against plausibility (release date, file era),
  not against a known-dated archive of the WiFi archive's writer.
- Name restoration was traced by reading and by a list-level test; no PFS3 volume holding an
  escaped name was written and read back, and no WHDLoad HDF in the owner's material was shown to
  contain an escaped name (not searched this round).
- hst-imager's handling of `.uaem` files without `--uaemetadata` was not checked.
- Whether the archive gate fails on an entry named `AUX` or `a:b` was read from `safe_join`, not run.
