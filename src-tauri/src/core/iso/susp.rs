//! The System Use Sharing Protocol, Rock Ridge, and the Amiga `AS` entry.
//!
//! An ISO9660 directory record ends with a **System Use Area**: whatever is
//! left over after the identifier. SUSP (IEEE P1281) fills it with tagged
//! entries, and two families of them matter to ART:
//!
//! - **Rock Ridge** (`NM`) carries the file's real, mixed-case name. A disc
//!   mastered on a Unix host and given no Joliet descriptor has its only real
//!   names here; without this, `MyGame.info` is read as `MYGAME.INF` and the
//!   icon stops matching the drawer.
//! - **The Amiga `AS` entry** carries the AmigaDOS **protection bits** and the
//!   **file comment** — the `S` and `P` bits `core::volume::write::uaem`'s own
//!   module doc calls load-bearing, because WHDLoad slaves and
//!   `Resident C:Assign PURE` stop working without them.
//!
//! Both are what ART-078 was filed about.
//!
//! # Where the layout came from
//!
//! Not from prose. Three sources, in the order they were consulted:
//!
//! 1. **The specification itself**, `Rock_Ridge_Amiga_Specific` v2.4
//!    (1996-12-05, Angela Schmidt with Andrew Young — the primary author of
//!    RRIP and SUSP), read off the owner's own *Amiga Developer CD v2.1* at
//!    `/Contributions/Angela_Schmidt/Reference/`. The disc carries the
//!    document that describes the entries the disc itself uses.
//! 2. **A working implementation**: `ODFileSystem`'s
//!    `backends/rock_ridge/rock_ridge.c` (`rr_parse_as`) and its unit vectors
//!    in `tests/unit/test_rock_ridge.c`, both BSD-2-Clause,
//!    <https://github.com/reinauer/ODFileSystem>. Its vectors are reproduced
//!    byte for byte in this module's tests, so ART and an independent
//!    implementation agree on the same input.
//! 3. **44 796 real `AS` entries**, decoded by `scripts/iso-susp-census.py`
//!    across the owner's four discs. Every one of them accounts for its
//!    payload exactly — no entry with bytes left over, none that the layout
//!    fails to fit. That is the check that the reading above is right rather
//!    than merely plausible.
//!
//! # The `AS` entry
//!
//! ```text
//! 'A' 'S'  LEN  VER=1  FLAGS  [protection: 4 bytes]  [len][comment bytes]
//! ```
//!
//! `FLAGS` bit 0 = protection present, bit 1 = comment present, bit 2 = the
//! comment continues in the **next** `AS` entry of this same System Use Area.
//! The comment's own length byte counts itself, so it is followed by
//! `len - 1` characters.
//!
//! The four protection bytes are, in the specification's own words,
//! `User | 0 | Multiuser Flags | Protection Bits` — which read big-endian is
//! exactly the 32-bit protection long AmigaDOS stores in a header block, and
//! exactly what [`crate::core::volume::write::uaem::Sidecar::protection`]
//! holds. So they are kept as one `u32` and never narrowed: the specification
//! says an application reading them "must preserve all four bytes. No bit
//! shall be cleared or set." The low byte's `RWED` half is inverted, which is
//! the inversion `uaem::format_bits` already applies — this module does not
//! apply it a second time.
//!
//! # Every length here came from a file ART did not write
//!
//! A System Use Area is a length-prefixed chain inside a length-prefixed
//! record, and a `CE` entry points at an arbitrary block. So: an entry
//! shorter than its own header stops the walk instead of stepping backwards,
//! an entry claiming to run past the area stops it too, the number of entries
//! is capped, a name and a comment are capped at the lengths AmigaDOS itself
//! allows, and the `CE` chain is resolved by the caller — [`IsoImage::list`]
//! — under a fixed depth limit, because only the caller can read a block.
//!
//! [`IsoImage::list`]: super::IsoImage::list

use crate::core::error::{CoreError, CoreResult};

use super::descriptor::decode_iso646;

/// Bytes of header every SUSP entry begins with: signature, length, version.
const ENTRY_HEADER_LEN: usize = 4;

