//! Where ART stages work it will throw away — the screen's side of it
//! (ART-196).
//!
//! Thin adapters over [`crate::scratch`], which holds the rule and the
//! validation. Two commands and one shape:
//!
//! - [`scratch_root`] answers what ART is using and whether that is the
//!   user's own choice or the platform's default. Two different sentences,
//!   and a screen cannot write either one from a single path.
//! - [`scratch_set_root`] takes a folder, or `null` to go back to the
//!   default, and answers with **where the previous one was**. That is not
//!   decoration: repointing the root moves nothing, and a user who is not
//!   told where their old staging went has been left to find it.
//!
//! **The screen may not out-claim the core.** When the chosen root cannot be
//! reached, [`crate::scratch::root`] refuses and every operation that needs
//! to stage refuses with it — so [`ScratchRootState::in_use`] is `null` and
//! [`ScratchRootState::unreachable`] carries the reason. Reporting the
//! default there would have the screen say ART is staging somewhere it is
//! not, which is this project's most expensive class of defect.
//!
//! **Nothing here moves or deletes anything.** ART does not touch what it
//! did not put there this run, and moving gigabytes while somebody waits on
//! a Settings screen is the wrong shape. The owner's rule over all of it:
//! nothing ART writes goes to `C:` once they have said otherwise, and
//! nothing deletes from `C:` at all.

use serde::Serialize;

use crate::error::AppResult;

/// What ART is staging into, and how it came to be that.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScratchRootState {
    /// The folder ART will stage into right now, or `null` when the chosen
    /// one cannot be reached — in which case nothing stages at all, rather
    /// than quietly staging somewhere else.
    pub in_use: Option<String>,
    /// The folder the user chose, or `null` while they are on the default.
    pub chosen: Option<String>,
    /// What "the default" means on this machine — the platform's own temp
    /// directory. Sent so the screen can *show* it rather than describe it:
    /// "ART uses its own folder" was exactly the sentence ART-214 was filed
    /// for, and it was not true.
    pub default: String,
    /// Why the chosen folder cannot be used, when it cannot. The whole
    /// refusal, id and remedy included, so the screen quotes rather than
    /// paraphrases.
    pub unreachable: Option<String>,
}

/// What ART is staging into.
///
/// **Always `Ok`.** A Settings screen that cannot render because the folder
/// it exists to change is unreachable is the one screen that must still
/// work, so an unusable root is reported as state rather than raised as an
/// error — while saying, in `unreachable`, that nothing will stage until it
/// is fixed.
#[tauri::command]
pub fn scratch_root() -> AppResult<ScratchRootState> {
    let default = std::env::temp_dir().display().to_string();
    let chosen = crate::scratch::chosen().map(|p| p.display().to_string());
    Ok(match crate::scratch::root() {
        Ok(path) => {
            // **Here, not at start-up.** The root is not known when the window
            // opens: the frontend pushes the remembered one, and until it has,
            // the effective root is the default. Sweeping the wrong folder
            // first and the right one never is how a start-up hook would have
            // behaved. This command resolves the root on every query and on
            // every change, and `sweep_once` makes the repetition free
            // (ART-184).
            crate::scratch::sweep_once(&path);
            ScratchRootState {
                in_use: Some(path.display().to_string()),
                chosen,
                default,
                unreachable: None,
            }
        }
        Err(err) => ScratchRootState {
            in_use: None,
            chosen,
            default,
            unreachable: Some(err.user_message()),
        },
    })
}

/// What [`scratch_set_root`] answers: the new state, and where the old one
/// was.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScratchRootChange {
    pub root: ScratchRootState,
    /// Where ART was staging until this call. **Nothing there was moved or
    /// removed**, and the screen says so — see the module doc comment.
    pub previous: String,
}

