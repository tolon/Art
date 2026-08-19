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
//!
//! ## A medium inside a medium: [`ArchiveSource::open_nested`]
//!
//! A real BoingBag (`BoingBag39-1.lha`) is a wrapper, not the medium itself:
//! beside its payload sit `C/Updater` and its catalogs in seventeen
//! languages — the Amiga-side installer, which ART does not run and does not
//! treat as part of the install source. The payload the recipe actually
//! wants is `AmigaOS-Update`, a **ZIP stored uncompressed inside the LHA**
//! (identical declared and stored size; its first four bytes are the ZIP
//! signature).
//!
//! [`core::archive::open`](crate::core::archive::open) takes a path, and
//! `ZipBackend` holds a `ZipArchive<BufReader<File>>` — there is no
//! constructor over bytes, and making every backend generic over its source
//! just to serve this one caller would be the wrong trade. So
//! [`open_nested`](ArchiveSource::open_nested) reads the wrapper the same way
//! [`open`](ArchiveSource::open) does, finds `member` among its
//! top-level-relative entries, reads it bounded by the same
//! [`MAX_ENTRY_OUTPUT`] ceiling `read` uses, and writes those bytes to a
//! throwaway file so the existing machinery — `core::archive::open`,
//! `core::detect` included — can open *that*. Three things fall out for
//! free: a member that claims to be a ZIP and is not gets caught by
//! `core::detect` rather than trusted; every backend stays unchanged; and the
//! member is bounded on the way out by the gate that already bounds
//! everything else this module reads. The temporary file is removed whether
//! the open succeeds or fails — never left for the open to fail *into*.
//!
//! **The inner archive gets no single-top-level-directory rule.** The
//! measured BoingBag payload's top level is thirteen directories (`C`,
//! `Classes`, `Devs`, `Fonts`, `L`, `Libs`, `Prefs`, `S`, `Storage`, `System`,
//! `Tools`, `Utilities`, `WBStartup`) — a Workbench volume, not a single
//! named package. Its paths are already volume-relative, so applying `open`'s
//! "exactly one top-level directory or refuse" rule to it would refuse every
//! real BoingBag outright. The nested source's volume name instead comes
//! from the **outer** archive's own top-level directory — the wrapper's
//! identity, since the inner archive carries none of its own.

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
    /// Validate every raw name through `safe_join` and keep only its
    /// components — never the joined path itself, which is never written to
    /// disk from this module. A name `safe_join` refuses is dropped, not
    /// fatal to the whole archive: the same "skip what one entry did wrong"
    /// rule `scan::find_media` already follows for a whole candidate file.
    ///
    /// Shared by [`open`](Self::open), which turns this into a single
    /// top-level directory, and [`open_flat`](Self::open_flat), which does
    /// not — a nested archive's paths are already volume-relative.
    fn parsed_entries(raw_entries: &[ArchiveEntry]) -> Vec<(Vec<String>, usize)> {
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
        parsed
    }

    /// Open `path`, list it once, and resolve its single top-level directory.
    pub fn open(path: &Path) -> CoreResult<Self> {
        let mut backend = crate::core::archive::open(path)?;
        let raw_entries = backend.entries()?;
        let parsed = Self::parsed_entries(&raw_entries);

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

    /// Open `path` the same way [`open`](Self::open) does, except every
    /// entry's path is kept **as-is**, with no single-top-level-directory
    /// rule applied and no `volume_name` resolved (the caller — only
    /// [`open_nested`](Self::open_nested)) — sets it from elsewhere. This is
    /// what an already-volume-relative archive (a BoingBag payload's own
    /// ZIP) needs: `open`'s rule would see thirteen top-level directories and
    /// refuse it.
    fn open_flat(path: &Path) -> CoreResult<Self> {
        let mut backend = crate::core::archive::open(path)?;
        let raw_entries = backend.entries()?;
        let entries: Vec<(String, usize, ArchiveEntry)> = Self::parsed_entries(&raw_entries)
            .into_iter()
            .map(|(components, index)| (components.join("/"), index, raw_entries[index].clone()))
            .collect();

        Ok(Self {
            path: path.to_path_buf(),
            volume_name: String::new(),
            entries,
            backend,
        })
    }

    /// Open `member` out of `outer` as its own medium — the BoingBag shape:
    /// a payload archive stored inside a wrapper archive.
    ///
    /// `outer` is opened and resolved exactly as [`open`](Self::open) does,
    /// so `member` is looked up **relative to the wrapper's own top-level
    /// directory** — a recipe names `AmigaOS-Update`, never
    /// `BoingBag3.9-1/AmigaOS-Update`. The found entry is read bounded by
    /// [`MAX_ENTRY_OUTPUT`], written to a throwaway file, and opened with
    /// [`open_flat`](Self::open_flat) rather than [`open`](Self::open) — the
    /// inner archive's paths are already volume-relative, and the resulting
    /// `volume_name` is set to the **wrapper's** top-level directory, not
    /// anything read from inside the payload.
    pub fn open_nested(outer: &Path, member: &str) -> CoreResult<Self> {
        let mut wrapper = Self::open(outer)?;
        let normalized_member = Self::normalized(member);
        let (index, is_dir) = {
            let Some((_, index, found)) = Self::find_by_path(&wrapper.entries, &normalized_member)
            else {
                return Err(CoreError::InvalidInput(format!(
                    "'{member}' is not in '{}'",
                    outer.display()
                )));
            };
            (*index, found.is_dir)
        };
        if is_dir {
            return Err(CoreError::InvalidInput(format!(
                "'{member}' is a drawer inside '{}', not a payload archive",
                outer.display()
            )));
        }

        // Bounded the same way every other read out of this module's
        // archives is: a declared size is a claim, never a promise.
        let bytes = wrapper.backend.read(index, MAX_ENTRY_OUTPUT)?;

        // A throwaway file so `core::archive::open` — `core::detect`
        // included — can open the member the same way it opens any other
        // archive on disk; there is no constructor over bytes. Removed on
        // both the success and the failure path, so a member that turns out
        // not to be an archive at all never leaves a leftover behind.
        let tmp = Self::write_nested_temp_file(member, &bytes)?;
        let opened = (|| -> CoreResult<Self> {
            let mut inner = Self::open_flat(&tmp)?;
            inner.volume_name = wrapper.volume_name().to_string();
            inner.path = outer.to_path_buf();
            Ok(inner)
        })();
        let _ = std::fs::remove_file(&tmp);

        opened
    }

    /// How many candidate names [`write_nested_temp_file`](Self::write_nested_temp_file)
    /// tries before giving up. A collision this many times in a row, each one
    /// guarded by its own atomic create, means something is actively wrong
    /// rather than merely unlucky.
    const NESTED_TEMP_ATTEMPTS: u32 = 8;

    /// Write `bytes` to a fresh scratch file for [`open_nested`](Self::open_nested)
    /// and return its path.
    ///
    /// **Why a timestamp alone is not enough here, unlike
    /// [`atomic_write`](crate::core::safety::atomic::atomic_write)'s own
    /// `temp_path_for`.** `atomic_write`'s stem is the real destination
    /// file's own name — it varies with whichever file the caller happens to
    /// be writing, so two callers colliding on both the same stem *and* the
    /// same nanosecond is already remote. `open_nested`'s stem is the
    /// **member name inside the archive**, and every real BoingBag names its
    /// payload the same fixed string, `AmigaOS-Update` — not a value that
    /// varies per call. `core::jobs::spawn_job` runs jobs on independent OS
    /// threads (an install and a preview can be mid-flight at once), so every
    /// concurrent `open_nested` call for a real package computes the *same*
    /// stem at the *same* time — a timestamp only narrows that race, it does
    /// not close it, and the thing at stake if it loses is one job silently
    /// reading another job's bytes. So uniqueness here comes from
    /// [`std::fs::File::create_new`] (`O_EXCL` / `CREATE_NEW`) succeeding,
    /// never from the name looking unique: a name that turns out to already
    /// exist is not reused, it is thrown away and a fresh one tried, up to
    /// [`NESTED_TEMP_ATTEMPTS`](Self::NESTED_TEMP_ATTEMPTS) times.
    fn write_nested_temp_file(member: &str, bytes: &[u8]) -> CoreResult<PathBuf> {
        use std::io::Write;
        use std::sync::atomic::{AtomicU64, Ordering};

        // A per-process counter *in addition to* the timestamp: two threads
        // reading `SystemTime::now()` in the same nanosecond is exactly the
        // case exclusive create exists to survive, but folding the counter
        // into the candidate name too means the common case never needs a
        // second attempt.
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let stem = Path::new(member)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "art-nested-member".to_string());

        for _ in 0..Self::NESTED_TEMP_ATTEMPTS {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
            let candidate = std::env::temp_dir().join(format!(".{stem}.art-tmp-{stamp}-{counter}"));

            match std::fs::File::create_new(&candidate) {
                Ok(mut file) => {
                    let write_result = file.write_all(bytes).and_then(|()| file.sync_all());
                    if let Err(e) = write_result {
                        let _ = std::fs::remove_file(&candidate);
                        return Err(CoreError::Io(e));
                    }
                    return Ok(candidate);
                }
                // Another caller's exclusive create won this exact name —
                // never write into or read back what it made; try again
                // under a fresh one.
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(CoreError::Io(e)),
            }
        }

        Err(CoreError::InvalidInput(format!(
            "could not create a scratch file for '{member}' after \
             {} attempts — every candidate name was already taken",
            Self::NESTED_TEMP_ATTEMPTS
        )))
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

    /// The BoingBag shape: an archive whose payload is another archive.
    #[test]
    fn a_nested_member_becomes_the_medium() {
        let dir = scratch("archive-nested");
        let inner = crate::core::archive::zip::tests::make_zip_with(&[
            ("Libs/version.library", b"lib bytes"),
            ("C/Version", b"cmd bytes"),
        ]);
        let outer = package_zip(
            &dir,
            "BoingBagLike.zip",
            &[
                ("BB/AmigaOS-Update", &inner),
                ("BB/C/Updater", b"an amiga program ART does not run"),
            ],
        );

        let mut src = ArchiveSource::open_nested(&outer, "AmigaOS-Update").unwrap();
        // The volume name is the *wrapper's* top-level directory ("BB"),
        // never anything read from inside the payload — swapping outer and
        // inner identity here would still open, still read, and still be
        // wrong, so this has to be its own assertion rather than implied by
        // the reads below succeeding.
        assert_eq!(src.volume_name(), "BB");
        assert_eq!(src.read("Libs/version.library").unwrap(), b"lib bytes");
        // The outer archive's own files are *not* visible: the medium is the
        // payload, and `C/Updater` belongs to the wrapper.
        assert!(src.entry("C/Updater").unwrap().is_none());
    }

    /// ART-style callers of `open_nested` are jobs on independent OS
    /// threads (`core::jobs::spawn_job`) — an install and a preview could be
    /// mid-flight together — and every real BoingBag names its payload the
    /// same fixed string, `AmigaOS-Update`. Two concurrent calls for that
    /// same member name, out of two *different* wrapper archives, must never
    /// let one see the bytes the other extracted: proof that
    /// `write_nested_temp_file`'s exclusive create — not a timestamp that
    /// merely looks unique — is what keeps them apart.
    #[test]
    fn two_concurrent_opens_of_the_same_member_name_never_cross_streams() {
        let dir = scratch("archive-nested-concurrent");
        let inner_a =
            crate::core::archive::zip::tests::make_zip_with(&[("C/Version", b"FROM ARCHIVE A")]);
        let inner_b =
            crate::core::archive::zip::tests::make_zip_with(&[("C/Version", b"FROM ARCHIVE B")]);
        let outer_a = package_zip(&dir, "A.zip", &[("BB/AmigaOS-Update", &inner_a)]);
        let outer_b = package_zip(&dir, "B.zip", &[("BB/AmigaOS-Update", &inner_b)]);

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));

        let barrier_a = barrier.clone();
        let handle_a = std::thread::spawn(move || {
            barrier_a.wait();
            let mut src = ArchiveSource::open_nested(&outer_a, "AmigaOS-Update").unwrap();
            src.read("C/Version").unwrap()
        });

        let barrier_b = barrier.clone();
        let handle_b = std::thread::spawn(move || {
            barrier_b.wait();
            let mut src = ArchiveSource::open_nested(&outer_b, "AmigaOS-Update").unwrap();
            src.read("C/Version").unwrap()
        });

        let got_a = handle_a.join().unwrap();
        let got_b = handle_b.join().unwrap();
        assert_eq!(got_a, b"FROM ARCHIVE A");
        assert_eq!(got_b, b"FROM ARCHIVE B");
    }

    /// A member that is not an archive at all is refused, not treated as an
    /// empty medium — a silently empty medium is a silently short plan.
    #[test]
    fn a_member_that_is_not_an_archive_is_refused() {
        let dir = scratch("archive-nested-bad");
        let outer = package_zip(
            &dir,
            "bad.zip",
            &[("BB/AmigaOS-Update", b"not an archive, just bytes")],
        );
        assert!(ArchiveSource::open_nested(&outer, "AmigaOS-Update").is_err());
    }

    /// A member the outer archive does not hold is refused by name.
    #[test]
    fn a_missing_member_is_refused_by_name() {
        let dir = scratch("archive-nested-missing");
        let outer = package_zip(&dir, "x.zip", &[("BB/Something", b"x")]);
        let err = ArchiveSource::open_nested(&outer, "AmigaOS-Update")
            .unwrap_err()
            .to_string();
        assert!(err.contains("AmigaOS-Update"), "got {err}");
    }

    /// Nothing is left behind, whether the open worked or not.
    ///
    /// `open_nested`'s scratch file follows `core::safety::atomic`'s own
    /// naming (`.{stem}.art-tmp-{stamp}`), which is also what
    /// `atomic::tests::leaves_no_temp_files_behind` greps for — so counting
    /// entries under the system temp directory whose name contains
    /// `art-tmp` is the existing convention for "did a temp file survive",
    /// not a new one invented for this test. The member name here
    /// (`NestedCleanupCheck`) is folded into the same filter and chosen to
    /// be unique to this test: `nested_temp_path` derives its stem from the
    /// member name, cargo runs tests in parallel by default, and the other
    /// tests in this module open a member literally named `AmigaOS-Update`
    /// — filtering on that name instead would transiently catch *their*
    /// temp files too, whichever happened to be mid-flight.
    #[test]
    fn the_temporary_extraction_is_cleaned_up_either_way() {
        const MEMBER: &str = "NestedCleanupCheck";

        fn count_leftover_temp_files() -> usize {
            std::fs::read_dir(std::env::temp_dir())
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok())
                .filter(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    name.contains("art-tmp") && name.contains(MEMBER)
                })
                .count()
        }

        let dir = scratch("archive-nested-cleanup");
        let inner = crate::core::archive::zip::tests::make_zip_with(&[("C/Version", b"cmd bytes")]);
        let good = package_zip(&dir, "Good.zip", &[(&format!("BB/{MEMBER}"), &inner)]);
        let bad = package_zip(
            &dir,
            "Bad.zip",
            &[(&format!("BB/{MEMBER}"), b"not an archive")],
        );

        let before = count_leftover_temp_files();

        let mut ok = ArchiveSource::open_nested(&good, MEMBER).unwrap();
        assert_eq!(ok.read("C/Version").unwrap(), b"cmd bytes");
        assert_eq!(
            count_leftover_temp_files(),
            before,
            "a successful open_nested left its extraction behind"
        );

        assert!(ArchiveSource::open_nested(&bad, MEMBER).is_err());
        assert_eq!(
            count_leftover_temp_files(),
            before,
            "a failed open_nested left its extraction behind"
        );
    }
}
