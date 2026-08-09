//! The undo journal: how ART changes a two-gigabyte image without a
//! two-gigabyte backup (brief §2).
//!
//! > *A 2 GB image is not a big floppy. It needs a journal, not a bigger buffer.*
//!
//! Classic write-ahead undo logging, scoped to **one** operation:
//!
//! ```text
//! begin(blocks)  save the current contents of every block the op will touch,
//!                fsync the journal                       ← nothing written yet
//! write_block()  in-place writes into the image           ← recoverable
//! commit()       validated → delete the journal
//! roll_back()    failed → restore the saved blocks, then delete the journal
//! ```
//!
//! ## Why writing is gated on having journaled first
//!
//! [`Journalled::write_block`] refuses any block that was not named in
//! [`Journalled::begin`]. That is the whole safety property in one check: a
//! block ART cannot undo is a block ART will not touch. It turns "remember to
//! journal first" from a rule someone has to follow into one the code enforces.
//!
//! ## Crash recovery
//!
//! `panic = "abort"` means an out-of-range index kills the process outright,
//! possibly between two block writes. So the journal outlives the process: it
//! sits next to the image, and [`find_journal`] is called when a volume is
//! mounted. A journal that is there at mount time means an operation died
//! part-way, and rolling it back restores the exact pre-operation image.
//!
//! Crashing *during* `begin` is safe for the same reason it is uninteresting:
//! no image write has happened yet, so replaying the complete entries writes
//! back bytes that are already there.
//!
//! ## Identifying the image
//!
//! The header records the image path, its size and its modification time. Only
//! **path and size** are checked before a rollback. The mtime is recorded for
//! the report and deliberately *not* an equality gate: it is the mtime from
//! before the operation, and a crash mid-write is precisely the case where the
//! file has been modified since. Gating on it would reject every journal worth
//! replaying. Size is the strong invariant instead — these writes are in-place
//! and never resize the file, so a size that no longer matches means a
//! different file, and ART touches nothing (§89).

use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use super::BlockDeviceMut;
use crate::core::error::{CoreError, CoreResult};

/// The extension ART's journals use, appended to the image's own file name.
pub const JOURNAL_EXTENSION: &str = "artjournal";

const MAGIC: &[u8; 8] = b"ARTJRNL\x00";
const FORMAT_VERSION: u32 = 1;
const ENTRY_MAGIC: u32 = 0x454e_5452; // "ENTR"

/// The most blocks one operation may journal.
///
/// A single file operation touches its header, its data and extension blocks,
/// the bitmap blocks covering them and its parent directory. Even a large file
/// stays far below this. The ceiling exists so a corrupt caller cannot ask for
/// an allocation the machine cannot hold — the same rule as everywhere else in
/// ART: never size a buffer from an unchecked number.
pub const MAX_JOURNALLED_BLOCKS: usize = 262_144;

/// The most bytes ART will read back from a journal file.
const MAX_JOURNAL_BYTES: u64 = 512 * 1024 * 1024;

/// Where to put the journal for `image`.
///
/// Next to the image, on the same filesystem: same failure domain, and it
/// travels with the file if the user copies both (brief §2).
pub fn journal_path_for(image: &Path) -> PathBuf {
    let mut name = image.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(JOURNAL_EXTENSION);
    image.with_file_name(name)
}

/// What a journal says about the operation it is protecting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalHeader {
    /// The image this journal belongs to, as ART saw it.
    pub image_path: String,
    /// Size of the image before the operation. In-place writes never change
    /// it, so this is the identity check.
    pub image_bytes: u64,
    /// Modification time before the operation, seconds since the Unix epoch.
    /// Recorded for the report; see the module docs for why it is not a gate.
    pub image_mtime_secs: u64,
    /// Where the volume starts inside the image, so block numbers resolve.
    pub volume_offset: u64,
    pub block_size: u32,
    /// What the operation was, in the user's words.
    pub description: String,
}

/// An operation in progress: the journal, plus the device it guards.
///
/// Obtained from [`Journalled::begin`] and ended by exactly one of
/// [`commit`](Journalled::commit) or [`roll_back`](Journalled::roll_back).
/// Dropping it without either leaves the journal on disk, which is the correct
/// outcome — that is what a crash looks like, and the next mount will recover.
pub struct Journalled<'a> {
    device: &'a mut dyn BlockDeviceMut,
    path: PathBuf,
    header: JournalHeader,
    /// Blocks whose previous contents are safely on disk.
    saved: HashSet<u32>,
    /// Saved contents, kept in memory too so a rollback in the same process
    /// needs no re-read. The disk copy is what survives a crash.
    previous: Vec<(u32, Vec<u8>)>,
}

