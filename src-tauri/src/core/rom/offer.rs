//! *"This title asks for `kick34005.A500` — you have it. Place it?"*
//!
//! [ART-130], and work-list item 11. G10 built the **reading** half: a WHDLoad
//! slave at `ws_Version >= 16` declares the Kickstart image it needs by name,
//! by size and by CRC-16, ART reads all three and **reports** which declared
//! images are missing from a tree. What it never did was close the loop, and
//! that was deliberate rather than forgotten.
//!
//! # It offers; it never places
//!
//! **The owner's own decision, 2026-08-21**: yes, ART should close the loop —
//! *"but in its own round, and always as a proposal."* Putting somebody's ROM
//! onto their card touches ROM Manager, the licensed Amiga Forever decode path
//! and the card's layout, and those are their decisions rather than a side
//! effect of a metadata pass.
//!
//! So **nothing in this module writes a file**. It answers one question —
//! *which of the images this title will accept do you already have?* — and the
//! placing is a separate, confirmed action somewhere else.
//!
//! # Matched by checksum, never by filename
//!
//! A slave declares the CRC-16 of the **image bytes** WHDLoad will load. What
//! the file is called in somebody's collection says nothing at all: a ROM
//! named `kick34005.A500` may be any dump, and the one that matches may be
//! called `kick.rom`. So the match is [`RomInfo::whdload_crc16`], computed over
//! the decoded image when `identify_rom` read one.
//!
//! Size is checked too, and a **size that disagrees with a matching checksum
//! is reported rather than resolved** — two dumps whose bytes hash the same and
//! whose lengths differ is not something this module gets to decide about.
//!
//! # Four endings, not two
//!
//! `Supplied`, `Encrypted`, `NotHere`, `Unreadable` — because "ART cannot
//! offer this" collapses four different next steps into one sentence, which is
//! this project's most expensive defect class in its ordinary form. The one
//! that matters most is `Encrypted`: a licensed Amiga Forever ROM without its
//! `rom.key` beside it is a file the user **has**, and telling them they do not
//! have it would send them looking for something already on their disk.
//!
//! # This module declares its own record
//!
//! [`WantedImage`] rather than `core::gameindex`'s `KickstartNeed`, and that is
//! the layering rule CLAUDE.md states with this exact module as its example: a
//! lower `core/` module must not import a higher one, and `core/rom` taking
//! `core::osinstall::PairedRom` for two fields is the named mistake. The
//! command layer maps one to the other, the way
//! `commands/preload.rs::rom_pairing_for` already does for `core/rom/pairing`.
//!
//! [ART-130]: ../../../../docs/ISSUES.md

use serde::{Deserialize, Serialize};

use super::RomInfo;

/// One Kickstart image something asks for.
///
/// **This module's own record**, carrying only what matching needs — see the
/// module doc on why it is not `core::gameindex::KickstartNeed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WantedImage {
    /// The name WHDLoad looks for in `DEVS:Kickstarts/`, e.g.
    /// `kick34005.A500`. **Carried to say it, never to match on it** — see the
    /// module doc.
    pub name: String,
    /// WHDLoad's CRC-16/ARC over the image. `None` when the slave declared the
    /// `$ffff` sentinel, which is not a checksum but a way of saying "the name
    /// field is a list".
    pub crc16: Option<u16>,
    /// The image's length, when the slave states one.
    pub size: Option<u32>,
}

/// Which ROM in the collection answers a wanted image.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuppliedBy {
    /// The file, so the confirmation can name it.
    pub path: String,
    /// What `identify_rom` calls it — *"Kickstart 3.1 (40.068)"*.
    pub name: String,
    /// Present and different from [`WantedImage::size`]: reported, never
    /// resolved. Two dumps whose checksums agree and whose lengths do not is
    /// not something this module decides about.
    pub size_disagrees: Option<u32>,
}

/// What ART can say about one image a title asks for.
///
/// **Four endings, and they stay apart.** Collapsing them into "cannot offer"
/// would tell a user with an encrypted ROM the same thing it tells a user with
/// no ROM at all, and those are different next steps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum Offer {
    /// It is here, and ART can name the file.
    Supplied { wanted: WantedImage, by: SuppliedBy },
    /// A licensed Amiga Forever ROM is in the collection whose checksum ART
    /// cannot compute, because its `rom.key` is not beside it. **The user may
    /// well have this image**; ART cannot tell, and says so rather than
    /// reporting it missing.
    Encrypted {
        wanted: WantedImage,
        /// Every unreadable candidate, so the sentence can name them.
        candidates: Vec<String>,
    },
    /// Nothing in the collection carries that checksum.
    NotHere { wanted: WantedImage },
    /// The slave declared no checksum, so nothing can be matched. Not a
    /// failure of the collection: `$ffff` is how a slave says "the name field
    /// is a list", and a list is what [`offer_for`] was given.
    Unmatchable { wanted: WantedImage },
}

