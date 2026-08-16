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
///
/// **A stack frame is not a sentence** (ART-123). `hst-imager` handles its own
/// errors with one `[ERR] …` line, which the last non-empty line answers
/// exactly — but an *unhandled* exception prints the .NET stack trace after
/// the message, so the last line is `at Hst.Imager.ConsoleApp.CommandHandler
/// .Execute(CommandBase command)` and the sentence the user needs
/// (`System.IO.IOException: ERROR_DISK_FULL`) is twelve lines above it. Frames
/// are skipped so the message underneath them is what reaches
/// [`CoreError::Malformed`] and the screen; when a trace is *all* there is,
/// the frame is still better than nothing and is used as before.
fn last_meaningful_line(text: &str) -> Option<String> {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    lines
        .iter()
        .rfind(|line| !is_stack_frame(line))
        .or_else(|| lines.last())
        .map(|line| line.to_string())
}

/// A .NET stack frame — `at Namespace.Type.Method(Args)`, indented in the
/// original and already trimmed by the time this sees it. Deliberately narrow:
/// it must not swallow a real message that happens to begin with "at".
fn is_stack_frame(line: &str) -> bool {
    line.starts_with("at ") && line.contains('(') && line.ends_with(')')
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
    // **Unanswered until a line answers it (ART-125).** `CopySummary`'s own
    // default is a *known* zero, which is right for an accumulator and wrong
    // here: this reads somebody else's sentence, and that sentence rounds
    // (`12.2 MB`). A total ART cannot recover is left as `None` rather than
    // reported as zero, which is what the result panel used to print against
    // a twelve-megabyte copy.
    let mut summary = CopySummary {
        bytes: None,
        ..Default::default()
    };
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
                "b" | "bytes" => summary.bytes = Some(count),
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
            .unwrap_or(CopySummary {
                bytes: None,
                ..Default::default()
            }))
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
                bytes: Some(20),
                ..Default::default()
            }
        );
    }

    /// **ART-125 — a size ART cannot have is not zero.** `fs dir` rounds its
    /// own total (*"280 directories, 3933 files, 12.2 MB"*), so the byte
    /// count is unrecoverable rather than merely unparsed: turning `12.2 MB`
    /// into 12 782 141 would invent a number nothing measured. The counts
    /// beside it are exact and are still read. Before this, `bytes` stayed at
    /// its `0` default and the result panel printed "0 bytes" for a copy of
    /// twelve megabytes.
    #[test]
    fn a_rounded_size_is_not_answered_rather_than_answered_wrongly() {
        let summary = parse_copy_summary(
            "280 directories, 3933 files, 12.2 MB
",
        );
        assert_eq!(summary.files, 3933);
        assert_eq!(summary.directories, 280);
        assert_eq!(
            summary.bytes, None,
            "a rounded total is not a byte count ART can report"
        );
    }

    /// A listing ART cannot read at all answers nothing — including the
    /// bytes, which is the case `unwrap_or_default` used to turn into zero.
    #[test]
    fn an_unreadable_listing_answers_no_byte_count_either() {
        assert_eq!(
            parse_copy_summary(
                "done
"
            )
            .bytes,
            None
        );
    }

    /// A summary ART cannot read is not a failure — the copy still happened,
    /// and inventing numbers would be worse than reporting none. Zero files
    /// and zero directories; the byte total is *unanswered* rather than zero
    /// (ART-125), which is the one field that differs from `default()`.
    #[test]
    fn an_unreadable_summary_is_zero_rather_than_an_error() {
        let unknown = CopySummary {
            bytes: None,
            ..Default::default()
        };
        assert_eq!(parse_copy_summary("done\n"), unknown);
        assert_eq!(parse_copy_summary(""), unknown);
    }

    /// The progress line names the step, not the user's paths.
    #[test]
    fn the_progress_line_is_the_subcommand() {
        assert_eq!(
            describe(&format_args(&img(), None, 1, "Work")),
            "rdb part format"
        );
    }

    /// **ART-123.** The real output of a failed `fs copy`, captured verbatim
    /// from `hst-imager 1.6.616` while measuring the real AmigaOS 3.2 tree
    /// (ART-122): an unhandled exception, its message, then eight stack
    /// frames. The last line is a frame, and reporting it told the user
    /// nothing at all about what went wrong.
    #[test]
    fn an_unhandled_exception_reports_its_message_and_not_a_stack_frame() {
        let stderr = "\
[19:50:32 ERR] Failed to execute command 'Hst.Imager.Core.Commands.FsCopyCommand'
System.IO.IOException: ERROR_DISK_FULL
   at Hst.Amiga.FileSystems.Pfs3.Directory.NewFile(Boolean found, objectinfo directory, String filename)
   at Hst.Amiga.FileSystems.Pfs3.Pfs3Volume.CreateFile(String fileName, Boolean overwrite)
   at Hst.Imager.Core.Commands.FsCopyCommand.Execute(CancellationToken token)
   at Hst.Imager.ConsoleApp.CommandHandler.Execute(CommandBase command)
";
        assert_eq!(
            last_meaningful_line(stderr).as_deref(),
            Some("System.IO.IOException: ERROR_DISK_FULL")
        );
    }

    /// The ordinary case is unchanged: `hst-imager` handling its own error
    /// prints one `[ERR]` line and nothing after it.
    #[test]
    fn a_handled_error_is_still_its_last_line() {
        assert_eq!(
            last_meaningful_line(
                "[19:48:59 INF] Copying\n[19:48:59 ERR] Partition 'dh9' not found\n"
            )
            .as_deref(),
            Some("[19:48:59 ERR] Partition 'dh9' not found")
        );
    }

    /// Nothing but frames still says *something*, and a message that merely
    /// begins with "at" is not mistaken for one.
    #[test]
    fn a_trace_with_no_message_falls_back_to_its_last_frame() {
        assert_eq!(
            last_meaningful_line("   at A.B.C(D e)\n   at F.G.H(I j)\n").as_deref(),
            Some("at F.G.H(I j)")
        );
        assert_eq!(
            last_meaningful_line("at least one partition must be chosen\n").as_deref(),
            Some("at least one partition must be chosen")
        );
    }
}
