//! The package's own files, unpacked to a host directory ART can mount.
//!
//! **This module exists because of ART-185.** [`run::media_for`] mounted the
//! distribution tree and ART's own work volume, and nothing anywhere put the
//! package's `Updater` where the generated script could reach it. A BoingBag's
//! wrapper cannot simply be placed into the tree beforehand — *not being
//! placeable on the host is the whole reason this round exists* (ART-166) — so
//! the wrapper is unpacked to a directory of ART's own and that directory
//! becomes the third mount.
//!
//! [`run::media_for`]: super::run::media_for
//!
//! ## Nothing here decrypts anything
//!
//! The wrapper is **plain LHA** and ART has always read it. What is encrypted
//! is the payload archive *inside* it — `AmigaOS-Update`, ZipCrypto, every one
//! of its 233 entries (measured in the content-layer round, three independent
//! ways) — and that stays encrypted: it is copied out as the opaque blob it
//! is, and the package's own `Updater` decrypts it on the Amiga, which is this
//! design's arrangement from the start and the owner's recorded decision. No
//! protection is bypassed and none is examined.
//!
//! ## What was measured, and what the fixtures therefore look like
//!
//! Read with 7-Zip 26.02 on 2026-08-21, against the owner's own archives in
//! `E:\amiga\Amigatolon\paketler`:
//!
//! ```text
//! BoingBag39-1.lha        1112 entries   1111 under `BoingBag3.9-1\`
//!                                          1 `BoingBag3.9-1.info`
//! BoingBag39-2.lha         112 entries    111 under `BoingBag3.9-2\`
//!                                          1 `BoingBag3.9-2.info`
//! Euro-Update.lha          106 entries    105 under `Euro-Update\`
//!                                          1 `Euro-Update.info`
//! ```
//!
//! Three facts fall out of that listing and every fixture in this file carries
//! all three, because a fixture tidier than the real thing is a test that
//! passes against the defect it was written for:
//!
//! 1. **The top level is not one directory.** It is a drawer *and its icon* —
//!    a plain file sitting beside it. So "the archive's single top-level
//!    entry" is not a thing that can be read, and nothing here tries to.
//! 2. **There are no directory entries at all.** Every row is a file with a
//!    `/`-bearing name; the drawers exist only implicitly. The extraction gate
//!    creates the parents, and [`unpack`] therefore asks the *filesystem*
//!    whether the drawer arrived rather than asking the archive's index.
//! 3. **Entry names are not ASCII.** `C\Catalogs\türkçe\Updater.catalog` and
//!    `português-brasil` are real rows. Nothing here may slice a `&str` by
//!    byte offset.
//!
//! ## What is *not* read from the archive
//!
//! The drawer's name and the installer's path inside it are **not** taken from
//! the archive. They are the shipped recipe's (`media` and
//! `amiga_installer.program` — see [`crate::core::osinstall::package`]), or a
//! caller's override which the command layer has already put through its four
//! gates. The design's rule is that nothing ART generates is assembled from a
//! string ART did not author, and the drawer name reaches a generated
//! AmigaDOS script.
//!
//! What the archive *is* asked is whether those names are true of it — after
//! the extraction, of the files that actually arrived. That is this project's
//! "ask the artefact what it is; never infer it" rule, and it is what turns
//! ART-185's silent shape into a refusal: an archive that carries no such
//! drawer, or a drawer that carries no such installer, is refused **before**
//! the emulator starts rather than discovered as a `CD` that failed and an
//! answer of "the installer said no" about a program that never ran.

use std::path::{Path, PathBuf};

use crate::core::archive::extract::{extract_with_backend, OverwritePolicy};
use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;
use crate::core::security::safe_join;

/// How many names a refusal lists when it says what the archive actually
/// carried. Enough to recognise a wrong archive, bounded so that a hostile one
/// with a hundred thousand entries cannot write its whole index into an error
/// message the UI then renders.
const NAMES_IN_A_REFUSAL: usize = 8;

/// What arrived, for the report and the log.
///
/// `refused` carries the entries the extraction gate would not write — a
/// traversal name, an over-sized claim, a declared size that was a lie. They
/// are **reported and not fatal**, exactly as every other extraction in ART
/// treats them: the guarantee that matters is that none of them was written,
/// and that guarantee is absolute whether or not this module reacts. What
/// would be dishonest is unpacking an archive that tried and saying nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unpacked {
    /// The directory the package now sits in — the host side of the mount.
    pub root: PathBuf,
    pub files: usize,
    pub bytes: u64,
    pub refused: Vec<String>,
}

