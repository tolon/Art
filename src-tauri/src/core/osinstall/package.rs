//! What a package recipe says: which files an official (or unofficial)
//! update archive contributes, and where each one goes.
//!
//! A package is the same shape a release recipe already uses — [`Component`]
//! — because it goes through the same placer once a future task applies it.
//! What differs is the wrapper: a package is chosen by the user, never
//! switched on by a ROM version (`Component.required` is always `false` and
//! `.condition` always `None`, see [`parse`]), it may depend on another
//! package having been applied first (`requires`), and its media may not be
//! the archive a person downloaded at all but a payload archive nested
//! inside it (`member`, read via
//! [`source_archive::ArchiveSource::open_nested`](super::source_archive::ArchiveSource::open_nested)).
//!
//! ## The mechanism, copied from `recipe.rs`
//!
//! `include_str!` for the same three reasons: reviewable in a diff, shipped
//! without a network, unable to grow a code path. [`packages`] is the one
//! list a fourth package JSON has to join to be reachable at all — from
//! there, [`by_id`] resolving it, `recipe.rs`'s widened invariant tests
//! reaching it, and [`order`] sequencing it all fall out for free, the same
//! way `recipe.rs`'s doc comment on `shipped_recipes` explains for releases.
//!
//! ## `overrides` crosses recipes, and is measured rather than assumed
//!
//! **Fix round 1 (Task 4 review) corrected this section outright — the
//! original draft shipped every package's `overrides` empty, reasoned from
//! the wrong scope, and review caught it two ways at once.**
//!
//! First, the reasoning: `overrides` is not only read by
//! `no_two_components_claim_one_destination_without_declaring_it` — the one
//! test that ever consulted it before this round, and the reason the empty
//! array *looked* consistent (every shipped rule is `subtree`, and that test
//! only polices `file` rules, treating a Subtree destination as a merge
//! point rather than a claim). It exists to answer a *file-level* question a
//! later stage will ask: is a package's file replacing something on purpose,
//! or landing on a name nobody expected? Measured directly against the
//! owner's real media (review round): **every one of BoingBag 3.9-1's 210
//! files, and every one of 3.9-2's 121, sits at a path `workbench-base`
//! already writes.** An empty `overrides` on data that is 100% overlap does
//! not mean "no relationship" — it means the field is constant across every
//! row a reader would ever check it against, which is the exact noise §3
//! warns against: a flag that never varies separates nothing. So both
//! BoingBags now declare `overrides: ["workbench-base"]`, and
//! `locale-turkish` declares `overrides: ["locale-base"]` once ART-162 gave
//! it a real, measured collision to name (`Locale/Catalogs/türkçe`, written
//! by both the base disc and this update — see that package's own JSON for
//! the count).
//!
//! Second, the mechanism: `Component.overrides` is checked by
//! `recipe.rs`'s `every_override_names_a_component_that_exists`, and that
//! test used to resolve `over` against `recipe.component(over)` — one
//! recipe's own component list. Every package became its own
//! one-component pseudo-`Recipe` in `shipped_recipes()`, so an override
//! naming a *release's* component (`workbench-base`, `locale-base`) could
//! never resolve there, no matter how true it was. Fixed in `recipe.rs`
//! by resolving `over` against the union of every shipped id — every
//! release's components and every package's own — via
//! `all_shipped_component_ids`, so a package's `overrides` can now
//! legitimately cross into a release recipe the same way a rule's `from`
//! or `to` already could.
//!
//! What still is not built: which file's bytes actually survive when two
//! components' subtrees really do collide on one file name is the applying
//! engine's job, not this task's. `overrides` records the fact for that
//! engine to read; it does not yet act on it.
//!
//! ## `requires` is a dependency, not a suggestion
//!
//! BoingBag 3.9-2 assumes BoingBag 3.9-1 is already on the volume (spec §8:
//! applying them the other way round is a wrong system, not a warning). So
//! [`order`] topologically sorts a chosen set by `requires` — the order the
//! user ticked boxes in is not the order that gets applied — and refuses,
//! by name, either a requirement that was not itself chosen or a cycle in
//! the shipped data (which would otherwise hang whatever applies them).
//!
//! ## `requires_components` is not `requires`, and the difference is ART-162
//!
//! `requires` relates a package to another **package**; `locale-turkish`
//! does not depend on a package at all. It depends on `locale-base`, a
//! **component of a release recipe** — the thing that puts
//! `Locale/Catalogs` (and `Locale/Languages`, and the locale preferences
//! that read them) on the volume in the first place. Ticking the Turkish
//! pack without it lands thirty-six catalogs into a drawer nothing can
//! open: exactly ART-162, arriving through the selection instead of through
//! the recipe.
//!
//! Expressing that as a `requires` entry cannot work — there is no package
//! called `locale-base`, so [`order`] would refuse a correct selection by
//! name — and leaving it undeclared makes it a silent no-op, which is the
//! outcome the whole round exists to prevent. So a package may declare
//! [`Package::requires_components`], and `plan()` refuses the combination
//! by name ([`super::RefusalReason::PackageComponentMissing`]) when one of
//! them is not in the resolved `components_on`. It is checked against the
//! *resolved* set, never against `InstallRequest::chosen`, for the same
//! reason `detect_exclusive_group_conflicts` is: a component can be
//! switched on without ever being chosen.
//!
//! `overrides` and `requires_components` say two different things about the
//! same id and both are true of `locale-turkish` → `locale-base`: the first
//! says "when we land on the same file, mine wins"; the second says "there
//! is no point in mine at all unless yours is there".

//! ## Identity needs two things, and the measurement is why (ART-167)
//!
//! A package's `media` is its archive's **single top-level directory, read
//! from inside it** — chosen so a file somebody renamed still identifies
//! itself, exactly as `AdfSource` reads a volume label off a root block and
//! `CdSource` reads one off a volume descriptor. That rule is not wrong. It
//! is **not sufficient on its own**, and the owner's own folder is what
//! proved it. Every `.lha` in the owner's own package folder was walked
//! (44 of them, `scripts/lha-package-identity.py`, run 2026-08-20) and asked
//! what top-level directory ART would read from it:
//!
//! ```text
//! LocaleUpdate    claimed by 8 archives  BoingBag39-2-{deutsch,francais,greek,
//!                                        italiano,polski,portugues,
//!                                        portugues-brasil,turkce}.lha
//! BoingBag3.9-2   claimed by 2 archives  BoingBag39-2.lha,
//!                                        BoingBag39-2-Contribution.lha
//! ```
//!
//! Eight language packs legitimately carry `LocaleUpdate`; the name is the
//! *product's*, not the variant's. So `scan::package_for` answered
//! `Ambiguous` for two of the three shipped packages and the Turkish
//! catalog pack — the owner's own language — could not be selected at all.
//!
//! **The tiebreak is not the filename.** The eight were compared entry by
//! entry (same script): exactly four entries are common to all eight
//! (`LocaleUpdate.info`, `LocaleUpdate/Install Locale`, that icon, and
//! `LocaleUpdate/getlocale`), and each carries **one distinct**
//! `locale/catalogs/<language>` drawer — `türkçe`, `deutsch`, `français`,
//! `italiano`, `polski`, `português`, `português-brasil`, `greek`. The two
//! `BoingBag3.9-2` archives separate the same way: `BoingBag39-2.lha`
//! carries `AmigaOS-Update`, `BoingBag39-2-Contribution.lha` carries
//! `Contribution` and no payload member at all. **Every collision in the
//! owner's real folder is resolvable from inside the archive**, so the
//! identity rule keeps its whole point and simply grows a second fact:
//! [`Package::distinguished_by`], a path that must exist inside the archive
//! below its top-level directory.
//!
//! Three consequences worth stating, because each is a behaviour change:
//!
//! 1. **The check runs whether or not there is a collision.** Filtering only
//!    when `package_for` already sees two candidates would leave the worse
//!    half of the bug in place: with only `BoingBag39-2-deutsch.lha` in the
//!    folder, `locale-turkish` had exactly one candidate and resolved to it
//!    — a German archive placed for a user who asked for Turkish. Applied
//!    always, that case is now `Missing`, which is the truth.
//! 2. **An unresolvable ambiguity stays a refusal naming the candidates.**
//!    Narrowing that still leaves two is `MediaMatch::Ambiguous` over the
//!    *narrowed* list; nothing here ever picks a winner.
//! 3. **`member` is deliberately not reused as the distinguisher**, even
//!    though `boingbag-39-2` declares the same string for both. They say
//!    different things and the measurement shows it: `AmigaOS-Update` is
//!    carried by `BoingBag39-1.lha` **and** `BoingBag39-2.lha`, so as an
//!    identity it separates nothing — it only works for `boingbag-39-2`
//!    because the *other* claimant of `BoingBag3.9-2` happens to lack it.
//!    `member` says where the payload is; `distinguished_by` says which
//!    archive this is.
//!
//! A package whose `media` was unique across all 44 archives —
//! `boingbag-39-1` — declares no distinguisher. A condition that is not
//! needed to separate anything can only add a way to refuse an archive that
//! would have worked, and §89's rule cuts that way too.
//!
//! ## `amiga_installer`: the run is data too, and it is not a whole path
//!
//! ART places files from the host. Both BoingBags are where that stops
//! (ART-166): their payload is ZipCrypto-encrypted and the password belongs
//! to the package's own `Updater`, which runs on an Amiga. Nothing is
//! decrypted and no protection is bypassed — the package's own program runs
//! where it was always meant to. What this module gains is the *declaration*
//! of that, [`AmigaInstaller`], so a fourth package is a JSON file rather
//! than a code path, the same rule every other fact about a package follows.
//!
//! **The one distinction to hold on to.**
//! `crate::core::amigainstall::PlannedRun::program` is a whole AmigaDOS path
//! (`PKG:C/Updater`), and its own module refuses any value that names ART's
//! work volume. [`AmigaInstaller::program`] is a path **inside the package**
//! (`C/Updater`), and `validate_installer` refuses a `:` in it outright. The
//! two are one small edit apart in appearance and not at all alike in what
//! they authorise: the volume a run reaches is ART's decision, made where
//! the package is mounted, and a recipe that could write a volume here would
//! be taking that decision from the other end. Composing the whole path from
//! this one is a command-layer job, and so is proving the result stayed in
//! bounds.
//!
//! **What a declaration is measured against.** `C/Updater` was read out of
//! the owner's own archives (7-Zip 26.02, 2026-08-20) and the argument out
//! of each package's own `Install` script, which runs
//! `C/Updater AmigaOS-Update "<target>"`. `<target>` is not written here: it
//! is a fact about the run, not about the package. Each JSON's
//! `_why_amiga_installer` carries the readings; the `#[ignore]`d
//! `every_declared_installer_exists_in_its_own_archive_when_asked` re-runs
//! them against the real archives on demand, because the AmigaOS 3.9 recipe
//! shipped fourteen unread paths and every one was wrong.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use super::recipe::{self, validate_component, validate_path};
use super::{Component, HostPlacementBlock, PathRule};
use crate::core::error::{CoreError, CoreResult};
use crate::core::security::refuse_shell_metacharacters;

