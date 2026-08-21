//! What a catalogued title needs before anything starts. (SD-wave-c)
//!
//! **A decision, not an action.** Given a machine, a set of candidate ROMs
//! and what the title physically is, this module works out which machine,
//! which Kickstart and which media a WinUAE launch would need — and refuses
//! rather than guessing when the answer is not there. It does three things
//! it will never do, on purpose:
//!
//! - **It starts nothing.** No WinUAE process, no config file — that is a
//!   later task's job, once this module has decided what the config should
//!   say.
//! - **It reads no files.** Every path arrives as a `String` from the
//!   caller. Whether `system_volume` or a floppy image actually exists on
//!   disk is the command layer's question, answered before or after this
//!   module runs, never inside it.
//! - **It does not know `core::rom::RomInfo`.** [`LaunchRom`] is this
//!   module's own record, carrying only the two facts the decision reads —
//!   which models a ROM suits, and where it lives. `core/rom/pairing.rs`
//!   made this exact call for the same reason: a lower-level module that
//!   takes a higher-level module's type can no longer be read, or
//!   extracted, without dragging that module in behind it. `commands/
//!   launch.rs` maps `RomInfo` to `LaunchRom` in a later task; that
//!   translation belongs at the command layer, not here.

pub mod extract;
pub mod whdload_boot;

use serde::{Deserialize, Serialize};

use crate::core::profile::WHDLOAD_PROFILE_FAST_RAM_MB;

/// The two facts about a candidate Kickstart this module's decision reads.
///
/// Mirrors `core::rom::RomInfo` without depending on it — see the module
/// header. Mapped from the real type by the caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchRom {
    pub name: String,
    /// Which Amiga models this ROM suits, e.g. `["A500", "A2000"]`.
    pub models: Vec<String>,
    pub path: String,
    /// Mirrors `core::rom::RomInfo::major` — see that field's doc comment
    /// for what "major" means and why it is not an identifier. `None` when
    /// the ROM states no numeric version ART could read (ART-148): an AROS
    /// replacement or an encrypted Amiga Forever dump ART has no key for.
    /// [`WHDLOAD_MIN_KICKSTART_MAJOR`] is the one thing this module reads it
    /// for, so a ROM with no known major can never satisfy that floor —
    /// deliberately: ART does not guess a ROM new enough for WHDLoad out of
    /// one it could not identify.
    pub major: Option<u16>,
}

/// Which Amiga a title runs on. Only the two chipset tiers ART's catalogue
/// distinguishes when choosing a machine to launch on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Machine {
    A500,
    A1200,
}

impl Machine {
    /// The model name as a [`LaunchRom`]'s `models` list states it.
    fn model_name(self) -> &'static str {
        match self {
            Machine::A500 => "A500",
            Machine::A1200 => "A1200",
        }
    }
}

/// What was decided ART should mount, once a machine and a ROM are settled.
///
/// The planned counterpart of [`RequestKind`]: `Floppies` has already been
/// trimmed to what WinUAE can actually take, and `Whdload` now carries the
/// `system` volume the request only promised.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum LaunchKind {
    /// At most [`MAX_FLOPPY_DRIVES`] images, in the order they mount.
    Floppies { images: Vec<String> },
    /// A hardfile that is the title itself.
    Hardfile { image: String },
    /// A WHDLoad drawer, mounted alongside the user's own bootable system.
    Whdload {
        drawer: String,
        slave: String,
        /// The user's own bootable system volume — never ART's; ART owns
        /// none. See [`LaunchRequest::system_volume`].
        system: String,
        /// Y2 by default; `false` runs the slave under Y1 instead.
        one_click: bool,
    },
}

/// What a title physically needs to mount, before ART knows whether the
/// pieces are actually there. The caller's half of [`LaunchKind`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RequestKind {
    Floppies {
        images: Vec<String>,
    },
    /// `whdload: true` for a self-booting WHDLoad hardfile
    /// (`Media::WhdloadHardfile` — `commands/launch.rs::request_kind_from`),
    /// `false` for a hardfile that is a plain AmigaOS installation
    /// (`Media::Hardfile`, or an `.rp9`'s own `<harddrive>`). This is the
    /// one bit `plan_for` needs to know whether [`WHDLOAD_MIN_KICKSTART_MAJOR`]
    /// applies: WHDLoad's own stated minimum is a fact about *that*
    /// software, not about hardfiles in general, and a hand-installed
    /// AmigaOS hardfile may need — and legitimately run on — a Kickstart
    /// older than WHDLoad ever supported. Getting this backwards in either
    /// direction is wrong: raising the floor for every hardfile would refuse
    /// old-OS installs that work today; never raising it is ART-148, the bug
    /// this field exists to fix.
    Hardfile {
        image: String,
        whdload: bool,
    },
    Whdload {
        drawer: String,
        slave: String,
    },
}

/// The finished decision: what to launch, and anything the user should be
/// told about it before it runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchPlan {
    pub machine: Machine,
    pub rom: LaunchRom,
    pub kind: LaunchKind,
    /// Never silent about a limitation (spec §89) — populated instead of
    /// leaving the user to notice a title only partly loaded.
    pub notes: Vec<LaunchNote>,
}

/// Something true about the plan that is not wrong, but that the user
/// should see before it runs. Never a refusal — the plan still runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum LaunchNote {
    /// WinUAE has `floppy0`..`floppy3` and nothing past it. Mounting the
    /// first [`MAX_FLOPPY_DRIVES`] and saying so is the whole fix; claiming
    /// a larger set loads in full is what spec §89 forbids.
    MoreDisksThanDrives { total: usize, mounted: usize },
}

