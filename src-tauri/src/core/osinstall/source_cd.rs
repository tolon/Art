//! [`MediaSource`] for an optical disc — the second implementation
//! `core::osinstall`'s engine is written against, never a rewrite of it: a
//! future `apply` targets the trait, not `AdfSource` by name.
//!
//! ## Walked once, at `open`
//!
//! The same call [`crate::core::iso::IsoSource`] makes, and for the same
//! reason: a disc's directory tree does not change under ART's feet the way
//! a live host folder theoretically could, so [`CdSource::open`] walks it
//! once and every later `entry`/`walk`/`read` is a lookup into that
//! snapshot, not a fresh read of the medium.
//!
//! ## Identified by what is inside, never by a filename
//!
//! The rule `AdfSource` follows (see its own module doc, `source.rs`):
//! [`CdSource::open`] reads the volume name the disc itself carries, so a
//! renamed ISO still resolves against a recipe's `Component::media`.
//!
//! ## Protection and comment: absent from the medium, not measured
//!
//! ISO9660 has nowhere to keep an AmigaDOS protection byte or a file
//! comment — ordinary AmigaDOS metadata that simply is not part of this
//! format. `core::volume::write`'s own convention for "nothing set" is
//! [`default_protection`] (`----rwed`, i.e. `0`), and
//! `core::iso::IsoSource::metadata` — the *other* place a disc is read for
//! the same reason — already answers the same question the same way. So
//! every [`MediaEntry`] this type produces carries `protection:
//! default_protection()` and `comment: String::new()` as **declared
//! defaults**, on the same footing `AdfSource::root_entry` gives its own
//! unmeasurable fields — never a fact read off the disc, because there is no
//! such fact on a disc to read. A caller must not treat either as evidence
//! about the original file (§89).
//!
//! The date is different: a disc *does* record one (`IsoEntry::date`, when
//! the disc did not leave it blank), so it is genuinely read, through the
//! same `amiga_from_unix` conversion `IsoSource::metadata` already uses.
//! Only when the disc itself left it blank does this fall back to
//! `AmigaDate::default()` — the Amiga epoch, and the same declared-default
//! reasoning as the other two fields, for a value that is absent rather
//! than zero.

use std::path::Path;

use crate::core::adf::bcpl::AmigaDate;
use crate::core::error::{CoreError, CoreResult};
use crate::core::iso::{IsoImage, IsoWalkEntry, MAX_WALK_DEPTH, MAX_WALK_ENTRIES};
use crate::core::volume::write::file::default_protection;
use crate::core::volume::write::layout::amiga_from_unix;

use super::source::{MediaEntry, MediaSource};

/// [`MediaSource`] for an ISO9660 disc — a CD or DVD image, Joliet-aware
/// through [`IsoImage`] itself.
#[derive(Debug)]
pub struct CdSource {
    image: IsoImage,
    /// The whole disc, walked once at [`open`](Self::open) —
    /// `IsoWalkEntry::path` is already `/`-separated and relative to the
    /// disc's own root, exactly what [`MediaEntry::path`] promises.
    entries: Vec<IsoWalkEntry>,
}

impl CdSource {
    /// Open `path` and walk its whole tree once.
    ///
    /// `IsoImage::walk` can stop short of the whole disc — `depth_limited`
    /// when a directory sits at [`MAX_WALK_DEPTH`], `truncated` at
    /// [`MAX_WALK_ENTRIES`] — and returns what it found anyway rather than
    /// failing outright, because a caller like a file-manager listing would
    /// rather show a partial tree than nothing. An install source is not
    /// that caller: a `CdSource` missing files it silently never saw would
    /// turn into an install plan silently missing files, which is exactly
    /// the "truncated listing presented as complete" `IsoImage::walk`'s own
    /// doc comment names as the thing to avoid (§10, §89). So here, unlike
    /// the file-manager path, either flag is refused rather than carried
    /// through — the one call site allowed to decide "some of the disc is
    /// good enough".
    pub fn open(path: &Path) -> CoreResult<Self> {
        let image = IsoImage::open(path)?;
        let walk = image.walk()?;
        if walk.truncated {
            return Err(CoreError::Malformed {
                format: "iso9660".into(),
                detail: format!(
                    "this disc holds more than {MAX_WALK_ENTRIES} entries; ART stopped \
                     reading before it reached the end of the tree, so an install plan \
                     built from it would be missing files without saying so"
                ),
            });
        }
        if walk.depth_limited {
            return Err(CoreError::Malformed {
                format: "iso9660".into(),
                detail: format!(
                    "this disc nests directories deeper than {MAX_WALK_DEPTH} levels; ART \
                     stopped descending before it reached the bottom of the tree, so an \
                     install plan built from it would be missing files without saying so"
                ),
            });
        }
        Ok(Self {
            image,
            entries: walk.entries,
        })
    }

