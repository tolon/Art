//! Pre-flight: what a copy will cost, and everything about it that will not
//! work — all of it decided before a single block is written (brief §3.3, §4.1).
//!
//! > *Explain before modify.*
//!
//! The failure this exists to prevent is the drip feed: a hundred-file copy
//! that stops on file 37 because the name was too long, then on 52 because the
//! disk filled, then on 61 because a name collided. Each stop leaves the user
//! guessing how much more is coming. One report up front, with real numbers,
//! and they can decide once.
//!
//! Nothing here writes. It reads the volume's free-space map and the list of
//! things the user asked to copy, and produces a description.

use std::collections::{BTreeMap, BTreeSet};

use crate::core::error::CoreResult;
use crate::core::volume::{BlockDevice, VolumeGeometry};

use super::bitmap::Allocator;
use super::dir::{check_name, MAX_NAME_LEN};
use super::file::budget_for;

/// The most entries ART will plan in one operation.
///
/// A copy larger than this is real but wants splitting: the report itself
/// stops being readable, and the block set for one journalled batch stops
/// being something that fits in memory.
pub const MAX_PLANNED_ENTRIES: usize = 20_000;

/// One thing the user asked to copy.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SourceEntry {
    /// Path relative to the root of the copy, with `/` separators.
    /// A directory's own entry has no trailing slash.
    pub relative: String,
    pub is_dir: bool,
    /// Zero for a directory.
    pub bytes: u64,
}

/// A name AmigaDOS will not take, and what to call it instead.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct NameProblem {
    pub relative: String,
    /// The name that failed — the last component of `relative`.
    pub name: String,
    /// Why, in the user's words.
    pub reason: String,
    /// What ART would call it. `None` when nothing sensible is left.
    pub suggestion: Option<String>,
}

/// An `.info` file separated from the object it belongs to (§7.1).
///
/// Copying a drawer without its `.info` produces a drawer that is invisible on
/// Workbench. Not an error — the user may well mean it — but never a silent
/// one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SplitIconPair {
    /// The object whose icon is missing, or the icon whose object is.
    pub relative: String,
    /// True when the `.info` is present and the object is not.
    pub icon_without_object: bool,
}

/// What a copy would cost and what would go wrong.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CopyPlan {
    pub files: usize,
    pub directories: usize,
    pub total_bytes: u64,
    /// Blocks needed: data, extension, header and directory blocks together.
    pub blocks_needed: usize,
    pub blocks_free: usize,
    pub block_size: usize,
    /// Names AmigaDOS cannot store, with suggestions.
    pub name_problems: Vec<NameProblem>,
    /// Names already present in the destination.
    pub collisions: Vec<String>,
    /// Icons being separated from what they belong to.
    pub split_icons: Vec<SplitIconPair>,
}

impl CopyPlan {
    /// Whether the volume has room.
    pub fn fits(&self) -> bool {
        self.blocks_needed <= self.blocks_free
    }

    pub fn bytes_needed(&self) -> u64 {
        self.blocks_needed as u64 * self.block_size as u64
    }

    pub fn bytes_free(&self) -> u64 {
        self.blocks_free as u64 * self.block_size as u64
    }

    /// Whether the copy can run as asked, with nothing for the user to decide.
    pub fn is_clean(&self) -> bool {
        self.fits() && self.name_problems.is_empty() && self.collisions.is_empty()
    }

    /// One sentence for the user when the copy will not fit.
    ///
    /// Blocks *and* bytes: blocks are the honest unit — a thousand one-byte
    /// files cost a thousand blocks — and bytes are the one people think in.
    pub fn shortfall(&self) -> Option<String> {
        (!self.fits()).then(|| {
            format!(
                "This needs {} blocks ({}) and {} are free ({}). \
                 {} more blocks than there is room for.",
                self.blocks_needed,
                human_bytes(self.bytes_needed()),
                self.blocks_free,
                human_bytes(self.bytes_free()),
                self.blocks_needed - self.blocks_free
            )
        })
    }
}

fn human_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} bytes")
    }
}

