//! [`MediaSource`] for a package archive — an official update
//! (`BoingBag39-1.lha`) or an unofficial one (a Turkish catalog pack), read
//! the same way an ADF or an install CD already is. This round puts official
//! update packages on top of an installed tree; before anything can be
//! applied, a package has to answer the same questions `AdfSource` and
//! `CdSource` already do.
//!
//! ## The gate an archive entry name goes through
//!
//! An archive entry name is untrusted the same way any of the other two
//! media's names would be if they had names to give — except an archive
//! genuinely can hold `../../etc/passwd` or `C:\Windows\evil`, where a floppy
//! or a disc's own directory structure cannot. [`crate::core::security::safe_join`]
//! is the one place in this codebase that turns an untrusted name into a
//! path, and this module uses it for exactly that even though nothing here
//! ever writes to disk: [`ArchiveSource::open`] joins every entry's raw name
//! against a throwaway root, and a name `safe_join` refuses (`..`, an
//! absolute path, a Windows prefix) is dropped rather than shown — the same
//! "refuse, don't sanitise" rule [`crate::core::archive::tree::ArchiveTree`]
//! already follows for the same reason: turning `..\..\Startup` into a
//! harmless-looking `_.._.Startup` would make a caller act on a name that is
//! not what the archive actually said.
//!
//! ## Identified by its single top-level directory, never its filename
//!
//! An archive has no volume label the way a floppy's root block or a disc's
//! volume descriptor does, so the equivalent [`AdfSource`](super::source::AdfSource)
//! and [`CdSource`](super::source_cd::CdSource) already use — read the
//! identity from inside, so a renamed file still resolves — becomes: the
//! archive's single top-level **directory**. Measured on the owner's own
//! packages:
//!
//! ```text
//! BoingBag39-1.lha        →  BoingBag3.9-1     (plus BoingBag3.9-1.info at the root)
//! BoingBag39-2-turkce.lha →  LocaleUpdate
//! ```
//!
//! `BoingBag3.9-1.info` is a Workbench icon sitting at the archive root
//! *beside* `BoingBag3.9-1/` — it is not the payload, and it must not count
//! as a second top-level name. So the rule looks at what an entry *implies*
//! about the archive's shape, not merely where it sits: an entry that nests
//! something (two or more path segments) or that is itself a directory
//! contributes its first segment as a candidate top-level directory; a bare
//! file sitting at the archive's own root contributes nothing. Zero
//! candidates (a flat archive) or more than one (two sibling top-level
//! directories) is refused by name — this round supports the packages it
//! ships recipes for, and an archive of an unexpected shape is exactly the
//! case §89 requires ART to name rather than guess at.
//!
//! ## Everything is relative to that directory, and it is stripped, not kept
//!
//! [`ArchiveSource::entries`] is built by stripping the chosen top-level
//! directory off every path under it, so a rule's `from` reads `C/Assign`,
//! never `BoingBag3.9-1/C/Assign` — the same shape a floppy or a disc's own
//! root-relative paths already have. An entry sitting *outside* that
//! directory (the `.info` icon) is dropped: it is not part of the volume
//! this type represents, the same way a stray `readme.txt` beside an ADF is
//! not part of that floppy's volume.
//!
//! ## Protection, comment and date: absent from the medium, not measured
//!
//! An archive format carries none of AmigaDOS's protection bits, comment, or
//! timestamp the way a real AmigaDOS volume's header block does — ZIP, LHA
//! and 7z all have their own, unrelated metadata, and none of it is any of
//! these three. So, exactly as [`CdSource`](super::source_cd::CdSource)'s own
//! module doc explains for ISO9660: every [`MediaEntry`] this type produces
//! carries [`default_protection`] and empty defaults for comment and date as
//! **declared defaults**, never a reading, because there is no such fact on
//! a package archive to read. A caller must not treat either as evidence
//! about the original file (§89).
//!
//! ## Reading is bounded, never a bare decompress
//!
//! [`ArchiveBackend::read`] takes a limit because a declared size is a claim
//! an archive can lie about — four declared bytes can decompress to ten
//! gigabytes. [`ArchiveSource::read`] passes
//! [`crate::core::archive::extract::MAX_ENTRY_OUTPUT`], the same ceiling
//! every other reader of this codebase's archives is held to, rather than
//! inventing a second bound for this one caller.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use crate::core::adf::bcpl::AmigaDate;
use crate::core::archive::{extract::MAX_ENTRY_OUTPUT, ArchiveBackend, ArchiveEntry};
use crate::core::error::{CoreError, CoreResult};
use crate::core::security::safe_join;
use crate::core::volume::write::file::default_protection;

