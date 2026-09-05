//! Which AmigaOS release is this pile of install media?
//!
//! Work-list item 5, and what survives of the 2026-08-22 list's item 4 after
//! [ART-222] closed. The screen asks a user to pick a release from a dropdown
//! and then to point at a folder, and until now **nothing checked that the two
//! agreed**. Pointing 3.9's disc at a 3.2 install does not produce a wrong
//! tree — `plan` refuses `MediaMissing` — but it produces a refusal that names
//! a disk the user does not own and cannot get, which is this project's
//! "a refusal must be actionable" rule broken in the ordinary way.
//!
//! # The table is the recipes, and that is the whole point
//!
//! The item this module closes is titled *"a table ART can stand behind"*,
//! because the round before it **dropped** a 186-row Kickstart hash table that
//! ART could not verify. So there is no table here. Every name below is read
//! out of [`recipe::by_release`] at call time: a recipe declares each
//! component's `media` (the volume name **inside** the image) and whether the
//! component is `required`, and those two fields are already checked against
//! the owner's real discs by the plan tests. A future release is a JSON file,
//! exactly as `CLAUDE.md` requires — and it teaches this module its media for
//! free.
//!
//! # Evidence, and the names that are not evidence
//!
//! Not every volume name says something. Checked against two independent
//! sources rather than recalled:
//!
//! - HstWB Installer's own catalogue, `data/amiga-os-entries.csv` (78 rows,
//!   read 2026-08-24). It carries `Name;Required;Set;AmigaOsVersion;…;
//!   VolumeName;Filename;…`, and the required set for AmigaOS 3.1, 3.1.4 and
//!   3.2 alike is six disks: Workbench, Extras, **Fonts**, Install,
//!   **Locale**, Storage.
//! - The abime.net and pjhutchison.org 3.1 installation guides, which name the
//!   same six.
//!
//! `Workbench3.1`, `Workbench3.1.4` and `Workbench3.2` are three different
//! names. **`Fonts` and `Locale` are one name across all three releases** —
//! they carry no version suffix at all. ART's 3.2 recipe lists both, so a
//! folder holding nothing but `Fonts` and `Locale` matches ART's 3.2 signature
//! perfectly and is just as likely to be a 3.1 disk set. Calling that "AmigaOS
//! 3.2" is precisely the confident wrong sentence this project's most
//! expensive defects are made of, so those names are carried in the report and
//! **counted as nothing**.
//!
//! That list is the one hand-maintained thing in this file, it is four
//! entries long, each is cited, and [`AMBIGUOUS_ACROSS_RELEASES`] says so.
//!
//! # What it will not do
//!
//! **It will not name a release ART does not install.** A 3.5 or 3.1.4 disc
//! could be recognised and reported as "not one ART builds", which would be a
//! better sentence than silence — but ART has no 3.5 disc to read a volume
//! name off, and asserting one from recollection is how a tree that was
//! AmigaOS **3.5** shipped as 3.9 ([ART-159]). Unnamed is the honest answer
//! and [`MediaVerdict::Unknown`] is where it goes.
//!
//! **It does not hash anything.** HstWB identifies each disk by MD5 as well as
//! by volume name, which is strictly stronger evidence and which ART cannot
//! carry: a hash table is a claim about pressings nobody here can check, and
//! it goes stale silently. Volume names are read from the artefact every time.
//!
//! [ART-159]: ../../../../docs/ISSUES.md
//! [ART-222]: ../../../../docs/ISSUES.md

use std::path::Path;

use serde::Serialize;

use super::recipe;
use super::Recipe;
use crate::core::{CoreError, CoreResult};

/// Volume names that belong to more than one AmigaOS release, so their
/// presence says nothing about which one a folder holds.
///
/// **Four entries, each cited**, and deliberately not a general table:
///
/// | name | releases carrying it | source |
/// |---|---|---|
/// | `Fonts` | 3.1, 3.1.4, 3.2 | HstWB `amiga-os-entries.csv`; abime.net 3.1 guide |
/// | `Locale` | 3.1, 3.1.4, 3.2 | as above |
/// | `Storage` | 3.0 and earlier | the 3.1 set suffixes it (`Storage3.1`), earlier ones do not |
/// | `DiskDoctor` | AmigaOS 3.2 and 3.2.2 | ART's own two recipes (Task 8) |
///
/// `Storage` is here without a second source and is inert today — no shipped
/// recipe names it, so it can only ever be ignored — but leaving it out would
/// mean a bare `Storage` disk counting as evidence the day a recipe does.
///
/// **`DiskDoctor` used to be deliberately absent** — until Task 8, ART's 3.2
/// recipe covered 3.2.1 as well (the `update-3.2.1` placeholder off
/// `Update3.2.1`), so only one recipe ever named `DiskDoctor` and the name
/// really was evidence. AmigaOS 3.2.2 is now its own recipe with its **own**
/// `DiskDoctor` volume — measured byte-different from 3.2's (see
/// `core::osinstall` module doc comment, ART-224/ART-097's sibling note) —
/// so a bare `DiskDoctor` floppy found beside nothing else is genuinely
/// consistent with either release and must not tip [`identify`] into naming
/// the more specific one on a coincidence of labelling. The layering
/// subsumption below (`base`-aware) still lets AmigaOS 3.2.2's *other* own
/// media (`Update3.2.2` and the rest) settle it outright.
pub const AMBIGUOUS_ACROSS_RELEASES: [&str; 4] = ["Fonts", "Locale", "Storage", "DiskDoctor"];

/// What one release's signature made of the media actually in hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseEvidence {
    /// `"AmigaOS 3.2"` — the recipe's own `release`.
    pub release: String,
    /// Volume names present that this release names and that no other AmigaOS
    /// release is known to carry. **The only rows that count.**
    pub distinguishing: Vec<String>,
    /// Present, named by this release, and also carried by a release ART does
    /// not install ([`AMBIGUOUS_ACROSS_RELEASES`]). Reported so a person can
    /// see what was found, never counted.
    pub shared: Vec<String>,
    /// Media this release marks `required` that the folder does not hold.
    ///
    /// This is the field that makes a verdict useful rather than merely
    /// correct: *"this is AmigaOS 3.2 and `Install3.2` is missing"* sends
    /// somebody to a drawer, where `MediaMissing` at plan time sends them
    /// back to the beginning.
    pub missing_required: Vec<String>,
}

impl ReleaseEvidence {
    /// Nothing this release names was found at all — not even an ambiguous
    /// name. Such a candidate is dropped rather than reported as a zero.
    fn is_silent(&self) -> bool {
        self.distinguishing.is_empty() && self.shared.is_empty()
    }
}

/// What ART is willing to say about a folder of install media.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "verdict", rename_all = "kebab-case")]
pub enum MediaVerdict {
    /// One release, and no other release's distinguishing media in the way.
    ///
    /// Note what this does **not** promise: that every disk is here. Read
    /// [`ReleaseEvidence::missing_required`] before offering to build.
    Identified { evidence: ReleaseEvidence },
    /// Distinguishing media from more than one release. ART names them all
    /// and picks none — a folder holding both a 3.2 disk set and the 3.9 disc
    /// is a real thing a collector has, and guessing between them would be
    /// choosing for somebody who has not been asked.
    Ambiguous { candidates: Vec<ReleaseEvidence> },
    /// Nothing distinguishing was found, so ART says nothing.
    ///
    /// `shared_only` carries the names that *are* real install media but
    /// cannot separate one release from another, so the screen can say "these
    /// look like install disks, but `Fonts` and `Locale` are the same disks in
    /// 3.1 and 3.2" rather than "not recognised".
    Unknown { shared_only: Vec<String> },
}

