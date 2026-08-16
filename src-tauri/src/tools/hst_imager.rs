//! `hst-imager` as ART's PFS3 formatter (SD-2 · G3, route E).
//!
//! The implementation of [`VolumeFormatter`]. It lives here rather than in
//! `core/` because every method runs a program, and `core/` does not
//! (CLAUDE.md). When route B — ART's own PFS3 writer — exists, this is what it
//! is measured against, and removing it is deleting one file plus a setting.
//!
//! ## The command set is discovered, not invented
//!
//! Every argument below comes from the run SD-0 made on 2026-08-12 against
//! **1.6.616**, whose own `scripts/create_1gb_vhd_rdb_pfs3.txt` supplied the
//! shape. Nothing here is a guess at a flag, which is the mistake ART-091 and
//! ART-103 both were on the Emu68 side.
//!
//! ## Structured argv, never a shell string
//!
//! `core/security`'s rule. A volume name with a space in it is a volume name,
//! not two arguments, and a path the user picked is never concatenated into a
//! command line.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;
use crate::core::preload::{CopySummary, ToolVersion, VolumeFormatter};

/// The version SD-0 derived this command set from.
///
/// Not a gate: another version is recorded and mentioned rather than refused,
/// because refusing a tool that would have worked is worse than saying which
/// one ART was written against.
pub const TESTED_VERSION: &str = "1.6.616";

pub struct HstImager {
    exe: PathBuf,
}

impl HstImager {
    pub fn at(exe: impl Into<PathBuf>) -> Self {
        Self { exe: exe.into() }
    }

    /// Run it, returning stdout. A non-zero exit is an error carrying what the
    /// tool said — the user needs the tool's own words, not "it failed".
    fn run(&self, args: &[String], sink: &dyn ProgressSink) -> CoreResult<String> {
        if sink.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        sink.report(0, None, &describe(args));

        let output = Command::new(&self.exe).args(args).output().map_err(|err| {
            CoreError::InvalidInput(format!(
                "could not run '{}': {err}. Point ART at hst-imager in Settings.",
                self.exe.display()
            ))
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CoreError::Malformed {
                format: "hst-imager".into(),
                detail: format!(
                    "{} failed: {}",
                    describe(args),
                    last_meaningful_line(&stderr).unwrap_or_else(
                        || last_meaningful_line(&stdout).unwrap_or_else(|| "no output".into())
                    )
                ),
            });
        }
        Ok(stdout)
    }
}

/// What a step is, for the progress bar — the subcommand, not the whole line
/// with the user's paths in it.
fn describe(args: &[String]) -> String {
    args.iter()
        .take_while(|arg| !arg.contains(std::path::MAIN_SEPARATOR) && !arg.starts_with("--"))
        .cloned()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The last line that says something, for an error message.
fn last_meaningful_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .map(str::to_string)
}

/// The disk an RDB command addresses: the image, or an MBR slot inside it.
///
/// **The correction a real run forced.** SD-0's exit test ran against a plain
/// image whose RDB sits at byte zero, so the image path was enough. Pointed at
/// a card ART built, the tool answered *"Rigid Disk Block not found"* — of
/// course it did: byte zero of a card is a partition table and the Amiga disk
/// begins 1.1 GB in. That is ART-095 from the other side, and the Emu68
/// Imager's own command set already had the answer:
/// `<image>` + separator + `mbr` + separator + slot.
pub fn disk_target(image: &Path, slot: Option<usize>) -> String {
    match slot {
        Some(slot) => format!(
            "{}{}mbr{}{}",
            image.display(),
            std::path::MAIN_SEPARATOR,
            std::path::MAIN_SEPARATOR,
            slot
        ),
        None => image.display().to_string(),
    }
}

/// How the tool addresses a partition inside a disk: the disk, then `rdb`,
/// then the drive name.
///
/// Lowercased because that is the form SD-0's run used and the form the tool's
/// own scripts write.
pub fn partition_target(image: &Path, slot: Option<usize>, drive_name: &str) -> String {
    format!(
        "{}{}rdb{}{}",
        disk_target(image, slot),
        std::path::MAIN_SEPARATOR,
        std::path::MAIN_SEPARATOR,
        drive_name.to_lowercase()
    )
}

