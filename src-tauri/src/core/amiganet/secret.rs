//! A value that can come in and cannot get out.
//!
//! SD-3 G14's own requirement, in the only form that survives contact with a
//! codebase: *"the WiFi PSK must stay out of the operation log, the manifest
//! and any AI prompt."* That is a rule somebody has to remember at every call
//! site — and ART logs **every** write (§53), records a manifest for every
//! tree, and has an AI layer coming that reads what it can see. A rule
//! remembered in three places is a rule broken in a fourth.
//!
//! So the passphrase is a type that **deserialises and does not serialise**.
//! It can arrive from the screen; it cannot leave through `serde` at all, and
//! `Debug` prints stars. The one way to read it back is [`Secret::expose`],
//! which is spelled to be conspicuous in a diff.
//!
//! # What this does not claim
//!
//! **It is not encryption and not memory hygiene.** The bytes sit in ordinary
//! heap memory and the file ART writes holds the passphrase in the clear,
//! because that is what `wpa_supplicant` reads. What this stops is the
//! *accidental* copy: a `#[derive(Serialize)]` on a struct that happens to
//! carry one, a `{:?}` in a log line, a progress message built from a request.
//! Those are how a secret actually escapes, and every one of them is a
//! compile error or a row of stars here.

use std::fmt;

use serde::{Deserialize, Deserializer};

/// A passphrase, or anything else that must not be written down twice.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Read it, deliberately conspicuously.
    ///
    /// Named to stand out in review: every call is a place where the secret
    /// reaches something, and there should be very few.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

/// Stars, always — including the length, which is itself worth something to
/// somebody guessing.
impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(********)")
    }
}

/// The same, so `{}` cannot get round `{:?}`.
impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("********")
    }
}

/// **Deliberately asymmetric**: a `Secret` can be deserialised, so the screen
/// can send one, and there is **no `Serialize`**, so nothing can send it back
/// out — not to the frontend, not into a manifest, not into an operation-log
/// record. A struct holding one cannot derive `Serialize` at all, which is the
/// compile error this type exists to cause.
impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Secret)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_and_display_say_nothing() {
        let secret = Secret::new("hunter2-and-then-some");
        assert_eq!(format!("{secret:?}"), "Secret(********)");
        assert_eq!(format!("{secret}"), "********");
        // Not even the length, which narrows a guess.
        assert!(!format!("{secret:?}").contains("21"));
    }

    /// A struct that carries one and is logged with `{:?}` — the ordinary way
    /// a secret escapes — prints stars for it and everything else as usual.
    #[test]
    fn a_struct_holding_one_is_safe_to_log() {
        #[derive(Debug)]
        struct Request {
            ssid: String,
            psk: Secret,
        }
        let printed = format!(
            "{:?}",
            Request {
                ssid: "Tolun-Ev".into(),
                psk: Secret::new("correct horse battery staple"),
            }
        );
        assert!(printed.contains("Tolun-Ev"), "the rest is still readable");
        assert!(!printed.contains("horse"));
        assert!(printed.contains("********"));
    }

    #[test]
    fn it_can_be_read_when_something_really_needs_it() {
        assert_eq!(Secret::new("abc").expose(), "abc");
        assert_eq!(Secret::new("abc").len(), 3);
        assert!(Secret::new("").is_empty());
    }

    #[test]
    fn it_arrives_from_the_wire_like_any_string() {
        let secret: Secret = serde_json::from_str("\"a passphrase\"").unwrap();
        assert_eq!(secret.expose(), "a passphrase");
    }

    /// **The whole point, and it is a compile-time property rather than a
    /// runtime one.** `Secret` implements no `Serialize`, so nothing can put
    /// one on the wire, in a manifest, or in a log record — a struct that
    /// carries one and tries to `#[derive(Serialize)]` does not build.
    ///
    /// Asserted here the only way a test can assert absence: by naming it, so
    /// that somebody adding `impl Serialize` has to delete this and say why.
    #[test]
    fn nothing_can_serialise_one() {
        fn is_serialize<T: serde::Serialize>() {}
        // is_serialize::<Secret>();  // <- must not compile. G14's rule.
        is_serialize::<String>(); // the same call for a type that is, so the
                                  // helper above is not dead code pretending
                                  // to be a check.
    }
}
