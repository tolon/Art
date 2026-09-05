# Prefs and Wallpaper Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A distribution tree ART builds carries the owner's own wallpaper, screen mode and shell defaults, written into the release's own prefs files without regenerating them.

**Architecture:** Three new pure-`core` modules — `core/amigaprefs` (an IFF `FORM PREF` container plus the `PTRN` and `SCRM` chunks), `core/ilbm` (an ILBM encoder taking RGB8 pixels), `core/picture` (PNG/JPEG decode, scale and median-cut quantisation) — joined by an applier that places the picture and edits the tree's prefs in place, a verify check, a command layer and one OS Builder panel.

**Tech Stack:** Rust (`std` + `serde` + existing core deps, plus `png` and `jpeg-decoder`), React + `react-i18next`, Vitest, Python for the ffmpeg oracle.

**Spec:** [docs/superpowers/specs/2026-09-05-prefs-and-wallpaper-design.md](../specs/2026-09-05-prefs-and-wallpaper-design.md)

## Global Constraints

- **`core/` stays platform-independent.** No `use tauri`, no Windows API, no network. `png` and `jpeg-decoder` are the only new dependencies and both are pure-Rust read-only decoders; add each to `THIRD_PARTY_LICENSES.md` **in the same commit** that adds it to `Cargo.toml`.
- **MSRV 1.93.** `cargo clippy --all-targets -- -D warnings` is blocking; `lib.rs` allows only `dead_code`.
- **Bounds before indexing.** The release profile sets `panic = "abort"`. Every offset is computed with `checked_add` / `checked_mul` and validated against the real buffer length before slicing. A file that does not parse is refused with `CoreError`, never read past its bounds and never rewritten best-effort. Copy the discipline documented at the top of `src-tauri/src/core/amigaicon/mod.rs`.
- **Edit in place, never regenerate** (§39/§40). Chunks this code does not recognise, the `PRHD`, and `PTRN` chunks the user did not set pass through **byte-for-byte**.
- **Every write through `core/safety`.** `guarded_write(path, bytes, BackupPolicy::CONFIG)` — never `std::fs::write` on a user file.
- **ART ships no copyrighted Amiga content.** Unit-test fixtures are **synthetic and built at runtime**. The real-file checks are `#[ignore]`d, env-gated hooks.
- **Test scratch is `core::ScratchDir`**, which removes itself on `Drop`. Its name must be unique **within one process** — `ScratchDir::new(prefix, tag)` already uses a process-wide counter. Never write a trailing `remove_dir_all`.
- **Strings.** Rust-side `CoreError` messages stay English (ART-060). Every new UI string goes in **both** `src/i18n/en.json` and `tr.json` in the same commit.
- **Measured constants only.** Every byte offset below was measured from a real file in the spec's §1. Do not "correct" one from memory.

---

## File structure

| File | Responsibility |
|---|---|
| `src-tauri/src/core/amigaprefs/mod.rs` | module doc, re-exports, shared `CoreResult` helpers |
| `src-tauri/src/core/amigaprefs/iff.rs` | `FORM`…`PREF` container: index chunks, replace one, serialise |
| `src-tauri/src/core/amigaprefs/wbpattern.rs` | the `PTRN` chunk — read and write a `Backdrop` |
| `src-tauri/src/core/amigaprefs/screenmode.rs` | the `SCRM` chunk |
| `src-tauri/src/core/amigaprefs/env.rs` | `Prefs/Env-Archive/<name>` plain files |
| `src-tauri/src/core/ilbm/mod.rs` | ILBM encoder: `BMHD`, `CMAP`, ByteRun1 `BODY` |
| `src-tauri/src/core/ilbm/packbits.rs` | ByteRun1 (PackBits) row compressor |
| `src-tauri/src/core/picture/mod.rs` | decode PNG/JPEG to RGB8, scale, quantise |
| `src-tauri/src/core/picture/quantise.rs` | median-cut palette reduction |
| `src-tauri/src/core/appearance/mod.rs` | the applier: place the picture, edit the tree's prefs |
| `src-tauri/src/core/osinstall/verify.rs` | **modify** — add the "every prefs path resolves" check |
| `src-tauri/src/commands/appearance.rs` | thin adapter |
| `src/lib/appearance.ts` | typed `invoke` wrapper |
| `src/components/osbuilder/AppearancePanel.tsx` | the Görünüm panel |
| `scripts/ilbm-oracle-check.py` | ART writes, ffmpeg reads, pixels compared |

**Reachability.** The WHDLoad-drawer round shipped two features whose producers had no caller while every test was green. Tasks 5 and 6 build primitives; **Task 7 owns their first real caller**, Task 9 owns the command, and Task 10 owns the frontend call. When Task 10 is done, trace the chain from the panel's Save button to `core::ilbm::encode` hop by hop and record the hops in the task's commit message.

---

### Task 1: The `FORM PREF` container

**Files:**
- Create: `src-tauri/src/core/amigaprefs/mod.rs`
- Create: `src-tauri/src/core/amigaprefs/iff.rs`
- Modify: `src-tauri/src/core/mod.rs` — add `pub mod amigaprefs;`

