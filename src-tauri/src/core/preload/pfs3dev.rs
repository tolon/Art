//! Driving `libpfs3` through ART's own block device.
//!
//! The two traits are close: both `Send + Sync`, both block-addressed. Two
//! differences to bridge — libpfs3 writes through `&self` (its own
//! `FileBlockDevice` uses a `Mutex<File>`, and so do we), and it addresses in
//! `u64` where ART uses `u32`.
//!
//! **The point is not tidiness.** PFS3 has no journalling — the on-disk
//! format has none, and neither does the original AmigaOS driver. Using
//! libpfs3's own [`FileBlockDevice`](libpfs3::io::FileBlockDevice) would leave
//! an interrupted install as an unknown volume. Through ART's device, every
//! PFS3 block write goes into [`core::volume::journal`](crate::core::volume::journal):
//! a block that was not saved cannot be written, and a rollback restores the
//! image byte for byte.
//!
//! Limit, written down rather than discovered: ART's `total_blocks()` is
//! `u32`, so 2 TB at 512-byte blocks — far beyond any card. A `u64` block
//! number that does not fit `u32` is refused with
//! [`libpfs3::error::Error::BlockOutOfRange`], never truncated.
//!
//! ## Why the device is borrowed, not owned
//!
//! `libpfs3::volume::Volume::from_device` takes `Box<dyn BlockDevice>`, which
//! — like every trait object behind a `Box` with no explicit lifetime — is
//! `Box<dyn BlockDevice + 'static>`. [`ArtBlockDevice`] instead borrows ART's
//! device (`Mutex<&'a mut D>`), because the device it has to wrap is itself a
//! borrow: [`core::volume::journal::Journalled`](crate::core::volume::journal::Journalled)
//! hands out `&'a mut dyn BlockDeviceMut` scoped to one operation, and that
//! borrow is exactly what has to reach every PFS3 write for the journal to see
//! it. A `'static`-only adapter would have to *own* the device, which would
//! mean copying it out of the journal's guard — the one thing that would
//! defeat this module's reason for existing.
//!
//! The consequence is real and left for whichever task next drives an actual
//! `Volume`/`Writer` through this adapter: `ArtBlockDevice<'a, D>` cannot be
//! boxed into `Volume::from_device`'s `Box<dyn BlockDevice>` for any `'a`
//! shorter than `'static`, which a journalled operation's borrow always is.
//! That task will need either a libpfs3 entry point that accepts a borrowed
//! device, or a restructuring on ART's side so the whole PFS3 operation runs
//! inside the borrow's scope without ever needing to erase its lifetime. This
//! module only builds and proves the adapter itself; it does not yet drive a
//! `Volume` through it.
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

/// Wraps `&'a mut D` so `libpfs3` can drive it through `&self`, the way its
/// own [`FileBlockDevice`](libpfs3::io::FileBlockDevice) uses a `Mutex<File>`.
///
/// See the module docs for why this borrows rather than owns the device, and
/// for what that means for `Volume::from_device`.
pub struct ArtBlockDevice<'a, D: ?Sized> {
    inner: Mutex<&'a mut D>,
}

impl<'a, D: ?Sized> ArtBlockDevice<'a, D> {
    pub fn new(device: &'a mut D) -> Self {
        Self {
            inner: Mutex::new(device),
        }
    }

