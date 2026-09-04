//! Writing a **dynamic** VHD: a card image that costs what it holds.
//!
//! Work-list item 7's other half, and the premise was measured before any of
//! this was written rather than taken from the item.
//!
//! # What a 32 GiB card costs today, measured
//!
//! `core::card::build`'s own doc comment said the image was *"created sparse
//! where the filesystem underneath allows it — `set_len` on NTFS costs nothing
//! and takes no space"*. **That is false, and it was checked rather than
//! argued about.** On the owner's own D: drive, 2026-08-24:
//!
//! ```text
//! SetLength(2 GB) on NTFS
//!   logical length : 2 147 483 648
//!   free consumed  : 2 147 483 648
//!   sparse flag    : This file is NOT set as sparse
//! ```
//!
//! Exactly the length, to the byte. So a 32 GiB card really does cost 32 GiB,
//! and the comment that said otherwise was the confident-wrong sentence this
//! project keeps paying for — sitting in the file somebody would read *first*
//! when trying to fix this. It is corrected in the same commit as this module.
//!
//! # Why a dynamic VHD rather than a sparse file
//!
//! Marking a file sparse is `FSCTL_SET_SPARSE`, a Windows API call — and
//! `core/` may not make one (CLAUDE.md's core-independence rule), so it would
//! need a trait and an implementation outside. A dynamic VHD needs neither: it
//! is a **portable, self-describing** format that hst-imager, WinUAE, Hyper-V
//! and qemu all read, so the saving survives the file being copied to another
//! machine or another filesystem. Sparseness on NTFS does not.
//!
//! # The format, checked rather than recalled
//!
//! Read 2026-08-24 from libyal's `libvhdi` documentation, the same source
//! [`super`]'s footer parsing was written from:
//!
//! ```text
//! 0                     footer copy            512 bytes
//! 512                   dynamic disk header   1024 bytes  (cookie `cxsparse`)
//! 1536                  block allocation table
//! ...                   blocks, each: sector bitmap then data
//! end - 512             footer
//! ```
//!
//! - Dynamic disk header: cookie `[0..8]`, next offset `[8..16]`, **BAT offset
//!   `[16..24]`**, format version `[24..28]`, **block count `[28..32]`**,
//!   **block size `[32..36]`**, checksum `[36..40]`, parent identifier
//!   `[40..56]`, parent timestamp `[56..60]`, reserved `[60..64]`, parent name
//!   `[64..576]`, parent locators `[576..768]`, reserved `[768..1024]`.
//! - BAT entries are **32-bit sector offsets**, `0xFFFF_FFFF` for a block that
//!   has never been written.
//! - Each block begins with a **sector bitmap** of `block_size / (512 * 8)`
//!   bytes rounded up to a sector, then the data. At the 2 MiB block size used
//!   here that is exactly one 512-byte sector.
//!
//! # Deterministic on purpose
//!
//! No clock and no randomness: the timestamp is zero and the identifier is
//! derived from the disk's own geometry. Building the same card twice produces
//! the same bytes, which is the same reason `Cargo.lock` is tracked. It also
//! keeps this module inside the core rule — nothing here reads a clock, so
//! nothing here needs a trait to be testable.

use std::io::{Read, Seek, SeekFrom, Write};

use super::{checksum, FOOTER_COOKIE, FOOTER_LEN, NO_DATA_OFFSET};
use crate::core::error::{CoreError, CoreResult};

/// One sector, everywhere in this format.
pub const SECTOR: u64 = 512;

/// The dynamic disk header is always this long.
pub const HEADER_LEN: usize = 1024;

/// 2 MiB — what every VHD in the wild uses, and what makes the sector bitmap
/// exactly one sector (`2 097 152 / (512 * 8) = 512`).
pub const DEFAULT_BLOCK_SIZE: u32 = 2 * 1024 * 1024;

/// A BAT entry that has never been written.
const UNALLOCATED: u32 = u32::MAX;

/// Where the footer copy ends and the header begins.
const HEADER_OFFSET: u64 = FOOTER_LEN as u64;
/// Where the BAT begins: after the footer copy and the header.
const BAT_OFFSET: u64 = HEADER_OFFSET + HEADER_LEN as u64;

