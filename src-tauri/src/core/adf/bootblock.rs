//! Bootblock parsing (blocks 0–1 of an ADF = 1024 bytes).
//!
//! Layout (1024 bytes across 2 standard 512-byte sectors):
//! - offset 0..4:   signature `DOS\X` (4 bytes)
//! - offset 4..8:   checksum (longword, big-endian)
//! - offset 8..12:  root block pointer (big-endian; usually 880)
//! - offset 12..1024: bootcode (68000 machine code, ignored or checked for bootable flag)

use serde::{Deserialize, Serialize};

use crate::core::error::{CoreError, CoreResult};

/// Checksum longword offset within the bootblock.
pub const CHECKSUM_OFFSET: usize = 4;

/// Default root block for a standard DD ADF.
pub const DEFAULT_ROOT_BLOCK: u32 = 880;

/// Standard Amiga bootblock size in bytes (2 sectors).
pub const BOOTBLOCK_SIZE: usize = 1024;

/// The DOS-type flags encoded in the third signature byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileSystemType {
    /// Old/Original File System — every data block carries a 24-byte header.
    Ofs,
    /// Fast File System — data blocks are raw.
    Ffs,
}

impl FileSystemType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ofs => "OFS",
            Self::Ffs => "FFS",
        }
    }

    /// True if data blocks carry the 24-byte OFS extension header.
    pub fn has_data_header(self) -> bool {
        matches!(self, Self::Ofs)
    }
}

/// Parsed bootblock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootBlock {
    /// Raw 4-byte signature (e.g. `[0x44, 0x4F, 0x53, 0x00]` = `DOS\0`).
    pub signature: [u8; 4],
    pub fs_type: FileSystemType,
    /// International filenames mode (bit 1 of the type byte).
    pub international: bool,
    /// Directory cache mode (bit 2 of the type byte).
    pub dir_cache: bool,
    /// Root block pointer (usually 880).
    pub root_block: u32,
    /// Whether the bootblock checksum validates.
    pub checksum_valid: bool,
    /// Whether the image looks bootable (non-trivial bootcode present).
    pub bootable: bool,
}

impl BootBlock {
    /// Parse a bootblock from the first 1024 bytes (or at least 512 bytes) of an ADF image.
    pub fn parse(bootblock_bytes: &[u8]) -> CoreResult<Self> {
        if bootblock_bytes.len() < 512 {
            return Err(CoreError::Malformed {
                format: "adf".into(),
                detail: "image smaller than one block".into(),
            });
        }

        let signature = [
            bootblock_bytes[0],
            bootblock_bytes[1],
            bootblock_bytes[2],
            bootblock_bytes[3],
        ];
        if &signature[0..3] != b"DOS" {
            return Err(CoreError::UnsupportedFormat(format!(
                "not an AmigaDOS ADF (signature {:?})",
                signature
            )));
        }

        let type_byte = signature[3];
        let is_ffs = (type_byte & 0x01) != 0;
        let international = (type_byte & 0x02) != 0;
        let dir_cache = (type_byte & 0x04) != 0;
        let fs_type = if is_ffs {
            FileSystemType::Ffs
        } else {
            FileSystemType::Ofs
        };

        let root_block = u32::from_be_bytes([
            bootblock_bytes[8],
            bootblock_bytes[9],
            bootblock_bytes[10],
            bootblock_bytes[11],
        ]);
        // Some images leave root at 0; default to 880.
        let root_block = if root_block == 0 {
            DEFAULT_ROOT_BLOCK
        } else {
            root_block
        };

        let checksum_valid = Self::verify_checksum(bootblock_bytes);

        // Bootable heuristic: any non-zero byte beyond the header (offset 12+).
        let check_len = bootblock_bytes.len().min(BOOTBLOCK_SIZE);
        let bootable = bootblock_bytes[12..check_len].iter().any(|&b| b != 0);

        Ok(Self {
            signature,
            fs_type,
            international,
            dir_cache,
            root_block,
            checksum_valid,
            bootable,
        })
    }

