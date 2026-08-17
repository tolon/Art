//! File integrity hashing.
//!
//! SHA256 is ART's canonical integrity hash (used for duplicate detection,
//! operation verification, and snapshot metadata). MD5 is available only for
//! compatibility with historical databases — never as a security primitive.
//!
//! Streams files in chunks so large HDF images do not blow up memory.

use std::io::Read;
use std::path::Path;

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
        let d = std::env::temp_dir().join(format!(
            "art-hash-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("data.bin");
        std::fs::write(&p, b"abc").unwrap();
        let from_file = sha256_file(&p).unwrap();
        assert_eq!(from_file, sha256_bytes(b"abc"));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn large_file_does_not_panic() {
        // 1 MiB of data — ensures the streaming path works.
        let d = std::env::temp_dir().join(format!(
            "art-hash-big-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("big.bin");
        let one_mib = vec![0x42u8; 1024 * 1024];
        std::fs::write(&p, &one_mib).unwrap();
        let from_file = sha256_file(&p).unwrap();
        assert_eq!(from_file, sha256_bytes(&one_mib));
        std::fs::remove_dir_all(&d).ok();
    }
}
