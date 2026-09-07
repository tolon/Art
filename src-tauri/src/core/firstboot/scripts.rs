//! The fixed AmigaDOS text, compiled in and pinned.
//!
//! These files never vary per build. They are checked in as files rather
//! than string literals so a reader sees AmigaDOS, not Rust escapes, and
//! they are pinned exact-match below so a one-line edit cannot land without
//! its test changing with it (`core::amigainstall::workvol`'s precedent).

/// One file ART writes into the tree unchanged.
#[derive(Debug, Clone, Copy)]
pub struct FixedFile {
    /// Tree-relative, `/`-separated.
    pub tree_path: &'static str,
    pub text: &'static str,
}

pub const DISPATCHER: &str = include_str!("scripts/ART-FirstBoot");
pub const STEP_WRAPPER: &str = include_str!("scripts/ART-FirstBoot-Step");
pub const STEP_10_HARDWARE: &str = include_str!("scripts/10-hardware");
pub const STEP_20_AUX: &str = include_str!("scripts/20-aux");
pub const STEP_30_DATATYPES: &str = include_str!("scripts/30-datatypes");
pub const SD0_PI3: &str = include_str!("scripts/SD0pi3");
pub const SD0_PI4: &str = include_str!("scripts/SD0pi4");

/// Every fixed file, at the path it takes in the tree.
pub fn fixed_files() -> [FixedFile; 7] {
    [
        FixedFile {
            tree_path: super::DISPATCHER_PATH,
            text: DISPATCHER,
        },
        FixedFile {
            tree_path: super::STEP_WRAPPER_PATH,
            text: STEP_WRAPPER,
        },
        FixedFile {
            tree_path: "S/FirstBoot/10-hardware",
            text: STEP_10_HARDWARE,
        },
        FixedFile {
            tree_path: "S/FirstBoot/20-aux",
            text: STEP_20_AUX,
        },
        FixedFile {
            tree_path: "S/FirstBoot/30-datatypes",
            text: STEP_30_DATATYPES,
        },
        FixedFile {
            tree_path: "Storage/DOSDrivers/SD0pi3",
            text: SD0_PI3,
        },
        FixedFile {
            tree_path: "Storage/DOSDrivers/SD0pi4",
            text: SD0_PI4,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every script is ASCII with LF endings — an AmigaDOS shell reads a CR
    /// as part of the line, and a stray high byte is the control-byte class.
    #[test]
    fn every_fixed_file_is_ascii_with_lf_endings() {
        for file in fixed_files() {
            assert!(file.text.is_ascii(), "{} is not ASCII", file.tree_path);
            assert!(!file.text.contains('\r'), "{} carries a CR", file.tree_path);
            assert!(
                file.text.ends_with('\n'),
                "{} lacks a final newline",
                file.tree_path
            );
        }
    }

    #[test]
    fn dispatcher_sets_the_measured_fail_level_first() {
        let first_command = DISPATCHER
            .lines()
            .find(|l| !l.starts_with(';') && !l.trim().is_empty())
            .unwrap();
        assert_eq!(first_command, format!("FailAt {}", super::super::FAIL_AT));
    }

    #[test]
    fn dispatcher_writes_the_format_version_and_both_endings() {
        assert!(DISPATCHER.contains("\"art-firstboot 1\""));
        assert!(DISPATCHER.contains("\"done all\""));
        assert!(DISPATCHER.contains("\"done partial\""));
        assert!(DISPATCHER.contains("\"copy-to-fat failed\""));
        // The ending decides "any step left behind" by asking the directory
        // itself — a generated `SetEnv ART_AllDone *"FALSE*"` line per
        // remaining file (an AmigaDOS LFORMAT with an escaped literal quote)
        // — rather than trusting `List`'s own return code on an empty
        // directory, which is not a verified fact.
        assert!(DISPATCHER.contains("SetEnv ART_AllDone *\"FALSE*\""));
    }

    #[test]
    fn dispatcher_clears_the_flag_before_running_any_step() {
        let clear = DISPATCHER
            .find("Echo >ENVARC:ART_FirstBoot \"FALSE\"")
            .unwrap();
        let run = DISPATCHER.find("Execute T:ART-FirstBoot-Sorted").unwrap();
        assert!(clear < run);
    }

    #[test]
    fn step_wrapper_reports_started_before_it_executes_and_deletes_only_on_ok() {
        let started = STEP_WRAPPER.find("started").unwrap();
        let execute = STEP_WRAPPER.find("Execute S:FirstBoot/[name]").unwrap();
        let delete = STEP_WRAPPER
            .find("Delete >NIL: S:FirstBoot/[name]")
            .unwrap();
        let else_branch = STEP_WRAPPER.find("ELSE").unwrap();
        assert!(started < execute);
        assert!(else_branch < delete, "the delete must sit in the ok branch");
        assert!(STEP_WRAPPER.contains("refused rc=${ART_StepRc}"));
        // The wrapper reads the step's verdict from a shell variable it
        // reset itself, never from a Quit code (see `no_script_ever_quits`).
        let reset = STEP_WRAPPER.find("Set ART_StepRc 0").unwrap();
        assert!(reset < execute, "the verdict is reset before the step runs");
    }

    #[test]
    fn hardware_step_skips_under_uae_and_without_fat95_before_touching_anything() {
        let uae = STEP_10_HARDWARE.find("skipped uae").unwrap();
        let fat95 = STEP_10_HARDWARE.find("skipped no-fat95").unwrap();
        let copy = STEP_10_HARDWARE.find("Copy >NIL:").unwrap();
        assert!(uae < fat95 && fat95 < copy);
        assert!(STEP_10_HARDWARE.contains("SD0pi4 TO DEVS:DOSDrivers/SD0"));
        assert!(STEP_10_HARDWARE.contains("SD0pi3 TO DEVS:DOSDrivers/SD0"));
        assert!(STEP_10_HARDWARE.contains("Mount >NIL: SD0:"));
        assert!(STEP_10_HARDWARE.contains("Assign >NIL: EMU68BOOT: SD0:"));
    }

    #[test]
    fn aux_step_only_acts_on_a_3_9_tree_on_real_hardware() {
        assert!(STEP_20_AUX.contains("skipped uae"));
        assert!(STEP_20_AUX.contains("IF NOT ${ART_Kick} EQ \"3.9\""));
        assert!(STEP_20_AUX.contains("skipped not-3.9"));
        assert!(STEP_20_AUX.contains("skipped already-aux"));
    }

    #[test]
    fn datatypes_step_names_all_four_and_skips_when_nothing_to_patch() {
        for name in ["akPNG", "akGIF", "akJFIF", "akTIFF"] {
            assert!(
                STEP_30_DATATYPES.contains(&format!("{name}-040.pch")),
                "{name}"
            );
        }
        assert!(STEP_30_DATATYPES.contains("skipped no-spatch"));
        assert!(STEP_30_DATATYPES.contains("skipped no-patches"));
    }

    #[test]
    fn mountlists_differ_only_in_the_device_and_need_fat95() {
        assert!(SD0_PI3.contains("Device          = brcm-sdhc.device"));
        assert!(SD0_PI4.contains("Device          = brcm-emmc.device"));
        assert_eq!(
            SD0_PI3.replace("brcm-sdhc.device", "X"),
            SD0_PI4.replace("brcm-emmc.device", "X")
        );
        assert!(SD0_PI3.contains("FileSystem      = L:fat95"));
        assert!(SD0_PI3.contains("DOSTYPE         = 0x46415401"));
    }

    /// Measured under WinUAE on 2026-09-07: a `Quit` inside an `Execute`d
    /// script ended the wrapper, the generated run list and the dispatcher
    /// above it — the log stopped at the first step's `skipped` line and
    /// `done` was never written. A step leaves through `Skip end` instead.
    #[test]
    fn no_script_ever_quits() {
        for file in fixed_files() {
            for line in file.text.lines() {
                let word = line.split_whitespace().next().unwrap_or("");
                assert!(
                    !word.eq_ignore_ascii_case("quit"),
                    "{} quits: {line:?}",
                    file.tree_path
                );
            }
        }
    }

    /// Measured the same day: `$ART_System` did not expand and
    /// `${ART_System}` did, so every read of a variable is braced. The scan
    /// is over the text, not a table (CLAUDE.md, "a test that reads a table
    /// instead of the file is a copy").
    #[test]
    fn every_variable_read_is_braced() {
        for file in fixed_files() {
            for (i, line) in file.text.lines().enumerate() {
                let bytes = line.as_bytes();
                for (j, &b) in bytes.iter().enumerate() {
                    if b == b'$' {
                        let next = bytes.get(j + 1).copied().unwrap_or(b' ');
                        assert!(
                            next == b'{',
                            "{} line {}: unbraced variable read in {line:?} -- checked strictly, \
                             a `$` in a comment counts too, so write the word out there rather \
                             than using `$` at all",
                            file.tree_path,
                            i + 1
                        );
                    }
                }
            }
        }
    }

    /// Every step that leaves early skips to a label it carries itself.
    #[test]
    fn every_step_that_skips_carries_its_end_label() {
        for file in fixed_files()
            .iter()
            .filter(|f| f.tree_path.starts_with("S/FirstBoot/"))
        {
            assert!(file.text.contains("Skip end"), "{}", file.tree_path);
            assert!(
                file.text.trim_end().ends_with("Lab end"),
                "{} must end at its label",
                file.tree_path
            );
        }
    }

    /// Measured: `List` answered 30, 20, 10 under WinUAE. The run list is
    /// sorted before it is executed, and it is the sorted file that runs.
    #[test]
    fn dispatcher_sorts_the_run_list_before_executing_it() {
        let listed = DISPATCHER.find("List >T:ART-FirstBoot-Run").unwrap();
        let sorted = DISPATCHER
            .find("Sort T:ART-FirstBoot-Run T:ART-FirstBoot-Sorted")
            .unwrap();
        let run = DISPATCHER.find("Execute T:ART-FirstBoot-Sorted").unwrap();
        assert!(listed < sorted && sorted < run);
        assert!(!DISPATCHER.contains("Execute T:ART-FirstBoot-Run"));
    }

    /// The wrapper's key brackets are `[]`: with the default `{}` a
    /// `${ART_StepRc}` in it would be taken for a key by Execute.
    #[test]
    fn step_wrapper_uses_square_key_brackets() {
        assert!(STEP_WRAPPER.starts_with(".KEY name/A\n.BRA [\n.KET ]\n"));
        assert!(!STEP_WRAPPER.contains("{name}"));
    }

    /// Measured on the owner's 3.2 tree: after `done all` the dispatcher was
    /// still there, because a script cannot delete itself while it runs. So
    /// the finished state is "the step directory is gone", the dispatcher
    /// checks for that first, and it never tries to delete itself.
    #[test]
    fn a_finished_first_boot_removes_the_step_directory_and_the_dispatcher_stays_inert() {
        let guard = DISPATCHER
            .find(
                "IF NOT EXISTS S:FirstBoot
  Skip end",
            )
            .unwrap();
        let log = DISPATCHER.find("Echo >S:FirstBoot.log").unwrap();
        assert!(
            guard < log,
            "the directory guard comes before the log is created"
        );
        let done_all = DISPATCHER.find("\"done all\"").unwrap();
        let remove = DISPATCHER.find("Delete >NIL: S:FirstBoot ALL").unwrap();
        let else_branch = DISPATCHER[done_all..].find("ELSE").unwrap() + done_all;
        assert!(
            done_all < remove && remove < else_branch,
            "the removal sits in the done-all branch"
        );
        assert!(
            !DISPATCHER.contains("S:ART-FirstBoot S:")
                && !DISPATCHER.contains(
                    "Delete >NIL: S:ART-FirstBoot
"
                ),
            "never deletes itself"
        );
        assert!(DISPATCHER.trim_end().ends_with("Lab end"));
    }

    /// Pin every byte. A script edit must come with this test's expectation
    /// changing, which is the review point.
    #[test]
    fn every_fixed_file_hash_is_pinned() {
        use sha2::{Digest, Sha256};
        let mut got = Vec::new();
        for file in fixed_files() {
            let hash = Sha256::digest(file.text.as_bytes());
            got.push(format!("{} {:x}", file.tree_path, hash));
        }
        // Fill in from the first run's output, then never change without a review.
        let expected: [&str; 7] = [
            "S/ART-FirstBoot a52f88b0fe21d7df60c87c8b7a000829477c1b1499ebb389c621638f7e9ccdfb",
            "S/ART-FirstBoot-Step 12e80a7abc1f80068679d8ee4b2e34e2bfcb1f4df07b354fec40c2665fb6f59d",
            "S/FirstBoot/10-hardware b0d3c3585c15c010b1d75b5ba0396a457a36ae5e62ae1c4a6f3ab11cc5f5a0fa",
            "S/FirstBoot/20-aux 35e7e0973830a73c708ecbdd70f7d4dc1962b4208a0429ebcd4160044bb223f9",
            "S/FirstBoot/30-datatypes 315d95fe3ed552f69a376a91b3a3fee308324595434f0bb5308413b5b87c67dd",
            "Storage/DOSDrivers/SD0pi3 8b6a120d5e4d8e1f98a9d41d2d42758d42d91b5047afb63f5155db04bf3fbb4a",
            "Storage/DOSDrivers/SD0pi4 75b989b50c5881f5f1030dce5131053675b00bbb554a34c153ebc6b89b35bff7",
        ];
        assert_eq!(
            got, expected,
            "a fixed script changed; review the diff and re-pin"
        );
    }
}
