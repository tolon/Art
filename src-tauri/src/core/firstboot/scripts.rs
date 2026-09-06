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
        let run = DISPATCHER.find("Execute T:ART-FirstBoot-Run").unwrap();
        assert!(clear < run);
    }

    #[test]
    fn step_wrapper_reports_started_before_it_executes_and_deletes_only_on_ok() {
        let started = STEP_WRAPPER.find("started").unwrap();
        let execute = STEP_WRAPPER.find("Execute S:FirstBoot/{name}").unwrap();
        let delete = STEP_WRAPPER
            .find("Delete >NIL: S:FirstBoot/{name}")
            .unwrap();
        let else_branch = STEP_WRAPPER.find("ELSE").unwrap();
        assert!(started < execute);
        assert!(else_branch < delete, "the delete must sit in the ok branch");
        assert!(STEP_WRAPPER.contains("refused rc=$RC"));
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
        assert!(STEP_20_AUX.contains("IF NOT $ART_Kick EQ \"3.9\""));
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
            "S/ART-FirstBoot 061c3566a0a835535aca3eb81ceaf7b2f42b79cf8574dab27c03caecbd9feb27",
            "S/ART-FirstBoot-Step a87e192509549e22ea07da05a110f7c90889dbeaf1678f430977db0f6226bb9b",
            "S/FirstBoot/10-hardware 23acf086f2703236f013f19c5cb8d9e269909e75260da58880e80db4556df0fe",
            "S/FirstBoot/20-aux c04d320331b77b33787f3366ec19e09be86029f75cbf67410e2cddc1232cdea4",
            "S/FirstBoot/30-datatypes a2b77e67eceaad2e58fa1b6c03feee8a059726e66c4a3b217f73d52dc9e34a06",
            "Storage/DOSDrivers/SD0pi3 8b6a120d5e4d8e1f98a9d41d2d42758d42d91b5047afb63f5155db04bf3fbb4a",
            "Storage/DOSDrivers/SD0pi4 75b989b50c5881f5f1030dce5131053675b00bbb554a34c153ebc6b89b35bff7",
        ];
        assert_eq!(
            got, expected,
            "a fixed script changed; review the diff and re-pin"
        );
    }
}
