//! Reading a PiStorm SD card — the screen half of ART-095/ART-097.
//!
//! `core/card.rs` has been able to read a real card since 2026-08-13; nothing
//! could show one. This is the adapter that lets the Hard Disk studio ask.
//!
//! The two derived answers travel with the card rather than being left for the
//! UI to work out, and that is deliberate: **both of them are questions the UI
//! would get wrong.** `file_systems` is the union across every area, and
//! `unmountable` is asked against that union — MultibootOS 2.2 carries PFS3 in
//! its first RDB and not its second, so an interface that asked one area in
//! isolation would report fifteen working partitions as broken (ART-097). The
//! rule lives in `core/`, where a test pins it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::core::card::build::{build_card, AreaSpec, CardSpec};
use crate::core::card::health::{check_image, HealthReport};
use crate::core::card::intake::{role_for, CardRole};
use crate::core::card::manifest::{
    describe_card, manifest_path_for, read_manifest, render_manifest, ManifestFile, SourceFacts,
};
use crate::core::card::payload::{emu68_payload, PayloadSpec};
use crate::core::card::{read_card, CardImage};
use crate::core::detect::detect;
use crate::core::error::CoreResult;
use crate::core::hashing::{sha256_bytes, sha256_file};
use crate::core::jobs::{JobId, ProgressSink};
use crate::core::mbr::{plan_card, CardLayout};
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::core::pistorm::firmware::FirmwareConfig;
use crate::core::pistorm::hardware::{Emu68Line, PistormHardware};
use crate::core::pistorm::options::Emu68Options;
use crate::core::pistorm::rom_suits;
use crate::core::rdb::{ParsedFileSystem, PartitionSpec};
use crate::core::rom::{identify_rom, RomInfo};
use crate::core::safety::atomic::atomic_write;
use crate::error::AppResult;

use super::jobs::{spawn_job, JobRegistry};
use super::oplog::{user_operation, write_to_path};

/// A partition naming a filesystem **no** area on the card carries.
///
/// An Amiga ignores such a partition in silence, which is the failure ART-084
/// was about — and the only way a user finds out is that a drive they expected
/// is not there.
#[derive(Debug, Clone, Serialize)]
pub struct UnmountablePartition {
    /// Which Amiga disk on the card it belongs to.
    pub area: usize,
    pub drive_name: String,
    /// `PDS\3`, `SFS\0` — the four characters as they read.
    pub dostype_str: String,
}

/// What ART found on a card, ready for a screen.
#[derive(Debug, Clone, Serialize)]
pub struct CardReport {
    pub card: CardImage,
    /// Every driver on the **whole card**, deduplicated by DosType.
    pub file_systems: Vec<ParsedFileSystem>,
    pub unmountable: Vec<UnmountablePartition>,
}

/// Read a card — or a plain HDF, which comes back as one area at offset zero.
///
/// Header-only: a window at each area's start, never the sixty to a hundred
/// and twenty gigabytes in between (§56).
#[tauri::command]
pub fn card_open(path: String) -> AppResult<CardReport> {
    Ok(report_for(read_card(&PathBuf::from(path.trim()))?))
}

/// The two derived answers, in the one place that knows the rule.
fn report_for(card: CardImage) -> CardReport {
    let file_systems = card.file_systems();
    let unmountable = card
        .partitions_missing_driver()
        .into_iter()
        .map(|(area, part)| UnmountablePartition {
            area,
            drive_name: part.drive_name,
            dostype_str: part.dostype_str,
        })
        .collect();

    CardReport {
        card,
        file_systems,
        unmountable,
    }
}

// ---------------------------------------------------------------------------
// Building one (SD-1 · G2)
// ---------------------------------------------------------------------------

/// What a screen asks for when it wants a card.
///
/// One request type for **both** the plan and the build, and one function that
/// turns it into a [`CardSpec`], so there is no way for a screen to show the
/// user one card and write another.
#[derive(Debug, Clone, Deserialize)]
pub struct CardBuildRequest {
    /// The user's own Emu68 release archive. ART never downloads it (§2).
    pub archive: String,
    /// Their Kickstart. `None` builds a card with no ROM on it, which will not
    /// boot — allowed and warned about, never substituted for.
    pub kickstart: Option<String>,
    /// Where the image goes. `SAFE_CREATE`: an existing file is refused.
    pub dest: String,
    pub total_bytes: u64,
    /// `0` for the 1.10 GiB measured off both real cards.
    #[serde(default)]
    pub boot_bytes: u64,
    pub label: String,
    pub hardware: PistormHardware,
    /// Which Emu68 release line the archive came from. It decides what the
    /// archive's *name* means, and ART cannot tell from the bytes (ART-091).
    pub line: Emu68Line,
    #[serde(default)]
    pub firmware: FirmwareConfig,
    #[serde(default)]
    pub options: Emu68Options,
    /// When the build happened, for the manifest. **The caller's**, because
    /// `core` has no clock and this command layer has no business inventing a
    /// date format the user's own screen already knows how to make.
    #[serde(default)]
    pub built_at: Option<String>,
    /// The partitions of the card's one Amiga disk.
    ///
    /// One disk, taking whatever is left after the boot partition. Two and
    /// three disks are multiboot — SD-3's G16 — and are not offered here
    /// rather than half-built (§96).
    pub partitions: Vec<PartitionSpec>,
}