/// A dynamic VHD being written, seen by the caller as a plain disk.
///
/// Implements `Read + Write + Seek` over the *virtual* disk, so anything that
/// writes to a file can write to one of these instead — including
/// [`crate::core::fat32::create_boot_partition`], which is already generic
/// over exactly that.
///
/// **Blocks are allocated on first write and never freed.** Reading somewhere
/// nothing has been written gives zeros, which is what an unwritten disk holds
/// anyway; writing zeros to an unallocated block still allocates it, because
/// distinguishing "wrote zeros" from "never wrote" would need the sector
/// bitmap to be maintained per sector and buys nothing ART uses.
pub struct DynamicVhd<F> {
    file: F,
    disk_size: u64,
    block_size: u32,
    /// Sector offsets, one per block, [`UNALLOCATED`] until first written.
    bat: Vec<u32>,
    /// The sector the footer currently sits at — and where the next block
    /// goes, since a new block is written *over* the footer and a fresh
    /// footer appended after it.
    footer_sector: u64,
    /// The caller's position on the virtual disk.
    position: u64,
}

impl<F: Read + Seek> DynamicVhd<F> {
    /// Open a dynamic VHD somebody has already written — ART's own, or one a
    /// distribution shipped.
    ///
    /// **This exists because `build_card` reads back what it wrote.** A card
    /// ART cannot re-open is a card ART cannot verify, and §92's pipeline ends
    /// in VERIFY for a reason; writing a format ART could only write would have
    /// been the half-finished shape §89 forbids.
    ///
    /// Refuses a *fixed* image by name rather than reading it wrongly: a fixed
    /// VHD has no header, no table and no blocks, so there is nothing here to
    /// do with one — open it as a plain file, which is what it is.
    pub fn open(mut file: F) -> CoreResult<Self> {
        let malformed = |detail: &str| CoreError::Malformed {
            format: "VHD".into(),
            detail: detail.to_string(),
        };

        let mut footer = vec![0u8; FOOTER_LEN];
        file.rewind()?;
        file.read_exact(&mut footer)
            .map_err(|_| malformed("too short to hold a footer"))?;
        let parsed =
            super::parse_footer(&footer).ok_or_else(|| malformed("no footer at offset 0"))?;
        if parsed.kind != super::VhdKind::Dynamic {
            return Err(malformed(&format!(
                "this is a {:?} VHD; only a dynamic one has a block table to read",
                parsed.kind
            )));
        }

        let mut header = vec![0u8; HEADER_LEN];
        file.seek(SeekFrom::Start(parsed.data_offset))?;
        file.read_exact(&mut header)
            .map_err(|_| malformed("the dynamic disk header is not there"))?;
        if header[0..8] != super::DYNAMIC_HEADER_COOKIE {
            return Err(malformed("the dynamic disk header has the wrong signature"));
        }

        let bat_offset = u64::from_be_bytes(header[16..24].try_into().expect("8 bytes"));
        let block_count = u32::from_be_bytes(header[28..32].try_into().expect("4 bytes"));
        let block_size = u32::from_be_bytes(header[32..36].try_into().expect("4 bytes"));
        if block_size == 0 || !(block_size as u64).is_multiple_of(SECTOR) {
            return Err(malformed(&format!(
                "a block size of {block_size} is not usable"
            )));
        }
        // A bound, not a guess: the table's length comes from a field in the
        // file, which is exactly the shape CLAUDE.md says never to allocate
        // from unchecked. A disk cannot have more blocks than it has sectors.
        if u64::from(block_count) > parsed.disk_size.div_ceil(SECTOR).max(1) {
            return Err(malformed(&format!(
                "the header claims {block_count} blocks, more than the disk has sectors"
            )));
        }

        let mut raw = vec![0u8; block_count as usize * 4];
        file.seek(SeekFrom::Start(bat_offset))?;
        file.read_exact(&mut raw)
            .map_err(|_| malformed("the block table is shorter than the header says"))?;
        let bat: Vec<u32> = raw
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_be_bytes(*c))
            .collect();

        Ok(Self {
            file,
            disk_size: parsed.disk_size,
            block_size,
            bat,
            // Only meaningful while writing; an opened image is not grown.
            footer_sector: 0,
            position: 0,
        })
    }
}

impl<F> DynamicVhd<F> {
    /// The size of the disk this image represents — not the size of the file.
    pub fn disk_size(&self) -> u64 {
        self.disk_size
    }

    /// How many blocks have actually been written. **The whole point of the
    /// format**, and what a test asserts against a raw image's size.
    pub fn allocated_blocks(&self) -> usize {
        self.bat
            .iter()
            .filter(|entry| **entry != UNALLOCATED)
            .count()
    }

    /// How many bytes the file occupies right now.
    pub fn file_size(&self) -> u64 {
        (self.footer_sector + 1) * SECTOR
    }

