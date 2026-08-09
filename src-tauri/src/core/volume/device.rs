//! The two places a volume's blocks actually live.
//!
//! - [`SliceDevice`] — an image already in memory. That is every ADF: 880 KB is
//!   nothing, and the existing code already holds it as a buffer.
//! - [`FileRegion`] — a window into a file, read on demand. That is every HDF
//!   partition, and it is why ART can browse a partition two gigabytes into a
//!   file without reading the file (the ART-021 lesson: never pull a whole
//!   hard disk image into memory).
//!
//! Both present **volume-relative** block numbers, so the filesystem code above
//! them cannot tell which it is talking to.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Mutex;

use super::BlockDevice;
use crate::core::error::{CoreError, CoreResult};

/// A volume held in memory.
#[derive(Debug, Clone, Copy)]
pub struct SliceDevice<'a> {
    bytes: &'a [u8],
    block_size: usize,
}

impl<'a> SliceDevice<'a> {
    pub fn new(bytes: &'a [u8], block_size: usize) -> CoreResult<Self> {
        if block_size == 0 {
            return Err(CoreError::InvalidInput("block size cannot be zero".into()));
        }
        Ok(Self { bytes, block_size })
    }

    /// The common case: a 512-byte-block image.
    pub fn floppy(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            block_size: super::SECTOR_BYTES,
        }
    }
}

impl BlockDevice for SliceDevice<'_> {
    fn block_size(&self) -> usize {
        self.block_size
    }

    fn total_blocks(&self) -> u32 {
        // A partial trailing block is not a block. Truncated images are common
        // and counting a half one would hand the filesystem a short read.
        (self.bytes.len() / self.block_size).min(u32::MAX as usize) as u32
    }

    fn read_block(&self, n: u32, buf: &mut [u8]) -> CoreResult<()> {
        check_buffer(buf, self.block_size)?;

        let start = (n as usize)
            .checked_mul(self.block_size)
            .ok_or_else(|| out_of_range(n, self.total_blocks()))?;
        let end = start
            .checked_add(self.block_size)
            .ok_or_else(|| out_of_range(n, self.total_blocks()))?;

        if end > self.bytes.len() {
            return Err(out_of_range(n, self.total_blocks()));
        }

        buf.copy_from_slice(&self.bytes[start..end]);
        Ok(())
    }
}

/// A window into a file: a partition, or a whole bare hardfile.
///
/// The handle is shared behind a mutex because reading needs to seek, and a
/// volume is read from several places at once (a directory walk while a file
/// extracts). One handle with a lock beats reopening the file per block.
#[derive(Debug)]
pub struct FileRegion {
    file: Mutex<File>,
    /// Where this volume starts in the file.
    offset: u64,
    block_size: usize,
    total_blocks: u32,
}

impl FileRegion {
    /// Open `length` bytes of `path` starting at `offset` as a volume.
    ///
    /// A region that runs past the end of the file is **clamped and reported**,
    /// never read as garbage: RDB tables outlive the images they describe, and
    /// a partition claiming more than the file holds is a truncated download,
    /// not a reason to hand the filesystem random bytes.
    pub fn open(path: &Path, offset: u64, length: u64, block_size: usize) -> CoreResult<Self> {
        if block_size == 0 || block_size % super::SECTOR_BYTES != 0 {
            return Err(CoreError::Malformed {
                format: "volume".into(),
                detail: format!("block size {block_size} is not a multiple of 512"),
            });
        }

        let file = File::open(path)?;
        let file_len = file.metadata()?.len();

        if offset >= file_len {
            return Err(CoreError::Malformed {
                format: "volume".into(),
                detail: format!(
                    "this volume starts at byte {offset}, past the end of a {file_len}-byte file"
                ),
            });
        }

        let available = file_len - offset;
        let usable = length.min(available);
        let total_blocks = (usable / block_size as u64).min(u32::MAX as u64) as u32;

        if total_blocks == 0 {
            return Err(CoreError::Malformed {
                format: "volume".into(),
                detail: "this volume holds no whole blocks".into(),
            });
        }

        Ok(Self {
            file: Mutex::new(file),
            offset,
            block_size,
            total_blocks,
        })
    }

