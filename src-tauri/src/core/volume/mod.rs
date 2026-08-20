//! Volumes: the abstraction that stops ART thinking every disk is a floppy.
//!
//! > *ADF was never the product. The volume driver is. ADF is just the smallest
//! > disk it mounts.*
//!
//! `core/adf` grew against floppy geometry — 512-byte blocks, 1760 of them,
//! root at 880, one bitmap block. An HDF partition is the **same filesystem at
//! different geometry**, so the fix is not a second implementation but a pair
//! of abstractions everything above them goes through:
//!
//! - [`BlockDevice`] — where the blocks are. A whole ADF in memory, or a
//!   partition slice of a multi-gigabyte HDF read on demand.
//! - [`VolumeGeometry`] — how many blocks, how big, where the root is.
//!
//! ## Reading and writing are different types
//!
//! [`BlockDevice`] cannot write; [`BlockDeviceMut`] can. Mounting for browsing
//! hands out the first, so the large majority of the code physically cannot
//! change a user's image — a half-generalised writer is how a 2 GB disk gets a
//! floppy-shaped hole in it (§57, §93).
//!
//! Mutations go one step further and never touch `BlockDeviceMut` directly:
//! they go through [`journal::Journalled`], which refuses to write a block
//! whose previous contents are not already saved and fsynced.

pub mod checkout;
pub mod device;
#[cfg(test)]
pub mod fixture;
pub mod integrity;
pub mod journal;
pub mod mount;
pub mod write;

use crate::core::error::{CoreError, CoreResult};

/// The largest volume ART will mount in this pass.
///
/// Classic FFS lives under 2 GiB per partition — the 32-bit signedness history
/// is real and the failure modes past it are not tested. Refusing is honest;
/// mounting and hoping is not. Lifting this needs large-disk tests, not
/// optimism.
pub const MAX_VOLUME_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Every AmigaDOS block size is a multiple of this.
pub const SECTOR_BYTES: usize = 512;

/// A bounded range of blocks ART can read.
///
/// Implementations must treat `n` as **volume-relative**: block 0 is the start
/// of this volume, not of the file it lives in. That is what lets the same
/// filesystem code read a floppy and a partition two gigabytes into an HDF.
pub trait BlockDevice: Send + Sync {
    fn block_size(&self) -> usize;

    /// Blocks in *this* volume.
    fn total_blocks(&self) -> u32;

    /// Read one block into `buf`, which must be exactly [`block_size`] long.
    ///
    /// A block number past the end is an error, never a short read or zeroes:
    /// a corrupt image that points off the end must be caught, not smoothed
    /// over (`panic = "abort"` means a bad index is fatal, so it is checked
    /// here rather than indexed).
    ///
    /// [`block_size`]: BlockDevice::block_size
    fn read_block(&self, n: u32, buf: &mut [u8]) -> CoreResult<()>;
}

/// Read one block into a fresh buffer.
///
/// Convenience for the many call sites that want the bytes rather than a
/// buffer to fill.
///
/// Generic rather than `&dyn BlockDevice` on purpose: passing a
/// `&dyn BlockDeviceMut` here would be trait-object upcasting, which only
/// became stable in Rust 1.86 and would break the 1.77 MSRV that CI enforces.
/// A generic parameter reaches the same call sites with no upcast at all.
pub fn read_block_vec<D: BlockDevice + ?Sized>(device: &D, n: u32) -> CoreResult<Vec<u8>> {
    let mut buf = vec![0u8; device.block_size()];
    device.read_block(n, &mut buf)?;
    Ok(buf)
}

