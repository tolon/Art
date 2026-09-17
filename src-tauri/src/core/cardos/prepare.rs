//! Measure a whole card, say what free space it needs, and stage it.
//!
//! Three steps the card command runs in order (design § 6):
//!
//! 1. [`measure_card`] — the PFS3 driver first (a missing driver is said
//!    before anything slow), then System's tree and every partition's
//!    sources, classified and measured; each partition checked; each
//!    partition's writer chosen; the card laid out. It writes nothing but a
//!    driver unpacked out of an archive into `driver_stage`.
//! 2. [`space_needs`] and [`check_free_space`] — the image on its folder and
//!    the staging on the scratch folder, summed when they share a volume,
//!    checked through a closure so `core/` never asks Windows itself (P7).
//! 3. [`stage_card`] — every non-folder source unpacked into staging, the
//!    titles found, WHDLoad chosen, Kickstarts proposed (never placed), and
//!    System checked again with WHDLoad and every proposed image added.

use std::path::{Component, Path, PathBuf};

use crate::core::card::sizing::{
    pfs3_data_blocks, pfs3_fits, plan_card_image, CardImagePlan, ContentMeasure,
    RequestedPartition, SizingRefusal, PFS3_BLOCK,
};
use crate::core::cardos::content::{
    self, check_partition, classify, measure, Classified, SourceKind, SourceMeasure, Unusable,
};
use crate::core::cardos::driver::{find_pfs3_driver, FoundDriver};
use crate::core::cardos::kickstarts::{find_title_needs, propose_kickstarts, KickstartProposal};
use crate::core::cardos::searched_folder;
use crate::core::cardos::whdload::{
    find_whdload, read_tree_whdload, tree_has_whdload, WhdloadChoice,
};
use crate::core::clock::AmigaClock;
use crate::core::error::{CoreError, CoreResult, SpacePlace};
use crate::core::jobs::{cancelled_error, ProgressSink};
use crate::core::preload::native::MAX_NAMED_NON_ASCII;
use crate::core::rom::offer::Offer;

/// The volume name ART gives the tree's partition.
const SYSTEM: &str = "System";

/// One of the user's partitions, as the screen asks for it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartitionInput {
    pub volume_name: String,
    pub sources: Vec<PathBuf>,
    #[serde(default)]
    pub floor_bytes: u64,
}

/// Whether hst-imager can fill a partition ART's own writer cannot (ART-113).
/// The command layer asks the tool itself (`probe`) — `core/` never runs a
/// program — and prepare and build use the same answer (card round 3, I1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HstImagerState {
    /// No hst-imager path was given.
    NotConfigured,
    /// A path was given and the tool answered.
    Usable,
    /// A path was given and the tool is missing or did not run.
    Unusable { path: String, why: String },
}