    /// True when the file was shorter than the region claimed.
    pub fn is_clamped(path: &Path, offset: u64, length: u64) -> CoreResult<bool> {
        let file_len = File::open(path)?.metadata()?.len();
        Ok(offset.saturating_add(length) > file_len)
    }
}

impl BlockDevice for FileRegion {
    fn block_size(&self) -> usize {
        self.block_size
    }

    fn total_blocks(&self) -> u32 {
        self.total_blocks
    }

    fn read_block(&self, n: u32, buf: &mut [u8]) -> CoreResult<()> {
        check_buffer(buf, self.block_size)?;

        if n >= self.total_blocks {
            return Err(out_of_range(n, self.total_blocks));
        }

        let position = self
            .offset
            .checked_add(
                (n as u64)
                    .checked_mul(self.block_size as u64)
                    .ok_or_else(|| out_of_range(n, self.total_blocks))?,
            )
            .ok_or_else(|| out_of_range(n, self.total_blocks))?;

        let mut file = self
            .file
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        file.seek(SeekFrom::Start(position))?;
        file.read_exact(buf)?;
        Ok(())
    }
}

/// A volume held in memory that can be written to.
///
/// The [`WriteStrategy::WholeFile`] side: the image is read in, mutated here,
/// validated, and only then written back through `core/safety`. Nothing this
/// type does reaches the user's disk — that is the point. A failed operation
/// throws the buffer away and the file was never opened for writing.
///
/// [`WriteStrategy::WholeFile`]: super::WriteStrategy::WholeFile
#[derive(Debug, Clone)]
pub struct VecDevice {
    bytes: Vec<u8>,
    block_size: usize,
}

impl VecDevice {
    pub fn new(bytes: Vec<u8>, block_size: usize) -> CoreResult<Self> {
        if block_size == 0 || block_size % super::SECTOR_BYTES != 0 {
            return Err(CoreError::InvalidInput(format!(
                "block size {block_size} is not a multiple of {}",
                super::SECTOR_BYTES
            )));
        }
        Ok(Self { bytes, block_size })
    }

    /// The image as it stands, for validation and for writing back.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Take the image back out, consuming the device.
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

impl BlockDevice for VecDevice {
    fn block_size(&self) -> usize {
        self.block_size
    }

    fn total_blocks(&self) -> u32 {
        (self.bytes.len() / self.block_size).min(u32::MAX as usize) as u32
    }

    fn read_block(&self, n: u32, buf: &mut [u8]) -> CoreResult<()> {
        check_buffer(buf, self.block_size)?;
        let (start, end) = self.span(n)?;
        buf.copy_from_slice(&self.bytes[start..end]);
        Ok(())
    }
}

impl super::BlockDeviceMut for VecDevice {
    fn write_block(&mut self, n: u32, buf: &[u8]) -> CoreResult<()> {
        check_buffer(buf, self.block_size)?;
        let (start, end) = self.span(n)?;
        self.bytes[start..end].copy_from_slice(buf);
        Ok(())
    }

    fn sync(&mut self) -> CoreResult<()> {
        // Memory is already as durable as this device gets. The write-back to
        // disk is a separate, atomic step the caller performs.
        Ok(())
    }
}

impl VecDevice {
    fn span(&self, n: u32) -> CoreResult<(usize, usize)> {
        let start = (n as usize)
            .checked_mul(self.block_size)
            .ok_or_else(|| out_of_range(n, self.total_blocks()))?;
        let end = start
            .checked_add(self.block_size)
            .ok_or_else(|| out_of_range(n, self.total_blocks()))?;
        if end > self.bytes.len() {
            return Err(out_of_range(n, self.total_blocks()));
        }
        Ok((start, end))
    }
}

/// A writable window into a file: the [`WriteStrategy::BlockJournal`] side.
///
/// Opened read-write, and it writes **in place** — the file is never
/// reallocated, moved or truncated, so its size is an invariant the journal
/// can identify it by. Everything that makes this safe lives in
/// [`journal`](super::journal); this type is deliberately dumb.
///
/// [`WriteStrategy::BlockJournal`]: super::WriteStrategy::BlockJournal
#[derive(Debug)]
pub struct FileRegionMut {
    file: File,
    offset: u64,
    block_size: usize,
    total_blocks: u32,
}

impl FileRegionMut {
    /// Open `length` bytes of `path` at `offset` for reading and writing.
    ///
    /// Unlike [`FileRegion::open`], a region running past the end of the file
    /// is **refused rather than clamped**. Clamping is right for browsing —
    /// show what is really there — and wrong for writing: a partition table
    /// that outlived its image would let a write land wherever the arithmetic
    /// happened to point.
    pub fn open(path: &Path, offset: u64, length: u64, block_size: usize) -> CoreResult<Self> {
        if block_size == 0 || block_size % super::SECTOR_BYTES != 0 {
            return Err(CoreError::Malformed {
                format: "volume".into(),
                detail: format!("block size {block_size} is not a multiple of 512"),
            });
        }

        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)?;
        let file_len = file.metadata()?.len();