impl<'a> Journalled<'a> {
    /// Save the current contents of `blocks`, fsync, and return a guard that
    /// can write exactly those blocks.
    ///
    /// The journal is complete and durable before this returns. Nothing has
    /// been written to the image.
    pub fn begin(
        device: &'a mut dyn BlockDeviceMut,
        image: &Path,
        volume_offset: u64,
        description: &str,
        blocks: &[u32],
    ) -> CoreResult<Self> {
        if blocks.is_empty() {
            return Err(CoreError::InvalidInput(
                "an operation that touches no blocks has nothing to journal".into(),
            ));
        }
        if blocks.len() > MAX_JOURNALLED_BLOCKS {
            return Err(CoreError::InvalidInput(format!(
                "this operation would touch {} blocks, more than ART journals in one go",
                blocks.len()
            )));
        }

        let metadata = std::fs::metadata(image)?;
        let header = JournalHeader {
            image_path: image.display().to_string(),
            image_bytes: metadata.len(),
            image_mtime_secs: mtime_secs(&metadata),
            volume_offset,
            block_size: device.block_size() as u32,
            description: description.to_string(),
        };

        // De-duplicate: the same bitmap block often covers several of the
        // blocks an operation allocates, and saving it twice would make the
        // rollback order matter.
        let mut wanted: Vec<u32> = blocks.to_vec();
        wanted.sort_unstable();
        wanted.dedup();

        let mut previous = Vec::with_capacity(wanted.len());
        for &block in &wanted {
            previous.push((block, super::read_block_vec(device, block)?));
        }

        let path = journal_path_for(image);
        write_journal(&path, &header, &previous)?;

        Ok(Self {
            device,
            path,
            header,
            saved: wanted.into_iter().collect(),
            previous,
        })
    }

    pub fn header(&self) -> &JournalHeader {
        &self.header
    }

    /// Read a block through the guard, so callers do not need a second borrow.
    pub fn read_block(&self, n: u32) -> CoreResult<Vec<u8>> {
        super::read_block_vec(self.device, n)
    }

    pub fn block_size(&self) -> usize {
        self.device.block_size()
    }

    pub fn total_blocks(&self) -> u32 {
        self.device.total_blocks()
    }

    /// Write a block whose previous contents this journal holds.
    ///
    /// A block that was not declared to [`begin`](Journalled::begin) is
    /// refused. This is the safety property, not a convenience check: writing
    /// a block ART cannot undo would leave a rollback that silently restores
    /// only part of the operation, which is worse than not rolling back at all.
    pub fn write_block(&mut self, n: u32, buf: &[u8]) -> CoreResult<()> {
        if !self.saved.contains(&n) {
            return Err(CoreError::InvalidInput(format!(
                "block {n} was not journalled, so ART will not write to it"
            )));
        }
        self.device.write_block(n, buf)
    }

    /// The operation succeeded and has been validated: drop the journal.
    ///
    /// The image is synced first. Deleting the journal while the writes it
    /// protects are still in a cache would leave a window with neither the new
    /// data nor a way back.
    pub fn commit(self) -> CoreResult<()> {
        self.device.sync()?;
        remove_journal(&self.path)
    }

    /// The operation failed: put every saved block back, then drop the journal.
    ///
    /// Restores from the in-memory copy, which is byte-identical to the one on
    /// disk. Rollback failing part-way leaves the journal in place on purpose —
    /// the next mount tries again rather than the image being left half-undone
    /// with nothing to say so.
    pub fn roll_back(self) -> CoreResult<()> {
        for (block, bytes) in &self.previous {
            self.device.write_block(*block, bytes)?;
        }
        self.device.sync()?;
        remove_journal(&self.path)
    }
}

/// A journal found next to an image, from an operation that did not finish.
#[derive(Debug)]
pub struct PendingJournal {
    path: PathBuf,
    image: PathBuf,
    header: JournalHeader,
    entries: Vec<(u32, Vec<u8>)>,
    /// True when the file ended part-way through an entry.
    truncated: bool,
}

/// What a recovery did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryReport {
    pub description: String,
    pub blocks_restored: usize,
    /// The journal ended mid-entry, so the crash happened while it was still
    /// being written — which means no image write had started.
    pub was_truncated: bool,
}

