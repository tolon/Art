//! WHDLoad package detection.
//!
//! A WHDLoad install typically contains a `.slave` file, a host executable,
//! one or more data directories, and a `.info` icon. We score these signals
//! and report a confidence level rather than a binary yes/no — the spec
//! demands we never present uncertain information as fact.

use serde::{Deserialize, Serialize};

use crate::core::lha::LhaEntry;
use crate::core::workflow::types::Confidence;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhdloadVerdict {
    pub confidence: Confidence,
    pub slave: Option<String>,
    pub executable: Option<String>,
    pub has_data_dir: bool,
    pub has_icon: bool,
    pub notes: String,
}

/// Analyse a list of archive entries for WHDLoad markers.
pub fn detect_whdload(entries: &[LhaEntry]) -> WhdloadVerdict {
    let files: Vec<&str> = entries
        .iter()
        .filter(|e| !e.is_dir)
        .map(|e| e.path.as_str())
        .collect();

    let dirs: Vec<&str> = entries
        .iter()
        .filter(|e| e.is_dir)
        .map(|e| e.path.as_str())
        .collect();

    let slave = files
        .iter()
        .find(|f| has_ext(f, "slave"))
        .map(|s| s.to_string());
    let icon = files.iter().any(|f| has_ext(f, "info"));
    // A "host executable" heuristic: a file with no extension in the root
    // whose name matches the archive base, or simply any extension-less file.
    let executable = files
        .iter()
        .find(|f| {
            let base = f.rsplit(['/', '\\']).next().unwrap_or(f);
            !base.contains('.') && !base.eq_ignore_ascii_case("Install")
        })
        .map(|s| s.to_string());

    let has_data_dir = dirs.iter().any(|d| {
        let lower = d.to_ascii_lowercase();
        lower.contains("data") || lower.contains("files")
    }) || files.iter().any(|f| f.contains('/') || f.contains('\\'));

    let score = [slave.is_some(), executable.is_some(), has_data_dir, icon]
        .iter()
        .filter(|&&b| b)
        .count();

    let (confidence, notes) = match score {
        4 | 3 => (
            Confidence::High,
            "Strong WHDLoad markers found (slave + executable + data + icon).".into(),
        ),
        2 => (
            Confidence::Medium,
            "Some WHDLoad markers found; not conclusive.".into(),
        ),
        1 => (
            Confidence::Low,
            "Weak WHDLoad signal; could be a generic archive.".into(),
        ),
        _ => (Confidence::Unknown, "No WHDLoad markers detected.".into()),
    };

    WhdloadVerdict {
        confidence,
        slave,
        executable,
        has_data_dir,
        has_icon: icon,
        notes,
    }
}

fn has_ext(name: &str, ext: &str) -> bool {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    base.rsplit_once('.')
        .map(|(_, e)| e.eq_ignore_ascii_case(ext))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, is_dir: bool) -> LhaEntry {
        LhaEntry {
            path: path.into(),
            is_dir,
            compressed_size: 100,
            uncompressed_size: 100,
            method: "-lh0-".into(),
            last_modified: 0,
            comment: String::new(),
        }
    }

    #[test]
    fn full_whdload_high_confidence() {
        let entries = vec![
            entry("Turrican/Turrican.slave", false),
            entry("Turrican/Turrican", false),
            entry("Turrican/Turrican.info", false),
            entry("Turrican/Data/", true),
        ];
        let v = detect_whdload(&entries);
        assert_eq!(v.confidence, Confidence::High);
        assert!(v.slave.is_some());
        assert!(v.executable.is_some());
        assert!(v.has_icon);
        assert!(v.has_data_dir);
    }

    #[test]
    fn no_markers_unknown() {
        let entries = vec![entry("readme.txt", false)];
        let v = detect_whdload(&entries);
        assert_eq!(v.confidence, Confidence::Unknown);
    }

    #[test]
    fn slave_only_low_confidence() {
        let entries = vec![entry("Game.slave", false)];
        let v = detect_whdload(&entries);
        assert_eq!(v.confidence, Confidence::Low);
    }

    #[test]
    fn slave_plus_exe_medium() {
        let entries = vec![entry("Game.slave", false), entry("Game", false)];
        let v = detect_whdload(&entries);
        assert_eq!(v.confidence, Confidence::Medium);
    }

    #[test]
    fn ext_case_insensitive() {
        assert!(has_ext("foo.SLAVE", "slave"));
        assert!(has_ext("dir/bar.slave", "slave"));
        assert!(!has_ext("foo.txt", "slave"));
        assert!(!has_ext("noslave", "slave"));
    }
}
