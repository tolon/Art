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
//! ## Protection and comment: read when the disc records them (ART-078)
//!
//! This module used to say plain ISO9660 "has nowhere to keep an AmigaDOS
//! protection byte or a file comment". That is true of ISO9660 and false of
//! an Amiga-mastered disc: both live in the **Amiga `AS` System Use entry**,
//! which `core::iso::susp` now reads. Two of the four discs measured for
//! ART-078 carry one on every file — the owner's AmigaOS 3.9 CD and Amiga
//! Developer CD v2.1, 44 796 entries between them
//! (`scripts/iso-susp-census.py`).
//!
//! So [`MediaEntry::protection`] and [`MediaEntry::comment`] are **read off
//! the disc when the disc recorded them**, and what
//! `apply.rs::settle_sidecar` writes into the `.uaem` beside each file and
//! records in `distribution.json` is that measured value rather than a
//! declared default. That is the point of the change: `apply.rs`'s own doc
//! says a manifest must "never state something said", and the manifest can
//! now say what the medium said.
//!
//! When a disc carries no `AS` entry — the AmigaOS 3.2 CD in the same
//! folder carries no System Use Area at all — the fields stay
//! [`default_protection`] (`----rwed`, i.e. `0`) and `String::new()` as
//! **declared defaults**, on the same footing `AdfSource::root_entry` gives
//! its own unmeasurable fields. A caller must not treat *those* as evidence
//! about the original file (§89). The two cases are not distinguished in
//! `MediaEntry` itself, because AmigaDOS has no third state between "these
//! bits" and "no bits"; what distinguishes them is the disc, and the disc is
//! recorded in the manifest beside the value.
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