/// Look for an unfinished operation next to `image`.
///
/// Called when a volume is mounted. `Ok(None)` is the normal answer.
///
/// A journal that cannot be parsed at all is an error rather than a silent
/// removal: ART does not delete a file it does not understand from next to a
/// user's disk image (§89).
pub fn find_journal(image: &Path) -> CoreResult<Option<PendingJournal>> {
    let path = journal_path_for(image);
    if !path.exists() {
        return Ok(None);
    }

    let parsed = read_journal(&path)?;
    Ok(Some(PendingJournal {
        path,
        image: image.to_path_buf(),
        header: parsed.header,
        entries: parsed.entries,
        truncated: parsed.truncated,
    }))
}

impl PendingJournal {
    pub fn header(&self) -> &JournalHeader {
        &self.header
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn blocks(&self) -> usize {
        self.entries.len()
    }

    /// Whether this journal describes the image it was found next to.
    ///
    /// Path and size only — see the module docs for why the mtime is recorded
    /// but not compared.
    pub fn matches_image(&self) -> CoreResult<bool> {
        let metadata = match std::fs::metadata(&self.image) {
            Ok(metadata) => metadata,
            Err(_) => return Ok(false),
        };
        Ok(metadata.len() == self.header.image_bytes)
    }

    /// Why this journal cannot be applied, or `None` when it can.
    pub fn refusal(&self) -> CoreResult<Option<String>> {
        if self.matches_image()? {
            return Ok(None);
        }
        let actual = std::fs::metadata(&self.image).map(|m| m.len()).unwrap_or(0);
        Ok(Some(format!(
            "This journal was written for a {}-byte image and '{}' is {} bytes. \
             ART has not touched anything — check whether the file was replaced, \
             then delete the journal by hand if it is stale.",
            self.header.image_bytes,
            self.image.display(),
            actual
        )))
    }

    /// Undo the unfinished operation, then delete the journal.
    ///
    /// Refuses outright when the header does not describe this image.
    pub fn roll_back(self) -> CoreResult<RecoveryReport> {
        if let Some(reason) = self.refusal()? {
            return Err(CoreError::InvalidInput(reason));
        }

        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.image)?;

        for (block, bytes) in &self.entries {
            let position =
                self.header
                    .volume_offset
                    .checked_add((*block as u64).checked_mul(bytes.len() as u64).ok_or_else(
                        || CoreError::Malformed {
                            format: "journal".into(),
                            detail: format!("block {block} is at an impossible offset"),
                        },
                    )?)
                    .ok_or_else(|| CoreError::Malformed {
                        format: "journal".into(),
                        detail: format!("block {block} is at an impossible offset"),
                    })?;

            // The size check above proved the file is the right length, but the
            // journal's own numbers are still untrusted input.
            if position.saturating_add(bytes.len() as u64) > self.header.image_bytes {
                return Err(CoreError::Malformed {
                    format: "journal".into(),
                    detail: format!(
                        "this journal wants to restore block {block} past the end of the image"
                    ),
                });
            }

            use std::io::{Seek, SeekFrom};
            file.seek(SeekFrom::Start(position))?;
            file.write_all(bytes)?;
        }
        file.sync_all()?;
        drop(file);

        let report = RecoveryReport {
            description: self.header.description.clone(),
            blocks_restored: self.entries.len(),
            was_truncated: self.truncated,
        };
        remove_journal(&self.path)?;
        Ok(report)
    }

    /// Leave the image alone and delete the journal.
    ///
    /// For a stale journal the user has decided about. Separate from
    /// [`roll_back`](PendingJournal::roll_back) so discarding is always a
    /// deliberate act.
    pub fn discard(self) -> CoreResult<()> {
        remove_journal(&self.path)
    }
}

// ---------------------------------------------------------------------------
// The file format
// ---------------------------------------------------------------------------
//
// header:  magic[8] version:u32 block_size:u32 image_bytes:u64 mtime:u64
//          volume_offset:u64 path_len:u16 path desc_len:u16 desc
// entries: ENTRY_MAGIC:u32 block_no:u32 payload[block_size] checksum:u32
//
// Each entry carries its own checksum so a replay can tell a complete entry
// from one interrupted mid-write. There is no entry count in the header: it
// would have to be written last, and a header that has to be revisited after
// the payload is a header that can disagree with it.

/// A journal read back from disk.
struct ParsedJournal {
    header: JournalHeader,
    entries: Vec<(u32, Vec<u8>)>,
    /// The file ended part-way through an entry, so the crash happened while
    /// the journal itself was still being written.
    truncated: bool,
}