    /// `path`, with no leading, trailing or doubled slash — the same
    /// normalisation `AdfSource::normalized` applies for the same reason: a
    /// path built by joining two rules must never carry a stray slash into a
    /// comparison against `IsoWalkEntry::path`, which never has one either.
    fn normalized(path: &str) -> String {
        path.split('/')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("/")
    }

    /// Map one walked disc entry into the shape a recipe reads.
    fn to_media_entry(walked: &IsoWalkEntry) -> MediaEntry {
        MediaEntry {
            path: walked.path.clone(),
            is_dir: walked.entry.is_dir,
            size: walked.entry.bytes,
            protection: default_protection(),
            date: walked.entry.date.map(amiga_from_unix).unwrap_or_default(),
            comment: String::new(),
        }
    }

    /// The disc's own root, as a [`MediaEntry`] — never one of
    /// `self.entries` itself, because `IsoImage::walk` lists a directory's
    /// *contents*, not the directory. A flat, whole-disc component (the
    /// disc equivalent of `AdfSource`'s `Fonts.adf`) still needs its
    /// `Subtree` rule's `from: ""` to resolve to *something*:
    /// `core/osinstall/plan.rs` calls `source.entry(&rule.from)` for every
    /// rule, regardless of kind, and turns `None` into a `MediaPathMissing`
    /// refusal that would wrongly say the whole component is not on the
    /// disc. `AdfSource::root_entry` answers the same question for a floppy
    /// with a declared default rather than a reading, because there is
    /// nothing on a bare volume's root block to read as protection,
    /// comment or date either; a disc's root is the same kind of nothing,
    /// for the same reason this whole module already gives every entry —
    /// there is no protection byte, comment or (for the root specifically)
    /// recording date on an ISO9660 disc to measure.
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
}

impl MediaSource for CdSource {
    fn volume_name(&self) -> &str {
        self.image.volume_name()
    }

    fn entry(&mut self, path: &str) -> CoreResult<Option<MediaEntry>> {
        let normalized = Self::normalized(path);
        if normalized.is_empty() {
            return Ok(Some(Self::root_entry()));
        }
        Ok(self
            .entries
            .iter()
            .find(|e| e.path == normalized)
            .map(Self::to_media_entry))
    }

    fn walk(&mut self, path: &str) -> CoreResult<Vec<MediaEntry>> {
        let normalized = Self::normalized(path);
        if normalized.is_empty() {
            // `""` names the disc's own root, which — like a directory
            // resolved by name — is never itself one of `self.entries`
            // (`IsoImage::walk` lists a directory's contents, not the
            // directory). So the whole disc is exactly every entry there is.
            return Ok(self.entries.iter().map(Self::to_media_entry).collect());
        }
        // Strict descendants only, matching `AdfSource::walk`: the
        // directory named by `path` is never included in its own listing,
        // only what is inside it — `core::osinstall::plan` relies on this
        // exact shape ("`walk` yields only what is *inside* `from`, never
        // `from` itself").
        let prefix = format!("{normalized}/");
        Ok(self
            .entries
            .iter()
            .filter(|e| e.path.starts_with(&prefix))
            .map(Self::to_media_entry)
            .collect())
    }

    fn read(&mut self, path: &str) -> CoreResult<Vec<u8>> {
        let normalized = Self::normalized(path);
        let found = self
            .entries
            .iter()
            .find(|e| e.path == normalized)
            .ok_or_else(|| CoreError::InvalidInput(format!("'{path}' is not on this media")))?;
        if found.entry.is_dir {
            return Err(CoreError::InvalidInput(format!(
                "'{path}' is a drawer on this media, not a file"
            )));
        }
        self.image.read_file(found.entry.extent, found.entry.bytes)
    }
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::iso::fixture::{dir, file, IsoBuilder};