/// Is `name` one of the names that cannot separate two releases?
fn is_ambiguous(name: &str) -> bool {
    AMBIGUOUS_ACROSS_RELEASES
        .iter()
        .any(|shared| super::amiga_names_equal(name, shared))
}

/// Name the release a set of volume names belongs to, or decline to.
///
/// Takes the names rather than a folder on purpose: `scan::find_media` has
/// already opened every image once and read each volume name from inside it,
/// and re-opening them here would be a second pass over a folder that can hold
/// thirty ADFs. It also keeps this function pure, which is what lets the tests
/// below cover the 3.1-versus-3.2 case ART has no 3.1 disks to reproduce.
///
/// Comparison is [`super::amiga_names_equal`] — the same international fold
/// `scan::media_for` matches media with, not `eq_ignore_ascii_case`.
pub fn identify(volume_names: &[String]) -> CoreResult<MediaVerdict> {
    let mut candidates: Vec<ReleaseEvidence> = Vec::new();

    for release in recipe::releases() {
        let recipe = recipe::by_release(release)?;
        let mut evidence = ReleaseEvidence {
            release: recipe.release.clone(),
            distinguishing: Vec::new(),
            shared: Vec::new(),
            missing_required: Vec::new(),
        };

        for component in &recipe.components {
            let present = volume_names
                .iter()
                .find(|found| super::amiga_names_equal(found, &component.media));

            match present {
                // Report the name the **medium** spells, not the recipe's —
                // the same reasoning ART-225 cost a day to learn on the other
                // side of this codebase, where destinations were retyped off a
                // listing. A disc that spells it `TÜRKÇE` should read back
                // `TÜRKÇE`.
                Some(found) => {
                    let bucket = if is_ambiguous(&component.media) {
                        &mut evidence.shared
                    } else {
                        &mut evidence.distinguishing
                    };
                    if !bucket.iter().any(|n| super::amiga_names_equal(n, found)) {
                        bucket.push(found.clone());
                    }
                }
                // Two components can mark the same disk required, so the
                // absence is recorded once — a person told `Install3.2` is
                // missing twice would go looking for two disks.
                None if component.required
                    && !evidence
                        .missing_required
                        .iter()
                        .any(|n| super::amiga_names_equal(n, &component.media)) =>
                {
                    evidence.missing_required.push(component.media.clone());
                }
                None => {}
            }
        }

        if !evidence.is_silent() {
            candidates.push(evidence);
        }
    }

    let mut named: Vec<ReleaseEvidence> = candidates
        .iter()
        .filter(|c| !c.distinguishing.is_empty())
        .cloned()
        .collect();

    // A based release (Task 8: `"AmigaOS 3.2.2"` on `"AmigaOS 3.2"`) inherits
    // its base's *entire* component list, so every one of the base's own
    // volume names is, correctly, also this release's own distinguishing
    // evidence — a folder holding nothing but `Workbench3.2` would otherwise
    // name both `"AmigaOS 3.2"` and `"AmigaOS 3.2.2"` at once, which is not
    // real ambiguity (a collector owning two unrelated releases' discs) but
    // one release being a superset of the other's signature.
    //
    // Resolved the same way a person would: **the update's own media, never
    // the inherited base's, is what tells the two apart.** If a based
    // release has found evidence among components that are genuinely its
    // own — declared on a layer other than the first, which `merge_base`
    // reserves for the stamped-in base — that release wins outright and its
    // base is dropped from `named`. If it found nothing but the base's own
    // names (the ordinary "just the base, nothing more" case), the based
    // release itself is dropped instead: it was never really evidenced, only
    // inherited into existence.
    //
    // **One own disk is not the whole update (fix round 1, Finding 3).** The
    // first version subsumed the base on *any* own-layer match, so a
    // complete AmigaOS 3.2 disk set plus a single stray AmigaOS 3.2.2 disk
    // (say, one locale update the owner happened to also have) answered
    // "AmigaOS 3.2.2" outright — a confident wrong sentence that sends a
    // user looking for the rest of an update set they never meant to
    // install. Gated on `missing_required.is_empty()` too: a based release
    // only wins when its *own* required media (`update-322-system`'s
    // `Update3.2.2`, not just any one of its optional locale disks) is
    // actually all present, the same bar `release_holding` already holds an
    // unbased release to.
    for release in recipe::releases() {
        let recipe = recipe::by_release(release)?;
        let Some(base_release) = recipe.base.clone() else {
            continue;
        };
        let first_layer = recipe.layers.first().map(|l| l.id.as_str());
        let own_media: std::collections::HashSet<&str> = recipe
            .components
            .iter()
            .filter(|c| c.layer.as_deref() != first_layer)
            .map(|c| c.media.as_str())
            .collect();

        let has_own_evidence = named
            .iter()
            .find(|c| c.release == *release)
            .is_some_and(|c| {
                c.missing_required.is_empty()
                    && c.distinguishing
                        .iter()
                        .any(|found| own_media.iter().any(|m| super::amiga_names_equal(found, m)))
            });

        if has_own_evidence {
            named.retain(|c| c.release != base_release);
        } else {
            named.retain(|c| c.release != *release);
        }
    }

    Ok(match named.len() {
        1 => MediaVerdict::Identified {
            evidence: named.into_iter().next().expect("length checked"),
        },
        0 => {
            // Every name found was one that cannot separate two releases —
            // or nothing was found at all.
            let mut shared_only: Vec<String> = Vec::new();
            for candidate in &candidates {
                for name in &candidate.shared {
                    if !shared_only
                        .iter()
                        .any(|n| super::amiga_names_equal(n, name))
                    {
                        shared_only.push(name.clone());
                    }
                }
            }
            MediaVerdict::Unknown { shared_only }
        }
        _ => MediaVerdict::Ambiguous { candidates: named },
    })
}

