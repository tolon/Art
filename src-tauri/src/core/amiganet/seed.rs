//! Putting the two files into a system volume.
//!
//! The owner's decision, 2026-08-24: *"ART sorsun, kart kurarken WiFi
//! bilgilerini girelim"* — ART asks, and the credentials are entered while the
//! card is being set up. This is where that lands: a tree ART has built, plus
//! the two files that make it reach the network on its first boot.
//!
//! # Two files, and only one of them is a secret
//!
//! - `Envarc/Sys/Wireless.prefs` — [`super::wpa`]'s `network={}` blocks, read
//!   by `tolunwifi`'s driver adapters. **The passphrase is in here.**
//! - `Devs/tolunnet.config` — [`super::tolunnet`]'s `KEY=VALUE` lines. No
//!   secret, and deliberately no field for one.
//!
//! Either may be seeded without the other: somebody running Roadshow still
//! wants the WiFi credentials, and somebody on wired Ethernet wants only the
//! address.
//!
//! # Edited in place, and that is the whole reason this is not two `fs::write`
//!
//! Both files are things a person edits and another program writes —
//! `tolunwifi` and `TolunnetPrefs` both own theirs. So an existing file is
//! **merged**, never replaced: ART rewrites what it manages and leaves
//! everything else exactly where it was. CLAUDE.md's rule for `FF.CFG` and
//! `config.txt`, and the same reasoning.
//!
//! `Wireless.prefs` is the exception and says so: ART writes the profiles it
//! was given as the whole file, because a *set of networks* is not a set of
//! keys to rewrite — merging two lists of `network={}` blocks means deciding
//! which of somebody's networks to keep, and that is a decision, not a merge.
//! [`Seeded::replaced_networks`] carries how many blocks were there before, so
//! the screen can say it before the button rather than after.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::{tolunnet, wpa};
use crate::core::error::{CoreError, CoreResult};

/// What was asked for. **No `Serialize`**: it carries passphrases.
#[derive(Debug, Clone)]
pub struct Seed {
    /// The networks to join, in order. Empty leaves `Wireless.prefs` alone.
    pub networks: Vec<wpa::Profile>,
    /// The stack's own configuration. `None` leaves `tolunnet.config` alone.
    pub tolunnet: Option<tolunnet::Config>,
}

/// What was written, in a shape safe to log and to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Seeded {
    /// The files ART wrote, tree-relative and in AmigaDOS spelling.
    pub written: Vec<String>,
    /// How many `network={}` blocks the old `Wireless.prefs` held, when there
    /// was one. **Replaced, not merged** — see the module doc.
    pub replaced_networks: Option<usize>,
    /// Whether `tolunnet.config` was edited rather than created.
    pub tolunnet_merged: bool,
    /// How many networks were written. **A count, never a name and never a
    /// passphrase** — an SSID is the name of somebody's home.
    pub networks: usize,
}

fn under(tree: &Path, parts: &[&str]) -> PathBuf {
    parts
        .iter()
        .fold(tree.to_path_buf(), |at, part| at.join(part))
}

/// Count the `network={` blocks in a file, to say what a rewrite would lose.
fn networks_in(text: &str) -> usize {
    text.lines()
        .filter(|line| line.trim_start().starts_with("network={"))
        .count()
}

/// How many networks a rewrite would replace, without writing anything.
///
/// Its own function so the screen can say it **before** the button, the same
/// reason `osinstall_destination_taken` exists: a surprise that arrives after
/// somebody has committed reads as the application doing something it did not
/// warn about.
pub fn networks_already_there(tree: &Path) -> Option<usize> {
    let path = under(tree, &wpa::PREFS_IN_TREE);
    std::fs::read_to_string(path)
        .ok()
        .map(|text| networks_in(&text))
}

