# One-button card — Round 1: sizing, and the two card-size defects

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A pure, measured sizing core for the one-button card — image size from the card's label, PFS3 footprint of host content, the partition table — plus the fixes for ART-308 (card sized in GiB) and ART-309 (last RDB partition past its area).

**Architecture:** One new pure module, `src-tauri/src/core/card/sizing.rs`, holding every number the design's § 4 names; a one-line change of rounding in `core/rdb.rs`; the card builder's request carrying the card's **label** so the bytes are computed once, in Rust. The PFS3 arithmetic is a port of `libpfs3`'s own reserved-area calculation, proved identical by formatting in memory, and the whole estimate is proved by filling a partition of the estimated size with `libpfs3`'s own writer.

**Tech Stack:** Rust (MSRV 1.93), `libpfs3 = 0.1.3` (already a dependency of `core`), React/TypeScript (Vitest), Tauri 2.

**Spec:** `docs/superpowers/specs/2026-09-11-one-button-card-design.md` (§ 4 is this round). **Research:** `docs/superpowers/notes/2026-09-11-card-layout-research.md`.

## Global Constraints

- `src-tauri/src/core/` is platform-independent: `std`, `serde`, `serde_json`, `sha2`, `log`, `thiserror`, `delharc`, `zip`, `sevenz-rust2`, `quick-xml`, `fatfs`, `libpfs3` — nothing else; never `use tauri`, no process spawn outside a test.
- Image total = **95 % of the card's decimal gigabytes**: `card_gb × 950 000 000` bytes (emu68hatcher `partition_helpers.py:34-36`, MIT, attribute in the doc comment).
- FAT32 boot stays `core::card::propose::MEASURED_BOOT_BYTES` (1 178 599 424, end of boot incl. alignment); System stays `core::card::propose::MEASURED_SYSTEM_MB` (800) — reuse, never restate.
- PFS3 partition: minimum 10 MiB, maximum 213 021 952 blocks of 512 bytes = 109 067 239 424 bytes (pfs3aio `blocks.h:106-120`).
- Content partition = the smallest PFS3 size its content fits in **keeping the 5 % always-free**, × 1.25, rounded up to whole 516 096-byte cylinders.
- RDB geometry is fixed: 16 heads × 63 sectors × 512 bytes = 516 096 bytes a cylinder; 2 cylinders reserved (`core/rdb.rs`).
- Tests: scratch through `let (_guard, dir) = ScratchDir::pair(..)` / the module's `scratch()`; fixtures synthetic and generated at runtime; no Amiga content in the repo.
- Run Rust tests with `TMP`/`TEMP` on `E:` (`E:\amiga\ProjeART\build\tmp`); never write to `C:`.
- A test run is finished when it prints `test result:`; never pipe a command whose exit status matters.
- Commit messages from a file (`git commit -F`); stage files by name, never `git add -A`.
- Every fixed defect: the test seen **red** before the fix, and a mutation of the guard seen to fail it.

## File structure

| File | Responsibility |
|---|---|
| `src-tauri/src/core/rdb.rs` (modify) | cylinder count rounded down; tests that pinned the round-up |
| `src-tauri/src/core/card/sizing.rs` (create) | card label → bytes; `ContentMeasure`; PFS3 reserved/data arithmetic; fit; the card image plan |
| `src-tauri/src/core/card/mod.rs` (modify) | `pub mod sizing;` |
| `src-tauri/src/commands/card.rs` (modify) | `CardBuildRequest::card_gb`; `card_propose_table(card_gb, …)` |
| `src/lib/cardBuild.ts` (modify) | `card_gb` on the request type; `cardProposeTable(cardGb, …)` |
| `src/components/osbuilder/CardBuilder.tsx` (modify) | send the label, never `× GIB` |
| docs | ISSUES (ART-308, ART-309 → Fixed), CHANGELOG, STATUS, session log |

---

### Task 1: ART-309 — the last partition ends inside its area

**Files:**
- Modify: `src-tauri/src/core/rdb.rs:800-802` (cylinder count), the test at `:1348-1357`, the test at `:2006-2007`
- Test: `src-tauri/src/core/rdb.rs` (tests module, `mod tests` at `:1305`)

**Interfaces:**
- Consumes: `create_rdb_layout(total_bytes: u64, partitions: &[PartitionSpec], file_systems: &[FileSystemSpec]) -> CoreResult<RdbLayout>`; the test helper `read_partition_extent(blocks: &[u8], part_block: usize) -> (u32, u32)` (low, high cylinder) already in the tests module.
- Produces: `RdbLayout::total_size` is now `floor(total_bytes / 516 096) × 516 096` — never more than asked.

- [ ] **Step 1: Write the failing test** — add to `mod tests` in `rdb.rs`:

```rust
    /// **ART-309.** A card's Amiga area is almost never a whole number of
    /// cylinders. The cylinder count used to be rounded **up**, so the last
    /// partition — "whatever is left" — ended past the area: 90 112 bytes past
    /// the end of the owner's own `1card.img`. Asserted by reading the partition
    /// block back, never by recomputing the writer's arithmetic.
    #[test]
    fn the_last_partition_ends_inside_an_area_that_is_not_whole_cylinders() {
        let bytes_per_cyl = 16u64 * 63 * 512;
        // The owner's card's area, to the byte.
        let total = 67_540_877_312u64;
        assert_ne!(total % bytes_per_cyl, 0, "the fixture must not be whole cylinders");

        let layout = create_rdb_layout(
            total,
            &[
                PartitionSpec {
                    drive_name: "SDH0".into(),
                    fs_type: AmigaHardDiskFs::Pfs3DirectScsi,
                    size_mb: 512,
                    bootable: true,
                    boot_priority: 0,
                    num_buffers: 0,
                },
                PartitionSpec {
                    drive_name: "SDH1".into(),
                    fs_type: AmigaHardDiskFs::Pfs3DirectScsi,
                    size_mb: 0,
                    bootable: false,
                    boot_priority: 0,
                    num_buffers: 0,
                },
            ],
            &[],
        )
        .unwrap();

        let (_, last_high) = read_partition_extent(&layout.blocks, 2);
        let ends_at = (u64::from(last_high) + 1) * bytes_per_cyl;
        assert!(
            ends_at <= total,
            "the last partition ends at {ends_at}, past the area's {total} bytes"
        );
        assert!(layout.total_size <= total, "the geometry describes {} bytes", layout.total_size);
        assert!(total - ends_at < bytes_per_cyl, "and it loses less than one cylinder");
    }
```