/// Take `path` as the scratch root, or `null` to go back to the default.
///
/// Refused here, on the screen the user is looking at, if the folder is not
/// there or cannot be written to — and the root they had stays in force, so
/// a bad choice never leaves ART with nowhere to stage.
#[tauri::command]
pub fn scratch_set_root(path: Option<String>) -> AppResult<ScratchRootChange> {
    // Read before the change, so the answer can name it. A root that has
    // gone away is still where the user's staging is, so it is named as the
    // previous one rather than skipped over for the default.
    let previous = crate::scratch::chosen()
        .or_else(|| crate::scratch::root().ok())
        .unwrap_or_else(std::env::temp_dir)
        .display()
        .to_string();

    let trimmed = path.as_deref().map(str::trim).filter(|s| !s.is_empty());
    crate::scratch::set(trimmed.map(std::path::Path::new))?;

    Ok(ScratchRootChange {
        root: scratch_root()?,
        previous,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ScratchDir;

    /// **The crate's one lock, not a second one here.** `CHOSEN` is a single
    /// process-wide value, so a private mutex in this module would guard it
    /// against nothing that `crate::scratch`'s tests are doing — which is how
    /// three of these failed against each other before it was shared.
    use crate::scratch::serially;

    #[test]
    fn on_the_default_the_state_says_so_rather_than_naming_a_choice() {
        serially(|| {
            let state = scratch_root().unwrap();
            assert_eq!(state.chosen, None);
            assert_eq!(state.in_use.as_deref(), Some(state.default.as_str()));
            assert_eq!(state.unreachable, None);
        });
    }

    #[test]
    fn setting_a_root_answers_with_it_and_with_where_the_old_one_was() {
        serially(|| {
            let dir = ScratchDir::new("art-scratch-cmd", "set");
            let picked = dir.path().display().to_string();
            let before = std::env::temp_dir().display().to_string();

            let change = scratch_set_root(Some(picked.clone())).unwrap();
            assert_eq!(change.root.in_use.as_deref(), Some(picked.as_str()));
            assert_eq!(change.root.chosen.as_deref(), Some(picked.as_str()));
            assert_eq!(
                change.previous, before,
                "the user has to be told where what ART already staged still is"
            );
        });
    }

    /// An empty box is "go back to the default", not "stage into the current
    /// working directory".
    #[test]
    fn an_empty_string_clears_the_choice() {
        serially(|| {
            let dir = ScratchDir::new("art-scratch-cmd", "clear");
            let picked = dir.path().display().to_string();
            scratch_set_root(Some(picked.clone())).unwrap();

            let change = scratch_set_root(Some("   ".to_string())).unwrap();
            assert_eq!(change.root.chosen, None);
            assert_eq!(
                change.root.in_use.as_deref(),
                Some(std::env::temp_dir().display().to_string().as_str())
            );
            assert_eq!(
                change.previous, picked,
                "going back to the default still leaves the old staging where it was"
            );
        });
    }

    /// **The Settings screen must still open when the folder it exists to
    /// change has gone away** — otherwise the one way out of the problem is
    /// the one thing the problem breaks.
    ///
    /// And it must not claim ART is staging into the default while
    /// `crate::scratch::root()` is refusing: the screen may not out-claim the
    /// core.
    #[test]
    fn a_vanished_root_is_reported_as_unusable_not_as_the_default() {
        serially(|| {
            let dir = ScratchDir::new("art-scratch-cmd", "vanished");
            let picked = dir.path().to_path_buf();
            scratch_set_root(Some(picked.display().to_string())).unwrap();
            std::fs::remove_dir_all(&picked).unwrap();

            let state = scratch_root().expect("the screen must still be able to render");
            assert_eq!(
                state.chosen.as_deref(),
                Some(picked.display().to_string().as_str()),
                "it must still name what the user asked for"
            );
            assert_eq!(
                state.in_use, None,
                "nothing is staging, and the screen must not say otherwise"
            );
            let why = state.unreachable.expect("and it must say why");
            assert!(why.contains(&picked.display().to_string()), "{why}");
            assert!(why.contains("ART-SCRATCH-UNAVAILABLE"), "{why}");
        });
    }
}
