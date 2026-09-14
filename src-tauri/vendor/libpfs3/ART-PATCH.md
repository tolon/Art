# `libpfs3` 0.1.3+art.2 — ART's vendored copy

This directory is `libpfs3` 0.1.3 as published on crates.io, vendored into ART for
[ART-310](../../../docs/ISSUES.md). ART's build uses it through `[patch.crates-io]` in
`src-tauri/Cargo.toml`; the `libpfs3 = "=0.1.3"` pin there names the release it was taken from.

| | |
|---|---|
| Original | `https://static.crates.io/crates/libpfs3/libpfs3-0.1.3.crate`, SHA-256 `02f457ef99a09ddebf56e454c6a25dc3a6860a602c878489f132a4ca3eed4317` |
| Upstream source | `metaneutrons/pfs3` commit `33e9ff6ba8462cc4e434dfb6e2783d91b7dd5b14`, `crates/libpfs3` (the crate's `.cargo_vcs_info.json`) |
| Licence | LGPL-3.0-or-later. `LICENSE` is upstream's own file at that commit, unchanged; the full LGPL-3.0 text is `COPYING.LESSER`; the GPL-3.0 text it builds on is ART's `LICENSE` |
| Modified | 2026-09-13 and 2026-09-14, by ART: `src/format.rs` and `src/writer.rs`; each file's header says so |
| Carried | `src/`, `README.md`, `Cargo.toml` (from `Cargo.toml.orig`: version `0.1.3+art.2`, `[dev-dependencies]` removed), `LICENSE`, `COPYING.LESSER` |
| Not carried | `tests/`: `GPL-3.0-only` headers, 9.3 MB of fixtures, and a dev-dependency (`sevenz-rust` 0.6) with RUSTSEC-2026-0245 and RUSTSEC-2026-0246. ART's own tests prove the patch (`src-tauri/src/core/preload/native.rs`) |

## Changes against 0.1.3

**`src/format.rs`** ([ART-310](../../../docs/ISSUES.md); research
`docs/superpowers/notes/2026-09-11-libpfs3-format-fix-research.md`, design
`docs/superpowers/specs/2026-09-13-art-310-libpfs3-format-fix-design.md`):

1. **SUPERINDEX mode writes the super index level.** One more reserved block is allocated after the
   bitmap index blocks and before the anode index block, which is pfs3aio's order. It is written as
   `SBLKID`, datestamp 1, seqnr 0, `index[0]` = the anode index block, and `rext.superindex[0]` names it.
   0.1.3 pointed `superindex[0]` at the anode index block. hst-imager could not mount such a volume, and
   `libpfs3` could not read its own root directory (`anode 5 not found`).
2. **Anodes 0–4 are reserved at every size**: `clustersize 0, blocknr 0xFFFFFFFF, next 0`, what pfs3aio's
   `AllocAnode` leaves. 0.1.3 left them `(0, 0, 0)`. hst-imager's first new directory then failed
   `ERROR_DISK_FULL`, and a later one was given reserved anode 1.

Written in this crate's own idiom from the on-disk layout, not translated from pfs3aio (BSD-4-Clause) or
any port of it. Reserved-area sizing, option flags, datestamps and everything else are 0.1.3's.

**`src/writer.rs`** ([ART-312](../../../docs/ISSUES.md)):

3. **An operation that allocates two anodes gets two different ones.** `get_anode_block_nr` resolves an
   anode block through the writer's own `read_reserved_raw` — `rootblock.indexblocks` or
   `rext.superindex`, then (SB, then) IB, then the entry — instead of through the volume's cache, which
   read the device. An index entry the same, still-uncommitted operation had set lived only in
   `pending_writes`, so 0.1.3 saw 0, created the anode block again and handed out the same anode number
   twice; the file then read back as its directory's continuation block, with no error.
   `pending_writes` and the commit order are unchanged. The SB read in `alloc_anode_block` still goes
   through the cache; it matters only when one operation allocates two new index blocks in SUPERINDEX
   mode.

Everything else in the writer is 0.1.3's, including its anode ceiling (ART-311).

**2026-09-14, the debt round** (plan `docs/superpowers/plans/2026-09-14-debt-3-pfs3.md`; pfs3aio read at
`tonioni/pfs3aio` `211f7f0`):

4. **A directory block's `parent` is the directory that holds it ([ART-313](../../../docs/ISSUES.md)).** The
   root directory's block is written with parent 0 (`format.rs`; pfs3aio `format.c:548`), and a continuation
   directory block copies the parent of the directory's existing blocks (`writer.rs` `add_dir_entry`; pfs3aio
   `directory.c:3176,3204`). 0.1.3 wrote the root with 5 and every continuation block with the directory's
   own anode. `libpfs3`'s reader never reads `parent`.