/// A volume ART can write into.
///
/// Split from [`BlockDevice`] rather than added to it so that every read path
/// keeps a type that *cannot* write. Mounting for browsing hands out a
/// `&dyn BlockDevice`; only the code that has been given a `&mut dyn
/// BlockDeviceMut` can change anything, and the compiler says which is which.
///
/// Callers do not use this directly for mutations. Everything that changes a
/// user's image goes through [`journal::Journalled`], which refuses to write a
/// block whose previous contents it has not saved first.
pub trait BlockDeviceMut: BlockDevice {
    /// Overwrite one block. `buf` must be exactly [`BlockDevice::block_size`].
    ///
    /// A block number past the end is an error, exactly as for reads — a
    /// mutation that ran off the end of a volume would corrupt whatever came
    /// after it in the file.
    fn write_block(&mut self, n: u32, buf: &[u8]) -> CoreResult<()>;

    /// Push everything written so far to durable storage.
    ///
    /// Not optional bookkeeping: the journal protocol depends on ordering, and
    /// an unsynced write is a write that may or may not have happened.
    fn sync(&mut self) -> CoreResult<()>;
}

/// How a mutation reaches the user's file.
///
/// A 2 GB image is not a big floppy. Backing up 880 KB before every rename is
/// free; backing up two gigabytes is not, and doing it anyway is how ART would
/// turn one keystroke into minutes of disk I/O (brief §1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteStrategy {
    /// Read whole → mutate in memory → validate → back up → atomically replace.
    ///
    /// The existing, audited ADF pipeline, unchanged. Every floppy and every
    /// small hardfile takes this path, so the common case gains no new risk
    /// from Stage W at all.
    WholeFile,
    /// In-place block writes, guarded by an undo journal.
    ///
    /// Same guarantee as `WholeFile` — the original is recoverable until the
    /// operation is verified — at block granularity instead of file
    /// granularity.
    BlockJournal,
}

/// Images at or below this size use [`WriteStrategy::WholeFile`].
///
/// 16 MiB covers every ADF (880 KB), every HD floppy (1.76 MB) and the small
/// hardfiles people keep for a single application. Above it, whole-file copies
/// stop being cheap enough to do per operation.
pub const WHOLE_FILE_LIMIT_BYTES: u64 = 16 * 1024 * 1024;

impl WriteStrategy {
    /// Which strategy an image of `bytes` gets.
    pub fn for_image(bytes: u64) -> Self {
        if bytes <= WHOLE_FILE_LIMIT_BYTES {
            Self::WholeFile
        } else {
            Self::BlockJournal
        }
    }
}

/// The four-byte filesystem identifier at the start of a boot block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DosType(pub [u8; 4]);

impl DosType {
    pub const fn new(bytes: [u8; 4]) -> Self {
        Self(bytes)
    }

    /// The `DOS`/`PFS`/`SFS` prefix.
    pub fn family(&self) -> &[u8] {
        &self.0[..3]
    }

    /// The flavour byte: 0..=7 for AmigaDOS.
    pub fn flavour(&self) -> u8 {
        self.0[3]
    }

    pub fn is_dos(&self) -> bool {
        self.family() == b"DOS"
    }

    /// FFS stores file data in plain blocks; OFS gives every data block a
    /// 24-byte header. It changes extraction, not directory walking.
    pub fn is_ffs(&self) -> bool {
        self.is_dos() && self.flavour() & 0x01 != 0
    }

    /// International mode changes how names hash — and it comes from the
    /// **volume's own** dostype, never a global setting (the ART-010 lesson).
    pub fn is_international(&self) -> bool {
        self.is_dos() && matches!(self.flavour(), 2 | 3 | 6 | 7)
    }

    /// Directory cache blocks. Safe to ignore while reading; writing without
    /// maintaining them corrupts listings on real AmigaOS.
    pub fn has_dircache(&self) -> bool {
        self.is_dos() && matches!(self.flavour(), 4 | 5)
    }

    /// The long-filename filesystem. A different directory layout, not a flag.
    pub fn is_long_filename(&self) -> bool {
        self.is_dos() && matches!(self.flavour(), 6 | 7)
    }

    /// Whether ART can walk this filesystem's contents.
    pub fn is_browsable(&self) -> bool {
        self.is_dos() && self.flavour() <= 5
    }