pub fn import_args(
    image: &Path,
    slot: Option<usize>,
    driver: &Path,
    dostype: &str,
    name: &str,
) -> Vec<String> {
    vec![
        "rdb".into(),
        "fs".into(),
        "import".into(),
        disk_target(image, slot),
        driver.display().to_string(),
        "--dos-type".into(),
        dostype.into(),
        "--name".into(),
        name.into(),
    ]
}

pub fn format_args(image: &Path, slot: Option<usize>, index: usize, volume: &str) -> Vec<String> {
    vec![
        "rdb".into(),
        "part".into(),
        "format".into(),
        disk_target(image, slot),
        index.to_string(),
        volume.into(),
    ]
}

pub fn copy_args(image: &Path, slot: Option<usize>, drive: &str, source: &Path) -> Vec<String> {
    vec![
        "fs".into(),
        "copy".into(),
        source.display().to_string(),
        partition_target(image, slot, drive),
        "--recursive".into(),
        "--makedir".into(),
    ]
}

/// Ask what a partition holds. Its summary line is the one ART reads.
pub fn dir_args(image: &Path, slot: Option<usize>, drive: &str) -> Vec<String> {
    vec![
        "fs".into(),
        "dir".into(),
        partition_target(image, slot, drive),
        "--recursive".into(),
    ]
}

/// Read `1 directory, 2 files, 20 B` off a **listing**.
///
/// **Not off the copy.** `fs copy` logs one line per file and prints no
/// summary at all; the first version of this parsed for one and reported two
/// files, no directories and no bytes — numbers that came from matching the
/// word "file" in something else entirely. Running it found that. The listing
/// is asked for afterwards instead, which also means the count is the tool
/// reading the volume back rather than ART believing its own log parse.
///
/// Absent or unreadable is **not** an error: the copy happened, and a summary
/// ART could not parse is a summary, not a failure.
pub fn parse_copy_summary(stdout: &str) -> CopySummary {
    let mut summary = CopySummary::default();
    for line in stdout.lines().rev() {
        let lower = line.to_lowercase();
        if !lower.contains("file") {
            continue;
        }
        for part in lower.split(',') {
            let part = part.trim();
            let Some((count, unit)) = part.split_once(' ') else {
                continue;
            };
            let Ok(count) = count.trim().parse::<u64>() else {
                continue;
            };
            match unit.trim() {
                "directory" | "directories" => summary.directories = count,
                "file" | "files" => summary.files = count,
                "b" | "bytes" => summary.bytes = count,
                _ => {}
            }
        }
        if summary.files > 0 || summary.directories > 0 {
            break;
        }
    }
    summary
}

impl VolumeFormatter for HstImager {
    fn probe(&self) -> CoreResult<ToolVersion> {
        let output = Command::new(&self.exe)
            .arg("--version")
            .output()
            .map_err(|err| {
                CoreError::InvalidInput(format!("could not run '{}': {err}", self.exe.display()))
            })?;
        let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if raw.is_empty() {
            return Err(CoreError::Malformed {
                format: "hst-imager".into(),
                detail: format!("'{}' did not report a version", self.exe.display()),
            });
        }
        Ok(ToolVersion { raw })
    }

    fn import_filesystem(
        &self,
        image: &Path,
        slot: Option<usize>,
        driver: &Path,
        dostype: &str,
        name: &str,
        sink: &dyn ProgressSink,
    ) -> CoreResult<()> {
        self.run(&import_args(image, slot, driver, dostype, name), sink)?;
        Ok(())
    }

    fn format_partition(
        &self,
        image: &Path,
        slot: Option<usize>,
        index: usize,
        volume: &str,
        sink: &dyn ProgressSink,
    ) -> CoreResult<()> {
        self.run(&format_args(image, slot, index, volume), sink)?;
        Ok(())
    }

