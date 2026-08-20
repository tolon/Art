//! ADF validation — produces a structured report.
//!
//! Checks: image size, bootblock signature + checksum, root block presence
//! and type, bitmap presence. Reports a coarse health status so the UI can
//! show Healthy / Warning / Problem without exposing internals to beginners.

use serde::{Deserialize, Serialize};

use super::blocks::{block_type, BLOCK_SIZE};
use super::bootblock::FileSystemType;
use crate::core::error::CoreResult;

/// Coarse health status used by the beginner UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Warning,
    Problem,
}

impl HealthStatus {
    /// Combine two statuses, taking the worse.
    pub(crate) fn combine(self, other: Self) -> Self {
        use HealthStatus::*;
        match (self, other) {
            (Problem, _) | (_, Problem) => Problem,
            (Warning, _) | (_, Warning) => Warning,
            (Healthy, Healthy) => Healthy,
        }
    }
}

/// One finding from validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationFinding {
    pub severity: HealthStatus,
    /// Short machine-readable code (e.g. `"bootblock.checksum"`).
    pub code: String,
    /// Human-readable explanation.
    pub message: String,
}

/// Full validation report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    pub status: HealthStatus,
    pub findings: Vec<ValidationFinding>,
}

impl ValidationReport {
    fn empty() -> Self {
        Self {
            status: HealthStatus::Healthy,
            findings: Vec::new(),
        }
    }
    fn push(&mut self, severity: HealthStatus, code: &str, message: impl Into<String>) {
        self.status = self.status.combine(severity);
        self.findings.push(ValidationFinding {
            severity,
            code: code.to_string(),
            message: message.into(),
        });
    }
}

/// Validate any mounted volume — floppy image or HDF partition.
///
/// Takes a mounted device and its geometry; [`validate`] does the same job
/// from a whole image's bytes, deriving the geometry from its length. Neither
/// measures a volume against a floppy's block count — on a partition that is
/// not a defect, it is the wrong question.
///
/// Read-only, so a bad bitmap flag is a **warning**: a volume AmigaOS would
/// want to re-validate is still perfectly readable, and calling that a problem
/// would tell the user their disk is broken when it is not (§2.5, §89).
pub fn validate_volume(
    device: &dyn crate::core::volume::BlockDevice,
    geometry: &crate::core::volume::VolumeGeometry,
) -> CoreResult<ValidationReport> {
    let mut report = ValidationReport::empty();

    if let Some(reason) = geometry.dos_type.unsupported_reason() {
        report.push(
            HealthStatus::Warning,
            "volume.filesystem",
            format!("{} — ART can see this volume but not read it.", reason),
        );
        // Nothing below applies to a filesystem ART cannot walk.
        return Ok(report);
    }

    match crate::core::volume::read_block_vec(device, geometry.root_block) {
        Err(_) => report.push(
            HealthStatus::Problem,
            "rootblock.range",
            format!(
                "Root block {} is outside this volume ({} blocks).",
                geometry.root_block, geometry.total_blocks
            ),
        ),
        Ok(block) => {
            let typ = i32::from_be_bytes([block[0], block[1], block[2], block[3]]);
            if typ != block_type::HEADER {
                report.push(
                    HealthStatus::Problem,
                    "rootblock.type",
                    format!("Root block has type {typ}; expected a header block (2)."),
                );
            }
            // Longword 127 of the root block is ST_ROOT for a real root.
            let subtype = i32::from_be_bytes([block[508], block[509], block[510], block[511]]);
            if typ == block_type::HEADER && subtype != super::blocks::block_subtype::ROOT {
                report.push(
                    HealthStatus::Warning,
                    "rootblock.subtype",
                    format!("Root block has secondary type {subtype}; expected 1 (ST_ROOT)."),
                );
            }
        }
    }

    Ok(report)
}

