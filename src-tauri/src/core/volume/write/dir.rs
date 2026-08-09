//! Directory entries: hashing a name into a bucket, and the chain that hangs
//! off it.
//!
//! Ported from `core/adf/mutate.rs`, which had the same logic audited into
//! shape by ART-009 … ART-013. The differences are all consequences of the
//! geometry no longer being a floppy's:
//!
//! - the hash table's size comes from the block, not from a constant 72
//! - `international` comes from **this volume's** dostype, never a global
//!   setting (the ART-010 lesson)
//! - every block read or written goes through a [`BlockSet`], so the caller
//!   knows the complete set before the journal opens
//!
//! ## Why the chain walk is bounded
//!
//! A hash bucket is the head of a singly linked list threaded through the
//! `next_hash` field of each entry. A corrupt image can point an entry at
//! itself, and an unbounded walk is then an infinite loop inside an operation
//! the user cannot cancel (ART-008). Every walk here stops after
//! [`MAX_CHAIN_STEPS`].

use crate::core::adf::bcpl::read_bcpl_string;
use crate::core::adf::hash::name_hash;
use crate::core::error::{CoreError, CoreResult};
use crate::core::volume::{BlockDevice, VolumeGeometry};

use super::layout::{
    amiga_now, get_i32, get_u32, hash_table_size, set_date, set_i32, set_u32, BlockSet,
    CHECKSUM_OFFSET, DEFAULT_PROTECTION, NAME_FIELD_LEN, NAME_OFFSET, NEXT_HASH_OFFSET,
    PARENT_OFFSET, PROTECT_OFFSET, SUBTYPE_OFFSET, TABLE_OFFSET, TYPE_OFFSET,
};

/// The longest hash chain ART will walk.
///
/// A bucket with more entries than a volume has blocks is a loop, not a
/// directory.
pub const MAX_CHAIN_STEPS: usize = 65_536;

/// The longest name AmigaDOS stores.
pub const MAX_NAME_LEN: usize = 30;

pub mod block_type {
    pub const HEADER: i32 = 2;
    pub const DATA: i32 = 8;
    pub const LIST: i32 = 16;
}

pub mod subtype {
    pub const ROOT: i32 = 1;
    pub const USERDIR: i32 = 2;
    pub const FILE: i32 = -3;
}

/// One entry as the writer needs to see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub block: u32,
    pub name: String,
    pub is_dir: bool,
}

/// Check a name AmigaDOS will have to store.
///
/// Refuses rather than silently truncating or substituting: a user who asked
/// for `My Very Long Document Name.txt` and got `My Very Long Document Name.t`
/// has a file they cannot find by the name they typed. The caller offers a
/// rename; the filesystem layer says no (brief §4.1).
pub fn check_name(name: &str) -> CoreResult<String> {
    let trimmed = name.trim();

    if trimmed.is_empty() {
        return Err(CoreError::InvalidInput("a name cannot be empty".into()));
    }
    // Characters, not bytes. AmigaDOS stores one byte per Latin-1 character,
    // while Rust counts UTF-8 bytes — so `str::len` would refuse a perfectly
    // legal 30-character name like "Grüße vom Süden" as too long, and the
    // longer the name's accented content the worse the answer.
    let length = trimmed.chars().count();
    if length > MAX_NAME_LEN {
        return Err(CoreError::InvalidInput(format!(
            "'{trimmed}' is {length} characters; AmigaDOS names stop at {MAX_NAME_LEN}"
        )));
    }
    if let Some(bad) = trimmed.chars().find(|c| *c == ':' || *c == '/') {
        return Err(CoreError::InvalidInput(format!(
            "'{trimmed}' contains '{bad}', which separates paths on the Amiga and \
             cannot appear in a name"
        )));
    }
    if trimmed
        .chars()
        .any(|c| (c as u32) < 0x20 || (c as u32) == 0x7F)
    {
        return Err(CoreError::InvalidInput(format!(
            "'{trimmed}' contains a control character"
        )));
    }
    // AmigaDOS names are Latin-1. A character outside it has no byte to be
    // stored as, and picking a substitute would rename the user's file behind
    // their back.
    if let Some(bad) = trimmed.chars().find(|c| (*c as u32) > 0xFF) {
        return Err(CoreError::InvalidInput(format!(
            "'{trimmed}' contains '{bad}', which AmigaDOS's Latin-1 character set \
             cannot store"
        )));
    }

    Ok(trimmed.to_string())
}

