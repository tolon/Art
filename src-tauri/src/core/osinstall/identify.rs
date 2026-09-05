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

use serde::Serialize;

use super::recipe;
use crate::core::CoreResult;

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
}