/// Which shipped release's own install media these volume names are —
/// `None` when they are nobody's, or more than one release's (ART-208).
///
/// **Why this exists.** The owner chose AmigaOS 3.2 with the folder holding
/// their AmigaOS 3.9 disc still selected, and `plan()` answered with sixteen
/// `MediaMissing` refusals: one per component, every one of them true, and
/// together telling them "a lot of programs are missing" about a folder that
/// was simply the wrong one. Sixteen correct sentences that add up to a wrong
/// impression are this project's most expensive class of defect, and the
/// information needed to replace them with one sentence was already in hand —
/// the volume names read out of the folder, and every recipe ART ships.
///
/// **Ambiguity is answered with silence, deliberately.** A folder somebody
/// keeps everything in matches two releases, and "both" is not something a
/// one-click switch can act on; naming one of two at random would be the
/// confident-and-wrong sentence again, in a smaller place. The caller's own
/// fallback — "none of these disks are what this release asks for" — is true
/// whatever the folder holds.
///
/// # ART-229: this used to answer "AmigaOS 3.2" about an AmigaOS 3.1 folder
///
/// It lived in `recipe.rs` until 2026-08-24 and counted **any** matching
/// volume name as proof, which made `Fonts` and `Locale` — carried unsuffixed
/// by the 3.1, 3.1.4 and 3.2 disk sets alike — enough on their own. Measured
/// before it was changed:
///
/// ```text
/// ["Fonts"]                            -> Some("AmigaOS 3.2")
/// ["Locale"]                           -> Some("AmigaOS 3.2")
/// ["Workbench3.1", "Fonts", "Locale"]  -> Some("AmigaOS 3.2")
/// ```
///
/// The third is the one that matters: a folder holding a real AmigaOS **3.1**
/// Workbench disk was announced as the user's 3.2 folder, and the screen it
/// feeds offers a one-click switch on the strength of that sentence. It now
/// delegates to [`identify`], which counts only names no other release
/// carries, and all three answer `None`.
///
/// It also moved here rather than staying put, because a `recipe` that asks
/// [`identify`] a question would invert the layering `CLAUDE.md` requires: a
/// lower `core/` module may not import a higher one, and `identify` is the one
/// that reads recipes.
pub fn release_holding(found: &[String]) -> CoreResult<Option<String>> {
    Ok(match identify(found)? {
        MediaVerdict::Identified { evidence } => Some(evidence.release),
        MediaVerdict::Ambiguous { .. } | MediaVerdict::Unknown { .. } => None,
    })
}

/// Which of `recipe`'s own layers this pile of media looks like — `None`
/// when `recipe` declares fewer than two layers (the question does not
/// apply), or when nothing distinguishing was found.
///
/// **The counterpart to [`release_holding`], one level down** (Task 10 fix
/// round, Finding 1). That answers "which release is this folder", scoped
/// across every shipped recipe; a layered release's own two layers share a
/// release name, so it cannot see the mistake this answers — the update
/// disks pointed at the base field, or the reverse. Scoped inside one
/// already-resolved recipe rather than iterating every release the way
/// [`identify`] does, because the question is "which of *this* release's own
/// layers", never "which release".
///
/// Same evidence rule as [`identify`]: a volume name only distinguishes a
/// layer when no *other* layer of this same recipe also names it.
/// `DiskDoctor` is AmigaOS 3.2.2's own example — both its `base` layer
/// (inherited from AmigaOS 3.2's own `DiskDoctor` component) and its
/// `update-3.2.2` layer (`update-322-diskdoctor`) name a component media of
/// `DiskDoctor`, so it decides nothing between the two, exactly as it
/// decides nothing between releases in [`AMBIGUOUS_ACROSS_RELEASES`] — here
/// discovered structurally, per recipe, rather than off a hand-maintained
/// list, since the two names being compared both come from the one recipe
/// already in hand.
///
/// The layer with the most distinguishing hits wins; a tie, or zero hits
/// everywhere, answers `None` rather than guessing.
pub fn layer_holding(recipe: &Recipe, volume_names: &[String]) -> Option<String> {
    if recipe.layers.len() < 2 {
        return None;
    }

    let media_of = |layer_id: &str| -> Vec<String> {
        recipe
            .components
            .iter()
            .filter(|c| c.layer.as_deref() == Some(layer_id))
            .map(|c| c.media.clone())
            .collect()
    };

    let mut best: Option<(&str, usize)> = None;
    let mut tied = false;
    for layer in &recipe.layers {
        let mine = media_of(&layer.id);
        let others: Vec<String> = recipe
            .layers
            .iter()
            .filter(|l| l.id != layer.id)
            .flat_map(|l| media_of(&l.id))
            .collect();
        let distinguishing: Vec<&String> = mine
            .iter()
            .filter(|name| !others.iter().any(|o| super::amiga_names_equal(o, name)))
            .collect();

        let hits = volume_names
            .iter()
            .filter(|found| {
                distinguishing
                    .iter()
                    .any(|d| super::amiga_names_equal(d, found))
            })
            .count();

        if hits == 0 {
            continue;
        }
        match best {
            None => best = Some((layer.id.as_str(), hits)),
            Some((_, best_hits)) if hits > best_hits => {
                best = Some((layer.id.as_str(), hits));
                tied = false;
            }
            Some((_, best_hits)) if hits == best_hits => tied = true,
            Some(_) => {}
        }
    }

    if tied {
        return None;
    }
    best.map(|(id, _)| id.to_string())
}

/// `Prefs/Env-Archive/Versions/Release`, relative to a distribution tree's
/// root — the one file this project trusts to say what a *built* tree is,
/// because a release wrote it, not because ART inferred it.
const RELEASE_MARKER_PATH: &str = "Prefs/Env-Archive/Versions/Release";

/// A real marker is 11 bytes (`Release 3.2`, the base disk's own copy) or 14
/// (`Release 3.2.2\n`, the update's). 256 is generous headroom over either,
/// not a real expectation of anything close to it — see [`release_of_tree`]'s
/// own doc comment for why a bound is enforced rather than merely typical.
const MAX_RELEASE_MARKER_BYTES: u64 = 256;

