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
use tauri::{AppHandle, State};

use super::oplog::{user_operation, write_result};
use crate::core::error::CoreError;
use crate::core::gameindex::record::Media;
use crate::core::hdf::detect_hardfile_shape;
use crate::core::launch::extract::{unpack_floppies, unpack_hardfile};
use crate::core::launch::whdload_boot::write_boot_dir;
use crate::core::launch::{
    is_whdload_shaped, machine_for_request, plan_for, Chipset, LaunchKind, LaunchPlan,
    LaunchRefusal, LaunchRequest, LaunchRom, Machine, RequestKind, DEFAULT_WHDLOAD_FAST_RAM_MB,
    WHDLOAD_PROFILE_MACHINE,
};
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::core::profile::{AmigaProfile, MemoryConfig};
use crate::core::rom::{scan_rom_directory, RomInfo};
use crate::core::winuae::{
    detect_winuae, generate_uae_config, launch_winuae, DirMount, LaunchMedia,
};
use crate::error::{AppError, AppResult};

/// `RomInfo` → the three fields `core/launch` reads. The lower module must
/// not know the higher one's type; this is where the translation lives.
fn launch_rom_from(info: &RomInfo) -> LaunchRom {
    LaunchRom {
        name: info.name.clone(),
        models: info.compatible_models.clone(),
        path: info.file_path.clone(),
        major: info.major,
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
    /// The user's explicit opt-in to mount a `Media::WhdloadHardfile` image
    /// writable, so a save the game makes survives. Ignored by every other
    /// media kind. Defaults to `false` on the frontend
    /// (`TitleDetail.tsx`'s `launch.allowWrite.<id>`, remembered per title) —
    /// §93's "originals are immutable by default" stands, and this field is
    /// how the user is asked to override it for *this* title, having been
    /// told what turning it on means (`MountNote::WhdloadHardfile`).
    /// `#[serde(default)]`: a screen still running the previous build's
    /// bundled frontend against a rebuilt backend sends no such field, and
    /// the safe default — read-only, §93 — must be what it gets.
    #[serde(default)]
    pub allow_write: bool,
    /// Fast RAM, in MB, added to a WHDLoad launch's profile — ART-151.
    /// Ignored for anything that is not WHDLoad-shaped
    /// (`core::launch::is_whdload_shaped`), the same predicate that decides
    /// [`crate::core::launch::WHDLOAD_MIN_KICKSTART_MAJOR`]'s Kickstart
    /// floor. From Settings (`launch.whdloadFastRamMb`), guarded by
    /// `isWholeNumberBetween` on the frontend so a hand-edited or stale
    /// settings file falls back to the default instead of putting a
    /// nonsense value in a launch. `#[serde(default = ...)]`: a screen still
    /// running an older bundled frontend against a rebuilt backend sends no
    /// such field, and the safe default — WHDLoad's own headroom, not zero —
    /// is what it gets, the same shape `allow_write`'s own default takes
    /// just above.
    #[serde(default = "default_whdload_fast_ram_mb")]
    pub whdload_fast_ram_mb: u32,
}

/// [`LaunchArgs::whdload_fast_ram_mb`]'s serde default — see that field and
/// [`DEFAULT_WHDLOAD_FAST_RAM_MB`]'s own doc comment for why this number and
/// not zero.
fn default_whdload_fast_ram_mb() -> u32 {
    DEFAULT_WHDLOAD_FAST_RAM_MB
}

/// The machine a launch actually uses: the user's own per-title choice when
/// there is one, otherwise [`machine_for_request`] — which is
/// [`crate::core::launch::WHDLOAD_PROFILE_MACHINE`] for a WHDLoad-shaped
/// request (ART-152) and `machine_for`'s inference from the catalogue's
/// chipset and the user's default for everything else.
/// `machine_override` is never folded into
/// `default_machine` before this call — doing that once made the per-title
/// picker inert for any title with a stated chipset, because `machine_for`
/// only consults its `default` argument when the chipset is `None`. It stays
/// outside the WHDLoad rule for the same reason and a stronger one: ART-152
/// is a default ART chose, and this is the user saying otherwise for one
/// title.
fn resolved_machine(request: &LaunchArgs) -> Machine {
    request.machine_override.unwrap_or_else(|| {
        machine_for_request(
            &request_kind_from(request),
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
/// `Media::WhdloadHardfile` takes the **same** `RequestKind::Hardfile` path,
/// not `RequestKind::Whdload` — that was ART-147. The image boots itself
/// (`core::gameindex::readers::whdhdf`'s own header), so there is no system
/// volume to mount alongside it and no boot directory for ART to write; the
/// slave's name is a fact carried on the record, not something a launch needs
/// to act on.
///
/// `Media::WhdloadDrawer` is the shape `RequestKind::Whdload` was actually
/// built for: an already-unpacked drawer that needs a separate bootable
/// system, the same shape `core::whdload` installs onto a card. `dir` maps
/// straight onto `drawer` — the system volume and Y1/Y2 choice are not part
/// of `Media` at all, they are `args.system_volume` / `args.one_click`, and
/// `plan_for` folds them in.
///
/// `Media::WhdloadArchive` reaches **neither** `RequestKind::Whdload` nor
/// `RequestKind::Hardfile`: `RequestKind::Whdload` needs a directory on a
/// filesystem and this is a path inside a compressed file ART has not
/// unpacked, so it is refused before this function is ever called
/// (`archived_refusal`, checked by every caller of this function that can
/// actually start a launch). The arm below exists only so this match stays
/// exhaustive — it is unreachable in the real flow, and even if some future
/// caller skipped the check, an empty `Floppies` set is `plan_for`'s own
/// `NothingToMount` refusal rather than anything that could be mistaken for
/// a launch. **Never give this arm a shape that could pass for real media** —
/// that is ART-147, repeated for a different `Media` variant.
///
/// **`whdload` on the request is not the same field it looks like.** It is
/// `RequestKind::Hardfile::whdload` — whether `core::launch::plan_for` must
/// enforce WHDLoad's own Kickstart floor (ART-148) — set from *which `Media`
/// variant this is*, never from anything the catalogue's `KickstartNeed`
/// states. A WHDLoad slave's own declared Kickstart (`kick34005.A500`, say)
/// is the ROM *WHDLoad itself* loads for the game once the machine is
/// already running; it says nothing about what floor the machine's own boot
/// ROM needs, and this mapping must never read it as if it did.
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
            whdload: false,
        },
        Media::WhdloadHardfile { file, .. } => RequestKind::Hardfile {
            image: file.clone(),
            whdload: true,
        },
        Media::WhdloadDrawer { dir, slave } => RequestKind::Whdload {
            drawer: dir.clone(),
            slave: slave.clone(),
        },
        Media::WhdloadArchive { .. } => RequestKind::Floppies { images: vec![] },
    }
}

/// Whether this title can be launched at all before anything about ROMs or
/// disks is even considered.
///
/// `Media::WhdloadArchive` is the one shape this catalogue can hold that has
/// no `RequestKind` worth reaching: the drawer is real, but it is a path
/// inside a compressed file, and `RequestKind::Whdload` needs a directory on
/// a filesystem. Checked before [`request_kind_from`] / `plan_for` are ever
/// reached, so an archived title never takes the launchable path — that
/// wrong turn, for a different `Media` variant, is exactly ART-147.
fn archived_refusal(media: &Media) -> Option<LaunchRefusal> {
    match media {
        Media::WhdloadArchive { file, .. } => {
            Some(LaunchRefusal::ArchivedWhdload { file: file.clone() })
        }
        Media::Floppies { .. }
        | Media::Hardfile { .. }
        | Media::WhdloadHardfile { .. }
        | Media::WhdloadDrawer { .. } => None,
    }
}

/// The four numbers `generate_uae_config` actually writes into the WinUAE
/// config (`core::winuae`'s `fastmem_size=` and friends) — mirrors
/// `core::profile::MemoryConfig` rather than handing the frontend the whole
/// `AmigaProfile` (CPU, chipset, display, ROM hash…) just to show four
/// numbers on the confirmation screen.
#[derive(Debug, Clone, Serialize)]
pub struct MemorySummary {
    pub chip_kb: u32,
    pub slow_kb: u32,
    pub fast_mb: u32,
    pub z3_fast_mb: u32,
}

impl From<&MemoryConfig> for MemorySummary {
    fn from(memory: &MemoryConfig) -> Self {
        Self {
            chip_kb: memory.chip_kb,
            slow_kb: memory.slow_kb,
            fast_mb: memory.fast_mb,
            z3_fast_mb: memory.z3_fast_mb,
        }
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
    /// The memory the planned machine will actually have — `None` on a
    /// refusal, since nothing is going to be tried. ART-151: DOS-Error #103
    /// ("not enough memory available") is exactly this number falling short,
    /// discovered only after WHDLoad itself refused — the confirmation
    /// screen states it beside the machine and the ROM so the user can see
    /// what will be tried before pressing Start, rather than learn it from
    /// WHDLoad's own error screen a second time.
    pub memory: Option<MemorySummary>,
}

/// Work out what a launch would need. Starts nothing, reads no media —
/// only the ROM folder is scanned, which is what a Kickstart choice needs.
///
/// Takes `roms` and `request` rather than an `AppHandle` for the same reason
/// `media_for_plan` does (that function's own doc comment): this is exactly
/// the logic worth exercising directly in a test, without a running Tauri
/// app to produce one.
fn preview_for(request: &LaunchArgs, roms: &[LaunchRom]) -> LaunchPreview {
    if let Some(refusal) = archived_refusal(&request.media) {
        return LaunchPreview {
            plan: None,
            refusal: Some(refusal),
            mounts: vec![],
            memory: None,
        };
    }
    let machine = resolved_machine(request);
    let kind = request_kind_from(request);
    let plan = plan_for(&LaunchRequest {
        machine,
        roms,
        kind: kind.clone(),
        system_volume: request.system_volume.clone(),
        one_click: request.one_click,
    });

    match plan {
        Ok(plan) => {
            let mounts = mount_notes_for(request, &plan);
            let profile = profile_for_request(&kind, plan.machine, request.whdload_fast_ram_mb);
            LaunchPreview {
                plan: Some(plan),
                refusal: None,
                mounts,
                memory: Some(MemorySummary::from(&profile.memory)),
            }
        }
        Err(refusal) => LaunchPreview {
            plan: None,
            refusal: Some(refusal),
            mounts: vec![],
            memory: None,
        },
    }
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

    Ok(preview_for(&request, &roms))
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
        LaunchRefusal::NoRomMeetsWhdloadMinimum { machine } => {
            format!(
                "this WHDLoad title needs Kickstart 2.0 or newer, and the ROM folder holds \
                 none for {machine:?} that meet it"
            )
        }
        LaunchRefusal::NoWhdloadMachineRom { machine } => {
            let name = format!("{machine:?}").to_uppercase();
            format!(
                "WHDLoad titles run on ART's {name} profile, so this launch needs an \
                 {name} Kickstart 3.x. A Kickstart for another model — a 3.1 for the \
                 A500/A600/A2000, say — does not suit it however new it is. Add an \
                 {name} Kickstart to the ROM folder"
            )
        }
        LaunchRefusal::NoSystemVolume => {
            "no bootable system volume is configured for this WHDLoad title".to_string()
        }
        LaunchRefusal::FileMissing { path } => format!("'{path}' no longer exists"),
        LaunchRefusal::NothingToMount => {
            "this title's media names no disk to mount — there is nothing for WinUAE to load"
                .to_string()
        }
        LaunchRefusal::ArchivedWhdload { file } => format!(
            "'{file}' is a WHDLoad drawer inside an archive ART has not unpacked — \
             unpack it first, then try again"
        ),
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

/// Whether a `Hardfile`-shaped title's image mounts read-only.
///
/// A bare `.hdf` is the user's own original file — read-only by default
/// (CHANGELOG, FEATURES, spec §93). A `.rp9`'s hardfile is unpacked into
/// ART's own launch directory and is ART's copy, not the user's original, so
/// it stays writable so its saves can persist (`unpack_hardfile`'s reuse).
///
/// A `Media::WhdloadHardfile` is the one exception to "read-only unless it's
/// ART's own copy" (ART-147): it is the user's own file, but WHDLoad writes
/// saved games back into the image it boots from, so mounting it read-only
/// by default silently loses them. §93 still stands — the default here is
/// still `true` — but `request.allow_write` is the user's own explicit
/// opt-in for *this* title, made after the confirmation screen told them
/// plainly what leaving it off means (`MountNote::WhdloadHardfile`). Turning
/// it on is never inferred from anything else.
///
/// Shared by [`media_for_plan`], which acts on it, and [`mount_notes_for`],
/// which states it on the confirmation screen before anything is mounted —
/// the two must never compute a different answer from each other.
fn hardfile_write_protected(request: &LaunchArgs) -> bool {
    if is_rp9(&request.path) {
        return false;
    }
    if matches!(request.media, Media::WhdloadHardfile { .. }) {
        return !request.allow_write;
    }
    true
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
    /// A self-booting WHDLoad hardfile (ART-147): distinct from the plain
    /// `Hardfile` note so the confirmation screen can say, plainly, that a
    /// game's own saves are lost while `read_only` holds — the collision
    /// between §93 (originals immutable by default) and WHDLoad writing
    /// saves back into the very image it boots from, resolved by telling the
    /// user rather than picking silently either way.
    WhdloadHardfile {
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
        LaunchKind::Hardfile { .. } => {
            let read_only = hardfile_write_protected(request);
            if matches!(request.media, Media::WhdloadHardfile { .. }) {
                vec![MountNote::WhdloadHardfile { read_only }]
            } else {
                vec![MountNote::Hardfile { read_only }]
            }
        }
        LaunchKind::Whdload { one_click, .. } => vec![MountNote::Whdload {
            one_click: *one_click,
        }],
    }
}

/// Where a `.rp9`'s disks are unpacked to for one launch (Task 8).
///
/// Under the scratch root, not the application data directory (ART-196):
/// unpacked disks are thrown away and rewritten every launch, and on Windows
/// `app_data_dir` is on the system drive whatever the user would prefer. `app`
/// is still taken so the signature does not change shape when a future launch
/// wants something that genuinely is application data.
fn launch_dir_for(_app: &AppHandle, scratch_root: &Path, id: &str) -> PathBuf {
    scratch_root.join("art-launch").join(id)
}

/// The one boot directory ART owns for a one-click WHDLoad launch (Task 10).
///
/// Not per-title: it is rewritten fresh before every Y2 launch from whatever
/// slave, system and game device names that launch needs, so there is
/// nothing in it worth keeping between titles.
fn boot_dir_for(_app: &AppHandle, scratch_root: &Path) -> PathBuf {
    scratch_root.join("art-launch").join("boot")
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

/// The profile a plan will actually run with — [`profile_for`]'s stock
/// preset, with a WHDLoad launch's fast-RAM headroom
/// ([`DEFAULT_WHDLOAD_FAST_RAM_MB`], or whatever Settings has raised it to)
/// folded in. ART-151: a WHDLoad launch on `Machine::A500` used to get
/// `AmigaProfile::a500_ocs` completely unmodified — 512 KB Chip, 512 KB Slow,
/// no Fast RAM at all — which is exactly the memory DOS-Error #103 measured
/// as too small. Never applied to a floppy or a plain (non-WHDLoad) hardfile
/// — [`is_whdload_shaped`] is the same predicate [`plan_for`] already reads
/// for the Kickstart floor, so a title never becomes WHDLoad-shaped for one
/// purpose and not the other.
///
/// **Never mutates a shared preset.** [`profile_for`] returns a fresh
/// [`AmigaProfile`] on every call — `AmigaProfile::a500_ocs()` /
/// `a1200_aga()` build a new struct each time rather than handing back a
/// shared instance — so raising `.memory.fast_mb` here only ever changes the
/// copy this one launch is about to use. `core/profile.rs`'s presets, and
/// every other screen that reads them (the Profile Studio among them), are
/// untouched — CLAUDE.md is explicit that a WHDLoad launch "should adjust
/// the memory of the profile it plans with, not redefine what an A500 is".
///
/// **The user's number is used exactly, including when it is lower.** This
/// was `.max(profile.memory.fast_mb)` until the ART-152 review: the profile's
/// own 8 MB acted as a floor, so a user who opened Settings and *lowered*
/// Fast RAM watched the number change and nothing happen, with nothing said.
/// That is "nothing changes unless the user changes it" broken from the
/// other side — the user changed it and ART overruled them in silence — and
/// a control that cannot lower is worse than no control, because it claims
/// to do something it does not.
///
/// **There is no floor left to defend.** WHDLoad's only documented memory
/// requirement is a *total* one — "a minimum of 1.0 MiB RAM"
/// (<https://www.whdload.de/docs/en/need.html>) — and
/// `AmigaProfile::whdload_a1200`'s 2 MB of Chip RAM meets it twice over
/// before a single MB of Fast RAM is added. Nothing on that page states a
/// Fast RAM minimum at all, so the 8 MB is a *default*, not a floor, and
/// clamping to it was defending ART's own preference against the user's
/// explicit instruction. Setting it to 0 is a supported choice; a title that
/// then runs out of memory says so through WHDLoad's own DOS-Error #103, and
/// the confirmation screen states the memory (`LaunchPreview::memory`)
/// before anything starts, so the consequence is visible in advance rather
/// than clamped away behind the user's back.
///
/// **ART-152: which profile, not just how much memory.** A WHDLoad-shaped
/// request on [`crate::core::launch::WHDLOAD_PROFILE_MACHINE`] gets
/// `AmigaProfile::whdload_a1200` — the named, documented launch profile —
/// rather than the Profile Studio's A1200 machine preset, so editing that
/// preset cannot move what WHDLoad launches on. The `machine == A1200` guard
/// is not redundant with `is_whdload_shaped`: the user's per-title picker
/// (`LaunchArgs::machine_override`) can still put a WHDLoad title back on an
/// A500, and when they do they must get the A500 they asked for — the stock
/// preset plus ART-151's Fast RAM headroom, exactly the behaviour that
/// measured `1000 Miglia` past DOS-Error #103 — not an AGA machine wearing
/// an A500 label.
fn profile_for_request(kind: &RequestKind, machine: Machine, fast_ram_mb: u32) -> AmigaProfile {
    let whdload = is_whdload_shaped(kind);
    let mut profile = if whdload && machine == WHDLOAD_PROFILE_MACHINE {
        AmigaProfile::whdload_a1200()
    } else {
        profile_for(machine)
    };
    if whdload {
        profile.memory.fast_mb = fast_ram_mb;
    }
    profile
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
            // - A `Media::WhdloadHardfile` (ART-147) is also the user's own
            //   original, so it defaults read-only same as any other bare
            //   `.hdf` — but it is the one shape where that default silently
            //   loses the game's own saves, since WHDLoad writes them back
            //   into the exact image it boots from. `hardfile_write_protected`
            //   honours `request.allow_write`, the per-title opt-in the
            //   confirmation screen offers after stating what leaving it off
            //   means.
            let real_path = if is_rp9(&request.path) {
                require_exists(&request.path)?;
                // ART's own copy, not the user's original (§93 does not
                // reach it) — writable so it can hold saves, made explicit
                // here rather than left to `LaunchMedia::default()`.
                media.write_protect_hardfiles = false;
                unpack_hardfile(Path::new(&request.path), image, launch_dir)?
                    .to_string_lossy()
                    .to_string()
            } else {
                require_exists(&request.path)?;
                media.write_protect_hardfiles = hardfile_write_protected(request);
                request.path.clone()
            };
            // ART-146: decide the image's shape from the file itself —
            // a bare filesystem image needs WinUAE told its geometry, an
            // RDB (or anything else, including a VHD container like the
            // user's own `AmiKit.hdf`) does not, and forcing it there is
            // what produced "Not a DOS disk in unit 0" against real material.
            let shape = detect_hardfile_shape(Path::new(&real_path))?;
            media.hardfile_paths = vec![real_path];
            media.hardfile_shapes = vec![shape];
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
            //
            // ART-146: it is also the exact image the real run hit —
            // `AmiKit.hdf` is a VHD container, not a bare filesystem image,
            // so it must not be forced through bare-image geometry either.
            let shape = detect_hardfile_shape(Path::new(system))?;
            media.hardfile_paths = vec![system.clone()];
            media.hardfile_shapes = vec![shape];
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
    scratch_root: &Path,
    app: &AppHandle,
) -> Result<u32, CoreError> {
    if let Some(refusal) = archived_refusal(&request.media) {
        return Err(refusal_error(refusal));
    }

    let roms: Vec<LaunchRom> = scan_rom_directory(Path::new(&request.rom_dir))
        .unwrap_or_default()
        .iter()
        .map(launch_rom_from)
        .collect();

    let machine = resolved_machine(request);
    let kind = request_kind_from(request);
    // Computed again, not carried from the preview: the screen may have sat
    // open for a while, and a ROM folder or a file on disk can change under
    // it in the meantime.
    let plan = plan_for(&LaunchRequest {
        machine,
        roms: &roms,
        kind: kind.clone(),
        system_volume: request.system_volume.clone(),
        one_click: request.one_click,
    })
    .map_err(refusal_error)?;

    let launch_dir = launch_dir_for(app, scratch_root, &request.id);
    let boot_dir = boot_dir_for(app, scratch_root);
    let media = media_for_plan(request, &plan, &launch_dir, &boot_dir)?;
    let profile = profile_for_request(&kind, plan.machine, request.whdload_fast_ram_mb);
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

    launch_winuae(Path::new(&exe), &config_text, scratch_root)
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
    let result: AppResult<u32> = crate::scratch::root().and_then(|scratch_root| {
        launch_title_inner(&request, winuae_path.as_deref(), &scratch_root, &app)
            .map_err(AppError::from)
    });

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
    use crate::core::hdf::HardfileShape;
    use crate::core::rom::RomChecksum;

    /// `core/launch` must not know what a `RomInfo` is — that is the
    /// lower-imports-higher mistake `core/rom/pairing.rs` documents. The
    /// translation happens here, which is where this codebase puts it.
    #[test]
    fn a_rom_info_becomes_the_three_fields_the_launcher_reads() {
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
            major: Some(40),
            whdload_crc16: None,
        };

        let rom = launch_rom_from(&info);

        assert_eq!(rom.name, "Kickstart 40.68 (A1200)");
        assert_eq!(rom.models, vec!["A1200".to_string()]);
        assert_eq!(rom.path, r"D:\roms\kick.rom");
        assert_eq!(rom.major, Some(40));
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

    // ---- profile_for_request: ART-151's fast-RAM headroom -----------------
    //
    // `1000 Miglia` reached WHDLoad on a stock `AmigaProfile::a500_ocs` — 512
    // KB Chip, 512 KB Slow, no Fast RAM at all, exactly 1 MB — and WHDLoad
    // itself refused with "DOS-Error #103 (not enough memory available) on
    // loading 1000Miglia.Slave". `docs/ISSUES.md`'s ART-151 entry has the
    // full measurement.

    fn a500_rom() -> LaunchRom {
        LaunchRom {
            name: "Kickstart 3.1 (40.063) A500".into(),
            models: vec!["A500".into()],
            path: r"D:\roms\kick40063.A500".into(),
            major: Some(40),
        }
    }

    /// The Kickstart the ART-152 profile's machine actually needs — an
    /// A1200 3.1 ROM. A WHDLoad launch now plans `Machine::A1200`, so
    /// [`a500_rom`] no longer suits one at all: that is the refusal working,
    /// not a broken fixture.
    fn a1200_rom() -> LaunchRom {
        LaunchRom {
            name: "Kickstart 3.1 (40.068) A1200".into(),
            models: vec!["A1200".into()],
            path: r"D:\roms\kick40068.A1200".into(),
            major: Some(40),
        }
    }

    /// A WHDLoad-shaped hardfile gets the *configured* fast RAM folded into
    /// its profile — 16, not `DEFAULT_WHDLOAD_FAST_RAM_MB`'s 8, so this test
    /// cannot pass by coincidence with the default. `chip_kb`/`slow_kb` stay
    /// exactly `AmigaProfile::a500_ocs`'s own numbers: this fix adds Fast
    /// RAM headroom, it does not touch the Chip RAM the emulated game itself
    /// sees.
    #[test]
    fn a_whdload_hardfile_plans_the_configured_fast_ram() {
        let kind = RequestKind::Hardfile {
            image: "1000 Miglia.hdf".into(),
            whdload: true,
        };
        let profile = profile_for_request(&kind, Machine::A500, 16);

        assert_eq!(profile.memory.fast_mb, 16);
        assert_eq!(profile.memory.chip_kb, 512);
        assert_eq!(profile.memory.slow_kb, 512);
    }

    /// The same predicate covers `RequestKind::Whdload` too — a title that is
    /// WHDLoad-shaped by construction, not by the `whdload` flag on a
    /// hardfile.
    #[test]
    fn a_whdload_drawer_plans_the_configured_fast_ram() {
        let kind = RequestKind::Whdload {
            drawer: r"D:\games\Turrican".into(),
            slave: "Turrican.slave".into(),
        };
        let profile = profile_for_request(&kind, Machine::A500, 16);

        assert_eq!(profile.memory.fast_mb, 16);
    }

    /// A floppy title must not get the WHDLoad headroom silently applied —
    /// it is not WHDLoad-shaped, and a stock A500 profile is exactly what a
    /// bare `.adf` is meant to boot on.
    #[test]
    fn a_floppy_title_does_not_get_whdload_memory_silently_applied() {
        let kind = RequestKind::Floppies {
            images: vec![r"D:\g\a.adf".into()],
        };
        let profile = profile_for_request(&kind, Machine::A500, 16);

        assert_eq!(
            profile.memory.fast_mb, 0,
            "a floppy title must keep the stock A500 profile's own memory"
        );
    }

    /// A plain (non-WHDLoad) hardfile is the same "not held to WHDLoad's own
    /// rules" case `WHDLOAD_MIN_KICKSTART_MAJOR`'s own tests cover — a
    /// hand-installed AmigaOS hardfile has nothing to do with WHDLoad and
    /// must not have its memory grown either.
    #[test]
    fn a_plain_hardfile_does_not_get_whdload_memory_applied() {
        let kind = RequestKind::Hardfile {
            image: "Game.hdf".into(),
            whdload: false,
        };
        let profile = profile_for_request(&kind, Machine::A500, 16);

        assert_eq!(profile.memory.fast_mb, 0);
    }

    /// A value the user lowered is used, not clamped back up to the
    /// profile's own 8 MB. This test asserted the opposite until the ART-152
    /// review: the clamp meant a user could move the Settings slider down,
    /// watch the number change and have ART quietly ignore them. There is no
    /// documented Fast RAM floor to enforce — whdload.de states 1.0 MiB
    /// *total*, which the profile's 2 MB of Chip RAM already meets.
    #[test]
    fn a_configured_value_lower_than_the_profile_is_used_not_clamped() {
        let kind = RequestKind::Hardfile {
            image: "Game.hdf".into(),
            whdload: true,
        };

        assert_eq!(
            profile_for_request(&kind, Machine::A1200, 2).memory.fast_mb,
            2
        );
        // 0 is a supported choice, not a "leave it at the default" sentinel.
        assert_eq!(
            profile_for_request(&kind, Machine::A1200, 0).memory.fast_mb,
            0
        );
    }

    // ---- ART-152: the WHDLoad machine profile, as the .uae actually says --
    //
    // These assert the **generated configuration**, not the profile struct.
    // A test that reads `profile.cpu` proves ART built the record it meant
    // to; only the config text proves WinUAE is told. `cpu_model=` in
    // particular is derived (`CpuModel::M68EC020` → `68020`) and
    // `chipmem_size=` is a unit conversion (2048 KB → 4 × 512 KB), so the
    // struct and the file are genuinely two different claims.

    /// The route `launch_title_inner` takes, minus the media extraction:
    /// resolve the machine, plan, build the profile, generate the config.
    /// Assembled here rather than asserting on `profile_for_request` alone
    /// so the machine decision and the config generation are exercised
    /// together — a profile that never reaches `generate_uae_config` is what
    /// this whole block exists to rule out.
    fn uae_config_for(request: &LaunchArgs, roms: &[LaunchRom]) -> String {
        let machine = resolved_machine(request);
        let kind = request_kind_from(request);
        let plan = plan_for(&LaunchRequest {
            machine,
            roms,
            kind: kind.clone(),
            system_volume: request.system_volume.clone(),
            one_click: request.one_click,
        })
        .expect("these fixtures all supply a suitable ROM");
        let profile = profile_for_request(&kind, plan.machine, request.whdload_fast_ram_mb);
        generate_uae_config(
            &profile,
            &LaunchMedia {
                kickstart_path: Some(plan.rom.path.clone()),
                ..Default::default()
            },
        )
        .expect("a synthetic profile and one ROM path must generate a config")
    }

    /// Whole-line, not substring: `fastmem_size=8` is a prefix of
    /// `fastmem_size=80`, and `contains` would call that a pass.
    fn has_line(config: &str, line: &str) -> bool {
        config.lines().any(|l| l == line)
    }

    /// **The guard this decision stands on.** An OCS title, on a user whose
    /// default machine is the A500 — the exact request that used to produce
    /// a 68000/OCS/512 KB configuration — must now produce the ART-152
    /// profile in the generated `.uae`: 68020, AGA, 2 MB Chip
    /// (`chipmem_size=4`, in WinUAE's 512 KB units) and 8 MB Fast.
    #[test]
    fn a_whdload_launch_of_an_ocs_title_writes_the_68020_2mb_chip_8mb_fast_config() {
        let mut request = args(Media::WhdloadHardfile {
            file: "1000 Miglia.hdf".into(),
            slave: "1000Miglia.Slave".into(),
        });
        request.chipset = Some("ocsecs".into());
        request.default_machine = Machine::A500;

        assert_eq!(resolved_machine(&request), Machine::A1200);

        let config = uae_config_for(&request, &[a1200_rom(), a500_rom()]);

        for expected in [
            // Named, so this cannot pass on the Profile Studio's A1200
            // machine preset, which happens to carry the same hardware
            // today and could be edited tomorrow.
            "# Profile: WHDLoad A1200 (68020, 2MB Chip, 8MB Fast)",
            "cpu_model=68020",
            "cpu_type=68020",
            "chipset=aga",
            "chipmem_size=4",
            "bogomem_size=0",
            "fastmem_size=8",
        ] {
            assert!(has_line(&config, expected), "missing {expected}\n{config}");
        }
    }

    /// The profile is for WHDLoad launches and nothing else. The same OCS
    /// title as a plain floppy set still gets the Amiga its game was written
    /// for — 68000, OCS, no Fast RAM — because that is the machine the game
    /// itself talks to the hardware of.
    #[test]
    fn a_floppy_launch_of_an_ocs_title_still_writes_a_68000_ocs_config() {
        let mut request = args(Media::Floppies {
            ordered: vec!["Disk1.adf".into()],
        });
        request.chipset = Some("ocsecs".into());
        request.default_machine = Machine::A500;

        assert_eq!(resolved_machine(&request), Machine::A500);

        let config = uae_config_for(&request, &[a1200_rom(), a500_rom()]);

        for expected in ["cpu_model=68000", "chipset=ocs", "fastmem_size=0"] {
            assert!(has_line(&config, expected), "missing {expected}\n{config}");
        }
    }

    /// A default ART chose must stay one the user can undo. Putting a
    /// WHDLoad title back on an A500 by hand gives an A500 in the generated
    /// config — with ART-151's Fast RAM headroom still folded in, since that
    /// is what got `1000 Miglia` past DOS-Error #103 on exactly that
    /// machine.
    #[test]
    fn a_per_title_machine_choice_still_beats_the_whdload_profile_in_the_config() {
        let mut request = args(Media::WhdloadHardfile {
            file: "1000 Miglia.hdf".into(),
            slave: "1000Miglia.Slave".into(),
        });
        request.machine_override = Some(Machine::A500);

        assert_eq!(resolved_machine(&request), Machine::A500);

        let config = uae_config_for(&request, &[a1200_rom(), a500_rom()]);

        for expected in [
            "cpu_model=68000",
            "chipset=ocs",
            "chipmem_size=1",
            "fastmem_size=8",
        ] {
            assert!(has_line(&config, expected), "missing {expected}\n{config}");
        }
    }

    /// "Nothing changes unless the user changes it": a Settings value the
    /// user raised must reach the generated config, not be overwritten by
    /// the shipped default. 32 rather than a value the Settings control
    /// itself offers — `WHDLOAD_FAST_RAM_MAX_MB` caps that at 8, WinUAE's
    /// 24-bit `fastmem_size=` ceiling — because every in-range choice either
    /// *is* the default or sits below it, and would pass by coincidence.
    /// What this asserts is the plumbing: whatever number arrives, arrives.
    #[test]
    fn a_raised_settings_value_reaches_the_generated_whdload_config() {
        let mut request = args(Media::WhdloadHardfile {
            file: "Game.hdf".into(),
            slave: "Game.Slave".into(),
        });
        request.whdload_fast_ram_mb = 32;

        let config = uae_config_for(&request, &[a1200_rom()]);

        assert!(has_line(&config, "fastmem_size=32"), "{config}");
        // Raising the memory must not have quietly changed the machine.
        assert!(has_line(&config, "cpu_model=68020"), "{config}");
        assert!(has_line(&config, "chipmem_size=4"), "{config}");
    }

    /// The other direction, in the file WinUAE actually reads: a Settings
    /// value the user *lowered* reaches the generated config unchanged. A
    /// control that silently refuses to go down is worse than no control —
    /// it claims to do something it does not.
    #[test]
    fn a_lowered_settings_value_reaches_the_generated_config() {
        let mut request = args(Media::WhdloadHardfile {
            file: "Game.hdf".into(),
            slave: "Game.Slave".into(),
        });
        request.whdload_fast_ram_mb = 1;

        let config = uae_config_for(&request, &[a1200_rom()]);

        assert!(has_line(&config, "fastmem_size=1"), "{config}");
        // Lowering the memory must not have changed the machine either.
        assert!(has_line(&config, "cpu_model=68020"), "{config}");
        assert!(has_line(&config, "chipmem_size=4"), "{config}");
    }

    /// **The sentence a 40.63-only user gets.** A Kickstart 3.1 rev 40.63 is
    /// a modern, perfectly good ROM that lists `A500`/`A600`/`A2000` and not
    /// `A1200`, so since ART-152 it no longer suits a WHDLoad launch.
    /// Refusing is right; reporting it as "your Kickstart is too old" is not,
    /// because the user holding Kickstart **3.1** then goes looking for a
    /// version that does not exist. The refusal must name the machine and
    /// what suits it.
    #[test]
    fn a_40_63_a500_rom_is_refused_with_the_machine_not_with_too_old() {
        let request = args(Media::WhdloadHardfile {
            file: "Game.hdf".into(),
            slave: "Game.Slave".into(),
        });
        let kick4063 = LaunchRom {
            name: "Kickstart 3.1 (40.063) A500/A600/A2000".into(),
            models: vec!["A500".into(), "A600".into(), "A2000".into()],
            path: r"D:\roms\kick40063.A500".into(),
            major: Some(40),
        };

        let preview = preview_for(&request, std::slice::from_ref(&kick4063));
        assert_eq!(
            preview.refusal,
            Some(LaunchRefusal::NoWhdloadMachineRom {
                machine: Machine::A1200
            }),
            "a 40.63 A500 ROM meets the Kickstart floor — the missing thing is the machine"
        );

        // The sentence itself, not just the variant: this is what the user
        // reads, and it is the half that was wrong.
        let message = refusal_error(preview.refusal.unwrap()).to_string();
        // The whole phrase, not "A1200" and "3.x" found anywhere in the
        // sentence: a message that names the A1200 profile and then asks for
        // "a suitable Kickstart 3.x" tells the user nothing they can act on,
        // and it passed a two-substring check (mutation M15).
        assert!(
            message.contains("needs an A1200 Kickstart 3.x"),
            "the refusal must name the machine and the version together: {message}"
        );
        assert!(
            message.contains("Add an A1200 Kickstart"),
            "the refusal must say what to do about it: {message}"
        );
        assert!(
            message.contains("A500/A600/A2000"),
            "the message must name the ROM the user is holding: {message}"
        );
        for misleading in ["too old", "2.0 or newer", "newer"] {
            assert!(
                !message.contains(misleading),
                "the refusal must not read as a version complaint: {message}"
            );
        }
    }

    /// An A1200 profile must not silently accept a Kickstart 1.3 dump.
    /// `WHDLOAD_MIN_KICKSTART_MAJOR` is checked against the ROM, not the
    /// machine, so moving every WHDLoad launch to the A1200 must not have
    /// weakened it — and the refusal names the machine ART actually planned.
    #[test]
    fn a_whdload_launch_refuses_a_kickstart_below_the_minimum_on_the_a1200() {
        let request = args(Media::WhdloadHardfile {
            file: "Game.hdf".into(),
            slave: "Game.Slave".into(),
        });
        let kick13 = LaunchRom {
            name: "Kickstart 1.3 (34.5) A1200".into(),
            models: vec!["A1200".into()],
            path: r"D:\roms\kick34005.A1200".into(),
            major: Some(34),
        };

        let preview = preview_for(&request, &[kick13]);

        assert!(preview.plan.is_none(), "{:?}", preview.plan);
        assert_eq!(
            preview.refusal,
            Some(LaunchRefusal::NoRomMeetsWhdloadMinimum {
                machine: Machine::A1200
            })
        );
    }

    // ---- preview_for: what the confirmation screen is actually told -------

    /// The confirmation screen's own note must name the memory a WHDLoad
    /// launch will use — this is `LaunchPreview.memory`, which ART-151's
    /// frontend change reads into the same sentence that already states the
    /// machine and the ROM.
    #[test]
    fn preview_for_states_the_memory_a_whdload_launch_will_use() {
        let mut request = args(Media::WhdloadHardfile {
            file: "1000 Miglia.hdf".into(),
            slave: "1000Miglia.Slave".into(),
        });
        request.whdload_fast_ram_mb = 16;

        let preview = preview_for(&request, &[a1200_rom()]);

        let memory = preview
            .memory
            .expect("a settled plan must state its memory");
        assert_eq!(memory.fast_mb, 16);
        assert!(preview.plan.is_some());
    }

    /// A refusal has nothing to try, so it states no memory either — the
    /// field must not be left populated with a plan that was never settled.
    #[test]
    fn preview_for_states_no_memory_on_a_refusal() {
        let request = args(Media::WhdloadHardfile {
            file: "1000 Miglia.hdf".into(),
            slave: "1000Miglia.Slave".into(),
        });

        // No ROM at all: `plan_for` refuses with `NoRomMeetsWhdloadMinimum`.
        let preview = preview_for(&request, &[]);

        assert!(preview.plan.is_none());
        assert!(preview.memory.is_none());
    }

    /// The real path a screen calling `launch_plan` takes for an archived
    /// title: refused before `plan_for` ever runs, with a ROM folder that
    /// would otherwise happily settle a plan — proving the refusal comes
    /// from the media shape and not from a missing ROM.
    #[test]
    fn preview_for_refuses_an_archived_drawer_before_planning() {
        let request = args(Media::WhdloadArchive {
            file: "WHDLoadDemos100.lha".into(),
            inner: "Demos/T/Tag".into(),
            slave: "Tag.Slave".into(),
        });

        let preview = preview_for(&request, &[a1200_rom()]);

        assert!(preview.plan.is_none());
        assert!(preview.memory.is_none());
        match preview.refusal {
            Some(LaunchRefusal::ArchivedWhdload { file }) => {
                assert_eq!(file, "WHDLoadDemos100.lha");
            }
            other => panic!("expected ArchivedWhdload, got {other:?}"),
        }
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

    /// ART-147. Default is read-only, same as any other bare hardfile — and
    /// the mount note distinguishes this shape from a plain `Hardfile` one,
    /// which is what lets the screen say plainly that a save will not be
    /// kept rather than reusing the generic hardfile wording.
    #[test]
    fn mount_notes_state_a_whdload_hardfile_as_read_only_by_default() {
        let request = LaunchArgs {
            path: r"D:\games\1000 Miglia.hdf".into(),
            ..args(Media::WhdloadHardfile {
                file: "1000 Miglia.hdf".into(),
                slave: "1000Miglia.Slave".into(),
            })
        };
        let plan = plan_with(LaunchKind::Hardfile {
            image: "1000 Miglia.hdf".into(),
        });

        let notes = mount_notes_for(&request, &plan);
        assert!(matches!(
            notes.as_slice(),
            [MountNote::WhdloadHardfile { read_only: true }]
        ));
    }

    /// The user's own explicit opt-in — never inferred — is what turns this
    /// writable, and the confirmation screen must be told so it can say
    /// saves will be kept.
    #[test]
    fn mount_notes_state_a_whdload_hardfile_as_writable_when_the_user_opts_in() {
        let request = LaunchArgs {
            path: r"D:\games\1000 Miglia.hdf".into(),
            allow_write: true,
            ..args(Media::WhdloadHardfile {
                file: "1000 Miglia.hdf".into(),
                slave: "1000Miglia.Slave".into(),
            })
        };
        let plan = plan_with(LaunchKind::Hardfile {
            image: "1000 Miglia.hdf".into(),
        });

        let notes = mount_notes_for(&request, &plan);
        assert!(matches!(
            notes.as_slice(),
            [MountNote::WhdloadHardfile { read_only: false }]
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
            serde_json::to_value(MountNote::WhdloadHardfile { read_only: true }).unwrap(),
            serde_json::json!({ "kind": "whdload-hardfile", "read_only": true })
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
            allow_write: false,
            whdload_fast_ram_mb: DEFAULT_WHDLOAD_FAST_RAM_MB,
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
            RequestKind::Hardfile { image, whdload } => {
                assert_eq!(image, "af-application.hdf");
                assert!(!whdload, "a plain hardfile is not WHDLoad-shaped");
            }
            other => panic!("{other:?}"),
        }
    }

    /// ART-147. A self-booting hardfile takes the plain `Hardfile` request
    /// kind, not `Whdload` — there is no system to mount alongside it and no
    /// boot directory to write. `slave` is not read here at all; it is a fact
    /// carried on the record for the title's provenance, not something a
    /// launch decision consults.
    ///
    /// ART-148: `whdload` on the request kind must be `true` here — this is
    /// exactly `1000 Miglia`'s own shape, and it is what tells `plan_for` to
    /// enforce WHDLoad's Kickstart floor instead of accepting any ROM that
    /// merely suits the machine's model.
    #[test]
    fn media_whdload_hardfile_becomes_request_kind_hardfile() {
        let a = args(Media::WhdloadHardfile {
            file: "1000 Miglia.hdf".into(),
            slave: "1000Miglia.Slave".into(),
        });
        match request_kind_from(&a) {
            RequestKind::Hardfile { image, whdload } => {
                assert_eq!(image, "1000 Miglia.hdf");
                assert!(whdload, "a self-booting WHDLoad hardfile is WHDLoad-shaped");
            }
            other => panic!("{other:?}"),
        }
    }

    /// `archived_refusal` as a plain sentence — the same conversion
    /// `require_exists` already leans on for `FileMissing`.
    fn launch_refusal_for(args: &LaunchArgs) -> Option<String> {
        archived_refusal(&args.media).map(|refusal| refusal_error(refusal).to_string())
    }

    /// An unpacked drawer is the one shape `RequestKind::Whdload` exists for
    /// — `dir` maps straight onto `drawer`.
    #[test]
    fn an_unpacked_drawer_launches_through_the_whdload_path() {
        let a = args(Media::WhdloadDrawer {
            dir: "Games/Turrican".into(),
            slave: "Turrican.slave".into(),
        });
        match request_kind_from(&a) {
            RequestKind::Whdload { drawer, slave } => {
                assert_eq!(drawer, "Games/Turrican");
                assert_eq!(slave, "Turrican.slave");
            }
            other => panic!("a drawer is the one shape that path exists for, got {other:?}"),
        }
    }

    /// ART-147, for the shape this task adds: an archived drawer must never
    /// take the launchable path, and must say why in a sentence that names
    /// the archive.
    #[test]
    fn an_archived_drawer_is_not_launchable_and_says_which_archive() {
        let a = args(Media::WhdloadArchive {
            file: "WHDLoadDemos100.lha".into(),
            inner: "Demos/T/Tag".into(),
            slave: "Tag.Slave".into(),
        });
        let refusal =
            launch_refusal_for(&a).expect("an archived title cannot be launched and must say so");
        assert!(
            refusal.contains("WHDLoadDemos100.lha"),
            "the refusal names the archive the user has to unpack, got: {refusal}"
        );
        assert!(
            !matches!(request_kind_from(&a), RequestKind::Whdload { .. }),
            "an archived title must never reach the launchable path - that is ART-147"
        );
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
        let dir = std::env::temp_dir().join(format!(
            "art-launch-cmd-{tag}-{}",
            crate::core::test_scratch_id()
        ));
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
                major: Some(40),
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

    /// ART-146: the plain-hardfile path decides the image's shape from the
    /// file itself rather than assuming every hardfile is bare — an `RDSK`
    /// image here (unlike the placeholder bytes the other hardfile tests
    /// use) must come out `HardfileShape::Rdb`.
    #[test]
    fn a_plain_hardfile_shape_is_detected_from_its_own_bytes() {
        let dir = scratch("hardfile-shape-rdb");
        let hdf = dir.join("Rdb.hdf");
        let mut image = vec![0u8; 512 * 4];
        image[0..4].copy_from_slice(b"RDSK");
        std::fs::write(&hdf, &image).unwrap();

        let request = LaunchArgs {
            path: hdf.to_string_lossy().to_string(),
            ..args(Media::Hardfile {
                file: "Rdb.hdf".into(),
            })
        };
        let plan = plan_with(LaunchKind::Hardfile {
            image: "Rdb.hdf".into(),
        });

        let media =
            media_for_plan(&request, &plan, &dir.join("launch"), &dir.join("boot")).unwrap();

        assert_eq!(media.hardfile_shapes, vec![HardfileShape::Rdb]);

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

    /// ART-147, the fix itself. A self-booting WHDLoad hardfile is the
    /// user's own original file, so it mounts read-only by default just like
    /// a plain `Hardfile` — §93 still stands even though this is the shape
    /// where read-only silently loses the game's saves.
    #[test]
    fn a_whdload_hardfile_mounts_read_only_by_default() {
        let dir = scratch("whdload-hardfile-default");
        let hdf = dir.join("1000 Miglia.hdf");
        std::fs::write(&hdf, b"HARDFILE").unwrap();

        let request = LaunchArgs {
            path: hdf.to_string_lossy().to_string(),
            ..args(Media::WhdloadHardfile {
                file: "1000 Miglia.hdf".into(),
                slave: "1000Miglia.Slave".into(),
            })
        };
        let plan = plan_with(LaunchKind::Hardfile {
            image: "1000 Miglia.hdf".into(),
        });

        let media =
            media_for_plan(&request, &plan, &dir.join("launch"), &dir.join("boot")).unwrap();

        assert_eq!(
            media.hardfile_paths,
            vec![hdf.to_string_lossy().to_string()]
        );
        assert!(
            media.write_protect_hardfiles,
            "default must protect the user's own file (spec §93)"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The user's own opt-in — never inferred — is what makes this mount
    /// writable, so a save WHDLoad writes back into the image survives.
    #[test]
    fn a_whdload_hardfile_mounts_writable_when_the_user_allows_it() {
        let dir = scratch("whdload-hardfile-allow-write");
        let hdf = dir.join("1000 Miglia.hdf");
        std::fs::write(&hdf, b"HARDFILE").unwrap();

        let request = LaunchArgs {
            path: hdf.to_string_lossy().to_string(),
            allow_write: true,
            ..args(Media::WhdloadHardfile {
                file: "1000 Miglia.hdf".into(),
                slave: "1000Miglia.Slave".into(),
            })
        };
        let plan = plan_with(LaunchKind::Hardfile {
            image: "1000 Miglia.hdf".into(),
        });

        let media =
            media_for_plan(&request, &plan, &dir.join("launch"), &dir.join("boot")).unwrap();

        assert!(
            !media.write_protect_hardfiles,
            "the user's explicit opt-in must make this writable"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `allow_write` must only affect a `WhdloadHardfile` — a plain
    /// `Hardfile` (Enzo's collection, no known slave) stays read-only
    /// regardless, since nothing about that shape asked the user for this
    /// opt-in and there is no confirmation-screen sentence explaining what it
    /// would mean for it.
    #[test]
    fn allow_write_is_ignored_for_a_plain_hardfile() {
        let dir = scratch("hardfile-allow-write-ignored");
        let hdf = dir.join("Game.hdf");
        std::fs::write(&hdf, b"HARDFILE").unwrap();

        let request = LaunchArgs {
            path: hdf.to_string_lossy().to_string(),
            allow_write: true,
            ..args(Media::Hardfile {
                file: "Game.hdf".into(),
            })
        };
        let plan = plan_with(LaunchKind::Hardfile {
            image: "Game.hdf".into(),
        });

        let media =
            media_for_plan(&request, &plan, &dir.join("launch"), &dir.join("boot")).unwrap();

        assert!(
            media.write_protect_hardfiles,
            "a plain Hardfile has no allow_write switch on screen, so the field must not act on it"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Exercises `media_for_plan`'s `LaunchKind::Whdload` arm directly, with a
    /// hand-built plan — not through `request_kind_from`, which (ART-147) no
    /// `Media` variant reaches any more. The `media` field is therefore
    /// irrelevant to what these tests check and carries a placeholder value.
    fn whdload_args(system: &Path, drawer: &Path) -> LaunchArgs {
        LaunchArgs {
            path: drawer.to_string_lossy().to_string(),
            system_volume: Some(system.to_string_lossy().to_string()),
            ..args(Media::Hardfile {
                file: "unused-placeholder.hdf".into(),
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

    /// ART-146, the exact scenario the real run hit: the WHDLoad system
    /// image is a VHD container (`conectix` at offset 0 — `AmiKit.hdf`'s
    /// actual bytes), not a bare filesystem image. `media_for_plan` must
    /// record that so `generate_uae_config` stops forcing bare-image
    /// geometry over it, which is what produced "Not a DOS disk in unit 0".
    #[test]
    fn a_whdload_systems_vhd_shape_is_detected() {
        let dir = scratch("whdload-shape-vhd");
        let system = dir.join("AmiKit.hdf");
        let mut image = vec![0u8; 512 * 4];
        image[0..8].copy_from_slice(b"conectix");
        std::fs::write(&system, &image).unwrap();
        let drawer = dir.join("Turrican");
        std::fs::create_dir_all(&drawer).unwrap();

        let request = whdload_args(&system, &drawer);
        let plan = plan_with(LaunchKind::Whdload {
            drawer: drawer.to_string_lossy().to_string(),
            slave: "Turrican.slave".into(),
            system: system.to_string_lossy().to_string(),
            one_click: true,
        });

        let media =
            media_for_plan(&request, &plan, &dir.join("launch"), &dir.join("boot")).unwrap();

        assert_eq!(media.hardfile_shapes, vec![HardfileShape::Unknown]);

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
