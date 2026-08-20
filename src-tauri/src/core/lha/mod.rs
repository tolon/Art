//! LHA archive engine (Phase 1).
//!
//! Uses the mature `delharc` crate for LZSS/Huffman decompression, while ART
//! owns header interpretation, security (path traversal), and WHDLoad detection.

pub mod safe_extract;
pub mod whdload;

pub use safe_extract::{extract_archive, ExtractOutcome, OverwritePolicy};
pub use whdload::detect_whdload;

use serde::{Deserialize, Serialize};

use crate::core::error::{CoreError, CoreResult};

/// One archive entry, surfaced to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LhaEntry {
    pub path: String,
    pub is_dir: bool,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub method: String,
    /// MS-DOS timestamp bits (raw); decoded to unix on the frontend if needed.
    pub last_modified: u32,
    /// The AmigaDOS file comment, when the archive carried one — see
    /// [`entry_name`]. Empty for the overwhelming majority of entries, and
    /// `#[serde(default)]` so a value serialised before this field existed
    /// still reads back.
    #[serde(default)]
    pub comment: String,
}

/// High-level archive info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LhaInfo {
    pub entry_count: usize,
    pub total_uncompressed: u64,
    pub total_compressed: u64,
    pub entries: Vec<LhaEntry>,
}

/// LHA's own path separator inside a stored path.
///
/// Not `/` and not `\\`: the format uses `0xFF`, which is also a perfectly
/// ordinary Latin-1 character (`ÿ`) — so it has to be recognised as a
/// separator *before* the bytes are decoded, or `doc<FF>x` reads as `docÿx`.
/// `/` and `\\` are left alone and handled by
/// [`safe_join`](crate::core::security::path::safe_join), which already
/// normalises both and is the layer allowed to refuse.
const PATH_SEPARATOR_FF: u8 = 0xFF;

/// Extension header type `0x02` — the directory an entry lives in.
const EXT_HEADER_PATH: u8 = 0x02;
/// Extension header type `0x01` — the entry's own name.
const EXT_HEADER_FILENAME: u8 = 0x01;

/// One entry's name, as ART reads it: the path, and the Amiga file comment
/// when the archive carried one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct EntryName {
    /// `/`-separated, relative to the archive root.
    pub path: String,
    /// The AmigaDOS file comment, or empty. See [`entry_name`]'s own doc.
    pub comment: String,
}

