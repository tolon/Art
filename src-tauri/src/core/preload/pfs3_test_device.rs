//! An in-memory `libpfs3` block device for tests (ART-311, ART-315).
//!
//! **Sparse**: only written sectors take memory, so a 5 GB SUPERINDEX volume
//! costs its reserved area and what the test writes. **Shared**: a clone sees
//! the same sectors, so a test drops the writer that filled a volume and
//! reopens it from a clone — the reopened `Volume` knows only what reached the
//! device — or reads raw blocks afterwards. **Bounded** when made with
//! [`MemDevice::with_end`]: a write at or past the end is refused and
//! remembered, the way ART's own devices refuse a write past a partition
//! (`core/volume/device.rs`, `FileRegionMut::position`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use libpfs3::error::{Error, Result};

#[derive(Clone, Default)]
pub(crate) struct MemDevice {
    sectors: Arc<Mutex<HashMap<u64, Vec<u8>>>>,
    end: Option<u64>,
    refused: Arc<Mutex<Vec<u64>>>,
}

impl MemDevice {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// A device whose sectors `end..` do not exist.
    pub(crate) fn with_end(end: u64) -> Self {
        Self {
            end: Some(end),
            ..Self::default()
        }
    }

    /// Every sector a write was refused at, in order.
    pub(crate) fn refused(&self) -> Vec<u64> {
        self.refused.lock().unwrap().clone()
    }

    /// `len` bytes from `sector` on; a sector never written reads as zeros.
    pub(crate) fn read(&self, sector: u64, len: usize) -> Vec<u8> {
        let sectors = self.sectors.lock().unwrap();
        let mut out = vec![0u8; len];
        for (i, chunk) in out.chunks_mut(512).enumerate() {
            if let Some(data) = sectors.get(&(sector + i as u64)) {
                chunk.copy_from_slice(&data[..chunk.len()]);
            }
        }
        out
    }

    /// A test's own raw write of whole sectors from `sector` on. Never refused.
    pub(crate) fn patch(&self, sector: u64, data: &[u8]) {
        assert_eq!(data.len() % 512, 0, "patch whole sectors");
        let mut sectors = self.sectors.lock().unwrap();
        for (i, chunk) in data.chunks(512).enumerate() {
            sectors.insert(sector + i as u64, chunk.to_vec());
        }
    }
}

impl libpfs3::io::BlockDevice for MemDevice {
    fn read_block(&self, block: u64, buf: &mut [u8]) -> Result<()> {
        buf.copy_from_slice(&self.read(block, buf.len()));
        Ok(())
    }

    fn read_blocks(&self, block: u64, count: u32, buf: &mut [u8]) -> Result<()> {
        let len = count as usize * 512;
        buf[..len].copy_from_slice(&self.read(block, len));
        Ok(())
    }

    fn block_size(&self) -> u32 {
        512
    }

    fn write_block(&self, block: u64, data: &[u8]) -> Result<()> {
        self.write_blocks(block, 1, data)
    }

    fn write_blocks(&self, block: u64, count: u32, data: &[u8]) -> Result<()> {
        for i in 0..u64::from(count) {
            let n = block + i;
            if self.end.is_some_and(|end| n >= end) {
                self.refused.lock().unwrap().push(n);
                return Err(Error::BlockOutOfRange(n));
            }
            let at = i as usize * 512;
            self.sectors
                .lock()
                .unwrap()
                .insert(n, data[at..at + 512].to_vec());
        }
        Ok(())
    }

    fn flush(&self) -> Result<()> {
        Ok(())
    }
}