fn write_journal(
    path: &Path,
    header: &JournalHeader,
    entries: &[(u32, Vec<u8>)],
) -> CoreResult<()> {
    let mut out = Vec::with_capacity(entries.len() * (header.block_size as usize + 12) + 128);

    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_be_bytes());
    out.extend_from_slice(&header.block_size.to_be_bytes());
    out.extend_from_slice(&header.image_bytes.to_be_bytes());
    out.extend_from_slice(&header.image_mtime_secs.to_be_bytes());
    out.extend_from_slice(&header.volume_offset.to_be_bytes());
    put_string(&mut out, &header.image_path)?;
    put_string(&mut out, &header.description)?;

    for (block, bytes) in entries {
        out.extend_from_slice(&ENTRY_MAGIC.to_be_bytes());
        out.extend_from_slice(&block.to_be_bytes());
        out.extend_from_slice(bytes);
        out.extend_from_slice(&entry_checksum(*block, bytes).to_be_bytes());
    }

    // Written directly rather than through `atomic_write`: a rename would put
    // the journal in place only after the temp file was durable, which is the
    // same guarantee, but `sync_all` on the journal itself is what the protocol
    // actually requires and it is clearer to do exactly that.
    let mut file = File::create(path)?;
    file.write_all(&out)?;
    file.sync_all()?;
    Ok(())
}

fn read_journal(path: &Path) -> CoreResult<ParsedJournal> {
    let size = std::fs::metadata(path)?.len();
    if size > MAX_JOURNAL_BYTES {
        return Err(CoreError::Malformed {
            format: "journal".into(),
            detail: format!("the journal is {size} bytes, larger than ART will read"),
        });
    }

    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_JOURNAL_BYTES)
        .read_to_end(&mut bytes)?;

    let mut cursor = 0usize;
    if bytes.len() < MAGIC.len() || &bytes[..MAGIC.len()] != MAGIC {
        return Err(CoreError::Malformed {
            format: "journal".into(),
            detail: "this is not an ART journal".into(),
        });
    }
    cursor += MAGIC.len();

    let version = take_u32(&bytes, &mut cursor)?;
    if version != FORMAT_VERSION {
        return Err(CoreError::UnsupportedFormat(format!(
            "this journal is version {version}; this ART writes version {FORMAT_VERSION}"
        )));
    }

    let block_size = take_u32(&bytes, &mut cursor)?;
    if block_size == 0 || block_size as usize % super::SECTOR_BYTES != 0 {
        return Err(CoreError::Malformed {
            format: "journal".into(),
            detail: format!("the journal claims a {block_size}-byte block"),
        });
    }

    let image_bytes = take_u64(&bytes, &mut cursor)?;
    let image_mtime_secs = take_u64(&bytes, &mut cursor)?;
    let volume_offset = take_u64(&bytes, &mut cursor)?;
    let image_path = take_string(&bytes, &mut cursor)?;
    let description = take_string(&bytes, &mut cursor)?;

    let header = JournalHeader {
        image_path,
        image_bytes,
        image_mtime_secs,
        volume_offset,
        block_size,
        description,
    };

    // Entries are read until one does not complete. A short trailing entry is
    // a crash during `begin`, which is safe: no image write had started.
    let mut entries = Vec::new();
    let mut truncated = false;
    let payload = block_size as usize;
    let entry_len = 4 + 4 + payload + 4;

    while cursor < bytes.len() {
        if bytes.len() - cursor < entry_len {
            truncated = true;
            break;
        }
        let magic = take_u32(&bytes, &mut cursor)?;
        if magic != ENTRY_MAGIC {
            truncated = true;
            break;
        }
        let block = take_u32(&bytes, &mut cursor)?;
        let data = bytes[cursor..cursor + payload].to_vec();
        cursor += payload;
        let checksum = take_u32(&bytes, &mut cursor)?;

        if checksum != entry_checksum(block, &data) {
            truncated = true;
            break;
        }
        entries.push((block, data));
    }

    Ok(ParsedJournal {
        header,
        entries,
        truncated,
    })
}

