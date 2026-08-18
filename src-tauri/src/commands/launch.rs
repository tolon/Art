//! Play: turning a catalogued title into a running WinUAE session.
//!
//! A thin adapter, as CLAUDE.md requires: deserialize, call core, serialize
//! back. Deciding *what* to launch lives in `core/launch`; deciding *how to
//! get there* — unpacking a `.rp9`, writing the WHDLoad boot directory,
//! detecting WinUAE, building the `.uae` text and starting the process —
//! lives in `core/launch/extract`, `core/launch/whdload_boot` and
//! `core/winuae`. This module only wires the two together and answers the
//! two questions `core/launch` deliberately does not: which files actually
//! exist on disk, and where ART's own scratch files go.
//!
//! **Two commands, one plan.** `launch_plan` computes a [`LaunchPlan`] (or a
//! refusal) and starts nothing — that is what the confirmation screen shows.
//! `launch_title` computes the same plan again (the screen may have sat open
//! for a while) and then, only then, does the work: unpack, write, launch.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use super::oplog::{user_operation, write_result};
use crate::core::error::CoreError;
use crate::core::gameindex::record::Media;
use crate::core::launch::extract::{unpack_floppies, unpack_hardfile};
use crate::core::launch::whdload_boot::write_boot_dir;
use crate::core::launch::{
    machine_for, plan_for, Chipset, LaunchKind, LaunchPlan, LaunchRefusal, LaunchRequest,
    LaunchRom, Machine, RequestKind,
};
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::core::profile::AmigaProfile;
use crate::core::rom::{scan_rom_directory, RomInfo};
use crate::core::winuae::{
    detect_winuae, generate_uae_config, launch_winuae, DirMount, LaunchMedia,
};
use crate::error::{AppError, AppResult};

/// `RomInfo` → the two fields `core/launch` reads. The lower module must not
/// know the higher one's type; this is where the translation lives.
fn launch_rom_from(info: &RomInfo) -> LaunchRom {
    LaunchRom {
        name: info.name.clone(),
        models: info.compatible_models.clone(),
        path: info.file_path.clone(),
    }
}

/// The catalogue's chipset string (`core::gameindex::record::ChipsetRequirement`,
/// which serialises as `"ocsecs"` / `"aga"`) into `core/launch`'s own
/// three-way [`Chipset`]. `"ocs"` and `"ecs"` are accepted too — nothing in
/// this catalogue emits them separately today, but [`machine_for`] treats
/// both identically (both mean an A500), so accepting either spelling costs
/// nothing and does not risk a future three-way source being silently
/// dropped to `None`. Anything else — including no chipset at all, the
/// common case for the 1536 WHDLoad titles that state none — is `None`,
/// which falls back to the user's own default machine.
fn chipset_from(chipset: Option<&str>) -> Option<Chipset> {
    match chipset {
        Some("aga") => Some(Chipset::Aga),
        Some("ocsecs") | Some("ocs") => Some(Chipset::Ocs),
        Some("ecs") => Some(Chipset::Ecs),
        _ => None,
    }
}

/// What a catalogued title asks the frontend to launch.
#[derive(Debug, Clone, Deserialize)]
pub struct LaunchArgs {
    pub id: String,
    pub title: String,
    /// The catalogued file: the `.adf`, the `.hdf`, the `.rp9`, or the
    /// WHDLoad drawer.
    pub path: String,
    pub media: Media,
    pub chipset: Option<String>,
    /// From Settings: the ROM folder, the user's default machine, and the
    /// bootable system a WHDLoad title needs.
    pub rom_dir: String,
    pub default_machine: Machine,
    /// The user's own choice for *this* title (`TitleDetail`'s per-title
    /// machine picker), kept apart from `default_machine` rather than folded
    /// into it. A choice the user made explicitly must outrank an inference
    /// — the same rule `Provenance::UserEdit` states everywhere else in this
    /// codebase — and `machine_for`'s chipset→machine inference is exactly
    /// an inference: it must never override a machine the user picked by
    /// hand for this title. `None` is "auto": no override, let `machine_for`
    /// decide from the catalogue's chipset and `default_machine`.
    pub machine_override: Option<Machine>,
    pub system_volume: Option<String>,
    pub one_click: bool,
}

/// The machine a launch actually uses: the user's own per-title choice when
/// there is one, otherwise `machine_for`'s inference from the catalogue's
/// chipset and the user's default. `machine_override` is never folded into
/// `default_machine` before this call — doing that once made the per-title
/// picker inert for any title with a stated chipset, because `machine_for`
/// only consults its `default` argument when the chipset is `None`.
fn resolved_machine(request: &LaunchArgs) -> Machine {
    request.machine_override.unwrap_or_else(|| {
        machine_for(
            chipset_from(request.chipset.as_deref()),
            request.default_machine,
        )
    })
}

