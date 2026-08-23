//! What a package needs done **after** its own installer has run, and why ART
//! does it on the host rather than on the Amiga.
//!
//! ART-227. A BoingBag's `Updater` finishes and leaves work behind that it
//! does not do itself — measured, not assumed (2026-08-24, both of the
//! owner's BoingBags installed against their own material):
//!
//! - `Devs/AmigaOS ROM Update.BB39-2`, **321 768 bytes**, sitting beside the
//!   CD's own 127 956-byte `AmigaOS ROM Update` under a name nothing loads.
//!   `SetPatch` goes on loading the old one, so BoingBag 2's ROM update is
//!   inert in a tree that otherwise reports itself updated.
//! - Not one of the seven `C:` commands HstWB sets `+prwed` on has the `p`
//!   bit afterwards, and `S/Start-Amplifier.rexx` has no `s` bit.
//!
//! ## Why the host, when the one readable builder does it on the Amiga
//!
//! HstWB Installer performs these as AmigaDOS lines appended to the script it
//! boots (`amiga/amiga-os-3.9/S/Amiga-OS-3.9/Install-Boing-Bag-2`), because
//! **HstWB has no host side while the install is running**. ART does: it
//! stages a copy, runs the installer against it, and only then decides what
//! becomes of it. Every step here is an ordinary file operation on that copy.
//!
//! Doing it here rather than there is not a shortcut; it removes three
//! problems at once.
//!
//! - **Quoting.** The file to rotate is `Devs/AmigaOS ROM Update` — spaces in
//!   the name — and `core::amigainstall::workvol` cannot express that on
//!   purpose. `refuse_shell_metacharacters` refuses `"` *because a quote
//!   changes where a string ends*, and the generated line joins arguments
//!   with spaces, so a composed value carrying whitespace is refused rather
//!   than silently turned into two arguments. That refusal is right for an
//!   installer's own argument list, which ART cannot parse. It would have had
//!   to be weakened for this.
//! - **A fifth ending.** A fix-up that failed on the Amiga would need its own
//!   `RunOutcome`, because "the installer said no" and "the installer worked
//!   and ART could not finish" are different things to tell a person. Here a
//!   failure is an ordinary `CoreError` on the host, raised before
//!   [`super::stage::settle`] is reached — so the copy is **not** promoted and
//!   the user's own tree is untouched, which is the answer §92 already gives.
//! - **Testing.** Nothing below needs an emulator, a ROM or a licence. Every
//!   case in this module's tests runs in a tempdir.
//!
//! ## What is deliberately not here
//!
//! HstWB does five more things after the `Updater`: WarpUp libraries copied
//! only if the tree already has them, locale catalogs, HDToolBox's icon
//! position, a second `Updater` run for `XAD-Update` gated on
//! `xadmaster.library`, and `C/Installer` copied into `SYS:C` and
//! `SYS:Utilities`. **None of them has been shown necessary against ART's own
//! result**, and building an operation for a step nobody has measured is how a
//! vocabulary grows past what anyone can check. They are recorded in ART-227;
//! each becomes a variant here on the day a measurement asks for one.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::error::{CoreError, CoreResult};
use crate::core::security::path::safe_join;
use crate::core::volume::write::uaem;

/// The backup names tried, in order, before a rotation gives up.
///
/// HstWB's own list, and it stops where HstWB stops — except that running out
/// is a refusal here rather than an overwrite of the last one. A fifth
/// generation of the same file means something is being installed repeatedly
/// and losing the oldest copy silently is not ART's call to make.
const BACKUP_SUFFIXES: [&str; 4] = [".old", ".old2", ".old3", ".old4"];

