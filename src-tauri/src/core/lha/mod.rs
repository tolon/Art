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
}

/// High-level archive info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LhaInfo {
    pub entry_count: usize,
    pub total_uncompressed: u64,
    pub total_compressed: u64,
    pub entries: Vec<LhaEntry>,
}

/// The entry's path, whichever header level it came from (ART-031).
///
/// The raw `filename` field is only populated for level 0 and 1 headers. Level
/// 2 and 3 — what modern tools write, and what Aminet actually hosts — leave it
/// empty and carry the name in extended headers instead. Reading the field
/// directly made ART reject those archives outright with "empty entry name".
///
/// The raw field is still preferred when it has something in it, and it is
/// decoded as **ISO-8859-1 (Latin-1)** — ART-168.
///
/// # Why Latin-1 and not UTF-8
///
/// An LHA level-0/1 filename field is a byte string with no declared
/// character set: the format predates Unicode and states no encoding at all.
/// The archives ART reads are Amiga ones, and **AmigaDOS's own native
/// character set is ISO-8859-1** — "the developers chose to use the ANSI–ISO
/// standard ISO-8859-1 (Latin 1), which includes the ASCII character set"
/// (<https://en.wikipedia.org/wiki/AmigaDOS>). That is the same fact, and the
/// same decision, as [`decode_iso646`](crate::core::iso::descriptor::decode_iso646)
/// in the ISO9660 reader, whose module doc carries the full reasoning and the
/// cross-check against a second implementation; ART-155 fixed it there and
/// left this reader untouched, which is what ART-168 is.
///
/// `String::from_utf8_lossy` was the wrong tool twice over. A Latin-1 byte
/// sequence is almost never valid UTF-8, so every high-bit byte became
/// U+FFFD — and U+FFFD is not merely ugly, it **merges distinct names**:
/// `türkçe` and `tirkçe` both collapse to `t<U+FFFD>rk<U+FFFD>e`. Latin-1 is
/// `b as char` for the whole 0x80..=0xFF range (Unicode's first 256 code
/// points *are* Latin-1 by construction), so no table is needed and no two
/// byte values ever fold together.
///
/// Measured, not assumed. The owner's own `BoingBag39-2-turkce.lha` stores
/// `LocaleUpdate\locale\catalogs\t<FC>rk<E7>e\…` in a **level-0** header
/// (read straight out of the file's header bytes), and `FC`/`E7` are exactly
/// the Latin-1 code points for `ü`/`ç`. Under the old decode ART wrote its 36
/// catalogs into a drawer AmigaDOS cannot see: the booted system listed 20
/// drawers in `SYS:Locale/Catalogs` where the host directory held 21. Every
/// non-ASCII name in the owner's whole `.lha` collection — the Turkish,
/// French, Portuguese and Brazilian BoingBags, `JanoEditor`, `Picasso96` —
/// sits in a level-0 header, so this branch is the one that carries them.
///
/// # What this does *not* change
///
/// Level 2/3 names still come from `delharc`'s `parse_pathname_to_str`, which
/// percent-encodes any byte outside 0x20..0x7E (`ü` → `%fc`). That is wrong
/// for the same reason, but fixing it means re-parsing the extended headers
/// by hand instead of using the crate's own path assembly — which is also
/// where its `..`/separator filtering lives — and no archive in the material
/// measured above carries a non-ASCII name in a level-2/3 header. Left as is,
/// deliberately, rather than rewritten blind.
///
/// Either way the result goes through [`safe_join`](crate::core::security::path::safe_join)
/// before it becomes a path. That choke point does not move.
pub(crate) fn entry_path(header: &delharc::LhaHeader) -> String {
    if !header.filename.is_empty() {
        return decode_latin1(&header.filename);
    }
    header.parse_pathname_to_str().to_string()
}

/// Decode a byte string as ISO-8859-1. See [`entry_path`] for why.
fn decode_latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

fn header_to_entry(header: &delharc::LhaHeader) -> CoreResult<LhaEntry> {
    let method = String::from_utf8_lossy(&header.compression).to_string();
    let is_dir = method == "-lhd-";
    let path = entry_path(header);
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
        let dir = std::env::temp_dir().join(format!("art-lha2-{}", std::process::id()));
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
        let dir = std::env::temp_dir().join(format!("art-lha0-{}", std::process::id()));
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
            std::process::id(),
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

    #[test]
    fn open_minimal_lha_lists_one_entry() {
        let dir = std::env::temp_dir().join(format!(
            "art-lha-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
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
        let dir = std::env::temp_dir().join(format!(
            "art-lha-err-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let bad = dir.join("bad.lha");
        std::fs::write(&bad, b"not an lha").unwrap();

        let err = open_archive(&bad).unwrap_err();
        assert!(matches!(err, CoreError::Malformed { .. }));

        std::fs::remove_dir_all(&dir).ok();
    }
}
