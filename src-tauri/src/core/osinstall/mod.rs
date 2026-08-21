//! Installing AmigaOS from the user's own media (SD-2 · G5).
//!
//! ## A component is a set of paths, not a disk
//!
//! Measured, not assumed: `ModulesA1200_3.2.adf` holds 14 commands in `C/`,
//! and **thirteen are boot-floppy copies of commands `Workbench3.2` already
//! carries**. Exactly one, `LoadModule`, is new. Copying that disk onto `SYS:`
//! downgrades thirteen commands. `HDSetup3.2` (22), `DiskDoctor` (39) and
//! `Storage3.2` (9) have the same shape.
//!
//! So the unit of choice is a named set of [`PathRule`]s, and the recipe says
//! which paths on which media are actually wanted.
//!
//! ## The one genuine file collision: `C/LoadModule`
//!
//! `Storage3.2` and `ModulesA1200_3.2` both carry a file at `C/LoadModule` —
//! the only place two components in the shipped recipe actually write the
//! same file (everything else two or more components touch is a `Subtree`
//! merge point, like `Devs/` or `Locale/Languages`, not a collision — see
//! the doc comment on `recipe::tests::no_two_components_claim_one_destination_without_declaring_it`).
//! The two copies are measured **byte-identical**: SHA-256
//! `35acfea734816965d271352f59c3643963f69c7e4b2469e3473c5f5a8a60fc14` for
//! both. So the direction can't break anything either way; the recipe has
//! `modules-a1200` declare the override, because the Modules disk is the one
//! that ships `LoadModule` *beside the modules it exists to load* — the
//! command belongs with that disk's own purpose, not with the general
//! toolkit disk that happens to also carry a copy.
//!
//! `recipe`, `source`, `scan`, `plan` (the ROM condition and [`plan::plan`]
//! itself — components, media and ROM resolved into an [`plan::InstallPlan`]
//! or a collected list of [`RefusalReason`]s), `apply` ([`apply::apply`]
//! — the plan turned into a real distribution tree, with a `.uaem` sidecar
//! beside every file that needs one and `apply::DistributionManifest`
//! recording what built it), `startup` ([`startup::merge_user_startup`]
//! — `apply`'s own last step, composing `S:User-Startup` from every
//! switched-on component's own lines), `verify` ([`verify::verify_volume`]
//! — reading the finished PFS3 or FFS volume back and checking it against
//! the manifest, §92's own VERIFY step), `source_cd`
//! ([`source_cd::CdSource`] — a second [`source::MediaSource`], an ISO9660
//! disc rather than an ADF, so AmigaOS 3.9's own install CD is install media
//! too) and now `source_archive` ([`source_archive::ArchiveSource`] — a
//! third [`source::MediaSource`], a package archive (`BoingBag39-1.lha`, a
//! Turkish catalog pack) so an official update package is install media too)
//! and `package` ([`package::Package`] — what a package archive actually
//! contains and where each of its files goes, the same [`Component`] shape a
//! release recipe already uses, driven through the identical
//! `recipe.rs`-established `include_str!` + shipped-list mechanism) and now
//! `collide` ([`collide::preview`] — entirely read-only: what every planned
//! item in a package would land on inside a tree already built, so a
//! downgrade or an undeclared overwrite is a fact seen before it happens
//! rather than after) exist so far — this module tree lands one task at a
//! time, each adding its own `pub mod` line, so the crate compiles at the
//! end of every task rather than only at the end of the feature.

pub mod apply;
pub mod chain;
pub mod collide;
pub mod package;
pub mod plan;
pub mod recipe;
pub mod scan;
pub mod source;
pub mod source_archive;
pub mod source_cd;
/// One set of questions asked of **every** [`source::MediaSource`]
/// implementation — tests only. Three divergences between `AdfSource` and
/// `CdSource` were found one at a time by reading the two files side by
/// side; this is where the fourth fails a test instead.
#[cfg(test)]
mod source_contract;
pub mod startup;
pub mod verify;

use serde::{Deserialize, Serialize};

use crate::core::error::{CoreError, CoreResult};

/// Whether a rule takes one file or a whole subtree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuleKind {
    File,
    Subtree,
}

/// One path taken out of a component's media.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathRule {
    /// Where it lives on the media, `/`-separated: `LIBS/Modules`.
    pub from: String,
    /// Where it goes in the tree, `/`-separated: `Libs/Modules`.
    pub to: String,
    pub kind: RuleKind,
}

/// When a component applies without the user being asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "condition", rename_all = "kebab-case")]
pub enum Condition {
    /// On when the paired Kickstart's own stated major is below this.
    ///
    /// The ROM's **own header** answers it (`core::rom::stated_version`), not a
    /// checksum table — which is what keeps ART-104 from repeating: the
    /// user's licensed A1200 dump is not in `KNOWN_ROMS`, and a condition
    /// resting on that table would misfire on a ROM that is right.
    RomOlderThan { major: u16 },
    /// The paired Kickstart's own stated major must be **at least** this
    /// (ART-157).
    ///
    /// The mirror of [`Condition::RomOlderThan`], and it exists because the
    /// recipe format could state a Kickstart *maximum* and not a minimum, so
    /// AmigaOS 3.9's real requirement — V40 or newer — went unstated and
    /// therefore unchecked. Approximating it through `rom-older-than` would
    /// have asserted something false, which is why the gap was filed rather
    /// than papered over.
    ///
    /// **On a `required` component this states the release's own floor, not
    /// a switch.** A `Condition` can only ever turn a component *on*
    /// (`plan::resolve_components_on`), so on a component that is on
    /// regardless it changes nothing about what gets placed — what it
    /// changes is what the finished tree *records*:
    /// `plan::rom_requirement` reads it into `PairedRom::requires_major`,
    /// which is what G9's `core::rom::pairing::compare` puts to the card's
    /// own Kickstart before anything destructive runs.
    ///
    /// Same evidence rule as its sibling: the ROM's own header answers it
    /// (`core::rom::stated_version`), never `KNOWN_ROMS` (ART-104).
    RomAtLeast { major: u16 },
}