/// One thing a package needs done to the tree after its installer has run.
///
/// **Typed, never a script.** A free-text AmigaDOS field in recipe data would
/// drive a hole straight through `workvol`'s metacharacter gate, and a
/// free-text *host* command would be worse. Each variant is a shape ART
/// understands well enough to check, which is what lets the paths below go
/// through [`safe_join`] like any other name that arrives from data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "kebab-case")]
pub enum PostStep {
    /// Add protection-bit letters to a file the installer placed.
    ///
    /// `add` is letters from `hsparwed`; anything else is refused. Only
    /// *adding* is expressible, because every measured case adds (`+prwed`,
    /// `+srwed`) and a step that could clear a bit could quietly make a file
    /// unreadable.
    Protect { path: String, add: String },
    /// Move `target` aside to the first free backup name and put
    /// `replacement` where it was.
    ///
    /// The rotation BoingBag 2 needs. `replacement` must exist — it is the
    /// point of the step — while a missing `target` is not an error: there is
    /// simply nothing to back up, and the replacement is placed.
    ReplaceKeepingBackup { target: String, replacement: String },
}

/// What one step actually did, so the report can say rather than imply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppliedStep {
    /// `path`'s bits went from the first spelling to the second.
    Protected {
        path: String,
        was: String,
        now: String,
    },
    /// `replacement` is now `target`; the previous `target`, if there was one,
    /// is at `backup`.
    Replaced {
        target: String,
        replacement: String,
        backup: Option<String>,
    },
}

/// Apply a package's post-install steps to the staged copy.
///
/// `root` is the **copy**, never the user's own tree — the caller runs this
/// between a successful run and [`super::stage::settle`], so a failure here
/// means the copy is kept and the original is untouched.
pub fn apply(root: &Path, steps: &[PostStep]) -> CoreResult<Vec<AppliedStep>> {
    let mut done = Vec::with_capacity(steps.len());
    for step in steps {
        done.push(match step {
            PostStep::Protect { path, add } => protect(root, path, add)?,
            PostStep::ReplaceKeepingBackup {
                target,
                replacement,
            } => replace_keeping_backup(root, target, replacement)?,
        });
    }
    Ok(done)
}

/// Resolve one recipe-supplied AmigaDOS path inside the copy.
fn resolve(root: &Path, path: &str) -> CoreResult<PathBuf> {
    safe_join(root, path).map_err(|e| {
        CoreError::InvalidInput(format!("'{path}' is not a path inside the tree: {e}"))
    })
}

fn protect(root: &Path, path: &str, add: &str) -> CoreResult<AppliedStep> {
    let file = resolve(root, path)?;
    if !file.is_file() {
        // Not a skip. The installer was supposed to place this, and a step
        // that quietly does nothing is the failure this project is most
        // expensive at. A pressing that genuinely lacks the file is a
        // measurement worth stopping for.
        return Err(CoreError::InvalidInput(format!(
            "'{path}' is not in the tree the installer produced, so its protection bits \
             cannot be set — the package's own recipe expects it"
        )));
    }

    let letters: Vec<char> = add.chars().collect();
    if letters.is_empty() {
        return Err(CoreError::InvalidInput(format!(
            "'{path}': no protection letters to add"
        )));
    }
    for letter in &letters {
        if !uaem::BIT_LETTERS.contains(&letter.to_ascii_lowercase()) {
            return Err(CoreError::InvalidInput(format!(
                "'{letter}' is not one of AmigaDOS's protection letters (hsparwed)"
            )));
        }
    }

    let sidecar_path = uaem::sidecar_path(&file);
    // No sidecar means WinUAE's default for the file, which is what the
    // Amiga-side installer left when it wrote nothing special. Starting from
    // `Sidecar::default()` and taking the file's own date keeps that true
    // rather than inventing a protection nobody set.
    let mut sidecar = if sidecar_path.is_file() {
        uaem::parse(&std::fs::read_to_string(&sidecar_path)?)?
    } else {
        uaem::Sidecar::default()
    };

    let was = uaem::format_bits(sidecar.protection);
    let mut bits: Vec<char> = was.chars().collect();
    for letter in letters {
        let wanted = letter.to_ascii_lowercase();
        let index = uaem::BIT_LETTERS
            .iter()
            .position(|l| *l == wanted)
            .expect("checked above");
        bits[index] = wanted;
    }
    let now: String = bits.into_iter().collect();
    sidecar.protection = uaem::parse_bits(&now)?;

    std::fs::write(&sidecar_path, uaem::render(&sidecar))?;
    Ok(AppliedStep::Protected {
        path: path.to_string(),
        was,
        now,
    })
}

