//! Reading a card's first-boot report back (task 10 / task 10b).
//!
//! `core::firstboot::write` puts the dispatcher into a distribution tree;
//! once that card has actually booted on a real Amiga, the machine leaves
//! its own answer in two places — `S/FirstBoot.log` on the Amiga volume
//! itself, and a copy at `art-firstboot.log` on the FAT boot partition,
//! where Windows can see it without mounting an Amiga filesystem at all
//! (see `core::firstboot`'s own module doc for the tree layout). This module
//! is the read, so the card screen can show what actually happened without
//! opening WinUAE.
//!
//! Spec §9: the Amiga volume's own copy wins where both exist, because the
//! FAT copy exists only as a fallback for a Windows user who has no way to
//! read PFS3 or FFS. **Task 10 lands the FAT half only** — the Amiga-volume
//! read is task 10b, landing as its own commit; until then this module (and
//! [`super::super::commands::firstboot::card_firstboot_report`] that calls
//! it) always answers [`ReportSource::Fat`] or [`ReportSource::None`], never
//! [`ReportSource::AmigaVolume`] — the variant exists from the start so the
//! wire shape does not change between the two commits.
//!
//! A read never opens the card for writing. `fat_report` wraps the file in
//! [`crate::core::fat32::ReadOnly`] before handing it to
//! [`crate::core::fat32::Region`] — see that type's own doc comment for why.

use std::path::Path;

use serde::Serialize;

use crate::core::card::{read_card, CardImage};
use crate::core::error::CoreResult;
use crate::core::fat32::{read_root_file, ReadOnly, Region};

use super::report::{parse_bytes, FirstBootReport};
use super::FAT_REPORT_NAME;

/// Where the report [`read_card_report`] returned actually came from.
///
/// Serialises kebab-case, matching every other tagged enum in
/// `core::firstboot` ([`super::report::Ending`], [`super::report::StepOutcome`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReportSource {
    /// The Amiga volume's own `S/FirstBoot.log` — the one spec §9 says wins.
    /// Not produced yet; lands with task 10b.
    AmigaVolume,
    /// The FAT boot partition's `art-firstboot.log` — a fallback for a
    /// Windows user who cannot otherwise read the Amiga volume.
    Fat,
    /// Neither carried a report — the card has not booted the first-boot
    /// block yet. Not an error: [`FirstBootReport::ending`] says so as
    /// [`super::report::Ending::NotBooted`].
    None,
}

/// What [`read_card_report`] found: where it came from, and the report
/// itself, parsed exactly as [`super::report::parse`] would parse the file
/// on either side.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardFirstBootReport {
    pub source: ReportSource,
    pub report: FirstBootReport,
}

/// Read a card's first-boot report back.
///
/// Structural failures — the image will not open at all — are a hard `Err`,
/// the same convention `core::osinstall::verify::verify_volume` uses: nothing
/// here could be read, so there is nothing to report. A card that opens fine
/// but simply carries no report yet is not an error — it comes back as
/// [`ReportSource::None`] with an empty, [`super::report::Ending::NotBooted`]
/// report.
pub fn read_card_report(path: &Path) -> CoreResult<CardFirstBootReport> {
    let card = read_card(path)?;

    if let Some(bytes) = fat_report(&card, path)? {
        return Ok(CardFirstBootReport {
            source: ReportSource::Fat,
            report: parse_bytes(&bytes),
        });
    }

    Ok(CardFirstBootReport {
        source: ReportSource::None,
        report: parse_bytes(b""),
    })
}

/// The FAT boot partition's copy, or `None` when there is no FAT32 partition
/// on this card or it carries no report yet.
fn fat_report(card: &CardImage, path: &Path) -> CoreResult<Option<Vec<u8>>> {
    let Some(boot) = card.mbr.as_ref().and_then(|mbr| mbr.boot_partition()) else {
        return Ok(None);
    };

    // Read-only end to end: this is a look at the user's own card, not an
    // operation on it.
    let file = std::fs::File::open(path)?;
    let mut region = Region::new(ReadOnly::new(file), boot.start_bytes(), boot.length_bytes());
    read_root_file(&mut region, FAT_REPORT_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::fat32::{create_boot_partition, DEFAULT_LABEL};
    use crate::core::mbr::{plan_card, write_mbr};
    use crate::core::ScratchDir;
    use std::io::Write;

    /// A card with only the FAT32 boot partition actually laid out — there is
    /// no RDB written on it, which `read_card` tolerates (it simply finds no
    /// Amiga area), and that is exactly the shape this test wants: a card
    /// with nothing on the Amiga side to confuse the FAT-only read task 10
    /// implements.
    fn card_with_fat_report(dir: &std::path::Path, contents: Option<&[u8]>) -> std::path::PathBuf {
        let total = 400 * 1024 * 1024u64;
        let boot_bytes = 300 * 1024 * 1024u64;
        let layout = plan_card(total, boot_bytes, &[0]).unwrap();
        let mbr_sector = write_mbr(&layout);
        let boot = layout.boot;

        let path = dir.join("card.img");
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        file.set_len(total).unwrap();
        file.write_all(&mbr_sector).unwrap();

        create_boot_partition(
            &mut file,
            boot.start_bytes(),
            boot.length_bytes(),
            DEFAULT_LABEL,
            &[],
        )
        .unwrap();

        if let Some(bytes) = contents {
            let mut region = Region::new(&mut file, boot.start_bytes(), boot.length_bytes());
            let fs = fatfs::FileSystem::new(&mut region, fatfs::FsOptions::new()).unwrap();
            let mut report = fs.root_dir().create_file(FAT_REPORT_NAME).unwrap();
            report.write_all(bytes).unwrap();
            report.flush().unwrap();
        }

        path
    }

    #[test]
    fn a_cards_fat_copy_reads_back_as_the_fat_source() {
        let dir = ScratchDir::new("art-firstboot-cardread", "fat-present");
        let path = card_with_fat_report(dir.path(), Some(b"art-firstboot 1\ndone all\n"));

        let found = read_card_report(&path).unwrap();
        assert_eq!(found.source, ReportSource::Fat);
        assert_eq!(found.report.ending, super::super::report::Ending::DoneAll);
    }

    #[test]
    fn a_card_with_no_report_yet_is_not_booted_and_not_an_error() {
        let dir = ScratchDir::new("art-firstboot-cardread", "fat-absent");
        let path = card_with_fat_report(dir.path(), None);

        let found = read_card_report(&path).unwrap();
        assert_eq!(found.source, ReportSource::None);
        assert_eq!(found.report.ending, super::super::report::Ending::NotBooted);
    }

    /// The wire shape `src/lib/firstboot.ts` reads: three kebab-case strings,
    /// none of them `null`/renamed.
    #[test]
    fn the_source_crosses_the_wire_as_one_of_three_kebab_strings() {
        for (source, wire) in [
            (ReportSource::Fat, "fat"),
            (ReportSource::AmigaVolume, "amiga-volume"),
            (ReportSource::None, "none"),
        ] {
            let found = CardFirstBootReport {
                source,
                report: parse_bytes(b""),
            };
            let json = serde_json::to_value(&found).unwrap();
            assert_eq!(json["source"], wire, "{source:?}");
        }
    }
}