    /// What to call it, and — when ART cannot read it — why not.
    ///
    /// §2.5's rule: the pane never says "cannot see HDF". It names the
    /// filesystem and gives the exact reason, so the user learns the disk is
    /// healthy even when ART cannot walk it.
    pub fn label(&self) -> String {
        match (&self.0[..3], self.flavour()) {
            (b"DOS", 0) => "OFS".into(),
            (b"DOS", 1) => "FFS".into(),
            (b"DOS", 2) => "OFS INTL".into(),
            (b"DOS", 3) => "FFS INTL".into(),
            (b"DOS", 4) => "OFS INTL + dircache".into(),
            (b"DOS", 5) => "FFS INTL + dircache".into(),
            (b"DOS", 6) => "OFS long filenames".into(),
            (b"DOS", 7) => "FFS long filenames".into(),
            (b"PFS", _) | (b"PDS", _) => "PFS3".into(),
            (b"SFS", _) => "SFS".into(),
            _ => format!(
                "unknown ({})",
                self.0
                    .iter()
                    .map(|b| if b.is_ascii_graphic() {
                        (*b as char).to_string()
                    } else {
                        format!("\\x{b:02x}")
                    })
                    .collect::<String>()
            ),
        }
    }

    /// Why ART cannot browse this volume, or `None` when it can.
    pub fn unsupported_reason(&self) -> Option<String> {
        if self.is_browsable() {
            return None;
        }
        Some(match (&self.0[..3], self.flavour()) {
            (b"DOS", 6) | (b"DOS", 7) => "long-filename filesystem — not supported yet".to_string(),
            (b"PFS", _) | (b"PDS", _) => "PFS3 — not supported yet".to_string(),
            (b"SFS", _) => "SFS — not supported yet".to_string(),
            _ => format!("{} — not a filesystem ART can read", self.label()),
        })
    }
}

/// Where the filesystem's furniture is, for one volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolumeGeometry {
    pub block_size: usize,
    /// Blocks in this volume.
    pub total_blocks: u32,
    /// Blocks at the start reserved for the boot block. Two, in practice.
    pub reserved: u32,
    pub root_block: u32,
    pub dos_type: DosType,
}

impl VolumeGeometry {
    /// Where the root block sits in a volume of `total_blocks`.
    ///
    /// **Verified against two independent implementations on 2026-08-09**,
    /// because the brief's formula and the references disagree:
    ///
    /// - ADFlib `adfVolCalcRootBlk`: `(lastBlock - firstBlock + 1) / 2`
    /// - amitools `BootBlock.calc_root_blk`: `num_blocks // 2`
    ///
    /// Both are `total_blocks / 2`, with the reserved count playing no part.
    /// The brief proposed `(reserved + total_blocks - 1) / 2`, which gives the
    /// same 880 for a DD floppy — and a different answer as soon as `reserved`
    /// is not 2 or the block count is odd. The references win.
    pub fn root_block_for(total_blocks: u32) -> u32 {
        total_blocks / 2
    }