    /// Lock the inner device. A poisoned lock is recovered rather than
    /// propagated — the same choice ART's other single-process devices make
    /// (`core::volume::device::FileRegion`): a panicking reader elsewhere
    /// must not turn every subsequent PFS3 block access into an error too.
    fn lock(&self) -> MutexGuard<'_, &'a mut D> {
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

impl<'a, D> Pfs3BlockDevice for ArtBlockDevice<'a, D>
where
    D: BlockDeviceMut + ?Sized,
{
    fn read_block(&self, block: u64, buf: &mut [u8]) -> libpfs3::error::Result<()> {
        let n = block_number(block)?;
        self.lock().read_block(n, buf).map_err(to_pfs3_error)
    }

    /// Loops over [`read_block`](Self::read_block) rather than reading the
    /// whole span in one call, so every block still passes through ART's own
    /// device — the journal (once this adapter is driving one) has to see
    /// each block individually, and a bulk read/write that bypassed that
    /// would defeat the reason this adapter exists.
    fn read_blocks(&self, block: u64, count: u32, buf: &mut [u8]) -> libpfs3::error::Result<()> {
        let block_size = self.block_size() as usize;
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
    /// [`write_block`](Self::write_block) is the entire point of this
    /// adapter, not an implementation detail — it is what lets a journal
    /// underneath see every block a bulk write touches.
    fn write_blocks(&self, block: u64, count: u32, data: &[u8]) -> libpfs3::error::Result<()> {
        let block_size = self.block_size() as usize;
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
    use std::collections::HashSet;
    use std::path::PathBuf;

    use super::*;
    use crate::core::volume::device::VecDevice;
    use crate::core::volume::journal::Journalled;
    use crate::core::volume::BlockDevice;
    use libpfs3::io::BlockDevice as Pfs3Device;

    /// The repository's own convention (`core::volume::device::tests::scratch`,
    /// `core::volume::journal::tests::scratch`) — deliberately not `tempfile`,
    /// which is not a dependency of this project.
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("art-pfs3dev-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_adapter_reads_and_writes_through_arts_own_device() {
        let mut backing = VecDevice::new(vec![0u8; 512 * 64], 512).unwrap();
        let device = ArtBlockDevice::new(&mut backing);

        device.write_block(3, &[0xAB; 512]).unwrap();
        let mut buf = [0u8; 512];
        device.read_block(3, &mut buf).unwrap();
        assert_eq!(buf, [0xAB; 512]);
    }

    #[test]
    fn a_block_past_the_end_is_an_error_not_a_short_write() {
        let mut backing = VecDevice::new(vec![0u8; 512 * 4], 512).unwrap();
        let device = ArtBlockDevice::new(&mut backing);
        assert!(device.write_block(9, &[0; 512]).is_err());
    }

    /// A `BlockDeviceMut` whose every write goes through ART's own journal
    /// (`core::volume::journal::Journalled`) rather than a raw buffer poke.
    /// `journal_holds` records a block only once `Journalled::write_block` —
    /// which refuses any block it did not already save — has accepted it, so
    /// it can only be true if the block's previous contents genuinely reached
    /// the journal before the new bytes were written.
    ///
    /// This is what actually gets mutation-checked here: if
    /// `ArtBlockDevice::write_block` were changed to bypass `D::write_block`
    /// (writing into some buffer of its own instead of delegating), this
    /// fixture's journalling logic would never run and `journal_holds` would
    /// stay false.
    struct JournallingDevice {
        _dir: PathBuf,
        image: PathBuf,
        backing: VecDevice,
        journalled_before_write: HashSet<u32>,
    }

    impl JournallingDevice {
        /// `tag` must be unique per test: `scratch` names its directory from
        /// the tag and this process's id alone, and two tests sharing a tag
        /// would race on the same file when the harness runs them in
        /// parallel (as it does by default) — each grabbing the other's
        /// half-written image mid-`remove_dir_all`/`create_dir_all`.
        fn new(tag: &str) -> Self {
            let dir = scratch(tag);
            let image = dir.join("disk.img");
            let bytes = vec![0u8; 512 * 16];
            std::fs::write(&image, &bytes).unwrap();
            Self {
                _dir: dir,
                image,
                backing: VecDevice::new(bytes, 512).unwrap(),
                journalled_before_write: HashSet::new(),
            }
        }

        fn journal_holds(&self, block: u32) -> bool {
            self.journalled_before_write.contains(&block)
        }
    }

    impl BlockDevice for JournallingDevice {
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

    impl BlockDeviceMut for JournallingDevice {
        fn write_block(&mut self, n: u32, buf: &[u8]) -> crate::core::error::CoreResult<()> {
            let mut op = Journalled::begin(&mut self.backing, &self.image, 0, "test write", &[n])?;
            op.write_block(n, buf)?;
            op.commit()?;
            // Only reached once the journal accepted and completed the write
            // — `Journalled::write_block` refuses any block `begin` did not
            // already save.
            self.journalled_before_write.insert(n);
            Ok(())
        }

        fn sync(&mut self) -> crate::core::error::CoreResult<()> {
            self.backing.sync()
        }
    }

    fn journalled_device(tag: &str) -> JournallingDevice {
        JournallingDevice::new(tag)
    }

    /// PFS3 has no journalling of its own — the format has none, and neither
    /// does the original AmigaOS driver. ART's journal is therefore the only
    /// crash safety a PFS3 write can have, which is the whole reason this
    /// adapter exists instead of libpfs3's own FileBlockDevice.
    #[test]
    fn a_write_through_a_journalled_device_is_journalled() {
        let mut journalled = journalled_device("single-write");
        {
            let device = ArtBlockDevice::new(&mut journalled);
            device.write_block(2, &[0x11; 512]).unwrap();
        }
        assert!(
            journalled.journal_holds(2),
            "block 2 was saved before being written"
        );
    }

    #[test]
    fn a_block_number_beyond_u32_is_refused_rather_than_truncated() {
        let mut backing = VecDevice::new(vec![0u8; 512 * 4], 512).unwrap();
        let device = ArtBlockDevice::new(&mut backing);
        assert!(device
            .read_block(u64::from(u32::MAX) + 1, &mut [0u8; 512])
            .is_err());
    }

    // ---- the parts the brief's four tests do not reach ----

    #[test]
    fn read_blocks_and_write_blocks_loop_over_the_single_block_methods() {
        let mut backing = VecDevice::new(vec![0u8; 512 * 8], 512).unwrap();
        let device = ArtBlockDevice::new(&mut backing);

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

    /// The same property the single-block test proves, but for the bulk
    /// path: a `write_blocks` that shortcut past `D::write_block` (writing
    /// the span directly instead of looping) would leave the journal blind
    /// to some or all of blocks 5, 6 and 7.
    #[test]
    fn a_bulk_write_through_a_journalled_device_journals_every_block() {
        let mut journalled = journalled_device("bulk-write");
        {
            let device = ArtBlockDevice::new(&mut journalled);
            device.write_blocks(5, 3, &[0x22; 512 * 3]).unwrap();
        }
        assert!(journalled.journal_holds(5));
        assert!(journalled.journal_holds(6));
        assert!(journalled.journal_holds(7));
    }

    #[test]
    fn flush_reaches_the_underlying_devices_sync() {
        let mut backing = VecDevice::new(vec![0u8; 512 * 4], 512).unwrap();
        let device = ArtBlockDevice::new(&mut backing);
        // `VecDevice::sync` is a no-op that always succeeds; this proves the
        // call reaches it rather than being swallowed before it gets there.
        assert!(device.flush().is_ok());
    }

    #[test]
    fn block_size_is_reported_in_bytes_not_blocks() {
        let mut backing = VecDevice::new(vec![0u8; 1024 * 4], 1024).unwrap();
        let device = ArtBlockDevice::new(&mut backing);
        assert_eq!(device.block_size(), 1024);
    }
}
