//! Writing into an AmigaDOS volume, at any geometry (brief §3).
//!
//! `core/adf/mutate.rs` writes into a floppy. It hardcodes block 881 as *the*
//! bitmap block, 880 as the root and 1760 as the size, which is correct for
//! every ADF and wrong for every hard disk. This module is the same operations
//! with the geometry as a parameter, working through [`BlockDeviceMut`] so the
//! bytes may be a buffer in memory or a partition two gigabytes into a file.
//!
//! ## Order of work, and why
//!
//! Every mutation here follows the same sequence:
//!
//! ```text
//! 1. load the allocator          — the whole free-space map, one read per page
//! 2. plan                        — decide every block the operation will touch
//! 3. journal those blocks        — old contents saved and fsynced (§2)
//! 4. write                       — through the journal, which refuses the rest
//! 5. validate                    — re-read and check what was just written
//! 6. commit, or roll back        — never leave a half-written volume
//! ```
//!
//! Step 2 finishing before step 3 begins is the part that matters: the journal
//! has to know the *complete* block set up front, and an allocator that hands
//! out blocks lazily mid-write cannot provide it.
//!
//! ## What ART will not write to
//!
//! | DosType | Write |
//! |---|---|
//! | `DOS\0`, `DOS\1` — OFS/FFS | yes |
//! | `DOS\2`, `DOS\3` — INTL | yes; hashing follows the volume's own flag |
//! | `DOS\4`, `DOS\5` — dircache | **no**. Reading stays on. |
//! | `DOS\6`, `DOS\7` — long filenames | **no**, and reading is off too |
//! | PFS3, SFS | **no**, not mounted at all |
//!
//! Dircache is refused because a directory has *two* representations on those
//! volumes and updating only one leaves listings that are wrong on a real
//! AmigaOS while looking right in ART — a corruption the user would not see
//! until they booted the disk (§3.4, §89).

pub mod bitmap;
pub mod copy;
pub mod dir;
pub mod file;
pub mod layout;
pub mod plan;
pub mod uaem;

use std::path::{Path, PathBuf};

use crate::core::adf::bcpl::AmigaDate;
use crate::core::error::{CoreError, CoreResult};
use crate::core::volume::journal::Journalled;
use crate::core::volume::{BlockDevice, BlockDeviceMut, VolumeGeometry};

use bitmap::Allocator;
use layout::{get_u32, BlockSet, CHECKSUM_OFFSET};

/// Why ART will not write to a volume, or `None` when it will.
///
/// Read support is a separate question and stays exactly as it was: a volume
/// listed here can still be browsed and copied *from*.
pub fn write_refusal(geometry: &VolumeGeometry) -> Option<String> {
    let dos = geometry.dos_type;

    if !dos.is_dos() {
        return Some(format!(
            "{} — ART cannot write to this filesystem",
            dos.label()
        ));
    }
    if dos.is_long_filename() {
        return Some("long-filename filesystem — ART cannot read or write these yet".to_string());
    }
    if dos.has_dircache() {
        return Some(
            "dircache volume — read-only in ART for now. A directory is stored twice on \
             these volumes, and writing only one copy leaves listings that look right in \
             ART and wrong on a real Amiga."
                .to_string(),
        );
    }
    if geometry.block_size != crate::core::volume::SECTOR_BYTES {
        return Some(format!(
            "{}-byte blocks — ART only writes 512-byte blocks so far",
            geometry.block_size
        ));
    }
    None
}

/// What a mutation did, for the operation log and the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteOutcome {
    /// The header block of whatever was created, when something was.
    pub block: Option<u32>,
    /// Every block the operation wrote.
    pub blocks_touched: usize,
    /// Blocks still free afterwards.
    pub free_blocks: usize,
    /// True when the operation was verified by reading it back.
    pub verified: bool,
}

/// What the attributes dialog shows (§7.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryAttributes {
    pub name: String,
    /// `HSPARWED` exactly as stored, inversion included. Use
    /// [`uaem::format_bits`](uaem::format_bits) to render it.
    pub protection: u32,
    pub comment: String,
    pub date: AmigaDate,
    pub is_dir: bool,
}

/// Metadata a copied file carries over (§4.1, §7.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FileMeta {
    /// `HSPARWED`. `None` uses the AmigaDOS default of `----RWED`.
    pub protection: Option<u32>,
    /// `None` stamps now.
    pub date: Option<AmigaDate>,
}

/// A volume open for writing.
///
/// Holds the device and the geometry; each operation loads its own allocator,
/// because a writer kept across operations would hold a free-space map that
/// another process could have invalidated.
pub struct VolumeWriter<'a> {
    device: &'a mut dyn BlockDeviceMut,
    geometry: VolumeGeometry,
    image: PathBuf,
    volume_offset: u64,
}

impl<'a> VolumeWriter<'a> {
    /// Open a volume for writing, refusing the filesystems ART cannot write.
    ///
    /// `image` is the file the volume lives in and `volume_offset` where it
    /// starts inside it — the journal needs both to put blocks back where they
    /// came from after a crash.
    pub fn open(
        device: &'a mut dyn BlockDeviceMut,
        geometry: VolumeGeometry,
        image: &Path,
        volume_offset: u64,
    ) -> CoreResult<Self> {
        if let Some(reason) = write_refusal(&geometry) {
            return Err(CoreError::UnsupportedFormat(reason));
        }
        if geometry.total_blocks > device.total_blocks() {
            return Err(CoreError::Malformed {
                format: "volume".into(),
                detail: format!(
                    "this volume claims {} blocks but only {} are there",
                    geometry.total_blocks,
                    device.total_blocks()
                ),
            });
        }

        Ok(Self {
            device,
            geometry,
            image: image.to_path_buf(),
            volume_offset,
        })
    }

    pub fn geometry(&self) -> &VolumeGeometry {
        &self.geometry
    }

    /// Every block of the volume, concatenated into one buffer — the shape
    /// `core::adf::AdfImage` expects.
    ///
    /// Reads through the device this session already has open, block by
    /// block, rather than a second independent file open. A caller that just
    /// committed a write and wants a fresh in-memory snapshot must build it
    /// from *this* session, not by reopening the file: the moment a handle to
    /// a just-modified file is released, another process (an antivirus
    /// scan-on-close, a search indexer) can briefly lock it, and a second
    /// open racing that lock would report a durable, already-successful write
    /// as a failure. Reading through the still-open handle cannot hit that
    /// race.
    ///
    /// Only sensible for a volume small enough to hold entirely in memory —
    /// an ADF, not a multi-gigabyte hard disk partition.
    pub fn all_bytes(&self) -> CoreResult<Vec<u8>> {
        let block_size = self.geometry.block_size;
        let mut out = Vec::with_capacity(self.geometry.total_blocks as usize * block_size);
        let mut buf = vec![0u8; block_size];
        for block in 0..self.geometry.total_blocks {
            self.device.read_block(block, &mut buf)?;
            out.extend_from_slice(&buf);
        }
        Ok(out)
    }

    /// Blocks the bitmap reports as free.
    pub fn free_blocks(&self) -> CoreResult<usize> {
        Ok(Allocator::load(self.device, &self.geometry)?.free_count())
    }

    /// Free space in bytes, for the pane footer (§8).
    pub fn free_bytes(&self) -> CoreResult<u64> {
        Ok(self.free_blocks()? as u64 * self.geometry.block_size as u64)
    }

    /// What is in a directory. `0` means the root.
    ///
    /// Reads the volume as it stands, with no pending edits — a writer between
    /// operations has none.
    pub fn list(&self, dir_block: u32) -> CoreResult<Vec<dir::DirEntry>> {
        let dir_block = self.resolve_directory(dir_block)?;
        let set = BlockSet::new(self.geometry.block_size);
        dir::entries_in(self.device, &set, &self.geometry, dir_block)
    }

