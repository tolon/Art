# Known Issues & Technical Debt

Every defect found in ART gets an ID here — fixed or not. The ID is stable and
never reused, so a commit message, a code comment or an error message can point
at one and stay meaningful.

Spec §68 requires user-facing errors to carry an identifier rather than an
opaque code. These IDs are the registry those identifiers come from.

**Format:** `ART-NNN` · severity · one-line claim · where · how it fails ·
what fixed it (with the test that proves it).

**Severity:**

| | Meaning |
|---|---|
| 🔴 **Critical** | Can destroy or corrupt user data, or crash the application. |
| 🟠 **High** | Wrong results, or a spec rule broken in a way users will notice. |
| 🟡 **Medium** | Incorrect behaviour with limited blast radius. |
| 🔵 **Low** | Hygiene, dead code, developer-facing friction. |

---

## Open

**ART-043** 🟠 **A partition inside a small image is written at the wrong offset**
`commands/volume_write.rs` · The whole-file strategy is chosen by the *file's*
size, but it then builds its `VecDevice` from the whole file and opens the
writer at offset `0`, while the geometry it was given describes a **partition**
that may start megabytes into that file. For any RDB image of 16 MiB or less
this reads and writes volume-relative block numbers as if they were
file-absolute ones: the root block lands in the middle of the partition's data.
In practice the first read then fails with something unhelpful ("block N is not
a directory") rather than corrupting anything, and — since ART-042 — a result
that did somehow get written is refused by the whole-image gate before it
reaches the file. So the user's data is not at risk today; the strategy choice
is simply wrong. The fix is to pick the block-journal strategy whenever the
volume does not start at byte 0 and cover the whole file, or to give the
whole-file branch the partition's slice and write it back at its offset. Needs
its own task and its own fixture (a small RDB image with a formatted
partition) — no test covers it today, which is why it survived this long.

**ART-050** 🟡 **The §92 pre-flight gate does not check bitmap consistency or hash-chain integrity**
`core/adf/validate.rs::validate_image` · `commit_whole_file` (ART-042) refuses
a write when `validate_image` finds a `Problem`, but `validate_image` only
covers the bootblock signature, its checksum, the block count and the root
block's type. It does not walk the bitmap against what is actually allocated
and does not walk a hash chain for consistency, so an operation that leaves
two files owning the same block, or an entry linked into the wrong bucket,
still passes the gate and commits. `CHANGELOG.md`'s entry for ART-042 was
corrected in this pass to stop implying broader coverage than this. Deepening
`validate_image` to catch these is real work, not a flag to flip.

**ART-049** 🟡 **`create.rs`'s oracle hook and `VolumeWriter::open` agree by hand, not by a check**
`core/adf/create.rs` (`oracle_export`), `core/volume/write/mod.rs`
(`VolumeWriter::open`) · The oracle export hook hardcodes `DOS\x01` geometry
while formatting with `FileSystemType::Ffs` — the two are only consistent
because the hook's author chose them to match. `VolumeWriter::open` does not
cross-check the geometry it is handed against the dostype the image's own
bootblock declares, so a caller that passed a geometry for one filesystem
against an image formatted as another would not be refused at the boundary
that is supposed to catch exactly that. No test exercises the disagreement
because nothing in the suite constructs one. Needs a guard in
`VolumeWriter::open` and a fixture that deliberately mismatches geometry and
bootblock.

**ART-048** 🔵 **A source comment still describes a module that no longer exists**
`commands/adf.rs:369` (a test's doc comment) · Written for task 8's routing
tests, before `core/adf/mutate` was deleted in task 10 (`fbd35ef`): "a later
task deleting `core/adf/mutate` would be unsafe" now describes something
already done. `docs/architecture.md`'s reference to `mutate_disk_file` carried
the same staleness and is corrected in this pass — the one exception to
"intent docs are not rewritten" that CLAUDE.md names, because the module it
points at is gone. The source comment is left for whoever next touches that
test; this pass changes no production code.

**ART-047** 🔵 **Dead code that clippy cannot see**
`core/adf/blocks.rs:81-99` · `block_slice`, `block_slice_mut`, `read_u32_at`
and `write_u32_at` have no callers left outside their own tests —
`core/adf/mutate.rs` was the last production caller, and task 10 (`fbd35ef`)
deleted it. `lib.rs`'s `#![allow(dead_code)]` (CLAUDE.md's one permitted
blanket allow) means clippy does not flag them. Either give them a real
caller or remove them; audited bounds-checking code that production no longer
reaches is exactly the kind of gap ART-020 exists to stop CI from hiding.

**ART-046** 🔵 **A doc comment claims a guarantee the public API does not give**
`core/volume/write/mod.rs:684` · The comment above the test-only
`commit_blocks` says "every public operation here deliberately cannot get
into that state" — a volume whose touched blocks are each individually
well-formed but whose structure is wrong. `add_file` on an image whose bitmap
is flagged valid but marks the root block free reaches exactly that state
through the public API: the allocator hands out the root block, `add_file`
writes a well-formed file there, and every touched block's own checksum is
fine. The gate that catches it (ART-042's whole-image `validate_image` in
`commit_whole_file`) is stronger than its own documentation claims — and that
comment is the argument someone will use to delete the gate as redundant.
Needs either a narrower claim or a test proving the public API really cannot
reach that state; today it can.

Missing features are not defects — see [FEATURES.md](FEATURES.md) for what is
not built yet, and [STATUS.md](STATUS.md) for what is scheduled.

Every module with working logic has now been audited. The remaining `core`
modules are stubs that only return `NotImplemented` (`recovery.rs`,
`conversion.rs`, `binary.rs`, `validation.rs`) or hold types with no logic
(`compatibility.rs`) — see [FEATURES.md](FEATURES.md) for their planned state.

Two areas were reviewed and found sound, and are recorded here so nobody
re-audits them without reason:

- `core/analysis.rs` — the hex reader clamps both offset and length, and the
  signature scan guards its window.
- `core/profile.rs` — preset data only, no parsing of untrusted input.

---

## Fixed

### Phase 0a

**ART-037** 🔴 **ADF Studio could not open any bootable ADF**
`core/adf/bootblock.rs`, `core/adf/mod.rs` · The parser read bytes 8..11 of the
boot block as a "root block pointer". An AmigaDOS boot block has no such field:
0..3 is the DOS type, 4..7 the checksum, and 8 onwards is boot code. ART
therefore read 68000 machine code as a block number and refused every bootable
disk with `root block has type <nonsense>`. Invisible to ART's own tests
because every fixture ART builds has zeros where a real disk has boot code —
and invisible in day-to-day use because the file manager, which runs on
`core/volume`, opened the same disks perfectly.
→ The root block is computed, `total_blocks / 2` via `root_block_of()`, the
way `VolumeGeometry::root_block_for` already did. Pinned by
`a_bootable_image_opens_because_the_root_block_is_computed` and by an oracle
check that has `xdftool` write a bootable floppy — its own boot code,
checksum-legal — for ART to open.

**ART-038** 🟡 **HD ADFs reported half their capacity**
`core/adf/mod.rs` · `info()` computed capacity and parsed the bitmap with
`DD_TOTAL_BLOCKS`, so every number it reported for a 1.76 MB image was a
floppy-shaped guess. Same root cause as ART-037: `core/adf` predated the
Stage R geometry and never adopted it.
→ Both come from the image's own length, via `total_blocks_of()`. Pinned by
`an_hd_image_reports_its_real_capacity`.

**ART-039** 🟡 **Disabled controls were indistinguishable from active ones**
`src/styles/global.css` · The word `disabled` appeared in no stylesheet, so a
disabled primary button kept its accent fill. On the WHDLoad screen a
correctly-refused install showed a solid blue "Install to the disk" button
that did nothing when clicked.
→ `:disabled` and `:focus-visible` styles for buttons and form controls.

**ART-040** 🟡 **Content was clipped instead of scrolled, and nothing scaled**
`src/components/layout/layout.css` · `.app-main` is a grid item with the
default `min-height: auto`, so it grew to fit its content and `.app-content`'s
`overflow: auto` never engaged; the shell's `overflow: hidden` then clipped the
excess. `min-height: 0` appeared nowhere in the codebase. Separately, the
application had zero `@media` queries and eleven pages carried hand-written
`maxWidth` values.
→ The `min-height: 0` chain, breakpoints, and one central content width.

**ART-044** 🟠 **WHDLoad and Aminet installs could land a package partially with no warning**
`commands/sources.rs` · `sources_install_adf` and `sources_install_volume`
both wrote through `run_copy_in_folder`, which lands what fits and reports the
rest as `skipped` — the right contract for the general-purpose F5 copy, wrong
for an install. Neither install command ran a pre-flight fit check before
writing, so a package that did not fit was discovered mid-loop: a WHDLoad pack
whose `.slave` ended up in `skipped` is a broken, non-bootable result the user
was never warned about (§92: explain before modify).
→ `plan_copy_in_folder` (`commands/volume_write.rs`) mirrors
`volume_plan_copy`'s own pre-flight against an already-built `HostFolder` and
refuses atomically, with the real block numbers, before either install command
writes anything. Pinned by
`installing_a_package_that_does_not_fit_is_refused_without_touching_the_image`
and `installing_a_package_that_fits_installs_completely`.

**ART-045** 🟠 **A transient re-read after a successful write could report it as failed**
`commands/adf.rs` · `MutationOutcome::from_write` re-opened the image with
`AdfImage::open()` after `with_volume` had already committed the write and
closed its handle. That second, independent open could race an external lock
(antivirus scan-on-close, a search indexer) taken the moment the first handle
released, turning an already-durable, successful write into a reported
failure — and losing the backup path along with it, contradicting the
guarantee that the user is always told where the previous version went.
→ `VolumeWriter::all_bytes()` (`core/volume/write/mod.rs`) reads the volume
block by block through the session's own still-open device; `info_from_session`
builds `AdfInfo` from that, called from *inside* every `with_volume` closure,
so a failure there aborts before any byte reaches disk rather than after.
`MutationOutcome::from_write` is deleted, not disarmed. Pinned by
`a_failure_after_the_mutation_never_reaches_the_disk` plus per-command routing
tests (`creating_a_directory_goes_through_the_volume_writer`,
`deleting_an_entry_goes_through_the_volume_writer`,
`renaming_an_entry_goes_through_the_volume_writer`).

### Stage W

**ART-042** 🔴 **A write that produced an invalid volume was committed anyway**
`commands/volume_write.rs` · The retired `core/adf/mutate.rs::mutate_disk_file`
validated the finished image in memory before writing it. `with_volume`, which
replaced it, ran the operation and went straight to `guarded_write` — under a
comment that said the opposite. The only surviving check was
`core/volume/write::validate_touched`, which sees just the blocks the operation
touched and, of those, only their checksums. A write that left every touched
block well-formed and the volume structurally wrong — a root block that is no
longer a header block, an image that stopped being an AmigaDOS volume —
replaced the user's file silently, and the backup it took was of the last good
version, so the damage was recoverable only if the user noticed.
→ `commit_whole_file` validates the whole in-memory image at all three
whole-file commit points and refuses before `guarded_write` is reached. Only
`Problem` findings refuse; a warning describes an image that was already like
that. Pinned by `a_write_that_would_not_validate_never_reaches_the_file`
(refused, and the file byte-for-byte unchanged) and
`a_valid_write_still_commits_through_the_gate`.

**ART-041** 🟠 **Validation measured every image against a DD floppy**
`core/adf/validate.rs` · `validate` compared `image.len()` against
`DD_TOTAL_BLOCKS * BLOCK_SIZE` and warned on anything else, so every HD floppy
and every hard-disk image was reported as suspect. Harmless while validation
was only a report; a wall the moment it became the gate in front of every write
(ART-042), which would have refused writes to every image that is not a DD ADF.
→ The block count comes from the image's own length and the root block from
that count (`VolumeGeometry::root_block_for`). Pinned by
`an_hd_floppy_image_is_healthy_at_its_own_geometry`,
`a_hard_disk_sized_image_is_healthy_at_its_own_geometry` and, end to end,
`an_hd_floppy_is_written_through_the_gate` /
`a_hard_disk_image_is_written_through_the_gate`.

**ART-036** 🟡 **Names with accented characters were refused as too long**
`core/volume/write/dir.rs` · `check_name` compared `str::len()` against the
30-character AmigaDOS limit. That counts **UTF-8 bytes**, not characters, so
every Latin-1 character above `~` counted double: `Grüße vom Süden` is 15
characters and 18 bytes, and a name of thirty accented characters was rejected
as sixty. AmigaDOS stores one byte per Latin-1 character, so all thirty fit.
The more accents a name had, the more wrong the answer — which is why it went
unnoticed in ASCII-only testing.
→ The check counts characters, and `plan.rs` shortens by characters too:
byte-slicing a Latin-1 stem would cut a character in half and panic. Pinned by
`the_length_limit_counts_characters_not_utf_8_bytes` and
`shortening_a_latin_1_name_does_not_split_a_character`.

### Amiga format compatibility (Stage R oracle)

All four were found the same way: by pointing **amitools** — an independent
implementation — at images ART had written. Every one of them was invisible to
ART's own test suite, because ART's reader and writer agreed with each other.
That is the project's oldest failure pattern, and this is the first time an
outside implementation has been asked.

**ART-033** 🔴 **Every ADF ART wrote had invalid block checksums**
`core/adf/checksum.rs` · AmigaDOS uses **two** checksum algorithms: the boot
block sums with end-around carry and stores the bitwise NOT, while root, header,
OFS data and bitmap blocks use a plain wrapping sum stored as its two's
complement. ART applied the boot block algorithm to everything. `core/rdb.rs`
had the correct one all along, which is why RDB blocks always validated
elsewhere and ADFs never did.
Every disk ART created or modified was rejected by AmigaOS, WinUAE and every
other Amiga tool — while ART reported it healthy, because its verifier used the
same wrong algorithm. Confirmed: `xdftool` refused an ART-made blank ADF with
"Invalid Root Block(2)".
→ `block_checksum` is now the plain sum; the boot block keeps its own
implementation in `bootblock.rs`. Verified: `xdftool` now reads an ART-made ADF
and lists its volume.
Tests: `a_checksummed_block_sums_to_zero`,
`the_algorithm_matches_the_reference_implementation`,
`checksum_all_zero_block` (which had been pinning the bug).

**ART-034** 🔴 **Files ART added to a disk were zero bytes on a real Amiga**
`core/adf/blocks.rs`, `core/adf/mutate.rs` · The file header's `byte_size` was
read and written at longword 79, and the comment at longword 80. The real
layout is `byte_size` at longword 81 (offset 324) and the comment at longword 82
(offset 328) — 80 is the protection bits. Both halves of ART used the same wrong
offsets, so a file round-tripped through ART perfectly and appeared as an empty
file to AmigaOS. The root block's bitmap-extension pointer was one longword
early too (412 instead of 416), which would matter on any volume large enough to
need extension blocks — that is, exactly the HDF partitions Stage R adds.
→ Offsets corrected against `amitools` (`EntryBlock._read_nac_modts`,
`FileHeaderBlock._read`). Verified: `xdftool` lists a file ART wrote with the
right size and `xdftool type` returns its contents.

**ART-035** 🔴 **The free-space bitmap was laid out the wrong way round**
`core/adf/blocks.rs`, `core/adf/create.rs`, `core/adf/mutate.rs` · ART recorded
a block's bit at longword `block / 32`, counting from the **most** significant
bit. AmigaDOS records it at `(block - reserved) / 32`, counting from the
**least** significant bit — the bitmap does not describe the boot blocks at all.
ART was wrong on both axes, so every block it marked used landed on some other
block's bit. AmigaOS would have seen ART's occupied blocks as free and handed
them out to the next file written, destroying data on a real machine.
Invisible from inside ART: the allocator, the free-space count and the parser
all shared the same arithmetic, and a permutation of bit positions leaves the
*count* of free blocks unchanged, so even the totals looked right.
→ One `bit_position()` helper now owns the arithmetic and all four call sites
go through it. Confirmed against ADFlib (`adf_bitm.c`: `bitMask[i] == 1 << i`,
`sectOfMap = nSect - 2`) and amitools (`ADFSBitmap.get_bit`). Verified by
decoding an ART-written disk with the reference mapping: the blocks marked used
are exactly the root, the bitmap and the file that was added.
Tests: `bit_positions_match_the_reference_implementations`,
`bitmap_marks_used_and_free` (which had been pinning the bug),
`a_large_volume_spills_into_more_bitmap_blocks`.

**ART-032** 🟠 **RDB partitions described the wrong DosType and boot priority**
`core/rdb.rs` · `BootPri` was read and written at longword 46 and `DosType` at
47. They belong at 47 and 48; longword 46 is `Mask`. An ART-created hard disk
image therefore told AmigaOS its partitions had DosType 0 — no filesystem —
while ART read its own images back correctly. Found by `rdbtool`, which showed
`pri=1146049281` (the ASCII for `DOS`) sitting in the priority field.
→ Offsets corrected and pinned.
Tests: `dosenv_offsets_match_the_amiga_layout`,
`a_partition_reads_back_the_filesystem_it_was_written_with`.

### Software Sources (Aminet, §41.5)

Both found the same way: by running the finished engine against real Aminet
mirrors instead of only against its own fixtures. Neither was reachable from
the test suite as written, which is the lesson worth keeping.

**ART-030** 🔴 **Mirror failover concatenated a dead mirror's bytes onto the next one's**
`core/sources/mirror.rs` · `fetch_with_failover` passed the same `Write` to
every attempt. A mirror that failed *after* sending part of the body left those
bytes in the destination, and the next mirror's response was appended to them.
The result was one file assembled from two mirrors — which parses, hashes and
caches perfectly happily while being wrong.
Observed live: `aminet.net` dropped the connection 519 416 bytes into `INDEX`,
and the catalog came back with 91 413 entries instead of the 85 435 the file
actually holds. No error, no skipped line, no warning.
→ The destination is now a `FetchTarget`, wound back before every attempt.
`HttpMirrorClient` also refuses a body shorter than an announced
`Content-Length`, so a silently truncated response cannot pass as complete.
Tests: `a_mirror_that_dies_mid_body_does_not_pollute_the_next_one`,
`a_file_destination_is_wound_back_too`,
`a_dirty_destination_is_wound_back_before_the_first_attempt`,
`a_body_shorter_than_the_announced_length_is_never_a_success`.

**ART-031** 🟠 **LHA archives with level 2 or 3 headers could not be opened at all**
`core/lha/mod.rs`, `core/lha/safe_extract.rs` · both read `header.filename`
directly. That field is only populated for level 0 and 1 headers; levels 2 and
3 — what modern tools write, and what Aminet hosts — leave it empty and carry
the name in an extended header. Every such archive failed with
`ART-FORMAT-MALFORMED: empty entry name`, so LHA Studio could not list or
extract a typical Aminet download.
→ `entry_path()` falls back to `parse_pathname_to_str()` when the raw field is
empty. Levels 0 and 1 deliberately keep using the raw field: `delharc`
percent-encodes non-ASCII bytes, and switching those levels over would rename
the Latin-1 Amiga filenames that extract correctly today. `safe_join` remains
the choke point for both.
Tests: `a_level_two_header_is_read_from_its_extended_header`,
`a_level_zero_name_still_comes_from_the_raw_field`.

All fixed 2026-08-09 unless noted. Test names are in `src-tauri/`.

### Hard disk images (Stage 3 audit)

**ART-021** 🔴 **Creating or opening an HDF allocated the whole image in memory**
`core/hdf.rs`, `core/rdb.rs` · `create_rdb_image` built the entire image as a
`Vec<u8>` before writing it, and `open_hdf` read the whole file back to report
its geometry. A 4 GB hard disk image therefore needed 4 GB of RAM — on a machine
that could not spare it, ART died. Violates §56 (never allocate from an
unchecked length).
→ `create_rdb_layout` returns only the structured leading blocks; the file is
created sparsely with `set_len`. `open_hdf` reads a 1 MB header window and takes
the size from the file's metadata. Tests: `large_images_are_created_sparsely`,
`create_and_parse_rdb_image_with_pfs3_and_ffs`.

**ART-022** 🔴 **Creating an image silently destroyed an existing one**
`core/hdf.rs`, `core/adf/create.rs` · Both creation paths called
`std::fs::write`, which replaces whatever is already there. Typing the name of
an existing `Workbench.hdf` destroyed it — gigabytes, with no backup and no
prompt. Creation is a `SAFE_CREATE` operation and may only ever write something
new (§57).
→ Both refuse when the target exists; the HDF path uses `create_new` and removes
a partially written file if anything fails. Tests:
`create_refuses_to_replace_an_existing_image`,
`save_refuses_to_replace_an_existing_file`.

**ART-023** 🟠 **A tiny requested size aborted the application**
`core/hdf.rs` · A plain image smaller than four bytes was indexed at `[0..4]` to
write the bootblock signature, panicking — and with `panic = "abort"` that ends
the process.
→ A minimum size is enforced and reported as an error.
Test: `absurdly_small_sizes_error_rather_than_panic`.

**ART-024** 🔴 **The RDSK block described a zero-capacity disk to AmigaOS**
`core/rdb.rs` · The logical-drive fields were written at the wrong longwords.
`HiCylinder` (LW 35) and `CylBlocks` (LW 36) were never written at all, so they
stayed zero; `RDBBlocksHi` got the value meant for another field, and
`cylinders - 1` landed in a reserved longword. ART read its own disks back
correctly because the parser only reads geometry from LW 16–18 — the same shape
of defect as ART-009: self-consistent, and wrong for every other Amiga tool.
Verified against `amitools`' `RDBlock.py` field indices.
→ Fields written at their documented offsets.
Test: `rdsk_logical_drive_fields_are_written`.

**ART-025** 🟠 **Empty RDB block lists pointed at block 0**
`core/rdb.rs` · `BadBlockList`, `FileSysHeaderList` and `DriveInit` were left at
zero. Zero is a valid block number, so AmigaOS would follow them into block 0 —
the RDSK block itself. `BlockBytes` was also zero rather than 512.
→ Absent lists are written as `-1` (`NO_BLOCK`) and `BlockBytes` is set.
Test: `rdsk_absent_lists_use_the_no_block_sentinel`.

**ART-026** 🟠 **Oversized partitions were silently truncated**
`core/rdb.rs` · Each partition's end was clamped with `.min(cylinders - 1)`. Ask
for two 500 MB partitions on a 100 MB disk and the first was quietly shrunk to
whatever fit, the rest were skipped mid-loop — leaving the previous partition's
`next` pointer aimed at a block that was never written. The user was told
nothing. Violates §89.
→ The total is checked up front and an impossible layout is refused with the
numbers involved. Tests: `oversized_partitions_are_refused_not_truncated`,
`partitions_do_not_overlap_and_stay_inside_the_disk`,
`too_many_partitions_are_refused`.

**ART-027** 🟡 **RDB checksums ignored the block's own `SummedLongs`**
`core/rdb.rs` · The checksum summed a fixed 128 longwords instead of the count
the block declares (LW 1, conventionally 64). ART's own images happened to pass
because their later longwords are zero, but a real Amiga disk carrying vendor
strings there would be reported as corrupt.
→ Both compute and verify honour `SummedLongs`, with a guard against a
nonsensical declared value. Tests: `checksum_honours_summed_longs`,
`checksum_survives_a_nonsense_summed_longs_field`.

### Collection & ROM (Stage 3 audit)

**ART-028** 🟠 **A folder scan could be sent into unbounded recursion**
`core/collection.rs` · `collect_files_recursive` followed every subdirectory
with no depth limit and no symlink check. A Windows junction pointing back up
its own tree recursed until the stack overflowed, which aborts the process.
Dropping such a folder onto ART closed the application.
→ Depth is capped and symlinks are not followed.
Tests: `scanning_stops_at_the_depth_limit`, `scanning_finds_images_within_the_limit`.

**ART-029** 🟡 **Any file could be read whole as a "ROM"**
`core/rom.rs` · `identify_rom` read the entire file before checking anything.
Kickstarts are at most 1 MB, but the user picks the file — a mistaken pick of a
large image pulled all of it into memory.
→ Size is checked against a 4 MB ceiling before reading.

### Data safety

**ART-001** 🔴 **ADF edits overwrote the original with no backup and no atomicity**
`core/adf/mod.rs` · `mutate_disk_file` called `std::fs::write` straight onto the
user's file. An interrupted write left a truncated — that is, destroyed — disk
image, and there was no previous version to fall back on. Violates §57, §93.
→ Pipeline is now `read → mutate → validate → backup → atomic commit` via
`core/safety`. Tests: `mutation_backs_up_the_previous_version`,
`failed_mutation_leaves_the_original_untouched`,
`mutation_that_corrupts_the_image_is_not_committed`.

**ART-002** 🔴 **LHA extraction silently overwrote existing files**
`core/lha/safe_extract.rs` · `File::create` replaced whatever was already at the
destination. Extracting an archive into a working folder destroyed the user's
files without a word. Violates §89.
→ `OverwritePolicy::{Skip,Overwrite,Rename}`, defaulting to `Skip`.
Test: `existing_files_are_skipped_by_default`.

**ART-003** 🔴 **FlashFloppy `FF.CFG` was regenerated from scratch**
`core/gotek.rs` · Saving rewrote the whole file from ART's own fields, deleting
every setting it does not model — `pin02`, `head-settle-ms`, `display-order` and
dozens more that Gotek owners tune by hand. Directly violates §39
("Unknown settings must be preserved").
→ The file is now edited in place; unmanaged keys and comments pass through
verbatim. Test: `round_trip_preserves_unknown_settings`.

**ART-004** 🔴 **PiStorm `cmdline.txt` was regenerated, leaving the SD card unbootable**
`core/pistorm.rs` · Same defect, worse consequence: regenerating the Raspberry Pi
kernel command line dropped `root=` and `console=`, so the Pi would not boot at
all after ART "saved" the configuration. Violates §40.
→ `config.txt` and `cmdline.txt` are merged, not rewritten.
Test: `cmdline_round_trip_preserves_boot_parameters`.

**ART-005** 🟠 **Config files were replaced without a backup**
`core/gotek.rs`, `core/pistorm.rs` · No previous version was kept.
→ Both go through `guarded_write` with `BackupPolicy::CONFIG` (5 generations).
Tests: `saving_backs_up_the_previous_config`,
`saving_backs_up_the_previous_sd_configuration`.

**ART-006** 🟡 **Aborted extractions left truncated files behind**
`core/lha/safe_extract.rs` · A file that failed to decompress, or was cut short
by the bomb guard, stayed on disk looking like a successful extraction.
→ Partial output is removed and the entry is reported as skipped with a reason.

### Crashes

**ART-007** 🔴 **An invalid block number from the UI killed the whole application**
`core/adf/blocks.rs`, `core/volume/write/mod.rs` · Block numbers arrive from
the frontend (`dirBlock`, `headerBlock`) and were used to index the image
directly. Out of range meant a panic, and the release profile sets
`panic = "abort"` — so the entire app died.
→ All access goes through `blocks::block_slice`/`block_slice_mut`/`read_u32_at`/
`write_u32_at`, which return a `CoreError`. *(Location updated: the fix and its
tests originally lived in `core/adf/mutate.rs`, deleted when task 10 retired it
in favour of the single volume writer — fixed, not reopened; see ART-047 for
the primitives' now-orphaned callers.)* Tests:
`block_access_rejects_out_of_range_numbers` (`core/adf/blocks.rs`),
`out_of_range_directory_block_is_an_error` (`core/volume/write/mod.rs`).

**ART-008** 🟠 **Malformed images could hang the UI forever**
`core/volume/write/dir.rs`, `core/volume/write/file.rs` · Hash-bucket and
file-extension chain walks had no step limit; a chain pointing back at itself
looped indefinitely.
→ Both walks are bounded and report a malformed image rather than looping.
*(Location updated: originally `core/adf/mutate.rs`, deleted when task 10
retired it — fixed, not reopened.)* Tests:
`a_hash_chain_that_loops_is_an_error_not_a_hang` (`dir.rs`),
`an_extension_chain_that_loops_is_an_error_not_a_hang` (`file.rs`).

### Filesystem correctness

**ART-009** 🔴 **The ADF hash function was not AmigaDOS-compatible**
`core/adf/hash.rs` · `name_hash` omitted the `& 0x7ff` mask that the reference
algorithm applies after *every character*. Entries went into the wrong hash
buckets. ART could read its own images back — it hashed consistently — but
AmigaDOS, WinUAE and every other Amiga tool could not find the files. Any image
ART wrote before this fix is suspect.
→ Matches `adfGetHashValue`, with reference values pinned.
Tests: `matches_the_amigados_reference_values`,
`the_mask_is_applied_every_iteration`.

**ART-010** 🟠 **International volumes hashed names with the wrong case folding**
`core/adf/hash.rs` · `name_hash` took no `international` flag, so INTL volumes
(which fold the Latin-1 224–254 range) used plain ASCII rules.
→ The flag now comes from the volume's own bootblock.
Test: `international_folding_differs_for_accented_names`.

**ART-011** 🔴 **`rename_entry` corrupted directories when the chain was inconsistent**
`core/volume/write/dir.rs` · If the entry was not found in its parent's hash
chain the function carried on silently and linked it into the new bucket
anyway, leaving it referenced from two places. `delete_entry` had the check;
rename did not.
→ Missing entries now error out via `predecessor_of`, which ends its walk with
`Malformed { "block N is not in the bucket its name hashes to" }` rather than
returning "no predecessor". *(Location updated: originally `core/adf/mutate.rs`,
deleted when task 10 retired it — fixed, not reopened; the moved test also now
asserts the image is byte-for-byte unchanged after the refusal, which the
in-memory original could not check.)*
Test: `rename_of_an_unlinked_entry_reports_an_error` (`core/volume/write/mod.rs`).

**ART-012** 🟠 **Duplicate names could be created in one directory**
`core/volume/write/dir.rs` · AmigaDOS compares names case-insensitively and
cannot hold two entries with the same name; nothing checked.
→ Name collisions are rejected, case-insensitively, including a directory
shadowing an existing file's name. *(Location updated: originally
`core/adf/mutate.rs`, deleted when task 10 retired it — fixed, not reopened.)*
Tests: `a_name_that_differs_only_in_case_is_already_taken` (`dir.rs`),
`a_name_already_taken_is_refused_and_nothing_is_written`,
`a_directory_cannot_shadow_an_existing_file_name`,
`rename_onto_an_existing_name_is_rejected` (`core/volume/write/mod.rs`).

**ART-013** 🟠 **A file header could be used as a directory**
`core/volume/write/mod.rs` · Passing a file's header block as the insert
target wrote hash-table entries into it, corrupting the file.
→ `VolumeWriter::resolve_directory` verifies the block is `ST_ROOT` or
`ST_USERDIR` via `dir::is_directory`. *(Location updated: originally
`core/adf/mutate.rs`, deleted when task 10 retired it — fixed, not reopened.)*
Test: `writing_into_a_file_header_instead_of_a_directory_is_refused`.

### Security

**ART-014** 🟠 **The zip-bomb guard could be bypassed by integer overflow**
`core/lha/safe_extract.rs` · `total_written + header.original_size` was unchecked,
so an archive declaring a huge size could wrap the total past the limit.
→ `checked_add`; a size that cannot be added aborts the extraction.

**ART-015** 🟡 **Media paths could inject arbitrary WinUAE directives**
`core/winuae.rs` · `.uae` files are line-oriented and paths were written into
them unescaped, so a newline in a path became further configuration.
→ `checked_config_value` rejects line breaks.
Test: `line_breaks_in_paths_are_rejected`.

### Emulator integration

**ART-016** 🟠 **Only the first of several HDFs was reachable**
`core/winuae.rs` · Multiple hard drives produced repeated bare `hardfile=` lines
with no device names, and an invalid `hardfile_type{i}` key.
→ One `hardfile2=rw,DH<n>:…` line per image, per WinUAE `cfgfile.cpp`.
Test: `each_hardfile_gets_its_own_device`.

**ART-017** 🟡 **WinUAE detection used hard-coded drive letters**
`core/winuae.rs` · The candidate list included `D:\WinUAE\…`, left over from a
developer's machine, and assumed Windows lives on `C:`.
→ Paths come from the `ProgramFiles*` environment variables.

**ART-018** 🔵 **Concurrent launches clobbered each other's configuration**
`core/winuae.rs` · Every launch wrote the same `art_launch.uae` temp file.
→ The filename now carries a nanosecond stamp.

### Analysis

**ART-019** 🟠 **HD floppy geometry was never reported**
`core/analysis.rs` · The check read `size == 901_120 || size == 880 * 1024` —
the same number twice — so the HD branch was unreachable. Clippy's `eq_op` lint
caught this, but CI ran clippy with `continue-on-error`, so nobody saw it.
→ Explicit `DD_IMAGE_SIZE` / `HD_IMAGE_SIZE` constants, 11 vs 22 sectors.

### Build hygiene

**ART-020** 🟠 **CI hid real correctness errors**
`.github/workflows/ci.yml` · Clippy ran with `continue-on-error: true`, and
`lib.rs` carried a blanket `allow(dead_code, unused_imports, unused_variables,
unused_assignments)`. Between them, ART-019 stayed invisible for months.
→ Clippy is blocking; the blanket allow is narrowed to `dead_code`.

---

## Adding an entry

1. Take the next free `ART-NNN`.
2. State the defect as a claim, not a symptom — what is wrong, not what looked odd.
3. Name the file, and say **how it fails for a user**. If you cannot describe a
   way it hurts someone, it is a preference, not a defect.
4. Cite the spec section when a rule is broken.
5. When fixing it, add a regression test and name it here. A fix without a test
   is not fixed — it is untested.