/// The entry's path and comment, whichever header level it came from
/// (ART-031, ART-168, and this round's F1/F3).
///
/// # Where a name actually lives, measured across the owner's 41 archives
///
/// 8,016 entries, every one parsed straight out of its header bytes rather
/// than from a listing tool:
///
/// | level | entries | non-ASCII name | `name\0comment` | drawer in a `0x02` header |
/// |---|---|---|---|---|
/// | 0 | 4,843 | 483 | 126 | — (the field holds the whole path) |
/// | 1 | 914 | 0 | 0 | **880** |
/// | 2 | 2,259 | 0 | 0 | 2,252 |
///
/// Three separate things follow from that table, and this function does all
/// three.
///
/// # 1. Latin-1, not UTF-8 (ART-168)
///
/// An LHA filename field is a byte string with no declared character set: the
/// format predates Unicode and states no encoding at all. The archives ART
/// reads are Amiga ones, and **AmigaDOS's own native character set is
/// ISO-8859-1** — "the developers chose to use the ANSI–ISO standard
/// ISO-8859-1 (Latin 1), which includes the ASCII character set"
/// (<https://en.wikipedia.org/wiki/AmigaDOS>). Same fact, same decision, as
/// [`decode_iso646`](crate::core::iso::descriptor::decode_iso646) in the
/// ISO9660 reader, whose module doc carries the full reasoning; ART-155 fixed
/// it there and left this reader untouched, which is what ART-168 was.
///
/// `String::from_utf8_lossy` was wrong twice over. A Latin-1 byte sequence is
/// almost never valid UTF-8, so every high-bit byte became U+FFFD — and
/// U+FFFD **merges distinct names**: `türkçe` and `tirkçe` both collapse to
/// `t<U+FFFD>rk<U+FFFD>e`. Latin-1 is `b as char` across 0x80..=0xFF, so no
/// table is needed and no two byte values ever fold together.
///
/// Measured: `BoingBag39-2-turkce.lha` stores
/// `LocaleUpdate\locale\catalogs\t<FC>rk<E7>e\…` in a level-0 header, and
/// `FC`/`E7` are exactly Latin-1 `ü`/`ç`. Under the old decode the booted
/// system listed 20 drawers in `SYS:Locale/Catalogs` where the host held 21.
/// All 483 non-ASCII names in the collection are level-0, but that is a fact
/// about this collection, not about the format — every level decodes the same
/// way here, so a level-1 archive with an accented name is right too.
///
/// # 2. A level-1 entry's drawer lives in an extension header (F1)
///
/// A level-1 header's `filename` field holds the **base name only**; the
/// directory is in extension header `0x02`, separated by `0xFF`. Reading the
/// field alone and stopping — which is what ART did — flattens the archive:
/// **880 of the 914 level-1 entries** in the owner's collection lose their
/// drawer, including all 316 of `AmiSSL-v5-OS3.lha` and all 283 of
/// `Update3.2.2.lha`, an AmigaOS update this engine is meant to install. Every
/// one of them would have landed in the archive root, on top of each other.
///
/// So the extension headers are read for every level that has them, and the
/// `0x02` directory is prepended to the name.
///
/// # 3. A level-0/1 name can carry an Amiga comment after a NUL (F3)
///
/// Amiga LhA stores `name\0comment` in the one field. `delharc` truncates at
/// the NUL; decoding the whole field does not, so the name came back with a
/// NUL and the comment glued on — 126 entries in the collection, e.g.
/// `BoingBag3.9-1\…\spatch` + `6.50 (26.8.93)`. A NUL cannot be part of a
/// filename on any system ART writes to, so the name is always cut there.
///
/// The tail is **kept**, not discarded: an AmigaDOS file comment is real
/// user-visible metadata, and losing it silently is exactly what
/// [ART-078](../../../docs/ISSUES.md) is filed about on the ISO9660 side. It
/// travels as [`EntryName::comment`] and reaches [`LhaEntry::comment`]. (The
/// `0x3F` comment *extension* header is not read: no archive in the measured
/// collection carries one, and untested code for an unmeasured case is worse
/// than none.)
///
/// # Why there is no "cannot resolve the drawer" refusal
///
/// The obvious guard — refuse a level-1 entry whose extension area cannot be
/// read, rather than root it — was written, and then removed as unreachable
/// once `delharc` was read rather than assumed:
///
/// * `parser.rs:316-329` walks the **whole** extension chain before building
///   the header, checks each declared length against the level-1 skip size
///   (`SkipSizeMismatch`) or the level-2/3 long header length
///   (`LongSizeMismatch`), and `?`-propagates a short read. A header whose
///   chain does not add up never reaches this function at all.
/// * `ExtraHeaderIter::next` (`parser.rs:71-88`) returns `None` **only** when
///   the remaining length is `0`. So `first_header_len > 0` with an empty
///   iterator cannot happen, and neither can the `split_at` in it overrun —
///   which matters, because the release profile aborts on panic.
///
/// Probed as well as read: three mutations of a level-1 fixture (`next_ext`
/// zeroed, huge, and pointing at an empty header) are all rejected by
/// `delharc`'s own base-header checksum, since `next_ext` sits inside the
/// checksummed region.
///
/// A level-1 entry with **no** `0x02` header is not an error either — it is a
/// file at the archive root, which 34 of the owner's 914 level-1 entries
/// genuinely are. So the honest answer is that reading the header is the
/// whole fix, and a damaged archive is refused by the layer that can actually
/// tell (`a_level_one_header_with_a_damaged_extension_area_is_refused` pins
/// that it is refused rather than silently rooted).
///
/// Whatever comes back still goes through
/// [`safe_join`](crate::core::security::path::safe_join) before it becomes a
/// path. That choke point does not move.
pub(crate) fn entry_name(header: &delharc::LhaHeader) -> EntryName {
    // The directory, from extension header 0x02. Present on levels 1..3.
    let mut directory: Option<String> = None;
    let mut ext_name: Option<Vec<u8>> = None;
    for extension in header.iter_extra() {
        match extension {
            [EXT_HEADER_PATH, data @ ..] if !data.is_empty() => {
                directory = Some(decode_path(data));
            }
            [EXT_HEADER_FILENAME, data @ ..] if !data.is_empty() => {
                ext_name = Some(split_at_nul(data).0.to_vec());
            }
            _ => {}
        }
    }

    // The raw field is preferred when it has something in it: it is the only
    // place ART can apply the right charset itself, since `delharc`'s own
    // path assembly percent-encodes every byte outside 0x20..0x7E.
    let (raw_name, comment) = if !header.filename.is_empty() {
        let (name, rest) = split_at_nul(&header.filename);
        (name.to_vec(), decode_latin1(rest))
    } else if let Some(name) = ext_name {
        (name, String::new())
    } else {
        (Vec::new(), String::new())
    };

    // A level-0 field holds the whole path; a level-1 field holds one name,
    // with the drawer in the extension header above. Decoding both the same
    // way is what makes one rule cover both.
    let name = decode_path(&raw_name);
    let path = match directory {
        // A stored directory ends with its own separator (`doc<FF>`), so the
        // trailing `/` is the terminator rather than an empty component —
        // trimmed here, and only here, so nothing else has to know.
        Some(dir) => {
            let dir = dir.trim_end_matches('/');
            match (dir.is_empty(), name.is_empty()) {
                (true, _) => name,
                (false, true) => dir.to_string(),
                (false, false) => format!("{dir}/{name}"),
            }
        }
        None => name,
    };

    // Nothing above can produce a name for a level-3 header, whose extension
    // headers use 32-bit lengths `iter_extra` handles but which ART has never
    // seen in the wild. Fall back to `delharc`'s own assembly rather than
    // returning nothing at all.
    let path = if path.is_empty() {
        header.parse_pathname_to_str().to_string()
    } else {
        path
    };

    EntryName { path, comment }
}