/// A file on its way to the boot partition: its name and its size, never its
/// bytes. A payload is megabytes and a screen needs a list.
#[derive(Debug, Clone, Serialize)]
pub struct PlacedFile {
    pub name: String,
    pub bytes: u64,
}

/// Something true about this card that the user would otherwise find out from
/// an Amiga that does not come up.
///
/// A typed value rather than a sentence: `CoreError`'s English strings reach
/// the UI untranslated (ART-060) and this is a screen's own text, so the kind
/// travels and the words are the interface's, in the user's language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CardBuildWarning {
    /// No Kickstart was chosen. The card is built and it will not boot.
    NoKickstart,
    /// A ROM was chosen and ART cannot say anything about it. A label, never
    /// a refusal — an unknown ROM may still be the right one.
    RomUnrecognised,
    /// **ART knows what the ROM is and not which machine it is for.** A
    /// Kickstart states its version and revision in its own header, and three
    /// real 3.1 dumps state `40.68` while claiming three different machines —
    /// so the revision names the ROM and only the checksum names the machine
    /// (ART-104). Saying "I do not recognise this" about a ROM ART has just
    /// named would be the wrong sentence.
    RomMachineUnknown { rom: String },
    /// The ROM is one ART knows and it is not for this Amiga.
    RomWrongMachine { rom: String },
    /// **What SD-1 builds.** The Amiga sees a partition table it understands
    /// and volumes it will offer to format; putting a system on them is SD-2's
    /// work. Said plainly rather than left to be discovered (§10, §89).
    VolumesUnformatted,
}

/// What building this request would produce. Writes nothing.
#[derive(Debug, Clone, Serialize)]
pub struct CardBuildPlan {
    /// Where every part of the card lands, to the sector.
    pub layout: CardLayout,
    pub boot_files: Vec<PlacedFile>,
    /// The file `config.txt` points the Pi's firmware at — the release's own
    /// answer, not ART's (ART-103).
    pub kernel_file: String,
    /// The name the Kickstart is written under, if there is one.
    pub kickstart_file: Option<String>,
    /// The ROM as ART identifies it, so the user confirms what they picked.
    pub rom: Option<RomInfo>,
    pub warnings: Vec<CardBuildWarning>,
    /// `SAFE_CREATE` would refuse. Said here so the screen can, before the
    /// button rather than after it.
    pub dest_exists: bool,
}

/// What a finished build produced.
#[derive(Debug, Clone, Serialize)]
pub struct CardBuildResult {
    pub job_id: JobId,
    pub dest: String,
    pub layout: CardLayout,
    /// Where the build manifest was written (G7).
    pub manifest_path: String,
    /// The card **read back out of the file that was just written**, through
    /// the same reader and the same report the Hard Disk studio shows for
    /// somebody else's card. A build that cannot be read is not a build.
    pub verified: CardReport,
}

/// The event a finished build arrives on.
pub const CARD_BUILD_EVENT: &str = "card-build-result";

/// The request as a card, without the payload. The one mapping both the plan
/// and the build go through.
fn card_spec(
    request: &CardBuildRequest,
    boot_files: Vec<crate::core::fat32::BootFile>,
) -> CardSpec {
    CardSpec {
        total_bytes: request.total_bytes,
        boot_bytes: request.boot_bytes,
        label: request.label.clone(),
        boot_files,
        areas: vec![AreaSpec {
            // Whatever is left after the boot partition.
            size_bytes: 0,
            partitions: request.partitions.clone(),
            // No driver is embedded: SD-1 writes FFS partitions, which
            // Kickstart mounts itself. A PFS3 card needs `create_rdb_layout`'s
            // driver embedding and a driver to embed — SD-2 (ART-084).
            file_systems: Vec::new(),
        }],
    }
}

