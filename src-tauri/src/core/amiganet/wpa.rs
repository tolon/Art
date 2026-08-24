//! `ENVARC:Sys/Wireless.prefs` — the WiFi credentials a card boots with.
//!
//! SD-3 G14's WiFi half. A PiStorm card ART builds can reach the network on
//! its first boot instead of the owner typing a passphrase into an Amiga whose
//! keyboard is still American — which is the same argument the keymap
//! selection makes ([ART-226]), and the reason both belong to this round.
//!
//! # The format was read out of the owner's own program
//!
//! **`DEVS:NetInterfaces` would have been the wrong file.** That is Roadshow's,
//! and this owner's stack is their own: `tolunwifi` (`D:\\Projeler\\TolunWifi`)
//! configures the *driver* and writes `ENVARC:Sys/Wireless.prefs`, with
//! `ENV:Sys/Wireless.prefs` as its live twin. Read 2026-08-24 from
//! `src/adapters/wifipi.c` and `src/core/wpaconf.c` \u2014 and their own header
//! records that the format was itself verified against AmiKit's NetworkWizard
//! and the zenPrismWifi writer, so this is a second-hand reading of a
//! first-hand check rather than a guess about a guess.
//!
//! Emitted byte for byte as `tw_wpaconf_write` emits it \u2014 four-space indent,
//! `\\n` endings, one `network={}` block per profile:
//!
//! ```text
//! network={
//!     ssid="MyNetwork"
//!     psk="mypassword"
//!     scan_ssid=1
//! }
//! ```
//!
//! Five rules that a reading of the *shape* would have got wrong, each of
//! which produces a file the supplicant misreads rather than an error:
//!
//! 1. **A passphrase is quoted; a 64-hex PMK is not.** Their own comment:
//!    *"NEVER quote the hex form, NEVER leave a passphrase bare."*
//! 2. **A WPA/WPA2 block carries no `key_mgmt` line at all** \u2014 the supplicant
//!    defaults to WPA-PSK when a `psk` is present. Writing one is not
//!    harmless; it is a different configuration.
//! 3. **`scan_ssid=1` on every form except the hashed one**, which is emitted
//!    verbatim as `ssid` + `psk` and nothing else.
//! 4. **`priority=N` only when non-zero.**
//! 5. **`"` and `\\` are escaped inside quoted values**, and a control byte
//!    anywhere in an SSID or a passphrase is **refused**: the escaping covers
//!    quotes and backslashes and *not* newlines, so a raw `\\n` would inject
//!    lines into the block. tolunwifi refuses it as its last line of defence;
//!    ART refuses it as its first.
//!
//! # The passphrase cannot be logged
//!
//! [`super::Secret`] deserialises and does not serialise, so a profile cannot
//! reach the operation log, a manifest or an AI prompt through `serde` at all,
//! and `{:?}` on one prints stars. G14 asks for that as a rule; this makes it
//! a compile error.
//!
//! [ART-226]: ../../../../docs/ISSUES.md

use serde::Deserialize;

use super::Secret;
use crate::core::error::{CoreError, CoreResult};

/// Where the persistent copy lives on the Amiga.
pub const PREFS_PATH: &str = "ENVARC:Sys/Wireless.prefs";
/// The live twin `tolunwifi` keeps beside it.
pub const PREFS_PATH_LIVE: &str = "ENV:Sys/Wireless.prefs";

/// Its place inside a distribution tree ART builds, as host path components.
pub const PREFS_IN_TREE: [&str; 3] = ["Envarc", "Sys", "Wireless.prefs"];

/// A WPA passphrase is 8 to 63 characters \u2014 the supplicant's own range.
pub const PASSPHRASE_MIN: usize = 8;
pub const PASSPHRASE_MAX: usize = 63;
/// A precomputed PMK is 64 hex characters.
pub const PMK_HEX: usize = 64;

/// How a network is secured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Security {
    /// No passphrase at all: `key_mgmt=NONE` and no `psk` line.
    Open,
    /// WPA or WPA2 with a passphrase, or with a precomputed PMK.
    ///
    /// **The two are not distinguished on disk beyond the quoting**, and
    /// neither is WPA from WPA2 without a `proto=` line: a `psk` alone means
    /// WPA-PSK to the supplicant.
    Wpa,
}

