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
use crate::core::launch::extract::unpack_floppies;
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
    pub system_volume: Option<String>,
    pub one_click: bool,
}

/// `Media` → the shape `core/launch::plan_for` reads.
///
/// A `Floppies` whose file is a `.rp9` still arrives as
/// [`RequestKind::Floppies`], with the entry names exactly as the catalogue
/// holds them — they are archive entry names, not host paths, when the
/// catalogued file is a `.rp9` (`from_rp9` in `core/gameindex/scan.rs`
/// reads them out of `<floppy priority="n">`), and are the file's own single
/// name otherwise (`read_one` in the same module, for a bare `.adf`/`.img`).
/// [`launch_title_inner`] is what turns either shape into real paths on
/// disk; the preview shows the disk names the user recognises rather than a
/// temporary directory they have never seen.
fn request_kind_from(args: &LaunchArgs) -> RequestKind {
    match &args.media {
        Media::Floppies { ordered } => RequestKind::Floppies {
            images: ordered.clone(),
        },
        Media::Hardfile { .. } => RequestKind::Hardfile {
            image: args.path.clone(),
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

    let machine = machine_for(
        chipset_from(request.chipset.as_deref()),
        request.default_machine,
    );
    let plan = plan_for(&LaunchRequest {
        machine,
        roms: &roms,
        kind: request_kind_from(&request),
        system_volume: request.system_volume.clone(),
        one_click: request.one_click,
    });

    Ok(match plan {
        Ok(plan) => LaunchPreview {
            plan: Some(plan),
            refusal: None,
        },
        Err(refusal) => LaunchPreview {
            plan: None,
            refusal: Some(refusal),
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
fn media_for_plan(
    app: &AppHandle,
    request: &LaunchArgs,
    plan: &LaunchPlan,
) -> Result<LaunchMedia, CoreError> {
    let mut media = LaunchMedia {
        kickstart_path: Some(plan.rom.path.clone()),
        ..Default::default()
    };

    match &plan.kind {
        LaunchKind::Floppies { images } => {
            let paths: Vec<String> = if is_rp9(&request.path) {
                require_exists(&request.path)?;
                unpack_floppies(
                    Path::new(&request.path),
                    images,
                    &launch_dir_for(app, &request.id),
                )?
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
            require_exists(image)?;
            media.hardfile_paths = vec![image.clone()];
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
                let boot_dir = boot_dir_for(app);
                write_boot_dir(&boot_dir, slave, "DH0", "DH1")?;
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

fn launch_title_inner(request: &LaunchArgs, app: &AppHandle) -> Result<u32, CoreError> {
    let roms: Vec<LaunchRom> = scan_rom_directory(Path::new(&request.rom_dir))
        .unwrap_or_default()
        .iter()
        .map(launch_rom_from)
        .collect();

    let machine = machine_for(
        chipset_from(request.chipset.as_deref()),
        request.default_machine,
    );
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

    let media = media_for_plan(app, request, &plan)?;
    let profile = profile_for(plan.machine);
    let config_text = generate_uae_config(&profile, &media)?;

    let install = detect_winuae(None);
    let exe = install.executable_path.ok_or_else(|| {
        CoreError::InvalidInput(
            "WinUAE was not found in a standard install location — set its path in Settings"
                .to_string(),
        )
    })?;

    launch_winuae(Path::new(&exe), &config_text)
}

/// Launch a catalogued title. Unpacks a `.rp9`'s disks, writes the WHDLoad
/// boot directory when Y2 is asked for, then starts WinUAE — and logs the
/// result either way (§53: an external process against the user's own files
/// is exactly what the operation log is for).
#[tauri::command]
pub fn launch_title(
    request: LaunchArgs,
    app: AppHandle,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<u32> {
    let result: AppResult<u32> = launch_title_inner(&request, &app).map_err(AppError::from);

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

    fn args(media: Media) -> LaunchArgs {
        LaunchArgs {
            id: "id".into(),
            title: "A Title".into(),
            path: r"D:\games\Title.rp9".into(),
            media,
            chipset: None,
            rom_dir: r"D:\roms".into(),
            default_machine: Machine::A500,
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

    /// A hardfile record's launch path is the catalogued file itself — the
    /// `file` field inside `Media::Hardfile` names the packaged entry, but
    /// there is only ever one file for this kind of title.
    #[test]
    fn media_hardfile_uses_the_catalogued_path() {
        let a = args(Media::Hardfile {
            file: "Game.hdf".into(),
        });
        match request_kind_from(&a) {
            RequestKind::Hardfile { image } => assert_eq!(image, r"D:\games\Title.rp9"),
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
}