/// Why a plan could not be made. A black screen is never an acceptable
/// substitute for one of these.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum LaunchRefusal {
    /// None of the candidate ROMs suit the chosen machine.
    NoSuitableRom { machine: Machine },
    /// The chosen machine has a suitable ROM for its model, but nothing new
    /// enough to meet WHDLoad's own stated minimum
    /// ([`WHDLOAD_MIN_KICKSTART_MAJOR`]) — ART-148. A Kickstart 1.x machine
    /// cannot run WHDLoad at all (whdload.de's requirements page:
    /// <https://www.whdload.de/docs/en/need.html>), and a bare `DOS\1` FFS
    /// hardfile with no RDB has no filesystem for a 1.x ROM to mount it
    /// with either way — the "not a DOS disk" a user sees is this refusal,
    /// arrived at the hard way instead of stated up front. Distinct from
    /// [`LaunchRefusal::NoSuitableRom`] so the message can name the actual
    /// requirement instead of leaving the user to guess why an otherwise
    /// working-looking ROM was not picked.
    NoRomMeetsWhdloadMinimum { machine: Machine },
    /// The ROM folder holds no Kickstart for the machine a WHDLoad launch
    /// runs on **at all** — ART-152's follow-up, and a distinct fact from
    /// [`LaunchRefusal::NoRomMeetsWhdloadMinimum`] above.
    ///
    /// **Why this had to be split off.** Since ART-152 every WHDLoad launch
    /// plans [`WHDLOAD_PROFILE_MACHINE`], and `best_rom` only considers a ROM
    /// whose `models` list names that machine. A **Kickstart 3.1 rev 40.63
    /// (A500/A600/A2000)** is a perfectly good, perfectly modern ROM that
    /// names none of them — so a user whose only 3.x dump is that one now
    /// gets refused on every WHDLoad title. That refusal is correct: a 40.63
    /// A500 ROM is not an A1200, and launching it would produce something
    /// broken. What was *wrong* was reporting it as
    /// `NoRomMeetsWhdloadMinimum`, whose whole sentence is "your Kickstart is
    /// too old" — a user holding Kickstart **3.1** reads that and goes
    /// looking for a version that does not exist. The two cases need two
    /// sentences, so they need two variants: this one says *which machine's*
    /// Kickstart to add, that one says *how new* it must be, and each is
    /// raised only when it is the true one.
    NoWhdloadMachineRom { machine: Machine },
    /// A WHDLoad drawer needs a system to boot into, and ART owns none.
    NoSystemVolume,
    /// A path the plan needs turned out not to exist, once the command
    /// layer checked. Never raised by this module itself — it reads no
    /// files — but part of the same refusal type so callers have one place
    /// to render either kind of "cannot launch this".
    FileMissing { path: String },
    /// A floppy set with nothing in it — `Media::Floppies { ordered: [] }`
    /// is constructible (an `.rp9` manifest naming neither a hardfile nor a
    /// floppy, `core::gameindex::scan::from_rp9`) and would otherwise plan
    /// and launch WinUAE with no disk mounted at all: exactly the
    /// insert-disk hand the design (§4.2) says ART refuses to produce,
    /// rather than a black screen with no explanation.
    NothingToMount,
}

/// What a title asks for in the catalogue's own terms.
///
/// Deliberately not `core::gameindex::ChipsetRequirement` — same reason as
/// [`LaunchRom`]: this module reads its own three-way answer (`Ocs`, `Ecs`,
/// `Aga`) rather than depending on the catalogue's two-way one, and the
/// command layer maps between them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Chipset {
    Ocs,
    Ecs,
    Aga,
}

/// What to launch and how, before ART has checked that any of it exists on
/// disk or picked a ROM that actually suits it.
pub struct LaunchRequest<'a> {
    pub machine: Machine,
    pub roms: &'a [LaunchRom],
    pub kind: RequestKind,
    /// The user's own bootable system, for a WHDLoad drawer. `None` is a
    /// refusal, not a guess.
    pub system_volume: Option<String>,
    /// Y2 by default; `false` is the user asking for Y1.
    pub one_click: bool,
}

/// WinUAE names its floppy drives `floppy0` through `floppy3` — four, never
/// more. A set with more disks mounts the first four; the rest is a note,
/// not a silent drop.
pub const MAX_FLOPPY_DRIVES: usize = 4;

/// WHDLoad's own stated minimum — "Kickstart 2.0 (version 37)" — from its
/// requirements page: <https://www.whdload.de/docs/en/need.html>. Applied as
/// a floor on the **booted machine's** ROM ([`LaunchRom::major`]), never as a
/// hardcoded machine choice: an A500 with a 3.1 ROM meets it exactly as well
/// as an A1200 does (ART-148).
///
/// **This is not the same number a WHDLoad slave names.** A slave like
/// `Turrican.slave` carries its own declared Kickstart — `kick34005.A500`,
/// say — in `SlaveFacts::kickstart` / `Media::WhdloadHardfile`'s catalogued
/// `KickstartNeed`, and that name is the ROM image *WHDLoad itself* loads
/// from `DEVS:Kickstarts` for the game, on a machine that is **already
/// running something modern**. It is not what the machine should boot, and
/// this floor must never be read off it — that would put a 1.3-shaped
/// requirement back into a check built to rule 1.3 out. `machine_for` and
/// `plan_for` read only the catalogue's `Chipset` and the ROM's own stated
/// major; neither ever looks at a slave's declared Kickstart.
pub const WHDLOAD_MIN_KICKSTART_MAJOR: u16 = 37;