const BOINGBAG_39_1_JSON: &str = include_str!("recipes/packages/boingbag-39-1.json");
const BOINGBAG_39_2_JSON: &str = include_str!("recipes/packages/boingbag-39-2.json");
const LOCALE_TURKISH_JSON: &str = include_str!("recipes/packages/locale-turkish.json");

/// Every package JSON this project ships. The one list a new package JSON
/// has to join — see the module doc comment.
const SHIPPED_JSON: &[&str] = &[BOINGBAG_39_1_JSON, BOINGBAG_39_2_JSON, LOCALE_TURKISH_JSON];

/// An update package on top of an installed AmigaOS tree — an official
/// BoingBag or an unofficial pack like the Turkish catalogs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub id: String,
    /// Shown on screen. Not the id, and not translated — a package's name is
    /// its own, the way a volume name is (ART-060).
    pub name: String,
    /// Which AmigaOS releases this package belongs on — the release recipe's
    /// own names, e.g. `["AmigaOS 3.9"]` (**ART-209**).
    ///
    /// The owner chose AmigaOS 3.2 and was offered **BoingBags**, which are
    /// 3.9's and of which 3.2 has none: *"3.2 ile 3.9 seçenekleri GUI'de
    /// karışmış birbirine girmiş."* Nothing was filtering, because until this
    /// field existed there was nothing to filter *by* — a package declared no
    /// release, [`packages`] returned all of them, the command took no
    /// release and neither panel was handed one. Four layers, none of which
    /// could answer the question, so the answer became "show everything".
    ///
    /// It is the second layer of the model the owner stated: a **base** (the
    /// release), the **updates** that go on that base, and the **program
    /// sets** that go on those. Each layer's options are a function of the
    /// one below it, and this field is what makes that expressible rather
    /// than merely intended.
    ///
    /// **Required, not defaulted.** A default would have to be either "every
    /// release" — today's defect made permanent — or one named release, which
    /// is a guess about a package nobody has looked at. A JSON that omits it
    /// fails to parse, which is the only answer that cannot be wrong quietly.
    /// Every name is checked against [`recipe::releases`], so a package
    /// naming a release ART ships no recipe for is refused rather than
    /// becoming unreachable from every screen.
    pub releases: Vec<String>,
    /// The archive's single top-level directory, read from inside it.
    pub media: String,
    /// The member holding the payload, for a package whose files sit inside
    /// a second archive. `None` for loose files at direct paths.
    pub member: Option<String>,
    /// The second half of this package's identity: a path that must exist
    /// **inside** the archive, below its top-level directory, for that
    /// archive to be this package's — `locale/catalogs/türkçe` for the
    /// Turkish pack, `AmigaOS-Update` for BoingBag 3.9-2.
    ///
    /// `None` when `media` alone separates this package from every archive
    /// a user is likely to hold beside it. See the module doc comment's
    /// "Identity needs two things" section for the measurement that made
    /// this field necessary and for why it is not folded into `member`.
    ///
    /// Resolved case-insensitively and **as a whole path**, never as a
    /// prefix — `português` and `português-brasil` are two of the eight
    /// language drawers, and a prefix match would let the first claim the
    /// second's archive.
    pub distinguished_by: Option<String>,
    pub requires: Vec<String>,
    /// Recipe **component** ids this package needs switched on — not
    /// packages; see the module doc comment's "`requires_components` is not
    /// `requires`" section.
    pub requires_components: Vec<String>,
    /// `Some` when ART **cannot place this package's files from the host at
    /// all**, whatever the user's folder holds — see
    /// [`HostPlacementBlock`]. `None` is the ordinary case.
    ///
    /// Recipe data rather than code, for the same reason every other fact
    /// about a package is: the day an Amiga-side install round lands, both
    /// BoingBags stop being blocked by deleting one line from each JSON,
    /// not by finding the `if` that named them.
    pub host_placement_block: Option<HostPlacementBlock>,
    /// The rules and `overrides`, in the shape the placer already takes.
    pub component: Component,
    /// What to run on the Amiga when ART cannot place this package's files
    /// from the host — see [`AmigaInstaller`]. `None` for a package ART
    /// places directly.
    ///
    /// Independent of [`host_placement_block`](Self::host_placement_block)
    /// on purpose, though today's three packages make the two look like one
    /// field asked twice. They answer different questions — *can ART place
    /// this on the host?* and *what would the Amiga run instead?* — and a
    /// package can honestly say no to the first without ART knowing the
    /// answer to the second, which is precisely the state both BoingBags
    /// were in between ART-166 and this round. Folding them into one enum
    /// would have made that state unrepresentable and so unreportable.
    pub amiga_installer: Option<AmigaInstaller>,
}

/// What to run on the Amiga to install this package, when ART cannot place
/// its files from the host. **Data, not a code path** — a fourth package is a
/// JSON file. `None` means this package is not one this round can run, which
/// is the honest answer for a package ART already places directly.
///
/// **Spelled snake_case, deliberately, and with no `rename_all`.** The brief
/// wrote this type with `#[serde(rename_all = "camelCase")]`, which is a
/// no-op for two single-word fields and was removed in fix round 1 rather
/// than left as a no-op that reads as intent (review M1). The key that
/// matters is the *container's* — `amiga_installer` on [`RawPackage`], which
/// carries no `rename_all` either, so every key in a package JSON is
/// snake_case and a recipe author has one spelling to learn rather than two.
/// A `rename_all` here would have obliged a future multi-word field to be
/// camelCase among snake_case siblings.
///
/// This type also derives `Serialize`, and ART's frontend convention is
/// camelCase — but a recipe is read far more often than this is written, and
/// turning one module's representation into another's is a command-layer job
/// (CLAUDE.md), not a reason to make the file on disk harder to write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmigaInstaller {
    /// Where the program sits **inside the package**, `/`-separated.
    pub program: String,
    /// Arguments, **each its own string**: a recipe cannot hand over one
    /// line for ART to split, because serde refuses a bare string where this
    /// list belongs, and no argument may carry a shell metacharacter, so
    /// none of them can end the command and begin a second one
    /// (`validate_installer`).
    ///
    /// What this does **not** say, because it is not enforced here (review
    /// m1): whether an argument containing a space arrives at the Amiga as
    /// one token or two. That is decided by whoever composes and quotes the
    /// command line — `commands/amigainstall.rs` and
    /// `core::amigainstall::workvol`, Task 3 of this round. A space is not
    /// refused here because AmigaDOS names legitimately contain them: the
    /// owner's own `BoingBag39-1.lha` carries
    /// `Manuals/English/Guides Vol. 1/`, so a package whose installer sat in
    /// such a drawer would be undeclarable if this refused one.
    #[serde(default)]
    pub args: Vec<String>,
    /// The lowest `$VER:` version ART will launch this program at, written
    /// the way AmigaOS writes it — `"45.15"`. `None` when nothing is known
    /// to be wrong with any build of it, which is the ordinary case.
    ///
    /// **Measured, and it is the reason this field exists (ART-186).**
    /// `BoingBag39-1.lha`'s own `C/Updater` states `$VER: Updater 45.13
    /// (3.4.2001)`; `BoingBag39-1-UAE.lha`'s states `$VER: Updater 45.15
    /// (17.4.2001)`, and that archive's readme says exactly what changed —
    /// *"This archive contains a file, Updater 45.15, that fixes the
    /// following problem: You can install the BoingBag on UAE now."* This
    /// round launches that program **inside an emulator**, so 45.13 cannot
    /// work; left alone it would fail, the generated script's `If Warn`
    /// would write `failed`, and ART would report that the installer ran and
    /// refused — about a program that could not have worked. §89 forbids
    /// exactly that, so the run is refused before it starts instead.
    ///
    /// **A version, not a size.** `25 588` bytes is not an identity: it is
    /// consistent with any build that happens to be that long, and reading a
    /// coincidence as proof is the mistake that shipped an AmigaOS 3.5 tree
    /// labelled 3.9. The `$VER:` string is the file's own statement about
    /// itself, which is what "ask the artefact what it is; never infer it"
    /// asks for. All three of the owner's real archives carry one, and
    /// `the_owners_real_updaters_state_the_versions_this_recipe_relies_on`
    /// re-reads them on demand.
    #[serde(default)]
    pub minimum_version: Option<String>,
    /// Second and later media whose files are copied **over** the package's
    /// own drawer before the installer is looked for, in the order written.
    ///
    /// The remedy the same readme prescribes: *"Simply update your old
    /// BoingBag3.9-1 by copying the contents within the `BoingBag3.9-1`
    /// drawer in this package to it."* Declared here as data so a fifth
    /// package with the same shape is a JSON file, not a code path.
    ///
    /// Supplying one is the **user's** choice, not ART's: a copy downloaded
    /// after 2001-04-20 already carries the fix, and
    /// [`minimum_version`](Self::minimum_version) is what decides whether
    /// one was needed. ART never fetches an overlay itself.
    #[serde(default)]
    pub overlays: Vec<InstallerOverlay>,
    /// The medium this package's installer verifies before it will work —
    /// see [`RequiredMedium`]. `None` for an installer that checks nothing,
    /// which is the ordinary case.
    #[serde(default)]
    pub required_medium: Option<RequiredMedium>,
    /// What the tree needs done **after** this installer has run, and the
    /// installer does not do itself (ART-227).
    ///
    /// Measured against the owner's own material rather than copied from
    /// another builder's script: BoingBag 2's `Updater` leaves
    /// `Devs/AmigaOS ROM Update.BB39-2` beside the old file under a name
    /// nothing loads, and BoingBag 1 leaves seven `C:` commands without the
    /// `p` bit that `Resident` needs. Both are ordinary file operations, so
    /// ART performs them **on the host**, against the staged copy, before it
    /// decides whether to promote — see
    /// [`crate::core::amigainstall::finish`] for why that is better than
    /// appending AmigaDOS lines to the boot script.
    #[serde(default)]
    pub post_install: Vec<crate::core::amigainstall::finish::PostStep>,
}