fn replace_keeping_backup(root: &Path, target: &str, replacement: &str) -> CoreResult<AppliedStep> {
    let target_path = resolve(root, target)?;
    let replacement_path = resolve(root, replacement)?;

    if !replacement_path.is_file() {
        return Err(CoreError::InvalidInput(format!(
            "'{replacement}' is not in the tree the installer produced, so there is nothing \
             to put in place of '{target}'"
        )));
    }

    // A missing target is not a failure: nothing to move aside, and the
    // replacement still belongs where the recipe says.
    let backup = if target_path.is_file() {
        let name = target_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| CoreError::InvalidInput(format!("'{target}' has no file name")))?
            .to_string();
        let chosen = BACKUP_SUFFIXES
            .iter()
            .map(|suffix| target_path.with_file_name(format!("{name}{suffix}")))
            .find(|candidate| !candidate.exists())
            .ok_or_else(|| {
                CoreError::InvalidInput(format!(
                    "'{target}' has been replaced {} times already and every backup name is \
                     taken; ART will not overwrite the oldest copy to make room",
                    BACKUP_SUFFIXES.len()
                ))
            })?;
        copy_with_sidecar(&target_path, &chosen)?;
        Some(
            chosen
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string(),
        )
    } else {
        None
    };

    copy_with_sidecar(&replacement_path, &target_path)?;

    Ok(AppliedStep::Replaced {
        target: target.to_string(),
        replacement: replacement.to_string(),
        backup,
    })
}