    fn copy_in(
        &self,
        image: &Path,
        slot: Option<usize>,
        drive: &str,
        source: &Path,
        sink: &dyn ProgressSink,
    ) -> CoreResult<CopySummary> {
        self.run(&copy_args(image, slot, drive, source), sink)?;
        // The count comes from asking the volume, not from reading the copy's
        // own log. A listing that fails leaves the copy standing: the files
        // are there either way, and a number ART could not get is a number,
        // not a failure.
        Ok(self
            .run(&dir_args(image, slot, drive), sink)
            .map(|listing| parse_copy_summary(&listing))
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img() -> PathBuf {
        PathBuf::from("E:").join("cards").join("card.img")
    }

    /// The arguments SD-0 ran, in the order it ran them. Pinned because a
    /// flag ART invented is the mistake ART-091 and ART-103 both were.
    #[test]
    fn the_arguments_are_the_ones_sd0_verified() {
        let driver = PathBuf::from("pfs3aio.lha");
        assert_eq!(
            import_args(&img(), None, &driver, "PDS3", "pfs3aio"),
            vec![
                "rdb",
                "fs",
                "import",
                &img().display().to_string(),
                "pfs3aio.lha",
                "--dos-type",
                "PDS3",
                "--name",
                "pfs3aio",
            ]
        );
        assert_eq!(
            format_args(&img(), None, 1, "Work"),
            vec![
                "rdb",
                "part",
                "format",
                &img().display().to_string(),
                "1",
                "Work"
            ]
        );
    }

    /// A volume name with a space stays **one** argument. Structured argv is
    /// the whole reason (`core/security`).
    #[test]
    fn a_volume_name_with_a_space_is_one_argument() {
        let args = format_args(&img(), None, 2, "My Games");
        assert_eq!(args.last().unwrap(), "My Games");
        assert_eq!(args.len(), 6);
    }

    /// `<image>\rdb\dh0` — the tool's own way of naming a partition inside an
    /// image, lowercased as its scripts write it.
    #[test]
    fn a_partition_is_addressed_inside_the_image() {
        let target = partition_target(&img(), None, "DH0");
        assert!(target.ends_with("card.img\\rdb\\dh0"), "{target}");
    }

    #[test]
    fn a_copy_names_its_source_then_its_destination() {
        let source = PathBuf::from("C:").join("staging");
        let args = copy_args(&img(), None, "DH0", &source);
        assert_eq!(args[0], "fs");
        assert_eq!(args[1], "copy");
        assert_eq!(args[2], source.display().to_string(), "source comes first");
        assert!(args[3].ends_with("rdb\\dh0"), "{}", args[3]);
        assert!(args.contains(&"--recursive".to_string()));
    }

    /// The line a **listing** prints, measured against 1.6.616 on a real card
    /// on 2026-08-15 — after `fs copy` turned out to print no summary at all
    /// and this parser reported numbers it had matched in something else.
    #[test]
    fn a_listings_summary_is_read_back() {
        let summary = parse_copy_summary(
            "Name    | Size\nReadme  | 9 B\nS       | <DIR>\n\n1 directory, 2 files, 20 B\n",
        );
        assert_eq!(
            summary,
            CopySummary {
                files: 2,
                directories: 1,
                bytes: 20,
                ..Default::default()
            }
        );
    }

    /// A summary ART cannot read is not a failure — the copy still happened,
    /// and inventing numbers would be worse than reporting none.
    #[test]
    fn an_unreadable_summary_is_zero_rather_than_an_error() {
        assert_eq!(parse_copy_summary("done\n"), CopySummary::default());
        assert_eq!(parse_copy_summary(""), CopySummary::default());
    }

    /// The progress line names the step, not the user's paths.
    #[test]
    fn the_progress_line_is_the_subcommand() {
        assert_eq!(
            describe(&format_args(&img(), None, 1, "Work")),
            "rdb part format"
        );
    }
}