/// A disc a package's own installer insists on seeing (ART-193).
///
/// **Measured, and it is the reason this field exists.** Both AmigaOS 3.9
/// BoingBags carry an `Updater` that verifies the original CD-ROM before it
/// does anything, and each says so in its own printable strings — read on
/// the host on 2026-08-21 from the owner's own archives:
///
/// ```text
///   Checking AmigaOS 3.9 CD-ROM ...
///   Failed to check AmigaOS 3.9 CD-ROM.
///   Did you really insert a original AmigaOS 3.9 CD-ROM
///   into a mounted CD-ROM drive?
///   AmigaOS3.9:Videos/Angels.avi                     (45.15 and 45.19 both)
///   AmigaOS3.9:Audio/Circle Orbital.mp3              (45.15)
///   AmigaOS3.9:Audio/Circle - Orbital.mp3            (45.19)
/// ```
///
/// Without that disc the program opens its own screen and never finishes —
/// measured three times, up to 1 200 s, with not one file written.
///
/// **This is a medium the user has, not a protection to be worked around.**
/// Supplying the disc the installer asks for is *meeting* its check. Nothing
/// here decrypts anything, and ART deliberately does **not** satisfy such a
/// check by extracting the handful of files a program happens to name: that
/// would be satisfying a media check rather than meeting it.
///
/// **What this type says and what it does not.** It names the *volume* the
/// installer looks for — a fact about the package, readable in the package's
/// own binary, and shipped data like everything else in a recipe. It never
/// names a file on the host: ART ships no Amiga media and never will, so
/// *which* image is a fact about the run and arrives on the request
/// (`commands::amigainstall::AmigaInstallRequest::medium`), exactly as the
/// user's own Kickstart and the user's own package archives do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequiredMedium {
    /// The Amiga volume name the installer checks for — `AmigaOS3.9`,
    /// **without** a trailing colon.
    ///
    /// A recipe naming a volume is refused everywhere else in this file
    /// ([`AmigaInstaller::program`]), and this is not that: there the volume
    /// would be one ART mounts, which is ART's decision to make. Here the
    /// volume is the disc's *own* name, which neither ART nor the recipe
    /// chooses — the disc carries it, and the command layer asks the image
    /// what it is called rather than assuming.
    pub volume: String,
    /// What to call the disc in a sentence — *"the original AmigaOS 3.9
    /// CD-ROM"*. Untranslated, like a package's `name` (ART-060).
    pub name: String,
}

/// One overlay medium: where its files are inside its own archive, and where
/// they land inside the package's drawer.
///
/// **`from` is a whole path from the overlay archive's own root, including
/// that archive's top-level drawer**, because a real one is not shaped like
/// the package it patches. Measured with 7-Zip 26.02 on 2026-08-21 against
/// the owner's `BoingBag39-1-UAE.lha`: its seven entries sit under
/// `BoingBag3.9-1-UAE\`, and the `Updater` is at
/// `BoingBag3.9-1-UAE\BoingBag3.9-1\C\Updater` — one drawer deeper than the
/// `BoingBag3.9-1\C\Updater` it replaces. Extracting it over the package with
/// an overwrite policy would therefore have written a second, parallel
/// drawer and left the old `Updater` exactly where it was; the archive had to
/// be opened to find that out, which is why it was.
///
/// It doubles as the archive's identity: an archive carrying no such path is
/// refused by name rather than silently contributing nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallerOverlay {
    /// `/`-separated, from the overlay archive's own root.
    pub from: String,
    /// `/`-separated, inside the package's own drawer. Empty — the default —
    /// means the drawer itself, which is what the readme's remedy describes.
    #[serde(default)]
    pub to: String,
}

/// The flat shape a person actually edits. `id`/`media`/`rules`/`overrides`
/// fold into [`Package::component`] at parse time — the file on disk should
/// not have to know the engine's own struct layout.
///
/// ## A key serde does not recognise is a refusal, not a shrug
///
/// **Fix round 1 (review M1), and the defect was real rather than
/// hypothetical.** A recipe written `"amigaInstaller": { … }` used to parse
/// as `Ok`, with the declaration **silently dropped** — the package was
/// accepted and `amiga_installer` was `None`, so the only symptom would have
/// been ART reporting that this package is not one it can run: a lie that
/// reads exactly like a design decision. Two doc comments in this file
/// recommended that dead spelling at the time, so an author following the
/// documentation wrote the one form that could not work.
///
/// Plain `#[serde(deny_unknown_fields)]` is the usual answer and is **not
/// usable here**: every shipped package JSON carries `_why_…` keys — the
/// measurements, with their dates and tools, that make a recipe reviewable
/// in a diff — and there is no way to enumerate them. So the catch-all below
/// keeps them and [`RawPackage::check_unknown_keys`] refuses everything else
/// by name. A key beginning `_` is a note to a human; anything else is a
/// person trying to tell ART something ART did not hear.
///
/// Scoped to packages on purpose: `recipe.rs`'s release recipes have the
/// same exposure and are not changed here, because this task's evidence is
/// about packages and a change made without measuring the other file's own
/// data would be the guess this project keeps paying for.
#[derive(Debug, Deserialize)]
struct RawPackage {
    id: String,
    name: String,
    /// ART-209 — required on purpose; see `Package::releases`.
    releases: Vec<String>,
    media: String,
    #[serde(default)]
    member: Option<String>,
    #[serde(default)]
    distinguished_by: Option<String>,
    #[serde(default)]
    requires: Vec<String>,
    #[serde(default)]
    requires_components: Vec<String>,
    #[serde(default)]
    host_placement_block: Option<HostPlacementBlock>,
    #[serde(default)]
    amiga_installer: Option<AmigaInstaller>,
    #[serde(default)]
    overrides: Vec<String>,
    rules: Vec<PathRule>,
    /// Every key serde did not place in a field above — see this struct's
    /// own doc comment. Never read as data; only checked, by
    /// [`check_unknown_keys`](Self::check_unknown_keys), and dropped in
    /// [`into_package`](Self::into_package).
    #[serde(flatten)]
    extra: HashMap<String, serde_json::Value>,
}

impl RawPackage {
    /// Fold the flat JSON shape into a [`Package`], without validating it —
    /// shared by [`parse`], which validates, and
    /// [`raw_packages`](self::raw_packages), which deliberately does not
    /// (see that function's own doc comment).
    /// Refuse any key that is neither a field of this struct nor a `_…`
    /// note. See the struct's own doc comment for why this is not
    /// `deny_unknown_fields`.
    ///
    /// The message names the key **and** what a note looks like, because the
    /// two mistakes this catches want different fixes: `amigaInstaller` is a
    /// misspelling of a real field, and `why_this_is_here` is a note whose
    /// author forgot the underscore.
    fn check_unknown_keys(&self, id: &str) -> CoreResult<()> {
        // Sorted, so a file with two bad keys refuses the same way twice
        // rather than by `HashMap` iteration order.
        let mut unknown: Vec<&str> = self
            .extra
            .keys()
            .map(String::as_str)
            .filter(|key| !key.starts_with('_'))
            .collect();
        unknown.sort_unstable();
        if let Some(key) = unknown.first() {
            return Err(CoreError::Malformed {
                format: "package".into(),
                detail: format!(
                    "'{id}': '{key}' is not a package key (keys are snake_case, and a note to a human must begin with '_')"
                ),
            });
        }
        Ok(())
    }

    fn into_package(self) -> Package {
        let component = Component {
            id: self.id.clone(),
            media: self.media.clone(),
            rules: self.rules,
            // A package is something the user chooses, never something a
            // ROM version switches on (Task 4's brief, verbatim).
            required: false,
            condition: None,
            overrides: self.overrides,
            user_startup: Vec::new(),
            activate: Vec::new(),
            exclusive_group: None,
            label_key: None,
            available: true,
        };
        Package {
            id: self.id,
            name: self.name,
            releases: self.releases,
            media: self.media,
            member: self.member,
            distinguished_by: self.distinguished_by,
            requires: self.requires,
            requires_components: self.requires_components,
            host_placement_block: self.host_placement_block,
            component,
            amiga_installer: self.amiga_installer,
        }
    }
}

