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
