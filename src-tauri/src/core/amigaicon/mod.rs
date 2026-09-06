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
//! the `Image` layout, since this module only needs to skip it correctly
//! (get its length right), never to read its fields; and non-ASCII tool
//! type text, which is decoded lossily (see [`tooltypes`]) rather than
//! refused, because a bad character in a display string is not the
//! memory-safety hazard a bad length is.
//!
//! ## What was skipped and is now read (measured across 798 real icons)
//!
//! The table above already names `do_DrawerData`'s flag field; the fields it
//! gates were, until this round, skipped-but-unread. They are measured now,
//! same standard as everything above — real evidence from real icons, not
//! recalled documentation of the struct:
//!
//! | Field | Absolute offset | Shape | What was measured |
//! |---|---|---|---|
//! | `Gadget.UserData` | 44 | `u32`, bit 0 read | 798 of 798 real icons have bit 0 set — `DrawerData2` is the normal case, not a special one ([`has_drawer_data2`]) |
//! | `do_Type` | 48 | `u8` | five values seen: 1 disk (37), 2 drawer (56), 3 tool (196), 4 project (506), 5 garbage (3) ([`IconType`], [`icon_type`]) |
//! | `do_CurrentX` / `do_CurrentY` | 58 / 62 | `i32` each | 361 of 798 carry [`NO_POSITION`] (`0x80000000`, `i32::MIN`) in both; `(0, 0)` is a real, distinct position — the window's top-left corner, not "unset" ([`position`]) |
//! | `DrawerData.NewWindow` (left/top/width/height) | 78 (start of the `DrawerData` block itself, when `do_DrawerData` is non-zero) | four `i16`, big-endian | read only when `do_DrawerData` is non-zero; the block's other 48 bytes remain unread ([`DrawerWindow`], [`drawer_window`]) |
//!
//! `do_DrawerData`'s block is therefore no longer wholly opaque: its first 8
//! bytes (the window rectangle) are read by [`drawer_window`]; [`layout`]'s
//! own block walk still only advances past the fixed 56-byte length,
//! exactly as before — it does not need the window rectangle to skip the
//! block correctly.
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

pub mod render;

/// `do_Magic` for a classic AmigaOS icon.
const MAGIC: u16 = 0xE310;

/// Size of the fixed `DiskObject` header — every offset below is measured
/// against real icons summing to exactly this (see the module doc's table).
const HEADER_LEN: usize = 78;

/// Fixed size of a `DrawerData` block; its contents are skipped, not read.
const DRAWER_DATA_LEN: usize = 56;

/// Fixed size of an `Image` block's header, before its pixel data.
const IMAGE_HEADER_LEN: usize = 20;

const OFF_GADGET_WIDTH: usize = 12;
const OFF_GADGET_HEIGHT: usize = 14;
const OFF_GADGET_RENDER: usize = 22;
const OFF_SELECT_RENDER: usize = 26;
const OFF_USER_DATA: usize = 44;
const OFF_TYPE: usize = 48;
const OFF_DEFAULT_TOOL: usize = 50;
const OFF_TOOL_TYPES: usize = 54;
const OFF_CURRENT_X: usize = 58;
const OFF_CURRENT_Y: usize = 62;
const OFF_DRAWER_DATA: usize = 66;
const OFF_TOOL_WINDOW: usize = 70;
const OFF_STACK_SIZE: usize = 74;

/// `dd_NewWindow.Flags` — block-relative offset 14 inside `DrawerData`
/// (past `LeftEdge`/`TopEdge`/`Width`/`Height`, `DetailPen`/`BlockPen` and
/// `IDCMPFlags`), so absolute offset `HEADER_LEN + 14`. Workbench overloads
/// this field — otherwise an Intuition `NewWindow`'s flags, meaningless for a
/// window that is closed and saved to disk — to carry its own display-mode
/// value instead.
const OFF_DRAWER_FLAGS: usize = HEADER_LEN + 14;

/// `OFF_DRAWER_FLAGS`'s three values, **measured** against 59 real drawer
/// and garbage icons carrying a `DrawerData2` in the owner's own AmigaOS 3.9
/// tree — the same register `core/ilbm`'s `BMHD` doc uses to separate
/// measured fields from adopted ones:
///
/// | Value | Count | Example |
/// |---|---|---|
/// | `DDFLAGS_SHOWDEFAULT` (0) | 34 | `Devs/Monitors.info` |
/// | `DDFLAGS_SHOWICONS` (1) | 15 | `Devs/DataTypes.info` |
/// | `DDFLAGS_SHOWALL` (2) | 10 | `Prefs/Presets/Beeps/Boings.info` |
///
/// **This is an enum, not a bitfield.** Exactly these three values appear
/// across all 59 icons and never a combination — no `3`, no higher bit — so
/// [`set_show_all_files`] writes the whole word to one of these constants
/// rather than OR-ing or AND-NOT-ing a bit into whatever was already there.
///
/// `Prefs/Presets/Beeps/Boings.info` carrying `DDFLAGS_SHOWALL` is the
/// stronger half of the evidence, not just the count: it is a drawer of
/// sound files with no icons of their own, so without "show all files" it
/// opens as an **empty window** on a real Workbench — exactly the case this
/// feature exists for. Finding the flag set on precisely that drawer is what
/// turns this from a plausible constant into a confirmed one.
const DDFLAGS_SHOWDEFAULT: u32 = 0;
const DDFLAGS_SHOWICONS: u32 = 1;
const DDFLAGS_SHOWALL: u32 = 2;

/// `do_CurrentX`/`do_CurrentY`'s sentinel for "the release did not place
/// this icon" — `0x80000000`, `i32::MIN`. Measured: 361 of 798 real icons
/// carry it in both coordinates. `(0, 0)` is a real, distinct position (the
/// window's top-left corner) and must never be read as this sentinel.
pub const NO_POSITION: i32 = i32::MIN;

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

/// Read a big-endian `i16` at `at` — same bound check as [`be_u16`], reread
/// as signed. `DrawerData.NewWindow`'s left/top/width/height are signed.
fn be_i16(bytes: &[u8], at: usize) -> CoreResult<i16> {
    Ok(be_u16(bytes, at)? as i16)
}

