//! Installing a WHDLoad pack onto a hard disk — the layout half (spec §82).
//!
//! ```text
//! Game.lha → DROP → WHDLoad detected → Install to HDF → Backup → Apply → Verify
//! ```
//!
//! Everything in that line except "Install to HDF" already existed. This module
//! is the piece that made the one click possible: working out **what inside the
//! archive is the game**, and **what it is called on the Amiga**.
//!
//! ## Why that is not obvious
//!
//! A WHDLoad archive almost always wraps the pack in a drawer:
//!
//! ```text
//! Turrican.lha
//!  ├── Turrican/            ← this is the pack
//!  │    ├── Turrican.slave
//!  │    ├── Turrican
//!  │    └── data/
//!  └── Turrican.info        ← and this is its icon, OUTSIDE the drawer
//! ```
//!
//! Copying the archive's contents verbatim into `Games/` would produce
//! `Games/Turrican/…` — right — but naive code that copies "everything" also
//! drops the readme in, and code that copies "the folder" leaves `Turrican.info`
//! behind. Without that icon the drawer exists and is **invisible on
//! Workbench**, which is indistinguishable from the install having failed.
//!
//! Some archives have no wrapper and the slave sits at the root. Then the
//! archive itself is the pack and its name has to come from somewhere else.
//!
//! ## Launch configuration from an icon's ToolTypes
//!
//! WHDLoad games installed as drawers carry an icon whose ToolTypes name the
//! slave and options. The manual calls the key `Slave`; real icons say `SLAVE=`
//! — upper case — so the comparison is case-insensitive.
//!
//! A real WHDLoad drawer icon holds 98 ToolTypes, and 94 of them are `IM1=`/`IM2=`
//! NewIcon image data, following a line reading `*** DON'T EDIT THE FOLLOWING
//! LINES!! ***`. Read naively, a "launch configuration" is mostly a picture.
//! Both the marker and the image keys are filtered.
//!
//! ## What this module will not do
//!
//! It does not run the pack's `Install` script. A script is an Amiga executable
//! and ART is not an Amiga; running one would mean either an emulator or a
//! guess. So ART installs an **already-installed pack** — which is what the
//! archives on Aminet's `game/whd` actually contain — and says so when it finds
//! an installer instead.

use serde::{Deserialize, Serialize};

use crate::core::error::{CoreError, CoreResult};

/// How deep ART looks for a slave.
///
/// A pack is a drawer with a slave in it, occasionally one level down. Past
/// that, a "slave" is more likely to be a save file or an example than the
/// thing to install — and a search with no bound is a search that finds
/// something eventually.
pub const MAX_SLAVE_DEPTH: usize = 3;

/// The most entries ART will analyse.
pub const MAX_ENTRIES: usize = 20_000;

/// One thing found in the unpacked archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Path relative to the unpack root, `/` separated.
    pub relative: String,
    pub is_dir: bool,
}

impl Entry {
    pub fn file(relative: &str) -> Self {
        Self {
            relative: relative.into(),
            is_dir: false,
        }
    }

    pub fn dir(relative: &str) -> Self {
        Self {
            relative: relative.into(),
            is_dir: true,
        }
    }
}

/// Where the pack is, what it is called, and what is not part of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackLayout {
    /// The drawer that *is* the pack, relative to the unpack root.
    /// Empty when the archive root is the pack.
    pub root: String,
    /// What the drawer will be called on the Amiga.
    pub name: String,
    /// The slave, relative to the unpack root.
    pub slave: String,
    /// The pack's own icon, relative to the unpack root.
    ///
    /// It sits **beside** the drawer, not inside it, and it is what makes the
    /// game visible on Workbench. `None` is not an error — plenty of archives
    /// ship without one — but it is worth telling the user about.
    pub icon: Option<String>,
    /// Files in the archive that are not part of the pack, with a plain reason.
    ///
    /// Usually a readme at the archive root. Listed rather than dropped
    /// silently: a user who expected the readme on the disk should be able to
    /// see that ART left it behind on purpose.
    pub outside: Vec<String>,
    /// True when the archive holds an `Install` script — a pack that has *not*
    /// been installed yet.
    pub needs_installer: bool,
}

