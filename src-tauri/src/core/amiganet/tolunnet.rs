//! `DEVS:tolunnet.config` — the address a card comes up on.
//!
//! The other half of [`super`]. `tolunnet` is the owner's own TCP/IP stack
//! (`D:\\Projeler\\tolunnet`), GPL-3.0, built on lwIP; its configuration is a
//! plain `KEY=VALUE` file, and ART can write it so a freshly built card is on
//! the network before anybody types anything.
//!
//! # Read out of the stack's own parser, not its documentation
//!
//! `src/common/prefs.c`, 2026-08-24 — and the **reader** was the one to read,
//! because ART is the writing side and what matters is what `tn_prefs_load`
//! will accept. Four things a look at the file's shape would have got wrong:
//!
//! 1. **Keys are compared case-insensitively** (`str_equal_nocase`) — the
//!    opposite of `igame.data`, which uses `strcmp`. Two neighbouring formats,
//!    two opposite rules, and only reading tells you which is which.
//! 2. **The value is left-trimmed and the key is not.** `IP= 1.2.3.4` works;
//!    `IP =1.2.3.4` gives the key `"IP "` and matches nothing. So: never a
//!    space before the `=`.
//! 3. **Several keys have aliases** — `DHCP`/`USE_DHCP`, `IP`/`IP_ADDR`,
//!    `NETMASK`/`MASK`, `GATEWAY`/`GW`, `DNS`/`DNS1`/`DNS2`/`NAMESERVER`.
//!    Anything ART rewrites has to recognise all of them, or a user's
//!    `GW=` line would survive beside ART's new `GATEWAY=` and the last one
//!    read would win.
//! 4. **A value stops at the first space or tab** (`str_copy_clean`), so a
//!    trailing comment on a line is harmless — and a value containing a space
//!    is silently truncated, which is why one is refused here instead.
//!
//! `DHCP` is true for `YES`, `1` or `TRUE`, case-insensitively; anything else
//! is false. A line with no `=` is skipped, which is what makes `#` comments
//! safe.
//!
//! # Edited in place, never regenerated
//!
//! CLAUDE.md's rule for `FF.CFG` and `config.txt`. The stack's own preferences
//! GUI writes this file too, so it is a file two programs share and a user
//! hand-edits: ART rewrites the keys it manages and passes every other line
//! through verbatim, comments and ordering included.
//!
//! **Nothing here is a secret.** The passphrase lives in
//! [`super::wpa`]'s file, and there is deliberately no field for one in this
//! one.

use serde::{Deserialize, Serialize};

use crate::core::error::{CoreError, CoreResult};

/// Where it lives on the Amiga.
pub const CONFIG_PATH: &str = "DEVS:tolunnet.config";

/// Its place inside a distribution tree ART builds, as host path components.
pub const CONFIG_IN_TREE: [&str; 2] = ["Devs", "tolunnet.config"];

/// The keys ART manages, each with every alias the stack's parser accepts.
///
/// **The aliases matter for rewriting, not for writing.** ART emits the first
/// spelling; it has to *recognise* the rest, or a user's `GW=` line would
/// survive beside ART's new `GATEWAY=` and whichever the parser read last
/// would win.
pub const MANAGED_KEYS: [&[&str]; 7] = [
    &["DEVICE"],
    &["UNIT"],
    &["DHCP", "USE_DHCP"],
    &["IP", "IP_ADDR"],
    &["NETMASK", "MASK"],
    &["GATEWAY", "GW"],
    &["DNS", "DNS1", "DNS2", "NAMESERVER"],
];

/// How a card gets its address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "how", rename_all = "kebab-case")]
pub enum Address {
    /// Ask the network. Every other field is left to the stack.
    Dhcp,
    /// Fixed, and every field is required: a static configuration missing its
    /// gateway is a card that talks to its own subnet and nothing else, which
    /// looks like a working network until it is not.
    Static {
        ip: String,
        netmask: String,
        gateway: String,
        dns: String,
    },
}