/// Fast RAM, in MB, ART adds to a WHDLoad launch's profile — ART-151:
/// `1000 Miglia` reached WHDLoad on a stock `AmigaProfile::a500_ocs`
/// (`chip_kb: 512, slow_kb: 512, fast_mb: 0`, exactly 1 MB and none of it
/// Fast) and WHDLoad itself refused with "DOS-Error #103 (not enough memory
/// available) on loading 1000Miglia.Slave".
///
/// **Why this is a default and not a floor `plan_for` enforces.** WHDLoad's
/// own requirements page (<https://www.whdload.de/docs/en/need.html>) states
/// its minimum as "a minimum of 1.0 MiB RAM (sometimes more, it depends on
/// the installed program)" — a floor `plan_for` already meets exactly
/// (§WHDLOAD_MIN_KICKSTART_MAJOR is a Kickstart floor, not a memory one; a
/// stock A500's 1 MB total is what the DOS-Error above measured against).
/// "Sometimes more" is a fact about each game, not a number WHDLoad states —
/// this constant is ART's own headroom on top of that floor, not a second
/// reading of it.
///
/// **Why Fast RAM specifically.** WHDLoad's own autodoc
/// (<https://www.whdload.de/docs/autodoc.html>, Overview) states that
/// `BaseMem` — the memory the installed program itself runs in — "is always
/// Chip-memory", while `ExpMem`, the one optional extra a slave may request,
/// "may be Chip- or Fast-memory dependently on what is available". Growing
/// Chip RAM would change what the emulated game itself sees, on a machine
/// profile other screens rely on staying a real one; growing Fast RAM gives
/// WHDLoad, AmigaDOS and any `ExpMem` request somewhere to live that never
/// competes with the game for the Chip RAM it was written against.
///
/// **8 is not invented here.** `AmigaProfile::a1200_aga` (`core/profile.rs`)
/// already settled on 8 MB of Fast RAM, describing itself as "the ideal
/// WHDLoad setup" — this reuses that already-vetted number for the default
/// rather than adding a second constant this codebase would have to keep in
/// step with it.
///
/// Never applied by mutating a shared preset — see
/// `commands/launch.rs::profile_for_request`, the one place this constant is
/// read, for how a fresh per-launch copy is what actually changes.
/// Settings exposes it (`launch.whdloadFastRamMb`) so a title that needs more
/// has somewhere to ask, and the "nothing changes unless the user changes it"
/// rule means a value the user set there must survive.
///
/// **ART-152 made this the profile's number rather than a second one.** It is
/// now defined as `core::profile::WHDLOAD_PROFILE_FAST_RAM_MB` — the Fast RAM
/// `AmigaProfile::whdload_a1200` itself carries — so the shipped default, the
/// profile and the Settings control are one value that cannot drift apart.
///
/// **A default, and nothing more.** `profile_for_request` uses whatever the
/// user configured, exactly, in both directions: it briefly clamped a lower
/// value up to this one, which meant a user who lowered Fast RAM in Settings
/// was silently overruled. There is no documented Fast RAM floor to enforce —
/// whdload.de's only stated memory requirement is 1.0 MiB *total*, which the
/// profile's 2 MB of Chip RAM already meets — so this number's whole job is
/// to be a sensible starting point, not a limit.
pub const DEFAULT_WHDLOAD_FAST_RAM_MB: u32 = WHDLOAD_PROFILE_FAST_RAM_MB;

/// The one machine every WHDLoad launch runs on, whatever chipset the
/// catalogue records for the original game — ART-152, the owner's decision.
///
/// **This deliberately overrides [`machine_for`] for WHDLoad-shaped
/// requests.** `machine_for` answers "which Amiga was this game written
/// for?", which is the right question for a floppy and the wrong one for a
/// WHDLoad install: what boots is AmigaDOS and WHDLoad, and only then the
/// patched game. See [`crate::core::profile::AmigaProfile::whdload_a1200`] for the full reasoning,
/// the sources, and — importantly — what has *not* been measured about
/// running an OCS-only title here.
///
/// It changes the ROM choice as well as the profile, and that is the point of
/// putting it before [`plan_for`]'s lookup rather than after: an A1200
/// machine is matched against ROMs whose `models` list says `A1200`, and
/// [`WHDLOAD_MIN_KICKSTART_MAJOR`] still applies on top, so a Kickstart 1.3
/// dump can no more satisfy this machine than it could the A500 one — never a
/// silent downgrade.
///
/// **It narrows which ROMs are eligible, and that is a real behaviour
/// change.** `best_rom` matches on the ROM's own `models` list, so a
/// Kickstart 3.1 rev 40.63 for the A500/A600/A2000 no longer suits a WHDLoad
/// launch however new it is. Refusing is right — that ROM is not an A1200 —
/// but the user has to be told *which* Kickstart to add rather than that
/// theirs is too old, which is why [`LaunchRefusal::NoWhdloadMachineRom`]
/// exists separately from [`LaunchRefusal::NoRomMeetsWhdloadMinimum`].
pub const WHDLOAD_PROFILE_MACHINE: Machine = Machine::A1200;