/// Write what was asked for into `tree`.
///
/// Refuses a `tree` that is not a directory rather than creating one: a typo
/// in a path is not a reason to scatter `Devs/` and `Envarc/` somewhere
/// nobody will look.
pub fn seed_tree(tree: &Path, seed: &Seed) -> CoreResult<Seeded> {
    if !tree.is_dir() {
        return Err(CoreError::InvalidInput(format!(
            "'{}' is not a folder, so there is no system volume to put these in",
            tree.display()
        )));
    }

    // Rendered **before** anything is written, so a refusal leaves the tree
    // exactly as it was rather than half seeded.
    let wireless = if seed.networks.is_empty() {
        None
    } else {
        Some(wpa::render(&seed.networks)?)
    };
    let config = match &seed.tolunnet {
        Some(wanted) => {
            let path = under(tree, &tolunnet::CONFIG_IN_TREE);
            let existing = std::fs::read_to_string(&path).ok();
            let text = match &existing {
                Some(text) => tolunnet::merge_into(text, wanted)?,
                None => tolunnet::render(wanted)?,
            };
            Some((text, existing.is_some()))
        }
        None => None,
    };

    let mut written = Vec::new();
    let mut replaced_networks = None;

    if let Some(text) = wireless {
        let path = under(tree, &wpa::PREFS_IN_TREE);
        replaced_networks = std::fs::read_to_string(&path)
            .ok()
            .map(|old| networks_in(&old));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        crate::core::safety::atomic_write(&path, text.as_bytes())?;
        written.push(wpa::PREFS_PATH.to_string());
    }

    let mut tolunnet_merged = false;
    if let Some((text, merged)) = config {
        let path = under(tree, &tolunnet::CONFIG_IN_TREE);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        crate::core::safety::atomic_write(&path, text.as_bytes())?;
        written.push(tolunnet::CONFIG_PATH.to_string());
        tolunnet_merged = merged;
    }

    Ok(Seeded {
        written,
        replaced_networks,
        tolunnet_merged,
        networks: seed.networks.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::amiganet::Secret;
    use crate::core::ScratchDir;

    fn tree(tag: &str) -> ScratchDir {
        let dir = ScratchDir::new("art-amiganet-seed", tag);
        std::fs::create_dir_all(dir.join("Devs")).unwrap();
        dir
    }

    fn wifi(ssid: &str) -> wpa::Profile {
        wpa::Profile {
            ssid: ssid.into(),
            security: wpa::Security::Wpa,
            psk: Secret::new("correct-horse"),
            priority: 0,
        }
    }

    fn stack() -> tolunnet::Config {
        tolunnet::Config {
            device: "wifipi.device".into(),
            unit: 0,
            address: tolunnet::Address::Dhcp,
        }
    }

    #[test]
    fn both_files_land_where_the_amiga_reads_them() {
        let dir = tree("both");
        let done = seed_tree(
            dir.path(),
            &Seed {
                networks: vec![wifi("Tolun-Ev")],
                tolunnet: Some(stack()),
            },
        )
        .unwrap();

        let prefs = dir.join("Envarc").join("Sys").join("Wireless.prefs");
        let config = dir.join("Devs").join("tolunnet.config");
        assert!(prefs.is_file(), "ENVARC:Sys/Wireless.prefs");
        assert!(config.is_file(), "DEVS:tolunnet.config");
        assert!(std::fs::read_to_string(&prefs)
            .unwrap()
            .contains("Tolun-Ev"));
        assert!(std::fs::read_to_string(&config)
            .unwrap()
            .contains("DEVICE=wifipi.device"));

        // The report names the files in AmigaDOS spelling, and counts the
        // networks rather than naming them.
        assert_eq!(
            done.written,
            vec![
                "ENVARC:Sys/Wireless.prefs".to_string(),
                "DEVS:tolunnet.config".to_string()
            ]
        );
        assert_eq!(done.networks, 1);
    }

    /// The drawer does not have to be there: a tree assembled by hand may have
    /// no `Envarc/Sys`.
    #[test]
    fn the_drawers_are_made_when_they_are_not_there() {
        let dir = tree("makes-drawers");
        assert!(!dir.join("Envarc").exists());
        seed_tree(
            dir.path(),
            &Seed {
                networks: vec![wifi("N")],
                tolunnet: None,
            },
        )
        .unwrap();
        assert!(dir.join("Envarc").join("Sys").is_dir());
    }

    /// Either half alone. Somebody on Roadshow still wants the credentials;
    /// somebody on wired Ethernet wants only the address.
    #[test]
    fn each_file_can_be_written_without_the_other() {
        let dir = tree("wifi-only");
        let done = seed_tree(
            dir.path(),
            &Seed {
                networks: vec![wifi("N")],
                tolunnet: None,
            },
        )
        .unwrap();
        assert_eq!(done.written, vec!["ENVARC:Sys/Wireless.prefs".to_string()]);
        assert!(!dir.join("Devs").join("tolunnet.config").exists());

        let dir = tree("stack-only");
        let done = seed_tree(
            dir.path(),
            &Seed {
                networks: Vec::new(),
                tolunnet: Some(stack()),
            },
        )
        .unwrap();
        assert_eq!(done.written, vec!["DEVS:tolunnet.config".to_string()]);
        assert!(!dir.join("Envarc").exists());
        assert_eq!(done.networks, 0);
    }

    /// `tolunnet.config` is **merged**: the stack's own preferences GUI writes
    /// it too, and a user's unmanaged keys and comments are theirs.
    #[test]
    fn an_existing_stack_config_is_edited_rather_than_replaced() {
        let dir = tree("merge-config");
        std::fs::write(
            dir.join("Devs").join("tolunnet.config"),
            b"# mine\nMTU=1500\nDEVICE=ethernet.device\n",
        )
        .unwrap();

        let done = seed_tree(
            dir.path(),
            &Seed {
                networks: Vec::new(),
                tolunnet: Some(stack()),
            },
        )
        .unwrap();
        assert!(done.tolunnet_merged, "and the screen can say it was edited");

        let text = std::fs::read_to_string(dir.join("Devs").join("tolunnet.config")).unwrap();
        assert!(text.contains("# mine"));
        assert!(text.contains("MTU=1500"));
        assert!(text.contains("DEVICE=wifipi.device"));
        assert!(!text.contains("ethernet.device"));
    }

    /// **`Wireless.prefs` is replaced, and the count says what that costs.**
    /// Merging two lists of `network={}` blocks means deciding which of
    /// somebody's networks to keep, which is a decision rather than a merge —
    /// so ART writes what it was given and reports what was there.
    #[test]
    fn replacing_the_networks_says_how_many_were_there() {
        let dir = tree("replace-networks");
        std::fs::create_dir_all(dir.join("Envarc").join("Sys")).unwrap();
        // A real `Wireless.prefs` carries more than blocks. **The comment
        // mentions `ssid` on purpose**: counting lines that merely contain the
        // word, rather than lines that open a block, gives three here — and
        // the number is shown to somebody as *"you are about to replace N
        // networks"*, so it has to be the count of networks.
        std::fs::write(
            dir.join("Envarc").join("Sys").join("Wireless.prefs"),
            b"# the ssid below is the old one\nnetwork={\n    ssid=\"Old1\"\n}\n\
              network={\n    ssid=\"Old2\"\n}\n",
        )
        .unwrap();

        // And it can be asked **before** the button, which is the point of it
        // being its own function.
        assert_eq!(networks_already_there(dir.path()), Some(2));

        let done = seed_tree(
            dir.path(),
            &Seed {
                networks: vec![wifi("New")],
                tolunnet: None,
            },
        )
        .unwrap();
        assert_eq!(done.replaced_networks, Some(2));

        let text =
            std::fs::read_to_string(dir.join("Envarc").join("Sys").join("Wireless.prefs")).unwrap();
        assert!(text.contains("New"));
        assert!(!text.contains("Old1"), "replaced, and the count said so");
    }

    #[test]
    fn nothing_there_before_is_not_a_replacement() {
        let dir = tree("fresh");
        assert_eq!(networks_already_there(dir.path()), None);
        let done = seed_tree(
            dir.path(),
            &Seed {
                networks: vec![wifi("N")],
                tolunnet: None,
            },
        )
        .unwrap();
        assert_eq!(done.replaced_networks, None);
        assert!(!done.tolunnet_merged);
    }

    /// **A refusal leaves the tree exactly as it was.** Both files are
    /// rendered before either is written, so a bad passphrase cannot leave a
    /// `tolunnet.config` behind with no `Wireless.prefs` beside it.
    #[test]
    fn a_refusal_writes_nothing_at_all() {
        let dir = tree("refusal");
        let bad = wpa::Profile {
            psk: Secret::new("short"),
            ..wifi("N")
        };
        assert!(seed_tree(
            dir.path(),
            &Seed {
                networks: vec![bad],
                tolunnet: Some(stack()),
            },
        )
        .is_err());

        assert!(!dir.join("Envarc").exists(), "no half-seeded tree");
        assert!(
            !dir.join("Devs").join("tolunnet.config").exists(),
            "and not the other file either"
        );
    }

    /// A path that is not a folder is refused rather than created: a typo is
    /// not a reason to scatter `Devs/` somewhere nobody will look.
    #[test]
    fn a_path_that_is_not_a_tree_is_refused() {
        let dir = tree("not-a-tree");
        assert!(seed_tree(
            &dir.join("nowhere"),
            &Seed {
                networks: vec![wifi("N")],
                tolunnet: None,
            },
        )
        .is_err());
    }

    /// **The report is safe to log**, which is the half of G14's rule that a
    /// type cannot enforce on its own: `Seeded` counts the networks and never
    /// names one, because an SSID is the name of somebody's home.
    #[test]
    fn the_report_carries_no_secret_and_no_network_name() {
        let dir = tree("report");
        let done = seed_tree(
            dir.path(),
            &Seed {
                networks: vec![wifi("Tolun-Ev")],
                tolunnet: None,
            },
        )
        .unwrap();

        let json = serde_json::to_string(&done).unwrap();
        assert!(!json.contains("correct-horse"), "{json}");
        assert!(!json.contains("Tolun-Ev"), "not even the name: {json}");
        assert!(json.contains("\"networks\":1"), "{json}");
    }
}