use super::source::{MediaEntry, MediaSource};

/// [`MediaSource`] for a package archive — the third implementation the
/// engine above (`core::osinstall::plan`, `::apply`) is written against by
/// trait, never by name.
pub struct ArchiveSource {
    path: PathBuf,
    volume_name: String,
    /// Every entry, path **relative to the top-level directory**, with the
    /// backend index that reads it. Listing is cheap and reading is not, so
    /// the listing is held and the bytes are not.
    entries: Vec<(String, usize, ArchiveEntry)>,
    backend: Box<dyn ArchiveBackend>,
}

impl std::fmt::Debug for ArchiveSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArchiveSource")
            .field("path", &self.path)
            .field("volume_name", &self.volume_name)
            .finish()
    }
}

impl ArchiveSource {
    /// Open `path`, list it once, and resolve its single top-level directory.
    pub fn open(path: &Path) -> CoreResult<Self> {
        let mut backend = crate::core::archive::open(path)?;
        let raw_entries = backend.entries()?;

        // Validate every raw name through `safe_join` and keep only its
        // components — never the joined path itself, which is never written
        // to disk from this module. A name `safe_join` refuses is dropped,
        // not fatal to the whole archive: the same "skip what one entry did
        // wrong" rule `scan::find_media` already follows for a whole
        // candidate file.
        let virtual_root = Path::new("archive");
        let mut parsed: Vec<(Vec<String>, usize)> = Vec::new();
        for (index, entry) in raw_entries.iter().enumerate() {
            let Ok(joined) = safe_join(virtual_root, &entry.name) else {
                continue;
            };
            let Ok(relative) = joined.strip_prefix(virtual_root) else {
                continue;
            };
            let components: Vec<String> = relative
                .components()
                .filter_map(|component| match component {
                    Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
                    _ => None,
                })
                .collect();
            if components.is_empty() {
                continue;
            }
            parsed.push((components, index));
        }

        // The archive's identity: the single top-level directory. A root
        // file (one segment, not itself a directory — the `.info` icon)
        // contributes nothing; anything that nests something, or is a
        // directory outright, contributes its first segment.
        let mut tops: BTreeSet<&str> = BTreeSet::new();
        for (components, index) in &parsed {
            let is_dir = raw_entries[*index].is_dir;
            if components.len() >= 2 || is_dir {
                tops.insert(components[0].as_str());
            }
        }

        let volume_name = match tops.len() {
            0 => {
                return Err(CoreError::UnsupportedFormat(format!(
                    "'{}' has no top-level directory — every entry sits at the archive's own \
                     root, so ART has nothing to name this package by",
                    path.display()
                )));
            }
            1 => (*tops.iter().next().expect("len == 1")).to_string(),
            _ => {
                let names: Vec<&str> = tops.into_iter().collect();
                return Err(CoreError::UnsupportedFormat(format!(
                    "'{}' has more than one top-level directory ({}) — ART cannot tell which \
                     one is this package's identity",
                    path.display(),
                    names.join(", ")
                )));
            }
        };

        // Keep only what sits under the chosen directory, with its name
        // stripped: a rule's `from` reads `C/Assign`, never
        // `BoingBag3.9-1/C/Assign`. The directory's own entry (nothing left
        // after stripping) is dropped too — `entry("")` answers the root
        // synthetically, matching `AdfSource::root_entry` and
        // `CdSource::root_entry`, so it must not also appear as a row here.
        let mut entries: Vec<(String, usize, ArchiveEntry)> = Vec::new();
        for (components, index) in parsed {
            if components[0] != volume_name {
                continue;
            }
            let rest = &components[1..];
            if rest.is_empty() {
                continue;
            }
            entries.push((rest.join("/"), index, raw_entries[index].clone()));
        }

        Ok(Self {
            path: path.to_path_buf(),
            volume_name,
            entries,
            backend,
        })
    }

