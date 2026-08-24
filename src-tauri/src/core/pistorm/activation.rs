//! What activating a firmware set *means*, not what it changes in the text.
//!
//! `preview_activate_config_set` already shows the two files side by side, and
//! a diff is the honest thing to show — it hides nothing. What it cannot do is
//! read: a line moving from `kernel=Emu68-pistorm.gz` to `kernel=Emu68.img` is
//! one changed character to a diff and a card that does not boot to a Pi.
//!
//! That is [ART-103](../../../../docs/ISSUES.md) exactly, and it is available
//! again here. ART-103 was ART writing a kernel name no release has ever
//! carried; this is a user activating a set that names a file which is not on
//! their card. Both fail the same way — **on the Amiga, where ART cannot see
//! it**, with nothing wrong on screen beforehand.
//!
//! # What this refuses to guess
//!
//! A Raspberry Pi `config.txt` is a conditional-section format, and Emu68 uses
//! it to boot a different kernel depending on which PiStorm is fitted — the
//! board is detected **at boot**, from a GPIO. A release shaped like that names
//! `kernel=` once per stanza, and its `initramfs` is a *firmware* rather than a
//! Kickstart ([ART-204](../../../../docs/ISSUES.md), and the reason
//! [`super::firmware::selects_boot_per_board`] exists).
//!
//! So when the file chooses per board, this says **that** and stops. Naming
//! "the" kernel of a file that has three, or calling a firmware blob somebody's
//! Kickstart, is the confident wrong sentence this module is here to prevent —
//! not one to commit on the way.

use serde::{Deserialize, Serialize};

use super::firmware::selects_boot_per_board;

/// The boot files a flat `config.txt` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootNames {
    /// The file selects its kernel per board, so ART does not say which.
    PerBoard,
    /// One kernel and at most one Kickstart, as this file states them.
    Flat {
        kernel: Option<String>,
        /// The `initramfs` directive's file. On a flat Emu68 config this is
        /// the Kickstart.
        kickstart: Option<String>,
    },
}

/// Something true about activating this set that the diff does not say.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "effect", rename_all = "kebab-case")]
pub enum ActivationEffect {
    /// The Amiga will load a different Kickstart afterwards.
    KickstartChanges { from: Option<String>, to: String },
    /// The Pi will load a different kernel afterwards.
    KernelChanges { from: Option<String>, to: String },
    /// The set names a Kickstart that is not on the boot partition.
    ///
    /// **The Amiga comes up with no ROM.** Worth saying loudest, because it is
    /// the one whose symptom appears somewhere ART will never be looking.
    KickstartNotOnTheCard { name: String },
    /// The set names a kernel that is not on the boot partition — ART-103's
    /// own failure, reached by a different road: the Pi loads nothing at all.
    KernelNotOnTheCard { name: String },
    /// A flat config with no `kernel=` line. Legitimate on a card whose
    /// firmware is set up some other way, and said out loud because it is also
    /// what a hand-edited file that lost the line looks like.
    NoKernelNamed,
    /// The set chooses its kernel from the board at boot, so ART does not name
    /// one. Not a problem — a statement about what was and was not checked.
    ChoosesPerBoard,
}

/// Read the first token after a directive, ignoring the rest of the line.
///
/// `initramfs kick.rom followkernel` — the trailing word is the firmware's, not
/// a second file.
fn directive_argument<'a>(line: &'a str, directive: &str) -> Option<&'a str> {
    let rest = line.trim().strip_prefix(directive)?;
    // A prefix match alone would take `initramfsomething` as a directive.
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let first = rest.split_whitespace().next()?;
    (!first.is_empty()).then_some(first)
}

/// `key=value`, with the value trimmed. Comments are not values.
fn assignment<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let trimmed = line.trim();
    if trimmed.starts_with('#') {
        return None;
    }
    let rest = trimmed.strip_prefix(key)?.trim_start();
    let value = rest.strip_prefix('=')?.trim();
    (!value.is_empty()).then_some(value)
}

/// What this `config.txt` says it boots.
pub fn boot_names(config: &str) -> BootNames {
    if selects_boot_per_board(config) {
        return BootNames::PerBoard;
    }

    // **The last one wins**, which is the firmware's own rule for a repeated
    // key in one section — and a hand-edited file with two `kernel=` lines is
    // ordinary rather than broken.
    let mut kernel = None;
    let mut kickstart = None;
    for line in config.lines() {
        if let Some(value) = assignment(line, "kernel") {
            kernel = Some(value.to_string());
        }
        if line.trim().starts_with('#') {
            continue;
        }
        if let Some(value) = directive_argument(line, "initramfs") {
            kickstart = Some(value.to_string());
        }
    }

    BootNames::Flat { kernel, kickstart }
}