/// Encode a checked name the way AmigaDOS stores it: one byte per character.
fn latin1(name: &str) -> Vec<u8> {
    name.chars().map(|c| c as u8).collect()
}

/// Which bucket `name` hashes into, for this volume.
pub fn bucket_for(name: &str, geometry: &VolumeGeometry) -> CoreResult<usize> {
    let bucket = name_hash(name, geometry.dos_type.is_international()) as usize;
    let size = hash_table_size(geometry.block_size);
    if bucket >= size {
        return Err(CoreError::Malformed {
            format: "volume".into(),
            detail: format!("hash bucket {bucket} is outside a table of {size}"),
        });
    }
    Ok(bucket)
}

fn bucket_offset(bucket: usize) -> usize {
    TABLE_OFFSET + bucket * 4
}

/// The name stored in a header block.
pub fn name_of(block: &[u8]) -> String {
    read_bcpl_string(block, NAME_OFFSET).unwrap_or_default()
}

/// Whether a header block is a directory (or the root).
pub fn is_directory(block: &[u8]) -> CoreResult<bool> {
    let subtype = get_i32(block, SUBTYPE_OFFSET)?;
    Ok(subtype == subtype::USERDIR || subtype == subtype::ROOT)
}

/// Every entry directly inside `dir_block`.
///
/// Reads through the working set, so an operation sees its own half-finished
/// changes rather than what is still on the disk.
pub fn entries_in<D: BlockDevice + ?Sized>(
    device: &D,
    set: &BlockSet,
    geometry: &VolumeGeometry,
    dir_block: u32,
) -> CoreResult<Vec<DirEntry>> {
    let dir = set.view(device, dir_block)?;
    let mut out = Vec::new();

    for bucket in 0..hash_table_size(geometry.block_size) {
        let mut next = get_u32(&dir, bucket_offset(bucket))?;
        let mut steps = 0usize;

        while next != 0 {
            steps += 1;
            if steps > MAX_CHAIN_STEPS {
                return Err(CoreError::Malformed {
                    format: "volume".into(),
                    detail: format!("hash bucket {bucket} does not end"),
                });
            }
            if next >= geometry.total_blocks {
                return Err(CoreError::Malformed {
                    format: "volume".into(),
                    detail: format!("entry block {next} is outside the volume"),
                });
            }

            let entry = set.view(device, next)?;
            out.push(DirEntry {
                block: next,
                name: name_of(&entry),
                is_dir: is_directory(&entry)?,
            });
            next = get_u32(&entry, NEXT_HASH_OFFSET)?;
        }
    }

    Ok(out)
}

/// Find an entry by name. Case-insensitive, as AmigaDOS is (ART-012).
pub fn find_entry<D: BlockDevice + ?Sized>(
    device: &D,
    set: &BlockSet,
    geometry: &VolumeGeometry,
    dir_block: u32,
    name: &str,
) -> CoreResult<Option<DirEntry>> {
    let wanted = name.to_lowercase();
    Ok(entries_in(device, set, geometry, dir_block)?
        .into_iter()
        .find(|entry| entry.name.to_lowercase() == wanted))
}

/// Refuse a name already taken in this directory.
///
/// Case-insensitive: AmigaDOS would treat `Readme` and `README` as the same
/// entry, so creating the second silently shadows the first.
pub fn ensure_available<D: BlockDevice + ?Sized>(
    device: &D,
    set: &BlockSet,
    geometry: &VolumeGeometry,
    dir_block: u32,
    name: &str,
) -> CoreResult<()> {
    if find_entry(device, set, geometry, dir_block, name)?.is_some() {
        return Err(CoreError::InvalidInput(format!(
            "'{name}' already exists here"
        )));
    }
    Ok(())
}

