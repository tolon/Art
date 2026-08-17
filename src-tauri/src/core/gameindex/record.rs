//! The neutral record, and the rule that a statement is not a guess.

use serde::{Deserialize, Serialize};

use crate::core::hashing::sha256_bytes;

/// The schema this ART writes and understands.
///
/// A record carrying a higher number is refused rather than half-read — the
/// same rule `CardManifest` follows, and for the same reason: the fields this
/// ART does not know about are exactly the ones a newer build used to describe
/// something it cannot check.
pub const GAMEINDEX_SCHEMA: u32 = 1;

/// Where a fact came from. The first two **state**; the last two **suggest**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Provenance {
    /// `rp9-manifest.xml` inside an `.rp9` package.
    Rp9Manifest,
    /// The `WHDLoadSlave` structure's own `ws_name` / `ws_copy` / `ws_Flags`.
    WhdloadSlave,
    /// A TOSEC-shaped filename.
    TosecName,
    /// The name of the drawer the slave sits in — the weakest source, and the
    /// reason it is last: `Moonstone Install` is a drawer, not a title.
    DrawerName,
}

impl Provenance {
    /// Whether this source *declared* the fact rather than implying it.
    ///
    /// Used when two sources disagree: a declaration wins, and the loser is
    /// still recoverable because the winner records itself.
    pub fn is_stated(self) -> bool {
        matches!(self, Self::Rp9Manifest | Self::WhdloadSlave)
    }
}

/// A value and the source that gave it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fact<T> {
    pub value: T,
    pub from: Provenance,
}

impl<T> Fact<T> {
    pub fn new(value: T, from: Provenance) -> Self {
        Self { value, from }
    }
}

/// What a packager said this is — **declared only**.
///
/// `core/layout` says there is no `Demo` and there will not be one, because
/// nothing derivable from the bytes separates one from a game. That holds. This
/// is not derived: `.rp9` carries `<type>demo</type>`, written by the packager.
/// §14/§34 forbid acting on an uncertain classification as fact; they do not
/// forbid recording a statement as a statement.
///
/// The variants are the values **measured** across the 242 real `.rp9`
/// packages on this machine (2026-08-17): 111 `demo`, 96 `game`, 15 `system`,
/// 10 `gallery`, 10 `video`. The first cut of this enum had only `Game` and
/// `Demo`, which folded thirty-five stated types into "said nothing" — exactly
/// the collapse `Fact` exists to prevent, arriving from the other direction.
/// [`TitleKind::Other`] is there so the next unfamiliar word is recorded rather
/// than discarded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TitleKind {
    Game,
    Demo,
    /// An operating system or system software package.
    System,
    /// A picture collection.
    Gallery,
    Video,
    /// A word this ART does not know, kept verbatim.
    Other(String),
}

impl TitleKind {
    /// Whether this is something a game launcher should list.
    ///
    /// A `gallery` or a `video` on a GAMES: volume is not a game, and iGame
    /// showing one is a menu entry that does nothing useful.
    pub fn is_playable(&self) -> bool {
        matches!(self, Self::Game | Self::Demo)
    }
}

/// What chipset a title needs.
///
/// **Moved here from `core/collection.rs`, not redefined.** That module already
/// had this exact enum with these exact two variants; a second one would be a
/// type to keep in step, and the one that drifts is the one nobody is looking
/// at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChipsetRequirement {
    /// OCS / ECS (Amiga 500 / 600 / 2000)
    OcsEcs,
    /// AGA (Amiga 1200 / 4000 / CD32)
    Aga,
}

impl ChipsetRequirement {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::OcsEcs => "OCS / ECS",
            Self::Aga => "AGA",
        }
    }
}

/// A Kickstart a title asks for.
///
/// Every field is optional and they arrive from different places: a WHDLoad
/// slave at `ws_Version >= 16` gives `image`/`size`/`crc16`, while an `.rp9`
/// gives `<systemrom>310</systemrom>` and nothing else. A slave below v16 has
/// no such field at all, and one at v16 may carry offset 0 meaning "none" —
/// three different situations that must not collapse into one.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct KickstartNeed {
    /// `kick34005.A500` — what WHDLoad looks for in `DEVS:Kickstarts/`.
    pub image: Option<String>,
    pub size: Option<u32>,
    pub crc16: Option<u16>,
    /// `.rp9`'s `<systemrom>`, e.g. `310`.
    pub rom_version: Option<u16>,
}

impl KickstartNeed {
    /// Whether anything was actually asked for.
    pub fn is_empty(&self) -> bool {
        self.image.is_none() && self.rom_version.is_none()
    }
}