/// Which writer fills a partition.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "writer", rename_all = "kebab-case")]
pub enum PartitionWriter {
    Native,
    HstImager,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasuredSource {
    pub path: PathBuf,
    pub kind: SourceKind,
    #[serde(skip)]
    pub measure: SourceMeasure,
    pub files: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasuredPartition {
    pub volume_name: String,
    pub drive_name: String,
    pub sources: Vec<MeasuredSource>,
    pub writer: PartitionWriter,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasuredCard {
    pub plan: CardImagePlan,
    /// The tree, drive SDH0.
    pub system: MeasuredPartition,
    /// The user's, in order.
    pub partitions: Vec<MeasuredPartition>,
    pub driver: FoundDriver,
    /// Bytes the non-folder sources will take in staging, from their measures.
    pub staging_bytes: u64,
}

/// The sentence for a source's [`Unusable`] reason, said inside
/// [`CoreError::CardSourceUnusable`].
fn unusable_sentence(why: &Unusable) -> String {
    match why {
        Unusable::Missing => "it does not exist".to_string(),
        Unusable::Unreadable { detail } => format!("ART could not read it ({detail})"),
        Unusable::ArchiveUnreadable { detail } => {
            format!("ART could not read the archive ({detail})")
        }
        Unusable::HardfileNotWhdload { detail } => detail.clone(),
        Unusable::NotAnAmigaSource { format_hint } => format!(
            "it is not a folder, an archive, a WHDLoad hardfile or a floppy image ({format_hint})"
        ),
    }
}

/// What one source of `partition` is, or the refusal naming both.
fn classify_source(partition: &str, path: &Path, clock: &dyn AmigaClock) -> CoreResult<SourceKind> {
    match classify(path, clock) {
        Classified::Usable { kind } => Ok(kind),
        Classified::NotUsable { why } => Err(CoreError::CardSourceUnusable {
            partition: partition.to_string(),
            source_path: path.display().to_string(),
            why: unusable_sentence(&why),
        }),
    }
}

/// Measure one source already classified as `kind`.
fn measure_source(
    path: &Path,
    kind: SourceKind,
    clock: &dyn AmigaClock,
    progress: &dyn ProgressSink,
) -> CoreResult<MeasuredSource> {
    let measure = measure(path, &kind, clock, progress)?;
    Ok(MeasuredSource {
        path: path.to_path_buf(),
        files: measure.content.files,
        bytes: measure.content.data_blocks * PFS3_BLOCK,
        kind,
        measure,
    })
}

/// Check a partition's sources together and choose its writer: ART's own
/// PFS3 writer, unless a name is not ASCII (ART-113) — then hst-imager when
/// it answered, and a refusal naming the partition (and the tool, when one
/// was given and did not run) when it did not.
fn check_and_choose_writer(
    partition: &str,
    sources: &[MeasuredSource],
    hst_imager: &HstImagerState,
) -> CoreResult<PartitionWriter> {
    let pairs: Vec<(PathBuf, SourceMeasure)> = sources
        .iter()
        .map(|s| (s.path.clone(), s.measure.clone()))
        .collect();
    check_partition(&pairs)?;

    let mut paths = Vec::new();
    let mut more = 0usize;
    for source in sources {
        for path in &source.measure.non_ascii {
            if paths.len() < MAX_NAMED_NON_ASCII {
                paths.push(path.clone());
            } else {
                more += 1;
            }
        }
        more += source.measure.non_ascii_more;
    }
    if paths.is_empty() && more == 0 {
        return Ok(PartitionWriter::Native);
    }
    match hst_imager {
        HstImagerState::Usable => Ok(PartitionWriter::HstImager),
        HstImagerState::NotConfigured => Err(CoreError::CardNamesNeedHstImager {
            partition: partition.to_string(),
            paths,
            more,
        }),
        HstImagerState::Unusable { path, why } => Err(CoreError::HstImagerUnusable {
            partitions: vec![partition.to_string()],
            path: path.clone(),
            why: why.clone(),
        }),
    }
}

/// Classify and measure every source, check every partition, choose each
/// partition's writer, find the driver, and lay the card out. Writes only
/// `driver_stage` (a driver out of an archive).
#[allow(clippy::too_many_arguments)]
pub fn measure_card(
    card_gb: u32,
    tree: &Path,
    partitions: &[PartitionInput],
    material: &[PathBuf],
    explicit_driver: Option<&Path>,
    driver_stage: &Path,
    hst_imager: &HstImagerState,
    clock: &dyn AmigaClock,
    progress: &dyn ProgressSink,
) -> CoreResult<MeasuredCard> {
    // § 6 order: a missing driver is said before anything slow.
    let driver = find_pfs3_driver(explicit_driver, material, driver_stage)?;

    // System: the tree, which must be a folder.
    if classify_source(SYSTEM, tree, clock)? != SourceKind::Folder {
        return Err(CoreError::CardSourceUnusable {
            partition: SYSTEM.to_string(),
            source_path: tree.display().to_string(),
            why: "the System tree must be a folder".to_string(),
        });
    }
    let system_sources = vec![measure_source(tree, SourceKind::Folder, clock, progress)?];
    let system_writer = check_and_choose_writer(SYSTEM, &system_sources, hst_imager)?;

    let total_sources: usize = partitions.iter().map(|p| p.sources.len()).sum();
    let mut done = 0u64;
    let mut measured = Vec::with_capacity(partitions.len());
    for input in partitions {
        let mut sources = Vec::with_capacity(input.sources.len());
        for path in &input.sources {
            if progress.is_cancelled() {
                return Err(cancelled_error());
            }
            progress.report(
                done,
                Some(total_sources as u64),
                &path.display().to_string(),
            );
            let kind = classify_source(&input.volume_name, path, clock)?;
            sources.push(measure_source(path, kind, clock, progress)?);
            done += 1;
        }
        let writer = check_and_choose_writer(&input.volume_name, &sources, hst_imager)?;
        measured.push((input, sources, writer));
    }

    let requested: Vec<RequestedPartition> = measured
        .iter()
        .map(|(input, sources, _)| {
            let mut content = ContentMeasure::default();
            for source in sources {
                content.merge(&source.measure.content);
            }
            RequestedPartition {
                volume_name: input.volume_name.clone(),
                content: Some(content),
                floor_bytes: input.floor_bytes,
            }
        })
        .collect();
    let plan = plan_card_image(
        card_gb,
        Some(&system_sources[0].measure.content),
        &requested,
    )
    .map_err(CoreError::CardDoesNotFit)?;

    let drive = |index: usize| -> CoreResult<String> {
        plan.partitions
            .get(index)
            .map(|p| p.drive_name.clone())
            .ok_or_else(|| {
                CoreError::InvalidInput(format!(
                    "the card plan has no partition {index} for the partitions asked for"
                ))
            })
    };

    let mut staging_bytes = 0u64;
    let mut user_partitions = Vec::with_capacity(measured.len());
    for (index, (input, sources, writer)) in measured.into_iter().enumerate() {
        for source in &sources {
            if source.kind != SourceKind::Folder {
                staging_bytes += source.measure.content.data_blocks * PFS3_BLOCK;
            }
        }
        user_partitions.push(MeasuredPartition {
            volume_name: input.volume_name.clone(),
            drive_name: drive(index + 1)?,
            sources,
            writer,
        });
    }
    let system = MeasuredPartition {
        volume_name: SYSTEM.to_string(),
        drive_name: drive(0)?,
        sources: system_sources,
        writer: system_writer,
    };

    Ok(MeasuredCard {
        plan,
        system,
        partitions: user_partitions,
        driver,
        staging_bytes,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceNeed {
    pub place: PathBuf,
    pub bytes: u64,
    /// Which place this is, so a refusal names where it is chosen.
    pub what: SpacePlace,
}

/// The image (`plan.image_bytes`) on the image's folder; staging on the
/// scratch folder. Two places on one volume (the same path prefix — a drive
/// letter or a share) are summed into one need at the first place. A mount
/// point inside a folder is not detected (P7, disclosed). A place that needs
/// nothing is not asked about.
pub fn space_needs(card: &MeasuredCard, image: &Path, scratch: &Path) -> Vec<SpaceNeed> {
    let image_folder = image
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(image);
    let wanted = [
        (image_folder, card.plan.image_bytes, SpacePlace::Image),
        (scratch, card.staging_bytes, SpacePlace::Scratch),
    ];
    let mut needs: Vec<(Option<Component<'_>>, SpaceNeed)> = Vec::new();
    for (place, bytes, what) in wanted {
        if bytes == 0 {
            continue;
        }
        let volume = place.components().next();
        match needs.iter_mut().find(|(key, _)| *key == volume) {
            Some((_, need)) => {
                need.bytes += bytes;
                need.what = SpacePlace::ImageAndScratch;
            }
            None => needs.push((
                volume,
                SpaceNeed {
                    place: place.to_path_buf(),
                    bytes,
                    what,
                },
            )),
        }
    }
    needs.into_iter().map(|(_, need)| need).collect()
}

/// Refuse the first need `available` cannot meet, naming the place and both
/// numbers.
pub fn check_free_space(
    needs: &[SpaceNeed],
    available: impl Fn(&Path) -> std::io::Result<u64>,
) -> CoreResult<()> {
    for need in needs {
        let free = available(&need.place).map_err(|err| {
            CoreError::Io(std::io::Error::new(
                err.kind(),
                format!(
                    "ART could not ask how much space is free at '{}': {err}",
                    need.place.display()
                ),
            ))
        })?;
        if free < need.bytes {
            return Err(CoreError::NotEnoughSpace {
                place: need.place.display().to_string(),
                needed: need.bytes,
                available: free,
                what: need.what,
            });
        }
    }
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedCard {
    pub measured: MeasuredCard,
    /// Per user partition, in order: the folders its copy takes (a folder
    /// source itself, or its staging folder).
    pub roots: Vec<Vec<PathBuf>>,
    pub left_behind: Vec<String>,
    pub whdload: Option<WhdloadChoice>,
    /// The tree's own `C/WHDLoad`, when titles need one and the tree already
    /// has it — the tree's copy is kept, and this says which version that is
    /// beside the best one found elsewhere (card round 3, M2).
    pub tree_whdload: Option<TreeWhdload>,
    pub kickstarts: KickstartProposal,
}

/// The tree's own `C/WHDLoad`, which the build keeps, and what else there was.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeWhdload {
    pub path: PathBuf,
    /// As it states itself (`WHDLoad 18.9`); `None` when it states nothing.
    pub name: Option<String>,
    /// The best WHDLoad in the material folders and hardfiles, as it states
    /// itself; `None` when there is none.
    pub best_elsewhere: Option<String>,
    /// True when `best_elsewhere` states a higher version than the tree's
    /// own (or the tree's states none).
    pub newer_elsewhere: bool,
}

/// The largest Kickstart image there is; the upper bound for one whose file
/// ART cannot stat.
const KICKSTART_MAX_BYTES: u64 = 1024 * 1024;
/// What System is charged for one `.RTB` (a real one is a few KB).
const RTB_CHARGE_BYTES: u64 = 8192;

/// Stage every non-folder source into `staging_root/<p>-<s>`, find the
/// titles, choose WHDLoad, propose Kickstarts, and check System again with
/// WHDLoad and every proposed image and `.RTB` added (an upper bound: what the
/// user agrees to later can only be less).
pub fn stage_card(
    measured: MeasuredCard,
    tree: &Path,
    staging_root: &Path,
    material: &[PathBuf],
    collection_dirs: &[PathBuf],
    clock: &dyn AmigaClock,
    progress: &dyn ProgressSink,
) -> CoreResult<PreparedCard> {
    let total_sources: usize = measured.partitions.iter().map(|p| p.sources.len()).sum();
    let mut done = 0u64;
    let mut roots = Vec::with_capacity(measured.partitions.len());
    let mut left_behind = Vec::new();
    let mut hardfiles = Vec::new();
    for (p, partition) in measured.partitions.iter().enumerate() {
        let mut partition_roots = Vec::with_capacity(partition.sources.len());
        for (s, source) in partition.sources.iter().enumerate() {
            if progress.is_cancelled() {
                return Err(cancelled_error());
            }
            progress.report(
                done,
                Some(total_sources as u64),
                &source.path.display().to_string(),
            );
            done += 1;
            if source.kind == SourceKind::Folder {
                left_behind.extend(source.measure.left_behind.iter().cloned());
                partition_roots.push(source.path.clone());
                continue;
            }
            if source.kind == SourceKind::WhdloadHardfile {
                hardfiles.push(source.path.clone());
            }
            let dir = staging_root.join(format!("{p}-{s}"));
            std::fs::create_dir_all(&dir)?;
            let prepared = content::prepare(&source.path, &source.kind, &dir, clock, progress)?;
            left_behind.extend(prepared.left_behind);
            partition_roots.push(prepared.root);
        }
        roots.push(partition_roots);
    }

    let all_roots: Vec<PathBuf> = roots.iter().flatten().cloned().collect();
    let needs = find_title_needs(&all_roots, progress)?;

    // P6: every slave is a title that needs WHDLoad, whether or not it names
    // a Kickstart.
    let whdload = if needs.slaves > 0 && !tree_has_whdload(tree) {
        match find_whdload(material, &hardfiles, clock)? {
            Some(choice) => Some(choice),
            None => {
                let searched = material
                    .iter()
                    .map(|folder| searched_folder(folder))
                    .chain(hardfiles.iter().map(|p| p.display().to_string()))
                    .collect();
                return Err(CoreError::WhdloadNotFound {
                    titles: needs.slaves,
                    searched,
                });
            }
        }
    } else {
        None
    };

    // M2: a tree that already has WHDLoad keeps it, and the proposal says
    // which version that is beside the best one found elsewhere.
    let tree_whdload = if needs.slaves > 0 {
        match read_tree_whdload(tree)? {
            Some((path, stated)) => {
                let best = find_whdload(material, &hardfiles, clock)?;
                let newer_elsewhere = match (&best, &stated) {
                    (Some(best), Some(own)) => {
                        (best.version, best.revision) > (own.version, own.revision)
                    }
                    (Some(_), None) => true,
                    (None, _) => false,
                };
                Some(TreeWhdload {
                    path,
                    name: stated.map(|v| format!("{} {}.{}", v.name, v.version, v.revision)),
                    best_elsewhere: best.map(|choice| choice.name),
                    newer_elsewhere,
                })
            }
            None => None,
        }
    } else {
        None
    };

    let kickstarts = propose_kickstarts(&needs, collection_dirs, material)?;

    // System again, with everything this card may still add to it.
    let tree_only =
        measured
            .system
            .sources
            .iter()
            .fold(ContentMeasure::default(), |mut sum, source| {
                sum.merge(&source.measure.content);
                sum
            });
    let mut system = tree_only;
    if let Some(choice) = &whdload {
        system.add_file("WHDLoad", choice.binary.len() as u64);
        if let Some(prefs) = &choice.prefs {
            system.add_file("WHDLoad.prefs", prefs.len() as u64);
        }
    }
    let mut supplied = Vec::new();
    for item in &kickstarts.items {
        if let Offer::Supplied { wanted, by } = &item.offer {
            // The file's size on disk: an encrypted Amiga Forever ROM is 11
            // bytes larger than the image `place` decodes from it, so this
            // errs on the side of counting too much (the ledger's T10 note).
            let rom_bytes = std::fs::metadata(&by.path)
                .map(|m| m.len())
                .unwrap_or_else(|_| wanted.size.map(u64::from).unwrap_or(KICKSTART_MAX_BYTES));
            system.add_file(&item.name, rom_bytes);
            system.add_file(&format!("{}.RTB", item.name), RTB_CHARGE_BYTES);
            supplied.push(item.name.clone());
        }
    }
    if !supplied.is_empty() {
        system.add_directory("Kickstarts");
    }
    let system_blocks = measured
        .plan
        .partitions
        .first()
        .map(|p| p.bytes / PFS3_BLOCK)
        .unwrap_or(0);
    if !pfs3_fits(system_blocks, &system) {
        let available_blocks = pfs3_data_blocks(system_blocks);
        // I5: two refusals, two next steps — the tree alone, or what the
        // card adds to it.
        let refusal = if !pfs3_fits(system_blocks, &tree_only) {
            SizingRefusal::PartitionContentDoesNotFit {
                volume_name: SYSTEM.to_string(),
                needed_blocks: tree_only.data_blocks,
                available_blocks,
            }
        } else {
            SizingRefusal::SystemAdditionsDoNotFit {
                needed_blocks: system.data_blocks,
                available_blocks,
                tree_blocks: tree_only.data_blocks,
                whdload: whdload.as_ref().map(|choice| choice.name.clone()),
                kickstarts: supplied,
            }
        };
        return Err(CoreError::CardDoesNotFit(refusal));
    }

    Ok(PreparedCard {
        measured,
        roots,
        left_behind,
        whdload,
        tree_whdload,
        kickstarts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::cardos::kickstarts::RtbSource;
    use crate::core::clock::UtcClock;
    use crate::core::gameindex::readers::slave::tests_support::{build_slave, slave_needing};
    use crate::core::jobs::NoProgress;

    fn scratch(tag: &str) -> (crate::core::ScratchDir, PathBuf) {
        crate::core::ScratchDir::pair("art-cardos-prepare", tag)
    }

    fn write_lha(path: &Path, files: &[(&str, &[u8])]) {
        std::fs::write(path, crate::core::lha::tests::make_lha_with(files)).unwrap();
    }

    fn whdload_bytes(version: &str) -> Vec<u8> {
        let mut bytes = vec![0u8; 286];
        bytes.extend_from_slice(
            format!("$VER: WHDLoad {version} [build 7051] (27.03.2026)\0").as_bytes(),
        );
        bytes
    }

    /// A folder `name` under `dir` holding `files` (paths `/`-separated).
    fn folder_with(dir: &Path, name: &str, files: &[(&str, &[u8])]) -> PathBuf {
        let root = dir.join(name);
        std::fs::create_dir_all(&root).unwrap();
        for (path, bytes) in files {
            let host = path
                .split('/')
                .fold(root.clone(), |acc, part| acc.join(part));
            std::fs::create_dir_all(host.parent().unwrap()).unwrap();
            std::fs::write(host, bytes).unwrap();
        }
        root
    }

    /// A System tree: `C/Dir`, `S/Startup-Sequence`, and `C/WHDLoad` when asked.
    fn small_tree(dir: &Path, with_whdload: bool) -> PathBuf {
        let mut files: Vec<(&str, Vec<u8>)> = vec![
            ("C/Dir", vec![0u8; 3000]),
            ("S/Startup-Sequence", b"Echo hello\n".to_vec()),
        ];
        if with_whdload {
            files.push(("C/WHDLoad", whdload_bytes("19.0")));
        }
        let borrowed: Vec<(&str, &[u8])> = files.iter().map(|(p, b)| (*p, &b[..])).collect();
        folder_with(dir, "tree", &borrowed)
    }

    /// A material folder with a loose `pfs3aio` 19.2.
    fn material_with_driver(dir: &Path) -> PathBuf {
        let material = dir.join("material");
        std::fs::create_dir_all(&material).unwrap();
        let mut driver = vec![0u8; 64];
        driver.extend_from_slice(b"$VER: pfs3aio 19.2 (01.01.2026)\0");
        std::fs::write(material.join("pfs3aio"), driver).unwrap();
        material
    }

    fn slave() -> Vec<u8> {
        build_slave("Turrican", "2026 ART", 16)
    }

    fn two_partitions(dir: &Path) -> [PartitionInput; 2] {
        let games = folder_with(dir, "Games", &[("Turrican/Turrican.slave", &slave())]);
        let stuff = folder_with(dir, "Stuff", &[("Café/readme", b"x")]);
        [
            PartitionInput {
                volume_name: "Games".into(),
                sources: vec![games],
                floor_bytes: 0,
            },
            PartitionInput {
                volume_name: "Stuff".into(),
                sources: vec![stuff],
                floor_bytes: 0,
            },
        ]
    }

    #[test]
    fn a_card_is_measured_with_system_first_and_each_partition_s_writer() {
        let (_guard, dir) = scratch("measure");
        let tree = small_tree(&dir, true);
        let parts = two_partitions(&dir);
        let material = material_with_driver(&dir);

        let card = measure_card(
            16,
            &tree,
            &parts,
            &[material],
            None,
            &dir.join("drv"),
            &HstImagerState::Usable,
            &UtcClock,
            &NoProgress,
        )
        .unwrap();

        assert_eq!(card.system.drive_name, "SDH0");
        assert_eq!(card.system.writer, PartitionWriter::Native);
        assert_eq!(
            card.partitions
                .iter()
                .map(|p| (&p.volume_name[..], &p.drive_name[..], p.writer.clone()))
                .collect::<Vec<_>>(),
            vec![
                ("Games", "SDH1", PartitionWriter::Native),
                ("Stuff", "SDH2", PartitionWriter::HstImager)
            ]
        );
        assert_eq!((card.driver.version, card.driver.revision), (19, 2));
        assert_eq!(card.partitions[0].sources[0].files, 1);
        assert_eq!(card.staging_bytes, 0, "folders are copied in place");
        assert!(!dir.join("drv").exists(), "a loose driver is not staged");
    }

    #[test]
    fn non_ascii_names_without_hst_imager_are_refused_naming_the_partition() {
        let (_guard, dir) = scratch("non-ascii");
        let tree = small_tree(&dir, true);
        let parts = two_partitions(&dir);
        let material = material_with_driver(&dir);

        let err = measure_card(
            16,
            &tree,
            &parts,
            &[material],
            None,
            &dir.join("drv"),
            &HstImagerState::NotConfigured,
            &UtcClock,
            &NoProgress,
        )
        .unwrap_err();

        assert_eq!(err.code(), "ART-CARD-NAMES-NEED-HST", "{err}");
        let msg = err.to_string();
        assert!(msg.contains("Stuff") && msg.contains("Café"), "{msg}");
    }

    #[test]
    fn an_unusable_source_is_refused_naming_its_partition_and_why() {
        let (_guard, dir) = scratch("unusable");
        let tree = small_tree(&dir, true);
        let material = material_with_driver(&dir);
        let gone = dir.join("NotThere");
        let parts = [PartitionInput {
            volume_name: "Games".into(),
            sources: vec![gone.clone()],
            floor_bytes: 0,
        }];

        let err = measure_card(
            16,
            &tree,
            &parts,
            &[material],
            None,
            &dir.join("drv"),
            &HstImagerState::Usable,
            &UtcClock,
            &NoProgress,
        )
        .unwrap_err();

        assert_eq!(err.code(), "ART-CARD-SOURCE-UNUSABLE", "{err}");
        let msg = err.to_string();
        assert!(
            msg.contains("Games")
                && msg.contains(&gone.display().to_string())
                && msg.contains("does not exist"),
            "{msg}"
        );
    }

    #[test]
    fn free_space_refuses_the_first_short_place_with_both_numbers_and_sums_one_volume() {
        let (_guard, dir) = scratch("space");
        let tree = small_tree(&dir, true);
        let material = material_with_driver(&dir);
        let mut card = measure_card(
            16,
            &tree,
            &[],
            &[material],
            None,
            &dir.join("drv"),
            &HstImagerState::Usable,
            &UtcClock,
            &NoProgress,
        )
        .unwrap();
        card.staging_bytes = 50;
        let image_bytes = card.plan.image_bytes;

        // One volume: the image's folder and the scratch folder are one need.
        let same = space_needs(
            &card,
            Path::new(r"E:\cards\art.img"),
            Path::new(r"E:\scratch"),
        );
        assert_eq!(
            same,
            vec![SpaceNeed {
                place: PathBuf::from(r"E:\cards"),
                bytes: image_bytes + 50,
                what: SpacePlace::ImageAndScratch,
            }]
        );
        // Two volumes: two needs, in order.
        let apart = space_needs(
            &card,
            Path::new(r"E:\cards\art.img"),
            Path::new(r"D:\scratch"),
        );
        assert_eq!(
            apart,
            vec![
                SpaceNeed {
                    place: PathBuf::from(r"E:\cards"),
                    bytes: image_bytes,
                    what: SpacePlace::Image,
                },
                SpaceNeed {
                    place: PathBuf::from(r"D:\scratch"),
                    bytes: 50,
                    what: SpacePlace::Scratch,
                },
            ]
        );

        let needs = vec![
            SpaceNeed {
                place: r"D:\scratch".into(),
                bytes: 10,
                what: SpacePlace::Scratch,
            },
            SpaceNeed {
                place: r"E:\cards".into(),
                bytes: 150,
                what: SpacePlace::Image,
            },
        ];
        let available = |place: &Path| -> std::io::Result<u64> {
            Ok(if place.starts_with("E:\\") { 120 } else { 100 })
        };
        let err = check_free_space(&needs, available).unwrap_err();
        match &err {
            CoreError::NotEnoughSpace {
                place,
                needed,
                available,
                what,
            } => {
                assert_eq!((&place[..], *needed, *available), (r"E:\cards", 150, 120));
                assert_eq!(*what, SpacePlace::Image);
            }
            other => panic!("expected NotEnoughSpace, got {other:?}"),
        }
        assert!(err.to_string().contains("150") && err.to_string().contains("120"));
        assert!(check_free_space(&needs[..1], available).is_ok());
    }

    /// Card round 3, M7: a free-space question that fails names the place it
    /// was about, rather than arriving as a bare OS error.
    #[test]
    fn a_free_space_question_that_fails_names_its_place() {
        let needs = vec![SpaceNeed {
            place: r"Q:\cards".into(),
            bytes: 10,
            what: SpacePlace::Image,
        }];
        let err = check_free_space(&needs, |_| {
            Err(std::io::Error::other("The device is not ready."))
        })
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains(r"Q:\cards") && msg.contains("not ready"),
            "{msg}"
        );
    }

    /// Card round 3, I1: an hst-imager that was given but does not run is
    /// refused by name for the partition that needs it — not accepted at
    /// prepare and discovered at the build, after the image exists.
    #[test]
    fn an_hst_imager_that_does_not_run_is_refused_by_name_for_the_partition_that_needs_it() {
        let (_guard, dir) = scratch("hst-unusable");
        let tree = small_tree(&dir, true);
        let parts = two_partitions(&dir);
        let material = material_with_driver(&dir);
        let tool = r"E:\tools\hst.imager.exe";

        let err = measure_card(
            16,
            &tree,
            &parts,
            &[material],
            None,
            &dir.join("drv"),
            &HstImagerState::Unusable {
                path: tool.into(),
                why: "the system cannot find the file specified".into(),
            },
            &UtcClock,
            &NoProgress,
        )
        .unwrap_err();

        assert_eq!(err.code(), "ART-HST-IMAGER-UNUSABLE", "{err}");
        let msg = err.to_string();
        assert!(
            msg.contains("Stuff") && msg.contains(tool) && msg.contains("cannot find"),
            "{msg}"
        );
    }

    /// `Games`: one folder and one pack archive.
    fn games_with_pack(dir: &Path) -> PartitionInput {
        let folder = folder_with(dir, "Games", &[("Turrican/Turrican.slave", &slave())]);
        let pack = dir.join("Lotus.lha");
        write_lha(
            &pack,
            &[("Lotus/Lotus.slave", &build_slave("Lotus", "2026 ART", 16))],
        );
        PartitionInput {
            volume_name: "Games".into(),
            sources: vec![folder, pack],
            floor_bytes: 0,
        }
    }

    #[test]
    fn staging_puts_archive_contents_in_staging_and_uses_folders_in_place() {
        let (_guard, dir) = scratch("staging");
        let tree = small_tree(&dir, true);
        let material = material_with_driver(&dir);
        let parts = [games_with_pack(&dir)];
        let materials = [material];
        let measured = measure_card(
            16,
            &tree,
            &parts,
            &materials,
            None,
            &dir.join("drv"),
            &HstImagerState::Usable,
            &UtcClock,
            &NoProgress,
        )
        .unwrap();
        assert!(measured.staging_bytes > 0, "the pack is staged");
        let staging = dir.join("staging");

        let prepared = stage_card(
            measured,
            &tree,
            &staging,
            &materials,
            &[],
            &UtcClock,
            &NoProgress,
        )
        .unwrap();

        assert_eq!(
            prepared.roots,
            vec![vec![parts[0].sources[0].clone(), staging.join("0-1")]]
        );
        assert!(staging
            .join("0-1")
            .join("Lotus")
            .join("Lotus.slave")
            .is_file());
    }

    #[test]
    fn staging_finds_titles_chooses_whdload_and_proposes_kickstarts() {
        let (_guard, dir) = scratch("titles");
        let tree = small_tree(&dir, false);
        let material = material_with_driver(&dir);
        write_lha(
            &material.join("WHDLoad_usr.lha"),
            &[
                ("WHDLoad/C/WHDLoad", &whdload_bytes("20.0")),
                ("WHDLoad/S/WHDLoad.prefs", b";prefs\n"),
            ],
        );
        write_lha(
            &material.join("skick346.lha"),
            &[("Kickstarts/kick34005.A500.RTB", &[1u8; 4000][..])],
        );
        let roms = dir.join("roms");
        std::fs::create_dir_all(&roms).unwrap();
        let rom = vec![0x11u8; 262_144];
        std::fs::write(roms.join("my-13.rom"), &rom).unwrap();
        let crc = crate::core::hashing::crc16_arc(&rom);
        let games = folder_with(
            &dir,
            "Games",
            &[(
                "Turrican/Turrican.slave",
                &slave_needing("kick34005.A500", crc, 262_144),
            )],
        );
        let parts = [PartitionInput {
            volume_name: "Games".into(),
            sources: vec![games],
            floor_bytes: 0,
        }];
        let materials = [material];
        let measured = measure_card(
            16,
            &tree,
            &parts,
            &materials,
            None,
            &dir.join("drv"),
            &HstImagerState::Usable,
            &UtcClock,
            &NoProgress,
        )
        .unwrap();

        let prepared = stage_card(
            measured,
            &tree,
            &dir.join("staging"),
            &materials,
            std::slice::from_ref(&roms),
            &UtcClock,
            &NoProgress,
        )
        .unwrap();

        let whdload = prepared.whdload.expect("WHDLoad chosen");
        assert_eq!((whdload.version, whdload.revision), (20, 0));
        let item = &prepared.kickstarts.items[0];
        assert!(
            matches!(item.offer, Offer::Supplied { .. }),
            "{:?}",
            item.offer
        );
        assert!(
            matches!(item.rtb, RtbSource::InArchive { .. }),
            "{:?}",
            item.rtb
        );
    }

    /// The title asks for no Kickstart — most do not — and still needs
    /// WHDLoad to start.
    #[test]
    fn titles_and_no_whdload_anywhere_is_refused() {
        let (_guard, dir) = scratch("no-whdload");
        let tree = small_tree(&dir, false);
        let material = material_with_driver(&dir);
        let games = folder_with(&dir, "Games", &[("Turrican/Turrican.slave", &slave())]);
        let parts = [PartitionInput {
            volume_name: "Games".into(),
            sources: vec![games],
            floor_bytes: 0,
        }];
        let gone = dir.join("no-such-material");
        let materials = [material, gone.clone()];
        let measured = measure_card(
            16,
            &tree,
            &parts,
            &materials,
            None,
            &dir.join("drv"),
            &HstImagerState::Usable,
            &UtcClock,
            &NoProgress,
        )
        .unwrap();

        let err = stage_card(
            measured,
            &tree,
            &dir.join("staging"),
            &materials,
            &[],
            &UtcClock,
            &NoProgress,
        )
        .unwrap_err();

        assert_eq!(err.code(), "ART-WHDLOAD-NOT-FOUND", "{err}");
        assert!(err.to_string().starts_with("1 WHDLoad title(s)"), "{err}");
        // M8: a material folder that does not exist is not "searched".
        assert!(
            err.to_string()
                .contains(&format!("{} (does not exist)", gone.display())),
            "{err}"
        );
    }

    #[test]
    fn a_tree_that_already_has_whdload_needs_none_from_the_material() {
        let (_guard, dir) = scratch("tree-whdload");
        let tree = small_tree(&dir, true);
        let material = material_with_driver(&dir);
        write_lha(
            &material.join("WHDLoad_usr.lha"),
            &[("WHDLoad/C/WHDLoad", &whdload_bytes("20.0"))],
        );
        let games = folder_with(&dir, "Games", &[("Turrican/Turrican.slave", &slave())]);
        let parts = [PartitionInput {
            volume_name: "Games".into(),
            sources: vec![games],
            floor_bytes: 0,
        }];
        let materials = [material];
        let measured = measure_card(
            16,
            &tree,
            &parts,
            &materials,
            None,
            &dir.join("drv"),
            &HstImagerState::Usable,
            &UtcClock,
            &NoProgress,
        )
        .unwrap();

        let prepared = stage_card(
            measured,
            &tree,
            &dir.join("staging"),
            &materials,
            &[],
            &UtcClock,
            &NoProgress,
        )
        .unwrap();
        assert!(prepared.whdload.is_none());
        // M2: the tree's copy is kept, and the proposal says which it is
        // beside the newer one in the material — never silently.
        let kept = prepared.tree_whdload.expect("the tree's WHDLoad is said");
        assert_eq!(kept.name.as_deref(), Some("WHDLoad 19.0"));
        assert_eq!(kept.best_elsewhere.as_deref(), Some("WHDLoad 20.0"));
        assert!(kept.newer_elsewhere);
    }

    /// System measured just under its limit: WHDLoad and one agreed-to-be
    /// Kickstart push it over, and the refusal names System. The control is
    /// the same card with room for them — one variable, the tree's measure.
    #[test]
    fn a_whdload_and_kickstarts_that_push_system_over_are_refused() {
        let (_guard, dir) = scratch("system-over");
        let tree = small_tree(&dir, false);
        let material = material_with_driver(&dir);
        write_lha(
            &material.join("WHDLoad_usr.lha"),
            &[("WHDLoad/C/WHDLoad", &whdload_bytes("20.0"))],
        );
        write_lha(
            &material.join("skick346.lha"),
            &[("Kickstarts/kick34005.A500.RTB", &[1u8; 4000][..])],
        );
        let roms = dir.join("roms");
        std::fs::create_dir_all(&roms).unwrap();
        let rom = vec![0x22u8; 262_144];
        std::fs::write(roms.join("k.rom"), &rom).unwrap();
        let crc = crate::core::hashing::crc16_arc(&rom);
        let games = folder_with(
            &dir,
            "Games",
            &[("T/T.slave", &slave_needing("kick34005.A500", crc, 262_144))],
        );
        let parts = [PartitionInput {
            volume_name: "Games".into(),
            sources: vec![games],
            floor_bytes: 0,
        }];
        let materials = [material];
        let measured = measure_card(
            16,
            &tree,
            &parts,
            &materials,
            None,
            &dir.join("drv"),
            &HstImagerState::Usable,
            &UtcClock,
            &NoProgress,
        )
        .unwrap();

        let blocks = measured.plan.partitions[0].bytes / PFS3_BLOCK;
        let data = pfs3_data_blocks(blocks);
        let limit = data - data / 20;
        let at = |data_blocks: u64| {
            let mut card = measured.clone();
            card.system.sources[0].measure.content.data_blocks = data_blocks;
            assert!(pfs3_fits(blocks, &card.system.sources[0].measure.content));
            card
        };

        let over = stage_card(
            at(limit),
            &tree,
            &dir.join("staging-over"),
            &materials,
            std::slice::from_ref(&roms),
            &UtcClock,
            &NoProgress,
        )
        .unwrap_err();
        match &over {
            CoreError::CardDoesNotFit(SizingRefusal::SystemAdditionsDoNotFit {
                whdload,
                kickstarts,
                tree_blocks,
                ..
            }) => {
                assert_eq!(whdload.as_deref(), Some("WHDLoad 20.0"));
                assert_eq!(kickstarts, &["kick34005.A500".to_string()]);
                assert_eq!(*tree_blocks, limit);
            }
            other => panic!("expected System's additions not to fit, got {other:?}"),
        }
        // I5: the advice is one the user can follow before the build.
        assert!(over.to_string().starts_with("System needs"), "{over}");
        assert!(over.to_string().contains("ROM files"), "{over}");

        // Control: 2 000 blocks of room hold WHDLoad, a 512-block ROM and its .RTB.
        let room = stage_card(
            at(limit - 2000),
            &tree,
            &dir.join("staging-room"),
            &materials,
            std::slice::from_ref(&roms),
            &UtcClock,
            &NoProgress,
        );
        assert!(room.is_ok(), "{:?}", room.err());
    }
}
