//! The registered workflow set — ART's answer to "what can I do with this?".
//!
//! Spec §46 and §91: every recognised object must expose useful actions, and no
//! object is a dead end. This module is the single place where that catalogue
//! lives; `lib.rs::build_engine()` registers everything returned by [`all`].
//!
//! Most entries are [`WorkflowKind::Navigate`]: they open a studio with the
//! dropped object loaded. That is a deliberate split — the engine decides *what
//! is offered and in what order*, the UI only knows how to open a route. A few
//! entries are [`WorkflowKind::Execute`] and do real work here.
//!
//! Ordering is by `priority` (lower first). `Recommended` entries become the
//! starred suggestions; `Standard` and `Advanced` fill the rest of the panel.
//! Anything not yet implemented is registered with `available: false` so it
//! shows as "Coming Later" rather than silently missing (spec §96).

use std::path::Path;

use super::types::{
    Safety, Workflow, WorkflowCategory, WorkflowInfo, WorkflowKind, WorkflowOutcome,
};
use crate::core::detect::{Detection, FormatCategory};
use crate::core::error::CoreResult;

// ---------------------------------------------------------------------------
// Routes (must match the paths declared in src/App.tsx)
// ---------------------------------------------------------------------------

mod route {
    pub const ADF: &str = "/disk-tools";
    pub const LHA: &str = "/archive-tools";
    pub const HDF: &str = "/hard-disk";
    pub const WINUAE: &str = "/winuae";
    pub const ROM: &str = "/rom";
    pub const GOTEK: &str = "/gotek";
    pub const COLLECTION: &str = "/collection";
    pub const HEX: &str = "/tools";
    pub const FILES: &str = "/files";
    pub const WHDLOAD: &str = "/whdload";
    pub const OS_BUILDER: &str = "/os-builder";
    pub const LAYOUT: &str = "/layout";
}

// ---------------------------------------------------------------------------
// Detection predicates
// ---------------------------------------------------------------------------

fn is_floppy(d: &Detection) -> bool {
    d.category == FormatCategory::FloppyImage
}
fn is_archive(d: &Detection) -> bool {
    d.category == FormatCategory::Archive
}
/// LHA specifically, not "any archive".
///
/// The distinction earns its keep from Phase 2a Task 4: `Archive` used to mean
/// LHA and nothing else, so every LHA action was written against the category.
/// Detection recognises ZIP and 7z now, and LHA Studio cannot open either —
/// offering "Open in LHA Studio" for a ZIP would be an action that exists to
/// fail (§46, §89).
fn is_lha(d: &Detection) -> bool {
    is_archive(d) && d.format_hint == "lha"
}
fn is_harddisk(d: &Detection) -> bool {
    d.category == FormatCategory::HardDiskImage
}
/// A hard-disk image ART cannot read as a sequence of disk sectors, because
/// its data is not laid out that way.
///
/// **The owner's own `AmiKit.hdf` is one**: a *dynamic* VHD under an `.hdf`
/// name, whose offset 0 holds a copy of its own footer rather than the disk's
/// first sector. A *fixed* VHD is deliberately not here — that one is a raw
/// image with 512 bytes appended, so every reader works on it unchanged.
///
/// `"vhd"` bare is the disk type the specification does not name, and it is
/// treated as opaque for the reason `core::vhd::VhdKind` gives: a layout
/// nobody has read the specification for is not a layout to assume.
fn is_opaque_container(d: &Detection) -> bool {
    matches!(
        d.format_hint.as_str(),
        "vhd-dynamic" | "vhd-differencing" | "vhd"
    )
}
/// A hard-disk image whose sectors really are where a reader would look.
///
/// The same shape as [`is_lha`] above and for the same reason: offering "Open
/// in Hard Disk Studio" for a dynamic VHD is an action that exists to fail
/// (§46, §89), and it would fail by *saying* the disk has no partition table
/// — a true sentence about the bytes at offset 0 and a wrong one about the
/// disk.
///
/// Hex and Collection deliberately keep the wider [`is_harddisk`]: hashing a
/// file works whatever is inside it, and a hex viewer showing raw bytes is
/// exactly what somebody looking at an unreadable container wants.
fn is_raw_harddisk(d: &Detection) -> bool {
    is_harddisk(d) && !is_opaque_container(d)
}
fn is_rom(d: &Detection) -> bool {
    d.category == FormatCategory::Rom
}
fn is_directory(d: &Detection) -> bool {
    d.category == FormatCategory::Directory
}
fn is_optical(d: &Detection) -> bool {
    d.category == FormatCategory::OpticalImage
}
/// A Commodore 8-bit container ART can walk into: D64, D71, D81, T64.
fn is_c64_browsable(d: &Detection) -> bool {
    d.category == FormatCategory::Commodore8Bit && crate::core::cbm::is_browsable(&d.format_hint)
}
/// The Commodore formats ART identifies and deliberately does not browse —
/// TAP, PRG, CRT. They are not dead ends: `c64.identify` says what they are,
/// which for a TAP is the honest whole answer (§10, §89).
fn is_c64_identify_only(d: &Detection) -> bool {
    d.category == FormatCategory::Commodore8Bit && !crate::core::cbm::is_browsable(&d.format_hint)
}
/// Any recognised file (not a directory, not unknown).
fn is_known_file(d: &Detection) -> bool {
    !d.is_dir && d.category != FormatCategory::Unknown
}
/// Anything ART can hold in its collection.
///
/// `FormatCategory::OpticalImage` is deliberately absent: ART can open a disc
/// now (`core::iso`) and browse it in the file manager, but nothing hashes or
/// catalogues one yet — the Collection Studio has no ISO code path, and
/// claiming otherwise would overclaim support (spec §10, §89). It still
/// isn't a dead end — `iso.browse` offers the file manager,
/// `os.install-from-disc` offers the OS Builder, and `any.hex` below accepts
/// any known-but-not-collectable file too.
fn is_collectable(d: &Detection) -> bool {
    matches!(
        d.category,
        FormatCategory::FloppyImage
            | FormatCategory::HardDiskImage
            | FormatCategory::Archive
            | FormatCategory::Rom
    )
}
/// Only a plain uncompressed ADF can be written to a Gotek as-is. ADZ and DMS
/// have to be converted first, so offering "copy to Gotek" for them would be a
/// promise ART cannot keep (spec §89).
fn is_raw_adf(d: &Detection) -> bool {
    d.format_hint == "adf"
}