/// `Media` → the shape `core/launch::plan_for` reads.
///
/// A `Floppies` whose file is a `.rp9` still arrives as
/// [`RequestKind::Floppies`], with the entry names exactly as the catalogue
/// holds them — they are archive entry names, not host paths, when the
/// catalogued file is a `.rp9` (`from_rp9` in `core/gameindex/scan.rs`
/// reads them out of `<floppy priority="n">`), and are the file's own single
/// name otherwise (`read_one` in the same module, for a bare `.adf`/`.img`).
///
/// `Hardfile` follows the **same rule**, and getting this wrong was ART-141:
/// `Media::Hardfile { file }`'s `file` is the zip entry name a `.rp9`'s
/// `<harddrive>` names (`core::gameindex::readers::rp9`) when the catalogued
/// file is a `.rp9`, and is the file's own name — the same one `args.path`
/// already points at — otherwise. Either way `file` is carried through
/// untouched here; [`media_for_plan`] is what turns it into a real path,
/// extracting from the `.rp9` when there is one to extract from.
///
/// [`launch_title_inner`] is what turns either shape into real paths on
/// disk; the preview shows the disk/hardfile name the user recognises rather
/// than a temporary directory they have never seen.
fn request_kind_from(args: &LaunchArgs) -> RequestKind {
    match &args.media {
        Media::Floppies { ordered } => RequestKind::Floppies {
            images: ordered.clone(),
        },
        Media::Hardfile { file } => RequestKind::Hardfile {
            image: file.clone(),
        },
        Media::WhdloadDrawer { slave } => RequestKind::Whdload {
            drawer: args.path.clone(),
            slave: slave.clone(),
        },
    }
}

/// What a launch would do, or why it cannot — computed without starting
/// anything, so the confirmation screen has something to show.
#[derive(Debug, Clone, Serialize)]
pub struct LaunchPreview {
    pub plan: Option<LaunchPlan>,
    pub refusal: Option<LaunchRefusal>,
    /// What will be mounted and whether it can be written to (design §4.4) —
    /// empty on a refusal, since nothing is going to be mounted.
    pub mounts: Vec<MountNote>,
}

/// Work out what a launch would need. Starts nothing, reads no media —
/// only the ROM folder is scanned, which is what a Kickstart choice needs.
#[tauri::command]
pub fn launch_plan(request: LaunchArgs, _app: AppHandle) -> AppResult<LaunchPreview> {
    let roms: Vec<LaunchRom> = scan_rom_directory(Path::new(&request.rom_dir))
        .unwrap_or_default()
        .iter()
        .map(launch_rom_from)
        .collect();

    let machine = resolved_machine(&request);
    let plan = plan_for(&LaunchRequest {
        machine,
        roms: &roms,
        kind: request_kind_from(&request),
        system_volume: request.system_volume.clone(),
        one_click: request.one_click,
    });

    Ok(match plan {
        Ok(plan) => {
            let mounts = mount_notes_for(&request, &plan);
            LaunchPreview {
                plan: Some(plan),
                refusal: None,
                mounts,
            }
        }
        Err(refusal) => LaunchPreview {
            plan: None,
            refusal: Some(refusal),
            mounts: vec![],
        },
    })
}

/// A [`LaunchRefusal`] as a [`CoreError`], for the one place ART cannot show
/// the structured variant — `launch_title` returns a process id or an error,
/// with no room for a `LaunchPreview`. Also where a path that has vanished
/// since the preview was drawn (`FileMissing`) turns into the same kind of
/// error as a refusal `plan_for` raised itself; the type the two share exists
/// for exactly this (`LaunchRefusal::FileMissing`'s own doc comment).
fn refusal_error(refusal: LaunchRefusal) -> CoreError {
    let message = match &refusal {
        LaunchRefusal::NoSuitableRom { machine } => {
            format!("no Kickstart in the ROM folder suits {machine:?}")
        }
        LaunchRefusal::NoSystemVolume => {
            "no bootable system volume is configured for this WHDLoad title".to_string()
        }
        LaunchRefusal::FileMissing { path } => format!("'{path}' no longer exists"),
        LaunchRefusal::NothingToMount => {
            "this title's media names no disk to mount — there is nothing for WinUAE to load"
                .to_string()
        }
    };
    CoreError::InvalidInput(message)
}

/// Refuse rather than launch against a path that vanished between the
/// preview and the confirm. `core/launch` reads no files itself (its module
/// header says so); this is the command layer answering the question it
/// deliberately leaves open.
fn require_exists(path: &str) -> Result<(), CoreError> {
    if Path::new(path).exists() {
        Ok(())
    } else {
        Err(refusal_error(LaunchRefusal::FileMissing {
            path: path.to_string(),
        }))
    }
}

fn is_rp9(path: &str) -> bool {
    Path::new(path)
        .extension()
        .map(|e| e.eq_ignore_ascii_case("rp9"))
        .unwrap_or(false)
}

