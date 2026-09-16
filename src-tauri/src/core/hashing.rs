//! File integrity hashing.
//!
//! SHA256 is ART's canonical integrity hash (used for duplicate detection,
//! operation verification, and snapshot metadata) and the only one that
//! speaks for the *safety* of a write — nothing about MD5 below changes that.
//!
//! MD5 exists here for exactly one job: it is the primary key of a table ART
//! did not build — `core/osinstall/media_hashes.json`'s 186-row list of known
//! install media, adopted from Emu68 Hatcher (the lookup that reads this
//! table is a following task; this module only supplies the key it is keyed
//! on). Identifying a user's own disk against somebody else's database is
//! not a security decision; it is a
//! lookup, and the lookup only works if the key is computed the same way the
//! table's own author computed it. That is MD5, not a choice ART made and not
//! one it can revisit — "upgrading" [`md5_file`] to SHA256 would silently
//! break every row in the table rather than make anything safer. Never use
//! either function in this module as a security primitive, and never use
//! [`md5_file`]/[`md5_bytes`] anywhere ART needs to know a file has not been
//! tampered with — that is [`sha256_file`]/[`sha256_bytes`]'s job alone.
//!
//! Streams files in chunks so large HDF images do not blow up memory.

use std::io::Read;
use std::path::Path;

use md5::Md5;
use sha2::{Digest, Sha256};

use crate::core::error::CoreResult;

/// Chunk size used when streaming files through the hasher (64 KiB).
const CHUNK: usize = 64 * 1024;

/// Compute the SHA256 hex digest of a file, streaming from disk.
pub fn sha256_file(path: &Path) -> CoreResult<String> {
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex_encode(&hasher.finalize()))
}

/// Compute the SHA256 hex digest of an in-memory byte slice.
pub fn sha256_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex_encode(&hasher.finalize())
}

/// Compute the MD5 hex digest of a file, streaming from disk.
///
/// **Table lookup only** — see the module doc. Mirrors [`sha256_file`]'s
/// chunking exactly, for the same reason: a multi-gigabyte HDF must not be
/// read into memory whole just to key it into `mediahash.rs`'s table.
pub fn md5_file(path: &Path) -> CoreResult<String> {
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Md5::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex_encode(&hasher.finalize()))
}

/// Compute the MD5 hex digest of an in-memory byte slice.
///
/// **Table lookup only** — see the module doc.
pub fn md5_bytes(data: &[u8]) -> String {
    let mut hasher = Md5::new();
    hasher.update(data);
    hex_encode(&hasher.finalize())
}

/// CRC-16/ARC — reflected, polynomial `0xA001`, init `0x0000`, no final XOR.
///
/// This is the checksum a WHDLoad Slave puts in `ws_kickcrc` to identify the
/// Kickstart image it wants loaded out of `DEVS:Kickstarts/`. ART's own
/// integrity hash is SHA-256 ([`sha256_bytes`]); this exists **only** to
/// compare against a value somebody else computed, and must never be used as
/// a security primitive.
///
/// Reference: `WHDLoad/Src/programs/CRC16.asm`, Aminet `dev/misc/WHDLoad_dev.lha`.
/// Its table is built by shifting right and conditionally `eor`-ing `$a001`,
/// which is the reflected form; the accumulator starts at zero and is
/// returned unmodified.
pub fn crc16_arc(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &byte in data {
        crc ^= byte as u16;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xA001;
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}

/// The CRC-32/ISO-HDLC (a.k.a. IEEE 802.3, "zip CRC-32") table: reflected,
/// polynomial `0xEDB8_8320`.
///
/// Built once, at compile time, with a `while` loop rather than an iterator —
/// `const fn` cannot use `for`. This is the **one** CRC-32 table in ART:
/// `core::rom::compute_crc32` and every archive backend's checksum go through
/// [`crc32_ieee`]/[`Crc32`] rather than each carrying its own copy.
const CRC32_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut n = 0usize;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
};

/// CRC-32/ISO-HDLC, computed incrementally.
///
/// The one-shot [`crc32_ieee`] is a thin wrapper over this; use `Crc32`
/// directly when the bytes do not all arrive at once (an archive backend
/// reading a stream in chunks, for instance) — the two must and do agree,
/// which `crc32_matches_the_standard_vector_whole_and_in_pieces` checks.
pub struct Crc32(u32);

impl Crc32 {
    pub fn new() -> Self {
        Self(0xFFFF_FFFF)
    }

    pub fn update(&mut self, bytes: &[u8]) {
        let mut crc = self.0;
        for &b in bytes {
            crc = CRC32_TABLE[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
        }
        self.0 = crc;
    }

    pub fn finish(&self) -> u32 {
        !self.0
    }
}

impl Default for Crc32 {
    fn default() -> Self {
        Self::new()
    }
}

/// CRC-32/ISO-HDLC (a.k.a. IEEE 802.3) over a whole byte slice — the standard
/// checksum ZIP, PNG and Ethernet all use.
///
/// One place: [`core::rom::compute_crc32`](crate::core::rom::compute_crc32)
/// delegates here, and every archive backend that has to report or verify a
/// CRC-32 uses this rather than keeping its own table.
pub fn crc32_ieee(bytes: &[u8]) -> u32 {
    let mut crc = Crc32::new();
    crc.update(bytes);
    crc.finish()
}

/// Lowercase hex encoding of a byte slice.
pub fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ScratchDir;