/// Most SUSP entries ART will read from one System Use Area.
///
/// A real area holds a handful — the measured discs top out at five. This
/// only ever bites an area that has been crafted to be walked forever.
pub const MAX_SUSP_ENTRIES: usize = 256;

/// Longest Rock Ridge name ART will assemble from `NM` fragments.
///
/// Rock Ridge names are unbounded in principle and `NM` fragments chain, so a
/// cap is the only thing between ART and a name built out of a crafted disc's
/// whole free space. 1024 is far past anything AmigaDOS or NTFS will hold and
/// far short of a memory problem.
pub const MAX_ROCK_NAME_LEN: usize = 1024;

/// `AS` flag bit 0 — the four protection bytes follow.
const AS_PROTECTION: u8 = 0x01;
/// `AS` flag bit 1 — a comment fragment follows.
const AS_COMMENT: u8 = 0x02;
/// `AS` flag bit 2 — the comment continues in the next `AS` entry.
const AS_COMMENT_CONTINUE: u8 = 0x04;

/// `NM` flag bit 0 — the name continues in the next `NM` entry.
const NM_CONTINUE: u8 = 0x01;
/// `NM` flag bit 1 — this entry names `.`; bit 2 names `..`. Either way the
/// entry is structure, not a filename, so it is not collected.
const NM_CURRENT_OR_PARENT: u8 = 0x06;

/// A `CE` entry: the System Use Area continues somewhere else on the disc.
///
/// The three fields are stored both-endian (little then big, 8 bytes each);
/// only the little-endian half is read, the same choice the rest of
/// `core::iso` makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Continuation {
    /// Logical block where the continuation area starts.
    pub block: u32,
    /// Byte offset of the area within that block.
    pub offset: u32,
    /// Length of the area in bytes.
    pub length: u32,
}

/// What ART takes out of a System Use Area.
///
/// Everything is `Option` because everything is optional: a disc may carry
/// Rock Ridge without `AS`, `AS` without a comment, or neither.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemUse {
    /// The Rock Ridge `NM` name, fragments already joined. `None` when the
    /// record carries no `NM`, in which case the ISO9660 identifier stands.
    pub name: Option<String>,
    /// The AmigaDOS 32-bit protection long from `AS`, `RWED` still inverted.
    pub protection: Option<u32>,
    /// The AmigaDOS file comment from `AS`, fragments already joined.
    pub comment: Option<String>,
    /// Where the area continues, if it does. Resolved by the caller.
    pub continuation: Option<Continuation>,
    /// True once an `AS` entry has contributed protection bits. The
    /// specification says only the *first* `AS` entry of a record may carry
    /// them; a later one claiming to is ignored rather than allowed to
    /// overwrite what the first said.
    protection_seen: bool,
    /// True while the last `NM`/`AS` fragment asked for a continuation, so a
    /// fragment arriving from a `CE` area appends instead of replacing.
    name_open: bool,
    comment_open: bool,
}

impl SystemUse {
    /// True when nothing at all was found — the state of every record on a
    /// disc that carries no System Use data.
    pub fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.protection.is_none()
            && self.comment.is_none()
            && self.continuation.is_none()
    }
}

/// Read the `SP` entry's skip count out of a root directory's `.` record.
///
/// `SP` is how a disc says "System Use Areas are here at all". It sits in the
/// first record of the root directory, carries the check bytes `BE EF`, and
/// states how many bytes every *later* System Use Area begins with that are
/// not SUSP entries. Returns `None` when the disc carries no `SP`, which is
/// how ART decides not to look at System Use Areas at all.
///
/// The check bytes are not decoration: without them any two bytes that happen
/// to read as `SP` would turn a plain ISO9660 disc into one ART tried to
/// parse Rock Ridge out of.
pub fn sp_skip(area: &[u8]) -> Option<usize> {
    let mut pos = 0usize;
    let mut seen = 0usize;
    while pos + ENTRY_HEADER_LEN <= area.len() && seen < MAX_SUSP_ENTRIES {
        seen += 1;
        let length = area[pos + 2] as usize;
        if length < ENTRY_HEADER_LEN || pos + length > area.len() {
            return None;
        }
        if &area[pos..pos + 2] == b"SP" {
            let payload = &area[pos + ENTRY_HEADER_LEN..pos + length];
            if payload.len() >= 3 && payload[0] == 0xBE && payload[1] == 0xEF {
                return Some(payload[2] as usize);
            }
            return None;
        }
        if &area[pos..pos + 2] == b"ST" {
            return None;
        }
        pos += length;
    }
    None
}