/// Split a level-0/1 filename field at the first NUL: the name, then whatever
/// Amiga LhA stored after it as the file comment. See [`entry_name`].
fn split_at_nul(bytes: &[u8]) -> (&[u8], &[u8]) {
    match bytes.iter().position(|&b| b == 0) {
        Some(at) => (&bytes[..at], &bytes[at + 1..]),
        None => (bytes, &[]),
    }
}

/// Decode stored path bytes as Latin-1, turning the format's own `0xFF`
/// separator into `/` and changing **nothing else**.
///
/// # Two drafts of this were wrong in the same way
///
/// The first split on every separator, dropped `.`/`..` components the way
/// `delharc` does, and rejoined. That turned `../../evil.txt` into
/// `evil.txt`: still inside the destination, but reported as a **successful
/// extraction** rather than a refused traversal
/// (`traversal_entry_is_rejected_not_extracted`). The second kept `..` but
/// still dropped *empty* components, which turned the absolute
/// `/art-oracle-root-escape.txt` into a relative name that
/// [`safe_join`](crate::core::security::path::safe_join) then accepted
/// (`hostile_entries_are_rejected_at_their_real_target_not_just_absent_from_scratch`).
///
/// Both are the same mistake: **normalising a hostile name into a benign
/// one destroys the report**, which is the quiet this whole round is about.
/// Containment was never in question — `safe_join` had it either way — but a
/// security boundary that silently rewrites is one nobody can audit.
///
/// So this does the one thing the *format* requires and nothing a
/// *filesystem* might want. `0xFF` is LHA's own separator and has no other
/// meaning, so it becomes `/`. Everything else — `..`, a leading `/`, a
/// `C:` prefix, a `\\` — survives verbatim to `safe_join`, which normalises
/// and **refuses by name**. That is also the smallest possible change from
/// the behaviour that shipped, which passed the raw field through untouched.
fn decode_path(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| {
            if b == PATH_SEPARATOR_FF {
                '/'
            } else {
                b as char
            }
        })
        .collect()
}

/// Decode a byte string as ISO-8859-1. See [`entry_name`] for why.
fn decode_latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

/// The entry's path alone, for callers with nothing to say about a comment.
pub(crate) fn entry_path(header: &delharc::LhaHeader) -> String {
    entry_name(header).path
}

fn header_to_entry(header: &delharc::LhaHeader) -> CoreResult<LhaEntry> {
    let method = String::from_utf8_lossy(&header.compression).to_string();
    let is_dir = method == "-lhd-";
    let EntryName { path, comment } = entry_name(header);
    if path.is_empty() {
        return Err(CoreError::Malformed {
            format: "lha".into(),
            detail: "empty entry name".into(),
        });
    }
    Ok(LhaEntry {
        path,
        is_dir,
        compressed_size: header.compressed_size,
        uncompressed_size: header.original_size,
        method,
        last_modified: header.last_modified,
        comment,
    })
}