// ---------------------------------------------------------------------------
// Navigation workflows
// ---------------------------------------------------------------------------

/// A workflow whose entire job is to open the right studio with the object
/// loaded. One implementation serves them all; the catalogue below is data.
pub struct NavWorkflow {
    info: WorkflowInfo,
    accepts: fn(&Detection) -> bool,
}

impl Workflow for NavWorkflow {
    fn info(&self) -> &WorkflowInfo {
        &self.info
    }
    fn can_handle(&self, detection: &Detection) -> bool {
        (self.accepts)(detection)
    }
    // `run` uses the trait default: navigation is the UI's job.
}

/// Terse constructor so the catalogue below reads as a table.
#[allow(clippy::too_many_arguments)]
const fn nav(
    id: &'static str,
    name: &'static str,
    description: &'static str,
    route: &'static str,
    category: WorkflowCategory,
    priority: u32,
    available: bool,
    accepts: fn(&Detection) -> bool,
) -> NavWorkflow {
    NavWorkflow {
        info: WorkflowInfo {
            id,
            name,
            description,
            category,
            // Opening a studio never writes anything; the operations *inside*
            // the studio carry their own safety classification.
            safety: Safety::ReadOnly,
            priority,
            available,
            kind: WorkflowKind::Navigate { route },
        },
        accepts,
    }
}

// ---------------------------------------------------------------------------
// Execute workflows
// ---------------------------------------------------------------------------

/// Read-only integrity check of a floppy image.
pub struct AdfValidate;

impl Workflow for AdfValidate {
    fn info(&self) -> &WorkflowInfo {
        &WorkflowInfo {
            id: "adf.validate",
            name: "Check Disk Health",
            description: "Verify the boot block, checksums and bitmap without changing anything.",
            category: WorkflowCategory::Standard,
            safety: Safety::ReadOnly,
            priority: 60,
            available: true,
            kind: WorkflowKind::Execute,
        }
    }
    fn can_handle(&self, d: &Detection) -> bool {
        // Only a raw ADF can be validated in place; ADZ/DMS need unpacking first.
        is_raw_adf(d)
    }
    fn run(&self, input: &Path, _d: &Detection) -> CoreResult<WorkflowOutcome> {
        use crate::core::adf::{AdfImage, HealthStatus};

        let report = AdfImage::open(input)?.validate()?;
        let headline = match report.status {
            HealthStatus::Healthy => "Healthy — no problems found.".to_string(),
            HealthStatus::Warning | HealthStatus::Problem => {
                let details: Vec<String> =
                    report.findings.iter().map(|f| f.message.clone()).collect();
                format!("{:?}: {}", report.status, details.join("; "))
            }
        };

        Ok(WorkflowOutcome {
            workflow_id: "adf.validate".into(),
            success: true,
            message: headline,
            verification: Some(report.status == HealthStatus::Healthy),
        })
    }
}