/// One network to join.
///
/// **No `Serialize`**, and that is not an omission: it carries a
/// [`Secret`], and a profile that could be serialised is a passphrase that
/// could reach a log.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub ssid: String,
    pub security: Security,
    /// Empty for [`Security::Open`]. Either a passphrase or a 64-hex PMK.
    #[serde(default = "empty_secret")]
    pub psk: Secret,
    /// Only written when non-zero, for picking between several networks.
    #[serde(default)]
    pub priority: u8,
}

fn empty_secret() -> Secret {
    Secret::new("")
}

/// Is this a precomputed 64-hex PMK rather than a passphrase?
///
/// The same test `tw_wpaconf_write` makes: 64 characters, every one a hex
/// digit in either case.
pub fn is_hex_pmk(psk: &str) -> bool {
    psk.len() == PMK_HEX && psk.chars().all(|c| c.is_ascii_hexdigit())
}

/// Refuse a profile the supplicant would misread, before anything is written.
///
/// Every refusal here is one `tw_wpaconf_write` also makes \u2014 ART refuses it
/// first, where the sentence can reach a person, rather than leaving the last
/// line of defence to do the talking.
pub fn validate(profile: &Profile) -> CoreResult<()> {
    let refuse = |why: String| Err(CoreError::InvalidInput(why));

    if profile.ssid.is_empty() {
        return refuse("a network needs a name".into());
    }
    // The escaping covers `"` and `\` and not newlines, so a control byte
    // would inject lines into the block. Refused in both fields.
    if profile.ssid.chars().any(|c| (c as u32) < 0x20) {
        return refuse(format!(
            "'{}' carries a control character, which would inject lines into the file",
            profile.ssid
        ));
    }
    if profile.psk.expose().chars().any(|c| (c as u32) < 0x20) {
        // **The sentence names the network, never the passphrase.**
        return refuse(format!(
            "the passphrase for '{}' carries a control character",
            profile.ssid
        ));
    }

    match profile.security {
        Security::Open if !profile.psk.is_empty() => refuse(format!(
            "'{}' is an open network and cannot carry a passphrase",
            profile.ssid
        )),
        Security::Open => Ok(()),
        Security::Wpa if profile.psk.is_empty() => {
            refuse(format!("'{}' needs a passphrase", profile.ssid))
        }
        Security::Wpa => {
            let psk = profile.psk.expose();
            if is_hex_pmk(psk) || (PASSPHRASE_MIN..=PASSPHRASE_MAX).contains(&psk.len()) {
                Ok(())
            } else {
                // Says the range, never the value.
                refuse(format!(
                    "the passphrase for '{}' is {} characters; it must be {PASSPHRASE_MIN} to \
                     {PASSPHRASE_MAX}, or a {PMK_HEX}-character key",
                    profile.ssid,
                    psk.len()
                ))
            }
        }
    }
}