    /// Build a geometry, rejecting anything a filesystem could not live in.
    ///
    /// Every field here comes from an RDB written by some tool decades ago, so
    /// none of it is trusted: a partition that claims a zero block size or more
    /// reserved blocks than it has is a corrupt table, not a volume.
    pub fn new(
        block_size: usize,
        total_blocks: u32,
        reserved: u32,
        dos_type: DosType,
    ) -> CoreResult<Self> {
        if block_size == 0 || !block_size.is_multiple_of(SECTOR_BYTES) {
            return Err(CoreError::Malformed {
                format: "volume".into(),
                detail: format!("block size {block_size} is not a multiple of {SECTOR_BYTES}"),
            });
        }
        // Boot block plus a root block is the least a volume can be.
        if total_blocks < reserved.saturating_add(2) {
            return Err(CoreError::Malformed {
                format: "volume".into(),
                detail: format!(
                    "volume claims {total_blocks} blocks with {reserved} reserved, \
                     which leaves no room for a filesystem"
                ),
            });
        }

        let bytes = (total_blocks as u64).saturating_mul(block_size as u64);
        if bytes > MAX_VOLUME_BYTES {
            return Err(CoreError::UnsupportedFormat(format!(
                "volume is {} MB; ART does not mount volumes over {} MB yet",
                bytes / (1024 * 1024),
                MAX_VOLUME_BYTES / (1024 * 1024)
            )));
        }

        let root_block = Self::root_block_for(total_blocks);
        if root_block < reserved || root_block >= total_blocks {
            return Err(CoreError::Malformed {
                format: "volume".into(),
                detail: format!("computed root block {root_block} is outside the volume"),
            });
        }

        Ok(Self {
            block_size,
            total_blocks,
            reserved,
            root_block,
            dos_type,
        })
    }

    /// The geometry of a double-density floppy — the case `core/adf` was
    /// written for, now just one volume shape among several.
    pub fn floppy_dd(dos_type: DosType) -> Self {
        Self {
            block_size: SECTOR_BYTES,
            total_blocks: 1760,
            reserved: 2,
            root_block: 880,
            dos_type,
        }
    }

