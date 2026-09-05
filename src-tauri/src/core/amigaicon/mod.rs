//! Amiga `.info` icons — read the layout, and merge tooltypes and stack size.
//!
//! ART-104's context: the AmigaOS 3.2.2 update ships `Tools/IconEdit.info`
//! with `do_StackSize` **doubled** from 4 096 to 8 192 for a binary the same
//! update replaces — and the icon already in an ART-built tree is not the
//! update's icon at all. It is the GlowIcons one, which carries 1 486 bytes
//! of appended IFF ColorIcon artwork after the classic icon fields and sits
//! at a different desktop position. Copying the update's icon over the
//! tree's would drop that artwork and the position; skipping the merge
//! altogether would run the replaced binary on the old, undersized stack.
//! Neither is acceptable, so this module reads enough of the classic `.info`
//! layout to find the `ToolTypes` block and `do_StackSize`, and nothing more
//! — it does not need to understand a `DrawerData`, an `Image`, or an
//! appended `ColorIcon`/`NewIcon` blob to preserve them untouched.
//!
//! ## The format, as measured
//!
//! `do_Magic` `0xE310` at offset 0, then a fixed 78-byte `DiskObject`. Six
//! optional blocks can follow, **in this order**, each present only when its
//! flag field inside the 78-byte header is non-zero:
//!
//! | Block | Flag field (absolute offset) | Shape |
//! |---|---|---|
//! | `DrawerData` | `do_DrawerData` @ 66 | fixed 56 bytes, contents unread |
//! | `GadgetRender` | `Gadget.GadgetRender` @ 22 | an `Image` |
//! | `SelectRender` | `Gadget.SelectRender` @ 26 | an `Image` |
//! | `DefaultTool` | `do_DefaultTool` @ 50 | a string |
//! | `ToolTypes` | `do_ToolTypes` @ 54 | a `u32` size, then that many strings |
//! | `ToolWindow` | `do_ToolWindow` @ 70 | a string |
//!
//! `do_StackSize` is the `u32` at offset 74, always present (it is the last
//! field of the fixed header, ending exactly at byte 78). A **string** is a
//! `u32` length then that many bytes. An **`Image`** is a 20-byte header —
//! `LeftEdge`, `TopEdge`, `Width`, `Height`, `Depth` (each a `u16`), then an
//! `ImageData` pointer, `PlanePick`, `PlaneOnOff` and a `NextImage` pointer,
//! none of which this module reads — followed by
//! `((width + 15) / 16) * 2 * height * depth` bytes of word-aligned bitmap
//! data. The `ToolTypes` size field is `(count + 1) * 4`: the `+ 1` is the
//! `NULL` terminator that ends the in-memory pointer array the icon was
//! saved from, so `count = size / 4 - 1`.
//!
//! Everything after the last present block — an appended ColorIcon or
//! NewIcon `FORM`, or simply nothing — is the **trailing region**. This
//! module carries it through as opaque bytes and never parses it.
//!
//! **What this is checked against, and what it is not.** The field offsets
//! above were measured against three real `.info` files and land exactly on
//! end-of-file for two of them and exactly on the start of an appended IFF
//! `FORM` for the third (research note §8) — that is real evidence, not
//! recalled documentation. What is **not** independently re-verified here:
//! the `Image` and `DrawerData` layouts, since this module only needs to
//! skip them correctly (get their length right), never to read their
//! fields; and non-ASCII tool type text, which is decoded lossily (see
//! [`tooltypes`]) rather than refused, because a bad character in a display
//! string is not the memory-safety hazard a bad length is.
//!
//! ## Every offset is bound-checked before it is indexed
//!
//! This module parses files ART did not write. The release profile sets
//! `panic = "abort"`, so an out-of-range index kills the whole application,
//! not just this operation — the same discipline `core/archive/compress.rs`
//! documents for its own from-scratch decoder applies here. Every length and
//! offset used to advance through the file is computed with `checked_add` /
//! `checked_mul` and validated against the buffer's real length before
//! anything is sliced; a file that does not parse is refused
//! ([`CoreError::Malformed`]), never read past its own bounds and never
//! rewritten best-effort.