- [ ] **Step 2: Run it and see it fail**

Run (from `src-tauri`, `TMP`/`TEMP` on E:): `cargo test --lib the_last_partition_ends_inside_an_area_that_is_not_whole_cylinders`
Expected: `FAILED` with `the last partition ends at 67540967424, past the area's 67540877312 bytes`.

- [ ] **Step 3: Round down** — `rdb.rs:800-802`:

```rust
    // **Rounded down (ART-309).** The geometry may describe less than the area
    // — under one cylinder is lost — but never more: a cylinder the device does
    // not have is one the Amiga will write past the end of.
    let cylinders = u32::try_from(total_bytes / bytes_per_cyl).map_err(|_| {
        CoreError::InvalidInput("Hard disk image size is too large to describe in an RDB".into())
    })?;
```

- [ ] **Step 4: Correct the two tests that pinned the round-up**, each with its reason.
  - `:1348-1357` (`the_last_partition_can_ask_for_whatever_is_left`): its own comment says *"a partition that claims a cylinder the image does not have is one an Amiga will read past the end of"* — the assertion contradicted it. Change `let cylinders = total.div_ceil(bytes_per_cyl) as u32;` to `let cylinders = (total / bytes_per_cyl) as u32;` and add above it: `// ART-309: whole cylinders only — the round-up this used to pin is the defect.`
  - `:2006-2007`: replace `assert!(layout.total_size >= 200 * 1024 * 1024);` with
    ```rust
        // ART-309: never more than asked, and less by under one cylinder.
        let asked = 200 * 1024 * 1024;
        assert!(layout.total_size <= asked && asked - layout.total_size < 16 * 63 * 512);
    ```

- [ ] **Step 5: Run the whole suite** — `cargo test --lib > E:\amiga\ProjeART\build\tmp\r1-t1.txt 2>&1` and read the `test result:` line. Every other failure is a test that pinned the round-up through an HDF's size (`core/hdf.rs:267-269` takes `layout.total_size` as the file size). For each: change the expectation to the floor value and write the reason in a comment (`// ART-309: an HDF is now a whole number of cylinders at or under the size asked for`). Do **not** change any production code to make those pass. List every test changed in the commit message.

- [ ] **Step 6: Run the task's tests again** — the new test passes; the suite prints `test result: ok`.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/core/rdb.rs <every test file changed in step 5, by name>
git commit -F <message file: "RDB: whole cylinders only, the last partition inside its area (ART-309)" + the list>
```

---

### Task 2: The card's label becomes bytes, once (ART-308, core)

**Files:**
- Create: `src-tauri/src/core/card/sizing.rs`
- Modify: `src-tauri/src/core/card/mod.rs:17-24` (add `pub mod sizing;` in alphabetical order, after `propose`)

**Interfaces:**
- Produces: `pub const CARD_MARGIN_PER_MILLE: u64 = 950;` `pub fn image_bytes_for_label(card_gb: u32) -> u64`

- [ ] **Step 1: Write the module with its failing test first** — `sizing.rs`:

```rust
//! How big a card image and its partitions are — the one-button card
//! design's § 4 (`docs/superpowers/specs/2026-09-11-one-button-card-design.md`).
//!
//! Every number here is measured or read from the code of a project that
//! works; the sources are in `docs/superpowers/notes/2026-09-11-card-layout-research.md`.

/// The share of a card's printed capacity an image may use, per mille.
///
/// **95 %, emu68hatcher's rule** — *"95% of decimal GB for SD card safety"*
/// (`rootrootde/emu68hatcher`, MIT, `config/partition_helpers.py:34-36`).
/// Cards carrying one label differ by up to ~2 %: two real "64 GB" cards
/// expose 62 534 975 488 and 63 864 569 856 bytes, and an image has to fit the
/// smaller. MultibootOS's last partition ends at 121.60 × 10⁹ on a 128 GB card
/// — exactly 95 %.
pub const CARD_MARGIN_PER_MILLE: u64 = 950;