use super::source::{starts_with_ignoring_case, MediaEntry, MediaSource};

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
    ///
    /// **Refused as [`CoreError::LimitExceeded`], not `Malformed`** —
    /// ART-158. Both refusals used to carry `ART-FORMAT-MALFORMED`, which is
    /// the identifier for "this file is not what it claims to be". A disc
    /// past these caps is exactly what it claims to be; it is ART that stops
    /// early, and the two failures want opposite answers from whoever reads
    /// them ("this medium cannot be used" against "ART's own limit, which
    /// could be raised"). `IsoImage::open`'s own errors above still come back
    /// as `Malformed`, because a disc that fails *there* really is broken.
    pub fn open(path: &Path) -> CoreResult<Self> {
        let image = IsoImage::open(path)?;
        let walk = image.walk()?;
        // `LimitExceeded`, not `Malformed` — ART-158. Such a disc is
        // **valid**; it is ART that stops early, and telling the user their
        // disc is broken would be telling them the wrong thing about their
        // own material (§89). `Malformed` stays for a disc that really is.
        if walk.truncated {
            return Err(CoreError::LimitExceeded {
                subject: "iso9660 walk".into(),
                detail: format!(
                    "this disc holds more than {MAX_WALK_ENTRIES} entries; ART stopped \
                     reading before it reached the end of the tree, so an install plan \
                     built from it would be missing files without saying so"
                ),
            });
        }
        if walk.depth_limited {
            return Err(CoreError::LimitExceeded {
                subject: "iso9660 walk".into(),
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
    /// One walked disc entry as a [`MediaEntry`].
    ///
    /// `protection` and `comment` come from the entry's Amiga `AS` System
    /// Use entry when it has one — see this module's own doc on why that is
    /// a measured value and not a declared default any more.
    fn to_media_entry(walked: &IsoWalkEntry) -> MediaEntry {
        MediaEntry {
            path: walked.path.clone(),
            is_dir: walked.entry.is_dir,
            size: walked.entry.bytes,
            protection: walked.entry.protection.unwrap_or_else(default_protection),
            date: walked.entry.date.map(amiga_from_unix).unwrap_or_default(),
            comment: walked.entry.comment.clone().unwrap_or_default(),
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

    /// Resolve `normalized` against `entries` — an exact match first,
    /// falling back to a case-insensitive one only when nothing matches
    /// exactly.
    ///
    /// A recipe's `from`/`to` values are AmigaDOS paths, and AmigaDOS
    /// filesystems are case-insensitive: `AdfSource::resolve` goes through
    /// `core::volume::write::dir::find_entry`, which lowercases both sides
    /// before comparing, "as AmigaDOS is (ART-012)". `CdSource` matched only
    /// the exact byte case until this task — invisible against the
    /// uppercase-only Primary tree a disc mastered without Joliet produces
    /// (matching the recipe's own now-uppercase `from` values), but wrong
    /// the moment a disc *is* pressed with Joliet: `IsoImage`'s own
    /// `scan_descriptors` prefers Joliet when present ("Joliet wins when it
    /// is there"), and a Joliet tree carries names in their natural mixed
    /// case (`OS-Version3.9`, not `OS-VERSION3.9`) — exactly the disc-vs-tree
    /// mismatch ART-155 hit in reverse, unguarded, before this fix.
    ///
    /// Exact-first, not case-insensitive-only: ISO9660 puts no case-folding
    /// rule on Joliet names, so a disc can legitimately hold two entries at
    /// the same level differing only in case, and a query that spells one of
    /// them exactly should never be answered with the other.
    fn find_by_path<'a>(entries: &'a [IsoWalkEntry], normalized: &str) -> Option<&'a IsoWalkEntry> {
        entries.iter().find(|e| e.path == normalized).or_else(|| {
            // International, not ASCII-only (fix round 1, F1) — this disc's
            // own `LOCALE/CATALOGS/TÜRKÇE` is exactly the case at issue.
            entries
                .iter()
                .find(|e| super::amiga_names_equal(&e.path, normalized))
        })
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
        Ok(Self::find_by_path(&self.entries, &normalized).map(Self::to_media_entry))
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
        // Nothing on the disc by that name: an empty `Vec`, not an error —
        // the trait's own contract, and the same answer `AdfSource::walk`
        // gives for a path it cannot resolve.
        let Some(found) = Self::find_by_path(&self.entries, &normalized) else {
            return Ok(Vec::new());
        };
        // A path naming a *file* is refused, word for word as
        // `AdfSource::walk` refuses it. Without this the walk answered
        // `Ok(vec![])` — indistinguishable from an empty drawer, and from a
        // missing path — so a `Subtree` rule aimed at a file by mistake
        // would produce a silently short plan on a disc where the identical
        // mistake raises on a floppy. Two implementations of one trait must
        // answer one question one way (this is the third such divergence
        // found on this branch; see `source_contract.rs`, which now asks
        // both of them the same questions).
        if !found.entry.is_dir {
            return Err(CoreError::InvalidInput(format!(
                "'{path}' is a file on this media, not a drawer"
            )));
        }
        // The prefix is built from the entry's *own* casing, so every path
        // in the answer is the disc's from end to end (the trait's casing
        // rule; `AdfSource` and `ArchiveSource` do the same).
        let base = found.path.clone();
        // Strict descendants only, matching `AdfSource::walk`: the
        // directory named by `path` is never included in its own listing,
        // only what is inside it — `core::osinstall::plan` relies on this
        // exact shape ("`walk` yields only what is *inside* `from`, never
        // `from` itself").
        let prefix = format!("{base}/");
        // Case-**insensitively**, matching `find_by_path`'s own resolution
        // and `ArchiveSource::walk`'s prefix test.
        //
        // **M2 of the final whole-branch review, and a reversal of what
        // stood here.** The exact-case filter had a real argument behind
        // it: ISO9660 puts no case-folding rule on Joliet names, so a disc
        // can legitimately hold `Locale` and `locale` at one level, and an
        // exact prefix walks precisely the subtree that was resolved.
        // `ArchiveSource` argued the opposite way, in its own file, just as
        // convincingly. Two implementations of one trait answering one
        // question two ways is the condition `source_contract.rs` exists to
        // make impossible, so it is settled rather than left balanced.
        //
        // Folded wins for two reasons. A rule's `from` names an **AmigaDOS**
        // drawer, and the drawer AmigaDOS would see if this disc were copied
        // is the union of both spellings — not whichever one `find_by_path`
        // happened to land on. And the two failure modes are not
        // symmetrical: over-including is loud (the extra files appear in the
        // plan, and a genuine clash between them raises
        // `DestinationCollision`, which folds case itself), while
        // under-including is silent — a short walk, a short plan, a file
        // missing from the volume with nothing said about it (§89).
        Ok(self
            .entries
            .iter()
            .filter(|e| starts_with_ignoring_case(&e.path, &prefix))
            .map(Self::to_media_entry)
            .collect())
    }

    fn read(&mut self, path: &str) -> CoreResult<Vec<u8>> {
        let normalized = Self::normalized(path);
        // `""` names the disc's own root, which is never one of
        // `self.entries` — so without this it fell through to "is not on
        // this media", while `AdfSource::read("")` resolves to the root
        // block and answers "is a drawer … not a file". Same question, same
        // answer: the root of any media is a drawer, and it is *present*.
        if normalized.is_empty() {
            return Err(CoreError::InvalidInput(format!(
                "'{path}' is a drawer on this media, not a file"
            )));
        }
        let found = Self::find_by_path(&self.entries, &normalized)
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
        let folder = std::env::temp_dir().join(format!(
            "art-cdsource-{dirname}-{}",
            crate::core::test_scratch_id()
        ));
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

    /// **ART-158.** A disc nested deeper than `MAX_WALK_DEPTH` is refused
    /// as a **limit**, not as a malformed disc.
    ///
    /// It is a perfectly valid ISO9660 image — `IsoImage::open` reads it, and
    /// `walk()` returns `Ok` with what it found — so `ART-FORMAT-MALFORMED`
    /// was telling the user their own disc was broken when the truth is that
    /// ART stops descending at 16 levels. The identifier is asserted, not
    /// just the variant: `code()` is what a user quotes and a maintainer
    /// searches for.
    #[test]
    fn a_disc_deeper_than_art_will_walk_is_a_limit_not_a_malformed_disc() {
        let folder = std::env::temp_dir().join(format!(
            "art-cdsource-toodeep-{}-{}",
            crate::core::test_scratch_id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();

        // Twenty levels, past MAX_WALK_DEPTH's sixteen.
        let mut node = dir("L20", "L20", vec![file("LEAF.;1", "Leaf", b"deep")]);
        for level in (1..20).rev() {
            node = dir(&format!("L{level}"), &format!("L{level}"), vec![node]);
        }
        let bytes = IsoBuilder {
            volume: "DEEP".to_string(),
            joliet_volume: "DEEP".to_string(),
            joliet: true,
            children: vec![node],
            ..Default::default()
        }
        .build();
        let path = folder.join("deep.iso");
        std::fs::write(&path, bytes).unwrap();

        let err = CdSource::open(&path).unwrap_err();
        assert_eq!(err.code(), "ART-LIMIT-EXCEEDED");
        assert!(
            matches!(err, CoreError::LimitExceeded { .. }),
            "a valid disc past ART's cap is not malformed: {err}"
        );
        let msg = format!("{err}");
        assert!(msg.contains(&MAX_WALK_DEPTH.to_string()), "{msg}");

        std::fs::remove_dir_all(&folder).ok();
    }

    /// The other half of ART-158's boundary: a disc that really is damaged
    /// is still `Malformed`, so the two classes are apart rather than one
    /// having swallowed the other.
    ///
    /// Damaged by truncation, which is a real way a disc image arrives
    /// broken: the descriptors still read, the root directory record still
    /// says where its extent is, and that extent is now past the end of the
    /// file.
    #[test]
    fn a_disc_that_is_damaged_is_still_malformed() {
        let path = disc("truncated");
        let bytes = std::fs::read(&path).unwrap();
        // Sector 16 is the Primary descriptor, 17 the Joliet one, 18 the
        // terminator — keep those and cut everything the root points at.
        std::fs::write(&path, &bytes[..19 * 2048]).unwrap();

        let err = CdSource::open(&path).unwrap_err();
        assert_eq!(err.code(), "ART-FORMAT-MALFORMED", "{err}");
        assert!(
            !matches!(err, CoreError::LimitExceeded { .. }),
            "a damaged disc is not a disc past a limit"
        );
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
        let folder = std::env::temp_dir().join(format!(
            "art-cdsource-joliet-{}",
            crate::core::test_scratch_id()
        ));
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

    // ---- whole-branch review: findings 2 and 5 ----

    /// The third divergence. `AdfSource::walk` refuses a file by name;
    /// `CdSource::walk` used to answer `Ok(vec![])` — indistinguishable from
    /// an empty drawer and from a missing path, so a `Subtree` rule aimed at
    /// a file by mistake would produce a silently short plan on a disc where
    /// the identical mistake raises on a floppy. See `source_contract.rs`,
    /// which now asks both implementations this and seven other questions.
    #[test]
    fn walk_refuses_a_path_that_names_a_file() {
        let path = disc("walk-a-file");
        let mut source = CdSource::open(&path).unwrap();

        let err = source
            .walk("OS-Version3.9/Workbench3.5/C/List")
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("is a file on this media, not a drawer"),
            "{err}"
        );
    }

    /// `read("")` names the media's own root, which is a drawer and is
    /// *present* — the answer `AdfSource::read("")` gives. `CdSource` used
    /// to say "is not on this media", which is a different fact about the
    /// same call.
    #[test]
    fn read_of_the_root_says_it_is_a_drawer_not_that_it_is_absent() {
        let path = disc("read-root");
        let mut source = CdSource::open(&path).unwrap();

        let err = source.read("").unwrap_err();
        assert!(
            err.to_string()
                .contains("is a drawer on this media, not a file"),
            "{err}"
        );
    }

    // ---- Task 5 fix round 1: review item 1 (case-insensitive resolution) ----

    /// A recipe's `from` is an AmigaDOS path, and AmigaDOS is
    /// case-insensitive — the same rule `AdfSource::resolve` already follows
    /// (`core::volume::write::dir::find_entry`, ART-012). Before this fix,
    /// `CdSource` matched only the exact byte case, which happened to be
    /// invisible against the shipped 3.9 recipe's own uppercase `from`
    /// values read against a disc mastered *without* Joliet (uppercase
    /// Primary tree, matching case by construction) but would refuse
    /// everything the moment a disc carries the same names in mixed case —
    /// a Joliet-pressed disc, this fixture, and `entry`/`read`/`walk` all
    /// three, since a directory's own case has to resolve before its
    /// contents can be walked.
    #[test]
    fn a_mixed_case_path_resolves_case_insensitively() {
        let folder = std::env::temp_dir().join(format!(
            "art-cdsource-case-fallback-{}",
            crate::core::test_scratch_id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let bytes = IsoBuilder {
            volume: "AMIGAOS39".to_string(),
            joliet_volume: "AmigaOS3.9".to_string(),
            joliet: true,
            children: vec![dir(
                "STORAGE",
                "Storage",
                vec![file("AUX.;1", "Aux", b"aux-bytes")],
            )],
            ..Default::default()
        }
        .build();
        let path = folder.join("os39.iso");
        std::fs::write(&path, bytes).unwrap();

        let mut source = CdSource::open(&path).unwrap();
        // The disc's own (Joliet) tree spells this `Storage/Aux`; a `from`
        // written in the recipe's own uppercase convention must still find
        // it, through all three trait methods.
        let entry = source
            .entry("STORAGE/AUX")
            .unwrap()
            .expect("case-insensitive fallback should have found it");
        assert_eq!(entry.path, "Storage/Aux", "the real, on-disc casing");
        assert_eq!(source.read("STORAGE/AUX").unwrap(), b"aux-bytes");
        let walked = source.walk("STORAGE").unwrap();
        assert!(walked.iter().any(|e| e.path == "Storage/Aux"), "{walked:?}");

        let _ = std::fs::remove_dir_all(&folder);
    }

    /// The fallback must never shadow an exact match: a disc that
    /// legitimately holds two entries differing only in case (ISO9660 puts
    /// no case-folding rule on Joliet names) has to answer a query naming
    /// one of them exactly with *that* one, not whichever the
    /// case-insensitive search happens to reach first.
    #[test]
    fn an_exact_case_match_wins_over_a_differently_cased_entry() {
        let folder = std::env::temp_dir().join(format!(
            "art-cdsource-exact-wins-{}",
            crate::core::test_scratch_id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let bytes = IsoBuilder {
            volume: "AMIGAOS39".to_string(),
            joliet_volume: "AmigaOS3.9".to_string(),
            joliet: true,
            children: vec![
                file("SAME1.;1", "Same", b"exact-bytes"),
                file("SAME2.;1", "SAME", b"other-bytes"),
            ],
            ..Default::default()
        }
        .build();
        let path = folder.join("os39.iso");
        std::fs::write(&path, bytes).unwrap();

        let mut source = CdSource::open(&path).unwrap();
        let entry = source.entry("Same").unwrap().unwrap();
        assert_eq!(entry.path, "Same", "the exactly-cased entry, not \"SAME\"");
        assert_eq!(source.read("Same").unwrap(), b"exact-bytes");

        let _ = std::fs::remove_dir_all(&folder);
    }

    /// **M2, the half `source_contract.rs` cannot ask.** A Joliet disc may
    /// legitimately hold `Locale` and `locale` side by side; a real
    /// AmigaDOS volume cannot (`dir::ensure_available` refuses the second
    /// name), so this question is only answerable by the two media that can
    /// express it, and it is pinned here and in
    /// `source_archive.rs::a_drawer_spelled_two_ways_walks_as_one_drawer`.
    ///
    /// A rule's `from` names an **AmigaDOS** drawer, and the drawer
    /// AmigaDOS would see if this disc were copied is both of them. `walk`
    /// used to filter descendants with an exact-case `starts_with` built
    /// from whichever spelling `find_by_path` resolved, so half the drawer
    /// silently disappeared from the plan — while `ArchiveSource`, asked
    /// the identical question, answered with the union.
    ///
    /// Note what does *not* change: `entry` and `read` still reach each
    /// spelling exactly (the test above), because resolution is
    /// exact-match-first. Only containment is folded.
    #[test]
    fn two_drawers_differing_only_in_case_are_one_drawer_to_amigados() {
        let folder = std::env::temp_dir().join(format!(
            "art-cdsource-fold-walk-{}",
            crate::core::test_scratch_id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let bytes = IsoBuilder {
            volume: "AMIGAOS39".to_string(),
            joliet_volume: "AmigaOS3.9".to_string(),
            joliet: true,
            children: vec![
                dir("LOCALE1", "Locale", vec![file("ONE.;1", "One", b"one")]),
                dir("LOCALE2", "locale", vec![file("TWO.;1", "Two", b"two")]),
            ],
            ..Default::default()
        }
        .build();
        let path = folder.join("os39.iso");
        std::fs::write(&path, bytes).unwrap();

        let mut source = CdSource::open(&path).unwrap();
        for asked in ["Locale", "locale", "LOCALE"] {
            let mut walked: Vec<String> = source
                .walk(asked)
                .unwrap()
                .into_iter()
                .map(|e| e.path)
                .collect();
            walked.sort();
            assert_eq!(
                walked,
                vec!["Locale/One".to_string(), "locale/Two".to_string()],
                "walk({asked:?}) must list the whole drawer AmigaDOS would see"
            );
        }

        let _ = std::fs::remove_dir_all(&folder);
    }
}