**Interfaces:**
- Consumes: `crate::core::error::{CoreError, CoreResult}`
- Produces:
  - `pub struct PrefsFile { bytes: Vec<u8>, chunks: Vec<ChunkSpan> }`
  - `pub struct ChunkSpan { pub id: [u8; 4], pub body: std::ops::Range<usize> }`
  - `pub fn parse(bytes: &[u8]) -> CoreResult<PrefsFile>`
  - `impl PrefsFile { pub fn chunks(&self) -> &[ChunkSpan]; pub fn body(&self, i: usize) -> CoreResult<&[u8]>; pub fn replace_bodies(&self, edits: &[(usize, Vec<u8>)]) -> CoreResult<Vec<u8>>; pub fn to_bytes(&self) -> Vec<u8> }`

  `body` returns a `CoreResult` rather than panicking on an out-of-range index — decided during Task 1's review, because `panic = "abort"` turns a caller's off-by-one into an application kill and Tasks 7 and 8 call it with computed indices. Callers use `?`.

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/src/core/amigaprefs/iff.rs` with only the test module and a `tests_support` builder. The builder is how every later task makes a fixture without shipping Amiga content.

```rust
#[cfg(test)]
pub(crate) mod tests_support {
    /// Build a `FORM`…`PREF` by hand: a 6-byte `PRHD` then the given chunks.
    /// Odd-sized chunk bodies get the IFF pad byte, which is *not* counted in
    /// the chunk's own size field — the trap this builder exists to reproduce.
    pub(crate) fn synthetic_prefs(chunks: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut content = Vec::new();
        content.extend_from_slice(b"PREF");
        content.extend_from_slice(b"PRHD");
        content.extend_from_slice(&6u32.to_be_bytes());
        content.extend_from_slice(&[0u8; 6]);
        for (id, body) in chunks {
            content.extend_from_slice(id);
            content.extend_from_slice(&(body.len() as u32).to_be_bytes());
            content.extend_from_slice(body);
            if body.len() % 2 == 1 {
                content.push(0);
            }
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"FORM");
        out.extend_from_slice(&(content.len() as u32).to_be_bytes());
        out.extend_from_slice(&content);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tests_support::synthetic_prefs;

    #[test]
    fn a_prefs_file_round_trips_byte_for_byte() {
        let bytes = synthetic_prefs(&[(*b"PTRN", vec![1, 2, 3, 4]), (*b"SCRM", vec![9; 28])]);
        let parsed = parse(&bytes).unwrap();
        assert_eq!(parsed.to_bytes(), bytes);
    }

    #[test]
    fn every_chunk_is_indexed_including_the_header() {
        let bytes = synthetic_prefs(&[(*b"PTRN", vec![7; 24])]);
        let parsed = parse(&bytes).unwrap();
        let ids: Vec<[u8; 4]> = parsed.chunks().iter().map(|c| c.id).collect();
        assert_eq!(ids, vec![*b"PRHD", *b"PTRN"]);
        assert_eq!(parsed.body(1), &[7u8; 24]);
    }

    #[test]
    fn replacing_one_body_leaves_every_other_chunk_untouched() {
        // `synthetic_prefs` prepends PRHD, so the indices are
        // PRHD=0, PTRN=1, XXXX=2, PTRN=3. Replace the *second* PTRN.
        let bytes = synthetic_prefs(&[
            (*b"PTRN", vec![1; 24]),
            (*b"XXXX", vec![0xAB; 10]),
            (*b"PTRN", vec![2; 24]),
        ]);
        let parsed = parse(&bytes).unwrap();
        let out = parsed.replace_bodies(&[(3, vec![5; 40])]).unwrap();
        let after = parse(&out).unwrap();
        assert_eq!(after.body(0), &[0u8; 6], "PRHD must survive verbatim");
        assert_eq!(after.body(1), &[1u8; 24], "the first PTRN must survive verbatim");
        assert_eq!(after.body(2), &[0xABu8; 10], "the unknown chunk must survive verbatim");
        assert_eq!(after.body(3), &[5u8; 40]);
    }

    #[test]
    fn an_odd_sized_body_is_padded_but_the_size_field_is_not() {
        let bytes = synthetic_prefs(&[(*b"PTRN", vec![1; 24])]);
        let parsed = parse(&bytes).unwrap();
        let out = parsed.replace_bodies(&[(1, vec![3; 5])]).unwrap();
        let after = parse(&out).unwrap();
        assert_eq!(after.body(1).len(), 5, "the size field states the real length");
        assert_eq!(out.len() % 2, 0, "the file itself stays word-aligned");
    }

    #[test]
    fn a_file_that_is_not_a_form_pref_is_refused() {
        assert!(parse(b"FORM\x00\x00\x00\x04ILBM").is_err());
        assert!(parse(b"NOPE\x00\x00\x00\x04PREF").is_err());
        assert!(parse(b"FORM").is_err());
    }

    #[test]
    fn a_chunk_running_past_the_end_is_refused_not_clamped() {
        let mut bytes = synthetic_prefs(&[(*b"PTRN", vec![1; 24])]);
        let last = bytes.len();
        // the PTRN size field is the four bytes before its 24-byte body
        bytes[last - 28..last - 24].copy_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
        let err = parse(&bytes).unwrap_err();
        assert!(
            format!("{err}").contains("chunk"),
            "the refusal must name the chunk, got: {err}"
        );
    }

    #[test]
    fn a_form_size_larger_than_the_file_is_refused() {
        let mut bytes = synthetic_prefs(&[(*b"PTRN", vec![1; 24])]);
        bytes[4..8].copy_from_slice(&0x7FFF_FFFFu32.to_be_bytes());
        assert!(parse(&bytes).is_err());
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test amigaprefs::iff`
Expected: FAIL to compile — `parse`, `PrefsFile`, `ChunkSpan` do not exist.

- [ ] **Step 3: Write the implementation**

Add above the test module in `iff.rs`:

```rust
//! The IFF `FORM`…`PREF` container every AmigaOS preferences file uses.
//!
//! Measured against real files rather than recalled: `WBPattern.prefs` from
//! ART's own built AmigaOS 3.2 tree and from 3.9 are byte-identical at 462
//! bytes, and both open `FORM` / size / `PREF` / `PRHD` / 6 / six zero bytes
//! before the first payload chunk (design doc §1.1).
//!
//! This module never rewrites a chunk it was not asked about and never
//! reorders one. That is not tidiness: a release's `WBPattern.prefs` carries
//! three `PTRN` chunks and a user who sets only the root backdrop must keep
//! the other two exactly as the release shipped them (§39/§40).

use crate::core::error::{CoreError, CoreResult};
use std::ops::Range;

/// Where one chunk's body lives inside the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkSpan {
    pub id: [u8; 4],
    pub body: Range<usize>,
}

/// A parsed preferences file. Holds the original bytes so anything this
/// module does not understand can be handed back untouched.
#[derive(Debug, Clone)]
pub struct PrefsFile {
    bytes: Vec<u8>,
    chunks: Vec<ChunkSpan>,
}

fn malformed(detail: &str) -> CoreError {
    CoreError::Malformed {
        format: "IFF PREF".to_string(),
        detail: detail.to_string(),
    }
}

fn be_u32(bytes: &[u8], at: usize) -> CoreResult<u32> {
    let end = at.checked_add(4).ok_or_else(|| malformed("offset overflow"))?;
    let slice = bytes
        .get(at..end)
        .ok_or_else(|| malformed("a length field runs past the end of the file"))?;
    Ok(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

/// Parse a `FORM`…`PREF`, indexing every chunk including `PRHD`.
pub fn parse(bytes: &[u8]) -> CoreResult<PrefsFile> {
    if bytes.len() < 12 {
        return Err(malformed("shorter than an IFF FORM header"));
    }
    if &bytes[0..4] != b"FORM" {
        return Err(malformed("does not begin with FORM"));
    }
    let form_size = be_u32(bytes, 4)? as usize;
    let form_end = form_size
        .checked_add(8)
        .ok_or_else(|| malformed("FORM size overflows"))?;
    if form_end > bytes.len() {
        return Err(malformed("the FORM size is larger than the file"));
    }
    if &bytes[8..12] != b"PREF" {
        return Err(malformed("the FORM type is not PREF"));
    }

    let mut chunks = Vec::new();
    let mut at = 12usize;
    while at < form_end {
        let header_end = at
            .checked_add(8)
            .ok_or_else(|| malformed("chunk header overflows"))?;
        if header_end > form_end {
            return Err(malformed("a chunk header runs past the end of the FORM"));
        }
        let mut id = [0u8; 4];
        id.copy_from_slice(&bytes[at..at + 4]);
        let size = be_u32(bytes, at + 4)? as usize;
        let body_start = header_end;
        let body_end = body_start
            .checked_add(size)
            .ok_or_else(|| malformed("chunk size overflows"))?;
        if body_end > form_end {
            return Err(malformed(
                "a chunk claims more bytes than the FORM contains",
            ));
        }
        chunks.push(ChunkSpan {
            id,
            body: body_start..body_end,
        });
        // IFF pads an odd-sized body with one byte that the size field
        // does not count.
        at = body_end + (size & 1);
    }

    Ok(PrefsFile {
        bytes: bytes.to_vec(),
        chunks,
    })
}

impl PrefsFile {
    pub fn chunks(&self) -> &[ChunkSpan] {
        &self.chunks
    }

    /// The body of chunk `index`. Panics only on an index this file never
    /// produced, which is a programming error rather than bad input.
    pub fn body(&self, index: usize) -> &[u8] {
        &self.bytes[self.chunks[index].body.clone()]
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    /// Rebuild the file with the named chunks' bodies replaced. Every other
    /// chunk — `PRHD`, an unrecognised one, a `PTRN` the caller did not
    /// name — is copied verbatim, in its original order.
    pub fn replace_bodies(&self, edits: &[(usize, Vec<u8>)]) -> CoreResult<Vec<u8>> {
        for (index, _) in edits {
            if *index >= self.chunks.len() {
                return Err(malformed("edit names a chunk this file does not have"));
            }
        }
        let mut content = Vec::new();
        content.extend_from_slice(b"PREF");
        for (index, span) in self.chunks.iter().enumerate() {
            let body: &[u8] = match edits.iter().find(|(i, _)| *i == index) {
                Some((_, replacement)) => replacement,
                None => &self.bytes[span.body.clone()],
            };
            let size = u32::try_from(body.len())
                .map_err(|_| malformed("a replacement body does not fit an IFF chunk"))?;
            content.extend_from_slice(&span.id);
            content.extend_from_slice(&size.to_be_bytes());
            content.extend_from_slice(body);
            if body.len() % 2 == 1 {
                content.push(0);
            }
        }
        let form_size = u32::try_from(content.len())
            .map_err(|_| malformed("the rebuilt FORM does not fit an IFF size field"))?;
        let mut out = Vec::with_capacity(content.len() + 8);
        out.extend_from_slice(b"FORM");
        out.extend_from_slice(&form_size.to_be_bytes());
        out.extend_from_slice(&content);
        Ok(out)
    }
}
```

Create `src-tauri/src/core/amigaprefs/mod.rs`:

```rust
//! AmigaOS preferences files — the IFF `FORM PREF` container and the chunks
//! ART writes into it.
//!
//! Every byte offset in this module tree was measured from a real file on
//! the owner's own material; see
//! `docs/superpowers/specs/2026-09-05-prefs-and-wallpaper-design.md` §1 for
//! the dumps and the commands that reproduce them. Do not "correct" one of
//! them from memory.

pub mod env;
pub mod iff;
pub mod screenmode;
pub mod wbpattern;
```

For this task, create `env.rs`, `screenmode.rs` and `wbpattern.rs` as empty files containing only a `//! placeholder — see Task N` doc comment so the module compiles; each later task replaces its own.

Add to `src-tauri/src/core/mod.rs`, in the existing alphabetical `pub mod` list, between `amiganet` (line 12) and `amigaver` (line 13):

```rust
pub mod amigaprefs;
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test amigaprefs::iff`
Expected: PASS, 7 tests.

- [ ] **Step 5: Mutate the guards and report what falls**

Run each, confirm the named test fails, then restore the file with `shutil.copyfile` (**not** `shutil.move` — it loses the mtime and cargo then rebuilds nothing) or `touch` it afterwards.

| Mutation | Must fail |
|---|---|
| in `replace_bodies`, always use the replacement for every chunk | `replacing_one_body_leaves_every_other_chunk_untouched` |
| drop the `at = body_end + (size & 1)` pad, use `at = body_end` | `an_odd_sized_body_is_padded_but_the_size_field_is_not` |
| change `if body_end > form_end` to `let body_end = body_end.min(form_end)` | `a_chunk_running_past_the_end_is_refused_not_clamped` |
| remove the `&bytes[8..12] != b"PREF"` check | `a_file_that_is_not_a_form_pref_is_refused` |

Record every survivor in the commit message, and for each ask the 2026-08-24 question first: is the guard weak, or was the mutation wrong for it?

- [ ] **Step 6: Commit**

```bash
cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings
cd .. && git add src-tauri/src/core/amigaprefs src-tauri/src/core/mod.rs
git commit -F <a message file>
```

---

### Task 2: The `PTRN` chunk — backdrops

**Files:**
- Modify: `src-tauri/src/core/amigaprefs/wbpattern.rs` (replaces the Task 1 placeholder)

**Interfaces:**
- Consumes: `super::iff::{parse, PrefsFile, ChunkSpan}`
- Produces:
  - `pub enum Which { Root, Drawer, Screen }` — serialising as `0`, `1`, `2`
  - `pub enum Placement { Tile, Center, Scale, ScaleGood }`
  - `pub enum Precision { Default, Icon, Image, Exact }`
  - `pub enum Dither { Default, Bad, Good, Best }`
  - `pub enum Content { Picture(String), Pattern { depth: u8, planes: Vec<u8> } }`
  - `pub struct Backdrop { pub which: Which, pub placement: Placement, pub precision: Precision, pub dither: Dither, pub no_remap: bool, pub content: Content }`
  - `pub fn read_backdrop(body: &[u8]) -> CoreResult<Backdrop>`
  - `pub fn write_backdrop(b: &Backdrop) -> CoreResult<Vec<u8>>`
  - `pub const DEFAULT_PICTURE_FLAGS: u16 = 0x2A00;`

- [ ] **Step 1: Write the failing tests**

The two fixtures below are the **measured** ones from design doc §1.1 — the release's own root backdrop and the `Christmas` preset's screen pattern. They are built here, not shipped.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// The root `PTRN` of the `WBPattern.prefs` in ART's own built AmigaOS
    /// 3.2 tree: 16 reserved zero bytes, Which=0, Flags=0x2A00, Revision=0,
    /// Depth=0, DataLength=43, then the path — **with no terminator**. The
    /// trailing `00` in a hex dump of that file is the IFF pad byte for an
    /// odd chunk size (67), not part of the data.
    fn measured_picture_body() -> Vec<u8> {
        let path = b"Sys:Prefs/Presets/Backdrops/default_pal.iff";
        assert_eq!(path.len(), 43, "the measured DataLength is 43");
        let mut body = vec![0u8; 16];
        body.extend_from_slice(&0u16.to_be_bytes()); // Which = root
        body.extend_from_slice(&0x2A00u16.to_be_bytes()); // Flags
        body.push(0); // Revision
        body.push(0); // Depth
        body.extend_from_slice(&(path.len() as u16).to_be_bytes());
        body.extend_from_slice(path);
        body
    }

    /// The `Christmas` preset's screen `PTRN`: WBPF_PATTERN set, Depth=3,
    /// DataLength=0x60=96, which is 16x16 bits times three planes.
    fn measured_pattern_body() -> Vec<u8> {
        let mut body = vec![0u8; 16];
        body.extend_from_slice(&2u16.to_be_bytes()); // Which = screen
        body.extend_from_slice(&0x0001u16.to_be_bytes()); // WBPF_PATTERN
        body.push(0);
        body.push(3); // Depth
        body.extend_from_slice(&96u16.to_be_bytes());
        body.extend(std::iter::repeat(0xA5u8).take(96));
        body
    }

    /// The **release's own** screen `PTRN`, from the same 3.2 file as
    /// `measured_picture_body`: WBPF_PATTERN set but Depth=0 and a
    /// **256-byte** blank buffer. `DataLength` is therefore *not*
    /// `Depth * 32`, and a guard that assumed it would refuse a file
    /// AmigaOS itself ships.
    fn measured_blank_pattern_body() -> Vec<u8> {
        let mut body = vec![0u8; 16];
        body.extend_from_slice(&2u16.to_be_bytes()); // Which = screen
        body.extend_from_slice(&0x0001u16.to_be_bytes()); // WBPF_PATTERN
        body.push(0);
        body.push(0); // Depth = 0
        body.extend_from_slice(&256u16.to_be_bytes());
        body.extend(std::iter::repeat(0u8).take(256));
        body
    }

    #[test]
    fn the_measured_picture_backdrop_reads_as_a_path() {
        let b = read_backdrop(&measured_picture_body()).unwrap();
        assert_eq!(b.which, Which::Root);
        assert_eq!(b.placement, Placement::Scale);
        assert_eq!(b.precision, Precision::Image);
        assert_eq!(b.dither, Dither::Good);
        assert!(!b.no_remap);
        assert_eq!(
            b.content,
            Content::Picture("Sys:Prefs/Presets/Backdrops/default_pal.iff".to_string())
        );
    }

    #[test]
    fn the_measured_pattern_backdrop_reads_as_planes() {
        let b = read_backdrop(&measured_pattern_body()).unwrap();
        assert_eq!(b.which, Which::Screen);
        match b.content {
            Content::Pattern { depth, ref planes } => {
                assert_eq!(depth, 3);
                assert_eq!(planes.len(), 96, "16x16 bits times three planes");
            }
            _ => panic!("WBPF_PATTERN set must read as a pattern, got {:?}", b.content),
        }
    }

    #[test]
    fn every_measured_body_round_trips_byte_for_byte() {
        for body in [
            measured_picture_body(),
            measured_pattern_body(),
            measured_blank_pattern_body(),
        ] {
            let parsed = read_backdrop(&body).unwrap();
            assert_eq!(write_backdrop(&parsed).unwrap(), body);
        }
    }

    #[test]
    fn a_release_pattern_chunk_with_depth_zero_and_256_bytes_is_accepted() {
        // DataLength is not Depth * 32. The release's own screen chunk is
        // Depth=0 with 256 bytes; refusing it would refuse AmigaOS itself.
        let b = read_backdrop(&measured_blank_pattern_body()).unwrap();
        match b.content {
            Content::Pattern { depth, ref planes } => {
                assert_eq!(depth, 0);
                assert_eq!(planes.len(), 256);
            }
            _ => panic!("WBPF_PATTERN set must read as a pattern, got {:?}", b.content),
        }
    }

    #[test]
    fn a_written_picture_backdrop_carries_the_sixteen_reserved_bytes() {
        let out = write_backdrop(&Backdrop {
            which: Which::Root,
            placement: Placement::Scale,
            precision: Precision::Image,
            dither: Dither::Good,
            no_remap: false,
            content: Content::Picture("Sys:X/y.iff".to_string()),
        })
        .unwrap();
        assert_eq!(&out[0..16], &[0u8; 16], "wbp_Reserved is 16 bytes, not zero");
        assert_eq!(u16::from_be_bytes([out[18], out[19]]), 0x2A00);
        assert_eq!(
            out.len(),
            24 + "Sys:X/y.iff".len(),
            "no terminator: DataLength is the exact string length"
        );
        assert_eq!(u16::from_be_bytes([out[22], out[23]]) as usize, "Sys:X/y.iff".len());
    }

    #[test]
    fn the_data_length_must_agree_with_what_follows() {
        let mut body = measured_picture_body();
        body[22..24].copy_from_slice(&9999u16.to_be_bytes());
        assert!(read_backdrop(&body).is_err());
    }

    #[test]
    fn a_body_shorter_than_the_header_is_refused() {
        assert!(read_backdrop(&[0u8; 23]).is_err());
    }

    #[test]
    fn a_picture_path_is_not_nul_terminated() {
        // Measured across four real PTRN chunks: DataLength is exactly the
        // string length. Appending a NUL would make every ART path one byte
        // longer than the one AmigaOS wrote.
        let mut body = vec![0u8; 16];
        body.extend_from_slice(&0u16.to_be_bytes());
        body.extend_from_slice(&0x2A00u16.to_be_bytes());
        body.push(0);
        body.push(0);
        body.extend_from_slice(&4u16.to_be_bytes());
        body.extend_from_slice(b"abcd");
        let b = read_backdrop(&body).unwrap();
        assert_eq!(b.content, Content::Picture("abcd".to_string()));
        assert_eq!(write_backdrop(&b).unwrap(), body);
    }

    #[test]
    fn a_trailing_nul_inside_the_data_is_kept_not_stripped() {
        // A path is taken verbatim for DataLength bytes. Stripping a NUL
        // would silently shorten a name and break the round trip.
        let mut body = vec![0u8; 16];
        body.extend_from_slice(&0u16.to_be_bytes());
        body.extend_from_slice(&0x2A00u16.to_be_bytes());
        body.push(0);
        body.push(0);
        body.extend_from_slice(&5u16.to_be_bytes());
        body.extend_from_slice(b"abcd\0");
        let b = read_backdrop(&body).unwrap();
        assert_eq!(b.content, Content::Picture("abcd\u{0}".to_string()));
        assert_eq!(write_backdrop(&b).unwrap(), body);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test amigaprefs::wbpattern`
Expected: FAIL to compile.

- [ ] **Step 3: Write the implementation**

Replace `wbpattern.rs` with a module documenting the measured table from §1.1, then:

```rust
use crate::core::error::{CoreError, CoreResult};

const RESERVED_LEN: usize = 16;
const HEADER_LEN: usize = 24;
/// `PAT_WIDTH` x `PAT_HEIGHT` = 16x16 bits = 32 bytes, per plane.
const PATTERN_BYTES_PER_PLANE: usize = 32;

const WBPF_PATTERN: u16 = 0x0001;
const WBPF_NOREMAP: u16 = 0x0010;
const DITHER_MASK: u16 = 0x0300;
const PRECISION_MASK: u16 = 0x0C00;
const PLACEMENT_MASK: u16 = 0x3000;

/// The flags the release's own root backdrop carries:
/// `PLACEMENT_SCALE | PRECISION_IMAGE | DITHER_GOOD`. ART's default for a
/// picture is the release's choice, not one invented here.
pub const DEFAULT_PICTURE_FLAGS: u16 = 0x2A00;
```

Define the four enums with `to_bits`/`from_bits` helpers over their masks, `Content`, `Backdrop`, then `read_backdrop` (bounds-check `HEADER_LEN`, read the fields at the measured offsets, require `data_length` to equal the remaining bytes, and branch on `WBPF_PATTERN`) and `write_backdrop` (16 zero bytes, the fields, the data).

Two rules, both measured and both the opposite of a plausible guess:

- **A `Picture` writes the path's bytes and nothing else** — no NUL, `DataLength` is the exact string length — and sets `depth` to `0`. Decode and encode the path as ISO-8859-1 (`char::from(byte)` and the inverse), the same way `env.rs` does, so a non-ASCII drawer name survives.
- **A `Pattern` carries its bytes opaquely.** Do **not** require `planes.len() == depth * PATTERN_BYTES_PER_PLANE`: the AmigaOS 3.2 and 3.9 release ships a screen chunk with `Depth=0` and a 256-byte blank buffer, so that rule would refuse the OS's own file. `PATTERN_BYTES_PER_PLANE` stays as documentation of what a *populated* pattern looks like — the `Christmas` preset's `3 × 32 = 96` — and is not enforced. ART writes pictures this round, never patterns; the container's own length checks already bound the data.

Refuse with `CoreError::Malformed { format: "WBPattern PTRN", detail }`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test amigaprefs::wbpattern`
Expected: PASS, 10 tests.

- [ ] **Step 5: Mutate the guards**

| Mutation | Must fail |
|---|---|
| write 6 reserved bytes instead of 16 (Hatcher's own bug) | `a_written_picture_backdrop_carries_the_sixteen_reserved_bytes`, `every_measured_body_round_trips_byte_for_byte` |
| ignore `WBPF_PATTERN`, always read a path | `the_measured_pattern_backdrop_reads_as_planes` |
| drop the `data_length` agreement check | `the_data_length_must_agree_with_what_follows` |
| append a NUL to a written path | `a_picture_path_is_not_nul_terminated`, `a_written_picture_backdrop_carries_the_sixteen_reserved_bytes` |
| strip a trailing NUL when reading | `a_trailing_nul_inside_the_data_is_kept_not_stripped` |
| add `planes.len() == depth * 32` back as a refusal | `a_release_pattern_chunk_with_depth_zero_and_256_bytes_is_accepted` |

- [ ] **Step 6: Commit**

---

### Task 3: The `SCRM` chunk — screen mode

**Files:**
- Modify: `src-tauri/src/core/amigaprefs/screenmode.rs`

**Interfaces:**
- Produces:
  - `pub struct ScreenMode { pub display_id: u32, pub width: u16, pub height: u16, pub depth: u16, pub control: u16 }`
  - `pub const USE_MODE_DEFAULT: u16 = 0xFFFF;`
  - `pub fn read_screen_mode(body: &[u8]) -> CoreResult<ScreenMode>`
  - `pub fn write_screen_mode(m: &ScreenMode) -> CoreResult<Vec<u8>>`

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// The `SCRM` body measured in the AmigaOS 3.9 tree: 28 bytes, reserved
    /// 16, DisplayID 0x00029000, Width and Height 0xFFFF, Depth 4, Control 1.
    fn measured_body() -> Vec<u8> {
        let mut body = vec![0u8; 16];
        body.extend_from_slice(&0x0002_9000u32.to_be_bytes());
        body.extend_from_slice(&0xFFFFu16.to_be_bytes());
        body.extend_from_slice(&0xFFFFu16.to_be_bytes());
        body.extend_from_slice(&4u16.to_be_bytes());
        body.extend_from_slice(&1u16.to_be_bytes());
        assert_eq!(body.len(), 28, "the measured chunk size is 28");
        body
    }

    #[test]
    fn the_measured_body_reads_as_the_release_shipped_it() {
        let m = read_screen_mode(&measured_body()).unwrap();
        assert_eq!(m.display_id, 0x0002_9000);
        assert_eq!(m.width, USE_MODE_DEFAULT);
        assert_eq!(m.height, USE_MODE_DEFAULT);
        assert_eq!(m.depth, 4, "sixteen colours");
        assert_eq!(m.control, 1);
    }

    #[test]
    fn it_round_trips_byte_for_byte() {
        let body = measured_body();
        assert_eq!(write_screen_mode(&read_screen_mode(&body).unwrap()).unwrap(), body);
    }

    #[test]
    fn the_depth_is_a_whole_u16_not_its_low_byte() {
        let mut m = read_screen_mode(&measured_body()).unwrap();
        m.depth = 0x0108;
        let out = write_screen_mode(&m).unwrap();
        assert_eq!(u16::from_be_bytes([out[24], out[25]]), 0x0108);
    }

    #[test]
    fn a_body_that_is_not_twenty_eight_bytes_is_refused() {
        assert!(read_screen_mode(&[0u8; 27]).is_err());
        assert!(read_screen_mode(&[0u8; 29]).is_err());
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test amigaprefs::screenmode`
Expected: FAIL to compile.

- [ ] **Step 3: Write the implementation**

Offsets, all measured: reserved `0..16`, `display_id` `16..20`, `width` `20..22`, `height` `22..24`, `depth` `24..26`, `control` `26..28`. `read_screen_mode` requires exactly 28 bytes; `write_screen_mode` emits 16 zero bytes then the five fields, big-endian.

Document in the module doc that `depth` is a `u16` and that Hatcher's `body + 25` byte write is its low half — so nobody "fixes" this to match theirs later.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test amigaprefs::screenmode`
Expected: PASS, 4 tests.

- [ ] **Step 5: Mutate the guards**

| Mutation | Must fail |
|---|---|
| write the depth as one byte at offset 25 | `the_depth_is_a_whole_u16_not_its_low_byte` |
| accept any body length ≥ 28 | `a_body_that_is_not_twenty_eight_bytes_is_refused` |
| read `display_id` at offset 12 | `the_measured_body_reads_as_the_release_shipped_it` |

- [ ] **Step 6: Commit**

---

### Task 4: `Prefs/Env-Archive/<name>` variables

**Files:**
- Modify: `src-tauri/src/core/amigaprefs/env.rs`

**Interfaces:**
- Produces:
  - `pub fn encode_value(value: &str) -> CoreResult<Vec<u8>>` — ISO-8859-1, bare LF, no trailing newline added
  - `pub fn decode_value(bytes: &[u8]) -> String`
  - `pub const SHELL_DEFAULTS: [(&str, &str); 5]`

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_is_iso_8859_1_not_utf_8() {
        // U+00FC LATIN SMALL LETTER U WITH DIAERESIS is one byte in
        // ISO-8859-1 and two in UTF-8. An Amiga reads the file as bytes.
        assert_eq!(encode_value("\u{00FC}").unwrap(), vec![0xFC]);
    }

    #[test]
    fn a_character_outside_iso_8859_1_is_refused_not_mangled() {
        // U+011F LATIN SMALL LETTER G WITH BREVE has no ISO-8859-1 byte.
        // Silently writing "?" would put a wrong value on an Amiga and say
        // nothing, which is this project's most expensive failure class.
        let err = encode_value("\u{011F}").unwrap_err();
        assert!(
            format!("{err}").contains("ISO-8859-1"),
            "the refusal must say why, got: {err}"
        );
    }

    #[test]
    fn line_endings_are_bare_lf() {
        assert_eq!(encode_value("a\r\nb").unwrap(), b"a\nb".to_vec());
        assert_eq!(encode_value("a\nb").unwrap(), b"a\nb".to_vec());
    }

    #[test]
    fn no_newline_is_appended_to_a_single_line_value() {
        assert_eq!(encode_value("Workbench:").unwrap(), b"Workbench:".to_vec());
    }

    #[test]
    fn the_shell_defaults_are_the_five_measured_names() {
        let names: Vec<&str> = SHELL_DEFAULTS.iter().map(|(n, _)| *n).collect();
        assert_eq!(
            names,
            vec![
                "Sys/def_shell",
                "Sys/def_editor",
                "Sys/def_cli",
                "Sys/def_width",
                "Sys/def_height"
            ]
        );
    }

    #[test]
    fn a_value_round_trips() {
        for v in ["Workbench:", "C:Ed", "640", "CON:0/50//150/Shell/CLOSE"] {
            assert_eq!(decode_value(&encode_value(v).unwrap()), v);
        }
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test amigaprefs::env`
Expected: FAIL to compile.

- [ ] **Step 3: Write the implementation**

```rust
pub const SHELL_DEFAULTS: [(&str, &str); 5] = [
    ("Sys/def_shell", "CON:0/50//150/Shell/CLOSE"),
    ("Sys/def_editor", "C:Ed"),
    ("Sys/def_cli", "NewShell"),
    ("Sys/def_width", "640"),
    ("Sys/def_height", "256"),
];
```

`encode_value` normalises `\r\n` to `\n`, then maps each `char`: `c as u32 <= 0xFF` becomes that byte, anything else is `CoreError::Malformed { format: "Env-Archive value", detail: "'<c>' has no ISO-8859-1 byte" }`. `decode_value` maps each byte to `char::from(byte)`.

Note in the module doc that these five values are Hatcher's list minus its sixth (`Workbench` = `Workbench:`), which is omitted because a release sets it itself and ART does not second-guess the release.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test amigaprefs::env`
Expected: PASS, 6 tests.

- [ ] **Step 5: Mutate the guards**

| Mutation | Must fail |
|---|---|
| use `value.as_bytes()` (UTF-8) | `a_value_is_iso_8859_1_not_utf_8` |
| substitute `?` for an unmappable char | `a_character_outside_iso_8859_1_is_refused_not_mangled` |
| keep `\r\n` | `line_endings_are_bare_lf` |

- [ ] **Step 6: Commit**

---

### Task 5: ByteRun1 and the ILBM encoder

**Files:**
- Create: `src-tauri/src/core/ilbm/mod.rs`
- Create: `src-tauri/src/core/ilbm/packbits.rs`
- Modify: `src-tauri/src/core/mod.rs` — add `pub mod ilbm;`

**Interfaces:**
- Consumes: nothing new
- Produces:
  - `pub fn pack_row(row: &[u8]) -> Vec<u8>` (in `packbits`)
  - `pub fn unpack_row(packed: &[u8], expect: usize) -> CoreResult<Vec<u8>>` (test-side inverse, also used by the oracle script's Rust half)
  - `pub struct Indexed { pub width: u16, pub height: u16, pub palette: Vec<[u8; 3]>, pub pixels: Vec<u8> }`
  - `pub fn encode(image: &Indexed) -> CoreResult<Vec<u8>>`
  - `pub fn planes_for(colours: usize) -> u8`

- [ ] **Step 1: Write the failing tests**

```rust
// packbits.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_becomes_one_control_byte_and_one_value() {
        // 10 identical bytes: control = 257 - 10 = 247 = 0xF7, then the byte.
        assert_eq!(pack_row(&[0xAA; 10]), vec![0xF7, 0xAA]);
    }

    #[test]
    fn a_literal_run_is_prefixed_with_its_length_minus_one() {
        assert_eq!(pack_row(&[1, 2, 3]), vec![2, 1, 2, 3]);
    }

    #[test]
    fn a_run_longer_than_128_is_split() {
        let packed = pack_row(&[7u8; 200]);
        assert_eq!(unpack_row(&packed, 200).unwrap(), vec![7u8; 200]);
        assert!(packed.len() < 200, "compression must actually compress");
    }

    #[test]
    fn the_control_byte_0x80_is_never_emitted() {
        // 0x80 means "no operation" and some decoders treat it as a stop.
        for len in 1..300usize {
            let row: Vec<u8> = (0..len).map(|i| (i % 7) as u8).collect();
            assert!(!pack_row(&row).contains(&0x80), "len {len}");
        }
    }

    #[test]
    fn every_row_shape_round_trips() {
        let cases: Vec<Vec<u8>> = vec![
            vec![],
            vec![1],
            vec![5, 5],
            vec![1, 1, 1, 2, 3, 4, 4, 4, 4, 4],
            (0..255u8).collect(),
            vec![0u8; 1000],
        ];
        for row in cases {
            let packed = pack_row(&row);
            assert_eq!(unpack_row(&packed, row.len()).unwrap(), row);
        }
    }
}
```

```rust
// mod.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ilbm::packbits::unpack_row;

    fn two_by_two_black_and_white() -> Indexed {
        Indexed {
            width: 2,
            height: 2,
            palette: vec![[0, 0, 0], [255, 255, 255]],
            pixels: vec![0, 1, 1, 0],
        }
    }

    #[test]
    fn a_two_colour_image_uses_one_plane_not_eight() {
        assert_eq!(planes_for(2), 1);
        assert_eq!(planes_for(3), 2);
        assert_eq!(planes_for(16), 4);
        assert_eq!(planes_for(256), 8);
    }

    #[test]
    fn the_form_type_is_ilbm() {
        let out = encode(&two_by_two_black_and_white()).unwrap();
        assert_eq!(&out[0..4], b"FORM");
        assert_eq!(&out[8..12], b"ILBM");
    }

    #[test]
    fn the_bmhd_carries_the_measured_field_order() {
        let out = encode(&two_by_two_black_and_white()).unwrap();
        assert_eq!(&out[12..16], b"BMHD");
        assert_eq!(u32::from_be_bytes([out[16], out[17], out[18], out[19]]), 20);
        assert_eq!(u16::from_be_bytes([out[20], out[21]]), 2, "width");
        assert_eq!(u16::from_be_bytes([out[22], out[23]]), 2, "height");
        assert_eq!(out[28], 1, "nPlanes for two colours");
        assert_eq!(out[29], 0, "masking is mskNone for a backdrop");
        assert_eq!(out[30], 1, "compression is ByteRun1");
        assert_eq!(out[31], 0, "pad1");
    }

    #[test]
    fn the_cmap_holds_three_bytes_for_every_colour_the_planes_can_index() {
        let out = encode(&two_by_two_black_and_white()).unwrap();
        let at = out.windows(4).position(|w| w == b"CMAP").unwrap();
        let size = u32::from_be_bytes([out[at + 4], out[at + 5], out[at + 6], out[at + 7]]);
        assert_eq!(size, 2 * 3, "one plane indexes two colours");
    }

    #[test]
    fn a_palette_larger_than_the_planes_can_index_is_refused() {
        let mut img = two_by_two_black_and_white();
        img.palette = vec![[0, 0, 0]; 300];
        assert!(encode(&img).is_err());
    }

    #[test]
    fn a_pixel_index_outside_the_palette_is_refused() {
        let mut img = two_by_two_black_and_white();
        img.pixels = vec![0, 1, 1, 9];
        assert!(encode(&img).is_err());
    }

    #[test]
    fn pixels_must_be_width_times_height() {
        let mut img = two_by_two_black_and_white();
        img.pixels = vec![0, 1, 1];
        assert!(encode(&img).is_err());
    }

    #[test]
    fn a_row_is_padded_to_a_whole_number_of_words() {
        // 17 pixels wide needs ((17 + 15) / 16) * 2 = 4 bytes per plane row.
        let img = Indexed {
            width: 17,
            height: 1,
            palette: vec![[0, 0, 0], [255, 255, 255]],
            pixels: vec![1; 17],
        };
        let out = encode(&img).unwrap();
        let at = out.windows(4).position(|w| w == b"BODY").unwrap();
        let size = u32::from_be_bytes([out[at + 4], out[at + 5], out[at + 6], out[at + 7]]) as usize;
        let unpacked = unpack_row(&out[at + 8..at + 8 + size], 4).unwrap();
        assert_eq!(unpacked.len(), 4);
        assert_eq!(unpacked, vec![0xFF, 0xFF, 0x80, 0x00], "17 set bits, then padding");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test ilbm::`
Expected: FAIL to compile.

- [ ] **Step 3: Write the implementation**

`packbits.rs` — the classic PackBits, per row:

```rust
//! ByteRun1, the compression an ILBM `BMHD` selects with `compression = 1`.
//!
//! A control byte `n` in `0..=127` means "the next `n + 1` bytes are
//! literal"; `n` in `129..=255` means "repeat the next byte `257 - n`
//! times"; `128` is a no-operation that this encoder never emits, because
//! some historical decoders treat it as end-of-data.
//!
//! ILBM compresses **each plane row independently** — a run never crosses a
//! row boundary, which is why this takes a row rather than the whole body.

pub fn pack_row(row: &[u8]) -> Vec<u8> { /* ... */ }
```

Implement with the standard two-state scan: find runs of ≥ 3 equal bytes and emit them as a replicate (capped at 128 bytes per control byte), accumulate everything else into a literal buffer flushed at 128 bytes or at the next run.

`mod.rs` — `planes_for(colours)` is `max(1, ceil(log2(colours)))` capped at 8. `encode`:

1. Validate: `pixels.len() == width * height`, palette non-empty and ≤ 256, every pixel `< palette.len()`, `palette.len() <= 1 << planes`.
2. `BMHD`, 20 bytes at the offsets measured in §1.3: `width` u16, `height` u16, `x` i16 = 0, `y` i16 = 0, `nPlanes` u8, `masking` u8 = 0, `compression` u8 = 1, `pad1` u8 = 0, `transparentColor` u16 = 0, `xAspect` u8 = 22, `yAspect` u8 = 22, `pageWidth` i16 = `width`, `pageHeight` i16 = `height`.
3. `CMAP`, `palette.len() * 3` bytes.
4. `BODY`: for each row `y`, for each plane `p` in `0..nPlanes`, build `row_bytes = ((width + 15) / 16) * 2` bytes where bit `7 - (x % 8)` of byte `x / 8` is bit `p` of `pixels[y * width + x]`, then `pack_row` it. Concatenate in that order.
5. Wrap each in a chunk with the IFF odd-size pad, then in `FORM`…`ILBM`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test ilbm::`
Expected: PASS, 13 tests.

- [ ] **Step 5: Mutate the guards**

| Mutation | Must fail |
|---|---|
| emit `0x80` as a literal control byte | `the_control_byte_0x80_is_never_emitted` |
| let a run cross a row boundary (compress the whole body at once) | `a_row_is_padded_to_a_whole_number_of_words` |
| use `width.div_ceil(8)` instead of the word-aligned width | `a_row_is_padded_to_a_whole_number_of_words` |
| always write `nPlanes = 8` | `a_two_colour_image_uses_one_plane_not_eight`, `the_bmhd_carries_the_measured_field_order` |
| write `compression = 0` but still pack the body | `the_bmhd_carries_the_measured_field_order` |
| drop the pixel-index bound check | `a_pixel_index_outside_the_palette_is_refused` |

- [ ] **Step 6: Commit**

---

### Task 6: Decode, scale and quantise

**Files:**
- Create: `src-tauri/src/core/picture/mod.rs`
- Create: `src-tauri/src/core/picture/quantise.rs`
- Modify: `src-tauri/src/core/mod.rs` — add `pub mod picture;`
- Modify: `src-tauri/Cargo.toml` — add `png` and `jpeg-decoder`
- Modify: `THIRD_PARTY_LICENSES.md` — **in this same commit**

**Interfaces:**
- Consumes: `crate::core::ilbm::Indexed`
- Produces:
  - `pub struct Rgb { pub width: u16, pub height: u16, pub pixels: Vec<[u8; 3]> }`
  - `pub fn decode(bytes: &[u8]) -> CoreResult<Rgb>` — sniffs PNG and JPEG magic
  - `pub fn scale_to_fit(src: &Rgb, max_w: u16, max_h: u16) -> Rgb`
  - `pub fn quantise(src: &Rgb, colours: usize) -> Indexed` (in `quantise`)
  - `pub const MAX_SOURCE_BYTES: usize = 32 * 1024 * 1024;`

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u16, height: u16, colour: [u8; 3]) -> Rgb {
        Rgb { width, height, pixels: vec![colour; width as usize * height as usize] }
    }

    #[test]
    fn a_file_that_is_neither_png_nor_jpeg_is_refused_by_name() {
        let err = decode(b"GIF89a....").unwrap_err();
        let text = format!("{err}");
        assert!(text.contains("PNG"), "the refusal must name what ART accepts: {text}");
        assert!(text.contains("JPEG"), "the refusal must name what ART accepts: {text}");
    }

    #[test]
    fn an_oversized_source_is_refused_before_it_is_decoded() {
        let bytes = vec![0x89u8; MAX_SOURCE_BYTES + 1];
        assert!(decode(&bytes).is_err());
    }

    #[test]
    fn a_single_colour_image_quantises_to_one_entry() {
        let out = quantise(&solid(4, 4, [10, 20, 30]), 256);
        assert_eq!(out.palette, vec![[10, 20, 30]]);
        assert_eq!(out.pixels, vec![0u8; 16]);
    }

    #[test]
    fn quantising_never_returns_more_colours_than_asked_for() {
        let mut src = solid(16, 16, [0, 0, 0]);
        for (i, p) in src.pixels.iter_mut().enumerate() {
            *p = [(i * 7) as u8, (i * 13) as u8, (i * 3) as u8];
        }
        for want in [2usize, 4, 16, 256] {
            let out = quantise(&src, want);
            assert!(out.palette.len() <= want, "asked {want}, got {}", out.palette.len());
            assert!(!out.palette.is_empty());
            assert!(out.pixels.iter().all(|&i| (i as usize) < out.palette.len()));
        }
    }

    #[test]
    fn quantising_preserves_the_pixel_count_and_the_dimensions() {
        let src = solid(7, 5, [1, 2, 3]);
        let out = quantise(&src, 16);
        assert_eq!((out.width, out.height), (7, 5));
        assert_eq!(out.pixels.len(), 35);
    }

    #[test]
    fn scaling_keeps_the_aspect_ratio_and_never_enlarges() {
        let src = solid(800, 400, [0, 0, 0]);
        let out = scale_to_fit(&src, 640, 512);
        assert_eq!((out.width, out.height), (640, 320));

        let small = solid(100, 50, [0, 0, 0]);
        let same = scale_to_fit(&small, 640, 512);
        assert_eq!((same.width, same.height), (100, 50), "a small picture is left alone");
    }

    #[test]
    fn a_decoded_png_has_the_dimensions_the_png_declares() {
        let png = tests_support::synthetic_png(3, 2, [200, 100, 50]);
        let rgb = decode(&png).unwrap();
        assert_eq!((rgb.width, rgb.height), (3, 2));
        assert_eq!(rgb.pixels[0], [200, 100, 50]);
        assert_eq!(rgb.pixels.len(), 6);
    }
}
```

Add a `tests_support::synthetic_png(width, height, colour)` that builds a real PNG with the `png` crate's encoder (a dev-time use of the same crate is fine — the test asserts ART's *decode* path, and a fixture the test wrote is still an independent statement of what the bytes mean).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test picture::`
Expected: FAIL to compile.

- [ ] **Step 3: Write the implementation**

Add to `Cargo.toml` under `[dependencies]`, beside the other read-only decoders, with the comment explaining why they are allowed in `core/`:

```toml
# Wallpaper input. Both are pure Rust, read-only decoders that make no
# platform call — the same category as `delharc` and `sevenz-rust2`, and so
# they leave `core/`'s platform independence intact. They exist because a
# stock AmigaOS 3.2 tree has `ilbm.datatype` and nothing else, so a user's
# own picture must be converted host-side rather than placed and hoped for.
png = "0.17"
jpeg-decoder = "0.3"
```

Add both to `THIRD_PARTY_LICENSES.md` in the same commit.

`decode` refuses anything over `MAX_SOURCE_BYTES` **before** handing bytes to a decoder, sniffs `\x89PNG\r\n\x1a\n` and `\xFF\xD8\xFF`, and refuses anything else with a message naming PNG and JPEG. It converts to RGB8 whatever the source's colour type.

`scale_to_fit` is a box filter, never enlarging.

`quantise` is median cut: put every pixel in one box, repeatedly split the box with the largest channel range at that channel's median until there are `colours` boxes or no box can split, average each box for its palette entry, then map every pixel to its box. Deduplicate identical entries so a single-colour image yields exactly one.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test picture:: && cargo deny check licenses`
Expected: PASS; `cargo deny` ok.

- [ ] **Step 5: Mutate the guards**

| Mutation | Must fail |
|---|---|
| drop the size cap | `an_oversized_source_is_refused_before_it_is_decoded` |
| accept any bytes and let the PNG decoder fail | `a_file_that_is_neither_png_nor_jpeg_is_refused_by_name` |
| let median cut return `colours + 1` boxes | `quantising_never_returns_more_colours_than_asked_for` |
| enlarge a small picture to fill the target | `scaling_keeps_the_aspect_ratio_and_never_enlarges` |
| skip the deduplication | `a_single_colour_image_quantises_to_one_entry` |

- [ ] **Step 6: Commit**

---

### Task 7: The applier — first real caller of Tasks 1–6

**Files:**
- Create: `src-tauri/src/core/appearance/mod.rs`
- Modify: `src-tauri/src/core/mod.rs` — add `pub mod appearance;`

**Interfaces:**
- Consumes: `core::amigaprefs::{iff, wbpattern, screenmode, env}`, `core::ilbm`, `core::picture`, `core::safety::{guarded_write, BackupPolicy}`
- Produces:
  - `pub enum WallpaperSource { AlreadyInTree { amiga_path: String }, HostPicture { path: PathBuf, colours: usize } }`
  - `pub struct AppearanceRequest { pub wallpaper: Option<(wbpattern::Which, WallpaperSource, wbpattern::Placement)>, pub screen_depth: Option<u16>, pub shell_defaults: bool }`
  - `pub struct AppearanceOutcome { pub written: Vec<PathBuf>, pub backups: Vec<PathBuf>, pub picture_placed: Option<PathBuf>, pub amiga_path: Option<String> }`
  - `pub fn apply_appearance(tree: &Path, req: &AppearanceRequest) -> CoreResult<AppearanceOutcome>`
  - `pub fn backdrops_in_tree(tree: &Path) -> CoreResult<Vec<String>>`
  - `pub const BACKDROPS_DRAWER: [&str; 3] = ["Prefs", "Presets", "Backdrops"];`

- [ ] **Step 1: Write the failing tests**

Build a synthetic tree in a `ScratchDir`: `Prefs/Env-Archive/Sys/WBPattern.prefs` containing the three measured `PTRN` chunks from Task 2 and a `ScreenMode.prefs` from Task 3.

```rust
#[test]
fn setting_the_root_backdrop_leaves_the_other_two_chunks_byte_for_byte() { /* ... */ }

#[test]
fn a_host_picture_is_encoded_to_ilbm_and_placed_in_the_backdrops_drawer() {
    // asserts the written file begins FORM....ILBM and that the PTRN's
    // Content::Picture equals "Sys:Prefs/Presets/Backdrops/<name>.iff"
}

#[test]
fn the_previous_prefs_file_is_backed_up_and_the_path_is_returned() {
    // outcome.backups must be non-empty and the backup must hold the
    // original bytes
}

#[test]
fn refusing_to_overwrite_a_backdrop_that_is_already_there() {
    // SAFE_CREATE: a second apply with the same picture name is an error,
    // and the existing file is unchanged byte-for-byte
}

#[test]
fn a_failed_apply_leaves_every_prefs_file_unchanged() {
    // point at a picture that is not a picture; assert WBPattern.prefs is
    // byte-identical to before
}

#[test]
fn backdrops_in_tree_lists_what_the_release_actually_shipped() { /* ... */ }

#[test]
fn a_tree_with_no_wbpattern_prefs_is_refused_by_name() {
    // the refusal names Prefs/Env-Archive/Sys/WBPattern.prefs, so the user
    // can see which file is missing
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test appearance::`
Expected: FAIL to compile.

- [ ] **Step 3: Write the implementation**

`apply_appearance`:
1. Resolve `Prefs/Env-Archive/Sys/WBPattern.prefs` **case-insensitively** under `tree`; refuse by name if absent.
2. For a `HostPicture`: read it (`picture::decode`), `scale_to_fit` the screen size, `quantise`, `ilbm::encode`, and write it into `BACKDROPS_DRAWER` under `<stem>.iff` — **`SAFE_CREATE`: refuse if it exists**. Create the drawer if it is missing.
3. Parse the prefs with `iff::parse`, find the `PTRN` chunk whose `Which` matches, build the new body with `wbpattern::write_backdrop` using `DEFAULT_PICTURE_FLAGS` adjusted for the requested placement, and `replace_bodies` **only that index**.
4. `guarded_write(prefs_path, &new_bytes, BackupPolicy::CONFIG)`; record the backup path.
5. Same shape for `ScreenMode.prefs` when `screen_depth` is set, and for the five `SHELL_DEFAULTS` when `shell_defaults` is true.
6. Keep the `.uaem` sidecar in step — follow what `core/osinstall/apply.rs::settle_sidecar` does for a rewritten file.

Everything that can fail happens **before** the first `guarded_write`, so a refusal leaves the tree untouched.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test appearance::`
Expected: PASS.

- [ ] **Step 5: Mutate the guards**

| Mutation | Must fail |
|---|---|
| replace every `PTRN` rather than the matching one | `setting_the_root_backdrop_leaves_the_other_two_chunks_byte_for_byte` |
| use `atomic_write` instead of `guarded_write` | `the_previous_prefs_file_is_backed_up_and_the_path_is_returned` |
| overwrite an existing backdrop | `refusing_to_overwrite_a_backdrop_that_is_already_there` |
| write the prefs before encoding the picture | `a_failed_apply_leaves_every_prefs_file_unchanged` |

- [ ] **Step 6: Commit**

---

### Task 8: Verify — every prefs path resolves

**Files:**
- Modify: `src-tauri/src/core/osinstall/verify.rs`

**Interfaces:**
- Consumes: `core::amigaprefs::{iff, wbpattern}`
- Produces: `pub fn check_prefs_paths(tree: &Path) -> CoreResult<Vec<FileVerdict>>`, folded into `verify_volume`'s report

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_backdrop_the_tree_does_not_have_is_reported_by_both_names() {
    // the verdict must name the prefs file AND the missing Amiga path —
    // this is the dist-3.2 orphan of the design doc's §1.5
}

#[test]
fn a_backdrop_the_tree_does_have_passes_even_when_the_case_differs() {
    // AmigaDOS is case-insensitive; a tree holding "Default_Pal.iff" must
    // satisfy a PTRN naming "default_pal.iff"
}

#[test]
fn a_pattern_chunk_names_no_path_and_is_not_checked() { /* ... */ }

#[test]
fn a_tree_with_no_prefs_at_all_is_not_checked_and_does_not_fail() {
    // CheckState::NotChecked, never rendered as a tick
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test osinstall::verify`
Expected: FAIL to compile.

- [ ] **Step 3: Write the implementation**

Walk `Prefs/Env-Archive/Sys/*.prefs`, parse each with `iff::parse`, read each `PTRN` with `wbpattern::read_backdrop`, and for every `Content::Picture(path)` strip the `Sys:` prefix and resolve each path component case-insensitively under `tree`. Report a `FileVerdict` naming both the prefs file and the unresolved path. A tree with no prefs is `CheckState::NotChecked` — the three states stay apart, and `NotChecked` is never rendered as a pass.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test osinstall::verify`
Expected: PASS.

- [ ] **Step 5: Mutate the guards**

| Mutation | Must fail |
|---|---|
| compare paths case-sensitively | `a_backdrop_the_tree_does_have_passes_even_when_the_case_differs` |
| report only the missing path, not the prefs file | `a_backdrop_the_tree_does_not_have_is_reported_by_both_names` |
| return `CheckState::Passed` for a tree with no prefs | `a_tree_with_no_prefs_at_all_is_not_checked_and_does_not_fail` |

- [ ] **Step 6: Commit**

---

### Task 9: The command layer

**Files:**
- Create: `src-tauri/src/commands/appearance.rs`
- Modify: `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs` (`invoke_handler![]`)
- Create: `src/lib/appearance.ts`

**Interfaces:**
- Produces: `appearance_backdrops(tree)`, `appearance_apply(tree, request)` — thin adapters only: deserialise, call `core::appearance`, serialise back.

- [ ] **Step 1: Write the failing test**

A Rust test asserting each command is registered — follow the existing `every_command_is_registered`-style test if one exists, otherwise assert the adapter returns the core's own error text unchanged for a missing tree. Plus `src/lib/appearance.test.ts` asserting the wrapper passes its arguments through.

- [ ] **Step 2: Run to verify it fails** — `cd src-tauri && cargo test appearance` and `pnpm vitest run src/lib/appearance.test.ts`

- [ ] **Step 3: Write the adapters.** Add both names to `invoke_handler![]` in `lib.rs` **and** the typed wrapper in `src/lib/appearance.ts`; the frontend never calls `invoke` from a component.

- [ ] **Step 4: Run to verify they pass**

- [ ] **Step 5: Commit**

---

### Task 10: The Görünüm panel

**Files:**
- Create: `src/components/osbuilder/AppearancePanel.tsx`
- Create: `src/components/osbuilder/AppearancePanel.test.tsx`
- Modify: the volumes step that renders `NetworkPanel`
- Modify: `src/i18n/en.json`, `src/i18n/tr.json` — **both, same commit**

- [ ] **Step 1: Write the failing tests**

```tsx
it("offers only the backdrops the built tree actually contains", async () => { /* ... */ });
it("shows the refusal text when a picture is neither PNG nor JPEG", async () => { /* ... */ });
it("remembers the placement across a remount", async () => { /* ... */ });
it("says which file the backup went to after a successful apply", async () => { /* ... */ });
it("renders the panel in Turkish when the language is tr", async () => { /* ... */ });
```

**Assert the specific sentence, never a state with more than one cause.** "The Apply button is disabled" is true before a tree is chosen as well, so put the panel in a state where only the thing under test can produce it, then assert the message text.

- [ ] **Step 2: Run to verify they fail** — `pnpm vitest run src/components/osbuilder/AppearancePanel.test.tsx`

- [ ] **Step 3: Implement.** A `.tsx` file so jsdom applies. Every user-visible string via `t()`; anything built in `src/lib` returns a `Phrase`. Every choice goes through `src/lib/remembered.ts` with a guard (`isOneOf` for the placement, `isWholeNumberBetween` for the depth). These are consumed by the same step that asks for them, so they are remembered keys, **not** `buildSession` fields — apply `CLAUDE.md`'s test: can the value ART wrote and the value the next step operates on drift apart? Here, no.

- [ ] **Step 4: Run to verify they pass**, then `pnpm lint` and `pnpm test`

- [ ] **Step 5: Trace the chain and record it.** From the panel's Apply button through `src/lib/appearance.ts` → `commands/appearance.rs` → `core::appearance::apply_appearance` → `core::ilbm::encode`, hop by hop. Put the hops in the commit message. This is the WHDLoad round's lesson made into a step.

- [ ] **Step 6: Commit**

---

### Task 11: The oracles

**Files:**
- Create: `scripts/ilbm-oracle-check.py`
- Modify: `src-tauri/src/core/amigaprefs/iff.rs` — add the `#[ignore]`d real-material hook

- [ ] **Step 1: Write the oracle script**

`scripts/ilbm-oracle-check.py` finds `ffmpeg` (PATH first, then `%LOCALAPPDATA%\Microsoft\WinGet\Packages` — it is installed there on this machine, `Gyan.FFmpeg` 9.0.1). The fixtures come from **the product's own encoder**, through one mechanism and no new binary: an `#[ignore]`d Rust test writes the ILBM set plus a JSON manifest of the pixels it meant into the directory named by `ART_ILBM_OUT`, and the script sets that variable, runs

```bash
cd src-tauri && ART_ILBM_OUT="<dir>" cargo test write_the_ilbm_oracle_fixtures -- --ignored --nocapture
```

then decodes each `.iff` with `ffmpeg -y -i in.iff out.png` and compares **every pixel** against the manifest. Cover 1, 4 and 8 planes, a solid image, a noisy one, and an odd width (17) so the word-alignment padding is exercised. It exits non-zero on the first mismatch and prints the coordinates. It refuses to pass silently when ffmpeg is absent — it reports "ffmpeg not found, nothing was checked", which is not the same claim as success.

- [ ] **Step 2: Run it and confirm it fails against a deliberate defect**

Break the encoder (write `compression = 0` while still packing), run the script, confirm it reports a mismatch rather than passing. Restore.

- [ ] **Step 3: Write the prefs round-trip hook**

```rust
#[test]
#[ignore = "needs the owner's own material; set ART_PREFS_DIR"]
fn every_real_prefs_file_round_trips_byte_for_byte() {
    let Ok(dir) = std::env::var("ART_PREFS_DIR") else { return };
    // walk *.prefs, parse each, assert to_bytes() == the file's own bytes,
    // print the count so the number in STATUS.md is measured not guessed
}
```

- [ ] **Step 4: Run both against real material**

```bash
python scripts/ilbm-oracle-check.py
cd src-tauri && ART_PREFS_DIR="E:\amiga\Amigatolon\os39" \
  cargo test every_real_prefs_file_round_trips_byte_for_byte -- --ignored --nocapture
```

Record the counts. A round-trip over N real files with 0 failures is the claim; "it works" is not.

- [ ] **Step 5: Commit**

---

### Task 12: Land it

- [ ] **Step 1: Full suite, twice** (ART-059). `pnpm lint`, `pnpm test`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` — and **quote the `test result:` line, never the exit code**: a killed harness exits 0 with no summary.
- [ ] **Step 2: The sweeps.** `python scripts/control-byte-sweep.py`, `scripts/scratch-root-sweep.py`, `scripts/scratch-counter-sweep.py`, `scripts/contrast-check.py --quiet`.
- [ ] **Step 3: The documents.** `docs/session-log.md` (a row at the top), `docs/STATUS.md` (the snapshot's numbers and the "Picking up next session" block, **updated in place**), `docs/FEATURES.md` (flip G14's wallpaper/prefs rows — only where a test exists), `docs/ISSUES.md` (any `ART-NNN` filed), `CHANGELOG.md`. Correct the work list's stale "item 5 is the next build round" line in place, and update `docs/superpowers/notes/2026-09-05-emu68hatcher-teardown.md` with the six findings it missed.
- [ ] **Step 4: Merge.** `git branch --show-current` first — a subagent may have moved the tree. Merge `--no-ff` into `main`, push, wait for CI, delete the branch.

---

## Self-review against the spec

| Spec section | Task |
|---|---|
| §3.1 `core/amigaprefs` | 1, 2, 3, 4 |
| §3.2 `core/ilbm` | 5 |
| §3.3 `core/picture` + the dependency decision | 6 |
| §3.4 where the picture lands, `SAFE_CREATE` | 7 |
| §3.5 the verify check | 8 |
| §3.6 `guarded_write`, `BackupPolicy::CONFIG`, `.uaem` | 7 |
| §3.7 the screen, remembered values, both languages | 9, 10 |
| §4 the oracles and the mutation rule | every task's Step 5, plus 11 |
| §5 what is out of scope | nothing implements it, by design |
| §6 known risks | §6's first risk is why Task 12 does not claim the wallpaper looks right; the second is why no task claims an Amiga has opened an ART-written ILBM |