/// Blocks that linking a new entry into `dir_block` would touch.
///
/// The directory block itself, always: the new entry goes at the head of its
/// bucket, so no existing entry has to be rewritten. Knowing that up front is
/// what lets the journal be complete.
pub fn link_touches(dir_block: u32) -> Vec<u32> {
    vec![dir_block]
}

/// Put `entry_block` at the head of its bucket in `dir_block`.
///
/// Head insertion rather than appending: it touches one block instead of
/// walking to the end of a chain and rewriting the last entry, which keeps the
/// journalled block set small and predictable.
pub fn link_into<D: BlockDevice + ?Sized>(
    device: &D,
    set: &mut BlockSet,
    geometry: &VolumeGeometry,
    dir_block: u32,
    entry_block: u32,
    name: &str,
) -> CoreResult<()> {
    let bucket = bucket_for(name, geometry)?;
    let offset = bucket_offset(bucket);

    let current_head = {
        let dir = set.edit(device, dir_block)?;
        get_u32(dir, offset)?
    };

    {
        let entry = set.edit(device, entry_block)?;
        set_u32(entry, NEXT_HASH_OFFSET, current_head)?;
    }
    set.checksum(entry_block, CHECKSUM_OFFSET)?;

    {
        let dir = set.edit(device, dir_block)?;
        set_u32(dir, offset, entry_block)?;
    }
    set.checksum(dir_block, CHECKSUM_OFFSET)?;
    Ok(())
}

/// Where an entry sits in its bucket: the block pointing at it, if any.
///
/// `None` means the entry is the head and the directory's own table points at
/// it. `Some(block)` means `block`'s `next_hash` does.
fn predecessor_of<D: BlockDevice + ?Sized>(
    device: &D,
    set: &BlockSet,
    geometry: &VolumeGeometry,
    dir_block: u32,
    entry_block: u32,
    name: &str,
) -> CoreResult<(usize, Option<u32>)> {
    let bucket = bucket_for(name, geometry)?;
    let dir = set.view(device, dir_block)?;

    let mut current = get_u32(&dir, bucket_offset(bucket))?;
    if current == entry_block {
        return Ok((bucket, None));
    }

    let mut steps = 0usize;
    while current != 0 {
        steps += 1;
        if steps > MAX_CHAIN_STEPS {
            return Err(CoreError::Malformed {
                format: "volume".into(),
                detail: "a hash chain does not end".into(),
            });
        }
        if current >= geometry.total_blocks {
            return Err(CoreError::Malformed {
                format: "volume".into(),
                detail: format!("entry block {current} is outside the volume"),
            });
        }

        let node = set.view(device, current)?;
        let next = get_u32(&node, NEXT_HASH_OFFSET)?;
        if next == entry_block {
            return Ok((bucket, Some(current)));
        }
        current = next;
    }

    Err(CoreError::Malformed {
        format: "volume".into(),
        detail: format!("block {entry_block} is not in the bucket its name hashes to"),
    })
}

/// Blocks that unlinking an entry would touch.
///
/// Either the directory block (the entry is a bucket head) or the entry
/// before it in the chain — plus the entry itself, whose `next_hash` is
/// cleared. Computed before the journal opens.
pub fn unlink_touches<D: BlockDevice + ?Sized>(
    device: &D,
    set: &BlockSet,
    geometry: &VolumeGeometry,
    dir_block: u32,
    entry_block: u32,
    name: &str,
) -> CoreResult<Vec<u32>> {
    let (_, predecessor) = predecessor_of(device, set, geometry, dir_block, entry_block, name)?;
    Ok(match predecessor {
        Some(previous) => vec![previous, entry_block],
        None => vec![dir_block, entry_block],
    })
}