/// Check one package's [`AmigaInstaller`] declaration, if it has one.
///
/// **A recipe is shipped data, and it is still parsed.** The file on disk can
/// be hand-edited, and what it declares eventually becomes a line in a
/// generated AmigaDOS script — a command interpreter's input. So the same
/// guard `workvol` applies to a composed run is applied here, at the boundary
/// where the JSON is read, rather than trusting the file because ART shipped
/// one version of it.
///
/// Two rules, and the first is the one that keeps this type from quietly
/// becoming the other one:
///
/// - **`program` is a path inside the package, never a whole AmigaDOS path.**
///   `crate::core::amigainstall::PlannedRun::program` is the whole path, and
///   its module refuses any value naming ART's own work volume. If a recipe
///   could write a volume here, it would be composing that whole path from
///   the other end — the command layer prefixes a volume onto this value, so
///   a `:` in it means the recipe, not ART, decided what volume the run
///   reaches. Every segment goes through `validate_path`, which refuses `:`
///   and `/` inside a segment (so no volume, no empty segment, so neither a
///   leading nor a doubled `/` — both AmigaDOS's parent-directory notation)
///   and refuses `..`.
///
///   This is deliberately **not** a second copy of `claims_work_volume`.
///   That guard decides *which* volumes a composed run may name; this one
///   decides that a recipe may not name a volume **at all**, which is a
///   strictly narrower question with a single answer, and so cannot drift
///   away from it.
///
/// - **An argument may name a volume**, because an installer's argument
///   legitimately can (`TO=SYS:`), and deciding which volumes a run may
///   reach belongs to the one guard that already does it. What is checked
///   here is only what this layer can honestly answer: an argument is not
///   empty — an empty one would vanish from the composed line, making a
///   recipe that declares it indistinguishable from one that does not — and
///   carries no shell metacharacter.
fn validate_installer(package: &Package) -> CoreResult<()> {
    let Some(installer) = &package.amiga_installer else {
        return Ok(());
    };
    // Checked before `validate_path`, which would otherwise refuse this by
    // the segment it happened to land in: `PKG:C/Updater` would be reported
    // as a bad name `PKG:C`, which is true and unhelpful. A reader of the
    // error needs to see the whole thing they wrote.
    if installer.program.contains(':') {
        return Err(CoreError::Malformed {
            format: "package".into(),
            detail: format!(
                "'{}': the installer path '{}' names a volume; it must be a path inside \
                 the package, and the volume it is reached under is ART's to decide",
                package.id, installer.program
            ),
        });
    }
    // The label is this module's, not `recipe.rs`'s: a person who mistypes
    // a path in a *package* JSON was reading a package file, and an error
    // announcing `format: "recipe"` sends them to the wrong one (review m3).
    validate_path(
        "package",
        &package.id,
        "amiga_installer.program",
        &installer.program,
        false,
    )?;
    refuse_shell_metacharacters("installer path", &installer.program)?;
    for arg in &installer.args {
        if arg.trim().is_empty() {
            return Err(CoreError::Malformed {
                format: "package".into(),
                detail: format!("'{}': an installer argument is empty", package.id),
            });
        }
        refuse_shell_metacharacters("installer argument", arg)?;
    }
    if let Some(minimum) = &installer.minimum_version {
        if parse_version_pair(minimum).is_none() {
            return Err(CoreError::Malformed {
                format: "package".into(),
                detail: format!(
                    "'{}': the installer's minimum_version '{minimum}' is not a version and a \
                     revision — AmigaOS writes one as '45.15'",
                    package.id
                ),
            });
        }
    }
    for overlay in &installer.overlays {
        // `from` carries the overlay archive's own top-level drawer, so it is
        // never empty; `to` is inside the package's drawer and an empty one
        // means the drawer itself, which is what every overlay declared today
        // says.
        validate_path(
            "package",
            &package.id,
            "amiga_installer.overlays[].from",
            &overlay.from,
            false,
        )?;
        validate_path(
            "package",
            &package.id,
            "amiga_installer.overlays[].to",
            &overlay.to,
            true,
        )?;
        refuse_shell_metacharacters("installer overlay path", &overlay.from)?;
        refuse_shell_metacharacters("installer overlay path", &overlay.to)?;
    }
    // A required medium is a **volume name**, not a path and not a device
    // reference. A trailing colon is the mistake a recipe author will make —
    // the installer's own strings carry one (`AmigaOS3.9:Videos/…`) — and
    // letting it through would make ART compare `AmigaOS3.9:` against the
    // name a disc actually states, `AmigaOS3.9`, and refuse the right disc.
    if let Some(medium) = &installer.required_medium {
        for (field, value) in [("volume", &medium.volume), ("name", &medium.name)] {
            if value.trim().is_empty() {
                return Err(CoreError::Malformed {
                    format: "package".into(),
                    detail: format!(
                        "'{}': the installer's required_medium.{field} is empty",
                        package.id
                    ),
                });
            }
            refuse_shell_metacharacters("required medium", value)?;
        }
        if medium.volume.contains(':') || medium.volume.contains('/') {
            return Err(CoreError::Malformed {
                format: "package".into(),
                detail: format!(
                    "'{}': the installer's required_medium.volume '{}' is a path or carries a \
                     colon; it must be the disc's own volume name alone, as the image states it",
                    package.id, medium.volume
                ),
            });
        }
    }
    Ok(())
}

/// `"45.15"` → `(45, 15)`; `None` for anything that is not two whole numbers
/// separated by a dot.
///
/// Deliberately the same shape [`crate::core::amigaver`] reads out of a
/// file's own `$VER:` marker, so a declaration and the thing it is compared
/// against are never two different notions of a version. It is a separate
/// two-line function rather than a call into that module because what is
/// parsed here is a bare number a recipe author typed, not a marker found by
/// substring search inside arbitrary bytes — the surrounding guards that
/// module needs (bounded window, plausible-name check) have nothing to do
/// here, and reusing it would mean writing `$VER: x 45.15` to make it fit.
pub fn parse_version_pair(text: &str) -> Option<(u32, u32)> {
    let (version, revision) = text.trim().split_once('.')?;
    Some((version.trim().parse().ok()?, revision.trim().parse().ok()?))
}

/// Parse and validate one package's JSON — `recipe::parse`'s own validation
/// shape, applied to the single component a package wraps.
fn parse(json: &str) -> CoreResult<Package> {
    let raw: RawPackage = serde_json::from_str(json).map_err(|e| CoreError::Malformed {
        format: "package".into(),
        detail: e.to_string(),
    })?;
    // Before `into_package` consumes `raw` and drops what serde could not
    // place. A key ART did not recognise is refused by name here rather than
    // vanishing (review M1).
    raw.check_unknown_keys(&raw.id)?;
    let package = raw.into_package();
    validate_component("package", &package.component)?;
    validate_installer(&package)?;
    validate_releases(&package)?;
    Ok(package)
}

/// ART-209 — a package belongs to at least one release ART ships a recipe
/// for, and says which.
///
/// Both halves are refusals rather than warnings. An empty list would put a
/// package on no screen at all, silently; a name ART has no recipe for is the
/// same thing with a typo behind it (`"AmigaOS 3.5"`), and both are the kind
/// of quiet unreachability §89 exists to stop.
fn validate_releases(package: &Package) -> CoreResult<()> {
    if package.releases.is_empty() {
        return Err(CoreError::Malformed {
            format: "package".into(),
            detail: format!("'{}': names no release", package.id),
        });
    }
    for release in &package.releases {
        if !recipe::releases().contains(&release.as_str()) {
            return Err(CoreError::Malformed {
                format: "package".into(),
                detail: format!(
                    "'{}': names release '{release}', which ART ships no recipe for",
                    package.id
                ),
            });
        }
    }
    Ok(())
}

/// The shipped packages that belong on `release` — what a screen showing one
/// release's updates must be handed (ART-209).
///
/// An unknown release answers with an empty list rather than an error: the
/// caller is a screen asking "what goes on this?", and "nothing" is a true
/// answer for a release ART ships no packages for. `by_release` refuses an
/// unknown *recipe* name because building the wrong operating system is a
/// different kind of harm.
pub fn packages_for(release: &str) -> CoreResult<Vec<Package>> {
    Ok(packages()?
        .into_iter()
        .filter(|p| p.releases.iter().any(|r| r == release))
        .collect())
}

/// Parse and validate every JSON in `jsons`, then refuse if two share an id
/// — the package-list counterpart of `recipe::validate`'s own
/// id-uniqueness check over one recipe's components, which `parse` alone
/// cannot catch since it only ever sees one JSON at a time. Parameterised
/// (like [`order_over`]) rather than reading [`SHIPPED_JSON`] directly, so a
/// test can feed it a deliberately colliding pair without editing the real
/// shipped list. [`packages`] is the thin wrapper over the real one.
///
/// This is what actually catches the failure mode review named: two entries
/// in `SHIPPED_JSON` naming the same id — the copy-paste that silently drops
/// a third package while looking, at a glance, like the array still has
/// three entries.
fn parse_all(jsons: &[&str]) -> CoreResult<Vec<Package>> {
    let all: Vec<Package> = jsons
        .iter()
        .map(|json| parse(json))
        .collect::<CoreResult<Vec<Package>>>()?;

    let mut seen = HashSet::new();
    for package in &all {
        if !seen.insert(package.id.as_str()) {
            return Err(CoreError::Malformed {
                format: "package".into(),
                detail: format!("two shipped packages share the id '{}'", package.id),
            });
        }
    }
    Ok(all)
}

/// Every package ART ships, parsed and validated. Order here is not
/// application order — see [`order`] for that.
pub fn packages() -> CoreResult<Vec<Package>> {
    parse_all(SHIPPED_JSON)
}

/// The shipped JSON, deserialised **without** going through
/// [`validate_component`] — `recipe.rs`'s own `raw_recipe`'s reason for
/// existing, applied here: a test built on [`packages`] alone would already
/// have failed via `?` on any bad name, so a test that means to police the
/// *shipped data* independently of whether `validate_component` itself has a
/// bug has to deserialise directly. Test-only, like its `recipe.rs`
/// counterpart, but `pub(crate)` rather than module-private: `recipe.rs`'s
/// own test module needs it too, to widen `raw_shipped_recipes()` (Task 4,
/// Step 3).
#[cfg(test)]
pub(crate) fn raw_packages() -> Vec<Package> {
    SHIPPED_JSON
        .iter()
        .map(|json| {
            let raw: RawPackage = serde_json::from_str(json)
                .unwrap_or_else(|e| panic!("a shipped package must at least deserialise: {e}"));
            raw.into_package()
        })
        .collect()
}

/// One shipped package by id.
///
/// An unknown id is refused rather than defaulted — the same rule
/// `recipe::by_release` follows and for the same reason: picking a different
/// package than the one asked for would be wrong data reaching the volume,
/// not a warning.
pub fn by_id(id: &str) -> CoreResult<Package> {
    packages()?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| CoreError::InvalidInput(format!("ART ships no package '{id}'")))
}

