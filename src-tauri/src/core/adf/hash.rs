//! AmigaDOS filename hash function (directory hash-table bucket selection).
//!
//! Mirrors adflib's `adfGetHashValue`:
//!
//! ```c
//! l = hash = strlen(name);
//! for (i = 0; i < l; i++) {
//!     hash = hash * 13;
//!     hash += intl ? intlToUpper(name[i]) : toUpper(name[i]);
//!     hash &= 0x7ff;          // <-- applied on every iteration
//! }
//! hash = hash % HT_SIZE;
//! ```
//!
//! The `& 0x7ff` after *each* character is not an optimisation — it is part of
//! the algorithm. Dropping it yields different buckets for names longer than
//! about three characters, which produces images that ART can read back
//! (it hashes consistently) but that AmigaDOS, WinUAE and every other Amiga
//! tool cannot: the files land in buckets nothing else looks in.
//!
//! `international` comes from the volume's bootblock (`DOS\2`/`DOS\3`), because
//! INTL volumes fold the 224–254 range to upper case as well.

use super::blocks::HASH_TABLE_SIZE;

/// Mask applied after every character, per the AmigaDOS algorithm.
const HASH_MASK: u32 = 0x7ff;

/// Upper-case folding for non-international volumes: plain ASCII only.
fn to_upper(c: u8) -> u8 {
    if c.is_ascii_lowercase() {
        c - (b'a' - b'A')
    } else {
        c
    }
}

/// Upper-case folding for international volumes: ASCII plus the Latin-1
/// accented range 224–254, excluding 247 (÷, which has no upper-case form).
fn intl_to_upper(c: u8) -> u8 {
    if c.is_ascii_lowercase() || ((224..=254).contains(&c) && c != 247) {
        c - (b'a' - b'A')
    } else {
        c
    }
}

/// Compute the hash-table bucket (0..72) for a filename.
///
/// `international` must reflect the volume's own flag — hashing a name with the
/// wrong folding rules puts it in a bucket the volume will never search.
pub fn name_hash(name: &str, international: bool) -> u32 {
    let bytes = name.as_bytes();
    let mut hash: u32 = bytes.len() as u32;

    for &b in bytes {
        let up = if international {
            intl_to_upper(b)
        } else {
            to_upper(b)
        };
        hash = hash.wrapping_mul(13).wrapping_add(up as u32) & HASH_MASK;
    }

    hash % HASH_TABLE_SIZE as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_in_table_range() {
        for name in &["a", "Game", "S", "DEVS", "system-configuration", "x"] {
            for intl in [false, true] {
                let h = name_hash(name, intl);
                assert!(h < HASH_TABLE_SIZE as u32, "{name} (intl={intl}) → {h}");
            }
        }
    }

    #[test]
    fn empty_name_hashes_to_zero() {
        assert_eq!(name_hash("", false), 0);
        assert_eq!(name_hash("", true), 0);
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(name_hash("Game", false), name_hash("GAME", false));
        assert_eq!(name_hash("Game", false), name_hash("game", false));
    }

    /// Values produced by the reference algorithm (adflib `adfGetHashValue`).
    /// These pin the `& 0x7ff` masking: without it "Game" hashes to 46 instead
    /// of 54 and the image becomes unreadable on a real Amiga.
    #[test]
    fn matches_the_amigados_reference_values() {
        assert_eq!(name_hash("S", false), 24);
        assert_eq!(name_hash("Game", false), 54);
        assert_eq!(name_hash("DEVS", false), 22);
        assert_eq!(name_hash("Startup-Sequence", false), 49);
        assert_eq!(name_hash("a", false), 6);
    }

    #[test]
    fn the_mask_is_applied_every_iteration() {
        // Recompute "Game" the naive way (mask only at the end) and confirm the
        // two disagree — a regression guard against silently dropping the mask.
        let naive = {
            let mut h: u32 = 4;
            for b in "Game".bytes() {
                h = h
                    .wrapping_mul(13)
                    .wrapping_add(b.to_ascii_uppercase() as u32);
            }
            h % HASH_TABLE_SIZE as u32
        };
        assert_ne!(
            naive,
            name_hash("Game", false),
            "masking per iteration must change the result"
        );
    }

    #[test]
    fn international_folding_differs_for_accented_names() {
        // 0xE9 is 'é' in Latin-1: folded on INTL volumes, left alone otherwise.
        let name = unsafe { String::from_utf8_unchecked(vec![b'C', b'a', b'f', 0xE9]) };
        assert_ne!(
            name_hash(&name, false),
            name_hash(&name, true),
            "INTL volumes must fold the accented range"
        );
    }

    #[test]
    fn plain_ascii_names_hash_the_same_either_way() {
        for name in &["Game", "DEVS", "Startup-Sequence"] {
            assert_eq!(name_hash(name, false), name_hash(name, true));
        }
    }
}
