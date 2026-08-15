//! Whether a conditional [`Component`](super::Component) is switched on —
//! the ROM half only. The rest of planning (Task 5: turning a recipe plus a
//! media folder into an ordered list of copies) is not built yet.
//!
//! ## The ROM's own header, never `KNOWN_ROMS`
//!
//! `Condition::RomOlderThan` exists because `Workbench3.2.adf:S/Startup-sequence`
//! opens with `Version exec.library version 47` / `If Warn` / … / `Quit` — a
//! 3.2 system installed on an older Kickstart, without `LIBS:Modules`, does
//! not boot at all. So the condition has to be decided, not skipped.
//!
//! It is decided from `core::rom::stated_version`, which reads the major and
//! minor a Kickstart states about itself at offset 12 in its own header —
//! never from `KNOWN_ROMS`, the curated table of dump checksums. ART-104 is
//! why: the user's own licensed A1200 Kickstart hashes to a dump that table
//! does not carry, so it comes back unidentified even though it is a
//! perfectly good 3.1 ROM. A condition resting on that table would misfire
//! on a ROM that is right; asking the ROM what it is costs nothing extra and
//! cannot be wrong about a dump nobody has catalogued yet.
//!
//! ## Refuse, never guess
//!
//! An unidentified ROM makes [`condition_holds`] return
//! `Err(RefusalReason::RomUnknown)` rather than picking a default. Guessing
//! "off" on a pre-V47 ROM produces a system that quits at boot; guessing
//! "on" on a V47 ROM wastes 800 KB installing modules nothing loads. Neither
//! is ART's to choose for the user, so neither is chosen.
//!
//! ## Two functions, kept apart on purpose
//!
//! `condition_holds` is pure — it takes the facts already read, never a
//! `Path` — so Task 5 can call it once per conditional component in a recipe
//! without re-reading the ROM file each time. `rom_facts` is the one place
//! that touches disk, called once per install plan.

use std::path::Path;

use super::{Condition, RefusalReason};
use crate::core::error::{CoreError, CoreResult};

/// What a planning decision needs to know about the paired Kickstart.
///
/// Only the major: every `Condition` variant so far (`RomOlderThan`) tests
/// the major alone, and `stated_version`'s minor is trivially available
/// later — by widening this struct — should a future condition ever need
/// finer granularity than "3.1 vs 3.2". Carrying it now, unread by anything,
/// would be a guess about a need that does not exist yet, which is exactly
/// what this module's own `RomOlderThan` rule is built to avoid making
/// about the ROM itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RomFacts {
    pub major: u16,
}

/// Read the paired Kickstart's own stated major.
///
/// Strips a Cloanto header first (`core::rom::strip_cloanto_header`) — the
/// user has Amiga Forever, and those dumps carry an 11-byte `AMIROMTYPE1`
/// prefix that is not part of the ROM proper. A Kickstart is 512 KB, small
/// enough that reading it whole here does not need the windowed-read
/// treatment `open_hdf` gives a multi-gigabyte HDF.
pub fn rom_facts(rom: &Path) -> CoreResult<RomFacts> {
    let bytes = crate::core::rom::strip_cloanto_header(&std::fs::read(rom)?);
    let (major, _minor) = crate::core::rom::stated_version(&bytes).ok_or_else(|| {
        CoreError::InvalidInput("this file does not state a Kickstart version".into())
    })?;
    Ok(RomFacts { major })
}

/// Whether a conditional component switches on, given the facts already
/// read about the paired ROM — `None` when the ROM could not be identified
/// at all, which refuses rather than guessing (see the module doc comment).
pub fn condition_holds(
    condition: &Condition,
    rom: Option<&RomFacts>,
) -> Result<bool, RefusalReason> {
    let rom = rom.ok_or(RefusalReason::RomUnknown)?;
    match condition {
        Condition::RomOlderThan { major } => Ok(rom.major < *major),
    }
}

#[cfg(test)]
mod condition_tests {
    use super::*;