/// Copy a file and whatever AmigaDOS metadata travels with it.
///
/// The sidecar is not optional bookkeeping: it carries the protection bits and
/// the date, and a `Devs/AmigaOS ROM Update` that arrived without them would
/// be a different file to the Amiga than the one the package shipped. When the
/// source has none, any stale sidecar at the destination is removed rather
/// than left describing bytes that are gone.
fn copy_with_sidecar(from: &Path, to: &Path) -> CoreResult<()> {
    std::fs::copy(from, to)?;
    let from_sidecar = uaem::sidecar_path(from);
    let to_sidecar = uaem::sidecar_path(to);
    if from_sidecar.is_file() {
        std::fs::copy(&from_sidecar, &to_sidecar)?;
    } else if to_sidecar.is_file() {
        std::fs::remove_file(&to_sidecar)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ScratchDir;

    /// A tree with one file, optionally carrying a sidecar.
    fn tree(tag: &str) -> (ScratchDir, PathBuf) {
        let scratch = ScratchDir::new("art-finish", tag);
        let root = scratch.path().join("tree");
        std::fs::create_dir_all(root.join("C")).unwrap();
        std::fs::create_dir_all(root.join("Devs")).unwrap();
        (scratch, root)
    }

    fn write(root: &Path, rel: &str, bytes: &[u8]) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn bits_of(root: &Path, rel: &str) -> String {
        let text = std::fs::read_to_string(uaem::sidecar_path(&root.join(rel))).unwrap();
        text.split_whitespace().next().unwrap().to_string()
    }

    // ---- protect ---------------------------------------------------------

    /// The measured case: after a real BoingBag 1 the seven commands read
    /// `----rwed` and HstWB sets `+prwed`. Only the `p` moves.
    #[test]
    fn protect_adds_the_pure_bit_and_leaves_the_rest_alone() {
        let (_s, root) = tree("protect-adds");
        write(&root, "C/LoadMonDrvs", b"cmd");
        write(
            &root,
            "C/LoadMonDrvs.uaem",
            b"----rwed 1994-07-06 02:00:42.00 a comment\n",
        );

        let done = apply(
            &root,
            &[PostStep::Protect {
                path: "C/LoadMonDrvs".into(),
                add: "p".into(),
            }],
        )
        .unwrap();

        assert_eq!(bits_of(&root, "C/LoadMonDrvs"), "--p-rwed");
        assert_eq!(
            done,
            vec![AppliedStep::Protected {
                path: "C/LoadMonDrvs".into(),
                was: "----rwed".into(),
                now: "--p-rwed".into(),
            }]
        );
    }

    /// The date and the comment are somebody else's data and must survive a
    /// step that is about protection. A rewritten sidecar that lost the date
    /// would change what the Amiga thinks the file is.
    #[test]
    fn protect_keeps_the_date_and_the_comment() {
        let (_s, root) = tree("protect-keeps");
        write(&root, "C/WBRun", b"cmd");
        write(
            &root,
            "C/WBRun.uaem",
            b"----rwed 1994-07-06 02:00:42.00 kept text\n",
        );

        apply(
            &root,
            &[PostStep::Protect {
                path: "C/WBRun".into(),
                add: "p".into(),
            }],
        )
        .unwrap();

        let line = std::fs::read_to_string(uaem::sidecar_path(&root.join("C/WBRun"))).unwrap();
        assert!(line.contains("1994-07-06 02:00:42.00"), "{line}");
        assert!(line.contains("kept text"), "{line}");
    }

    /// Two of the seven had no sidecar at all after the real run — WinUAE
    /// writes one only when the metadata differs from its default. Starting
    /// from the default rather than refusing is what lets those two be set.
    #[test]
    fn protect_creates_a_sidecar_when_the_file_has_none() {
        let (_s, root) = tree("protect-creates");
        write(&root, "C/SetEnv", b"cmd");
        assert!(!uaem::sidecar_path(&root.join("C/SetEnv")).exists());

        apply(
            &root,
            &[PostStep::Protect {
                path: "C/SetEnv".into(),
                add: "p".into(),
            }],
        )
        .unwrap();

        assert_eq!(bits_of(&root, "C/SetEnv"), "--p-rwed");
    }

    /// The `s` bit, because BoingBag 1's three ARexx scripts want `+srwed`
    /// and a test that only ever adds `p` would pass against a hard-coded
    /// one.
    #[test]
    fn protect_can_add_the_script_bit_too() {
        let (_s, root) = tree("protect-script");
        write(&root, "S/Start-Amplifier.rexx", b"rexx");
        apply(
            &root,
            &[PostStep::Protect {
                path: "S/Start-Amplifier.rexx".into(),
                add: "s".into(),
            }],
        )
        .unwrap();
        assert_eq!(bits_of(&root, "S/Start-Amplifier.rexx"), "-s--rwed");
    }

    /// **Not a skip.** A step that quietly does nothing when its file is
    /// absent is the confident-and-wrong shape: the tree would look installed
    /// and the bit would not be there.
    #[test]
    fn protect_refuses_a_file_the_installer_did_not_leave() {
        let (_s, root) = tree("protect-missing");
        let err = apply(
            &root,
            &[PostStep::Protect {
                path: "C/NotThere".into(),
                add: "p".into(),
            }],
        )
        .unwrap_err();
        let text = err.to_string();
        assert!(text.contains("C/NotThere"), "{text}");
        assert!(text.contains("not in the tree"), "{text}");
    }

    #[test]
    fn protect_refuses_a_letter_amigados_does_not_have() {
        let (_s, root) = tree("protect-letter");
        write(&root, "C/X", b"cmd");
        let err = apply(
            &root,
            &[PostStep::Protect {
                path: "C/X".into(),
                add: "z".into(),
            }],
        )
        .unwrap_err();
        assert!(err.to_string().contains("hsparwed"), "{err}");
    }

    #[test]
    fn a_path_that_climbs_out_of_the_tree_is_refused() {
        let (_s, root) = tree("escape");
        for step in [
            PostStep::Protect {
                path: "../outside".into(),
                add: "p".into(),
            },
            PostStep::ReplaceKeepingBackup {
                target: "../outside".into(),
                replacement: "Devs/x".into(),
            },
        ] {
            let err = apply(&root, &[step]).unwrap_err();
            assert!(
                err.to_string().contains("not a path inside the tree"),
                "{err}"
            );
        }
    }

    // ---- replace-keeping-backup -----------------------------------------

    /// ART-227's own case, with the real names and the real sizes.
    #[test]
    fn the_rom_update_is_rotated_and_the_old_one_kept() {
        let (_s, root) = tree("rotate");
        write(&root, "Devs/AmigaOS ROM Update", &vec![b'o'; 127_956]);
        write(
            &root,
            "Devs/AmigaOS ROM Update.uaem",
            b"----rw-d 1994-07-06 02:00:42.00 \n",
        );
        write(
            &root,
            "Devs/AmigaOS ROM Update.BB39-2",
            &vec![b'n'; 321_768],
        );

        let done = apply(
            &root,
            &[PostStep::ReplaceKeepingBackup {
                target: "Devs/AmigaOS ROM Update".into(),
                replacement: "Devs/AmigaOS ROM Update.BB39-2".into(),
            }],
        )
        .unwrap();

        assert_eq!(
            std::fs::metadata(root.join("Devs/AmigaOS ROM Update"))
                .unwrap()
                .len(),
            321_768,
            "the file SetPatch loads must now be BoingBag 2's"
        );
        assert_eq!(
            std::fs::metadata(root.join("Devs/AmigaOS ROM Update.old"))
                .unwrap()
                .len(),
            127_956,
            "and the one it replaced must still be there"
        );
        assert_eq!(
            done,
            vec![AppliedStep::Replaced {
                target: "Devs/AmigaOS ROM Update".into(),
                replacement: "Devs/AmigaOS ROM Update.BB39-2".into(),
                backup: Some("AmigaOS ROM Update.old".into()),
            }]
        );
    }

    /// The sidecar is not bookkeeping: it carries the protection bits and the
    /// date, and a ROM update that arrived without them is a different file to
    /// the Amiga than the one the package shipped.
    #[test]
    fn the_replacements_own_metadata_travels_with_it() {
        let (_s, root) = tree("rotate-uaem");
        write(&root, "Devs/A", b"old");
        write(
            &root,
            "Devs/A.uaem",
            b"----rw-d 1994-07-06 02:00:42.00 old\n",
        );
        write(&root, "Devs/A.new", b"new");
        write(
            &root,
            "Devs/A.new.uaem",
            b"--p-rwed 2001-12-07 11:22:33.00 new\n",
        );

        apply(
            &root,
            &[PostStep::ReplaceKeepingBackup {
                target: "Devs/A".into(),
                replacement: "Devs/A.new".into(),
            }],
        )
        .unwrap();

        assert_eq!(
            bits_of(&root, "Devs/A"),
            "--p-rwed",
            "the replacement's bits"
        );
        assert_eq!(
            bits_of(&root, "Devs/A.old"),
            "----rw-d",
            "the backup keeps its own"
        );
    }

    /// A replacement with no sidecar must not inherit the old file's, which is
    /// what would happen if the destination's stale one were left in place.
    #[test]
    fn a_replacement_without_metadata_does_not_inherit_the_old_files() {
        let (_s, root) = tree("rotate-stale");
        write(&root, "Devs/A", b"old");
        write(
            &root,
            "Devs/A.uaem",
            b"--p-rwed 1994-07-06 02:00:42.00 old\n",
        );
        write(&root, "Devs/A.new", b"new");

        apply(
            &root,
            &[PostStep::ReplaceKeepingBackup {
                target: "Devs/A".into(),
                replacement: "Devs/A.new".into(),
            }],
        )
        .unwrap();

        assert!(
            !uaem::sidecar_path(&root.join("Devs/A")).exists(),
            "the old sidecar described bytes that are gone"
        );
    }

    /// Nothing to move aside is not a failure — the replacement still belongs
    /// where the recipe says.
    #[test]
    fn a_missing_target_is_placed_rather_than_refused() {
        let (_s, root) = tree("rotate-notarget");
        write(&root, "Devs/A.new", b"new");
        let done = apply(
            &root,
            &[PostStep::ReplaceKeepingBackup {
                target: "Devs/A".into(),
                replacement: "Devs/A.new".into(),
            }],
        )
        .unwrap();
        assert_eq!(std::fs::read(root.join("Devs/A")).unwrap(), b"new");
        assert!(matches!(
            done.as_slice(),
            [AppliedStep::Replaced { backup: None, .. }]
        ));
    }

    /// The point of the step. Without it there is nothing to put anywhere, and
    /// carrying on would leave the old file in place while the report said a
    /// replacement happened.
    #[test]
    fn a_missing_replacement_is_refused_by_name() {
        let (_s, root) = tree("rotate-noreplacement");
        write(&root, "Devs/A", b"old");
        let err = apply(
            &root,
            &[PostStep::ReplaceKeepingBackup {
                target: "Devs/A".into(),
                replacement: "Devs/A.new".into(),
            }],
        )
        .unwrap_err();
        let text = err.to_string();
        assert!(text.contains("Devs/A.new"), "{text}");
        assert!(text.contains("nothing to put in place"), "{text}");
    }

    /// HstWB stops after four and would overwrite the fourth. Losing the
    /// oldest copy silently is not ART's call, so this refuses instead — and
    /// names why.
    #[test]
    fn running_out_of_backup_names_refuses_rather_than_overwriting() {
        let (_s, root) = tree("rotate-full");
        write(&root, "Devs/A", b"old");
        write(&root, "Devs/A.new", b"new");
        for suffix in ["old", "old2", "old3", "old4"] {
            write(&root, &format!("Devs/A.{suffix}"), b"kept");
        }
        let err = apply(
            &root,
            &[PostStep::ReplaceKeepingBackup {
                target: "Devs/A".into(),
                replacement: "Devs/A.new".into(),
            }],
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("every backup name is taken"),
            "{err}"
        );
        for suffix in ["old", "old2", "old3", "old4"] {
            assert_eq!(
                std::fs::read(root.join(format!("Devs/A.{suffix}"))).unwrap(),
                b"kept",
                "nothing may be overwritten on the way to refusing"
            );
        }
    }

    /// Order matters and a later step must see what an earlier one did.
    #[test]
    fn steps_run_in_the_order_they_are_declared() {
        let (_s, root) = tree("order");
        write(&root, "Devs/A", b"old");
        write(&root, "Devs/A.new", b"new");
        let done = apply(
            &root,
            &[
                PostStep::ReplaceKeepingBackup {
                    target: "Devs/A".into(),
                    replacement: "Devs/A.new".into(),
                },
                PostStep::Protect {
                    path: "Devs/A".into(),
                    add: "p".into(),
                },
            ],
        )
        .unwrap();
        assert_eq!(done.len(), 2);
        assert_eq!(bits_of(&root, "Devs/A"), "--p-rwed");
    }

    /// The recipe shape a package will actually carry, parsed rather than
    /// hand-built — a variant renamed in the enum and not in the JSON would
    /// otherwise pass every test above.
    #[test]
    fn the_json_a_recipe_carries_deserialises_to_these_steps() {
        let json = r#"[
            { "step": "protect", "path": "C/LoadMonDrvs", "add": "p" },
            { "step": "replace-keeping-backup",
              "target": "Devs/AmigaOS ROM Update",
              "replacement": "Devs/AmigaOS ROM Update.BB39-2" }
        ]"#;
        let steps: Vec<PostStep> = serde_json::from_str(json).unwrap();
        assert_eq!(
            steps,
            vec![
                PostStep::Protect {
                    path: "C/LoadMonDrvs".into(),
                    add: "p".into()
                },
                PostStep::ReplaceKeepingBackup {
                    target: "Devs/AmigaOS ROM Update".into(),
                    replacement: "Devs/AmigaOS ROM Update.BB39-2".into()
                },
            ]
        );
    }
}