/// SHA256 hashing of any recognised file.
pub struct Hash;

impl Workflow for Hash {
    fn info(&self) -> &WorkflowInfo {
        &WorkflowInfo {
            id: "analyze.hash",
            name: "Compute SHA256",
            description: "Compute a SHA256 hash for integrity and duplicate detection.",
            category: WorkflowCategory::Advanced,
            safety: Safety::ReadOnly,
            priority: 90,
            available: true,
            kind: WorkflowKind::Execute,
        }
    }
    fn can_handle(&self, d: &Detection) -> bool {
        is_known_file(d)
    }
    fn run(&self, input: &Path, _d: &Detection) -> CoreResult<WorkflowOutcome> {
        let digest = crate::core::hashing::sha256_file(input)?;
        Ok(WorkflowOutcome {
            workflow_id: "analyze.hash".into(),
            success: true,
            message: digest,
            verification: Some(true),
        })
    }
}

/// What a Commodore file ART does not browse actually is.
///
/// The counterpart to "identify only" being a real answer rather than a
/// refusal: a TAP has no directory in it, and saying so — with the size, and
/// with what reading it would actually take — is the whole of what ART can
/// honestly offer. Registering nothing here would leave those files with only
/// the Advanced catch-alls and no starred action at all (§46).
pub struct C64Identify;

impl Workflow for C64Identify {
    fn info(&self) -> &WorkflowInfo {
        &WorkflowInfo {
            id: "c64.identify",
            name: "What is this?",
            description: "Name the format, its size and why there is nothing inside it to open.",
            category: WorkflowCategory::Recommended,
            safety: Safety::ReadOnly,
            priority: 10,
            available: true,
            kind: WorkflowKind::Execute,
        }
    }
    fn can_handle(&self, d: &Detection) -> bool {
        is_c64_identify_only(d)
    }
    fn run(&self, input: &Path, d: &Detection) -> CoreResult<WorkflowOutcome> {
        Ok(WorkflowOutcome {
            workflow_id: "c64.identify".into(),
            success: true,
            message: crate::core::cbm::identify(input, &d.format_hint)?,
            verification: None,
        })
    }
}

// ---------------------------------------------------------------------------
// The catalogue
// ---------------------------------------------------------------------------

use WorkflowCategory::{Advanced, Recommended, Standard};