5. **Data blocks stay below the partition's end ([ART-315](../../../docs/ISSUES.md)).** `alloc_data_blocks`
   skips a bitmap bit at or past `disksize` (0.1.3: `disksize + bitmapstart`), `free_data_block` ignores a
   block at or past it, and `load_data_bitmap` sizes the bitmap from `disksize − bitmapstart`. pfs3aio refuses
   `blocknr >= numblocks` (`allocation.c:344`) and leaves the bits past the last block set, so on a volume
   formatted by pfs3aio or hst-imager 0.1.3 could hand out a block past the partition.
6. **A name the volume cannot find again is refused ([ART-314](../../../docs/ISSUES.md)).** The format writes
   `rext.fnsize` 107 (`FORMAT_FNSIZE`; 0.1.3 wrote 32), and every writer call that names an entry returns
   `Error::NameTooLong` for a name longer than `fnsize − 1` bytes (at most 107), before anything is allocated.
   0.1.3 stored up to 107 bytes whatever the volume's `fnsize`, and cut longer names silently. pfs3aio cuts
   every name to `fnsize − 1` on create and on lookup (`directory.c:1489-1490,721-722`) and compares lengths
   first (`assroutines.c:163`), so a longer name lists and never opens.
7. **The anode ceiling is pfs3aio's ([ART-311](../../../docs/ISSUES.md)), part 1.** `alloc_anode` searches
   every anode block the volume can address — 65 536 (pfs3aio's `UWORD` seqnr, `anodes.c:582`), fewer where
   the index levels end — instead of 256. In small mode `alloc_anode_block` makes a missing index block, up
   to `MAXSMALLINDEXNR` (pfs3aio `NewIndexBlock`, `anodes.c:740-761`), and `update_rootblock` writes the
   rootblock's `indexblocks` union; the volume's `AnodeReader` is rebuilt when an index root changes. 0.1.3
   held 253 × 84 − 6 = 21 246 anodes in small mode and 256 × 84 − 6 = 21 498 in SUPERINDEX mode.

## Re-vendoring

After replacing this directory, run `cargo update -p libpfs3 --precise <version>` in `src-tauri`: Cargo
did not refresh the lock entry on its own when this copy was vendored (2026-09-13). Then
`core::preload::native`'s `the_pinned_version_constant_matches_cargo_toml` and `probe_names_libpfs3`
must pass.

## Upstream

Prepared, not offered. Branch `fix/format-superindex-reserved-anodes` (`6eb44df`, on `main` at `05f50b0`) in
a local clone of `metaneutrons/pfs3`: the same change to `crates/libpfs3/src/format.rs`, two tests in
`tests/format.rs` (seen red on `main`, green on the change), upstream's fmt, clippy, test and deny checks
clean, and a Conventional Commit with no AI attribution, as upstream's `CONTRIBUTING.md` requires. The
owner opens the pull request after a patched volume has been mounted under real pfs3aio. That branch
carries the format change only; the writer change (ART-312) is not prepared for upstream yet.

## Diff against 0.1.3

```diff
--- a/src/error.rs
+++ b/src/error.rs
@@ -30,6 +30,14 @@ pub enum Error {
     #[error("already exists: {0}")]
     AlreadyExists(String),
 
+    /// ART-314: a name longer than the volume can store and find again.
+    #[error("name too long: '{name}' is {len} bytes, this volume stores at most {max}")]
+    NameTooLong {
+        name: String,
+        len: usize,
+        max: usize,
+    },
+
     #[error("disk full: {0}")]
     DiskFull(String),
 
--- a/src/format.rs
+++ b/src/format.rs
@@ -3,13 +3,19 @@
 //! Creates a new PFS3 filesystem on a block device.
 //! Ported from pfs3aio/format.c and amitools PFSFormat.py.
 //!
+//! Modified by ART on 2026-09-13 (ART-310): the super index level and the
+//! reserved anodes 0-4; on 2026-09-14 (ART-313): the root directory's parent,
+//! (ART-314) names — `rext.fnsize` writes 107, not 32.
+//! `ART-PATCH.md` in this crate's root says what and why.
+//!
 //! Format sequence:
 //! 1. Write boot block (PFS\1 magic)
 //! 2. Calculate reserved area size
 //! 3. Build rootblock + reserved bitmap
 //! 4. Allocate and write rootblock extension
 //! 5. Allocate and write bitmap index + bitmap blocks
-//! 6. Allocate and write anode index + anode block (with ANODE_ROOTDIR)
+//! 6. Allocate and write the super index block (SUPERINDEX mode only), the
+//!    anode index block and the anode block (anodes 0-4 reserved, ANODE_ROOTDIR)
 //! 7. Write root directory block (empty)
 
 use crate::error::{Error, Result};
@@ -32,6 +38,12 @@ impl Default for FormatOptions {
     }
 }
 
+/// ART-314: the `rext.fnsize` a format writes — the length limit, plus one, of
+/// every name on the volume. pfs3aio's own format writes 32 (`format.c:520`)
+/// and cuts every name to `fnsize - 1` bytes; hst-imager formats 107, the
+/// longest this crate's directory entries hold.
+pub const FORMAT_FNSIZE: u16 = 107;
+
 /// Result of a successful format operation.
 #[derive(Debug)]
 pub struct FormatResult {
@@ -146,7 +158,14 @@ pub fn format_with_size(
         bmi_blocknrs.push(firstreserved + idx * rescluster);
     }
 
-    // 4. Allocate anode index + anode block
+    // 4. Allocate the anode index path. In SUPERINDEX mode pfs3aio's first
+    //    AllocAnode goes through NewSuperBlock before NewIndexBlock, so the
+    //    super index block is allocated first.
+    let sb_blk = if supermode {
+        Some(firstreserved + alloc.alloc()? * rescluster)
+    } else {
+        None
+    };
     let anidx_blk = firstreserved + alloc.alloc()? * rescluster;
     let anode_blk = firstreserved + alloc.alloc()? * rescluster;
 
@@ -225,9 +244,11 @@ pub fn format_with_size(
     put_u16(&mut rext, 0x10, cday);
     put_u16(&mut rext, 0x12, cmin);
     put_u16(&mut rext, 0x14, ctick);
-    put_u16(&mut rext, 0x38, 32); // fnsize
-    if supermode {
-        put_u32(&mut rext, 0x40, anidx_blk); // superindex[0]
+    put_u16(&mut rext, 0x38, FORMAT_FNSIZE); // fnsize (ART-314)
+    if let Some(sb_blk) = sb_blk {
+        // superindex[0] names the super index block, never the anode index
+        // block: every reader walks SB -> IB -> AB.
+        put_u32(&mut rext, 0x40, sb_blk); // superindex[0]
     }
     write_reserved_blocks(dev, rext_blk as u64, &rext, rescluster, bs)?;
 
@@ -267,6 +288,17 @@ pub fn format_with_size(
         write_reserved_blocks(dev, bm_blknr as u64, &bm, rescluster, bs)?;
     }
 
+    // Write the super index block, as pfs3aio's NewSuperBlock leaves it:
+    // id SB, seqnr 0, index[0] = the anode index block.
+    if let Some(sb_blk) = sb_blk {
+        let mut sb = vec![0u8; resblocksize as usize];
+        put_u16(&mut sb, 0, SBLKID);
+        put_u32(&mut sb, 4, 1); // datestamp
+        put_u32(&mut sb, 8, 0); // seqnr
+        put_u32(&mut sb, 12, anidx_blk); // index[0] = anode index block
+        write_reserved_blocks(dev, sb_blk as u64, &sb, rescluster, bs)?;
+    }
+
     // Write anode index block
     let mut anidx = vec![0u8; resblocksize as usize];
     put_u16(&mut anidx, 0, IBLKID);
@@ -280,18 +312,27 @@ pub fn format_with_size(
     put_u16(&mut an, 0, ABLKID);
     put_u32(&mut an, 4, 1);
     put_u32(&mut an, 8, 0); // seqnr
+    // Anodes 0..ANODE_ROOTDIR-1 are reserved, as pfs3aio's format leaves them
+    // (AllocAnode: clustersize 0, blocknr 0xffffffff, next 0). An allocator
+    // ported from pfs3aio hands out any anode that reads (0, 0, 0).
+    for nr in 0..ANODE_ROOTDIR as usize {
+        let off = ANODE_BLOCK_HEADER_SIZE + nr * ANODE_SIZE;
+        put_u32(&mut an, off + 4, 0xFFFF_FFFF);
+    }
     let an_off = ANODE_BLOCK_HEADER_SIZE + ANODE_ROOTDIR as usize * ANODE_SIZE;
     put_u32(&mut an, an_off, 1); // clustersize = 1
     put_u32(&mut an, an_off + 4, rootdir_blk); // blocknr
     put_u32(&mut an, an_off + 8, 0); // next = EOF
     write_reserved_blocks(dev, anode_blk as u64, &an, rescluster, bs)?;
 
-    // Write root directory block (empty)
+    // Write root directory block (empty). ART-313: the root's own blocks carry
+    // parent 0 — pfs3aio's format.c:548 `MakeDirBlock(blocknr, anodenr, anodenr, 0, g)`,
+    // and GetParent treats 0 as "this is the root". 0.1.3 wrote ANODE_ROOTDIR here.
     let mut dir = vec![0u8; resblocksize as usize];
     put_u16(&mut dir, 0x00, DBLKID);
     put_u32(&mut dir, 0x04, 1); // datestamp
     put_u32(&mut dir, 0x0C, ANODE_ROOTDIR);
-    put_u32(&mut dir, 0x10, ANODE_ROOTDIR); // parent = self
+    put_u32(&mut dir, 0x10, 0); // parent: none, this is the root
     write_reserved_blocks(dev, rootdir_blk as u64, &dir, rescluster, bs)?;
 
     dev.flush()?;
--- a/src/writer.rs
+++ b/src/writer.rs
@@ -6,6 +6,13 @@
 //! - Anode allocation and chain building
 //! - Directory entry creation and removal
 //! - Rootblock update
+//!
+//! Modified by ART on 2026-09-13 (ART-312): `get_anode_block_nr` sees this
+//! writer's own pending writes; on 2026-09-14 (ART-313): a continuation directory
+//! block's parent; (ART-315) the data bitmap's bounds; (ART-314) names — a name
+//! longer than `fnsize - 1` bytes is refused before anything is allocated;
+//! (ART-311) the anode search range, index blocks on demand.
+//! `ART-PATCH.md` in this crate's root says what and why.
 
 use crate::error::{Error, Result};
 use crate::ondisk::*;
@@ -74,6 +81,29 @@ impl Writer {
         self.datestamp
     }
 
+    /// ART-314: the longest name this volume can store and find again. pfs3aio
+    /// cuts a new name to `fnsize - 1` bytes (`directory.c:1489-1490,1663-1664`)
+    /// and a searched-for name the same way (`:721-722`) before a compare that
+    /// needs equal lengths (`assroutines.c:163`). 107 is the longest name this
+    /// writer's directory entries hold (`build_dir_entry`).
+    fn max_name_bytes(&self) -> usize {
+        usize::from(self.vol.fnsize()).saturating_sub(1).min(107)
+    }
+
+    /// ART-314: refuse a name the volume would store and never find again,
+    /// before anything is allocated. 0.1.3 stored up to 107 bytes and cut the rest.
+    fn check_name_len(&self, name: &str) -> Result<()> {
+        let max = self.max_name_bytes();
+        if name.len() > max {
+            return Err(Error::NameTooLong {
+                name: name.to_string(),
+                len: name.len(),
+                max,
+            });
+        }
+        Ok(())
+    }
+
     // ---- High-level API (path-based, for CLI) ----
 
     /// Write a file at the given path. Parent directories must exist.
@@ -124,6 +154,7 @@ impl Writer {
 
     /// Create a file in a directory identified by anode.
     pub fn write_file_in(&mut self, parent_anode: u32, name: &str, data: &[u8]) -> Result<()> {
+        self.check_name_len(name)?;
         // Check if file already exists — if so, overwrite it
         if let Ok((_, entry_data, pos)) = self.find_dir_entry(parent_anode, name) {
             let entry_type = entry_data[pos + 1] as i8;
@@ -144,6 +175,7 @@ impl Writer {
         name: &str,
         data: &[u8],
     ) -> Result<()> {
+        self.check_name_len(name)?;
         let bs = self.vol.block_size() as usize;
         let num_blocks = data.len().div_ceil(bs).max(1);
 
@@ -165,6 +197,7 @@ impl Writer {
 
     /// Create a directory in a parent identified by anode. Returns the new dir's anode number.
     pub fn create_dir_in(&mut self, parent_anode: u32, name: &str) -> Result<()> {
+        self.check_name_len(name)?;
         let dir_blk = self.alloc_reserved_block()?;
         let anodenr = self.alloc_anode(1, dir_blk, 0)?;
 
@@ -192,6 +225,7 @@ impl Writer {
         name: &str,
         target: &str,
     ) -> Result<()> {
+        self.check_name_len(name)?;
         let data = target.as_bytes();
         let bs = self.vol.block_size() as usize;
         let num_blocks = data.len().div_ceil(bs).max(1);
@@ -221,6 +255,7 @@ impl Writer {
     /// Create a hardlink in a parent directory.
     pub fn create_hardlink(&mut self, path: &str, target_anode: u32) -> Result<()> {
         let (parent_anode, name) = self.split_path(path)?;
+        self.check_name_len(&name)?;
         self.add_dir_entry(parent_anode, &name, ST_LINKFILE, target_anode, 0, 0)?;
         self.update_rootblock()
     }
@@ -595,6 +630,7 @@ impl Writer {
         dst_parent: u32,
         dst_name: &str,
     ) -> Result<()> {
+        self.check_name_len(dst_name)?;
         let entries = self.vol.list_dir_by_anode(src_parent)?;
         let entry = entries
             .iter()
@@ -739,9 +775,11 @@ impl Writer {
     fn load_data_bitmap(&mut self) -> Result<()> {
         let no_bmb = {
             let bits_per_bmb = self.index_per_block * 32;
-            let ds = self.vol.rootblock.disksize;
+            // ART-315: the bitmap covers the data blocks, [bitmapstart, disksize),
+            // not the whole disk (format.rs sizes it the same way).
+            let data_blocks = self.vol.rootblock.disksize.saturating_sub(self.bitmapstart);
             // Cap at a reasonable maximum to prevent OOM on corrupt disksize
-            ds.div_ceil(bits_per_bmb).min(16384)
+            data_blocks.div_ceil(bits_per_bmb).min(16384)
         };
         for seq in 0..no_bmb {
             if let Some(blk) = self.get_bitmap_block_nr(seq)? {
@@ -778,7 +816,10 @@ impl Writer {
                             .ok_or_else(|| {
                                 Error::Corrupt("block number overflow in bitmap".into())
                             })?;
-                        if data_blk >= self.vol.rootblock.disksize + self.bitmapstart {
+                        // ART-315: a data block is below the partition's end. pfs3aio
+                        // allocation.c:344 refuses `blocknr >= numblocks`; the bits past
+                        // it are left set by pfs3aio's and hst-imager's formats.
+                        if data_blk >= self.vol.rootblock.disksize {
                             continue; // skip out-of-range bitmap bits
                         }
                         longs[li] &= !(0x8000_0000 >> bit);
@@ -825,7 +866,9 @@ impl Writer {
     }
 
     fn free_data_block(&mut self, blk: u32) -> Result<()> {
-        if blk < self.bitmapstart {
+        // ART-315: a block outside [bitmapstart, disksize) has no bit of its own;
+        // freeing one past the end would set a tail bit the allocator must never see.
+        if blk < self.bitmapstart || blk >= self.vol.rootblock.disksize {
             return Ok(());
         }
         let rel = blk - self.bitmapstart;
@@ -898,7 +941,7 @@ impl Writer {
 
     fn alloc_anode(&mut self, clustersize: u32, blocknr: u32, next: u32) -> Result<u32> {
         let split = self.vol.rootblock.is_splitted_anodes();
-        for seqnr in 0..256u32 {
+        for seqnr in 0..self.anode_block_limit() {
             let blk_num = self.get_anode_block_nr(seqnr)?;
             if blk_num == 0 {
                 // No anode block at this seqnr — allocate one
@@ -947,6 +990,29 @@ impl Writer {
         Err(Error::DiskFull("no free anode slots".into()))
     }
 
+    /// ART-311: the anode blocks this volume can address. pfs3aio passes an
+    /// anode block's number as a UWORD (`anodes.c:582`), so 65 536 at most; the
+    /// index levels bound it further — small mode's rootblock names
+    /// `MAXSMALLINDEXNR + 1` index blocks (`anodes.c:740`), SUPERINDEX mode's
+    /// extension `MAXSUPER + 1` super blocks (`anodes.c:851`). 0.1.3 searched 256.
+    fn anode_block_limit(&self) -> u32 {
+        let ipb = u64::from(self.index_per_block);
+        let by_index = if self.vol.rootblock.is_large() {
+            (MAXSUPER as u64 + 1) * ipb * ipb
+        } else {
+            (MAXSMALLINDEXNR as u64 + 1) * ipb
+        };
+        by_index.min(1 << 16) as u32
+    }
+
+    /// ART-311: the volume's `AnodeReader` copies the index roots when it is
+    /// built (`anode.rs:41-42`). After this writer names a new index or super
+    /// block it is rebuilt, so a chain walk later in this session resolves it.
+    fn refresh_anode_reader(&mut self) {
+        self.vol.anodes =
+            crate::anode::AnodeReader::new(&self.vol.rootblock, self.vol.rootblock_ext.as_ref());
+    }
+
     /// Allocate a new anode block and register it in the index.
     fn alloc_anode_block(&mut self, seqnr: u32) -> Result<u32> {
         let new_blk = self.alloc_reserved_block()?;
@@ -1012,8 +1078,14 @@ impl Writer {
                 self.write_reserved(idx_blk, &idata)?;
             }
         } else {
-            // Small mode: rootblock.indexblocks[idx_nr] → index block
-            let idx_blk = self
+            // Small mode: rootblock.indexblocks[idx_nr] -> index block. ART-311:
+            // a missing index block is made here, as pfs3aio's NewIndexBlock
+            // does (`anodes.c:740-761`), up to MAXSMALLINDEXNR; the rootblock
+            // names it when `update_rootblock` commits.
+            if idx_nr as usize > MAXSMALLINDEXNR {
+                return Err(Error::DiskFull("no index block slot available".into()));
+            }
+            let mut idx_blk = self
                 .vol
                 .rootblock
                 .indexblocks
@@ -1021,7 +1093,18 @@ impl Writer {
                 .copied()
                 .unwrap_or(0);
             if idx_blk == 0 {
-                return Err(Error::DiskFull("no index block slot available".into()));
+                idx_blk = self.alloc_reserved_block()?;
+                let mut idata = vec![0u8; self.resblocksize as usize];
+                put_u16(&mut idata, 0, IBLKID);
+                put_u32(&mut idata, 4, self.datestamp);
+                put_u32(&mut idata, 8, idx_nr); // seqnr (`anodes.c:767`)
+                self.write_reserved(idx_blk, &idata)?;
+                let slots = &mut self.vol.rootblock.indexblocks;
+                if slots.len() <= idx_nr as usize {
+                    slots.resize(idx_nr as usize + 1, 0);
+                }
+                slots[idx_nr as usize] = idx_blk;
+                self.refresh_anode_reader();
             }
             let mut idata = self.read_reserved_raw(idx_blk)?;
             let entry_off = INDEX_BLOCK_HEADER_SIZE + idx_off as usize * 4;
@@ -1097,6 +1180,7 @@ impl Writer {
                 .anodes
                 .get_chain(dir_anode, self.vol.dev.as_ref(), &mut self.vol.cache)?;
 
+        let mut dir_parent = None;
         for an in &chain {
             for i in 0..an.clustersize {
                 let blk = an.blocknr + i;
@@ -1104,6 +1188,11 @@ impl Writer {
                 if u16::from_be_bytes(data[0..2].try_into().unwrap()) != DBLKID {
                     continue;
                 }
+                // ART-313: every block of a directory carries the same parent;
+                // a new continuation block copies it (pfs3aio directory.c:3176,3204).
+                if dir_parent.is_none() {
+                    dir_parent = Some(u32::from_be_bytes(data[0x10..0x14].try_into().unwrap()));
+                }
                 // Find end of entries
                 let mut pos = DIR_BLOCK_HEADER_SIZE;
                 while pos < self.resblocksize as usize {
@@ -1123,13 +1212,17 @@ impl Writer {
                 }
             }
         }
-        // No space — allocate new dir block and extend chain
+        // No space — allocate new dir block and extend chain. Refused before
+        // anything is allocated when the directory has no block to copy from.
+        let parent = dir_parent.ok_or_else(|| {
+            Error::Corrupt(format!("directory {dir_anode} has no directory block"))
+        })?;
         let new_blk = self.alloc_reserved_block()?;
         let mut new_data = vec![0u8; self.resblocksize as usize];
         put_u16(&mut new_data, 0x00, DBLKID);
         put_u32(&mut new_data, 0x04, self.next_datestamp());
         put_u32(&mut new_data, 0x0C, dir_anode);
-        put_u32(&mut new_data, 0x10, dir_anode);
+        put_u32(&mut new_data, 0x10, parent);
         new_data[DIR_BLOCK_HEADER_SIZE..DIR_BLOCK_HEADER_SIZE + entry_bytes.len()]
             .copy_from_slice(&entry_bytes);
         self.write_reserved(new_blk, &new_data)?;
@@ -1236,6 +1329,25 @@ impl Writer {
             }
         }
 
+        // ART-311: in small mode the rootblock names every anode index block
+        // (`blocks.h:133`, `idx.small.indexblocks`, after the five bitmap index
+        // numbers); pfs3aio sets it in NewIndexBlock (`anodes.c:759-760`) and
+        // writes the rootblock last (`update.c:265-270`). SUPERINDEX mode keeps
+        // bitmap index numbers there instead.
+        if !self.vol.rootblock.is_large() {
+            let base = RB_OFF_INDEX_UNION + (MAXSMALLBITMAPINDEX + 1) * 4;
+            for (i, &blk) in self
+                .vol
+                .rootblock
+                .indexblocks
+                .iter()
+                .enumerate()
+                .take(MAXSMALLINDEXNR + 1)
+            {
+                put_u32(&mut cluster, base + i * 4, blk);
+            }
+        }
+
         self.vol
             .dev
             .write_blocks(self.firstreserved as u64, rblkcluster, &cluster)?;
@@ -1286,10 +1398,44 @@ impl Writer {
         Ok(())
     }
 
+    /// The anode block at `seqnr`, or 0 when none is allocated yet.
+    ///
+    /// ART-312: resolved through `read_reserved_raw`, so an index entry this
+    /// writer set earlier in the same, still-uncommitted operation is seen.
+    /// Going through the volume's cache read the device, where that entry is
+    /// still 0 until `update_rootblock` flushes `pending_writes` — a second
+    /// allocation in one operation then created the same anode block again
+    /// and handed out the same anode number twice.
     fn get_anode_block_nr(&mut self, seqnr: u32) -> Result<u32> {
-        self.vol
-            .anodes
-            .resolve_anode_block(seqnr, self.vol.dev.as_ref(), &mut self.vol.cache)
+        let ipb = self.index_per_block;
+        let entry = |data: &[u8], nr: u32| -> u32 {
+            let off = INDEX_BLOCK_HEADER_SIZE + nr as usize * 4;
+            data.get(off..off + 4)
+                .map_or(0, |b| u32::from_be_bytes(b.try_into().unwrap()))
+        };
+        let idx_blk = if self.vol.rootblock.is_large() {
+            let super_blk = self
+                .vol
+                .rootblock_ext
+                .as_ref()
+                .and_then(|e| e.superindex.get((seqnr / (ipb * ipb)) as usize).copied())
+                .unwrap_or(0);
+            if super_blk == 0 {
+                return Ok(0);
+            }
+            entry(&self.read_reserved_raw(super_blk)?, (seqnr % (ipb * ipb)) / ipb)
+        } else {
+            self.vol
+                .rootblock
+                .indexblocks
+                .get((seqnr / ipb) as usize)
+                .copied()
+                .unwrap_or(0)
+        };
+        if idx_blk == 0 {
+            return Ok(0);
+        }
+        Ok(entry(&self.read_reserved_raw(idx_blk)?, seqnr % ipb))
     }
 
     fn get_bitmap_block_nr(&mut self, seqnr: u32) -> Result<Option<u32>> {
```