/// What a **built** tree's own `Prefs/Env-Archive/Versions/Release` states —
/// `None` when the tree carries no such file.
///
/// # This is the fix for shipping AmigaOS 3.5 labelled 3.9
///
/// [`identify`] above answers "which release is this pile of donor **media**"
/// — asked before a tree exists, from volume names nobody but Commodore or
/// Hyperion wrote. This function answers a different question, asked
/// *after*: what does the **tree ART actually built** say about itself? The
/// two must never be confused, because trusting the wrong kind of evidence
/// for this second question is exactly how ART's most expensive defect
/// happened.
///
/// A distribution tree that booted cleanly was shipped as AmigaOS **3.9** and
/// was really **3.5** — eight tasks and a merged branch later. The spec that
/// authorised it read the CD's own `Workbench3.5` top-level drawer name and a
/// copyright line, called both "and not a mistake", and moved on. Both are
/// consistent with more than one release; neither is the release itself
/// stating what it is. `version full`, run against the booted system, would
/// have answered in one line and did not get asked until after the branch
/// merged (CLAUDE.md's "ask the artefact" rule, and the "research before
/// design" section's own retelling of this exact round).
///
/// `Prefs/Env-Archive/Versions/Release` is that same kind of evidence, moved
/// from "ask the booted system" to "ask the tree ART is about to hand
/// someone" — a file AmigaOS's own installer writes, that ART never
/// generates and never edits. The base `Workbench3.2` disk carries it
/// already (`workbench-base` copies `Prefs` as a subtree, marker and all);
/// the 3.2.2 update ships its own copy at `Update/Release` and the update's
/// own `HowToInstall` has it overwrite the base's. A tree that carries
/// `Release 3.2.2` is a tree an update genuinely touched, stated by the
/// update itself — not by ART counting which components it switched on.
///
/// # Three answers, not two
///
/// Read [`super::apply::DistributionManifest::layers`]'s caller
/// (`commands::osinstall::osinstall_apply`) for where these three sentences
/// actually surface. This function only ever returns two things — the
/// marker's own text, trimmed, or `None` — and the *third* sentence (the
/// marker naming something ART did not expect) is a comparison the caller
/// makes against the release it built, never something this function
/// decides. Folding "found but wrong" and "not found" into one `None` would
/// destroy the distinction CLAUDE.md's "endings stay distinct" rule exists
/// for: an absent marker and a contradicting one call for different next
/// steps, and a caller that only sees `None` for both cannot tell them apart.
///
/// # Bounded, and refused rather than truncated
///
/// The file is 11 or 14 bytes on every release ART has read. Reading
/// whatever is at this path without a cap would mean a stray file — a user's
/// own note left at a name ART did not choose, or a future release that
/// changes the format in some way nobody here has seen — is loaded and
/// treated as this tree's own claim about itself with no idea how large it
/// might be. Refused outright past [`MAX_RELEASE_MARKER_BYTES`] rather than
/// silently reading a truncated prefix (the way `collide.rs`'s own
/// `read_bounded` does for a version-search window): a truncated *guess* at
/// what a release states is exactly the kind of confident wrong sentence this
/// function exists to stop making, so past the bound it says nothing rather
/// than something built from a fragment.
///
/// # Absent is `None`, never the recipe's own `release` string
///
/// A tree with no marker at all — every release ART ships except an update
/// layer — states no release, and that is the honest answer. Falling back to
/// what the recipe *says* it built (`plan.release`, e.g. `"AmigaOS 3.2.2"`)
/// would be ART asserting what it hoped rather than reading what the tree
/// actually carries — the 3.5-as-3.9 defect again, only moved one level
/// down. See the mutation table in the Task 9 implementation plan: that
/// exact fallback is the mutation `a_tree_with_no_marker_says_so_rather_than_guessing`
/// exists to catch.
pub fn release_of_tree(root: &Path) -> CoreResult<Option<String>> {
    let path = root.join(RELEASE_MARKER_PATH);

    let metadata = match std::fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(CoreError::Io(err)),
    };

    // Not `collide.rs`'s own `read_bounded(path, bound)` (fix round 1,
    // Finding 4): that helper opens the file and lets `Read::take(bound)`
    // silently hand back a truncated prefix, which is exactly right for a
    // version-search window that only ever wants "the first N bytes,
    // whatever they are" — but wrong here, where the whole point is to
    // refuse rather than guess at a release from a fragment (see this
    // function's own "Bounded, and refused rather than truncated" section
    // above). Checked against `metadata.len()` *before* any read, so an
    // oversized file is never opened at all.
    if metadata.len() > MAX_RELEASE_MARKER_BYTES {
        return Err(CoreError::Malformed {
            format: "release marker".into(),
            detail: format!(
                "'{}' is {} byte(s) — a real one is 11 or 14, and ART refuses to read past {} \
                 rather than guess at a release from a truncated fragment",
                path.display(),
                metadata.len(),
                MAX_RELEASE_MARKER_BYTES
            ),
        });
    }

    let bytes = std::fs::read(&path)?;
    // Latin-1, the same convention `apply.rs`'s own module doc comment
    // states for every AmigaDOS text file this project reads — a plain cast
    // that can never fail, unlike `String::from_utf8`.
    let text: String = bytes.iter().map(|&b| b as char).collect();
    let trimmed = text.trim_end();

    Ok(if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    })
}

/// The releases whose expected `Prefs/Env-Archive/Versions/Release` marker
/// has actually been **measured** against a real, correctly-built tree —
/// design §5's own table, reproduced on `core::rom::residents`'s doc
/// comment, is the same discipline applied here: "Release 3.2" and
/// "Release 3.2.2" were read off real trees before either was relied on.
/// AmigaOS 3.9 is deliberately absent — nobody has yet measured what a
/// correct 3.9 tree's own marker says, and 3.9 is built from a 3.5 base
/// layer plus an overlay, so guessing "Release 3.9" the same way the other
/// two are guessed is exactly the "the header answers it" mistake this
/// project keeps re-finding (CLAUDE.md, "research before design"). Naming a
/// release here is a claim that has been checked; add one only after
/// measuring it the same way, never merely to silence
/// [`StatedRelease::ExpectedUnknown`].
const MEASURED_RELEASE_MARKERS: &[&str] = &["AmigaOS 3.2", "AmigaOS 3.2.2"];

/// The five sentences a finished build's own marker can produce, compared
/// with the release ART built the tree as — see [`stated_release`] and
/// [`release_of_tree`]'s own doc comment for why there are this many and why
/// they may never collapse into a pass/fail.
///
/// **`Unreadable` is its own variant, not folded into [`Unstated`](Self::Unstated)**
/// — fix round 1, Finding 1. `commands::osinstall::osinstall_apply` used to
/// map any error out of [`release_of_tree`] (an oversized or otherwise
/// unreadable marker) onto the same `Unstated` a tree that honestly carries
/// no marker produces, which is the identical defect shape Task 7's own
/// review fixed one task earlier in this round: `core::osinstall::plan`
/// used to fold "the resident table could not be read" into the same empty
/// `Vec` as "this Kickstart has no such resident", and `RefusalReason` grew
/// its own `ResidentTableUnreadable` variant rather than let a read failure
/// impersonate a genuine absence. "This tree states no release" and "ART
/// could not find out what this tree states" are two different facts and
/// send a user to two different places — the first is ordinary for most
/// releases, the second means something is wrong with the tree or the read
/// itself and is worth a bug report. Collapsing them here would let a
/// corrupted or oversized marker read back exactly like an unremarkable
/// AmigaOS 3.2 tree, which is the same "endings stay distinct" rule this
/// whole task exists to enforce, broken inside the task itself.
///
/// **`ExpectedUnknown` is its own variant too, not folded into `Mismatch`**
/// (final whole-branch review, Finding E). `Mismatch` is a claim that the
/// tree is wrong, and that claim is only honest for a release whose expected
/// marker `MEASURED_RELEASE_MARKERS` actually names — for AmigaOS 3.9, the
/// "expected" string above is a guessed formula nobody has checked against a
/// real tree, and a correct 3.9 build whose marker reads something else
/// (the 3.5 base layer's own, say) must never be told it is wrong for a
/// mistake that may be entirely ART's guess. Never folded into `Confirmed`
/// either — the two strings really do differ, and saying so plainly, without
/// judging which side is right, is what `stated` is for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "verdict", rename_all = "kebab-case")]
pub enum StatedRelease {
    /// The tree's own marker names the release ART built it as.
    Confirmed { stated: String },
    /// The tree's own marker names something else, for a release whose
    /// expected marker `MEASURED_RELEASE_MARKERS` confirms — both sides are
    /// reported; which one is right is not this module's call to make.
    Mismatch { expected: String, stated: String },
    /// The tree carries no marker at all — not a failure. Most releases ART
    /// ships have never had an `Update/Release` to overwrite the base's own
    /// copy with; only a layered update does.
    Unstated,
    /// [`release_of_tree`] could not answer at all — the marker is larger
    /// than ART will read, or some other I/O error stopped the read. Never
    /// the same sentence as [`Unstated`](Self::Unstated): the tree may well
    /// state a release, ART simply could not read it. `detail` carries the
    /// core's own sentence, never claimed away (CLAUDE.md: "the screen may
    /// not out-claim the core").
    Unreadable { detail: String },
    /// The tree states `stated`, and it differs from the guessed formula —
    /// but this build's own release is not in `MEASURED_RELEASE_MARKERS`, so
    /// ART has never checked what a correct tree for it actually writes
    /// there. Reporting a `Mismatch` here would be the confident-wrong
    /// sentence CLAUDE.md warns against: a correct tree told it is wrong,
    /// sent looking for a fix that does not exist.
    ExpectedUnknown { stated: String },
}

