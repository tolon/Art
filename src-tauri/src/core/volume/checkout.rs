//! F4 — editing a file inside an image, safely (brief §6).
//!
//! ART does not implement an editor. It implements the round trip:
//!
//! ```text
//! CHECKOUT                          CHECKIN
//! image:path ──extract──▶ temp dir  temp file ──journalled write──▶ image:path
//!              + SHA-256 recorded              only if the SHA-256 changed
//! ```
//!
//! ## Why the hash
//!
//! An editor that opens a file and closes it without saving must not cause a
//! write. Comparing the hash rather than the modification time is what makes
//! that reliable: plenty of editors touch mtime on open, and a spurious write
//! into a disk image is not a harmless one — it reallocates blocks and burns a
//! backup generation.
//!
//! ## Why the manifest survives a restart
//!
//! A checkout is a file the user is editing right now. Losing the mapping when
//! ART closes would leave an orphan in the temp directory and an edit with
//! nowhere to go back to. The manifest is written on checkout and removed on
//! checkin, so a restart finds the work still waiting.
//!
//! ## Line endings are never converted silently
//!
//! Amiga text is Latin-1 with LF. A Windows editor will happily save CRLF, and
//! a Slave or a startup-sequence with CRLF in it behaves differently on a real
//! Amiga. ART detects it and *offers* the conversion — converting without
//! asking would change the user's file behind their back, and refusing would
//! make the feature useless.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::core::error::{CoreError, CoreResult};
use crate::core::safety::atomic::atomic_write;

/// The most bytes ART will check out for editing.
///
/// A text file or an icon; not a disk-sized blob. The cap exists because the
/// whole file is read into memory twice — once out, once back — and because
/// "edit this 200 MB file" is a request with no good outcome.
pub const MAX_CHECKOUT_BYTES: u64 = 64 * 1024 * 1024;

/// The most manifest bytes ART will read.
const MAX_MANIFEST_BYTES: u64 = 8 * 1024 * 1024;

/// One file checked out for editing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkout {
    /// Stable across restarts: derived from the image and the entry, so the
    /// same file always checks out to the same place and a second F4 on it
    /// reopens the existing copy rather than making a rival one.
    pub id: String,
    pub image: String,
    pub volume_index: usize,
    /// The directory the entry lives in, so checkin knows where to put it back.
    pub dir_block: u32,
    pub entry_block: u32,
    pub name: String,
    pub temp_path: String,
    /// What the file hashed to when it came out.
    pub sha256: String,
    pub bytes: u64,
    /// True when the file came out with no CRLF in it, so gaining one is a
    /// change worth mentioning.
    pub was_lf_only: bool,
    /// True when the file has a NUL byte. Binary files never get the warning.
    pub is_binary: bool,
}

/// Where a checked-out file stands now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CheckoutState {
    /// The temp file is byte-for-byte what came out. Nothing to check in.
    Unchanged,
    /// It has been edited.
    Modified {
        bytes: u64,
        /// The edit introduced CRLF into a file that had none, and it is not
        /// binary. Offer the conversion; never do it unasked.
        gained_crlf: bool,
    },
    /// The temp file is gone — the user deleted it, or the temp directory was
    /// cleaned. Reported, not silently forgotten.
    Missing,
}

/// Where a checkout's temp file goes.
///
/// One folder per image, named from a hash of its path: two images with the
/// same file name must not share a directory, and a path is not a folder name.
pub fn temp_path_for(root: &Path, image: &Path, id: &str, name: &str) -> CoreResult<PathBuf> {
    let folder = root.join(&id[..id.len().min(16)]);

    // The name comes off an Amiga disk, so it goes through the same choke
    // point as every other untrusted name.
    let safe = crate::core::security::path::safe_join(&folder, name).map_err(|err| {
        CoreError::SafetyRefused(format!(
            "'{name}' from {} cannot be written to a checkout folder: {err}",
            image.display()
        ))
    })?;
    Ok(safe)
}