    /// Find an entry by name, ignoring case as AmigaDOS does.
    pub fn find(&self, dir_block: u32, name: &str) -> CoreResult<Option<dir::DirEntry>> {
        let dir_block = self.resolve_directory(dir_block)?;
        let set = BlockSet::new(self.geometry.block_size);
        dir::find_entry(self.device, &set, &self.geometry, dir_block, name)
    }

    /// Read a file back out of the volume.
    pub fn read_file(&self, header_block: u32) -> CoreResult<Vec<u8>> {
        let set = BlockSet::new(self.geometry.block_size);
        file::read_file(self.device, &set, &self.geometry, header_block)
    }

    /// The name stored in an entry's header block.
    pub fn entry_name(&self, block: u32) -> CoreResult<String> {
        if block >= self.geometry.total_blocks {
            return Err(CoreError::Malformed {
                format: "volume".into(),
                detail: format!("block {block} is outside this volume"),
            });
        }
        let bytes = crate::core::volume::read_block_vec(self.device, block)?;
        Ok(dir::name_of(&bytes))
    }

    /// Move an entry from one directory to another **within the same volume**.
    ///
    /// One journalled operation, so the entry is never listed in neither
    /// directory — the state a crash between an unlink and a link would leave,
    /// and the one that loses a file while its blocks stay allocated.
    ///
    /// Moving between two volumes is a copy, a verification and then a delete,
    /// done by the caller so the delete can be refused when the copy did not
    /// verify (§4.3).
    pub fn relink(
        &mut self,
        from_dir: u32,
        entry_block: u32,
        to_dir: u32,
        new_name: &str,
    ) -> CoreResult<WriteOutcome> {
        let checked = dir::check_name(new_name)?;
        let from_dir = self.resolve_directory(from_dir)?;
        let to_dir = self.resolve_directory(to_dir)?;

        if from_dir == to_dir {
            return self.rename(from_dir, entry_block, &checked);
        }
        if entry_block == to_dir {
            return Err(CoreError::InvalidInput(
                "a folder cannot be moved inside itself".into(),
            ));
        }

        let mut set = BlockSet::new(self.geometry.block_size);
        let old_name = dir::name_of(&set.view(self.device, entry_block)?);
        dir::ensure_available(self.device, &set, &self.geometry, to_dir, &checked)?;

        dir::unlink_from(
            self.device,
            &mut set,
            &self.geometry,
            from_dir,
            entry_block,
            &old_name,
        )?;
        dir::set_name(self.device, &mut set, entry_block, &checked)?;

        // The parent pointer has to follow the entry, or a walk upward from it
        // arrives at a directory that no longer lists it.
        {
            let header = set.edit(self.device, entry_block)?;
            layout::set_u32(header, layout::PARENT_OFFSET, to_dir)?;
        }
        set.checksum(entry_block, CHECKSUM_OFFSET)?;

        dir::link_into(
            self.device,
            &mut set,
            &self.geometry,
            to_dir,
            entry_block,
            &checked,
        )?;

        let wanted = checked.clone();
        self.commit(&format!("Move {old_name}"), set, move |device, set, geo| {
            // In the new directory…
            match dir::find_entry(device, set, geo, to_dir, &wanted)? {
                Some(entry) if entry.block == entry_block => {}
                _ => {
                    return Err(CoreError::Malformed {
                        format: "volume".into(),
                        detail: format!("'{wanted}' is not in the folder it was moved to"),
                    })
                }
            }
            // …and out of the old one.
            if dir::entries_in(device, set, geo, from_dir)?
                .iter()
                .any(|entry| entry.block == entry_block)
            {
                return Err(CoreError::Malformed {
                    format: "volume".into(),
                    detail: format!("'{wanted}' is still listed in the folder it left"),
                });
            }
            Ok(())
        })
        .map(|mut outcome| {
            outcome.block = Some(entry_block);
            outcome
        })
    }

    /// Write a file into `parent`, which is a directory header block.
    ///
    /// Refuses before touching anything when the name is taken, the name is
    /// one AmigaDOS cannot store, or the file will not fit — with the real
    /// numbers in the message (§3.3).
    pub fn add_file(
        &mut self,
        parent: u32,
        name: &str,
        data: &[u8],
        meta: FileMeta,
    ) -> CoreResult<WriteOutcome> {
        let checked = dir::check_name(name)?;
        let parent = self.resolve_directory(parent)?;

        let mut set = BlockSet::new(self.geometry.block_size);
        dir::ensure_available(self.device, &set, &self.geometry, parent, &checked)?;

        let mut allocator = Allocator::load(self.device, &self.geometry)?;
        let budget = file::budget_for(data.len() as u64, &self.geometry)?;

        if allocator.free_count() < budget.total() {
            return Err(CoreError::InvalidInput(format!(
                "'{checked}' needs {} blocks and only {} are free",
                budget.total(),
                allocator.free_count()
            )));
        }
        let allocated = allocator.allocate(budget.total())?;

        let written = file::write_file_blocks(
            &mut set,
            &self.geometry,
            &allocated,
            parent,
            &checked,
            data,
            meta.protection.unwrap_or_else(file::default_protection),
            meta.date,
        )?;
        dir::link_into(
            self.device,
            &mut set,
            &self.geometry,
            parent,
            written.header_block,
            &checked,
        )?;

        self.stamp_bitmap(&mut set, &allocator, &allocated)?;

        let header = written.header_block;
        self.commit(
            &format!("Copy in {checked}"),
            set,
            move |device, set, geo| {
                // Content verification (§5): the bytes have to come back out of
                // the volume, not out of the buffer they were written from.
                let back = file::read_file(device, set, geo, header)?;
                if back != data {
                    return Err(CoreError::Malformed {
                        format: "volume".into(),
                        detail: "the file read back from the volume is not what was written".into(),
                    });
                }
                Ok(())
            },
        )
        .map(|mut outcome| {
            outcome.block = Some(header);
            outcome
        })
    }

    /// Create a directory inside `parent`.
    pub fn make_dir(&mut self, parent: u32, name: &str) -> CoreResult<WriteOutcome> {
        let checked = dir::check_name(name)?;
        let parent = self.resolve_directory(parent)?;

        let mut set = BlockSet::new(self.geometry.block_size);
        dir::ensure_available(self.device, &set, &self.geometry, parent, &checked)?;

        let mut allocator = Allocator::load(self.device, &self.geometry)?;
        let allocated = allocator.allocate(1)?;
        let block = allocated[0];

        dir::write_dir_header(&mut set, block, parent, &checked)?;
        dir::link_into(
            self.device,
            &mut set,
            &self.geometry,
            parent,
            block,
            &checked,
        )?;
        self.stamp_bitmap(&mut set, &allocator, &allocated)?;

        let wanted = checked.clone();
        self.commit(
            &format!("New folder {checked}"),
            set,
            move |device, set, geo| match dir::find_entry(device, set, geo, parent, &wanted)? {
                Some(entry) if entry.block == block && entry.is_dir => Ok(()),
                _ => Err(CoreError::Malformed {
                    format: "volume".into(),
                    detail: format!("'{wanted}' is not in the directory it was just added to"),
                }),
            },
        )
        .map(|mut outcome| {
            outcome.block = Some(block);
            outcome
        })
    }

