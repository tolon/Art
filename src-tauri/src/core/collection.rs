//! Amiga Game & Software Collection Organizer Engine (Phase 8).
//!
//! Provides TOSEC naming standard metadata extraction, multi-disk grouping,
//! WHDLoad slave title extraction, chipset requirement inference (AGA vs OCS/ECS),
//! and batch collection indexing.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::core::error::{CoreError, CoreResult};
// `ChipsetRequirement` lives in `core/gameindex` now: the index needs it and a
// second copy of a two-variant enum is a type to keep in step.
use crate::core::gameindex::record::ChipsetRequirement;
use crate::core::jobs::ProgressSink;

/// Type of retro media.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    /// Amiga Floppy Disk (ADF / ADZ)
    Adf,
    /// WHDLoad Pre-installed Archive (LHA)
    LhaWhdload,
    /// Hard Disk File / Partition (HDF)
    Hdf,
}

/// A cataloged game or software title.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionItem {
    pub id: String,
    pub title: String,
    pub clean_title: String,
    pub year: Option<u16>,
    pub publisher: Option<String>,
    pub chipset: ChipsetRequirement,
    pub media_kind: MediaKind,
    pub primary_path: String,
    pub disks: Vec<String>,
    pub disk_count: usize,
}

/// Which disk of a multi-disk set an image is: `(index, total)`.
pub type DiskPosition = (usize, usize);

/// One file found during a collection scan, before titles are grouped:
/// `(path, clean title, year, publisher, chipset, media kind)`.
type ScannedFile = (
    String,
    String,
    Option<u16>,
    Option<String>,
    ChipsetRequirement,
    MediaKind,
);

/// Metadata recovered from a TOSEC-style filename.
pub type TosecMetadata = (
    /* clean title */ String,
    /* year */ Option<u16>,
    /* publisher */ Option<String>,
    ChipsetRequirement,
    Option<DiskPosition>,
);

/// Parse TOSEC formatted filename metadata.
///
/// Example: `Sensible World of Soccer 96-97 (1996)(Renegade)(AGA)(Disk 1 of 2)[!]`
pub fn parse_tosec_metadata(filename: &str) -> TosecMetadata {
    let name_without_ext = if let Some(dot_idx) = filename.rfind('.') {
        &filename[..dot_idx]
    } else {
        filename
    };

    let mut clean_title;
    let mut year = None;
    let mut publisher = None;
    let mut chipset = ChipsetRequirement::OcsEcs;
    let mut disk_info = None;

    // Check for AGA indicators in filename
    let upper = filename.to_uppercase();
    if upper.contains("AGA")
        || upper.contains("CD32")
        || upper.contains("A1200")
        || upper.contains("68020")
    {
        chipset = ChipsetRequirement::Aga;
    }

    // Extract parentheses tokens: (1996), (Renegade), (Disk 1 of 2), etc.
    let mut tokens = Vec::new();
    let mut base_name = String::new();
    let mut in_paren = false;
    let mut in_bracket = false;
    let mut cur_token = String::new();

    for c in name_without_ext.chars() {
        if c == '(' {
            in_paren = true;
            cur_token.clear();
        } else if c == ')' {
            in_paren = false;
            tokens.push(cur_token.trim().to_string());
            cur_token.clear();
        } else if c == '[' {
            in_bracket = true;
        } else if c == ']' {
            in_bracket = false;
        } else if in_paren {
            cur_token.push(c);
        } else if !in_bracket && !c.is_control() {
            base_name.push(c);
        }
    }

    clean_title = base_name
        .trim()
        .trim_end_matches('_')
        .trim_end_matches('-')
        .trim()
        .to_string();
    if clean_title.is_empty() {
        clean_title = name_without_ext.to_string();
    }

    for t in tokens {
        let t_upper = t.to_uppercase();

        // 1. Year pattern (e.g. "1991", "1996")
        if t.len() == 4 && t.chars().all(|ch| ch.is_ascii_digit()) {
            if let Ok(y) = t.parse::<u16>() {
                if (1980..=2030).contains(&y) {
                    year = Some(y);
                    continue;
                }
            }
        }

        // 2. Disk pattern (e.g. "Disk 1 of 2", "Disk 1", "Disk A")
        if t_upper.contains("DISK") {
            let parts: Vec<&str> = t_upper.split_whitespace().collect();
            if let Some(pos) = parts.iter().position(|&x| x == "DISK") {
                if pos + 1 < parts.len() {
                    let d_num = parts[pos + 1].parse::<usize>().unwrap_or(1);
                    let mut total = d_num;
                    if let Some(of_pos) = parts.iter().position(|&x| x == "OF") {
                        if of_pos + 1 < parts.len() {
                            total = parts[of_pos + 1].parse::<usize>().unwrap_or(d_num);
                        }
                    }
                    disk_info = Some((d_num, total));
                    continue;
                }
            }
        }

        // 3. Publisher
        if publisher.is_none()
            && !t_upper.contains("AGA")
            && !t_upper.contains("PAL")
            && !t_upper.contains("NTSC")
            && !t_upper.contains("CRACK")
        {
            publisher = Some(t);
        }
    }

    (clean_title, year, publisher, chipset, disk_info)
}