/// What the title physically is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Media {
    /// Floppy images in the order they are asked for. `.rp9` states the order
    /// through `<floppy priority="n">`; a TOSEC set infers it from the names.
    Floppies { ordered: Vec<String> },
    /// A hardfile that *is* the title — Enzo's collection, one game per image.
    Hardfile { file: String },
    /// A WHDLoad drawer, named by the slave inside it.
    WhdloadDrawer { slave: String },
}

/// Which file this record was read from.
///
/// **The name, never the path.** A record travels with a card, and where
/// somebody keeps their downloads is not part of what the card is —
/// `SourceFacts` in `core/card/manifest.rs` makes the same choice for the same
/// reason. The catalogue keeps the path beside the record, not inside it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRef {
    pub name: String,
    pub sha256: String,
    pub bytes: u64,
}

/// One title, as the sources describe it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameRecord {
    pub schema: u32,
    pub id: String,
    pub title: Fact<String>,
    pub kind: Option<Fact<TitleKind>>,
    pub year: Option<Fact<u16>>,
    pub publisher: Option<Fact<String>>,
    pub genre: Option<Fact<String>>,
    pub rating: Option<Fact<u8>>,
    pub chipset: Option<Fact<ChipsetRequirement>>,
    /// Absent when nothing asked for a Kickstart at all.
    pub kickstart: Option<Fact<KickstartNeed>>,
    pub media: Media,
    /// Relative to the source package, when one carried a picture.
    pub preview: Option<String>,
    pub source: SourceRef,
}

/// A stable identity: what the title is called, plus what its bytes are.
///
/// Path-independent on purpose. `core/collection.rs` used `md5(path)`, so
/// moving a file changed its identity; a catalogue that renumbers itself when
/// a folder is renamed cannot be joined to anything written earlier.
///
/// The eight hex characters are enough to keep two dumps of one game apart
/// while staying short enough to read in a listing.
pub fn derive_id(title: &str, primary_sha256: &str) -> String {
    let mut slug = String::new();
    let mut pending_separator = false;
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_separator && !slug.is_empty() {
                slug.push('-');
            }
            pending_separator = false;
            slug.push(ch.to_ascii_lowercase());
        } else {
            pending_separator = true;
        }
    }
    if slug.is_empty() {
        slug.push_str("untitled");
    }

    // A short hash of the hash: `primary_sha256` is already hex, but taking its
    // first eight characters would make two records that share a prefix collide
    // for a reason nobody could see. Hashing it again spreads them.
    let short = &sha256_bytes(primary_sha256.as_bytes())[..8];
    format!("{slug}-{short}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The id is derived from what the title *is*, never from where the file
    /// sits.
    #[test]
    fn the_id_does_not_depend_on_a_path() {
        let a = derive_id("Lotus 3", "abc123def456");
        let b = derive_id("Lotus 3", "abc123def456");
        assert_eq!(a, b);
        assert!(a.starts_with("lotus-3-"), "{a}");
    }

    /// Two dumps of the same game are two records. That is deliberate: they
    /// are different bytes, and a launcher pointing at the wrong one is a game
    /// that does not start.
    #[test]
    fn different_bytes_are_different_records() {
        assert_ne!(
            derive_id("Lotus 3", "abc123def456"),
            derive_id("Lotus 3", "999888777666")
        );
    }

    /// A title AmigaDOS or a filesystem would choke on still yields a usable
    /// slug — punctuation out, spaces to hyphens, case folded, no run of
    /// separators, and never a leading or trailing hyphen.
    #[test]
    fn a_slug_survives_a_hostile_title() {
        let id = derive_id(
            "  A-Train & Construction Set (v1.4)!!  ",
            "0011223344556677",
        );
        assert!(id.starts_with("a-train-construction-set-v1-4-"), "{id}");
        assert!(!id.contains("--"), "{id}");
    }

    /// A title with nothing sluggable must still produce an id rather than a
    /// bare hyphen or an empty string.
    #[test]
    fn a_title_with_no_usable_characters_still_gets_an_id() {
        let id = derive_id("!!!", "0011223344556677");
        assert!(id.starts_with("untitled-"), "{id}");
    }

    /// The provenance travels with the value. This is the whole point of the
    /// wrapper: the same field arrives from two tiers and they will disagree.
    #[test]
    fn a_fact_carries_where_it_came_from() {
        let stated = Fact::new(ChipsetRequirement::Aga, Provenance::WhdloadSlave);
        let guessed = Fact::new(ChipsetRequirement::Aga, Provenance::TosecName);
        assert_eq!(stated.value, guessed.value);
        assert_ne!(stated.from, guessed.from);
        assert!(stated.from.is_stated());
        assert!(!guessed.from.is_stated());
    }
}
