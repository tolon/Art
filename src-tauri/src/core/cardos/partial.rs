//! The `.partial` name a card image is written under while it is being
//! built, and what happens to it when a build fails, stops, or finishes
//! (design § 9, P2/P3).
//!
//! **Why a name and not a temp folder.** The image is written straight at
//! its final destination folder — the card's own free space is what has to
//! hold it — so the only way to tell "still being written" from "finished"
//! apart is the name itself. `partial_path_for` is measured, not assumed:
//! `hst.imager info` and `fs dir <img>\rdb\dh0` gave identical output for
//! `control.hdf`, `control.img`, `arm.img.partial` and `arm.partial.img`, the
//! same bytes — so appending `.partial` to the *whole* name (not swapping the
//! extension) is safe for every reader ART or hst-imager point at it, and it
//! is what the plan records (P2).
//!
//! **A stale `.partial` is refused, never removed** (P3): ART only ever
//! removes a file it created in *this* run, and a `.partial` already on disk
//! when a build starts was not created by this call — it might be an earlier
//! run's half-built image somebody has not looked at yet, and silently
//! deleting somebody's file to build a new one over it is the exact kind of
//! confident, unasked-for cleanup this project refuses to do.

use std::path::{Path, PathBuf};

use crate::core::card::manifest::manifest_path_for;
use crate::core::error::{CoreError, CoreResult};

/// `E:\x\amiga.img` → `E:\x\amiga.img.partial`. The whole name, not the
/// extension: `image.with_extension("partial")` would turn `amiga.img` into
/// `amiga.partial`, dropping the `.img` a reader keys off.
pub fn partial_path_for(image: &Path) -> PathBuf {
    let mut name = image.as_os_str().to_os_string();
    name.push(".partial");
    PathBuf::from(name)
}

/// Refuse before anything is written: a `.vhd` destination (design § 9), an
/// existing image (`SAFE_CREATE`), an existing `<image>.partial` (P3, named,
/// never removed), or an existing `<image>.manifest.json` — someone's record
/// of a card, which the finished build would otherwise write over (card
/// round 3, M17).
pub fn refuse_partial_destination(image: &Path) -> CoreResult<()> {
    if image
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("vhd"))
    {
        return Err(CoreError::UnsupportedFormat(
            "a card image built with its partitions filled cannot be a .vhd: ART's PFS3 writer \
             cannot fill a dynamic VHD — choose a .img name"
                .into(),
        ));
    }

    if image.exists() {
        return Err(CoreError::SafetyRefused(format!(
            "'{}' already exists — ART will not build over a card that is already there",
            image.display()
        )));
    }

    let partial = partial_path_for(image);
    if partial.exists() {
        return Err(CoreError::PartialImageExists {
            path: partial.display().to_string(),
        });
    }

    let manifest = manifest_path_for(image);
    if manifest.exists() {
        return Err(CoreError::SafetyRefused(format!(
            "'{}' already exists — a card manifest ART would write over when this card is \
             finished. Move it away, or choose another image name, and build again.",
            manifest.display()
        )));
    }

    Ok(())
}

/// Rename `<image>.partial` to `<image>`, the finish of a build that read
/// back correctly. Refuses (and leaves both files exactly as they are) when
/// `<image>` appeared while the build was running — nothing here decides
/// which of the two is the one to keep, and the raced-in file is not ART's
/// own to move aside.
///
/// **The name is taken atomically (card round 3, M3).** `std::fs::rename`
/// replaces an existing file on Windows (`MOVEFILE_REPLACE_EXISTING`), so a
/// check followed by a rename leaves a window in which a file that appears is
/// silently replaced. A hard link to the new name fails when the name exists,
/// with no window; the `.partial` name is then removed. A volume without hard
/// links (FAT32, exFAT) falls back to check-then-rename and keeps that small
/// window — disclosed, not closed.
pub fn finish_partial(image: &Path) -> CoreResult<()> {
    finish_partial_removing(image, |path: &Path| std::fs::remove_file(path))
}

