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
/// The raw field is still preferred when it has something in it. `delharc`'s
/// parser also percent-encodes non-ASCII bytes, and Amiga archives are full of
/// Latin-1 names: switching level 0 and 1 over as well would rename files that
/// extract correctly today, to fix a problem those levels do not have.
///
/// Either way the result goes through [`safe_join`](crate::core::security::path::safe_join)
/// before it becomes a path. That choke point does not move.
pub(crate) fn entry_path(header: &delharc::LhaHeader) -> String {
    if !header.filename.is_empty() {
        return String::from_utf8_lossy(&header.filename).to_string();
    }
    header.parse_pathname_to_str().to_string()
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
    /// Latin-1 names — switching those levels over would rename files that
    /// extract correctly today.
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