    /// `Workbench3.2.adf:S/Startup-sequence` opens with
    /// `Version exec.library version 47 … If Warn … Quit`. So a 3.2 system on a
    /// 3.1 ROM without `LIBS:Modules` does not boot at all.
    #[test]
    fn a_pre_v47_rom_turns_the_modules_component_on() {
        let holds = condition_holds(
            &Condition::RomOlderThan { major: 47 },
            Some(&RomFacts { major: 40 }),
        );
        assert_eq!(holds, Ok(true));
    }

    #[test]
    fn a_v47_rom_leaves_it_off() {
        let holds = condition_holds(
            &Condition::RomOlderThan { major: 47 },
            Some(&RomFacts { major: 47 }),
        );
        assert_eq!(holds, Ok(false));
    }

    /// Guessing costs 800 KB, or a system that quits at boot. Neither is ART's
    /// to choose.
    #[test]
    fn an_unidentified_rom_refuses_rather_than_guessing() {
        let holds = condition_holds(&Condition::RomOlderThan { major: 47 }, None);
        assert_eq!(holds, Err(RefusalReason::RomUnknown));
    }

    /// The ROM's own header, not `KNOWN_ROMS` — the user's licensed A1200 dump
    /// is not in that table (ART-104) and is still a perfectly good 3.1 ROM.
    ///
    /// The brief's own version of this test used `tempfile::tempdir()`, but
    /// this project deliberately does not depend on `tempfile` (see
    /// `fixtures::scratch`'s doc comment) — `scratch` is the repository's
    /// existing way to get a private directory for one test.
    #[test]
    fn the_major_comes_from_the_roms_own_header() {
        let dir = super::super::fixtures::scratch("plan-rom-header");
        let path = dir.join("fake.rom");
        let mut bytes = vec![0u8; 512 * 1024];
        bytes[12..14].copy_from_slice(&40u16.to_be_bytes());
        bytes[14..16].copy_from_slice(&68u16.to_be_bytes());
        std::fs::write(&path, &bytes).unwrap();

        assert_eq!(rom_facts(&path).unwrap().major, 40);
    }

    /// The user has licensed Amiga Forever (desktop and mobile — see
    /// `docs/STATUS.md`), so a Cloanto-headered dump is ordinary input on
    /// this machine, not an edge case. Without the strip, `rom_facts` would
    /// read bytes 12..16 eleven bytes early, land outside the plausible
    /// major range, and refuse a perfectly good ROM — ART-104's exact shape,
    /// surfacing at the user's Amiga instead of in CI. `fake_rom` alone
    /// cannot express this: it never carries the `AMIROMTYPE1` prefix, so
    /// this test builds one by hand, the one byte-for-byte thing `fake_rom`
    /// does not do.
    #[test]
    fn a_cloanto_headered_dump_still_reads_its_stated_major() {
        let dir = super::super::fixtures::scratch("plan-rom-cloanto");
        let path = dir.join("cloanto.rom");

        let mut bytes = b"AMIROMTYPE1".to_vec();
        let mut body = vec![0u8; 512 * 1024];
        body[12..14].copy_from_slice(&40u16.to_be_bytes());
        body[14..16].copy_from_slice(&68u16.to_be_bytes());
        bytes.extend_from_slice(&body);
        std::fs::write(&path, &bytes).unwrap();

        assert_eq!(rom_facts(&path).unwrap().major, 40);
    }

    /// A file that exists and reads fine but is not a ROM at all — plain
    /// text, far too short to carry a version field. This is the case
    /// `an_unreadable_rom_is_a_core_error_not_a_panic` (a *missing* file)
    /// could not pin: that test's `is_err()` would pass for an I/O failure
    /// just as readily as for a content problem, so it never proved
    /// `rom_facts` actually rejects bad content rather than merely
    /// propagating `std::fs::read`'s own error. This one names the exact
    /// variant.
    #[test]
    fn content_that_is_not_a_rom_is_refused_as_invalid_input() {
        let dir = super::super::fixtures::scratch("plan-rom-not-a-rom");
        let path = dir.join("readme.txt");
        std::fs::write(&path, b"this is not a Kickstart image").unwrap();

        assert!(matches!(rom_facts(&path), Err(CoreError::InvalidInput(_))));
    }
}