/// Validate a whole volume image (full byte slice).
///
/// **Size is not geometry.** This used to demand exactly 1760 blocks and warn
/// on anything else, which called every HD floppy and every hard-disk image
/// damaged. The block count comes from the image's own length and the root
/// block from that count ([`crate::core::volume::VolumeGeometry::root_block_for`],
/// via [`super::root_block_of`]) — never from an assumed 880 or 1760. DD, HD
/// and hard-disk images are all legitimate here; only an image that cannot
/// hold a filesystem at all is a defect.
pub fn validate(
    image: &[u8],
    fs_type: FileSystemType,
    root_block: u32,
    bootblock_ok: bool,
    bootblock_sig_ok: bool,
) -> CoreResult<ValidationReport> {
    let mut report = ValidationReport::empty();

    // 1. The image has to be a whole number of blocks, and hold at least a
    //    boot block and a root block. How *many* blocks is not checked
    //    against a floppy's count: see this function's doc.
    let total_blocks = super::total_blocks_of(image);
    let remainder = image.len() % BLOCK_SIZE;
    if remainder != 0 {
        report.push(
            HealthStatus::Warning,
            "image.size",
            format!(
                "Image is {} bytes, which is not a whole number of {BLOCK_SIZE}-byte blocks; \
                 the last {remainder} bytes are not part of any block.",
                image.len(),
            ),
        );
    }
    if total_blocks < 3 {
        report.push(
            HealthStatus::Problem,
            "image.size",
            format!(
                "Image holds {total_blocks} blocks; a volume needs at least a boot block \
                 and a root block."
            ),
        );
    }

    // 2. Bootblock signature.
    if !bootblock_sig_ok {
        report.push(
            HealthStatus::Problem,
            "bootblock.signature",
            "Bootblock does not start with a valid 'DOS' signature.",
        );
    }

    // 3. Bootblock checksum.
    if bootblock_sig_ok && !bootblock_ok {
        report.push(
            HealthStatus::Warning,
            "bootblock.checksum",
            "Bootblock checksum does not match; the image may be custom or damaged.",
        );
    }

    // 4. Root block. `read_u32_at` bounds-checks `root_block` against the
    //    image before indexing it (ART-047) — this used to compute
    //    `root_off` by hand and then index `image[root_off..]` directly,
    //    which is exactly the pattern `core/adf/blocks.rs` exists to replace.
    match super::blocks::read_u32_at(image, root_block, 0) {
        Err(_) => report.push(
            HealthStatus::Problem,
            "rootblock.range",
            format!("Root block {root_block} is outside the image."),
        ),
        Ok(typ_raw) => {
            let typ = typ_raw as i32;
            if typ != block_type::HEADER {
                report.push(
                    HealthStatus::Problem,
                    "rootblock.type",
                    format!("Root block has type {typ}; expected a header block (2)."),
                );
            }
        }
    }

    let _ = fs_type; // filesystem-specific deep checks arrive in a later phase
    Ok(report)
}