    /// `path`, with no leading, trailing or doubled slash — the same
    /// normalisation `AdfSource::normalized`/`CdSource::normalized` apply for
    /// the same reason: a path built by joining two rules must never carry a
    /// stray slash into a comparison against a stored relative path, which
    /// never has one either.
    fn normalized(path: &str) -> String {
        path.split('/')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("/")
    }

    /// The archive's own root, as a [`MediaEntry`] — declared, not measured,
    /// on the same footing as [`AdfSource::root_entry`](super::source::AdfSource)
    /// and `CdSource::root_entry`.
    fn root_entry() -> MediaEntry {
        MediaEntry {
            path: String::new(),
            is_dir: true,
            size: 0,
            protection: default_protection(),
            date: AmigaDate::default(),
            comment: String::new(),
        }
    }

    /// Map one stored row into the shape a recipe reads.
    fn to_media_entry(relative_path: &str, entry: &ArchiveEntry) -> MediaEntry {
        MediaEntry {
            path: relative_path.to_string(),
            is_dir: entry.is_dir,
            size: if entry.is_dir {
                0
            } else {
                entry.declared_bytes
            },
            protection: default_protection(),
            date: AmigaDate::default(),
            comment: String::new(),
        }
    }

    /// Resolve `normalized` against `entries` — an exact match first, falling
    /// back to a case-insensitive one, matching `CdSource::find_by_path`
    /// (AmigaDOS is case-insensitive, ART-012, and a package's rules are
    /// AmigaDOS paths like any other media's).
    fn find_by_path<'a>(
        entries: &'a [(String, usize, ArchiveEntry)],
        normalized: &str,
    ) -> Option<&'a (String, usize, ArchiveEntry)> {
        entries
            .iter()
            .find(|(relative, ..)| relative == normalized)
            .or_else(|| {
                entries
                    .iter()
                    .find(|(relative, ..)| relative.eq_ignore_ascii_case(normalized))
            })
    }
}

impl MediaSource for ArchiveSource {
    fn volume_name(&self) -> &str {
        &self.volume_name
    }

    fn entry(&mut self, path: &str) -> CoreResult<Option<MediaEntry>> {
        let normalized = Self::normalized(path);
        if normalized.is_empty() {
            return Ok(Some(Self::root_entry()));
        }
        Ok(Self::find_by_path(&self.entries, &normalized)
            .map(|(relative, _, entry)| Self::to_media_entry(relative, entry)))
    }

    fn walk(&mut self, path: &str) -> CoreResult<Vec<MediaEntry>> {
        let normalized = Self::normalized(path);
        if normalized.is_empty() {
            return Ok(self
                .entries
                .iter()
                .map(|(relative, _, entry)| Self::to_media_entry(relative, entry))
                .collect());
        }
        let Some((base, _, found)) = Self::find_by_path(&self.entries, &normalized) else {
            return Ok(Vec::new());
        };
        // A path naming a *file* is refused, word for word as `AdfSource`
        // and `CdSource` both refuse it — see `source_contract.rs`.
        if !found.is_dir {
            return Err(CoreError::InvalidInput(format!(
                "'{path}' is a file on this media, not a drawer"
            )));
        }
        let prefix = format!("{base}/");
        Ok(self
            .entries
            .iter()
            .filter(|(relative, ..)| relative.starts_with(&prefix))
            .map(|(relative, _, entry)| Self::to_media_entry(relative, entry))
            .collect())
    }