/// Take `entry_block` out of its bucket in `dir_block`.
pub fn unlink_from<D: BlockDevice + ?Sized>(
    device: &D,
    set: &mut BlockSet,
    geometry: &VolumeGeometry,
    dir_block: u32,
    entry_block: u32,
    name: &str,
) -> CoreResult<()> {
    let (bucket, predecessor) =
        predecessor_of(device, set, geometry, dir_block, entry_block, name)?;

    let successor = {
        let entry = set.edit(device, entry_block)?;
        let next = get_u32(entry, NEXT_HASH_OFFSET)?;
        set_u32(entry, NEXT_HASH_OFFSET, 0)?;
        next
    };
    set.checksum(entry_block, CHECKSUM_OFFSET)?;

    match predecessor {
        Some(previous) => {
            let node = set.edit(device, previous)?;
            set_u32(node, NEXT_HASH_OFFSET, successor)?;
            set.checksum(previous, CHECKSUM_OFFSET)?;
        }
        None => {
            let dir = set.edit(device, dir_block)?;
            set_u32(dir, bucket_offset(bucket), successor)?;
            set.checksum(dir_block, CHECKSUM_OFFSET)?;
        }
    }

    Ok(())
}

/// Fill in a new directory header block.
///
/// The block must already be allocated and blank in the set; linking it into
/// its parent is a separate step, so a caller can build several and link them
/// in one journalled operation.
pub fn write_dir_header(set: &mut BlockSet, block: u32, parent: u32, name: &str) -> CoreResult<()> {
    let checked = check_name(name)?;

    {
        let dir = set.blank(block);
        set_i32(dir, TYPE_OFFSET, block_type::HEADER)?;
        set_u32(dir, super::layout::HEADER_KEY_OFFSET, block)?;
        set_u32(dir, PROTECT_OFFSET, DEFAULT_PROTECTION)?;
        set_date(dir, amiga_now())?;
        put_name(dir, &checked)?;
        set_u32(dir, PARENT_OFFSET, parent)?;
        set_i32(dir, SUBTYPE_OFFSET, subtype::USERDIR)?;
    }
    set.checksum(block, CHECKSUM_OFFSET)?;
    Ok(())
}

/// Write a checked name into a header block's BSTR field.
///
/// AmigaDOS stores names as Latin-1, one byte per character. Rust strings are
/// UTF-8, where anything above `~` takes two bytes — so `write_bcpl_string`
/// would store `ü` as two characters and the name would come back wrong on a
/// real Amiga. [`check_name`] has already proved every character fits in a
/// byte, so the conversion here cannot lose anything.
fn put_name(header: &mut [u8], checked: &str) -> CoreResult<()> {
    let bytes = latin1(checked);
    if NAME_OFFSET + 1 + bytes.len() > header.len() {
        return Err(CoreError::InvalidInput("that name does not fit".into()));
    }
    header[NAME_OFFSET..NAME_OFFSET + NAME_FIELD_LEN].fill(0);
    header[NAME_OFFSET] = bytes.len() as u8;
    header[NAME_OFFSET + 1..NAME_OFFSET + 1 + bytes.len()].copy_from_slice(&bytes);
    Ok(())
}

/// Write a name into a header block already in the set, and re-checksum it.
///
/// The file writer builds its header itself but does not own the Latin-1
/// encoding, so it comes back here for the name.
pub fn write_header_name(set: &mut BlockSet, block: u32, checked: &str) -> CoreResult<()> {
    {
        let Some((_, header)) = set.iter_mut().find(|(n, _)| **n == block) else {
            return Err(CoreError::InvalidInput(format!(
                "block {block} is not in this operation"
            )));
        };
        put_name(header, checked)?;
    }
    set.checksum(block, CHECKSUM_OFFSET)
}