    /// Bytes of sector bitmap in front of each block's data.
    fn bitmap_bytes(&self) -> u64 {
        let bits = u64::from(self.block_size) / SECTOR;
        bits.div_ceil(8).div_ceil(SECTOR) * SECTOR
    }

    /// Where block `index` lives, or `None` when it has never been written.
    fn block_at(&self, index: usize) -> Option<u64> {
        match self.bat.get(index) {
            Some(&UNALLOCATED) | None => None,
            Some(&entry) => Some(u64::from(entry) * SECTOR + self.bitmap_bytes()),
        }
    }
}

impl<F: Read + Write + Seek> DynamicVhd<F> {
    /// Start a dynamic VHD of `disk_size` bytes at the head of `file`.
    ///
    /// The file is written immediately with a complete, valid, entirely empty
    /// image: footer copy, header, an all-`0xFF` BAT and the footer. That
    /// costs `1 536 + BAT + 512` bytes — **17 KB for a 32 GiB card** — and it
    /// means an interrupted build leaves a readable image rather than a
    /// prefix of one.
    pub fn create(file: F, disk_size: u64) -> CoreResult<Self> {
        Self::create_with_block_size(file, disk_size, DEFAULT_BLOCK_SIZE)
    }

    pub fn create_with_block_size(file: F, disk_size: u64, block_size: u32) -> CoreResult<Self> {
        if disk_size == 0 || !disk_size.is_multiple_of(SECTOR) {
            return Err(CoreError::InvalidInput(format!(
                "a VHD's size must be a non-zero multiple of {SECTOR} bytes, not {disk_size}"
            )));
        }
        if block_size == 0
            || !(block_size as u64).is_multiple_of(SECTOR)
            || !block_size.is_power_of_two()
        {
            return Err(CoreError::InvalidInput(format!(
                "a VHD's block size must be a power of two and a multiple of {SECTOR}, not \
                 {block_size}"
            )));
        }

        let blocks = disk_size.div_ceil(u64::from(block_size));
        let block_count = u32::try_from(blocks).map_err(|_| {
            CoreError::InvalidInput(format!(
                "a {disk_size}-byte disk needs {blocks} blocks, more than a VHD's table can hold"
            ))
        })?;

        // The BAT is padded to a whole sector: every offset in this format is
        // a sector number, so a block starting mid-sector is not expressible.
        let bat_bytes = u64::from(block_count) * 4;
        let bat_sectors = bat_bytes.div_ceil(SECTOR);

        let mut vhd = Self {
            file,
            disk_size,
            block_size,
            bat: vec![UNALLOCATED; block_count as usize],
            footer_sector: BAT_OFFSET / SECTOR + bat_sectors,
            position: 0,
        };

        let footer = vhd.footer_bytes();
        vhd.file.rewind()?;
        vhd.file.write_all(&footer)?;
        vhd.file.write_all(&vhd.header_bytes(bat_sectors))?;
        vhd.write_bat()?;
        vhd.write_footer()?;
        Ok(vhd)
    }

    /// Write the table and the footer out and hand the file back.
    ///
    /// Both are rewritten on every allocation as well, so a build that is
    /// killed still leaves a valid image; this is the ordinary close, and it
    /// is where the caller gets its file back to `sync_all`.
    pub fn finish(mut self) -> CoreResult<F> {
        self.write_bat()?;
        self.write_footer()?;
        self.file.flush()?;
        Ok(self.file)
    }

    // -- the two structures -------------------------------------------------

