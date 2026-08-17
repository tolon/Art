//! A WHDLoad Slave's own header — the widest source of stated metadata here.
//!
//! Format: whdload.de's autodoc, section `WHDLoad.Slave/--Overview--`, plus
//! `WHDLoad/Include/whdload.i` v16.8 from Aminet `dev/misc/WHDLoad_dev.lha`
//! for the `ws_Flags` bit numbers, which the autodoc names but does not
//! number. Neither file is copied into ART; the format is read and implemented
//! independently, the way ART's boot code was written from the published LVO
//! table.
//!
//! ```text
//!  0  ws_Security  4 bytes   70 FF 4E 75  (moveq #-1,d0 / rts)
//!  4  ws_ID        8 bytes   "WHDLOADS"
//! 12  ws_Version   UWORD     gates everything below
//! 14  ws_Flags     UWORD     bit 4 = Req68020, bit 5 = ReqAGA
//! 16  ws_BaseMemSize · 20 ws_ExecInstall · 24 ws_GameLoader
//! 26  ws_CurrentDir · 28 ws_DontCache
//! 30  ws_keydebug · 31 ws_keyexit                     v4+
//! 32  ws_ExpMem                                       v8+
//! 36  ws_name · 38 ws_copy · 40 ws_info               v10+
//! 42  ws_kickname · 44 ws_kicksize · 48 ws_kickcrc    v16+
//! 50  ws_config                                       v17+
//! 52  ws_MemConfig                                    v20+
//! ```
//!
//! Every `RPTR` is documented as "a relative (to the start of the structure)
//! 16-bit pointer".
//!
//! Two things this module refuses to do, both of which look like shortcuts:
//!
//! - **It does not find the structure by searching for `WHDLOADS`.** A slave
//!   "may contain debug/symbol hunks which are ignored", so the magic can
//!   appear somewhere that is not the structure. The hunk header says where the
//!   code begins; `ws_Security`'s fixed `70 FF 4E 75` then confirms it.
//! - **It does not read a field the slave's version does not have.** At
//!   `ws_Version` 13 there *is* no `ws_kickname`; the game's own code is at
//!   that offset. Every field below is gated.
//!
//! `ws_config` (v17+) and `ws_MemConfig` (v20+) are deliberately not read: they
//! describe splash-window gadgets, which no exporter needs.

use crate::core::error::{CoreError, CoreResult};
use crate::core::gameindex::record::{ChipsetRequirement, KickstartNeed};

const HUNK_HEADER: u32 = 0x0000_03F3;
const HUNK_CODE: u32 = 0x0000_03E9;
const HUNK_DATA: u32 = 0x0000_03EA;

/// `moveq #-1,d0 / rts` — the `SLAVE_HEADER` macro's first four bytes.
const WS_SECURITY: [u8; 4] = [0x70, 0xFF, 0x4E, 0x75];

/// How many hunks (and library-name longwords) ART will walk before calling a
/// header a loop. A real slave declares exactly one hunk.
const MAX_HUNKS: u32 = 64;

/// The longest string ART will read out of a slave before giving up on finding
/// a terminator. `ws_info` is the long one and runs to a couple of lines.
const MAX_STRING: usize = 512;

/// `ws_Flags` bit 4 — the slave states it needs a 68020 or better.
const WHDLB_REQ68020: u16 = 1 << 4;
/// `ws_Flags` bit 5 — the slave states it needs AGA.
const WHDLB_REQAGA: u16 = 1 << 5;

/// What a slave says about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlaveFacts {
    pub version: u16,
    pub name: Option<String>,
    /// `ws_copy` verbatim. Splitting it into year and publisher is
    /// [`split_copyright`]'s job, because the split can fail and the raw string
    /// is still worth keeping.
    pub copyright: Option<String>,
    pub info: Option<String>,
    pub requires_aga: bool,
    pub requires_68020: bool,
    pub kickstart: KickstartNeed,
}

fn malformed(detail: &str) -> CoreError {
    CoreError::Malformed {
        format: "whdload-slave".into(),
        detail: detail.into(),
    }
}

