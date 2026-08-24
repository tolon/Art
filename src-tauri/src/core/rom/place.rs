//! Putting a Kickstart where WHDLoad will look for it \u2014 once, and only when
//! asked.
//!
//! [ART-130]'s other half. [`super::offer`] answers *"do you have it?"* and
//! deliberately cannot act; this is the acting, kept apart so that the
//! answering half stays a pure function over data and the writing half is one
//! small module somebody can read in full before trusting it.
//!
//! **The owner's decision, 2026-08-21, is the shape of this file**: ART offers,
//! and places only what the user has agreed to place, *"never as a silent
//! copy"*. So there is no function here that takes a title and does the right
//! thing; there is one that takes a single agreed placement and carries it out.
//!
//! # The name comes from a slave, and a slave is not ART's file
//!
//! `kick34005.A500` is read out of a WHDLoad `.slave` \u2014 a binary somebody
//! downloaded. Joining it to a path without checking is the archive-entry
//! traversal in a new costume, so it goes through
//! [`crate::core::security::safe_join`] like every other untrusted name.
//! A slave declaring `..\\..\\Windows\\System32\\kernel32.dll` is refused by
//! name rather than by luck.
//!
//! # A licensed ROM is written decoded
//!
//! An Amiga Forever ROM on disk is a header plus a repeating XOR, and WHDLoad
//! cannot read one. Copying the file verbatim would produce a card that looks
//! right and does not work \u2014 the confident-wrong shape, in the place it is
//! hardest to notice. [`super::decoded_image`] is what is written, which also
//! means a ROM whose `rom.key` is absent cannot be placed at all, and says so.
//!
//! # Four endings again, and one of them is a refusal
//!
//! `Placed` \u00b7 `AlreadyThere` \u00b7 `Occupied` \u00b7 and the errors. `AlreadyThere` is
//! the byte-for-byte identical file: doing nothing and saying so is right, and
//! reporting it as a failure would send somebody looking for a problem that is
//! not there. `Occupied` is a **different** file under that name, and it is
//! refused rather than replaced \u2014 `SAFE_CREATE`: the ROM already sitting there
//! is somebody's, and "it was already there" has never been a reason to lose
//! it.
//!
//! [ART-130]: ../../../../docs/ISSUES.md

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::core::error::{CoreError, CoreResult};
use crate::core::security::safe_join;

/// The drawer WHDLoad loads Kickstart images from, as AmigaDOS spells it.
///
/// `DEVS:Kickstarts` on the Amiga; two components on the host. Not a
/// caller-supplied path: every placement goes to the one place WHDLoad
/// actually looks, and offering to put a ROM somewhere else would be offering
/// something that does not work.
pub const KICKSTART_DRAWER: [&str; 2] = ["Devs", "Kickstarts"];

/// One placement the user has agreed to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    /// The ROM in the user's collection.
    pub from: PathBuf,
    /// The name the slave asks for \u2014 **untrusted**, and checked as such.
    pub as_name: String,
    /// The system volume it goes onto.
    pub tree: PathBuf,
}

/// What happened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum PlaceOutcome {
    /// Written. `to` is where, so the sentence can name it.
    Placed { to: String, bytes: usize },
    /// The identical image is already there. Nothing was written, and that is
    /// not a failure.
    AlreadyThere { to: String },
    /// A **different** file already has that name. Refused, never replaced.
    Occupied { to: String },
}

/// Where a placement would go, refusing a name that is not a name.
///
/// Separate from [`place`] so a caller can show the destination *before* the
/// confirmation \u2014 the same reason `osinstall_destination_taken` exists: a
/// refusal the user only meets after committing reads as the application doing
/// nothing.
pub fn destination_of(placement: &Placement) -> CoreResult<PathBuf> {
    let drawer = KICKSTART_DRAWER
        .iter()
        .fold(placement.tree.clone(), |at, part| at.join(part));
    safe_join(&drawer, &placement.as_name).map_err(|err| {
        CoreError::SafetyRefused(format!(
            "'{}' is not a name a Kickstart can be written under: {err}",
            placement.as_name
        ))
    })
}