    fn footer_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![0u8; FOOTER_LEN];
        bytes[0..8].copy_from_slice(&FOOTER_COOKIE);
        // Features: bit 1 is the "reserved" bit every writer sets.
        bytes[8..12].copy_from_slice(&0x0000_0002u32.to_be_bytes());
        bytes[12..16].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        // Data offset: where the dynamic disk header is — the field that
        // makes this a dynamic image rather than a fixed one.
        bytes[16..24].copy_from_slice(&HEADER_OFFSET.to_be_bytes());
        // Timestamp: zero, meaning 2000-01-01T00:00:00Z. Deliberate — see the
        // module doc on determinism.
        bytes[24..28].copy_from_slice(&0u32.to_be_bytes());
        bytes[28..32].copy_from_slice(b"art ");
        bytes[32..36].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        // Creator host OS: "Wi2k", the value Windows writers use.
        bytes[36..40].copy_from_slice(b"Wi2k");
        bytes[40..48].copy_from_slice(&self.disk_size.to_be_bytes());
        bytes[48..56].copy_from_slice(&self.disk_size.to_be_bytes());
        bytes[56..60].copy_from_slice(&geometry(self.disk_size / SECTOR));
        bytes[60..64].copy_from_slice(&3u32.to_be_bytes()); // dynamic
        bytes[68..84].copy_from_slice(&self.identifier());
        let sum = checksum(&bytes);
        bytes[64..68].copy_from_slice(&sum.to_be_bytes());
        bytes
    }

    fn header_bytes(&self, bat_sectors: u64) -> Vec<u8> {
        let _ = bat_sectors; // the BAT's own length is implied by the count
        let mut bytes = vec![0u8; HEADER_LEN];
        bytes[0..8].copy_from_slice(&super::DYNAMIC_HEADER_COOKIE);
        // Next offset: nothing follows this header.
        bytes[8..16].copy_from_slice(&NO_DATA_OFFSET.to_be_bytes());
        bytes[16..24].copy_from_slice(&BAT_OFFSET.to_be_bytes());
        bytes[24..28].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        bytes[28..32].copy_from_slice(&(self.bat.len() as u32).to_be_bytes());
        bytes[32..36].copy_from_slice(&self.block_size.to_be_bytes());
        // Parent identifier, timestamp, name and locators stay zero: this is
        // not a differencing image and has no parent.
        let sum = checksum_over(&bytes, 36..40);
        bytes[36..40].copy_from_slice(&sum.to_be_bytes());
        bytes
    }

    /// A stable identifier derived from the disk's own shape.
    ///
    /// Not random, and not a clock: two ART cards of the same size share one,
    /// which matters only for differencing images — which ART does not write —
    /// and buys reproducible builds, which it does want. Said out loud rather
    /// than left for somebody to discover.
    fn identifier(&self) -> [u8; 16] {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(b"amiga-retro-toolkit/vhd");
        hasher.update(self.disk_size.to_be_bytes());
        hasher.update(self.block_size.to_be_bytes());
        let digest = hasher.finalize();
        let mut id = [0u8; 16];
        id.copy_from_slice(&digest[0..16]);
        id
    }

    // -- placing them -------------------------------------------------------

    fn write_bat(&mut self) -> CoreResult<()> {
        let mut bytes = Vec::with_capacity(self.bat.len() * 4);
        for entry in &self.bat {
            bytes.extend_from_slice(&entry.to_be_bytes());
        }
        // Pad to a whole sector with `0xFF`, which is what an unallocated
        // entry looks like — so a reader that trusts the padding as entries
        // still sees "nothing there".
        while !bytes.len().is_multiple_of(SECTOR as usize) {
            bytes.push(0xFF);
        }
        self.file.seek(SeekFrom::Start(BAT_OFFSET))?;
        self.file.write_all(&bytes)?;
        Ok(())
    }

    fn write_footer(&mut self) -> CoreResult<()> {
        let footer = self.footer_bytes();
        self.file
            .seek(SeekFrom::Start(self.footer_sector * SECTOR))?;
        self.file.write_all(&footer)?;
        Ok(())
    }

    /// Make sure block `index` exists, and answer where its **data** starts.
    fn ensure_block(&mut self, index: usize) -> CoreResult<u64> {
        let bitmap = self.bitmap_bytes();
        if self.bat[index] != UNALLOCATED {
            return Ok(u64::from(self.bat[index]) * SECTOR + bitmap);
        }

        // The new block goes where the footer is, and a fresh footer is
        // appended after it — the standard way a dynamic image grows.
        let at = self.footer_sector;
        let entry = u32::try_from(at).map_err(|_| {
            CoreError::InvalidInput(
                "this VHD has grown past what its 32-bit block table can address".into(),
            )
        })?;

        self.file.seek(SeekFrom::Start(at * SECTOR))?;
        // Every sector present: ART writes whole blocks' worth of zeros here
        // and then fills them in, so claiming anything less would be a lie the
        // reader could act on.
        self.file.write_all(&vec![0xFFu8; bitmap as usize])?;
        self.file.write_all(&vec![0u8; self.block_size as usize])?;

        self.bat[index] = entry;
        self.footer_sector = at + bitmap / SECTOR + u64::from(self.block_size) / SECTOR;
        // Both structures go out now rather than at `finish`, so an
        // interrupted build leaves a readable image rather than a prefix.
        self.write_bat()?;
        self.write_footer()?;
        Ok(at * SECTOR + bitmap)
    }
}