/// What ART writes into the file.
///
/// `Serialize` here on purpose, and safely: nothing in this struct is a
/// secret, and the install report is allowed to say which device a card came
/// up on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// `wifipi.device` on a PiStorm card. The stack's own default is
    /// `ethernet.device`.
    pub device: String,
    pub unit: u32,
    pub address: Address,
}

fn value_is_writable(field: &str, value: &str) -> CoreResult<()> {
    if value.is_empty() {
        return Err(CoreError::InvalidInput(format!("{field} cannot be empty")));
    }
    // `str_copy_clean` stops at the first space or tab, so a value containing
    // one is silently truncated on the Amiga rather than refused. Refuse here,
    // where the sentence reaches somebody.
    if value.chars().any(|c| c == ' ' || c == '\t') {
        return Err(CoreError::InvalidInput(format!(
            "{field} cannot contain a space: tolunnet reads a value up to the first one"
        )));
    }
    if value.chars().any(|c| (c as u32) < 0x20 || c == '=') {
        return Err(CoreError::InvalidInput(format!(
            "{field} cannot contain a control character or an '='"
        )));
    }
    Ok(())
}

/// What each managed key should say, **index-aligned with [`MANAGED_KEYS`]**.
///
/// `None` means ART is not setting that key this time, and the difference
/// carries weight: switching a card from a static address to DHCP has to
/// **remove** the old `IP=` rather than leave it under a `DHCP=YES`. The stack
/// ignores it while DHCP is on, so it is not a fault — it is a line that says
/// something untrue to the next person who reads the file, and to the same
/// person the day they turn DHCP off by hand.
///
/// **A fixed-width array rather than a list, and that is the point.** The
/// first version returned a `Vec` whose length changed with the address mode
/// and then indexed it by [`MANAGED_KEYS`] position — which panicked the
/// moment a DHCP configuration met a file carrying a `DNS=` line. CLAUDE.md's
/// own rule is never to index by a number that came from somewhere else; this
/// shape makes the mismatch unrepresentable rather than checked.
pub fn values(config: &Config) -> CoreResult<[Option<String>; MANAGED_KEYS.len()]> {
    value_is_writable("the device name", &config.device)?;

    let mut out: [Option<String>; MANAGED_KEYS.len()] = Default::default();
    out[0] = Some(config.device.clone());
    out[1] = Some(config.unit.to_string());
    match &config.address {
        Address::Dhcp => out[2] = Some("YES".to_string()),
        Address::Static {
            ip,
            netmask,
            gateway,
            dns,
        } => {
            for (field, value) in [
                ("the address", ip),
                ("the netmask", netmask),
                ("the gateway", gateway),
                ("the DNS server", dns),
            ] {
                value_is_writable(field, value)?;
            }
            out[2] = Some("NO".to_string());
            out[3] = Some(ip.clone());
            out[4] = Some(netmask.clone());
            out[5] = Some(gateway.clone());
            out[6] = Some(dns.clone());
        }
    }
    Ok(out)
}

/// A fresh file.
pub fn render(config: &Config) -> CoreResult<String> {
    let mut out = String::from("# tolunnet configuration file\n");
    for (aliases, value) in MANAGED_KEYS.iter().zip(values(config)?) {
        let Some(value) = value else { continue };
        // The first spelling is what ART writes; the aliases exist to be
        // recognised, not emitted.
        out.push_str(aliases[0]);
        out.push('=');
        out.push_str(&value);
        out.push('\n');
    }
    Ok(out)
}

/// Which managed key, if any, this line sets — by any of its aliases.
fn managed_key_of(line: &str) -> Option<usize> {
    let key = line.split_once('=')?.0;
    MANAGED_KEYS
        .iter()
        .position(|aliases| aliases.iter().any(|alias| alias.eq_ignore_ascii_case(key)))
}

