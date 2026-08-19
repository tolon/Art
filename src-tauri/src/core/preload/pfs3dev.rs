//! Driving `libpfs3` through ART's own block device.
//!
//! The two traits are close: both `Send + Sync`, both block-addressed. Two
//! differences to bridge — libpfs3 writes through `&self` (its own
//! `FileBlockDevice` uses a `Mutex<File>`, and so do we), and it addresses in
//! `u64` where ART uses `u32`.
//!
//! Limit, written down rather than discovered: ART's `total_blocks()` is
//! `u32`, so 2 TB at 512-byte blocks — far beyond any card. A `u64` block
//! number that does not fit `u32` is refused with
//! [`libpfs3::error::Error::BlockOutOfRange`], never truncated.
//!
//! ## What crash safety this actually provides — and does not
//!
//! PFS3 has no journalling of its own — the on-disk format has none, and
//! neither does the original AmigaOS driver. That is still true, but it does
//! **not** mean `core/volume/journal.rs` is underneath a PFS3 write: journalling
//! needs the block set up front (`Journalled::begin(device, image, offset,
//! description, blocks: &[u32])` saves exactly those blocks, fsyncs, and then
//! permits writing exactly those and no others — the shape Stage W's own
//! writer uses, because it plans its `BlockSet` before touching anything).
//! libpfs3 decides which blocks to touch as it walks the filesystem and
//! cannot supply that list up front, so there is no journal here. An earlier
//! draft of this module claimed one anyway; that claim was wrong and has been
//! withdrawn.
//!
//! What this module actually guarantees is narrower and still real: **writes
//! cannot leave the partition being written.** [`ArtBlockDevice`] wraps
//! whatever [`BlockDeviceMut`] it is given, and for the real target —
//! [`FileRegionMut`](crate::core::volume::device::FileRegionMut) — that device
//! refuses a block number past its own end rather than clamping it (see its
//! own doc comment; `core/fat32.rs::Region` gives the boot partition the same
//! guarantee). So a PFS3 write, however libpfs3 sequences it, cannot reach the
//! neighbouring partition, the RDB, or the FAT32 area where the Amiga's first
//! RDB begins — not because this adapter checks it, but because the device
//! underneath already does, for every write, one block at a time.
//!
//! What that does *not* cover: G5 (the route that drives this) always
//! **formats before it fills** — the partition's prior contents are already
//! forfeit the moment the user confirms that step, so there is nothing there
//! worth protecting with a rollback. An interrupted run leaves an
//! **incomplete** volume. G5 does not need to notice that specifically: it
//! always formats before it fills, so the next attempt overwrites whatever
//! was left — complete or not — from scratch. Nothing in `core/preload/`
//! detects an incomplete volume as its own case, and this doc does not claim
//! it does. That is not the same thing as a corrupted volume with a recovery
//! path, and this module makes no claim of one — there is no rollback,
//! because there is nothing saved to roll back to.
//!
//! The one case where crash safety would genuinely matter — writing into an
//! **existing, already-populated** PFS3 volume without formatting it first —
//! is exactly the case this module cannot make safe, for the same reason it
//! cannot journal: libpfs3 does not hand over the block set in advance. ART
//! has no answer for that case, so it is refused rather than half-supported.
//! Enforcing that refusal is Task 9's job; this paragraph is only the record
//! of why it has to exist.
//!
//! ## Why the device is owned, not borrowed
//!
//! `libpfs3::volume::Volume::from_device` takes `Box<dyn BlockDevice>`, which
//! — like every trait object behind a `Box` with no explicit lifetime — is
//! `Box<dyn BlockDevice + 'static>`. [`ArtBlockDevice`] owns its device
//! (`Mutex<D>`, `D: BlockDeviceMut`) rather than borrowing it, which is what
//! lets a real one — `FileRegionMut`, which owns its `File` and carries no
//! lifetime of its own — satisfy that bound with nothing further to bridge.
//! The test
//! `the_owned_adapter_composes_with_libpfs3s_volume_from_device` proves the
//! composition actually compiles and runs, not just that the types look
//! right on paper.
//!
//! ## What `flush` maps to
//!
//! [`core::volume::BlockDeviceMut::sync`](crate::core::volume::BlockDeviceMut::sync) —
//! ART's own durability primitive, whatever it means for the device
//! underneath: an `fsync` for [`FileRegionMut`](crate::core::volume::device::FileRegionMut),
//! a no-op for [`VecDevice`](crate::core::volume::device::VecDevice) because
//! memory is already as durable as it gets and the write-back to disk is a
//! separate, atomic step the caller performs. Mapping `flush` to whatever
//! `sync` really is — rather than making it a no-op inside this adapter — is
//! what keeps the call honest instead of a claim the code does not keep.