    pub fn total_bytes(&self) -> u64 {
        (self.total_blocks as u64) * (self.block_size as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The number every Amiga tool agrees on, and the one the whole refactor
    /// has to keep producing.
    #[test]
    fn a_double_density_floppy_still_puts_its_root_at_880() {
        assert_eq!(VolumeGeometry::root_block_for(1760), 880);
        assert_eq!(
            VolumeGeometry::floppy_dd(DosType::new(*b"DOS\x01")).root_block,
            880
        );
    }

    #[test]
    fn a_high_density_floppy_puts_its_root_at_1760() {
        assert_eq!(VolumeGeometry::root_block_for(3520), 1760);
    }

    /// ADFlib and amitools both compute `total / 2` and ignore `reserved`. The
    /// brief proposed `(reserved + total - 1) / 2`, which agrees only by
    /// coincidence at floppy geometry — this pins the difference so nobody
    /// "fixes" it back.
    #[test]
    fn the_root_block_does_not_depend_on_the_reserved_count() {
        let two = VolumeGeometry::new(512, 1760, 2, DosType::new(*b"DOS\x01")).unwrap();
        let four = VolumeGeometry::new(512, 1760, 4, DosType::new(*b"DOS\x01")).unwrap();

        assert_eq!(two.root_block, four.root_block);
        assert_eq!(two.root_block, 880);

        // The brief's formula would have said 881 for the second one.
        assert_ne!(four.root_block, (4 + 1760 - 1) / 2);
    }

    #[test]
    fn an_odd_block_count_rounds_down() {
        assert_eq!(VolumeGeometry::root_block_for(1761), 880);
    }

    // ---- refusing what cannot be a volume ----

    #[test]
    fn a_block_size_that_is_not_a_multiple_of_512_is_refused() {
        for size in [0, 100, 513, 700] {
            assert!(
                VolumeGeometry::new(size, 1760, 2, DosType::new(*b"DOS\x01")).is_err(),
                "accepted block size {size}"
            );
        }
        // 1024-byte blocks are unusual but legal.
        assert!(VolumeGeometry::new(1024, 1760, 2, DosType::new(*b"DOS\x01")).is_ok());
    }

    #[test]
    fn a_volume_with_no_room_for_a_filesystem_is_refused() {
        for (total, reserved) in [(0, 2), (2, 2), (3, 2), (1, 0), (10, 9)] {
            assert!(
                VolumeGeometry::new(512, total, reserved, DosType::new(*b"DOS\x01")).is_err(),
                "accepted {total} blocks with {reserved} reserved"
            );
        }
    }

    /// §2.6: classic FFS past 2 GiB is untested territory, so it is refused
    /// rather than mounted hopefully.
    #[test]
    fn a_volume_over_two_gigabytes_is_refused_with_its_size() {
        let blocks = (MAX_VOLUME_BYTES / 512) as u32 + 1;
        let err = VolumeGeometry::new(512, blocks, 2, DosType::new(*b"DOS\x01")).unwrap_err();

        assert_eq!(err.code(), "ART-FORMAT-UNSUPPORTED");
        assert!(err.to_string().contains("2048 MB"), "{err}");
    }

    #[test]
    fn exactly_two_gigabytes_is_still_allowed() {
        let blocks = (MAX_VOLUME_BYTES / 512) as u32;
        assert!(VolumeGeometry::new(512, blocks, 2, DosType::new(*b"DOS\x01")).is_ok());
    }

    /// Arithmetic on numbers straight out of an RDB must not wrap.
    #[test]
    fn absurd_geometry_does_not_overflow() {
        assert!(VolumeGeometry::new(512, u32::MAX, 2, DosType::new(*b"DOS\x01")).is_err());
        assert!(VolumeGeometry::new(512, 1760, u32::MAX, DosType::new(*b"DOS\x01")).is_err());
    }

    // ---- dostype ----

    #[test]
    fn dostype_flavours_are_read_the_way_amigados_reads_them() {
        let ofs = DosType::new(*b"DOS\x00");
        let ffs = DosType::new(*b"DOS\x01");
        let ofs_intl = DosType::new(*b"DOS\x02");
        let ffs_intl = DosType::new(*b"DOS\x03");
        let ffs_dc = DosType::new(*b"DOS\x05");
        let lnfs = DosType::new(*b"DOS\x07");

        assert!(!ofs.is_ffs() && ffs.is_ffs());
        assert!(!ffs.is_international() && ffs_intl.is_international());
        assert!(ofs_intl.is_international() && !ofs_intl.is_ffs());
        assert!(ffs_dc.has_dircache() && ffs_dc.is_ffs());
        assert!(lnfs.is_long_filename());
    }

    /// Everything up to DOS\5 can be walked; the dircache blocks are simply
    /// ignored while reading.
    #[test]
    fn browsable_stops_at_dos5() {
        for flavour in 0..=5u8 {
            assert!(
                DosType::new([b'D', b'O', b'S', flavour]).is_browsable(),
                "DOS\\{flavour} should be browsable"
            );
        }
        for flavour in 6..=7u8 {
            assert!(!DosType::new([b'D', b'O', b'S', flavour]).is_browsable());
        }
        assert!(!DosType::new(*b"PFS\x03").is_browsable());
        assert!(!DosType::new(*b"SFS\x00").is_browsable());
    }

    /// §2.5: never "cannot see HDF" — always the name and the exact reason.
    #[test]
    fn an_unsupported_filesystem_says_which_one_and_why() {
        assert_eq!(DosType::new(*b"PFS\x03").label(), "PFS3");
        assert!(DosType::new(*b"PFS\x03")
            .unsupported_reason()
            .unwrap()
            .contains("PFS3"));

        assert_eq!(DosType::new(*b"SFS\x00").label(), "SFS");
        assert!(DosType::new(*b"DOS\x07")
            .unsupported_reason()
            .unwrap()
            .contains("long-filename"));

        // A readable filesystem has nothing to explain.
        assert!(DosType::new(*b"DOS\x03").unsupported_reason().is_none());
        assert_eq!(DosType::new(*b"DOS\x03").label(), "FFS INTL");
    }

    #[test]
    fn an_unrecognised_dostype_is_described_not_guessed() {
        let odd = DosType::new([0x00, 0xff, b'X', 0x01]);
        let label = odd.label();

        assert!(label.contains("unknown"), "{label}");
        assert!(odd.unsupported_reason().is_some());
    }
}