/// Every navigation workflow ART offers, in catalogue order.
///
/// Priorities are grouped so the starred actions of each format land first:
/// 10–29 primary, 30–59 secondary, 60+ advanced.
fn navigation_workflows() -> Vec<NavWorkflow> {
    vec![
        // --- Floppy images (ADF / ADZ / DMS) ---
        nav(
            "adf.browse",
            "Open in ADF Studio",
            "Browse the disk's filesystem, extract files, and edit its contents.",
            route::ADF,
            Recommended,
            10,
            true,
            is_floppy,
        ),
        nav(
            "adf.launch_winuae",
            "Launch in WinUAE",
            "Boot this disk in the WinUAE emulator.",
            route::WINUAE,
            Recommended,
            20,
            true,
            is_floppy,
        ),
        nav(
            "adf.add_collection",
            "Add to Collection",
            "Catalogue this disk with its hash so duplicates can be spotted.",
            route::COLLECTION,
            Recommended,
            30,
            true,
            is_floppy,
        ),
        nav(
            "adf.copy_gotek",
            "Copy to Gotek",
            "Validate and copy this disk to a Gotek USB drive.",
            route::GOTEK,
            Standard,
            40,
            true,
            is_raw_adf,
        ),
        // Stage W made this real: the two-pane manager copies between any two
        // volumes, and a floppy and a partition are both volumes.
        nav(
            "adf.install_hdf",
            "Copy into a hard disk image",
            "Open this disk beside a hard disk image and copy between them.",
            route::FILES,
            Standard,
            50,
            true,
            is_floppy,
        ),
        nav(
            "adf.hex",
            "Inspect in Hex Viewer",
            "Examine raw blocks, sectors and boot code.",
            route::HEX,
            Advanced,
            80,
            true,
            is_floppy,
        ),
        // --- Commodore 8-bit ---
        //
        // A D64 is the same thing to the commander an ADF is: a container you
        // walk into. It is deliberately *not* offered the Amiga floppy
        // actions — ADF Studio cannot read it, and "copy to Gotek" would put
        // a 1541 image on a device expecting an Amiga floppy.
        nav(
            "c64.browse",
            "Open in the file manager",
            "Walk the disk's directory and copy files out of it.",
            route::FILES,
            Recommended,
            10,
            true,
            is_c64_browsable,
        ),
        // --- Archives ---
        //
        // Every archive ART reads — LHA, ZIP, 7z — opens in the commander as
        // a pane you can walk into and copy out of (Task 4). The LHA-only
        // actions below it stay LHA-only: LHA Studio, WHDLoad detection and
        // install-to-hard-disk are all written against `core::lha` and would
        // fail on a ZIP.
        nav(
            "archive.browse",
            "Open in the file manager",
            "Walk the archive's folders and copy files out of it, to a folder or into an Amiga volume.",
            route::FILES,
            Recommended,
            5,
            true,
            is_archive,
        ),
        nav(
            "lha.browse",
            "Open in LHA Studio",
            "List the archive's contents and check for a WHDLoad package.",
            route::LHA,
            Recommended,
            10,
            true,
            is_lha,
        ),
        nav(
            "lha.extract",
            "Extract Files",
            "Safely extract the archive to a folder you choose.",
            route::LHA,
            Recommended,
            20,
            true,
            is_lha,
        ),
        nav(
            "lha.add_collection",
            "Add to Collection",
            "Catalogue this archive with its hash.",
            route::COLLECTION,
            Recommended,
            30,
            true,
            is_lha,
        ),
        // §82's success scenario, end to end: drop a package, ART detects
        // WHDLoad, and one click puts it on a hard disk with a backup and a
        // verification. `Recommended` because for a WHDLoad archive it is what
        // the user came to do.
        nav(
            "lha.install_hdf",
            "Install to a hard disk",
            "Put this package on a hard disk image — the drawer, its icon, and a check that every file arrived.",
            route::WHDLOAD,
            Recommended,
            25,
            true,
            is_lha,
        ),
        nav(
            "lha.launch_winuae",
            "Launch in WinUAE",
            "Extract and run this package in the emulator.",
            route::WINUAE,
            Standard,
            50,
            false,
            is_lha,
        ),
        // --- Hard disk images (HDF / HDZ) ---
        nav(
            "hdf.browse",
            "Open in Hard Disk Studio",
            "Inspect partitions, RDB and filesystem contents.",
            route::HDF,
            Recommended,
            10,
            true,
            is_raw_harddisk,
        ),
        nav(
            "hdf.launch_winuae",
            "Launch in WinUAE",
            "Boot this hard disk image in the emulator.",
            route::WINUAE,
            Recommended,
            20,
            true,
            is_raw_harddisk,
        ),
        nav(
            "hdf.add_collection",
            "Add to Collection",
            "Catalogue this image with its hash.",
            route::COLLECTION,
            Standard,
            30,
            true,
            is_harddisk,
        ),
        nav(
            "hdf.hex",
            "Inspect in Hex Viewer",
            "Examine the RDB, partition blocks and raw sectors.",
            route::HEX,
            Advanced,
            80,
            true,
            is_harddisk,
        ),
        // --- Kickstart ROMs ---
        nav(
            "rom.identify",
            "Identify in ROM Studio",
            "Determine the Kickstart version, size and checksum.",
            route::ROM,
            Recommended,
            10,
            true,
            is_rom,
        ),
        nav(
            "rom.use_in_profile",
            "Use in a Machine Profile",
            "Attach this ROM to an Amiga profile for launching.",
            route::WINUAE,
            Recommended,
            20,
            true,
            is_rom,
        ),
        nav(
            "rom.hex",
            "Inspect in Hex Viewer",
            "Examine the ROM header and raw contents.",
            route::HEX,
            Advanced,
            80,
            true,
            is_rom,
        ),
        // --- Dropped folders ---
        //
        // ART-108: the layout screen's own framing is "drop four hundred
        // files, get an organised card", and until this entry existed there
        // was no way to reach it by dropping anything — the sidebar and a
        // file dialog were the only doors into the module ART's one drop
        // pipeline was built to feed.
        //
        // **Ranked** below `dir.scan_collection`, not registered below it.
        // This entry sits *first* in the list and carries priority 15 against
        // its 10, and priority is what orders the panel — source order is
        // not. The old wording read backwards against the code directly under
        // it (F10 of the wave-C1 review).
        //
        // Both are starred, so neither folder action is a dead end (§46);
        // which of "catalogue this folder" and "lay this folder out onto a
        // card" should be offered *first* is a product judgement, and this
        // task's job was to open the door, not to reorder the room.
        nav(
            "dir.organise",
            "Organise onto a card",
            "Sort this folder's games, floppies and archives into a staging tree \
             ready to copy onto a PiStorm card.",
            route::LAYOUT,
            Recommended,
            15,
            true,
            is_directory,
        ),
        nav(
            "dir.scan_collection",
            "Scan into Collection",
            "Catalogue every Amiga file in this folder, detecting duplicates.",
            route::COLLECTION,
            Recommended,
            10,
            true,
            is_directory,
        ),
        nav(
            "dir.to_hdf",
            "Build an HDF from this folder",
            "Create a hard disk image containing this folder's contents.",
            route::HDF,
            Recommended,
            20,
            false,
            is_directory,
        ),
        nav(
            "dir.prepare_gotek",
            "Prepare as Gotek drive",
            "Organise the disk images in this folder for a Gotek.",
            route::GOTEK,
            Standard,
            30,
            true,
            is_directory,
        ),
        // --- Optical discs (ISO9660 / Joliet) ---
        nav(
            "iso.browse",
            "Open in the File Manager",
            "Browse the disc's filesystem and copy files out — a disc is read-only.",
            route::FILES,
            Recommended,
            10,
            true,
            is_optical,
        ),
        nav(
            "os.install-from-disc",
            "Build an AmigaOS system from this disc",
            "Use the disc as install media in the OS Builder. ART reads what is \
             on it and tells you what it can install — a disc that is not \
             install media simply offers nothing.",
            route::OS_BUILDER,
            Recommended,
            20,
            true,
            is_optical,
        ),
        // --- Anything collectable ---
        nav(
            "any.hex",
            "Inspect in Hex Viewer",
            "Examine the raw bytes of this file.",
            route::HEX,
            Advanced,
            85,
            true,
            |d| is_known_file(d) && !is_collectable(d),
        ),
    ]
}

