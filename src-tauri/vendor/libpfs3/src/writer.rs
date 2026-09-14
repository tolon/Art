//! Writable PFS3 volume — file/directory creation and deletion.
//!
//! Builds on the read-only Volume and adds:
//! - Data block allocation/free via bitmap
//! - Reserved block allocation via reserved bitmap
//! - Anode allocation and chain building
//! - Directory entry creation and removal
//! - Rootblock update
//!
//! Modified by ART on 2026-09-13 (ART-312): `get_anode_block_nr` sees this
//! writer's own pending writes; on 2026-09-14 (ART-313): a continuation directory
//! block's parent; (ART-315) the data bitmap's bounds; (ART-314) names — a name
//! longer than `fnsize - 1` bytes is refused before anything is allocated;
//! (ART-311) the anode search range, index blocks on demand, super blocks,
//! the rootblock extension, the roving anode search; (ART-318) the deldir write path;
//! on 2026-09-14, the final review: `Writer::open` refuses a `reserved_blksize`
//! other than 1024/2048/4096 (M3); `max_name_bytes` calls the one name-limit
//! rule now in `format::pfs3_name_limit` (M6);
//! on 2026-09-14 (ART-317): a caller-supplied datestamp;
//! on 2026-09-14 (ART-319): every public mutator discards back to the last
//! successful commit on error, and a commit that fails part-way locks the
//! writer;
//! `ART-PATCH.md` in this crate's root says what and why.

use crate::error::{Error, Result};
use crate::ondisk::*;
use crate::volume::Volume;

/// Writable PFS3 volume — file/directory creation, deletion, and formatting.
pub struct Writer {
    /// The underlying read-only volume (also used for writes to disk).
    pub vol: Volume,
    // Geometry
    resblocksize: u32,
    rescluster: u32,
    index_per_block: u32,
    anodes_per_block: u32,
    firstreserved: u32,
    numreserved: u32,
    bitmapstart: u32,
    /// Authoritative datestamp counter (copied from rootblock on open, incremented on each commit).
    datestamp: u32,
    // Mutable state
    res_bitmap: Vec<u32>,
    data_bm: Vec<(u32, Vec<u32>)>, // (blk_num, longs)
    /// Pending reserved block writes, flushed in place before the rootblock
    /// update — not copy-on-write (M5, final review). Each write lands on
    /// disk as `flush_pending` runs it; the rootblock, written last, is where
    /// this became true, not the point every earlier write becomes true at
    /// once.
    pending_writes: Vec<(u32, Vec<u8>)>,
    /// ART-311: per anode block, whether a search found no free anode in it —
    /// pfs3aio's in-memory `anblkbitmap` (`anodes.c:949-960`), inverted. Never on disk.
    anode_block_full: Vec<bool>,
    /// ART-311: the anode block the last allocation came from — pfs3aio's
    /// `curranseqnr` (`anodes.c:465`). In memory only: pfs3aio also saves it in
    /// the rootblock extension (`update.c:247`) as a hint it recovers from
    /// (`anodes.c:436-443`); this writer starts each session at 0.
    anode_roving: u32,
    /// ART-311: `rootblock_ext.superindex` changed; `update_rootblock` writes the extension.
    rext_dirty: bool,
    /// "Now" for this writer, as (days, minutes, ticks): the date of each new or
    /// rewritten directory entry, and the date a delete stamps on the deldir
    /// block and `rext.dd_creation*` (pfs3aio `AddToDeldir`, `directory.c:4556-4560`).
    /// `None` is the current time, as 0.1.3 stamped it (UTC). Added by ART for
    /// ART-317, whose caller sets local time before each operation. A deldir
    /// entry's own date is not this: it is copied from the deleted entry.
    entry_date: Option<(u16, u16, u16)>,
    /// ART-319: set when a commit itself (`update_rootblock`, or
    /// `set_volume_name`'s own direct write) failed part-way — the device may
    /// already be half-written, since a pending write lands in place, not
    /// copy-on-write (M5) — or when `discard_to_last_commit`'s own reload
    /// could not even read the device back (M1, final review), leaving
    /// nothing safe to fall back to. Once set, every later public mutating
    /// call refuses immediately with `Error::CommitFailed`, before touching
    /// anything; nothing clears it — the caller must reopen the volume, by
    /// building a new `Volume` from the device, not by calling `Writer::open`
    /// on the `Volume` `into_volume` hands back, which still holds this
    /// failed attempt's in-memory state (M2, final review).
    poisoned: bool,
}

impl Writer {
    /// Open a volume for writing.
    pub fn open(vol: Volume) -> Result<Self> {
        let rb = &vol.rootblock;
        let rbs = rb.reserved_blksize as u32;
        // M3 (final review): the writer indexes fixed offsets that only fit
        // these three sizes — the rootblock extension's superindex write
        // reaches 0x80 (`update_rootblock`), and a deldir block's fixed 31
        // entries (`DELENTRIES_PER_BLOCK`) reach byte 1024 (`move_to_deldir`).
        // `Volume` itself only requires >= 64 (`validate_rbs`, read paths are
        // bounds-checked); the writer needs the stronger refusal here, before
        // any of those writes, rather than an out-of-bounds index under
        // `panic = "abort"`.
        if rbs != 1024 && rbs != 2048 && rbs != 4096 {
            return Err(Error::Corrupt(format!(
                "reserved_blksize {rbs} is not 1024, 2048 or 4096 — the writer cannot write this volume"
            )));
        }
        let rescluster = rbs / vol.block_size();
        let firstreserved = rb.firstreserved;
        let numreserved = (rb.lastreserved - firstreserved + 1) / rescluster;
        let index_per_block = rb.index_per_block();
        let anodes_per_block = rb.anodes_per_block();
        let bitmapstart = rb.lastreserved + 1;
        let datestamp = rb.datestamp;

        let mut w = Self {
            resblocksize: rbs,
            rescluster,
            index_per_block,
            anodes_per_block,
            firstreserved,
            numreserved,
            bitmapstart,
            datestamp,
            res_bitmap: Vec::new(),
            data_bm: Vec::new(),
            pending_writes: Vec::new(),
            anode_block_full: Vec::new(),
            anode_roving: 0,
            rext_dirty: false,
            entry_date: None,
            poisoned: false,
            vol,
        };
        w.load_reserved_bitmap()?;
        w.load_data_bitmap()?;
        Ok(w)
    }

    /// Consume the writer and return the underlying volume. **M2 (final
    /// review): if this writer is locked (`self.poisoned`), the `Volume`
    /// returned here still holds the failed attempt's own in-memory
    /// rootblock and other state** — `Writer::open` on it has no lock of its
    /// own and its next commit would write those values. Reopening a locked
    /// volume means building a fresh `Volume` from the device
    /// (`Volume::from_device` / `open*`), not calling `Writer::open` on the
    /// value this returns.
    pub fn into_volume(self) -> Volume {
        self.vol
    }

    /// "Now" for this writer, as (days, minutes, ticks): the date of each new or
    /// rewritten directory entry, and the date a delete stamps on the deldir
    /// block and `rext.dd_creation*` (pfs3aio `AddToDeldir`, `directory.c:4556-4560`).
    /// `None` is the current time, as 0.1.3 stamped it (UTC). Added by ART for
    /// ART-317, whose caller sets local time before each operation. A deldir
    /// entry's own date is not this: it is copied from the deleted entry.
    pub fn set_entry_date(&mut self, date: Option<(u16, u16, u16)>) {
        self.entry_date = date;
    }

    fn entry_datestamp(&self) -> (u16, u16, u16) {
        self.entry_date.unwrap_or_else(crate::util::current_amiga_datestamp)
    }

    fn next_datestamp(&mut self) -> u32 {
        self.datestamp += 1;
        self.datestamp
    }

    /// ART-314: the longest name this volume can store and find again — M6
    /// (final review): the one rule now lives in `format::pfs3_name_limit`,
    /// which ART's `core::preload::native` calls too.
    fn max_name_bytes(&self) -> usize {
        crate::format::pfs3_name_limit(self.vol.fnsize())
    }

    /// ART-314: refuse a name the volume would store and never find again,
    /// before anything is allocated. 0.1.3 stored up to 107 bytes and cut the rest.
    fn check_name_len(&self, name: &str) -> Result<()> {
        let max = self.max_name_bytes();
        if name.len() > max {
            return Err(Error::NameTooLong {
                name: name.to_string(),
                len: name.len(),
                max,
            });
        }
        Ok(())
    }

    /// ART-319: entry point for every public mutator, `op`. On `Err`, unless
    /// the failure already locked the writer (`self.poisoned` — set by
    /// `update_rootblock`/`set_volume_name`'s own commit write, because the
    /// device may already be half-written), this discards back to the last
    /// successful commit and returns the original error unchanged. Calling
    /// this from within an already-guarded call (a path wrapper calling its
    /// `_in` twin, `rename_in` calling `delete_in`) is harmless: a reload
    /// twice reads the identical, still-current state a second time, and an
    /// inner call that already committed (its own `update_rootblock` ran) is
    /// simply what "the last commit" now is for the outer discard to reload.
    /// Research (`D:\Projeler\Amiga\scratch-0913\art319-pfs3aio-research.md`,
    /// pfs3aio `211f7f0`): pfs3aio publishes state only at its own commit
    /// point, the root block write (`update.c:265-270`), and on a failed
    /// commit explicitly reverses the in-memory bitmap changes it had staged
    /// (`UndoFreeList`, `update.c:311-328`; the deferred-free list itself,
    /// `allocation.c:765-799`, with its intent stated in the comment at
    /// `allocation.c:741-745`) rather than re-reading the disk — this
    /// crate's writer discards by re-reading instead, because every write of
    /// this writer already goes through `pending_writes` and only reaches
    /// disk at that same commit point (see `discard_to_last_commit` below).
    fn guarded<T>(&mut self, op: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        if self.poisoned {
            return Err(Error::CommitFailed);
        }
        let result = op(self);
        if result.is_err() && !self.poisoned {
            self.discard_to_last_commit();
        }
        result
    }