/// Whether a plain `Hardfile` title's image mounts read-only.
///
/// A bare `.hdf` is the user's own original file — read-only (CHANGELOG,
/// FEATURES, spec §93). A `.rp9`'s hardfile is unpacked into ART's own launch
/// directory and is ART's copy, not the user's original, so it stays
/// writable so its saves can persist (`unpack_hardfile`'s reuse). Shared by
/// [`media_for_plan`], which acts on it, and [`mount_notes_for`], which
/// states it on the confirmation screen before anything is mounted — the two
/// must never compute a different answer from each other.
fn hardfile_write_protected(request: &LaunchArgs) -> bool {
    !is_rp9(&request.path)
}

/// What the confirmation screen must say before Start is reached (design
/// §4.4): what each medium's kind mounts, and whether it can be written to.
/// Computed from the same facts [`media_for_plan`] later acts on, but
/// without touching disk — the preview command's own contract ("starts
/// nothing, reads no media") — so this can run during `launch_plan` as well
/// as `launch_title`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum MountNote {
    Floppies {
        count: usize,
    },
    Hardfile {
        read_only: bool,
    },
    /// The system volume is always read-only and the game's drawer is always
    /// writable (§4.4) — only `one_click` varies, which is what decides
    /// whether ART's own boot directory is mounted too.
    Whdload {
        one_click: bool,
    },
}

fn mount_notes_for(request: &LaunchArgs, plan: &LaunchPlan) -> Vec<MountNote> {
    match &plan.kind {
        LaunchKind::Floppies { images } => vec![MountNote::Floppies {
            count: images.len(),
        }],
        LaunchKind::Hardfile { .. } => vec![MountNote::Hardfile {
            read_only: hardfile_write_protected(request),
        }],
        LaunchKind::Whdload { one_click, .. } => vec![MountNote::Whdload {
            one_click: *one_click,
        }],
    }
}

/// Where a `.rp9`'s disks are unpacked to for one launch (Task 8).
fn launch_dir_for(app: &AppHandle, id: &str) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("launch")
        .join(id)
}

/// The one boot directory ART owns for a one-click WHDLoad launch (Task 10).
///
/// Not per-title: it is rewritten fresh before every Y2 launch from whatever
/// slave, system and game device names that launch needs, so there is
/// nothing in it worth keeping between titles.
fn boot_dir_for(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("launch")
        .join("boot")
}

/// The machine's built-in profile — CPU, chipset, memory, display — that
/// `generate_uae_config` needs. `core/launch` only decides *which* Amiga;
/// this is the command layer filling in what that Amiga actually is.
fn profile_for(machine: Machine) -> AmigaProfile {
    match machine {
        Machine::A500 => AmigaProfile::a500_ocs(),
        Machine::A1200 => AmigaProfile::a1200_aga(),
    }
}

/// Turn a settled [`LaunchPlan`] into the media WinUAE mounts, unpacking or
/// writing whatever the plan's kind needs along the way.
///
/// Takes `launch_dir`/`boot_dir` as plain paths rather than an `AppHandle` —
/// the only thing either was ever used for is `app.path().app_data_dir()`,
/// resolved once by the caller (`launch_title_inner`). Keeping the platform
/// handle out of this function is what makes it possible to test directly:
/// this is the highest-risk part of the whole command (device names,
/// read-only flags, boot priorities, which `.rp9` branch to take), and it
/// should not need a running Tauri app to exercise.
fn media_for_plan(
    request: &LaunchArgs,
    plan: &LaunchPlan,
    launch_dir: &Path,
    boot_dir: &Path,
) -> Result<LaunchMedia, CoreError> {
    let mut media = LaunchMedia {
        kickstart_path: Some(plan.rom.path.clone()),
        ..Default::default()
    };

    match &plan.kind {
        LaunchKind::Floppies { images } => {
            let paths: Vec<String> = if is_rp9(&request.path) {
                require_exists(&request.path)?;
                unpack_floppies(Path::new(&request.path), images, launch_dir)?
                    .into_iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect()
            } else {
                require_exists(&request.path)?;
                vec![request.path.clone()]
            };
            media.floppy_paths = paths;
        }
        LaunchKind::Hardfile { image } => {
            // ART-141: `image` is a `.rp9`'s zip entry name when the
            // catalogued file is a `.rp9` — mounting `request.path` itself in
            // that case would hand WinUAE the zip, not the hard disk image
            // inside it. Otherwise `image` is just the file's own name, which
            // `request.path` already points at directly.
            //
            // Write protection differs between the two, and deliberately:
            //
            // - A bare `.hdf` is the user's own original file. CHANGELOG,
            //   FEATURES and spec §93 all say a hardfile mounts read-only,
            //   and until this fix the code did not — writing to somebody's
            //   original hard disk image without asking is exactly what
            //   §93's "immutable by default" exists to prevent.
            // - A `.rp9`'s hardfile is unpacked into ART's own launch
            //   directory (`unpack_hardfile`), never beside the user's file —
            //   it is ART's copy, not their original, so §93 does not reach
            //   it. It stays writable so WHDLoad/game saves inside it can
            //   actually happen, and `unpack_hardfile` reuses that same copy
            //   on a later launch rather than overwriting it, which is what
            //   makes those saves survive between sessions.
            let real_path = if is_rp9(&request.path) {
                require_exists(&request.path)?;
                unpack_hardfile(Path::new(&request.path), image, launch_dir)?
                    .to_string_lossy()
                    .to_string()
            } else {
                require_exists(&request.path)?;
                media.write_protect_hardfiles = hardfile_write_protected(request);
                request.path.clone()
            };
            media.hardfile_paths = vec![real_path];
        }
        LaunchKind::Whdload {
            drawer,
            slave,
            system,
            one_click,
        } => {
            require_exists(drawer)?;
            require_exists(system)?;

            // The system image is the user's own — never ART's, and never
            // writable (spec §93: originals are immutable by default).
            media.hardfile_paths = vec![system.clone()];
            media.write_protect_hardfiles = true;

            if *one_click {
                write_boot_dir(boot_dir, slave, "DH0", "DH1")?;
                media.directories = vec![
                    DirMount {
                        host_path: drawer.clone(),
                        volume: "DH1".into(),
                        label: "Game".into(),
                        boot_priority: 0,
                        read_only: false,
                    },
                    // Highest priority of anything mounted — the whole
                    // mechanism behind "one click starts the game".
                    DirMount {
                        host_path: boot_dir.to_string_lossy().to_string(),
                        volume: "DH2".into(),
                        label: "ARTBoot".into(),
                        boot_priority: 10,
                        read_only: false,
                    },
                ];
            } else {
                // Y1: mount the system and the game, boot to Workbench, let
                // the user run WHDLoad by hand. No boot directory is written
                // — the system hardfile (priority 0 by construction, see
                // `generate_uae_config`) is what boots.
                media.directories = vec![DirMount {
                    host_path: drawer.clone(),
                    volume: "DH1".into(),
                    label: "Game".into(),
                    boot_priority: -128,
                    read_only: false,
                }];
            }
        }
    }

    Ok(media)
}