/// [`order`]'s own machinery, parameterised over the package list — so a
/// test can hand it a small, hand-built [`Package`] set that contains a
/// cycle, rather than only ever exercising it through the three shipped,
/// cycle-free packages. [`order`] is the thin wrapper that calls this with
/// [`packages`].
///
/// A plain Kahn's-algorithm topological sort over `chosen`'s own `requires`
/// edges. Every requirement is checked against `chosen` itself first — a
/// requirement that exists as a package but was not itself chosen is refused
/// by name rather than silently pulled in, because adding a whole package
/// the user did not ask for is a bigger surprise than a refusal. What is
/// left unsorted after the sort — every id whose in-degree never reached
/// zero — is exactly a cycle, refused **by name** (review found the first
/// version of this said "by name" in its own doc comment and then did not
/// do it) rather than left to hang whatever applies the result.
///
/// `chosen` naming the same id twice is refused up front, before any of the
/// graph bookkeeping below runs — review found, by executing it, that a
/// duplicate otherwise corrupts the in-degree count kept per id (every
/// occurrence re-walks that id's own `requires`, inflating its dependents'
/// in-degree once per repeat) and comes out looking exactly like a cycle:
/// `result.len() != chosen.len()` fires for the wrong reason, and the actual
/// mistake — the caller chose the same package twice — is never named.
pub(super) fn order_over(chosen: &[String], all: &[Package]) -> CoreResult<Vec<String>> {
    let mut seen_chosen = HashSet::new();
    for id in chosen {
        if !seen_chosen.insert(id.as_str()) {
            return Err(CoreError::InvalidInput(format!(
                "'{id}' was chosen more than once"
            )));
        }
    }

    let index: HashMap<&str, &Package> = all.iter().map(|p| (p.id.as_str(), p)).collect();
    let chosen_set: HashSet<&str> = chosen.iter().map(|s| s.as_str()).collect();

    for id in chosen {
        let package = index
            .get(id.as_str())
            .ok_or_else(|| CoreError::InvalidInput(format!("ART ships no package '{id}'")))?;
        for need in &package.requires {
            if !chosen_set.contains(need.as_str()) {
                return Err(CoreError::InvalidInput(format!(
                    "'{id}' requires '{need}', which was not chosen"
                )));
            }
        }
    }

    let mut in_degree: HashMap<&str, usize> = chosen.iter().map(|id| (id.as_str(), 0)).collect();
    let mut dependents: HashMap<&str, Vec<&str>> =
        chosen.iter().map(|id| (id.as_str(), Vec::new())).collect();

    for id in chosen {
        let package = index[id.as_str()];
        for need in &package.requires {
            dependents.get_mut(need.as_str()).unwrap().push(id.as_str());
            *in_degree.get_mut(id.as_str()).unwrap() += 1;
        }
    }

    // Ties broken by `chosen`'s own order, never a `HashMap`'s iteration
    // order — nothing here should depend on that.
    let mut queue: VecDeque<&str> = chosen
        .iter()
        .map(|s| s.as_str())
        .filter(|id| in_degree[id] == 0)
        .collect();

    let mut result: Vec<String> = Vec::with_capacity(chosen.len());
    while let Some(id) = queue.pop_front() {
        result.push(id.to_string());
        for &next in &dependents[id] {
            let degree = in_degree.get_mut(next).unwrap();
            *degree -= 1;
            if *degree == 0 {
                queue.push_back(next);
            }
        }
    }

    if result.len() != chosen.len() {
        // Every id `chosen` named but the sort never emitted — its in-degree
        // never reached zero, which is what a cycle looks like. Named
        // explicitly rather than left as "a cycle exists somewhere": with
        // `chosen`'s own duplicates already refused above, `result` is a
        // genuine subset here, not a miscount.
        let stuck: Vec<&str> = chosen
            .iter()
            .map(String::as_str)
            .filter(|id| !result.iter().any(|done| done == id))
            .collect();
        return Err(CoreError::InvalidInput(format!(
            "the chosen packages contain a dependency cycle: {}",
            stuck.join(", ")
        )));
    }

    Ok(result)
}