/// The Kickstart a distribution tree was planned against, and what it needs
/// of a future one (SD-2 · G9).
///
/// **Recorded rather than recomputed.** Which components a plan switches on
/// depends on the ROM it was given — `modules-a1200` is on for a pre-V47 ROM
/// and off otherwise — so a tree carries a fact about a file that may be
/// nowhere by the time somebody puts the tree on a card. Re-planning to
/// recover it would need the original media, which is exactly what is gone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairedRom {
    /// As `core::rom::identify_rom` names it: `Kickstart 40.68 (A1200)`.
    pub name: String,
    /// Of the ROM image, decoded — so a licensed Amiga Forever dump and a
    /// bare dump of the same ROM hash alike (ART-128).
    pub sha256: String,
    /// What the ROM states about itself. `None` for pre-2.0 ROMs, which
    /// state nothing at all.
    pub stated_major: Option<u16>,
    pub compatible_models: Vec<String>,
    /// **The load-bearing field.** `Some(47)` when this tree needs a ROM of
    /// at least that major to start; `None` when it needs nothing, either
    /// because it carries its own ROM modules or because no component in it
    /// ever depended on the ROM.
    ///
    /// Taken from the recipe's own `Condition::RomOlderThan`, so the
    /// threshold is not written down a second time.
    pub requires_major: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Component {
    pub id: String,
    /// The **volume name inside the image**, not a filename: `Workbench3.2`.
    pub media: String,
    pub rules: Vec<PathRule>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub condition: Option<Condition>,
    /// Component ids this one may legitimately write over.
    #[serde(default)]
    pub overrides: Vec<String>,
    /// Lines for `S:User-Startup`, written inside this component's own block.
    #[serde(default)]
    pub user_startup: Vec<String>,
    /// Only one of a group may be chosen — `"modules"` for the Modules disks.
    #[serde(default)]
    pub exclusive_group: Option<String>,
    /// Registered but not built (CLAUDE.md, §96): shown as Coming Later.
    #[serde(default = "yes")]
    pub available: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recipe {
    /// `"AmigaOS 3.2"`.
    pub release: String,
    pub components: Vec<Component>,
}

impl Recipe {
    pub fn component(&self, id: &str) -> Option<&Component> {
        self.components.iter().find(|c| c.id == id)
    }
}

/// The key two destination paths are compared, matched and indexed by.
///
/// **A destination is compared the way AmigaDOS compares it: without regard
/// to case (ART-012).** Every *resolution* around these paths already works
/// that way — `AdfSource`, `CdSource` and `ArchiveSource` all resolve a
/// rule's `from` case-insensitively, `plan::relative_to` strips its prefix
/// case-insensitively, and the Windows filesystem the tree is built on
/// treats `C/Assign` and `C/ASSIGN` as one file. Only the *comparisons*
/// between destinations were exact, and that mismatch is a real defect on
/// ART's own shipped material, not a hypothetical:
///
/// - the Joliet-less `AmigaOS39.iso` yields `C/ASSIGN` (see `recipe.rs`'s
///   own note on why the Primary tree forces all-caps)
/// - BoingBag 3.9-1's ZIP payload yields `C/Assign` — measured
/// - so all ~211 of that package's files collide in reality and did not
///   collide in the comparison
///
/// Both entry points then failed, in opposite and equally bad ways: `Add`
/// refused all 211 as undeclared overwrites *despite* the package declaring
/// `overrides: ["workbench-base"]`, and `Produce` wrote them silently and
/// left `distribution.json` naming a file whose `sha256` matched nothing on
/// disk — a manifest lying about the tree it describes.
///
/// So there is one function, and everything that pairs destinations goes
/// through it: `plan::detect_collisions`, `apply::undeclared_overwrites`,
/// `apply::TreeWriter`'s three maps, and `collide::preview`'s own index. A
/// `HashMap` or `BTreeMap` keyed on a raw destination is the same defect in
/// a quieter form, so those are keyed on this instead.
///
/// Folded with [`fold_amiga_case`] — **international**, not ASCII-only, and
/// the correction is measured rather than tidy (fix round 1, F1). A
/// destination is never normalised for *storage*: what the manifest records
/// and what the filesystem holds stays whatever the medium spelled. This is
/// only how two of them are told apart.
pub fn destination_key(path: &str) -> String {
    fold_amiga_case(path)
}

/// Whether two destination paths name the same place — see
/// [`destination_key`].
pub fn same_destination(a: &str, b: &str) -> bool {
    fold_amiga_case(a) == fold_amiga_case(b)
}

/// One AmigaDOS name folded for comparison — ASCII **and** the Latin-1
/// accented range, the way an *international* AmigaDOS volume folds.
///
/// # Why international, when there is no volume to ask
///
/// `hash::name_hash` takes an `international` flag because a real volume
/// carries one in its bootblock (`DOS\2`/`DOS\3`), and an INTL volume folds
/// 0xC0–0xDE against 0xE0–0xFE as well as A–Z against a–z. There is no
/// bootblock behind an archive entry name, a recipe's `from`, or a
/// destination path, so nothing can be asked — the flag has to be *chosen*,
/// and this codebase chose ASCII-only by default and never said so.
///
/// **International is the right choice here, and it is the owner's own
/// material that decides it.** Read with ART's own reader, not a third-party
/// tool (fix round 1): `AmigaOS39.iso` carries **no Joliet descriptor at
/// all** — its descriptor chain is `[Primary, Primary, Terminator]` — so
/// `CdSource` reads the Primary tree and answers
/// `OS-VERSION3.9/LOCALE/CATALOGS/TÜRKÇE` (`T U+00DC R K U+00C7 E`), while
/// `BoingBag39-2-turkce.lha` spells its own drawer `türkçe`
/// (`t U+00FC r k U+00E7 e`). Four of the owner's eight language drawers are
/// non-ASCII (`türkçe`, `français`, `português`, `português-brasil`) and the
/// disc has an upper-case counterpart for every one of them. Under
/// `eq_ignore_ascii_case` those are two different names, so:
///
/// - the Turkish pack's distinguisher missed an archive that spells its
///   drawer in upper case, which is ART-167's entire point, and
/// - `destination_key` reported **no collision** between the base disc's
///   `Locale/Catalogs/TÜRKÇE` and the package's `Locale/Catalogs/türkçe` —
///   the ~34 overlapping catalogs ART-169 predicted would appear were still
///   going to read as zero, for a second and unrelated reason.
///
/// A modern AmigaOS volume is an INTL one, the owner's own names need the
/// fold, and folding *more* can only merge names AmigaDOS already treats as
/// one. Nothing that folds can turn a refusal into a wrong file: two
/// candidates that fold together become `MediaMatch::Ambiguous`, which is a
/// named refusal listing both.
///
/// # The rule, exactly
///
/// The inverse of [`hash::intl_to_upper`](crate::core::adf::hash): every
/// ASCII `A`–`Z` and every code point in `0xC0..=0xDE` **except `0xD7`**
/// (`×`, which has no lower-case form) folds down by 0x20. Expressed over
/// `char` rather than bytes because Rust strings are UTF-8 and Unicode's
/// first 256 code points *are* Latin-1 — the same identity `core::lha`'s own
/// ART-168 fix rests on, so a name decoded from a Latin-1 archive header
/// folds against a name decoded from an ISO9660 record without either being
/// re-encoded.
///
/// `0xDF` (`ß`) and `0xFF` (`ÿ`) are untouched, because Latin-1 gives
/// neither an upper-case partner and AmigaDOS's own table does not invent
/// one. Turkish's dotless `ı`/`İ` are outside Latin-1 entirely and are not
/// special-cased: this matches what an Amiga does, which is the only thing
/// it is trying to match.
pub fn fold_amiga_case(name: &str) -> String {
    name.chars().map(fold_amiga_char).collect()
}

/// Whether two AmigaDOS names are the same name — see [`fold_amiga_case`].
pub fn amiga_names_equal(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.chars()
            .map(fold_amiga_char)
            .eq(b.chars().map(fold_amiga_char))
}

/// One character folded. See [`fold_amiga_case`] for the rule and its source.
fn fold_amiga_char(c: char) -> char {
    let code = c as u32;
    if c.is_ascii_uppercase() || ((0xC0..=0xDE).contains(&code) && code != 0xD7) {
        // Safe by construction: both ranges map into valid scalar values.
        char::from_u32(code + 0x20).unwrap_or(c)
    } else {
        c
    }
}

/// The **host** path a distribution-tree destination is written at (ART-160).
///
/// A distribution tree is an Amiga volume held in a host folder, so its file
/// names are AmigaDOS names — and AmigaDOS allows names a Windows filesystem
/// does not. `Storage/DOSDrivers/AUX` is on the owner's own AmigaOS 3.9 disc,
/// and `AUX` is one of the 22 device names Windows has reserved since DOS;
/// `Prices: 1993` is a legal AmigaDOS filename that NTFS refuses outright.
/// Every other "copy out" path in ART already escapes those through
/// [`windows_safe_name`](crate::core::volume::write::copy::windows_safe_name),
/// and this module wrote straight through under whatever name the media
/// carried. It happens to work on Windows 11 Pro 26200 for the reserved
/// names (measured while closing ART-155, plain path and `\\?\` path alike)
/// and it is one build of one Windows on one filesystem.
///
/// # Two questions, in this order, never merged into one
///
/// The same shape `commands/volume_write.rs::folder_destination` already
/// uses, and for the same reason:
///
/// 1. **Containment**, of the *raw* destination. `windows_safe_name` turns
///    `..\..\Startup` into `_.._..Startup` — a name that passes containment
///    trivially — so asking containment afterwards would be asking a question
///    whose answer had already been changed.
/// 2. **Host legality**, of the escaped path: what the filesystem will
///    actually accept.
///
/// # Segment by segment, because a destination is a path
///
/// `windows_safe_name` escapes `/` to `_`, so handing it a whole destination
/// would flatten `Storage/DOSDrivers/AUX` into one filename. Each segment is
/// escaped on its own and the `/`s are put back.
///
/// # The Amiga name is not lost
///
/// A renamed file's *AmigaDOS* name stays in `distribution.json`
/// ([`apply::FileRecord::path`](apply::FileRecord::path) is always the Amiga
/// path, never the host one), and the host path it actually landed at is
/// recorded beside it in [`apply::FileRecord::host_path`](apply::FileRecord::host_path).
/// That pair is what `core/preload` reads back so the *Amiga* name — not the
/// escaped host one — is what reaches the finished volume. Escaping without
/// recording would put `_AUX` on the card and make `verify_volume`, which
/// looks the manifest's `path` up on the volume, fail a file that is really
/// there under the wrong name.
pub fn host_destination(root: &std::path::Path, to: &str) -> CoreResult<std::path::PathBuf> {
    let refuse = |err: crate::core::security::PathTraversalError| {
        CoreError::SafetyRefused(format!(
            "'{to}' does not stay inside the distribution root: {err}"
        ))
    };
    crate::core::security::safe_join(root, to).map_err(refuse)?;
    crate::core::security::safe_join(root, &host_relative(to)).map_err(refuse)
}

/// The escaped, `/`-separated form of `to` — see [`host_destination`].
///
/// Equal to `to` for every destination a host filesystem can carry verbatim,
/// which is the overwhelming majority of a real install tree.
pub fn host_relative(to: &str) -> String {
    to.split('/')
        .map(crate::core::volume::write::copy::windows_safe_name)
        .collect::<Vec<_>>()
        .join("/")
}

/// Two AmigaDOS destinations that would land on the **same host file** —
/// ART-160's own corollary, and a silent data loss until it was found.
///
/// [`host_relative`] is **many-to-one**. `windows_safe_name` maps every
/// character a filesystem refuses onto the single replacement `_`, and
/// prefixes a reserved device name with one, so a medium holding two
/// genuinely different names can escape both onto one:
///
/// | on the medium | on the host |
/// |---|---|
/// | `Devs/Prices: 1993` | `Devs/Prices_ 1993` |
/// | `Devs/Prices? 1993` | `Devs/Prices_ 1993` |
/// | `Storage/DOSDrivers/AUX` | `Storage/DOSDrivers/_AUX` |
/// | `Storage/DOSDrivers/_AUX` | `Storage/DOSDrivers/_AUX` |
///
/// `apply` writes items in order and `atomic_write` replaces, so the second
/// of a colliding pair **silently overwrote the first** and the tree ended up
/// holding one file where the medium had two. Worse, `distribution.json`
/// recorded both, each claiming the same `hostPath` — so `core/preload` would
/// then copy that one file onto the volume under whichever AmigaDOS name the
/// map resolved, renaming a genuine `_AUX` to `AUX`.
///
/// Escaping exists to protect a name the host cannot store, never to merge
/// two names into one. There is no correct silent answer here — renaming
/// further would invent a name no medium carried — so a collision is
/// **refused, by name, before a single byte is written**, exactly the way an
/// undeclared overwrite already is.
///
/// Returns the colliding pairs as `(host path, first destination, second
/// destination)`, in plan order, so a refusal can name what actually clashed
/// rather than only counting. Destinations that are the *same* place
/// ([`same_destination`] — the medium spelled one file two ways) are not a
/// collision: that is the ordinary overwrite `apply` already records through
/// `FileRecord::overwrote`.
///
/// # Drawers are checked too, and that is a reversal
///
/// An earlier version skipped directory items, on the reasoning that
/// "directories are merge points, not claims" — the rule `detect_collisions`
/// and `undeclared_overwrites` genuinely do follow. It does not transfer.
/// Those two ask whether *the same drawer* is claimed twice, which is
/// ordinary; this asks whether **two different drawers become one**, which is
/// the same loss as two files becoming one and is worse in reach: every file
/// under `Storage/AUX` and every file under `Storage/_AUX` would land in one
/// host drawer, and `core/preload` resolves that drawer to a single AmigaDOS
/// name — so one whole subtree arrives on the volume under the other's name.
///
/// Two components creating *the same* drawer is still fine, and stays fine
/// for the same reason a file does: [`same_destination`] answers that, not
/// the `is_dir` flag. So no flag is needed and none is taken.
pub fn host_name_collisions(destinations: &[String]) -> Vec<(String, String, String)> {
    use std::collections::BTreeMap;

    // Keyed on `destination_key`, **not** on the host path as spelled. The
    // first version keyed the map exact-case while comparing with
    // `same_destination` (which folds ASCII case), so a pair differing only
    // in case never met in the map at all: `Devs/Prices: 1993` claimed
    // `Devs/Prices_ 1993` and `Devs/prices? 1993` claimed `Devs/prices_ 1993`,
    // two different keys for one file on a case-insensitive filesystem.
    // `apply` then returned `Ok`, wrote one file and recorded two — the exact
    // loss this function exists to stop, one fold narrower.
    //
    // `destination_key`'s own doc comment says a `BTreeMap` keyed on a raw
    // destination is the same defect in a quieter form. That warning has now
    // been earned four times in this round; this is the last place that was
    // still ignoring it.
    let mut claimed: BTreeMap<String, String> = BTreeMap::new();
    let mut clashes = Vec::new();
    for to in destinations {
        let host = host_relative(to);
        let key = destination_key(&host);
        match claimed.get(&key) {
            // Two destinations that are the *same place* are not a collision
            // — the media spelled one file two ways (`C/ASSIGN` off the 3.9
            // disc's Primary tree, `C/Assign` off a BoingBag), which is the
            // ordinary overwrite `FileRecord::overwrote` already records.
            Some(first) if !same_destination(first, to) => {
                clashes.push((host, first.clone(), to.clone()));
            }
            Some(_) => {}
            None => {
                claimed.insert(key, to.clone());
            }
        }
    }
    clashes
}

/// Why ART cannot place a package's files from the host at all — a
/// property of the *package*, not of the user's folder or their selection.
///
/// A value, never a sentence (ART-060), and a **closed enum on purpose**:
/// the checklist and the refusal both `match` on it, so a second kind of
/// block is a compile error at every place that has to say something about
/// it rather than a silent "unavailable, no reason given".
///
/// This exists because of ART-166, and because §10/§89 say ART must not
/// offer what it cannot do. Both shipped BoingBag recipes name a payload
/// (`member: "AmigaOS-Update"`) that is a **password-encrypted ZIP** — 233
/// of 233 entries in BoingBag 3.9-1 and 147 of 147 in 3.9-2, measured three
/// independent ways. The password belongs to the BoingBag's own `Updater`,
/// an Amiga executable that has to run on an Amiga; every established
/// distribution builder installs a BoingBag by running exactly that inside
/// an emulator rather than by decrypting anything, and the owner's recorded
/// decision is that ART will not bypass it. So the fact is not "this
/// archive is missing" and not "this failed while running" — it is a fixed,
/// knowable property of the package, and the screen has to say it *before*
/// the tick, in the user's own language, rather than let a raw ZIP error
/// arrive after they confirmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HostPlacementBlock {
    /// The package's payload archive is encrypted, and only the package's
    /// own Amiga-side `Updater` holds the password (ART-166).
    EncryptedPayload,
}