/// Recursively scan a folder for Amiga software collection files.
///
/// Convenience wrapper for callers with nothing to report to.
pub fn scan_collection_directory(dir: &Path) -> CoreResult<Vec<CollectionItem>> {
    scan_collection_directory_with(dir, &crate::core::jobs::NoProgress)
}

/// Scan a folder, reporting progress and honouring cancellation.
///
/// A collection can hold tens of thousands of files, which is exactly the case
/// spec §54/§55 has in mind: the work belongs on a background job and the user
/// must be able to stop it.
pub fn scan_collection_directory_with(
    dir: &Path,
    progress: &dyn ProgressSink,
) -> CoreResult<Vec<CollectionItem>> {
    if !dir.is_dir() {
        return Err(CoreError::InvalidInput(format!(
            "Directory not found at '{}'",
            dir.display()
        )));
    }

    // Walking the tree comes first, and its size is unknown until it finishes —
    // report an indefinite phase rather than inventing a percentage.
    progress.report(0, None, "Looking for Amiga files…");
    let mut found_files = Vec::new();
    collect_files_recursive(dir, &mut found_files);

    if progress.is_cancelled() {
        return Err(crate::core::jobs::cancelled_error());
    }

    let total = found_files.len() as u64;

    // Group multi-disk titles by clean title and directory
    let mut groups: HashMap<String, Vec<ScannedFile>> = HashMap::new();

    for (index, file_path) in found_files.into_iter().enumerate() {
        // Between files is a safe place to stop: nothing has been written.
        if progress.is_cancelled() {
            return Err(crate::core::jobs::cancelled_error());
        }
        let short = Path::new(&file_path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        progress.report(index as u64 + 1, Some(total), &short);

        let p = Path::new(&file_path);
        let filename = p
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let ext = p
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        let media_kind = match ext.as_str() {
            "adf" | "adz" => MediaKind::Adf,
            "lha" | "lzh" => MediaKind::LhaWhdload,
            "hdf" | "img" => MediaKind::Hdf,
            _ => continue,
        };

        let (clean_title, year, publisher, chipset, _disk_info) = parse_tosec_metadata(&filename);

        let group_key = format!(
            "{}:{}:{:?}",
            p.parent().unwrap_or(Path::new("")).display(),
            clean_title.to_lowercase(),
            media_kind
        );

        groups.entry(group_key).or_default().push((
            file_path,
            clean_title,
            year,
            publisher,
            chipset,
            media_kind,
        ));
    }

    let mut items = Vec::new();

    for (_key, mut list) in groups {
        list.sort_by(|a, b| a.0.cmp(&b.0)); // Sort disk paths alphabetically
        let primary_path = list[0].0.clone();
        let title = list[0].1.clone();
        let year = list[0].2;
        let publisher = list[0].3.clone();
        let chipset = list
            .iter()
            .map(|item| item.4)
            .find(|&c| c == ChipsetRequirement::Aga)
            .unwrap_or(ChipsetRequirement::OcsEcs);
        let media_kind = list[0].5;
        let disks: Vec<String> = list.into_iter().map(|item| item.0).collect();
        let disk_count = disks.len();

        let id = format!("{:x}", md5_hash(&primary_path));

        items.push(CollectionItem {
            id,
            title: title.clone(),
            clean_title: title,
            year,
            publisher,
            chipset,
            media_kind,
            primary_path,
            disks,
            disk_count,
        });
    }

    items.sort_by_key(|a| a.clean_title.to_lowercase());
    Ok(items)
}

/// How deep a collection scan will descend.
///
/// Windows junctions and symlinks can form a cycle, and unbounded recursion on
/// one overflows the stack — which, with `panic = "abort"`, takes the whole
/// application down rather than reporting an error.
const MAX_SCAN_DEPTH: usize = 32;

fn collect_files_recursive(dir: &Path, acc: &mut Vec<String>) {
    collect_files_at_depth(dir, acc, 0);
}

fn collect_files_at_depth(dir: &Path, acc: &mut Vec<String>, depth: usize) {
    if depth >= MAX_SCAN_DEPTH {
        return;
    }

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            // `symlink_metadata` does not follow links, so a directory symlink
            // pointing back up the tree is skipped instead of followed.
            let is_symlink = std::fs::symlink_metadata(&p)
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false);
            if is_symlink {
                continue;
            }

            if p.is_dir() {
                collect_files_at_depth(&p, acc, depth + 1);
            } else if p.is_file() {
                if let Some(ext) = p.extension() {
                    let ext_l = ext.to_string_lossy().to_lowercase();
                    if ext_l == "adf"
                        || ext_l == "adz"
                        || ext_l == "lha"
                        || ext_l == "lzh"
                        || ext_l == "hdf"
                    {
                        acc.push(p.to_string_lossy().to_string());
                    }
                }
            }
        }
    }
}

