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

use serde::{Deserialize, Serialize};

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
    Floppies { images: Vec<String> },
    Hardfile { image: String },
    Whdload { drawer: String, slave: String },
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
    /// A WHDLoad drawer needs a system to boot into, and ART owns none.
    NoSystemVolume,
    /// A path the plan needs turned out not to exist, once the command
    /// layer checked. Never raised by this module itself — it reads no
    /// files — but part of the same refusal type so callers have one place
    /// to render either kind of "cannot launch this".
    FileMissing { path: String },
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

/// Decide what a launch needs, or refuse rather than guess.
pub fn plan_for(request: &LaunchRequest) -> Result<LaunchPlan, LaunchRefusal> {
    let rom = request
        .roms
        .iter()
        .find(|rom| rom.models.iter().any(|m| m == request.machine.model_name()))
        .cloned()
        .ok_or(LaunchRefusal::NoSuitableRom {
            machine: request.machine,
        })?;

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
        RequestKind::Hardfile { image } => LaunchKind::Hardfile {
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
        }
    }

    fn a500_rom() -> LaunchRom {
        LaunchRom {
            name: "Kickstart 34.5 (A500/A2000/A1000)".into(),
            models: vec!["A500".into(), "A2000".into()],
            path: r"D:\roms\kick34005.A500".into(),
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
}