/// Why an install cannot proceed. A value, never a sentence — the UI
/// translates it (ART-060).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "refusal", rename_all = "kebab-case")]
pub enum RefusalReason {
    /// No image in the folder carries this volume name.
    MediaMissing {
        component: String,
        volume_name: String,
    },
    /// The media is here and the path the recipe expects is not — so the
    /// recipe is wrong about *this* media, probably a different revision.
    /// Skipping it silently would give a system missing a library.
    MediaPathMissing {
        component: String,
        media: String,
        path: String,
    },
    /// The media carrying this component's volume name was found in the
    /// folder and could not be *opened* — a truncated ADF, a disc image
    /// deleted between the scan and the plan, a file the reader no longer
    /// recognises.
    ///
    /// **A refusal, not a `CoreError` (ART-119 #5).** `plan()` used to
    /// propagate `open_media`'s error with `?`, which failed the whole plan
    /// over one damaged disk: every other component's file list vanished,
    /// and the OS Builder — which requests *two* plans through one
    /// `Promise.all`, one of them deliberately with nothing excluded —
    /// blanked both, including the plan the user had explicitly excluded
    /// that component from. One unreadable disk is exactly what
    /// [`RefusalReason::MediaMissing`] and
    /// [`RefusalReason::MediaPathMissing`] already treat as a per-component
    /// fact, and this is the third face of the same fact: the disk is
    /// there, it is not usable, and the other twenty-five components are
    /// unaffected. It still blocks the install (`osinstallBlocker` reads
    /// `refusals`) — it just no longer takes the screen with it.
    ///
    /// `reason` carries the reader's own sentence, because "which disk, and
    /// what is wrong with it" is the user's next question and the file name
    /// alone does not answer it. English, like every `CoreError` `Display`
    /// (ART-060), and shown after the translated sentence rather than
    /// instead of it.
    MediaUnreadable {
        component: String,
        volume_name: String,
        path: String,
        reason: String,
    },
    /// The ROM was not identified, so a `Condition` cannot be decided.
    RomUnknown,
    /// Two components claim one destination and neither declared an override.
    DestinationCollision {
        path: String,
        components: Vec<String>,
    },
    /// More than one file in the media folder carries this component's
    /// volume name. `scan::find_media`/`scan::media_for` report every
    /// match rather than guessing at one, so a folder with a stray backup
    /// copy of a disk nobody selected still installs everything else — this
    /// only fires for the component that actually names the ambiguous
    /// volume, at plan time, never as a whole-folder scan failure. `paths`
    /// carries every file that claimed the name, because the user's next
    /// question is always "which two files?".
    ///
    /// `String`, not `PathBuf` — every other path-carrying field in this
    /// enum already is (`MediaPathMissing::path`), and `RefusalReason`
    /// derives `Serialize`: `serde`'s `PathBuf` implementation errors on a
    /// non-UTF-8 path, which would mean a refusal built to explain a
    /// problem could itself fail to cross the command boundary on Windows.
    /// Rendered with `.display().to_string()`, the same conversion
    /// `MediaPathMissing` already uses.
    MediaAmbiguous {
        component: String,
        volume_name: String,
        paths: Vec<String>,
    },
    /// Two or more components sharing an `exclusive_group` are both
    /// switched on at once — `plan()` checks this against the **resolved**
    /// set (`InstallPlan::components_on`), not the request, because a
    /// condition-satisfied component can be switched on without ever being
    /// chosen. `components` carries every conflicting id, because the
    /// user's next question is always "which two?" — the same reasoning
    /// `MediaAmbiguous::paths` already applies to files.
    ExclusiveGroupConflict {
        group: String,
        components: Vec<String>,
    },
    /// A rule's `kind` disagrees with what the media actually holds at
    /// `from` — a `File` rule resolving to a directory, or a `Subtree` rule
    /// resolving to a file. Recipes are **data**: this whole design's
    /// promise is that a future release (AmigaOS 3.9, CaffeineOS) arrives
    /// as a new JSON file, not new code, and `recipe.rs`'s own `validate`
    /// cannot catch this — it has no media to resolve a path against. This
    /// is the only place a wrong `kind` can be caught, which is why it is
    /// refused rather than silently emitted with the wrong shape (a `File`
    /// rule over a directory would otherwise carry `is_dir: true` and
    /// escape `detect_collisions`, which only looks at files; a `Subtree`
    /// rule over a file would otherwise carry `bytes: 0` and be silently
    /// short). `expected` is what the rule declared; `found` is what the
    /// media actually holds — a recipe author's next question is always
    /// "which rule?".
    RuleKindMismatch {
        component: String,
        from: String,
        expected: RuleKind,
        found: RuleKind,
    },
    /// A chosen package id ART ships no recipe for — a selection saved
    /// against an older build, most likely. Refused rather than skipped:
    /// silently installing one fewer package than the user ticked is the
    /// same class of quiet wrongness `MediaMissing` exists to prevent.
    PackageUnknown { package: String },
    /// Packages were chosen and `InstallRequest::package_folder` is `None`.
    /// Names every package that was asked for, because the folder is the
    /// one thing the user has to supply to make any of them resolvable —
    /// and the media folder cannot stand in for it: the owner keeps discs
    /// in `Amigatolon\iso` and archives in `Amigatolon\paketler`.
    PackageFolderMissing { packages: Vec<String> },
    /// A package needs another package that was not itself chosen. Pulling
    /// it in silently would install something the user never asked for
    /// (`package::order`'s own rule, surfaced as a typed refusal rather
    /// than its English sentence — ART-060).
    PackageRequirementMissing { package: String, requires: String },
    /// A package needs a **recipe component** that is not switched on —
    /// `locale-turkish` without `locale-base`, which lands thirty-six
    /// catalogs into a `Locale/Catalogs` drawer nothing can open (ART-162
    /// arriving through the selection instead of through the recipe).
    /// `requires` cannot express this: it relates packages to packages, and
    /// a component is not a package.
    PackageComponentMissing { package: String, component: String },
    /// No archive in the package folder carries this package's own
    /// top-level directory name. The package counterpart of
    /// [`RefusalReason::MediaMissing`], and separate from it because the
    /// folder it names is a different folder.
    PackageArchiveMissing { package: String, media: String },
    /// More than one archive in the package folder claims this package's
    /// top-level directory name — `BoingBag39-2.lha` sitting beside its
    /// eight language variants is the real case. Every claimant is carried,
    /// for the reason [`RefusalReason::MediaAmbiguous`] already gives:
    /// the user's next question is always "which two files?".
    PackageArchiveAmbiguous {
        package: String,
        media: String,
        paths: Vec<String>,
    },
    /// A chosen package ART ships a recipe for but **cannot place from the
    /// host at all** — see [`HostPlacementBlock`]. Refused from the
    /// selection itself, before any archive is opened, so the sentence the
    /// user reads names what the package needs rather than reporting
    /// whatever the reader happened to fail on first (ART-166: that was a
    /// raw, English `Password required to decrypt file` arriving *after*
    /// the confirmation).
    PackageNotPlaceableOnHost {
        package: String,
        block: HostPlacementBlock,
    },
}