/// A plain additive checksum over the block number and its payload.
///
/// Enough to tell a complete entry from one cut off mid-write, which is all
/// this has to do — the journal is ART's own file in ART's own directory, not
/// untrusted input from a network.
fn entry_checksum(block: u32, bytes: &[u8]) -> u32 {
    let mut sum = block;
    for chunk in bytes.chunks(4) {
        let mut word = [0u8; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum = sum.wrapping_add(u32::from_be_bytes(word));
    }
    sum
}

fn put_string(out: &mut Vec<u8>, value: &str) -> CoreResult<()> {
    let bytes = value.as_bytes();
    let len = u16::try_from(bytes.len())
        .map_err(|_| CoreError::InvalidInput("that description is too long to journal".into()))?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

fn take_u32(bytes: &[u8], cursor: &mut usize) -> CoreResult<u32> {
    let end = cursor.checked_add(4).ok_or_else(short)?;
    if end > bytes.len() {
        return Err(short());
    }
    let value = u32::from_be_bytes(bytes[*cursor..end].try_into().expect("checked"));
    *cursor = end;
    Ok(value)
}

fn take_u64(bytes: &[u8], cursor: &mut usize) -> CoreResult<u64> {
    let end = cursor.checked_add(8).ok_or_else(short)?;
    if end > bytes.len() {
        return Err(short());
    }
    let value = u64::from_be_bytes(bytes[*cursor..end].try_into().expect("checked"));
    *cursor = end;
    Ok(value)
}

fn take_string(bytes: &[u8], cursor: &mut usize) -> CoreResult<String> {
    let end = cursor.checked_add(2).ok_or_else(short)?;
    if end > bytes.len() {
        return Err(short());
    }
    let len = u16::from_be_bytes(bytes[*cursor..end].try_into().expect("checked")) as usize;
    *cursor = end;

    let text_end = cursor.checked_add(len).ok_or_else(short)?;
    if text_end > bytes.len() {
        return Err(short());
    }
    let text = String::from_utf8_lossy(&bytes[*cursor..text_end]).into_owned();
    *cursor = text_end;
    Ok(text)
}

fn short() -> CoreError {
    CoreError::Malformed {
        format: "journal".into(),
        detail: "the journal header is incomplete".into(),
    }
}

fn remove_journal(path: &Path) -> CoreResult<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        // Already gone is the outcome that was wanted.
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(CoreError::from(err)),
    }
}

