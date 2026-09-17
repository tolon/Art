//! The PFS3 driver ART writes into a card's RDB — found loose or inside
//! `pfs3aio.lha`, its version read from its own `$VER:` (design § 5 phase 4).
//!
//! A PFS3 partition needs the driver's *bytes* embedded in the RDB
//! (`core/rdbedit`), and this module is only the part before that: deciding
//! **which** file those bytes come from. Aminet ships the driver as
//! `pfs3aio.lha` (`disk/misc/pfs3aio`) holding one member, `pfs3aio/pfs3aio`;
//! some material folders instead carry the driver already unpacked, loose.
//! Both are read the same way material always is here — by what the file
//! itself says, not by trusting a name — so the version compared across
//! candidates is the one `core::amigaver` reads out of the driver's own
//! `$VER:` string, never a filename or a folder's date.
//!
//! **Ordering (R4, spec brief step 1).** `explicit` — the user's own choice
//! — wins outright and is not compared against anything. Otherwise every
//! material folder is searched in order: a loose `pfs3aio` at the folder's
//! top level, then under `L/`, then any archive whose name starts `pfs3aio`
//! (case-insensitive) holding a member whose last path segment is
//! `pfs3aio`. Among everything found, the highest `$VER:` wins; a file read
//! successfully but carrying no `$VER:` at all is skipped rather than
//! treated as version `0.0`, because a driver that cannot state its own
//! version cannot honestly outrank one that can, or lose to one either — it
//! simply is not a candidate (see `the_highest_version_wins_and_an_unversioned_file_is_skipped`).
//!
//! **Nothing is written until the winner is known.** Every candidate's bytes
//! are kept in memory while folders are searched; only the one that wins is,
//! if it came out of an archive, written to `stage_dir.join("pfs3aio")` —
//! through `atomic_create_new`, so a stage folder that already holds a
//! `pfs3aio` from an earlier attempt is refused rather than overwritten
//! (`SAFE_CREATE`).
//!
//! **An unreadable archive does not stop the search.** A file whose name
//! looks like the driver (`pfs3aio.lha`, or any `pfs3aio*`) but which ART
//! could not open or read — corrupt, a different format under that name,
//! too large — is skipped and named in `Pfs3DriverNotFound::unreadable`
//! when nothing usable is ultimately found, so a refusal does not read as
//! "nothing was there" when in fact something was, and ART could not use it.

use std::path::{Path, PathBuf};

use crate::core::amigaver::{self, AmigaVersion};
use crate::core::archive;
use crate::core::error::{CoreError, CoreResult};
use crate::core::safety::{atomic_create_new, Created};

/// The most of a candidate driver file ART will read into memory — loose or
/// out of an archive. A real `pfs3aio` is a few tens of KB; a hostile file
/// under that name is not let past this regardless of what it claims to be.
pub const DRIVER_MAX_BYTES: u64 = 1024 * 1024;

/// The PFS3 driver ART chose, and where its bytes now live.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundDriver {
    /// A loose file ART reads in place, or the copy `find_pfs3_driver` wrote
    /// into `stage_dir`.
    pub path: PathBuf,
    /// The archive it came out of, when it did.
    pub from_archive: Option<PathBuf>,
    pub version: u32,
    pub revision: u32,
}

/// A driver ART has read and can compare, with its bytes still in memory —
/// nothing is written to disk until the winner across every folder is known.
struct Candidate {
    bytes: Vec<u8>,
    version: AmigaVersion,
    /// `Some` for a loose file (its own path); `None` for one read out of an
    /// archive, where `from_archive` carries the archive's path instead.
    loose_path: Option<PathBuf>,
    from_archive: Option<PathBuf>,
}