    /// Delete an entry from `parent`.
    ///
    /// A directory has to be empty. Deleting one recursively is the caller's
    /// job, one journalled operation per entry, so a cancelled batch leaves a
    /// consistent volume rather than a directory tree half gone (§2).
    pub fn delete(&mut self, parent: u32, entry_block: u32) -> CoreResult<WriteOutcome> {
        let parent = self.resolve_directory(parent)?;
        let set = BlockSet::new(self.geometry.block_size);

        let entry = set.view(self.device, entry_block)?;
        let name = dir::name_of(&entry);
        let is_dir = dir::is_directory(&entry)?;

        if is_dir && !dir::entries_in(self.device, &set, &self.geometry, entry_block)?.is_empty() {
            return Err(CoreError::InvalidInput(format!(
                "'{name}' still has things in it. Empty it first."
            )));
        }

        // Which blocks come back: the entry's own, plus everything a file
        // holds. Worked out before the journal opens.
        let freed = if is_dir {
            vec![entry_block]
        } else {
            file::blocks_of(self.device, &set, &self.geometry, entry_block)?.all_blocks()
        };

        let mut set = set;
        dir::unlink_from(
            self.device,
            &mut set,
            &self.geometry,
            parent,
            entry_block,
            &name,
        )?;

        let mut allocator = Allocator::load(self.device, &self.geometry)?;
        allocator.free_blocks(&freed);
        self.stamp_bitmap(&mut set, &allocator, &freed)?;

        let gone = name.clone();
        self.commit(
            &format!("Delete {name}"),
            set,
            move |device, set, geo| match dir::find_entry(device, set, geo, parent, &gone)? {
                None => Ok(()),
                Some(_) => Err(CoreError::Malformed {
                    format: "volume".into(),
                    detail: format!("'{gone}' is still listed after being deleted"),
                }),
            },
        )
    }

    /// Rename an entry inside `parent`.
    ///
    /// Unlink, rename, relink — never rename in place. A new name usually
    /// hashes to a different bucket, and an entry left in the old one is
    /// findable by a full listing and invisible to a lookup, which is how a
    /// file comes to "disappear" while still taking up space.
    pub fn rename(
        &mut self,
        parent: u32,
        entry_block: u32,
        new_name: &str,
    ) -> CoreResult<WriteOutcome> {
        let checked = dir::check_name(new_name)?;
        let parent = self.resolve_directory(parent)?;

        let mut set = BlockSet::new(self.geometry.block_size);
        let old_name = dir::name_of(&set.view(self.device, entry_block)?);

        if old_name.to_lowercase() != checked.to_lowercase() {
            dir::ensure_available(self.device, &set, &self.geometry, parent, &checked)?;
        }

        dir::unlink_from(
            self.device,
            &mut set,
            &self.geometry,
            parent,
            entry_block,
            &old_name,
        )?;
        dir::set_name(self.device, &mut set, entry_block, &checked)?;
        dir::link_into(
            self.device,
            &mut set,
            &self.geometry,
            parent,
            entry_block,
            &checked,
        )?;

        let wanted = checked.clone();
        self.commit(
            &format!("Rename {old_name} to {checked}"),
            set,
            move |device, set, geo| match dir::find_entry(device, set, geo, parent, &wanted)? {
                Some(entry) if entry.block == entry_block => Ok(()),
                _ => Err(CoreError::Malformed {
                    format: "volume".into(),
                    detail: format!("'{wanted}' cannot be found after the rename"),
                }),
            },
        )
        .map(|mut outcome| {
            outcome.block = Some(entry_block);
            outcome
        })
    }

    /// Change an entry's protection bits, comment and date (§7.2).
    ///
    /// One journalled operation touching one block. The bits are functional,
    /// not decorative: WHDLoad packs rely on `S` and `P`, and a Slave with the
    /// wrong bits is a game that does not start.
    ///
    /// A field left `None` keeps what is there — an attributes dialog that
    /// only changed the comment must not silently restamp the date.
    pub fn set_attributes(
        &mut self,
        entry_block: u32,
        protection: Option<u32>,
        comment: Option<&str>,
        date: Option<AmigaDate>,
    ) -> CoreResult<WriteOutcome> {
        if entry_block >= self.geometry.total_blocks {
            return Err(CoreError::Malformed {
                format: "volume".into(),
                detail: format!("block {entry_block} is outside this volume"),
            });
        }

        // AmigaDOS comments are Latin-1 in an 80-byte BSTR, so 79 characters.
        // Refused rather than truncated, for the same reason names are.
        if let Some(text) = comment {
            let length = text.chars().count();
            if length > layout::COMMENT_FIELD_LEN - 1 {
                return Err(CoreError::InvalidInput(format!(
                    "that comment is {length} characters; AmigaDOS stores {}",
                    layout::COMMENT_FIELD_LEN - 1
                )));
            }
            if let Some(bad) = text.chars().find(|c| (*c as u32) > 0xFF) {
                return Err(CoreError::InvalidInput(format!(
                    "'{bad}' is outside the Latin-1 character set AmigaDOS stores"
                )));
            }
        }

        let mut set = BlockSet::new(self.geometry.block_size);
        let name = dir::name_of(&set.view(self.device, entry_block)?);

        {
            let header = set.edit(self.device, entry_block)?;
            if let Some(bits) = protection {
                layout::set_u32(header, layout::PROTECT_OFFSET, bits)?;
            }
            if let Some(text) = comment {
                let bytes: Vec<u8> = text.chars().map(|c| c as u8).collect();
                let start = layout::COMMENT_OFFSET;
                if start + 1 + bytes.len() > header.len() {
                    return Err(CoreError::InvalidInput("that comment does not fit".into()));
                }
                header[start..start + layout::COMMENT_FIELD_LEN].fill(0);
                header[start] = bytes.len() as u8;
                header[start + 1..start + 1 + bytes.len()].copy_from_slice(&bytes);
            }
            if let Some(stamp) = date {
                layout::set_date(header, stamp)?;
            }
        }
        set.checksum(entry_block, CHECKSUM_OFFSET)?;

        let expected = protection;
        self.commit(
            &format!("Change attributes of {name}"),
            set,
            move |device, _, _| {
                // The bits are the point of the operation, so they are what
                // gets read back rather than merely re-checksummed.
                let Some(wanted) = expected else {
                    return Ok(());
                };
                let header = crate::core::volume::read_block_vec(device, entry_block)?;
                if get_u32(&header, layout::PROTECT_OFFSET)? != wanted {
                    return Err(CoreError::Malformed {
                        format: "volume".into(),
                        detail: "the protection bits did not read back as written".into(),
                    });
                }
                Ok(())
            },
        )
        .map(|mut outcome| {
            outcome.block = Some(entry_block);
            outcome
        })
    }

    /// An entry's protection bits, comment and date, for the dialog.
    pub fn attributes(&self, entry_block: u32) -> CoreResult<EntryAttributes> {
        if entry_block >= self.geometry.total_blocks {
            return Err(CoreError::Malformed {
                format: "volume".into(),
                detail: format!("block {entry_block} is outside this volume"),
            });
        }
        let header = crate::core::volume::read_block_vec(self.device, entry_block)?;

        Ok(EntryAttributes {
            name: dir::name_of(&header),
            protection: get_u32(&header, layout::PROTECT_OFFSET)?,
            comment: crate::core::adf::bcpl::read_bcpl_string(&header, layout::COMMENT_OFFSET)
                .unwrap_or_default(),
            date: AmigaDate {
                days: get_u32(&header, layout::DAYS_OFFSET)?,
                mins: get_u32(&header, layout::MINS_OFFSET)?,
                ticks: get_u32(&header, layout::TICKS_OFFSET)?,
            },
            is_dir: dir::is_directory(&header)?,
        })
    }

    // ---- the shared machinery ----