fn mtime_secs(metadata: &std::fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::volume::device::FileRegionMut;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("art-journal-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// An image whose every block is filled with its own number, so a block
    /// that was not restored is obvious rather than merely different.
    fn image_at(path: &Path, blocks: usize) -> Vec<u8> {
        let mut bytes = vec![0u8; blocks * 512];
        for block in 0..blocks {
            bytes[block * 512..block * 512 + 512].fill(block as u8);
        }
        std::fs::write(path, &bytes).unwrap();
        bytes
    }

    fn open(path: &Path, blocks: usize) -> FileRegionMut {
        FileRegionMut::open(path, 0, (blocks * 512) as u64, 512).unwrap()
    }

    fn filled(value: u8) -> Vec<u8> {
        vec![value; 512]
    }

    #[test]
    fn a_committed_operation_leaves_the_new_data_and_no_journal() {
        let dir = scratch("commit");
        let image = dir.join("disk.hdf");
        image_at(&image, 8);

        let mut device = open(&image, 8);
        let mut op = Journalled::begin(&mut device, &image, 0, "Copy in", &[3, 5]).unwrap();
        op.write_block(3, &filled(0xAA)).unwrap();
        op.write_block(5, &filled(0xBB)).unwrap();
        op.commit().unwrap();

        let after = std::fs::read(&image).unwrap();
        assert_eq!(after[3 * 512], 0xAA);
        assert_eq!(after[5 * 512], 0xBB);
        assert!(
            !journal_path_for(&image).exists(),
            "a committed op keeps no journal"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_rolled_back_operation_restores_every_byte() {
        let dir = scratch("rollback");
        let image = dir.join("disk.hdf");
        let before = image_at(&image, 8);

        let mut device = open(&image, 8);
        let mut op = Journalled::begin(&mut device, &image, 0, "Copy in", &[1, 2, 6]).unwrap();
        op.write_block(1, &filled(0xAA)).unwrap();
        op.write_block(2, &filled(0xBB)).unwrap();
        op.write_block(6, &filled(0xCC)).unwrap();
        op.roll_back().unwrap();
        drop(device);

        assert_eq!(
            std::fs::read(&image).unwrap(),
            before,
            "rollback must be exact"
        );
        assert!(!journal_path_for(&image).exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The property the whole design rests on: a block ART cannot undo is a
    /// block ART will not touch.
    #[test]
    fn writing_a_block_that_was_not_journalled_is_refused() {
        let dir = scratch("ungated");
        let image = dir.join("disk.hdf");
        let before = image_at(&image, 8);

        let mut device = open(&image, 8);
        let mut op = Journalled::begin(&mut device, &image, 0, "Copy in", &[3]).unwrap();
        let err = op.write_block(4, &filled(0xFF)).unwrap_err();
        assert!(
            err.to_string().contains("not journalled"),
            "unexpected message: {err}"
        );
        op.roll_back().unwrap();
        drop(device);

        assert_eq!(std::fs::read(&image).unwrap(), before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The crash case. The guard is dropped without commit or rollback —
    /// exactly what `panic = "abort"` leaves behind — and the next mount finds
    /// the journal and undoes the half-finished write.
    #[test]
    fn a_crash_between_writes_is_undone_at_the_next_mount() {
        let dir = scratch("crash");
        let image = dir.join("disk.hdf");
        let before = image_at(&image, 8);

        {
            let mut device = open(&image, 8);
            let mut op = Journalled::begin(&mut device, &image, 0, "Copy in", &[2, 4, 7]).unwrap();
            op.write_block(2, &filled(0xAA)).unwrap();
            // …and the process dies here, with block 4 and 7 never written.
            std::mem::forget(op);
        }

        let pending = find_journal(&image)
            .unwrap()
            .expect("the journal must survive");
        assert_eq!(pending.header().description, "Copy in");
        assert_eq!(pending.blocks(), 3);
        assert!(pending.matches_image().unwrap());

        let report = pending.roll_back().unwrap();
        assert_eq!(report.blocks_restored, 3);
        assert!(!report.was_truncated);

        assert_eq!(std::fs::read(&image).unwrap(), before);
        assert!(!journal_path_for(&image).exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// §89: a journal that does not describe this image is surfaced, never
    /// applied. Restoring blocks from someone else's operation would be ART
    /// corrupting a file entirely on its own initiative.
    #[test]
    fn a_journal_for_a_different_image_is_refused_not_applied() {
        let dir = scratch("mismatch");
        let image = dir.join("disk.hdf");
        image_at(&image, 8);

        {
            let mut device = open(&image, 8);
            let op = Journalled::begin(&mut device, &image, 0, "Copy in", &[2]).unwrap();
            std::mem::forget(op);
        }

        // The image is replaced by a different, larger one.
        let replacement = image_at(&image, 12);

        let pending = find_journal(&image).unwrap().unwrap();
        assert!(!pending.matches_image().unwrap());
        assert!(pending.refusal().unwrap().is_some());

        let err = pending.roll_back().unwrap_err();
        assert!(
            err.to_string().contains("has not touched anything"),
            "{err}"
        );
        assert_eq!(
            std::fs::read(&image).unwrap(),
            replacement,
            "a refused journal must leave the image exactly as it was"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A crash while the journal itself was still being written. No image
    /// write had started, so the complete entries are replayed and the partial
    /// one is ignored.
    #[test]
    fn a_journal_cut_off_mid_entry_replays_only_its_whole_entries() {
        let dir = scratch("truncated");
        let image = dir.join("disk.hdf");
        let before = image_at(&image, 8);

        {
            let mut device = open(&image, 8);
            let op = Journalled::begin(&mut device, &image, 0, "Copy in", &[1, 2, 3]).unwrap();
            std::mem::forget(op);
        }

        // Chop the last entry in half, the way a lost write would.
        let journal = journal_path_for(&image);
        let bytes = std::fs::read(&journal).unwrap();
        std::fs::write(&journal, &bytes[..bytes.len() - 300]).unwrap();

        let pending = find_journal(&image).unwrap().unwrap();
        assert_eq!(pending.blocks(), 2, "only the complete entries are usable");

        let report = pending.roll_back().unwrap();
        assert!(report.was_truncated);
        assert_eq!(std::fs::read(&image).unwrap(), before);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupted_entry_stops_the_replay_there() {
        let dir = scratch("corrupt-entry");
        let image = dir.join("disk.hdf");
        image_at(&image, 8);

        {
            let mut device = open(&image, 8);
            let op = Journalled::begin(&mut device, &image, 0, "Copy in", &[1, 2, 3]).unwrap();
            std::mem::forget(op);
        }

        // Flip a byte inside the second entry's payload.
        let journal = journal_path_for(&image);
        let mut bytes = std::fs::read(&journal).unwrap();
        let header_len = bytes.len() - 3 * (4 + 4 + 512 + 4);
        let second_payload = header_len + (4 + 4 + 512 + 4) + 8;
        bytes[second_payload] ^= 0xFF;
        std::fs::write(&journal, &bytes).unwrap();

        let pending = find_journal(&image).unwrap().unwrap();
        assert_eq!(pending.blocks(), 1, "the replay stops at the damaged entry");
        assert!(pending.header().description == "Copy in");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_is_not_a_journal_is_an_error_not_a_deletion() {
        let dir = scratch("not-a-journal");
        let image = dir.join("disk.hdf");
        image_at(&image, 4);

        let journal = journal_path_for(&image);
        std::fs::write(&journal, b"this is somebody else's file").unwrap();

        let err = find_journal(&image).unwrap_err();
        assert!(err.to_string().contains("not an ART journal"), "{err}");
        assert!(
            journal.exists(),
            "ART must not delete a file it cannot read"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn discarding_leaves_the_image_alone() {
        let dir = scratch("discard");
        let image = dir.join("disk.hdf");

        image_at(&image, 8);
        {
            let mut device = open(&image, 8);
            let mut op = Journalled::begin(&mut device, &image, 0, "Copy in", &[2]).unwrap();
            op.write_block(2, &filled(0xAA)).unwrap();
            std::mem::forget(op);
        }
        let mid_write = std::fs::read(&image).unwrap();

        find_journal(&image).unwrap().unwrap().discard().unwrap();

        assert_eq!(
            std::fs::read(&image).unwrap(),
            mid_write,
            "discard is not a rollback"
        );
        assert!(!journal_path_for(&image).exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_journal_is_the_normal_answer() {
        let dir = scratch("none");
        let image = dir.join("disk.hdf");
        image_at(&image, 4);
        assert!(find_journal(&image).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_journal_sits_next_to_its_image() {
        assert_eq!(
            journal_path_for(Path::new("D:/amiga/Work.hdf")),
            PathBuf::from("D:/amiga/Work.hdf.artjournal")
        );
    }

    #[test]
    fn an_operation_that_touches_nothing_is_refused() {
        let dir = scratch("empty");
        let image = dir.join("disk.hdf");
        image_at(&image, 4);
        let mut device = open(&image, 4);

        // `Journalled` holds a `&mut` device, so the Ok side is not `Debug`
        // and `unwrap_err` cannot be used here.
        match Journalled::begin(&mut device, &image, 0, "Nothing", &[]) {
            Ok(_) => panic!("an operation touching no blocks must be refused"),
            Err(err) => assert!(err.to_string().contains("nothing to journal"), "{err}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The same bitmap block covers many allocated blocks, so duplicates are
    /// the normal case, not a caller error.
    #[test]
    fn a_block_named_twice_is_journalled_once() {
        let dir = scratch("dedup");
        let image = dir.join("disk.hdf");
        let before = image_at(&image, 8);

        let mut device = open(&image, 8);
        let mut op = Journalled::begin(&mut device, &image, 0, "Copy in", &[3, 3, 5, 3]).unwrap();
        assert_eq!(op.header().block_size, 512);
        op.write_block(3, &filled(0xAA)).unwrap();
        op.roll_back().unwrap();
        drop(device);

        assert_eq!(std::fs::read(&image).unwrap(), before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A volume inside a partition: block 0 of the volume is not byte 0 of the
    /// file, and recovery has to put the bytes back where they came from.
    #[test]
    fn recovery_respects_where_the_volume_starts_in_the_file() {
        let dir = scratch("offset");
        let image = dir.join("disk.hdf");
        let before = image_at(&image, 16);
        let offset = 4 * 512u64;

        {
            let mut device = FileRegionMut::open(&image, offset, 8 * 512, 512).unwrap();
            let mut op = Journalled::begin(&mut device, &image, offset, "Copy in", &[1]).unwrap();
            // Volume block 1 is file block 5.
            op.write_block(1, &filled(0xAA)).unwrap();
            std::mem::forget(op);
        }
        assert_eq!(std::fs::read(&image).unwrap()[5 * 512], 0xAA);

        find_journal(&image).unwrap().unwrap().roll_back().unwrap();
        assert_eq!(std::fs::read(&image).unwrap(), before);

        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ---------------------------------------------------------------------------
// Crash safety, with a real process kill (brief §9.5)
// ---------------------------------------------------------------------------
//
// `panic = "abort"` means an out-of-range index kills the process outright,
// possibly between two block writes. Every other test in this file simulates
// that with `std::mem::forget`, which proves the *journal* is right but not
// that a genuinely half-written file on disk can be recovered — a forgotten
// guard still had its writes flushed by a cooperating runtime.
//
// So this one really does it: the test re-runs its own binary as a child, the
// child journals, writes some of its blocks and calls `abort()`, and the
// parent then checks that mounting the image finds the journal and that
// rolling it back restores the file byte for byte.

#[cfg(test)]
mod crash_safety {
    use std::path::{Path, PathBuf};

    use crate::core::volume::device::FileRegionMut;
    use crate::core::volume::journal::{find_journal, journal_path_for, Journalled};

    /// The child test's full path. `--exact` matches the whole name, and a
    /// filter that matches nothing makes the harness exit 0 — which would turn
    /// this test into one that silently proves nothing.
    const CHILD_TEST: &str = "core::volume::journal::crash_safety::crash_child_when_asked";

    /// Which point the child dies at. Each one is a different half-written
    /// state, and all three have to recover to the same bytes.
    const INJECTION_POINTS: [&str; 3] = ["after-journal", "after-first-write", "mid-write"];

    /// The child half. Does nothing unless the parent asked for it.
    ///
    /// A `#[test]` rather than a binary so it reaches the internal API, and so
    /// the parent can invoke it by name through the same test harness.
    #[test]
    fn crash_child_when_asked() {
        let Ok(image) = std::env::var("ART_CRASH_IMAGE") else {
            return;
        };
        let point = std::env::var("ART_CRASH_POINT").unwrap_or_default();
        let image = PathBuf::from(image);

        let mut device = FileRegionMut::open(&image, 0, 8 * 512, 512).unwrap();
        let mut journal =
            Journalled::begin(&mut device, &image, 0, "Copy in Payload.bin", &[3, 4, 5]).unwrap();

        // The journal is on disk and fsynced. Dying here must be recoverable
        // even though no image write has happened — the rollback writes back
        // bytes that are already there, which is harmless.
        if point == "after-journal" {
            std::process::abort();
        }

        journal.write_block(3, &[0xAA; 512]).unwrap();
        if point == "after-first-write" {
            std::process::abort();
        }

        journal.write_block(4, &[0xBB; 512]).unwrap();
        // Block 5 is journalled and never written: the classic half-done op.
        if point == "mid-write" {
            std::process::abort();
        }

        journal.commit().unwrap();
    }

    /// An image whose every block is filled with its own number, so a block
    /// that was not restored is obvious rather than merely different.
    fn image_at(path: &Path) -> Vec<u8> {
        let mut bytes = vec![0u8; 8 * 512];
        for block in 0..8 {
            bytes[block * 512..block * 512 + 512].fill(block as u8);
        }
        std::fs::write(path, &bytes).unwrap();
        bytes
    }

    #[test]
    fn a_killed_process_leaves_an_image_that_recovers_byte_for_byte() {
        // The child is this same test binary, run with `--exact` so only the
        // child test executes.
        let Ok(exe) = std::env::current_exe() else {
            return;
        };

        for point in INJECTION_POINTS {
            let dir =
                std::env::temp_dir().join(format!("art-crash-{point}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let image = dir.join("disk.hdf");
            let before = image_at(&image);

            let status = std::process::Command::new(&exe)
                .args(["--exact", "--nocapture", CHILD_TEST])
                .env("ART_CRASH_IMAGE", &image)
                .env("ART_CRASH_POINT", point)
                // The child aborts on purpose; its output is noise.
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .expect("the child test binary should start");

            assert!(
                !status.success(),
                "the child was supposed to die at {point}, and exited cleanly"
            );

            // The journal outlived the process. That is the whole point.
            let pending = find_journal(&image)
                .unwrap()
                .unwrap_or_else(|| panic!("no journal survived the crash at {point}"));
            assert_eq!(pending.header().description, "Copy in Payload.bin");
            assert!(
                pending.matches_image().unwrap(),
                "the journal should describe the image it is next to ({point})"
            );

            let report = pending.roll_back().unwrap();
            assert_eq!(report.blocks_restored, 3, "at {point}");
            assert_eq!(
                std::fs::read(&image).unwrap(),
                before,
                "recovery from a real crash at {point} must be byte-for-byte"
            );
            assert!(!journal_path_for(&image).exists());

            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// The other half of §9.5: a journal that does not describe the image it
    /// is next to must never be applied. Restoring blocks from someone else's
    /// operation would be ART corrupting a file on its own initiative.
    #[test]
    fn a_stale_journal_is_not_applied_after_a_crash() {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };

        let dir = std::env::temp_dir().join(format!("art-crash-stale-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let image = dir.join("disk.hdf");
        image_at(&image);

        let status = std::process::Command::new(&exe)
            .args(["--exact", "--nocapture", CHILD_TEST])
            .env("ART_CRASH_IMAGE", &image)
            .env("ART_CRASH_POINT", "mid-write")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("the child test binary should start");
        assert!(!status.success());

        // The image is replaced by a different, larger one — the user restored
        // a backup, or copied a different disk over it.
        let mut replacement = vec![0u8; 12 * 512];
        for block in 0..12 {
            replacement[block * 512..block * 512 + 512].fill(0x77);
        }
        std::fs::write(&image, &replacement).unwrap();

        let pending = find_journal(&image).unwrap().unwrap();
        assert!(!pending.matches_image().unwrap());

        let err = pending.roll_back().unwrap_err();
        assert!(
            err.to_string().contains("has not touched anything"),
            "{err}"
        );
        assert_eq!(
            std::fs::read(&image).unwrap(),
            replacement,
            "a refused journal must leave the image exactly as it was"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