    /// WHDLoad's `ws_kickcrc` is CRC-16/ARC, and this test states the
    /// parameters it is asserting so a later mismatch can be read as "ART's
    /// bug" or "a different CRC16" without re-deriving anything:
    /// **reflected, polynomial 0xA001, init 0x0000, no final XOR**.
    ///
    /// The reference is `WHDLoad/Src/programs/CRC16.asm` (Aminet
    /// `dev/misc/WHDLoad_dev.lha`), whose own header comment calls it
    /// "ANSI CRC16" and whose table loop is `lsr.w #1,d1` / `eor.w #$a001,d1`
    /// with `moveq #0,d0` as the initial value.
    ///
    /// `"123456789"` is the standard check vector for this parameterisation.
    #[test]
    fn crc16_matches_the_arc_check_vector() {
        assert_eq!(crc16_arc(b"123456789"), 0xBB3D);
    }

    /// The standard CRC-32 check vector, whole and split across two
    /// `update` calls — the incremental path must land on the same value as
    /// the one-shot function.
    #[test]
    fn crc32_matches_the_standard_vector_whole_and_in_pieces() {
        assert_eq!(crc32_ieee(b""), 0);
        assert_eq!(crc32_ieee(b"123456789"), 0xCBF4_3926);
        let mut split = Crc32::new();
        split.update(b"1234");
        split.update(b"56789");
        assert_eq!(split.finish(), 0xCBF4_3926);
    }

    /// The empty input is the init value, unmodified. WHDLoad's own routine
    /// returns early on a zero length without touching `d0`, which was set
    /// to 0.
    #[test]
    fn crc16_of_nothing_is_the_init_value() {
        assert_eq!(crc16_arc(b""), 0x0000);
    }

    /// Byte order matters: a reflected CRC over reversed input must differ,
    /// or the implementation is not actually reflecting.
    #[test]
    fn crc16_is_order_sensitive() {
        assert_ne!(crc16_arc(b"AB"), crc16_arc(b"BA"));
    }

    #[test]
    fn known_vector_empty() {
        // SHA256("") known value.
        assert_eq!(
            sha256_bytes(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn known_vector_abc() {
        // SHA256("abc") known value.
        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn file_matches_bytes() {
        // `ScratchDir`, not a bare `PathBuf` plus a trailing `remove_dir_all`
        // (M7, fix wave 2) — a trailing statement is skipped exactly when the
        // test panics, which is when a leak costs most (ART-184: 169,291
        // directories, ~987 GB, from this same shape).
        let d = ScratchDir::new("art-hash", "sha256-bytes");
        let p = d.join("data.bin");
        std::fs::write(&p, b"abc").unwrap();
        let from_file = sha256_file(&p).unwrap();
        assert_eq!(from_file, sha256_bytes(b"abc"));
    }

    #[test]
    fn large_file_does_not_panic() {
        // 1 MiB of data — ensures the streaming path works.
        let d = ScratchDir::new("art-hash", "sha256-big");
        let p = d.join("big.bin");
        let one_mib = vec![0x42u8; 1024 * 1024];
        std::fs::write(&p, &one_mib).unwrap();
        let from_file = sha256_file(&p).unwrap();
        assert_eq!(from_file, sha256_bytes(&one_mib));
    }

    /// RFC 1321, §A.5 ("Test suite") — the empty string, published there as
    /// `MD5 ("") = d41d8cd98f00b204e9800998ecf8427e`. Pinned from the RFC
    /// rather than derived from this module's own output, so a wrong
    /// implementation of ART's own making cannot pass by agreeing with
    /// itself.
    #[test]
    fn md5_known_vector_empty() {
        assert_eq!(md5_bytes(b""), "d41d8cd98f00b204e9800998ecf8427e");
    }

    /// RFC 1321, §A.5 — `MD5 ("abc") = 900150983cd24fb0d6963f7d28e17f72`.
    #[test]
    fn md5_known_vector_abc() {
        assert_eq!(md5_bytes(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
    }

    #[test]
    fn md5_file_matches_bytes() {
        let d = ScratchDir::new("art-hash", "md5-bytes");
        let p = d.join("data.bin");
        std::fs::write(&p, b"abc").unwrap();
        let from_file = md5_file(&p).unwrap();
        assert_eq!(from_file, md5_bytes(b"abc"));
    }

    /// Mirrors `large_file_does_not_panic` above, but with varying content
    /// (rather than one repeated byte) across more than sixteen 64 KiB
    /// chunks, so a broken chunk loop that reads only part of the file — the
    /// defect Task 1 asks to mutate in — changes the digest rather than
    /// happening to agree with the whole-file one.
    #[test]
    fn md5_large_file_streams_the_whole_content() {
        let d = ScratchDir::new("art-hash", "md5-big");
        let p = d.join("big.bin");
        let one_mib: Vec<u8> = (0..1024 * 1024).map(|i| (i % 256) as u8).collect();
        std::fs::write(&p, &one_mib).unwrap();
        let from_file = md5_file(&p).unwrap();
        assert_eq!(from_file, md5_bytes(&one_mib));
    }
}