fn launch_title_inner(
    request: &LaunchArgs,
    winuae_path: Option<&str>,
    app: &AppHandle,
) -> Result<u32, CoreError> {
    let roms: Vec<LaunchRom> = scan_rom_directory(Path::new(&request.rom_dir))
        .unwrap_or_default()
        .iter()
        .map(launch_rom_from)
        .collect();

    let machine = resolved_machine(request);
    // Computed again, not carried from the preview: the screen may have sat
    // open for a while, and a ROM folder or a file on disk can change under
    // it in the meantime.
    let plan = plan_for(&LaunchRequest {
        machine,
        roms: &roms,
        kind: request_kind_from(request),
        system_volume: request.system_volume.clone(),
        one_click: request.one_click,
    })
    .map_err(refusal_error)?;

    let launch_dir = launch_dir_for(app, &request.id);
    let boot_dir = boot_dir_for(app);
    let media = media_for_plan(request, &plan, &launch_dir, &boot_dir)?;
    let profile = profile_for(plan.machine);
    let config_text = generate_uae_config(&profile, &media)?;

    // The same configured path `commands/winuae.rs::winuae_launch` already
    // honours — Play must not be the one launch path in ART that only finds
    // WinUAE when it happens to sit in a standard install location.
    let install = detect_winuae(winuae_path);
    let exe = install.executable_path.ok_or_else(|| {
        CoreError::InvalidInput(
            "WinUAE was not found in a standard install location — set its path in Settings"
                .to_string(),
        )
    })?;

    launch_winuae(Path::new(&exe), &config_text)
}