    /// ART-319: rebuild this writer's in-memory state from what the device
    /// actually holds — the last successful commit. Every metadata write of
    /// this writer goes through `write_reserved` into `pending_writes`, only
    /// reaching disk in `update_rootblock`'s `flush_pending`; a failure
    /// anywhere before that point (`DiskFull` mid `alloc_data_blocks`,
    /// `move_to_deldir` staging an entry and then `free_data_blocks`
    /// failing) has therefore changed nothing on disk, so reloading from the
    /// device discards exactly the failed attempt's in-memory half-state and
    /// nothing else — the same commit-point discipline pfs3aio's own
    /// `UpdateDisk`/`UndoFreeList` keeps (`update.c:265-270,311-328`,
    /// `allocation.c:741-799`; see `guarded`'s own comment above). `datestamp`
    /// is kept, monotonic, rather than reloaded — the disk's own counter can
    /// only be lower or equal; `anode_roving` (a hint) and `entry_date`
    /// (caller-set) are left untouched. **If the device cannot even be
    /// re-read here, there is nothing safe left to fall back to, so this
    /// locks the writer instead of leaving it running on state it could not
    /// refresh (M1, final review) — even when the operation that triggered
    /// this discard never itself touched the device** (a refusal that fires
    /// before any I/O, e.g. `check_name_len`).
    fn discard_to_last_commit(&mut self) {
        self.pending_writes.clear();
        if self.vol.reload().is_err() {
            self.poisoned = true;
            return;
        }
        self.res_bitmap.clear();
        self.data_bm.clear(); // load_data_bitmap pushes
        if self.load_reserved_bitmap().is_err() || self.load_data_bitmap().is_err() {
            self.poisoned = true;
            return;
        }
        self.anode_block_full.clear();
        self.rext_dirty = false;
        self.datestamp = self.datestamp.max(self.vol.rootblock.datestamp);
    }

    // ---- High-level API (path-based, for CLI) ----

    /// Write a file at the given path. Parent directories must exist.
    pub fn write_file(&mut self, path: &str, data: &[u8]) -> Result<()> {
        self.guarded(|w| w.write_file_impl(path, data))
    }

    fn write_file_impl(&mut self, path: &str, data: &[u8]) -> Result<()> {
        let (parent_anode, filename) = self.split_path(path)?;
        self.write_file_in(parent_anode, &filename, data)
    }

    /// Create a directory at the given path.
    pub fn create_dir(&mut self, path: &str) -> Result<()> {
        self.guarded(|w| w.create_dir_impl(path))
    }

    fn create_dir_impl(&mut self, path: &str) -> Result<()> {
        let (parent_anode, dirname) = self.split_path(path)?;
        self.create_dir_in(parent_anode, &dirname)
    }

    /// Delete a file or empty directory at the given path.
    pub fn delete(&mut self, path: &str) -> Result<()> {
        self.guarded(|w| w.delete_impl(path))
    }

    fn delete_impl(&mut self, path: &str) -> Result<()> {
        let (parent_anode, name) = self.split_path(path)?;
        self.delete_in(parent_anode, &name)
    }

    /// Set the volume name (max 30 characters).
    pub fn set_volume_name(&mut self, name: &str) -> Result<()> {
        self.guarded(|w| w.set_volume_name_impl(name))
    }

    fn set_volume_name_impl(&mut self, name: &str) -> Result<()> {
        let name_bytes = name.as_bytes();
        let len = name_bytes.len().min(30);
        self.vol.rootblock.diskname = name[..len].to_string();
        // Patch the rootblock cluster in-place
        let bs = self.vol.block_size() as usize;
        let rblkcluster = self.vol.rootblock.rblkcluster as u32;
        let cluster_size = rblkcluster as usize * bs;
        let mut cluster = vec![0u8; cluster_size];
        self.vol
            .dev
            .read_blocks(self.firstreserved as u64, rblkcluster, &mut cluster)?;
        for b in &mut cluster[RB_OFF_DISKNAME..RB_OFF_DISKNAME + 32] {
            *b = 0;
        }
        cluster[RB_OFF_DISKNAME] = len as u8;
        cluster[RB_OFF_DISKNAME + 1..RB_OFF_DISKNAME + 1 + len].copy_from_slice(&name_bytes[..len]);
        let ds = self.next_datestamp();
        put_u32(&mut cluster, RB_OFF_DATESTAMP, ds);
        // ART-319: this writes the rootblock cluster directly — its own
        // commit point, not routed through `update_rootblock` — so a failure
        // here is treated the same way: the device may already hold a
        // half-written sector, so this locks the writer rather than letting
        // `guarded` discard and continue.
        if let Err(e) = self
            .vol
            .dev
            .write_blocks(self.firstreserved as u64, rblkcluster, &cluster)
            .and_then(|()| self.vol.dev.flush())
        {
            self.poisoned = true;
            return Err(e);
        }
        Ok(())
    }

    // ---- Anode-based API (for FUSE) ----

    /// Create a file in a directory identified by anode.
    pub fn write_file_in(&mut self, parent_anode: u32, name: &str, data: &[u8]) -> Result<()> {
        self.guarded(|w| w.write_file_in_impl(parent_anode, name, data))
    }

    fn write_file_in_impl(&mut self, parent_anode: u32, name: &str, data: &[u8]) -> Result<()> {
        self.check_name_len(name)?;
        // Check if file already exists — if so, overwrite it
        if let Ok((_, entry_data, pos)) = self.find_dir_entry(parent_anode, name) {
            let entry_type = entry_data[pos + 1] as i8;
            if entry_type == ST_FILE || entry_type == ST_ROLLOVERFILE {
                let file_anode =
                    u32::from_be_bytes(entry_data[pos + 2..pos + 6].try_into().unwrap());
                return self.overwrite_file_in(parent_anode, name, file_anode, data);
            }
        }
        self.write_file_in_no_commit(parent_anode, name, data)?;
        self.update_rootblock()
    }

    /// Write file without committing — caller must call update_rootblock().
    fn write_file_in_no_commit(
        &mut self,
        parent_anode: u32,
        name: &str,
        data: &[u8],
    ) -> Result<()> {
        self.check_name_len(name)?;
        let bs = self.vol.block_size() as usize;
        let num_blocks = data.len().div_ceil(bs).max(1);

        let blocks = self.alloc_data_blocks(num_blocks as u32)?;
        for (i, &blk) in blocks.iter().enumerate() {
            let start = i * bs;
            let end = (start + bs).min(data.len());
            let mut sector = vec![0u8; bs];
            if start < data.len() {
                sector[..end - start].copy_from_slice(&data[start..end]);
            }
            self.vol.dev.write_block(blk as u64, &sector)?;
        }
        self.vol.dev.flush()?; // data durable before metadata

        let anodenr = self.create_anode_chain(&blocks)?;
        self.add_dir_entry(parent_anode, name, ST_FILE, anodenr, data.len() as u64, 0)
    }

    /// Create a directory in a parent identified by anode. Returns the new dir's anode number.
    pub fn create_dir_in(&mut self, parent_anode: u32, name: &str) -> Result<()> {
        self.guarded(|w| w.create_dir_in_impl(parent_anode, name))
    }

    fn create_dir_in_impl(&mut self, parent_anode: u32, name: &str) -> Result<()> {
        self.check_name_len(name)?;
        let dir_blk = self.alloc_reserved_block()?;
        let anodenr = self.alloc_anode(1, dir_blk, 0)?;

        let mut dir_data = vec![0u8; self.resblocksize as usize];
        put_u16(&mut dir_data, 0x00, DBLKID);
        put_u32(&mut dir_data, 0x04, self.next_datestamp());
        put_u32(&mut dir_data, 0x0C, anodenr);
        put_u32(&mut dir_data, 0x10, parent_anode);
        self.write_reserved(dir_blk, &dir_data)?;

        self.add_dir_entry(parent_anode, name, ST_USERDIR, anodenr, 0, 0)?;
        self.update_rootblock()
    }

    /// Create a softlink in a parent directory.
    pub fn create_softlink(&mut self, path: &str, target: &str) -> Result<()> {
        self.guarded(|w| w.create_softlink_impl(path, target))
    }

    fn create_softlink_impl(&mut self, path: &str, target: &str) -> Result<()> {
        let (parent_anode, name) = self.split_path(path)?;
        self.create_softlink_in(parent_anode, &name, target)
    }

    /// Create a softlink by parent anode.
    pub fn create_softlink_in(
        &mut self,
        parent_anode: u32,
        name: &str,
        target: &str,
    ) -> Result<()> {
        self.guarded(|w| w.create_softlink_in_impl(parent_anode, name, target))
    }