/// Carry out one agreed placement.
///
/// Writes the **decoded** image, so a licensed Amiga Forever ROM lands as
/// something WHDLoad can actually load \u2014 and a ROM whose `rom.key` is not
/// beside it fails here rather than producing a card that looks right.
pub fn place(placement: &Placement) -> CoreResult<PlaceOutcome> {
    let to = destination_of(placement)?;
    let bytes = super::decoded_image(&placement.from)?;

    if to.exists() {
        // Byte-for-byte identical is not a conflict, and calling it one would
        // send somebody looking for a problem that is not there.
        let existing = std::fs::read(&to)?;
        if existing == bytes {
            return Ok(PlaceOutcome::AlreadyThere {
                to: to.display().to_string(),
            });
        }
        return Ok(PlaceOutcome::Occupied {
            to: to.display().to_string(),
        });
    }

    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::core::safety::atomic_write(&to, &bytes)?;
    Ok(PlaceOutcome::Placed {
        to: to.display().to_string(),
        bytes: bytes.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ScratchDir;
    use std::path::Path;

    fn scratch(tag: &str) -> ScratchDir {
        ScratchDir::new("art-rom-place", tag)
    }

    /// A plain, readable ROM image. `decoded_image` returns a non-Cloanto file
    /// unchanged, so any bytes will do for the placing tests.
    fn a_rom(at: &Path, name: &str) -> PathBuf {
        let path = at.join(name);
        std::fs::write(&path, vec![0xA5u8; 4096]).unwrap();
        path
    }

    fn placement(from: PathBuf, tree: &Path, name: &str) -> Placement {
        Placement {
            from,
            as_name: name.to_string(),
            tree: tree.to_path_buf(),
        }
    }

    #[test]
    fn it_lands_in_the_drawer_whdload_reads() {
        let dir = scratch("lands");
        let rom = a_rom(dir.path(), "kick31.rom");
        let tree = dir.join("dist");
        std::fs::create_dir_all(&tree).unwrap();

        let done = place(&placement(rom, &tree, "kick40068.A1200")).unwrap();
        let PlaceOutcome::Placed { to, bytes } = done else {
            panic!("{done:?}");
        };
        assert_eq!(bytes, 4096);
        // `DEVS:Kickstarts` and nothing else: a ROM anywhere else is a ROM
        // WHDLoad will not find.
        let expected = tree.join("Devs").join("Kickstarts").join("kick40068.A1200");
        assert_eq!(std::path::Path::new(&to), expected);
        assert_eq!(std::fs::read(&expected).unwrap().len(), 4096);
    }

    /// The drawer does not have to exist first. A tree ART built has a `Devs`;
    /// one somebody assembled by hand may not.
    #[test]
    fn the_drawer_is_made_when_it_is_not_there() {
        let dir = scratch("makes-drawer");
        let rom = a_rom(dir.path(), "kick.rom");
        let tree = dir.join("bare");
        std::fs::create_dir_all(&tree).unwrap();
        assert!(!tree.join("Devs").exists());

        place(&placement(rom, &tree, "kick34005.A500")).unwrap();
        assert!(tree.join("Devs").join("Kickstarts").is_dir());
    }

    /// **The name comes out of a downloaded binary.** A slave declaring a
    /// traversal is refused by name, not by luck \u2014 the archive-entry problem
    /// in a new costume.
    #[test]
    fn a_slave_that_declares_a_traversal_is_refused() {
        let dir = scratch("traversal");
        let rom = a_rom(dir.path(), "kick.rom");
        let tree = dir.join("dist");
        std::fs::create_dir_all(&tree).unwrap();

        for hostile in [
            "..\\..\\Windows\\System32\\kernel32.dll",
            "../../../etc/passwd",
            "C:\\Windows\\notepad.exe",
            "",
            "   ",
        ] {
            let err = place(&placement(rom.clone(), &tree, hostile)).unwrap_err();
            assert!(
                matches!(err, CoreError::SafetyRefused(_)),
                "'{hostile}' must be refused by name: {err:?}"
            );
        }

        // And nothing was written anywhere, including where it would have gone
        // had the check not been there.
        assert!(!tree.join("Devs").exists());
    }

    /// Byte-for-byte identical is not a conflict. Reporting it as one would
    /// send somebody looking for a problem that is not there.
    #[test]
    fn the_same_image_twice_is_said_plainly_and_written_once() {
        let dir = scratch("already");
        let rom = a_rom(dir.path(), "kick.rom");
        let tree = dir.join("dist");
        std::fs::create_dir_all(&tree).unwrap();

        let first = place(&placement(rom.clone(), &tree, "kick.A500")).unwrap();
        assert!(matches!(first, PlaceOutcome::Placed { .. }));

        let again = place(&placement(rom, &tree, "kick.A500")).unwrap();
        assert!(
            matches!(again, PlaceOutcome::AlreadyThere { .. }),
            "the identical image is not a conflict: {again:?}"
        );
    }

    /// **`SAFE_CREATE`.** A *different* ROM already under that name is
    /// somebody's, and "it was already there" has never been a reason to lose
    /// it.
    #[test]
    fn a_different_rom_under_that_name_is_refused_not_replaced() {
        let dir = scratch("occupied");
        let mine = a_rom(dir.path(), "mine.rom");
        let tree = dir.join("dist");
        let drawer = tree.join("Devs").join("Kickstarts");
        std::fs::create_dir_all(&drawer).unwrap();
        let theirs = drawer.join("kick.A500");
        std::fs::write(&theirs, b"somebody else's ROM").unwrap();

        let done = place(&placement(mine, &tree, "kick.A500")).unwrap();
        assert!(matches!(done, PlaceOutcome::Occupied { .. }), "{done:?}");
        assert_eq!(
            std::fs::read(&theirs).unwrap(),
            b"somebody else's ROM",
            "and it is byte-for-byte what it was"
        );
    }

    /// **A licensed ROM is written decoded**, because WHDLoad cannot read an
    /// Amiga Forever file. Copying it verbatim would produce a card that looks
    /// right and does not work.
    ///
    /// Without the key there is nothing to decode, so it fails here rather
    /// than placing something unusable.
    #[test]
    fn an_encrypted_rom_without_its_key_is_not_placed() {
        let dir = scratch("encrypted");
        let tree = dir.join("dist");
        std::fs::create_dir_all(&tree).unwrap();

        let locked = dir.join("amiga-os-310-a1200.rom");
        let mut bytes = b"AMIROMTYPE1".to_vec();
        bytes.extend_from_slice(&[0x5A; 512]);
        std::fs::write(&locked, &bytes).unwrap();

        assert!(
            place(&placement(locked, &tree, "kick40068.A1200")).is_err(),
            "a ROM ART cannot decode must not be placed as-is"
        );
        assert!(
            !tree
                .join("Devs")
                .join("Kickstarts")
                .join("kick40068.A1200")
                .exists(),
            "and nothing was written"
        );
    }

    /// The destination can be shown before the confirmation, which is why it
    /// is its own function \u2014 a refusal the user only meets after committing
    /// reads as the application doing nothing.
    #[test]
    fn the_destination_can_be_asked_for_without_writing() {
        let dir = scratch("dest-only");
        let tree = dir.join("dist");
        let to = destination_of(&placement(dir.join("nothing.rom"), &tree, "kick.A500")).unwrap();
        assert_eq!(to, tree.join("Devs").join("Kickstarts").join("kick.A500"));
        assert!(!tree.exists(), "asking must not create anything");
    }
}