/// Compare [`release_of_tree`] against the release a build was made for.
///
/// `expected_release` is a recipe's own `release` string (`"AmigaOS
/// 3.2.2"`); the marker's own wording carries no `AmigaOS` prefix
/// (`"Release 3.2.2"`), so the comparison strips the expected release's own
/// first word rather than growing a second hand-maintained table for a
/// spelling rule that is really just "the part after the space".
///
/// **Infallible on purpose** (fix round 1, Finding 1). A `CoreResult` return
/// here would leave every caller to decide for itself what an `Err` means,
/// which is exactly how the fold into `Unstated` happened the first time —
/// this function is the single place that decision gets made, once, into
/// [`StatedRelease::Unreadable`], so a caller cannot quietly re-fold the two
/// apart ever again.
///
/// **A disagreement is only ever reported as `Mismatch` for a release this
/// module has actually measured** (final whole-branch review, Finding E) —
/// see [`MEASURED_RELEASE_MARKERS`]. For any other release, a differing
/// marker comes back as [`StatedRelease::ExpectedUnknown`] instead: what the
/// tree states, without judging it against a formula nobody has checked.
pub fn stated_release(root: &Path, expected_release: &str) -> StatedRelease {
    let expected_marker = format!(
        "Release {}",
        expected_release
            .split_once(' ')
            .map_or(expected_release, |(_, version)| version)
    );
    let expected_is_measured = MEASURED_RELEASE_MARKERS.contains(&expected_release);

    match release_of_tree(root) {
        Ok(None) => StatedRelease::Unstated,
        Ok(Some(stated)) if stated == expected_marker => StatedRelease::Confirmed { stated },
        Ok(Some(stated)) if expected_is_measured => StatedRelease::Mismatch {
            expected: expected_marker,
            stated,
        },
        Ok(Some(stated)) => StatedRelease::ExpectedUnknown { stated },
        Err(err) => StatedRelease::Unreadable {
            detail: err.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    fn identified(list: &[&str]) -> ReleaseEvidence {
        match identify(&names(list)).expect("the shipped recipes must load") {
            MediaVerdict::Identified { evidence } => evidence,
            other => panic!("expected one release, got {other:?}"),
        }
    }

    // ART-208, moved here with the function on 2026-08-24 -----------------
    //
    // The owner chose AmigaOS 3.2 with the folder holding their AmigaOS 3.9
    // disc still selected, and got sixteen `MediaMissing` refusals — one per
    // component, each of them true, and together telling them "a lot of
    // programs are missing" about a folder that was simply the wrong one.
    //
    // ART already knows better than that: it has the volume names it read out
    // of the folder, and it has every shipped recipe. If the names it found
    // are one release's own media, it can say which — and offer the switch
    // instead of listing sixteen absences.

    #[test]
    fn names_the_release_whose_media_a_folder_actually_holds() {
        let found = vec!["AmigaOS3.9".to_string()];
        assert_eq!(
            release_holding(&found).unwrap().as_deref(),
            Some("AmigaOS 3.9")
        );
    }

    #[test]
    fn names_the_other_release_the_same_way_round() {
        // Not a mirror for symmetry's sake: a check that answers "3.9" for
        // everything would pass the test above.
        let found = vec![
            "Workbench3.2".to_string(),
            "Locale-TR".to_string(),
            "Storage3.2".to_string(),
        ];
        assert_eq!(
            release_holding(&found).unwrap().as_deref(),
            Some("AmigaOS 3.2")
        );
    }

    #[test]
    fn a_folder_is_identified_the_way_its_disks_are_matched() {
        // `scan::media_for` folds case through `amiga_names_equal`, so a disc
        // labelled in upper case still resolves. This has to fold the same
        // way: identifying a folder by a stricter rule than the one that
        // refused it would let ART announce "this is your 3.9 folder" about a
        // folder whose disks it had just matched, or the reverse.
        let found = vec!["AMIGAOS3.9".to_string()];
        assert_eq!(
            release_holding(&found).unwrap().as_deref(),
            Some("AmigaOS 3.9")
        );
    }

    #[test]
    fn names_no_release_for_media_no_recipe_asks_for() {
        let found = vec!["MyBackup".to_string(), "Games".to_string()];
        assert_eq!(release_holding(&found).unwrap(), None);
    }

    #[test]
    fn names_no_release_when_the_folder_holds_two_releases_media() {
        // A folder somebody keeps everything in answers "both", and "both" is
        // not an answer a one-click switch can act on. Saying nothing is
        // better than naming one of two at random.
        let found = vec!["Workbench3.2".to_string(), "AmigaOS3.9".to_string()];
        assert_eq!(release_holding(&found).unwrap(), None);
    }

    #[test]
    fn names_no_release_for_an_empty_folder() {
        // "This folder holds no install media at all" is a different sentence
        // the screen already has, and it must not be overwritten by a guess.
        assert_eq!(release_holding(&[]).unwrap(), None);
    }

    // layer_holding — Task 10 fix round, Finding 1 --------------------------
    //
    // The mistake a two-field screen invites and `release_holding` cannot
    // see: both AmigaOS 3.2.2's layers belong to the one release, so pointing
    // the update disks at the base field (or the reverse) answers "yes, this
    // is your AmigaOS 3.2.2 media" at the release level while being exactly
    // backwards at the layer level.

    #[test]
    fn names_the_update_layer_by_its_own_disk() {
        let recipe = recipe::by_release("AmigaOS 3.2.2").expect("the shipped recipe must load");
        assert_eq!(
            layer_holding(&recipe, &names(&["Update3.2.2"])),
            Some("update-3.2.2".to_string())
        );
    }

    #[test]
    fn names_the_base_layer_the_same_way_round() {
        // Not a mirror for symmetry's sake — see `names_the_other_release_the_same_way_round`
        // above for why this project writes the reverse case rather than
        // trusting the first one implies it.
        let recipe = recipe::by_release("AmigaOS 3.2.2").expect("the shipped recipe must load");
        assert_eq!(
            layer_holding(&recipe, &names(&["Workbench3.2"])),
            Some("base".to_string())
        );
    }

    #[test]
    fn a_name_both_layers_carry_decides_nothing() {
        // `DiskDoctor`: the base layer's own (inherited from AmigaOS 3.2) and
        // the update layer's own (`update-322-diskdoctor`) both name it, so
        // it must not tip this toward either — the same rule
        // `AMBIGUOUS_ACROSS_RELEASES` states for `identify`, discovered here
        // structurally rather than off a hand-maintained list.
        let recipe = recipe::by_release("AmigaOS 3.2.2").expect("the shipped recipe must load");
        assert_eq!(layer_holding(&recipe, &names(&["DiskDoctor"])), None);
    }

    #[test]
    fn a_mix_of_both_layers_disks_answers_by_the_larger_pile() {
        // A folder holding two update disks and one base disk (a plausible
        // "I keep everything together" folder) names the layer with more
        // evidence rather than refusing outright — `identify`'s own
        // `Ambiguous` shape is for two different *releases*' worth of
        // evidence, which is not what one release's own two layers sharing a
        // folder means.
        let recipe = recipe::by_release("AmigaOS 3.2.2").expect("the shipped recipe must load");
        assert_eq!(
            layer_holding(
                &recipe,
                &names(&["Update3.2.2", "Classes3.2.2", "Workbench3.2"])
            ),
            Some("update-3.2.2".to_string())
        );
    }

    #[test]
    fn nothing_distinguishing_answers_none() {
        let recipe = recipe::by_release("AmigaOS 3.2.2").expect("the shipped recipe must load");
        assert_eq!(layer_holding(&recipe, &names(&["MyBackup"])), None);
        assert_eq!(layer_holding(&recipe, &[]), None);
    }

    #[test]
    fn an_unlayered_recipe_never_answers() {
        // The question does not apply — AmigaOS 3.2 and 3.9 both read from
        // one implicit layer, so there is nothing to tell apart.
        let recipe = recipe::by_release("AmigaOS 3.2").expect("the shipped recipe must load");
        assert_eq!(
            layer_holding(&recipe, &names(&["Workbench3.2"])),
            None,
            "an unlayered recipe has nothing for this question to answer"
        );
    }

    /// A minimal component naming one volume, on one layer — everything else
    /// is a value no shipped recipe would actually vary, held constant so the
    /// synthetic recipe below reads as data rather than boilerplate.
    fn layer_component(layer: &str, media: &str) -> super::super::Component {
        super::super::Component {
            id: format!("{layer}-{media}"),
            media: media.to_string(),
            rules: vec![],
            required: false,
            condition: None,
            overrides: vec![],
            user_startup: vec![],
            activate: vec![],
            exclusive_group: None,
            label_key: None,
            available: true,
            layer: Some(layer.to_string()),
            removes: Vec::new(),
        }
    }

    /// **Why a synthetic recipe, when this file's own doctrine is "the table
    /// is the recipes".** That doctrine is about not hand-maintaining a
    /// *name* table `identify` could instead read off shipped JSON — it is
    /// not a ban on a pure function's own unit test choosing its inputs.
    /// Every shipped recipe has exactly two layers today, and a shared name's
    /// exclusion can only ever move a *tied* pair of layers, never flip which
    /// one wins, when there are only two: the shared name adds the same
    /// count to both sides. Three layers is the smallest shape where
    /// excluding it is observably different from not — one layer's own
    /// disk, one it shares with a second, and a third with a real disk of
    /// its own — which no shipped recipe has any reason to need. See
    /// `a_shared_names_exclusion_can_hide_a_real_winner_among_three_layers`
    /// below for what this recipe is for.
    fn three_layer_recipe() -> Recipe {
        Recipe {
            release: "Test".to_string(),
            base: None,
            layers: vec![
                super::super::MediaLayer {
                    id: "a".to_string(),
                    label_key: None,
                },
                super::super::MediaLayer {
                    id: "b".to_string(),
                    label_key: None,
                },
                super::super::MediaLayer {
                    id: "c".to_string(),
                    label_key: None,
                },
            ],
            components: vec![
                layer_component("a", "OnlyA"),
                layer_component("a", "Shared"),
                layer_component("b", "OnlyB"),
                layer_component("b", "Shared"),
                layer_component("c", "OnlyC"),
            ],
        }
    }

    /// **Why `a_name_both_layers_carry_decides_nothing` cannot be the whole
    /// guard.** Mutating away the shared-name exclusion still passed that
    /// test: with exactly two layers, adding the same count to both sides of
    /// a comparison can only ever preserve or create a tie, never flip a
    /// winner, and the mutated code's own tie-handling silently absorbed the
    /// difference. This is the case where the distinction is real: `Shared`
    /// is `a`'s and `b`'s alike, so it must decide nothing between them —
    /// but `c`'s own disk should still win outright, which the two-layer
    /// case has no way to exercise (a two-layer tie and "nothing decided"
    /// look the same either way).
    #[test]
    fn a_shared_names_exclusion_can_hide_a_real_winner_among_three_layers() {
        let recipe = three_layer_recipe();
        assert_eq!(
            layer_holding(&recipe, &names(&["Shared", "OnlyC"])),
            Some("c".to_string()),
            "one disk shared by two layers plus a third layer's own disk \
             names the third layer outright, not a three-way tie"
        );
    }

    /// **ART-229, the three measured cases.** Pinned as a table rather than
    /// as prose because every one of them answered `Some("AmigaOS 3.2")`
    /// before 2026-08-24, and the third is a folder holding a real AmigaOS
    /// **3.1** Workbench disk.
    #[test]
    fn a_3_1_disk_set_is_no_longer_announced_as_3_2_art_229() {
        for found in [
            vec!["Fonts".to_string()],
            vec!["Locale".to_string()],
            vec!["Fonts".to_string(), "Locale".to_string()],
            vec![
                "Workbench3.1".to_string(),
                "Fonts".to_string(),
                "Locale".to_string(),
            ],
        ] {
            assert_eq!(
                release_holding(&found).unwrap(),
                None,
                "{found:?} is as much a 3.1 disk set as a 3.2 one, and the screen \
                 this feeds offers a one-click release switch on the answer"
            );
        }
    }

    #[test]
    fn a_3_2_disk_set_is_named_amigaos_3_2() {
        let evidence = identified(&["Workbench3.2", "Install3.2", "Extras3.2", "Storage3.2"]);
        assert_eq!(evidence.release, "AmigaOS 3.2");
        assert!(evidence
            .distinguishing
            .contains(&"Workbench3.2".to_string()));
        assert!(evidence.missing_required.is_empty());
    }

    /// **Fix round 1, Finding 3.** A complete AmigaOS 3.2 disk set plus one
    /// stray AmigaOS 3.2.2 disk that happens not to be required
    /// (`Locale3.2.2-DE`, one optional locale update) used to answer "AmigaOS
    /// 3.2.2" outright — the base-subsumption pass triggered on *any* own
    /// evidence, so one locale disk was enough to claim the whole update.
    /// That is a confident wrong sentence: `Update3.2.2`, the update's own
    /// required media, is not here, so this is a 3.2 folder with an extra
    /// disk in it, not a 3.2.2 folder. Gating subsumption on the based
    /// release's own `missing_required` being empty is what keeps this one
    /// disk from claiming the whole update.
    #[test]
    fn one_stray_322_disk_beside_a_complete_32_set_does_not_claim_the_whole_update() {
        let evidence = identified(&[
            "Workbench3.2",
            "Install3.2",
            "Extras3.2",
            "Storage3.2",
            "Locale3.2.2-DE",
        ]);
        assert_eq!(
            evidence.release, "AmigaOS 3.2",
            "one optional update disk must not promote this to AmigaOS 3.2.2 while \
             Update3.2.2 itself is still missing"
        );
        assert!(evidence.missing_required.is_empty());
    }

    /// **This is also the de-duplication guard**, which is not obvious and was
    /// found by mutation rather than by design: the 3.9 recipe is *six*
    /// components off one disc, so without the check in `identify` this reads
    /// `["AmigaOS3.9"; 6]` and a person is told their disc is six discs.
    #[test]
    fn the_3_9_disc_is_named_amigaos_3_9() {
        let evidence = identified(&["AmigaOS3.9"]);
        assert_eq!(evidence.release, "AmigaOS 3.9");
        assert_eq!(evidence.distinguishing, vec!["AmigaOS3.9".to_string()]);
    }

    /// **The one this module exists for.** `Fonts` and `Locale` are the same
    /// volume names in the AmigaOS 3.1 disk set as in 3.2 — verified against
    /// HstWB's catalogue and the abime.net 3.1 guide — so a folder holding
    /// only those two matches ART's 3.2 signature and is just as likely to be
    /// 3.1. Naming it would be the confident wrong sentence.
    #[test]
    fn fonts_and_locale_alone_name_nothing() {
        match identify(&names(&["Fonts", "Locale"])).unwrap() {
            MediaVerdict::Unknown { shared_only } => {
                assert_eq!(shared_only.len(), 2, "both are reported, neither counts");
                assert!(shared_only.contains(&"Fonts".to_string()));
                assert!(shared_only.contains(&"Locale".to_string()));
            }
            other => panic!("a 3.1 disk set must not be called 3.2: {other:?}"),
        }
    }

    /// The other half of that: one suffixed disk beside them settles it, and
    /// the ambiguous pair is still reported rather than swallowed.
    #[test]
    fn one_suffixed_disk_beside_them_settles_it() {
        let evidence = identified(&["Fonts", "Locale", "Workbench3.2"]);
        assert_eq!(evidence.release, "AmigaOS 3.2");
        assert_eq!(evidence.distinguishing, vec!["Workbench3.2".to_string()]);
        let mut shared = evidence.shared.clone();
        shared.sort();
        assert_eq!(shared, vec!["Fonts".to_string(), "Locale".to_string()]);
    }

    #[test]
    fn a_folder_holding_both_releases_names_both() {
        match identify(&names(&["Workbench3.2", "Install3.2", "AmigaOS3.9"])).unwrap() {
            MediaVerdict::Ambiguous { candidates } => {
                let mut releases: Vec<&str> =
                    candidates.iter().map(|c| c.release.as_str()).collect();
                releases.sort();
                assert_eq!(releases, vec!["AmigaOS 3.2", "AmigaOS 3.9"]);
            }
            other => panic!("both sets are present; ART may not pick one: {other:?}"),
        }
    }

    /// The field that makes a verdict actionable rather than merely right.
    #[test]
    fn a_named_release_says_which_required_disk_is_missing() {
        let evidence = identified(&["Workbench3.2", "Extras3.2"]);
        assert_eq!(evidence.release, "AmigaOS 3.2");
        assert_eq!(
            evidence.missing_required,
            vec!["Install3.2".to_string()],
            "the user owns Workbench but not Install, and the sentence has to say so"
        );
    }

    #[test]
    fn an_empty_folder_says_nothing_and_names_nothing() {
        match identify(&[]).unwrap() {
            MediaVerdict::Unknown { shared_only } => assert!(shared_only.is_empty()),
            other => panic!("nothing in hand is not evidence of anything: {other:?}"),
        }
    }

    #[test]
    fn something_that_is_not_install_media_at_all_says_nothing() {
        match identify(&names(&["Empty", "MyGames", "Workbench"])).unwrap() {
            MediaVerdict::Unknown { shared_only } => assert!(
                shared_only.is_empty(),
                "'Workbench' bare is not one of the three shared names, and no recipe \
                 names it either"
            ),
            other => panic!("expected silence, got {other:?}"),
        }
    }

    /// Volume names are compared the way `scan::media_for` compares them, so
    /// a disk whose name is upper case is the same disk.
    #[test]
    fn the_comparison_is_the_one_media_for_uses() {
        let evidence = identified(&["WORKBENCH3.2", "INSTALL3.2"]);
        assert_eq!(evidence.release, "AmigaOS 3.2");
        assert!(
            evidence.missing_required.is_empty(),
            "both required disks are here, spelt loudly"
        );
        assert_eq!(
            evidence.distinguishing,
            vec!["WORKBENCH3.2".to_string(), "INSTALL3.2".to_string()],
            "reported in the medium's own spelling, not the recipe's"
        );
    }

    /// Two copies of the same ADF in one folder are one disk, not two votes.
    ///
    /// **What actually holds this up is `find`, not the de-duplication**, and
    /// saying so is the point of the comment: `identify` asks once per
    /// *component* and takes the first name that matches, so a second copy is
    /// never looked at. Removing the de-duplication does not fail this test —
    /// it fails [`the_3_9_disc_is_named_amigaos_3_9`], where six components
    /// share one disc. Both properties are real and they are guarded in
    /// different places; a reader who assumed this test covered both would be
    /// wrong, which is why the mutation round is recorded here.
    #[test]
    fn the_same_disk_twice_is_counted_once() {
        let evidence = identified(&["Workbench3.2", "workbench3.2", "Install3.2"]);
        assert_eq!(evidence.distinguishing.len(), 2);
    }

    /// Driven from the shipped recipes rather than from a list here, so
    /// adding a release cannot leave this module behind: every release
    /// `releases()` offers must be namable from its own required media alone.
    #[test]
    fn every_shipped_release_can_name_itself() {
        for release in recipe::releases() {
            let recipe = recipe::by_release(release).unwrap();
            let required: Vec<String> = recipe
                .components
                .iter()
                .filter(|c| c.required)
                .map(|c| c.media.clone())
                .collect();
            assert!(
                !required.is_empty(),
                "'{release}' marks no component required, so nothing about a folder could \
                 ever be said to be missing"
            );
            match identify(&required).unwrap() {
                MediaVerdict::Identified { evidence } => {
                    assert_eq!(evidence.release, *release);
                    assert!(evidence.missing_required.is_empty());
                }
                other => panic!("'{release}' cannot name itself from its own disks: {other:?}"),
            }
        }
    }

    /// The cited list is the one hand-maintained thing here, so it is checked
    /// against the recipes rather than trusted: a name that no shipped recipe
    /// mentions can only ever be inert, and one that two *unrelated* recipes
    /// mention would be a real collision this module resolves the wrong way
    /// round.
    ///
    /// **A base and the release it is inherited into are the one exception**
    /// (Task 8). `"AmigaOS 3.2.2"` carries every one of `"AmigaOS 3.2"`'s own
    /// components verbatim, so `Fonts`, `Locale` and now `DiskDoctor` are
    /// named by both **on purpose** — that is inheritance, not two
    /// independent recipes coincidentally choosing the same volume name, and
    /// `identify`'s own base-subsumption pass is what actually resolves it.
    /// Two releases sharing a name without one being the other's `base` is
    /// still refused: that combination genuinely needs a decision this
    /// module does not make.
    #[test]
    fn every_ambiguous_name_is_either_inert_or_named_by_a_base_and_its_own_release() {
        for shared in AMBIGUOUS_ACROSS_RELEASES {
            let mut naming: Vec<&str> = Vec::new();
            for release in recipe::releases() {
                let recipe = recipe::by_release(release).unwrap();
                if recipe
                    .components
                    .iter()
                    .any(|c| super::super::amiga_names_equal(&c.media, shared))
                {
                    naming.push(release);
                }
            }
            let allowed = naming.len() <= 1 || {
                naming.len() == 2 && {
                    let a = recipe::by_release(naming[0]).unwrap();
                    let b = recipe::by_release(naming[1]).unwrap();
                    a.base.as_deref() == Some(naming[1]) || b.base.as_deref() == Some(naming[0])
                }
            };
            assert!(
                allowed,
                "'{shared}' is named by {naming:?}; two shipped recipes sharing an \
                 already-ambiguous name, neither based on the other, needs a decision this \
                 module does not make"
            );
        }
    }

    // ---- Task 9: `release_of_tree` and `stated_release` ------------------

    /// **The one this task exists for.** A tree that carries the marker a
    /// real update wrote states its own release — read back, not asserted.
    #[test]
    fn a_tree_states_its_own_release_from_the_file_the_release_wrote() {
        let dir = crate::core::osinstall::fixtures::scratch("release-marker");
        let root = dir.join("tree");
        std::fs::create_dir_all(root.join("Prefs/Env-Archive/Versions")).unwrap();
        std::fs::write(
            root.join("Prefs/Env-Archive/Versions/Release"),
            b"Release 3.2.2\n",
        )
        .unwrap();
        assert_eq!(
            release_of_tree(&root).unwrap().as_deref(),
            Some("Release 3.2.2")
        );
    }

    /// The base disk's own copy carries no trailing newline at all (measured
    /// 11 bytes) — trimming must not depend on one being there.
    #[test]
    fn the_base_disks_own_marker_with_no_trailing_newline_reads_back_too() {
        let dir = crate::core::osinstall::fixtures::scratch("release-marker-base");
        let root = dir.join("tree");
        std::fs::create_dir_all(root.join("Prefs/Env-Archive/Versions")).unwrap();
        std::fs::write(
            root.join("Prefs/Env-Archive/Versions/Release"),
            b"Release 3.2",
        )
        .unwrap();
        assert_eq!(
            release_of_tree(&root).unwrap().as_deref(),
            Some("Release 3.2")
        );
    }

    /// **The other half of the point.** No marker is `None`, never a guess
    /// built from the recipe's own `release` string — that fallback is
    /// exactly the AmigaOS-3.5-shipped-as-3.9 defect, moved one level down.
    #[test]
    fn a_tree_with_no_marker_says_so_rather_than_guessing() {
        let dir = crate::core::osinstall::fixtures::scratch("release-marker-absent");
        let root = dir.join("tree");
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(release_of_tree(&root).unwrap(), None);
    }

    /// Bounded, and refused rather than truncated — reading a fragment of an
    /// oversized file and calling it "the release" would be exactly the
    /// confident-wrong-sentence shape this function exists to avoid.
    #[test]
    fn a_marker_far_larger_than_any_real_one_is_refused_not_truncated() {
        let dir = crate::core::osinstall::fixtures::scratch("release-marker-oversized");
        let root = dir.join("tree");
        std::fs::create_dir_all(root.join("Prefs/Env-Archive/Versions")).unwrap();
        std::fs::write(
            root.join("Prefs/Env-Archive/Versions/Release"),
            vec![b'X'; (MAX_RELEASE_MARKER_BYTES + 1) as usize],
        )
        .unwrap();
        let err = release_of_tree(&root).expect_err("oversized marker must be refused");
        assert!(
            format!("{err}").contains("release marker"),
            "the refusal must name what it refused: {err}"
        );
    }

    #[test]
    fn stated_release_confirms_a_matching_marker() {
        let dir = crate::core::osinstall::fixtures::scratch("stated-release-confirmed");
        let root = dir.join("tree");
        std::fs::create_dir_all(root.join("Prefs/Env-Archive/Versions")).unwrap();
        std::fs::write(
            root.join("Prefs/Env-Archive/Versions/Release"),
            b"Release 3.2.2",
        )
        .unwrap();
        assert_eq!(
            stated_release(&root, "AmigaOS 3.2.2"),
            StatedRelease::Confirmed {
                stated: "Release 3.2.2".to_string()
            }
        );
    }

    /// The sentence naming both sides — never just "this looks wrong".
    #[test]
    fn stated_release_names_both_sides_of_a_mismatch() {
        let dir = crate::core::osinstall::fixtures::scratch("stated-release-mismatch");
        let root = dir.join("tree");
        std::fs::create_dir_all(root.join("Prefs/Env-Archive/Versions")).unwrap();
        std::fs::write(
            root.join("Prefs/Env-Archive/Versions/Release"),
            b"Release 3.2",
        )
        .unwrap();
        assert_eq!(
            stated_release(&root, "AmigaOS 3.2.2"),
            StatedRelease::Mismatch {
                expected: "Release 3.2.2".to_string(),
                stated: "Release 3.2".to_string()
            }
        );
    }

    /// **Final review, Finding E.** AmigaOS 3.9 is not in
    /// `MEASURED_RELEASE_MARKERS`, so a 3.9 tree whose marker differs from
    /// the guessed `"Release 3.9"` formula — the 3.5 base layer's own
    /// `Prefs/Env-Archive/Versions/Release`, unmeasured and unoverwritten by
    /// the 3.9 overlay, is exactly the shape that could produce this — must
    /// never come back as `Mismatch`. A correct AmigaOS 3.9 build must not
    /// be told it is wrong for a formula nobody has checked.
    #[test]
    fn stated_release_reports_a_differing_marker_as_expected_unknown_for_an_unmeasured_release() {
        let dir = crate::core::osinstall::fixtures::scratch("stated-release-expected-unknown");
        let root = dir.join("tree");
        std::fs::create_dir_all(root.join("Prefs/Env-Archive/Versions")).unwrap();
        std::fs::write(
            root.join("Prefs/Env-Archive/Versions/Release"),
            b"Release 3.5",
        )
        .unwrap();
        assert_eq!(
            stated_release(&root, "AmigaOS 3.9"),
            StatedRelease::ExpectedUnknown {
                stated: "Release 3.5".to_string()
            },
            "AmigaOS 3.9's own expected marker has never been measured, so a differing \
             marker must be reported plainly, never asserted as a mismatch"
        );
    }

    #[test]
    fn stated_release_is_unstated_for_a_tree_with_no_marker() {
        let dir = crate::core::osinstall::fixtures::scratch("stated-release-unstated");
        let root = dir.join("tree");
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(
            stated_release(&root, "AmigaOS 3.2"),
            StatedRelease::Unstated
        );
    }

    /// **Fix round 1, Finding 1 — the covering test the first report
    /// admitted it was missing.** An oversized marker forces
    /// `release_of_tree` to return `Err`, and this is the exact call
    /// `osinstall_apply`'s job closure makes (`stated_release` is the whole
    /// of what runs there now — there is no separate folding logic left in
    /// `commands::osinstall` for the two to disagree about). The assertion
    /// is the specific fourth variant, never merely "this is not
    /// `Confirmed`": before this fix, this tree's honestly-unreadable
    /// marker and a tree that truly states nothing both answered
    /// `Unstated`, and a test that only checked "not confirmed" would have
    /// passed against that defect exactly as happily as against the fix.
    #[test]
    fn stated_release_is_unreadable_not_unstated_when_the_marker_cannot_be_read() {
        let dir = crate::core::osinstall::fixtures::scratch("stated-release-unreadable");
        let root = dir.join("tree");
        std::fs::create_dir_all(root.join("Prefs/Env-Archive/Versions")).unwrap();
        std::fs::write(
            root.join("Prefs/Env-Archive/Versions/Release"),
            vec![b'X'; (MAX_RELEASE_MARKER_BYTES + 1) as usize],
        )
        .unwrap();

        match stated_release(&root, "AmigaOS 3.2") {
            StatedRelease::Unreadable { detail } => {
                assert!(
                    detail.contains("release marker"),
                    "the core's own sentence must be carried, not swallowed: {detail}"
                );
            }
            other => panic!(
                "an unreadable marker must never answer the same as a tree that honestly \
                 states nothing: {other:?}"
            ),
        }
    }
}