    /// Compute the 32-bit AmigaDOS bootblock checksum with end-around carry.
    /// Evaluates all longwords across 1024 bytes (or available length), treating offset 4..8 as 0.
    pub fn compute_checksum(bootblock_bytes: &[u8]) -> u32 {
        let len = bootblock_bytes.len().min(BOOTBLOCK_SIZE);
        let mut sum: u32 = 0;
        for i in (0..len).step_by(4) {
            let lw = if i == CHECKSUM_OFFSET {
                0
            } else if i + 4 <= bootblock_bytes.len() {
                u32::from_be_bytes([
                    bootblock_bytes[i],
                    bootblock_bytes[i + 1],
                    bootblock_bytes[i + 2],
                    bootblock_bytes[i + 3],
                ])
            } else {
                0
            };
            let (new, carry) = sum.overflowing_add(lw);
            sum = new.wrapping_add(carry as u32);
        }
        !sum
    }

    /// Verify the bootblock checksum against stored value at offset 4..8.
    pub fn verify_checksum(bootblock_bytes: &[u8]) -> bool {
        if bootblock_bytes.len() < 512 {
            return false;
        }
        let stored = u32::from_be_bytes([
            bootblock_bytes[4],
            bootblock_bytes[5],
            bootblock_bytes[6],
            bootblock_bytes[7],
        ]);
        Self::compute_checksum(bootblock_bytes) == stored
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_dos_signature() {
        let mut block = vec![0u8; 1024];
        block[0..4].copy_from_slice(b"XXXX");
        let err = BootBlock::parse(&block).unwrap_err();
        assert!(matches!(err, CoreError::UnsupportedFormat(_)));
    }

    #[test]
    fn parses_ofs_bootblock() {
        let mut block = vec![0u8; 1024];
        block[0..4].copy_from_slice(b"DOS\0"); // OFS
        let bb = BootBlock::parse(&block).unwrap();
        assert_eq!(bb.fs_type, FileSystemType::Ofs);
        assert!(!bb.international);
        assert!(!bb.dir_cache);
        assert_eq!(bb.root_block, DEFAULT_ROOT_BLOCK);
        assert!(!bb.bootable);
    }

    #[test]
    fn parses_ffs_intl_dircache() {
        let mut block = vec![0u8; 1024];
        block[0..4].copy_from_slice(b"DOS\x07"); // 0x07 = FFS|intl|dircache
        let bb = BootBlock::parse(&block).unwrap();
        assert_eq!(bb.fs_type, FileSystemType::Ffs);
        assert!(bb.international);
        assert!(bb.dir_cache);
    }

    #[test]
    fn detects_bootable() {
        let mut block = vec![0u8; 1024];
        block[0..4].copy_from_slice(b"DOS\0");
        block[100] = 0x4E; // some bootcode byte
        let bb = BootBlock::parse(&block).unwrap();
        assert!(bb.bootable);
    }

    #[test]
    fn root_block_defaults_when_zero() {
        let mut block = vec![0u8; 1024];
        block[0..4].copy_from_slice(b"DOS\0");
        let bb = BootBlock::parse(&block).unwrap();
        assert_eq!(bb.root_block, DEFAULT_ROOT_BLOCK);
    }

    #[test]
    fn checksum_validates_after_write() {
        let mut block = vec![0u8; 1024];
        block[0..4].copy_from_slice(b"DOS\0");
        // Put some arbitrary bootcode across block 0 and block 1
        block[100] = 0x12;
        block[600] = 0x34;
        let cks = BootBlock::compute_checksum(&block);
        block[4..8].copy_from_slice(&cks.to_be_bytes());
        let bb = BootBlock::parse(&block).unwrap();
        assert!(bb.checksum_valid);
    }
}
