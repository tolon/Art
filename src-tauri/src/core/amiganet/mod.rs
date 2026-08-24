//! Pre-seeding an Amiga's network before its first boot.
//!
//! SD-3 G14's WiFi and networking half. A PiStorm card ART builds can reach
//! the network the first time it starts, instead of the owner typing a
//! passphrase into an Amiga whose keyboard is still American — the same
//! argument the keymap selection makes (ART-226), and the reason both belong
//! to the same round.
//!
//! # Which files, and how that was settled
//!
//! **Not by recalling that Amiga networking lives in `DEVS:NetInterfaces`.**
//! That is Roadshow's, and this owner's stack is their own. Read on
//! 2026-08-24 from their two projects:
//!
//! | | file | what it holds |
//! |---|---|---|
//! | `tolunwifi` | `ENVARC:Sys/Wireless.prefs` (+ `ENV:` twin) | the WiFi credentials — **the secret** |
//! | `tolunnet` | `DEVS:tolunnet.config` | device, unit, DHCP or static address |
//! | Roadshow | `DEVS:NetInterfaces/<name>` | what ART would have written, wrongly |
//!
//! `tolunwifi` configures the **driver** and is stack-agnostic; `tolunnet` is
//! the TCP/IP stack. Two files, two subjects, and only one of them has a
//! passphrase in it.
//!
//! # The secret is a type, not a rule
//!
//! G14 says the PSK must stay out of the operation log, the manifest and any
//! AI prompt. [`Secret`] makes that a compile error rather than something
//! three call sites have to remember: it deserialises and does not serialise,
//! and `Debug` prints stars.

pub mod secret;
pub mod seed;
pub mod tolunnet;
pub mod wpa;

pub use secret::Secret;