/// [`finish_partial`], with the removal of a name given — so a test can make
/// both removals fail, which no file state on NTFS reliably does (N6).
pub(crate) fn finish_partial_removing(
    image: &Path,
    remove: impl Fn(&Path) -> std::io::Result<()>,
) -> CoreResult<()> {
    let partial = partial_path_for(image);
    let appeared = || {
        CoreError::SafetyRefused(format!(
            "'{}' appeared while ART was building it — the finished image is still at '{}'; \
             check which one you want and remove the other yourself",
            image.display(),
            partial.display()
        ))
    };
    match std::fs::hard_link(&partial, image) {
        Ok(()) => {
            if let Err(err) = remove(&partial) {
                // Both names are one file now. Take the new name back off it,
                // so the image is only at its partial name, as the caller's
                // ending will say.
                if let Err(rollback) = remove(image) {
                    // Neither name came off: both are ART's own card, and
                    // the sentence says so rather than leaving the caller to
                    // find a file at the image name and guess whose it is.
                    return Err(CoreError::CardFinishLeftBothNames {
                        image: image.display().to_string(),
                        partial: partial.display().to_string(),
                        why: format!("{err}; and the new name: {rollback}"),
                    });
                }
                return Err(CoreError::Io(err));
            }
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => Err(appeared()),
        Err(_) => {
            if image.exists() {
                return Err(appeared());
            }
            std::fs::rename(&partial, image)?;
            Ok(())
        }
    }
}

/// What happened to the `.partial` file on a failed or stopped build.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum PartialRemoval {
    /// This run never created a `.partial` — there is nothing of this run's
    /// to remove, and a file already sitting there (P3) is left untouched.
    NotCreated,
    Removed {
        path: String,
    },
    /// This run created it, and it was no longer there when the run went to
    /// remove it — this call removed nothing (card round 3, M4).
    AlreadyGone {
        path: String,
    },
    NotRemoved {
        path: String,
        why: String,
    },
}