impl PackLayout {
    /// The drawer's name with `.info` on the end, as it must land on the Amiga.
    pub fn icon_name(&self) -> String {
        format!("{}.info", self.name)
    }
}

/// Work out the layout of an unpacked WHDLoad archive.
///
/// Pure: `entries` is whatever walked the unpack directory. Nothing here reads
/// a file, so the whole decision is testable against a list of names.
pub fn analyse(entries: &[Entry]) -> CoreResult<PackLayout> {
    if entries.len() > MAX_ENTRIES {
        return Err(CoreError::InvalidInput(format!(
            "this archive holds {} entries, more than ART will analyse",
            entries.len()
        )));
    }

    let slave = pick_slave(entries).ok_or_else(|| {
        CoreError::InvalidInput(
            "no .slave file was found, so this is not a WHDLoad pack ART can install".into(),
        )
    })?;

    let root = parent_of(&slave).to_string();
    let name = if root.is_empty() {
        // No wrapper drawer: the slave's own stem is the best name available.
        // `Turrican.slave` → `Turrican`, which is the convention anyway.
        stem_of(last_component(&slave)).to_string()
    } else {
        last_component(&root).to_string()
    };

    let icon = find_icon(entries, &root, &name);

    let outside = entries
        .iter()
        .filter(|entry| !entry.is_dir)
        .filter(|entry| !is_inside(&entry.relative, &root))
        .filter(|entry| Some(&entry.relative) != icon.as_ref())
        .map(|entry| entry.relative.clone())
        .collect();

    // An `Install` script means the archive is a *source* pack, not an
    // installed one. ART cannot run it, so it says so rather than copying a
    // directory of raw disk images and calling it a game.
    let needs_installer = entries.iter().any(|entry| {
        !entry.is_dir && last_component(&entry.relative).eq_ignore_ascii_case("Install")
    });

    Ok(PackLayout {
        root,
        name,
        slave,
        icon,
        outside,
        needs_installer,
    })
}

/// The slave to install, when there are several.
///
/// Packs ship variants — `Game.slave`, `GameNTSC.slave`, `GameCD32.slave`. The
/// shallowest wins, and among equals the shortest name, which is the plain one
/// rather than a variant. ART installs the whole drawer either way, so this
/// only decides *where the pack root is*, not which slave gets run.
fn pick_slave(entries: &[Entry]) -> Option<String> {
    entries
        .iter()
        .filter(|entry| !entry.is_dir)
        .filter(|entry| has_extension(&entry.relative, "slave"))
        .filter(|entry| depth_of(&entry.relative) <= MAX_SLAVE_DEPTH)
        .min_by(|a, b| {
            depth_of(&a.relative)
                .cmp(&depth_of(&b.relative))
                .then_with(|| a.relative.len().cmp(&b.relative.len()))
                .then_with(|| a.relative.cmp(&b.relative))
        })
        .map(|entry| entry.relative.clone())
}

/// The pack's icon: `<root>.info`, beside the drawer rather than inside it.
///
/// Falls back to `<name>.info` at the archive root, which is where it lives
/// when the archive has no wrapper drawer at all.
fn find_icon(entries: &[Entry], root: &str, name: &str) -> Option<String> {
    let wanted = if root.is_empty() {
        format!("{name}.info")
    } else {
        format!("{root}.info")
    };
    let wanted_lower = wanted.to_lowercase();

    entries
        .iter()
        .filter(|entry| !entry.is_dir)
        .find(|entry| entry.relative.to_lowercase() == wanted_lower)
        .map(|entry| entry.relative.clone())
}

/// Whether `path` is `root` itself or lives under it.
///
/// An empty root means the whole archive, so everything is inside it.
fn is_inside(path: &str, root: &str) -> bool {
    if root.is_empty() {
        return true;
    }
    path == root || path.starts_with(&format!("{root}/"))
}

fn depth_of(path: &str) -> usize {
    path.matches('/').count()
}

fn parent_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(slash) => &path[..slash],
        None => "",
    }
}

fn last_component(path: &str) -> &str {
    match path.rfind('/') {
        Some(slash) => &path[slash + 1..],
        None => path,
    }
}

fn stem_of(name: &str) -> &str {
    match name.rfind('.') {
        Some(dot) if dot > 0 => &name[..dot],
        _ => name,
    }
}