/// A stable id for one file inside one image.
pub fn checkout_id(image: &Path, volume_index: usize, entry_block: u32) -> String {
    let mut hasher = Sha256::new();
    hasher.update(image.display().to_string().as_bytes());
    hasher.update(b"\0");
    hasher.update(volume_index.to_be_bytes());
    hasher.update(entry_block.to_be_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn hash_of(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// A NUL byte means binary, and binary files never get the line-ending
/// warning — an icon full of 0x0D 0x0A pairs is not a text file with CRLF.
pub fn looks_binary(bytes: &[u8]) -> bool {
    bytes.contains(&0)
}

pub fn has_crlf(bytes: &[u8]) -> bool {
    bytes.windows(2).any(|pair| pair == b"\r\n")
}

/// Turn CRLF into LF, the way an Amiga text file has it.
///
/// Only ever called when the user asked. A lone CR is left alone: it is not a
/// Windows line ending, and rewriting it would be a second guess on top of the
/// first.
pub fn to_amiga_line_endings(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\r' && bytes.get(index + 1) == Some(&b'\n') {
            out.push(b'\n');
            index += 2;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    out
}

/// Where checkouts are remembered.
pub trait CheckoutStore: Send + Sync {
    fn put(&self, checkout: Checkout) -> CoreResult<()>;
    fn all(&self) -> CoreResult<Vec<Checkout>>;
    fn get(&self, id: &str) -> CoreResult<Option<Checkout>>;
    fn remove(&self, id: &str) -> CoreResult<()>;
}

/// What a checked-out file's temp copy says now.
pub fn state_of(checkout: &Checkout) -> CheckoutState {
    let path = Path::new(&checkout.temp_path);
    let Ok(bytes) = std::fs::read(path) else {
        return CheckoutState::Missing;
    };

    if hash_of(&bytes) == checkout.sha256 {
        return CheckoutState::Unchanged;
    }

    CheckoutState::Modified {
        bytes: bytes.len() as u64,
        // Only for a text file that had none before: a file that already used
        // CRLF is not being changed by keeping it.
        gained_crlf: !checkout.is_binary
            && !looks_binary(&bytes)
            && checkout.was_lf_only
            && has_crlf(&bytes),
    }
}

// ---------------------------------------------------------------------------
// The manifest
// ---------------------------------------------------------------------------

/// Checkouts in a JSON Lines file, next to the temp folders they describe.
///
/// Same shape as the catalog and the operation log, for the same reasons: one
/// object per line, survives a crash with at most one damaged line, and gains
/// fields without a migration.
#[derive(Debug)]
pub struct JsonlCheckouts {
    path: PathBuf,
    entries: std::sync::Mutex<BTreeMap<String, Checkout>>,
}

impl JsonlCheckouts {
    pub fn load(path: impl Into<PathBuf>) -> CoreResult<Self> {
        let path = path.into();
        let mut entries = BTreeMap::new();

        if path.exists() {
            let size = std::fs::metadata(&path).map_err(CoreError::from)?.len();
            if size > MAX_MANIFEST_BYTES {
                return Err(CoreError::InvalidInput(format!(
                    "the checkout manifest is {size} bytes, larger than ART will read"
                )));
            }
            let text = std::fs::read_to_string(&path).map_err(CoreError::from)?;
            for line in text.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(checkout) = serde_json::from_str::<Checkout>(line) {
                    entries.insert(checkout.id.clone(), checkout);
                }
            }
        }

        Ok(Self {
            path,
            entries: std::sync::Mutex::new(entries),
        })
    }

    pub fn empty_at(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            entries: std::sync::Mutex::new(BTreeMap::new()),
        }
    }

    fn flush(&self, entries: &BTreeMap<String, Checkout>) -> CoreResult<()> {
        let mut text = String::new();
        for checkout in entries.values() {
            text.push_str(&serde_json::to_string(checkout).map_err(|e| {
                CoreError::InvalidInput(format!("a checkout could not be written: {e}"))
            })?);
            text.push('\n');
        }
        atomic_write(&self.path, text.as_bytes())
    }
}

impl CheckoutStore for JsonlCheckouts {
    fn put(&self, checkout: Checkout) -> CoreResult<()> {
        let mut held = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        held.insert(checkout.id.clone(), checkout);
        self.flush(&held)
    }

    fn all(&self) -> CoreResult<Vec<Checkout>> {
        let held = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Ok(held.values().cloned().collect())
    }

    fn get(&self, id: &str) -> CoreResult<Option<Checkout>> {
        let held = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Ok(held.get(id).cloned())
    }

    fn remove(&self, id: &str) -> CoreResult<()> {
        let mut held = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        held.remove(id);
        self.flush(&held)
    }
}

// ---------------------------------------------------------------------------
// `.info` pairing (§7.1)
// ---------------------------------------------------------------------------

/// The icon that belongs to `name`, or `None` when `name` is itself an icon.
///
/// Workbench shows an object only when its `.info` is next to it, so renaming
/// `Game` without renaming `Game.info` makes the game invisible on a real
/// Amiga while looking fine in ART.
pub fn icon_for(name: &str) -> Option<String> {
    if name.to_lowercase().ends_with(".info") {
        return None;
    }
    Some(format!("{name}.info"))
}

/// The object an icon describes, or `None` when `name` is not an icon.
pub fn object_for(name: &str) -> Option<String> {
    let lower = name.to_lowercase();
    lower
        .ends_with(".info")
        .then(|| name[..name.len() - ".info".len()].to_string())
}

/// The name an icon should take when its object is renamed.
pub fn renamed_icon(new_name: &str) -> String {
    format!("{new_name}.info")
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-checkout-{name}-{}",
            crate::core::test_scratch_id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn checkout_at(path: &Path, contents: &[u8]) -> Checkout {
        std::fs::write(path, contents).unwrap();
        Checkout {
            id: "abc123".into(),
            image: "D:/amiga/Work.adf".into(),
            volume_index: 0,
            dir_block: 880,
            entry_block: 900,
            name: "Startup-Sequence".into(),
            temp_path: path.display().to_string(),
            sha256: hash_of(contents),
            bytes: contents.len() as u64,
            was_lf_only: !has_crlf(contents),
            is_binary: looks_binary(contents),
        }
    }

    /// The point of hashing rather than watching the modification time: an
    /// editor that opens a file and closes it must not cause a write into a
    /// disk image.
    #[test]
    fn an_unedited_file_reads_as_unchanged_even_after_being_touched() {
        let dir = scratch("unchanged");
        let path = dir.join("Startup-Sequence");
        let checkout = checkout_at(&path, b"echo hello\n");

        // Rewrite the same bytes, the way an editor's "save" with no edit does.
        std::fs::write(&path, b"echo hello\n").unwrap();
        assert_eq!(state_of(&checkout), CheckoutState::Unchanged);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_edited_file_reads_as_modified_with_its_new_size() {
        let dir = scratch("modified");
        let path = dir.join("Startup-Sequence");
        let checkout = checkout_at(&path, b"echo hello\n");

        std::fs::write(&path, b"echo hello\necho again\n").unwrap();
        assert_eq!(
            state_of(&checkout),
            CheckoutState::Modified {
                bytes: 22,
                gained_crlf: false
            }
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A Windows editor saving CRLF into a startup-sequence changes how it
    /// behaves on a real Amiga. Detected and offered — never converted unasked.
    #[test]
    fn crlf_arriving_in_a_text_file_that_had_none_is_flagged() {
        let dir = scratch("crlf");
        let path = dir.join("Startup-Sequence");
        let checkout = checkout_at(&path, b"echo hello\n");

        std::fs::write(&path, b"echo hello\r\n").unwrap();
        assert_eq!(
            state_of(&checkout),
            CheckoutState::Modified {
                bytes: 12,
                gained_crlf: true
            }
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file that already used CRLF is not being changed by keeping it, and
    /// warning about it would be noise.
    #[test]
    fn a_file_that_already_had_crlf_is_not_flagged_for_keeping_it() {
        let dir = scratch("crlf-already");
        let path = dir.join("Readme.txt");
        let checkout = checkout_at(&path, b"line one\r\n");
        assert!(!checkout.was_lf_only);

        std::fs::write(&path, b"line one\r\nline two\r\n").unwrap();
        match state_of(&checkout) {
            CheckoutState::Modified { gained_crlf, .. } => assert!(!gained_crlf),
            other => panic!("expected Modified, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An icon full of 0x0D 0x0A pairs is not a text file with CRLF.
    #[test]
    fn a_binary_file_never_gets_the_line_ending_warning() {
        let dir = scratch("binary");
        let path = dir.join("Game.info");
        let checkout = checkout_at(&path, b"\xE3\x10\x00\x01\x0D\x0A\x00");
        assert!(checkout.is_binary);

        std::fs::write(&path, b"\xE3\x10\x00\x01\x0D\x0A\x0D\x0A\x00").unwrap();
        match state_of(&checkout) {
            CheckoutState::Modified { gained_crlf, .. } => assert!(!gained_crlf),
            other => panic!("expected Modified, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_temp_file_that_has_gone_is_reported_not_forgotten() {
        let dir = scratch("missing");
        let path = dir.join("Gone.txt");
        let checkout = checkout_at(&path, b"x");
        std::fs::remove_file(&path).unwrap();

        assert_eq!(state_of(&checkout), CheckoutState::Missing);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn converting_line_endings_touches_only_crlf() {
        assert_eq!(to_amiga_line_endings(b"a\r\nb\r\n"), b"a\nb\n");
        assert_eq!(to_amiga_line_endings(b"a\nb\n"), b"a\nb\n");
        // A lone CR is not a Windows line ending and is left alone.
        assert_eq!(to_amiga_line_endings(b"a\rb"), b"a\rb");
        // …including at the very end, where there is no byte to look at.
        assert_eq!(to_amiga_line_endings(b"a\r"), b"a\r");
    }

    /// The same file must always check out to the same place, so a second F4
    /// reopens the existing copy rather than making a rival one.
    #[test]
    fn the_id_is_stable_for_the_same_file_and_different_for_others() {
        let image = Path::new("D:/amiga/Work.adf");
        let first = checkout_id(image, 0, 900);

        assert_eq!(first, checkout_id(image, 0, 900));
        assert_ne!(first, checkout_id(image, 0, 901));
        assert_ne!(first, checkout_id(image, 1, 900));
        assert_ne!(first, checkout_id(Path::new("D:/amiga/Other.adf"), 0, 900));
    }

    /// Two images with the same file name must not share a checkout folder.
    #[test]
    fn two_images_get_different_checkout_folders() {
        let root = Path::new("C:/temp/checkout");
        let one = temp_path_for(
            root,
            Path::new("D:/a/Work.adf"),
            &checkout_id(Path::new("D:/a/Work.adf"), 0, 900),
            "Startup-Sequence",
        )
        .unwrap();
        let two = temp_path_for(
            root,
            Path::new("D:/b/Work.adf"),
            &checkout_id(Path::new("D:/b/Work.adf"), 0, 900),
            "Startup-Sequence",
        )
        .unwrap();

        assert_ne!(one, two);
        assert!(one.ends_with("Startup-Sequence"));
    }

    /// The name comes off an Amiga disk. It is untrusted like any other.
    #[test]
    fn a_name_that_would_escape_the_checkout_folder_is_refused() {
        let root = Path::new("C:/temp/checkout");
        assert!(temp_path_for(root, Path::new("D:/Work.adf"), "abc", "../evil.exe").is_err());
    }

    #[test]
    fn the_manifest_survives_a_reload() {
        let dir = scratch("manifest");
        let file = dir.join("checkouts.jsonl");
        let temp = dir.join("Startup-Sequence");

        let store = JsonlCheckouts::load(&file).unwrap();
        store.put(checkout_at(&temp, b"echo hello\n")).unwrap();

        let reopened = JsonlCheckouts::load(&file).unwrap();
        let all = reopened.all().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "Startup-Sequence");
        assert!(reopened.get("abc123").unwrap().is_some());

        reopened.remove("abc123").unwrap();
        assert!(JsonlCheckouts::load(&file)
            .unwrap()
            .all()
            .unwrap()
            .is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn checking_the_same_file_out_twice_keeps_one_entry() {
        let dir = scratch("manifest-replace");
        let file = dir.join("checkouts.jsonl");
        let temp = dir.join("Startup-Sequence");

        let store = JsonlCheckouts::load(&file).unwrap();
        store.put(checkout_at(&temp, b"one")).unwrap();
        store.put(checkout_at(&temp, b"two")).unwrap();

        assert_eq!(store.all().unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- .info pairing ----

    /// Workbench shows an object only when its icon is next to it, so renaming
    /// one without the other makes it invisible on a real Amiga.
    #[test]
    fn an_object_names_the_icon_that_belongs_to_it() {
        assert_eq!(icon_for("Game").as_deref(), Some("Game.info"));
        assert_eq!(icon_for("Readme.txt").as_deref(), Some("Readme.txt.info"));
        // An icon has no icon of its own.
        assert_eq!(icon_for("Game.info"), None);
        assert_eq!(icon_for("Game.INFO"), None, "case does not matter");
    }

    #[test]
    fn an_icon_names_the_object_it_describes() {
        assert_eq!(object_for("Game.info").as_deref(), Some("Game"));
        assert_eq!(object_for("Game.INFO").as_deref(), Some("Game"));
        assert_eq!(object_for("Game"), None);
    }

    #[test]
    fn renaming_an_object_renames_its_icon_to_match() {
        assert_eq!(renamed_icon("NewName"), "NewName.info");
    }
}