/// Which machine a stated chipset requirement asks for, falling back to the
/// user's own default when the catalogue states none.
///
/// OCS and ECS both run on an A500; only AGA needs the A1200. `default` is
/// the common case, not the edge: most WHDLoad titles in ART's catalogue
/// state no chipset at all.
pub fn machine_for(stated: Option<Chipset>, default: Machine) -> Machine {
    match stated {
        Some(Chipset::Aga) => Machine::A1200,
        Some(Chipset::Ocs) | Some(Chipset::Ecs) => Machine::A500,
        None => default,
    }
}

/// Whether a request is WHDLoad-shaped — see [`RequestKind::Hardfile`]'s own
/// doc comment for why a plain hardfile is excluded. Backs two decisions, not
/// one: [`WHDLOAD_MIN_KICKSTART_MAJOR`]'s floor on the chosen ROM (`plan_for`,
/// below), and [`DEFAULT_WHDLOAD_FAST_RAM_MB`]'s Fast RAM on the chosen
/// profile's memory (`commands/launch.rs::profile_for_request`). Both are
/// facts about WHDLoad itself, not about hardfiles or drawers in general, so
/// both read the same predicate rather than risking two definitions drifting
/// apart.
pub fn is_whdload_shaped(kind: &RequestKind) -> bool {
    match kind {
        RequestKind::Whdload { .. } => true,
        RequestKind::Hardfile { whdload, .. } => *whdload,
        RequestKind::Floppies { .. } => false,
    }
}

/// The machine a request actually launches on: [`WHDLOAD_PROFILE_MACHINE`]
/// for anything WHDLoad-shaped, [`machine_for`]'s catalogue inference for
/// everything else — ART-152.
///
/// The split is the whole decision in one function. A floppy still gets the
/// Amiga its game was written for, because that is the machine the game
/// itself talks to the hardware of; a WHDLoad title gets the one known-good
/// machine, because what boots there is AmigaDOS and WHDLoad rather than the
/// game. `stated` and `default` are ignored — not overridden, ignored — in
/// the WHDLoad case, which is why they are still taken: the same call site
/// serves both, so a future reader sees exactly what is being set aside.
///
/// This is *not* the last word. `commands/launch.rs::resolved_machine` still
/// lets the user's own per-title choice (`LaunchArgs::machine_override`)
/// outrank this, the same way it already outranks [`machine_for`] — a
/// decision ART makes for the user must stay one the user can undo.
pub fn machine_for_request(
    kind: &RequestKind,
    stated: Option<Chipset>,
    default: Machine,
) -> Machine {
    if is_whdload_shaped(kind) {
        WHDLOAD_PROFILE_MACHINE
    } else {
        machine_for(stated, default)
    }
}

/// The ROM this launch will boot, among every candidate that suits the
/// machine's model.
///
/// **The rule: highest known major wins, ties break by name.** A ROM whose
/// major ART could not read ([`LaunchRom::major`] is `None`) sorts below
/// every known one — `Option<u16>`'s own `Ord` already orders `None` before
/// `Some`, so this reads as plainly as it decides. Picking the newest
/// suitable ROM rather than "first in scan order" (ART-148: that scan order
/// is alphabetic, so a folder holding both a 1.3 and a 3.1 dump for the same
/// machine picked the 1.3) is deliberate for two reasons: it is what a
/// WHDLoad floor needs to be checkable — filtering by `min_major` first and
/// taking the newest survivor is the same operation, not a special case —
/// and a newer Kickstart is the more broadly compatible default absent any
/// other signal, since ART has no per-title data saying an *older* one is
/// required. `min_major` is `None` for anything that does not carry
/// [`WHDLOAD_MIN_KICKSTART_MAJOR`]'s requirement (a floppy, a plain
/// hardfile), in which case a ROM with no known major is still eligible —
/// only the WHDLoad floor demands a *proven* major, because an unproven one
/// might be exactly the 1.x machine that requirement exists to rule out.
fn best_rom(roms: &[LaunchRom], machine: Machine, min_major: Option<u16>) -> Option<&LaunchRom> {
    roms.iter()
        .filter(|rom| rom.models.iter().any(|m| m == machine.model_name()))
        .filter(|rom| match min_major {
            Some(floor) => rom.major.is_some_and(|major| major >= floor),
            None => true,
        })
        .max_by_key(|rom| (rom.major, std::cmp::Reverse(rom.name.clone())))
}