/// Parse one System Use Area into `out`, appending to whatever is there.
///
/// Called once for the area inside the directory record, then again for each
/// `CE` continuation the caller resolves — which is why it accumulates rather
/// than returning a fresh value: an `NM` or `AS` comment split across a `CE`
/// boundary has to join up.
///
/// `skip` is the `SP` entry's count, already validated by [`sp_skip`]. An
/// area shorter than the skip is empty, not an error: a record with no room
/// for System Use data is ordinary.
pub fn parse_into(area: &[u8], skip: usize, out: &mut SystemUse) -> CoreResult<()> {
    let Some(area) = area.get(skip..) else {
        return Ok(());
    };

    let mut pos = 0usize;
    let mut seen = 0usize;
    // Cleared by every entry so a `CE` that is not the last thing in the area
    // still means "continue here", but a stale one from the previous area
    // never survives into the next round.
    out.continuation = None;

    while pos + ENTRY_HEADER_LEN <= area.len() {
        seen += 1;
        if seen > MAX_SUSP_ENTRIES {
            return Err(malformed(format!(
                "a System Use Area on this disc holds more than {MAX_SUSP_ENTRIES} entries"
            )));
        }

        let signature = &area[pos..pos + 2];
        let length = area[pos + 2] as usize;

        // A zero or short length would step backwards or in place. A run of
        // NULs is how an area is padded, and reads as exactly that.
        if length < ENTRY_HEADER_LEN {
            break;
        }
        // `checked_add` because both halves are file data: `pos` grows by
        // attacker-chosen lengths and `length` is one of them.
        let end = pos
            .checked_add(length)
            .ok_or_else(|| malformed("a System Use entry's length overflows".to_string()))?;
        if end > area.len() {
            break;
        }

        let payload = &area[pos + ENTRY_HEADER_LEN..end];
        match signature {
            b"NM" => read_nm(payload, out),
            b"AS" => read_as(payload, out),
            b"CE" => read_ce(payload, out),
            // `ST` ends the area explicitly. Everything else — `PX`, `TF`,
            // `RR`, `SL`, `PN`, `ER` — is a signature ART has no use for and
            // must step over rather than stumble on.
            b"ST" => break,
            _ => {}
        }

        pos = end;
    }

    Ok(())
}

/// `NM`: `[flags][name bytes]`, fragments joined in the order recorded.
fn read_nm(payload: &[u8], out: &mut SystemUse) {
    let Some((&flags, bytes)) = payload.split_first() else {
        return;
    };
    // `.` and `..` name themselves through `NM` on some discs. They are
    // structure and the record walk drops them anyway; collecting one here
    // would give the parent directory a filename.
    if flags & NM_CURRENT_OR_PARENT != 0 {
        return;
    }

    let text = decode_iso646(bytes);
    let target = if out.name_open {
        out.name.get_or_insert_with(String::new)
    } else {
        out.name = Some(String::new());
        out.name.as_mut().expect("just set")
    };
    // Truncated at a character boundary, not a byte one: `decode_iso646` maps
    // Latin-1 bytes to `char`, so pushing char by char is what keeps the cap
    // from splitting a multi-byte UTF-8 sequence in the `String`.
    for ch in text.chars() {
        if target.len() + ch.len_utf8() > MAX_ROCK_NAME_LEN {
            break;
        }
        target.push(ch);
    }
    out.name_open = flags & NM_CONTINUE != 0;
}