/// Whether a name is on the boot partition.
///
/// Case-insensitive: the boot partition is FAT32, where `KICK.ROM` and
/// `kick.rom` are one file, and a check that said otherwise would report a
/// missing Kickstart that is sitting right there.
fn present(name: &str, files: &[String]) -> bool {
    files.iter().any(|f| f.eq_ignore_ascii_case(name))
}

/// What activating this set does, beyond replacing the text.
///
/// `files` is what the boot partition actually holds — the caller's job,
/// because `core` reads a folder here and the question is about a real card.
pub fn activation_effects(before: &str, after: &str, files: &[String]) -> Vec<ActivationEffect> {
    let BootNames::Flat {
        kernel: new_kernel,
        kickstart: new_kickstart,
    } = boot_names(after)
    else {
        return vec![ActivationEffect::ChoosesPerBoard];
    };

    // The current file may be per-board even when the incoming one is not. In
    // that case there is no single "from" to name, and `None` is the honest
    // answer rather than a guess at which stanza applies.
    let (old_kernel, old_kickstart) = match boot_names(before) {
        BootNames::Flat { kernel, kickstart } => (kernel, kickstart),
        BootNames::PerBoard => (None, None),
    };

    let mut effects = Vec::new();

    match &new_kickstart {
        Some(name) => {
            if old_kickstart.as_deref() != Some(name.as_str()) {
                effects.push(ActivationEffect::KickstartChanges {
                    from: old_kickstart.clone(),
                    to: name.clone(),
                });
            }
            if !present(name, files) {
                effects.push(ActivationEffect::KickstartNotOnTheCard { name: name.clone() });
            }
        }
        None => {
            // No `initramfs` at all is not reported as a change: a card whose
            // Kickstart is supplied some other way is somebody else's setup,
            // and ART has no business calling it wrong.
        }
    }

    match &new_kernel {
        Some(name) => {
            if old_kernel.as_deref() != Some(name.as_str()) {
                effects.push(ActivationEffect::KernelChanges {
                    from: old_kernel.clone(),
                    to: name.clone(),
                });
            }
            if !present(name, files) {
                effects.push(ActivationEffect::KernelNotOnTheCard { name: name.clone() });
            }
        }
        None => effects.push(ActivationEffect::NoKernelNamed),
    }

    effects
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL: &str = "\
# Emu68 for PiStorm
arm_64bit=1
enable_uart=1
kernel=Emu68-pistorm.gz
initramfs kick.rom
";

    fn files(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn a_flat_config_states_its_kernel_and_its_kickstart() {
        assert_eq!(
            boot_names(REAL),
            BootNames::Flat {
                kernel: Some("Emu68-pistorm.gz".into()),
                kickstart: Some("kick.rom".into()),
            }
        );
    }

    /// The ART-204 shape. A release that chooses per board names several
    /// kernels and an `initramfs` that is firmware, so ART names none of them.
    #[test]
    fn a_per_board_config_is_not_read_for_names() {
        let per_board = "\
[gpio24=0]
kernel=Emu68-pistorm32lc.gz
[all]
kernel=Emu68-pistorm.gz
initramfs some-firmware.bin
";
        assert_eq!(boot_names(per_board), BootNames::PerBoard);
        assert_eq!(
            activation_effects(REAL, per_board, &files(&[])),
            vec![ActivationEffect::ChoosesPerBoard],
            "naming a firmware blob as somebody's Kickstart is the sentence this avoids"
        );
    }

    /// **ART-103 reached by the other road.** The set names a kernel no
    /// release carries; the card would not boot and nothing on screen would
    /// have said so.
    #[test]
    fn a_kernel_that_is_not_on_the_card_is_named() {
        let after = REAL.replace("Emu68-pistorm.gz", "Emu68.img");
        let effects = activation_effects(REAL, &after, &files(&["Emu68-pistorm.gz", "kick.rom"]));

        assert!(effects.contains(&ActivationEffect::KernelNotOnTheCard {
            name: "Emu68.img".into()
        }));
        assert!(effects.contains(&ActivationEffect::KernelChanges {
            from: Some("Emu68-pistorm.gz".into()),
            to: "Emu68.img".into()
        }));
    }

    #[test]
    fn a_kickstart_that_is_not_on_the_card_is_named() {
        let after = REAL.replace("kick.rom", "kick31.rom");
        let effects = activation_effects(REAL, &after, &files(&["Emu68-pistorm.gz", "kick.rom"]));

        assert!(effects.contains(&ActivationEffect::KickstartNotOnTheCard {
            name: "kick31.rom".into()
        }));
    }

    /// The ordinary multiboot use: two sets, two Kickstarts, both on the card.
    /// The change is reported; nothing is called missing.
    #[test]
    fn swapping_to_a_kickstart_that_is_there_is_a_change_and_not_a_problem() {
        let after = REAL.replace("kick.rom", "kick31.rom");
        let effects = activation_effects(
            REAL,
            &after,
            &files(&["Emu68-pistorm.gz", "kick.rom", "kick31.rom"]),
        );

        assert_eq!(
            effects,
            vec![ActivationEffect::KickstartChanges {
                from: Some("kick.rom".into()),
                to: "kick31.rom".into()
            }]
        );
    }

    /// Activating the set that is already active says nothing at all.
    #[test]
    fn an_identical_set_has_no_effects() {
        assert!(
            activation_effects(REAL, REAL, &files(&["Emu68-pistorm.gz", "kick.rom"])).is_empty()
        );
    }

    /// FAT32 does not distinguish them, so neither may this — reporting
    /// `KICK.ROM` as missing while it sits on the card is the false alarm that
    /// teaches people to ignore the true ones.
    #[test]
    fn the_boot_partition_is_case_insensitive() {
        let effects = activation_effects(REAL, REAL, &files(&["EMU68-PISTORM.GZ", "KICK.ROM"]));
        assert!(effects.is_empty(), "{effects:?}");
    }

    /// A commented-out line is not a setting.
    #[test]
    fn comments_name_nothing() {
        let commented = "# kernel=Emu68.img\n# initramfs kick.rom\narm_64bit=1\n";
        assert_eq!(
            boot_names(commented),
            BootNames::Flat {
                kernel: None,
                kickstart: None
            }
        );
    }

    /// Legitimate, and said out loud because it is also what a hand-edited
    /// file that lost its `kernel=` line looks like.
    #[test]
    fn a_config_with_no_kernel_line_says_so() {
        let effects = activation_effects(REAL, "arm_64bit=1\n", &files(&[]));
        assert_eq!(effects, vec![ActivationEffect::NoKernelNamed]);
    }

    /// `initramfs kick.rom followkernel` — the trailing word is the
    /// firmware's, not a second file, and taking it would report a missing
    /// file called `followkernel`.
    #[test]
    fn a_trailing_firmware_word_is_not_a_file_name() {
        let after = "kernel=Emu68-pistorm.gz\ninitramfs kick.rom followkernel\n";
        let effects = activation_effects(after, after, &files(&["Emu68-pistorm.gz", "kick.rom"]));
        assert!(effects.is_empty(), "{effects:?}");
    }

    /// The wire, pinned as a literal.
    ///
    /// [ART-233](../../../../docs/ISSUES.md) was this exact class two days
    /// ago: a serde attribute the frontend did not agree with, defaulting
    /// silently rather than failing. The variant names are what `src/lib`
    /// switches on, so they are asserted as the strings they have to be
    /// rather than round-tripped through Rust - a round trip agrees with
    /// whatever the attribute says this week.
    #[test]
    fn the_wire_shape_is_what_the_frontend_reads() {
        let json = serde_json::to_string(&ActivationEffect::KickstartNotOnTheCard {
            name: "kick31.rom".into(),
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"effect":"kickstart-not-on-the-card","name":"kick31.rom"}"#
        );

        let json = serde_json::to_string(&ActivationEffect::KernelChanges {
            from: None,
            to: "Emu68-pistorm.gz".into(),
        })
        .unwrap();
        assert_eq!(
            json, r#"{"effect":"kernel-changes","from":null,"to":"Emu68-pistorm.gz"}"#,
            "`from` is null rather than absent: the frontend reads it as string | null"
        );

        assert_eq!(
            serde_json::to_string(&ActivationEffect::ChoosesPerBoard).unwrap(),
            r#"{"effect":"chooses-per-board"}"#
        );
    }

    /// A key that merely starts with the directive's name is not the
    /// directive.
    #[test]
    fn a_lookalike_key_is_not_the_directive() {
        let odd = "kernel_address=0x8000\ninitramfsomething=1\n";
        assert_eq!(
            boot_names(odd),
            BootNames::Flat {
                kernel: None,
                kickstart: None
            }
        );
    }
}