use std::sync::{Mutex, MutexGuard};

use libpfs3::error::Error as Pfs3Error;
use libpfs3::io::BlockDevice as Pfs3BlockDevice;

use crate::core::error::CoreError;
use crate::core::volume::BlockDeviceMut;

/// Wraps `D` so `libpfs3` can drive it through `&self`, the way its own
/// [`FileBlockDevice`](libpfs3::io::FileBlockDevice) uses a `Mutex<File>`.
///
/// See the module docs for why this owns rather than borrows the device, and
/// for what that buys against `Volume::from_device`.
pub struct ArtBlockDevice<D> {
    inner: Mutex<D>,
}

impl<D> ArtBlockDevice<D>
where
    D: BlockDeviceMut,
{
    pub fn new(device: D) -> Self {
        Self {
            inner: Mutex::new(device),
        }
    }

    /// Lock the inner device. A poisoned lock is recovered rather than
    /// propagated — the same choice ART's other single-process devices make
    /// (`core::volume::device::FileRegion`): a panicking reader elsewhere
    /// must not turn every subsequent PFS3 block access into an error too.
    fn lock(&self) -> MutexGuard<'_, D> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// A `u64` block address that libpfs3 hands in must fit ART's `u32` before it
/// reaches any ART device. Refused, never truncated — see the module docs.
fn block_number(block: u64) -> Result<u32, Pfs3Error> {
    u32::try_from(block).map_err(|_| Pfs3Error::BlockOutOfRange(block))
}

/// ART's `CoreError` carries the detail libpfs3's callers need to see, so it
/// goes into the `io::Error` message rather than being collapsed to a bare
/// `Error::Io(ErrorKind::Other)` with nothing to say why.
fn to_pfs3_error(err: CoreError) -> Pfs3Error {
    Pfs3Error::Io(std::io::Error::other(err.to_string()))
}

impl<D> Pfs3BlockDevice for ArtBlockDevice<D>
where
    D: BlockDeviceMut,
{
    fn read_block(&self, block: u64, buf: &mut [u8]) -> libpfs3::error::Result<()> {
        let n = block_number(block)?;
        self.lock().read_block(n, buf).map_err(to_pfs3_error)
    }

    /// Loops over [`read_block`](Self::read_block) rather than reading the
    /// whole span in one call, so every block still passes through the
    /// underlying device's own bounds check individually. A raw multi-block
    /// read that computed one offset and one length from `count` could reach
    /// past the partition's own end in a single call; looping through the
    /// single-block path means that boundary is enforced block by block,
    /// exactly where the device (`FileRegionMut::read_block`, for the real
    /// target) already enforces it.
    fn read_blocks(&self, block: u64, count: u32, buf: &mut [u8]) -> libpfs3::error::Result<()> {
        let block_size = self.block_size() as usize;
        let expected = block_size
            .checked_mul(count as usize)
            .ok_or(Pfs3Error::TooShort("read buffer"))?;
        // A short buffer is already caught per-chunk below; an over-long one
        // would not be — its trailing bytes would simply go unfilled and
        // unremarked — so the exact length is refused up front rather than
        // silently accepted.
        if buf.len() != expected {
            return Err(Pfs3Error::TooShort("read buffer"));
        }
        for i in 0..u64::from(count) {
            let start = (i as usize)
                .checked_mul(block_size)
                .ok_or(Pfs3Error::TooShort("read buffer"))?;
            let end = start
                .checked_add(block_size)
                .ok_or(Pfs3Error::TooShort("read buffer"))?;
            let chunk = buf
                .get_mut(start..end)
                .ok_or(Pfs3Error::TooShort("read buffer"))?;
            let n = block_number(
                block
                    .checked_add(i)
                    .ok_or(Pfs3Error::BlockOutOfRange(block))?,
            )?;
            self.lock().read_block(n, chunk).map_err(to_pfs3_error)?;
        }
        Ok(())
    }

    fn block_size(&self) -> u32 {
        // ART's own devices reject a block size that is not a small multiple
        // of 512 at construction time, so this cannot realistically overflow
        // — but it is a cast rather than a checked conversion because there
        // is nothing sensible to refuse into; a `usize` block size this large
        // could not back a real device in the first place.
        self.lock().block_size() as u32
    }

    fn write_block(&self, block: u64, data: &[u8]) -> libpfs3::error::Result<()> {
        let n = block_number(block)?;
        self.lock().write_block(n, data).map_err(to_pfs3_error)
    }

    /// See [`read_blocks`](Self::read_blocks): looping over
    /// [`write_block`](Self::write_block) is what keeps the partition
    /// boundary enforced for every block a bulk write touches, not an
    /// implementation detail. A write that starts in range and runs past the
    /// partition's end fails on the first out-of-range block rather than
    /// silently landing bytes beyond it — the blocks already written stay
    /// written, which is fine under this module's actual safety story (see
    /// the module docs): G5 always formats before it fills, so a partially
    /// written volume is exactly as disposable as an unwritten one, and gets
    /// reformatted rather than resumed.
    fn write_blocks(&self, block: u64, count: u32, data: &[u8]) -> libpfs3::error::Result<()> {
        let block_size = self.block_size() as usize;
        let expected = block_size
            .checked_mul(count as usize)
            .ok_or(Pfs3Error::TooShort("write buffer"))?;
        // Same reasoning as `read_blocks`: a short buffer is already caught
        // per-chunk below, but an over-long one would have its trailing
        // bytes silently ignored rather than refused.
        if data.len() != expected {
            return Err(Pfs3Error::TooShort("write buffer"));
        }
        for i in 0..u64::from(count) {
            let start = (i as usize)
                .checked_mul(block_size)
                .ok_or(Pfs3Error::TooShort("write buffer"))?;
            let end = start
                .checked_add(block_size)
                .ok_or(Pfs3Error::TooShort("write buffer"))?;
            let chunk = data
                .get(start..end)
                .ok_or(Pfs3Error::TooShort("write buffer"))?;
            let n = block_number(
                block
                    .checked_add(i)
                    .ok_or(Pfs3Error::BlockOutOfRange(block))?,
            )?;
            self.lock().write_block(n, chunk).map_err(to_pfs3_error)?;
        }
        Ok(())
    }

    fn flush(&self) -> libpfs3::error::Result<()> {
        self.lock().sync().map_err(to_pfs3_error)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::core::volume::device::{FileRegionMut, VecDevice};
    use crate::core::volume::BlockDevice;
    use libpfs3::io::BlockDevice as Pfs3Device;

    /// The repository's own convention (`core::volume::device::tests::scratch`,
    /// `core::volume::journal::tests::scratch`) — deliberately not `tempfile`,
    /// which is not a dependency of this project.
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-pfs3dev-{tag}-{}",
            crate::core::test_scratch_id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_adapter_reads_and_writes_through_arts_own_device() {
        let backing = VecDevice::new(vec![0u8; 512 * 64], 512).unwrap();
        let device = ArtBlockDevice::new(backing);

        device.write_block(3, &[0xAB; 512]).unwrap();
        let mut buf = [0u8; 512];
        device.read_block(3, &mut buf).unwrap();
        assert_eq!(buf, [0xAB; 512]);
    }

    #[test]
    fn a_block_past_the_end_is_an_error_not_a_short_write() {
        let backing = VecDevice::new(vec![0u8; 512 * 4], 512).unwrap();
        let device = ArtBlockDevice::new(backing);
        assert!(device.write_block(9, &[0; 512]).is_err());
    }

    #[test]
    fn a_block_number_beyond_u32_is_refused_rather_than_truncated() {
        let backing = VecDevice::new(vec![0u8; 512 * 4], 512).unwrap();
        let device = ArtBlockDevice::new(backing);
        assert!(device
            .read_block(u64::from(u32::MAX) + 1, &mut [0u8; 512])
            .is_err());
        // Same helper (`block_number`) backs both directions; one assertion
        // closes the gap rather than leaving it implied.
        assert!(device
            .write_block(u64::from(u32::MAX) + 1, &[0u8; 512])
            .is_err());
    }

    // ---- the actual safety property: writes cannot leave the partition ----

    /// The property this module actually guarantees, proved against a real
    /// [`FileRegionMut`] rather than the in-memory `VecDevice`: an image with
    /// a "neighbouring partition" laid out right after the one being written,
    /// the way an RDB or a FAT32 boot area would sit next to it on a real
    /// card. A write past the region's own end is refused, and the bytes on
    /// both sides — the successful write just inside the boundary, and every
    /// block belonging to the next partition — are exactly what they were.
    #[test]
    fn a_write_past_the_regions_end_cannot_reach_the_next_partition() {
        let dir = scratch("bounded-writes");
        let image = dir.join("card.img");

        // Eight blocks: 0..4 are "our" partition, 4..8 are the next one.
        // Each block is stamped with its own number so a write landing in
        // the wrong place is obvious rather than merely different.
        let mut bytes = vec![0u8; 512 * 8];
        for block in 0..8u8 {
            bytes[block as usize * 512] = block;
        }
        std::fs::write(&image, &bytes).unwrap();

        let region = FileRegionMut::open(&image, 0, 4 * 512, 512).unwrap();
        let device = ArtBlockDevice::new(region);

        // In range: lands.
        device.write_block(3, &[0xAA; 512]).unwrap();
        // One block past this region's own end — refused, not clamped and
        // not silently written into the next partition's first block.
        assert!(device.write_block(4, &[0xBB; 512]).is_err());

        drop(device);
        let after = std::fs::read(&image).unwrap();
        assert_eq!(after[3 * 512], 0xAA, "the in-range write landed");
        for block in 4..8usize {
            assert_eq!(
                after[block * 512],
                block as u8,
                "block {block}, in the next partition, must be untouched"
            );
        }
    }

    /// The same property through the bulk path: a `write_blocks` call that
    /// starts inside the region and asks for more blocks than it has must
    /// not spill into what comes after it.
    #[test]
    fn a_bulk_write_that_would_overrun_the_region_touches_nothing_past_it() {
        let dir = scratch("bounded-bulk-write");
        let image = dir.join("card.img");

        let mut bytes = vec![0u8; 512 * 8];
        for block in 0..8u8 {
            bytes[block as usize * 512] = block;
        }
        std::fs::write(&image, &bytes).unwrap();

        let region = FileRegionMut::open(&image, 0, 4 * 512, 512).unwrap();
        let device = ArtBlockDevice::new(region);

        // Blocks 2, 3, 4: only 2 and 3 belong to this region. The loop
        // writes block by block, so 2 and 3 land before block 4 is refused —
        // a `write_blocks` that returned `Err` without writing anything would
        // pass a test that checked only the refusal, so both outcomes are
        // asserted here: the in-range blocks actually received the write,
        // and nothing outside the region — on either side — did.
        let err = device.write_blocks(2, 3, &[0xCC; 512 * 3]);
        assert!(err.is_err());

        drop(device);
        let after = std::fs::read(&image).unwrap();
        for block in 0..2usize {
            assert_eq!(
                after[block * 512],
                block as u8,
                "block {block}, before the write, must be untouched"
            );
        }
        for block in 2..4usize {
            assert_eq!(
                after[block * 512],
                0xCC,
                "block {block} is in range and must have received the write"
            );
        }
        for block in 4..8usize {
            assert_eq!(
                after[block * 512],
                block as u8,
                "block {block}, in the next partition, must be untouched"
            );
        }
    }

    // ---- ownership: the composition that could not be written before ----

    /// Confirms `ArtBlockDevice` really does satisfy `Box<dyn BlockDevice>`'s
    /// implicit `'static` bound, by handing one to libpfs3's own entry point
    /// rather than only implementing the trait in isolation. The image is
    /// blank, so `Volume::from_device` is expected to fail parsing a root
    /// block that was never written — the point here is that this compiles
    /// and runs at all, which the earlier borrowed-device shape could not do.
    #[test]
    fn the_owned_adapter_composes_with_libpfs3s_volume_from_device() {
        let dir = scratch("compose-with-volume");
        let image = dir.join("disk.hdf");
        std::fs::write(&image, vec![0u8; 512 * 16]).unwrap();

        let region = FileRegionMut::open(&image, 0, 512 * 16, 512).unwrap();
        let device = ArtBlockDevice::new(region);

        let result = libpfs3::volume::Volume::from_device(Box::new(device));
        assert!(
            result.is_err(),
            "a blank image is not a PFS3 volume — the point is that this call compiles at all"
        );
    }

    // ---- the rest ----

    #[test]
    fn read_blocks_and_write_blocks_loop_over_the_single_block_methods() {
        let backing = VecDevice::new(vec![0u8; 512 * 8], 512).unwrap();
        let device = ArtBlockDevice::new(backing);

        let mut payload = vec![0u8; 512 * 3];
        payload[0] = 1;
        payload[512] = 2;
        payload[1024] = 3;
        device.write_blocks(2, 3, &payload).unwrap();

        let mut buf = vec![0u8; 512 * 3];
        device.read_blocks(2, 3, &mut buf).unwrap();
        assert_eq!(buf, payload);

        // And every block landed at its own offset, not just the first.
        let mut one = [0u8; 512];
        device.read_block(4, &mut one).unwrap();
        assert_eq!(one[0], 3);
    }

    /// `VecDevice::sync` is a documented no-op — asserting only
    /// `flush().is_ok()` against it cannot fail even if `flush` never called
    /// through to the device at all, since `Ok(())` and "did nothing" are
    /// indistinguishable there. This fixture observes the call instead of
    /// hoping its absence would show up as an error: `synced` flips only
    /// inside `BlockDeviceMut::sync`, so it can only be true if `flush`
    /// actually reached it.
    struct SyncTrackingDevice {
        backing: VecDevice,
        synced: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    impl BlockDevice for SyncTrackingDevice {
        fn block_size(&self) -> usize {
            self.backing.block_size()
        }

        fn total_blocks(&self) -> u32 {
            self.backing.total_blocks()
        }

        fn read_block(&self, n: u32, buf: &mut [u8]) -> crate::core::error::CoreResult<()> {
            self.backing.read_block(n, buf)
        }
    }

    impl BlockDeviceMut for SyncTrackingDevice {
        fn write_block(&mut self, n: u32, buf: &[u8]) -> crate::core::error::CoreResult<()> {
            self.backing.write_block(n, buf)
        }

        fn sync(&mut self) -> crate::core::error::CoreResult<()> {
            self.synced.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn flush_reaches_the_underlying_devices_sync() {
        let synced = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let backing = SyncTrackingDevice {
            backing: VecDevice::new(vec![0u8; 512 * 4], 512).unwrap(),
            synced: synced.clone(),
        };
        let device = ArtBlockDevice::new(backing);

        assert!(
            !synced.load(std::sync::atomic::Ordering::SeqCst),
            "sanity: nothing has synced yet"
        );
        device.flush().unwrap();
        assert!(
            synced.load(std::sync::atomic::Ordering::SeqCst),
            "flush must reach BlockDeviceMut::sync, not just return Ok"
        );
    }

    #[test]
    fn block_size_is_reported_in_bytes_not_blocks() {
        let backing = VecDevice::new(vec![0u8; 1024 * 4], 1024).unwrap();
        let device = ArtBlockDevice::new(backing);
        assert_eq!(device.block_size(), 1024);
    }
}