    // `folder`, not `dir`: `dir` is the imported fixture builder above and a
    // local of the same name would shadow it inside this function.
    //
    // `IsoBuilder` in this codebase is a plain struct with a `Default` impl
    // rather than a `new`/`child` builder, so this constructs it as a
    // literal. `joliet: true` (with `joliet_volume` carrying the same names
    // as the `joliet` argument to `dir`/`file` below) is what makes
    // `IsoImage::open` prefer the long-name tree — the tree that carries
    // `OS-Version3.9`, `Workbench3.5` and the rest of the deep, long-named
    // path every test here reads back, exactly as a real AmigaOS 3.9 disc
    // does (test 5 below is the one that makes this load-bearing explicit).
    fn disc(dirname: &str) -> std::path::PathBuf {
        let folder =
            std::env::temp_dir().join(format!("art-cdsource-{dirname}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let bytes = IsoBuilder {
            volume: "AmigaOS3.9".to_string(),
            joliet_volume: "AmigaOS3.9".to_string(),
            joliet: true,
            children: vec![dir(
                "OS-VERSION3.9",
                "OS-Version3.9",
                vec![dir(
                    "WORKBENCH3.5",
                    "Workbench3.5",
                    vec![
                        dir("C", "C", vec![file("LIST.;1", "List", b"list-bytes")]),
                        file("STARTUP.;1", "Startup-Sequence", b"boot"),
                    ],
                )],
            )],
            ..Default::default()
        }
        .build();
        let path = folder.join("os39.iso");
        std::fs::write(&path, bytes).unwrap();
        path
    }

    /// The recipe names a volume, never a filename — the same rule
    /// `AdfSource` follows so a renamed image still identifies itself.
    #[test]
    fn a_disc_answers_with_the_volume_name_recorded_inside_it() {
        let path = disc("volume");
        let source = CdSource::open(&path).unwrap();
        assert_eq!(source.volume_name(), "AmigaOS3.9");
    }

    /// A recipe's `from` is a `/`-separated path from the media's own root,
    /// and a CD's is deep where a floppy's is shallow.
    #[test]
    fn a_deep_path_is_found_and_read() {
        let path = disc("deep");
        let mut source = CdSource::open(&path).unwrap();

        let entry = source
            .entry("OS-Version3.9/Workbench3.5/C/List")
            .unwrap()
            .expect("the disc holds it");
        assert!(!entry.is_dir);
        assert_eq!(entry.size, b"list-bytes".len() as u64);
        assert_eq!(
            source.read("OS-Version3.9/Workbench3.5/C/List").unwrap(),
            b"list-bytes"
        );
    }

    /// `walk` returns paths relative to the media's own root, not to the
    /// path asked for — the trait says so, and `apply` relies on it.
    #[test]
    fn walk_returns_paths_from_the_discs_own_root() {
        let path = disc("walk");
        let mut source = CdSource::open(&path).unwrap();

        let entries = source.walk("OS-Version3.9/Workbench3.5/C").unwrap();
        let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        assert!(
            paths.contains(&"OS-Version3.9/Workbench3.5/C/List"),
            "{paths:?}"
        );
    }

    /// Absence is not an error — the trait's own contract, and the engine
    /// decides whether a missing path is a refusal.
    #[test]
    fn a_path_the_disc_does_not_hold_is_absent_rather_than_an_error() {
        let path = disc("absent");
        let mut source = CdSource::open(&path).unwrap();

        assert!(source.entry("OS-Version3.9/NotThere").unwrap().is_none());
        assert!(source.walk("OS-Version3.9/NotThere").unwrap().is_empty());
    }

    /// The CD carries names with spaces (`AmigaOS 3.9 Manual`) that plain
    /// ISO9660 would mangle, so the long names are the ones a recipe is
    /// written against.
    #[test]
    fn the_long_names_are_the_ones_a_recipe_sees() {
        let folder =
            std::env::temp_dir().join(format!("art-cdsource-joliet-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let bytes = IsoBuilder {
            volume: "AmigaOS3.9".to_string(),
            joliet_volume: "AmigaOS3.9".to_string(),
            joliet: true,
            children: vec![dir(
                "MANUAL",
                "AmigaOS 3.9 Manual",
                vec![file("A.;1", "index.html", b"x")],
            )],
            ..Default::default()
        }
        .build();
        let path = folder.join("os39.iso");
        std::fs::write(&path, bytes).unwrap();

        let mut source = CdSource::open(&path).unwrap();
        assert!(source
            .entry("AmigaOS 3.9 Manual/index.html")
            .unwrap()
            .is_some());

        let _ = std::fs::remove_dir_all(&folder);
    }

    // ---- fix round 1: review item 1 ----

    /// `core/osinstall/plan.rs` calls `source.entry(&rule.from)` for a
    /// `Subtree` rule's own root before it ever walks the rule's contents —
    /// including when `from: ""` names the whole disc — so `entry("")` has
    /// to answer with a directory, not `None`, or a flat whole-disc
    /// component would be refused as `MediaPathMissing` against media that
    /// genuinely holds it. `AdfSource` has the identical test for the
    /// identical reason.
    #[test]
    fn an_empty_path_entry_resolves_to_the_root_directory() {
        let path = disc("root-entry");
        let mut source = CdSource::open(&path).unwrap();

        let entry = source.entry("").unwrap().unwrap();
        assert!(entry.is_dir);
        assert_eq!(entry.path, "");
    }

    // ---- fix round 1: review item 2 ----

    /// `read("OS-Version3.9/Workbench3.5/C")` would otherwise hand
    /// `IsoImage::read_file` a directory's own extent and length — bounds
    /// checked, so not a crash, but a `kind: "file"` rule aimed at a drawer
    /// by mistake has to be refused by name, not produce whatever bytes an
    /// ISO9660 directory extent happens to read back as.
    #[test]
    fn read_refuses_a_path_that_names_a_drawer() {
        let path = disc("read-wrong-kind");
        let mut source = CdSource::open(&path).unwrap();

        let err = source.read("OS-Version3.9/Workbench3.5/C").unwrap_err();
        assert!(err.to_string().contains("not a file"), "{err}");
    }
}
