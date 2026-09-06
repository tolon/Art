# Media identification by hash — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** ART identifies an install disk by what it *is* — a content hash looked up in a table of known media — additively to the volume name it already reads, and says where every claim came from.

**Architecture:** A compiled-in data table (186 rows, adopted from Emu68 Hatcher, MIT) plus a pure lookup in `core/osinstall/`. MD5 is added to `core/hashing.rs` for table lookup only; SHA256 stays ART's integrity hash. Results are cached against the `MediaIdentity` `scan_cache.rs` already keys on, never recomputed per render. Nothing fetches at runtime.

**Tech Stack:** Rust (`core/osinstall`, `core/hashing`), `serde_json`, `include_str!`; TypeScript + react-i18next on the frontend.

**Spec:** `docs/superpowers/specs/2026-09-06-media-identification-by-hash-design.md` — read it; this plan argues from it and conflicts resolve against it.

## Global Constraints

- `src-tauri/src/core/` is platform-independent: no `tauri`, no Windows APIs, **no network**. `commands/*.rs` are thin adapters. A lower-level `core/` module must not import a higher-level one.
- **Nothing fetches at runtime.** The table is compiled in. No URL, no download button, no "update the table" action.
- MD5 is used for **table lookup only**. SHA256 remains the integrity hash for verification, duplicates and snapshots.
- **A match is a claim about the row, not about the disk**: ART reports what the row says and that the row is Hatcher's. **A non-match says nothing about identity** and must never weaken what the volume name established.
- **A row's `volume` is NOT the disk's AmigaDOS volume name, and the two must never be compared.** Measured 2026-09-06 against the owner's own 3.2 media: **0 of 12 matched.** The row says `Backdrops3_2`, `LocaleDE3_2`, `DiskDoctor3_2`; the disk's own root block says `Backdrops3.2`, `Locale-DE`, `DiskDoctor`. They are different namespaces — Hatcher's internal identifier versus what AmigaDOS wrote. Task 3 considered cross-checking them as a "conflicting" test and correctly did not; had it, **all 35 of the owner's good disks would have been reported in conflict**. So: never join hash results to name results on that field, never show a row's `volume` as the disk's name, and where both are known, show them as two facts from two sources rather than one reconciled answer.
- **The mapping is many-to-one**: one logical disk has many hashes (`Workbench3_1` has 11). Never index disk → hash.
- **Never re-derive a version from the row.** The source's own `version` and `source` disagree for the Hotfix Pack (28 rows tagged `3.2.2`, 1 tagged `3.2.2.1`, all from the same pack). Report both fields as stated.
- Hashing is not free — `scan_cache.rs` records that hashing 468 MB costs about what the walk costs. Compute once per `MediaIdentity`, cache, never per render.
- New commands go in **both** `lib.rs`'s `invoke_handler![]` and a typed wrapper in `src/lib/*.ts`. Components never call `invoke`.
- i18n: both `en.json` and `tr.json` in the same commit; Turkish a real translation. `src/lib/*` returns `Phrase`, the component calls `t()`; every variant enumerated in `phrase-keys.test.ts`.
- **A test is not a guard until the defect has been put back and seen to fail it.** Quote the actual failure message. Never assert a state with more than one cause.
- Test scratch removes itself on `Drop` (`core::ScratchDir`) and its name must be unique **within one process** (a pid is shared across cargo's parallel test threads) — use a process-wide atomic counter.

---

### Task 1: MD5, and a module doc that becomes true

**Files:**
- Modify: `src-tauri/src/core/hashing.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `THIRD_PARTY_LICENSES.md`

`core/hashing.rs`'s module doc already says *"MD5 is available only for compatibility with historical databases — never as a security primitive."* **There is no MD5 in ART** — no function, no crate. Verify that yourself first (`grep -rn md5 src-tauri/src`, and the `Cargo.toml`), then make the sentence true.

- [ ] **Step 1: Confirm the gap.** Report what you found before changing anything.
- [ ] **Step 2: Add the dependency.** A maintained, pure-Rust MD5 crate from the `RustCrypto` family, pinned exactly the way this project's other deps are. Add it to `THIRD_PARTY_LICENSES.md` **in the same commit** — that rule is in CLAUDE.md and is not optional. Check `deny.toml` accepts the licence.
- [ ] **Step 3: Write the failing tests.** `md5_file` (streaming, mirroring `sha256_file`'s chunking so a large image does not blow up memory) and `md5_bytes`. Pin at least one **published** reference value from RFC 1321 so the test cannot pass against a wrong implementation of ART's own making — and one real value: `Workbench3.2.adf` is `5edf0b7a10409ef992ea351565ef8b6c`, but do **not** ship that ADF; test it in the `#[ignore]`d hook of Task 3 instead.
- [ ] **Step 4: Implement.** Follow `sha256_file`'s existing shape exactly.
- [ ] **Step 5: Sharpen the doc.** It should now say what MD5 *is* for here — a historical-database key — and that it is not an integrity hash and must not be used as one.
- [ ] **Step 6: Mutate.** Break the chunking (e.g. hash only the first chunk) and confirm a test fails on a large-input case. Quote the message. If no test covers it, that is the test you are missing.
- [ ] **Step 7: `cargo test`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo deny check`. Commit.**

---

### Task 2: The table, and a lookup that respects it

**Files:**
- Create: `src-tauri/src/core/osinstall/mediahash.rs`
- Modify: `src-tauri/src/core/osinstall/mod.rs`
- Already present: `src-tauri/src/core/osinstall/media_hashes.json` (186 rows, generated 2026-09-06; **do not regenerate it**)

**Interfaces produced** (later tasks consume these — keep the names):

```rust
pub struct MediaRow { pub md5: String, pub version: String, pub volume: String,
                      pub name: String, pub source: String, pub sequence: Option<u32> }
pub fn row_for(md5: &str) -> CoreResult<Option<&'static MediaRow>>;
pub fn rows() -> CoreResult<&'static [MediaRow]>;
```

**Corrected 2026-09-06, and the error was mine.** The first version of this
block returned bare `Option` / `&[MediaRow]`, which makes a parse failure
impossible to report and forces a `panic!`. `core/osinstall/recipe.rs:636`
records this project having tried exactly that and **reversed it**: *"The first
version reached for `panic!`/`expect` on a shipped recipe that would not
parse… That reasoning does not survive the release profile: `panic = "abort"`
means an abort here takes the whole application down."* `core/distro::profiles`
returns `CoreResult` for the same reason. Shipped data being wrong is still a
bug; it must be a bug that produces a refusal a user can read and a process
that is still running.

Cache the parse **result**, not just the success, so a broken table is reported
identically on the first call and the thousandth. `CoreError` is not `Clone`,
so hold the failure as its own text and rebuild a `CoreError::Malformed` from
it — `recipe.rs`'s doc describes this exact arrangement; copy it.

- [ ] **Step 1: Read the JSON's `$comment` field first.** It states the table's provenance and the two traps (many-to-one; do not re-derive a version). Then read the spec's §2.2.
- [ ] **Step 2: Write the failing invariant tests** — these guard the *data*, which is what drifts:
  - exactly **186** rows;
  - **186 distinct** `md5` values;
  - every `md5` is 32 lowercase hex characters;
  - `version`, `volume`, `name`, `source` all non-empty on every row;
  - the many-to-one property is real and asserted, not assumed: `Workbench3_1` resolves to **11** distinct hashes, `Install3_1` 11, `Locale3_1` 10, `Storage3_1` 10, `Extras3_1` 9, `Fonts3_1` 6;
  - the Hotfix Pack disagreement is pinned so nobody "tidies" it: source `Hyperion (3.2.2.1 Hotfix Pack)` has **29** rows, **28** tagged version `3.2.2` and **1** tagged `3.2.2.1`.
- [ ] **Step 3: Load it** with `include_str!` + `serde_json`, parsed once (`OnceLock`), the way `core/distro/`'s registry already does. Read that first and copy its shape.
- [ ] **Step 4: `row_for`** — case-insensitive on the input (a user's tool may produce uppercase hex), exact on the stored value. Test both cases and a hash that is in no row.
- [ ] **Step 5: Mutate.** (a) Delete a row and confirm the count test fails. (b) Duplicate a hash and confirm the distinctness test fails. (c) Make `row_for` case-sensitive and confirm an uppercase lookup test fails. Quote all three messages.
- [ ] **Step 6: Commit.**

---

### Task 3: Prove the table against real material, on both sides

**Files:**
- Create: `scripts/media-table-check.py`
- Modify: `src-tauri/src/core/osinstall/mediahash.rs` (an `#[ignore]`d test)
- Modify: `docs/STATUS.md` (the reproduce block)

The spec's §4.2. This is the sibling of `scripts/rom-table-check.py`; read that script before writing this one and follow its shape and its output style.

- [ ] **Step 1: The script.** Takes a directory, hashes every `.adf`/`.iso`/`.lha` under it, and reports per row: **verified** (a file here matches it), **unverified** (no file here does), **conflicting** (a file matches but something disagrees). Print counts, then the detail. It must never write to the directory it is given.
- [ ] **Step 2: Run it against the owner's own media** at `E:\amiga\Amigatolon\paketler\3.2\AmigaOs 3.2\ADF`. The standing measurement is **35 of 35 matched, zero misses** — if you get a different number, that is a finding, not something to adjust the script until it agrees.
- [ ] **Step 3: The Rust twin.** An `#[ignore]`d test taking a directory from an env var (`ART_MEDIA_DIR`), doing the same lookup through `core`'s own code, so the check exists on both sides. Follow `round_trip_every_icon_in_a_folder_when_asked`'s shape.
- [ ] **Step 4: Not in CI.** It needs media ART must never ship. Add both command lines to STATUS.md's reproduce block, next to the other real-material hooks.
- [ ] **Step 5: Commit.**

---

### Task 4: Identify a folder, once, and remember it

**Files:**
- Modify: `src-tauri/src/core/osinstall/scan_cache.rs`
- Modify: `src-tauri/src/core/osinstall/mediahash.rs`
- Modify: `src-tauri/src/commands/osinstall.rs`, `src-tauri/src/lib.rs`
- Modify: `src/lib/osinstall.ts`

**Interfaces produced:**

```rust
pub struct MediaMatch { pub path: PathBuf, pub volume_name: Option<String>,
                        pub row: Option<MediaRow>, pub md5: String }
```

- [ ] **Step 1: Read `scan_cache.rs` in full first**, especially its doc on why change detection is `(path, size, mtime)` and **not** a hash. That refusal stands; you are adding a hash *result* cached against that identity, not replacing the identity.
- [ ] **Step 2: Extend the cache** so a computed MD5 is stored against a `MediaIdentity` and reused. One producer, not a second cache — ART-249 is the recorded cost of two copies of one job.
- [ ] **Step 3: The command.** `osinstall_identify_media(folder) -> Vec<MediaMatch>`, run through `core/jobs` with a `ProgressSink` because hashing a folder of ADFs is long (§54/§55) — and **the progress total must be real**: a fixed-width bar with an unknown total is the defect CLAUDE.md names. Check `is_cancelled()` between whole files, never mid-read.
- [ ] **Step 4: Register** in `lib.rs`'s `invoke_handler![]` and add the typed wrapper in `src/lib/osinstall.ts`. Add a test asserting the serialized key names the frontend declares, the way `apply_outcome_serializes_with_the_keys_the_frontend_declares` does.
- [ ] **Step 5: Tests.** A folder with a known-good synthetic file (generated at runtime in a tempdir — ART ships no copyrighted content), a folder with an unreadable file, cancellation mid-folder leaving nothing half-written, and a second call using the cache rather than re-hashing (assert the counted difference, not an impression).
- [ ] **Step 6: Mutate.** Remove the cache reuse and confirm the counted test fails. Remove the cancel check and confirm the cancellation test fails. Quote both.
- [ ] **Step 7: Commit.**

---

### Task 5: What the screen says, and what it refuses to say

**Files:**
- Modify: `src/lib/osinstall.ts` (a `Phrase`-returning helper)
- Modify: `src/components/osbuilder/OsInstall.tsx`
- Modify: `src/i18n/en.json`, `src/i18n/tr.json`, `src/i18n/phrase-keys.test.ts`

The spec's §4.3 is binding here and is the whole point of the round. Read it before writing a single string.

- [ ] **Step 1: The sentences.** At minimum, four distinct endings, never collapsed:
  - **matched, and confirmed on this machine** — name the row and say the owner's own media confirmed it;
  - **matched, unconfirmed** — name the row and say so; 151 of 186 rows are unconfirmed and the screen must not present all rows with equal confidence;
  - **not in the table** — the file is fine and simply unknown. This must read as *"not in the table"*, never as *"not genuine"*. A re-imaged disk is a legitimate miss.
  - **not hashed yet** — different from all three above.
- [ ] **Step 2: Attribution is in the sentence, not a footnote.** A match says the claim is Hatcher's table's, not ART's.
- [ ] **Step 3: Additive, never subtractive.** A miss must not remove or weaken what the volume name already said. Write the test that proves this before the code.
- [ ] **Step 4: Both catalogues, Turkish a real translation.** Enumerate every variant in `phrase-keys.test.ts`.
- [ ] **Step 5: Mutate.** Collapse "unconfirmed" into "confirmed" and confirm a test fails on the specific sentence. Make a miss weaken the name-based result and confirm that test fails. Quote both.
- [ ] **Step 6: `pnpm test`, `pnpm lint`. Commit.**

---

### Task 6: Land it

- [ ] **Step 1: The suites.** `pnpm test`, `pnpm lint` (**unpiped** — a pipe reports `tail`'s status, not `tsc`'s), `cd src-tauri && cargo test` quoting the **lib target's** summary line, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo deny check`.
- [ ] **Step 2: The sweeps.** `control-byte-sweep.py`, `scratch-root-sweep.py`, `contrast-check.py --quiet`.
- [ ] **Step 3: The documents.** `docs/session-log.md` (a row at the top), `docs/STATUS.md` (snapshot numbers, and the "Picking up next session" block **updated in place** — never stacked), `docs/FEATURES.md` (a row per built thing, and **only if a test exists**; the table's header is four cells, so make yours four and escape any `|` inside a cell), `CHANGELOG.md`.
- [ ] **Step 4: Record what the round measured**, not only what it built: 35 of 35 of the owner's own disks matched, which is what reversed the design. A future reader should meet that here rather than rediscover it.
- [ ] **Step 5: Stop.** Do **not** merge or push.