/// `chosen`, reordered so every package comes after everything it
/// `requires` — the order an applier should actually use, independent of
/// the order the user happened to tick the boxes in.
pub fn order(chosen: &[String]) -> CoreResult<Vec<String>> {
    order_over(chosen, &packages()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::osinstall::RuleKind;

    /// One minimal package JSON with the `releases` list spelled by the
    /// caller — for the two gate tests below, which are about the field and
    /// not about any shipped file.
    fn package_json_with_releases(releases: &str) -> String {
        format!(
            r#"{{
                "id": "test-package",
                "name": "Test package",
                "media": "TestPackage",
                "releases": {releases},
                "rules": [{{ "from": "C/Thing", "to": "C/Thing", "kind": "file" }}]
            }}"#
        )
    }

    // ART-209 --------------------------------------------------------------
    //
    // The owner chose AmigaOS 3.2 and the update-packages panel below the
    // component checklist still offered **BoingBags** — which are AmigaOS
    // 3.9's, and of which 3.2 has none. Their words: "3.2 ile 3.9 seçenekleri
    // GUI'de karışmış birbirine girmiş."
    //
    // The root cause was that no layer could answer the question: a package
    // declared no release, `packages()` returned all of them, the command took
    // no release, and neither panel was given one. So the answer defaulted to
    // "show everything" — one screen offering two operating systems' parts,
    // which is the defect a whole-branch review once called the worst on its
    // list when the *component* checklist had it.
    //
    // `releases` is required rather than defaulted. A default would have to be
    // either "all releases" (today's defect, made permanent) or one named
    // release (a guess about a package nobody has looked at). A package whose
    // JSON omits it fails to parse, which is the only answer that cannot be
    // wrong quietly.

    #[test]
    fn every_shipped_package_names_the_releases_it_belongs_to() {
        for package in super::packages().unwrap() {
            assert!(
                !package.releases.is_empty(),
                "package '{}' names no release",
                package.id
            );
            for release in &package.releases {
                assert!(
                    crate::core::osinstall::recipe::releases().contains(&release.as_str()),
                    "package '{}' names release {release:?}, which ART ships no recipe for",
                    package.id
                );
            }
        }
    }

    #[test]
    fn all_three_shipped_packages_belong_to_amigaos_39() {
        let for_39 = super::packages_for("AmigaOS 3.9").unwrap();
        let ids: Vec<&str> = for_39.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["boingbag-39-1", "boingbag-39-2", "locale-turkish"],
            "the two BoingBags and the Turkish catalogs from BoingBag 3.9-2"
        );
    }

    #[test]
    fn amigaos_32_is_offered_no_update_package_at_all() {
        // Not "fewer" — none. ART ships no update package for AmigaOS 3.2
        // today: a BoingBag is a 3.9 archive, and 3.2's own updates
        // (`Update3.2.1`) arrive as a release component, not as a package.
        // The screen has to be able to say that, which it cannot do while the
        // list it is handed is the same list for every release.
        let for_32 = super::packages_for("AmigaOS 3.2").unwrap();
        assert!(
            for_32.is_empty(),
            "got {:?}",
            for_32.iter().map(|p| &p.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_package_naming_a_release_art_does_not_ship_is_refused() {
        // The shipped data is checked by the test above; this checks the
        // *gate*, so a fourth package added later cannot name "AmigaOS 3.5"
        // and be silently unreachable from every screen.
        let json = package_json_with_releases(r#"["AmigaOS 3.5"]"#);
        let err = super::parse(&json).expect_err("an unknown release must be refused");
        assert!(
            format!("{err}").contains("AmigaOS 3.5"),
            "the refusal must name the release it did not recognise, got: {err}"
        );
    }

    #[test]
    fn a_package_naming_no_release_is_refused() {
        let json = package_json_with_releases("[]");
        let err = super::parse(&json).expect_err("an empty release list must be refused");
        assert!(
            format!("{err}").contains("release"),
            "the refusal must say what is missing, got: {err}"
        );
    }

    /// Every shipped package parses and validates. This is the test that
    /// fails when a JSON file is added and its rules are wrong.
    #[test]
    fn every_shipped_package_parses() {
        for p in super::packages().expect("the shipped packages must parse") {
            assert!(!p.id.is_empty());
            assert!(!p.media.is_empty());
            assert!(!p.component.rules.is_empty(), "{} has no rules", p.id);
        }
    }

    /// `requires` names a package that exists. A dependency on something ART
    /// does not ship is a recipe that can never be satisfied.
    #[test]
    fn every_requirement_names_a_shipped_package() {
        let all = super::packages().unwrap();
        let ids: Vec<&str> = all.iter().map(|p| p.id.as_str()).collect();
        for p in &all {
            for need in &p.requires {
                assert!(
                    ids.contains(&need.as_str()),
                    "{} requires unknown {need}",
                    p.id
                );
            }
        }
    }

    /// **ART-167, as shipped data.** The two packages whose top-level
    /// directory is claimed by more than one of the owner's archives declare
    /// the second identity fact; the one whose is unique declares none.
    ///
    /// Both directions on purpose. A missing distinguisher is the bug that
    /// was filed — the Turkish pack unselectable against a real folder — and
    /// a distinguisher that spread to every package would be the opposite
    /// bug: a condition that separates nothing can only refuse an archive
    /// that would have worked.
    ///
    /// The strings are the measurement's, not a guess: 44 `.lha` archives
    /// walked (`scripts/lha-package-identity.py`, 2026-08-20), `LocaleUpdate`
    /// claimed by eight, `BoingBag3.9-2` by two, `BoingBag3.9-1` by one.
    #[test]
    fn only_the_packages_with_a_shared_top_level_directory_declare_a_distinguisher() {
        assert_eq!(
            super::by_id("locale-turkish")
                .unwrap()
                .distinguished_by
                .as_deref(),
            Some("locale/catalogs/t\u{FC}rk\u{E7}e"),
            "eight archives carry LocaleUpdate; only the language drawer tells them apart"
        );
        assert_eq!(
            super::by_id("boingbag-39-2")
                .unwrap()
                .distinguished_by
                .as_deref(),
            Some("AmigaOS-Update"),
            "BoingBag39-2-Contribution.lha carries the same top-level name and no payload"
        );
        assert_eq!(
            super::by_id("boingbag-39-1").unwrap().distinguished_by,
            None,
            "its top-level name is unique; a needless condition can only refuse a good archive"
        );
    }

    /// BoingBag 2 assumes BoingBag 1. Applying them the other way round is a
    /// wrong system, not a warning (spec §8).
    #[test]
    fn boingbag_two_requires_boingbag_one() {
        let two = super::by_id("boingbag-39-2").unwrap();
        assert!(two.requires.contains(&"boingbag-39-1".to_string()));
    }

    /// **ART-166, as shipped data rather than as prose.** Both BoingBag
    /// recipes name a payload ART cannot read on the host, so neither may
    /// be offered as placeable; the Turkish catalog pack has no such block
    /// and must not acquire one by accident. Asserted in both directions
    /// because a block that spreads to every package would silently turn
    /// the whole feature off and still look "safe".
    #[test]
    fn both_boingbags_declare_the_encrypted_payload_block_and_the_catalog_pack_does_not() {
        use super::super::HostPlacementBlock;
        for id in ["boingbag-39-1", "boingbag-39-2"] {
            assert_eq!(
                super::by_id(id).unwrap().host_placement_block,
                Some(HostPlacementBlock::EncryptedPayload),
                "{id} names an encrypted payload and cannot be placed from the host"
            );
        }
        assert_eq!(
            super::by_id("locale-turkish").unwrap().host_placement_block,
            None,
        );
    }

    /// A package that declares nothing about placement is placeable — the
    /// field's default, pinned so a `#[serde(default)]` that ever became a
    /// `Some` could not go unnoticed.
    #[test]
    fn a_package_that_says_nothing_about_placement_is_placeable() {
        let package = parse(
            r#"{ "id": "x", "name": "X", "releases": ["AmigaOS 3.9"], "media": "X",
                 "rules": [ { "from": "C/A", "to": "C/A", "kind": "file" } ] }"#,
        )
        .unwrap();
        assert_eq!(package.host_placement_block, None);
    }

    // -----------------------------------------------------------------
    // `amiga_installer` — what runs on the Amiga when the host cannot place
    // this package's files.
    // -----------------------------------------------------------------

    /// Both BoingBags declare an installer; the Turkish pack does not, because
    /// ART already places its files directly.
    #[test]
    fn the_boingbags_declare_an_installer_and_the_locale_pack_does_not() {
        assert!(super::by_id("boingbag-39-1")
            .unwrap()
            .amiga_installer
            .is_some());
        assert!(super::by_id("boingbag-39-2")
            .unwrap()
            .amiga_installer
            .is_some());
        assert!(super::by_id("locale-turkish")
            .unwrap()
            .amiga_installer
            .is_none());
    }

    /// The measured command line, pinned. Both BoingBags' own `Install`
    /// scripts run `C/Updater AmigaOS-Update "<target>"` — read out of the
    /// owner's real archives on 2026-08-20 with 7-Zip 26.02, line 858 of
    /// `BoingBag3.9-1/Install` and line 1615 of `BoingBag3.9-2/Install` —
    /// and `C/Updater` is a real entry in each wrapper LHA (25 588 bytes in
    /// 3.9-1, 42 676 in 3.9-2), not an assumption.
    ///
    /// Pinned rather than left to
    /// [`the_boingbags_declare_an_installer_and_the_locale_pack_does_not`],
    /// which only asks whether *something* is declared: the AmigaOS 3.9
    /// recipe shipped fourteen paths nobody had read out of the medium, and
    /// every one of them was wrong. A test that says "some path is declared"
    /// would have passed for all fourteen.
    #[test]
    fn the_declared_updater_is_the_one_the_packages_own_install_script_runs() {
        for id in ["boingbag-39-1", "boingbag-39-2"] {
            let installer = super::by_id(id).unwrap().amiga_installer.unwrap();
            assert_eq!(installer.program, "C/Updater", "{id}");
            assert_eq!(installer.args, vec!["AmigaOS-Update".to_string()], "{id}");
        }
    }

    /// **The installer path is inside the package, and only inside it.**
    /// `PlannedRun::program` is a whole AmigaDOS path; this is not, and the
    /// difference is load-bearing: a recipe that could write `ARTWork:...`
    /// here would reach into ART's own work volume — the one thing
    /// `workvol::script` refuses — by travelling as a *package-relative*
    /// path that the command layer then prefixes a volume onto.
    ///
    /// Refusing every `:` is what keeps the two kinds of path apart. It is
    /// not a copy of `claims_work_volume`: that guard decides which volumes
    /// a composed run may name, this one decides that a recipe may not name
    /// a volume at all.
    #[test]
    fn an_installer_path_may_not_name_a_volume() {
        for program in ["ARTWork:art-install", "artwork:x", "PKG:C/Updater", "SYS:C"] {
            let err = installer_json(program, &[]).unwrap_err().to_string();
            assert!(err.contains(program), "{program} was accepted: {err}");
        }
    }

    /// Nor climb out of it. A leading or doubled `/` is AmigaDOS's own
    /// parent-directory notation, so `//C/Updater` is one drawer above the
    /// package — a path the archive never carried.
    #[test]
    fn an_installer_path_may_not_climb_out_of_the_package() {
        for program in ["/C/Updater", "//C/Updater", "../C/Updater", "C//Updater"] {
            assert!(
                installer_json(program, &[]).is_err(),
                "{program} was accepted"
            );
        }
    }

    /// An empty program is not a declaration, it is an omission spelled
    /// `""` — refused rather than parsed into a run with nothing to run.
    #[test]
    fn an_installer_must_actually_name_a_program() {
        assert!(installer_json("", &[]).is_err());
        assert!(installer_json("   ", &[]).is_err());
    }

    /// **A recipe is parsed, so it is checked.** Shipped data is still data
    /// on disk: a hand-edited file must not be able to add a second command
    /// to the script this eventually becomes. The guard is the one
    /// `workvol::script` uses, applied here so a bad value is refused when
    /// the JSON is read rather than several layers later.
    ///
    /// **Not one of these programs contains a `:`, and that is the point.**
    /// The first version of this test wrote `C/Updater;Delete SYS:#?`, and
    /// it passed with `refuse_shell_metacharacters` deleted from the program
    /// path entirely — the `SYS:` at the end tripped the volume refusal
    /// above, so the guard the test was named for was never reached. Putting
    /// that defect back is what found it. Every case below is refusable by
    /// this guard and by nothing else in `validate_installer`: `;`, `>`, `$`
    /// and `` ` `` all pass `check_name`, which only polices `:`, `/`,
    /// control characters and Latin-1.
    #[test]
    fn an_installer_may_not_carry_a_shell_metacharacter() {
        for program in [
            "C/Updater;Delete SYS",
            "C/Updater >L/x",
            "C/$Updater",
            "C/`Updater`",
        ] {
            assert!(
                installer_json(program, &[]).is_err(),
                "{program} was accepted"
            );
        }
        for arg in [
            "AmigaOS-Update >S/x",
            "a`Delete DH0`",
            "AmigaOS-Update;Delete",
            "AmigaOS-Update\nDelete",
            "$Update",
        ] {
            assert!(
                installer_json("C/Updater", &[arg]).is_err(),
                "{arg} was accepted"
            );
        }
    }

    /// An empty argument is refused too: it would vanish from the composed
    /// command line, so a recipe saying it and a recipe not saying it would
    /// produce the same run.
    #[test]
    fn an_installer_argument_may_not_be_empty() {
        assert!(installer_json("C/Updater", &[""]).is_err());
    }

    /// The plain case, so every refusal above is refusing something rather
    /// than refusing everything.
    #[test]
    fn an_ordinary_installer_declaration_parses() {
        let package = installer_json("C/Updater", &["AmigaOS-Update"]).unwrap();
        let installer = package.amiga_installer.unwrap();
        assert_eq!(installer.program, "C/Updater");
        assert_eq!(installer.args, vec!["AmigaOS-Update".to_string()]);
    }

    /// A package with no `amiga_installer` key parses to `None` — the honest
    /// answer for a package ART already places directly, pinned so a
    /// `#[serde(default)]` that ever grew a value could not go unnoticed.
    ///
    /// **This test cannot tell an absence from a misspelling, and its first
    /// version claimed it could** (review M1). `None` is what a file with no
    /// such key gives *and* what a file with a mistyped one used to give, so
    /// the property that actually matters is pinned next door, by
    /// [`a_misspelled_key_is_refused_by_name_not_silently_dropped`]. What
    /// remains here is only what it says: the default is `None`.
    #[test]
    fn a_package_that_names_no_installer_declares_none() {
        let package = parse(
            r#"{ "id": "x", "name": "X", "releases": ["AmigaOS 3.9"], "media": "X",
                 "rules": [ { "from": "C/A", "to": "C/A", "kind": "file" } ] }"#,
        )
        .unwrap();
        assert_eq!(package.amiga_installer, None);
    }

    /// **The defect review M1 found, put where it cannot come back.** A
    /// recipe written `"amigaInstaller"` — the camelCase spelling two doc
    /// comments in this file used to recommend — parsed as `Ok`, with the
    /// declaration dropped and nothing said. The package would then have
    /// been reported as one ART cannot run: a lie that reads like a design
    /// decision.
    ///
    /// Three spellings, because they fail for three different reasons and
    /// all three have to be refused **by name**: the camelCase one that was
    /// documented, an ordinary typo, and a note whose author forgot the
    /// leading underscore. The fourth case is the one that must still be
    /// accepted — `_why_…`, which is how every shipped recipe carries its
    /// measurements, and which is why `deny_unknown_fields` could not be
    /// used.
    #[test]
    fn a_misspelled_key_is_refused_by_name_not_silently_dropped() {
        for key in ["amigaInstaller", "amiga_instaler", "why_no_installer"] {
            let json = serde_json::json!({
                "id": "x",
                "name": "X", "releases": ["AmigaOS 3.9"],
                "media": "X",
                "rules": [ { "from": "C/A", "to": "C/A", "kind": "file" } ],
                key: { "program": "C/Updater", "args": ["AmigaOS-Update"] },
            });
            let err = parse(&json.to_string())
                .expect_err(&format!("'{key}' must be refused, not dropped"))
                .to_string();
            assert!(
                err.contains(key),
                "'{key}' was refused without naming it: {err}"
            );
        }
    }

    /// The other half, and it is not decoration: a rule that refused
    /// everything unknown would break every shipped recipe, since all three
    /// carry `_why_…` notes.
    #[test]
    fn a_note_to_a_human_is_still_allowed_through() {
        let json = serde_json::json!({
            "id": "x",
            "name": "X", "releases": ["AmigaOS 3.9"],
            "media": "X",
            "_why_this_exists": ["a note, kept in the file, ignored by ART"],
            "rules": [ { "from": "C/A", "to": "C/A", "kind": "file" } ],
        });
        parse(&json.to_string()).expect("a `_`-prefixed note must be allowed");
    }

    /// **A fault in a package file is reported against a package file**
    /// (review m3). `validate_path` is shared with `recipe.rs` — correctly,
    /// since a second copy would drift — but it used to label every refusal
    /// it made `malformed recipe`, so a person who mistyped a path in a
    /// *package* JSON was sent to a different file to look for a mistake
    /// that was not there. The label now follows the caller.
    ///
    /// Asserted in both directions: this is a `format` field, so a mutation
    /// that hard-coded `"package"` everywhere would be just as wrong and
    /// would pass a one-sided test. `recipe::parse` still says `recipe`.
    #[test]
    fn a_package_path_fault_is_reported_against_the_package_file() {
        let json = serde_json::json!({
            "id": "x",
            "name": "X", "releases": ["AmigaOS 3.9"],
            "media": "X",
            "rules": [ { "from": "C/A", "to": "C/A", "kind": "file" } ],
            // `..` climbs out, and `validate_path` is the only thing that
            // refuses it — so the message under test is one it wrote.
            "amiga_installer": { "program": "../C/Updater" },
        });
        let err = parse(&json.to_string()).unwrap_err().to_string();
        assert!(err.contains("malformed package"), "got {err}");

        let recipe_err = super::super::recipe::parse(
            r#"{ "release": "X", "components": [
                   { "id": "c", "media": "M",
                     "rules": [ { "from": "..", "to": "C", "kind": "file" } ] } ] }"#,
        )
        .unwrap_err()
        .to_string();
        assert!(recipe_err.contains("malformed recipe"), "got {recipe_err}");
    }

    /// **The half of "each its own string" that is enforced here**, pinned
    /// because the field's doc comment claims it and a claim with no test is
    /// how ART-180 and ART-181 happened. A recipe cannot hand ART one line
    /// to split: `args` is a list, and serde refuses a bare string in its
    /// place at parse time.
    ///
    /// The other half — whether an argument that *contains* a space arrives
    /// as one token or two — is not enforced here and the doc comment now
    /// says so (review m1). It belongs to whoever quotes the command line.
    #[test]
    fn args_given_as_one_line_are_refused_by_the_type() {
        let json = serde_json::json!({
            "id": "x",
            "name": "X", "releases": ["AmigaOS 3.9"],
            "media": "X",
            "rules": [ { "from": "C/A", "to": "C/A", "kind": "file" } ],
            "amiga_installer": { "program": "C/Updater", "args": "AmigaOS-Update DH0:" },
        });
        let err = parse(&json.to_string()).unwrap_err().to_string();
        assert!(err.contains("invalid type"), "got {err}");
    }

    /// One minimal package JSON carrying an `amiga_installer`, so the
    /// refusals above test [`parse`]'s real boundary rather than a
    /// hand-built `AmigaInstaller` that never went through it.
    fn installer_json(program: &str, args: &[&str]) -> CoreResult<Package> {
        let installer = AmigaInstaller {
            program: program.to_string(),
            args: args.iter().map(|a| a.to_string()).collect(),
            minimum_version: None,
            overlays: Vec::new(),
            required_medium: None,
            post_install: Vec::new(),
        };
        let json = serde_json::json!({
            "id": "x",
            "name": "X", "releases": ["AmigaOS 3.9"],
            "media": "X",
            "rules": [ { "from": "C/A", "to": "C/A", "kind": "file" } ],
            // The key is snake_case, like every other key in a shipped
            // package JSON (`host_placement_block`, `distinguished_by`):
            // `RawPackage` declares no `rename_all`, and a recipe file's own
            // spelling is what a person editing one has to type.
            "amiga_installer": installer,
        });
        parse(&json.to_string())
    }

    /// The declared program exists inside the archive it belongs to. A path
    /// nobody checked is how the 3.9 recipe shipped fourteen wrong ones.
    #[test]
    #[ignore = "reads the owner's real packages: set ART_PACKAGE_FOLDER and run with --ignored --nocapture"]
    fn every_declared_installer_exists_in_its_own_archive_when_asked() {
        use super::super::scan::{find_packages, package_for, MediaMatch};
        use super::super::source::MediaSource;
        use super::super::source_archive::ArchiveSource;

        // **A skipped run must not read as a passed one** (review m4). With
        // no folder this test prints `ok`, exactly like a run that checked
        // both BoingBags against the owner's real archives — and this is the
        // one test standing between a recipe and the fourteen-wrong-paths
        // failure the 3.9 release recipe shipped. It still returns rather
        // than fails, because every other real-material test in this
        // codebase gates the same way and because the variable is genuinely
        // absent on a machine with no packages; what changes is that it says
        // so. `eprintln!` is captured unless `--nocapture` is passed, which
        // is why the `#[ignore]` message above names both.
        let Ok(folder) = std::env::var("ART_PACKAGE_FOLDER") else {
            eprintln!("SKIPPED: ART_PACKAGE_FOLDER is not set, so nothing was checked");
            return;
        };
        let found = find_packages(std::path::Path::new(&folder)).expect("the folder must scan");

        let mut checked = 0usize;
        for package in super::packages().unwrap() {
            let Some(installer) = package.amiga_installer.as_ref() else {
                continue;
            };
            // The archive this package's own identity resolves to — never
            // the first `.lha` whose name looks right. Eight of the owner's
            // archives share one top-level directory.
            let MediaMatch::Found(archive) =
                package_for(&found, &package.media, package.distinguished_by.as_deref())
            else {
                panic!(
                    "{}: no single archive in {folder} is this package's",
                    package.id
                );
            };

            // The **wrapper**, opened with `open` rather than `open_nested`:
            // `C/Updater` sits beside the payload member, not inside it, and
            // that is the whole reason it can be run at all (ART-166).
            let mut source = ArchiveSource::open(&archive.path).expect("the archive must open");
            let entry = source
                .entry(&installer.program)
                .expect("looking up a path must not fail")
                .unwrap_or_else(|| {
                    panic!(
                        "{}: '{}' is not in {}",
                        package.id,
                        installer.program,
                        archive.path.display()
                    )
                });
            assert!(
                !entry.is_dir,
                "{}: '{}' is a drawer in {}, not a program",
                package.id,
                installer.program,
                archive.path.display()
            );
            assert!(
                entry.size > 0,
                "{}: '{}' is empty",
                package.id,
                installer.program
            );

            // Every argument that names something inside the package has to
            // be there too. `AmigaOS-Update` is the payload member, and a
            // recipe that named the wrong one would run the Updater against
            // a file the archive does not carry. An argument carrying a `:`
            // names a volume rather than a path inside the package, so it is
            // not something this archive could answer for — skipped, not
            // failed.
            for arg in installer.args.iter().filter(|a| !a.contains(':')) {
                assert!(
                    source.entry(arg).expect("lookup must not fail").is_some(),
                    "{}: argument '{arg}' is not in {}",
                    package.id,
                    archive.path.display()
                );
            }
            checked += 1;
        }
        assert!(checked > 0, "no package with an installer was checked");
    }

    // -----------------------------------------------------------------
    // ART-186: the declaration a BoingBag 1 run needs
    // -----------------------------------------------------------------

    /// What the shipped recipes actually say. Both halves matter: BoingBag
    /// 3.9-1 declares the minimum and the overlay because its own `Updater` is
    /// 45.13, and BoingBag 3.9-2 declares **neither**, because no source says
    /// any build of its 45.19 is unfit and inventing a requirement would be
    /// §89's mistake from the other side.
    #[test]
    fn only_boingbag_one_declares_a_minimum_version_and_an_overlay() {
        let one = super::by_id("boingbag-39-1")
            .unwrap()
            .amiga_installer
            .unwrap();
        assert_eq!(one.minimum_version.as_deref(), Some("45.15"));
        assert_eq!(
            one.overlays,
            vec![InstallerOverlay {
                from: "BoingBag3.9-1-UAE/BoingBag3.9-1".to_string(),
                to: String::new(),
            }],
            "the overlay's `from` is a whole path from the UAE archive's own root, because that \
             archive nests the drawer one level deeper than the package it patches"
        );

        let two = super::by_id("boingbag-39-2")
            .unwrap()
            .amiga_installer
            .unwrap();
        assert_eq!(two.minimum_version, None);
        assert_eq!(two.overlays, Vec::new());
    }

    /// `"45.15"` is two integers or it is refused at parse time — so a `Some`
    /// reaching the command layer always parses, and the command layer never
    /// has to decide what a malformed one means.
    #[test]
    fn a_minimum_version_that_is_not_two_numbers_is_refused() {
        for bad in ["45", "forty-five.fifteen", "45.", ".15", "", "45.15.3"] {
            let json = serde_json::json!({
                "id": "x",
                "name": "X", "releases": ["AmigaOS 3.9"],
                "media": "X",
                "rules": [ { "from": "C/A", "to": "C/A", "kind": "file" } ],
                "amiga_installer": { "program": "C/Updater", "minimum_version": bad },
            });
            let err = parse(&json.to_string()).unwrap_err().to_string();
            assert!(
                err.contains("minimum_version"),
                "'{bad}' must be refused by name, got {err}"
            );
        }
        // And a well-formed one parses to the pair the run compares against.
        assert_eq!(super::parse_version_pair("45.15"), Some((45, 15)));
        assert_eq!(super::parse_version_pair("45.13"), Some((45, 13)));
    }

    /// An overlay path is a path inside an archive, and every gate a rule's
    /// own `from` goes through applies to it — a recipe that could climb out
    /// would be naming files outside the archive ART unpacked.
    #[test]
    fn an_overlay_path_that_leaves_its_archive_is_refused() {
        for (from, to) in [
            ("../outside", ""),
            ("BoingBag3.9-1-UAE/../../outside", ""),
            ("/absolute", ""),
            ("", ""),
            ("BoingBag3.9-1-UAE/BoingBag3.9-1", "../../outside"),
            ("BoingBag3.9-1-UAE/BoingBag3.9-1", "/absolute"),
        ] {
            let json = serde_json::json!({
                "id": "x",
                "name": "X", "releases": ["AmigaOS 3.9"],
                "media": "X",
                "rules": [ { "from": "C/A", "to": "C/A", "kind": "file" } ],
                "amiga_installer": {
                    "program": "C/Updater",
                    "overlays": [ { "from": from, "to": to } ],
                },
            });
            assert!(
                parse(&json.to_string()).is_err(),
                "'{from}' -> '{to}' must be refused"
            );
        }
    }

    /// An unknown id is refused by name, never defaulted to some other
    /// package — the same rule `recipe::by_release` follows, for the same
    /// reason.
    #[test]
    fn an_unknown_package_is_refused_by_name() {
        let err = super::by_id("boingbag-39-9").unwrap_err().to_string();
        assert!(err.contains("boingbag-39-9"), "got {err}");
    }

    /// Ordering is derived from `requires`, not from the order the user
    /// happened to tick the boxes in.
    #[test]
    fn selection_order_does_not_decide_application_order() {
        let a = super::order(&["boingbag-39-2".into(), "boingbag-39-1".into()]).unwrap();
        let b = super::order(&["boingbag-39-1".into(), "boingbag-39-2".into()]).unwrap();
        assert_eq!(a, b);
        assert_eq!(
            a,
            vec!["boingbag-39-1".to_string(), "boingbag-39-2".to_string()]
        );
    }

    /// Choosing a package without what it requires is refused, saying what
    /// is missing — not silently added, because adding a whole package the
    /// user did not ask for is a bigger surprise than a refusal.
    #[test]
    fn a_requirement_that_was_not_chosen_is_refused_by_name() {
        let err = super::order(&["boingbag-39-2".into()])
            .unwrap_err()
            .to_string();
        assert!(err.contains("boingbag-39-1"), "got {err}");
    }

    /// A minimal, valid `Package` for the synthetic tests below — real
    /// enough to pass `validate_component` (one file rule), not meant to
    /// resemble any shipped package.
    fn synthetic(id: &str, requires: &[&str]) -> Package {
        Package {
            id: id.to_string(),
            releases: vec!["AmigaOS 3.9".to_string()],
            name: id.to_string(),
            media: "SyntheticMedia".to_string(),
            member: None,
            distinguished_by: None,
            amiga_installer: None,
            requires: requires.iter().map(|s| s.to_string()).collect(),
            requires_components: Vec::new(),
            host_placement_block: None,
            component: Component {
                id: id.to_string(),
                media: "SyntheticMedia".to_string(),
                rules: vec![PathRule {
                    from: "X".to_string(),
                    to: "X".to_string(),
                    kind: RuleKind::File,
                }],
                required: false,
                condition: None,
                overrides: Vec::new(),
                user_startup: Vec::new(),
                activate: Vec::new(),
                exclusive_group: None,
                label_key: None,
                available: true,
            },
        }
    }

    /// `order` only ever reads the shipped list ([`packages`]), which has no
    /// cycle in it — a hand-built one is the only way to exercise the cycle
    /// branch at all, which is why this drives [`order_over`] directly
    /// rather than [`order`] itself.
    ///
    /// Checks the stuck ids are actually named, not just the word "cycle" —
    /// the first version of this refusal said "by name" in its own doc
    /// comment and did not do it (review, Task 4 fix round 1).
    #[test]
    fn a_dependency_cycle_in_hand_built_data_is_refused_rather_than_hung() {
        let x = synthetic("x", &["y"]);
        let y = synthetic("y", &["x"]);
        let err = super::order_over(&["x".to_string(), "y".to_string()], &[x, y])
            .unwrap_err()
            .to_string();
        assert!(err.contains("cycle"), "got {err}");
        assert!(err.contains('x') && err.contains('y'), "got {err}");
    }

    /// A duplicate `chosen` id used to inflate the in-degree of everything
    /// that id required, one extra time per repeat, and come out the far end
    /// misreported as a cycle rather than refused for what it actually is —
    /// confirmed by executing it (review, Task 4 fix round 1). Refused
    /// before any graph bookkeeping runs, by name, and distinguishably from
    /// the cycle error.
    #[test]
    fn a_duplicate_chosen_id_is_refused_rather_than_applied_twice_or_misreported_as_a_cycle() {
        let a = synthetic("a", &[]);
        let err = super::order_over(&["a".to_string(), "a".to_string()], &[a])
            .unwrap_err()
            .to_string();
        assert!(err.contains("more than once"), "got {err}");
        assert!(!err.contains("cycle"), "got {err}");
    }

    /// Two shipped JSON files naming the same id is the failure mode that
    /// silently drops a third: a copy-paste of one entry in `SHIPPED_JSON`
    /// leaves the array's length looking right while one real package
    /// vanishes. `parse_all` — what `packages()` itself calls — catches it
    /// directly; this test constructs the collision without touching the
    /// real shipped list.
    /// ART-193. Both BoingBags' `Updater`s verify the original AmigaOS 3.9
    /// CD-ROM — their own printable strings say so, and each recipe's
    /// `_why_required_medium` records the reading — so both must declare it.
    /// The volume is written the way a disc states it: no colon, whatever the
    /// installer's own `AmigaOS3.9:Videos/…` strings look like.
    #[test]
    fn both_boingbags_declare_the_disc_their_updater_verifies() {
        for id in ["boingbag-39-1", "boingbag-39-2"] {
            let package = by_id(id).unwrap();
            let medium = package
                .amiga_installer
                .as_ref()
                .and_then(|i| i.required_medium.as_ref())
                .unwrap_or_else(|| panic!("{id} declares no required medium"));
            assert_eq!(medium.volume, "AmigaOS3.9", "{id}");
            assert!(!medium.volume.contains(':'), "{id}");
            assert!(!medium.name.trim().is_empty(), "{id}");
        }
    }

    /// A recipe author will copy the volume out of the installer's own
    /// strings, colon and all. Left alone, `AmigaOS3.9:` would be compared
    /// against the name a disc actually states — `AmigaOS3.9` — and the right
    /// disc would be refused.
    #[test]
    fn a_required_medium_written_as_a_device_reference_is_refused() {
        let json = serde_json::json!({
            "id": "x",
            "name": "X", "releases": ["AmigaOS 3.9"],
            "media": "M",
            "amiga_installer": {
                "program": "C/Updater",
                "required_medium": { "volume": "AmigaOS3.9:", "name": "the disc" }
            },
            "rules": [{ "from": "C", "to": "C", "kind": "subtree" }]
        })
        .to_string();
        let err = parse(&json).unwrap_err();
        assert!(matches!(err, CoreError::Malformed { .. }), "{err:?}");
        assert!(err.to_string().contains("volume name"), "{err}");
    }

    #[test]
    fn two_shipped_packages_sharing_an_id_are_refused() {
        let err = super::parse_all(&[BOINGBAG_39_1_JSON, BOINGBAG_39_1_JSON])
            .unwrap_err()
            .to_string();
        assert!(err.contains("boingbag-39-1"), "got {err}");
    }

    /// The other half of the same failure mode: a fourth `recipes/packages/`
    /// JSON file added but never added to `SHIPPED_JSON`, silently
    /// unreachable — `recipe.rs`'s own history is exactly this bug for
    /// releases (AmigaOS 3.9 shipped with one component because nothing
    /// checked the component list matched what was intended). Filesystem
    /// truth, not a synthetic fixture, on purpose: what is being checked is
    /// that the crate's own embedded source tree — the thing `include_str!`
    /// actually reads from — has not drifted from `SHIPPED_JSON`.
    #[test]
    fn every_package_json_file_on_disk_is_wired_into_shipped_json() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/core/osinstall/recipes/packages");
        let on_disk = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()))
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".json"))
            .count();
        assert_eq!(
            on_disk,
            SHIPPED_JSON.len(),
            "recipes/packages/ holds {on_disk} JSON files but SHIPPED_JSON lists {} — \
             a file was added or removed without updating the other",
            SHIPPED_JSON.len()
        );
    }

    /// A three-package chain, out of order and with the middle package
    /// requiring the last rather than the first — the shipped two-package
    /// case can't tell a stable sort from a lucky one.
    #[test]
    fn a_longer_chain_still_sorts_correctly_regardless_of_input_order() {
        let a = synthetic("a", &[]);
        let b = synthetic("b", &["a"]);
        let c = synthetic("c", &["b"]);
        let chosen = vec!["c".to_string(), "a".to_string(), "b".to_string()];
        let ordered = super::order_over(&chosen, &[a, b, c]).unwrap();
        assert_eq!(
            ordered,
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
    }

    /// ART-162, declared in the data rather than left to the user to
    /// notice: the Turkish catalogs need the Locale component, and a
    /// component is not a package — so it cannot be, and is not, a
    /// `requires` entry.
    #[test]
    fn the_turkish_catalogs_declare_the_locale_component_they_need() {
        let package = by_id("locale-turkish").unwrap();
        assert_eq!(
            package.requires_components,
            vec!["locale-base".to_string()],
            "without locale-base this package writes catalogs nothing can open"
        );
        assert!(
            package.requires.is_empty(),
            "'locale-base' is a recipe component, not a package — naming it in \
             `requires` would make `order` refuse a correct selection by name"
        );
    }

    /// Every other shipped package says it needs no component, and says so
    /// by carrying an empty list rather than by the field being absent from
    /// the type — so a reader can tell "nothing needed" from "nobody
    /// thought about it" only by reading the JSON, which is why this test
    /// pins the measured answer for all three.
    #[test]
    fn only_the_turkish_catalogs_need_a_component_today() {
        let with_components: Vec<String> = packages()
            .unwrap()
            .into_iter()
            .filter(|p| !p.requires_components.is_empty())
            .map(|p| p.id)
            .collect();
        assert_eq!(with_components, vec!["locale-turkish".to_string()]);
    }

    /// **ART-227.** What the two BoingBags need done after their own
    /// `Updater` has run, pinned against the shipped data.
    ///
    /// Every path here was checked to exist in the tree ART's own run
    /// produced before it was written into a recipe, because
    /// [`crate::core::amigainstall::finish`] refuses a file that is not there
    /// rather than skipping it — a list with one wrong name would turn every
    /// BoingBag 1 install into a failure at the last step.
    ///
    /// The rotation is the one with teeth and is asserted whole: BoingBag 2's
    /// `Updater` leaves a 321 768-byte ROM update under a name `SetPatch`
    /// does not load, and without this step a tree reports itself updated
    /// while running the ROM update it had before.
    #[test]
    fn the_boingbags_declare_what_their_updater_leaves_undone_art_227() {
        use crate::core::amigainstall::finish::PostStep;

        let packages = packages().expect("the shipped packages must parse");
        let installer_of = |id: &str| {
            packages
                .iter()
                .find(|p| p.id == id)
                .unwrap_or_else(|| panic!("no shipped package '{id}'"))
                .amiga_installer
                .clone()
                .unwrap_or_else(|| panic!("'{id}' declares no Amiga-side installer"))
        };

        // --- BoingBag 1: ten files, two groups of protection bits ----------
        let one = installer_of("boingbag-39-1").post_install;
        let expected_one: Vec<PostStep> = [
            ("C/LoadMonDrvs", "p"),
            ("C/LoadResource", "p"),
            ("C/MakeDir", "p"),
            ("C/MakeLink", "p"),
            ("C/SetEnv", "p"),
            ("C/WBInfo", "p"),
            ("C/WBRun", "p"),
            ("S/Start-Amplifier.rexx", "s"),
            ("S/Startup-Sequence-BB3.9-1", "s"),
            ("S/Stream-Amplifier.rexx", "s"),
        ]
        .into_iter()
        .map(|(path, add)| PostStep::Protect {
            path: path.into(),
            add: add.into(),
        })
        .collect();
        assert_eq!(
            one, expected_one,
            "the seven commands `Resident` needs the pure bit on, and the three scripts \
             that need the script bit"
        );

        // --- BoingBag 2: the rotation --------------------------------------
        assert_eq!(
            installer_of("boingbag-39-2").post_install,
            vec![PostStep::ReplaceKeepingBackup {
                target: "Devs/AmigaOS ROM Update".into(),
                replacement: "Devs/AmigaOS ROM Update.BB39-2".into(),
            }],
            "without this the 321 768-byte ROM update sits beside the old one under a \
             name SetPatch does not load"
        );

        // --- and nothing else has grown one by accident --------------------
        for package in &packages {
            if package.id.starts_with("boingbag-") {
                continue;
            }
            let steps = package
                .amiga_installer
                .as_ref()
                .map(|i| i.post_install.len())
                .unwrap_or(0);
            assert_eq!(
                steps, 0,
                "'{}' declares post-install steps nobody has measured a need for",
                package.id
            );
        }
    }
}