impl Offer {
    /// The image this offer is about, whichever ending it is.
    pub fn wanted(&self) -> &WantedImage {
        match self {
            Self::Supplied { wanted, .. }
            | Self::Encrypted { wanted, .. }
            | Self::NotHere { wanted }
            | Self::Unmatchable { wanted } => wanted,
        }
    }

    /// Whether this one can be placed. The only question a button asks.
    pub fn can_place(&self) -> bool {
        matches!(self, Self::Supplied { .. })
    }
}

/// What ART found for every image a title will accept.
///
/// One entry per wanted image, **in the order given** — a slave's own order is
/// its preference, and reordering it would be ART expressing one.
///
/// A title that names three images and has one of them is satisfied. That
/// judgement is [`any_can_be_placed`]'s, kept beside the data rather than left
/// for each caller to re-derive.
pub fn offer_for(wanted: &[WantedImage], collection: &[RomInfo]) -> Vec<Offer> {
    wanted
        .iter()
        .map(|image| offer_one(image, collection))
        .collect()
}

/// Does the collection satisfy the title at all?
///
/// **One is enough.** An AGA title commonly names an A600, an A1200 and an
/// A4000 ROM, and WHDLoad needs whichever one it finds — reporting such a
/// title as unsatisfied because two of the three are absent is the sentence
/// ART-137 already cost this project once.
pub fn any_can_be_placed(offers: &[Offer]) -> bool {
    offers.iter().any(Offer::can_place)
}