/// Find the PFS3 driver: `explicit` (the user's own choice) wins outright;
/// otherwise every folder in `material`, in order — see the module doc for
/// the search and tie-break rules. A driver read out of an archive is
/// written into `stage_dir` before this returns.
pub fn find_pfs3_driver(
    explicit: Option<&Path>,
    material: &[PathBuf],
    stage_dir: &Path,
) -> CoreResult<FoundDriver> {
    if let Some(path) = explicit {
        let bytes = read_loose_bytes(path).map_err(|err| match err {
            CoreError::Io(io) => CoreError::InvalidInput(format!(
                "the PFS3 driver you chose, '{}', cannot be read ({io}). Choose the driver file \
                 again, or clear the choice so ART searches your material folders.",
                path.display()
            )),
            other => other,
        })?;
        // The user chose this file themselves, and it still has to state its
        // version: the card's RDB records it, and AmigaOS keeps the higher of
        // that and the one already loaded. The card writer refuses a silent
        // driver (`commands::hdf::read_file_systems`) — said here, before the
        // preparation stages anything, in the same words (the ledger's T7).
        // The same reading the card writer does, so this refuses exactly
        // what it would.
        let loose = || {
            crate::core::rdb::version_from_ver_string(&bytes).map(|(version, revision)| {
                AmigaVersion {
                    name: String::new(),
                    version: version.into(),
                    revision: revision.into(),
                }
            })
        };
        let version = amigaver::read(&bytes).or_else(loose).ok_or_else(|| {
            CoreError::InvalidInput(format!(
                "'{}' does not say what version it is. AmigaOS keeps the higher of the version in \
                 the disk and the one already loaded, so ART will not guess one — choose a \
                 pfs3aio that states its version (Aminet disk/misc/pfs3aio).",
                path.display()
            ))
        })?;
        let candidate = Candidate {
            bytes,
            version,
            loose_path: Some(path.to_path_buf()),
            from_archive: None,
        };
        return finish(candidate, stage_dir);
    }

    let mut searched = Vec::new();
    let mut unreadable = Vec::new();
    let mut best: Option<Candidate> = None;

    for folder in material {
        searched.push(crate::core::cardos::searched_folder(folder));

        for loose in [folder.join("pfs3aio"), folder.join("L").join("pfs3aio")] {
            if !loose.is_file() {
                continue;
            }
            match try_read_loose(&loose) {
                Ok(Some(candidate)) => best = pick(best, candidate),
                Ok(None) => {} // read fine, no $VER: — not a candidate
                Err(_) => unreadable.push(loose.display().to_string()),
            }
        }

        let Ok(entries) = std::fs::read_dir(folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.to_ascii_lowercase().starts_with("pfs3aio")
                || name.eq_ignore_ascii_case("pfs3aio")
            {
                // The exact loose name was already handled above; only an
                // archive *named* `pfs3aio...` (e.g. `pfs3aio.lha`) reaches
                // here.
                continue;
            }
            match try_read_archive(&path) {
                Ok(Some(candidate)) => best = pick(best, candidate),
                Ok(None) => {} // opened, but no matching member or no $VER:
                Err(_) => unreadable.push(path.display().to_string()),
            }
        }
    }

    match best {
        Some(candidate) => finish(candidate, stage_dir),
        None => Err(CoreError::Pfs3DriverNotFound {
            searched,
            unreadable,
        }),
    }
}

/// Keep `current` unless `new` states a strictly higher version — the first
/// of a tie wins, so folders earlier in `material` and, within one folder, a
/// loose file over an archive (search order above) keep their place.
fn pick(current: Option<Candidate>, new: Candidate) -> Option<Candidate> {
    match current {
        None => Some(new),
        Some(cur) => {
            if new.version.compare_version(&cur.version) == std::cmp::Ordering::Greater {
                Some(new)
            } else {
                Some(cur)
            }
        }
    }
}

/// All of `path` when it holds at most [`DRIVER_MAX_BYTES`] — the size comes
/// from metadata before a byte is read, the same order `core::rdbedit`'s
/// own driver load uses.
fn read_loose_bytes(path: &Path) -> CoreResult<Vec<u8>> {
    let len = std::fs::metadata(path)?.len();
    if len > DRIVER_MAX_BYTES {
        return Err(CoreError::LimitExceeded {
            subject: "PFS3 driver".into(),
            detail: format!(
                "'{}' is {len} bytes, more than the {DRIVER_MAX_BYTES}-byte cap ART reads for a \
                 driver",
                path.display()
            ),
        });
    }
    Ok(std::fs::read(path)?)
}

/// A loose material candidate: `Ok(None)` when the file reads fine but
/// states no `$VER:` — skipped, not an error.
fn try_read_loose(path: &Path) -> CoreResult<Option<Candidate>> {
    let bytes = read_loose_bytes(path)?;
    match amigaver::read(&bytes) {
        Some(version) => Ok(Some(Candidate {
            bytes,
            version,
            loose_path: Some(path.to_path_buf()),
            from_archive: None,
        })),
        None => Ok(None),
    }
}

/// An archive candidate: `Ok(None)` when the archive opens fine but holds no
/// member whose last path segment is `pfs3aio`, or that member states no
/// `$VER:`. `Err` — recorded as unreadable by the caller — is everything
/// that keeps ART from even examining it: the archive would not open, its
/// listing would not read, or the member itself would not.
fn try_read_archive(path: &Path) -> CoreResult<Option<Candidate>> {
    let mut backend = archive::open(path)?;
    let entries = backend.entries()?;
    let Some(index) = entries
        .iter()
        .position(|e| !e.is_dir && last_segment_is_pfs3aio(&e.name))
    else {
        return Ok(None);
    };
    let bytes = backend.read(index, DRIVER_MAX_BYTES)?;
    match amigaver::read(&bytes) {
        Some(version) => Ok(Some(Candidate {
            bytes,
            version,
            loose_path: None,
            from_archive: Some(path.to_path_buf()),
        })),
        None => Ok(None),
    }
}

