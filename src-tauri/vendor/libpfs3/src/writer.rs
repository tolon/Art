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
//! Modified by ART on 2026-09-15 (ART-319's disclosed gaps, third debt round):
//! `overwrite_file_in` is copy-on-write, and `rename_in` over an existing
//! destination is one commit;
//! Modified by ART on 2026-09-15 (ART-322): a `rename_in` destination that is
//! the source entry itself — a case-only rename in the same directory — is
//! renamed in place rather than deleted and recreated;
//! Modified by ART on 2026-09-15 (the third debt round's final review fix
//! wave): `rename_in`'s same-entry check compares the name too, and a hard
//! link found as the destination is refused (I1); the in-place rename writes
//! Latin-1 (M1); copy-on-write refuses a bitmap that offers the old file's
//! own block (M2);
//! Modified by ART on 2026-09-15 (ART-323): deleting a hard link removes only
//! its entry, a link pfs3aio made is refused, an object a link still names is
//! refused, and `create_hardlink` is refused;
//! Modified by ART on 2026-09-15 (ART-325/326/327): extra fields are read and
//! written in pfs3aio's layout through `ondisk::ExtraFields` (ART-325); a
//! creating call refuses a name its directory already holds (ART-326);
//! `rename_in` moves the source entry's own bytes and updates a moved link's
//! chain as pfs3aio's `MoveLink` does (ART-327); a directory-block walk is
//! bounded and refuses a malformed entry, and the delete-time link check
//! names what stopped it (the scoped re-review's items 5 and 4);
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
    /// `_in` twin, `write_file_in` calling `overwrite_file_in`) is harmless: a reload
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
        // Check if file already exists — if so, overwrite it.
        //
        // ART (ART-326): any other entry under the name — a directory, a soft
        // link, a hard link — is refused `AlreadyExists` before anything is
        // written, as pfs3aio refuses to create over a name its directory
        // holds (`directory.c:1666-1671,2552-2557,2665-2670`). 0.1.3 added a
        // second entry. pfs3aio's `NewFile` follows a hard link to its object
        // (`directory.c:1513-1514`); this writer refuses one instead. A lookup
        // that fails is returned, not taken for "not there".
        match self.find_dir_entry(parent_anode, name) {
            Ok((_, entry_data, pos)) => {
                // `find_dir_entry` returns an entry whose header lies in the block.
                let entry_type = entry_data.get(pos + 1).map_or(0, |&t| t as i8);
                if entry_type == ST_FILE || entry_type == ST_ROLLOVERFILE {
                    let file_anode = entry_data
                        .get(pos + 2..pos + 6)
                        .map_or(0, |b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
                    return self.overwrite_file_in(parent_anode, name, file_anode, data);
                }
                return Err(Error::AlreadyExists(name.to_string()));
            }
            Err(Error::NotFound(_)) => {}
            Err(e) => return Err(e),
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
        self.refuse_existing(parent_anode, name)?;
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
        self.refuse_existing(parent_anode, name)?;
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
    ///
    /// ART (2026-09-15, ART-323): **refused**, with
    /// `Error::HardLinkNotWritten`, before anything is read or written.
    /// 0.1.3 added an `ST_LINKFILE` entry holding the linked object's anode.
    /// pfs3aio's `CreateLink` gives the link an anode of its own — a link
    /// node whose clustersize is the object's directory and blocknr the
    /// link's — puts the object's anode in the link's `link` extra field, and
    /// adds the node to the chain of links headed by the object's own `link`
    /// field (`directory.c:2672-2748`, `tonioni/pfs3aio` `211f7f0`). This
    /// writer does not write that, and a link that is not pfs3aio's is not a
    /// link under pfs3aio.
    pub fn create_hardlink(&mut self, path: &str, target_anode: u32) -> Result<()> {
        self.guarded(|w| w.create_hardlink_impl(path, target_anode))
    }

    fn create_hardlink_impl(&mut self, path: &str, _target_anode: u32) -> Result<()> {
        Err(Error::HardLinkNotWritten(path.to_string()))
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

    /// Overwrite an existing file's data, reusing its anode.
    /// The anode number stays stable — safe for FUSE inode caching.
    ///
    /// ART (2026-09-15, ART-319's first disclosed gap): **copy-on-write.**
    /// The new content goes only into newly allocated free blocks; the file's
    /// head anode is rewritten to describe them, the old chain's other
    /// anodes are cleared and its data blocks freed, and the directory
    /// entry's size is set — all staged, all made true by the one
    /// `update_rootblock` commit. Until that commit nothing of the old file
    /// has been written, so an error anywhere before it is discarded by
    /// `guarded` and the device still holds the old file. **The cost:** the
    /// whole new content needs free space of its own. An overwrite that would
    /// fit only by reusing the file's own blocks is refused with
    /// `Error::DiskFull`, before any block is written, and the file keeps its
    /// old content. 0.1.3 wrote the new data over the old blocks first.
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

        // The old chain, as the last commit left it.
        let old_chain =
            self.vol
                .anodes
                .get_chain(file_anode, self.vol.dev.as_ref(), &mut self.vol.cache)?;

        // ART (ART-319's first gap): copy-on-write. The old file's blocks are
        // still allocated here, so none of them can be handed out; a volume
        // without room for the whole new content is refused now, before any
        // block is written.
        let new_blocks = self.alloc_data_blocks(new_blocks_needed)?;
        // Final review fix wave (M2): that holds only for a consistent
        // bitmap. One that marks an old block free hands it out here, and the
        // write below would overwrite the old file before any commit, then
        // the commit would free a block the new chain uses. Refused as
        // corruption before the first write; `guarded` discards the
        // allocation.
        if let Some(&shared) = new_blocks.iter().find(|&&blk| {
            old_chain
                .iter()
                .any(|an| blk >= an.blocknr && blk - an.blocknr < an.clustersize)
        }) {
            return Err(Error::Corrupt(format!(
                "the data bitmap marks block {shared} free, but file anode {file_anode} still uses it"
            )));
        }
        let mut sector = vec![0u8; bs];
        for (i, &blk) in new_blocks.iter().enumerate() {
            sector.fill(0);
            let start = i * bs;
            if start < data.len() {
                let end = (start + bs).min(data.len());
                sector[..end - start].copy_from_slice(&data[start..end]);
            }
            self.vol.dev.write_block(blk as u64, &sector)?;
        }
        self.vol.dev.flush()?; // data durable before metadata

        // The new chain under the same head anode: the extents after the first
        // get anodes of their own, allocated while the old chain's are still
        // taken, so none of them can be one this commit is about to clear.
        let extents = block_extents(&new_blocks);
        let mut next = ANODE_EOF;
        for &(start, count) in extents.iter().skip(1).rev() {
            next = self.alloc_anode(count, start, next)?;
        }
        // Only now, after the allocation, are the old blocks freed — freed
        // first, this very call could have taken them back for the new data.
        for an in &old_chain {
            for i in 0..an.clustersize {
                self.free_data_block(an.blocknr + i)?;
            }
        }
        for an in old_chain.iter().skip(1) {
            self.clear_single_anode(an.nr)?;
        }
        let (start, count) = extents[0];
        self.write_anode_fields(file_anode, count, start, next)?;

        // Update file size in the directory entry
        self.update_dir_entry_size(parent_anode, name, data.len() as u64)?;
        self.update_rootblock()
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
    ///
    /// ART (the scoped re-review's item 5): every entry is walked through
    /// `dir_entry_at`, so the entry returned holds its whole header and name
    /// inside the block (`pos + size <= data.len()`, `18 + nlen <= size`),
    /// and a malformed entry before it is refused. 0.1.3 indexed `pos + 17`
    /// and the name without a bound.
    fn find_dir_entry(&mut self, dir_anode: u32, name: &str) -> Result<(u32, Vec<u8>, usize)> {
        let chain =
            self.vol
                .anodes
                .get_chain(dir_anode, self.vol.dev.as_ref(), &mut self.vol.cache)?;
        for an in &chain {
            for i in 0..an.clustersize {
                let blk = an.blocknr + i;
                let data = self.read_reserved_raw(blk)?;
                if data.get(0..2) != Some(&DBLKID.to_be_bytes()[..]) {
                    continue;
                }
                let mut pos = DIR_BLOCK_HEADER_SIZE;
                while let Some((esize, nlen)) = Self::dir_entry_at(&data, blk, pos)? {
                    let ename = crate::util::latin1_to_string(
                        data.get(pos + 18..pos + 18 + nlen).unwrap_or(&[]),
                    );
                    if crate::util::name_eq_ci(&ename, name) {
                        return Ok((blk, data, pos));
                    }
                    pos += esize;
                }
            }
        }
        Err(Error::NotFound(name.to_string()))
    }

    /// ART (the scoped re-review's item 5): the entry at `pos` of directory
    /// block `blk`, bounded. `Ok(None)` at the end of the block's entries — a
    /// size byte of 0, or the end of the block. `Ok(Some((size, nlen)))` for
    /// an entry whose header and name lie inside it and inside the block.
    /// Anything else is refused as corruption, naming the block and the
    /// offset: an entry smaller than its own header, one that runs past the
    /// block, or one whose name runs past the entry.
    fn dir_entry_at(data: &[u8], blk: u32, pos: usize) -> Result<Option<(usize, usize)>> {
        let Some(&size) = data.get(pos) else {
            return Ok(None);
        };
        if size == 0 {
            return Ok(None);
        }
        let size = usize::from(size);
        let malformed = || {
            Error::Corrupt(format!(
                "directory block {blk} holds a malformed entry at offset {pos} (size {size}) — \
                 check this volume with a PFS3 repair tool before writing to it"
            ))
        };
        if size < 18 || pos + size > data.len() {
            return Err(malformed());
        }
        let nlen = usize::from(data.get(pos + 17).copied().ok_or_else(malformed)?);
        if 18 + nlen > size {
            return Err(malformed());
        }
        Ok(Some((size, nlen)))
    }

    /// ART (ART-326): `AlreadyExists` when `dir` holds an entry under `name`,
    /// compared as pfs3aio compares names (`util::name_eq_ci`) — pfs3aio's
    /// `SearchInDir`, then `ERROR_OBJECT_EXISTS`, in `NewDir`
    /// (`directory.c:1666-1671`), `CreateSoftLink` (`:2552-2557`),
    /// `CreateLink` (`:2665-2670`) and `CreateRollover` (`:2873-2878`) —
    /// before anything is allocated or written. 0.1.3 looked no name up.
    fn refuse_existing(&mut self, dir: u32, name: &str) -> Result<()> {
        match self.find_dir_entry(dir, name) {
            Ok(_) => Err(Error::AlreadyExists(name.to_string())),
            Err(Error::NotFound(_)) => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// ART (ART-327): anode `anodenr`'s three fields `(clustersize, blocknr,
    /// next)`, read through this writer's pending writes at the address
    /// `write_anode_fields` writes, and bounded: a slot that is not there is
    /// `Error::AnodeNotFound`.
    fn read_anode_fields(&mut self, anodenr: u32) -> Result<(u32, u32, u32)> {
        let (seqnr, offset) = if self.vol.rootblock.is_splitted_anodes() {
            (anodenr >> 16, anodenr & 0xFFFF)
        } else {
            (
                anodenr / self.anodes_per_block,
                anodenr % self.anodes_per_block,
            )
        };
        let blk_num = self.get_anode_block_nr(seqnr)?;
        if blk_num == 0 {
            return Err(Error::AnodeNotFound(anodenr));
        }
        let data = self.read_reserved_raw(blk_num)?;
        let base = ANODE_BLOCK_HEADER_SIZE + offset as usize * ANODE_SIZE;
        let field = |at: usize| {
            data.get(at..at + 4)
                .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
                .ok_or(Error::AnodeNotFound(anodenr))
        };
        Ok((field(base)?, field(base + 4)?, field(base + 8)?))
    }

    /// Update the fsize (and fsizex) of an existing directory entry in-place.
    fn update_dir_entry_size(&mut self, dir_anode: u32, name: &str, new_size: u64) -> Result<()> {
        let (blk, mut data, pos) = self.find_dir_entry(dir_anode, name)?;
        let esize = usize::from(data.get(pos).copied().unwrap_or(0));

        // ART (ART-325): `fsizex` is found where pfs3aio's `GetExtraFields`
        // finds it (`directory.c:3719-3731`), and an entry whose extra fields
        // do not fit is refused before anything is patched. It is patched only
        // where the entry already carries it: adding it grows the entry
        // ([ART-324], not fixed). 0.1.3 walked a layout of its own.
        let fsizex_at = ExtraFields::word_offsets(data.get(pos..pos + esize).unwrap_or(&[]))
            .map_err(|m| {
                Error::Corrupt(format!(
                    "the directory entry for '{name}' has extra fields that do not fit it (flags \
                     0x{:04x}) — check this volume with a PFS3 repair tool before writing to it",
                    m.flags
                ))
            })?[EXTRA_FSIZEX_WORD];

        // Patch fsize (low 32 bits)
        put_u32(&mut data, pos + 6, new_size as u32);
        if let Some(at) = fsizex_at {
            put_u16(&mut data, pos + at, (new_size >> 32) as u16);
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

    /// Move and/or rename an entry; an existing destination is deleted first
    /// — unless that destination is the entry being renamed (ART-322).
    ///
    /// ART (2026-09-15, ART-319's second disclosed gap): **one commit.** The
    /// destination's delete — to the deldir, exactly as `delete_in` sends a
    /// file there — the new entry and the old entry's removal are staged
    /// together and made true by one `update_rootblock`, so an error anywhere
    /// is discarded by `guarded` and the destination is still there. 0.1.3
    /// deleted the destination with `delete_in`, its own commit, first.
    ///
    /// ART (2026-09-15, ART-322): a destination that is the source itself —
    /// found in the same parent, under a name `name_eq_ci` matches to the
    /// source entry's own, with the same anode, which is true of a case-only
    /// rename in the same directory — is renamed in place instead of deleted
    /// and recreated: its anode, size, protection, dates and comment
    /// survive, and only the name bytes change. The identical name, same
    /// case too, is a no-op success.
    ///
    /// ART (2026-09-15, the third debt round's final review fix wave, I1):
    /// the name is part of that identity. A hard link stores its target's
    /// anode, so with the anode alone a link in the same directory was taken
    /// for the source. A found destination that is a hard link, or that
    /// shares the source's anode without being the source, is refused with
    /// `Error::AlreadyExists` before anything is staged: this writer's delete
    /// of a hard link frees the file it names (ART-323), and pfs3aio's
    /// `RenameAndMove` refuses every found destination that is not the
    /// source's own direntry (`ERROR_OBJECT_EXISTS`, `directory.c:2064-2076`,
    /// `tonioni/pfs3aio` `211f7f0`).
    ///
    /// ART (2026-09-15, ART-327): the entry that moves is **the source entry's
    /// own bytes with only the name changed**, as `RenameAndMove` builds it
    /// (`directory.c:2097-2124`): its type, anode, size, dates, protection,
    /// comment and every extra field — `link`, `uid`/`gid`, protection bits
    /// 8-31, rollover fields, `fsizex` — are the source's. Moved to another
    /// directory, a hard link's own node is pointed at its new directory
    /// (`blocknr`) and every node of a linked object's chain at the object's
    /// (`clustersize`), as `MoveLink` does (`directory.c:2136-2142,3993-4028`).
    /// The source entry and the chain are read first, and a chain that cannot
    /// be walked to its end is refused `Error::EntryNotMoved` before anything
    /// is staged. 0.1.3 rebuilt the entry with no comment, "now" as its date
    /// and no extra fields.
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

        // ART-322: a destination that names the very entry being renamed is
        // not "an existing destination" to delete. `name_eq_ci` makes a
        // case-only rename in the same directory find the source itself this
        // way; deleting it deleted the file and reported `Ok`. Rename it in
        // place instead, so its anode, size, protection, dates and comment
        // all survive and only the name bytes change. An identical name (the
        // same case too) is a no-op success.
        //
        // Final review fix wave (I1): "the very entry" is the same parent, a
        // found name `name_eq_ci` matches to the source entry's own — names
        // in one directory are unique under that compare — and the same
        // anode. The anode alone took a hard link in the same directory for
        // the source, since a link stores its target's anode.
        let mut replace_dst = false;
        if let Ok(dst_entries) = self.vol.list_dir_by_anode(dst_parent)
            && let Some(dst_entry) = dst_entries
                .iter()
                .find(|e| crate::util::name_eq_ci(&e.name, dst_name))
        {
            if dst_parent == src_parent
                && crate::util::name_eq_ci(&dst_entry.name, &entry.name)
                && dst_entry.anode == entry.anode
            {
                if entry.name == dst_name {
                    return Ok(());
                }
                return self.rename_dir_entry_in_place(src_parent, src_name, dst_name);
            }
            // Final review fix wave (I1): a hard link cannot be replaced —
            // `delete_in_no_commit` on one frees the file it names (ART-323) —
            // and neither can a different entry sharing the source's anode (a
            // link and the file it names are one file). Refused before
            // anything is staged. pfs3aio refuses every found destination
            // that is not the source's own direntry (`RenameAndMove`,
            // `directory.c:2064-2076`).
            if dst_entry.is_hardlink() || dst_entry.anode == entry.anode {
                return Err(Error::AlreadyExists(dst_name.to_string()));
            }
            replace_dst = true;
        }

        // ART (ART-327): what moves, and the link nodes it updates, are read
        // — and refused if they cannot be — before anything is staged.
        let (src_blk, src_block, src_pos) = self.find_dir_entry(src_parent, src_name)?;
        let moved = self.moved_entry(&src_block, src_blk, src_pos, dst_name)?;
        let link_moves = if dst_parent == src_parent {
            Vec::new()
        } else {
            self.link_moves(&moved, &entry.name, dst_parent)?
        };

        if replace_dst {
            // ART (ART-319's second gap): staged, not committed on its own.
            self.delete_in_no_commit(dst_parent, dst_name)?;
        }
        // The new entry is added before the old one is removed, as pfs3aio's
        // `RenameAcrossDirs` does (`directory.c:3028-3044`).
        self.add_dir_entry_bytes(dst_parent, &moved)?;
        self.remove_dir_entry(src_parent, src_name)?;
        for (nr, clustersize, blocknr, next) in link_moves {
            self.write_anode_fields(nr, clustersize, blocknr, next)?;
        }
        self.update_rootblock()
    }

    /// ART (ART-327): the entry pfs3aio's `RenameAndMove` builds for a rename
    /// (`directory.c:2097-2124`, `tonioni/pfs3aio` `211f7f0`) from the entry
    /// at `pos` of directory block `blk`: its header through `protection`
    /// copied, `new_name`, its comment copied, and — on a
    /// `MODE_DIR_EXTENSION` volume, as `g->dirextension` gates it there — its
    /// extra fields copied to the offset the new name puts them at. The name
    /// is written as Latin-1, what the reader decodes and an Amiga writes; a
    /// name holding a character above U+00FF is written as its UTF-8 bytes,
    /// the way `build_dir_entry` writes every name (ART-328).
    fn moved_entry(&self, block: &[u8], blk: u32, pos: usize, new_name: &str) -> Result<Vec<u8>> {
        let malformed = || {
            Error::Corrupt(format!(
                "directory block {blk} holds a malformed entry at offset {pos} (its comment runs \
                 past it) — check this volume with a PFS3 repair tool before writing to it"
            ))
        };
        let (size, nlen) = Self::dir_entry_at(block, blk, pos)?.ok_or_else(malformed)?;
        let src = block.get(pos..pos + size).ok_or_else(malformed)?;
        let clen = usize::from(*src.get(18 + nlen).ok_or_else(malformed)?);
        let comment = src.get(19 + nlen..19 + nlen + clen).ok_or_else(malformed)?;
        let name: Vec<u8> = new_name
            .chars()
            .map(|c| u8::try_from(u32::from(c)).ok())
            .collect::<Option<Vec<u8>>>()
            .unwrap_or_else(|| new_name.as_bytes().to_vec());
        let mut entry = Vec::with_capacity(size + name.len());
        entry.extend_from_slice(src.get(..17).ok_or_else(malformed)?);
        entry.push(name.len() as u8);
        entry.extend_from_slice(&name);
        entry.push(clen as u8);
        entry.extend_from_slice(comment);
        entry.resize(extra_fields_offset(name.len(), clen), 0);
        if self.vol.rootblock.has_flag(MODE_DIR_EXTENSION) {
            entry.extend_from_slice(src.get(extra_fields_offset(nlen, clen)..).unwrap_or(&[]));
        }
        let fixed = entry.len() - name.len();
        entry[0] = u8::try_from(entry.len()).map_err(|_| Error::NameTooLong {
            name: new_name.to_string(),
            len: name.len(),
            max: 255usize.saturating_sub(fixed),
        })?;
        Ok(entry)
    }

    /// ART (ART-327): the anode writes `(anode, clustersize, blocknr, next)`
    /// that pfs3aio's `MoveLink` makes when `entry` — the entry `moved_entry`
    /// built — moves to directory `new_dir` (`directory.c:3993-4028`): none
    /// without a `link` field; for a hard link, its own node (the entry's
    /// anode) with `blocknr` set to `new_dir`; for a linked object, each node
    /// of the chain its `link` field heads with `clustersize` set to
    /// `new_dir`. Read now, written after the entry has moved. A node that
    /// cannot be read, or a chain that loops, is refused
    /// `Error::EntryNotMoved` naming the anode.
    fn link_moves(
        &mut self,
        entry: &[u8],
        name: &str,
        new_dir: u32,
    ) -> Result<Vec<(u32, u32, u32, u32)>> {
        let refuse = |reason: String| Error::EntryNotMoved {
            name: name.to_string(),
            reason,
        };
        let link = self.entry_link_field(entry).map_err(|m| {
            refuse(format!(
                "its directory entry has extra fields that do not fit it (flags 0x{:04x})",
                m.flags
            ))
        })?;
        if link == 0 {
            return Ok(Vec::new());
        }
        let entry_type = entry.get(1).map_or(0, |&t| t as i8);
        if entry_type == ST_LINKFILE || entry_type == ST_LINKDIR {
            let node = entry
                .get(2..6)
                .map_or(0, |b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
            let (clustersize, _, next) = self.read_anode_fields(node).map_err(|e| {
                refuse(format!("its hard-link node, anode {node}, could not be read ({e})"))
            })?;
            return Ok(vec![(node, clustersize, new_dir, next)]);
        }
        let mut moves = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut nr = link;
        while nr != 0 {
            if !seen.insert(nr) {
                return Err(refuse(format!(
                    "its chain of hard links loops back to anode {nr}"
                )));
            }
            let (_, blocknr, next) = self.read_anode_fields(nr).map_err(|e| {
                refuse(format!(
                    "its chain of hard links names anode {nr}, which could not be read ({e})"
                ))
            })?;
            moves.push((nr, new_dir, blocknr, next));
            nr = next;
        }
        Ok(moves)
    }

    /// ART-322: rewrite a directory entry's name bytes in place, leaving its
    /// anode, size, protection, dates, comment and every extra field
    /// untouched. Only reached for a destination that is the source entry
    /// itself — same parent, a `name_eq_ci` match to the entry's own name,
    /// same anode.
    ///
    /// Final review fix wave (M1): the name is written as **Latin-1**, one
    /// byte a character, the encoding `find_dir_entry` and the reader decode
    /// it with (`util::latin1_to_string`) and the one an Amiga writes. It
    /// used to be the new name's UTF-8 bytes, so a case-only rename of a
    /// non-ASCII name ("Äa", `C4 61`, to "ÄA") measured three bytes against
    /// two and refused a healthy volume as corrupt. `name_eq_ci` folds only
    /// characters at or below U+00FF onto others there (ASCII, and since
    /// ART-326 Latin-1 letters too), over names decoded from those same
    /// bytes, so the new name always has one character at or below U+00FF
    /// for each stored byte; `Error::Corrupt` is a safety net for the entry no longer
    /// holding what was just found in it, never expected to fire, rather
    /// than indexing past the entry.
    fn rename_dir_entry_in_place(
        &mut self,
        dir_anode: u32,
        old_name: &str,
        new_name: &str,
    ) -> Result<()> {
        let (blk, mut data, pos) = self.find_dir_entry(dir_anode, old_name)?;
        let old_nlen = data[pos + 17] as usize;
        let new_name_bytes: Option<Vec<u8>> = new_name
            .chars()
            .map(|c| u8::try_from(u32::from(c)).ok())
            .collect();
        let new_name_bytes = match new_name_bytes {
            Some(bytes) if bytes.len() == old_nlen => bytes,
            _ => {
                return Err(Error::Corrupt(format!(
                    "the directory entry found for '{old_name}' does not hold a name that \
                     '{new_name}' differs from only in letter case"
                )));
            }
        };
        let new_nlen = new_name_bytes.len();
        data[pos + 18..pos + 18 + new_nlen].copy_from_slice(&new_name_bytes);
        put_u32(&mut data, 4, self.next_datestamp());
        self.write_reserved(blk, &data)?;
        self.update_rootblock()
    }

    /// Delete a file or empty directory by name in a parent directory.
    ///
    /// ART (2026-09-15, ART-323), hard links, before anything is staged:
    /// - **A hard link in 0.1.3's shape** (`ST_LINKFILE`/`ST_LINKDIR`, no
    ///   `link` extra field, the linked object's anode in its entry) loses its
    ///   entry and nothing else. 0.1.3 freed the data blocks and anodes of the
    ///   object the link names, which stayed listed and read back empty.
    ///   pfs3aio's `DeleteObject` likewise sends a link to `DeleteLink`, which
    ///   never frees the object (`directory.c:1778-1783`).
    /// - **A hard link pfs3aio made** (its `link` field set) is refused with
    ///   `Error::Pfs3aioLinkNotDeleted`: `DeleteLink` also takes the link's
    ///   node out of the object's chain of links, rewriting the object's entry
    ///   when the link is the head or the previous node otherwise
    ///   (`directory.c:3835-3895`), and this writer does not.
    /// - **An object hard links name** is refused with `Error::HasHardLinks`:
    ///   a link entry anywhere on the volume naming its anode, in either
    ///   shape, or its own `link` field (the head of its chain). pfs3aio
    ///   promotes the first link to be the object instead (`RemapLinks`,
    ///   `directory.c:1795-1799,3903-3965`), and this writer does not. **The
    ///   cost:** every directory on the volume is read on each delete of
    ///   anything that is not a link.
    ///
    /// Each refusal goes through `guarded`, so it discards back to the last
    /// commit.
    pub fn delete_in(&mut self, parent_anode: u32, name: &str) -> Result<()> {
        self.guarded(|w| w.delete_in_impl(parent_anode, name))
    }

    fn delete_in_impl(&mut self, parent_anode: u32, name: &str) -> Result<()> {
        self.delete_in_no_commit(parent_anode, name)?;
        self.update_rootblock()
    }

    /// Delete without committing — caller must call update_rootblock(). ART
    /// (ART-319's second gap): shared by `delete_in` and `rename_in`, so a
    /// rename's destination goes exactly where a delete sends it.
    fn delete_in_no_commit(&mut self, parent_anode: u32, name: &str) -> Result<()> {
        let entries = self.vol.list_dir_by_anode(parent_anode)?;
        let target = entries
            .iter()
            .find(|e| crate::util::name_eq_ci(&e.name, name))
            .ok_or_else(|| Error::NotFound(name.to_string()))?
            .clone();

        // ART-323: a hard link is only its entry; an object links name is not
        // deleted. See `delete_in`.
        let (_, block, pos) = self.find_dir_entry(parent_anode, name)?;
        let size = usize::from(block.get(pos).copied().unwrap_or(0));
        // The scoped re-review's item 4: an entry whose own extra fields do
        // not fit it is refused naming it, not with an offset.
        let link_field = self
            .entry_link_field(block.get(pos..pos + size).unwrap_or(&[]))
            .map_err(|m| Error::EntryNotDeleted {
                name: target.name.clone(),
                reason: format!(
                    "its directory entry has extra fields that do not fit it (flags 0x{:04x})",
                    m.flags
                ),
            })?;
        if target.is_hardlink() {
            if link_field != 0 {
                return Err(Error::Pfs3aioLinkNotDeleted(target.name));
            }
            return self.remove_dir_entry(parent_anode, name);
        }
        if let Some(links) = self.links_naming(&target.name, target.anode, link_field)? {
            return Err(Error::HasHardLinks {
                name: target.name,
                links,
            });
        }

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
        self.remove_dir_entry(parent_anode, name)
    }

    /// ART-323: the `link` extra field of `entry` — one whole entry — as
    /// pfs3aio's `GetExtraFields` reads it (`directory.c:3719-3731`).
    /// ART (ART-325): through `ExtraFields::read`, the one reading of that
    /// layout `DirEntry::parse` now shares; this used to be a reading of its
    /// own. Only a `MODE_DIR_EXTENSION` volume has extra fields (pfs3aio's
    /// `CreateLink` refuses without them, `directory.c:2628-2632`). An entry
    /// whose extra fields do not fit it is returned as such, for the caller
    /// to refuse naming it.
    fn entry_link_field(&self, entry: &[u8]) -> std::result::Result<u32, MalformedExtraFields> {
        if !self.vol.rootblock.has_flag(MODE_DIR_EXTENSION) {
            return Ok(0);
        }
        ExtraFields::read(entry).map(|x| x.link)
    }

    /// ART-323: what names `anode` as a hard link's object, or `None`. Every
    /// directory reachable from the root is read, each once: a link in
    /// 0.1.3's shape names it by its entry's anode, a link pfs3aio made by
    /// its `link` field (`CreateLink`, `directory.c:2686`). `head`, the
    /// object's own `link` field, is the head of its chain of links
    /// (`directory.c:2720-2726`); when no link entry is found it still names
    /// links — pfs3aio would discard nodes whose entries are gone
    /// (`directory.c:3922-3934`), but walking a chain this writer cannot
    /// check is not safe.
    ///
    /// ART (the scoped re-review's item 4): a check that cannot finish is
    /// refused `Error::EntryNotDeleted`, naming `object`, what stopped it and
    /// where — a directory that cannot be read, an entry that cannot be
    /// walked (by block and offset), a link whose extra fields do not fit.
    /// It used to return the bare error, or stop walking a block at a
    /// malformed entry and let the delete go ahead.
    fn links_naming(&mut self, object: &str, anode: u32, head: u32) -> Result<Option<String>> {
        let refuse = |reason: String| Error::EntryNotDeleted {
            name: object.to_string(),
            reason: format!("ART could not check that no hard link names it, because {reason}"),
        };
        let dir_phrase = |path: &str| {
            if path.is_empty() {
                "the root directory".to_string()
            } else {
                format!("the directory '{path}'")
            }
        };
        let mut dirs = vec![(String::new(), ANODE_ROOTDIR)];
        let mut seen = std::collections::HashSet::new();
        while let Some((path, dir)) = dirs.pop() {
            if !seen.insert(dir) {
                continue;
            }
            let unreadable = |e: Error| refuse(format!("{} could not be read ({e})", dir_phrase(&path)));
            let chain = self
                .vol
                .anodes
                .get_chain(dir, self.vol.dev.as_ref(), &mut self.vol.cache)
                .map_err(unreadable)?;
            for an in &chain {
                for i in 0..an.clustersize {
                    let blk = an.blocknr + i;
                    let data = self.read_reserved_raw(blk).map_err(unreadable)?;
                    if data.get(0..2) != Some(&DBLKID.to_be_bytes()[..]) {
                        continue;
                    }
                    let mut pos = DIR_BLOCK_HEADER_SIZE;
                    loop {
                        let (esize, nlen) = match Self::dir_entry_at(&data, blk, pos) {
                            Ok(Some(found)) => found,
                            Ok(None) => break,
                            Err(_) => {
                                return Err(refuse(format!(
                                    "{} holds a malformed entry at offset {pos} of block {blk} \
                                     (size {})",
                                    dir_phrase(&path),
                                    data.get(pos).copied().unwrap_or(0)
                                )));
                            }
                        };
                        // `dir_entry_at` bounds the entry and its name.
                        let entry = data.get(pos..pos + esize).unwrap_or(&[]);
                        let etype = entry.get(1).map_or(0, |&t| t as i8);
                        let eanode = entry
                            .get(2..6)
                            .map_or(0, |b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
                        let name =
                            crate::util::latin1_to_string(entry.get(18..18 + nlen).unwrap_or(&[]));
                        let full = if path.is_empty() {
                            name
                        } else {
                            format!("{path}/{name}")
                        };
                        if etype == ST_USERDIR {
                            dirs.push((full, eanode));
                        } else if etype == ST_LINKFILE || etype == ST_LINKDIR {
                            let link = self.entry_link_field(entry).map_err(|m| {
                                refuse(format!(
                                    "the hard link '{full}' has extra fields that do not fit its \
                                     entry (flags 0x{:04x})",
                                    m.flags
                                ))
                            })?;
                            if link == anode || (link == 0 && eanode == anode) {
                                return Ok(Some(format!("'{full}'")));
                            }
                        }
                        pos += esize;
                    }
                }
            }
        }
        if head != 0 {
            return Ok(Some(format!("a pfs3aio link chain from anode {head}")));
        }
        Ok(None)
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
        let clusters = block_extents(blocks);
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
        self.add_dir_entry_bytes(dir_anode, &entry_bytes)
    }

    /// ART (ART-327): `add_dir_entry`'s body, for an entry already built —
    /// `rename_in`'s moved entry. ART (the scoped re-review's item 5): the
    /// walk to the end of a block's entries goes through `dir_entry_at`, so a
    /// malformed entry is refused rather than stepped over.
    fn add_dir_entry_bytes(&mut self, dir_anode: u32, entry_bytes: &[u8]) -> Result<()> {
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
                while let Some((esize, _)) = Self::dir_entry_at(&data, blk, pos)? {
                    pos += esize;
                }
                if pos + entry_bytes.len() < self.resblocksize as usize {
                    data[pos..pos + entry_bytes.len()].copy_from_slice(entry_bytes);
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
            .copy_from_slice(entry_bytes);
        self.write_reserved(new_blk, &new_data)?;
        self.extend_anode_chain(dir_anode, new_blk)
    }

    fn remove_dir_entry(&mut self, dir_anode: u32, name: &str) -> Result<()> {
        let (blk, mut data, pos) = self.find_dir_entry(dir_anode, name)?;
        // ART (the scoped re-review's item 5): bounded by the block itself —
        // `find_dir_entry` returns an entry with `pos + esize <= data.len()`.
        // 0.1.3 took the block's length from `resblocksize`.
        let len = data.len();
        let esize = usize::from(data.get(pos).copied().unwrap_or(0));
        let end = (pos + esize).min(len);
        data.copy_within(end..len, pos);
        data[len - (end - pos)..].fill(0);
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
        // ART (ART-325): the extra fields in pfs3aio's layout
        // (`AddExtraFields`, `directory.c:3764-3800`) — `fsizex`, when there
        // are high bits, then the flags word — through `ExtraFields::encode`.
        // Below 4 GiB this is 0.1.3's entry byte for byte (a zero flags word
        // at the same offset); 0.1.3 put a set `fsizex` after the flags word,
        // behind bit 0x40.
        let extra = ExtraFields {
            fsizex: (fsize >> 32) as u16,
            ..ExtraFields::default()
        }
        .encode();
        let fields = extra_fields_offset(nlen, 0);
        let mut entry = vec![0u8; fields];
        entry.extend_from_slice(&extra);
        entry[0] = entry.len() as u8;
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

/// `blocks` as runs of consecutive block numbers, `(first, count)` each —
/// one anode per run. Shared by `create_anode_chain` and ART's copy-on-write
/// `overwrite_file_in` (2026-09-15); the same loop 0.1.3 had inline.
fn block_extents(blocks: &[u32]) -> Vec<(u32, u32)> {
    let mut clusters = Vec::new();
    let mut i = 0usize;
    while i < blocks.len() {
        let start = blocks[i];
        let mut count = 1u32;
        while i + (count as usize) < blocks.len() && blocks[i + count as usize] == start + count {
            count += 1;
        }
        clusters.push((start, count));
        i += count as usize;
    }
    clusters
}