    /// `0` means the root, as everywhere else in ART. Anything else has to be
    /// a real directory: writing a hash-table entry into a *file* header would
    /// corrupt the filesystem silently.
    fn resolve_directory(&self, block: u32) -> CoreResult<u32> {
        let block = if block == 0 {
            self.geometry.root_block
        } else {
            block
        };
        if block >= self.geometry.total_blocks {
            return Err(CoreError::Malformed {
                format: "volume".into(),
                detail: format!("block {block} is outside this volume"),
            });
        }

        let bytes = crate::core::volume::read_block_vec(self.device, block)?;
        if !dir::is_directory(&bytes)? {
            return Err(CoreError::InvalidInput(format!(
                "block {block} is not a directory"
            )));
        }
        Ok(block)
    }

    /// Add the rebuilt bitmap blocks to the working set.
    fn stamp_bitmap(
        &self,
        set: &mut BlockSet,
        allocator: &Allocator,
        changed: &[u32],
    ) -> CoreResult<()> {
        for (block, bytes) in allocator.rebuild_pages(self.device, changed)? {
            set.put(block, bytes)?;
        }
        Ok(())
    }

    /// Journal, write, validate, and either commit or roll back.
    ///
    /// The one place a block reaches the user's file. Everything above builds
    /// a `BlockSet` and hands it here, so there is a single point where the
    /// §57 pipeline — original, backup, operation, validation, commit — is
    /// either honoured or not.
    fn commit<F>(
        &mut self,
        description: &str,
        set: BlockSet,
        validate: F,
    ) -> CoreResult<WriteOutcome>
    where
        F: FnOnce(&dyn BlockDevice, &BlockSet, &VolumeGeometry) -> CoreResult<()>,
    {
        if set.is_empty() {
            return Err(CoreError::InvalidInput(
                "this operation would change nothing".into(),
            ));
        }

        let touched = set.touched();
        let geometry = self.geometry;
        let image = self.image.clone();
        let offset = self.volume_offset;

        let mut journal = Journalled::begin(self.device, &image, offset, description, &touched)?;

        // The writes. Any failure here rolls straight back.
        for (block, bytes) in set.iter() {
            if let Err(err) = journal.write_block(*block, bytes) {
                journal.roll_back()?;
                return Err(err);
            }
        }

        // Validation reads from the *device*, not the working set, so it
        // checks what actually landed. An empty set stands in for "no pending
        // edits" — everything is on the disk now.
        let fresh = BlockSet::new(geometry.block_size);
        let structural = validate_touched(&journal, &touched);
        let outcome = structural.and_then(|()| {
            let device: &dyn BlockDevice = &JournalView(&journal);
            validate(device, &fresh, &geometry)
        });

        if let Err(err) = outcome {
            journal.roll_back()?;
            return Err(err);
        }

        let blocks_touched = touched.len();
        journal.commit()?;

        Ok(WriteOutcome {
            block: None,
            blocks_touched,
            free_blocks: self.free_blocks()?,
            verified: true,
        })
    }
}

/// Every header block just written must have a checksum that adds up.
///
/// Cheap, and it catches the whole ART-033 class: a block ART is happy with
/// and AmigaDOS rejects. A free function rather than a method because the
/// journal already holds the device mutably, and the writer cannot lend it out
/// again while that borrow is alive.
fn validate_touched(journal: &Journalled<'_>, touched: &[u32]) -> CoreResult<()> {
    use crate::core::adf::checksum::block_checksum;

    for &block in touched {
        let bytes = journal.read_block(block)?;
        let block_type = get_u32(&bytes, layout::TYPE_OFFSET)?;

        // Bitmap blocks have no type field and carry their checksum at
        // longword 0; headers, data and list blocks carry it at 5.
        let offset = match block_type {
            2 | 8 | 16 => CHECKSUM_OFFSET,
            _ => continue,
        };

        let stored = get_u32(&bytes, offset)?;
        let mut check = bytes.clone();
        check[offset..offset + 4].fill(0);
        if block_checksum(&check, offset) != stored {
            return Err(CoreError::Malformed {
                format: "volume".into(),
                detail: format!("block {block} was written with a checksum that does not add up"),
            });
        }
    }
    Ok(())
}

/// Read a volume through an open journal.
///
/// Validation has to read what actually landed on the disk while the journal
/// still owns the device, so it borrows the device back through the guard.
struct JournalView<'a, 'b>(&'a Journalled<'b>);

impl BlockDevice for JournalView<'_, '_> {
    fn block_size(&self) -> usize {
        self.0.block_size()
    }

    fn total_blocks(&self) -> u32 {
        self.0.total_blocks()
    }