/// Where the `WHDLoadSlave` structure begins.
///
/// Parses the AmigaDOS hunk header rather than searching for the magic, for the
/// reason in this module's own documentation.
pub fn structure_base(bytes: &[u8]) -> CoreResult<usize> {
    if read_u32(bytes, 0).ok_or_else(|| malformed("file is shorter than a hunk header"))?
        != HUNK_HEADER
    {
        return Err(malformed("not an AmigaDOS executable (no HUNK_HEADER)"));
    }

    // The resident-library name list: each entry is a longword length in
    // longwords followed by that many longwords, terminated by a zero.
    let mut at = 4usize;
    let mut names = 0u32;
    loop {
        let len = read_u32(bytes, at).ok_or_else(|| malformed("truncated library-name list"))?;
        at += 4;
        if len == 0 {
            break;
        }
        names += 1;
        if len > MAX_HUNKS || names > MAX_HUNKS {
            return Err(malformed("implausible library-name list"));
        }
        at = at
            .checked_add(len as usize * 4)
            .ok_or_else(|| malformed("library-name list overflows"))?;
    }

    let table_size = read_u32(bytes, at).ok_or_else(|| malformed("truncated hunk table"))?;
    let first = read_u32(bytes, at + 4).ok_or_else(|| malformed("truncated hunk table"))?;
    let last = read_u32(bytes, at + 8).ok_or_else(|| malformed("truncated hunk table"))?;
    if table_size == 0 || table_size > MAX_HUNKS || last < first || last - first + 1 > MAX_HUNKS {
        return Err(malformed("implausible hunk table"));
    }
    at += 12;
    let sizes = last - first + 1;
    at = at
        .checked_add(sizes as usize * 4)
        .ok_or_else(|| malformed("hunk size table overflows"))?;

    // The first hunk block. A slave's is CODE; DATA is accepted because the
    // structure is data and some builds emit it that way.
    let kind = read_u32(bytes, at).ok_or_else(|| malformed("truncated first hunk"))?;
    if kind != HUNK_CODE && kind != HUNK_DATA {
        return Err(malformed("first hunk is neither CODE nor DATA"));
    }
    let base = at + 8;
    if base >= bytes.len() {
        return Err(malformed("first hunk has no data"));
    }
    Ok(base)
}

/// Read a slave's stated facts.
pub fn read_slave(bytes: &[u8]) -> CoreResult<SlaveFacts> {
    let base = structure_base(bytes)?;
    let body = &bytes[base..];

    if body.len() < 14 {
        return Err(malformed("structure is shorter than its fixed head"));
    }
    if body[0..4] != WS_SECURITY {
        return Err(malformed("ws_Security is not 'moveq #-1,d0 / rts'"));
    }
    if &body[4..12] != b"WHDLOADS" {
        return Err(malformed("ws_ID is not 'WHDLOADS'"));
    }

    let version = u16::from_be_bytes([body[12], body[13]]);
    let flags = read_u16(body, 14).unwrap_or(0);

    // v10 introduced ws_name / ws_copy / ws_info. Below it those offsets hold
    // whatever the program put there.
    let (name, copyright, info) = if version >= 10 {
        (
            read_rptr_string(body, 36),
            read_rptr_string(body, 38),
            read_rptr_string(body, 40),
        )
    } else {
        (None, None, None)
    };

    // v16 introduced ws_kickname / ws_kicksize / ws_kickcrc. An offset of 0
    // inside a v16 slave means "no image wanted", which is a different answer
    // from "this slave is too old to say" — and both differ again from naming
    // one. All three cases have a test.
    let kickstart = if version >= 16 {
        KickstartNeed {
            image: read_rptr_string(body, 42),
            size: read_u32(body, 44),
            crc16: read_u16(body, 48),
            rom_version: None,
        }
    } else {
        KickstartNeed::default()
    };

    Ok(SlaveFacts {
        version,
        name,
        copyright,
        info,
        requires_aga: flags & WHDLB_REQAGA != 0,
        requires_68020: flags & WHDLB_REQ68020 != 0,
        kickstart,
    })
}

/// The chipset a slave states it needs, or `None` when it states nothing.
///
/// A clear `ReqAGA` bit is **not** a statement that the game is OCS — most OCS
/// games simply never set it, and so do plenty of AGA-era slaves that check for
/// themselves. Only the set bit is evidence.
pub fn chipset_of(facts: &SlaveFacts) -> Option<ChipsetRequirement> {
    facts.requires_aga.then_some(ChipsetRequirement::Aga)
}