/// Whether `path`'s last component ends in `.<extension>`, case-insensitively.
///
/// **Public because there must be exactly one of these.** `Lotus3.islave` is an
/// installer's slave and not the game's, and the only thing keeping it out is
/// that this compares the whole extension rather than looking for a substring.
/// `core/gameindex` asks the same question of a file inside a hardfile, and a
/// second copy of this rule is a second place for `.islave` to slip through.
pub fn has_extension(path: &str, extension: &str) -> bool {
    last_component(path)
        .rsplit('.')
        .next()
        .is_some_and(|found| found.eq_ignore_ascii_case(extension))
        && last_component(path).contains('.')
}

// ---------------------------------------------------------------------------

/// The marker WHDLoad's own installer writes before a NewIcon's image data.
const DONT_EDIT: &str = "DON'T EDIT";

/// What an install's icon says about how it expects to start.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchOptions {
    /// The `Slave` ToolType's value, verbatim. `None` when the icon does not
    /// name one — WHDLoad then defaults to `WHDLoad.Slave`, and `Slave=*`
    /// searches the drawer, but choosing between those is the caller's.
    pub slave: Option<String>,
    /// Every other real setting, in the icon's own order: `PRELOAD`, `NTSC`,
    /// `QuitKey=…` and the rest.
    pub options: Vec<String>,
}