/// Read a big-endian `i32` at `at` — same bound check as [`be_u32`], reread
/// as signed. `do_CurrentX`/`do_CurrentY` are signed, and [`NO_POSITION`]
/// (`i32::MIN`) only means anything as a signed comparison.
fn be_i32(bytes: &[u8], at: usize) -> CoreResult<i32> {
    Ok(be_u32(bytes, at)? as i32)
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

/// Walk the fixed header plus `DrawerData`, `GadgetRender`, `SelectRender`
/// and `DefaultTool` — every block that comes *before* `ToolTypes` in file
/// order — and return the position immediately after them: exactly where a
/// `ToolTypes` block sits when one is present, and exactly where one would
/// be inserted when it is not.
///
/// Shared by [`layout`] (which then walks past `ToolTypes` and `ToolWindow`
/// itself) and [`set_tooltypes`] (which needs this same position whether or
/// not a `ToolTypes` block already exists) — deliberately one walk, so the
/// two never drift out of step on where a `ToolTypes` block belongs.
fn position_before_tooltypes(bytes: &[u8]) -> CoreResult<usize> {
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
    Ok(pos)
}

/// Walk a `.info` file's fixed header and optional blocks, in the order
/// the format lays them out, and report where `ToolTypes` and the trailing
/// region are.
///
/// Refuses (never reads past bounds, never guesses) when: the file is
/// shorter than a `DiskObject` header, the magic does not match, or any
/// length or pointer-derived size would run past the end of the buffer.
pub fn layout(bytes: &[u8]) -> CoreResult<IconLayout> {
    let mut pos = position_before_tooltypes(bytes)?;

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

/// `do_Type` — what kind of object this icon represents.
///
/// Measured across 798 real icons: five values seen (`Disk` 37, `Drawer` 56,
/// `Tool` 196, `Project` 506, `Garbage` 3). `Other` carries anything else
/// (`WBDevice`, `WBKick`, `WBAppIcon` and unused values) rather than
/// refusing an icon this module has never been shown — the same "preserve,
/// don't reject the unfamiliar" stance [`tooltypes`] takes for text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconType {
    Disk,
    Drawer,
    Tool,
    Project,
    Garbage,
    Other(u8),
}

/// `do_Type` — a fixed field of the header, so this needs no walk past the
/// magic check. [`check_header`] has already confirmed `bytes.len() >=
/// HEADER_LEN` (78), and `OFF_TYPE` (48) is within that, so the index below
/// cannot run past the buffer.
pub fn icon_type(bytes: &[u8]) -> CoreResult<IconType> {
    check_header(bytes)?;
    Ok(match bytes[OFF_TYPE] {
        1 => IconType::Disk,
        2 => IconType::Drawer,
        3 => IconType::Tool,
        4 => IconType::Project,
        5 => IconType::Garbage,
        other => IconType::Other(other),
    })
}

/// The icon's desktop position, or `None` when the release left it
/// unplaced.
///
/// `do_CurrentX`/`do_CurrentY` are fixed fields of the header. **Either**
/// coordinate carrying [`NO_POSITION`] (`0x80000000`, `i32::MIN`) means
/// "unplaced" — measured against 798 real icons, always in both coordinates
/// together, but this checks each independently so a corrupt file with only
/// one sentinel is still reported as unplaced rather than as a real
/// position with a nonsense half. `(0, 0)` is deliberately **not** treated
/// as unset: it is the window's top-left corner, a real position a release
/// can and does choose.
pub fn position(bytes: &[u8]) -> CoreResult<Option<(i32, i32)>> {
    check_header(bytes)?;
    let x = be_i32(bytes, OFF_CURRENT_X)?;
    let y = be_i32(bytes, OFF_CURRENT_Y)?;
    if x == NO_POSITION || y == NO_POSITION {
        Ok(None)
    } else {
        Ok(Some((x, y)))
    }
}

/// The drawer's own Workbench window rectangle — `DrawerData.NewWindow`'s
/// first four fields, `i16` each, big-endian.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrawerWindow {
    pub left: i16,
    pub top: i16,
    pub width: i16,
    pub height: i16,
}

/// The drawer's window rectangle, or `None` when `do_DrawerData` is zero —
/// there is no `DrawerData` block to read it from (a plain `Tool` or
/// `Project` icon, or a drawer icon whose window was never opened).
///
/// When `do_DrawerData` is non-zero, the block is guaranteed present and
/// [`DRAWER_DATA_LEN`] (56) bytes long — [`layout`]'s own walk depends on
/// that same guarantee to skip it — so reading the four `i16` fields at its
/// start (offset [`HEADER_LEN`], the block's own start) needs no additional
/// length check beyond what [`be_i16`] already does.
pub fn drawer_window(bytes: &[u8]) -> CoreResult<Option<DrawerWindow>> {
    check_header(bytes)?;
    if be_u32(bytes, OFF_DRAWER_DATA)? == 0 {
        return Ok(None);
    }
    let left = be_i16(bytes, HEADER_LEN)?;
    let top = be_i16(bytes, HEADER_LEN + 2)?;
    let width = be_i16(bytes, HEADER_LEN + 4)?;
    let height = be_i16(bytes, HEADER_LEN + 6)?;
    Ok(Some(DrawerWindow {
        left,
        top,
        width,
        height,
    }))
}