/// Rewrite the keys ART manages inside a file that already exists, leaving
/// every other line exactly as it was.
///
/// **A key ART sets is rewritten at the first line that sets it, by whatever
/// alias**, and any *later* line setting the same thing is dropped — because
/// the stack's parser takes the last one it reads, so leaving a stale `GW=`
/// below ART's `GATEWAY=` would quietly undo the edit.
pub fn merge_into(existing: &str, config: &Config) -> CoreResult<String> {
    let wanted = values(config)?;
    let mut written = [false; MANAGED_KEYS.len()];
    let mut out: Vec<String> = Vec::new();

    for line in existing.lines() {
        match managed_key_of(line) {
            Some(index) => match (&wanted[index], written[index]) {
                // ART is not setting this one, so the line goes: a stale `IP=`
                // under a `DHCP=YES` says something untrue to the next reader.
                (None, _) => continue,
                // A second line for the same setting, by any alias: dropped,
                // because the parser takes the last one it reads and it would
                // undo ART's edit.
                (Some(_), true) => continue,
                (Some(value), false) => {
                    written[index] = true;
                    out.push(format!("{}={value}", MANAGED_KEYS[index][0]));
                }
            },
            // Comments, blank lines and anything the stack does not read.
            None => out.push(line.to_string()),
        }
    }
    for (index, value) in wanted.iter().enumerate() {
        if let (Some(value), false) = (value, written[index]) {
            out.push(format!("{}={value}", MANAGED_KEYS[index][0]));
        }
    }

    let mut text = out.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dhcp() -> Config {
        Config {
            device: "wifipi.device".into(),
            unit: 0,
            address: Address::Dhcp,
        }
    }

    fn fixed() -> Config {
        Config {
            device: "wifipi.device".into(),
            unit: 0,
            address: Address::Static {
                ip: "192.168.1.50".into(),
                netmask: "255.255.255.0".into(),
                gateway: "192.168.1.1".into(),
                dns: "192.168.1.1".into(),
            },
        }
    }

    #[test]
    fn a_dhcp_card_says_so_and_nothing_more() {
        assert_eq!(
            render(&dhcp()).unwrap(),
            "# tolunnet configuration file\nDEVICE=wifipi.device\nUNIT=0\nDHCP=YES\n"
        );
    }

    #[test]
    fn a_static_card_carries_every_field() {
        let out = render(&fixed()).unwrap();
        for line in [
            "DHCP=NO",
            "IP=192.168.1.50",
            "NETMASK=255.255.255.0",
            "GATEWAY=192.168.1.1",
            "DNS=192.168.1.1",
        ] {
            assert!(out.contains(&format!("{line}\n")), "{out}");
        }
    }

    /// **No space before the `=`.** `str_copy_clean` trims the value's leading
    /// blanks, but the key is compared as it stands: `IP =` is the key `"IP "`
    /// and matches nothing, which is a file that looks right and does nothing.
    #[test]
    fn there_is_no_space_around_the_separator() {
        let out = render(&fixed()).unwrap();
        assert!(!out.contains(" ="), "{out}");
        assert!(!out.contains("= "), "{out}");
    }

    /// A value stops at the first space on the Amiga, so one containing a
    /// space would be silently truncated. Refused here instead.
    #[test]
    fn a_value_with_a_space_is_refused_rather_than_truncated() {
        let err = render(&Config {
            device: "my device.device".into(),
            ..dhcp()
        })
        .unwrap_err();
        assert!(err.to_string().contains("space"), "{err}");
    }

    #[test]
    fn an_empty_or_hostile_value_is_refused() {
        assert!(render(&Config {
            device: String::new(),
            ..dhcp()
        })
        .is_err());
        assert!(render(&Config {
            device: "wifipi\ndevice".into(),
            ..dhcp()
        })
        .is_err());
        assert!(render(&Config {
            device: "a=b".into(),
            ..dhcp()
        })
        .is_err());
    }

    // -- editing a file two programs share ------------------------------

    /// CLAUDE.md's config rule. The stack's own preferences GUI writes this
    /// file too, and the ordering and comments are the user's.
    #[test]
    fn an_existing_file_is_edited_and_the_rest_passes_through() {
        let existing = "# my own notes\nDEVICE=ethernet.device\nMTU=1500\nUNIT=1\n";
        let out = merge_into(existing, &dhcp()).unwrap();

        assert!(out.contains("# my own notes"), "a comment is theirs: {out}");
        assert!(out.contains("MTU=1500"), "a key ART does not manage: {out}");
        assert!(out.contains("DEVICE=wifipi.device"), "{out}");
        assert!(!out.contains("ethernet.device"));
        assert!(out.contains("UNIT=0"), "{out}");

        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "# my own notes", "their ordering is kept");
        assert_eq!(lines[1], "DEVICE=wifipi.device");
        assert_eq!(lines[2], "MTU=1500");
    }

    /// **The alias trap.** A user's `GW=` line sets the same thing as ART's
    /// `GATEWAY=`, and the stack takes the last one it reads — so a rewrite
    /// that only looked for `GATEWAY` would leave the old value winning.
    #[test]
    fn a_line_using_an_alias_is_the_line_that_gets_rewritten() {
        let existing = "GW=10.0.0.1\nMASK=255.0.0.0\nNAMESERVER=8.8.8.8\nIP_ADDR=10.0.0.9\n";
        let out = merge_into(existing, &fixed()).unwrap();

        assert!(!out.contains("10.0.0.1"), "the old gateway is gone: {out}");
        assert!(!out.contains("255.0.0.0"), "{out}");
        assert!(!out.contains("8.8.8.8"), "{out}");
        assert!(!out.contains("10.0.0.9"), "{out}");
        assert!(out.contains("GATEWAY=192.168.1.1"), "{out}");
    }

    /// Two lines setting the same thing: the second is dropped, because the
    /// parser takes the last one it reads and it would undo ART's edit.
    #[test]
    fn a_second_line_for_the_same_setting_does_not_survive() {
        let existing = "DEVICE=a.device\nDEVICE=b.device\n";
        let out = merge_into(existing, &dhcp()).unwrap();
        assert_eq!(out.matches("DEVICE=").count(), 1, "{out}");
        assert!(out.contains("DEVICE=wifipi.device"), "{out}");
    }

    #[test]
    fn something_the_file_lacks_is_appended() {
        let out = merge_into("MTU=1500\n", &dhcp()).unwrap();
        assert!(out.starts_with("MTU=1500"));
        assert!(out.contains("DEVICE=wifipi.device"));
        assert!(out.contains("DHCP=YES"));
    }

    /// Switching from a static address to DHCP has to **remove** the old
    /// address lines, not leave them below a `DHCP=YES` where they are read
    /// and ignored — or, worse, read.
    #[test]
    fn turning_dhcp_on_does_not_leave_the_old_address_behind() {
        let existing = render(&fixed()).unwrap();
        let out = merge_into(&existing, &dhcp()).unwrap();
        assert!(out.contains("DHCP=YES"), "{out}");
        assert!(
            !out.contains("192.168.1.50"),
            "the old address is gone: {out}"
        );
    }

    #[test]
    fn a_managed_key_is_recognised_whatever_its_case() {
        let out = merge_into("device=old.device\n", &dhcp()).unwrap();
        assert!(out.contains("DEVICE=wifipi.device"), "{out}");
        assert!(!out.contains("old.device"), "{out}");
    }

    /// There is no field for a passphrase here, and there should not be: the
    /// secret belongs in `Wireless.prefs`, and a `Config` that could carry one
    /// would be a `Config` that could be logged with one.
    #[test]
    fn nothing_here_is_a_secret() {
        let json = serde_json::to_string(&fixed()).unwrap();
        assert!(!json.to_lowercase().contains("psk"));
        assert!(!json.to_lowercase().contains("pass"));
    }
}