/// Whether an archive entry's name — `/` or `\` separated, an archive's raw
/// choice — ends in `pfs3aio` (Aminet's own member is `pfs3aio/pfs3aio`).
fn last_segment_is_pfs3aio(name: &str) -> bool {
    name.rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .eq_ignore_ascii_case("pfs3aio")
}

/// Turn the winning candidate into the driver ART reports: staging its bytes
/// first when it came out of an archive.
fn finish(candidate: Candidate, stage_dir: &Path) -> CoreResult<FoundDriver> {
    let path = match &candidate.from_archive {
        None => candidate
            .loose_path
            .clone()
            .expect("a loose candidate always carries its own path"),
        Some(_) => {
            let dest = stage_dir.join("pfs3aio");
            match atomic_create_new(&dest, &candidate.bytes)? {
                Created::Yes => dest,
                Created::AlreadyThere => {
                    return Err(CoreError::SafetyRefused(format!(
                        "'{}' already exists — ART will not overwrite the driver it staged \
                         earlier; remove it and run again",
                        dest.display()
                    )))
                }
            }
        }
    };
    Ok(FoundDriver {
        path,
        from_archive: candidate.from_archive,
        version: candidate.version.version,
        revision: candidate.version.revision,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> (crate::core::ScratchDir, PathBuf) {
        crate::core::ScratchDir::pair("art-cardos-driver", tag)
    }

    fn driver_bytes(version: &str) -> Vec<u8> {
        let mut bytes = vec![0u8; 64];
        bytes.extend_from_slice(format!("$VER: pfs3aio {version} (01.01.2026)\0").as_bytes());
        bytes
    }

    fn write_lha(path: &Path, files: &[(&str, &[u8])]) {
        std::fs::write(path, crate::core::lha::tests::make_lha_with(files)).unwrap();
    }

    #[test]
    fn a_loose_driver_is_found_in_place_with_its_version() {
        let (_guard, dir) = scratch("loose");
        std::fs::write(dir.join("pfs3aio"), driver_bytes("19.2")).unwrap();
        let found = find_pfs3_driver(None, std::slice::from_ref(&dir), &dir.join("stage")).unwrap();
        assert_eq!(
            (found.version, found.revision, found.from_archive),
            (19, 2, None)
        );
        assert_eq!(found.path, dir.join("pfs3aio"));
    }

    #[test]
    fn a_driver_inside_an_lha_is_unpacked_into_the_stage_folder() {
        let (_guard, dir) = scratch("lha");
        write_lha(
            &dir.join("pfs3aio.lha"),
            &[("pfs3aio/pfs3aio", &driver_bytes("19.2"))],
        );
        let stage = dir.join("stage");
        std::fs::create_dir(&stage).unwrap();
        let found = find_pfs3_driver(None, std::slice::from_ref(&dir), &stage).unwrap();
        assert_eq!(
            found.from_archive.as_deref(),
            Some(dir.join("pfs3aio.lha").as_path())
        );
        assert!(found.path.starts_with(&stage));
        assert_eq!(std::fs::read(&found.path).unwrap(), driver_bytes("19.2"));
    }

    #[test]
    fn the_highest_version_wins_and_an_unversioned_file_is_skipped() {
        let (_guard, dir) = scratch("highest");
        let folder_a = dir.join("a");
        let folder_b = dir.join("b");
        let folder_c = dir.join("c");
        std::fs::create_dir_all(&folder_a).unwrap();
        std::fs::create_dir_all(&folder_b).unwrap();
        std::fs::create_dir_all(&folder_c).unwrap();

        std::fs::write(folder_a.join("pfs3aio"), driver_bytes("18.5")).unwrap();
        write_lha(
            &folder_b.join("pfs3aio.lha"),
            &[("pfs3aio/pfs3aio", &driver_bytes("19.2"))],
        );
        // No $VER: at all — a plain file, deliberately not built with
        // `driver_bytes`.
        std::fs::write(folder_c.join("pfs3aio"), vec![0u8; 64]).unwrap();

        let stage = dir.join("stage");
        std::fs::create_dir(&stage).unwrap();
        let found = find_pfs3_driver(
            None,
            &[folder_a, folder_b.clone(), folder_c.clone()],
            &stage,
        )
        .unwrap();

        assert_eq!((found.version, found.revision), (19, 2));
        assert_eq!(
            found.from_archive.as_deref(),
            Some(folder_b.join("pfs3aio.lha").as_path())
        );

        // The part a higher real version can hide: searched on its own,
        // folder C's unversioned file must not be treated as version 0.0 and
        // "found" — it must be exactly as if C held nothing at all. Without
        // this arm, an unversioned file silently accepted as 0.0 is a
        // survivor: any real candidate elsewhere always outranks 0.0 anyway,
        // so the assertions above cannot tell the two behaviours apart.
        let alone = find_pfs3_driver(None, std::slice::from_ref(&folder_c), &dir.join("stage2"))
            .unwrap_err();
        assert_eq!(alone.code(), "ART-PFS3-DRIVER-NOT-FOUND");
    }

    #[test]
    fn nothing_found_names_every_folder_searched() {
        let (_guard, dir) = scratch("none");
        let err = find_pfs3_driver(None, std::slice::from_ref(&dir), &dir).unwrap_err();
        assert_eq!(err.code(), "ART-PFS3-DRIVER-NOT-FOUND");
        assert!(err.to_string().contains(&dir.display().to_string()));
    }

    #[test]
    fn an_explicit_driver_wins_over_the_material() {
        let (_guard, dir) = scratch("explicit");
        let explicit = dir.join("mine").join("pfs3aio");
        std::fs::create_dir_all(explicit.parent().unwrap()).unwrap();
        std::fs::write(&explicit, driver_bytes("18.0")).unwrap();

        let material_dir = dir.join("material");
        std::fs::create_dir_all(&material_dir).unwrap();
        std::fs::write(material_dir.join("pfs3aio"), driver_bytes("19.2")).unwrap();

        let found = find_pfs3_driver(Some(&explicit), &[material_dir], &dir.join("stage")).unwrap();

        assert_eq!(
            (found.version, found.revision, found.from_archive),
            (18, 0, None)
        );
        assert_eq!(found.path, explicit);
    }

    /// T7 (deferred minor): a driver the user chose that states no version is
    /// refused at the preparation, with the sentence the card writer would
    /// otherwise give only at the build, after the whole preparation ran.
    #[test]
    fn an_explicit_driver_silent_about_its_version_is_refused_at_once() {
        let (_guard, dir) = scratch("explicit-silent");
        let explicit = dir.join("pfs3aio");
        std::fs::write(&explicit, vec![0u8; 64]).unwrap();

        let err = find_pfs3_driver(Some(&explicit), &[], &dir.join("stage")).unwrap_err();

        assert_eq!(err.code(), "ART-INPUT-INVALID", "{err}");
        let msg = err.to_string();
        assert!(
            msg.contains(&explicit.display().to_string())
                && msg.contains("does not say what version it is"),
            "{msg}"
        );
    }

    /// M7: a driver the user chose that is not there is refused naming the
    /// file and the next step, not as a bare "cannot find the file".
    #[test]
    fn a_missing_explicit_driver_is_refused_by_name() {
        let (_guard, dir) = scratch("explicit-missing");
        let explicit = dir.join("gone").join("pfs3aio");

        let err = find_pfs3_driver(Some(&explicit), &[], &dir.join("stage")).unwrap_err();

        let msg = err.to_string();
        assert!(
            msg.contains(&explicit.display().to_string()) && msg.contains("Choose the driver"),
            "{msg}"
        );
    }

    /// M8: a material folder that does not exist is named as such, not as a
    /// folder that was searched and held nothing.
    #[test]
    fn a_material_folder_that_does_not_exist_is_said_so() {
        let (_guard, dir) = scratch("gone-folder");
        let gone = dir.join("no-such-folder");
        let err = find_pfs3_driver(None, &[dir.clone(), gone.clone()], &dir).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains(&format!("{} (does not exist)", gone.display()))
                && !msg.contains(&format!("{} (does not exist)", dir.display())),
            "{msg}"
        );
    }

    /// An archive whose name looks like the driver but which ART cannot open
    /// is skipped, not fatal — and named in the refusal when nothing else is
    /// found.
    #[test]
    fn an_unopenable_archive_is_skipped_and_named_in_the_refusal() {
        let (_guard, dir) = scratch("unreadable");
        std::fs::write(dir.join("pfs3aio.lha"), b"not really an lha archive").unwrap();

        let err = find_pfs3_driver(None, std::slice::from_ref(&dir), &dir).unwrap_err();

        assert_eq!(err.code(), "ART-PFS3-DRIVER-NOT-FOUND");
        assert!(
            err.to_string()
                .contains(&dir.join("pfs3aio.lha").display().to_string()),
            "{err}"
        );
    }
}