/// The Kickstart a card is to carry, as an Amiga would have to read it.
///
/// **A licensed Amiga Forever ROM is not ROM bytes yet** (ART-128). It is the
/// image behind an `AMIROMTYPE1` header and a repeating XOR against the
/// buyer's own `rom.key`, and this used to be a plain `std::fs::read`: the
/// encrypted file went onto the card verbatim, Emu68 loaded eleven bytes of
/// header and half a megabyte of ciphertext as its Kickstart, and the machine
/// did not boot. Nothing said why — the build's only note was the same
/// "ART does not recognise this ROM" it shows for any uncatalogued dump,
/// which reads as *probably fine*.
///
/// So: decoded when the key is beside it, and **refused** when it is not.
/// Refused rather than warned, because this is not a risk — a card built this
/// way cannot boot, and ART-103 is the precedent for stopping at a certainty
/// instead of writing it and hoping.
fn kickstart_for(path: &str) -> CoreResult<Vec<u8>> {
    let path = Path::new(path.trim());
    let raw = std::fs::read(path)?;
    if !raw.starts_with(b"AMIROMTYPE1") {
        return Ok(raw);
    }

    let info = identify_rom(path)?;
    if !info.key_available {
        return Err(crate::core::error::CoreError::InvalidInput(format!(
            concat!(
                "'{}' is an encrypted Amiga Forever ROM and its 'rom.key' is ",
                "not beside it. Written to a card as it stands, the Amiga would ",
                "find a header and encrypted bytes where its Kickstart should ",
                "be, and would not start. Put the 'rom.key' from the same Amiga ",
                "Forever installation in this folder, or point ART at a ",
                "decrypted ROM."
            ),
            path.display()
        )));
    }
    let key = std::fs::read(
        path.parent()
            .map(|dir| dir.join("rom.key"))
            .unwrap_or_else(|| Path::new("rom.key").to_path_buf()),
    )?;
    Ok(crate::core::rom::decode_cloanto(
        &crate::core::rom::strip_cloanto_header(&raw),
        &key,
    ))
}

/// The payload the request asks for, with the Kickstart read off disk.
fn payload_for(request: &CardBuildRequest) -> CoreResult<crate::core::card::payload::Emu68Payload> {
    let kickstart = match &request.kickstart {
        Some(path) => Some(kickstart_for(path)?),
        None => None,
    };

    emu68_payload(
        Path::new(request.archive.trim()),
        &PayloadSpec {
            hardware: request.hardware,
            line: request.line,
            firmware: request.firmware.clone(),
            options: request.options.clone(),
            kickstart,
        },
    )
}

/// What building this card would do. Writes nothing (§92's PREVIEW step).
///
/// Unpacking the release archive is what this costs, and it is one small
/// archive with a total ceiling on it — not ART-066's shape, where planning
/// meant unpacking a batch of the user's own archives on the command thread.
#[tauri::command]
pub fn card_plan_build(request: CardBuildRequest) -> AppResult<CardBuildPlan> {
    let payload = payload_for(&request)?;
    let spec = card_spec(&request, Vec::new());
    let layout = plan_card(spec.total_bytes, spec.boot_bytes, &[0])?;

    let mut warnings = vec![CardBuildWarning::VolumesUnformatted];

    let rom = match &request.kickstart {
        None => {
            warnings.push(CardBuildWarning::NoKickstart);
            None
        }
        Some(path) => {
            // Unreadable is a real failure; unrecognised is not — which is
            // why this is a `?` and everything below is a warning.
            let info = identify_rom(Path::new(path.trim()))?;
            {
                match rom_suits(&info, request.hardware.amiga) {
                    // Recognised and wrong for this machine. A note, never a
                    // block — the user may know something ART does not.
                    Some(false) => warnings.push(CardBuildWarning::RomWrongMachine {
                        rom: info.name.clone(),
                    }),
                    // Nothing to compare against. Which of the two sentences
                    // that deserves depends on whether ART named the ROM at
                    // all — `version` is `Custom` only when it could not.
                    None if info.version == "Custom" => {
                        warnings.push(CardBuildWarning::RomUnrecognised)
                    }
                    None => warnings.push(CardBuildWarning::RomMachineUnknown {
                        rom: info.name.clone(),
                    }),
                    Some(true) => {}
                }
                Some(info)
            }
        }
    };

    Ok(CardBuildPlan {
        layout,
        boot_files: payload
            .files
            .iter()
            .map(|file| PlacedFile {
                name: file.name.clone(),
                bytes: file.bytes.len() as u64,
            })
            .collect(),
        kernel_file: payload.kernel_file,
        kickstart_file: request
            .kickstart
            .as_ref()
            .map(|_| request.firmware.kickstart_file.clone()),
        rom,
        warnings,
        dest_exists: Path::new(request.dest.trim()).exists(),
    })
}

/// Build the requested card. The half a unit test can host — `card_build` adds
/// the job, the event and the log around it.
fn build_requested_card(
    request: &CardBuildRequest,
    progress: &dyn ProgressSink,
) -> CoreResult<BuiltRequestedCard> {
    let payload = payload_for(request)?;

    // Hashed here, from the bytes about to be written — the only place they
    // exist to be hashed, since ART writes FAT32 and cannot read one back.
    let boot_files: Vec<ManifestFile> = payload
        .files
        .iter()
        .map(|file| ManifestFile {
            name: file.name.clone(),
            bytes: file.bytes.len() as u64,
            sha256: sha256_bytes(&file.bytes),
        })
        .collect();
    let kernel_file = payload.kernel_file.clone();

    let image = Path::new(request.dest.trim());
    let spec = card_spec(request, payload.files);
    let built = build_card(image, &spec, progress)?;

    // G7: the manifest is written from the *finished* card, so it records what
    // is there rather than what the builder meant. Beside the image, through
    // `core/safety` like every other write.
    let manifest = describe_card(
        image,
        source_facts(request, &kernel_file)?,
        boot_files,
        request.built_at.clone(),
    )?;
    let manifest_path = manifest_path_for(image);
    atomic_write(&manifest_path, render_manifest(&manifest)?.as_bytes())?;

    Ok(BuiltRequestedCard {
        layout: built.layout,
        verified: report_for(built.verified),
        manifest_path: manifest_path.display().to_string(),
    })
}