/// Unpack `archive` into `into`, and prove the package is actually in there.
///
/// `drawer` is the package's own drawer inside the wrapper (`BoingBag3.9-1`),
/// `/`-separated, or `None` for an archive whose files are at its root.
/// `installer` is the program's path **inside that drawer** (`C/Updater`).
/// Both are the recipe's or the command layer's, never the archive's — see the
/// module documentation.
///
/// `into` must be absent or **empty**. That is not tidiness: this writes a
/// whole archive, and a mistyped path pointing at something of the user's
/// would scatter a BoingBag through it. The same rule and the same reasoning
/// as [`super::workvol::build`].
pub fn unpack(
    archive: &Path,
    into: &Path,
    drawer: Option<&str>,
    installer: &str,
    sink: &dyn ProgressSink,
) -> CoreResult<Unpacked> {
    if !archive.is_file() {
        return Err(CoreError::InvalidInput(format!(
            "running a package's own installer needs the package's own archive; '{}' is not a \
             file",
            archive.display()
        )));
    }

    if into.exists() {
        let mut entries = std::fs::read_dir(into)?;
        if entries.next().is_some() {
            return Err(CoreError::SafetyRefused(format!(
                "'{}' already has contents; a package is unpacked into an empty directory",
                into.display()
            )));
        }
    }

    // One gate, and it is not this module's. `core::archive::extract` owns
    // `safe_join`, the total and per-entry output caps, the entry cap, the
    // declared-size check and the overwrite policy, for every format ART
    // reads. A second extractor here would be a second copy of five defences,
    // and the copy is where the hole would be (`core::archive`'s own module
    // documentation says exactly this).
    //
    // `Skip` rather than `Overwrite`: `into` was just proved empty, so nothing
    // can legitimately be in the way, and an entry that collides with one
    // already written is an archive naming the same path twice — which is
    // reported rather than resolved by letting the later one win.
    let mut backend = crate::core::archive::open(archive)?;
    let outcome = extract_with_backend(backend.as_mut(), into, OverwritePolicy::Skip, sink)?;

    // A partial unpack is the ART-185 shape wearing a different hat: the run
    // would start, the drawer might even be there, and whichever file the cap
    // cut off would be missing at the moment the Amiga reached for it. There
    // is no honest way to report that afterwards, so it is refused here.
    if outcome.aborted {
        return Err(CoreError::SafetyRefused(format!(
            "'{}' was not unpacked whole and a partly-unpacked package must not be run: {}",
            archive.display(),
            outcome
                .abort_reason
                .unwrap_or_else(|| "the extraction stopped".to_string())
        )));
    }

    let refused: Vec<String> = outcome
        .extracted
        .iter()
        .filter(|e| e.skipped)
        .map(|e| match &e.reason {
            Some(reason) => format!("{}: {reason}", e.source_path),
            None => e.source_path.clone(),
        })
        .collect();

    // Ask what arrived, rather than trusting either the recipe or the archive
    // index. `is_dir` follows the real filesystem, which is also the thing the
    // mount will show the Amiga — so this is the same question the emulator
    // would ask, asked before the emulator is started.
    let drawer_dir = match drawer {
        Some(drawer) => resolve(into, drawer)?,
        None => into.to_path_buf(),
    };
    if !drawer_dir.is_dir() {
        return Err(CoreError::InvalidInput(format!(
            "'{}' carries no '{}' drawer, so it is not the archive this package's installer \
             lives in; it holds {}",
            archive.display(),
            drawer.unwrap_or(""),
            what_it_holds(into)
        )));
    }

    // And the installer itself. Without this the run reaches the emulator, the
    // shell fails to find the program, `If Warn` writes `failed`, and ART
    // tells the user the installer said no about a program that never started
    // — ART-185's own sentence, arriving from one directory deeper.
    let program = resolve(&drawer_dir, installer)?;
    if !program.is_file() {
        return Err(CoreError::InvalidInput(format!(
            "'{}' carries no '{installer}' inside its '{}' drawer, so there is nothing for the \
             Amiga to run",
            archive.display(),
            drawer.unwrap_or("")
        )));
    }

    Ok(Unpacked {
        root: into.to_path_buf(),
        files: outcome.total_files,
        bytes: outcome.total_bytes,
        refused,
    })
}