/// Clean up the `.partial` this run created, if it created one.
///
/// `created` is the caller's own record of whether *this* call wrote the
/// file — `remove_partial` never infers it from the filesystem, because a
/// `.partial` that exists is not proof this run is the one that made it
/// (P3).
pub fn remove_partial(image: &Path, created: bool) -> PartialRemoval {
    let partial = partial_path_for(image);
    if !created {
        return PartialRemoval::NotCreated;
    }
    match std::fs::remove_file(&partial) {
        Ok(()) => PartialRemoval::Removed {
            path: partial.display().to_string(),
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => PartialRemoval::AlreadyGone {
            path: partial.display().to_string(),
        },
        Err(err) => PartialRemoval::NotRemoved {
            path: partial.display().to_string(),
            why: err.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> (crate::core::ScratchDir, PathBuf) {
        crate::core::ScratchDir::pair("art-cardos-partial", tag)
    }

    /// N6: the finished card is linked to its name, and then neither name can
    /// be removed. The removal is injected: measured on NTFS, a handle that
    /// does not share delete blocks only the name it was opened by (the
    /// rollback through the other name works), and `std::fs::remove_file`
    /// removes a read-only file — neither makes both removals fail. The
    /// error is its own: it names both paths as ART's own card and never says
    /// the file "appeared" or is somebody else's.
    #[test]
    fn a_finish_that_can_remove_neither_name_says_both_are_art_s_own_card() {
        let (_guard, dir) = scratch("both-names");
        let image = dir.join("amiga.img");
        let partial = partial_path_for(&image);
        std::fs::write(&partial, b"card").unwrap();

        let err = finish_partial_removing(&image, |_: &Path| {
            Err(std::io::Error::other("held by a scanner"))
        })
        .unwrap_err();

        assert!(
            partial.exists() && image.exists(),
            "the premise: both names stayed"
        );
        assert_eq!(err.code(), "ART-CARD-FINISH-LEFT-BOTH-NAMES", "{err}");
        let message = err.to_string();
        assert!(message.contains(&image.display().to_string()), "{message}");
        assert!(
            message.contains(&partial.display().to_string()),
            "{message}"
        );
        assert!(message.contains("held by a scanner"), "{message}");
        assert!(message.contains("ART's own half-built card"), "{message}");
        assert!(!message.contains("appeared"), "{message}");
    }

    #[test]
    fn the_partial_name_is_the_whole_name_plus_partial() {
        assert_eq!(
            partial_path_for(Path::new(r"E:\x\amiga.img")),
            PathBuf::from(r"E:\x\amiga.img.partial")
        );
    }

    #[test]
    fn a_stale_partial_is_refused_by_name_and_left_alone() {
        let (_guard, dir) = scratch("stale");
        let image = dir.join("amiga.img");
        std::fs::write(partial_path_for(&image), b"old").unwrap();

        let err = refuse_partial_destination(&image).unwrap_err();

        assert_eq!(err.code(), "ART-CARD-PARTIAL-EXISTS");
        assert_eq!(std::fs::read(partial_path_for(&image)).unwrap(), b"old");
    }

    #[test]
    fn an_existing_image_and_a_vhd_are_refused_before_anything_is_written() {
        // Arm 1: a name ending .vhd is refused outright, before any existence
        // check — a card with its partitions filled cannot be a dynamic VHD.
        let vhd = Path::new(r"E:\x\amiga.vhd");
        let err = refuse_partial_destination(vhd).unwrap_err();
        assert_eq!(err.code(), "ART-FORMAT-UNSUPPORTED", "{err}");

        // Arm 2: an existing .img destination is SAFE_CREATE's refusal.
        let (_guard, dir) = scratch("exists");
        let image = dir.join("amiga.img");
        std::fs::write(&image, b"already here").unwrap();

        let err = refuse_partial_destination(&image).unwrap_err();
        assert_eq!(err.code(), "ART-SAFETY-REFUSED", "{err}");
        assert_eq!(std::fs::read(&image).unwrap(), b"already here");
    }

    #[test]
    fn finish_renames_and_refuses_when_the_image_appeared_meanwhile() {
        // Arm 1: only the partial exists — finish renames it into place.
        let (_guard1, dir1) = scratch("clean");
        let image1 = dir1.join("amiga.img");
        std::fs::write(partial_path_for(&image1), b"built").unwrap();

        finish_partial(&image1).unwrap();

        assert!(image1.exists(), "the image now exists");
        assert!(!partial_path_for(&image1).exists(), "the partial is gone");
        assert_eq!(std::fs::read(&image1).unwrap(), b"built");

        // Arm 2: the image appeared while the build ran — both are left.
        let (_guard2, dir2) = scratch("raced");
        let image2 = dir2.join("amiga.img");
        std::fs::write(partial_path_for(&image2), b"built").unwrap();
        std::fs::write(&image2, b"raced in").unwrap();

        let err = finish_partial(&image2).unwrap_err();

        assert_eq!(err.code(), "ART-SAFETY-REFUSED", "{err}");
        assert_eq!(std::fs::read(&image2).unwrap(), b"raced in");
        assert_eq!(std::fs::read(partial_path_for(&image2)).unwrap(), b"built");
    }

    #[test]
    fn removal_is_reported_as_it_happened() {
        let (_guard, dir) = scratch("removal");
        let image = dir.join("amiga.img");

        // `created` false: nothing of this run's to remove, and a file
        // sitting there anyway (P3's stale case) is left untouched.
        std::fs::write(partial_path_for(&image), b"not mine").unwrap();
        assert_eq!(remove_partial(&image, false), PartialRemoval::NotCreated);
        assert_eq!(
            std::fs::read(partial_path_for(&image)).unwrap(),
            b"not mine"
        );

        // `created` true: this run's own file is removed.
        let outcome = remove_partial(&image, true);
        match outcome {
            PartialRemoval::Removed { path } => {
                assert_eq!(path, partial_path_for(&image).display().to_string());
            }
            other => panic!("expected Removed, got {other:?}"),
        }
        assert!(!partial_path_for(&image).exists());

        // M4: `created` true and the file already gone — this call removed
        // nothing, and does not claim it did.
        assert_eq!(
            remove_partial(&image, true),
            PartialRemoval::AlreadyGone {
                path: partial_path_for(&image).display().to_string()
            }
        );
    }

    /// M17: `<image>.manifest.json` already there is someone's record — a
    /// card moved away and its manifest left — and is refused by name, not
    /// written over when the new card is finished.
    #[test]
    fn a_manifest_already_beside_the_image_name_is_refused_and_left_alone() {
        let (_guard, dir) = scratch("manifest");
        let image = dir.join("amiga.img");
        let manifest = dir.join("amiga.img.manifest.json");
        std::fs::write(&manifest, b"theirs").unwrap();

        let err = refuse_partial_destination(&image).unwrap_err();

        assert_eq!(err.code(), "ART-SAFETY-REFUSED", "{err}");
        assert!(
            err.to_string().contains(&manifest.display().to_string()),
            "{err}"
        );
        assert_eq!(std::fs::read(&manifest).unwrap(), b"theirs");
    }
}
