//! Disk analysis and raw sector forensics (Phase 6 / Phase 7).
//!
//! Provides sector-level hex chunk reading, ASCII decoding, and signature
//! recognition for Amiga disks (ADF, HDF), archives, and ROMs.

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::core::error::CoreResult;

/// A single formatted line of hex data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HexLine {
    pub offset: u64,
    pub offset_hex: String,
    pub bytes_hex: String,
    pub ascii: String,
}

/// Information about a detected Amiga data structure signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureMatch {
    pub offset: u64,
    pub signature: String,
    pub description: String,
}

/// A slice of forensic hex data with navigation metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HexChunk {
    pub file_path: String,
    pub total_file_size: u64,
    pub offset: u64,
    pub length: usize,
    pub track: Option<u32>,
    pub sector: Option<u32>,
    pub block: Option<u32>,
    pub lines: Vec<HexLine>,
    pub signatures: Vec<SignatureMatch>,
}

/// Block size used by every Amiga floppy format.
const DD_BLOCK_SIZE: usize = 512;
/// Double-density ADF: 80 cylinders × 2 heads × 11 sectors × 512 bytes.
const DD_IMAGE_SIZE: u64 = 901_120;
/// High-density ADF: the same layout with 22 sectors per track.
const HD_IMAGE_SIZE: u64 = 1_802_240;

/// Read a chunk of binary data from a file and produce forensic hex view.
pub fn read_hex_chunk(path: &Path, offset: u64, length: usize) -> CoreResult<HexChunk> {
    let mut file = File::open(path)?;
    let total_file_size = file.metadata()?.len();

    let safe_offset = offset.min(total_file_size);
    let safe_length = length
        .min(4096)
        .min((total_file_size - safe_offset) as usize);

    file.seek(SeekFrom::Start(safe_offset))?;
    let mut buffer = vec![0u8; safe_length];
    file.read_exact(&mut buffer)?;

    // Report track/sector geometry when the file is a standard Amiga floppy
    // image. DD holds 11 sectors per track, HD holds 22.
    //
    // The two size checks used to read `901_120 || 880 * 1024`, which are the
    // same number — so the HD branch never ran and HD images were reported with
    // no geometry at all.
    let blk = (safe_offset / DD_BLOCK_SIZE as u64) as u32;
    let (track, sector, block) = match total_file_size {
        DD_IMAGE_SIZE => (Some(blk / 11), Some(blk % 11), Some(blk)),
        HD_IMAGE_SIZE => (Some(blk / 22), Some(blk % 22), Some(blk)),
        _ => (None, None, Some(blk)),
    };

    // Format hex lines (16 bytes per line)
    let mut lines = Vec::new();
    for (i, chunk) in buffer.chunks(16).enumerate() {
        let line_offset = safe_offset + (i * 16) as u64;
        let offset_hex = format!("{:08X}", line_offset);

        let mut hex_parts = Vec::new();
        let mut ascii_chars = String::new();

        for b in chunk {
            hex_parts.push(format!("{:02X}", b));
            if b.is_ascii_graphic() || *b == b' ' {
                ascii_chars.push(*b as char);
            } else {
                ascii_chars.push('·');
            }
        }

        let bytes_hex = hex_parts.join(" ");

        lines.push(HexLine {
            offset: line_offset,
            offset_hex,
            bytes_hex,
            ascii: ascii_chars,
        });
    }

    // Scan for Amiga signatures within the read chunk
    let mut signatures = Vec::new();
    scan_signatures(&buffer, safe_offset, &mut signatures);

    Ok(HexChunk {
        file_path: path.to_string_lossy().to_string(),
        total_file_size,
        offset: safe_offset,
        length: safe_length,
        track,
        sector,
        block,
        lines,
        signatures,
    })
}

fn scan_signatures(buf: &[u8], base_offset: u64, acc: &mut Vec<SignatureMatch>) {
    if buf.len() < 4 {
        return;
    }

    for i in 0..=buf.len() - 4 {
        let sig = &buf[i..i + 4];
        let off = base_offset + i as u64;

        if sig == b"DOS\0" {
            acc.push(SignatureMatch {
                offset: off,
                signature: "DOS\\0".into(),
                description: "AmigaDOS Old File System (OFS) Header".into(),
            });
        } else if sig == b"DOS\x01" {
            acc.push(SignatureMatch {
                offset: off,
                signature: "DOS\\1".into(),
                description: "AmigaDOS Fast File System (FFS) Header".into(),
            });
        } else if sig == b"DOS\x03" {
            acc.push(SignatureMatch {
                offset: off,
                signature: "DOS\\3".into(),
                description: "AmigaDOS FFS Directory Cache Header".into(),
            });
        } else if sig == b"RDSK" {
            acc.push(SignatureMatch {
                offset: off,
                signature: "RDSK".into(),
                description: "Amiga Rigid Disk Block (RDB) Drive Header".into(),
            });
        } else if sig == b"PART" {
            acc.push(SignatureMatch {
                offset: off,
                signature: "PART".into(),
                description: "Amiga RDB Partition Block".into(),
            });
        } else if sig == b"PDS\x03" || sig == b"PFS\x03" {
            acc.push(SignatureMatch {
                offset: off,
                signature: "PFS\\3".into(),
                description: "Professional File System 3 (PFS3) Volume".into(),
            });
        } else if sig == b"FORM" {
            acc.push(SignatureMatch {
                offset: off,
                signature: "FORM".into(),
                description: "Amiga IFF Interchange File Format Container".into(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_hex_chunk_with_signature() {
        let dir = std::env::temp_dir().join("art-test-hex");
        std::fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("test.adf");

        let mut data = vec![0u8; 1024];
        data[0..4].copy_from_slice(b"DOS\x03");
        data[512..516].copy_from_slice(b"RDSK");
        std::fs::write(&file_path, &data).unwrap();

        let chunk = read_hex_chunk(&file_path, 0, 1024).unwrap();
        assert_eq!(chunk.lines.len(), 64); // 1024 / 16 = 64 lines
        assert_eq!(chunk.signatures.len(), 2);
        assert_eq!(chunk.signatures[0].signature, "DOS\\3");
        assert_eq!(chunk.signatures[1].signature, "RDSK");

        std::fs::remove_dir_all(&dir).ok();
    }
}