/// Work out what copying `entries` into `dest_dir` would cost.
///
/// `existing` is what is already in the destination directory, so collisions
/// are reported here rather than discovered mid-copy. Only the top level is
/// checked: entries deeper in the tree go into directories this copy creates,
/// which by definition start empty.
pub fn plan_copy<D: BlockDevice + ?Sized>(
    device: &D,
    geometry: &VolumeGeometry,
    entries: &[SourceEntry],
    existing: &[String],
) -> CoreResult<CopyPlan> {
    let capped: &[SourceEntry] = if entries.len() > MAX_PLANNED_ENTRIES {
        &entries[..MAX_PLANNED_ENTRIES]
    } else {
        entries
    };

    let allocator = Allocator::load(device, geometry)?;

    let mut files = 0usize;
    let mut directories = 0usize;
    let mut total_bytes = 0u64;
    let mut blocks_needed = 0usize;
    let mut name_problems = Vec::new();

    // Suggestions have to stay unique among themselves, or two files that were
    // distinct on Windows would collide on the Amiga.
    let mut taken: BTreeSet<String> = existing.iter().map(|n| n.to_lowercase()).collect();

    for entry in capped {
        if entry.is_dir {
            directories += 1;
            // A directory is one header block. Its hash table lives inside it.
            blocks_needed += 1;
        } else {
            files += 1;
            total_bytes += entry.bytes;
            blocks_needed += budget_for(entry.bytes, geometry)?.total();
        }

        let name = last_component(&entry.relative);
        if let Err(err) = check_name(name) {
            let suggestion = suggest_name(name, &mut taken);
            name_problems.push(NameProblem {
                relative: entry.relative.clone(),
                name: name.to_string(),
                reason: err.to_string(),
                suggestion,
            });
        } else {
            taken.insert(name.to_lowercase());
        }
    }

    // Collisions: only what lands directly in the destination directory.
    let present: BTreeSet<String> = existing.iter().map(|n| n.to_lowercase()).collect();
    let collisions: Vec<String> = capped
        .iter()
        .filter(|entry| !entry.relative.contains('/'))
        .map(|entry| last_component(&entry.relative).to_string())
        .filter(|name| present.contains(&name.to_lowercase()))
        .collect();

    Ok(CopyPlan {
        files,
        directories,
        total_bytes,
        blocks_needed,
        blocks_free: allocator.free_count(),
        block_size: geometry.block_size,
        name_problems,
        collisions,
        split_icons: find_split_icons(capped),
    })
}

fn last_component(relative: &str) -> &str {
    match relative.rfind('/') {
        Some(slash) => &relative[slash + 1..],
        None => relative,
    }
}

/// Which `.info` files in this copy have lost their partner (§7.1).
///
/// Workbench shows an object only if its `.info` is next to it, so a drawer
/// copied without one is invisible on the Amiga even though the files are all
/// there. The reverse — an `.info` with nothing to describe — is harmless but
/// worth saying, because it usually means the copy selection was wrong.
fn find_split_icons(entries: &[SourceEntry]) -> Vec<SplitIconPair> {
    let all: BTreeSet<String> = entries.iter().map(|e| e.relative.to_lowercase()).collect();
    let mut out = Vec::new();

    for entry in entries {
        let lower = entry.relative.to_lowercase();

        if let Some(object) = lower.strip_suffix(".info") {
            if !all.contains(object) {
                out.push(SplitIconPair {
                    relative: entry.relative.clone(),
                    icon_without_object: true,
                });
            }
        } else if !all.contains(&format!("{lower}.info")) {
            // Only worth mentioning for directories: a plain file without an
            // icon is completely normal, while a drawer without one is
            // invisible on Workbench.
            if entry.is_dir {
                out.push(SplitIconPair {
                    relative: entry.relative.clone(),
                    icon_without_object: false,
                });
            }
        }
    }

    out
}