/// Change the name stored in a header block.
///
/// The caller unlinks first and links afterwards: a rename moves the entry
/// between buckets whenever the new name hashes differently, and doing it in
/// place would leave it in a bucket its name no longer belongs to — findable
/// by a full listing and invisible to a lookup.
pub fn set_name<D: BlockDevice + ?Sized>(
    device: &D,
    set: &mut BlockSet,
    block: u32,
    name: &str,
) -> CoreResult<()> {
    let checked = check_name(name)?;
    put_name(set.edit(device, block)?, &checked)?;
    set.checksum(block, CHECKSUM_OFFSET)?;
    Ok(())
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::volume::device::VecDevice;
    use crate::core::volume::fixture::ffs_volume;
    use crate::core::volume::DosType;

    fn volume(dos: [u8; 4]) -> (VecDevice, VolumeGeometry) {
        let (bytes, geometry) = ffs_volume(1760, DosType::new(dos));
        (VecDevice::new(bytes, 512).unwrap(), geometry)
    }

    #[test]
    fn a_name_that_fits_is_accepted_unchanged() {
        assert_eq!(check_name("Readme").unwrap(), "Readme");
        assert_eq!(check_name("  Spaces.txt  ").unwrap(), "Spaces.txt");
        assert_eq!(check_name(&"a".repeat(30)).unwrap().chars().count(), 30);
    }

    /// The limit is thirty *characters*. Counting UTF-8 bytes instead would
    /// refuse a legal accented name — and get progressively more wrong the
    /// more accents it has.
    #[test]
    fn the_length_limit_counts_characters_not_utf_8_bytes() {
        let thirty_accented = "ü".repeat(30);
        assert_eq!(thirty_accented.len(), 60, "sixty bytes in UTF-8");
        assert!(
            check_name(&thirty_accented).is_ok(),
            "thirty Latin-1 characters fit in the thirty-byte field"
        );
        assert!(check_name(&"ü".repeat(31)).is_err());
    }

    /// Truncating would hand the user a file they cannot find by the name
    /// they typed. Refusing lets the caller offer a rename.
    #[test]
    fn a_name_too_long_for_amigados_is_refused_not_truncated() {
        let err = check_name(&"a".repeat(31)).unwrap_err();
        assert!(err.to_string().contains("stop at 30"), "{err}");
    }

    #[test]
    fn the_path_separators_are_refused() {
        assert!(check_name("DH0:Work").is_err());
        assert!(check_name("some/file").is_err());
    }

    #[test]
    fn a_character_outside_latin_1_is_refused() {
        // Latin-1 characters are fine.
        assert!(check_name("Grüße").is_ok());
        // This one has no byte to be stored as.
        let err = check_name("日本語").unwrap_err();
        assert!(err.to_string().contains("Latin-1"), "{err}");
    }

    #[test]
    fn an_empty_name_is_refused() {
        assert!(check_name("").is_err());
        assert!(check_name("   ").is_err());
    }

    /// The ART-010 lesson: international folding comes from the volume's own
    /// dostype. A global setting would hash names into buckets AmigaDOS would
    /// not look in.
    #[test]
    fn the_bucket_follows_the_volumes_own_dostype() {
        let (_, plain) = volume(*b"DOS\x01");
        let (_, intl) = volume(*b"DOS\x03");

        assert!(!plain.dos_type.is_international());
        assert!(intl.dos_type.is_international());

        // A name with a Latin-1 character above 'z' folds differently in the
        // two modes; ASCII names agree.
        assert_eq!(
            bucket_for("Readme", &plain).unwrap(),
            bucket_for("Readme", &intl).unwrap()
        );
        assert_eq!(
            bucket_for("Ärger", &plain).unwrap() == bucket_for("Ärger", &intl).unwrap(),
            name_hash("Ärger", false) == name_hash("Ärger", true)
        );
    }

    #[test]
    fn every_bucket_is_inside_the_hash_table() {
        let (_, geometry) = volume(*b"DOS\x01");
        for name in ["a", "Z", "Readme", "VeryLongNameIndeed", "x.info"] {
            let bucket = bucket_for(name, &geometry).unwrap();
            assert!(bucket < hash_table_size(512), "{name} hashed to {bucket}");
        }
    }

    #[test]
    fn a_linked_entry_shows_up_in_the_listing() {
        let (device, geometry) = volume(*b"DOS\x01");
        let mut set = BlockSet::new(512);

        write_dir_header(&mut set, 900, geometry.root_block, "Tools").unwrap();
        link_into(
            &device,
            &mut set,
            &geometry,
            geometry.root_block,
            900,
            "Tools",
        )
        .unwrap();

        let entries = entries_in(&device, &set, &geometry, geometry.root_block).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "Tools");
        assert!(entries[0].is_dir);
        assert_eq!(entries[0].block, 900);
    }

    #[test]
    fn several_entries_in_one_bucket_all_stay_findable() {
        let (device, geometry) = volume(*b"DOS\x01");
        let mut set = BlockSet::new(512);

        // Build enough entries that at least one bucket collides.
        let names: Vec<String> = (0..40).map(|i| format!("File{i:02}")).collect();
        for (index, name) in names.iter().enumerate() {
            let block = 900 + index as u32;
            write_dir_header(&mut set, block, geometry.root_block, name).unwrap();
            link_into(
                &device,
                &mut set,
                &geometry,
                geometry.root_block,
                block,
                name,
            )
            .unwrap();
        }

        let entries = entries_in(&device, &set, &geometry, geometry.root_block).unwrap();
        assert_eq!(entries.len(), names.len());
        for name in &names {
            assert!(
                find_entry(&device, &set, &geometry, geometry.root_block, name)
                    .unwrap()
                    .is_some(),
                "{name} went missing"
            );
        }
    }

    #[test]
    fn unlinking_a_bucket_head_leaves_the_rest_of_the_chain() {
        let (device, geometry) = volume(*b"DOS\x01");
        let mut set = BlockSet::new(512);

        for (index, name) in ["Alpha", "Beta", "Gamma"].iter().enumerate() {
            let block = 900 + index as u32;
            write_dir_header(&mut set, block, geometry.root_block, name).unwrap();
            link_into(
                &device,
                &mut set,
                &geometry,
                geometry.root_block,
                block,
                name,
            )
            .unwrap();
        }

        // Gamma went in last, so it is a head.
        unlink_from(
            &device,
            &mut set,
            &geometry,
            geometry.root_block,
            902,
            "Gamma",
        )
        .unwrap();

        let entries = entries_in(&device, &set, &geometry, geometry.root_block).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(!names.contains(&"Gamma"));
        assert!(names.contains(&"Alpha"));
        assert!(names.contains(&"Beta"));
    }

    /// The case a naive unlink gets wrong: an entry in the middle of a chain
    /// needs the entry *before* it rewritten, not the directory block.
    #[test]
    fn unlinking_from_the_middle_of_a_chain_repairs_the_link() {
        let (device, geometry) = volume(*b"DOS\x01");
        let mut set = BlockSet::new(512);

        // Force a collision: same bucket, three entries.
        let colliding = colliding_names(&geometry, 3);
        for (index, name) in colliding.iter().enumerate() {
            let block = 900 + index as u32;
            write_dir_header(&mut set, block, geometry.root_block, name).unwrap();
            link_into(
                &device,
                &mut set,
                &geometry,
                geometry.root_block,
                block,
                name,
            )
            .unwrap();
        }

        // The middle of the chain: inserted second, so second from the head.
        unlink_from(
            &device,
            &mut set,
            &geometry,
            geometry.root_block,
            901,
            &colliding[1],
        )
        .unwrap();

        let entries = entries_in(&device, &set, &geometry, geometry.root_block).unwrap();
        assert_eq!(entries.len(), 2, "only the unlinked entry should be gone");
        assert!(entries.iter().all(|e| e.block != 901));
        // And the survivors are still reachable by name, so the chain is intact.
        assert!(
            find_entry(&device, &set, &geometry, geometry.root_block, &colliding[0])
                .unwrap()
                .is_some()
        );
        assert!(
            find_entry(&device, &set, &geometry, geometry.root_block, &colliding[2])
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn unlink_touches_names_the_predecessor_not_the_directory() {
        let (device, geometry) = volume(*b"DOS\x01");
        let mut set = BlockSet::new(512);

        let colliding = colliding_names(&geometry, 2);
        for (index, name) in colliding.iter().enumerate() {
            let block = 900 + index as u32;
            write_dir_header(&mut set, block, geometry.root_block, name).unwrap();
            link_into(
                &device,
                &mut set,
                &geometry,
                geometry.root_block,
                block,
                name,
            )
            .unwrap();
        }

        // Block 900 went in first, so 901 points at it.
        let touched = unlink_touches(
            &device,
            &set,
            &geometry,
            geometry.root_block,
            900,
            &colliding[0],
        )
        .unwrap();
        assert_eq!(touched, vec![901, 900]);

        // The head, by contrast, needs the directory block.
        let touched = unlink_touches(
            &device,
            &set,
            &geometry,
            geometry.root_block,
            901,
            &colliding[1],
        )
        .unwrap();
        assert_eq!(touched, vec![geometry.root_block, 901]);
    }

    /// AmigaDOS is case-insensitive, so `README` and `Readme` are the same
    /// entry; creating the second would silently shadow the first (ART-012).
    #[test]
    fn a_name_that_differs_only_in_case_is_already_taken() {
        let (device, geometry) = volume(*b"DOS\x01");
        let mut set = BlockSet::new(512);

        write_dir_header(&mut set, 900, geometry.root_block, "Readme").unwrap();
        link_into(
            &device,
            &mut set,
            &geometry,
            geometry.root_block,
            900,
            "Readme",
        )
        .unwrap();

        assert!(ensure_available(&device, &set, &geometry, geometry.root_block, "README").is_err());
        assert!(ensure_available(&device, &set, &geometry, geometry.root_block, "Other").is_ok());
    }

    /// Case is preserved as typed, even though lookups ignore it (§7.3).
    #[test]
    fn the_case_the_user_typed_is_what_is_stored() {
        let (device, geometry) = volume(*b"DOS\x01");
        let mut set = BlockSet::new(512);

        write_dir_header(&mut set, 900, geometry.root_block, "MyStuff").unwrap();
        link_into(
            &device,
            &mut set,
            &geometry,
            geometry.root_block,
            900,
            "MyStuff",
        )
        .unwrap();

        let found = find_entry(&device, &set, &geometry, geometry.root_block, "mystuff")
            .unwrap()
            .unwrap();
        assert_eq!(found.name, "MyStuff");
    }

    /// A corrupt image can point an entry at itself. Without a bound this is
    /// an infinite loop inside an operation the user cannot cancel (ART-008).
    #[test]
    fn a_hash_chain_that_loops_is_an_error_not_a_hang() {
        let (device, geometry) = volume(*b"DOS\x01");
        let mut set = BlockSet::new(512);

        write_dir_header(&mut set, 900, geometry.root_block, "Loop").unwrap();
        link_into(
            &device,
            &mut set,
            &geometry,
            geometry.root_block,
            900,
            "Loop",
        )
        .unwrap();

        // Point the entry at itself.
        let entry = set.edit(&device, 900).unwrap();
        set_u32(entry, NEXT_HASH_OFFSET, 900).unwrap();

        let err = entries_in(&device, &set, &geometry, geometry.root_block).unwrap_err();
        assert!(err.to_string().contains("does not end"), "{err}");
    }

    #[test]
    fn an_entry_pointing_outside_the_volume_is_an_error() {
        let (device, geometry) = volume(*b"DOS\x01");
        let mut set = BlockSet::new(512);

        let dir = set.edit(&device, geometry.root_block).unwrap();
        set_u32(dir, TABLE_OFFSET, 99_999).unwrap();

        let err = entries_in(&device, &set, &geometry, geometry.root_block).unwrap_err();
        assert!(err.to_string().contains("outside the volume"), "{err}");
    }

    /// Find `count` names that all hash to the same bucket, so the chain
    /// tests exercise a real collision rather than hoping for one.
    fn colliding_names(geometry: &VolumeGeometry, count: usize) -> Vec<String> {
        let mut by_bucket: std::collections::HashMap<usize, Vec<String>> =
            std::collections::HashMap::new();

        for i in 0..5000 {
            let name = format!("N{i}");
            let bucket = bucket_for(&name, geometry).unwrap();
            let slot = by_bucket.entry(bucket).or_default();
            slot.push(name);
            if slot.len() == count {
                return slot.clone();
            }
        }
        panic!("no bucket collected {count} names");
    }
}