fn offer_one(image: &WantedImage, collection: &[RomInfo]) -> Offer {
    let Some(crc16) = image.crc16 else {
        return Offer::Unmatchable {
            wanted: image.clone(),
        };
    };

    if let Some(found) = collection
        .iter()
        .find(|rom| rom.whdload_crc16 == Some(crc16))
    {
        let size_disagrees = image
            .size
            .filter(|size| *size as usize != found.size_bytes)
            .map(|_| found.size_bytes as u32);
        return Offer::Supplied {
            wanted: image.clone(),
            by: SuppliedBy {
                path: found.file_path.clone(),
                name: found.name.clone(),
                size_disagrees,
            },
        };
    }

    // Nothing matched. Before saying "you do not have it", look for the case
    // where ART simply could not read a file the user does have.
    let candidates: Vec<String> = collection
        .iter()
        .filter(|rom| rom.is_cloanto && !rom.key_available)
        .map(|rom| rom.file_path.clone())
        .collect();
    if candidates.is_empty() {
        Offer::NotHere {
            wanted: image.clone(),
        }
    } else {
        Offer::Encrypted {
            wanted: image.clone(),
            candidates,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::rom::RomChecksum;

    fn rom(name: &str, path: &str, crc16: Option<u16>, size: usize) -> RomInfo {
        RomInfo {
            name: name.to_string(),
            version: "3.1".to_string(),
            revision: "40.068".to_string(),
            size_bytes: size,
            sha256: String::new(),
            crc32: String::new(),
            is_cloanto: false,
            key_available: false,
            is_aros: false,
            checksum: RomChecksum::NotChecked,
            compatible_models: Vec::new(),
            file_path: path.to_string(),
            major: Some(40),
            whdload_crc16: crc16,
        }
    }

    fn encrypted(path: &str) -> RomInfo {
        RomInfo {
            is_cloanto: true,
            key_available: false,
            whdload_crc16: None,
            ..rom(
                "Amiga Forever ROM (encrypted, needs rom.key)",
                path,
                None,
                0,
            )
        }
    }

    fn wanted(name: &str, crc16: Option<u16>, size: Option<u32>) -> WantedImage {
        WantedImage {
            name: name.to_string(),
            crc16,
            size,
        }
    }

    #[test]
    fn a_title_that_asks_for_a_rom_the_user_has_gets_the_file_named() {
        let collection = vec![
            rom(
                "Kickstart 1.3",
                "E:\\roms\\kick13.rom",
                Some(0x1234),
                262_144,
            ),
            rom(
                "Kickstart 3.1",
                "E:\\roms\\kick31.rom",
                Some(0xABCD),
                524_288,
            ),
        ];
        let offers = offer_for(
            &[wanted("kick40068.A1200", Some(0xABCD), Some(524_288))],
            &collection,
        );

        let Offer::Supplied { by, wanted } = &offers[0] else {
            panic!("expected a match, got {:?}", offers[0]);
        };
        assert_eq!(by.path, "E:\\roms\\kick31.rom");
        assert_eq!(wanted.name, "kick40068.A1200");
        assert!(by.size_disagrees.is_none());
        assert!(any_can_be_placed(&offers));
    }

    /// **The match is the checksum, and the filename says nothing.** A ROM
    /// called `kick.rom` answers a slave asking for `kick34005.A500`, and one
    /// *called* `kick34005.A500` does not answer it unless its bytes do.
    #[test]
    fn the_name_in_the_collection_is_not_what_matches() {
        let collection = vec![
            // Named exactly what the slave asks for, wrong bytes.
            rom(
                "Kickstart 3.1",
                "E:\\roms\\kick34005.A500",
                Some(0x1111),
                262_144,
            ),
            // Named nothing in particular, right bytes.
            rom(
                "Kickstart 1.3",
                "E:\\roms\\anything.bin",
                Some(0x2222),
                262_144,
            ),
        ];
        let offers = offer_for(&[wanted("kick34005.A500", Some(0x2222), None)], &collection);
        let Offer::Supplied { by, .. } = &offers[0] else {
            panic!("{:?}", offers[0]);
        };
        assert_eq!(by.path, "E:\\roms\\anything.bin");
    }

    /// **The ending that matters most.** A licensed Amiga Forever ROM without
    /// its `rom.key` is a file the user *has*; telling them it is missing
    /// sends them looking for something already on their disk.
    #[test]
    fn an_encrypted_rom_is_its_own_answer_and_not_a_missing_one() {
        let collection = vec![encrypted("E:\\Amiga Forever\\amiga-os-310-a1200.rom")];
        let offers = offer_for(
            &[wanted("kick40068.A1200", Some(0xABCD), None)],
            &collection,
        );

        let Offer::Encrypted { candidates, .. } = &offers[0] else {
            panic!("an unreadable file is not an absent one: {:?}", offers[0]);
        };
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].ends_with("amiga-os-310-a1200.rom"));
        assert!(
            !any_can_be_placed(&offers),
            "ART cannot place what it cannot read"
        );
    }

    /// **A Cloanto ROM *with* its key is an ordinary ROM.** ART decodes it,
    /// checksums it and matches on it like any other — so it must never be
    /// listed among the files ART could not read. Found by mutation: dropping
    /// `!key_available` from the candidate filter failed nothing, because
    /// every fixture here was keyless.
    #[test]
    fn a_decodable_cloanto_rom_is_not_an_unreadable_one() {
        let decodable = RomInfo {
            is_cloanto: true,
            key_available: true,
            ..rom(
                "Kickstart 3.1",
                "E:\\Amiga Forever\\a1200.rom",
                Some(0x4242),
                524_288,
            )
        };
        let collection = vec![decodable];

        // It answers a title that wants it, like any other ROM.
        let hit = offer_for(
            &[wanted("kick40068.A1200", Some(0x4242), None)],
            &collection,
        );
        assert!(hit[0].can_place(), "{:?}", hit[0]);

        // And when a *different* image is wanted, it is not reported as a
        // file ART could not read — ART read it perfectly well.
        let miss = offer_for(&[wanted("kick34005.A500", Some(0x9999), None)], &collection);
        assert!(
            matches!(miss[0], Offer::NotHere { .. }),
            "a ROM ART decoded is not an unreadable candidate: {:?}",
            miss[0]
        );
    }

    #[test]
    fn nothing_at_all_is_said_plainly() {
        let collection = vec![rom(
            "Kickstart 1.3",
            "E:\\roms\\kick13.rom",
            Some(0x1111),
            262_144,
        )];
        let offers = offer_for(
            &[wanted("kick40068.A1200", Some(0xABCD), None)],
            &collection,
        );
        assert!(matches!(offers[0], Offer::NotHere { .. }));
    }

    /// The `$ffff` sentinel is not a checksum. A slave using it is saying "the
    /// name field is a list", so there is nothing to match and saying "you do
    /// not have it" would be a claim about a ROM that does not exist.
    #[test]
    fn an_image_with_no_checksum_is_unmatchable_rather_than_missing() {
        let collection = vec![rom(
            "Kickstart 3.1",
            "E:\\roms\\kick31.rom",
            Some(0xABCD),
            524_288,
        )];
        let offers = offer_for(&[wanted("a list, not a name", None, None)], &collection);
        assert!(matches!(offers[0], Offer::Unmatchable { .. }));
    }

    /// **One of three is enough.** An AGA title names an A600, an A1200 and an
    /// A4000 ROM; WHDLoad needs whichever it finds. Reporting the title as
    /// unsatisfied because two are absent is the sentence ART-137 already cost
    /// this project once.
    #[test]
    fn a_title_naming_three_images_is_satisfied_by_one() {
        let collection = vec![rom(
            "Kickstart 3.1",
            "E:\\roms\\kick31.rom",
            Some(0xB00B),
            524_288,
        )];
        let offers = offer_for(
            &[
                wanted("kick40063.A600", Some(0x0001), None),
                wanted("kick40068.A1200", Some(0xB00B), None),
                wanted("kick40068.A4000", Some(0x0003), None),
            ],
            &collection,
        );
        assert_eq!(
            offers.len(),
            3,
            "one answer per image, in the slave's order"
        );
        assert!(matches!(offers[0], Offer::NotHere { .. }));
        assert!(offers[1].can_place());
        assert!(matches!(offers[2], Offer::NotHere { .. }));
        assert!(any_can_be_placed(&offers));
    }

    /// A checksum that matches and a size that does not is **reported, never
    /// resolved**. Two dumps whose bytes hash the same and whose lengths
    /// differ is not something this module gets to decide about.
    #[test]
    fn a_size_that_disagrees_is_carried_rather_than_hidden() {
        let collection = vec![rom(
            "Kickstart 3.1",
            "E:\\roms\\kick31.rom",
            Some(0xABCD),
            524_288,
        )];
        let offers = offer_for(
            &[wanted("kick40068.A1200", Some(0xABCD), Some(262_144))],
            &collection,
        );
        let Offer::Supplied { by, .. } = &offers[0] else {
            panic!("{:?}", offers[0]);
        };
        assert_eq!(by.size_disagrees, Some(524_288));
    }

    /// The order given is the order answered. A slave's own order is its
    /// preference, and reordering it would be ART expressing one.
    #[test]
    fn the_answers_come_back_in_the_order_they_were_asked() {
        let offers = offer_for(
            &[
                wanted("first", Some(1), None),
                wanted("second", Some(2), None),
                wanted("third", Some(3), None),
            ],
            &[],
        );
        assert_eq!(
            offers
                .iter()
                .map(|o| o.wanted().name.as_str())
                .collect::<Vec<_>>(),
            vec!["first", "second", "third"]
        );
    }

    #[test]
    fn an_empty_collection_is_not_a_panic() {
        assert!(offer_for(&[wanted("x", Some(1), None)], &[]).len() == 1);
        assert!(offer_for(&[], &[]).is_empty());
        assert!(!any_can_be_placed(&[]));
    }

    /// **Nothing here touches the filesystem**, which is what keeps the
    /// owner's *"always a proposal"* true by construction rather than by
    /// discipline.
    ///
    /// Observable, and that is the point of the test: a collection entry whose
    /// file has since been deleted is matched and reported exactly like any
    /// other, because matching reads `RomInfo` and nothing else. A module that
    /// had quietly started opening the file would fail here.
    #[test]
    fn matching_never_looks_at_the_filesystem() {
        let gone = "E:\roms\this-file-does-not-exist-anywhere.rom";
        assert!(
            !std::path::Path::new(gone).exists(),
            "the fixture's premise"
        );

        let collection = vec![rom("Kickstart 3.1", gone, Some(1), 524_288)];
        let offers = offer_for(&[wanted("kick40068.A1200", Some(1), None)], &collection);

        let Offer::Supplied { by, .. } = &offers[0] else {
            panic!("{:?}", offers[0]);
        };
        assert_eq!(by.path, gone);
        // `can_place` is a question, not a verb: it says a confirmation may be
        // offered, and whether the file is still there is that step's business.
        assert!(offers[0].can_place());
    }
}