/// A name AmigaDOS will take, as close to the original as possible.
///
/// The extension is kept when there is one: `My Very Long Document.txt` losing
/// its `.txt` is a bigger change than losing letters from the middle of the
/// stem, because the extension is what says what the file is.
///
/// `None` when nothing usable is left — a name made entirely of characters
/// AmigaDOS cannot store.
pub fn suggest_name(name: &str, taken: &mut BTreeSet<String>) -> Option<String> {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            ':' | '/' => '_',
            c if (c as u32) < 0x20 || (c as u32) == 0x7F => '_',
            c if (c as u32) > 0xFF => '_',
            c => c,
        })
        .collect();
    let cleaned = cleaned.trim().to_string();

    if cleaned.chars().all(|c| c == '_') || cleaned.is_empty() {
        return None;
    }

    let (stem, extension) = match cleaned.rfind('.') {
        // A leading dot is part of the name, not an extension.
        Some(dot) if dot > 0 && cleaned.len() - dot <= 5 => (&cleaned[..dot], &cleaned[dot..]),
        _ => (cleaned.as_str(), ""),
    };

    let mut candidate = shorten(stem, extension, MAX_NAME_LEN);

    // Uniqueness. `~1`, `~2`, … keeps the shape recognisable and does not
    // collide with the extension.
    let mut counter = 1u32;
    while taken.contains(&candidate.to_lowercase()) {
        let suffix = format!("~{counter}");
        let room = MAX_NAME_LEN.saturating_sub(extension.chars().count() + suffix.len());
        // By characters, again: byte-slicing a Latin-1 stem would cut one in
        // half and panic.
        let kept: String = stem.chars().take(room).collect();
        candidate = format!("{kept}{suffix}{extension}");
        counter += 1;
        if counter > 9999 {
            return None;
        }
    }

    // The suggestion has to survive the check it was made for.
    check_name(&candidate).ok()?;
    taken.insert(candidate.to_lowercase());
    Some(candidate)
}

fn shorten(stem: &str, extension: &str, limit: usize) -> String {
    let room = limit.saturating_sub(extension.chars().count());
    // Character boundaries, not byte ones: a name with Latin-1 characters is
    // multi-byte in Rust's UTF-8 and slicing it by bytes would panic.
    let kept: String = stem.chars().take(room).collect();
    format!("{kept}{extension}")
}