/// Open an LHA archive and list its entries (no extraction).
pub fn open_archive(path: &std::path::Path) -> CoreResult<LhaInfo> {
    let file = std::fs::File::open(path)?;
    let mut reader = delharc::LhaDecodeReader::new(file).map_err(|e| CoreError::Malformed {
        format: "lha".into(),
        detail: format!("failed to read LHA header: {e}"),
    })?;

    let mut entries = Vec::new();
    let mut total_uncompressed = 0u64;
    let mut total_compressed = 0u64;

    loop {
        let header = reader.header();
        let entry = header_to_entry(header)?;
        total_uncompressed += entry.uncompressed_size;
        total_compressed += entry.compressed_size;
        entries.push(entry);

        let has_more = reader.next_file().map_err(|e| CoreError::Malformed {
            format: "lha".into(),
            detail: format!("failed to seek past entry: {e}"),
        })?;
        if !has_more {
            break;
        }
    }

    let entry_count = entries.len();
    Ok(LhaInfo {
        entry_count,
        total_uncompressed,
        total_compressed,
        entries,
    })
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A scratch directory nothing else in the process will pick — see
    /// [`crate::core::test_scratch_id`] for why the counter is load-bearing.
    fn tmp(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("art-{tag}-{}", crate::core::test_scratch_id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A stored (-lh0-) level-0 archive holding the given files.
    ///
    /// Names may contain `/`, which is how an Amiga archive carries a folder —
    /// so this builds the nested-directory fixtures too. A name ending in `/`
    /// becomes an explicit **directory entry** (`-lhd-`, no payload), the same
    /// convention `sevenz::tests::make_7z_with` follows and the only way to
    /// express a genuinely empty drawer in an archive.
    pub fn make_lha_with(files: &[(&str, &[u8])]) -> Vec<u8> {
        let raw: Vec<(&[u8], &[u8])> = files
            .iter()
            .map(|(name, content)| (name.as_bytes(), *content))
            .collect();
        make_lha_with_raw_names(&raw)
    }

    /// The same, over **raw name bytes**.
    ///
    /// An LHA entry name is bytes, not UTF-8, and Amiga archives are full of
    /// Latin-1 ones: `BoingBag39-2-turkce.lha` spells its own drawer
    /// `t FC r k E7 e`. A `&str` fixture cannot express that at all, which is
    /// precisely why nothing in the suite ever did — and why ART-168 (an
    /// entry name's high-bit bytes replaced with U+FFFD) survived every test
    /// and was found only by a real run.
    pub fn make_lha_with_raw_names(files: &[(&[u8], &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        for (name, content) in files {
            match name.strip_suffix(b"/") {
                Some(dir_name) => buf.extend_from_slice(&level0_dir_entry(dir_name)),
                None => buf.extend_from_slice(&level0_entry(name, content)),
            }
        }
        buf.push(0x00); // end of archive
        buf
    }

    /// One level-0 **directory** header — `-lhd-`, zero sizes, no payload.
    /// `core::lha::header_to_entry` reads exactly the method field to decide
    /// `is_dir`, and so does `core::archive::lha`'s backend.
    fn level0_dir_entry(name: &[u8]) -> Vec<u8> {
        let mut buf = level0_entry(name, b"");
        buf[2..7].copy_from_slice(b"-lhd-");
        let header_len = buf[0] as usize;
        let cks: u8 = buf[2..2 + header_len]
            .iter()
            .fold(0u8, |a, &b| a.wrapping_add(b));
        buf[1] = cks;
        buf
    }

    /// One level-0 header plus its stored payload.
    fn level0_entry(filename: &[u8], content: &[u8]) -> Vec<u8> {
        let compressed_size: u32 = content.len() as u32;
        let uncompressed_size: u32 = content.len() as u32;
        let dos_date: u16 = (((2025 - 1980) << 9) | (1 << 5) | 1) as u16;
        let dos_time: u16 = 0;

        let header_len: u8 = (22 + filename.len()) as u8;
        let mut buf = Vec::new();
        buf.push(header_len);
        buf.push(0); // checksum placeholder
        buf.extend_from_slice(b"-lh0-");
        buf.extend_from_slice(&compressed_size.to_le_bytes());
        buf.extend_from_slice(&uncompressed_size.to_le_bytes());
        buf.extend_from_slice(&dos_time.to_le_bytes());
        buf.extend_from_slice(&dos_date.to_le_bytes());
        buf.push(0x20); // attribute (normal)
        buf.push(0x00); // level 0
        buf.push(filename.len() as u8);
        buf.extend_from_slice(filename);
        buf.extend_from_slice(&0u16.to_le_bytes()); // CRC

        let cks: u8 = buf[2..2 + header_len as usize]
            .iter()
            .fold(0u8, |a, &b| a.wrapping_add(b));
        buf[1] = cks;
        buf.extend_from_slice(content);
        buf
    }

    /// A stored (-lh0-) archive with a **level-1** header, whose drawer lives
    /// in extension header `0x02` and whose `filename` field holds the base
    /// name alone.
    ///
    /// This is the shape 914 of the owner's own entries have, and the shape
    /// ART used to flatten: 880 of them carry a `0x02` directory that
    /// `entry_path` never read, so every one would have landed in the archive
    /// root. Built byte-exact from the level-1 layout:
    ///
    /// ```text
    /// [hsize:1][cks:1][method:5][skip:4][usize:4][time:2][date:2][attr:1]
    /// [level:1][namelen:1][name:n][crc:2][os:1][next ext size:2][ext…][data]
    /// ```
    ///
    /// `hsize` counts from `method` through `next ext size`, `skip` is the
    /// compressed size **plus** every extension header byte, and each
    /// extension header is `[type:1][data…][next size:2]` whose declared size
    /// covers all three. The directory's own separator is `0xFF`, not `/`.
    pub fn make_level1_lha(directory: &[u8], name: &[u8], content: &[u8]) -> Vec<u8> {
        // One extension header: the 0x02 directory, then a zero terminator.
        let ext_size = (1 + directory.len() + 2) as u16;
        let mut ext = Vec::new();
        ext.push(0x02u8);
        ext.extend_from_slice(directory);
        ext.extend_from_slice(&0u16.to_le_bytes()); // no further ext header

        let header_len: u8 = (25 + name.len()) as u8;
        let skip = (content.len() + ext.len()) as u32;

        let mut buf = Vec::new();
        buf.push(header_len);
        buf.push(0); // checksum placeholder
        buf.extend_from_slice(b"-lh0-");
        buf.extend_from_slice(&skip.to_le_bytes());
        buf.extend_from_slice(&(content.len() as u32).to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes()); // dos time
        buf.extend_from_slice(&((((2025 - 1980) << 9) | (1 << 5) | 1) as u16).to_le_bytes());
        buf.push(0x20); // attribute
        buf.push(0x01); // level 1
        buf.push(name.len() as u8);
        buf.extend_from_slice(name);
        buf.extend_from_slice(&0u16.to_le_bytes()); // crc
        buf.push(b'A'); // OS: Amiga
        buf.extend_from_slice(&ext_size.to_le_bytes());

        let cks: u8 = buf[2..2 + header_len as usize]
            .iter()
            .fold(0u8, |a, &b| a.wrapping_add(b));
        buf[1] = cks;

        buf.extend_from_slice(&ext);
        buf.extend_from_slice(content);
        buf.push(0x00); // end of archive
        buf
    }

    /// A minimal stored (-lh0-) LHA archive containing one tiny file, ending
    /// with a 0x00 terminator byte. Built byte-exact per the level-0 spec.
    pub fn make_minimal_lha() -> Vec<u8> {
        let filename = b"hi.txt";
        let content = b"hi";
        let compressed_size: u32 = content.len() as u32;
        let uncompressed_size: u32 = content.len() as u32;
        // MS-DOS date/time: 2025-01-01 00:00:00
        let dos_date: u16 = (((2025 - 1980) << 9) | (1 << 5) | 1) as u16;
        let dos_time: u16 = 0;

        // Level-0 header layout:
        // [hdr_size:1][cks:1][method:5][csize:4][usize:4][time:2][date:2][attr:1][level:1][namelen:1][name:N][crc:2]
        // Header length from method (offset 2) to end of header is: 5 + 4 + 4 + 2 + 2 + 1 + 1 + 1 + N + 2 = 22 + N
        let header_len: u8 = (22 + filename.len()) as u8;
        let mut buf = Vec::new();
        buf.push(header_len);
        buf.push(0); // checksum placeholder
        buf.extend_from_slice(b"-lh0-");
        buf.extend_from_slice(&compressed_size.to_le_bytes());
        buf.extend_from_slice(&uncompressed_size.to_le_bytes());
        buf.extend_from_slice(&dos_time.to_le_bytes());
        buf.extend_from_slice(&dos_date.to_le_bytes());
        buf.push(0x20); // attribute (normal)
        buf.push(0x00); // level 0
        buf.push(filename.len() as u8);
        buf.extend_from_slice(filename);
        buf.extend_from_slice(&0u16.to_le_bytes()); // CRC

        let cks: u8 = buf[2..2 + header_len as usize]
            .iter()
            .fold(0u8, |a, &b| a.wrapping_add(b));
        buf[1] = cks;
        // Data payload
        buf.extend_from_slice(content);
        // End-of-archive marker (size 0)
        buf.push(0x00);
        buf
    }

    /// A stored (-lh0-) archive with a **level-2** header, where the name lives
    /// in an extended header and the raw `filename` field is empty.
    ///
    /// This is what modern tools write and what Aminet hosts, so it is the
    /// shape ART used to choke on (ART-031). Synthetic, like every fixture
    /// here — built byte-exact from the level-2 layout:
    ///
    /// ```text
    /// [total:2][method:5][csize:4][usize:4][unix time:4][reserved:1]
    /// [level:1][crc:2][os:1][next ext size:2][ext headers…]
    /// ```
    ///
    /// Each extended header is `[type:1][data…][next size:2]`, and its declared
    /// size counts all three parts. Type `0x01` is the file name.
    pub fn make_level2_lha(name: &str, content: &[u8]) -> Vec<u8> {
        let name = name.as_bytes();

        // One extended header: type byte + the name + the trailing size field.
        let ext_size = (1 + name.len() + 2) as u16;
        // Base header is fixed at 26 bytes up to and including "next ext size".
        let total_header = 26 + ext_size;

        let mut buf = Vec::new();
        buf.extend_from_slice(&total_header.to_le_bytes());
        buf.extend_from_slice(b"-lh0-");
        buf.extend_from_slice(&(content.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(content.len() as u32).to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // unix timestamp
        buf.push(0x20); // reserved
        buf.push(0x02); // header level 2
        buf.extend_from_slice(&0u16.to_le_bytes()); // file CRC
        buf.push(b'U'); // OS identifier: Unix
        buf.extend_from_slice(&ext_size.to_le_bytes());

        buf.push(0x01); // extended header type: file name
        buf.extend_from_slice(name);
        buf.extend_from_slice(&0u16.to_le_bytes()); // no further extended headers

        buf.extend_from_slice(content);
        buf.push(0x00); // end of archive
        buf
    }

    /// ART-031. Level 2 and 3 headers leave the raw `filename` field empty, so
    /// reading it directly made ART reject the archives Aminet actually hosts
    /// with "empty entry name". Found by fetching a real AmiSSL release.
    #[test]
    fn a_level_two_header_is_read_from_its_extended_header() {
        let dir = std::env::temp_dir().join(format!("art-lha2-{}", crate::core::test_scratch_id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("level2.lha");
        std::fs::write(&archive, make_level2_lha("readme.txt", b"hello")).unwrap();

        let info = open_archive(&archive).expect("a level-2 archive must open");
        assert_eq!(info.entry_count, 1);
        assert_eq!(info.entries[0].path, "readme.txt");
        assert_eq!(info.entries[0].uncompressed_size, 5);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The level 0 and 1 path must keep using the raw field. `delharc`'s
    /// parser percent-encodes non-ASCII bytes, and Amiga archives are full of
    /// Latin-1 names — the raw field is the only place ART can apply the
    /// right charset itself (ART-168).
    #[test]
    fn a_level_zero_name_still_comes_from_the_raw_field() {
        let dir = std::env::temp_dir().join(format!("art-lha0-{}", crate::core::test_scratch_id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("level0.lha");
        std::fs::write(&archive, make_minimal_lha()).unwrap();

        let info = open_archive(&archive).unwrap();
        assert_eq!(info.entries[0].path, "hi.txt");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// **ART-168.** A level-0 name's high-bit bytes are Latin-1, not UTF-8.
    ///
    /// The bytes are the owner's own `BoingBag39-2-turkce.lha`'s, read out of
    /// its header: `74 FC 72 6B E7 65` — `türkçe`, the drawer whose 36
    /// catalogs AmigaDOS could not see when every high-bit byte arrived as
    /// U+FFFD. Latin-1 is asserted here as a *name*, not as a byte identity:
    /// `t\u{FC}rk\u{E7}e` is what a user and AmigaDOS both mean by it.
    #[test]
    fn a_level_zero_name_s_high_bit_bytes_decode_as_latin1() {
        let dir = std::env::temp_dir().join(format!(
            "art-lha-latin1-{}-{}",
            crate::core::test_scratch_id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("turkce.lha");

        // `LocaleUpdate/locale/catalogs/türkçe/sys.catalog`, Latin-1.
        let mut name: Vec<u8> = b"LocaleUpdate/locale/catalogs/".to_vec();
        name.extend_from_slice(&[0x74, 0xFC, 0x72, 0x6B, 0xE7, 0x65]);
        name.extend_from_slice(b"/sys.catalog");
        std::fs::write(
            &archive,
            make_lha_with_raw_names(&[(&name, b"catalog" as &[u8])]),
        )
        .unwrap();

        let info = open_archive(&archive).unwrap();
        assert_eq!(
            info.entries[0].path,
            "LocaleUpdate/locale/catalogs/t\u{FC}rk\u{E7}e/sys.catalog"
        );
        assert!(
            !info.entries[0].path.contains('\u{FFFD}'),
            "no byte may be replaced: {}",
            info.entries[0].path
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Latin-1 keeps distinct byte values distinct, which `from_utf8_lossy`
    /// did not: `FC` and `E9` both used to become the same U+FFFD, so two
    /// different Amiga drawers collided into one host name.
    #[test]
    fn two_names_differing_only_above_ascii_stay_two_names() {
        assert_eq!(decode_latin1(&[0x74, 0xFC, 0x74]), "t\u{FC}t");
        assert_eq!(decode_latin1(&[0x74, 0xE9, 0x74]), "t\u{E9}t");
        assert_ne!(decode_latin1(&[0xFC]), decode_latin1(&[0xE9]));
        // The whole high range is one-to-one, no table needed.
        assert_eq!(decode_latin1(&[0x80, 0xFF]), "\u{80}\u{FF}");
    }

    /// **F1.** A level-1 entry's drawer lives in extension header `0x02`, and
    /// reading the `filename` field alone flattens the archive.
    ///
    /// Measured across the owner's 41 archives: 880 of 914 level-1 entries
    /// carry one, including all 316 of `AmiSSL-v5-OS3.lha` and all 283 of
    /// `Update3.2.2.lha`. Every one of them used to come back as a bare base
    /// name, so an extraction would have piled them all into the root.
    #[test]
    fn a_level_one_entry_keeps_the_drawer_from_its_extension_header() {
        let dir = tmp("lha1");
        let archive = dir.join("level1.lha");
        // `doc/ansi2knr.1` — the real shape from `doc.lha`, whose 0x02 header
        // stores `doc` with a trailing 0xFF separator.
        std::fs::write(
            &archive,
            make_level1_lha(b"doc\xFF", b"ansi2knr.1", b"manual"),
        )
        .unwrap();

        let info = open_archive(&archive).unwrap();
        assert_eq!(info.entries[0].path, "doc/ansi2knr.1");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The same, with a nested drawer and a Latin-1 name in it — the two
    /// fixes have to compose, not merely coexist.
    #[test]
    fn a_level_one_drawer_is_split_on_0xff_and_decoded_as_latin1() {
        let dir = tmp("lha1-intl");
        let archive = dir.join("level1.lha");
        // `Locale/Catalogs/türkçe/sys.catalog`, separators 0xFF.
        let mut directory: Vec<u8> = b"Locale\xFFCatalogs\xFF".to_vec();
        directory.splice(
            directory.len()..directory.len(),
            [0x74, 0xFC, 0x72, 0x6B, 0xE7, 0x65],
        );
        directory.push(0xFF);
        std::fs::write(
            &archive,
            make_level1_lha(&directory, b"sys.catalog", b"cat"),
        )
        .unwrap();

        let info = open_archive(&archive).unwrap();
        assert_eq!(
            info.entries[0].path,
            "Locale/Catalogs/t\u{FC}rk\u{E7}e/sys.catalog"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A level-1 entry with **no** `0x02` header is a file at the archive
    /// root, not an error — 34 of the owner's 914 level-1 entries are exactly
    /// this, so a refusal here would break real archives.
    #[test]
    fn a_level_one_entry_with_no_directory_header_sits_at_the_root() {
        let dir = tmp("lha1-root");
        let archive = dir.join("level1.lha");
        std::fs::write(&archive, make_level1_lha(b"", b"readme.txt", b"hi")).unwrap();

        let info = open_archive(&archive).unwrap();
        assert_eq!(info.entries[0].path, "readme.txt");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A damaged extension area is **refused**, never silently rooted.
    ///
    /// ART has no refusal branch of its own for this — see `entry_name`'s doc
    /// for why one would be unreachable — so what this pins is the
    /// *guarantee*, not the layer: whoever notices, the entry must not come
    /// back as a bare base name in the archive root.
    #[test]
    fn a_level_one_header_with_a_damaged_extension_area_is_refused() {
        let dir = tmp("lha1-damaged");
        let archive = dir.join("damaged.lha");
        let mut bytes = make_level1_lha(b"doc\xFF", b"ansi2knr.1", b"manual");
        // `next ext size` is the last two bytes of the base header.
        let header_len = bytes[0] as usize;
        let at = 2 + header_len - 2;
        bytes[at..at + 2].copy_from_slice(&0u16.to_le_bytes());
        std::fs::write(&archive, &bytes).unwrap();

        match open_archive(&archive) {
            Err(err) => assert_eq!(err.code(), "ART-FORMAT-MALFORMED", "{err}"),
            Ok(info) => assert_ne!(
                info.entries[0].path, "ansi2knr.1",
                "a damaged entry must not be quietly placed in the root"
            ),
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    /// **F3.** Amiga LhA stores `name\0comment` in the one field. Decoding the
    /// whole field glued the comment onto the name behind a NUL — 126 entries
    /// in the owner's collection do this, e.g. `…\spatch` + `6.50 (26.8.93)`.
    #[test]
    fn a_name_carrying_an_amiga_comment_is_split_at_the_nul() {
        let dir = tmp("lha-comment");
        let archive = dir.join("commented.lha");
        std::fs::write(
            &archive,
            make_lha_with_raw_names(&[(b"C/spatch\x006.50 (26.8.93)", b"exe" as &[u8])]),
        )
        .unwrap();

        let info = open_archive(&archive).unwrap();
        assert_eq!(info.entries[0].path, "C/spatch");
        assert!(
            !info.entries[0].path.contains('\0'),
            "no NUL may survive into a path"
        );
        // Kept, not discarded: an AmigaDOS file comment is real metadata, and
        // losing it quietly is what ART-078 is filed about on the disc side.
        assert_eq!(info.entries[0].comment, "6.50 (26.8.93)");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A comment can itself be Latin-1 — 9 of the 126 are.
    #[test]
    fn an_amiga_comment_is_latin1_too() {
        let dir = tmp("lha-comment-intl");
        let archive = dir.join("commented.lha");
        let mut name: Vec<u8> = b"Docs/liesmich\x00Gr".to_vec();
        name.push(0xFC); // ü
        name.extend_from_slice(b"\xDFe"); // ße
        std::fs::write(
            &archive,
            make_lha_with_raw_names(&[(&name, b"doc" as &[u8])]),
        )
        .unwrap();

        let info = open_archive(&archive).unwrap();
        assert_eq!(info.entries[0].path, "Docs/liesmich");
        assert_eq!(info.entries[0].comment, "Gr\u{FC}\u{DF}e");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// An entry with no comment reports none, rather than an empty-looking
    /// something. The overwhelming majority of entries are this.
    #[test]
    fn an_ordinary_entry_carries_no_comment() {
        let dir = tmp("lha-nocomment");
        let archive = dir.join("plain.lha");
        std::fs::write(&archive, make_minimal_lha()).unwrap();

        let info = open_archive(&archive).unwrap();
        assert_eq!(info.entries[0].path, "hi.txt");
        assert_eq!(info.entries[0].comment, "");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// `..` survives assembly so [`safe_join`] can refuse it and *say so*.
    ///
    /// An earlier draft of `join_components` dropped `.`/`..` the way
    /// `delharc` does, which turned `../../evil.txt` into `evil.txt`: still
    /// contained, but reported as a successful extraction rather than a
    /// refused traversal. Silently normalising an attack is the same class of
    /// quiet this whole round is about, so the components are kept and the
    /// security boundary decides.
    #[test]
    fn a_traversal_component_survives_assembly_for_safe_join_to_refuse() {
        let dir = tmp("lha-trav");
        let archive = dir.join("trav.lha");
        std::fs::write(
            &archive,
            make_lha_with_raw_names(&[(b"../../evil.txt", b"x" as &[u8])]),
        )
        .unwrap();

        let info = open_archive(&archive).unwrap();
        assert_eq!(info.entries[0].path, "../../evil.txt");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The same rule for an **absolute** name, which a second draft of the
    /// assembly also normalised away: dropping the empty component a leading
    /// `/` produces turned `/art-oracle-root-escape.txt` into a relative name
    /// `safe_join` was happy to accept, and the archives oracle test lost one
    /// of its three expected refusals.
    #[test]
    fn an_absolute_name_survives_assembly_too() {
        let dir = tmp("lha-abs");
        let archive = dir.join("abs.lha");
        std::fs::write(
            &archive,
            make_lha_with_raw_names(&[
                (b"/root-escape.txt", b"x" as &[u8]),
                (br"C:\drive-escape.txt", b"y" as &[u8]),
            ]),
        )
        .unwrap();

        let info = open_archive(&archive).unwrap();
        let paths: Vec<&str> = info.entries.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.contains(&"/root-escape.txt"), "{paths:?}");
        assert!(
            paths.iter().any(|p| p.starts_with("C:")),
            "a drive prefix must reach safe_join intact: {paths:?}"
        );
    }

    #[test]
    fn open_minimal_lha_lists_one_entry() {
        let dir = std::env::temp_dir().join(format!("art-lha-{}", crate::core::test_scratch_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("test.lha");
        std::fs::write(&archive, make_minimal_lha()).unwrap();

        let info = open_archive(&archive).unwrap();
        assert_eq!(info.entry_count, 1);
        assert_eq!(info.entries[0].path, "hi.txt");
        assert_eq!(info.entries[0].uncompressed_size, 2);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_non_archive() {
        let dir =
            std::env::temp_dir().join(format!("art-lha-err-{}", crate::core::test_scratch_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bad = dir.join("bad.lha");
        std::fs::write(&bad, b"not an lha").unwrap();

        let err = open_archive(&bad).unwrap_err();
        assert!(matches!(err, CoreError::Malformed { .. }));

        std::fs::remove_dir_all(&dir).ok();
    }
}