    fn read_block(&self, n: u32, buf: &mut [u8]) -> CoreResult<()> {
        let bytes = self.0.read_block(n)?;
        if buf.len() != bytes.len() {
            return Err(CoreError::InvalidInput(format!(
                "a block buffer must be {} bytes, got {}",
                bytes.len(),
                buf.len()
            )));
        }
        buf.copy_from_slice(&bytes);
        Ok(())
    }
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::volume::device::FileRegionMut;
    use crate::core::volume::fixture::ffs_volume;
    use crate::core::volume::journal::{find_journal, journal_path_for};
    use crate::core::volume::DosType;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("art-writer-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A real volume in a real file, which is what the journal needs.
    struct Disk {
        dir: PathBuf,
        path: PathBuf,
        geometry: VolumeGeometry,
    }

    impl Disk {
        fn new(name: &str, total_blocks: u32, dos: [u8; 4]) -> Self {
            let dir = scratch(name);
            let path = dir.join("disk.adf");
            let (bytes, geometry) = ffs_volume(total_blocks, DosType::new(dos));
            std::fs::write(&path, &bytes).unwrap();
            Self {
                dir,
                path,
                geometry,
            }
        }

        fn device(&self) -> FileRegionMut {
            FileRegionMut::open(
                &self.path,
                0,
                self.geometry.total_bytes(),
                self.geometry.block_size,
            )
            .unwrap()
        }

        fn bytes(&self) -> Vec<u8> {
            std::fs::read(&self.path).unwrap()
        }
    }

    impl Drop for Disk {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn floppy(name: &str) -> Disk {
        Disk::new(name, 1760, *b"DOS\x01")
    }

    /// `VolumeWriter` holds a `&mut` device, so the Ok side is not `Debug` and
    /// `unwrap_err` cannot be used on an open.
    fn refusal(result: CoreResult<VolumeWriter<'_>>) -> CoreError {
        match result {
            Ok(_) => panic!("this volume should have been refused for writing"),
            Err(err) => err,
        }
    }

    /// Run one operation against a disk and hand back the outcome.
    fn with_writer<F, T>(disk: &Disk, run: F) -> CoreResult<T>
    where
        F: FnOnce(&mut VolumeWriter<'_>) -> CoreResult<T>,
    {
        let mut device = disk.device();
        let mut writer = VolumeWriter::open(&mut device, disk.geometry, &disk.path, 0)?;
        run(&mut writer)
    }

    fn listing(disk: &Disk) -> Vec<dir::DirEntry> {
        let device = disk.device();
        let set = BlockSet::new(disk.geometry.block_size);
        dir::entries_in(&device, &set, &disk.geometry, disk.geometry.root_block).unwrap()
    }

    fn contents(disk: &Disk, block: u32) -> Vec<u8> {
        let device = disk.device();
        let set = BlockSet::new(disk.geometry.block_size);
        file::read_file(&device, &set, &disk.geometry, block).unwrap()
    }

    // ---- the happy paths ----

    #[test]
    fn a_file_written_to_a_real_disk_reads_back_and_is_listed() {
        let disk = floppy("add-file");
        let data: Vec<u8> = (0..5000u32).map(|i| (i % 241) as u8).collect();

        let outcome = with_writer(&disk, |w| {
            w.add_file(0, "Payload.bin", &data, FileMeta::default())
        })
        .unwrap();

        assert!(outcome.verified);
        let block = outcome.block.unwrap();
        assert_eq!(contents(&disk, block), data);

        let entries = listing(&disk);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "Payload.bin");
        assert!(!entries[0].is_dir);

        assert!(
            !journal_path_for(&disk.path).exists(),
            "a committed op leaves no journal"
        );
    }

    #[test]
    fn a_directory_is_created_and_can_hold_a_file() {
        let disk = floppy("mkdir");

        let made = with_writer(&disk, |w| w.make_dir(0, "Tools")).unwrap();
        let folder = made.block.unwrap();

        with_writer(&disk, |w| {
            w.add_file(folder, "Inside.txt", b"hello", FileMeta::default())
        })
        .unwrap();

        let root = listing(&disk);
        assert_eq!(root.len(), 1);
        assert!(root[0].is_dir);

        let device = disk.device();
        let set = BlockSet::new(512);
        let inside = dir::entries_in(&device, &set, &disk.geometry, folder).unwrap();
        assert_eq!(inside.len(), 1);
        assert_eq!(inside[0].name, "Inside.txt");
    }

    #[test]
    fn deleting_a_file_frees_its_blocks_and_removes_the_entry() {
        let disk = floppy("delete");
        let data = vec![9u8; 10_000];

        let before = with_writer(&disk, |w| w.free_blocks()).unwrap();
        let added = with_writer(&disk, |w| {
            w.add_file(0, "Big.bin", &data, FileMeta::default())
        })
        .unwrap();
        assert!(added.free_blocks < before);

        let deleted = with_writer(&disk, |w| w.delete(0, added.block.unwrap())).unwrap();
        assert!(listing(&disk).is_empty());
        assert_eq!(
            deleted.free_blocks, before,
            "every block a deleted file held must come back"
        );
    }

    #[test]
    fn renaming_moves_the_entry_to_its_new_bucket() {
        let disk = floppy("rename");
        let added = with_writer(&disk, |w| {
            w.add_file(0, "Before.txt", b"x", FileMeta::default())
        })
        .unwrap();
        let block = added.block.unwrap();

        with_writer(&disk, |w| w.rename(0, block, "After.txt")).unwrap();

        let entries = listing(&disk);
        assert_eq!(entries.len(), 1, "a rename must not duplicate the entry");
        assert_eq!(entries[0].name, "After.txt");
        assert_eq!(entries[0].block, block);

        // Findable by the new name, and not by the old one.
        let device = disk.device();
        let set = BlockSet::new(512);
        let root = disk.geometry.root_block;
        assert!(
            dir::find_entry(&device, &set, &disk.geometry, root, "After.txt")
                .unwrap()
                .is_some()
        );
        assert!(
            dir::find_entry(&device, &set, &disk.geometry, root, "Before.txt")
                .unwrap()
                .is_none()
        );
    }

    // ---- moved from `core/adf/mutate.rs` when that second writer was retired ----
    //
    // Each of these was the evidence for behaviour the old floppy-only writer
    // was audited into. The audit's value is the assertion, not the file it
    // lived in, so every one is restated here against the surviving path.

    /// A file long enough to need an extension chain must give *every* one of
    /// those blocks back, not just its data blocks. 100 × 512 bytes is 100
    /// data blocks, past the 72 a header holds inline, so the file grows an
    /// extension block the free-space accounting has to know about.
    #[test]
    fn deleting_frees_the_extension_blocks_too() {
        let disk = floppy("delete-extensions");
        let data = vec![9u8; 100 * 512];

        let before = with_writer(&disk, |w| w.free_blocks()).unwrap();
        let added = with_writer(&disk, |w| {
            w.add_file(0, "Big.bin", &data, FileMeta::default())
        })
        .unwrap();
        // More than the header plus its data: the extension block is in there.
        assert!(
            before - added.free_blocks > 1 + 100,
            "a 100-block file must also cost an extension block"
        );

        with_writer(&disk, |w| w.delete(0, added.block.unwrap())).unwrap();
        assert_eq!(
            with_writer(&disk, |w| w.free_blocks()).unwrap(),
            before,
            "every block a deleted file held must come back, extensions included"
        );
    }

    /// A rename touches the directory, never the payload.
    #[test]
    fn renaming_preserves_the_file_contents() {
        let disk = floppy("rename-contents");
        let payload = b"Original Content";

        let added = with_writer(&disk, |w| {
            w.add_file(0, "OldName.txt", payload, FileMeta::default())
        })
        .unwrap();
        let block = added.block.unwrap();

        with_writer(&disk, |w| w.rename(0, block, "NewName.txt")).unwrap();

        let entries = listing(&disk);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "NewName.txt");
        assert_eq!(contents(&disk, block), payload);
    }

    /// ART-012: AmigaDOS cannot hold two entries with the same name in one
    /// directory, and it compares them without regard to case. A rename onto a
    /// taken name has to be refused, and refused *before* the entry leaves its
    /// bucket — a half-done rename loses the file.
    #[test]
    fn rename_onto_an_existing_name_is_rejected() {
        let disk = floppy("rename-dupe");
        with_writer(&disk, |w| {
            w.add_file(0, "Taken.txt", b"a", FileMeta::default())
        })
        .unwrap();
        let second = with_writer(&disk, |w| {
            w.add_file(0, "Free.txt", b"b", FileMeta::default())
        })
        .unwrap()
        .block
        .unwrap();
        let before = disk.bytes();

        let err = with_writer(&disk, |w| w.rename(0, second, "Taken.txt")).unwrap_err();
        assert!(err.to_string().contains("already exists"), "{err}");

        // The failed rename left the directory exactly as it was.
        let names: Vec<String> = listing(&disk).into_iter().map(|e| e.name).collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"Taken.txt".to_string()));
        assert!(names.contains(&"Free.txt".to_string()));
        assert_eq!(disk.bytes(), before, "a refused rename must change nothing");
    }

    /// The exception to the rule above: an entry may always be renamed to a
    /// different casing of its own name. Checking availability first would find
    /// the entry itself and refuse.
    #[test]
    fn rename_can_change_only_case() {
        let disk = floppy("rename-case");
        let block = with_writer(&disk, |w| {
            w.add_file(0, "readme.txt", b"x", FileMeta::default())
        })
        .unwrap()
        .block
        .unwrap();

        with_writer(&disk, |w| w.rename(0, block, "README.TXT")).unwrap();

        let entries = listing(&disk);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "README.TXT");
    }

    /// ART-011: if the entry is not in the chain its name hashes to, the image
    /// is already inconsistent. Linking it into the new bucket anyway would
    /// leave it referenced from two places — the rename must refuse instead.
    #[test]
    fn rename_of_an_unlinked_entry_reports_an_error() {
        let disk = floppy("rename-orphan");
        let block = with_writer(&disk, |w| {
            w.add_file(0, "Ghost.txt", b"x", FileMeta::default())
        })
        .unwrap()
        .block
        .unwrap();

        // Simulate a corrupt image: clear every hash bucket, so the entry is no
        // longer reachable from its parent while its header still says it is.
        {
            let mut device = disk.device();
            let root_block = disk.geometry.root_block;
            let mut root = crate::core::volume::read_block_vec(&device, root_block).unwrap();
            for bucket in 0..layout::hash_table_size(disk.geometry.block_size) {
                layout::set_u32(&mut root, layout::TABLE_OFFSET + bucket * 4, 0).unwrap();
            }
            // Keep the block otherwise legal, so the refusal cannot be an
            // artefact of a checksum that no longer adds up.
            layout::set_u32(&mut root, CHECKSUM_OFFSET, 0).unwrap();
            let fixed = crate::core::adf::checksum::block_checksum(&root, CHECKSUM_OFFSET);
            layout::set_u32(&mut root, CHECKSUM_OFFSET, fixed).unwrap();
            device.write_block(root_block, &root).unwrap();
            device.sync().unwrap();
        }
        let before = disk.bytes();

        let err = with_writer(&disk, |w| w.rename(0, block, "Renamed.txt")).unwrap_err();
        assert!(matches!(err, CoreError::Malformed { .. }), "got {err:?}");
        assert_eq!(
            disk.bytes(),
            before,
            "refusing an inconsistent rename must not write anything"
        );
    }

    /// ART-012, the other half: a directory may not shadow a file's name any
    /// more than a second file may.
    #[test]
    fn a_directory_cannot_shadow_an_existing_file_name() {
        let disk = floppy("shadow");
        with_writer(&disk, |w| {
            w.add_file(0, "Same.txt", b"first", FileMeta::default())
        })
        .unwrap();
        let before = disk.bytes();

        let err = with_writer(&disk, |w| w.make_dir(0, "SAME.TXT")).unwrap_err();
        assert!(err.to_string().contains("already exists"), "{err}");
        assert_eq!(disk.bytes(), before);
    }

    /// A block number from the frontend or a corrupt image must become an
    /// error, never a panic — the release profile aborts on one.
    #[test]
    fn out_of_range_directory_block_is_an_error() {
        let disk = floppy("out-of-range");

        // Far past the end of an 880 KB image…
        let err = with_writer(&disk, |w| {
            w.add_file(99_999, "X.txt", b"x", FileMeta::default())
        })
        .unwrap_err();
        assert!(matches!(err, CoreError::Malformed { .. }), "got {err:?}");

        // …and the extreme value that would overflow an offset calculation.
        assert!(with_writer(&disk, |w| w.make_dir(u32::MAX, "Y")).is_err());
    }

    /// The whole reason for the generalisation: the same code, a volume that
    /// is not floppy-shaped, and a root block that is not 880.
    #[test]
    fn the_same_writer_works_on_a_volume_that_is_not_a_floppy() {
        let disk = Disk::new("big", 40_000, *b"DOS\x01");
        assert_ne!(disk.geometry.root_block, 880);
        assert!(
            bitmap::BitmapLayout::pages_needed(&disk.geometry) > 1,
            "this size must need more than one bitmap block"
        );

        let data: Vec<u8> = (0..60_000u32).map(|i| (i % 233) as u8).collect();
        let outcome = with_writer(&disk, |w| {
            w.add_file(0, "Large.bin", &data, FileMeta::default())
        })
        .unwrap();

        assert_eq!(contents(&disk, outcome.block.unwrap()), data);
    }

    #[test]
    fn an_ofs_volume_takes_files_too() {
        let disk = Disk::new("ofs", 1760, *b"DOS\x00");
        assert!(!disk.geometry.dos_type.is_ffs());

        let data: Vec<u8> = (0..3000u32).map(|i| (i % 251) as u8).collect();
        let outcome = with_writer(&disk, |w| {
            w.add_file(0, "Ofs.bin", &data, FileMeta::default())
        })
        .unwrap();

        assert_eq!(contents(&disk, outcome.block.unwrap()), data);
    }

    // ---- refusals, before anything is touched ----

    #[test]
    fn a_dircache_volume_is_read_only() {
        let disk = Disk::new("dircache", 1760, *b"DOS\x05");
        let before = disk.bytes();

        let mut device = disk.device();
        let err = refusal(VolumeWriter::open(
            &mut device,
            disk.geometry,
            &disk.path,
            0,
        ));
        assert!(err.to_string().contains("dircache"), "{err}");
        assert!(err.to_string().contains("read-only"), "{err}");

        drop(device);
        assert_eq!(disk.bytes(), before, "a refused open must change nothing");
    }

    #[test]
    fn a_long_filename_volume_is_refused() {
        let disk = Disk::new("lnfs", 1760, *b"DOS\x07");
        let mut device = disk.device();
        let err = refusal(VolumeWriter::open(
            &mut device,
            disk.geometry,
            &disk.path,
            0,
        ));
        assert!(err.to_string().contains("long-filename"), "{err}");
    }

    #[test]
    fn an_intl_volume_is_writable() {
        let disk = Disk::new("intl", 1760, *b"DOS\x03");
        let mut device = disk.device();
        assert!(VolumeWriter::open(&mut device, disk.geometry, &disk.path, 0).is_ok());
    }

    #[test]
    fn a_name_already_taken_is_refused_and_nothing_is_written() {
        let disk = floppy("duplicate");
        with_writer(&disk, |w| {
            w.add_file(0, "Readme", b"one", FileMeta::default())
        })
        .unwrap();
        let before = disk.bytes();

        let err = with_writer(&disk, |w| {
            w.add_file(0, "README", b"two", FileMeta::default())
        })
        .unwrap_err();
        assert!(err.to_string().contains("already exists"), "{err}");
        assert_eq!(disk.bytes(), before, "a refused write must change nothing");
    }

    /// §3.3: refuse up front with real numbers, never fail half way.
    #[test]
    fn a_file_that_does_not_fit_is_refused_with_the_numbers() {
        let disk = floppy("full");
        let free = with_writer(&disk, |w| w.free_blocks()).unwrap();
        let before = disk.bytes();

        let too_big = vec![0u8; (free + 10) * 512];
        let err = with_writer(&disk, |w| {
            w.add_file(0, "TooBig.bin", &too_big, FileMeta::default())
        })
        .unwrap_err();

        let message = err.to_string();
        assert!(message.contains("blocks and"), "{message}");
        assert!(message.contains("are free"), "{message}");
        assert_eq!(disk.bytes(), before, "a refused write must change nothing");
    }

    /// The boundary the previous test avoids: a copy that fills the disk
    /// exactly has to succeed, not be refused by an off-by-one.
    #[test]
    fn a_file_that_fills_the_disk_exactly_succeeds() {
        let disk = floppy("exact");
        let free = with_writer(&disk, |w| w.free_blocks()).unwrap();

        // The largest file that fits is not `free - 1` blocks of data: past 72
        // data blocks the file grows extension blocks, and those come out of
        // the same budget. Searching for it is the honest way to find the
        // boundary — and getting it wrong is exactly the off-by-one this test
        // exists to catch.
        let mut data_blocks = free - 1;
        while file::budget_for((data_blocks * 512) as u64, &disk.geometry)
            .unwrap()
            .total()
            > free
        {
            data_blocks -= 1;
        }
        assert_eq!(
            file::budget_for((data_blocks * 512) as u64, &disk.geometry)
                .unwrap()
                .total(),
            free,
            "the search must land on a file that fills the disk exactly"
        );

        let data = vec![3u8; data_blocks * 512];
        let outcome = with_writer(&disk, |w| {
            w.add_file(0, "Exact.bin", &data, FileMeta::default())
        })
        .unwrap();

        assert_eq!(outcome.free_blocks, 0, "the disk should now be full");
        assert_eq!(contents(&disk, outcome.block.unwrap()), data);
    }

    #[test]
    fn a_name_amigados_cannot_store_is_refused() {
        let disk = floppy("badname");
        let before = disk.bytes();

        for name in [
            "a".repeat(31),
            "with/slash".into(),
            "DH0:x".into(),
            "".into(),
        ] {
            assert!(
                with_writer(&disk, |w| w.add_file(0, &name, b"x", FileMeta::default())).is_err(),
                "'{name}' should have been refused"
            );
        }
        assert_eq!(disk.bytes(), before);
    }

    #[test]
    fn a_directory_with_things_in_it_is_not_deleted() {
        let disk = floppy("nonempty");
        let folder = with_writer(&disk, |w| w.make_dir(0, "Tools"))
            .unwrap()
            .block
            .unwrap();
        with_writer(&disk, |w| {
            w.add_file(folder, "Inside", b"x", FileMeta::default())
        })
        .unwrap();
        let before = disk.bytes();

        let err = with_writer(&disk, |w| w.delete(0, folder)).unwrap_err();
        assert!(err.to_string().contains("Empty it first"), "{err}");
        assert_eq!(disk.bytes(), before);
    }

    #[test]
    fn writing_into_a_file_header_instead_of_a_directory_is_refused() {
        let disk = floppy("notadir");
        let added = with_writer(&disk, |w| {
            w.add_file(0, "File.txt", b"x", FileMeta::default())
        })
        .unwrap();

        let err = with_writer(&disk, |w| {
            w.add_file(added.block.unwrap(), "Nested", b"x", FileMeta::default())
        })
        .unwrap_err();
        assert!(err.to_string().contains("not a directory"), "{err}");
    }

    /// §3.1: a bitmap marked invalid means the map may not describe the disk,
    /// and allocating from it is how two files come to own the same block.
    #[test]
    fn a_volume_whose_bitmap_is_marked_invalid_refuses_writes() {
        let disk = floppy("dirty-bitmap");

        // Mark the bitmap dirty, the way an unclean shutdown would.
        {
            let mut device = disk.device();
            let mut root =
                crate::core::volume::read_block_vec(&device, disk.geometry.root_block).unwrap();
            layout::set_u32(&mut root, bitmap::BM_FLAG_OFFSET, 0).unwrap();
            device.write_block(disk.geometry.root_block, &root).unwrap();
            device.sync().unwrap();
        }
        let before = disk.bytes();

        let err = with_writer(&disk, |w| {
            w.add_file(0, "Nope.txt", b"x", FileMeta::default())
        })
        .unwrap_err();
        assert!(err.to_string().contains("not shut down cleanly"), "{err}");
        assert_eq!(disk.bytes(), before);
    }

    // ---- the journal, end to end ----

    /// The §57 promise at block granularity: a validation failure puts every
    /// byte back. The operation writes a header block whose checksum will not
    /// add up, which is exactly what the post-write check exists to catch.
    #[test]
    fn a_failed_validation_rolls_the_whole_operation_back() {
        let disk = floppy("rollback");
        let before = disk.bytes();

        let mut device = disk.device();
        let mut writer = VolumeWriter::open(&mut device, disk.geometry, &disk.path, 0).unwrap();

        let mut set = BlockSet::new(512);
        let bytes = set.blank(900);
        layout::set_i32(bytes, layout::TYPE_OFFSET, 2).unwrap();
        layout::set_u32(bytes, CHECKSUM_OFFSET, 0xDEAD_BEEF).unwrap();

        let err = writer
            .commit("Deliberately broken", set, |_, _, _| Ok(()))
            .unwrap_err();
        assert!(err.to_string().contains("checksum"), "{err}");

        drop(writer);
        drop(device);
        assert_eq!(
            disk.bytes(),
            before,
            "a failed operation must leave the image byte-for-byte unchanged"
        );
        assert!(!journal_path_for(&disk.path).exists());
    }

    /// The crash case, end to end: the process dies mid-operation, the next
    /// mount finds the journal, and rolling it back restores the disk exactly.
    #[test]
    fn a_crash_mid_operation_is_recovered_at_the_next_mount() {
        let disk = floppy("crash");
        let before = disk.bytes();

        {
            let mut device = disk.device();
            let mut journal = crate::core::volume::journal::Journalled::begin(
                &mut device,
                &disk.path,
                0,
                "Copy in Something.bin",
                &[900, 901, 902],
            )
            .unwrap();
            journal.write_block(900, &[0xAA; 512]).unwrap();
            journal.write_block(901, &[0xBB; 512]).unwrap();
            // …and the process dies, with 902 never written.
            std::mem::forget(journal);
        }
        assert_ne!(disk.bytes(), before, "the half-finished write is on disk");

        let pending = find_journal(&disk.path).unwrap().unwrap();
        assert_eq!(pending.header().description, "Copy in Something.bin");
        let report = pending.roll_back().unwrap();
        assert_eq!(report.blocks_restored, 3);

        assert_eq!(disk.bytes(), before);
    }

    // ---- the invariant that matters most ----

    /// After any sequence of operations the bitmap must agree with what the
    /// files actually occupy: no block free that a file uses, no block used
    /// that nothing references, and no block claimed twice.
    ///
    /// This is the single highest-value test in the stage (brief §9.2) —
    /// almost every way a filesystem writer can be wrong shows up here.
    #[test]
    fn the_bitmap_always_agrees_with_what_the_files_occupy() {
        let disk = floppy("invariants");
        let mut blocks: Vec<u32> = Vec::new();

        // A deterministic mix of sizes, spanning the extension-block boundary
        // and both sides of it.
        let sizes = [0usize, 1, 511, 512, 513, 5000, 40_000, 100, 37_000];

        for (index, size) in sizes.iter().enumerate() {
            let data: Vec<u8> = (0..*size).map(|i| (i % 251) as u8).collect();
            let outcome = with_writer(&disk, |w| {
                w.add_file(0, &format!("F{index}.bin"), &data, FileMeta::default())
            })
            .unwrap();
            blocks.push(outcome.block.unwrap());
            check_invariants(&disk);
        }

        // Delete every other one, then refill the gaps.
        for block in blocks.iter().step_by(2) {
            with_writer(&disk, |w| w.delete(0, *block)).unwrap();
            check_invariants(&disk);
        }

        for index in 0..4 {
            let data = vec![index as u8; 2000 * (index + 1)];
            with_writer(&disk, |w| {
                w.add_file(0, &format!("Refill{index}"), &data, FileMeta::default())
            })
            .unwrap();
            check_invariants(&disk);
        }

        // And a rename, which must not change occupancy at all.
        let survivors = listing(&disk);
        let before = with_writer(&disk, |w| w.free_blocks()).unwrap();
        with_writer(&disk, |w| w.rename(0, survivors[0].block, "Renamed.bin")).unwrap();
        assert_eq!(
            with_writer(&disk, |w| w.free_blocks()).unwrap(),
            before,
            "a rename must not change how much space is used"
        );
        check_invariants(&disk);
    }

    /// Walk every file and compare against the bitmap.
    fn check_invariants(disk: &Disk) {
        let device = disk.device();
        let set = BlockSet::new(disk.geometry.block_size);
        let allocator = Allocator::load(&device, &disk.geometry).unwrap();

        let mut owned: std::collections::HashSet<u32> = std::collections::HashSet::new();

        // The furniture: reserved blocks, the root, and every bitmap page.
        for block in 0..disk.geometry.reserved {
            owned.insert(block);
        }
        owned.insert(disk.geometry.root_block);
        for page in allocator.all_pages() {
            assert!(owned.insert(*page), "bitmap page {page} is claimed twice");
        }

        let mut queue = vec![disk.geometry.root_block];
        while let Some(dir_block) = queue.pop() {
            for entry in dir::entries_in(&device, &set, &disk.geometry, dir_block).unwrap() {
                if entry.is_dir {
                    assert!(
                        owned.insert(entry.block),
                        "block {} is claimed by more than one entry",
                        entry.block
                    );
                    queue.push(entry.block);
                    continue;
                }

                let file = file::blocks_of(&device, &set, &disk.geometry, entry.block).unwrap();
                for block in file.all_blocks() {
                    assert!(
                        owned.insert(block),
                        "block {block} is claimed by more than one file"
                    );
                }
            }
        }

        for block in &owned {
            assert!(
                !allocator.is_free(*block),
                "block {block} is in use but the bitmap says it is free"
            );
        }

        let used_by_bitmap = disk.geometry.total_blocks as usize - allocator.free_count();
        assert_eq!(
            used_by_bitmap,
            owned.len(),
            "the bitmap says {used_by_bitmap} blocks are used and the files account for {}",
            owned.len()
        );
    }
}