/// Group entries by the directory they go into, parents before children.
///
/// A copy has to create `Tools` before it can put `Tools/Editor` in it, and a
/// tree arriving in arbitrary order is the normal case — a file manager hands
/// over what the user selected, not a sorted list.
pub fn order_for_creation(entries: &[SourceEntry]) -> Vec<SourceEntry> {
    let mut by_depth: BTreeMap<(usize, bool, String), SourceEntry> = BTreeMap::new();

    for entry in entries {
        let depth = entry.relative.matches('/').count();
        // Directories before files at the same depth, so a file never arrives
        // before the directory it belongs in.
        by_depth.insert(
            (depth, !entry.is_dir, entry.relative.clone()),
            entry.clone(),
        );
    }

    by_depth.into_values().collect()
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

    fn file(relative: &str, bytes: u64) -> SourceEntry {
        SourceEntry {
            relative: relative.into(),
            is_dir: false,
            bytes,
        }
    }

    fn folder(relative: &str) -> SourceEntry {
        SourceEntry {
            relative: relative.into(),
            is_dir: true,
            bytes: 0,
        }
    }

    #[test]
    fn a_plan_counts_files_directories_and_blocks() {
        let (device, geometry) = volume(*b"DOS\x01");
        let entries = vec![
            folder("Tools"),
            file("Tools/Editor", 1024),
            file("Readme", 100),
        ];

        let plan = plan_copy(&device, &geometry, &entries, &[]).unwrap();
        assert_eq!(plan.files, 2);
        assert_eq!(plan.directories, 1);
        assert_eq!(plan.total_bytes, 1124);
        // 1 dir block + (1 header + 2 data) + (1 header + 1 data) = 6
        assert_eq!(plan.blocks_needed, 6);
        assert!(plan.fits());
        assert!(plan.is_clean());
    }

    /// The 488-vs-512 difference is not a rounding detail: it changes whether
    /// a copy fits.
    #[test]
    fn an_ofs_destination_needs_more_blocks_than_an_ffs_one() {
        let (ffs_device, ffs) = volume(*b"DOS\x01");
        let (ofs_device, ofs) = volume(*b"DOS\x00");
        let entries = vec![file("Big.bin", 100_000)];

        let on_ffs = plan_copy(&ffs_device, &ffs, &entries, &[]).unwrap();
        let on_ofs = plan_copy(&ofs_device, &ofs, &entries, &[]).unwrap();

        assert!(
            on_ofs.blocks_needed > on_ffs.blocks_needed,
            "OFS spends 24 bytes of every block on its own header"
        );
    }

    #[test]
    fn a_copy_that_does_not_fit_says_so_with_real_numbers() {
        let (device, geometry) = volume(*b"DOS\x01");
        let entries = vec![file("Huge.bin", 5 * 1024 * 1024)];

        let plan = plan_copy(&device, &geometry, &entries, &[]).unwrap();
        assert!(!plan.fits());
        assert!(!plan.is_clean());

        let message = plan.shortfall().unwrap();
        assert!(message.contains("blocks"), "{message}");
        assert!(message.contains("are free"), "{message}");
        assert!(message.contains("MB"), "{message}");
    }

    #[test]
    fn every_name_problem_is_reported_at_once_not_one_per_attempt() {
        let (device, geometry) = volume(*b"DOS\x01");
        let entries = vec![
            file(&"a".repeat(40), 10),
            file("with/slash", 10),
            file("Fine.txt", 10),
            file("日本語.txt", 10),
        ];

        let plan = plan_copy(&device, &geometry, &entries, &[]).unwrap();
        // "with/slash" has a valid last component, so three entries are fine
        // and only two names are actually unstorable.
        assert_eq!(plan.name_problems.len(), 2, "{:?}", plan.name_problems);
        assert!(plan.name_problems.iter().all(|p| p.suggestion.is_some()));
        assert!(!plan.is_clean());
    }

    #[test]
    fn a_long_name_is_shortened_but_keeps_its_extension() {
        let mut taken = BTreeSet::new();
        let suggestion = suggest_name(&format!("{}.txt", "a".repeat(50)), &mut taken).unwrap();

        assert!(suggestion.len() <= MAX_NAME_LEN);
        assert!(
            suggestion.ends_with(".txt"),
            "the extension says what the file is: {suggestion}"
        );
        assert!(check_name(&suggestion).is_ok());
    }

    #[test]
    fn illegal_characters_become_underscores() {
        let mut taken = BTreeSet::new();
        let suggestion = suggest_name("DH0:my/file.txt", &mut taken).unwrap();
        assert_eq!(suggestion, "DH0_my_file.txt");
        assert!(check_name(&suggestion).is_ok());
    }

    /// Two files that were distinct on Windows must stay distinct on the
    /// Amiga, or the copy silently loses one.
    #[test]
    fn suggestions_do_not_collide_with_each_other() {
        let mut taken = BTreeSet::new();
        let long = "a".repeat(40);

        let first = suggest_name(&format!("{long}1.txt"), &mut taken).unwrap();
        let second = suggest_name(&format!("{long}2.txt"), &mut taken).unwrap();

        assert_ne!(first.to_lowercase(), second.to_lowercase());
        assert!(check_name(&second).is_ok());
        assert!(second.len() <= MAX_NAME_LEN);
    }

    #[test]
    fn a_suggestion_does_not_collide_with_what_is_already_there() {
        let mut taken: BTreeSet<String> = ["readme.txt".into()].into_iter().collect();
        let suggestion = suggest_name("Readme.txt\u{7}", &mut taken).unwrap();
        assert_ne!(suggestion.to_lowercase(), "readme.txt");
    }

    #[test]
    fn a_name_with_nothing_storable_left_gets_no_suggestion() {
        let mut taken = BTreeSet::new();
        assert!(suggest_name("日本語", &mut taken).is_none());
        assert!(suggest_name("", &mut taken).is_none());
    }

    /// Latin-1 names are multi-byte in Rust's UTF-8; shortening by bytes
    /// would slice one in half and panic.
    #[test]
    fn shortening_a_latin_1_name_does_not_split_a_character() {
        let mut taken = BTreeSet::new();
        let suggestion = suggest_name(&format!("{}.txt", "ü".repeat(40)), &mut taken).unwrap();
        assert!(check_name(&suggestion).is_ok());
        assert!(suggestion.chars().count() <= MAX_NAME_LEN);
    }

    #[test]
    fn a_name_already_in_the_destination_is_reported_as_a_collision() {
        let (device, geometry) = volume(*b"DOS\x01");
        let entries = vec![file("Readme", 10), file("Other", 10)];

        let plan = plan_copy(&device, &geometry, &entries, &["README".into()]).unwrap();
        assert_eq!(plan.collisions, vec!["Readme"], "collisions ignore case");
        assert!(!plan.is_clean());
    }

    /// Only the top level: everything deeper goes into directories this copy
    /// creates, which start empty.
    #[test]
    fn a_nested_name_matching_something_at_the_top_is_not_a_collision() {
        let (device, geometry) = volume(*b"DOS\x01");
        let entries = vec![folder("Tools"), file("Tools/Readme", 10)];

        let plan = plan_copy(&device, &geometry, &entries, &["Readme".into()]).unwrap();
        assert!(plan.collisions.is_empty());
    }

    /// §7.1: a drawer copied without its `.info` is invisible on Workbench.
    #[test]
    fn a_drawer_without_its_icon_is_flagged() {
        let (device, geometry) = volume(*b"DOS\x01");
        let entries = vec![folder("Game"), file("Game/data.bin", 10)];

        let plan = plan_copy(&device, &geometry, &entries, &[]).unwrap();
        assert_eq!(plan.split_icons.len(), 1);
        assert_eq!(plan.split_icons[0].relative, "Game");
        assert!(!plan.split_icons[0].icon_without_object);
    }

    #[test]
    fn a_drawer_with_its_icon_is_not_flagged() {
        let (device, geometry) = volume(*b"DOS\x01");
        let entries = vec![folder("Game"), file("Game.info", 400)];

        let plan = plan_copy(&device, &geometry, &entries, &[]).unwrap();
        assert!(plan.split_icons.is_empty(), "{:?}", plan.split_icons);
    }

    #[test]
    fn an_icon_with_nothing_to_describe_is_flagged_the_other_way() {
        let (device, geometry) = volume(*b"DOS\x01");
        let entries = vec![file("Orphan.info", 400)];

        let plan = plan_copy(&device, &geometry, &entries, &[]).unwrap();
        assert_eq!(plan.split_icons.len(), 1);
        assert!(plan.split_icons[0].icon_without_object);
    }

    /// A plain file with no icon is completely ordinary and must not produce
    /// a warning, or the report becomes noise nobody reads.
    #[test]
    fn a_plain_file_without_an_icon_is_not_flagged() {
        let (device, geometry) = volume(*b"DOS\x01");
        let entries = vec![file("Readme.txt", 10)];

        let plan = plan_copy(&device, &geometry, &entries, &[]).unwrap();
        assert!(plan.split_icons.is_empty());
    }

    /// A file cannot be written before the directory it goes in exists.
    #[test]
    fn creation_order_puts_every_directory_before_what_goes_in_it() {
        let entries = vec![
            file("Tools/Sub/Deep.txt", 10),
            folder("Tools/Sub"),
            file("Top.txt", 10),
            folder("Tools"),
        ];

        let ordered = order_for_creation(&entries);
        let position = |relative: &str| {
            ordered
                .iter()
                .position(|e| e.relative == relative)
                .unwrap_or_else(|| panic!("{relative} missing"))
        };

        assert!(position("Tools") < position("Tools/Sub"));
        assert!(position("Tools/Sub") < position("Tools/Sub/Deep.txt"));
        assert!(position("Tools") < position("Top.txt"), "directories first");
    }

    #[test]
    fn an_empty_copy_plans_to_nothing() {
        let (device, geometry) = volume(*b"DOS\x01");
        let plan = plan_copy(&device, &geometry, &[], &[]).unwrap();

        assert_eq!(plan.blocks_needed, 0);
        assert!(plan.fits());
        assert!(plan.is_clean());
        assert!(plan.shortfall().is_none());
    }

    /// A thousand one-byte files cost a thousand blocks. Reporting only bytes
    /// would say "1 KB" and be wrong about whether it fits.
    #[test]
    fn many_tiny_files_cost_a_block_each() {
        let (device, geometry) = volume(*b"DOS\x01");
        let entries: Vec<SourceEntry> = (0..100).map(|i| file(&format!("F{i}"), 1)).collect();

        let plan = plan_copy(&device, &geometry, &entries, &[]).unwrap();
        assert_eq!(plan.total_bytes, 100);
        // One header plus one data block each.
        assert_eq!(plan.blocks_needed, 200);
        assert!(plan.bytes_needed() > plan.total_bytes * 100);
    }
}