/// `AS`: `[flags][protection: 4][comment len][comment]`, each part present
/// only when its flag says so.
fn read_as(payload: &[u8], out: &mut SystemUse) {
    let Some((&flags, rest)) = payload.split_first() else {
        return;
    };
    let mut pos = 0usize;

    if flags & AS_PROTECTION != 0 {
        // A truncated protection field ends the entry: what follows it can
        // no longer be located, so guessing where the comment starts would
        // be inventing data.
        let Some(bytes) = rest.get(pos..pos + 4) else {
            return;
        };
        if !out.protection_seen {
            out.protection = Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
            out.protection_seen = true;
        }
        pos += 4;
    }

    if flags & AS_COMMENT != 0 {
        let Some(&declared) = rest.get(pos) else {
            return;
        };
        pos += 1;
        // The length byte counts itself, so zero is not "empty", it is
        // malformed — and `declared - 1` on it would wrap.
        if declared == 0 {
            return;
        }
        let take = declared as usize - 1;
        let Some(bytes) = rest.get(pos..pos + take) else {
            return;
        };

        let text = decode_iso646(bytes);
        let target = if out.comment_open {
            out.comment.get_or_insert_with(String::new)
        } else {
            out.comment = Some(String::new());
            out.comment.as_mut().expect("just set")
        };
        // An AmigaDOS comment is a 80-byte BSTR, so 79 characters is all any
        // destination can hold. Capping here rather than at the point of use
        // means the cap is applied once, where the bytes are untrusted.
        for ch in text.chars() {
            if target.chars().count() >= crate::core::volume::write::uaem::MAX_COMMENT_LEN {
                break;
            }
            target.push(ch);
        }
    }

    out.comment_open = flags & AS_COMMENT_CONTINUE != 0;
}

/// `CE`: block, offset and length, each recorded both-endian.
fn read_ce(payload: &[u8], out: &mut SystemUse) {
    if payload.len() < 24 {
        return;
    }
    let le = |at: usize| {
        u32::from_le_bytes([
            payload[at],
            payload[at + 1],
            payload[at + 2],
            payload[at + 3],
        ])
    };
    out.continuation = Some(Continuation {
        block: le(0),
        offset: le(8),
        length: le(16),
    });
}