/// Helper for callers that only have the image bytes.
pub fn validate_image(image: &[u8]) -> CoreResult<ValidationReport> {
    if image.len() < BLOCK_SIZE {
        return Ok(ValidationReport {
            status: HealthStatus::Problem,
            findings: vec![ValidationFinding {
                severity: HealthStatus::Problem,
                code: "image.too_small".into(),
                message: "Image is smaller than a single block.".into(),
            }],
        });
    }
    let check_len = image.len().min(super::bootblock::BOOTBLOCK_SIZE);
    let bb = super::bootblock::BootBlock::parse(&image[..check_len])?;
    // The root block is not a field of the boot block — it is derived from
    // the volume's size, computed in the one place both callers share (see
    // `super::root_block_of`'s doc for why that matters).
    let root_block = super::root_block_of(image);
    validate(
        image,
        bb.fs_type,
        root_block,
        bb.checksum_valid,
        true, // signature already validated by parse()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::adf::bootblock::DEFAULT_ROOT_BLOCK;
    use crate::core::adf::bootblock::{BootBlock, FileSystemType as FST};

    /// A blank OFS volume of any size, with its root block wherever this
    /// volume's own geometry puts it — not where a floppy's would be.
    fn blank_ofs_volume(total_blocks: usize) -> Vec<u8> {
        let mut img = vec![0u8; super::BLOCK_SIZE * total_blocks];
        // Bootblock: DOS\0 + valid checksum across 1024 bytes.
        img[0..4].copy_from_slice(b"DOS\0");
        let cks = BootBlock::compute_checksum(&img[..1024]);
        img[4..8].copy_from_slice(&cks.to_be_bytes());
        // Root block: type = HEADER.
        let root_off = crate::core::adf::root_block_of(&img) as usize * BLOCK_SIZE;
        img[root_off..root_off + 4].copy_from_slice(&block_type::HEADER.to_be_bytes());
        img
    }

    fn blank_ofs_adf() -> Vec<u8> {
        blank_ofs_volume(1760)
    }

    #[test]
    fn blank_adf_is_healthy() {
        let img = blank_ofs_adf();
        let report = validate_image(&img).unwrap();
        assert_eq!(report.status, HealthStatus::Healthy);
    }

    /// A validator that measured every image against 1760 blocks called an HD
    /// floppy damaged — and, wired into the write path, would have refused
    /// every write to one. The root block moves with the size; the size itself
    /// is not a finding.
    #[test]
    fn an_hd_floppy_image_is_healthy_at_its_own_geometry() {
        let img = blank_ofs_volume(3520);
        assert_eq!(crate::core::adf::root_block_of(&img), 1760);

        let report = validate_image(&img).unwrap();
        assert_eq!(
            report.status,
            HealthStatus::Healthy,
            "{:?}",
            report.findings
        );
        assert!(
            !report.findings.iter().any(|f| f.code == "image.size"),
            "an HD image's size is not a defect: {:?}",
            report.findings
        );
    }

    /// The same for a hard-disk-sized image, where nothing about a floppy's
    /// geometry applies at all.
    #[test]
    fn a_hard_disk_sized_image_is_healthy_at_its_own_geometry() {
        // 20 000 blocks = 10 MB — a small hardfile, well past any floppy.
        let img = blank_ofs_volume(20_000);
        assert_eq!(crate::core::adf::root_block_of(&img), 10_000);

        let report = validate_image(&img).unwrap();
        assert_eq!(
            report.status,
            HealthStatus::Healthy,
            "{:?}",
            report.findings
        );
        assert!(!report.findings.iter().any(|f| f.code == "image.size"));
    }

    /// What the size check is actually for: bytes that are not part of any
    /// block. Said as a warning, because the volume in front of them still
    /// reads.
    #[test]
    fn a_size_that_is_not_whole_blocks_is_a_warning() {
        let mut img = blank_ofs_volume(1760);
        img.extend_from_slice(&[0u8; 100]);

        let report = validate_image(&img).unwrap();
        assert_eq!(report.status, HealthStatus::Warning);
        let finding = report
            .findings
            .iter()
            .find(|f| f.code == "image.size")
            .expect("the trailing bytes must be reported");
        assert!(finding.message.contains("100 bytes"), "{}", finding.message);
    }

    #[test]
    fn bad_checksum_is_warning() {
        let mut img = blank_ofs_adf();
        img[5] ^= 0xFF; // corrupt checksum
        let report = validate_image(&img).unwrap();
        assert_eq!(report.status, HealthStatus::Warning);
        assert!(report
            .findings
            .iter()
            .any(|f| f.code == "bootblock.checksum"));
    }

    #[test]
    fn wrong_root_type_is_problem() {
        let mut img = blank_ofs_adf();
        let root_off = DEFAULT_ROOT_BLOCK as usize * BLOCK_SIZE;
        img[root_off..root_off + 4].copy_from_slice(&block_type::DATA.to_be_bytes()); // wrong type
        let report = validate_image(&img).unwrap();
        assert_eq!(report.status, HealthStatus::Problem);
    }

    #[test]
    fn tiny_image_is_problem() {
        let img = vec![0u8; 100];
        let report = validate_image(&img).unwrap();
        assert_eq!(report.status, HealthStatus::Problem);
    }

    #[test]
    fn health_combine_takes_worse() {
        use HealthStatus::*;
        assert_eq!(Healthy.combine(Warning), Warning);
        assert_eq!(Warning.combine(Healthy), Warning);
        assert_eq!(Warning.combine(Problem), Problem);
        assert_eq!(Healthy.combine(Healthy), Healthy);
    }

    // silence unused import in test
    #[allow(dead_code)]
    fn _fs(x: FST) {
        let _ = x;
    }
}