fn md5_hash(s: &str) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for b in s.bytes() {
        h = (h ^ (b as u64)).wrapping_mul(0x100000001b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tosec_filename() {
        let fn1 = "Monkey Island 2 - LeChuck's Revenge (1992)(LucasArts)(Disk 1 of 11)[!].adf";
        let (title, year, publ, chipset, disk) = parse_tosec_metadata(fn1);
        assert_eq!(title, "Monkey Island 2 - LeChuck's Revenge");
        assert_eq!(year, Some(1992));
        assert_eq!(publ, Some("LucasArts".into()));
        assert_eq!(chipset, ChipsetRequirement::OcsEcs);
        assert_eq!(disk, Some((1, 11)));
    }

    #[test]
    fn parse_aga_game_filename() {
        let fn2 = "Alien Breed 3D (1995)(Ocean)(AGA)(Disk 1 of 3).adf";
        let (title, year, publ, chipset, disk) = parse_tosec_metadata(fn2);
        assert_eq!(title, "Alien Breed 3D");
        assert_eq!(year, Some(1995));
        assert_eq!(publ, Some("Ocean".into()));
        assert_eq!(chipset, ChipsetRequirement::Aga);
        assert_eq!(disk, Some((1, 3)));
    }

    /// A directory tree deeper than the scan limit must stop cleanly rather
    /// than recursing until the stack overflows (which aborts the process).
    #[test]
    fn scanning_stops_at_the_depth_limit() {
        let root = std::env::temp_dir().join(format!(
            "art-scan-deep-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        // Build a tree twice as deep as the limit, with an ADF at the bottom.
        let mut deep = root.clone();
        for i in 0..(MAX_SCAN_DEPTH * 2) {
            deep = deep.join(format!("d{i}"));
        }
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("buried.adf"), b"x").unwrap();

        let mut found = Vec::new();
        collect_files_recursive(&root, &mut found);

        // The point is that it returned at all; the buried file is out of reach.
        assert!(found.is_empty(), "found {found:?}");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn scanning_finds_images_within_the_limit() {
        let root = std::env::temp_dir().join(format!(
            "art-scan-ok-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let nested = root.join("Games").join("Puzzle");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("Lemmings.adf"), b"x").unwrap();
        std::fs::write(root.join("notes.txt"), b"x").unwrap();

        let mut found = Vec::new();
        collect_files_recursive(&root, &mut found);

        assert_eq!(found.len(), 1, "got {found:?}");
        assert!(found[0].ends_with("Lemmings.adf"));

        std::fs::remove_dir_all(&root).ok();
    }
}