/// The CHS geometry field: cylinders (2 bytes), heads (1), sectors per track
/// (1), all big-endian.
///
/// The algorithm is the VHD specification's own, and it is here rather than
/// left zero because Hyper-V's `Get-VHD` reads it — which is also how it is
/// checked (`scripts/vhd-oracle-check.py`): ART's arithmetic is compared with
/// Microsoft's own implementation rather than with itself.
fn geometry(total_sectors: u64) -> [u8; 4] {
    // The format cannot express more than this, and every writer clamps.
    let total = total_sectors.min(65_535 * 16 * 255);

    let (mut sectors_per_track, mut heads, mut cylinder_times_heads);
    if total >= 65_535 * 16 * 63 {
        sectors_per_track = 255u64;
        heads = 16u64;
        cylinder_times_heads = total / sectors_per_track;
    } else {
        sectors_per_track = 17;
        cylinder_times_heads = total / sectors_per_track;
        heads = cylinder_times_heads.div_ceil(1024);
        if heads < 4 {
            heads = 4;
        }
        if cylinder_times_heads >= heads * 1024 || heads > 16 {
            sectors_per_track = 31;
            heads = 16;
            cylinder_times_heads = total / sectors_per_track;
        }
        if cylinder_times_heads >= heads * 1024 {
            sectors_per_track = 63;
            heads = 16;
            cylinder_times_heads = total / sectors_per_track;
        }
    }
    let cylinders = (cylinder_times_heads / heads) as u16;
    let mut out = [0u8; 4];
    out[0..2].copy_from_slice(&cylinders.to_be_bytes());
    out[2] = heads as u8;
    out[3] = sectors_per_track as u8;
    out
}

/// The same one's-complement sum [`checksum`] computes, over a structure whose
/// checksum field is somewhere else.
fn checksum_over(bytes: &[u8], field: std::ops::Range<usize>) -> u32 {
    let mut sum: u32 = 0;
    for (i, byte) in bytes.iter().enumerate() {
        if field.contains(&i) {
            continue;
        }
        sum = sum.wrapping_add(u32::from(*byte));
    }
    !sum
}

impl<F: Read + Seek> Read for DynamicVhd<F> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.position >= self.disk_size || buf.is_empty() {
            return Ok(0);
        }
        let block_size = u64::from(self.block_size);
        let index = (self.position / block_size) as usize;
        let within = self.position % block_size;
        // One block at a time: a caller reading across a boundary gets a short
        // read, which `Read` permits and `read_exact` handles.
        let want = buf
            .len()
            .min((block_size - within) as usize)
            .min((self.disk_size - self.position) as usize);

        match self.block_at(index) {
            None => buf[..want].fill(0),
            Some(at) => {
                self.file.seek(SeekFrom::Start(at + within))?;
                self.file.read_exact(&mut buf[..want])?;
            }
        }
        self.position += want as u64;
        Ok(want)
    }
}

impl<F: Read + Write + Seek> Write for DynamicVhd<F> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.position >= self.disk_size {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "past the end of the virtual disk",
            ));
        }
        let block_size = u64::from(self.block_size);
        let index = (self.position / block_size) as usize;
        let within = self.position % block_size;
        let want = buf
            .len()
            .min((block_size - within) as usize)
            .min((self.disk_size - self.position) as usize);

        let at = self
            .ensure_block(index)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        self.file.seek(SeekFrom::Start(at + within))?;
        self.file.write_all(&buf[..want])?;
        self.position += want as u64;
        Ok(want)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