/// A package-relative path, resolved under `root` through the same gate an
/// archive entry name goes through.
///
/// `safe_join` and not `Path::join`, even though these strings are ART's own:
/// a caller-supplied `package_dir` reaches here, and "it was validated
/// upstream" is the sentence in front of every traversal this project has
/// fixed. It costs one call and it makes the containment true here rather than
/// true elsewhere.
fn resolve(root: &Path, relative: &str) -> CoreResult<PathBuf> {
    safe_join(root, relative).map_err(|err| {
        CoreError::SafetyRefused(format!(
            "'{relative}' is not a path inside the package: {err}"
        ))
    })
}

/// The first few names directly inside `dir`, for a refusal that tells the
/// user which archive they actually pointed at.
fn what_it_holds(dir: &Path) -> String {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return "nothing that could be read".to_string();
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    if names.is_empty() {
        return "nothing".to_string();
    }
    names.sort();
    let more = names.len().saturating_sub(NAMES_IN_A_REFUSAL);
    names.truncate(NAMES_IN_A_REFUSAL);
    let listed = names.join(", ");
    if more > 0 {
        format!("{listed} and {more} more")
    } else {
        listed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::jobs::{CancelToken, NoProgress, ProgressSink};
    use crate::core::lha::tests::{make_lha_with, make_lha_with_raw_names};
    use crate::core::ScratchDir;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// ART-184: the directory removes itself on `Drop`, so a panicking test
    /// cleans up too.
    fn scratch(tag: &str) -> ScratchDir {
        ScratchDir::new("art-amigainstall-pkg", tag)
    }

    /// The **real** BoingBag layout, not a tidier one.
    ///
    /// Measured from the owner's `BoingBag39-1.lha` with 7-Zip 26.02 on
    /// 2026-08-21 and reproduced feature for feature: an icon file sitting
    /// beside the drawer at the top level, **no directory entries at all**, a
    /// non-ASCII catalog path, and the opaque encrypted payload blob next to
    /// the `Updater` that decrypts it.
    ///
    /// A fixture of `[("Pkg/C/Updater", …)]` alone would pass every test below
    /// while proving nothing about a real archive: the two things that would
    /// actually break — a sibling file at the top level, and drawers that
    /// exist only because a file name implies them — would both be absent.
    fn boingbag_shaped(drawer: &str) -> Vec<u8> {
        let icon = format!("{drawer}.info");
        let payload = format!("{drawer}/AmigaOS-Update");
        let updater = format!("{drawer}/C/Updater");
        let getlocale = format!("{drawer}/C/GetLocale");
        let install = format!("{drawer}/Install");
        let mut raw: Vec<(&[u8], &[u8])> = vec![
            (icon.as_bytes(), b"icon bytes"),
            // Stored, opaque, and never looked inside: the real one is a
            // ZipCrypto ZIP the Amiga-side Updater decrypts.
            (payload.as_bytes(), b"PK\x03\x04 encrypted payload"),
            (updater.as_bytes(), b"the updater program"),
            (getlocale.as_bytes(), b"getlocale"),
            (install.as_bytes(), b"; the package's own Installer script"),
        ];
        // `BoingBag3.9-1\C\Catalogs\türkçe\Updater.catalog`, in the Latin-1
        // the real archive stores. ART-168 was exactly this byte range.
        let mut catalog = Vec::new();
        catalog.extend_from_slice(drawer.as_bytes());
        catalog.extend_from_slice(b"/C/Catalogs/t\xFCrk\xE7e/Updater.catalog");
        raw.push((catalog.as_slice(), b"catalog"));
        make_lha_with_raw_names(&raw)
    }

    fn write_boingbag(at: &Path, drawer: &str) -> PathBuf {
        let archive = at.join(format!("{drawer}.lha"));
        std::fs::write(&archive, boingbag_shaped(drawer)).unwrap();
        archive
    }

    /// The whole point of the module: after this, the drawer and the program
    /// the script names are really on the host, so the mount has something to
    /// show the Amiga.
    #[test]
    fn a_real_shaped_wrapper_unpacks_and_its_updater_is_there() {
        let dir = scratch("happy");
        let archive = write_boingbag(dir.path(), "BoingBag3.9-1");
        let into = dir.join("pkg");

        let unpacked = unpack(
            &archive,
            &into,
            Some("BoingBag3.9-1"),
            "C/Updater",
            &NoProgress,
        )
        .unwrap();

        assert_eq!(unpacked.root, into);
        assert!(
            into.join("BoingBag3.9-1/C/Updater").is_file(),
            "the installer must be on the host, at the path the script will name"
        );
        assert!(
            into.join("BoingBag3.9-1/AmigaOS-Update").is_file(),
            "and its payload beside it, still encrypted"
        );
        assert_eq!(
            std::fs::read(into.join("BoingBag3.9-1/AmigaOS-Update")).unwrap(),
            b"PK\x03\x04 encrypted payload",
            "the payload is copied, never opened"
        );
        assert!(
            into.join("BoingBag3.9-1.info").is_file(),
            "the icon sits beside the drawer, as it does in the real archive"
        );
        assert_eq!(unpacked.files, 6);
        assert!(unpacked.refused.is_empty(), "{:?}", unpacked.refused);
    }

    /// The non-ASCII path survives. ART-168 was a name whose high-bit bytes
    /// became U+FFFD, and it survived every test in the suite because no
    /// fixture had one.
    #[test]
    fn a_latin_one_catalog_path_arrives_intact() {
        let dir = scratch("latin1");
        let archive = write_boingbag(dir.path(), "BoingBag3.9-1");
        let into = dir.join("pkg");

        unpack(
            &archive,
            &into,
            Some("BoingBag3.9-1"),
            "C/Updater",
            &NoProgress,
        )
        .unwrap();

        let catalogs: Vec<String> = std::fs::read_dir(into.join("BoingBag3.9-1/C/Catalogs"))
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(catalogs, vec!["türkçe".to_string()], "got {catalogs:?}");
    }

    /// The wrong archive is refused, and the refusal **lists what was really
    /// in it** so the user can see which one they picked.
    ///
    /// The listing is the assertion, and deliberately so. Deleting the
    /// drawer check leaves the installer check to refuse this anyway — the
    /// program cannot be a file under a directory that is not there — and a
    /// test that only asserted `is_err()`, or that the message mentioned the
    /// two names, **passed against that deletion** when it was measured. What
    /// the drawer check is actually for is the diagnosis: `Euro-Update` and
    /// `Euro-Update.info` are what the archive holds, and only this branch
    /// says so. A user told "there is no `C/Updater` in `BoingBag3.9-1`" about
    /// an archive that contains neither has been told the wrong thing about
    /// the wrong drawer.
    #[test]
    fn an_archive_without_the_packages_drawer_is_refused_and_lists_what_it_held() {
        let dir = scratch("wrong-archive");
        let archive = write_boingbag(dir.path(), "Euro-Update");
        let into = dir.join("pkg");

        let err = unpack(
            &archive,
            &into,
            Some("BoingBag3.9-1"),
            "C/Updater",
            &NoProgress,
        )
        .unwrap_err();

        let text = err.to_string();
        assert!(text.contains("BoingBag3.9-1"), "got {text}");
        assert!(
            text.contains("Euro-Update.info") && text.contains("it holds"),
            "it must list the archive's own top level, which is the whole point of \
             diagnosing the drawer separately: {text}"
        );
    }

    /// The drawer being right is not enough. This is ART-185's own sentence
    /// one directory deeper: without the check the run starts, the shell
    /// cannot find the program, and ART reports that the installer said no.
    #[test]
    fn a_drawer_without_the_installer_is_refused_before_anything_is_launched() {
        let dir = scratch("no-updater");
        let archive = dir.join("BoingBag3.9-1.lha");
        std::fs::write(
            &archive,
            make_lha_with(&[
                ("BoingBag3.9-1.info", b"icon"),
                ("BoingBag3.9-1/AmigaOS-Update", b"payload"),
                ("BoingBag3.9-1/Install", b"script"),
            ]),
        )
        .unwrap();
        let into = dir.join("pkg");

        let err = unpack(
            &archive,
            &into,
            Some("BoingBag3.9-1"),
            "C/Updater",
            &NoProgress,
        )
        .unwrap_err();

        assert!(err.to_string().contains("C/Updater"), "got {err}");
    }

    /// A hostile entry name never leaves the unpack directory, and the
    /// refusal is reported rather than swallowed.
    ///
    /// The traversal entries are given the **shape of the drawer's own
    /// files**, so a gate that refused only obviously-foreign names would
    /// still have to refuse these.
    #[test]
    fn a_traversal_entry_is_refused_and_writes_nothing_outside() {
        let dir = scratch("traversal");
        let outside = dir.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("keep.txt"), b"the user's own file").unwrap();

        let archive = dir.join("hostile.lha");
        std::fs::write(
            &archive,
            make_lha_with(&[
                ("BoingBag3.9-1.info", b"icon"),
                ("BoingBag3.9-1/C/Updater", b"the updater"),
                ("../outside/keep.txt", b"overwritten"),
                ("BoingBag3.9-1/../../outside/planted", b"planted"),
                ("C:/Windows/System32/art.dll", b"planted"),
                ("/etc/passwd", b"planted"),
            ]),
        )
        .unwrap();
        let into = dir.join("pkg");

        let unpacked = unpack(
            &archive,
            &into,
            Some("BoingBag3.9-1"),
            "C/Updater",
            &NoProgress,
        )
        .unwrap();

        assert_eq!(
            std::fs::read(outside.join("keep.txt")).unwrap(),
            b"the user's own file",
            "nothing outside the unpack directory may be touched"
        );
        assert!(!outside.join("planted").exists());
        assert!(
            unpacked.refused.len() >= 3,
            "and the refusals must be reported, not swallowed: {:?}",
            unpacked.refused
        );
    }

    /// A hostile **drawer name** cannot reach out of the unpack directory.
    ///
    /// **The escape target really exists**, and that is what makes this
    /// mutation-proof. A test that only asserted `is_err()` passed with
    /// `safe_join` swapped for `Path::join` — `into/../../Windows` is
    /// nowhere, so the `is_dir` check refused it anyway and the traversal
    /// guard could have been deleted unnoticed. Here `../outside` is a real
    /// directory carrying a real `C/Updater`, so `Path::join` would resolve
    /// it, find the program, and hand the run a mount **outside** the
    /// directory ART unpacked into. Only the gate refuses it.
    #[test]
    fn a_drawer_that_leaves_the_unpack_directory_is_refused_even_when_it_exists() {
        let dir = scratch("hostile-drawer");
        let archive = write_boingbag(dir.path(), "BoingBag3.9-1");

        // The place a traversal would land, fully furnished.
        std::fs::create_dir_all(dir.join("outside").join("C")).unwrap();
        std::fs::write(dir.join("outside").join("C").join("Updater"), b"not ours").unwrap();

        for (n, hostile) in [
            "../outside",
            "BoingBag3.9-1/../../outside",
            "../../Windows",
            "C:/Windows",
            "/Windows",
            "   ",
        ]
        .into_iter()
        .enumerate()
        {
            let into = dir.join(format!("pkg-{n}"));
            let err = unpack(&archive, &into, Some(hostile), "C/Updater", &NoProgress);
            assert!(
                matches!(
                    err,
                    Err(CoreError::SafetyRefused(_)) | Err(CoreError::InvalidInput(_))
                ),
                "'{hostile}' must be refused, got {err:?}"
            );
        }
    }

    /// The same for the installer's own path, and the same reason for the
    /// furnished escape target: a recipe or an override that climbed out of
    /// the drawer would be naming a program the package does not carry, and
    /// with `Path::join` it would find one.
    #[test]
    fn an_installer_path_that_leaves_the_drawer_is_refused_even_when_it_exists() {
        let dir = scratch("hostile-installer");
        let archive = write_boingbag(dir.path(), "BoingBag3.9-1");
        std::fs::write(dir.join("Updater"), b"not ours").unwrap();

        for (n, hostile) in [
            // `into/BoingBag3.9-1/../../Updater` → `dir/Updater`, which is
            // really there.
            "../../Updater",
            "C:/Windows/System32/cmd.exe",
            "/bin/sh",
            "   ",
        ]
        .into_iter()
        .enumerate()
        {
            let into = dir.join(format!("pkg-{n}"));
            let err = unpack(&archive, &into, Some("BoingBag3.9-1"), hostile, &NoProgress);
            assert!(
                matches!(
                    err,
                    Err(CoreError::SafetyRefused(_)) | Err(CoreError::InvalidInput(_))
                ),
                "'{hostile}' must be refused, got {err:?}"
            );
        }
    }

    /// A directory with anything in it is never written into — the same rule
    /// as `workvol::build`, and for the same reason: a mistyped path pointing
    /// at the user's own tree would scatter a BoingBag through it.
    #[test]
    fn a_directory_with_contents_is_refused_rather_than_unpacked_into() {
        let dir = scratch("not-empty");
        let archive = write_boingbag(dir.path(), "BoingBag3.9-1");
        let into = dir.join("theirs");
        std::fs::create_dir_all(&into).unwrap();
        std::fs::write(into.join("Startup-Sequence"), b"the user's own").unwrap();

        let err = unpack(
            &archive,
            &into,
            Some("BoingBag3.9-1"),
            "C/Updater",
            &NoProgress,
        )
        .unwrap_err();

        assert!(matches!(err, CoreError::SafetyRefused(_)), "got {err:?}");
        assert_eq!(
            std::fs::read(into.join("Startup-Sequence")).unwrap(),
            b"the user's own",
            "and it is left exactly as it was"
        );
        assert_eq!(
            std::fs::read_dir(&into).unwrap().count(),
            1,
            "nothing was unpacked into it"
        );
    }

    /// An archive too big for the gate to unpack whole is refused, not run
    /// half-unpacked.
    ///
    /// A partially unpacked package is ART-185 wearing a different hat: the
    /// drawer might well be there, and whichever file the cap cut off would be
    /// missing at the moment the Amiga reached for it — reported afterwards as
    /// the installer having said no. `MAX_ENTRIES` is the cheapest of the
    /// gate's caps to reach honestly, and it aborts before anything is
    /// written at all, which is also asserted.
    #[test]
    fn an_archive_the_gate_will_not_unpack_whole_is_refused_rather_than_run() {
        use crate::core::archive::extract::MAX_ENTRIES;

        let dir = scratch("too-many");
        let names: Vec<String> = (0..=MAX_ENTRIES)
            .map(|n| format!("BoingBag3.9-1/f{n}"))
            .collect();
        let entries: Vec<(&str, &[u8])> =
            names.iter().map(|n| (n.as_str(), b"x" as &[u8])).collect();
        let archive = dir.join("huge.lha");
        std::fs::write(&archive, make_lha_with(&entries)).unwrap();
        let into = dir.join("pkg");

        let err = unpack(
            &archive,
            &into,
            Some("BoingBag3.9-1"),
            "C/Updater",
            &NoProgress,
        )
        .unwrap_err();

        assert!(matches!(err, CoreError::SafetyRefused(_)), "got {err:?}");
        assert!(
            err.to_string().contains("whole"),
            "the refusal must say why: {err}"
        );
        assert_eq!(
            std::fs::read_dir(&into).unwrap().count(),
            0,
            "and nothing may have been written"
        );
    }

    /// A missing archive is a sentence, not an unpack of nothing that then
    /// fails somewhere less obvious.
    #[test]
    fn a_missing_archive_is_refused_by_name() {
        let dir = scratch("missing");
        let err = unpack(
            &dir.join("nowhere.lha"),
            &dir.join("pkg"),
            Some("BoingBag3.9-1"),
            "C/Updater",
            &NoProgress,
        )
        .unwrap_err();
        assert!(err.to_string().contains("nowhere.lha"), "got {err}");
    }

    /// Cancellation reaches through: the extraction gate checks between whole
    /// entries and this must not swallow the answer.
    #[test]
    fn a_cancelled_unpack_stops_and_says_so() {
        struct StopAfter {
            seen: AtomicU32,
            after: u32,
            token: CancelToken,
        }
        impl ProgressSink for StopAfter {
            fn report(&self, _d: u64, _t: Option<u64>, _m: &str) {}
            fn is_cancelled(&self) -> bool {
                if self.seen.fetch_add(1, Ordering::Relaxed) + 1 > self.after {
                    self.token.cancel();
                }
                self.token.is_cancelled()
            }
        }

        let dir = scratch("cancel");
        let archive = write_boingbag(dir.path(), "BoingBag3.9-1");
        let into = dir.join("pkg");

        let sink = StopAfter {
            seen: AtomicU32::new(0),
            after: 2,
            token: CancelToken::new(),
        };
        let err = unpack(&archive, &into, Some("BoingBag3.9-1"), "C/Updater", &sink).unwrap_err();

        assert!(matches!(err, CoreError::Cancelled), "got {err:?}");
    }

    /// A package whose files are at the wrapper's root is expressible, and the
    /// installer check still applies to it.
    #[test]
    fn a_package_with_no_drawer_uses_the_unpack_root() {
        let dir = scratch("no-drawer");
        let archive = dir.join("flat.lha");
        std::fs::write(
            &archive,
            make_lha_with(&[("C/Updater", b"updater"), ("Readme", b"about")]),
        )
        .unwrap();
        let into = dir.join("pkg");

        let unpacked = unpack(&archive, &into, None, "C/Updater", &NoProgress).unwrap();
        assert_eq!(unpacked.root, into);
        assert!(into.join("C/Updater").is_file());

        let other = dir.join("pkg2");
        assert!(
            unpack(&archive, &other, None, "C/Installer", &NoProgress).is_err(),
            "the installer must still have to be there"
        );
    }
}