// ---------------------------------------------------------------------------
// Shared test fixtures (plan doc "Shared test fixtures" section).
//
// Grown one task at a time rather than landing whole here: Task 1 added
// `scratch`, `media`, `workbench`, `CancelAfter`, `digest_of_folder` and
// `fake_rom`. Task 5 adds `entries_for` and `planned_with`, now that
// `InstallRequest` and `plan()` exist to build them against. `rdb_image` and
// `partition_offset` reference `core/card/build.rs`, which is still a later
// task, so they are not written yet.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod fold_tests {
    use super::{amiga_names_equal, destination_key, fold_amiga_case};

    /// **Fix round 1, F1.** The fold is the one an *international* AmigaDOS
    /// volume applies, and every pair below is a name that exists in the
    /// owner's own material — the AmigaOS 3.9 disc's Primary tree spells its
    /// language drawers in upper case (`TÜRKÇE`, `FRANÇAIS`), the language
    /// packs spell them in lower (`türkçe`, `français`).
    #[test]
    fn the_fold_covers_the_latin1_accented_range_not_only_ascii() {
        for (upper, lower) in [
            ("T\u{DC}RK\u{C7}E", "t\u{FC}rk\u{E7}e"),
            ("FRAN\u{C7}AIS", "fran\u{E7}ais"),
            ("PORTUGU\u{CA}S", "portugu\u{EA}s"),
            ("ESPA\u{D1}OL", "espa\u{F1}ol"),
            ("POLSKI", "polski"),
        ] {
            assert!(
                amiga_names_equal(upper, lower),
                "{upper} and {lower} are one AmigaDOS name"
            );
            assert_eq!(fold_amiga_case(upper), fold_amiga_case(lower));
        }
    }

    /// The two Latin-1 characters that have no case partner, and the one
    /// that is not a letter at all — folding any of them would invent a
    /// pairing AmigaDOS's own table does not have.
    #[test]
    fn characters_with_no_case_partner_are_left_alone() {
        // 0xD7 multiplication sign vs 0xF7 division sign: different
        // characters, and 0xD7 must not fold onto 0xF7.
        assert!(!amiga_names_equal("\u{D7}", "\u{F7}"));
        // 0xDF eszett and 0xFF y-diaeresis have no Latin-1 upper/lower pair.
        assert_eq!(fold_amiga_case("\u{DF}"), "\u{DF}");
        assert_eq!(fold_amiga_case("\u{FF}"), "\u{FF}");
    }

    /// Folding must not merge names that are genuinely different, or the
    /// refusals every caller of this depends on stop meaning anything.
    #[test]
    fn different_names_stay_different() {
        assert!(!amiga_names_equal(
            "portugu\u{EA}s",
            "portugu\u{EA}s-brasil"
        ));
        assert!(!amiga_names_equal("t\u{FC}rk\u{E7}e", "deutsch"));
        assert!(!amiga_names_equal("Libs", "Lib"));
    }

    /// `destination_key` is the collision map's key, and it goes through the
    /// same fold — the base disc writes `Locale/Catalogs/TÜRKÇE/x.catalog`
    /// and the Turkish pack writes `Locale/Catalogs/türkçe/x.catalog`, which
    /// under the old ASCII-only fold were two files.
    #[test]
    fn a_destination_the_disc_and_a_package_spell_differently_is_one_place() {
        let from_disc = "Locale/Catalogs/T\u{DC}RK\u{C7}E/workbench.catalog";
        let from_pack = "Locale/Catalogs/t\u{FC}rk\u{E7}e/workbench.catalog";
        assert_eq!(destination_key(from_disc), destination_key(from_pack));
        assert!(super::same_destination(from_disc, from_pack));
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use std::path::{Path, PathBuf};

    use crate::core::adf::create::create_blank_adf;
    use crate::core::adf::FileSystemType;
    use crate::core::jobs::ProgressSink;
    use crate::core::volume::device::FileRegionMut;
    use crate::core::volume::write::{FileMeta, VolumeWriter};
    use crate::core::volume::{DosType, VolumeGeometry};

    /// A fresh, empty directory for one call, named after `tag` and
    /// [`crate::core::test_scratch_id`].
    ///
    /// **Per call, not per tag** (ART-173's sweep). This used to key on tag
    /// plus process id alone, so two tests sharing a tag — or one test
    /// calling it twice — got the *same* directory, and Cargo runs tests in
    /// parallel threads of one process. `apply.rs`'s `planned()` already had
    /// to append its own counter to the tag to work around exactly that. The
    /// counter is inside the helper now, so no call site has to know.
    ///
    /// The repository's own convention (`core/archive/extract.rs::scratch`,
    /// `core/layout/apply.rs::scratch`) — deliberately not `tempfile`, which
    /// is not a dependency of this project.
    pub fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-osinstall-{tag}-{}",
            crate::core::test_scratch_id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Split a `/`-separated path into its directory segments and file name.
    fn split_path(path: &str) -> (Vec<&str>, &str) {
        let mut segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let file_name = segments.pop().expect("entry path must not be empty");
        (segments, file_name)
    }

    /// Write `entries` into the blank volume at `path`, creating whatever
    /// directories each entry's path needs one level at a time — the volume
    /// writer has no `mkdir -p`.
    fn write_entries(path: &Path, entries: &[(&str, &[u8], u32)]) {
        let geometry = VolumeGeometry::floppy_dd(DosType::new(*b"DOS\x01"));
        let mut device =
            FileRegionMut::open(path, 0, geometry.total_bytes(), geometry.block_size).unwrap();
        let mut writer = VolumeWriter::open(&mut device, geometry, path, 0).unwrap();

        for (entry_path, bytes, protection) in entries {
            let (dirs, file_name) = split_path(entry_path);
            let mut parent = 0u32;
            for dir_name in dirs {
                parent = match writer.find(parent, dir_name).unwrap() {
                    Some(existing) => existing.block,
                    None => writer.make_dir(parent, dir_name).unwrap().block.unwrap(),
                };
            }
            writer
                .add_file(
                    parent,
                    file_name,
                    bytes,
                    FileMeta {
                        protection: Some(*protection),
                        date: None,
                    },
                )
                .unwrap();
        }
    }

    /// A synthetic install disk. **ART ships no Amiga content** — this builds
    /// one, now, in a tempdir.
    ///
    /// `entries` is `(path, bytes, protection)`. Protection is `HSPARWED` with
    /// `RWED` inverted, so `0x20` is `--p-rwed` and `0x42` is `-s--rw-d`.
    pub fn media(
        dir: &Path,
        volume: &str,
        filename: &str,
        entries: &[(&str, &[u8], u32)],
    ) -> PathBuf {
        let path = dir.join(filename);
        std::fs::write(
            &path,
            create_blank_adf(volume, FileSystemType::Ffs, false).unwrap(),
        )
        .unwrap();
        write_entries(&path, entries);
        path
    }

    /// A synthetic install disc — one file is enough for `find_media` to
    /// open it and read its volume name back. Built the same way
    /// `source_cd`'s own fixtures are (`core::iso::fixture::IsoBuilder`,
    /// Joliet on so `volume` round-trips through the long-name tree exactly
    /// as typed, matching a real AmigaOS 3.9 disc).
    pub fn write_test_iso(dir: &Path, filename: &str, volume: &str) -> PathBuf {
        use crate::core::iso::fixture::{file, IsoBuilder};

        let bytes = IsoBuilder {
            volume: volume.to_string(),
            joliet_volume: volume.to_string(),
            joliet: true,
            children: vec![file("README.;1", "readme.txt", b"install disc")],
            ..Default::default()
        }
        .build();
        let path = dir.join(filename);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    /// `Workbench3.2` with the two files every test in this plan leans on.
    pub fn workbench(dir: &Path) -> PathBuf {
        media(
            dir,
            "Workbench3.2",
            "wb.adf",
            &[
                ("C/LoadModule", b"cmd", 0x20),            // --p-rwed
                ("S/Startup-sequence", b"; test\n", 0x42), // -s--rw-d
            ],
        )
    }

    /// Stops the job after `n` units, so a cancel path can be tested without
    /// timing.
    pub struct CancelAfter {
        limit: u64,
        seen: std::sync::atomic::AtomicU64,
    }

    impl CancelAfter {
        pub fn new(limit: u64) -> Self {
            Self {
                limit,
                seen: std::sync::atomic::AtomicU64::new(0),
            }
        }
    }

    impl ProgressSink for CancelAfter {
        fn report(&self, _done: u64, _total: Option<u64>, _message: &str) {
            self.seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        fn is_cancelled(&self) -> bool {
            self.seen.load(std::sync::atomic::Ordering::SeqCst) >= self.limit
        }
    }

    /// Every file and directory under `root`, in no particular order —
    /// `digest_of_folder` does its own sorting, over the *relative* keys it
    /// actually hashes. Does not swallow an unreadable directory: a scratch
    /// tree this code just created should always be readable, and silently
    /// skipping one would make "unchanged" pass over data that was never
    /// examined.
    fn walk_paths(root: &Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let read = std::fs::read_dir(&dir)
                .unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()));
            for entry in read {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path.clone());
                }
                found.push(path);
            }
        }
        found
    }

    /// `path`, relative to `root`, as a `/`-joined key — never the absolute
    /// path, and never the platform's own separator. Two identical trees
    /// rooted at different scratch directories must hash the same, which an
    /// absolute-path key would break by construction, and a bare
    /// `Path::to_string_lossy` would break on the one platform this project
    /// ships for, where the separator is `\`.
    fn relative_key(root: &Path, path: &Path) -> String {
        path.strip_prefix(root)
            .unwrap()
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/")
    }

    /// One hash over a whole folder, so "unchanged" is a single assertion,
    /// and so that the same tree copied to a different scratch directory
    /// still compares equal — the whole point of hashing a *copy* against
    /// its *original* (Task 6's proof). Sorted by the relative key it
    /// hashes, so it does not depend on directory read order.
    pub fn digest_of_folder(root: &Path) -> String {
        use sha2::{Digest, Sha256};
        let mut entries: Vec<(String, PathBuf)> = walk_paths(root)
            .into_iter()
            .map(|path| (relative_key(root, &path), path))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));

        let mut hasher = Sha256::new();
        for (key, path) in &entries {
            hasher.update(key.as_bytes());
            if path.is_file() {
                hasher.update(std::fs::read(path).unwrap());
            }
        }
        format!("{:x}", hasher.finalize())
    }

    /// A ROM file that states `major` in its own header — which is what
    /// `core::rom::stated_version` reads, and what the Modules condition asks.
    /// Deliberately *not* a real dump: ART ships none, and the condition does
    /// not consult `KNOWN_ROMS` anyway (ART-104).
    pub fn fake_rom(dir: &Path, major: u16) -> PathBuf {
        let path = dir.join(format!("kick-{major}.rom"));
        let mut bytes = vec![0u8; 512 * 1024];
        bytes[12..14].copy_from_slice(&major.to_be_bytes());
        bytes[14..16].copy_from_slice(&68u16.to_be_bytes());
        std::fs::write(&path, &bytes).unwrap();
        path
    }

    /// Every entry a component's own rules need present so `plan()` finds
    /// nothing missing: one placeholder file inside the drawer each
    /// `Subtree` rule names, and the literal file each `File` rule names.
    /// Built from the recipe actually passed in, not hand-copied from the
    /// JSON — so a rule added to `amigaos-3.2.json` later is automatically
    /// covered here too, and a test that wants *broken* media (Task 5's
    /// `plan_where_extras_has_no_l`) starts from this and removes the one
    /// entry it means to break, rather than drifting from what the shipped
    /// recipe actually asks for.
    pub fn entries_for(recipe: &super::Recipe, volume: &str) -> Vec<(String, Vec<u8>, u32)> {
        let mut entries = Vec::new();
        for component in recipe.components.iter().filter(|c| c.media == volume) {
            for rule in &component.rules {
                match rule.kind {
                    super::RuleKind::File => entries.push((rule.from.clone(), b"data".to_vec(), 0)),
                    super::RuleKind::Subtree if !rule.from.is_empty() => {
                        entries.push((format!("{}/placeholder", rule.from), b"data".to_vec(), 0));
                    }
                    // `from: ""` means the media's own root (`fonts`,
                    // `backdrops`) — `AdfSource::entry("")` always resolves
                    // to the root itself, so no placeholder is needed to
                    // make that rule satisfiable.
                    super::RuleKind::Subtree => {}
                }
            }
        }
        entries
    }

    /// A media folder plus a plan over it, so a test states only what it
    /// varies. `present` lists the volume names to create, each built with
    /// exactly the content its own component(s) in the shipped recipe need
    /// (via [`entries_for`]) — a test naming `"Workbench3.2"` gets media
    /// that satisfies every one of `workbench-base`'s rules, so nothing
    /// trips `MediaPathMissing` by accident. A fresh scratch directory every
    /// call (an atomic counter, not the caller's own tag) — this is called
    /// from many different tests, several of which run in parallel threads
    /// of the same process, and a shared tag would let two calls race over
    /// the same directory.
    /// Write the media of every **required** component the caller has not
    /// written itself.
    ///
    /// A required component's disk is a precondition of any plan at all, not
    /// something a test chooses: a fixture naming `["Extras3.2"]` is stating
    /// its own subject, not claiming the system disks are absent. This was
    /// invisible until ART-127, when `workbench-base` stopped being the only
    /// required component — every fixture happened to pass that one by hand,
    /// and eight of them broke at once the moment a second appeared. Adding a
    /// third required component should break nothing.
    pub fn required_media(folder: &Path, recipe: &super::Recipe, already: &[&str]) {
        for component in &recipe.components {
            if !component.required || already.contains(&component.media.as_str()) {
                continue;
            }
            let owned = entries_for(recipe, &component.media);
            let refs: Vec<(&str, &[u8], u32)> = owned
                .iter()
                .map(|(path, bytes, protection)| (path.as_str(), bytes.as_slice(), *protection))
                .collect();
            media(
                folder,
                &component.media,
                &format!("{}.adf", component.media),
                &refs,
            );
        }
    }

    pub fn planned_with(
        chosen: &[&str],
        present: &[&str],
        rom_major: Option<u16>,
    ) -> (crate::core::osinstall::plan::InstallPlan, PathBuf) {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = scratch(&format!("planned-with-{n}"));
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();

        let recipe = crate::core::osinstall::recipe::amigaos_32().unwrap();

        for volume in present {
            let owned = entries_for(&recipe, volume);
            let refs: Vec<(&str, &[u8], u32)> = owned
                .iter()
                .map(|(path, bytes, protection)| (path.as_str(), bytes.as_slice(), *protection))
                .collect();
            media(&folder, volume, &format!("{volume}.adf"), &refs);
        }
        required_media(&folder, &recipe, present);

        let rom = rom_major.map(|major| fake_rom(&dir, major));
        let request = crate::core::osinstall::plan::InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder,
            rom,
            chosen: chosen.iter().map(|s| s.to_string()).collect(),
            excluded: Vec::new(),
            destination: dir.join("dist"),
        };

        let plan = crate::core::osinstall::plan::plan(&request, &recipe).unwrap();
        (plan, dir)
    }

    // -----------------------------------------------------------------
    // Task 6: one base release and one package that lands on it.
    //
    // Shared between `plan.rs` (which resolves and orders packages) and
    // `apply.rs` (which places them, and proves both entry points agree),
    // for the reason every fixture in this module exists: a second copy of
    // a fixture builder is how two tests start disagreeing about what a
    // tree looks like.
    //
    // Deliberately **not** the shipped recipe or the shipped packages. The
    // property under test is "a package overwrote a base file and the two
    // entry points agree about the result", and that needs a base file, a
    // package file over it, a file only the base has and a file only the
    // package has — four facts stated in twenty lines here, rather than
    // inferred from a 26-component recipe and a 210-file BoingBag nobody
    // may redistribute.
    // -----------------------------------------------------------------

    /// The base file both sides write, and the one thing that makes
    /// `producing_with_a_package_equals_adding_it_afterwards` prove
    /// anything: a package that only *added* files would pass it while
    /// saying nothing about the case this whole round is about.
    pub const OVERWRITTEN_PATH: &str = "C/LoadModule";

    /// A one-component release: everything in `C` off a floppy called
    /// `TestBase`.
    pub fn package_test_recipe() -> super::Recipe {
        super::Recipe {
            release: "Test OS".to_string(),
            components: vec![super::Component {
                id: "base-c".to_string(),
                media: "TestBase".to_string(),
                rules: vec![super::PathRule {
                    from: "C".to_string(),
                    to: "C".to_string(),
                    kind: super::RuleKind::Subtree,
                }],
                required: true,
                condition: None,
                overrides: Vec::new(),
                user_startup: Vec::new(),
                exclusive_group: None,
                available: true,
            }],
        }
    }

    /// A package over that release, shaped exactly like a shipped one: one
    /// `Subtree` rule, and an `overrides` naming the component whose files
    /// it lands on — without which `plan::detect_collisions` refuses the
    /// combination, which is the correct answer for an *undeclared*
    /// overwrite and the wrong one here.
    pub fn package_test_package() -> super::package::Package {
        let component = super::Component {
            id: "test-package".to_string(),
            media: "TestPack".to_string(),
            rules: vec![super::PathRule {
                from: "C".to_string(),
                to: "C".to_string(),
                kind: super::RuleKind::Subtree,
            }],
            required: false,
            condition: None,
            overrides: vec!["base-c".to_string()],
            user_startup: Vec::new(),
            exclusive_group: None,
            available: true,
        };
        super::package::Package {
            id: "test-package".to_string(),
            name: "Test package".to_string(),
            media: "TestPack".to_string(),
            member: None,
            distinguished_by: None,
            amiga_installer: None,
            requires: Vec::new(),
            requires_components: Vec::new(),
            host_placement_block: None,
            component,
        }
    }

    /// The `TestBase` floppy: the file the package overwrites, plus one the
    /// package never touches (so a test can see the base survive).
    pub fn package_test_media(folder: &Path) -> PathBuf {
        media(
            folder,
            "TestBase",
            "base.adf",
            &[
                (OVERWRITTEN_PATH, b"base LoadModule", 0x20),
                ("C/OnlyBase", b"base only", 0x00),
            ],
        )
    }

    /// The `TestPack` archive: the same destination with different bytes,
    /// plus a file only the package brings.
    ///
    /// **No directory entries at all, deliberately.** The owner's real
    /// `BoingBag39-1.lha` payload declares 23 directories and not one of
    /// them is top-level, while every rule in both shipped BoingBag recipes
    /// names a top-level drawer — so an archive whose `C` is only *implied*
    /// by the paths under it is the realistic case, not the awkward one.
    /// An earlier draft of this fixture wrote an explicit `TestPack/C/` row
    /// and passed while the engine would have refused every real package;
    /// `ArchiveSource::with_implicit_directories` is what makes this
    /// version resolve, and this version is what proves it.
    pub fn package_test_archive(folder: &Path, file_name: &str) -> PathBuf {
        let path = folder.join(file_name);
        std::fs::write(
            &path,
            crate::core::archive::zip::tests::make_zip_with(&[
                ("TestPack/C/LoadModule", b"package LoadModule" as &[u8]),
                ("TestPack/C/OnlyPack", b"package only"),
            ]),
        )
        .unwrap();
        path
    }

    /// A **second** package, over the first — the shape a real user meets:
    /// BoingBag 3.9-1, then 3.9-2. It `requires` the first (so `order`
    /// sequences them) and overrides both the base component and the first
    /// package, because it lands on `C/LoadModule` for the third time.
    ///
    /// This exists because spec §2's equivalence is
    /// `produce(base + A + B) == add(produce(base + A), B)`, and the
    /// one-package form never adds onto a tree that already holds a package
    /// — which is precisely where Add's own rules have to agree with
    /// Produce's about a file two things already claimed.
    pub fn package_test_package_two() -> super::package::Package {
        let component = super::Component {
            id: "test-package-two".to_string(),
            media: "TestPack2".to_string(),
            rules: vec![super::PathRule {
                from: "C".to_string(),
                to: "C".to_string(),
                kind: super::RuleKind::Subtree,
            }],
            required: false,
            condition: None,
            overrides: vec!["base-c".to_string(), "test-package".to_string()],
            user_startup: Vec::new(),
            exclusive_group: None,
            available: true,
        };
        super::package::Package {
            id: "test-package-two".to_string(),
            name: "Test package two".to_string(),
            media: "TestPack2".to_string(),
            member: None,
            distinguished_by: None,
            amiga_installer: None,
            requires: vec!["test-package".to_string()],
            requires_components: Vec::new(),
            host_placement_block: None,
            component,
        }
    }

    /// The `TestPack2` archive. No directory entries, for the reason
    /// [`package_test_archive`] gives.
    pub fn package_test_archive_two(folder: &Path, file_name: &str) -> PathBuf {
        let path = folder.join(file_name);
        std::fs::write(
            &path,
            crate::core::archive::zip::tests::make_zip_with(&[
                (
                    "TestPack2/C/LoadModule",
                    b"second package LoadModule" as &[u8],
                ),
                ("TestPack2/C/OnlyPack2", b"second package only"),
            ]),
        )
        .unwrap();
        path
    }

    /// Tasks 2 through 10 build their evidence on these helpers, so the
    /// helpers get their own coverage rather than trusting a one-off
    /// exercise that was run once by hand and then deleted. `entries_for`
    /// and `planned_with` are exercised indirectly, by every `plan_tests`
    /// test that calls them — a dedicated test here would only restate the
    /// shipped recipe's own shape.
    #[cfg(test)]
    mod tests {
        use super::*;

        /// The contract this used to assert was the *opposite* one — that a
        /// tag plus the pid is a **stable** path, cleared on entry. That is
        /// precisely the ART-164 hazard: two parallel tests sharing a tag
        /// share the directory, and whichever writes second hands the other
        /// its fixture. `scratch` now gives every call its own, so the thing
        /// worth pinning is that two calls cannot collide.
        #[test]
        fn scratch_gives_every_call_its_own_empty_directory() {
            let dir = scratch("fixture-scratch");
            assert!(dir.is_dir());
            assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);

            std::fs::write(dir.join("leftover"), b"from another test").unwrap();

            let dir_again = scratch("fixture-scratch");
            assert_ne!(
                dir, dir_again,
                "the same tag twice must not name the same directory"
            );
            assert_eq!(
                std::fs::read_dir(&dir_again).unwrap().count(),
                0,
                "and the new one is empty, whatever the first now holds"
            );
            // The first is untouched: a fresh directory is not a cleared one,
            // and a test still holding a path must keep what it wrote.
            assert!(dir.join("leftover").is_file());
        }

        /// The protection byte is the one thing a test cannot see just by
        /// opening the file with ordinary I/O — it lives in the AmigaDOS
        /// header block, so this proves the volume writer really stored what
        /// `media()` was asked to store, not just that the bytes exist.
        #[test]
        fn media_writes_the_protection_bits_it_was_asked_for() {
            let dir = scratch("media-protection");
            let image = media(
                &dir,
                "Test",
                "t.adf",
                &[
                    ("C/LoadModule", b"cmd", 0x20),
                    ("S/Startup-sequence", b"; test\n", 0x42),
                ],
            );

            let parsed =
                crate::core::adf::AdfImage::from_bytes(std::fs::read(&image).unwrap()).unwrap();
            let root = parsed.list_root().unwrap();

            let c_dir = root.iter().find(|e| e.name == "C").unwrap();
            let load_module = parsed
                .list_dir(c_dir.header_block)
                .unwrap()
                .into_iter()
                .find(|e| e.name == "LoadModule")
                .unwrap();
            assert_eq!(load_module.attrs, "--p-rwed");

            let s_dir = root.iter().find(|e| e.name == "S").unwrap();
            let startup = parsed
                .list_dir(s_dir.header_block)
                .unwrap()
                .into_iter()
                .find(|e| e.name == "Startup-sequence")
                .unwrap();
            assert_eq!(startup.attrs, "-s--rw-d");
        }

        #[test]
        fn workbench_carries_the_two_files_every_test_in_this_plan_leans_on() {
            let dir = scratch("workbench-fixture");
            let image = workbench(&dir);

            let parsed =
                crate::core::adf::AdfImage::from_bytes(std::fs::read(&image).unwrap()).unwrap();
            let root = parsed.list_root().unwrap();
            let c_dir = root.iter().find(|e| e.name == "C").unwrap();
            let s_dir = root.iter().find(|e| e.name == "S").unwrap();

            assert!(parsed
                .list_dir(c_dir.header_block)
                .unwrap()
                .iter()
                .any(|e| e.name == "LoadModule"));
            assert!(parsed
                .list_dir(s_dir.header_block)
                .unwrap()
                .iter()
                .any(|e| e.name == "Startup-sequence"));
        }

        #[test]
        fn fake_rom_states_the_major_it_was_asked_for() {
            let dir = scratch("fake-rom");
            let rom = fake_rom(&dir, 45);
            let bytes = std::fs::read(&rom).unwrap();
            assert_eq!(crate::core::rom::stated_version(&bytes), Some((45, 68)));
        }

        #[test]
        fn cancel_after_flips_on_the_nth_report_and_not_before() {
            let sink = CancelAfter::new(3);
            assert!(!sink.is_cancelled());
            sink.report(0, None, "one");
            sink.report(0, None, "two");
            assert!(
                !sink.is_cancelled(),
                "the third report is the one that trips it"
            );
            sink.report(0, None, "three");
            assert!(sink.is_cancelled());
        }

        /// The property Task 6 actually leans on: two copies of the same
        /// tree, rooted at two different scratch directories, must digest
        /// identically — and a single changed byte must not.
        #[test]
        fn digest_of_folder_does_not_depend_on_where_the_tree_is_rooted() {
            let dir = scratch("digest-portable");
            let left = dir.join("left");
            let right = dir.join("right");
            std::fs::create_dir_all(left.join("sub")).unwrap();
            std::fs::create_dir_all(right.join("sub")).unwrap();
            std::fs::write(left.join("sub").join("a.txt"), b"hello").unwrap();
            std::fs::write(right.join("sub").join("a.txt"), b"hello").unwrap();

            assert_eq!(
                digest_of_folder(&left),
                digest_of_folder(&right),
                "identical trees under different roots must digest the same"
            );

            std::fs::write(right.join("sub").join("a.txt"), b"hello!").unwrap();
            assert_ne!(
                digest_of_folder(&left),
                digest_of_folder(&right),
                "a single changed byte must change the digest"
            );
        }
    }
}