use crate::core::error::{CoreError, CoreResult};
use std::ops::Range;

/// `do_Magic` for a classic AmigaOS icon.
const MAGIC: u16 = 0xE310;

/// Size of the fixed `DiskObject` header — every offset below is measured
/// against real icons summing to exactly this (see the module doc's table).
const HEADER_LEN: usize = 78;

/// Fixed size of a `DrawerData` block; its contents are skipped, not read.
const DRAWER_DATA_LEN: usize = 56;

/// Fixed size of an `Image` block's header, before its pixel data.
const IMAGE_HEADER_LEN: usize = 20;

const OFF_GADGET_RENDER: usize = 22;
const OFF_SELECT_RENDER: usize = 26;
const OFF_DEFAULT_TOOL: usize = 50;
const OFF_TOOL_TYPES: usize = 54;
const OFF_DRAWER_DATA: usize = 66;
const OFF_TOOL_WINDOW: usize = 70;
const OFF_STACK_SIZE: usize = 74;

fn malformed(detail: impl Into<String>) -> CoreError {
    CoreError::Malformed {
        format: "amiga icon (.info)".into(),
        detail: detail.into(),
    }
}

/// Read a big-endian `u32` at `at`, refusing rather than indexing out of
/// bounds.
fn be_u32(bytes: &[u8], at: usize) -> CoreResult<u32> {
    let end = at
        .checked_add(4)
        .ok_or_else(|| malformed("offset overflow reading a 32-bit field"))?;
    let slice = bytes
        .get(at..end)
        .ok_or_else(|| malformed("truncated icon: a 32-bit field runs past the file"))?;
    Ok(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

/// Read a big-endian `u16` at `at`, refusing rather than indexing out of
/// bounds.
fn be_u16(bytes: &[u8], at: usize) -> CoreResult<u16> {
    let end = at
        .checked_add(2)
        .ok_or_else(|| malformed("offset overflow reading a 16-bit field"))?;
    let slice = bytes
        .get(at..end)
        .ok_or_else(|| malformed("truncated icon: a 16-bit field runs past the file"))?;
    Ok(u16::from_be_bytes([slice[0], slice[1]]))
}

/// `pos + len`, checked for overflow and for running past the buffer.
/// Returns the new position (`pos + len`) — never the two bounds separately,
/// so a caller cannot forget to check one of them.
fn advance(bytes: &[u8], pos: usize, len: usize) -> CoreResult<usize> {
    let end = pos
        .checked_add(len)
        .ok_or_else(|| malformed("offset overflow"))?;
    if end > bytes.len() {
        return Err(malformed(format!(
            "truncated icon: needed {end} byte(s), file has {}",
            bytes.len()
        )));
    }
    Ok(end)
}

/// Read one length-prefixed string at `pos` — a `u32` length, then that many
/// bytes — decoded lossily (see [`tooltypes`]'s doc for why). Returns the
/// text and the position immediately after it.
///
/// This is the **one** bounds-checked implementation both [`skip_string`]
/// (used while walking `DefaultTool`, `ToolWindow` and each tool type during
/// [`layout`]) and [`tooltypes`] (reading the strings back out) go through —
/// deliberately, so a length that runs past the buffer is refused on every
/// path that reads a tool type, not on whichever one happens to be tested.
fn read_string(bytes: &[u8], pos: usize) -> CoreResult<(String, usize)> {
    let len = be_u32(bytes, pos)? as usize;
    let after_len = advance(bytes, pos, 4)?;
    let end = advance(bytes, after_len, len)?;
    Ok((
        String::from_utf8_lossy(&bytes[after_len..end]).into_owned(),
        end,
    ))
}

/// Skip a length-prefixed string (`DefaultTool`, `ToolWindow`, and each
/// individual tool type) without decoding it: same bound check as
/// [`read_string`], text discarded.
fn skip_string(bytes: &[u8], pos: usize) -> CoreResult<usize> {
    read_string(bytes, pos).map(|(_, end)| end)
}

/// Skip an `Image` block: a 20-byte header carrying `Width`/`Height`/`Depth`
/// at fixed offsets, then the bitmap data itself.
///
/// The pixel byte count is computed the way the brief measured it —
/// `((width + 15) / 16) * 2 * height * depth` — entirely with `checked_add`
/// / `checked_mul` so a hostile width/height/depth triple is refused by the
/// arithmetic (or, on a 64-bit host where the arithmetic itself does not
/// overflow, by the bounds check in [`advance`]) before a single pixel byte
/// is indexed.
fn skip_image(bytes: &[u8], pos: usize) -> CoreResult<usize> {
    let width = be_u16(bytes, pos + 4)?;
    let height = be_u16(bytes, pos + 6)?;
    let depth = be_u16(bytes, pos + 8)?;

    let words = (width as usize)
        .checked_add(15)
        .ok_or_else(|| malformed("image width overflow"))?
        / 16;
    let row_bytes = words
        .checked_mul(2)
        .ok_or_else(|| malformed("image row size overflow"))?;
    let plane_bytes = row_bytes
        .checked_mul(height as usize)
        .ok_or_else(|| malformed("image plane size overflow"))?;
    let pixel_bytes = plane_bytes
        .checked_mul(depth as usize)
        .ok_or_else(|| malformed("image pixel size overflow"))?;
    let total = IMAGE_HEADER_LEN
        .checked_add(pixel_bytes)
        .ok_or_else(|| malformed("image size overflow"))?;

    advance(bytes, pos, total)
}

/// Skip a `ToolTypes` block: a `u32` size equal to `(count + 1) * 4`, then
/// `count` length-prefixed strings.
fn skip_tooltypes(bytes: &[u8], pos: usize) -> CoreResult<usize> {
    let size = be_u32(bytes, pos)?;
    let mut p = advance(bytes, pos, 4)?;
    if size == 0 || !size.is_multiple_of(4) {
        return Err(malformed(format!(
            "ToolTypes size {size} is not a positive multiple of 4"
        )));
    }
    let count = size / 4 - 1;
    for _ in 0..count {
        p = skip_string(bytes, p)?;
    }
    Ok(p)
}

/// Confirm the file is at least a whole `DiskObject` header and carries the
/// right magic. Every public function starts here.
fn check_header(bytes: &[u8]) -> CoreResult<()> {
    if bytes.len() < HEADER_LEN {
        return Err(malformed(format!(
            "an icon needs at least {HEADER_LEN} header byte(s); got {}",
            bytes.len()
        )));
    }
    let magic = be_u16(bytes, 0)?;
    if magic != MAGIC {
        return Err(malformed(format!(
            "expected do_Magic 0x{MAGIC:04x}, found 0x{magic:04x} — not an Amiga icon"
        )));
    }
    Ok(())
}

/// Where the `ToolTypes` block sits (if the icon has one) and where the
/// trailing, unparsed region begins.
///
/// `tooltypes` spans the whole block — its own `u32` size field through its
/// last string — because that is exactly what [`merge_tooltypes`] splices
/// verbatim. `trailing` runs from the end of the last present block to the
/// end of the file; on an icon with no appended ColorIcon/NewIcon data,
/// `trailing.start == trailing.end == bytes.len()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconLayout {
    pub tooltypes: Option<Range<usize>>,
    pub trailing: Range<usize>,
}

/// Walk a `.info` file's fixed header and optional blocks, in the order
/// the format lays them out, and report where `ToolTypes` and the trailing
/// region are.
///
/// Refuses (never reads past bounds, never guesses) when: the file is
/// shorter than a `DiskObject` header, the magic does not match, or any
/// length or pointer-derived size would run past the end of the buffer.
pub fn layout(bytes: &[u8]) -> CoreResult<IconLayout> {
    check_header(bytes)?;

    let mut pos = HEADER_LEN;

    if be_u32(bytes, OFF_DRAWER_DATA)? != 0 {
        pos = advance(bytes, pos, DRAWER_DATA_LEN)?;
    }
    if be_u32(bytes, OFF_GADGET_RENDER)? != 0 {
        pos = skip_image(bytes, pos)?;
    }
    if be_u32(bytes, OFF_SELECT_RENDER)? != 0 {
        pos = skip_image(bytes, pos)?;
    }
    if be_u32(bytes, OFF_DEFAULT_TOOL)? != 0 {
        pos = skip_string(bytes, pos)?;
    }
    let tooltypes = if be_u32(bytes, OFF_TOOL_TYPES)? != 0 {
        let start = pos;
        pos = skip_tooltypes(bytes, pos)?;
        Some(start..pos)
    } else {
        None
    };
    if be_u32(bytes, OFF_TOOL_WINDOW)? != 0 {
        pos = skip_string(bytes, pos)?;
    }

    Ok(IconLayout {
        tooltypes,
        trailing: pos..bytes.len(),
    })
}

/// The icon's tool types, in file order.
///
/// An icon with no `ToolTypes` block returns an empty list, not an error —
/// absence is a normal, common shape (plenty of real icons carry none), and
/// is not the same claim as "this file is not an icon" or "this block is
/// corrupt".
///
/// Text is decoded with [`String::from_utf8_lossy`] rather than refused on
/// invalid UTF-8. A bad length is a memory-safety hazard and is refused by
/// [`layout`] before this function ever runs; a bad *character* in a
/// tool-type string is neither that nor a reason to refuse the whole icon —
/// real AmigaDOS text is Latin-1, not UTF-8, so a non-ASCII tool type (a
/// `PUBSCREEN` name, say) is exactly the case this is for.
pub fn tooltypes(bytes: &[u8]) -> CoreResult<Vec<String>> {
    let parsed = layout(bytes)?;
    let Some(range) = parsed.tooltypes else {
        return Ok(Vec::new());
    };

    let size = be_u32(bytes, range.start)?;
    let count = (size / 4 - 1) as usize;
    let mut items = Vec::with_capacity(count);
    let mut p = advance(bytes, range.start, 4)?;
    for _ in 0..count {
        let (text, next) = read_string(bytes, p)?;
        items.push(text);
        p = next;
    }
    Ok(items)
}

/// `do_StackSize` — a fixed field of the header, so this needs no walk past
/// the magic check.
pub fn stack_size(bytes: &[u8]) -> CoreResult<u32> {
    check_header(bytes)?;
    be_u32(bytes, OFF_STACK_SIZE)
}

/// Merge `source`'s tool types and stack size into `dest`, keeping every
/// other byte of `dest` — including the trailing region — untouched.
///
/// This is a splice, not a rebuild: everything before `dest`'s `ToolTypes`
/// block, then `source`'s `ToolTypes` block verbatim (its own size field and
/// all), then everything after — which is how the GlowIcons artwork and
/// desktop position in `dest`'s header, `DrawerData`, images and trailing
/// ColorIcon/NewIcon data survive a merge whose only stated job is
/// tooltypes and stack size. `do_StackSize` (offset 74, inside the header
/// copied from `dest`) is then overwritten with `source`'s value — this is
/// the concrete case that motivated this module: AmigaOS 3.2.2's
/// `IconEdit.info` doubles it from 4 096 to 8 192.
///
/// Refuses when either icon does not parse, or when either has no
/// `ToolTypes` block to merge — this module only implements the splice the
/// brief describes (block present on both sides); an icon with no
/// `ToolTypes` at all is a real but different shape this function does not
/// attempt to grow a new block for.
pub fn merge_tooltypes(dest: &[u8], source: &[u8]) -> CoreResult<Vec<u8>> {
    let dest_layout = layout(dest)?;
    let source_layout = layout(source)?;

    let dest_range = dest_layout
        .tooltypes
        .ok_or_else(|| malformed("the destination icon has no ToolTypes block to merge into"))?;
    let source_range = source_layout
        .tooltypes
        .ok_or_else(|| malformed("the source icon has no ToolTypes block to merge from"))?;

    let mut merged = Vec::with_capacity(dest.len() - dest_range.len() + source_range.len());
    merged.extend_from_slice(&dest[..dest_range.start]);
    merged.extend_from_slice(&source[source_range]);
    merged.extend_from_slice(&dest[dest_range.end..]);

    let stack = stack_size(source)?;
    merged[OFF_STACK_SIZE..OFF_STACK_SIZE + 4].copy_from_slice(&stack.to_be_bytes());

    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal, valid `.info` by hand: header, an optional
    /// `ToolTypes` block, then arbitrary trailing bytes (standing in for an
    /// appended ColorIcon/NewIcon `FORM`). No `DrawerData`, no images, no
    /// `DefaultTool`, no `ToolWindow` — this module does not need to
    /// exercise those to prove it skips them correctly; that is covered by
    /// [`synthetic_icon_with_image`] for the one block that does matter for
    /// the overflow guard.
    fn synthetic_icon(tooltypes: &[&str], stack: u32, trailing: &[u8]) -> Vec<u8> {
        let mut buf = vec![0u8; HEADER_LEN];
        buf[0..2].copy_from_slice(&MAGIC.to_be_bytes());
        buf[2..4].copy_from_slice(&1u16.to_be_bytes()); // do_Version
        buf[OFF_TOOL_TYPES..OFF_TOOL_TYPES + 4].copy_from_slice(&1u32.to_be_bytes());
        buf[OFF_STACK_SIZE..OFF_STACK_SIZE + 4].copy_from_slice(&stack.to_be_bytes());

        let size = ((tooltypes.len() + 1) * 4) as u32;
        buf.extend_from_slice(&size.to_be_bytes());
        for tt in tooltypes {
            let bytes = tt.as_bytes();
            buf.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
            buf.extend_from_slice(bytes);
        }
        buf.extend_from_slice(trailing);
        buf
    }

    /// A minimal icon with a `GadgetRender` `Image` whose claimed
    /// dimensions vastly exceed the buffer actually behind them — the shape
    /// [`skip_image`] must refuse rather than trust.
    fn synthetic_icon_with_image(width: u16, height: u16, depth: u16) -> Vec<u8> {
        let mut buf = vec![0u8; HEADER_LEN];
        buf[0..2].copy_from_slice(&MAGIC.to_be_bytes());
        buf[2..4].copy_from_slice(&1u16.to_be_bytes());
        buf[OFF_GADGET_RENDER..OFF_GADGET_RENDER + 4].copy_from_slice(&1u32.to_be_bytes());

        let mut image = vec![0u8; IMAGE_HEADER_LEN];
        image[4..6].copy_from_slice(&width.to_be_bytes());
        image[6..8].copy_from_slice(&height.to_be_bytes());
        image[8..10].copy_from_slice(&depth.to_be_bytes());
        buf.extend_from_slice(&image);
        buf
    }

    #[test]
    fn a_hand_built_icon_parses_to_its_own_length() {
        let icon = synthetic_icon(&["A=1", "B=2"], 4096, b"");
        let l = layout(&icon).unwrap();
        assert_eq!(l.trailing.start, icon.len(), "nothing is left over");
        assert_eq!(
            tooltypes(&icon).unwrap(),
            vec!["A=1".to_string(), "B=2".to_string()]
        );
        assert_eq!(stack_size(&icon).unwrap(), 4096);
    }

    #[test]
    fn merging_carries_the_trailing_block_through_byte_for_byte() {
        let trailing = b"FORM....ICONFACE....pretend colour icon";
        let dest = synthetic_icon(&["A=1"], 4096, trailing);
        let source = synthetic_icon(&["A=1", "(PUBSCREEN=<name>)"], 8192, b"");

        let merged = merge_tooltypes(&dest, &source).unwrap();

        let l = layout(&merged).unwrap();
        assert_eq!(
            &merged[l.trailing.clone()],
            trailing,
            "the ColorIcon survives"
        );
        assert_eq!(tooltypes(&merged).unwrap(), tooltypes(&source).unwrap());
        assert_eq!(
            stack_size(&merged).unwrap(),
            8192,
            "the stack comes from the source"
        );
    }

    #[test]
    fn merging_an_icon_with_itself_returns_it_unchanged() {
        let icon = synthetic_icon(&["A=1", "B=2"], 4096, b"FORM....trailing");
        assert_eq!(merge_tooltypes(&icon, &icon).unwrap(), icon);
    }

    #[test]
    fn a_length_that_runs_past_the_buffer_is_refused_not_read() {
        let mut icon = synthetic_icon(&["A=1"], 4096, b"");
        // Overwrite the first tooltype's length with something enormous.
        let l = layout(&icon).unwrap();
        let at = l.tooltypes.clone().unwrap().start + 4;
        icon[at..at + 4].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(
            tooltypes(&icon).is_err(),
            "a lying length is a refusal, never a read"
        );
    }

    #[test]
    fn an_image_whose_dimensions_multiply_past_the_buffer_is_refused() {
        // A 65535 x 65535 x 8 image claims about 34 GB of plane data behind a
        // 200-byte file. Refused on the arithmetic, before anything is indexed.
        let icon = synthetic_icon_with_image(0xFFFF, 0xFFFF, 8);
        assert!(layout(&icon).is_err());
    }

    #[test]
    fn something_that_is_not_an_icon_is_refused_by_magic() {
        assert!(layout(b"not an icon at all").is_err());
        assert!(layout(&[]).is_err());

        // The two cases above are both shorter than a `DiskObject` header
        // and so are refused by the length check alone — a magic check that
        // was deleted entirely would still pass them. Prove the magic check
        // itself: a full-length, well-formed-looking buffer with the wrong
        // magic must still be refused.
        let mut wrong_magic = synthetic_icon(&[], 0, b"");
        wrong_magic[0..2].copy_from_slice(&0x1234u16.to_be_bytes());
        assert!(layout(&wrong_magic).is_err(), "wrong magic, right length");
    }

    /// Recursively collect every path under `dir` whose extension is
    /// `.info`, case-insensitively — real Amiga media is not consistent
    /// about case. Local to this one test: nothing else in this module
    /// needs to walk a directory tree.
    fn collect_info_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_info_files(&path, out);
            } else if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("info"))
            {
                out.push(path);
            }
        }
    }

    /// **The icon oracle's Rust half** (Task 11). `layout` and
    /// `merge_tooltypes` above were measured against three real `.info`
    /// files (the module doc comment's own admission). This is what checks
    /// them against far more than three, without checking any of it into
    /// the repository: `scripts/icon-oracle-check.py` extracts every
    /// `.info` from the owner's own ADFs into a scratch directory and points
    /// `ART_ICON_DIR` at it — this test never reads the owner's media
    /// directly and is a no-op (not a failure) when the variable is unset,
    /// so the ordinary suite stays green with nothing extracted.
    ///
    /// **`merge_tooltypes(x, x) == x` is only asked of icons that carry a
    /// `ToolTypes` block, and that split is measured, not assumed.** The
    /// first version of this test asked it of every icon and failed 302 of
    /// 485 against the owner's real AmigaOS 3.2 media — every single one a
    /// plain `Disk.info`, drawer icon or similar with no `ToolTypes` block
    /// at all, and every one of those 302 had `layout` succeed cleanly first
    /// (confirmed by printing the two `Result`s separately before writing
    /// this comment). `merge_tooltypes`'s own doc comment already says why:
    /// "an icon with no `ToolTypes` at all is a real but different shape
    /// this function does not attempt to grow a new block for" — it is
    /// documented, not a bug, and real material turns out to be *mostly*
    /// that shape. Folding it into `failed` would be exactly the confident-
    /// wrong sentence CLAUDE.md's "failure that does not crash" section
    /// warns about: a report of "62% of icons are broken" over a parser that
    /// never once failed to parse. So an icon with no `ToolTypes` block is
    /// counted in `no_tooltypes`, printed on its own line, and never in
    /// `failed`.
    ///
    /// What **is** unconditional, for every icon regardless of shape: `layout`
    /// itself must not error, and its `trailing` range must run to the end
    /// of the buffer (true by construction, asserted anyway so a future
    /// change to `IconLayout` that broke it would be caught here too, not
    /// only by the synthetic fixtures above) — the "lands exactly at
    /// end-of-file or at the start of a trailing IFF block" claim this test
    /// exists to check.
    ///
    /// A file that does not parse, whose merge does not round-trip, or
    /// whose `trailing` region does not reach end-of-file is not a panic: it
    /// is recorded by name in `failed` and the whole test fails once, at the
    /// end, printing every one of them — machine-readable (`ART_ICON_RESULT
    /// checked=… failed=… no_tooltypes=…`, one `ART_ICON_FAIL <path>` per
    /// miss) so the driving script can report them without scraping prose.
    #[test]
    #[ignore = "needs a folder of real .info files"]
    fn round_trip_every_icon_in_a_folder_when_asked() {
        let Ok(folder) = std::env::var("ART_ICON_DIR") else {
            return;
        };
        let mut entries = Vec::new();
        collect_info_files(std::path::Path::new(&folder), &mut entries);
        entries.sort();

        let mut checked = 0usize;
        let mut no_tooltypes = 0usize;
        let mut failed: Vec<String> = Vec::new();
        for entry in &entries {
            let bytes = match std::fs::read(entry) {
                Ok(bytes) => bytes,
                Err(err) => {
                    failed.push(format!("{} (could not read: {err})", entry.display()));
                    continue;
                }
            };
            checked += 1;

            let parsed = match layout(&bytes) {
                Ok(l) => l,
                Err(err) => {
                    failed.push(format!("{}: layout failed: {err}", entry.display()));
                    continue;
                }
            };
            // The layout has to land on the file's end, or on the start of
            // an appended ColorIcon block — never mid-file.
            if parsed.trailing.end != bytes.len() {
                failed.push(format!(
                    "{}: trailing region does not run to end-of-file",
                    entry.display()
                ));
                continue;
            }
            if parsed.tooltypes.is_none() {
                // A real, common shape — see the doc comment above — not a
                // reason to call `merge_tooltypes` at all.
                no_tooltypes += 1;
                continue;
            }
            match merge_tooltypes(&bytes, &bytes) {
                Ok(same) if same == bytes => {}
                Ok(_) => failed.push(format!(
                    "{}: merge_tooltypes(x, x) did not return x byte for byte",
                    entry.display()
                )),
                Err(err) => failed.push(format!(
                    "{}: merge_tooltypes failed: {err}",
                    entry.display()
                )),
            }
        }

        println!(
            "ART_ICON_RESULT checked={checked} failed={} no_tooltypes={no_tooltypes}",
            failed.len()
        );
        for f in &failed {
            println!("ART_ICON_FAIL {f}");
        }
        assert!(
            failed.is_empty(),
            "{} icon(s) did not round-trip",
            failed.len()
        );
    }
}