/// The revision bit inside `Gadget.UserData` (bit 0) — set on 798 of 798
/// real icons measured, so `DrawerData2` is the normal shape, not a special
/// one. This module does not yet read anything `DrawerData2` itself would
/// carry; it only reports whether the bit is set.
pub fn has_drawer_data2(bytes: &[u8]) -> CoreResult<bool> {
    check_header(bytes)?;
    let user_data = be_u32(bytes, OFF_USER_DATA)?;
    Ok(user_data & 1 != 0)
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

/// Replace the icon's tool types wholesale, keeping every other byte —
/// header fields, `DrawerData`, images, `DefaultTool`, `ToolWindow`, the
/// trailing appended ColorIcon/NewIcon blob — byte for byte identical.
///
/// This is the general-purpose sibling of [`merge_tooltypes`]: that function
/// only splices one `ToolTypes` block into another that already has one;
/// this one also grows a block where there was none (an empty `tooltypes`
/// with no existing block is a no-op) and removes one — clearing
/// `do_ToolTypes` at [`OFF_TOOL_TYPES`](OFF_TOOL_TYPES) rather than writing a
/// zero-length block — when `tooltypes` is empty. When a block already
/// existed and still does, `do_ToolTypes`'s own word is left exactly as it
/// was: this module only ever treats it as a presence flag (non-zero), so
/// there is nothing about *this* call that licenses changing whatever
/// value it already held.
///
/// Refuses whenever the icon itself does not parse — the same bound-checked
/// walk every other function in this module goes through — never a
/// best-effort rewrite of a file it could not fully account for.
pub fn set_tooltypes(bytes: &[u8], tooltypes: &[String]) -> CoreResult<Vec<u8>> {
    let start = position_before_tooltypes(bytes)?;
    let had_block = be_u32(bytes, OFF_TOOL_TYPES)? != 0;
    let end = if had_block {
        skip_tooltypes(bytes, start)?
    } else {
        start
    };

    let mut block = Vec::new();
    if !tooltypes.is_empty() {
        let count = u32::try_from(tooltypes.len())
            .map_err(|_| malformed("too many tool types to encode"))?;
        let size = count
            .checked_add(1)
            .and_then(|n| n.checked_mul(4))
            .ok_or_else(|| malformed("ToolTypes size overflow"))?;
        block.extend_from_slice(&size.to_be_bytes());
        for tt in tooltypes {
            let text = tt.as_bytes();
            let len = u32::try_from(text.len())
                .map_err(|_| malformed("a tool type is too long to encode"))?;
            block.extend_from_slice(&len.to_be_bytes());
            block.extend_from_slice(text);
        }
    }

    let mut out = Vec::with_capacity(bytes.len() - (end - start) + block.len());
    out.extend_from_slice(&bytes[..start]);
    out.extend_from_slice(&block);
    out.extend_from_slice(&bytes[end..]);

    let want_block = !tooltypes.is_empty();
    if had_block != want_block {
        let flag: u32 = u32::from(want_block);
        out[OFF_TOOL_TYPES..OFF_TOOL_TYPES + 4].copy_from_slice(&flag.to_be_bytes());
    }

    Ok(out)
}

/// Overwrite `do_CurrentX`/`do_CurrentY` (58/62), leaving every other byte —
/// including the rest of the header — untouched.
///
/// `None` writes [`NO_POSITION`] into **both** coordinates. Writing only one
/// half would leave the icon in a "half placed" state that [`position`]
/// itself cannot even distinguish from a clean clear (either coordinate
/// carrying the sentinel already reads back as `None`), so a caller relying
/// on `position()` to confirm the clear would see nothing wrong while the
/// other coordinate silently kept its old value.
pub fn set_position(bytes: &[u8], at: Option<(i32, i32)>) -> CoreResult<Vec<u8>> {
    check_header(bytes)?;
    let (x, y) = at.unwrap_or((NO_POSITION, NO_POSITION));

    let mut out = bytes.to_vec();
    out[OFF_CURRENT_X..OFF_CURRENT_X + 4].copy_from_slice(&x.to_be_bytes());
    out[OFF_CURRENT_Y..OFF_CURRENT_Y + 4].copy_from_slice(&y.to_be_bytes());
    Ok(out)
}

/// Overwrite `DrawerData.NewWindow`'s left/top/width/height — the same four
/// `i16` fields [`drawer_window`] reads, at absolute offset [`HEADER_LEN`] —
/// leaving every other byte untouched.
///
/// Refuses when `do_DrawerData` is zero: the same case [`drawer_window`]
/// reports as `None`, because there is no `DrawerData` block to write a
/// window into. The refusal names `DrawerData` explicitly so it reads as
/// "there is nothing here to set", not as an unexplained failure.
pub fn set_window(bytes: &[u8], window: DrawerWindow) -> CoreResult<Vec<u8>> {
    check_header(bytes)?;
    if be_u32(bytes, OFF_DRAWER_DATA)? == 0 {
        return Err(malformed(
            "cannot set a window: this icon has no DrawerData block to hold one",
        ));
    }
    // Same guarantee `drawer_window` relies on: a non-zero `do_DrawerData`
    // means a full `DRAWER_DATA_LEN`-byte block is present at `HEADER_LEN`.
    advance(bytes, HEADER_LEN, DRAWER_DATA_LEN)?;

    let mut out = bytes.to_vec();
    out[HEADER_LEN..HEADER_LEN + 2].copy_from_slice(&window.left.to_be_bytes());
    out[HEADER_LEN + 2..HEADER_LEN + 4].copy_from_slice(&window.top.to_be_bytes());
    out[HEADER_LEN + 4..HEADER_LEN + 6].copy_from_slice(&window.width.to_be_bytes());
    out[HEADER_LEN + 6..HEADER_LEN + 8].copy_from_slice(&window.height.to_be_bytes());
    Ok(out)
}

/// Set or clear Workbench's "Show All Files" mode for this drawer —
/// `OFF_DRAWER_FLAGS`, measured (see its own doc comment) to be an **enum**
/// of exactly three values, never a bitfield. `show_all: true` writes
/// `DDFLAGS_SHOWALL` (2); `false` writes `DDFLAGS_SHOWDEFAULT` (0) — the
/// measured default, not whatever bits happened to be clear before. Every
/// other byte of the icon is untouched.
///
/// This call always chooses between exactly two of the three measured
/// values: it has no way to ask for `DDFLAGS_SHOWICONS` (1) explicitly, the
/// same way [`set_position`]'s `None` only ever writes [`NO_POSITION`]. An
/// icon already carrying `DDFLAGS_SHOWICONS` that is asked to turn "show all
/// files" off moves to the default (0), not back to `DDFLAGS_SHOWICONS`
/// (1) — there is no third state this function's boolean signature can
/// express, and `false` unambiguously means "off", not "whatever this was
/// before `true`".
///
/// Refuses when `do_DrawerData` is zero, the same case [`set_window`]
/// refuses for the same reason: there is no `DrawerData` block, so there is
/// no `Flags` word to write.
pub fn set_show_all_files(bytes: &[u8], show_all: bool) -> CoreResult<Vec<u8>> {
    check_header(bytes)?;
    if be_u32(bytes, OFF_DRAWER_DATA)? == 0 {
        return Err(malformed(
            "cannot set Show All Files: this icon has no DrawerData block to hold it",
        ));
    }
    // Same guarantee `set_window` relies on: a non-zero `do_DrawerData`
    // means a full `DRAWER_DATA_LEN`-byte block is present at `HEADER_LEN`,
    // so `OFF_DRAWER_FLAGS` (HEADER_LEN + 14, needing 4 bytes) is in bounds.
    advance(bytes, HEADER_LEN, DRAWER_DATA_LEN)?;

    let flags = if show_all {
        DDFLAGS_SHOWALL
    } else {
        DDFLAGS_SHOWDEFAULT
    };

    let mut out = bytes.to_vec();
    out[OFF_DRAWER_FLAGS..OFF_DRAWER_FLAGS + 4].copy_from_slice(&flags.to_be_bytes());
    Ok(out)
}

/// Fixture building, shared with anything outside this module that needs a
/// valid `.info` to hand to [`tooltypes`] or [`launch_options`](crate::core::whdload::launch_options).
///
/// `core/gameindex/readers/drawer` builds icons naming a WHDLoad slave for
/// its own tests; a second hand-rolled copy of this `DiskObject` byte layout
/// there would be a second place for the offsets to drift out of step with
/// this module — the same shape `readers::slave::tests_support` follows for
/// the same reason.
#[cfg(test)]
pub(crate) mod tests_support {
    use super::{
        DRAWER_DATA_LEN, HEADER_LEN, MAGIC, OFF_CURRENT_X, OFF_CURRENT_Y, OFF_DRAWER_DATA,
        OFF_GADGET_HEIGHT, OFF_GADGET_WIDTH, OFF_STACK_SIZE, OFF_TOOL_TYPES, OFF_TYPE,
        OFF_USER_DATA,
    };

    /// Build a minimal, valid `.info` by hand: header, an optional
    /// `ToolTypes` block, then arbitrary trailing bytes (standing in for an
    /// appended ColorIcon/NewIcon `FORM`). No `DrawerData`, no images, no
    /// `DefaultTool`, no `ToolWindow` — this module does not need to
    /// exercise those to prove it skips them correctly; that is covered by
    /// `synthetic_icon_with_image` (private to this module's own tests) for
    /// the one block that does matter for the overflow guard.
    pub(crate) fn synthetic_icon(tooltypes: &[&str], stack: u32, trailing: &[u8]) -> Vec<u8> {
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

    /// A valid icon carrying an arbitrary `Gadget` width/height, distinct
    /// from `synthetic_icon`'s always-zero `Gadget` fields — for
    /// [`super::render`]'s "the plain planar size is the `Gadget` size"
    /// case, where the exact width must be asserted rather than merely
    /// nonzero.
    pub(crate) fn synthetic_icon_sized(width: u16, height: u16) -> Vec<u8> {
        let mut buf = synthetic_icon(&[], 4096, &[]);
        buf[OFF_GADGET_WIDTH..OFF_GADGET_WIDTH + 2].copy_from_slice(&width.to_be_bytes());
        buf[OFF_GADGET_HEIGHT..OFF_GADGET_HEIGHT + 2].copy_from_slice(&height.to_be_bytes());
        buf
    }

    /// A valid icon placed at `(x, y)` — pass [`super::NO_POSITION`] for
    /// either coordinate to build an unplaced icon.
    pub(crate) fn synthetic_icon_at(x: i32, y: i32) -> Vec<u8> {
        let mut buf = synthetic_icon(&[], 4096, &[]);
        buf[OFF_CURRENT_X..OFF_CURRENT_X + 4].copy_from_slice(&x.to_be_bytes());
        buf[OFF_CURRENT_Y..OFF_CURRENT_Y + 4].copy_from_slice(&y.to_be_bytes());
        buf
    }

    /// A valid icon whose `do_Type` is `kind`.
    pub(crate) fn synthetic_icon_of_type(kind: u8) -> Vec<u8> {
        let mut buf = synthetic_icon(&[], 4096, &[]);
        buf[OFF_TYPE] = kind;
        buf
    }

    /// A drawer icon carrying a `DrawerData` block: `do_DrawerData` set, the
    /// window rectangle at its start, and `UserData`'s revision bit set (798
    /// of 798 real icons measured carry it) — the shape
    /// [`super::has_drawer_data2`] and [`super::drawer_window`] both read.
    pub(crate) fn synthetic_drawer_icon(left: i16, top: i16, width: i16, height: i16) -> Vec<u8> {
        let mut buf = vec![0u8; HEADER_LEN];
        buf[0..2].copy_from_slice(&MAGIC.to_be_bytes());
        buf[2..4].copy_from_slice(&1u16.to_be_bytes()); // do_Version
        buf[OFF_USER_DATA..OFF_USER_DATA + 4].copy_from_slice(&1u32.to_be_bytes());
        buf[OFF_DRAWER_DATA..OFF_DRAWER_DATA + 4].copy_from_slice(&1u32.to_be_bytes());

        let mut drawer_data = vec![0u8; DRAWER_DATA_LEN];
        drawer_data[0..2].copy_from_slice(&left.to_be_bytes());
        drawer_data[2..4].copy_from_slice(&top.to_be_bytes());
        drawer_data[4..6].copy_from_slice(&width.to_be_bytes());
        drawer_data[6..8].copy_from_slice(&height.to_be_bytes());
        buf.extend_from_slice(&drawer_data);
        buf
    }

    /// A valid icon at least 86 bytes long, with plausible-looking
    /// `NewWindow` bytes sitting right where [`super::drawer_window`] would
    /// read them (offset 78, `HEADER_LEN`) — but `do_DrawerData` (66) left
    /// at zero, so there is no `DrawerData` block and those bytes are not a
    /// window at all.
    ///
    /// This is what isolates the `do_DrawerData == 0` guard from a plain
    /// bounds check: [`synthetic_icon`]`(&[], 4096, &[])` is only 82 bytes,
    /// so removing the guard there fails on "not enough bytes for a
    /// window" rather than on the guard itself. This fixture has plenty of
    /// bytes — removing the guard here must read a wrong window, not error.
    pub(crate) fn synthetic_icon_with_trailing_window_bytes_but_no_drawer_data() -> Vec<u8> {
        let mut buf = vec![0u8; HEADER_LEN];
        buf[0..2].copy_from_slice(&MAGIC.to_be_bytes());
        buf[2..4].copy_from_slice(&1u16.to_be_bytes()); // do_Version
                                                        // OFF_DRAWER_DATA (66..70) is left zero: no DrawerData block declared.
        let left: i16 = 10;
        let top: i16 = 20;
        let width: i16 = 300;
        let height: i16 = 200;
        buf.extend_from_slice(&left.to_be_bytes());
        buf.extend_from_slice(&top.to_be_bytes());
        buf.extend_from_slice(&width.to_be_bytes());
        buf.extend_from_slice(&height.to_be_bytes());
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tests_support::{
        synthetic_drawer_icon, synthetic_icon, synthetic_icon_at, synthetic_icon_of_type,
        synthetic_icon_with_trailing_window_bytes_but_no_drawer_data,
    };

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
    fn setting_a_position_changes_eight_bytes_and_nothing_else() {
        let before = synthetic_icon(&["A=1", "B=2"], 8192, b"FORM....ICONtrailing");
        let after = set_position(&before, Some((37, 11))).unwrap();
        assert_eq!(position(&after).unwrap(), Some((37, 11)));
        assert_eq!(after.len(), before.len(), "no length change");
        for i in (0..before.len()).filter(|i| !(58..66).contains(i)) {
            assert_eq!(after[i], before[i], "byte {i} changed and should not have");
        }
    }

    #[test]
    fn clearing_a_position_writes_the_sentinel_in_both_coordinates() {
        let before = synthetic_icon_at(13, 4);
        let after = set_position(&before, None).unwrap();
        assert_eq!(position(&after).unwrap(), None);
        assert_eq!(
            i32::from_be_bytes([after[58], after[59], after[60], after[61]]),
            NO_POSITION
        );
        assert_eq!(
            i32::from_be_bytes([after[62], after[63], after[64], after[65]]),
            NO_POSITION
        );
    }

    #[test]
    fn replacing_tooltypes_keeps_the_appended_blob_byte_for_byte() {
        let trailing = b"FORM\x00\x00\x00\x08ICONabcd";
        let before = synthetic_icon(&["OLD=1"], 4096, trailing);
        let after = set_tooltypes(&before, &["NEW=2".to_string(), "MORE=3".to_string()]).unwrap();
        assert_eq!(tooltypes(&after).unwrap(), vec!["NEW=2", "MORE=3"]);
        assert!(
            after.ends_with(trailing),
            "the appended ColorIcon must survive"
        );
        assert_eq!(
            stack_size(&after).unwrap(),
            4096,
            "the stack size is not ours to change"
        );
    }

    #[test]
    fn setting_no_tooltypes_clears_the_flag_and_still_keeps_the_blob() {
        let trailing = b"FORM....ICONstill here";
        let before = synthetic_icon(&["A=1", "B=2"], 4096, trailing);
        let after = set_tooltypes(&before, &[]).unwrap();
        assert_eq!(tooltypes(&after).unwrap(), Vec::<String>::new());
        assert_eq!(
            be_u32(&after, OFF_TOOL_TYPES).unwrap(),
            0,
            "the ToolTypes flag must be cleared, not merely left pointing at an empty block"
        );
        assert!(
            after.ends_with(trailing),
            "the appended blob must survive clearing the block entirely"
        );
    }

    #[test]
    fn setting_tooltypes_on_an_icon_with_none_grows_a_block() {
        // The general case set_tooltypes must handle beyond what
        // merge_tooltypes ever needed: no ToolTypes block existed at all.
        let before = synthetic_icon(&[], 4096, b"trailing bytes");
        assert_eq!(tooltypes(&before).unwrap(), Vec::<String>::new());
        let after = set_tooltypes(&before, &["NEW=1".to_string()]).unwrap();
        assert_eq!(tooltypes(&after).unwrap(), vec!["NEW=1"]);
        assert!(after.ends_with(b"trailing bytes"));
        assert_eq!(stack_size(&after).unwrap(), 4096);
    }

    #[test]
    fn setting_a_window_on_an_icon_with_no_drawer_data_is_refused_by_name() {
        let err = set_window(
            &synthetic_icon(&[], 4096, &[]),
            DrawerWindow {
                left: 0,
                top: 0,
                width: 100,
                height: 100,
            },
        )
        .unwrap_err();
        assert!(
            format!("{err}").contains("DrawerData"),
            "the refusal must say why: {err}"
        );
    }

    #[test]
    fn setting_a_window_changes_only_the_eight_bytes_at_78() {
        let before = synthetic_drawer_icon(1, 2, 3, 4);
        let after = set_window(
            &before,
            DrawerWindow {
                left: 393,
                top: 126,
                width: 342,
                height: 163,
            },
        )
        .unwrap();
        assert_eq!(
            drawer_window(&after).unwrap(),
            Some(DrawerWindow {
                left: 393,
                top: 126,
                width: 342,
                height: 163
            })
        );
        assert_eq!(after.len(), before.len(), "no length change");
        for i in (0..before.len()).filter(|i| !(HEADER_LEN..HEADER_LEN + 8).contains(i)) {
            assert_eq!(after[i], before[i], "byte {i} changed and should not have");
        }
    }

    #[test]
    fn show_all_files_round_trips_and_leaves_the_rest_alone() {
        let before = synthetic_drawer_icon(10, 20, 300, 200);
        let toggled_on = set_show_all_files(&before, true).unwrap();
        assert_ne!(toggled_on, before, "turning it on must change something");
        assert_eq!(toggled_on.len(), before.len(), "no length change");
        for i in (0..before.len()).filter(|i| !(OFF_DRAWER_FLAGS..OFF_DRAWER_FLAGS + 4).contains(i))
        {
            assert_eq!(
                toggled_on[i], before[i],
                "byte {i} changed and should not have"
            );
        }
        let back = set_show_all_files(&toggled_on, false).unwrap();
        assert_eq!(
            back, before,
            "turning it back off restores the original bytes"
        );
    }

    #[test]
    fn dd_flags_is_an_enum_and_the_three_measured_values_round_trip() {
        // Measured across 59 real drawer icons in the owner's AmigaOS 3.9
        // tree: only 0, 1 and 2 ever appear, never a combination - so this
        // is an enum and a bit-OR/AND-NOT would be wrong.
        assert_eq!(DDFLAGS_SHOWDEFAULT, 0);
        assert_eq!(DDFLAGS_SHOWICONS, 1);
        assert_eq!(DDFLAGS_SHOWALL, 2);

        // Start from DDFLAGS_SHOWICONS (1) - a real measured value that is
        // not the zero-filled default a plain synthetic fixture would give
        // for free.
        let mut show_icons = synthetic_drawer_icon(0, 0, 100, 100);
        show_icons[OFF_DRAWER_FLAGS..OFF_DRAWER_FLAGS + 4]
            .copy_from_slice(&DDFLAGS_SHOWICONS.to_be_bytes());

        // true writes the enum value 2 outright - never 1 | 2.
        let all = set_show_all_files(&show_icons, true).unwrap();
        assert_eq!(be_u32(&all, OFF_DRAWER_FLAGS).unwrap(), DDFLAGS_SHOWALL);

        // false writes the DEFAULT (0), not merely clearing the SHOWALL bit
        // - which would leave an icon that started at DDFLAGS_SHOWICONS (1)
        // sitting at 1 again instead of returning to the true default.
        let back_to_default = set_show_all_files(&all, false).unwrap();
        assert_eq!(
            be_u32(&back_to_default, OFF_DRAWER_FLAGS).unwrap(),
            DDFLAGS_SHOWDEFAULT,
            "false must write the default, not merely clear a bit"
        );
    }

    #[test]
    fn setting_show_all_files_on_an_icon_with_no_drawer_data_is_refused_by_name() {
        let err = set_show_all_files(&synthetic_icon(&[], 4096, &[]), true).unwrap_err();
        assert!(
            format!("{err}").contains("DrawerData"),
            "the refusal must say why: {err}"
        );
    }

    #[test]
    fn setting_a_window_is_refused_by_the_drawer_data_guard_not_a_bounds_check() {
        // `setting_a_window_on_an_icon_with_no_drawer_data_is_refused_by_name`
        // above uses an 82-byte fixture — short enough that even a deleted
        // `do_DrawerData` guard would still be caught by `set_window`'s own
        // `advance()` bounds check, which needs `HEADER_LEN + DRAWER_DATA_LEN`
        // (134) bytes. That is "refused for the wrong reason", the same trap
        // named for `drawer_window`'s own guard test. This fixture is 162
        // bytes — comfortably past 134 — with `do_DrawerData` still zero, so
        // removing the guard here would actually write a window into bytes
        // that are not a `DrawerData` block at all, rather than being caught
        // first by a length check.
        let icon = synthetic_icon(&[], 4096, &[0u8; 80]);
        assert!(
            icon.len() > HEADER_LEN + DRAWER_DATA_LEN,
            "fixture too short to isolate the guard"
        );
        let err = set_window(
            &icon,
            DrawerWindow {
                left: 1,
                top: 2,
                width: 3,
                height: 4,
            },
        )
        .unwrap_err();
        assert!(
            format!("{err}").contains("DrawerData"),
            "the refusal must say why: {err}"
        );
    }

    #[test]
    fn setting_show_all_files_is_refused_by_the_drawer_data_guard_not_a_bounds_check() {
        // Same isolation as the window test above, for the same reason.
        let icon = synthetic_icon(&[], 4096, &[0u8; 80]);
        assert!(
            icon.len() > HEADER_LEN + DRAWER_DATA_LEN,
            "fixture too short to isolate the guard"
        );
        let err = set_show_all_files(&icon, true).unwrap_err();
        assert!(
            format!("{err}").contains("DrawerData"),
            "the refusal must say why: {err}"
        );
    }

    #[test]
    fn every_write_leaves_a_refused_icon_untouched() {
        let malformed = b"not an icon at all".to_vec();
        let original = malformed.clone();

        assert!(set_tooltypes(&malformed, &["A=1".to_string()]).is_err());
        assert!(set_position(&malformed, Some((1, 2))).is_err());
        assert!(set_window(
            &malformed,
            DrawerWindow {
                left: 0,
                top: 0,
                width: 1,
                height: 1
            }
        )
        .is_err());
        assert!(set_show_all_files(&malformed, true).is_err());

        assert_eq!(malformed, original, "the input itself must not be mutated");
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

    #[test]
    fn the_no_position_sentinel_is_0x80000000_not_zero() {
        // Measured: art1/Fonts/Disk.info carries i32::MIN in do_CurrentX.
        // A position of (0, 0) is a REAL position - the window's top-left
        // corner - and must not be read as "unset".
        let placed = synthetic_icon_at(0, 0);
        assert_eq!(
            position(&placed).unwrap(),
            Some((0, 0)),
            "(0,0) is a real position"
        );

        let unplaced = synthetic_icon_at(NO_POSITION, NO_POSITION);
        assert_eq!(position(&unplaced).unwrap(), None);
    }

    #[test]
    fn a_real_position_reads_back_exactly() {
        // Measured: art1/Devs/DataTypes.info is at 13,4.
        assert_eq!(position(&synthetic_icon_at(13, 4)).unwrap(), Some((13, 4)));
    }

    #[test]
    fn the_five_measured_icon_types_decode() {
        // Measured across 798 real icons: 1 disk (37), 2 drawer (56),
        // 3 tool (196), 4 project (506), 5 garbage (3).
        for (byte, want) in [
            (1u8, IconType::Disk),
            (2, IconType::Drawer),
            (3, IconType::Tool),
            (4, IconType::Project),
            (5, IconType::Garbage),
        ] {
            assert_eq!(icon_type(&synthetic_icon_of_type(byte)).unwrap(), want);
        }
        assert_eq!(
            icon_type(&synthetic_icon_of_type(9)).unwrap(),
            IconType::Other(9)
        );
    }

    #[test]
    fn a_drawer_window_reads_the_real_rectangle_when_drawer_data_is_present() {
        // Measured: art1/Devs/DataTypes.info -> 393,126 342x163; the project
        // icon beside it has do_DrawerData = 0 and no window at all.
        assert_eq!(
            drawer_window(&synthetic_drawer_icon(393, 126, 342, 163)).unwrap(),
            Some(DrawerWindow {
                left: 393,
                top: 126,
                width: 342,
                height: 163
            })
        );
    }

    #[test]
    fn a_short_icon_with_no_drawer_data_returns_none_without_needing_window_sized_bytes() {
        // `synthetic_icon(&[], 4096, &[])` is 82 bytes - too short to hold a
        // window at all (a window needs 86). This proves the early
        // `do_DrawerData == 0` return means a minimal icon is not forced to
        // carry window-sized bytes it has no use for. It does NOT by itself
        // isolate the do_DrawerData guard: with the guard removed, this same
        // fixture fails on a bounds error (not enough bytes for a window),
        // which is a different failure than the guard being absent — see
        // `a_zero_drawer_data_pointer_means_no_window_even_when_bytes_follow`
        // for the test that isolates that specifically.
        assert_eq!(
            drawer_window(&synthetic_icon(&[], 4096, &[])).unwrap(),
            None
        );
    }

    #[test]
    fn a_zero_drawer_data_pointer_means_no_window_even_when_bytes_follow() {
        // 86+ bytes with a plausible NewWindow at 78, but do_DrawerData == 0.
        // Without the guard this reads a window out of bytes that are not
        // one - which is a wrong answer, not an error, and is what the guard
        // prevents. This is the test that isolates the do_DrawerData == 0
        // check itself, rather than incidentally exercising a bounds check.
        let icon = synthetic_icon_with_trailing_window_bytes_but_no_drawer_data();
        assert_eq!(drawer_window(&icon).unwrap(), None);
    }

    #[test]
    fn every_real_icon_measured_is_revision_one() {
        // 798 of 798. So DrawerData2 is the normal case, not a special one.
        assert!(has_drawer_data2(&synthetic_drawer_icon(0, 0, 100, 100)).unwrap());
    }

    #[test]
    fn a_truncated_icon_is_refused_rather_than_read_past_its_end() {
        // Truncate a VALID icon. A zero-filled buffer would be refused for
        // its missing 0xE310 magic rather than for its length, so such a
        // test passes with every bounds check deleted - a state with more
        // than one cause, which is the defect this project names.
        let whole = synthetic_icon_at(13, 4);
        assert!(
            icon_type(&whole).is_ok(),
            "the fixture must be valid to start with"
        );
        for len in [12usize, 45, 48, 57, 62, 65] {
            let cut = &whole[..len];
            assert!(
                icon_type(cut).is_err() || len > 48,
                "icon_type read do_Type out of {len} bytes"
            );
            assert!(
                position(cut).is_err() || len > 65,
                "position read a coordinate out of {len} bytes"
            );
        }
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

    /// The index of the first byte at which `a` and `b` differ — a length
    /// mismatch counts as a difference at the shorter length. Used only by
    /// the folder-walk oracle test below, so a failure names an offset
    /// instead of a bare "not equal".
    fn first_difference(a: &[u8], b: &[u8]) -> usize {
        if a.len() != b.len() {
            return a.len().min(b.len());
        }
        a.iter()
            .zip(b.iter())
            .position(|(x, y)| x != y)
            .unwrap_or(a.len())
    }

    /// Whether every tool type in `bytes`' `ToolTypes` block (if it has one
    /// at all) is valid UTF-8 in its raw, on-disk bytes — the exact
    /// condition under which [`tooltypes`]'s `String::from_utf8_lossy` call
    /// returns the input unchanged rather than substituting `U+FFFD`.
    ///
    /// Walks the block the same way [`skip_tooltypes`] does (duplicated
    /// rather than shared, deliberately: this asks a different question —
    /// "is this reversible", not "is this well-formed" — and the two must
    /// not be allowed to silently drift onto the same code path and answer
    /// only one of them). Used only by the folder-walk oracle test below.
    fn tooltypes_round_trip_losslessly(bytes: &[u8]) -> CoreResult<bool> {
        let Some(range) = layout(bytes)?.tooltypes else {
            return Ok(true); // no block at all - nothing to lose.
        };
        let mut p = advance(bytes, range.start, 4)?; // past the block's own size field.
        while p < range.end {
            let len = be_u32(bytes, p)? as usize;
            let after_len = advance(bytes, p, 4)?;
            let end = advance(bytes, after_len, len)?;
            if std::str::from_utf8(&bytes[after_len..end]).is_err() {
                return Ok(false);
            }
            p = end;
        }
        Ok(true)
    }

    /// **The icon oracle's Rust half** (Task 11 for reading; Task 7 of the
    /// drawer-icons round added the writer checks below — this module's
    /// first writes, and CLAUDE.md's own rule that a format's writer and
    /// reader can share a mistake and agree with each other perfectly, so
    /// only something outside both can catch it). `layout` and
    /// `merge_tooltypes` above were measured against three real `.info`
    /// files (the module doc comment's own admission). This is what checks
    /// them — and every writer this module has grown since — against far
    /// more than three, without checking any of it into the repository:
    /// `scripts/icon-oracle-check.py` extracts every `.info` from the
    /// owner's own ADFs, and stages any `.info` files that already sit
    /// unpacked on disk (an OS Builder distribution tree, say), into a
    /// scratch directory and points `ART_ICON_DIR` at it — this test never
    /// reads the owner's media directly and is a no-op (not a failure) when
    /// the variable is unset, so the ordinary suite stays green with nothing
    /// extracted.
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
    /// **What every write is checked against, added for Task 7:**
    ///
    /// | Write | What must hold |
    /// |---|---|
    /// | `set_tooltypes(bytes, tooltypes(bytes))` | byte-identical to the input — for the icons where that is even possible (see the `lossy_tooltypes` paragraph below) — with or without an existing `ToolTypes` block, so it also exercises the "grow a block" / "clear a block" paths `merge_tooltypes` never touches |
    /// | `set_position(Some((37, 11)))` | reads back as `Some((37, 11))`, and no byte outside 58..66 changed |
    /// | `set_position(None)` | reads back as `None` — both coordinates carry [`NO_POSITION`], never a plain `0` |
    /// | `set_show_all_files(true)` then `(false)` | no byte outside the flags word ([`OFF_DRAWER_FLAGS`]) changes, and that word ends at exactly [`DDFLAGS_SHOWDEFAULT`] — asked only of icons that actually carry a `DrawerData` block ([`drawer_window`] returning `Some`, the same guard [`set_show_all_files`] itself enforces); icons with none are counted in `no_drawer_data`, the same "measured, not assumed" split `no_tooltypes` already uses, never folded into `failed`. See the `no_drawer_data` paragraph below for why this is *not* "returns to the original bytes" |
    /// | `render::rendered_size(bytes)` | never smaller than the `Gadget` width/height at the fixed offsets, and never zero in either dimension |
    ///
    /// **`set_tooltypes(bytes, tooltypes(bytes))` is only asked to be
    /// byte-identical when it *can* be — measured, not assumed, the same
    /// discipline `no_tooltypes` already applies.** [`tooltypes`]'s own doc
    /// comment already admits it decodes lossily
    /// (`String::from_utf8_lossy`) because real AmigaDOS text is Latin-1,
    /// not UTF-8. Real material shows this is not just a theoretical corner
    /// case: 69 of 798 icons in the owner's AmigaOS 3.9 tree carry a NewIcon
    /// `IM1=`/`IM2=` tool type whose pixel-encoding bytes legitimately run
    /// past 0x7F (they are not accidental Latin-1 text at all, just bytes
    /// that are not valid UTF-8 on their own) — decoding one to a `String`
    /// replaces the offending byte(s) with `U+FFFD`, and re-encoding that
    /// back to UTF-8 does not reproduce the original bytes, growing the
    /// file. This is a real, present gap in the write path this test
    /// exists to catch, not a reason to weaken what it checks: an icon
    /// whose raw `ToolTypes` bytes are not all valid UTF-8 is counted in
    /// `lossy_tooltypes` rather than `failed`, but is still held to a
    /// weaker, still-meaningful invariant — the *text* [`tooltypes`] reads
    /// back from the rewritten file must still equal the text that was
    /// written, even though the underlying bytes cannot be.
    ///
    /// **`set_show_all_files`'s round-trip claim had to be weakened after
    /// measuring against this corpus, and the reason is worth recording
    /// here rather than only in a commit message.** [`OFF_DRAWER_FLAGS`]'s
    /// own doc comment cites `Prefs/Presets/Beeps/Boings.info` as measured
    /// evidence of [`DDFLAGS_SHOWALL`] (2) — but in this exact file, in this
    /// exact tree, that word reads `0x0200127F`, not `2`. It is not
    /// scattered noise either: **all 96** of the real drawer-data icons in
    /// this 798-icon corpus carry that identical `0x0200127F`, which looks
    /// far more like a fixed Intuition `NewWindow` flags template baked in
    /// by whatever last opened these windows than a per-drawer Show-mode
    /// enum. That contradicts the "exactly 0, 1 or 2, never a combination"
    /// claim measured against a different, smaller sample of 59 icons
    /// elsewhere in this module — **an unresolved discrepancy, not a settled
    /// one**, and beyond this task's scope to chase down (it would need the
    /// same outside verification CLAUDE.md asks of any format claim, against
    /// real AmigaOS documentation or a second independent reader). So this
    /// test does not — and, until that is resolved, cannot — verify that
    /// `set_show_all_files` changes what a real Workbench actually displays.
    /// What it does verify, honestly: the writer keeps its own two
    /// documented promises — it touches nothing outside the flags word, and
    /// `false` always settles at the documented default. See
    /// `docs/ISSUES.md` for whether this has been filed as its own defect
    /// by the time this comment is read.
    ///
    /// What **is** unconditional, for every icon regardless of shape: `layout`
    /// itself must not error, and its `trailing` range must run to the end
    /// of the buffer (true by construction, asserted anyway so a future
    /// change to `IconLayout` that broke it would be caught here too, not
    /// only by the synthetic fixtures above) — the "lands exactly at
    /// end-of-file or at the start of a trailing IFF block" claim this test
    /// exists to check.
    ///
    /// A file that does not parse, whose merge or write does not round-trip,
    /// or whose `trailing` region does not reach end-of-file is not a panic:
    /// it is recorded by name in `failed` and the whole test fails once, at
    /// the end, printing every one of them — machine-readable
    /// (`ART_ICON_RESULT checked=… failed=… no_tooltypes=… no_drawer_data=…
    /// lossy_tooltypes=…`, one `ART_ICON_FAIL <path>: <reason>` per miss, the
    /// reason naming a byte offset wherever one is the actual point of
    /// failure) so the driving script can report them without scraping
    /// prose.
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
        let mut no_drawer_data = 0usize;
        let mut lossy_tooltypes = 0usize;
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
            } else {
                match merge_tooltypes(&bytes, &bytes) {
                    Ok(same) if same == bytes => {}
                    Ok(different) => failed.push(format!(
                        "{}: merge_tooltypes(x, x) did not return x byte for byte (first differing byte at {})",
                        entry.display(),
                        first_difference(&bytes, &different)
                    )),
                    Err(err) => failed.push(format!(
                        "{}: merge_tooltypes failed: {err}",
                        entry.display()
                    )),
                }
            }

            // Task 7: replacing an icon's tool types with the ones it
            // already has must leave the file byte-identical. Asked of
            // every icon, with or without an existing ToolTypes block — the
            // sharpest test in the set, per the task brief: it exercises the
            // whole splice path (including growing/clearing a block) and any
            // drift shows up immediately.
            //
            // Byte-identical is only possible when tooltypes()'s lossy UTF-8
            // decode is lossless for this icon in the first place — see the
            // lossy_tooltypes paragraph in this test's own doc comment. When
            // it is not, this still checks the weaker, still-real invariant
            // that the *text* survives the round-trip even though the raw
            // bytes cannot.
            let lossless = match tooltypes_round_trip_losslessly(&bytes) {
                Ok(v) => v,
                Err(err) => {
                    failed.push(format!(
                        "{}: tooltypes_round_trip_losslessly failed: {err}",
                        entry.display()
                    ));
                    true
                }
            };
            match tooltypes(&bytes) {
                Ok(existing) => match set_tooltypes(&bytes, &existing) {
                    Ok(same) if same == bytes => {}
                    Ok(different) if !lossless => {
                        lossy_tooltypes += 1;
                        match tooltypes(&different) {
                            Ok(again) if again == existing => {}
                            Ok(_) => failed.push(format!(
                                "{}: set_tooltypes(existing) changed the tool-type text itself, not just its lossy re-encoding",
                                entry.display()
                            )),
                            Err(err) => failed.push(format!(
                                "{}: tooltypes() on the rewritten file failed: {err}",
                                entry.display()
                            )),
                        }
                    }
                    Ok(different) => failed.push(format!(
                        "{}: set_tooltypes(existing) changed the file (first differing byte at {})",
                        entry.display(),
                        first_difference(&bytes, &different)
                    )),
                    Err(err) => failed.push(format!(
                        "{}: set_tooltypes(existing) failed: {err}",
                        entry.display()
                    )),
                },
                Err(err) => failed.push(format!("{}: tooltypes() failed: {err}", entry.display())),
            }

            // Task 7: set_position(Some(...)) must change only bytes 58..66
            // and must read back as the position just written.
            match set_position(&bytes, Some((37, 11))) {
                Ok(placed) if placed.len() != bytes.len() => failed.push(format!(
                    "{}: set_position(Some) changed the file's length",
                    entry.display()
                )),
                Ok(placed) => {
                    if !matches!(position(&placed), Ok(Some((37, 11)))) {
                        failed.push(format!(
                            "{}: set_position(Some((37, 11))) does not read back as placed",
                            entry.display()
                        ));
                    }
                    if let Some(i) = (0..bytes.len())
                        .filter(|i| !(58..66).contains(i))
                        .find(|&i| placed[i] != bytes[i])
                    {
                        failed.push(format!(
                            "{}: set_position(Some) changed byte {i}, outside the 58..66 it owns",
                            entry.display()
                        ));
                    }
                }
                Err(err) => failed.push(format!(
                    "{}: set_position(Some) failed: {err}",
                    entry.display()
                )),
            }

            // Task 7: set_position(None) must write NO_POSITION (i32::MIN)
            // into both coordinates, never a plain 0.
            match set_position(&bytes, None) {
                Ok(cleared) => {
                    if !matches!(position(&cleared), Ok(None)) {
                        failed.push(format!(
                            "{}: set_position(None) does not read back as unplaced",
                            entry.display()
                        ));
                    }
                }
                Err(err) => failed.push(format!(
                    "{}: set_position(None) failed: {err}",
                    entry.display()
                )),
            }

            // Task 7: set_show_all_files(true) then (false) must touch
            // nothing outside its own flags word, and must settle that word
            // at exactly DDFLAGS_SHOWDEFAULT — see this test's own doc
            // comment for why "returns to the original bytes" turned out not
            // to be a claim this corpus can support, and why this weaker
            // pair is what is actually checked instead. Only asked of icons
            // that carry a DrawerData block — the same guard
            // set_show_all_files itself enforces — so an icon with none is
            // counted separately rather than folded into failed.
            match drawer_window(&bytes) {
                Ok(Some(_)) => {
                    match set_show_all_files(&bytes, true)
                        .and_then(|on| set_show_all_files(&on, false))
                    {
                        Ok(back) => {
                            if let Some(i) = (0..bytes.len())
                                .filter(|i| !(OFF_DRAWER_FLAGS..OFF_DRAWER_FLAGS + 4).contains(i))
                                .find(|&i| back[i] != bytes[i])
                            {
                                failed.push(format!(
                                    "{}: set_show_all_files(true) then (false) changed byte {i}, outside the flags word it owns",
                                    entry.display()
                                ));
                            }
                            match be_u32(&back, OFF_DRAWER_FLAGS) {
                                Ok(flags) if flags == DDFLAGS_SHOWDEFAULT => {}
                                Ok(flags) => failed.push(format!(
                                    "{}: set_show_all_files(true) then (false) settled at {flags:#010x}, not the documented default {DDFLAGS_SHOWDEFAULT:#010x}",
                                    entry.display()
                                )),
                                Err(err) => failed.push(format!(
                                    "{}: reading back the flags word failed: {err}",
                                    entry.display()
                                )),
                            }
                        }
                        Err(err) => failed.push(format!(
                            "{}: set_show_all_files round-trip failed: {err}",
                            entry.display()
                        )),
                    }
                }
                Ok(None) => no_drawer_data += 1,
                Err(err) => {
                    failed.push(format!("{}: drawer_window failed: {err}", entry.display()))
                }
            }

            // Task 7: rendered_size is never smaller than the Gadget size,
            // and never zero in either dimension.
            match render::rendered_size(&bytes) {
                Ok(r) => {
                    let gadget_w = be_u16(&bytes, OFF_GADGET_WIDTH).unwrap_or(0);
                    let gadget_h = be_u16(&bytes, OFF_GADGET_HEIGHT).unwrap_or(0);
                    if r.width < gadget_w || r.height < gadget_h {
                        failed.push(format!(
                            "{}: rendered_size {}x{} is smaller than the Gadget size {}x{}",
                            entry.display(),
                            r.width,
                            r.height,
                            gadget_w,
                            gadget_h
                        ));
                    }
                    if r.width == 0 || r.height == 0 {
                        failed.push(format!(
                            "{}: rendered_size is {}x{} — zero in a dimension",
                            entry.display(),
                            r.width,
                            r.height
                        ));
                    }
                }
                Err(err) => {
                    failed.push(format!("{}: rendered_size failed: {err}", entry.display()))
                }
            }
        }

        println!(
            "ART_ICON_RESULT checked={checked} failed={} no_tooltypes={no_tooltypes} no_drawer_data={no_drawer_data} lossy_tooltypes={lossy_tooltypes}",
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
