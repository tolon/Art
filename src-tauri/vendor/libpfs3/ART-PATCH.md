# `libpfs3` 0.1.3+art.1 — ART's vendored copy

This directory is `libpfs3` 0.1.3 as published on crates.io, vendored into ART for
[ART-310](../../../docs/ISSUES.md). ART's build uses it through `[patch.crates-io]` in
`src-tauri/Cargo.toml`; the `libpfs3 = "=0.1.3"` pin there names the release it was taken from.

| | |
|---|---|
| Original | `https://static.crates.io/crates/libpfs3/libpfs3-0.1.3.crate`, SHA-256 `02f457ef99a09ddebf56e454c6a25dc3a6860a602c878489f132a4ca3eed4317` |
| Upstream source | `metaneutrons/pfs3` commit `33e9ff6ba8462cc4e434dfb6e2783d91b7dd5b14`, `crates/libpfs3` (the crate's `.cargo_vcs_info.json`) |
| Licence | LGPL-3.0-or-later. `LICENSE` is upstream's own file at that commit, unchanged; the full LGPL-3.0 text is `COPYING.LESSER`; the GPL-3.0 text it builds on is ART's `LICENSE` |
| Carried | `src/`, `README.md`, `Cargo.toml` (from `Cargo.toml.orig`: version `0.1.3+art.1`, `[dev-dependencies]` removed), `LICENSE`, `COPYING.LESSER` |
| Not carried | `tests/`: `GPL-3.0-only` headers, 9.3 MB of fixtures, and a dev-dependency (`sevenz-rust` 0.6) with RUSTSEC-2026-0245 and RUSTSEC-2026-0246. ART's own tests prove the patch (`src-tauri/src/core/preload/native.rs`) |

## Changes against 0.1.3

**`src/format.rs` only** ([ART-310](../../../docs/ISSUES.md); research
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

**The writer is 0.1.3's, unchanged.** Its two index-block gaps are ART-311.

## Upstream

Prepared, not offered. Branch `fix/format-superindex-reserved-anodes` (`6eb44df`, on `main` at `05f50b0`) in
a local clone of `metaneutrons/pfs3`: the same change to `crates/libpfs3/src/format.rs`, two tests in
`tests/format.rs` (seen red on `main`, green on the change), upstream's fmt, clippy, test and deny checks
clean, and a Conventional Commit with no AI attribution, as upstream's `CONTRIBUTING.md` requires. The
owner opens the pull request after a patched volume has been mounted under real pfs3aio.

## Diff against 0.1.3

```diff
--- a/src/format.rs
+++ b/src/format.rs
@@ -9,7 +9,8 @@
 //! 3. Build rootblock + reserved bitmap
 //! 4. Allocate and write rootblock extension
 //! 5. Allocate and write bitmap index + bitmap blocks
-//! 6. Allocate and write anode index + anode block (with ANODE_ROOTDIR)
+//! 6. Allocate and write the super index block (SUPERINDEX mode only), the
+//!    anode index block and the anode block (anodes 0-4 reserved, ANODE_ROOTDIR)
 //! 7. Write root directory block (empty)
 
 use crate::error::{Error, Result};
@@ -146,7 +147,14 @@
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
 
@@ -226,8 +234,10 @@
     put_u16(&mut rext, 0x12, cmin);
     put_u16(&mut rext, 0x14, ctick);
     put_u16(&mut rext, 0x38, 32); // fnsize
-    if supermode {
-        put_u32(&mut rext, 0x40, anidx_blk); // superindex[0]
+    if let Some(sb_blk) = sb_blk {
+        // superindex[0] names the super index block, never the anode index
+        // block: every reader walks SB -> IB -> AB.
+        put_u32(&mut rext, 0x40, sb_blk); // superindex[0]
     }
     write_reserved_blocks(dev, rext_blk as u64, &rext, rescluster, bs)?;
 
@@ -267,6 +277,17 @@
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
@@ -280,6 +301,13 @@
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
```