// ---------------------------------------------------------------------------
// Oracle export hooks
// ---------------------------------------------------------------------------
//
// ART's own tests cannot catch a format mistake its reader and writer *share*.
// Four of them shipped that way (ART-032 … ART-035): every one round-tripped
// through ART perfectly and was rejected by every real Amiga tool. These hooks
// write images with the Stage W writer so `scripts/oracle-check.py` can read
// them back with `amitools`, which shares no code with ART.
//
// They are `#[test]` functions rather than a binary so they reach the internal
// API without exposing it, and they do nothing at all unless their environment
// variable is set — `cargo test` on a normal run pays a function call.

#[cfg(test)]
mod oracle_exports {
    use std::path::{Path, PathBuf};

    use crate::core::volume::device::FileRegionMut;
    use crate::core::volume::fixture::ffs_volume;
    use crate::core::volume::write::{FileMeta, VolumeWriter};
    use crate::core::volume::{DosType, VolumeGeometry};

    /// Build a volume, run `work` against it, and leave it at `dest`.
    fn write_volume<F>(dest: &Path, total_blocks: u32, dos: [u8; 4], work: F)
    where
        F: FnOnce(&mut VolumeWriter<'_>),
    {
        let (bytes, geometry) = ffs_volume(total_blocks, DosType::new(dos));
        std::fs::write(dest, &bytes).unwrap();

        let mut device =
            FileRegionMut::open(dest, 0, geometry.total_bytes(), geometry.block_size).unwrap();
        let mut writer = VolumeWriter::open(&mut device, geometry, dest, 0).unwrap();
        work(&mut writer);
    }

    /// A tree the oracle can check every property of: nested folders, a file
    /// that fits in one block, one that spans several, and one long enough to
    /// need an extension block.
    fn build_tree(writer: &mut VolumeWriter<'_>, geometry: &VolumeGeometry) {
        let _ = geometry;

        writer
            .add_file(0, "Readme", b"hello from ART", FileMeta::default())
            .unwrap();

        let tools = writer.make_dir(0, "Tools").unwrap().block.unwrap();
        writer
            .add_file(tools, "Notes.txt", b"nested file", FileMeta::default())
            .unwrap();

        // Three blocks on FFS, four on OFS — the multi-block path either way.
        let medium: Vec<u8> = (0..1500u32).map(|i| (i % 251) as u8).collect();
        writer
            .add_file(0, "Medium.bin", &medium, FileMeta::default())
            .unwrap();

        // Past 72 data blocks, so the file grows an extension chain — the
        // part a reader that only ever saw small files never exercises.
        let large: Vec<u8> = (0..100 * 512u32).map(|i| (i % 253) as u8).collect();
        writer
            .add_file(0, "Large.bin", &large, FileMeta::default())
            .unwrap();
    }

    /// A floppy written entirely by the Stage W writer.
    #[test]
    fn export_written_adf_for_oracle_when_asked() {
        let Ok(dest) = std::env::var("ART_W_ADF_OUT") else {
            return;
        };
        let dest = PathBuf::from(dest);
        let geometry = VolumeGeometry::floppy_dd(DosType::new(*b"DOS\x01"));
        write_volume(&dest, 1760, *b"DOS\x01", |writer| {
            build_tree(writer, &geometry)
        });
    }

    /// The same tree on an OFS volume, where every data block carries its own
    /// 24-byte header, sequence number and checksum.
    #[test]
    fn export_written_ofs_adf_for_oracle_when_asked() {
        let Ok(dest) = std::env::var("ART_W_OFS_OUT") else {
            return;
        };
        let dest = PathBuf::from(dest);
        let geometry = VolumeGeometry::floppy_dd(DosType::new(*b"DOS\x00"));
        write_volume(&dest, 1760, *b"DOS\x00", |writer| {
            build_tree(writer, &geometry)
        });
    }

    /// A volume big enough to need more than one bitmap block and a root block
    /// that is not 880 — the whole point of the generalisation. Written with
    /// the block-journal strategy, since it is past the 16 MiB threshold.
    #[test]
    fn export_written_large_volume_for_oracle_when_asked() {
        let Ok(dest) = std::env::var("ART_W_LARGE_OUT") else {
            return;
        };
        let dest = PathBuf::from(dest);

        // 64 MiB: 33 bitmap blocks, so the extension chain is exercised too.
        let total_blocks = 131_072u32;
        let geometry =
            VolumeGeometry::new(512, total_blocks, 2, DosType::new(*b"DOS\x01")).unwrap();
        write_volume(&dest, total_blocks, *b"DOS\x01", |writer| {
            build_tree(writer, &geometry)
        });
    }

    /// Read a volume some other tool wrote, and print what ART found.
    ///
    /// The reverse of the export hooks: `scripts/oracle-check.py` has
    /// `xdftool` build an image, then runs this to see whether ART agrees
    /// about what is in it. Printing rather than asserting, so the script owns
    /// the expectations and one place says what "correct" means.
    #[test]
    fn read_foreign_volume_for_oracle_when_asked() {
        let Ok(source) = std::env::var("ART_W_READ_IN") else {
            return;
        };
        let image = PathBuf::from(source);

        let scan = crate::core::volume::mount::scan_image(&image).unwrap();
        let entry = scan
            .volumes
            .first()
            .expect("the image should hold a volume");
        let (device, geometry) = crate::core::volume::mount::mount(&image, entry).unwrap();

        let root = crate::core::volume::read_block_vec(&device, geometry.root_block).unwrap();
        let name = crate::core::adf::blocks::RootBlock::parse(&root)
            .map(|parsed| parsed.volume_name)
            .unwrap_or_default();
        println!("volume={name}");

        let set = crate::core::volume::write::layout::BlockSet::new(geometry.block_size);
        let entries = crate::core::volume::write::dir::entries_in(
            &device,
            &set,
            &geometry,
            geometry.root_block,
        )
        .unwrap();

        for found in &entries {
            println!(
                "{}:{}",
                if found.is_dir { "dir" } else { "file" },
                found.name
            );
        }

        // Walk into the folder, so a directory ART merely lists is not mistaken
        // for one it can actually open.
        for found in entries.iter().filter(|e| e.is_dir) {
            for inner in
                crate::core::volume::write::dir::entries_in(&device, &set, &geometry, found.block)
                    .unwrap()
            {
                println!("nested:{}", inner.name);
            }
        }

        // And read a file's bytes back, which is the claim that matters.
        let expected = b"written by amitools, not by ART
";
        let matched = entries
            .iter()
            .find(|e| !e.is_dir && e.name == "Readme")
            .map(|e| {
                crate::core::volume::write::file::read_file(&device, &set, &geometry, e.block)
                    .map(|data| data == expected)
                    .unwrap_or(false)
            })
            .unwrap_or(false);
        println!("contents-match={matched}");
    }

    /// Protection bits and a comment, so the oracle can confirm the `HSPARWED`
    /// inversion is the right way round. Getting it backwards produces files
    /// nobody can read, and it looks correct until someone boots the disk.
    #[test]
    fn export_written_attributes_for_oracle_when_asked() {
        let Ok(dest) = std::env::var("ART_W_ATTR_OUT") else {
            return;
        };
        let dest = PathBuf::from(dest);

        write_volume(&dest, 1760, *b"DOS\x01", |writer| {
            let block = writer
                .add_file(0, "Game.slave", b"slave bytes", FileMeta::default())
                .unwrap()
                .block
                .unwrap();

            // S and P set, RWED all granted: `-sp-rwed`. The low four bits are
            // inverted, so granting them means storing zeros.
            let script_and_pure = (1 << 6) | (1 << 5);
            writer
                .set_attributes(block, Some(script_and_pure), Some("kept by ART"), None)
                .unwrap();
        });
    }
}