    fn create_softlink_in_impl(
        &mut self,
        parent_anode: u32,
        name: &str,
        target: &str,
    ) -> Result<()> {
        self.check_name_len(name)?;
        let data = target.as_bytes();
        let bs = self.vol.block_size() as usize;
        let num_blocks = data.len().div_ceil(bs).max(1);
        let blocks = self.alloc_data_blocks(num_blocks as u32)?;
        for (i, &blk) in blocks.iter().enumerate() {
            let start = i * bs;
            let end = (start + bs).min(data.len());
            let mut sector = vec![0u8; bs];
            if start < data.len() {
                sector[..end - start].copy_from_slice(&data[start..end]);
            }
            self.vol.dev.write_block(blk as u64, &sector)?;
        }
        self.vol.dev.flush()?; // data durable before metadata
        let anodenr = self.create_anode_chain(&blocks)?;
        self.add_dir_entry(
            parent_anode,
            name,
            ST_SOFTLINK,
            anodenr,
            data.len() as u64,
            0,
        )?;
        self.update_rootblock()
    }

    /// Create a hardlink in a parent directory.
    pub fn create_hardlink(&mut self, path: &str, target_anode: u32) -> Result<()> {
        self.guarded(|w| w.create_hardlink_impl(path, target_anode))
    }

    fn create_hardlink_impl(&mut self, path: &str, target_anode: u32) -> Result<()> {
        let (parent_anode, name) = self.split_path(path)?;
        self.check_name_len(&name)?;
        self.add_dir_entry(parent_anode, &name, ST_LINKFILE, target_anode, 0, 0)?;
        self.update_rootblock()
    }

    /// Undelete a file from the deldir by index. Writes it to `dest_path`.
    pub fn undelete(&mut self, deldir_idx: usize, dest_path: &str) -> Result<()> {
        self.guarded(|w| w.undelete_impl(deldir_idx, dest_path))
    }

    fn undelete_impl(&mut self, deldir_idx: usize, dest_path: &str) -> Result<()> {
        // Read the deldir entry
        let rext = self
            .vol
            .rootblock_ext
            .as_ref()
            .ok_or_else(|| Error::NotFound("no rootblock extension".into()))?;
        let deldirblocks: Vec<u32> = rext
            .deldirblocks
            .iter()
            .copied()
            .filter(|&b| b != 0)
            .collect();
        let rbs = self.vol.rootblock.reserved_blksize;
        let entries_per_block = deldir_entries_per_block(rbs);
        if entries_per_block == 0 {
            return Err(Error::Corrupt("invalid reserved block size".into()));
        }

        let block_idx = deldir_idx / entries_per_block;
        let slot_idx = deldir_idx % entries_per_block;
        if block_idx >= deldirblocks.len() {
            return Err(Error::NotFound(format!(
                "deldir index {} out of range",
                deldir_idx
            )));
        }
        let blk = deldirblocks[block_idx];
        let data = self.read_reserved_raw(blk)?;
        let off = DELDIR_HEADER_SIZE + slot_idx * DELDIR_ENTRY_SIZE;
        // M2 (final review): `fsizex` is size only on a MODE_LARGEFILE volume.
        let largefile = self.vol.rootblock.has_largefile();
        let entry = DelDirEntry::parse(&data[off..off + DELDIR_ENTRY_SIZE], largefile)
            .ok_or_else(|| Error::NotFound("empty deldir slot".into()))?;

        // Check destination doesn't already exist
        if self.vol.lookup(dest_path)?.is_some() {
            return Err(Error::AlreadyExists(dest_path.to_string()));
        }

        let old_anode = entry.anode;

        // ART-318: a delete frees the file's blocks, so a later write may have
        // taken them; pfs3aio refuses such an entry (`IsDelfileValid`,
        // `directory.c:4092-4114`).
        let chain = self
            .vol
            .anodes
            .get_chain(old_anode, self.vol.dev.as_ref(), &mut self.vol.cache)?;
        let reused = chain
            .iter()
            .any(|an| (0..an.clustersize).any(|i| !self.data_block_is_free(an.blocknr + i)));
        if reused {
            return Err(Error::NotFound(format!(
                "deleted file '{}': its blocks have been reused",
                entry.filename
            )));
        }

        // Read file data via the anode chain (still intact)
        let file_data = self.vol.read_file_data(old_anode, entry.file_size())?;

        // Write as new file (no commit yet)
        let (parent_anode, filename) = self.split_path(dest_path)?;
        self.write_file_in_no_commit(parent_anode, &filename, &file_data)?;

        // Free old anode chain
        let _ = self.clear_anode_chain(old_anode);

        // Clear the deldir slot
        let mut block_data = self.read_reserved_raw(blk)?;
        for b in &mut block_data[off..off + DELDIR_ENTRY_SIZE] {
            *b = 0;
        }
        self.write_reserved(blk, &block_data)?;

        // Single commit
        self.update_rootblock()
    }

    /// Force-remove a directory entry without touching anodes or data blocks.
    /// Used by check --repair for entries with broken anode chains.
    pub fn force_remove_entry(&mut self, parent_anode: u32, name: &str) -> Result<()> {
        self.guarded(|w| w.force_remove_entry_impl(parent_anode, name))
    }

    fn force_remove_entry_impl(&mut self, parent_anode: u32, name: &str) -> Result<()> {
        self.remove_dir_entry(parent_anode, name)?;
        self.update_rootblock()
    }

    /// Repair: set the rootblock's blocksfree field.
    pub fn repair_blocksfree(&mut self, correct_free: u32) -> Result<()> {
        self.guarded(|w| w.repair_blocksfree_impl(correct_free))
    }

    fn repair_blocksfree_impl(&mut self, correct_free: u32) -> Result<()> {
        self.vol.rootblock.blocksfree = correct_free;
        self.update_rootblock()
    }

    /// Repair: set the rootblock's reserved_free field.
    pub fn repair_reserved_free(&mut self, correct_free: u32) -> Result<()> {
        self.guarded(|w| w.repair_reserved_free_impl(correct_free))
    }

    fn repair_reserved_free_impl(&mut self, correct_free: u32) -> Result<()> {
        self.vol.rootblock.reserved_free = correct_free;
        self.update_rootblock()
    }

    /// Overwrite an existing file's data in-place, reusing its anode.
    /// The anode number stays stable — safe for FUSE inode caching.
    pub fn overwrite_file_in(
        &mut self,
        parent_anode: u32,
        name: &str,
        file_anode: u32,
        data: &[u8],
    ) -> Result<()> {
        self.guarded(|w| w.overwrite_file_in_impl(parent_anode, name, file_anode, data))
    }

    fn overwrite_file_in_impl(
        &mut self,
        parent_anode: u32,
        name: &str,
        file_anode: u32,
        data: &[u8],
    ) -> Result<()> {
        let bs = self.vol.block_size() as usize;
        let new_blocks_needed = data.len().div_ceil(bs).max(1) as u32;

        // Get existing chain
        let old_chain =
            self.vol
                .anodes
                .get_chain(file_anode, self.vol.dev.as_ref(), &mut self.vol.cache)?;
        let old_total: u32 = old_chain.iter().map(|a| a.clustersize).sum();

        // Write data to existing blocks (reuse as many as possible)
        let mut written = 0usize;
        let mut blocks_used = 0u32;
        let mut sector = vec![0u8; bs];
        for an in &old_chain {
            for i in 0..an.clustersize {
                if blocks_used >= new_blocks_needed {
                    break;
                }
                sector.fill(0);
                let start = written;
                let end = (start + bs).min(data.len());
                if start < data.len() {
                    sector[..end - start].copy_from_slice(&data[start..end]);
                }
                self.vol
                    .dev
                    .write_block(an.blocknr as u64 + i as u64, &sector)?;
                written += bs;
                blocks_used += 1;
            }
            if blocks_used >= new_blocks_needed {
                break;
            }
        }
        self.vol.dev.flush()?;

        if new_blocks_needed <= old_total {
            // Shrink: free excess blocks and truncate the anode chain
            self.truncate_anode_chain(file_anode, new_blocks_needed)?;
        } else {
            // Grow: allocate additional blocks and extend the chain
            let extra = new_blocks_needed - old_total;
            let new_blocks = self.alloc_data_blocks(extra)?;
            for &blk in &new_blocks {
                sector.fill(0);
                let start = written;
                let end = (start + bs).min(data.len());
                if start < data.len() {
                    sector[..end - start].copy_from_slice(&data[start..end]);
                }
                self.vol.dev.write_block(blk as u64, &sector)?;
                written += bs;
            }
            self.vol.dev.flush()?;
            // Extend the existing chain with new blocks
            let new_chain_head = self.create_anode_chain(&new_blocks)?;
            self.append_to_anode_chain(file_anode, new_chain_head)?;
        }

        // Update file size in the directory entry
        self.update_dir_entry_size(parent_anode, name, data.len() as u64)?;
        self.update_rootblock()
    }