/// Split `ws_copy` into its year and its copyright holder.
///
/// The documented shape is "the year followed by the companies holding the
/// copyright", with multiple entries separated by `', '` — e.g.
/// `1983 Schega, 1989 Bad Dreams`. It says *should*, so anything that does not
/// match yields no year rather than a wrong one.
pub fn split_copyright(copy: &str) -> (Option<u16>, Option<String>) {
    let first = copy.split(',').next().unwrap_or(copy).trim();
    let mut parts = first.splitn(2, char::is_whitespace);
    let head = parts.next().unwrap_or("").trim();
    let tail = parts.next().unwrap_or("").trim();

    let year = head
        .parse::<u16>()
        .ok()
        .filter(|y| (1975..=2100).contains(y));

    match year {
        Some(y) if !tail.is_empty() => (Some(y), Some(tail.to_string())),
        Some(y) => (Some(y), None),
        // No leading year: the whole first entry is the holder, if there is one.
        None => (None, (!first.is_empty()).then(|| first.to_string())),
    }
}

/// Read the 16-bit offset at `at` and the NUL-terminated string it points to.
///
/// Every step is bounded: the offset is checked against the slice, the search
/// for a terminator stops at [`MAX_STRING`], and nothing is allocated from an
/// unchecked field. An offset of 0 means **absent**, which the format uses
/// deliberately.
fn read_rptr_string(body: &[u8], at: usize) -> Option<String> {
    let offset = read_u16(body, at)? as usize;
    if offset == 0 || offset >= body.len() {
        return None;
    }
    let end = body
        .iter()
        .skip(offset)
        .take(MAX_STRING)
        .position(|&b| b == 0)
        .map(|n| offset + n)
        .unwrap_or_else(|| (offset + MAX_STRING).min(body.len()));
    if end <= offset {
        return None;
    }
    Some(decode_amiga_text(&body[offset..end]))
}

/// Turn a slave's raw string bytes into text.
///
/// `ws_info` "may also contain line feeds ($0a). The character -1 has a special
/// meaning. It results in a line feed and an additional vertical skip of the
/// half font height." So `0xFF` is a separator, not a character: left alone it
/// renders as `ÿ`. The rest is Latin-1, which is what an Amiga string is, and
/// `u8 as char` is exactly the Latin-1 mapping.
fn decode_amiga_text(raw: &[u8]) -> String {
    let mut out = String::with_capacity(raw.len());
    for &byte in raw {
        match byte {
            0xFF => out.push('\n'),
            b => out.push(b as char),
        }
    }
    out.trim_end().to_string()
}

fn read_u16(buf: &[u8], at: usize) -> Option<u16> {
    let end = at.checked_add(2)?;
    buf.get(at..end).map(|s| u16::from_be_bytes([s[0], s[1]]))
}

