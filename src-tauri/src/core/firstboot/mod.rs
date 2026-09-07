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