    /// Truncate an anode chain to `keep_blocks` total blocks.
    /// Frees excess data blocks and anode slots.
    fn truncate_anode_chain(&mut self, head: u32, keep_blocks: u32) -> Result<()> {
        let chain = self
            .vol
            .anodes
            .get_chain(head, self.vol.dev.as_ref(), &mut self.vol.cache)?;
        let mut remaining = keep_blocks;

        for (idx, an) in chain.iter().enumerate() {
            if remaining == 0 {
                self.free_and_clear_anodes(&chain[idx..])?;
                return Ok(());
            } else if remaining < an.clustersize {
                // Partial: free tail blocks, shrink clustersize, set next=EOF
                for i in remaining..an.clustersize {
                    self.free_data_block(an.blocknr + i)?;
                }
                self.write_anode_fields(an.nr, remaining, an.blocknr, ANODE_EOF)?;
                self.free_and_clear_anodes(&chain[idx + 1..])?;
                return Ok(());
            } else {
                remaining -= an.clustersize;
                if remaining == 0 {
                    // This anode is the new tail — set next=EOF
                    self.write_anode_fields(an.nr, an.clustersize, an.blocknr, ANODE_EOF)?;
                    self.free_and_clear_anodes(&chain[idx + 1..])?;
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    /// Free all data blocks and clear anode slots for a slice of anodes.
    fn free_and_clear_anodes(&mut self, anodes: &[crate::ondisk::Anode]) -> Result<()> {
        for an in anodes {
            for i in 0..an.clustersize {
                self.free_data_block(an.blocknr + i)?;
            }
            self.clear_single_anode(an.nr)?;
        }
        Ok(())
    }

    /// Append a sub-chain to the tail of an existing anode chain.
    fn append_to_anode_chain(&mut self, head: u32, new_head: u32) -> Result<()> {
        let chain = self
            .vol
            .anodes
            .get_chain(head, self.vol.dev.as_ref(), &mut self.vol.cache)?;
        let tail = chain.last().ok_or(Error::AnodeNotFound(head))?;
        self.write_anode_fields(tail.nr, tail.clustersize, tail.blocknr, new_head)
    }

    /// Write the 3 fields of a single anode slot on disk.
    fn write_anode_fields(
        &mut self,
        anodenr: u32,
        clustersize: u32,
        blocknr: u32,
        next: u32,
    ) -> Result<()> {
        let split = self.vol.rootblock.is_splitted_anodes();
        let (seqnr, offset) = if split {
            (anodenr >> 16, anodenr & 0xFFFF)
        } else {
            (
                anodenr / self.anodes_per_block,
                anodenr % self.anodes_per_block,
            )
        };
        let blk_num = self.get_anode_block_nr(seqnr)?;
        let mut data = self.read_reserved_raw(blk_num)?;
        let base = ANODE_BLOCK_HEADER_SIZE + offset as usize * ANODE_SIZE;
        put_u32(&mut data, base, clustersize);
        put_u32(&mut data, base + 4, blocknr);
        put_u32(&mut data, base + 8, next);
        put_u32(&mut data, 4, self.datestamp);
        self.write_reserved(blk_num, &data)
    }

    /// Clear a single anode slot (set all 3 fields to 0).
    fn clear_single_anode(&mut self, anodenr: u32) -> Result<()> {
        self.write_anode_fields(anodenr, 0, 0, 0)?;
        // ART-311: its block has a free anode again — pfs3aio's FreeAnode sets
        // the block's bit (`anodes.c:495-500`).
        let seqnr = if self.vol.rootblock.is_splitted_anodes() {
            anodenr >> 16
        } else {
            anodenr / self.anodes_per_block
        };
        if let Some(full) = self.anode_block_full.get_mut(seqnr as usize) {
            *full = false;
        }
        Ok(())
    }

    /// Find a directory entry by name, returning (block_number, block_data, entry_offset).
    /// Returns Error::NotFound if the entry doesn't exist.
    fn find_dir_entry(&mut self, dir_anode: u32, name: &str) -> Result<(u32, Vec<u8>, usize)> {
        let chain =
            self.vol
                .anodes
                .get_chain(dir_anode, self.vol.dev.as_ref(), &mut self.vol.cache)?;
        for an in &chain {
            for i in 0..an.clustersize {
                let blk = an.blocknr + i;
                let data = self.read_reserved_raw(blk)?;
                if u16::from_be_bytes(data[0..2].try_into().unwrap()) != DBLKID {
                    continue;
                }
                let mut pos = DIR_BLOCK_HEADER_SIZE;
                while pos < self.resblocksize as usize {
                    let esize = data[pos] as usize;
                    if esize == 0 {
                        break;
                    }
                    let nlen = data[pos + 17] as usize;
                    let ename = crate::util::latin1_to_string(&data[pos + 18..pos + 18 + nlen]);
                    if crate::util::name_eq_ci(&ename, name) {
                        return Ok((blk, data, pos));
                    }
                    pos += esize;
                }
            }
        }
        Err(Error::NotFound(name.to_string()))
    }

    /// Update the fsize (and fsizex) of an existing directory entry in-place.
    fn update_dir_entry_size(&mut self, dir_anode: u32, name: &str, new_size: u64) -> Result<()> {
        let (blk, mut data, pos) = self.find_dir_entry(dir_anode, name)?;
        let esize = data[pos] as usize;
        let nlen = data[pos + 17] as usize;

        // Patch fsize (low 32 bits)
        put_u32(&mut data, pos + 6, new_size as u32);

        // Walk extra fields to patch fsizex if present
        let coff = pos + 18 + nlen;
        if coff < pos + esize {
            let clen = data[coff] as usize;
            let mut fp = coff + 1 + clen;
            if fp & 1 != 0 {
                fp += 1;
            }
            let end = pos + esize;
            if fp + 2 <= end {
                let flags = u16::from_be_bytes(data[fp..fp + 2].try_into().unwrap());
                fp += 2;
                'extra: {
                    if flags & 0x0001 != 0 {
                        fp += 4;
                        if fp > end {
                            break 'extra;
                        }
                    }
                    if flags & 0x0002 != 0 {
                        fp += 2;
                        if fp > end {
                            break 'extra;
                        }
                    }
                    if flags & 0x0004 != 0 {
                        fp += 2;
                        if fp > end {
                            break 'extra;
                        }
                    }
                    if flags & 0x0008 != 0 {
                        fp += 4;
                        if fp > end {
                            break 'extra;
                        }
                    }
                    if flags & 0x0010 != 0 {
                        fp += 4;
                        if fp > end {
                            break 'extra;
                        }
                    }
                    if flags & 0x0020 != 0 {
                        fp += 4;
                        if fp > end {
                            break 'extra;
                        }
                    }
                    if flags & 0x0040 != 0 && fp + 2 <= end {
                        put_u16(&mut data, fp, (new_size >> 32) as u16);
                    }
                }
            }
        }

        // Update datestamp
        let (cday, cmin, ctick) = self.entry_datestamp();
        put_u16(&mut data, pos + 10, cday);
        put_u16(&mut data, pos + 12, cmin);
        put_u16(&mut data, pos + 14, ctick);
        put_u32(&mut data, 4, self.next_datestamp());
        self.write_reserved(blk, &data)?;
        Ok(())
    }

    /// Update the protection bits of an existing directory entry in-place.
    pub fn update_dir_entry_protection(
        &mut self,
        dir_anode: u32,
        name: &str,
        protection: u8,
    ) -> Result<()> {
        self.guarded(|w| w.update_dir_entry_protection_impl(dir_anode, name, protection))
    }

    fn update_dir_entry_protection_impl(
        &mut self,
        dir_anode: u32,
        name: &str,
        protection: u8,
    ) -> Result<()> {
        let (blk, mut data, pos) = self.find_dir_entry(dir_anode, name)?;
        data[pos + 16] = protection;
        put_u32(&mut data, 4, self.next_datestamp());
        self.write_reserved(blk, &data)?;
        self.update_rootblock()
    }

    pub fn rename_in(
        &mut self,
        src_parent: u32,
        src_name: &str,
        dst_parent: u32,
        dst_name: &str,
    ) -> Result<()> {
        self.guarded(|w| w.rename_in_impl(src_parent, src_name, dst_parent, dst_name))
    }

    fn rename_in_impl(
        &mut self,
        src_parent: u32,
        src_name: &str,
        dst_parent: u32,
        dst_name: &str,
    ) -> Result<()> {
        self.check_name_len(dst_name)?;
        let entries = self.vol.list_dir_by_anode(src_parent)?;
        let entry = entries
            .iter()
            .find(|e| crate::util::name_eq_ci(&e.name, src_name))
            .ok_or_else(|| Error::NotFound(src_name.to_string()))?
            .clone();

        // If destination exists, delete it first
        if let Ok(dst_entries) = self.vol.list_dir_by_anode(dst_parent)
            && dst_entries
                .iter()
                .any(|e| crate::util::name_eq_ci(&e.name, dst_name))
        {
            self.delete_in(dst_parent, dst_name)?;
        }

        // Add entry in new location with new name
        self.add_dir_entry(
            dst_parent,
            dst_name,
            entry.entry_type,
            entry.anode,
            entry.file_size(),
            entry.protection,
        )?;
        // Remove from old location
        self.remove_dir_entry(src_parent, src_name)?;
        self.update_rootblock()
    }

    /// Delete a file or empty directory by name in a parent directory.
    pub fn delete_in(&mut self, parent_anode: u32, name: &str) -> Result<()> {
        self.guarded(|w| w.delete_in_impl(parent_anode, name))
    }

    fn delete_in_impl(&mut self, parent_anode: u32, name: &str) -> Result<()> {
        let entries = self.vol.list_dir_by_anode(parent_anode)?;
        let target = entries
            .iter()
            .find(|e| crate::util::name_eq_ci(&e.name, name))
            .ok_or_else(|| Error::NotFound(name.to_string()))?
            .clone();

        if target.is_dir() {
            let sub = self.vol.list_dir_by_anode(target.anode)?;
            if !sub.is_empty() {
                return Err(Error::NotEmpty);
            }
            self.free_anode_chain_reserved(target.anode)?;
            self.clear_anode_chain(target.anode)?;
        } else {
            // ART-318: pfs3aio's DeleteObject (`directory.c:1808-1830`). A file goes
            // to the deldir first when there is one; the data blocks are freed
            // either way, and the anodes are kept only for the deldir. Soft links
            // never go there.
            let kept = target.entry_type == ST_FILE && self.move_to_deldir(&target)?;
            self.free_data_blocks(target.anode)?;
            if !kept {
                self.clear_anode_chain(target.anode)?;
            }
        }
        self.remove_dir_entry(parent_anode, name)?;
        self.update_rootblock()
    }

    /// ART-318: put a deleted file into the deldir as pfs3aio's `AllocDeldirSlot`
    /// and `AddToDeldir` do (`directory.c:4489-4564`). The slot is
    /// `rext.deldirroving`, which advances modulo `deldirsize × 31`; a slot whose
    /// block is missing sends the roving pointer back to 0 and uses slot 0. A
    /// file still in the slot loses its anodes — its blocks were freed at its own
    /// delete. Returns `false` when the volume has no deldir. 0.1.3 took the first
    /// empty slot, wrote the name without its length byte, freed an evicted
    /// file's blocks a second time and swallowed every error.
    fn move_to_deldir(&mut self, entry: &crate::ondisk::DirEntry) -> Result<bool> {
        if !self.vol.rootblock.has_flag(MODE_DELDIR) || !self.vol.rootblock.has_extension() {
            return Ok(false);
        }
        let ext_blk = self.vol.rootblock.extension;
        let mut rext = self.read_reserved_raw(ext_blk)?;
        let u16_at = |b: &[u8], at: usize| u16::from_be_bytes([b[at], b[at + 1]]);
        let u32_at = |b: &[u8], at: usize| u32::from_be_bytes(b[at..at + 4].try_into().unwrap());
        // rext: deldirroving 0x34, deldirsize 0x36, deldir[32] 0x90 (`blocks.h:444-456`).
        let slots = usize::from(u16_at(&rext, 0x36)).min(MAXDELDIR + 1) * DELENTRIES_PER_BLOCK;
        if slots == 0 {
            return Ok(false);
        }
        let block_of =
            |rext: &[u8], slot: usize| u32_at(rext, 0x90 + (slot / DELENTRIES_PER_BLOCK) * 4);
        let roving = usize::from(u16_at(&rext, 0x34));
        let (slot, next_roving) = if roving < slots && block_of(&rext, roving) != 0 {
            (roving, (roving + 1) % slots)
        } else {
            (0, 0)
        };
        let dd_blk = block_of(&rext, slot);
        if dd_blk == 0 {
            return Ok(false);
        }
        let mut data = self.read_reserved_raw(dd_blk)?;
        if u16::from_be_bytes([data[0], data[1]]) != DELDIRID {
            return Err(Error::Corrupt(format!(
                "deldir block {dd_blk} is not a deldir block"
            )));
        }
        let off = DELDIR_HEADER_SIZE + (slot % DELENTRIES_PER_BLOCK) * DELDIR_ENTRY_SIZE;
        let evicted = u32_at(&data, off);
        if evicted != 0 {
            self.clear_anode_chain(evicted)?; // anodes only (`directory.c:4510-4518`)
        }
        self.write_deldir_entry(&mut data, off, entry);
        // The deldir block's date and rext.dd_creation* are now (`directory.c:4556-4560`).
        let (cday, cmin, ctick) = self.entry_datestamp();
        put_u16(&mut data, 0x1A, cday);
        put_u16(&mut data, 0x1C, cmin);
        put_u16(&mut data, 0x1E, ctick);
        put_u32(&mut data, 4, self.datestamp);
        self.write_reserved(dd_blk, &data)?;
        put_u16(&mut rext, 0x34, next_roving as u16);
        put_u16(&mut rext, 0x88, cday);
        put_u16(&mut rext, 0x8A, cmin);
        put_u16(&mut rext, 0x8C, ctick);
        self.write_reserved(ext_blk, &rext)?;
        Ok(true)
    }

    /// ART-318: pfs3aio's `deldirentry` (`blocks.h:368-379`) as `AddToDeldir` fills
    /// it (`directory.c:4544-4550`): `anodenr`, `fsize`, the entry's own creation
    /// date, a length byte and at most `DELENTRYFNSIZE - 1` name bytes. pfs3aio's
    /// makefile builds with `LARGE_FILE_SIZE=0` (`makefile:21`), where that is 17
    /// and a long name runs into what the struct calls `fsizex`; on a
    /// `MODE_LARGEFILE` volume, which only a `LARGE_FILE_SIZE` build mounts as one
    /// (`init.c:648`), the name stops at 15 and `fsizex` holds size bits 32-47
    /// (`SetDDFileSize`, `directory.c:3704-3714`).
    fn write_deldir_entry(&self, block: &mut [u8], off: usize, entry: &crate::ondisk::DirEntry) {
        put_u32(block, off, entry.anode);
        put_u32(block, off + 4, entry.file_size() as u32);
        put_u16(block, off + 8, entry.creation_day);
        put_u16(block, off + 10, entry.creation_minute);
        put_u16(block, off + 12, entry.creation_tick);
        for b in &mut block[off + 14..off + DELDIR_ENTRY_SIZE] {
            *b = 0;
        }
        let largefile = self.vol.rootblock.has_largefile();
        let fnsize = if largefile {
            DELENTRYFNSIZE_LARGE_FILE
        } else {
            DELENTRYFNSIZE
        };
        let name = entry.name.as_bytes();
        let len = name.len().min(fnsize - 1);
        block[off + 14] = len as u8;
        block[off + 15..off + 15 + len].copy_from_slice(&name[..len]);
        if largefile {
            put_u16(block, off + 30, (entry.file_size() >> 32) as u16);
        }
    }

    /// ART-318: whether `blk` is a data block the bitmap holds free.
    fn data_block_is_free(&self, blk: u32) -> bool {
        if blk < self.bitmapstart || blk >= self.vol.rootblock.disksize {
            return false;
        }
        let rel = blk - self.bitmapstart;
        let per_block = self.index_per_block * 32;
        let bit = rel % per_block;
        self.data_bm
            .get((rel / per_block) as usize)
            .and_then(|(_, longs)| longs.get((bit / 32) as usize))
            .is_some_and(|word| word & (0x8000_0000 >> (bit % 32)) != 0)
    }

    // ---- Data bitmap ----

    fn load_data_bitmap(&mut self) -> Result<()> {
        let no_bmb = {
            let bits_per_bmb = self.index_per_block * 32;
            // ART-315: the bitmap covers the data blocks, [bitmapstart, disksize),
            // not the whole disk (format.rs sizes it the same way).
            let data_blocks = self.vol.rootblock.disksize.saturating_sub(self.bitmapstart);
            // Cap at a reasonable maximum to prevent OOM on corrupt disksize
            data_blocks.div_ceil(bits_per_bmb).min(16384)
        };
        for seq in 0..no_bmb {
            if let Some(blk) = self.get_bitmap_block_nr(seq)? {
                let data = self.read_reserved_raw(blk)?;
                let mut longs = Vec::new();
                for i in 0..self.index_per_block as usize {
                    let off = 12 + i * 4;
                    if off + 4 <= data.len() {
                        longs.push(u32::from_be_bytes(data[off..off + 4].try_into().unwrap()));
                    }
                }
                self.data_bm.push((blk, longs));
            }
        }
        Ok(())
    }

    fn alloc_data_blocks(&mut self, count: u32) -> Result<Vec<u32>> {
        let mut allocated = Vec::new();
        for bm_idx in 0..self.data_bm.len() {
            let (_, ref mut longs) = self.data_bm[bm_idx];
            #[allow(clippy::needless_range_loop)]
            for li in 0..longs.len() {
                if longs[li] == 0 {
                    continue;
                }
                for bit in 0..32u32 {
                    if longs[li] & (0x8000_0000 >> bit) != 0 {
                        let data_blk = (bm_idx as u32)
                            .checked_mul(self.index_per_block)
                            .and_then(|v| v.checked_mul(32))
                            .and_then(|v| v.checked_add(li as u32 * 32 + bit))
                            .and_then(|v| v.checked_add(self.bitmapstart))
                            .ok_or_else(|| {
                                Error::Corrupt("block number overflow in bitmap".into())
                            })?;
                        // ART-315: a data block is below the partition's end. pfs3aio
                        // allocation.c:344 refuses `blocknr >= numblocks`; the bits past
                        // it are left set by pfs3aio's and hst-imager's formats.
                        if data_blk >= self.vol.rootblock.disksize {
                            continue; // skip out-of-range bitmap bits
                        }
                        longs[li] &= !(0x8000_0000 >> bit);
                        allocated.push(data_blk);
                        if allocated.len() as u32 == count {
                            self.write_data_bitmap_block(bm_idx)?;
                            self.vol.rootblock.blocksfree -= count;
                            return Ok(allocated);
                        }
                    }
                }
            }
            if !allocated.is_empty() {
                self.write_data_bitmap_block(bm_idx)?;
            }
        }
        Err(Error::DiskFull(format!(
            "not enough free blocks (need {})",
            count
        )))
    }

    fn write_data_bitmap_block(&mut self, bm_idx: usize) -> Result<()> {
        let (blk, ref longs) = self.data_bm[bm_idx];
        let mut data = self.read_reserved_raw(blk)?;
        put_u32(&mut data, 4, self.datestamp);
        for (i, &val) in longs.iter().enumerate() {
            put_u32(&mut data, 12 + i * 4, val);
        }
        self.write_reserved(blk, &data)
    }

    fn free_data_blocks(&mut self, anodenr: u32) -> Result<()> {
        let chain =
            self.vol
                .anodes
                .get_chain(anodenr, self.vol.dev.as_ref(), &mut self.vol.cache)?;
        for an in &chain {
            for i in 0..an.clustersize {
                self.free_data_block(an.blocknr + i)?;
            }
        }
        Ok(())
    }

    fn free_data_block(&mut self, blk: u32) -> Result<()> {
        // ART-315: a block outside [bitmapstart, disksize) has no bit of its own;
        // freeing one past the end would set a tail bit the allocator must never see.
        if blk < self.bitmapstart || blk >= self.vol.rootblock.disksize {
            return Ok(());
        }
        let rel = blk - self.bitmapstart;
        let bm_idx = rel / (self.index_per_block * 32);
        let remainder = rel % (self.index_per_block * 32);
        let li = (remainder / 32) as usize;
        let bit = remainder % 32;
        if (bm_idx as usize) < self.data_bm.len() {
            self.data_bm[bm_idx as usize].1[li] |= 0x8000_0000 >> bit;
            self.write_data_bitmap_block(bm_idx as usize)?;
            self.vol.rootblock.blocksfree += 1;
        }
        Ok(())
    }

    // ---- Reserved bitmap ----

    fn load_reserved_bitmap(&mut self) -> Result<()> {
        let rb = &self.vol.rootblock;
        let bs = self.vol.block_size() as usize;
        let cluster_size = rb.rblkcluster as usize * bs;
        let mut cluster = vec![0u8; cluster_size];
        self.vol.dev.read_blocks(
            self.firstreserved as u64,
            rb.rblkcluster as u32,
            &mut cluster,
        )?;
        let bm_off = bs + 12; // after rootblock sector + BM header
        self.res_bitmap.clear();
        for i in 0..=(self.numreserved / 32) {
            let off = bm_off + i as usize * 4;
            if off + 4 <= cluster.len() {
                self.res_bitmap.push(u32::from_be_bytes(
                    cluster[off..off + 4].try_into().unwrap(),
                ));
            }
        }
        Ok(())
    }

    fn alloc_reserved_block(&mut self) -> Result<u32> {
        for li in 0..self.res_bitmap.len() {
            if self.res_bitmap[li] == 0 {
                continue;
            }
            for bit in 0..32u32 {
                if self.res_bitmap[li] & (0x8000_0000 >> bit) != 0 {
                    let idx = li as u32 * 32 + bit;
                    self.res_bitmap[li] &= !(0x8000_0000 >> bit);
                    self.vol.rootblock.reserved_free -= 1;
                    return Ok(self.firstreserved + idx * self.rescluster);
                }
            }
        }
        Err(Error::DiskFull("out of reserved blocks".into()))
    }

    fn free_reserved_block(&mut self, blk: u32) -> Result<()> {
        let idx = (blk - self.firstreserved) / self.rescluster;
        let li = (idx / 32) as usize;
        let bit = idx % 32;
        if li < self.res_bitmap.len() {
            self.res_bitmap[li] |= 0x8000_0000 >> bit;
            self.vol.rootblock.reserved_free += 1;
        }
        Ok(())
    }

    // ---- Anode allocation ----

    /// ART-311: pfs3aio's `AllocAnode` (`anodes.c:366-471`). The search starts
    /// at the bitmap word holding the anode block the last allocation came from
    /// (`:389`) and skips blocks already found full (`:395-416`). A missing
    /// anode block is made where the search meets it — but a search that did
    /// not start at 0 starts over from 0 first (`:418-419,436-450`). 0.1.3
    /// rescanned 256 blocks from 0 for every anode.
    fn alloc_anode(&mut self, clustersize: u32, blocknr: u32, next: u32) -> Result<u32> {
        let split = self.vol.rootblock.is_splitted_anodes();
        let limit = self.anode_block_limit();
        if self.anode_block_full.len() < limit as usize {
            self.anode_block_full.resize(limit as usize, false);
        }
        let mut start = (self.anode_roving / 32) * 32;
        loop {
            let mut seqnr = start;
            while seqnr < limit {
                if self.anode_block_full[seqnr as usize] {
                    seqnr += 1;
                    continue;
                }
                let blk_num = self.get_anode_block_nr(seqnr)?;
                if blk_num == 0 {
                    if start != 0 {
                        break; // start over from 0 before making a block
                    }
                    // No anode block at this seqnr — allocate one
                    let new_blk = self.alloc_anode_block(seqnr)?;
                    // Now use the first usable slot in the new block
                    let mut data = self.read_reserved_raw(new_blk)?;
                    let anodenr = if split {
                        (seqnr << 16) | ANODE_USERFIRST
                    } else {
                        seqnr * self.anodes_per_block + ANODE_USERFIRST
                    };
                    let base = ANODE_BLOCK_HEADER_SIZE + ANODE_USERFIRST as usize * ANODE_SIZE;
                    put_u32(&mut data, base, clustersize);
                    put_u32(&mut data, base + 4, blocknr);
                    put_u32(&mut data, base + 8, next);
                    put_u32(&mut data, 4, self.datestamp);
                    self.write_reserved(new_blk, &data)?;
                    self.anode_roving = seqnr;
                    return Ok(anodenr);
                }
                let mut data = self.read_reserved_raw(blk_num)?;
                if u16::from_be_bytes(data[0..2].try_into().unwrap()) == ABLKID {
                    for offset in 0..self.anodes_per_block {
                        let anodenr = if split {
                            (seqnr << 16) | offset
                        } else {
                            seqnr * self.anodes_per_block + offset
                        };
                        if anodenr < ANODE_USERFIRST {
                            continue;
                        }
                        let base = ANODE_BLOCK_HEADER_SIZE + offset as usize * ANODE_SIZE;
                        let cs = u32::from_be_bytes(data[base..base + 4].try_into().unwrap());
                        let bn = u32::from_be_bytes(data[base + 4..base + 8].try_into().unwrap());
                        if cs == 0 && bn == 0 {
                            put_u32(&mut data, base, clustersize);
                            put_u32(&mut data, base + 4, blocknr);
                            put_u32(&mut data, base + 8, next);
                            put_u32(&mut data, 4, self.datestamp);
                            self.write_reserved(blk_num, &data)?;
                            self.anode_roving = seqnr;
                            return Ok(anodenr);
                        }
                    }
                }
                // No free anode in this block (pfs3aio clears its bit, `anodes.c:414-416`).
                self.anode_block_full[seqnr as usize] = true;
                seqnr += 1;
            }
            if start == 0 {
                return Err(Error::DiskFull("no free anode slots".into()));
            }
            start = 0;
        }
    }

    /// ART-311: the anode blocks this volume can address. pfs3aio passes an
    /// anode block's number as a UWORD (`anodes.c:582`), so 65 536 at most; the
    /// index levels bound it further — small mode's rootblock names
    /// `MAXSMALLINDEXNR + 1` index blocks (`anodes.c:740`), SUPERINDEX mode's
    /// extension `MAXSUPER + 1` super blocks (`anodes.c:851`). 0.1.3 searched 256.
    fn anode_block_limit(&self) -> u32 {
        let ipb = u64::from(self.index_per_block);
        let by_index = if self.vol.rootblock.is_large() {
            (MAXSUPER as u64 + 1) * ipb * ipb
        } else {
            (MAXSMALLINDEXNR as u64 + 1) * ipb
        };
        by_index.min(1 << 16) as u32
    }

    /// ART-311: the volume's `AnodeReader` copies the index roots when it is
    /// built (`anode.rs:41-42`). After this writer names a new index or super
    /// block it is rebuilt, so a chain walk later in this session resolves it.
    fn refresh_anode_reader(&mut self) {
        self.vol.anodes =
            crate::anode::AnodeReader::new(&self.vol.rootblock, self.vol.rootblock_ext.as_ref());
    }

    /// Allocate a new anode block and register it in the index.
    fn alloc_anode_block(&mut self, seqnr: u32) -> Result<u32> {
        let new_blk = self.alloc_reserved_block()?;

        // Initialize the anode block
        let mut data = vec![0u8; self.resblocksize as usize];
        put_u16(&mut data, 0, ABLKID);
        put_u32(&mut data, 4, self.next_datestamp());
        put_u32(&mut data, 8, seqnr);
        self.write_reserved(new_blk, &data)?;

        // Register in the index
        let ipb = self.index_per_block;
        let idx_nr = seqnr / ipb;
        let idx_off = seqnr % ipb;

        if self.vol.rootblock.is_large() {
            // Large mode: superindex → super block → index block → anode block
            let super_nr = seqnr / (ipb * ipb);
            let remainder = seqnr % (ipb * ipb);
            let idx_in_super = remainder / ipb;
            let off_in_idx = remainder % ipb;

            let mut super_blk = self
                .vol
                .rootblock_ext
                .as_ref()
                .and_then(|e| e.superindex.get(super_nr as usize).copied())
                .unwrap_or(0);
            if super_blk == 0 {
                // ART-311: a missing super block is made here, as pfs3aio's
                // NewSuperBlock does (`anodes.c:851-870`), up to MAXSUPER; the
                // rootblock extension names it when `update_rootblock` commits.
                if super_nr as usize > MAXSUPER || self.vol.rootblock_ext.is_none() {
                    return Err(Error::DiskFull("no superindex slot available".into()));
                }
                super_blk = self.alloc_reserved_block()?;
                let mut sdata = vec![0u8; self.resblocksize as usize];
                put_u16(&mut sdata, 0, SBLKID);
                put_u32(&mut sdata, 4, self.datestamp);
                put_u32(&mut sdata, 8, super_nr);
                self.write_reserved(super_blk, &sdata)?;
                if let Some(ext) = self.vol.rootblock_ext.as_mut() {
                    if ext.superindex.len() <= super_nr as usize {
                        ext.superindex.resize(super_nr as usize + 1, 0);
                    }
                    ext.superindex[super_nr as usize] = super_blk;
                }
                self.rext_dirty = true;
                self.refresh_anode_reader();
            }
            // ART-311: through this writer's own pending writes, like every index
            // read since ART-312. A super block made above exists only there, and
            // the reserved block it took may still hold a freed block's bytes.
            let sdata = self.read_reserved_raw(super_blk)?;
            let soff = INDEX_BLOCK_HEADER_SIZE + idx_in_super as usize * 4;
            let idx_blk = u32::from_be_bytes(sdata[soff..soff + 4].try_into().unwrap());
            if idx_blk == 0 {
                // Need to allocate an index block too
                let new_idx = self.alloc_reserved_block()?;
                let mut idata = vec![0u8; self.resblocksize as usize];
                put_u16(&mut idata, 0, IBLKID);
                put_u32(&mut idata, 4, self.next_datestamp());
                // ART-311: an index block's seqnr is its number across the volume,
                // `seqnr / indexperblock` (pfs3aio `anodes.c:592-595,767`). 0.1.3
                // wrote its position inside its super block — the same only under
                // super block 0.
                put_u32(&mut idata, 8, seqnr / ipb);
                let entry_off = INDEX_BLOCK_HEADER_SIZE + off_in_idx as usize * 4;
                put_u32(&mut idata, entry_off, new_blk);
                self.write_reserved(new_idx, &idata)?;
                // Update super block
                let mut sdata = self.read_reserved_raw(super_blk)?;
                put_u32(&mut sdata, soff, new_idx);
                put_u32(&mut sdata, 4, self.datestamp);
                self.write_reserved(super_blk, &sdata)?;
            } else {
                let mut idata = self.read_reserved_raw(idx_blk)?;
                let entry_off = INDEX_BLOCK_HEADER_SIZE + off_in_idx as usize * 4;
                put_u32(&mut idata, entry_off, new_blk);
                put_u32(&mut idata, 4, self.datestamp);
                self.write_reserved(idx_blk, &idata)?;
            }
        } else {
            // Small mode: rootblock.indexblocks[idx_nr] -> index block. ART-311:
            // a missing index block is made here, as pfs3aio's NewIndexBlock
            // does (`anodes.c:740-761`), up to MAXSMALLINDEXNR; the rootblock
            // names it when `update_rootblock` commits.
            if idx_nr as usize > MAXSMALLINDEXNR {
                return Err(Error::DiskFull("no index block slot available".into()));
            }
            let mut idx_blk = self
                .vol
                .rootblock
                .indexblocks
                .get(idx_nr as usize)
                .copied()
                .unwrap_or(0);
            if idx_blk == 0 {
                idx_blk = self.alloc_reserved_block()?;
                let mut idata = vec![0u8; self.resblocksize as usize];
                put_u16(&mut idata, 0, IBLKID);
                put_u32(&mut idata, 4, self.datestamp);
                put_u32(&mut idata, 8, idx_nr); // seqnr (`anodes.c:767`)
                self.write_reserved(idx_blk, &idata)?;
                let slots = &mut self.vol.rootblock.indexblocks;
                if slots.len() <= idx_nr as usize {
                    slots.resize(idx_nr as usize + 1, 0);
                }
                slots[idx_nr as usize] = idx_blk;
                self.refresh_anode_reader();
            }
            let mut idata = self.read_reserved_raw(idx_blk)?;
            let entry_off = INDEX_BLOCK_HEADER_SIZE + idx_off as usize * 4;
            put_u32(&mut idata, entry_off, new_blk);
            put_u32(&mut idata, 4, self.datestamp);
            self.write_reserved(idx_blk, &idata)?;
        }

        // Invalidate cache for the anode reader
        self.vol.cache.invalidate(new_blk as u64);
        Ok(new_blk)
    }

    fn create_anode_chain(&mut self, blocks: &[u32]) -> Result<u32> {
        let mut clusters = Vec::new();
        let mut i = 0usize;
        while i < blocks.len() {
            let start = blocks[i];
            let mut count = 1u32;
            while i + (count as usize) < blocks.len() && blocks[i + count as usize] == start + count
            {
                count += 1;
            }
            clusters.push((start, count));
            i += count as usize;
        }
        // Allocate in reverse so we can set next pointers
        let mut next_nr = 0u32;
        for &(start, count) in clusters.iter().rev() {
            next_nr = self.alloc_anode(count, start, next_nr)?;
        }
        Ok(next_nr)
    }

    fn clear_anode_chain(&mut self, anodenr: u32) -> Result<()> {
        let chain =
            self.vol
                .anodes
                .get_chain(anodenr, self.vol.dev.as_ref(), &mut self.vol.cache)?;
        for an in &chain {
            self.clear_single_anode(an.nr)?;
        }
        Ok(())
    }

    fn free_anode_chain_reserved(&mut self, anodenr: u32) -> Result<()> {
        let chain =
            self.vol
                .anodes
                .get_chain(anodenr, self.vol.dev.as_ref(), &mut self.vol.cache)?;
        for an in &chain {
            for i in 0..an.clustersize {
                self.free_reserved_block(an.blocknr + i)?;
            }
        }
        Ok(())
    }

    // ---- Directory entry ----

    fn add_dir_entry(
        &mut self,
        dir_anode: u32,
        name: &str,
        entry_type: i8,
        anode: u32,
        fsize: u64,
        protection: u8,
    ) -> Result<()> {
        let entry_bytes = self.build_dir_entry(name, entry_type, anode, fsize, protection);
        let chain =
            self.vol
                .anodes
                .get_chain(dir_anode, self.vol.dev.as_ref(), &mut self.vol.cache)?;

        let mut dir_parent = None;
        for an in &chain {
            for i in 0..an.clustersize {
                let blk = an.blocknr + i;
                let mut data = self.read_reserved_raw(blk)?;
                if u16::from_be_bytes(data[0..2].try_into().unwrap()) != DBLKID {
                    continue;
                }
                // ART-313: every block of a directory carries the same parent;
                // a new continuation block copies it (pfs3aio directory.c:3176,3204).
                if dir_parent.is_none() {
                    dir_parent = Some(u32::from_be_bytes(data[0x10..0x14].try_into().unwrap()));
                }
                // Find end of entries
                let mut pos = DIR_BLOCK_HEADER_SIZE;
                while pos < self.resblocksize as usize {
                    if data[pos] == 0 {
                        break;
                    }
                    pos += data[pos] as usize;
                }
                if pos + entry_bytes.len() < self.resblocksize as usize {
                    data[pos..pos + entry_bytes.len()].copy_from_slice(&entry_bytes);
                    if pos + entry_bytes.len() < self.resblocksize as usize {
                        data[pos + entry_bytes.len()] = 0;
                    }
                    put_u32(&mut data, 4, self.next_datestamp());
                    self.write_reserved(blk, &data)?;
                    return Ok(());
                }
            }
        }
        // No space — allocate new dir block and extend chain. Refused before
        // anything is allocated when the directory has no block to copy from.
        let parent = dir_parent.ok_or_else(|| {
            Error::Corrupt(format!("directory {dir_anode} has no directory block"))
        })?;
        let new_blk = self.alloc_reserved_block()?;
        let mut new_data = vec![0u8; self.resblocksize as usize];
        put_u16(&mut new_data, 0x00, DBLKID);
        put_u32(&mut new_data, 0x04, self.next_datestamp());
        put_u32(&mut new_data, 0x0C, dir_anode);
        put_u32(&mut new_data, 0x10, parent);
        new_data[DIR_BLOCK_HEADER_SIZE..DIR_BLOCK_HEADER_SIZE + entry_bytes.len()]
            .copy_from_slice(&entry_bytes);
        self.write_reserved(new_blk, &new_data)?;
        self.extend_anode_chain(dir_anode, new_blk)
    }

    fn remove_dir_entry(&mut self, dir_anode: u32, name: &str) -> Result<()> {
        let (blk, mut data, pos) = self.find_dir_entry(dir_anode, name)?;
        let esize = data[pos] as usize;
        let end = pos + esize;
        let remaining = self.resblocksize as usize - end;
        data.copy_within(end..end + remaining, pos);
        for b in &mut data[pos + remaining..pos + remaining + esize] {
            *b = 0;
        }
        put_u32(&mut data, 4, self.next_datestamp());
        self.write_reserved(blk, &data)?;
        Ok(())
    }

    fn build_dir_entry(
        &self,
        name: &str,
        entry_type: i8,
        anode: u32,
        fsize: u64,
        protection: u8,
    ) -> Vec<u8> {
        let name_bytes = name.as_bytes();
        let nlen = name_bytes.len().min(107);
        let fsizex = (fsize >> 32) as u16;
        let has_fsizex = fsizex > 0;
        let extra_bytes = if has_fsizex { 4 } else { 2 }; // flags(2) + optional fsizex(2)
        let mut base_size = 18 + nlen + 1 + extra_bytes;
        if base_size & 1 != 0 {
            base_size += 1;
        }
        let mut entry = vec![0u8; base_size];
        entry[0] = base_size as u8;
        entry[1] = entry_type as u8;
        put_u32(&mut entry, 2, anode);
        put_u32(&mut entry, 6, fsize as u32);
        let (cday, cmin, ctick) = self.entry_datestamp();
        put_u16(&mut entry, 10, cday);
        put_u16(&mut entry, 12, cmin);
        put_u16(&mut entry, 14, ctick);
        entry[16] = protection;
        entry[17] = nlen as u8;
        entry[18..18 + nlen].copy_from_slice(&name_bytes[..nlen]);
        entry[18 + nlen] = 0; // comment length
        let ef_off = if (18 + nlen + 1) & 1 != 0 {
            18 + nlen + 2
        } else {
            18 + nlen + 1
        };
        let flags: u16 = if has_fsizex { 0x0040 } else { 0 };
        put_u16(&mut entry, ef_off, flags);
        if has_fsizex {
            put_u16(&mut entry, ef_off + 2, fsizex);
        }
        entry
    }

    fn extend_anode_chain(&mut self, head_anode: u32, new_blk: u32) -> Result<()> {
        let new_anodenr = self.alloc_anode(1, new_blk, 0)?;
        self.append_to_anode_chain(head_anode, new_anodenr)
    }

    // ---- Rootblock update ----

    /// The commit point: flushes every pending reserved-block write, then
    /// writes the rootblock cluster last. ART-319: any error from in here
    /// locks the writer — `update_rootblock_body`'s own writes land in place,
    /// not copy-on-write (M5), so part of this operation's metadata (a
    /// mutated dir block, a new bitmap value) can already be on disk while
    /// the rootblock that would make it official is not; reloading and
    /// continuing over that could silently accept the half state as though
    /// it were the last commit, so this locks instead of guessing which of
    /// the three writes inside failed.
    fn update_rootblock(&mut self) -> Result<()> {
        match self.update_rootblock_body() {
            Ok(()) => Ok(()),
            Err(e) => {
                self.poisoned = true;
                Err(e)
            }
        }
    }

    fn update_rootblock_body(&mut self) -> Result<()> {
        // ART-311: a new super block is named by the rootblock extension
        // (`blocks.h:448`, superindex at 0x40); pfs3aio writes the extension
        // before the rootblock (`update.c:240-256,265-270`). Only the
        // superindex changes; every other extension field is left as read.
        if self.rext_dirty {
            let ext_blk = self.vol.rootblock.extension;
            let mut rext = self.read_reserved_raw(ext_blk)?;
            if let Some(ext) = &self.vol.rootblock_ext {
                for (i, &blk) in ext.superindex.iter().enumerate().take(MAXSUPER + 1) {
                    put_u32(&mut rext, 0x40 + i * 4, blk);
                }
            }
            self.write_reserved(ext_blk, &rext)?;
            self.rext_dirty = false;
        }

        // Flush all pending reserved block writes first
        self.flush_pending()?;

        // Write rootblock cluster (rootblock + reserved bitmap) last. M5
        // (final review): this is a commit *point*, not an atomic commit —
        // every pending write above already reached disk in place, and a new
        // super block named by the extension write just above can already be
        // on disk while the reserved bitmap on disk still marks it free.
        // Nothing here is new to this round: every reserved block ART writes
        // has worked this way since 0.1.3.
        let bs = self.vol.block_size() as usize;
        let rblkcluster = self.vol.rootblock.rblkcluster as u32;
        let cluster_size = rblkcluster as usize * bs;
        let mut cluster = vec![0u8; cluster_size];
        self.vol
            .dev
            .read_blocks(self.firstreserved as u64, rblkcluster, &mut cluster)?;

        // Update rootblock fields
        let ds = self.next_datestamp();
        put_u32(&mut cluster, RB_OFF_DATESTAMP, ds);
        put_u32(
            &mut cluster,
            RB_OFF_RESERVED_FREE,
            self.vol.rootblock.reserved_free,
        );
        put_u32(
            &mut cluster,
            RB_OFF_BLOCKSFREE,
            self.vol.rootblock.blocksfree,
        );

        // Update reserved bitmap in the same cluster
        let bm_off = bs + 12;
        for (i, &val) in self.res_bitmap.iter().enumerate() {
            let off = bm_off + i * 4;
            if off + 4 <= cluster.len() {
                put_u32(&mut cluster, off, val);
            }
        }

        // ART-311: in small mode the rootblock names every anode index block
        // (`blocks.h:133`, `idx.small.indexblocks`, after the five bitmap index
        // numbers); pfs3aio sets it in NewIndexBlock (`anodes.c:759-760`) and
        // writes the rootblock last (`update.c:265-270`). SUPERINDEX mode keeps
        // bitmap index numbers there instead.
        if !self.vol.rootblock.is_large() {
            let base = RB_OFF_INDEX_UNION + (MAXSMALLBITMAPINDEX + 1) * 4;
            for (i, &blk) in self
                .vol
                .rootblock
                .indexblocks
                .iter()
                .enumerate()
                .take(MAXSMALLINDEXNR + 1)
            {
                put_u32(&mut cluster, base + i * 4, blk);
            }
        }

        self.vol
            .dev
            .write_blocks(self.firstreserved as u64, rblkcluster, &cluster)?;
        self.vol.dev.flush()
    }

    /// Flush all pending reserved block writes to disk.
    fn flush_pending(&mut self) -> Result<()> {
        let bs = self.vol.block_size() as usize;
        let writes: Vec<(u32, Vec<u8>)> = self.pending_writes.drain(..).collect();
        for (blk, data) in &writes {
            write_reserved_blocks(
                self.vol.dev.as_ref(),
                *blk as u64,
                data,
                self.rescluster,
                bs,
            )?;
            self.vol.cache.invalidate(*blk as u64);
        }
        self.vol.dev.flush()
    }

    // ---- Helpers ----

    fn read_reserved_raw(&self, blk: u32) -> Result<Vec<u8>> {
        // Check pending writes first (most recent write wins)
        for (b, data) in self.pending_writes.iter().rev() {
            if *b == blk {
                return Ok(data.clone());
            }
        }
        let mut data = vec![0u8; self.resblocksize as usize];
        self.vol
            .dev
            .read_blocks(blk as u64, self.rescluster, &mut data)?;
        Ok(data)
    }

    fn write_reserved(&mut self, blk: u32, data: &[u8]) -> Result<()> {
        // Deduplicate: replace existing entry for same block
        if let Some(pos) = self.pending_writes.iter().position(|(b, _)| *b == blk) {
            self.pending_writes[pos].1 = data.to_vec();
        } else {
            self.pending_writes.push((blk, data.to_vec()));
        }
        self.vol.cache.invalidate(blk as u64);
        Ok(())
    }

    /// The anode block at `seqnr`, or 0 when none is allocated yet.
    ///
    /// ART-312: resolved through `read_reserved_raw`, so an index entry this
    /// writer set earlier in the same, still-uncommitted operation is seen.
    /// Going through the volume's cache read the device, where that entry is
    /// still 0 until `update_rootblock` flushes `pending_writes` — a second
    /// allocation in one operation then created the same anode block again
    /// and handed out the same anode number twice.
    fn get_anode_block_nr(&mut self, seqnr: u32) -> Result<u32> {
        let ipb = self.index_per_block;
        let entry = |data: &[u8], nr: u32| -> u32 {
            let off = INDEX_BLOCK_HEADER_SIZE + nr as usize * 4;
            data.get(off..off + 4)
                .map_or(0, |b| u32::from_be_bytes(b.try_into().unwrap()))
        };
        let idx_blk = if self.vol.rootblock.is_large() {
            let super_blk = self
                .vol
                .rootblock_ext
                .as_ref()
                .and_then(|e| e.superindex.get((seqnr / (ipb * ipb)) as usize).copied())
                .unwrap_or(0);
            if super_blk == 0 {
                return Ok(0);
            }
            entry(&self.read_reserved_raw(super_blk)?, (seqnr % (ipb * ipb)) / ipb)
        } else {
            self.vol
                .rootblock
                .indexblocks
                .get((seqnr / ipb) as usize)
                .copied()
                .unwrap_or(0)
        };
        if idx_blk == 0 {
            return Ok(0);
        }
        Ok(entry(&self.read_reserved_raw(idx_blk)?, seqnr % ipb))
    }

    fn get_bitmap_block_nr(&mut self, seqnr: u32) -> Result<Option<u32>> {
        self.vol
            .bitmap
            .get_bitmap_block(seqnr, self.vol.dev.as_ref(), &mut self.vol.cache)
    }

    fn split_path(&mut self, path: &str) -> Result<(u32, String)> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if parts.is_empty() {
            return Err(Error::NotFound("empty path".into()));
        }
        let filename = parts.last().unwrap().to_string();
        let mut parent = ANODE_ROOTDIR;
        for &part in &parts[..parts.len() - 1] {
            let entries = self.vol.list_dir_by_anode(parent)?;
            let dir = entries
                .iter()
                .find(|e| crate::util::name_eq_ci(&e.name, part) && e.is_dir())
                .ok_or_else(|| Error::NotFound(part.to_string()))?;
            parent = dir.anode;
        }
        Ok((parent, filename))
    }
}