pub fn launch_options(tooltypes: &[String]) -> LaunchOptions {
    let mut out = LaunchOptions::default();
    for entry in tooltypes {
        if entry.contains(DONT_EDIT) {
            break;
        }
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = trimmed.split('=').next().unwrap_or("").trim();
        if key.eq_ignore_ascii_case("IM1") || key.eq_ignore_ascii_case("IM2") {
            continue;
        }
        if key.eq_ignore_ascii_case("SLAVE") {
            if let Some((_, value)) = trimmed.split_once('=') {
                out.slave = Some(value.trim().to_string());
            }
            continue;
        }
        out.options.push(trimmed.to_string());
    }
    out
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape almost every WHDLoad archive on Aminet has.
    fn wrapped() -> Vec<Entry> {
        vec![
            Entry::dir("Turrican"),
            Entry::file("Turrican/Turrican.slave"),
            Entry::file("Turrican/Turrican"),
            Entry::dir("Turrican/data"),
            Entry::file("Turrican/data/level1.bin"),
            Entry::file("Turrican.info"),
            Entry::file("Turrican.readme"),
        ]
    }

    #[test]
    fn the_drawer_holding_the_slave_is_the_pack() {
        let layout = analyse(&wrapped()).unwrap();

        assert_eq!(layout.root, "Turrican");
        assert_eq!(layout.name, "Turrican");
        assert_eq!(layout.slave, "Turrican/Turrican.slave");
    }

    /// The icon lives *beside* the drawer. Copying only the drawer leaves it
    /// behind, and the game is then invisible on Workbench — which looks
    /// exactly like the install having failed.
    #[test]
    fn the_packs_icon_is_found_outside_the_drawer() {
        let layout = analyse(&wrapped()).unwrap();

        assert_eq!(layout.icon.as_deref(), Some("Turrican.info"));
        assert_eq!(layout.icon_name(), "Turrican.info");
    }

    /// A readme at the archive root is not part of the game. Listed, not
    /// silently dropped: a user who expected it on the disk should see that
    /// ART left it out on purpose.
    #[test]
    fn files_outside_the_pack_are_listed_with_the_icon_excluded() {
        let layout = analyse(&wrapped()).unwrap();

        assert_eq!(layout.outside, vec!["Turrican.readme"]);
        assert!(
            !layout.outside.contains(&"Turrican.info".to_string()),
            "the icon is part of the install, not something left behind"
        );
    }

    /// Some archives have no wrapper. Then the archive *is* the pack, and its
    /// name has to come from the slave.
    #[test]
    fn an_archive_with_no_wrapper_drawer_takes_its_name_from_the_slave() {
        let entries = vec![
            Entry::file("Lemmings.slave"),
            Entry::file("Lemmings"),
            Entry::dir("data"),
            Entry::file("data/levels.dat"),
        ];

        let layout = analyse(&entries).unwrap();
        assert_eq!(layout.root, "", "the archive root is the pack");
        assert_eq!(layout.name, "Lemmings");
        assert!(
            layout.outside.is_empty(),
            "with no wrapper, everything is inside the pack"
        );
    }

    #[test]
    fn an_unwrapped_archives_icon_is_found_at_its_root() {
        let entries = vec![Entry::file("Lemmings.slave"), Entry::file("Lemmings.info")];

        let layout = analyse(&entries).unwrap();
        assert_eq!(layout.icon.as_deref(), Some("Lemmings.info"));
        assert!(layout.outside.is_empty());
    }

    /// Packs ship NTSC and CD32 variants. The plain one is the pack root's
    /// evidence; ART installs the whole drawer either way.
    #[test]
    fn the_plainest_slave_decides_the_pack_root() {
        let entries = vec![
            Entry::dir("Game"),
            Entry::file("Game/GameNTSC.slave"),
            Entry::file("Game/Game.slave"),
            Entry::file("Game/GameCD32.slave"),
        ];

        let layout = analyse(&entries).unwrap();
        assert_eq!(layout.slave, "Game/Game.slave");
        assert_eq!(layout.root, "Game");
    }

    /// A slave nested deeper than the pack drawer must not drag the root down
    /// with it — the shallowest one wins.
    #[test]
    fn a_shallower_slave_wins_over_a_deeper_one() {
        let entries = vec![
            Entry::dir("Game"),
            Entry::file("Game/Game.slave"),
            Entry::dir("Game/Saves"),
            Entry::file("Game/Saves/Old.slave"),
        ];

        let layout = analyse(&entries).unwrap();
        assert_eq!(layout.root, "Game");
        assert_eq!(layout.slave, "Game/Game.slave");
    }

    #[test]
    fn case_does_not_matter_for_the_slave_or_the_icon() {
        let entries = vec![
            Entry::dir("Game"),
            Entry::file("Game/Game.SLAVE"),
            Entry::file("Game.INFO"),
        ];

        let layout = analyse(&entries).unwrap();
        assert_eq!(layout.slave, "Game/Game.SLAVE");
        assert_eq!(layout.icon.as_deref(), Some("Game.INFO"));
    }

    /// ART is not an Amiga and cannot run an Install script. Copying a source
    /// pack's raw disk images into a drawer and calling it a game would be a
    /// failure the user only discovers when it does not start.
    #[test]
    fn an_archive_with_an_install_script_is_flagged() {
        let entries = vec![
            Entry::dir("Game"),
            Entry::file("Game/Game.slave"),
            Entry::file("Game/Install"),
        ];

        let layout = analyse(&entries).unwrap();
        assert!(layout.needs_installer);
    }

    #[test]
    fn an_already_installed_pack_is_not_flagged() {
        assert!(!analyse(&wrapped()).unwrap().needs_installer);
    }

    /// Without a slave there is nothing to install, and saying so beats
    /// copying an arbitrary folder onto someone's hard disk.
    #[test]
    fn an_archive_with_no_slave_is_refused_with_the_reason() {
        let entries = vec![Entry::dir("Docs"), Entry::file("Docs/readme.txt")];

        let err = analyse(&entries).unwrap_err();
        assert!(err.to_string().contains("no .slave"), "{err}");
    }

    /// A "slave" buried six levels down is a save file or an example, not the
    /// thing to install.
    #[test]
    fn a_slave_buried_too_deep_does_not_count() {
        let entries = vec![Entry::file("a/b/c/d/e/Deep.slave")];

        assert!(analyse(&entries).is_err());
    }

    #[test]
    fn a_missing_icon_is_reported_rather_than_being_an_error() {
        let entries = vec![Entry::dir("Game"), Entry::file("Game/Game.slave")];

        let layout = analyse(&entries).unwrap();
        assert_eq!(layout.icon, None);
        assert_eq!(layout.icon_name(), "Game.info");
    }

    /// A file whose name merely ends in the letters "slave" is not a slave.
    #[test]
    fn a_name_ending_in_the_letters_slave_is_not_a_slave_file() {
        let entries = vec![Entry::file("Game/enslave"), Entry::file("Game/Real.slave")];

        let layout = analyse(&entries).unwrap();
        assert_eq!(layout.slave, "Game/Real.slave");
    }

    #[test]
    fn a_directory_named_like_a_slave_is_not_one() {
        let entries = vec![Entry::dir("Game.slave"), Entry::file("Game/Real.slave")];

        assert_eq!(analyse(&entries).unwrap().slave, "Game/Real.slave");
    }

    #[test]
    fn an_archive_with_too_many_entries_is_refused_before_the_analysis() {
        let entries: Vec<Entry> = (0..MAX_ENTRIES + 1)
            .map(|i| Entry::file(&format!("f{i}")))
            .collect();

        let err = analyse(&entries).unwrap_err();
        assert!(
            err.to_string().contains("more than ART will analyse"),
            "{err}"
        );
    }

    /// Everything under the pack drawer belongs to the pack, however deep.
    #[test]
    fn nested_content_counts_as_part_of_the_pack() {
        let entries = vec![
            Entry::dir("Game"),
            Entry::file("Game/Game.slave"),
            Entry::dir("Game/data"),
            Entry::dir("Game/data/music"),
            Entry::file("Game/data/music/title.mod"),
            Entry::file("Elsewhere.txt"),
        ];

        let layout = analyse(&entries).unwrap();
        assert_eq!(layout.outside, vec!["Elsewhere.txt"]);
    }

    /// A drawer whose name merely *starts* with the pack's name is a different
    /// drawer. `GameData/` is not inside `Game/`.
    #[test]
    fn a_sibling_with_a_matching_prefix_is_not_inside_the_pack() {
        let entries = vec![
            Entry::dir("Game"),
            Entry::file("Game/Game.slave"),
            Entry::dir("GameData"),
            Entry::file("GameData/extra.bin"),
        ];

        let layout = analyse(&entries).unwrap();
        assert_eq!(layout.outside, vec!["GameData/extra.bin"]);
    }

    // launch_options tests

    #[test]
    fn the_slave_comes_from_the_tooltype_whatever_its_case() {
        let tt = vec![
            "SLAVE=1001StolenIdeas.Slave".to_string(),
            "PRELOAD".to_string(),
        ];
        let got = launch_options(&tt);
        assert_eq!(got.slave.as_deref(), Some("1001StolenIdeas.Slave"));
        assert_eq!(got.options, vec!["PRELOAD".to_string()]);

        let lower = vec!["Slave=Turrican.slave".to_string()];
        assert_eq!(
            launch_options(&lower).slave.as_deref(),
            Some("Turrican.slave")
        );
    }

    #[test]
    fn a_newicon_picture_is_not_a_launch_configuration() {
        // The real icon's shape: two real settings, the marker, then 94 image
        // lines. Reading past the marker turns a configuration into a picture.
        let mut tt = vec![
            "SLAVE=Tag.Slave".to_string(),
            "PRELOAD".to_string(),
            " ".to_string(),
            "*** DON'T EDIT THE FOLLOWING LINES!! ***".to_string(),
        ];
        for _ in 0..94 {
            tt.push("IM1=a\"$(0@a\"$(0@a\"$(0@".to_string());
        }
        let got = launch_options(&tt);
        assert_eq!(got.slave.as_deref(), Some("Tag.Slave"));
        assert_eq!(
            got.options,
            vec!["PRELOAD".to_string()],
            "everything from the marker on is image data, and the blank line is not a setting"
        );
    }

    #[test]
    fn image_keys_are_dropped_even_without_the_marker() {
        // Belt and braces: an icon written by a different tool may carry the
        // image keys without the sentence in front of them.
        let tt = vec!["IM2=abc".to_string(), "SLAVE=X.slave".to_string()];
        let got = launch_options(&tt);
        assert_eq!(got.slave.as_deref(), Some("X.slave"));
        assert!(got.options.is_empty());
    }

    #[test]
    fn an_icon_with_no_slave_tooltype_states_none() {
        // WHDLoad's own default is `WHDLoad.Slave` and `Slave=*` searches the
        // drawer - both are decisions for the caller, not guesses to make here.
        assert_eq!(launch_options(&["PRELOAD".to_string()]).slave, None);
    }
}