/// What the image cannot say about itself: which files on the user's disk it
/// was built from, and what they hashed to.
///
/// Names only, never paths — a manifest is shareable, and where somebody keeps
/// their downloads is not part of what the card is.
fn source_facts(request: &CardBuildRequest, kernel_file: &str) -> CoreResult<SourceFacts> {
    let archive = Path::new(request.archive.trim());
    let kickstart = request.kickstart.as_ref().map(|p| PathBuf::from(p.trim()));

    Ok(SourceFacts {
        archive_name: file_name_of(archive),
        archive_sha256: sha256_file(archive)?,
        kickstart_name: kickstart.as_deref().map(file_name_of),
        kickstart_sha256: match &kickstart {
            Some(path) => Some(sha256_file(path)?),
            None => None,
        },
        kickstart_file: kickstart
            .as_ref()
            .map(|_| request.firmware.kickstart_file.clone()),
        kickstart_stated_major: match &kickstart {
            Some(path) => crate::core::rom::stated_version(&crate::core::rom::decoded_image(path)?)
                .map(|(major, _minor)| major),
            None => None,
        },
        hardware: request.hardware,
        line: request.line,
        kernel_file: kernel_file.to_string(),
    })
}

fn file_name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

struct BuiltRequestedCard {
    layout: CardLayout,
    verified: CardReport,
    manifest_path: String,
}

/// One dropped file, and what it becomes on the card being built.
#[derive(Debug, Clone, Serialize)]
pub struct CardIntakeItem {
    pub path: String,
    pub name: String,
    pub role: CardRole,
    /// Filled when the role is a Kickstart, so the screen can say *which*
    /// ROM was dropped rather than only that one was.
    pub rom: Option<RomInfo>,
}

/// What each of these files would become on a card (SD-1 · G15).
///
/// **It detects rather than being told.** The drop pipeline has already
/// produced a `Detection` for each path and the frontend is holding it, but an
/// answer about what goes on somebody's card must not rest on a category a
/// caller supplied — `detect` reads a header, and one truth is worth the read.
#[tauri::command]
pub fn card_intake(paths: Vec<String>) -> AppResult<Vec<CardIntakeItem>> {
    let mut out = Vec::with_capacity(paths.len());

    for given in paths {
        let path = PathBuf::from(given.trim());
        let detection = detect(&path)?;
        let role = role_for(&path, detection.category);

        // An unreadable ROM is still a ROM the user meant to use; the screen
        // says "a Kickstart" and leaves the detail out rather than failing the
        // whole drop.
        let rom = match role {
            CardRole::Kickstart => identify_rom(&path).ok(),
            _ => None,
        };

        out.push(CardIntakeItem {
            name: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            path: path.display().to_string(),
            role,
            rom,
        });
    }

    Ok(out)
}

/// Check a built image — the last gate before the file is handed over
/// (§92's VERIFY, SD-1 · G8).
///
/// One command rather than two: the manifest comparison (G7) is a section of
/// this report rather than a separate button, because a user asking "is this
/// card right" is asking one question.
///
/// `manifest` defaults to the image's own. A card ART did not build has none,
/// and that is answered as **not checked** rather than as a failure — so this
/// works on somebody else's card too, reporting less.
#[tauri::command]
pub fn card_check_image(
    image: String,
    manifest: Option<String>,
    pi: Option<String>,
) -> AppResult<HealthReport> {
    let image = PathBuf::from(image.trim());
    let manifest_path = manifest
        .map(|given| PathBuf::from(given.trim()))
        .unwrap_or_else(|| manifest_path_for(&image));

    // Absent is not an error: `read_manifest` would fail on a card nobody
    // wrote a manifest for, and that card is still worth checking.
    let manifest = match manifest_path.is_file() {
        true => Some(read_manifest(&manifest_path)?),
        false => None,
    };

    Ok(check_image(
        &image,
        manifest.as_ref(),
        pi.as_deref().unwrap_or_default(),
    )?)
}