        let end = offset
            .checked_add(length)
            .ok_or_else(|| CoreError::Malformed {
                format: "volume".into(),
                detail: "this volume's position overflows".into(),
            })?;
        if end > file_len {
            return Err(CoreError::Malformed {
                format: "volume".into(),
                detail: format!(
                    "this volume claims bytes {offset}..{end} of a {file_len}-byte file. \
                     ART will not write into a volume that is larger than the file holding it"
                ),
            });
        }

        let total_blocks = (length / block_size as u64).min(u32::MAX as u64) as u32;
        if total_blocks == 0 {
            return Err(CoreError::Malformed {
                format: "volume".into(),
                detail: "this volume holds no whole blocks".into(),
            });
        }

        Ok(Self {
            file,
            offset,
            block_size,
            total_blocks,
        })
    }

    fn position(&self, n: u32) -> CoreResult<u64> {
        if n >= self.total_blocks {
            return Err(out_of_range(n, self.total_blocks));
        }
        self.offset
            .checked_add(
                (n as u64)
                    .checked_mul(self.block_size as u64)
                    .ok_or_else(|| out_of_range(n, self.total_blocks))?,
            )
            .ok_or_else(|| out_of_range(n, self.total_blocks))
    }
}

impl BlockDevice for FileRegionMut {
    fn block_size(&self) -> usize {
        self.block_size
    }

    fn total_blocks(&self) -> u32 {
        self.total_blocks
    }

    fn read_block(&self, n: u32, buf: &mut [u8]) -> CoreResult<()> {
        check_buffer(buf, self.block_size)?;
        let position = self.position(n)?;
        // `&File` implements Read/Seek, so a shared borrow is enough and this
        // type needs no interior mutability.
        let mut handle = &self.file;
        handle.seek(SeekFrom::Start(position))?;
        handle.read_exact(buf)?;
        Ok(())
    }
}

impl super::BlockDeviceMut for FileRegionMut {
    fn write_block(&mut self, n: u32, buf: &[u8]) -> CoreResult<()> {
        check_buffer(buf, self.block_size)?;
        let position = self.position(n)?;
        self.file.seek(SeekFrom::Start(position))?;
        self.file.write_all(buf)?;
        Ok(())
    }

    fn sync(&mut self) -> CoreResult<()> {
        self.file.sync_all()?;
        Ok(())
    }
}

fn check_buffer(buf: &[u8], block_size: usize) -> CoreResult<()> {
    if buf.len() == block_size {
        Ok(())
    } else {
        Err(CoreError::InvalidInput(format!(
            "a block buffer must be {block_size} bytes, got {}",
            buf.len()
        )))
    }
}