fn read_u32(buf: &[u8], at: usize) -> Option<u32> {
    let end = at.checked_add(4)?;
    buf.get(at..end)
        .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a slave the way a real one is built: an AmigaDOS hunk executable
    /// of one hunk, with the `WHDLoadSlave` structure at the start of the
    /// hunk's data and the strings after it.
    ///
    /// The 32-byte header this produces is exactly what both real archives
    /// carry — `Lotus3.slave` and `Moonstone.Slave` both put their structure at
    /// offset 32 — so a fixture built this way is not a convenient shape, it is
    /// the shape.
    #[derive(Default)]
    struct SlaveBuilder {
        version: u16,
        flags: u16,
        name: Option<&'static str>,
        copyright: Option<&'static str>,
        info: Option<&'static str>,
        kickname: Option<&'static str>,
        /// Written into `ws_kickname` verbatim, overriding the string.
        kickname_offset_override: Option<u16>,
    }

    impl SlaveBuilder {
        fn new(version: u16) -> Self {
            Self {
                version,
                ..Default::default()
            }
        }

        /// The structure's own length, by version. Anything the version does
        /// not reach is simply not there — which is the case the reader has to
        /// get right.
        fn struct_len(version: u16) -> usize {
            match version {
                0..=3 => 30,
                4..=7 => 32,
                8..=9 => 36,
                10..=15 => 42,
                16 => 50,
                17..=19 => 52,
                _ => 54,
            }
        }

        fn build(&self) -> Vec<u8> {
            let struct_len = Self::struct_len(self.version);
            let mut body = vec![0u8; struct_len];
            body[0..4].copy_from_slice(&WS_SECURITY);
            body[4..12].copy_from_slice(b"WHDLOADS");
            body[12..14].copy_from_slice(&self.version.to_be_bytes());
            body[14..16].copy_from_slice(&self.flags.to_be_bytes());

            // Strings live after the structure; each RPTR is the offset from
            // the structure's own start.
            for (at, text) in [
                (36, self.name),
                (38, self.copyright),
                (40, self.info),
                (42, self.kickname),
            ] {
                if let (Some(text), true) = (text, at + 2 <= body.len()) {
                    let offset = body.len() as u16;
                    body[at..at + 2].copy_from_slice(&offset.to_be_bytes());
                    body.extend_from_slice(text.as_bytes());
                    body.push(0);
                }
            }
            if let Some(raw) = self.kickname_offset_override {
                if body.len() >= 44 {
                    body[42..44].copy_from_slice(&raw.to_be_bytes());
                }
            }

            hunk_wrap(&body)
        }
    }

    /// Wrap `body` as a one-hunk AmigaDOS executable.
    ///
    /// `0x3F3` header, an empty resident-library list, table_size 1, first 0,
    /// last 0, one size longword, then `0x3E9` (HUNK_CODE) and its size — so
    /// the data begins at byte 32.
    fn hunk_wrap(body: &[u8]) -> Vec<u8> {
        let longs = body.len().div_ceil(4) as u32;
        let mut out = Vec::new();
        out.extend_from_slice(&HUNK_HEADER.to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes()); // no library names
        out.extend_from_slice(&1u32.to_be_bytes()); // table_size
        out.extend_from_slice(&0u32.to_be_bytes()); // first hunk
        out.extend_from_slice(&0u32.to_be_bytes()); // last hunk
        out.extend_from_slice(&longs.to_be_bytes()); // hunk 0's size
        out.extend_from_slice(&HUNK_CODE.to_be_bytes());
        out.extend_from_slice(&longs.to_be_bytes());
        out.extend_from_slice(body);
        while out.len() % 4 != 0 {
            out.push(0);
        }
        out
    }

    /// The base is 32 for the standard one-hunk layout — measured on both real
    /// archives before this was written.
    #[test]
    fn the_structure_starts_where_the_hunk_data_starts() {
        let bytes = SlaveBuilder::new(16).build();
        assert_eq!(structure_base(&bytes).unwrap(), 32);
    }

    /// The base is **computed**, not assumed to be 32.
    ///
    /// Worth being precise about why this test exists, because the obvious
    /// argument for parsing the header — "a debug hunk could contain the magic"
    /// — is weaker than it looks: debug hunks come *after* the code hunk, so a
    /// search for `WHDLOADS` would still find the real structure first. The
    /// case where the two genuinely disagree is a **non-empty resident-library
    /// list**, which pushes the first hunk's data past 32. Nothing stops a
    /// slave from having one, and a reader that hardcoded 32 — or that a later
    /// hand "simplified" to `Ok(32)` — reads the game's code as a header here.
    #[test]
    fn the_base_is_computed_and_is_not_always_32() {
        let plain = SlaveBuilder::new(16).build();
        let mut with_names = plain.clone();

        // Splice a one-entry library-name list in at offset 4: a length of two
        // longwords, then the name, then the terminating zero.
        let mut list = Vec::new();
        list.extend_from_slice(&2u32.to_be_bytes());
        list.extend_from_slice(b"dos.library\0\0\0\0\0"[..8].as_ref());
        list.extend_from_slice(&0u32.to_be_bytes());
        with_names.splice(4..8, list.iter().copied());

        let shifted = structure_base(&with_names).unwrap();
        assert_eq!(structure_base(&plain).unwrap(), 32);
        assert_eq!(shifted, 32 + 12, "the list moves the hunk data along");

        // And the structure is still read correctly from the new base.
        assert_eq!(read_slave(&with_names).unwrap().version, 16);
    }

    /// A v16 slave with all three strings reads back as itself.
    #[test]
    fn a_v16_slave_reads_its_own_strings() {
        let bytes = SlaveBuilder {
            name: Some("Moonstone"),
            copyright: Some("1991 Mindscape"),
            info: Some("installed & fixed by Wepl"),
            ..SlaveBuilder::new(16)
        }
        .build();

        let facts = read_slave(&bytes).unwrap();
        assert_eq!(facts.version, 16);
        assert_eq!(facts.name.as_deref(), Some("Moonstone"));
        assert_eq!(facts.copyright.as_deref(), Some("1991 Mindscape"));
        assert_eq!(facts.info.as_deref(), Some("installed & fixed by Wepl"));
    }

    /// The three situations a Kickstart declaration can be in must give three
    /// different answers. This is the pair of cases the real archives cover
    /// between them: Moonstone is v16 with offset 0, Lotus3 is v13 with no
    /// field at all.
    #[test]
    fn a_kickstart_declaration_has_three_distinct_states() {
        let too_old = read_slave(&SlaveBuilder::new(13).build()).unwrap();
        assert_eq!(too_old.version, 13);
        assert!(too_old.kickstart.image.is_none());
        assert!(too_old.kickstart.is_empty());

        let says_none = read_slave(
            &SlaveBuilder {
                kickname_offset_override: Some(0),
                ..SlaveBuilder::new(16)
            }
            .build(),
        )
        .unwrap();
        assert!(says_none.kickstart.image.is_none());

        let names_one = read_slave(
            &SlaveBuilder {
                kickname: Some("kick34005.A500"),
                ..SlaveBuilder::new(16)
            }
            .build(),
        )
        .unwrap();
        assert_eq!(names_one.kickstart.image.as_deref(), Some("kick34005.A500"));
        assert!(!names_one.kickstart.is_empty());
    }

    /// A v9 slave has no `ws_name` at all: byte 36 onwards is the program's own
    /// data. This plants a perfectly valid-looking RPTR and string there, so a
    /// reader that ignored `ws_Version` would happily return the wrong name.
    #[test]
    fn a_pre_v10_slave_states_no_name_even_if_bytes_sit_there() {
        let mut bytes = SlaveBuilder::new(9).build();
        let base = structure_base(&bytes).unwrap();

        // A v9 structure is 36 bytes; put the decoy string at structure offset
        // 40 and point byte 36 at it.
        let text_at: u16 = 40;
        bytes.resize(base + text_at as usize, 0);
        bytes.extend_from_slice(b"Not A Real Name\0");
        bytes[base + 36..base + 38].copy_from_slice(&text_at.to_be_bytes());

        let facts = read_slave(&bytes).unwrap();
        assert_eq!(facts.version, 9);
        assert_eq!(facts.name, None, "a v9 slave has no ws_name field");
    }

    /// An RPTR past the end of the file reads as absent, not followed.
    #[test]
    fn an_offset_past_the_end_reads_as_absent() {
        let mut bytes = SlaveBuilder {
            name: Some("Fine"),
            ..SlaveBuilder::new(16)
        }
        .build();
        let base = structure_base(&bytes).unwrap();
        bytes[base + 36..base + 38].copy_from_slice(&u16::MAX.to_be_bytes());

        assert_eq!(read_slave(&bytes).unwrap().name, None);
    }

    /// A string with no terminator before the end of the file stops at the
    /// boundary instead of running off it.
    #[test]
    fn an_unterminated_string_stops_at_the_end() {
        let mut bytes = SlaveBuilder {
            name: Some("Runaway"),
            ..SlaveBuilder::new(16)
        }
        .build();
        let base = structure_base(&bytes).unwrap();
        for byte in bytes[base + 50..].iter_mut() {
            if *byte == 0 {
                *byte = b'x';
            }
        }

        let facts = read_slave(&bytes).unwrap();
        assert!(
            facts.name.as_deref().unwrap_or("").starts_with("Runaway"),
            "{:?}",
            facts.name
        );
    }

    /// `0xFF` in `ws_info` is a line break, not a character.
    #[test]
    fn ws_info_treats_the_minus_one_byte_as_a_break() {
        let mut bytes = SlaveBuilder {
            info: Some("line oneXline two"),
            ..SlaveBuilder::new(16)
        }
        .build();
        let marker = bytes.iter().position(|&b| b == b'X').unwrap();
        bytes[marker] = 0xFF;

        let facts = read_slave(&bytes).unwrap();
        assert_eq!(facts.info.as_deref(), Some("line one\nline two"));
    }

    /// Neither an executable nor a slave is refused with a reason, not silently
    /// accepted.
    #[test]
    fn a_file_that_is_not_a_slave_is_refused() {
        assert!(read_slave(b"this is not an executable at all").is_err());

        let mut bytes = SlaveBuilder::new(16).build();
        let base = structure_base(&bytes).unwrap();
        bytes[base + 4] = b'X'; // break ws_ID
        let err = read_slave(&bytes).unwrap_err();
        assert!(err.to_string().contains("WHDLOADS"), "{err}");
    }

    /// A header claiming an absurd number of hunks is refused before any
    /// arithmetic runs on it.
    #[test]
    fn an_implausible_hunk_table_is_refused() {
        let mut bytes = SlaveBuilder::new(16).build();
        bytes[8..12].copy_from_slice(&u32::MAX.to_be_bytes()); // table_size
        assert!(structure_base(&bytes).is_err());
    }

    /// The documented `ws_copy` shape, and what happens when it is not
    /// followed. Both real archives are the first case.
    #[test]
    fn the_copyright_string_splits_the_documented_way() {
        assert_eq!(
            split_copyright("1992 Gremlin"),
            (Some(1992), Some("Gremlin".to_string()))
        );
        assert_eq!(
            split_copyright("1991 Mindscape"),
            (Some(1991), Some("Mindscape".to_string()))
        );
        // Multiple holders: the first entry wins, the rest are not invented.
        assert_eq!(
            split_copyright("1983 Schega, 1989 Bad Dreams"),
            (Some(1983), Some("Schega".to_string()))
        );
        // No leading year — no year, rather than a wrong one.
        assert_eq!(
            split_copyright("Public Domain"),
            (None, Some("Public Domain".to_string()))
        );
        assert_eq!(split_copyright(""), (None, None));
    }

    /// Only the set bit is evidence.
    #[test]
    fn only_a_set_aga_bit_states_a_chipset() {
        let plain = read_slave(&SlaveBuilder::new(16).build()).unwrap();
        assert_eq!(chipset_of(&plain), None);

        let aga = read_slave(
            &SlaveBuilder {
                flags: WHDLB_REQAGA,
                ..SlaveBuilder::new(16)
            }
            .build(),
        )
        .unwrap();
        assert_eq!(chipset_of(&aga), Some(ChipsetRequirement::Aga));
        assert!(aga.requires_aga);
        assert!(!aga.requires_68020);
    }

    /// Real WHDLoad slaves, read through the same path the product uses.
    /// `#[ignore]`d and env-gated: ART ships no copyrighted content, so this
    /// needs the user's own files, unpacked somewhere the walk can see them.
    ///
    /// ```text
    /// cd src-tauri && ART_SLAVE_DIR="<a folder holding .slave files>" \
    ///   cargo test read_the_real_slaves_when_asked -- --nocapture --ignored
    /// ```
    ///
    /// Measured on 2026-08-17, before this module existed, against the two
    /// archives in `E:\amiga\Amigatolon\WHDload`:
    ///
    /// ```text
    /// Lotus3.slave     v13  "Lotus 3"    "1992 Gremlin"    kickname: field absent
    /// Moonstone.Slave  v16  "Moonstone"  "1991 Mindscape"  kickname: offset 0
    /// ```
    ///
    /// If a run disagrees with those, the *measurement* is what to re-check —
    /// they are this design's evidence, not a convenient expectation.
    #[test]
    #[ignore]
    fn read_the_real_slaves_when_asked() {
        let Ok(dir) = std::env::var("ART_SLAVE_DIR") else {
            eprintln!("ART_SLAVE_DIR unset — skipping");
            return;
        };

        let found = walk_for_slaves(std::path::Path::new(&dir));
        let mut read = 0usize;
        let mut refused = 0usize;
        for entry in &found {
            let bytes = std::fs::read(entry).unwrap();
            match read_slave(&bytes) {
                Ok(facts) => {
                    read += 1;
                    let (year, publisher) = facts
                        .copyright
                        .as_deref()
                        .map(split_copyright)
                        .unwrap_or((None, None));
                    println!(
                        "{}\n    v{} name={:?} year={:?} by={:?} aga={} 020={} kick={:?}",
                        entry.display(),
                        facts.version,
                        facts.name,
                        year,
                        publisher,
                        facts.requires_aga,
                        facts.requires_68020,
                        facts.kickstart.image
                    );
                }
                Err(err) => {
                    refused += 1;
                    println!("{}\n    REFUSED: {err}", entry.display());
                }
            }
        }
        println!(
            "\n{} slaves found, {read} read, {refused} refused",
            found.len()
        );
        assert!(!found.is_empty(), "no .slave files found under {dir}");
    }

    fn walk_for_slaves(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return out;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(walk_for_slaves(&path));
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("slave"))
            {
                out.push(path);
            }
        }
        out.sort();
        out
    }
}
