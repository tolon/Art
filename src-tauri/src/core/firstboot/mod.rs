//! The Amiga finishes what the host cannot start.
//!
//! ART writes a small set of AmigaDOS scripts into a distribution tree. They
//! run **once**, on the machine the card was built for, dispatched from
//! `S:User-Startup` — after the release's own `Startup-Sequence` has run
//! `SetPatch`, `Mount`, `BindDrivers`, `AddDataTypes` and `IPrefs`, which is
//! why the hook is there and not after `BindDrivers` (spec §1.2: both of the
//! owner's real sequences were read; the other project pays for the earlier
//! position with three patches, one of them the ART-193 line).
//!
//! Design: `docs/superpowers/specs/2026-09-07-firstboot-design.md`.
//!
//! ## What is in the tree afterwards
//!
//! ```text
//! S/User-Startup                 ;BEGIN art-firstboot … ;END art-firstboot
//! S/ART-FirstBoot                the dispatcher
//! S/ART-FirstBoot-Step           runs one step and writes its two report lines
//! S/FirstBoot/NN-name            one file per step; deleted when it succeeds
//! S/FirstBoot/90-prefs           the wizard; only when asked (phase 3)
//! Prefs/Env-Archive/ART_Set_Input, ART_Set_ScreenMode   what ART set; 90-prefs skips it
//! S/FirstBoot.log                the report, one line per event
//! Prefs/Env-Archive/ART_FirstBoot  TRUE until the first run begins
//! Storage/DOSDrivers/SD0pi3, SD0pi4   the two mountlists 10-hardware picks from
//! ```
//!
//! Variable and file names carry an `ART_` prefix and no `/` — an AmigaDOS
//! `$name` substitution does not reach into an `ENV:` subdirectory, so the
//! spec's `ENV:ART/…` spelling became `ENV:ART_…` here.
//!
//! ## Layering
//!
//! This module reads `core::osinstall`'s recipe types and calls
//! `core::osinstall::startup::merge_user_startup`. **`core::osinstall` must
//! never import this module** — joining a build to a plan is
//! `commands/firstboot.rs`'s job, the same rule as `core::artwork` and
//! `core::gameindex`.

use serde::Serialize;

pub mod cardread;
pub mod plan;
pub mod report;
pub mod scripts;
pub mod write;

/// The `;BEGIN`/`;END` marker id inside `S:User-Startup`.
pub const COMPONENT: &str = "art-firstboot";
/// Tree-relative paths, `/`-separated, as `distribution.json` spells them.
pub const DISPATCHER_PATH: &str = "S/ART-FirstBoot";
pub const STEP_WRAPPER_PATH: &str = "S/ART-FirstBoot-Step";
pub const STEP_DIR: &str = "S/FirstBoot";
pub const REPORT_PATH: &str = "S/FirstBoot.log";
/// The report's name on the FAT boot partition, where Windows can show it.
pub const FAT_REPORT_NAME: &str = "art-firstboot.log";
pub const FLAG_PATH: &str = "Prefs/Env-Archive/ART_FirstBoot";
/// The wizard's step name — its file name in `S:FirstBoot/`, and the name
/// the report uses (phase 3 design §3.1).
pub const WIZARD_STEP: &str = "90-prefs";
pub const WIZARD_PATH: &str = "S/FirstBoot/90-prefs";

/// A Preferences window the wizard may open, in the order `90-prefs` asks
/// them (phase 3 design §3.2). Wire: `locale`, `input`, `screen-mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WizardWindow {
    Locale,
    Input,
    ScreenMode,
}

impl WizardWindow {
    pub const ALL: [WizardWindow; 3] = [Self::Locale, Self::Input, Self::ScreenMode];

    /// The editor's own name in `SYS:Prefs/` — also the word `90-prefs`
    /// writes into its `detail` lines.
    pub fn amiga_name(self) -> &'static str {
        match self {
            Self::Locale => "Locale",
            Self::Input => "Input",
            Self::ScreenMode => "ScreenMode",
        }
    }
}

/// A window `90-prefs` opens in the foreground, so the whole boot waits in it
/// (experiment 3: 4 of 4). ScreenMode runs detached and is not one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ForegroundWindow {
    Locale,
    Input,
}

impl ForegroundWindow {
    pub fn amiga_name(self) -> &'static str {
        match self {
            Self::Locale => "Locale",
            Self::Input => "Input",
        }
    }
}

/// `core::amigainstall::workvol::FAIL_AT`'s reason, verbatim (ART-188).
pub const FAIL_AT: i64 = 2_000_000_000;

/// The four lines merged into `S:User-Startup`. Nothing else ever goes there.
pub fn user_startup_lines() -> Vec<String> {
    vec![
        "IF EXISTS S:ART-FirstBoot".to_string(),
        "  Execute S:ART-FirstBoot".to_string(),
        "ENDIF".to_string(),
    ]
}