/// `"` and `\` escaped, exactly as `buf_put_quoted` escapes them.
fn quoted(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// Render the file, one `network={}` block per profile.
///
/// Every profile is validated first, and **one bad profile writes no file at
/// all**: half a `Wireless.prefs` is a card that joins some networks and
/// silently not others.
pub fn render(profiles: &[Profile]) -> CoreResult<String> {
    for profile in profiles {
        validate(profile)?;
    }

    let mut out = String::new();
    for profile in profiles {
        out.push_str("network={\n");
        out.push_str("    ssid=");
        out.push_str(&quoted(&profile.ssid));
        out.push('\n');

        match profile.security {
            Security::Open => {
                out.push_str("    key_mgmt=NONE\n");
                out.push_str("    scan_ssid=1\n");
            }
            Security::Wpa if is_hex_pmk(profile.psk.expose()) => {
                // Verbatim: the hashed form carries no `scan_ssid`.
                out.push_str("    psk=");
                out.push_str(profile.psk.expose());
                out.push('\n');
            }
            Security::Wpa => {
                // **No `key_mgmt`**: the supplicant defaults to WPA-PSK when a
                // `psk` is present, and writing one is a different
                // configuration rather than a harmless extra line.
                out.push_str("    psk=");
                out.push_str(&quoted(profile.psk.expose()));
                out.push('\n');
                out.push_str("    scan_ssid=1\n");
            }
        }

        if profile.priority != 0 {
            out.push_str("    priority=");
            out.push_str(&profile.priority.to_string());
            out.push('\n');
        }
        out.push_str("}\n");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wpa(ssid: &str, psk: &str) -> Profile {
        Profile {
            ssid: ssid.into(),
            security: Security::Wpa,
            psk: Secret::new(psk),
            priority: 0,
        }
    }

    fn open(ssid: &str) -> Profile {
        Profile {
            ssid: ssid.into(),
            security: Security::Open,
            psk: Secret::new(""),
            priority: 0,
        }
    }

    /// The shape `tw_wpaconf_write` emits, byte for byte.
    #[test]
    fn a_wpa_network_is_the_block_the_supplicant_reads() {
        assert_eq!(
            render(&[wpa("Tolun-Ev", "correct-horse")]).unwrap(),
            "network={\n    ssid=\"Tolun-Ev\"\n    psk=\"correct-horse\"\n    scan_ssid=1\n}\n"
        );
    }

    /// **No `key_mgmt` for WPA.** The supplicant defaults to WPA-PSK when a
    /// `psk` is present; writing one is a different configuration, not an
    /// extra line.
    #[test]
    fn a_wpa_block_carries_no_key_mgmt_line() {
        assert!(!render(&[wpa("N", "passphrase")])
            .unwrap()
            .contains("key_mgmt"));
    }

    #[test]
    fn an_open_network_says_so_and_carries_no_psk() {
        assert_eq!(
            render(&[open("Kafe")]).unwrap(),
            "network={\n    ssid=\"Kafe\"\n    key_mgmt=NONE\n    scan_ssid=1\n}\n"
        );
    }

    /// **The rule their own comment states in capitals**: never quote the hex
    /// form, never leave a passphrase bare.
    #[test]
    fn a_precomputed_key_is_unquoted_and_a_passphrase_is_quoted() {
        let pmk = "3543848bc38c4b0f".repeat(4); // 64 hex characters
        assert_eq!(pmk.len(), PMK_HEX);
        let hashed = render(&[wpa("N", &pmk)]).unwrap();
        assert!(hashed.contains(&format!("    psk={pmk}\n")), "{hashed}");
        assert!(!hashed.contains("psk=\""), "the hex form is never quoted");
        // And the hashed form is emitted verbatim: no scan_ssid.
        assert!(!hashed.contains("scan_ssid"), "{hashed}");

        let passphrase = render(&[wpa("N", "a passphrase")]).unwrap();
        assert!(passphrase.contains("    psk=\"a passphrase\"\n"));
    }

    /// 63 hex characters is a passphrase, not a key. One character decides
    /// which of two shapes is written.
    #[test]
    fn the_hex_test_is_the_length_as_well_as_the_alphabet() {
        assert!(is_hex_pmk(&"a".repeat(64)));
        assert!(!is_hex_pmk(&"a".repeat(63)));
        assert!(!is_hex_pmk(&"a".repeat(65)));
        assert!(!is_hex_pmk(&"g".repeat(64)), "'g' is not a hex digit");
        assert!(is_hex_pmk(&"AbCdEf01".repeat(8)), "either case");
    }

    #[test]
    fn a_priority_is_written_only_when_it_says_something() {
        let none = render(&[wpa("N", "passphrase")]).unwrap();
        assert!(!none.contains("priority"));

        let ranked = render(&[Profile {
            priority: 3,
            ..wpa("N", "passphrase")
        }])
        .unwrap();
        assert!(ranked.contains("    priority=3\n"), "{ranked}");
    }

    /// Quotes and backslashes are escaped, the way `buf_put_quoted` escapes
    /// them \u2014 an SSID really can contain either.
    #[test]
    fn a_quote_or_a_backslash_in_a_name_is_escaped() {
        let out = render(&[wpa("say \"hi\"\\now", "passphrase")]).unwrap();
        assert!(out.contains(r#"ssid="say \"hi\"\\now""#), "{out}");
    }

    /// **The escaping covers `"` and `\` and not newlines**, so a control byte
    /// would inject lines into the block. Refused in both fields, and the
    /// refusal about a passphrase names the **network** rather than the
    /// passphrase.
    #[test]
    fn a_control_character_is_refused_in_either_field() {
        let in_name = render(&[wpa("evil\nnetwork={", "passphrase")]).unwrap_err();
        assert!(in_name.to_string().contains("control character"));

        // Deliberately not a value that is a substring of the sentence: the
        // first version used `pass\nword`, and "pass" sits inside the word
        // "passphrase" — an assertion that fails for the wrong reason is as
        // useless as one that passes for the wrong reason.
        let in_psk = render(&[wpa("Tolun-Ev", "hunter2\nhunter2")]).unwrap_err();
        let said = in_psk.to_string();
        assert!(said.contains("control character"));
        assert!(said.contains("Tolun-Ev"), "names the network");
        assert!(
            !said.contains("hunter2"),
            "and never the passphrase: {said}"
        );
    }

    /// Neither halfway state is written: a secured network with no passphrase
    /// would emit `psk=""`, and an open one with a passphrase would silently
    /// drop it.
    #[test]
    fn neither_halfway_state_reaches_the_file() {
        assert!(render(&[wpa("N", "")]).is_err());
        assert!(render(&[Profile {
            psk: Secret::new("something"),
            ..open("N")
        }])
        .is_err());
    }

    /// A passphrase outside the supplicant's own 8..63 is refused **and the
    /// sentence says the range without saying the value**.
    #[test]
    fn a_passphrase_of_the_wrong_length_is_refused_without_being_quoted() {
        let err = render(&[wpa("Tolun-Ev", "short")]).unwrap_err().to_string();
        assert!(err.contains("8"), "{err}");
        assert!(err.contains("63"), "{err}");
        assert!(err.contains('5'), "it may say how long it is: {err}");
        assert!(!err.contains("short"), "but never what it is: {err}");
    }

    /// **One bad profile writes no file at all.** Half a `Wireless.prefs` is a
    /// card that joins some networks and silently not others.
    #[test]
    fn one_bad_profile_stops_the_whole_file() {
        let out = render(&[wpa("Good", "passphrase"), wpa("Bad", "")]);
        assert!(out.is_err());
    }

    #[test]
    fn several_networks_are_several_blocks_in_order() {
        let out = render(&[
            Profile {
                priority: 2,
                ..wpa("Home", "passphrase")
            },
            open("Kafe"),
        ])
        .unwrap();
        assert_eq!(out.matches("network={").count(), 2);
        assert!(out.find("Home").unwrap() < out.find("Kafe").unwrap());
    }

    #[test]
    fn nothing_to_write_is_an_empty_file_rather_than_an_error() {
        assert_eq!(render(&[]).unwrap(), "");
    }

    /// The passphrase cannot be logged, whatever anybody does with a
    /// `Profile`. G14's rule, as a property of the type.
    #[test]
    fn a_profile_prints_no_passphrase() {
        let printed = format!("{:?}", wpa("Tolun-Ev", "correct-horse"));
        assert!(printed.contains("Tolun-Ev"));
        assert!(!printed.contains("correct-horse"), "{printed}");
        assert!(printed.contains("********"), "{printed}");
    }

    /// It arrives from the screen like any other request.
    #[test]
    fn a_profile_deserialises_from_the_wire() {
        let profile: Profile = serde_json::from_str(
            r#"{"ssid":"Tolun-Ev","security":"wpa","psk":"correct-horse","priority":1}"#,
        )
        .unwrap();
        assert_eq!(profile.ssid, "Tolun-Ev");
        assert_eq!(profile.priority, 1);
        assert!(render(&[profile]).unwrap().contains("correct-horse"));
    }
}