fn out_of_range(n: u32, total: u32) -> CoreError {
    CoreError::Malformed {
        format: "volume".into(),
        detail: format!("block {n} is outside this volume ({total} blocks)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(blocks: usize) -> Vec<u8> {
        let mut bytes = vec![0u8; blocks * 512];
        for block in 0..blocks {
            bytes[block * 512] = block as u8;
        }
        bytes
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("art-device-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // ---- in-memory ----

    #[test]
    fn a_slice_device_reads_the_block_asked_for() {
        let bytes = image(4);
        let device = SliceDevice::floppy(&bytes);

        assert_eq!(device.total_blocks(), 4);

        let mut buf = [0u8; 512];
        device.read_block(2, &mut buf).unwrap();
        assert_eq!(buf[0], 2);
    }

    /// `panic = "abort"` means an out-of-range index would kill the
    /// application, so it has to be an error at the boundary.
    #[test]
    fn a_block_past_the_end_is_an_error_not_a_panic() {
        let bytes = image(4);
        let device = SliceDevice::floppy(&bytes);
        let mut buf = [0u8; 512];

        assert!(device.read_block(4, &mut buf).is_err());
        assert!(device.read_block(u32::MAX, &mut buf).is_err());
    }

    /// A truncated image is common. Half a block is not a block.
    #[test]
    fn a_partial_trailing_block_is_not_counted() {
        let mut bytes = image(4);
        bytes.truncate(4 * 512 - 10);
        let device = SliceDevice::floppy(&bytes);

        assert_eq!(device.total_blocks(), 3);
        let mut buf = [0u8; 512];
        assert!(device.read_block(3, &mut buf).is_err());
    }

    #[test]
    fn the_buffer_has_to_be_one_block() {
        let bytes = image(2);
        let device = SliceDevice::floppy(&bytes);

        assert!(device.read_block(0, &mut [0u8; 511]).is_err());
        assert!(device.read_block(0, &mut [0u8; 1024]).is_err());
    }

    // ---- file windows ----

    #[test]
    fn a_file_region_reads_blocks_relative_to_its_own_start() {
        let dir = scratch("region");
        let path = dir.join("disk.hdf");
        std::fs::write(&path, image(10)).unwrap();

        // A "partition" starting at block 4.
        let device = FileRegion::open(&path, 4 * 512, 3 * 512, 512).unwrap();
        assert_eq!(device.total_blocks(), 3);

        let mut buf = [0u8; 512];
        device.read_block(0, &mut buf).unwrap();
        assert_eq!(buf[0], 4, "block 0 of the volume is block 4 of the file");

        device.read_block(2, &mut buf).unwrap();
        assert_eq!(buf[0], 6);

        assert!(
            device.read_block(3, &mut buf).is_err(),
            "past the partition"
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// §5: "a partition may extend past a truncated file; clamp and report,
    /// don't read garbage".
    #[test]
    fn a_region_longer_than_its_file_is_clamped_not_invented() {
        let dir = scratch("clamp");
        let path = dir.join("short.hdf");
        std::fs::write(&path, image(6)).unwrap();

        let device = FileRegion::open(&path, 2 * 512, 100 * 512, 512).unwrap();
        assert_eq!(
            device.total_blocks(),
            4,
            "only what the file actually holds"
        );
        assert!(FileRegion::is_clamped(&path, 2 * 512, 100 * 512).unwrap());
        assert!(!FileRegion::is_clamped(&path, 2 * 512, 4 * 512).unwrap());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_region_that_starts_past_the_end_is_refused() {
        let dir = scratch("past");
        let path = dir.join("small.hdf");
        std::fs::write(&path, image(2)).unwrap();

        let err = FileRegion::open(&path, 99 * 512, 512, 512).unwrap_err();
        assert_eq!(err.code(), "ART-FORMAT-MALFORMED");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_region_with_no_whole_block_is_refused() {
        let dir = scratch("tiny");
        let path = dir.join("tiny.hdf");
        std::fs::write(&path, vec![0u8; 600]).unwrap();

        assert!(FileRegion::open(&path, 512, 88, 512).is_err());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// `sizeBlock` in an RDB is a longword count; a tool writing nonsense must
    /// not produce a device at all.
    #[test]
    fn a_nonsense_block_size_is_refused() {
        let dir = scratch("blocksize");
        let path = dir.join("d.hdf");
        std::fs::write(&path, image(4)).unwrap();

        for size in [0, 1, 300, 513] {
            assert!(
                FileRegion::open(&path, 0, 4 * 512, size).is_err(),
                "accepted block size {size}"
            );
        }

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Reading a 2 GB partition must not mean holding it. This checks the
    /// device never grows with the file it points into.
    #[test]
    fn a_file_region_does_not_hold_the_file() {
        let dir = scratch("sparse");
        let path = dir.join("big.hdf");
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(64 * 1024 * 1024).unwrap();
        drop(file);

        let device = FileRegion::open(&path, 1024 * 1024, 32 * 1024 * 1024, 512).unwrap();
        assert_eq!(device.total_blocks(), 65536);

        let mut buf = [0u8; 512];
        device.read_block(65535, &mut buf).unwrap();

        std::fs::remove_dir_all(&dir).unwrap();
    }

    // ---- writable devices ----

    #[test]
    fn a_vec_device_reads_back_what_it_wrote() {
        use crate::core::volume::BlockDeviceMut;

        let mut device = VecDevice::new(image(4), 512).unwrap();
        device.write_block(2, &[0xAB; 512]).unwrap();

        let mut buf = [0u8; 512];
        device.read_block(2, &mut buf).unwrap();
        assert_eq!(buf, [0xAB; 512]);
        // And the other blocks are untouched.
        device.read_block(1, &mut buf).unwrap();
        assert_eq!(buf[0], 1);
    }

    #[test]
    fn a_vec_device_refuses_a_block_past_the_end() {
        use crate::core::volume::BlockDeviceMut;

        let mut device = VecDevice::new(image(4), 512).unwrap();
        assert!(device.write_block(4, &[0xAB; 512]).is_err());
        assert!(device.write_block(u32::MAX, &[0xAB; 512]).is_err());
    }

    #[test]
    fn a_vec_device_refuses_a_buffer_of_the_wrong_size() {
        use crate::core::volume::BlockDeviceMut;

        let mut device = VecDevice::new(image(4), 512).unwrap();
        assert!(device.write_block(0, &[0xAB; 256]).is_err());
    }

    #[test]
    fn a_writable_region_writes_where_the_volume_starts_not_where_the_file_does() {
        use crate::core::volume::BlockDeviceMut;

        let dir = scratch("region-mut");
        let path = dir.join("disk.hdf");
        std::fs::write(&path, image(16)).unwrap();

        // A volume four blocks into the file.
        let mut device = FileRegionMut::open(&path, 4 * 512, 8 * 512, 512).unwrap();
        device.write_block(1, &[0xCD; 512]).unwrap();
        device.sync().unwrap();
        drop(device);

        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes[5 * 512], 0xCD, "volume block 1 is file block 5");
        assert_eq!(bytes[4 * 512], 4, "the block before it is untouched");
        assert_eq!(bytes[6 * 512], 6, "and so is the one after");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Browsing clamps a partition that outran its file, because showing what
    /// is really there is right. Writing must not: a table that outlived its
    /// image would let a write land wherever the arithmetic pointed.
    #[test]
    fn a_writable_region_refuses_to_open_past_the_end_rather_than_clamping() {
        let dir = scratch("region-mut-short");
        let path = dir.join("disk.hdf");
        std::fs::write(&path, image(8)).unwrap();

        // Read-only opens and clamps.
        assert!(FileRegion::open(&path, 0, 32 * 512, 512).is_ok());
        // Writable refuses.
        let err = FileRegionMut::open(&path, 0, 32 * 512, 512).unwrap_err();
        assert!(
            err.to_string().contains("larger than the file holding it"),
            "unexpected message: {err}"
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_writable_region_refuses_a_block_past_its_end() {
        use crate::core::volume::BlockDeviceMut;

        let dir = scratch("region-mut-range");
        let path = dir.join("disk.hdf");
        std::fs::write(&path, image(8)).unwrap();

        let mut device = FileRegionMut::open(&path, 0, 4 * 512, 512).unwrap();
        assert!(device.write_block(4, &[0u8; 512]).is_err());
        // The file beyond the volume is still intact.
        drop(device);
        assert_eq!(std::fs::read(&path).unwrap()[4 * 512], 4);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A floppy must never take the journal path, and a real hard disk must
    /// never take the whole-file one.
    #[test]
    fn the_write_strategy_follows_the_size_of_the_image() {
        use crate::core::volume::{WriteStrategy, WHOLE_FILE_LIMIT_BYTES};

        assert_eq!(WriteStrategy::for_image(901_120), WriteStrategy::WholeFile);
        assert_eq!(
            WriteStrategy::for_image(1_802_240),
            WriteStrategy::WholeFile
        );
        assert_eq!(
            WriteStrategy::for_image(WHOLE_FILE_LIMIT_BYTES),
            WriteStrategy::WholeFile,
            "the limit itself is still whole-file"
        );
        assert_eq!(
            WriteStrategy::for_image(WHOLE_FILE_LIMIT_BYTES + 1),
            WriteStrategy::BlockJournal
        );
        assert_eq!(
            WriteStrategy::for_image(2 * 1024 * 1024 * 1024),
            WriteStrategy::BlockJournal
        );
    }
}