/// Register every workflow ART knows about onto `registry`.
pub fn register_all(registry: &mut super::registry::WorkflowRegistry) {
    for w in navigation_workflows() {
        registry.register(w);
    }
    registry.register(AdfValidate);
    registry.register(Hash);
    registry.register(C64Identify);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::detect::Detection;
    use crate::core::workflow::{WorkflowEngine, WorkflowRegistry};

    fn engine() -> WorkflowEngine {
        let mut reg = WorkflowRegistry::new();
        register_all(&mut reg);
        WorkflowEngine::new(reg)
    }

    fn detection(category: FormatCategory, hint: &str, is_dir: bool) -> Detection {
        Detection {
            category,
            format_hint: hint.to_string(),
            confidence: 0.9,
            size: if is_dir { 0 } else { 901_120 },
            is_dir,
        }
    }

    fn ids_for(d: &Detection) -> Vec<&'static str> {
        let mut reg = WorkflowRegistry::new();
        register_all(&mut reg);
        reg.candidates_for(d).iter().map(|w| w.info().id).collect()
    }

    /// ART-108: ART has exactly one drop pipeline, and until `dir.organise`
    /// existed nothing dropped could reach the layout screen at all — the
    /// module whose own framing is "drop four hundred files, get an organised
    /// card" was reachable only from the sidebar and a file dialog.
    ///
    /// Asserts the route as well as the id: an entry that pointed somewhere
    /// else would satisfy a bare "is it registered" check and still leave the
    /// screen unreachable.
    #[test]
    fn a_dropped_folder_can_reach_the_layout_screen() {
        let d = detection(FormatCategory::Directory, "directory", true);
        assert!(ids_for(&d).contains(&"dir.organise"), "{:?}", ids_for(&d));

        let entry = navigation_workflows()
            .into_iter()
            .find(|w| w.info.id == "dir.organise")
            .expect("registered");
        match entry.info.kind {
            WorkflowKind::Navigate { route } => assert_eq!(route, "/layout"),
            WorkflowKind::Execute => panic!("the layout screen is navigated to, not executed"),
        }
    }

    /// Spec §91: no recognised object may be a dead end.
    ///
    /// Includes `OpticalImage`: `iso.browse` (below) gives it a dedicated,
    /// starred action now that the file manager can open one.
    #[test]
    fn every_recognised_format_offers_actions() {
        let cases = [
            (FormatCategory::FloppyImage, "adf", false),
            (FormatCategory::HardDiskImage, "hdf", false),
            (FormatCategory::Archive, "lha", false),
            // ZIP and 7z are archives ART reads but LHA Studio cannot open.
            // They would have been dead ends the moment detection learned to
            // recognise them, if `archive.browse` did not exist (§91).
            (FormatCategory::Archive, "zip", false),
            (FormatCategory::Archive, "7z", false),
            (FormatCategory::Rom, "rom", false),
            (FormatCategory::Directory, "directory", true),
            (FormatCategory::OpticalImage, "iso9660", false),
            // Both halves of the Commodore side: one ART opens, one it only
            // names. Neither may be a dead end (§91).
            (FormatCategory::Commodore8Bit, "d64", false),
            (FormatCategory::Commodore8Bit, "tap", false),
        ];

        for (category, hint, is_dir) in cases {
            let d = detection(category, hint, is_dir);
            let ids = ids_for(&d);
            assert!(
                !ids.is_empty(),
                "{hint} produced no candidate workflows — dead-end object"
            );
        }
    }

    /// Spec §46: every object needs at least one starred (Recommended) action.
    #[test]
    fn every_recognised_format_has_a_recommendation() {
        let dir =
            std::env::temp_dir().join(format!("art-wf-rec-{}", crate::core::test_scratch_id()));
        std::fs::create_dir_all(&dir).unwrap();

        let adf = dir.join("game.adf");
        std::fs::write(&adf, vec![0u8; crate::core::detect::sizes::ADF_DD as usize]).unwrap();

        let plan = engine().plan(&adf).unwrap();
        assert!(
            !plan.recommendations.is_empty(),
            "an ADF must produce starred actions"
        );
        assert!(plan
            .recommendations
            .iter()
            .any(|r| r.info.id == "adf.browse"));

        // A dropped folder must also lead somewhere.
        let plan = engine().plan(&dir).unwrap();
        assert!(plan
            .recommendations
            .iter()
            .any(|r| r.info.id == "dir.scan_collection"));

        // And a real ZIP, dropped: it is an `Archive` like an LHA, but LHA
        // Studio cannot open one, so the LHA actions must *not* be offered
        // and the commander must be — otherwise recognising ZIP at all would
        // have turned it into a dead end with three broken buttons.
        let zip = dir.join("pack.zip");
        std::fs::write(
            &zip,
            crate::core::archive::zip::tests::make_zip_with(&[("readme.txt", b"hi")]),
        )
        .unwrap();

        let plan = engine().plan(&zip).unwrap();
        let ids: Vec<&str> = plan.candidates.iter().map(|c| c.id).collect();
        assert!(
            ids.contains(&"archive.browse"),
            "a ZIP must open in the commander: {ids:?}"
        );
        assert!(
            !ids.iter().any(|id| id.starts_with("lha.")),
            "no LHA-only action may be offered for a ZIP: {ids:?}"
        );
        assert!(plan
            .recommendations
            .iter()
            .any(|r| r.info.id == "archive.browse"));

        // A C64 disk: the commander, and none of the Amiga floppy actions —
        // ADF Studio cannot read a 1541 image, and `adf.to_gotek` would put
        // one on a device expecting an Amiga floppy.
        let d64 = dir.join("game.d64");
        std::fs::write(
            &d64,
            crate::core::cbm::d64::fixture::D64Builder::new(35).build(),
        )
        .unwrap();

        let plan = engine().plan(&d64).unwrap();
        let ids: Vec<&str> = plan.candidates.iter().map(|c| c.id).collect();
        assert!(ids.contains(&"c64.browse"), "{ids:?}");
        assert!(
            !ids.iter().any(|id| id.starts_with("adf.")),
            "an Amiga floppy action was offered for a 1541 image: {ids:?}"
        );
        assert!(plan
            .recommendations
            .iter()
            .any(|r| r.info.id == "c64.browse"));

        // And a TAP, which ART deliberately does not browse: it still gets a
        // starred action, because "what is this?" is a real answer.
        let tap = dir.join("game.tap");
        std::fs::write(&tap, b"C64-TAPE-RAW\x00\x00\x00\x00").unwrap();

        let plan = engine().plan(&tap).unwrap();
        let ids: Vec<&str> = plan.candidates.iter().map(|c| c.id).collect();
        assert!(ids.contains(&"c64.identify"), "{ids:?}");
        assert!(
            !ids.contains(&"c64.browse"),
            "a TAP has no directory to open: {ids:?}"
        );
        assert!(plan
            .recommendations
            .iter()
            .any(|r| r.info.id == "c64.identify"));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The candidate list, not the whole `plan()` pipeline (which needs a
    /// real optical-disc file on disk to detect): an optical image must get
    /// its own starred action now that the file manager opens one, not just
    /// fall through to `any.hex`.
    #[test]
    fn an_optical_image_recommends_the_file_manager() {
        let d = detection(FormatCategory::OpticalImage, "iso9660", false);
        let ids = ids_for(&d);
        assert!(ids.contains(&"iso.browse"), "got {ids:?}");

        let mut reg = WorkflowRegistry::new();
        register_all(&mut reg);
        let recommended: Vec<&str> = reg
            .candidates_for(&d)
            .into_iter()
            .filter(|w| w.info().category == Recommended)
            .map(|w| w.info().id)
            .collect();
        assert!(recommended.contains(&"iso.browse"), "got {recommended:?}");
    }

    /// The owner's request: an ISO dropped on the panel must offer the OS
    /// Builder, not only the file manager.
    #[test]
    fn an_optical_image_offers_the_os_builder() {
        let d = detection(FormatCategory::OpticalImage, "iso9660", false);
        let ids = ids_for(&d);
        assert!(ids.contains(&"os.install-from-disc"), "got {ids:?}");
        assert!(ids.contains(&"iso.browse"), "got {ids:?}");
    }

    /// Browsing is always right; installing is an offer. The file manager
    /// therefore stays first in the list (§89 — ART does not assert that a
    /// disc is install media before it has looked).
    #[test]
    fn browsing_a_disc_is_listed_before_installing_from_it() {
        let d = detection(FormatCategory::OpticalImage, "iso9660", false);
        let ids = ids_for(&d);
        // Both unwrapped before comparing. As two `Option<usize>` this read
        // `None < Some(n)`, which is `true` — so the test passed, asserting
        // nothing, if `iso.browse` ever stopped being offered at all.
        let browse = ids
            .iter()
            .position(|i| *i == "iso.browse")
            .unwrap_or_else(|| panic!("iso.browse must be offered for a disc at all: {ids:?}"));
        let install = ids
            .iter()
            .position(|i| *i == "os.install-from-disc")
            .unwrap_or_else(|| {
                panic!("os.install-from-disc must be offered for a disc at all: {ids:?}")
            });
        assert!(
            browse < install,
            "iso.browse must precede os.install-from-disc: {ids:?}"
        );
    }

    // -----------------------------------------------------------------
    // Work-list item 7: a hard-disk image ART cannot read as sectors.
    // -----------------------------------------------------------------

    /// **The owner's `AmiKit.hdf`.** Offering "Open in Hard Disk Studio" for a
    /// dynamic VHD is an action that exists to fail (§46, §89) — and it would
    /// fail by *saying* the disk has no partition table, which is true about
    /// the bytes at offset 0 and wrong about the disk.
    #[test]
    fn a_container_art_cannot_read_as_sectors_is_not_offered_the_raw_studios() {
        for hint in ["vhd-dynamic", "vhd-differencing", "vhd"] {
            let ids = ids_for(&detection(FormatCategory::HardDiskImage, hint, false));
            assert!(
                !ids.contains(&"hdf.browse"),
                "{hint} must not offer the Hard Disk Studio: {ids:?}"
            );
            assert!(
                !ids.contains(&"hdf.launch_winuae"),
                "{hint} must not offer WinUAE: {ids:?}"
            );
        }
    }

    /// **And it is not a dead end** (§91). Hashing a file works whatever is
    /// inside it, and a hex viewer showing raw bytes is exactly what somebody
    /// looking at a container ART cannot open wants to see.
    #[test]
    fn it_still_has_somewhere_to_go() {
        for hint in ["vhd-dynamic", "vhd-differencing", "vhd"] {
            let ids = ids_for(&detection(FormatCategory::HardDiskImage, hint, false));
            assert!(!ids.is_empty(), "{hint} is a dead end");
            assert!(ids.contains(&"hdf.hex"), "{hint}: {ids:?}");
            assert!(ids.contains(&"hdf.add_collection"), "{hint}: {ids:?}");
        }
    }

    /// **A fixed VHD is deliberately not narrowed.** It is a raw image with
    /// 512 bytes appended, so every reader works on it unchanged — and a test
    /// that lumped it in with the others would be pinning a restriction ART
    /// does not need and the user would feel.
    #[test]
    fn a_fixed_vhd_keeps_every_offer_an_ordinary_image_has() {
        let fixed = ids_for(&detection(
            FormatCategory::HardDiskImage,
            "vhd-fixed",
            false,
        ));
        let plain = ids_for(&detection(FormatCategory::HardDiskImage, "hdf", false));
        assert_eq!(fixed, plain);
        assert!(fixed.contains(&"hdf.browse"));
    }

    /// The offer is for discs only. A floppy image goes to the ADF studio and
    /// an install ADF set is chosen inside the OS Builder itself, so nothing
    /// else may pick this workflow up.
    #[test]
    fn only_a_disc_offers_the_os_builder() {
        for (category, hint) in [
            (FormatCategory::FloppyImage, "adf"),
            (FormatCategory::HardDiskImage, "hdf"),
            (FormatCategory::Archive, "lha"),
            (FormatCategory::Rom, "rom"),
            (FormatCategory::Commodore8Bit, "d64"),
        ] {
            let ids = ids_for(&detection(category, hint, false));
            assert!(
                !ids.contains(&"os.install-from-disc"),
                "{hint} must not offer os.install-from-disc: {ids:?}"
            );
        }
    }

    #[test]
    fn candidates_are_ordered_by_priority() {
        let d = detection(FormatCategory::FloppyImage, "adf", false);
        let ids = ids_for(&d);
        assert_eq!(ids.first(), Some(&"adf.browse"), "got {ids:?}");
        assert_eq!(ids.last(), Some(&"analyze.hash"), "got {ids:?}");
    }

    #[test]
    fn workflows_do_not_cross_formats() {
        let adf_ids = ids_for(&detection(FormatCategory::FloppyImage, "adf", false));
        assert!(!adf_ids.iter().any(|id| id.starts_with("lha.")));
        assert!(!adf_ids.iter().any(|id| id.starts_with("rom.")));

        let rom_ids = ids_for(&detection(FormatCategory::Rom, "rom", false));
        assert!(!rom_ids.iter().any(|id| id.starts_with("adf.")));
    }

    /// A compressed ADZ cannot be written to a Gotek without conversion, so it
    /// must not be offered — claiming otherwise breaks spec §89.
    #[test]
    fn gotek_copy_is_offered_only_for_raw_adf() {
        let adf = ids_for(&detection(FormatCategory::FloppyImage, "adf", false));
        assert!(adf.contains(&"adf.copy_gotek"));

        let adz = ids_for(&detection(FormatCategory::FloppyImage, "adz", false));
        assert!(!adz.contains(&"adf.copy_gotek"), "got {adz:?}");
    }

    #[test]
    fn unknown_objects_offer_nothing() {
        let d = detection(FormatCategory::Unknown, "unknown", false);
        assert!(ids_for(&d).is_empty());
    }

    #[test]
    fn navigation_workflows_refuse_to_execute() {
        let d = detection(FormatCategory::FloppyImage, "adf", false);
        let mut reg = WorkflowRegistry::new();
        register_all(&mut reg);
        let browse = reg
            .candidates_for(&d)
            .into_iter()
            .find(|w| w.info().id == "adf.browse")
            .unwrap();

        assert!(browse.run(Path::new("x.adf"), &d).is_err());
    }

    #[test]
    fn every_workflow_route_is_a_real_app_route() {
        // Kept in sync with the <Route> paths in src/App.tsx.
        const ROUTES: &[&str] = &[
            route::ADF,
            route::LHA,
            route::HDF,
            route::WINUAE,
            route::ROM,
            route::GOTEK,
            route::COLLECTION,
            route::HEX,
            route::FILES,
            route::WHDLOAD,
            route::OS_BUILDER,
            route::LAYOUT,
        ];

        for w in navigation_workflows() {
            match w.info.kind {
                WorkflowKind::Navigate { route } => assert!(
                    ROUTES.contains(&route),
                    "{} points at unknown route {route}",
                    w.info.id
                ),
                WorkflowKind::Execute => panic!("{} should be a navigation workflow", w.info.id),
            }
        }
    }

    #[test]
    fn workflow_ids_are_unique() {
        let mut reg = WorkflowRegistry::new();
        register_all(&mut reg);
        let mut ids: Vec<&str> = reg.list().iter().map(|i| i.id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate workflow id");
    }

    #[test]
    fn validate_workflow_reports_a_healthy_disk() {
        use crate::core::adf::create::create_blank_adf;
        use crate::core::adf::FileSystemType;

        let dir =
            std::env::temp_dir().join(format!("art-wf-val-{}", crate::core::test_scratch_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let disk = dir.join("blank.adf");
        std::fs::write(
            &disk,
            create_blank_adf("Healthy", FileSystemType::Ffs, false).unwrap(),
        )
        .unwrap();

        let d = crate::core::detect::detect(&disk).unwrap();
        assert!(AdfValidate.can_handle(&d));

        let outcome = AdfValidate.run(&disk, &d).unwrap();
        assert!(outcome.success);
        assert_eq!(outcome.verification, Some(true));
        assert!(outcome.message.contains("Healthy"), "{}", outcome.message);

        std::fs::remove_dir_all(&dir).ok();
    }
}
