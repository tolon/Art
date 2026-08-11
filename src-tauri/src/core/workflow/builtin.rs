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
fn is_harddisk(d: &Detection) -> bool {
    d.category == FormatCategory::HardDiskImage
}
fn is_rom(d: &Detection) -> bool {
    d.category == FormatCategory::Rom
}
fn is_directory(d: &Detection) -> bool {
    d.category == FormatCategory::Directory
}
/// Any recognised file (not a directory, not unknown).
fn is_known_file(d: &Detection) -> bool {
    !d.is_dir && d.category != FormatCategory::Unknown
}
/// Anything ART can hold in its collection.
///
/// `FormatCategory::OpticalImage` is deliberately absent: detection can now
/// name an ISO by content (see `core::detect`), but ART does not yet read
/// one, and cataloguing a format it cannot open would overclaim support
/// (spec §10, §89). It still isn't a dead end — `any.hex` below accepts any
/// known-but-not-collectable file, so "Inspect in Hex Viewer" is offered.
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
        // --- Archives (LHA) ---
        nav(
            "lha.browse",
            "Open in LHA Studio",
            "List the archive's contents and check for a WHDLoad package.",
            route::LHA,
            Recommended,
            10,
            true,
            is_archive,
        ),
        nav(
            "lha.extract",
            "Extract Files",
            "Safely extract the archive to a folder you choose.",
            route::LHA,
            Recommended,
            20,
            true,
            is_archive,
        ),
        nav(
            "lha.add_collection",
            "Add to Collection",
            "Catalogue this archive with its hash.",
            route::COLLECTION,
            Recommended,
            30,
            true,
            is_archive,
        ),
        // §82's success scenario, end to end: drop a package, ART detects
        // WHDLoad, and one click puts it on a hard disk with a backup and a
        // verification. `Recommended` because for a WHDLoad archive it is what
        // the user came to do.
        nav(
            "lha.install_hdf",
            "Install to a hard disk",
            "Put this package on a hard disk image — the drawer, its icon, and a              check that every file arrived.",
            route::WHDLOAD,
            Recommended,
            25,
            true,
            is_archive,
        ),
        nav(
            "lha.launch_winuae",
            "Launch in WinUAE",
            "Extract and run this package in the emulator.",
            route::WINUAE,
            Standard,
            50,
            false,
            is_archive,
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
            is_harddisk,
        ),
        nav(
            "hdf.launch_winuae",
            "Launch in WinUAE",
            "Boot this hard disk image in the emulator.",
            route::WINUAE,
            Recommended,
            20,
            true,
            is_harddisk,
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

    /// Spec §91: no recognised object may be a dead end.
    ///
    /// Includes `OpticalImage`: detection can name an ISO by content now,
    /// but ART has no ISO studio yet, so it falls to the generic `any.hex`
    /// candidate rather than a dedicated one. That still satisfies §91 — see
    /// `every_recognised_format_has_a_recommendation` below for why it is
    /// deliberately *not* also asserted to have a starred action yet.
    #[test]
    fn every_recognised_format_offers_actions() {
        let cases = [
            (FormatCategory::FloppyImage, "adf", false),
            (FormatCategory::HardDiskImage, "hdf", false),
            (FormatCategory::Archive, "lha", false),
            (FormatCategory::Rom, "rom", false),
            (FormatCategory::Directory, "directory", true),
            (FormatCategory::OpticalImage, "iso9660", false),
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
    ///
    /// `OpticalImage` is intentionally excluded from this test. Every entry
    /// in `navigation_workflows()` must point at a route that already exists
    /// in `src/App.tsx` (`every_workflow_route_is_a_real_app_route` enforces
    /// it), and there is no ISO studio route yet — adding one is frontend
    /// work outside this detection-only change. Registering a Recommended
    /// `available: false` placeholder against an existing, unrelated route
    /// (e.g. the Hex Viewer) would be more misleading than honest: it would
    /// claim a dedicated ISO action is merely "Coming Later" when no such
    /// action has been designed yet. `every_recognised_format_offers_actions`
    /// above already proves an optical image is not a dead end (`any.hex`
    /// picks it up); a Recommended-specific action arrives with the ISO
    /// studio itself.
    #[test]
    fn every_recognised_format_has_a_recommendation() {
        let dir = std::env::temp_dir().join(format!(
            "art-wf-rec-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
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

        std::fs::remove_dir_all(&dir).ok();
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

        let dir = std::env::temp_dir().join(format!(
            "art-wf-val-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
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