/// Build a card image. Returns a job id (§54, §55).
///
/// Long by nature — a payload to unpack and a partition table, a filesystem
/// and one to three RDBs to write — so it never runs on the command thread.
/// Cancelling is safe by construction: `build_card` checks between whole units
/// of work and removes the half-built file, which never existed a moment ago.
#[tauri::command]
pub fn card_build(
    request: CardBuildRequest,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<JobId> {
    let log_path = oplog.path().to_path_buf();
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let dest = request.dest.trim().to_string();
    let title = format!("Building {dest}");

    let id = spawn_job(&app, registry, &title, move |job_id, progress| {
        let outcome = build_requested_card(&request, progress);

        // §53: a card is user data the moment it exists, and where it came
        // from is the thing a manifest will later be built out of (G7).
        let record = user_operation("Build a PiStorm card image")
            .source(&request.archive)
            .destination(&dest)
            .detail("Card size", request.total_bytes.to_string())
            .detail(
                "Kickstart",
                request.kickstart.clone().unwrap_or_else(|| "none".into()),
            );
        let record = match &outcome {
            Ok(built) => record
                .detail("Amiga disks", built.verified.card.areas.len().to_string())
                .outcome(OperationOutcome::verified(
                    !built.verified.card.areas.is_empty(),
                )),
            Err(err) => record.failure(err.code(), err.to_string()),
        };
        write_to_path(&log_path, &record);

        let built = outcome?;
        let _ = emit_app.emit(
            CARD_BUILD_EVENT,
            CardBuildResult {
                job_id,
                dest,
                layout: built.layout,
                manifest_path: built.manifest_path,
                verified: built.verified,
            },
        );
        Ok(())
    });

    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plain HDF is not a card, and must not be reported as a broken one:
    /// one area at offset zero, no MBR, and nothing unmountable.
    ///
    /// The adapter is thin enough that this is really a test of the contract
    /// the screen depends on — that it can call one command for both kinds of
    /// file and branch on `mbr` rather than on a guess.
    #[test]
    fn a_plain_hdf_comes_back_as_one_area_at_offset_zero() {
        use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec};

        let dir =
            std::env::temp_dir().join(format!("art-card-cmd-{}", crate::core::test_scratch_id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("plain.hdf");

        crate::core::hdf::create_hdf(
            &path,
            32 * 1024 * 1024,
            true,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::FfsStandard,
                size_mb: 10,
                bootable: true,
                boot_priority: 0,
                num_buffers: 0,
            }],
            &[],
        )
        .unwrap();

        let report = card_open(path.display().to_string()).unwrap();

        assert!(
            report.card.mbr.is_none(),
            "an HDF carries no partition table"
        );
        assert_eq!(report.card.areas.len(), 1);
        assert_eq!(report.card.areas[0].offset_bytes, 0);
        assert_eq!(report.card.areas[0].rdb.partitions.len(), 1);
        assert!(
            report.unmountable.is_empty(),
            "FFS is Kickstart's own and needs no driver in the RDB"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A partition naming a filesystem nothing carries is named, with the area
    /// it is on — the answer ART-084 wanted and ART-097 corrected.
    #[test]
    fn a_partition_with_no_driver_anywhere_on_the_card_is_named() {
        use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec};

        let dir = std::env::temp_dir().join(format!(
            "art-card-cmd-pfs-{}",
            crate::core::test_scratch_id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pfs.hdf");

        // PFS3 is not in Kickstart: without a driver in the RDB, an Amiga
        // ignores this partition in silence.
        crate::core::hdf::create_hdf(
            &path,
            32 * 1024 * 1024,
            true,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::Pfs3Standard,
                size_mb: 10,
                bootable: true,
                boot_priority: 0,
                num_buffers: 0,
            }],
            &[],
        )
        .unwrap();

        let report = card_open(path.display().to_string()).unwrap();

        assert_eq!(report.file_systems.len(), 0, "no driver was embedded");
        assert_eq!(report.unmountable.len(), 1);
        assert_eq!(report.unmountable[0].area, 0);
        assert_eq!(report.unmountable[0].drive_name, "DH0");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------------
    // Building one
    // -----------------------------------------------------------------------

    use crate::core::pistorm::hardware::{AmigaTarget, PiModel, PistormVariant};
    use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec};

    const GIB: u64 = 1024 * 1024 * 1024;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-card-build-{name}-{}",
            crate::core::test_scratch_id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A zip shaped like the real Emu68 release: files at the root, a folder,
    /// and the Pi's own `config.txt` naming the kernel it boots.
    fn emu68_zip(dir: &std::path::Path) -> std::path::PathBuf {
        use std::io::Write as _;
        let path = dir.join("Emu68-pistorm.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (entry, contents) in [
            ("Emu68-pistorm.gz", &b"kernel"[..]),
            ("start.elf", b"firmware"),
            ("overlays/emu68.dtbo", b"overlay"),
            ("config.txt", b"kernel=Emu68-pistorm.gz\narm_64bit=1\n"),
        ] {
            zip.start_file(entry, options).unwrap();
            zip.write_all(contents).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    /// **ART-128.** An encrypted Amiga Forever ROM used to be copied onto the
    /// card exactly as it sits on disk — header, ciphertext and all — so the
    /// Amiga found no Kickstart where its Kickstart should be. The build is
    /// refused now, by name and with the remedy, before anything is written.
    #[test]
    fn an_encrypted_rom_with_no_key_is_refused_rather_than_written_to_a_card() {
        let dir = scratch("cloanto-no-key");
        let mut rom = b"AMIROMTYPE1".to_vec();
        rom.extend(std::iter::repeat_n(0xA5u8, 524_288));
        let path = dir.join("amiga-os-310-a1200.rom");
        std::fs::write(&path, &rom).unwrap();

        let err = kickstart_for(&path.display().to_string()).unwrap_err();

        assert_eq!(err.code(), "ART-INPUT-INVALID", "{err}");
        let said = err.to_string();
        assert!(said.contains("rom.key"), "the remedy is named: {said}");
        assert!(
            said.contains("would not start"),
            "and what happens if it is not: {said}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// With the key beside it, the card carries the ROM an Amiga can read —
    /// the decoded image, not the file.
    #[test]
    fn an_encrypted_rom_reaches_the_card_decoded() {
        let dir = scratch("cloanto-keyed-card");
        let plain: Vec<u8> = (0..524_288u32).map(|i| (i % 251) as u8).collect();
        let key = b"the buyer's own key".to_vec();
        std::fs::write(dir.join("rom.key"), &key).unwrap();

        let mut encoded = b"AMIROMTYPE1".to_vec();
        encoded.extend(
            plain
                .iter()
                .enumerate()
                .map(|(at, byte)| byte ^ key[at % key.len()]),
        );
        let path = dir.join("amiga-os-310-a1200.rom");
        std::fs::write(&path, &encoded).unwrap();

        let carried = kickstart_for(&path.display().to_string()).unwrap();

        assert_eq!(carried, plain, "the bytes an Amiga would have to read");
        assert_ne!(carried, encoded, "and not the ones on disk");

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn request(archive: &std::path::Path, dest: &std::path::Path) -> CardBuildRequest {
        CardBuildRequest {
            archive: archive.display().to_string(),
            kickstart: None,
            dest: dest.display().to_string(),
            total_bytes: 2 * GIB,
            boot_bytes: 0,
            label: "ART CARD".into(),
            hardware: PistormHardware {
                amiga: AmigaTarget::A500,
                variant: PistormVariant::Classic,
                pi: PiModel::Pi3APlus,
            },
            line: Emu68Line::Stable,
            firmware: FirmwareConfig::default(),
            options: Emu68Options::default(),
            built_at: None,
            partitions: vec![PartitionSpec {
                drive_name: "SDH0".into(),
                fs_type: AmigaHardDiskFs::FfsStandard,
                size_mb: 512,
                bootable: true,
                boot_priority: 0,
                num_buffers: 0,
            }],
        }
    }

    /// The screen asks for a card; this is the shape that comes out. **One
    /// Amiga disk taking whatever is left** — two and three disks are
    /// multiboot, which is SD-3's G16 and is not pretended at here.
    #[test]
    fn a_request_becomes_the_card_the_screen_asked_for() {
        let dir = scratch("spec");
        let archive = emu68_zip(&dir);
        let req = request(&archive, &dir.join("card.img"));

        let spec = card_spec(&req, Vec::new());

        assert_eq!(spec.total_bytes, 2 * GIB);
        assert_eq!(spec.boot_bytes, 0, "0 means the measured 1.10 GiB default");
        assert_eq!(spec.label, "ART CARD");
        assert_eq!(spec.areas.len(), 1);
        assert_eq!(
            spec.areas[0].size_bytes, 0,
            "the one Amiga disk takes the rest of the card"
        );
        assert_eq!(spec.areas[0].partitions.len(), 1);
        assert_eq!(spec.areas[0].partitions[0].drive_name, "SDH0");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `SAFE_CREATE` is the build's answer, and it is a bad one to discover
    /// after pressing the button: the plan says it, so the screen can.
    #[test]
    fn the_plan_says_the_destination_is_there_rather_than_refusing() {
        let dir = scratch("exists");
        let archive = emu68_zip(&dir);
        let dest = dir.join("card.img");
        std::fs::write(&dest, b"somebody's afternoon").unwrap();

        let plan = card_plan_build(request(&archive, &dest)).unwrap();

        assert!(plan.dest_exists, "and it is a plan, not a failure");
        assert_eq!(
            std::fs::read(&dest).unwrap(),
            b"somebody's afternoon",
            "planning writes nothing"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// What the plan is for: the files, the kernel the release names (ART-103),
    /// and the two things about this card that would otherwise be a surprise —
    /// no ROM means no boot, and SD-1 builds a shape rather than a system.
    #[test]
    fn a_card_with_no_kickstart_is_planned_and_said_so() {
        let dir = scratch("no-rom");
        let archive = emu68_zip(&dir);

        let plan = card_plan_build(request(&archive, &dir.join("card.img"))).unwrap();

        assert_eq!(plan.kernel_file, "Emu68-pistorm.gz");
        assert!(
            plan.boot_files.iter().any(|f| f.name == "config.txt"),
            "{:?}",
            plan.boot_files
        );
        assert!(
            plan.warnings.contains(&CardBuildWarning::NoKickstart),
            "{:?}",
            plan.warnings
        );
        assert!(
            plan.warnings
                .contains(&CardBuildWarning::VolumesUnformatted),
            "SD-1 builds a partition table, not a system: {:?}",
            plan.warnings
        );
        assert!(!plan.dest_exists);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// G7, end to end: a build leaves a manifest beside the image, and that
    /// manifest verifies the card it was written from.
    ///
    /// It also pins the two things only this layer can get wrong — the source
    /// facts, which no image can say about itself, and the boot files' hashes,
    /// which are taken from the bytes on their way in because ART writes FAT32
    /// and cannot read one back.
    #[test]
    fn a_build_leaves_a_manifest_that_verifies_the_card() {
        use crate::core::card::manifest::{manifest_path_for, read_manifest, verify_against_image};
        use crate::core::jobs::NoProgress;

        let dir = scratch("manifest");
        let archive = emu68_zip(&dir);
        let dest = dir.join("card.img");
        let mut req = request(&archive, &dest);
        req.built_at = Some("2026-08-14T18:00:00Z".into());

        build_requested_card(&req, &NoProgress).unwrap();

        let manifest = read_manifest(&manifest_path_for(&dest)).unwrap();
        assert_eq!(manifest.source.archive_name, "Emu68-pistorm.zip");
        assert_eq!(
            manifest.source.archive_sha256.len(),
            64,
            "the archive is hashed, not just named"
        );
        assert!(
            manifest.source.kickstart_name.is_none(),
            "this request supplies no ROM, and the manifest says so"
        );
        assert_eq!(manifest.source.kernel_file, "Emu68-pistorm.gz");
        assert_eq!(manifest.built_at.as_deref(), Some("2026-08-14T18:00:00Z"));
        assert!(
            manifest
                .boot_files
                .iter()
                .any(|f| f.name == "config.txt" && f.sha256.len() == 64),
            "{:?}",
            manifest.boot_files
        );

        let report = verify_against_image(&manifest, &dest).unwrap();
        assert!(report.matches(), "{:?}", report.findings);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **G9, task-2 review.** `source_facts`'s `Some` branch — the one that
    /// fills in `kickstart_file` and `kickstart_stated_major` — had no
    /// executed coverage: `request()` hard-codes `kickstart: None`, and the
    /// only test that supplies a real ROM (`identify_real_roms_when_asked`)
    /// is env-var gated and skipped in CI. A wrong field, a dropped `.clone()`
    /// or a swapped major/minor would pass every test that runs.
    ///
    /// A synthetic ROM here, built the way `core::rom`'s own tests build one
    /// — `0x1114` at offset 0, major/minor as big-endian words at 12..16 —
    /// needs no `rom.key`, since `decoded_image` only decodes a file starting
    /// with the Cloanto header. The on-card name (`firmware.kickstart_file`)
    /// is deliberately different from the source file's own name, so the
    /// assertion cannot pass by reading the wrong one; the major is
    /// deliberately neither 40 nor 47, and the minor deliberately different
    /// from the major, so a swap or a copied constant would show up.
    #[test]
    fn source_facts_names_the_on_card_kickstart_and_its_stated_version() {
        let dir = scratch("source-facts-rom");
        let archive = emu68_zip(&dir);

        let mut rom = vec![0u8; 524_288];
        rom[0..2].copy_from_slice(&0x1114u16.to_be_bytes());
        rom[12..14].copy_from_slice(&45u16.to_be_bytes());
        rom[14..16].copy_from_slice(&12u16.to_be_bytes());
        let rom_path = dir.join("A1200-kickstart-dump.rom");
        std::fs::write(&rom_path, &rom).unwrap();

        let mut req = request(&archive, &dir.join("card.img"));
        req.kickstart = Some(rom_path.display().to_string());
        req.firmware.kickstart_file = "kick.rom".into();

        let facts = source_facts(&req, "Emu68-pistorm.gz").unwrap();

        assert_eq!(
            facts.kickstart_file.as_deref(),
            Some("kick.rom"),
            "the on-card name, not the source file's own name"
        );
        assert_eq!(
            facts.kickstart_stated_major,
            Some(45),
            "read off the decoded ROM, major before minor"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ART-104, against the ROMs on this machine rather than a synthetic one.
    ///
    /// ```text
    /// ART_ROM_DIR="E:\amiga\Amigatolon\kickstart"     ///   cargo test identify_real_roms_when_asked -- --nocapture
    /// ```
    #[test]
    fn identify_real_roms_when_asked() {
        let Ok(dir) = std::env::var("ART_ROM_DIR") else {
            return;
        };
        let mut entries: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("rom")))
            .collect();
        entries.sort();

        let mut generic = 0;
        for path in &entries {
            match identify_rom(path) {
                Ok(info) => {
                    if info.version == "Custom" {
                        generic += 1;
                    }
                    println!(
                        "  {:<10} {:<8} {:<44} {}",
                        info.version,
                        info.revision,
                        info.name,
                        path.file_name().unwrap().to_string_lossy()
                    );
                }
                Err(err) => println!("  FAILED {err}  {}", path.display()),
            }
        }
        println!("{} of {} still unnamed", generic, entries.len());
    }

    /// The adapter, against the user's own release rather than a zip ART made
    /// up — the same reason `build_real_card_when_asked` exists one layer down.
    /// A synthetic fixture cannot be asked whether the plan a screen shows is
    /// the plan a real Emu68 archive produces.
    ///
    /// ```text
    /// ART_CARD_ZIP=…\Emu68-pistorm.zip ART_CARD_ROM=…\A1200.rom \
    ///   cargo test plan_a_real_card_when_asked -- --nocapture
    /// ```
    #[test]
    fn plan_a_real_card_when_asked() {
        let Ok(zip) = std::env::var("ART_CARD_ZIP") else {
            return;
        };
        let dir = scratch("real-plan");

        let mut req = request(std::path::Path::new(&zip), &dir.join("card.img"));
        req.kickstart = std::env::var("ART_CARD_ROM").ok();

        let plan = card_plan_build(req).unwrap();

        println!(
            "plan: {} files, booting {}, ROM {:?}",
            plan.boot_files.len(),
            plan.kernel_file,
            plan.rom.as_ref().map(|rom| rom.name.clone())
        );
        for warning in &plan.warnings {
            println!("  warning: {warning:?}");
        }
        assert!(
            plan.boot_files.iter().any(|f| f.name == plan.kernel_file),
            "the kernel the config names has to be among the files"
        );
        assert!(!plan.dest_exists);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The whole adapter against the user's own material, manifest included —
    /// and the file `scripts/fat-oracle-check.py <card.img>` is pointed at.
    ///
    /// ```text
    /// ART_CARD_ZIP=…\Emu68-pistorm.zip ART_CARD_ROM=…\A1200.rom \
    /// ART_CARD_OUT=E:\amiga\ProjeART\g7card.img ART_CARD_GB=8 \
    ///   cargo test build_real_card_with_manifest_when_asked -- --nocapture
    /// ```
    #[test]
    fn build_real_card_with_manifest_when_asked() {
        use crate::core::card::manifest::{manifest_path_for, read_manifest, verify_against_image};
        use crate::core::jobs::NoProgress;

        let (Ok(zip), Ok(out)) = (std::env::var("ART_CARD_ZIP"), std::env::var("ART_CARD_OUT"))
        else {
            return;
        };
        let size_gb: u64 = std::env::var("ART_CARD_GB")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(8);

        let dest = std::path::PathBuf::from(&out);
        let mut req = request(std::path::Path::new(&zip), &dest);
        req.kickstart = std::env::var("ART_CARD_ROM").ok();
        req.total_bytes = size_gb * GIB;
        req.built_at = Some("2026-08-15T00:00:00Z".into());

        let built = build_requested_card(&req, &NoProgress).unwrap();
        println!(
            "card: {} Amiga disk(s); manifest at {}",
            built.verified.card.areas.len(),
            built.manifest_path
        );

        let manifest = read_manifest(&manifest_path_for(&dest)).unwrap();
        println!(
            "manifest: archive {} ({}), kernel {}, {} boot file(s)",
            manifest.source.archive_name,
            &manifest.source.archive_sha256[..16],
            manifest.source.kernel_file,
            manifest.boot_files.len()
        );

        let report = verify_against_image(&manifest, &dest).unwrap();
        for finding in &report.findings {
            println!("  manifest finding: {finding:?}");
        }
        assert!(report.matches(), "{:?}", report.findings);

        // G8, on the same real card: the gate the file goes through before it
        // is handed over.
        let health = crate::core::card::health::check_image(
            &dest,
            Some(&manifest),
            &format!("{:?}", req.hardware.pi),
        )
        .unwrap();
        for item in &health.items {
            println!("  {:?} {:?}", item.state, item.check);
        }
        for step in &health.by_hand {
            println!("  by hand: {step:?}");
        }
        assert!(health.ok(), "{:?}", health.items);
    }

    /// The one that matters: the plan the user approved is the card that gets
    /// built. Both go through the same spec, so a screen cannot show one card
    /// and write another.
    #[test]
    fn the_plan_describes_the_card_the_build_produces() {
        use crate::core::jobs::NoProgress;

        let dir = scratch("agree");
        let archive = emu68_zip(&dir);
        let dest = dir.join("card.img");
        let req = request(&archive, &dest);

        let plan = card_plan_build(req.clone()).unwrap();
        let built = build_requested_card(&req, &NoProgress).unwrap();

        assert_eq!(
            plan.layout, built.layout,
            "the card that was written is the one that was described"
        );
        assert_eq!(built.verified.card.areas.len(), 1);
        assert_eq!(
            built.verified.card.areas[0].offset_bytes,
            plan.layout.areas[0].start_bytes(),
            "and the Amiga disk is where the plan put it"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