    fn read(&mut self, path: &str) -> CoreResult<Vec<u8>> {
        let normalized = Self::normalized(path);
        if normalized.is_empty() {
            return Err(CoreError::InvalidInput(format!(
                "'{path}' is a drawer on this media, not a file"
            )));
        }
        let (index, is_dir) = {
            let Some((_, index, found)) = Self::find_by_path(&self.entries, &normalized) else {
                return Err(CoreError::InvalidInput(format!(
                    "'{path}' is not on this media"
                )));
            };
            (*index, found.is_dir)
        };
        if is_dir {
            return Err(CoreError::InvalidInput(format!(
                "'{path}' is a drawer on this media, not a file"
            )));
        }
        // Never an unbounded read: a declared size is a claim, and this is
        // the same ceiling every other reader of this codebase's archives is
        // held to.
        self.backend.read(index, MAX_ENTRY_OUTPUT)
    }
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::fixtures::scratch;
    use super::*;

    /// Build a ZIP in a tempdir. Synthetic, generated at runtime — ART ships
    /// no Amiga content, and these names are the *shape* of the owner's real
    /// packages, not their contents.
    fn package_zip(dir: &std::path::Path, name: &str, files: &[(&str, &[u8])]) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(
            &path,
            crate::core::archive::zip::tests::make_zip_with(files),
        )
        .unwrap();
        path
    }

    #[test]
    fn the_volume_name_is_the_single_top_level_directory() {
        let dir = scratch("archive-volume");
        let p = package_zip(
            &dir,
            "renamed-by-the-user.zip",
            &[
                ("BoingBag3.9-1.info", b"icon"),
                ("BoingBag3.9-1/C/Assign", b"assign"),
                ("BoingBag3.9-1/Libs/x.library", b"lib"),
            ],
        );
        let src = ArchiveSource::open(&p).unwrap();
        assert_eq!(src.volume_name(), "BoingBag3.9-1");
    }

    #[test]
    fn two_top_level_directories_are_refused_by_name() {
        let dir = scratch("archive-two-tops");
        let p = package_zip(&dir, "two.zip", &[("One/a", b"a"), ("Two/b", b"b")]);
        let err = ArchiveSource::open(&p).unwrap_err().to_string();
        assert!(err.contains("One") && err.contains("Two"), "got {err}");
    }

    #[test]
    fn no_top_level_directory_is_refused() {
        let dir = scratch("archive-flat");
        let p = package_zip(&dir, "flat.zip", &[("a", b"a"), ("b", b"b")]);
        assert!(ArchiveSource::open(&p).is_err());
    }

    /// Paths are relative to the top-level directory, not to the archive: a
    /// rule's `from` says `C/Assign`, never `BoingBag3.9-1/C/Assign`.
    #[test]
    fn paths_are_relative_to_the_top_level_directory() {
        let dir = scratch("archive-rel");
        let p = package_zip(
            &dir,
            "bb.zip",
            &[("BB/C/Assign", b"assign"), ("BB/Libs/x.library", b"lib")],
        );
        let mut src = ArchiveSource::open(&p).unwrap();
        assert!(src.entry("C/Assign").unwrap().is_some());
        assert!(src.entry("BB/C/Assign").unwrap().is_none());
        assert_eq!(src.read("C/Assign").unwrap(), b"assign");
    }

    /// A traversing entry name never becomes a path. The gate is
    /// `core::security::safe_join`, and this proves the source does not go
    /// round it.
    #[test]
    fn a_traversing_entry_name_is_refused() {
        let dir = scratch("archive-traversal");
        let p = package_zip(
            &dir,
            "bad.zip",
            &[("BB/../../etc/passwd", b"x"), ("BB/ok", b"ok")],
        );
        // Either `open` refuses the archive or the entry never appears.
        match ArchiveSource::open(&p) {
            Err(_) => {}
            Ok(mut src) => {
                let all = src.walk("").unwrap();
                assert!(
                    all.iter().all(|e| !e.path.contains("..")),
                    "a traversing name survived: {:?}",
                    all.iter().map(|e| &e.path).collect::<Vec<_>>()
                );
            }
        }
    }
}