/// Launch a catalogued title. Unpacks a `.rp9`'s disks or hardfile, writes
/// the WHDLoad boot directory when Y2 is asked for, then starts WinUAE — and
/// logs the result either way (§53: an external process against the user's
/// own files is exactly what the operation log is for).
///
/// `winuae_path` is the user's configured path from Settings, the same
/// argument `winuae_launch` already takes — `None` falls back to
/// [`detect_winuae`]'s standard install locations.
#[tauri::command]
pub fn launch_title(
    request: LaunchArgs,
    winuae_path: Option<String>,
    app: AppHandle,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<u32> {
    let result: AppResult<u32> =
        launch_title_inner(&request, winuae_path.as_deref(), &app).map_err(AppError::from);

    write_result(
        &oplog,
        user_operation("Launch a title")
            .source(&request.path)
            .detail("Title", &request.title),
        &result,
        |record, pid: &u32| {
            record
                .detail("Process id", pid.to_string())
                .outcome(OperationOutcome::success())
        },
    );

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::rom::RomChecksum;

    /// `core/launch` must not know what a `RomInfo` is — that is the
    /// lower-imports-higher mistake `core/rom/pairing.rs` documents. The
    /// translation happens here, which is where this codebase puts it.
    #[test]
    fn a_rom_info_becomes_the_two_fields_the_launcher_reads() {
        let info = RomInfo {
            name: "Kickstart 40.68 (A1200)".into(),
            version: "3.1".into(),
            revision: "40.068".into(),
            size_bytes: 524_288,
            sha256: String::new(),
            crc32: String::new(),
            is_cloanto: false,
            key_available: false,
            is_aros: false,
            checksum: RomChecksum::Valid,
            compatible_models: vec!["A1200".into()],
            file_path: r"D:\roms\kick.rom".into(),
        };

        let rom = launch_rom_from(&info);

        assert_eq!(rom.name, "Kickstart 40.68 (A1200)");
        assert_eq!(rom.models, vec!["A1200".to_string()]);
        assert_eq!(rom.path, r"D:\roms\kick.rom");
    }

    /// The catalogue's two-way `ChipsetRequirement` ("ocsecs" / "aga") maps
    /// onto `core/launch`'s three-way `Chipset`, and anything unrecognised —
    /// including the common "no chipset stated" case — takes the default.
    #[test]
    fn chipset_strings_map_to_the_launch_enum() {
        assert_eq!(chipset_from(Some("aga")), Some(Chipset::Aga));
        assert_eq!(chipset_from(Some("ocsecs")), Some(Chipset::Ocs));
        assert_eq!(chipset_from(Some("ocs")), Some(Chipset::Ocs));
        assert_eq!(chipset_from(Some("ecs")), Some(Chipset::Ecs));
        assert_eq!(chipset_from(None), None);
        assert_eq!(chipset_from(Some("whatever")), None);
    }

    /// A choice the user made explicitly must outrank an inference. Before
    /// this fix, `TitleDetail.tsx` folded the per-title choice into
    /// `default_machine`, and `machine_for` only consults its `default` when
    /// the catalogue states no chipset — so picking A500 for a stated-AGA
    /// title changed nothing.
    #[test]
    fn a_users_per_title_choice_outranks_a_stated_chipset() {
        let mut request = args(Media::Hardfile {
            file: "Game.hdf".into(),
        });
        request.chipset = Some("aga".into());
        request.default_machine = Machine::A1200;
        request.machine_override = Some(Machine::A500);

        assert_eq!(resolved_machine(&request), Machine::A500);
    }

    /// With no per-title choice, the stated chipset still decides — the
    /// inference `machine_for` exists for is untouched.
    #[test]
    fn with_no_per_title_choice_the_stated_chipset_still_decides() {
        let mut request = args(Media::Hardfile {
            file: "Game.hdf".into(),
        });
        request.chipset = Some("aga".into());
        request.default_machine = Machine::A500;
        request.machine_override = None;

        assert_eq!(resolved_machine(&request), Machine::A1200);
    }

    // ---- mount_notes_for: what the confirmation screen is told ------------
    //
    // Design §4.4: the read-only system image, the writable game drawer and
    // the boot directory ART writes must be stated on the confirmation
    // screen rather than assumed. These must agree with `media_for_plan`'s
    // own decisions, which is why both read `hardfile_write_protected`.

    #[test]
    fn mount_notes_state_a_bare_hardfile_as_read_only() {
        let request = LaunchArgs {
            path: r"D:\games\Game.hdf".into(),
            ..args(Media::Hardfile {
                file: "Game.hdf".into(),
            })
        };
        let plan = plan_with(LaunchKind::Hardfile {
            image: "Game.hdf".into(),
        });

        let notes = mount_notes_for(&request, &plan);
        assert!(matches!(
            notes.as_slice(),
            [MountNote::Hardfile { read_only: true }]
        ));
    }

    #[test]
    fn mount_notes_state_an_rp9_hardfile_as_writable() {
        let request = LaunchArgs {
            path: r"D:\games\Enzo.rp9".into(),
            ..args(Media::Hardfile {
                file: "af-application.hdf".into(),
            })
        };
        let plan = plan_with(LaunchKind::Hardfile {
            image: "af-application.hdf".into(),
        });

        let notes = mount_notes_for(&request, &plan);
        assert!(matches!(
            notes.as_slice(),
            [MountNote::Hardfile { read_only: false }]
        ));
    }

    #[test]
    fn mount_notes_state_the_floppy_count() {
        let request = args(Media::Floppies {
            ordered: vec!["a.adf".into(), "b.adf".into()],
        });
        let plan = plan_with(LaunchKind::Floppies {
            images: vec!["a.adf".into(), "b.adf".into()],
        });

        let notes = mount_notes_for(&request, &plan);
        assert!(matches!(
            notes.as_slice(),
            [MountNote::Floppies { count: 2 }]
        ));
    }

    #[test]
    fn mount_notes_state_whdload_one_click() {
        let request = whdload_args(
            Path::new(r"E:\amikit\AmiKit.hdf"),
            Path::new(r"D:\Turrican"),
        );
        let plan = plan_with(LaunchKind::Whdload {
            drawer: r"D:\Turrican".into(),
            slave: "Turrican.slave".into(),
            system: r"E:\amikit\AmiKit.hdf".into(),
            one_click: true,
        });

        let notes = mount_notes_for(&request, &plan);
        assert!(matches!(
            notes.as_slice(),
            [MountNote::Whdload { one_click: true }]
        ));
    }

    /// The frontend's `MountNote` union (`src/lib/launch.ts`) must match this
    /// exactly, the same discipline `the_wire_shape_is_what_the_frontend_reads`
    /// applies in `core/launch/mod.rs`.
    #[test]
    fn mount_note_wire_shape_is_what_the_frontend_reads() {
        assert_eq!(
            serde_json::to_value(MountNote::Floppies { count: 2 }).unwrap(),
            serde_json::json!({ "kind": "floppies", "count": 2 })
        );
        assert_eq!(
            serde_json::to_value(MountNote::Hardfile { read_only: true }).unwrap(),
            serde_json::json!({ "kind": "hardfile", "read_only": true })
        );
        assert_eq!(
            serde_json::to_value(MountNote::Whdload { one_click: false }).unwrap(),
            serde_json::json!({ "kind": "whdload", "one_click": false })
        );
    }

    fn args(media: Media) -> LaunchArgs {
        LaunchArgs {
            id: "id".into(),
            title: "A Title".into(),
            path: r"D:\games\Title.rp9".into(),
            media,
            chipset: None,
            rom_dir: r"D:\roms".into(),
            default_machine: Machine::A500,
            machine_override: None,
            system_volume: None,
            one_click: true,
        }
    }

    /// A `.rp9`'s floppy entries arrive as `RequestKind::Floppies` with the
    /// entry names untouched — turning them into real paths is
    /// `launch_title`'s job, not this mapping's.
    #[test]
    fn media_floppies_becomes_request_kind_floppies_with_entries_as_is() {
        let a = args(Media::Floppies {
            ordered: vec!["Disk1.adf".into(), "Disk2.adf".into()],
        });
        match request_kind_from(&a) {
            RequestKind::Floppies { images } => {
                assert_eq!(
                    images,
                    vec!["Disk1.adf".to_string(), "Disk2.adf".to_string()]
                )
            }
            other => panic!("{other:?}"),
        }
    }

    /// ART-141. `Media::Hardfile { file }`'s `file` is carried through as
    /// given — it is a `.rp9`'s zip entry name when the catalogued file is a
    /// `.rp9`, and `media_for_plan` (tested below) is what decides whether
    /// that needs extracting. This mapping must not silently swap it for
    /// `args.path`, which is what made the old code mount the zip itself.
    #[test]
    fn media_hardfile_carries_the_records_file_field_untouched() {
        let a = args(Media::Hardfile {
            file: "af-application.hdf".into(),
        });
        match request_kind_from(&a) {
            RequestKind::Hardfile { image } => assert_eq!(image, "af-application.hdf"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn media_whdload_drawer_uses_the_catalogued_path_and_slave() {
        let a = args(Media::WhdloadDrawer {
            slave: "Turrican.slave".into(),
        });
        match request_kind_from(&a) {
            RequestKind::Whdload { drawer, slave } => {
                assert_eq!(drawer, r"D:\games\Title.rp9");
                assert_eq!(slave, "Turrican.slave");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn rp9_extension_is_recognised_case_insensitively() {
        assert!(is_rp9("Dune2.rp9"));
        assert!(is_rp9("Dune2.RP9"));
        assert!(!is_rp9("Dune2.adf"));
        assert!(!is_rp9("Dune2"));
    }

    /// A path that no longer exists is the same kind of refusal `plan_for`
    /// itself raises — the reason `LaunchRefusal` carries `FileMissing` at
    /// all (its own doc comment).
    #[test]
    fn a_missing_path_refuses_with_file_missing() {
        let err = require_exists(r"D:\definitely\not\here.adf").unwrap_err();
        assert!(matches!(err, CoreError::InvalidInput(_)));
        assert!(format!("{err}").contains("no longer exists"));
    }

    // ---- media_for_plan: the highest-risk function in this module --------
    //
    // Device names, read-only flags, boot priorities, and which `.rp9`
    // branch to take. `media_for_plan` takes plain paths rather than an
    // `AppHandle` for exactly this reason: none of the cases below need a
    // running Tauri app.

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("art-launch-cmd-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A `.rp9`-shaped zip, the same fixture shape `core/launch/extract.rs`'s
    /// own tests build.
    fn zip_package(dir: &Path, name: &str, entries: &[(&str, &[u8])]) -> PathBuf {
        use std::io::Write;
        let path = dir.join(name);
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for (entry, bytes) in entries {
            zip.start_file(*entry, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    fn plan_with(kind: LaunchKind) -> LaunchPlan {
        LaunchPlan {
            machine: Machine::A1200,
            rom: LaunchRom {
                name: "Kickstart 3.1".into(),
                models: vec!["A1200".into()],
                path: r"D:\roms\kick.rom".into(),
            },
            kind,
            notes: vec![],
        }
    }

    /// A bare `.adf` mounts directly — nothing to unpack, and the plan's
    /// `images` entry (the file's own name) is not even consulted.
    #[test]
    fn a_plain_floppy_mounts_the_catalogued_path_directly() {
        let dir = scratch("floppy-plain");
        let adf = dir.join("Game.adf");
        std::fs::write(&adf, b"DISK").unwrap();

        let request = LaunchArgs {
            path: adf.to_string_lossy().to_string(),
            media: Media::Floppies {
                ordered: vec!["Game.adf".into()],
            },
            ..args(Media::Floppies {
                ordered: vec!["Game.adf".into()],
            })
        };
        let plan = plan_with(LaunchKind::Floppies {
            images: vec!["Game.adf".into()],
        });

        let media =
            media_for_plan(&request, &plan, &dir.join("launch"), &dir.join("boot")).unwrap();
        assert_eq!(media.floppy_paths, vec![adf.to_string_lossy().to_string()]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A `.rp9`'s floppies are zip entries and have to be unpacked before
    /// WinUAE can mount them.
    #[test]
    fn rp9_floppies_are_extracted_into_the_launch_directory() {
        let dir = scratch("floppy-rp9");
        let pkg = zip_package(
            &dir,
            "Dune2.rp9",
            &[("a.adf", b"FIRST"), ("b.adf", b"SECOND")],
        );

        let request = LaunchArgs {
            path: pkg.to_string_lossy().to_string(),
            ..args(Media::Floppies {
                ordered: vec!["a.adf".into(), "b.adf".into()],
            })
        };
        let plan = plan_with(LaunchKind::Floppies {
            images: vec!["a.adf".into(), "b.adf".into()],
        });

        let launch_dir = dir.join("launch");
        let media = media_for_plan(&request, &plan, &launch_dir, &dir.join("boot")).unwrap();

        assert_eq!(media.floppy_paths.len(), 2);
        assert_eq!(std::fs::read(&media.floppy_paths[0]).unwrap(), b"FIRST");
        assert_eq!(std::fs::read(&media.floppy_paths[1]).unwrap(), b"SECOND");
        for p in &media.floppy_paths {
            assert!(
                Path::new(p).starts_with(&launch_dir),
                "{p} should be under the launch directory, not the package"
            );
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A plain hardfile — Enzo's collection — mounts directly, but
    /// **read-only**: it is the user's own original file, and CHANGELOG,
    /// FEATURES and spec §93 all say a hardfile mounts read-only. Named for
    /// what it must do, not for the defect it used to pin — this test used
    /// to assert the opposite and passed against the bug (the Critical
    /// finding from the whole-branch review).
    #[test]
    fn a_plain_hardfile_mounts_the_catalogued_path_directly_and_read_only() {
        let dir = scratch("hardfile-plain");
        let hdf = dir.join("Game.hdf");
        std::fs::write(&hdf, b"HARDFILE").unwrap();

        let request = LaunchArgs {
            path: hdf.to_string_lossy().to_string(),
            ..args(Media::Hardfile {
                file: "Game.hdf".into(),
            })
        };
        let plan = plan_with(LaunchKind::Hardfile {
            image: "Game.hdf".into(),
        });

        let media =
            media_for_plan(&request, &plan, &dir.join("launch"), &dir.join("boot")).unwrap();

        assert_eq!(
            media.hardfile_paths,
            vec![hdf.to_string_lossy().to_string()]
        );
        assert!(
            media.write_protect_hardfiles,
            "the user's own original hardfile must mount read-only (spec §93)"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **ART-141, the Critical fix.** A `.rp9`'s hardfile is a named zip
    /// entry, not the package itself — this test fails against the code that
    /// mounted `request.path` (the `.rp9`) unconditionally, and passes once
    /// the `Hardfile` arm has the same `is_rp9` branch the `Floppies` arm
    /// already had.
    #[test]
    fn an_rp9_hardfile_is_extracted_not_the_package_itself() {
        let dir = scratch("hardfile-rp9");
        let pkg = zip_package(
            &dir,
            "Enzo.rp9",
            &[("af-application.hdf", b"REAL-HARDFILE-BYTES")],
        );

        let request = LaunchArgs {
            path: pkg.to_string_lossy().to_string(),
            ..args(Media::Hardfile {
                file: "af-application.hdf".into(),
            })
        };
        let plan = plan_with(LaunchKind::Hardfile {
            image: "af-application.hdf".into(),
        });

        let launch_dir = dir.join("launch");
        let media = media_for_plan(&request, &plan, &launch_dir, &dir.join("boot")).unwrap();

        assert_eq!(media.hardfile_paths.len(), 1);
        assert_ne!(
            media.hardfile_paths[0],
            pkg.to_string_lossy().to_string(),
            "must mount the extracted image, not the .rp9 package"
        );
        assert!(Path::new(&media.hardfile_paths[0]).starts_with(&launch_dir));
        assert_eq!(
            std::fs::read(&media.hardfile_paths[0]).unwrap(),
            b"REAL-HARDFILE-BYTES"
        );
        assert!(
            !media.write_protect_hardfiles,
            "the extracted copy is ART's own, under its launch directory, not the user's \
             original — it stays writable so saves inside it can persist"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn whdload_args(system: &Path, drawer: &Path) -> LaunchArgs {
        LaunchArgs {
            path: drawer.to_string_lossy().to_string(),
            system_volume: Some(system.to_string_lossy().to_string()),
            ..args(Media::WhdloadDrawer {
                slave: "Turrican.slave".into(),
            })
        }
    }

    /// Y2: one click. The system is read-only, the game drawer is writable,
    /// and ART's own boot directory — which is what makes the click "one" —
    /// outranks both.
    #[test]
    fn whdload_one_click_writes_the_boot_directory_and_outranks_everything() {
        let dir = scratch("whdload-y2");
        let system = dir.join("System.hdf");
        std::fs::write(&system, b"SYSTEM").unwrap();
        let drawer = dir.join("Turrican");
        std::fs::create_dir_all(&drawer).unwrap();

        let request = whdload_args(&system, &drawer);
        let plan = plan_with(LaunchKind::Whdload {
            drawer: drawer.to_string_lossy().to_string(),
            slave: "Turrican.slave".into(),
            system: system.to_string_lossy().to_string(),
            one_click: true,
        });

        let boot_dir = dir.join("boot");
        let media = media_for_plan(&request, &plan, &dir.join("launch"), &boot_dir).unwrap();

        assert_eq!(
            media.hardfile_paths,
            vec![system.to_string_lossy().to_string()]
        );
        assert!(
            media.write_protect_hardfiles,
            "the user's own system must stay read-only"
        );

        assert_eq!(media.directories.len(), 2);
        let game = media
            .directories
            .iter()
            .find(|d| d.label == "Game")
            .unwrap();
        assert_eq!(game.host_path, drawer.to_string_lossy().to_string());
        assert!(!game.read_only, "WHDLoad keeps save games in the drawer");

        let boot = media
            .directories
            .iter()
            .find(|d| d.label == "ARTBoot")
            .unwrap();
        assert_eq!(boot.host_path, boot_dir.to_string_lossy().to_string());
        assert!(
            boot.boot_priority > game.boot_priority,
            "the boot directory must outrank the game drawer"
        );
        assert!(
            boot.boot_priority > 0,
            "and outrank the system hardfile too (priority 0 by construction)"
        );

        let startup = boot_dir.join("S").join("Startup-Sequence");
        assert!(startup.is_file(), "Y2 must write ART's own boot directory");
        assert!(std::fs::read_to_string(startup)
            .unwrap()
            .contains("Turrican.slave"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Y1: mount and hand over. No boot directory — the system hardfile
    /// itself boots to Workbench, and the user starts WHDLoad by hand.
    #[test]
    fn whdload_mount_and_hand_over_writes_no_boot_directory() {
        let dir = scratch("whdload-y1");
        let system = dir.join("System.hdf");
        std::fs::write(&system, b"SYSTEM").unwrap();
        let drawer = dir.join("Turrican");
        std::fs::create_dir_all(&drawer).unwrap();

        let request = whdload_args(&system, &drawer);
        let plan = plan_with(LaunchKind::Whdload {
            drawer: drawer.to_string_lossy().to_string(),
            slave: "Turrican.slave".into(),
            system: system.to_string_lossy().to_string(),
            one_click: false,
        });

        let boot_dir = dir.join("boot");
        let media = media_for_plan(&request, &plan, &dir.join("launch"), &boot_dir).unwrap();

        assert_eq!(
            media.hardfile_paths,
            vec![system.to_string_lossy().to_string()]
        );
        assert!(media.write_protect_hardfiles);

        assert_eq!(
            media.directories.len(),
            1,
            "only the game drawer, no ARTBoot"
        );
        assert_eq!(media.directories[0].label, "Game");
        assert!(!media.directories[0].read_only);

        assert!(
            !boot_dir.join("S").join("Startup-Sequence").is_file(),
            "Y1 must not write ART's own boot directory at all"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A vanished file — the drawer, the system image, an unpacked disk —
    /// refuses rather than handing WinUAE a path that is not there.
    #[test]
    fn a_missing_whdload_system_refuses() {
        let dir = scratch("whdload-missing-system");
        let drawer = dir.join("Turrican");
        std::fs::create_dir_all(&drawer).unwrap();

        let request = whdload_args(&dir.join("nope.hdf"), &drawer);
        let plan = plan_with(LaunchKind::Whdload {
            drawer: drawer.to_string_lossy().to_string(),
            slave: "Turrican.slave".into(),
            system: dir.join("nope.hdf").to_string_lossy().to_string(),
            one_click: true,
        });

        let err =
            media_for_plan(&request, &plan, &dir.join("launch"), &dir.join("boot")).unwrap_err();
        assert!(matches!(err, CoreError::InvalidInput(_)));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