/// Decide what a launch needs, or refuse rather than guess.
pub fn plan_for(request: &LaunchRequest) -> Result<LaunchPlan, LaunchRefusal> {
    // Checked before the ROM lookup: a floppy set with nothing to mount is a
    // broken request on its own terms, independent of whether a suitable
    // Kickstart happens to be on hand.
    if let RequestKind::Floppies { images } = &request.kind {
        if images.is_empty() {
            return Err(LaunchRefusal::NothingToMount);
        }
    }

    // A bare `.adf` boots on any Kickstart and must keep doing so (ART-148):
    // the floor below applies only when `is_whdload_shaped` says this
    // request is WHDLoad-shaped.
    let rom = if is_whdload_shaped(&request.kind) {
        best_rom(
            request.roms,
            request.machine,
            Some(WHDLOAD_MIN_KICKSTART_MAJOR),
        )
        .cloned()
        // Which refusal is a diagnosis, not a formality: asking again
        // without the floor separates "there is a Kickstart for this
        // machine and it is too old" from "there is no Kickstart for this
        // machine at all". Both refuse; only one of them is true at a time,
        // and telling the user the wrong one sends them hunting for a
        // Kickstart newer than the 3.1 they already own. See
        // [`LaunchRefusal::NoWhdloadMachineRom`].
        .ok_or_else(|| {
            if best_rom(request.roms, request.machine, None).is_some() {
                LaunchRefusal::NoRomMeetsWhdloadMinimum {
                    machine: request.machine,
                }
            } else {
                LaunchRefusal::NoWhdloadMachineRom {
                    machine: request.machine,
                }
            }
        })?
    } else {
        best_rom(request.roms, request.machine, None)
            .cloned()
            .ok_or(LaunchRefusal::NoSuitableRom {
                machine: request.machine,
            })?
    };

    let mut notes = Vec::new();

    let kind = match &request.kind {
        RequestKind::Floppies { images } => {
            let total = images.len();
            let mounted: Vec<String> = images.iter().take(MAX_FLOPPY_DRIVES).cloned().collect();
            if mounted.len() < total {
                notes.push(LaunchNote::MoreDisksThanDrives {
                    total,
                    mounted: mounted.len(),
                });
            }
            LaunchKind::Floppies { images: mounted }
        }
        RequestKind::Hardfile { image, .. } => LaunchKind::Hardfile {
            image: image.clone(),
        },
        RequestKind::Whdload { drawer, slave } => {
            let system = request
                .system_volume
                .clone()
                .ok_or(LaunchRefusal::NoSystemVolume)?;
            LaunchKind::Whdload {
                drawer: drawer.clone(),
                slave: slave.clone(),
                system,
                one_click: request.one_click,
            }
        }
    };

    Ok(LaunchPlan {
        machine: request.machine,
        rom,
        kind,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a1200_rom() -> LaunchRom {
        LaunchRom {
            name: "Kickstart 40.68 (A1200)".into(),
            models: vec!["A1200".into()],
            path: r"D:\roms\kick40068.A1200".into(),
            major: Some(40),
        }
    }

    fn a500_rom() -> LaunchRom {
        LaunchRom {
            name: "Kickstart 34.5 (A500/A2000/A1000)".into(),
            models: vec!["A500".into(), "A2000".into()],
            path: r"D:\roms\kick34005.A500".into(),
            major: Some(34),
        }
    }

    /// The other end of the ROM folder from [`a500_rom`] — same models, a
    /// major that meets [`WHDLOAD_MIN_KICKSTART_MAJOR`].
    fn a500_kick31() -> LaunchRom {
        LaunchRom {
            name: "Kickstart 3.1 (40.063) A500/A600/A2000".into(),
            models: vec!["A500".into(), "A600".into(), "A2000".into()],
            path: r"D:\roms\kick40063.A500".into(),
            major: Some(40),
        }
    }

    /// The chipset the catalogue recorded picks the machine.
    #[test]
    fn an_aga_title_asks_for_an_a1200_and_an_ocs_one_for_an_a500() {
        assert_eq!(
            machine_for(Some(Chipset::Aga), Machine::A500),
            Machine::A1200
        );
        assert_eq!(
            machine_for(Some(Chipset::Ocs), Machine::A500),
            Machine::A500
        );
    }

    /// 1536 WHDLoad titles state no chipset at all, so the default is the
    /// common case and not the edge.
    #[test]
    fn a_title_that_states_no_chipset_takes_the_users_default() {
        assert_eq!(machine_for(None, Machine::A1200), Machine::A1200);
    }

    /// ART-152: a WHDLoad-shaped request takes the one known-good machine
    /// whatever the catalogue says — including the case the whole decision
    /// turns on, an OCS title on a user whose default is the A500, which
    /// `machine_for` alone answers `A500` for.
    #[test]
    fn a_whdload_request_ignores_the_catalogues_chipset_and_plans_the_a1200() {
        for kind in [
            RequestKind::Whdload {
                drawer: r"D:\g\Turrican".into(),
                slave: "Turrican.Slave".into(),
            },
            RequestKind::Hardfile {
                image: r"D:\g\Turrican.hdf".into(),
                whdload: true,
            },
        ] {
            for stated in [
                Some(Chipset::Ocs),
                Some(Chipset::Ecs),
                Some(Chipset::Aga),
                None,
            ] {
                assert_eq!(
                    machine_for_request(&kind, stated, Machine::A500),
                    Machine::A1200,
                    "{kind:?} / {stated:?}"
                );
            }
        }
        // The same OCS answer `machine_for` gives on its own, so the
        // assertion above is a real change of behaviour and not a
        // restatement of the inference.
        assert_eq!(
            machine_for(Some(Chipset::Ocs), Machine::A500),
            Machine::A500
        );
    }

    /// The rule reaches WHDLoad-shaped requests and nothing else: a floppy
    /// set and a plain (non-WHDLoad) hardfile still follow the catalogue.
    #[test]
    fn a_non_whdload_request_still_follows_the_catalogues_chipset() {
        let floppies = RequestKind::Floppies {
            images: vec![r"D:\g\a.adf".into()],
        };
        let plain = RequestKind::Hardfile {
            image: r"D:\g\workbench.hdf".into(),
            whdload: false,
        };

        for kind in [&floppies, &plain] {
            assert_eq!(
                machine_for_request(kind, Some(Chipset::Ocs), Machine::A500),
                Machine::A500,
                "{kind:?}"
            );
            assert_eq!(
                machine_for_request(kind, Some(Chipset::Aga), Machine::A500),
                Machine::A1200,
                "{kind:?}"
            );
            assert_eq!(
                machine_for_request(kind, None, Machine::A1200),
                Machine::A1200,
                "{kind:?}"
            );
        }
    }

    /// The shipped default, the profile's own Fast RAM and the number
    /// Settings starts from are one value. Two constants that agree today
    /// are what this asserts cannot happen.
    #[test]
    fn the_default_fast_ram_is_the_whdload_profiles_own() {
        let profile = crate::core::profile::AmigaProfile::whdload_a1200();
        assert_eq!(profile.memory.fast_mb, DEFAULT_WHDLOAD_FAST_RAM_MB);
        assert_eq!(profile.memory.chip_kb, 2048);
        assert_eq!(profile.chipset, crate::core::profile::ChipsetModel::Aga);
    }

    /// ART-152's follow-up: the two WHDLoad ROM refusals are a diagnosis,
    /// and each must be raised only when it is the true one. A Kickstart 3.1
    /// rev 40.63 for the A500/A600/A2000 clears
    /// [`WHDLOAD_MIN_KICKSTART_MAJOR`] comfortably and still does not suit
    /// the A1200 the profile plans — reporting that as "too old" is the
    /// wrong sentence, and the wrong variant is how it gets there.
    #[test]
    fn the_two_whdload_rom_refusals_each_describe_their_own_case() {
        let kind = RequestKind::Whdload {
            drawer: r"D:\g\Turrican".into(),
            slave: "Turrican.Slave".into(),
        };
        let request = |roms: &[LaunchRom]| {
            plan_for(&LaunchRequest {
                machine: WHDLOAD_PROFILE_MACHINE,
                roms,
                kind: kind.clone(),
                system_volume: Some(r"D:\sys\Workbench.hdf".into()),
                one_click: true,
            })
            .unwrap_err()
        };

        // New enough, wrong machine: a 40.63 A500/A600/A2000 dump.
        assert_eq!(
            request(&[a500_kick31()]),
            LaunchRefusal::NoWhdloadMachineRom {
                machine: Machine::A1200
            }
        );
        // Right machine, too old: an A1200 dump below the floor.
        let old_a1200 = LaunchRom {
            name: "Kickstart 1.3 (34.5) A1200".into(),
            models: vec!["A1200".into()],
            path: r"D:\roms\kick34005.A1200".into(),
            major: Some(34),
        };
        assert_eq!(
            request(&[old_a1200]),
            LaunchRefusal::NoRomMeetsWhdloadMinimum {
                machine: Machine::A1200
            }
        );
    }

    /// A ROM that does not suit is a refusal, not a black screen.
    #[test]
    fn no_suitable_rom_refuses_rather_than_launching() {
        let refusal = plan_for(&LaunchRequest {
            machine: Machine::A1200,
            roms: &[a500_rom()],
            kind: RequestKind::Floppies {
                images: vec![r"D:\g\a.adf".into()],
            },
            system_volume: None,
            one_click: true,
        })
        .unwrap_err();

        assert!(matches!(
            refusal,
            LaunchRefusal::NoSuitableRom {
                machine: Machine::A1200
            }
        ));
    }

    /// An empty ROM folder is the commonest case there is, not an edge — a
    /// user who has added no Kickstarts yet has exactly this list.
    #[test]
    fn no_suitable_rom_refuses_when_the_rom_list_is_empty() {
        let refusal = plan_for(&LaunchRequest {
            machine: Machine::A1200,
            roms: &[],
            kind: RequestKind::Hardfile {
                image: r"D:\g\game.hdf".into(),
                whdload: false,
            },
            system_volume: None,
            one_click: true,
        })
        .unwrap_err();

        assert!(matches!(
            refusal,
            LaunchRefusal::NoSuitableRom {
                machine: Machine::A1200
            }
        ));
    }

    /// WinUAE has four drives. Saying so is the whole fix; pretending
    /// otherwise is what §89 forbids.
    #[test]
    fn a_set_larger_than_four_disks_mounts_four_and_says_so() {
        let plan = plan_for(&LaunchRequest {
            machine: Machine::A500,
            roms: &[a500_rom()],
            kind: RequestKind::Floppies {
                images: (1..=6).map(|n| format!(r"D:\g\disk{n}.adf")).collect(),
            },
            system_volume: None,
            one_click: true,
        })
        .unwrap();

        match plan.kind {
            LaunchKind::Floppies { ref images } => assert_eq!(images.len(), 4),
            ref other => panic!("{other:?}"),
        }
        assert!(plan.notes.contains(&LaunchNote::MoreDisksThanDrives {
            total: 6,
            mounted: 4
        }));
    }

    /// A floppy set with nothing in it — an `.rp9` manifest naming no disk
    /// and no hardfile — must refuse rather than plan and launch WinUAE with
    /// nothing mounted at all.
    #[test]
    fn an_empty_floppy_set_refuses_rather_than_launching_with_nothing_mounted() {
        let refusal = plan_for(&LaunchRequest {
            machine: Machine::A500,
            roms: &[a500_rom()],
            kind: RequestKind::Floppies { images: vec![] },
            system_volume: None,
            one_click: true,
        })
        .unwrap_err();

        assert!(matches!(refusal, LaunchRefusal::NothingToMount));
    }

    /// The refusal fires even when no ROM would have suited either — the
    /// request is broken on its own terms, not merely unlucky about the ROM
    /// folder.
    #[test]
    fn an_empty_floppy_set_refuses_even_with_no_roms_at_all() {
        let refusal = plan_for(&LaunchRequest {
            machine: Machine::A500,
            roms: &[],
            kind: RequestKind::Floppies { images: vec![] },
            system_volume: None,
            one_click: true,
        })
        .unwrap_err();

        assert!(matches!(refusal, LaunchRefusal::NothingToMount));
    }

    /// A WHDLoad drawer cannot run without a booting system, and ART owns none.
    #[test]
    fn a_whdload_title_without_a_system_volume_refuses() {
        let refusal = plan_for(&LaunchRequest {
            machine: Machine::A1200,
            roms: &[a1200_rom()],
            kind: RequestKind::Whdload {
                drawer: r"D:\games\Turrican".into(),
                slave: "Turrican.slave".into(),
            },
            system_volume: None,
            one_click: true,
        })
        .unwrap_err();

        assert!(matches!(refusal, LaunchRefusal::NoSystemVolume));
    }

    #[test]
    fn a_whdload_title_with_a_system_volume_plans_both_mounts() {
        let plan = plan_for(&LaunchRequest {
            machine: Machine::A1200,
            roms: &[a1200_rom()],
            kind: RequestKind::Whdload {
                drawer: r"D:\games\Turrican".into(),
                slave: "Turrican.slave".into(),
            },
            system_volume: Some(r"E:\amiga\amikit\AmiKit.hdf".into()),
            one_click: true,
        })
        .unwrap();

        match plan.kind {
            LaunchKind::Whdload {
                ref system,
                ref slave,
                one_click,
                ..
            } => {
                assert_eq!(system, r"E:\amiga\amikit\AmiKit.hdf");
                assert_eq!(slave, "Turrican.slave");
                assert!(one_click, "Y2 by default, with Y1 always one switch away");
            }
            ref other => panic!("{other:?}"),
        }
    }

    // ---- ART-148: WHDLoad's own Kickstart floor -----------------------
    //
    // <https://www.whdload.de/docs/en/need.html> states WHDLoad's minimum as
    // "Kickstart 2.0 (version 37)". `1000 Miglia` — a self-booting WHDLoad
    // hardfile whose catalogue record states no chipset — was planned on a
    // Kickstart 1.3 machine because `plan_for` used to take "first suitable
    // ROM in scan order" with no floor at all. A 1.3 machine cannot run
    // WHDLoad (below its stated minimum) and has no hard-disk filesystem in
    // ROM to mount the bare `DOS\1` FFS hardfile with either way — both
    // halves of why the emulator said "not a DOS disk".

    /// A folder holding both a 1.3 and a 3.1 dump for the same machine must
    /// plan the 3.1 — the 1.3 winning a name-sorted scan is exactly ART-148.
    #[test]
    fn a_whdload_hardfile_with_a_1_3_and_a_3_1_available_plans_the_3_1() {
        let plan = plan_for(&LaunchRequest {
            machine: Machine::A500,
            roms: &[a500_rom(), a500_kick31()],
            kind: RequestKind::Hardfile {
                image: r"D:\g\1000 Miglia.hdf".into(),
                whdload: true,
            },
            system_volume: None,
            one_click: true,
        })
        .unwrap();

        assert_eq!(plan.rom.major, Some(40));
    }

    /// A folder holding only Kickstart 1.x must refuse a WHDLoad hardfile
    /// rather than plan a machine that cannot run WHDLoad — or mount the
    /// hardfile — at all.
    #[test]
    fn a_whdload_hardfile_with_only_kickstart_1_x_refuses_with_the_floor() {
        let refusal = plan_for(&LaunchRequest {
            machine: Machine::A500,
            roms: &[a500_rom()],
            kind: RequestKind::Hardfile {
                image: r"D:\g\1000 Miglia.hdf".into(),
                whdload: true,
            },
            system_volume: None,
            one_click: true,
        })
        .unwrap_err();

        assert!(matches!(
            refusal,
            LaunchRefusal::NoRomMeetsWhdloadMinimum {
                machine: Machine::A500
            }
        ));
    }

    /// `RequestKind::Whdload` is WHDLoad-shaped by definition, so the same
    /// floor applies to it — checked before `NoSystemVolume`, so a title
    /// with no suitable Kickstart is refused for the right reason first.
    #[test]
    fn a_whdload_drawer_with_only_kickstart_1_x_refuses_with_the_floor() {
        let refusal = plan_for(&LaunchRequest {
            machine: Machine::A500,
            roms: &[a500_rom()],
            kind: RequestKind::Whdload {
                drawer: r"D:\games\Turrican".into(),
                slave: "Turrican.slave".into(),
            },
            system_volume: None,
            one_click: true,
        })
        .unwrap_err();

        assert!(matches!(
            refusal,
            LaunchRefusal::NoRomMeetsWhdloadMinimum {
                machine: Machine::A500
            }
        ));
    }

    /// The same folder, for a *plain* hardfile (`whdload: false`), must not
    /// be held to WHDLoad's floor — a hand-installed AmigaOS hardfile may
    /// need exactly this Kickstart and has nothing to do with WHDLoad.
    #[test]
    fn a_plain_hardfile_is_not_held_to_the_whdload_floor() {
        let plan = plan_for(&LaunchRequest {
            machine: Machine::A500,
            roms: &[a500_rom()],
            kind: RequestKind::Hardfile {
                image: r"D:\g\game.hdf".into(),
                whdload: false,
            },
            system_volume: None,
            one_click: true,
        })
        .unwrap();

        assert_eq!(plan.rom.major, Some(34));
    }

    /// A bare `.adf` must keep booting on any Kickstart — the floor never
    /// applies to floppies, even when the only ROM on hand is a 1.3.
    #[test]
    fn a_floppy_set_is_not_held_to_the_whdload_floor() {
        let plan = plan_for(&LaunchRequest {
            machine: Machine::A500,
            roms: &[a500_rom()],
            kind: RequestKind::Floppies {
                images: vec![r"D:\g\a.adf".into()],
            },
            system_volume: None,
            one_click: true,
        })
        .unwrap();

        assert_eq!(plan.rom.major, Some(34));
    }

    /// What a later task's TypeScript has to match, pinned exactly as
    /// `core/rom/pairing.rs::the_wire_shape_is_what_the_frontend_reads` pins
    /// `Pairing`'s — for the same reason: Rust keeps compiling if a variant
    /// is renamed or a `#[serde(tag = ...)]` is dropped, TypeScript keeps
    /// compiling too, and the break only shows up at runtime, in the user's
    /// hands.
    ///
    /// `one_click` is asserted as `"one_click"`, not `"oneClick"` — that is
    /// what this struct actually produces, with no `rename_all = "camelCase"`
    /// on `LaunchKind`. If the frontend plan expects `oneClick`, that is a
    /// choice for a later task to make, not for this test to paper over.
    #[test]
    fn the_wire_shape_is_what_the_frontend_reads() {
        assert_eq!(
            serde_json::to_value(Machine::A500).unwrap(),
            serde_json::json!("a500")
        );
        assert_eq!(
            serde_json::to_value(Machine::A1200).unwrap(),
            serde_json::json!("a1200")
        );

        assert_eq!(
            serde_json::to_value(LaunchNote::MoreDisksThanDrives {
                total: 6,
                mounted: 4
            })
            .unwrap(),
            serde_json::json!({ "kind": "more-disks-than-drives", "total": 6, "mounted": 4 })
        );

        assert_eq!(
            serde_json::to_value(LaunchRefusal::NoSuitableRom {
                machine: Machine::A1200
            })
            .unwrap(),
            serde_json::json!({ "kind": "no-suitable-rom", "machine": "a1200" })
        );
        assert_eq!(
            serde_json::to_value(LaunchRefusal::NoRomMeetsWhdloadMinimum {
                machine: Machine::A500
            })
            .unwrap(),
            serde_json::json!({ "kind": "no-rom-meets-whdload-minimum", "machine": "a500" })
        );

        assert_eq!(
            serde_json::to_value(LaunchRefusal::NoWhdloadMachineRom {
                machine: Machine::A1200
            })
            .unwrap(),
            serde_json::json!({ "kind": "no-whdload-machine-rom", "machine": "a1200" })
        );
        assert_eq!(
            serde_json::to_value(LaunchRefusal::NoSystemVolume).unwrap(),
            serde_json::json!({ "kind": "no-system-volume" })
        );
        assert_eq!(
            serde_json::to_value(LaunchRefusal::FileMissing {
                path: r"D:\g\a.adf".into()
            })
            .unwrap(),
            serde_json::json!({ "kind": "file-missing", "path": r"D:\g\a.adf" })
        );
        assert_eq!(
            serde_json::to_value(LaunchRefusal::NothingToMount).unwrap(),
            serde_json::json!({ "kind": "nothing-to-mount" })
        );

        assert_eq!(
            serde_json::to_value(LaunchKind::Floppies {
                images: vec!["a.adf".into()]
            })
            .unwrap(),
            serde_json::json!({ "kind": "floppies", "images": ["a.adf"] })
        );
        assert_eq!(
            serde_json::to_value(LaunchKind::Hardfile {
                image: "a.hdf".into()
            })
            .unwrap(),
            serde_json::json!({ "kind": "hardfile", "image": "a.hdf" })
        );
        assert_eq!(
            serde_json::to_value(LaunchKind::Whdload {
                drawer: r"D:\games\Turrican".into(),
                slave: "Turrican.slave".into(),
                system: r"E:\amiga\amikit\AmiKit.hdf".into(),
                one_click: true,
            })
            .unwrap(),
            serde_json::json!({
                "kind": "whdload",
                "drawer": r"D:\games\Turrican",
                "slave": "Turrican.slave",
                "system": r"E:\amiga\amikit\AmiKit.hdf",
                "one_click": true
            })
        );

        assert_eq!(
            serde_json::to_value(RequestKind::Floppies {
                images: vec!["a.adf".into()]
            })
            .unwrap(),
            serde_json::json!({ "kind": "floppies", "images": ["a.adf"] })
        );
        assert_eq!(
            serde_json::to_value(RequestKind::Hardfile {
                image: "a.hdf".into(),
                whdload: true,
            })
            .unwrap(),
            serde_json::json!({ "kind": "hardfile", "image": "a.hdf", "whdload": true })
        );
        assert_eq!(
            serde_json::to_value(RequestKind::Whdload {
                drawer: r"D:\games\Turrican".into(),
                slave: "Turrican.slave".into(),
            })
            .unwrap(),
            serde_json::json!({
                "kind": "whdload",
                "drawer": r"D:\games\Turrican",
                "slave": "Turrican.slave"
            })
        );
    }
}