/// The image size for a card sold as `card_gb` gigabytes — **decimal**, the
/// way every card maker counts (ART-308: this used to be `card_gb × 2³⁰`, and
/// ART's "64" was 4.7 × 10⁹ bytes past any 64 GB card).
pub fn image_bytes_for_label(card_gb: u32) -> u64 {
    u64::from(card_gb) * 1_000_000 * CARD_MARGIN_PER_MILLE
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The smallest real card measured for each label (fdisk output in the
    /// threads the research note cites). An image for that label must fit it.
    const SMALLEST_MEASURED: [(u32, u64); 3] = [
        (32, 31_914_983_424),
        (64, 62_534_975_488),
        (128, 127_865_454_592),
    ];

    #[test]
    fn a_card_image_is_ninety_five_percent_of_the_decimal_label() {
        assert_eq!(image_bytes_for_label(16), 15_200_000_000);
        assert_eq!(image_bytes_for_label(32), 30_400_000_000);
        assert_eq!(image_bytes_for_label(64), 60_800_000_000);
        assert_eq!(image_bytes_for_label(128), 121_600_000_000);
    }

    /// **ART-308.** The image for a label fits the smallest card of that label
    /// measured anywhere — and is no longer 64 GiB for "64".
    #[test]
    fn a_card_image_fits_the_smallest_real_card_of_its_label() {
        for (label, smallest) in SMALLEST_MEASURED {
            assert!(
                image_bytes_for_label(label) <= smallest,
                "{label} GB: {} > {smallest}",
                image_bytes_for_label(label)
            );
        }
        assert!(image_bytes_for_label(64) < 64 * 1024 * 1024 * 1024);
    }
}
```

Write the function body first as `u64::from(card_gb) * 1024 * 1024 * 1024` (today's behaviour), run, see both tests fail, then change it to the body above.

- [ ] **Step 2: Run it failing** — `cargo test --lib core::card::sizing` → the two tests FAIL (`17179869184 != 15200000000`).
- [ ] **Step 3: The real body** (above) — run again → PASS.
- [ ] **Step 4: Commit** — `git add src-tauri/src/core/card/sizing.rs src-tauri/src/core/card/mod.rs`, message "Card sizing: the label is decimal gigabytes, 95 % of it (ART-308, core)".

---

### Task 3: PFS3's own arithmetic, ported and proved identical

**Files:**
- Modify: `src-tauri/src/core/card/sizing.rs`

**Interfaces:**
- Produces:
  - `pub const PFS3_BLOCK: u64 = 512;`
  - `pub const PFS3_MIN_BYTES: u64 = 10 * 1024 * 1024;`
  - `pub const PFS3_MAX_BLOCKS: u64 = 213_021_952;` (→ `PFS3_MAX_BYTES = 109_067_239_424`)
  - `pub fn pfs3_reserved_block_bytes(total_blocks: u64) -> u32` (1024, 2048 or 4096)
  - `pub fn pfs3_num_reserved(total_blocks: u64) -> u32`
  - `pub fn pfs3_data_blocks(total_blocks: u64) -> u64`

- [ ] **Step 1: Write the in-memory device and the failing comparison test** — append to `mod tests`:

```rust
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// A block device that keeps only the blocks written to it — so a
    /// 100 GiB PFS3 format costs the reserved area in memory, not 100 GiB of
    /// disk. `libpfs3`'s own trait, `libpfs3::io::BlockDevice`.
    #[derive(Default)]
    struct MemDevice {
        blocks: Mutex<HashMap<u64, Vec<u8>>>,
    }

    impl libpfs3::io::BlockDevice for MemDevice {
        fn read_block(&self, block: u64, buf: &mut [u8]) -> libpfs3::error::Result<()> {
            match self.blocks.lock().unwrap().get(&block) {
                Some(data) => buf.copy_from_slice(data),
                None => buf.fill(0),
            }
            Ok(())
        }
        fn read_blocks(&self, block: u64, count: u32, buf: &mut [u8]) -> libpfs3::error::Result<()> {
            for i in 0..count as usize {
                self.read_block(block + i as u64, &mut buf[i * 512..(i + 1) * 512])?;
            }
            Ok(())
        }
        fn block_size(&self) -> u32 {
            512
        }
        fn write_block(&self, block: u64, data: &[u8]) -> libpfs3::error::Result<()> {
            self.blocks.lock().unwrap().insert(block, data[..512].to_vec());
            Ok(())
        }
        fn write_blocks(&self, block: u64, count: u32, data: &[u8]) -> libpfs3::error::Result<()> {
            for i in 0..count as usize {
                self.write_block(block + i as u64, &data[i * 512..(i + 1) * 512])?;
            }
            Ok(())
        }
        fn flush(&self) -> libpfs3::error::Result<()> {
            Ok(())
        }
    }

    fn format_in_memory(total_blocks: u64) -> (MemDevice, libpfs3::format::FormatResult) {
        let dev = MemDevice::default();
        let result = libpfs3::format::format_with_size(
            &dev,
            total_blocks,
            &libpfs3::format::FormatOptions { volume_name: "Test".into(), enable_deldir: false },
        )
        .unwrap();
        (dev, result)
    }

    /// The port is `libpfs3`'s own arithmetic or it is nothing: every size
    /// from the smallest PFS3 partition to the largest normal-mode one,
    /// formatted for real, and compared to the block.
    #[test]
    fn the_ported_reserved_area_matches_libpfs3s_own_format() {
        for total_blocks in [
            20_480u64,          // 10 MiB, the minimum
            204_800,            // 100 MiB
            2_097_152,          // 1 GiB
            8_388_608,          // 4 GiB
            10_485_760,         // 5 GiB, around the SUPERINDEX switch
            33_554_432,         // 16 GiB
            125_829_120,        // 60 GiB
            PFS3_MAX_BLOCKS,    // 101.58 GiB
        ] {
            let (_dev, result) = format_in_memory(total_blocks);
            assert_eq!(pfs3_num_reserved(total_blocks), result.num_reserved, "{total_blocks} blocks");
            assert_eq!(
                pfs3_reserved_block_bytes(total_blocks),
                result.reserved_blksize,
                "{total_blocks} blocks"
            );
            assert_eq!(pfs3_data_blocks(total_blocks), result.data_blocks, "{total_blocks} blocks");
        }
    }
```

If `libpfs3::error::Result` is not the crate's alias, use `Result<(), libpfs3::error::Error>` — `libpfs3-0.1.3/src/error.rs` says which; do not guess, read it.

- [ ] **Step 2: Add stubs so it compiles, and see it fail** — `pub fn pfs3_num_reserved(_: u64) -> u32 { 0 }`, `pub fn pfs3_reserved_block_bytes(_: u64) -> u32 { 0 }`, `pub fn pfs3_data_blocks(_: u64) -> u64 { 0 }`. Run `cargo test --lib the_ported_reserved_area_matches` → FAIL on the first size.

- [ ] **Step 3: The port** — replace the stubs:

```rust
/// One PFS3 block on a card: the partition's own block size.
pub const PFS3_BLOCK: u64 = 512;
/// The smallest PFS3 partition worth making (Emu68-Imager's own floor,
/// `Get-MinimumPartitionSizes.ps1`).
pub const PFS3_MIN_BYTES: u64 = 10 * 1024 * 1024;
/// The largest partition PFS3 supports in its normal mode — pfs3aio
/// `blocks.h:106-120`, *"normaldisk = 213.021.952 blocks of 512 byte"*. Past it
/// the format is experimental; both established imagers cap at 101 GiB.
pub const PFS3_MAX_BLOCKS: u64 = 213_021_952;
pub const PFS3_MAX_BYTES: u64 = PFS3_MAX_BLOCKS * PFS3_BLOCK;

// Ported from `libpfs3` 0.1.3 `format.rs` (itself pfs3aio's `format.c`), and
// held to it by `the_ported_reserved_area_matches_libpfs3s_own_format`.
const MAXSMALLBITMAPINDEX: u64 = 4;
const MAXBITMAPINDEX: u64 = 103;
const MAXSMALLDISK: u64 = (MAXSMALLBITMAPINDEX + 1) * 253 * 253 * 32;
const MAXNUMRESERVED: u32 = 4096 + 255 * 1024 * 8;

/// The size of one reserved block, from the partition's size.
pub fn pfs3_reserved_block_bytes(total_blocks: u64) -> u32 {
    let mut size = 1024;
    if total_blocks > MAXSMALLDISK {
        if total_blocks > (MAXBITMAPINDEX + 1) * 253 * 253 * 32 {
            size = 2048;
        }
        if total_blocks > (MAXBITMAPINDEX + 1) * 509 * 509 * 32 {
            size = 4096;
        }
    }
    size
}

/// How many reserved blocks a format sets aside — `calc_num_reserved`.
pub fn pfs3_num_reserved(total_blocks: u64) -> u32 {
    let resblocksize = pfs3_reserved_block_bytes(total_blocks);
    let mut taken: u32 = 32;
    let mut i: u64 = 2048;
    while i > 0 && i / 2 < total_blocks {
        let m: u32 = if i >= 512 * 2048 { 10 } else { 14 };
        taken += taken * m / 16;
        i = i.checked_shl(1).unwrap_or(0);
    }
    taken /= resblocksize / 1024;
    taken = taken.saturating_sub(1).min(MAXNUMRESERVED);
    taken = (taken + 31) & !0x1F;
    taken.max(32)
}

/// The blocks a fresh format leaves for file data.
pub fn pfs3_data_blocks(total_blocks: u64) -> u64 {
    let rescluster = u64::from(pfs3_reserved_block_bytes(total_blocks)) / PFS3_BLOCK;
    let reserved_area = rescluster * u64::from(pfs3_num_reserved(total_blocks)) + 2;
    total_blocks.saturating_sub(reserved_area)
}
```

**Check `MAXSMALLBITMAPINDEX` against `libpfs3-0.1.3/src/ondisk/mod.rs`** before relying on the `4` above — the test will say if it is wrong; if it fails on the 5 GiB row, read the constant, do not tune the number to pass.

- [ ] **Step 4: Run** → PASS for all eight sizes.
- [ ] **Step 5: Commit** — "Card sizing: PFS3's reserved area, ported from libpfs3 and proved identical".

---

### Task 4: Will this content fit — the estimate, proved by filling

**Files:**
- Modify: `src-tauri/src/core/card/sizing.rs`

**Interfaces:**
- Consumes: Task 3's functions.
- Produces:
  - `#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)] pub struct ContentMeasure { pub files: u64, pub directories: u64, pub data_blocks: u64, pub entry_bytes: u64 }`
  - `impl ContentMeasure { pub fn add_file(&mut self, name: &str, bytes: u64); pub fn add_directory(&mut self, name: &str); pub fn merge(&mut self, other: &ContentMeasure) }`
  - `pub fn pfs3_fits(total_blocks: u64, content: &ContentMeasure) -> bool`
  - `pub fn pfs3_fit_bytes(content: &ContentMeasure) -> Option<u64>` — smallest whole-cylinder PFS3 partition that holds it with the 5 % always-free kept; `None` past `PFS3_MAX_BYTES`
  - `pub const HEADROOM_PER_MILLE: u64 = 1250;` `pub const BYTES_PER_CYLINDER: u64 = 516_096;`
  - `pub fn content_partition_bytes(content: &ContentMeasure) -> Option<u64>` — fit × 1.25, whole cylinders, at least `PFS3_MIN_BYTES`; `None` past `PFS3_MAX_BYTES`

- [ ] **Step 1: The failing fill tests** — append to `mod tests`:

```rust
    /// Write `files` into a PFS3 partition of `total_blocks` in memory with
    /// `libpfs3`'s own writer, and return the free blocks left and the data
    /// blocks the format made — or the writer's own error.
    fn fill(
        total_blocks: u64,
        dirs: &[String],
        files: &[(String, usize)],
    ) -> Result<(u64, u64), libpfs3::error::Error> {
        let (dev, result) = format_in_memory(total_blocks);
        let vol = libpfs3::volume::Volume::from_device(Box::new(dev))?;
        let mut writer = libpfs3::writer::Writer::open(vol)?;
        for dir in dirs {
            writer.create_dir(dir)?;
        }
        for (path, size) in files {
            writer.write_file(path, &vec![0x5A; *size])?;
        }
        let vol = writer.into_volume();
        Ok((u64::from(vol.free_blocks()), result.data_blocks))
    }

    /// A content profile: `dirs` directories, `per_dir` files each, sized by
    /// `size_of(i)`, names 12 characters — measured into a `ContentMeasure`
    /// exactly as round 2's walker will measure host files.
    fn profile(dirs: usize, per_dir: usize, size_of: impl Fn(usize) -> usize)
        -> (Vec<String>, Vec<(String, usize)>, ContentMeasure) {
        let mut measure = ContentMeasure::default();
        let mut dir_names = Vec::new();
        let mut files = Vec::new();
        for d in 0..dirs {
            let dir = format!("Drawer{d:06}");
            measure.add_directory(&dir);
            for f in 0..per_dir {
                let name = format!("File{:08}", d * per_dir + f);
                let size = size_of(d * per_dir + f);
                measure.add_file(&name, size as u64);
                files.push((format!("{dir}/{name}"), size));
            }
            dir_names.push(dir);
        }
        (dir_names, files, measure)
    }

    /// Fill a partition of exactly the estimated size and require that it
    /// holds everything **and** keeps the twentieth pfs3aio holds back — the
    /// Amiga's handler refuses new files below it (`allocation.c:158`), and
    /// `libpfs3`'s writer does not (so the check is ours to make).
    fn assert_estimate_holds(label: &str, dirs: &[String], files: &[(String, usize)], m: &ContentMeasure) {
        let bytes = pfs3_fit_bytes(m).expect("within PFS3's range");
        let total_blocks = bytes / PFS3_BLOCK;
        let (free, data) = fill(total_blocks, dirs, files)
            .unwrap_or_else(|e| panic!("{label}: the estimated {bytes} bytes did not hold it: {e}"));
        assert!(free >= data / 20, "{label}: {free} free of {data}, under the always-free twentieth");
        // Not wasteful: at most a cylinder past the always-free line, plus
        // what the estimate's own rounding of directory space allows.
        let slack = free - data / 20;
        println!("{label}: {bytes} bytes, {free} free blocks, {slack} blocks past the reserve");
        assert!(
            slack * PFS3_BLOCK <= BYTES_PER_CYLINDER + (m.entry_bytes + 1024 * (m.directories + 1)),
            "{label}: {slack} blocks of slack is more than the estimate should leave"
        );
    }

    /// The owner's 3.9 tree: 4 599 files, median 521 bytes, half of them 512
    /// or less (research note § 5). Synthetic, same shape.
    #[test]
    fn a_tree_shaped_like_the_owners_fits_its_estimate() {
        let (dirs, files, m) = profile(277, 17, |i| match i % 4 {
            0 | 1 => 400,
            2 => 2_000,
            _ => 18_000,
        });
        assert_estimate_holds("3.9-shaped tree", &dirs, &files, &m);
    }

    /// AGS1 carries 140 602 files at 23 KB average; directory space comes out
    /// of PFS3's reserved area, so many small files are what could run it out.
    #[test]
    fn many_small_files_do_not_run_out_of_reserved_blocks() {
        let (dirs, files, m) = profile(200, 100, |_| 200);
        assert_estimate_holds("20 000 small files", &dirs, &files, &m);
    }

    #[test]
    fn a_few_large_files_fit_their_estimate() {
        let (dirs, files, m) = profile(2, 10, |_| 4 * 1024 * 1024);
        assert_estimate_holds("20 large files", &dirs, &files, &m);
    }

    #[test]
    fn a_content_partition_is_a_quarter_larger_than_its_fit_and_whole_cylinders() {
        let (_, _, m) = profile(277, 17, |_| 2_000);
        let fit = pfs3_fit_bytes(&m).unwrap();
        let part = content_partition_bytes(&m).unwrap();
        assert_eq!(part % BYTES_PER_CYLINDER, 0);
        assert!(part >= fit * HEADROOM_PER_MILLE / 1000);
        assert!(part >= PFS3_MIN_BYTES);
        assert!(part < fit * HEADROOM_PER_MILLE / 1000 + BYTES_PER_CYLINDER);
    }

    #[test]
    fn content_past_pfs3s_largest_partition_has_no_size() {
        let mut m = ContentMeasure::default();
        m.add_file("Huge", PFS3_MAX_BYTES);
        assert_eq!(pfs3_fit_bytes(&m), None);
        assert_eq!(content_partition_bytes(&m), None);
    }
```

- [ ] **Step 2: Stubs, and see them fail** — `ContentMeasure` with empty `add_file`/`add_directory`/`merge`, `pfs3_fits` returning `true`, `pfs3_fit_bytes` returning `Some(PFS3_MIN_BYTES)`, `content_partition_bytes` returning `Some(PFS3_MIN_BYTES)`. Run `cargo test --lib core::card::sizing` → the fill tests FAIL (the writer runs out of room, or the always-free assertion fires).

- [ ] **Step 3: The estimate**:

```rust
/// Bytes a cylinder holds in ART's fixed RDB geometry (`core::rdb`).
pub const BYTES_PER_CYLINDER: u64 = 16 * 63 * 512;
/// A content partition's room to grow, per mille of its fit — MultibootOS's
/// game partitions are 76–80 % full (research note § 5).
pub const HEADROOM_PER_MILLE: u64 = 1250;

/// What host content will cost on PFS3, counted the way PFS3 spends it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ContentMeasure {
    pub files: u64,
    pub directories: u64,
    /// Every file rounded up to whole 512-byte blocks.
    pub data_blocks: u64,
    /// Directory entries: 17 fixed bytes, the name, a comment-length byte,
    /// padded to even (pfs3aio `blocks.h:327-340`).
    pub entry_bytes: u64,
}

fn entry_bytes(name: &str) -> u64 {
    // Latin-1 on the Amiga side: one byte a character.
    let raw = 17 + name.chars().count() as u64 + 1;
    raw + raw % 2
}

impl ContentMeasure {
    pub fn add_file(&mut self, name: &str, bytes: u64) {
        self.files += 1;
        self.data_blocks += bytes.div_ceil(PFS3_BLOCK);
        self.entry_bytes += entry_bytes(name);
    }
    pub fn add_directory(&mut self, name: &str) {
        self.directories += 1;
        self.entry_bytes += entry_bytes(name);
    }
    pub fn merge(&mut self, other: &ContentMeasure) {
        self.files += other.files;
        self.directories += other.directories;
        self.data_blocks += other.data_blocks;
        self.entry_bytes += other.entry_bytes;
    }
}

/// Reserved blocks the content needs beyond the format's own: directory
/// blocks (at least one per directory, entries packed into 1 KB blocks with a
/// 20-byte header), anode blocks (12 bytes an anode, one per file and
/// directory), counted in 1 KB units and rounded to the reserved block size.
fn reserved_needed(total_blocks: u64, content: &ContentMeasure) -> u64 {
    let dir_blocks = (content.directories + 1) + content.entry_bytes.div_ceil(1024 - 20);
    let anode_blocks = (content.files + content.directories + 16).div_ceil(80) + 1;
    // The format's own use of the reserved area (bitmap, bitmap index, anode
    // index, root) is already inside `pfs3_num_reserved`'s count, and what it
    // leaves free is what this is checked against.
    let ones_k = dir_blocks + anode_blocks;
    let per = u64::from(pfs3_reserved_block_bytes(total_blocks)) / 1024;
    ones_k.div_ceil(per)
}

/// Whether `content` fits a fresh PFS3 partition of `total_blocks`, keeping
/// the always-free twentieth pfs3aio holds back on the Amiga.
pub fn pfs3_fits(total_blocks: u64, content: &ContentMeasure) -> bool {
    let data = pfs3_data_blocks(total_blocks);
    let usable = data - data / 20;
    // The format itself spends part of the reserved area; the bitmap alone is
    // data/(253×32) blocks. Leave that and a margin of 8 before counting ours.
    let format_own = data.div_ceil(253 * 32) + 8;
    let reserved_free = u64::from(pfs3_num_reserved(total_blocks)).saturating_sub(format_own);
    content.data_blocks <= usable && reserved_needed(total_blocks, content) <= reserved_free
}

/// The smallest whole-cylinder PFS3 partition `content` fits — `None` past
/// PFS3's normal-mode maximum.
pub fn pfs3_fit_bytes(content: &ContentMeasure) -> Option<u64> {
    let blocks_per_cyl = BYTES_PER_CYLINDER / PFS3_BLOCK;
    let min_cyl = PFS3_MIN_BYTES.div_ceil(BYTES_PER_CYLINDER);
    let max_cyl = PFS3_MAX_BYTES / BYTES_PER_CYLINDER;
    if !pfs3_fits(max_cyl * blocks_per_cyl, content) {
        return None;
    }
    // Fitting only grows with size, so the first size that fits is found by
    // bisection over whole cylinders.
    let (mut lo, mut hi) = (min_cyl, max_cyl);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if pfs3_fits(mid * blocks_per_cyl, content) {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    Some(lo * BYTES_PER_CYLINDER)
}

/// A content partition's size: its fit, a quarter again, whole cylinders.
pub fn content_partition_bytes(content: &ContentMeasure) -> Option<u64> {
    let fit = pfs3_fit_bytes(content)?;
    let wanted = (fit * HEADROOM_PER_MILLE / 1000).max(PFS3_MIN_BYTES);
    let bytes = wanted.div_ceil(BYTES_PER_CYLINDER) * BYTES_PER_CYLINDER;
    (bytes <= PFS3_MAX_BYTES).then_some(bytes)
}
```

- [ ] **Step 4: Run** `cargo test --lib core::card::sizing -- --nocapture` → all PASS; **copy the three printed `… blocks past the reserve` lines into the commit message** — they are the calibration's measured margins. If a fill test fails with *"out of reserved blocks"*, the reserved model is short: raise `reserved_needed` by what the failure shows (log `pfs3_num_reserved` and the count at that size), never by loosening the test.

- [ ] **Step 5: Commit** — "Card sizing: PFS3 footprint of host content, proved by filling libpfs3 in memory".

---

### Task 5: The card image plan — System, content, Work

**Files:**
- Modify: `src-tauri/src/core/card/sizing.rs`

**Interfaces:**
- Consumes: Tasks 2–4; `crate::core::card::propose::{MEASURED_BOOT_BYTES, MEASURED_SYSTEM_MB, MEASURED_BUFFERS}`; `crate::core::rdb::{PartitionSpec, AmigaHardDiskFs, create_rdb_layout}`.
- Produces:
  - `pub struct RequestedPartition { pub volume_name: String, pub content: Option<ContentMeasure>, pub floor_bytes: u64 }` — `content: None` is System (the first) or Work (the last, added by the planner); `floor_bytes` is the Advanced override (0 = none)
  - `#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)] #[serde(tag = "refusal", rename_all = "kebab-case", rename_all_fields = "camelCase")] pub enum SizingRefusal { DoesNotFit { needed: u64, available: u64 }, PartitionTooLarge { volume_name: String, bytes: u64 }, CardTooSmall { card_gb: u32 } }`
  - `#[derive(Debug, Clone, serde::Serialize)] pub struct PlannedPartition { pub drive_name: String, pub volume_name: String, pub bytes: u64, pub spec: PartitionSpec }`
  - `#[derive(Debug, Clone, serde::Serialize)] pub struct CardImagePlan { pub image_bytes: u64, pub area_bytes: u64, pub partitions: Vec<PlannedPartition> }`
  - `pub fn plan_card_image(card_gb: u32, content: &[RequestedPartition]) -> Result<CardImagePlan, SizingRefusal>` — `content` is the user's partitions **between** System and Work, in order

- [ ] **Step 1: The failing tests** — `create_rdb_layout` is not used by the module's production code, so `use super::*` does not bring it; add `use crate::core::rdb::create_rdb_layout;` at the top of `mod tests`:

```rust
    fn games(mb: u64) -> RequestedPartition {
        let mut m = ContentMeasure::default();
        m.add_file("Game", mb * 1024 * 1024);
        RequestedPartition { volume_name: "Games".into(), content: Some(m), floor_bytes: 0 }
    }

    #[test]
    fn a_card_is_system_then_the_users_partitions_then_work() {
        let plan = plan_card_image(64, &[games(4000)]).unwrap();
        let names: Vec<&str> = plan.partitions.iter().map(|p| p.volume_name.as_str()).collect();
        assert_eq!(names, ["System", "Games", "Work"]);
        let drives: Vec<&str> = plan.partitions.iter().map(|p| p.drive_name.as_str()).collect();
        assert_eq!(drives, ["SDH0", "SDH1", "SDH2"]);
        assert!(plan.partitions[0].spec.bootable && !plan.partitions[1].spec.bootable);
        assert_eq!(plan.partitions[0].bytes, u64::from(MEASURED_SYSTEM_MB) * 1024 * 1024);
        assert_eq!(plan.image_bytes, image_bytes_for_label(64));
    }

    /// The plan is only a plan if the RDB writer builds it: every partition
    /// lands at its size or larger, and the last ends inside the area.
    #[test]
    fn the_plan_is_one_the_rdb_writer_builds_inside_its_area() {
        let plan = plan_card_image(32, &[games(4000), games(900)]).unwrap();
        let specs: Vec<PartitionSpec> = plan.partitions.iter().map(|p| p.spec.clone()).collect();
        let layout = create_rdb_layout(plan.area_bytes, &specs, &[]).unwrap();
        assert!(layout.total_size <= plan.area_bytes);
        assert!(MEASURED_BOOT_BYTES + plan.area_bytes <= plan.image_bytes);
    }

    #[test]
    fn content_that_does_not_fit_is_refused_with_both_numbers() {
        let err = plan_card_image(16, &[games(20_000)]).unwrap_err();
        let SizingRefusal::DoesNotFit { needed, available } = err else { panic!("{err:?}") };
        assert!(needed > available);
    }

    #[test]
    fn work_past_pfs3s_largest_partition_is_split() {
        let plan = plan_card_image(128, &[]).unwrap();
        let names: Vec<&str> = plan.partitions.iter().map(|p| p.volume_name.as_str()).collect();
        assert_eq!(names, ["System", "Work", "Work_1"]);
        assert!(plan.partitions.iter().all(|p| p.bytes <= PFS3_MAX_BYTES));
    }

    #[test]
    fn an_override_is_a_floor_never_a_way_past_the_card() {
        let mut g = games(100);
        g.floor_bytes = 5 * 1024 * 1024 * 1024;
        let plan = plan_card_image(16, &[g]).unwrap();
        assert!(plan.partitions[1].bytes >= 5 * 1024 * 1024 * 1024);
        let mut huge = games(100);
        huge.floor_bytes = 40 * 1000 * 1000 * 1000;
        assert!(matches!(plan_card_image(16, &[huge]), Err(SizingRefusal::DoesNotFit { .. })));
    }
```

- [ ] **Step 2: Stub `plan_card_image` returning `Err(SizingRefusal::CardTooSmall { card_gb })`; run; see all five FAIL.**

- [ ] **Step 3: The planner**:

```rust
use super::propose::{MEASURED_BOOT_BYTES, MEASURED_BUFFERS, MEASURED_SYSTEM_MB};
use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec};

/// Two cylinders at the front of the area hold the RDB (`core::rdb`).
const RDB_RESERVED_BYTES: u64 = 2 * BYTES_PER_CYLINDER;

fn spec(drive: &str, bytes: u64, rest: bool, bootable: bool) -> PartitionSpec {
    PartitionSpec {
        drive_name: drive.to_string(),
        fs_type: AmigaHardDiskFs::Pfs3DirectScsi,
        // `core::rdb` rounds MB up to cylinders; floor here so the writer's
        // round-up lands on (never past) the cylinders planned.
        size_mb: if rest { 0 } else { u32::try_from(bytes / (1024 * 1024)).unwrap_or(u32::MAX) },
        bootable,
        boot_priority: 0,
        num_buffers: MEASURED_BUFFERS,
    }
}

pub fn plan_card_image(
    card_gb: u32,
    content: &[RequestedPartition],
) -> Result<CardImagePlan, SizingRefusal> {
    let image_bytes = image_bytes_for_label(card_gb);
    let area_bytes = image_bytes
        .checked_sub(MEASURED_BOOT_BYTES)
        .ok_or(SizingRefusal::CardTooSmall { card_gb })?;
    let usable = (area_bytes / BYTES_PER_CYLINDER) * BYTES_PER_CYLINDER - RDB_RESERVED_BYTES;

    let system = (u64::from(MEASURED_SYSTEM_MB) * 1024 * 1024).div_ceil(BYTES_PER_CYLINDER)
        * BYTES_PER_CYLINDER;
    let mut sized = vec![("System".to_string(), system)];
    for part in content {
        let from_content = match &part.content {
            Some(m) => content_partition_bytes(m).ok_or_else(|| SizingRefusal::PartitionTooLarge {
                volume_name: part.volume_name.clone(),
                bytes: m.data_blocks * PFS3_BLOCK,
            })?,
            None => PFS3_MIN_BYTES,
        };
        let bytes = from_content.max(part.floor_bytes).div_ceil(BYTES_PER_CYLINDER) * BYTES_PER_CYLINDER;
        sized.push((part.volume_name.clone(), bytes));
    }

    let needed: u64 = sized.iter().map(|(_, b)| b).sum::<u64>() + PFS3_MIN_BYTES;
    if needed > usable {
        return Err(SizingRefusal::DoesNotFit { needed, available: usable });
    }

    // Work: the rest, split every `PFS3_MAX_BYTES` like both imagers.
    let mut rest = usable - (needed - PFS3_MIN_BYTES);
    let mut works = Vec::new();
    while rest > PFS3_MAX_BYTES {
        let piece = (PFS3_MAX_BYTES / BYTES_PER_CYLINDER) * BYTES_PER_CYLINDER;
        works.push(piece);
        rest -= piece;
    }
    works.push(rest);

    let count = sized.len() + works.len();
    let mut partitions = Vec::with_capacity(count);
    for (i, (name, bytes)) in sized.into_iter().enumerate() {
        let drive = format!("SDH{i}");
        partitions.push(PlannedPartition {
            spec: spec(&drive, bytes, false, i == 0),
            drive_name: drive,
            volume_name: name,
            bytes,
        });
    }
    for (j, bytes) in works.into_iter().enumerate() {
        let i = partitions.len();
        let drive = format!("SDH{i}");
        let name = if j == 0 { "Work".to_string() } else { format!("Work_{j}") };
        partitions.push(PlannedPartition {
            spec: spec(&drive, bytes, i + 1 == count, false),
            drive_name: drive,
            volume_name: name,
            bytes,
        });
    }
    Ok(CardImagePlan { image_bytes, area_bytes, partitions })
}
```

- [ ] **Step 4: Run → PASS.** If `the_plan_is_one_the_rdb_writer_builds_inside_its_area` fails because `core::rdb`'s MB→cylinder round-up over-subscribes, keep the planner's floor-to-MB and make the planner reserve one cylinder per partition for it — never loosen the test.
- [ ] **Step 5: Commit** — "Card sizing: the card image plan — System, the user's partitions, Work split at 101 GiB".

---

### Task 6: The card builder sends the label (ART-308, wired)

**Files:**
- Modify: `src-tauri/src/commands/card.rs` — `CardBuildRequest` (add field after `total_bytes` at `:116`), `card_spec` (`:345-347`), `card_propose_table` (`:452-457`); its tests' `request()` helper (`:969-998`)
- Modify: `src/lib/cardBuild.ts:46` (request type), `:424-431` (`cardProposeTable`)
- Modify: `src/components/osbuilder/CardBuilder.tsx:324`, `:348`
- Test: `src-tauri/src/commands/card.rs` tests; `src/components/osbuilder/CardBuilder.test.tsx`

**Interfaces:**
- Consumes: `core::card::sizing::image_bytes_for_label`.
- Produces: `CardBuildRequest.card_gb: Option<u32>` (`#[serde(default)]`); when `Some`, the card is `image_bytes_for_label(card_gb)` and `total_bytes` is ignored. `card_propose_table(card_gb: u32, fs_type, rom_major)`.
- **Not changed, and why:** `distro_check_card` — `OsBuilder.tsx:295` multiplies by 2³⁰ and `core/distro/mod.rs:173` divides by 2³⁰ again, so the check already compares the label with the profile's `min_card_gb`. Leave it; the commit message says so.

- [ ] **Step 1: The failing Rust test** — in `card.rs` tests:

```rust
    /// **ART-308, wired.** The screen sends the label; the bytes are Rust's.
    #[test]
    fn a_card_named_by_its_label_is_ninety_five_percent_of_it() {
        let (_guard, dir) = scratch("card-label");
        let mut req = request(&emu68_zip(&dir), &dir.join("card.img"));
        req.card_gb = Some(64);
        req.total_bytes = 64 * 1024 * 1024 * 1024; // what the screen used to send
        let spec = card_spec(&req, Vec::new()).unwrap();
        assert_eq!(spec.total_bytes, 60_800_000_000);
    }
```

Add `card_gb: None,` to the `request()` helper. Run → FAIL (`68719476736 != 60800000000`).

- [ ] **Step 2: The field and its use**:

```rust
    /// The card's printed size, when the screen chose one by label. The bytes
    /// are then [`image_bytes_for_label`]'s — decimal gigabytes, 95 % of them —
    /// and `total_bytes` is ignored (ART-308: the screen used to multiply by
    /// 2³⁰ itself, and "64" was past any 64 GB card).
    #[serde(default)]
    pub card_gb: Option<u32>,
```

In `card_spec`: `total_bytes: request.card_gb.map(crate::core::card::sizing::image_bytes_for_label).unwrap_or(request.total_bytes),`.
`card_propose_table(card_gb: u32, fs_type, rom_major)` calls `propose(image_bytes_for_label(card_gb), fs_type, rom_major)`. Run → PASS.

- [ ] **Step 3: The frontend** — `cardBuild.ts`: add `card_gb?: number | null;` to the request type with a one-line doc; `cardProposeTable(cardGb: number, fsType, romMajor)` invoking `{ cardGb, fsType, romMajor }`. `CardBuilder.tsx:324` → `cardProposeTable(cardGb, fsType, romMajor)`; `:348` → `total_bytes: 0, card_gb: cardGb,`. Update `CardBuilder.test.tsx`'s expectations on `proposeMock` to the label (`toHaveBeenCalledWith(64, …)`) and on the built request (`card_gb: 64`). Run `pnpm vitest run src/components/osbuilder/CardBuilder.test.tsx src/lib/cardBuild.test.ts` → PASS; `pnpm lint` → clean.

- [ ] **Step 4: Commit** — "Card builder: the card is named by its label, sized in Rust (ART-308)".

---

### Task 7: Docs, mutations, the whole suite

- [ ] **Step 1: Mutations** (script in the session scratchpad, the house pattern: backup by absolute path, one exact replacement asserted to occur once, run the filtered tests, restore with `shutil.copyfile`, compare byte for byte). Each must be killed by the named test:

| Mutation | Killer |
|---|---|
| `rdb.rs`: `total_bytes / bytes_per_cyl` → `total_bytes.div_ceil(bytes_per_cyl)` | `the_last_partition_ends_inside_an_area_that_is_not_whole_cylinders` |
| `sizing.rs`: `* 1_000_000 * CARD_MARGIN_PER_MILLE` → `* 1024 * 1024 * 1024` | `a_card_image_fits_the_smallest_real_card_of_its_label` |
| `sizing.rs`: `let usable = data - data / 20;` → `let usable = data;` | `a_tree_shaped_like_the_owners_fits_its_estimate` |
| `sizing.rs`: `&& reserved_needed(total_blocks, content) <= reserved_free` → `` | `many_small_files_do_not_run_out_of_reserved_blocks` |
| `sizing.rs`: `if needed > usable {` → `if false {` | `content_that_does_not_fit_is_refused_with_both_numbers` |
| `sizing.rs`: `while rest > PFS3_MAX_BYTES {` → `while false {` | `work_past_pfs3s_largest_partition_is_split` |
| `card.rs`: `request.card_gb.map(` … → `None.map(` … | `a_card_named_by_its_label_is_ninety_five_percent_of_it` |

A survivor is either a weak guard or a wrong mutation; say which, and act on it.

- [ ] **Step 2: The whole suite, twice** — `cargo test --lib` ×2 (`test result: ok` both), `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `pnpm lint`, `pnpm test`, `python scripts/control-byte-sweep.py`, `scratch-root-sweep.py`, `scratch-guard-sweep.py`.

- [ ] **Step 3: Docs** — ISSUES: move ART-308 and ART-309 to Fixed with their tests, the red lines, the mutations and the calibration margins; CHANGELOG *Fixed*: "Card images fit the card they are named for" and "The last partition of a card no longer ends past the card"; STATUS: the round's line in *Start here* (updated in place) and the test counts; session log row. Commit — "docs: one-button card round 1 — sizing, ART-308 and ART-309 fixed".

---

## Rounds after this one (each gets its own plan, written when the previous lands)

2. **Content** — `core/card/content.rs`: classify a source (folder · archive · WHDLoad HDF · ADF · unusable); measure it into a `ContentMeasure` (and its non-ASCII names); prepare it into a staging directory (archive gate; a WHDLoad HDF's drawer + `.info` via `extract_from_volume`, the boot scaffold left behind). `core/whdload/system.rs`: WHDLoad from the material by `$VER:` into a tree; Kickstarts via `rom::offer`/`rom::place`. `core/preload`: a partition fed by several folders. **`SizingRefusal::DoesNotFit` must name the partition that does not fit** (spec §3/§4/§6/§7). Today it carries only `needed` and `available`; deferred here by the round 1 final review (I4).
3. **The run** — `card_os_prepare` (phases 4–5) and `card_os_build` (6–8) as jobs; `<image>.partial` renamed after the check; `run_with_fallback` reachable; free-space refusal; the manifest's contents. End to end, read back by ART and by hst-imager.
4. **The screen** — `buildSession` `CardTarget`; the Machine tab's card destination and partition rows on the one drop pipeline; the Build tab's phases and endings in both catalogues.
5. **The owner's material, mutations, docs, a package** — the env-gated real-material test; the round's mutation table; FEATURES, STATUS, CHANGELOG; `pnpm tauri build` for the owner to write and boot.