impl<F> Seek for DynamicVhd<F> {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        let next = match to {
            SeekFrom::Start(at) => at as i128,
            SeekFrom::Current(by) => self.position as i128 + i128::from(by),
            SeekFrom::End(by) => self.disk_size as i128 + i128::from(by),
        };
        if next < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "seek before the start of the disk",
            ));
        }
        // Seeking past the end is allowed, exactly as it is on a file; it is
        // *writing* there that is refused.
        self.position = next as u64;
        Ok(self.position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::vhd::{parse_footer, VhdKind};
    use std::io::Cursor;

    fn new_vhd(disk_size: u64) -> DynamicVhd<Cursor<Vec<u8>>> {
        DynamicVhd::create(Cursor::new(Vec::new()), disk_size).expect("a VHD")
    }

    /// **Write one for something that is not ART to read.**
    ///
    /// The house rule (`core::card::build`'s own: *"a card is verified by
    /// something that is not ART"*). `scripts/vhd-oracle-check.py` runs this
    /// and then reads the file with Microsoft's own `Get-VHD`, which is the
    /// only way to find out whether the geometry arithmetic, the checksums and
    /// the table are right rather than merely self-consistent.
    ///
    /// Permanent and `#[ignore]`d: it writes a file, so it does not belong in
    /// a suite that runs on every commit.
    #[test]
    #[ignore = "writes a file for the oracle; set ART_VHD_OUT"]
    fn write_a_vhd_for_the_oracle() {
        let Ok(out) = std::env::var("ART_VHD_OUT") else {
            return;
        };
        let size: u64 = std::env::var("ART_VHD_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(32 * 1024 * 1024 * 1024);

        let file = std::fs::File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&out)
            .expect("create");
        let mut vhd = DynamicVhd::create(file, size).expect("a VHD");

        // Something at the start, something in the middle, something at the
        // very last sector — so the table has holes in it and the last block
        // is allocated, which is where an off-by-one in the arithmetic shows.
        vhd.write_all(b"ART wrote this").unwrap();
        vhd.seek(SeekFrom::Start(size / 2)).unwrap();
        vhd.write_all(b"and this").unwrap();
        vhd.seek(SeekFrom::Start(size - SECTOR)).unwrap();
        vhd.write_all(&[0x5A; 512]).unwrap();

        let blocks = vhd.allocated_blocks();
        let file_size = vhd.file_size();
        vhd.finish().unwrap().sync_all().unwrap();
        println!("wrote {out}: disk={size} file={file_size} blocks={blocks}");
    }

    #[test]
    fn an_empty_image_is_kilobytes_rather_than_gigabytes() {
        let vhd = new_vhd(32 * 1024 * 1024 * 1024);
        // **The whole point.** 32 GiB of card, and the file is the two
        // structures plus a 64 KB table.
        assert!(
            vhd.file_size() < 128 * 1024,
            "an empty 32 GiB image is {} bytes",
            vhd.file_size()
        );
        assert_eq!(vhd.allocated_blocks(), 0);
        assert_eq!(vhd.disk_size(), 32 * 1024 * 1024 * 1024);
    }

    #[test]
    fn what_is_written_reads_back() {
        let mut vhd = new_vhd(8 * 1024 * 1024);
        vhd.seek(SeekFrom::Start(1_000_000)).unwrap();
        vhd.write_all(b"RDSK and then some").unwrap();

        vhd.seek(SeekFrom::Start(1_000_000)).unwrap();
        let mut read = [0u8; 18];
        vhd.read_exact(&mut read).unwrap();
        assert_eq!(&read, b"RDSK and then some");
    }

    #[test]
    fn a_write_that_crosses_a_block_boundary_lands_whole() {
        let mut vhd = DynamicVhd::create_with_block_size(Cursor::new(Vec::new()), 4096 * 4, 4096)
            .expect("a VHD");
        let payload: Vec<u8> = (0..8192u32).map(|i| (i % 251) as u8).collect();
        vhd.seek(SeekFrom::Start(2048)).unwrap();
        vhd.write_all(&payload).unwrap();

        vhd.seek(SeekFrom::Start(2048)).unwrap();
        let mut read = vec![0u8; payload.len()];
        vhd.read_exact(&mut read).unwrap();
        assert_eq!(read, payload, "three blocks, written through one call");
        assert_eq!(vhd.allocated_blocks(), 3);
    }

    /// Somewhere nothing was written reads as zeros, which is what an
    /// unwritten disk holds — and costs nothing on disk.
    #[test]
    fn an_untouched_region_reads_as_zeros_without_being_allocated() {
        let mut vhd = new_vhd(8 * 1024 * 1024);
        vhd.write_all(b"first block").unwrap();
        let before = vhd.file_size();

        vhd.seek(SeekFrom::Start(6 * 1024 * 1024)).unwrap();
        let mut read = [0xAAu8; 64];
        vhd.read_exact(&mut read).unwrap();
        assert_eq!(read, [0u8; 64]);
        assert_eq!(vhd.file_size(), before, "reading must not allocate");
        assert_eq!(vhd.allocated_blocks(), 1);
    }

    /// Writing the same block twice must not allocate it twice — that would
    /// leak a block per write and grow the file without bound.
    #[test]
    fn a_block_is_allocated_once_however_often_it_is_written() {
        let mut vhd = new_vhd(8 * 1024 * 1024);
        for i in 0..16u64 {
            vhd.seek(SeekFrom::Start(i * 64)).unwrap();
            vhd.write_all(b"x").unwrap();
        }
        assert_eq!(vhd.allocated_blocks(), 1);
    }

    /// **The second write to a block has to land where the first one did.**
    ///
    /// Found by mutation: dropping the bitmap offset from the
    /// *already-allocated* branch of `ensure_block` broke nothing, because
    /// every test wrote each block exactly once and reads go through a
    /// different function. A second write then landed **on the sector bitmap**
    /// — silently, and the read came back from the right place holding the old
    /// bytes.
    #[test]
    fn writing_a_block_twice_lands_in_the_same_place_both_times() {
        let mut vhd = new_vhd(8 * 1024 * 1024);
        vhd.write_all(b"first").unwrap();

        vhd.seek(SeekFrom::Start(1024)).unwrap();
        vhd.write_all(b"second, same block").unwrap();

        vhd.seek(SeekFrom::Start(1024)).unwrap();
        let mut read = [0u8; 18];
        vhd.read_exact(&mut read).unwrap();
        assert_eq!(&read, b"second, same block");

        // And the bitmap is intact: every sector still claimed present.
        let bytes = vhd.finish().unwrap().into_inner();
        let bat = u32::from_be_bytes(
            bytes[BAT_OFFSET as usize..BAT_OFFSET as usize + 4]
                .try_into()
                .unwrap(),
        );
        let bitmap_at = u64::from(bat) * SECTOR;
        assert!(
            bytes[bitmap_at as usize..(bitmap_at + SECTOR) as usize]
                .iter()
                .all(|b| *b == 0xFF),
            "a write that overran into the bitmap would have cleared bits here"
        );
    }

    /// `open` is for the kind that has a table. A fixed image has no header,
    /// no table and no blocks — it is a raw file with 512 bytes appended, and
    /// the right thing to do with one is open it as a file.
    #[test]
    fn open_refuses_a_fixed_image_by_name_rather_than_misreading_it() {
        let mut bytes = vec![0u8; FOOTER_LEN];
        bytes[0..8].copy_from_slice(&FOOTER_COOKIE);
        bytes[12..16].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        bytes[16..24].copy_from_slice(&NO_DATA_OFFSET.to_be_bytes());
        bytes[60..64].copy_from_slice(&2u32.to_be_bytes()); // fixed
        let sum = checksum(&bytes);
        bytes[64..68].copy_from_slice(&sum.to_be_bytes());

        let Err(err) = DynamicVhd::open(Cursor::new(bytes)) else {
            panic!("a fixed image has no block table to open");
        };
        let err = err.to_string();
        assert!(
            err.contains("Fixed"),
            "the refusal must name what it found: {err}"
        );
    }

    /// **Never allocate from an unchecked length field** (CLAUDE.md). The
    /// block count comes out of the file, and a header claiming four billion
    /// blocks is 16 GB of `Vec` before anything is validated.
    #[test]
    fn open_refuses_a_header_claiming_more_blocks_than_the_disk_has_sectors() {
        let mut vhd = new_vhd(8 * 1024 * 1024);
        vhd.write_all(b"real enough").unwrap();
        let mut bytes = vhd.finish().unwrap().into_inner();

        let count_at = HEADER_OFFSET as usize + 28;
        bytes[count_at..count_at + 4].copy_from_slice(&u32::MAX.to_be_bytes());

        let Err(err) = DynamicVhd::open(Cursor::new(bytes)) else {
            panic!("a header claiming 4 billion blocks must be refused");
        };
        assert!(
            err.to_string().contains("more than the disk has sectors"),
            "got {err}"
        );
    }

    /// The image is valid at every moment, not only after `finish` — an
    /// interrupted build leaves something readable rather than a prefix.
    #[test]
    fn the_footer_and_table_are_valid_after_every_allocation() {
        let mut vhd = new_vhd(8 * 1024 * 1024);
        vhd.seek(SeekFrom::Start(4 * 1024 * 1024)).unwrap();
        vhd.write_all(b"data").unwrap();
        let size = vhd.file_size();

        let bytes = vhd.finish().unwrap().into_inner();
        assert_eq!(bytes.len() as u64, size);

        let head = parse_footer(&bytes[0..FOOTER_LEN]).expect("a footer copy at offset 0");
        let tail = parse_footer(&bytes[bytes.len() - FOOTER_LEN..]).expect("a footer at the end");
        assert_eq!(head, tail, "the two copies must be identical");
        assert_eq!(head.kind, VhdKind::Dynamic);
        assert_eq!(head.disk_size, 8 * 1024 * 1024);
        assert_eq!(head.data_offset, HEADER_OFFSET);
        assert!(head.checksum_matches);
    }

    #[test]
    fn the_dynamic_header_says_where_the_table_is_and_how_big_a_block_is() {
        let vhd = new_vhd(8 * 1024 * 1024);
        let bytes = vhd.finish().unwrap().into_inner();
        let header = &bytes[HEADER_OFFSET as usize..HEADER_OFFSET as usize + HEADER_LEN];

        assert_eq!(&header[0..8], b"cxsparse");
        assert_eq!(
            u64::from_be_bytes(header[16..24].try_into().unwrap()),
            BAT_OFFSET
        );
        assert_eq!(u32::from_be_bytes(header[28..32].try_into().unwrap()), 4);
        assert_eq!(
            u32::from_be_bytes(header[32..36].try_into().unwrap()),
            DEFAULT_BLOCK_SIZE
        );
        let stored = u32::from_be_bytes(header[36..40].try_into().unwrap());
        assert_eq!(stored, checksum_over(header, 36..40));
    }

    /// A block that has never been written is `0xFFFFFFFF` in the table, and
    /// the padding is the same value — so a reader that treats the padding as
    /// entries still sees nothing there.
    #[test]
    fn the_table_marks_untouched_blocks_and_pads_with_the_same_value() {
        let mut vhd = new_vhd(8 * 1024 * 1024);
        vhd.write_all(b"only the first").unwrap();
        let bytes = vhd.finish().unwrap().into_inner();
        let bat = &bytes[BAT_OFFSET as usize..BAT_OFFSET as usize + SECTOR as usize];

        assert_ne!(
            u32::from_be_bytes(bat[0..4].try_into().unwrap()),
            UNALLOCATED
        );
        for entry in 1..4 {
            let at = entry * 4;
            assert_eq!(
                u32::from_be_bytes(bat[at..at + 4].try_into().unwrap()),
                UNALLOCATED,
                "block {entry} was never written"
            );
        }
        assert!(bat[16..].iter().all(|b| *b == 0xFF), "the padding");
    }

    #[test]
    fn writing_past_the_end_of_the_disk_is_refused() {
        let mut vhd = new_vhd(1024 * 1024);
        vhd.seek(SeekFrom::Start(1024 * 1024)).unwrap();
        assert!(vhd.write_all(b"nowhere").is_err());
    }

    #[test]
    fn a_size_that_is_not_whole_sectors_is_refused() {
        assert!(DynamicVhd::create(Cursor::new(Vec::new()), 1000).is_err());
        assert!(DynamicVhd::create(Cursor::new(Vec::new()), 0).is_err());
    }

    #[test]
    fn a_block_size_that_is_not_a_power_of_two_is_refused() {
        assert!(DynamicVhd::create_with_block_size(Cursor::new(Vec::new()), 4096, 1536).is_err());
    }

    /// At the 2 MiB block size, the sector bitmap is exactly one sector — the
    /// arithmetic the module doc states, asserted rather than left as prose.
    #[test]
    fn the_sector_bitmap_is_one_sector_at_the_default_block_size() {
        let vhd = new_vhd(8 * 1024 * 1024);
        assert_eq!(vhd.bitmap_bytes(), SECTOR);
    }

    /// The geometry the VHD specification's own algorithm produces. Checked
    /// against Microsoft's implementation by `scripts/vhd-oracle-check.py`;
    /// pinned here so a change to the arithmetic is visible without it.
    #[test]
    fn the_geometry_is_the_specifications_own() {
        // 8 MiB: 16 384 sectors, below the 17-sector-track threshold.
        let small = geometry(16_384);
        assert_eq!(small[3], 17, "sectors per track");
        assert_eq!(small[2], 4, "heads, floored at four");
        // 32 GiB: past 65 535 * 16 * 63, so the big geometry.
        let big = geometry(32 * 1024 * 1024 * 1024 / SECTOR);
        assert_eq!(big[3], 255);
        assert_eq!(big[2], 16);
    }

    /// Two builds of the same card are byte-identical: no clock, no
    /// randomness. The same reason `Cargo.lock` is tracked.
    #[test]
    fn building_the_same_image_twice_gives_the_same_bytes() {
        let build = || {
            let mut vhd = new_vhd(8 * 1024 * 1024);
            vhd.seek(SeekFrom::Start(4096)).unwrap();
            vhd.write_all(b"the same bytes both times").unwrap();
            vhd.finish().unwrap().into_inner()
        };
        assert_eq!(build(), build());
    }
}