fn malformed(detail: String) -> CoreError {
    CoreError::Malformed {
        format: "iso9660-susp".to_string(),
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(area: &[u8]) -> SystemUse {
        let mut out = SystemUse::default();
        parse_into(area, 0, &mut out).unwrap();
        out
    }

    /// ODFileSystem's own `rr_parse_as_basic` vector, byte for byte
    /// (`tests/unit/test_rock_ridge.c`). Reproduced rather than paraphrased:
    /// an independent implementation and ART must read the same bytes the
    /// same way, or one of them is wrong.
    #[test]
    fn the_odfilesystem_as_vector_reads_the_same_here() {
        let area = [
            b'A', b'S', 21, 1, 0x03, //
            0x00, 0x00, 0x00, 0x40, //
            12, b'B', b'o', b'o', b't', b' ', b's', b'c', b'r', b'i', b'p', b't', //
            b'S', b'T', 4, 1,
        ];
        let got = parse(&area);
        assert_eq!(got.protection, Some(0x0000_0040));
        assert_eq!(got.comment.as_deref(), Some("Boot script"));
    }

    /// ODFileSystem's `rr_parse_as_comment_continue` vector, likewise.
    #[test]
    fn a_comment_split_across_two_as_entries_is_joined() {
        let area = [
            b'A', b'S', 13, 1, 0x07, //
            0x00, 0x00, 0x00, 0x10, //
            4, b'A', b'm', b'i', //
            b'A', b'S', 13, 1, 0x02, //
            8, b'g', b'a', b' ', b'R', b'R', b'I', b'P', //
            b'S', b'T', 4, 1,
        ];
        let got = parse(&area);
        assert_eq!(got.protection, Some(0x0000_0010));
        assert_eq!(got.comment.as_deref(), Some("Amiga RRIP"));
    }

    /// The shape 44 796 real `AS` entries on the owner's AmigaOS 3.9 and
    /// Amiga Developer CD v2.1 discs actually have: flags `0x01`, four
    /// protection bytes, no comment. Counted by
    /// `scripts/iso-susp-census.py`; `0x02` is the commonest value there,
    /// and it means `e` set — not executable.
    #[test]
    fn the_shape_every_real_as_entry_has() {
        let area = [b'A', b'S', 9, 1, 0x01, 0x00, 0x00, 0x00, 0x02];
        let got = parse(&area);
        assert_eq!(got.protection, Some(0x0000_0002));
        assert_eq!(got.comment, None);
        // `hsparwed` with the RWED half inverted: `e` set means no execute.
        assert_eq!(
            crate::core::volume::write::uaem::format_bits(got.protection.unwrap()),
            "----rw-d"
        );
    }

    /// The `p` and `s` bits, which are the reason ART-078 mattered. Both
    /// values were measured on the owner's own discs — `0x20` 145 times,
    /// `0x40` 6 times — so this is a real shape, not an invented one.
    #[test]
    fn the_pure_and_script_bits_survive_into_uaem_spelling() {
        let pure = parse(&[b'A', b'S', 9, 1, 0x01, 0x00, 0x00, 0x00, 0x20]);
        assert_eq!(
            crate::core::volume::write::uaem::format_bits(pure.protection.unwrap()),
            "--p-rwed"
        );
        let script = parse(&[b'A', b'S', 9, 1, 0x01, 0x00, 0x00, 0x00, 0x40]);
        assert_eq!(
            crate::core::volume::write::uaem::format_bits(script.protection.unwrap()),
            "-s--rwed"
        );
    }

    /// The specification's FIGURE 1: byte 3 is the multiuser half and must
    /// survive, because "no bit shall be cleared or set".
    #[test]
    fn the_multiuser_byte_is_preserved_not_discarded() {
        let got = parse(&[b'A', b'S', 9, 1, 0x01, 0x00, 0x00, 0xAA, 0x20]);
        assert_eq!(got.protection, Some(0x0000_AA20));
    }

    /// Only the first `AS` entry of a record may carry protection bits
    /// (specification, paragraph 3). A second one claiming to does not get
    /// to overwrite the first.
    #[test]
    fn a_second_as_entry_cannot_replace_the_first_protection() {
        let area = [
            b'A', b'S', 9, 1, 0x01, 0x00, 0x00, 0x00, 0x20, //
            b'A', b'S', 9, 1, 0x01, 0xFF, 0xFF, 0xFF, 0xFF,
        ];
        assert_eq!(parse(&area).protection, Some(0x0000_0020));
    }

    #[test]
    fn a_rock_ridge_name_is_read_and_its_fragments_joined() {
        let area = [
            b'N', b'M', 9, 1, 0x01, b'M', b'y', b'G', b'a', //
            b'N', b'M', 9, 1, 0x00, b'm', b'e', b'.', b'i',
        ];
        assert_eq!(parse(&area).name.as_deref(), Some("MyGame.i"));
    }

    #[test]
    fn an_nm_naming_dot_or_dotdot_is_not_a_filename() {
        let area = [b'N', b'M', 5, 1, 0x02, b'.'];
        assert_eq!(parse(&area).name, None);
    }

    #[test]
    fn sp_is_only_believed_with_its_check_bytes() {
        assert_eq!(sp_skip(&[b'S', b'P', 7, 1, 0xBE, 0xEF, 0]), Some(0));
        assert_eq!(sp_skip(&[b'S', b'P', 7, 1, 0xBE, 0xEF, 4]), Some(4));
        // The same entry with the check bytes wrong is not an SP entry, and
        // must not turn a plain ISO9660 disc into one ART parses SUSP from.
        assert_eq!(sp_skip(&[b'S', b'P', 7, 1, 0x00, 0x00, 0]), None);
        assert_eq!(sp_skip(&[]), None);
    }

    #[test]
    fn the_sp_skip_is_applied_before_the_first_entry() {
        let mut area = vec![0xAA, 0xBB, 0xCC, 0xDD];
        area.extend_from_slice(&[b'A', b'S', 9, 1, 0x01, 0x00, 0x00, 0x00, 0x20]);
        let mut got = SystemUse::default();
        parse_into(&area, 4, &mut got).unwrap();
        assert_eq!(got.protection, Some(0x20));
        // Without the skip the four junk bytes are read as an entry header
        // whose length (0xCC) runs past the area, so the walk stops and
        // finds nothing. That is the failure the skip exists to prevent.
        let mut wrong = SystemUse::default();
        parse_into(&area, 0, &mut wrong).unwrap();
        assert_eq!(wrong.protection, None);
    }

    #[test]
    fn a_ce_entry_is_reported_for_the_caller_to_resolve() {
        let mut area = vec![b'C', b'E', 28, 1];
        area.extend_from_slice(&40u32.to_le_bytes());
        area.extend_from_slice(&40u32.to_be_bytes());
        area.extend_from_slice(&8u32.to_le_bytes());
        area.extend_from_slice(&8u32.to_be_bytes());
        area.extend_from_slice(&64u32.to_le_bytes());
        area.extend_from_slice(&64u32.to_be_bytes());
        assert_eq!(
            parse(&area).continuation,
            Some(Continuation {
                block: 40,
                offset: 8,
                length: 64
            })
        );
    }

    // --- hostile input -------------------------------------------------

    #[test]
    fn a_zero_length_entry_ends_the_area_and_does_not_loop() {
        // Length 0 read as a step is an infinite loop, and `panic = "abort"`
        // in the release profile makes a spin the *best* case.
        let area = [b'A', b'S', 0, 1, 0x01, 0x00, 0x00, 0x00, 0x20];
        assert_eq!(parse(&area).protection, None);
    }

    #[test]
    fn an_entry_running_past_the_area_is_ignored_not_read() {
        let area = [b'A', b'S', 200, 1, 0x01, 0x00, 0x00, 0x00, 0x20];
        assert_eq!(parse(&area).protection, None);
    }

    #[test]
    fn an_as_entry_with_a_truncated_protection_field_yields_nothing() {
        let area = [b'A', b'S', 7, 1, 0x01, 0x00, 0x00];
        assert_eq!(parse(&area).protection, None);
    }

    #[test]
    fn a_comment_length_byte_of_zero_is_refused_rather_than_wrapping() {
        // `declared - 1` on a zero would wrap to `usize::MAX` and index with
        // it. The measured discs never write one; a crafted disc would.
        let area = [b'A', b'S', 6, 1, 0x02, 0x00];
        assert_eq!(parse(&area).comment, None);
    }

    #[test]
    fn a_comment_longer_than_amigados_allows_is_capped() {
        let text = vec![b'x'; 200];
        let mut area = vec![b'A', b'S', 0, 1, 0x02, (text.len() + 1) as u8];
        area.extend_from_slice(&text);
        area[2] = area.len() as u8;
        let got = parse(&area);
        assert_eq!(
            got.comment.unwrap().chars().count(),
            crate::core::volume::write::uaem::MAX_COMMENT_LEN
        );
    }

    #[test]
    fn a_name_longer_than_the_cap_is_truncated_not_grown_without_bound() {
        // 40 fragments of 250 characters would build a 10 000-character
        // name; the cap stops it at 1024.
        let mut area = Vec::new();
        for _ in 0..40 {
            area.push(b'N');
            area.push(b'M');
            area.push(255);
            area.push(1);
            area.push(NM_CONTINUE);
            area.extend(std::iter::repeat_n(b'x', 250));
        }
        assert_eq!(parse(&area).name.unwrap().len(), MAX_ROCK_NAME_LEN);
    }

    #[test]
    fn an_area_of_more_entries_than_the_cap_is_an_error() {
        let mut area = Vec::new();
        for _ in 0..(MAX_SUSP_ENTRIES + 2) {
            area.extend_from_slice(&[b'R', b'R', 5, 1, 0x00]);
        }
        let mut out = SystemUse::default();
        let err = parse_into(&area, 0, &mut out).unwrap_err();
        assert!(err.to_string().contains("more than"), "{err}");
    }

    #[test]
    fn an_area_of_pure_padding_finds_nothing_and_is_not_an_error() {
        assert!(parse(&[0u8; 64]).is_empty());
    }

    #[test]
    fn an_unknown_signature_is_stepped_over_not_stumbled_on() {
        // `PX` and `TF` are on every Rock Ridge disc ART measured, and ART
        // reads neither — but a parser that stopped at the first signature it
        // did not know would never reach the `AS` behind them.
        let mut area = vec![b'P', b'X', 36, 1];
        area.extend(std::iter::repeat_n(0u8, 32));
        area.extend_from_slice(&[b'A', b'S', 9, 1, 0x01, 0x00, 0x00, 0x00, 0x20]);
        assert_eq!(parse(&area).protection, Some(0x20));
    }
}
