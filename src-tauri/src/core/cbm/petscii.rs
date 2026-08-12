//! PETSCII, which is not ASCII.
//!
//! Commodore disks store names in PETSCII, padded to their field width with
//! `0xA0` — a padding byte, not a character. Two things go wrong if this is
//! treated as ASCII or as Latin-1: names come back with trailing garbage, and
//! every letter above `0x40` lands in the wrong case or in the graphics set.
//!
//! ART decodes the **unshifted (upper case / graphics) set**, which is what a
//! drive writes into a directory: `0x41`–`0x5A` are `A`–`Z` there, and
//! `0xC1`–`0xDA` are the same letters again. The lower-case set is a display
//! mode of the machine, not a property of the bytes, so a name is rendered the
//! way the directory listing on a real C64 shows it.
//!
//! Nothing here can fail: a byte with no sensible Unicode counterpart becomes
//! `·`, because a name that is partly unreadable is still a name the user can
//! recognise, and refusing to list a file because of one byte would be worse.

/// The padding byte Commodore fields are filled with.
pub const PAD: u8 = 0xA0;

/// Decode a PETSCII field, dropping the `0xA0` padding at its end.
///
/// Padding is only stripped from the *end*: `0xA0` inside a name is a
/// character the name really has, and silently removing it would merge two
/// different files into one name.
pub fn decode_field(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .rposition(|b| *b != PAD)
        .map_or(0, |last| last + 1);
    decode(&bytes[..end])
}

/// Decode PETSCII bytes as they are, padding included.
pub fn decode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| decode_byte(*b)).collect()
}

fn decode_byte(b: u8) -> char {
    match b {
        // Space through `?`: the same as ASCII, digits and punctuation
        // included.
        0x20..=0x3F => b as char,
        // `@`, then the letters. Unshifted PETSCII has upper case here.
        0x40 => '@',
        0x41..=0x5A => b as char,
        0x5B => '[',
        0x5C => '£',
        0x5D => ']',
        0x5E => '↑',
        0x5F => '←',
        // The same letters again, in the second half of the set.
        0xC1..=0xDA => (b - 0x80) as char,
        // Everything else is one visible placeholder: control codes, colour
        // codes, and the graphics set.
        //
        // The graphics characters are deliberately *not* mapped to their
        // Unicode look-alikes yet. Half the demo scene drew directory art out
        // of them, so it would be a nice thing to show — but each mapping is
        // a specific claim about a specific byte, ART has nothing to check
        // those claims against, and a name rendered with the wrong symbols
        // looks correct while being wrong. One honest placeholder beats
        // twenty guesses (§10, §89).
        _ => '·',
    }
}

/// Encode a string back to PETSCII, for comparing a name the user typed with
/// what is on the disk.
///
/// Deliberately narrow: letters, digits and the punctuation that survives a
/// round trip. Anything else becomes `_`, so a search never matches by
/// accident on a byte it could not represent.
pub fn encode(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| match c {
            'A'..='Z' => c as u8,
            'a'..='z' => c.to_ascii_uppercase() as u8,
            ' '..='?' => c as u8,
            _ => b'_',
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The names a real directory holds: upper case, and padded to sixteen
    /// bytes with `0xA0`.
    #[test]
    fn a_padded_name_comes_back_without_its_padding() {
        let mut field = *b"ELITE           ";
        for byte in field.iter_mut().skip(5) {
            *byte = PAD;
        }
        assert_eq!(decode_field(&field), "ELITE");
    }

    #[test]
    fn digits_and_punctuation_survive_unchanged() {
        assert_eq!(decode(b"GAME 2 (V1.3)"), "GAME 2 (V1.3)");
    }

    /// Both halves of the set are the same letters. A drive writes `0x41`;
    /// plenty of software writes `0xC1`.
    #[test]
    fn both_letter_ranges_decode_to_the_same_letters() {
        assert_eq!(decode(&[0x41, 0x42, 0x5A]), "ABZ");
        assert_eq!(decode(&[0xC1, 0xC2, 0xDA]), "ABZ");
    }

    /// `0xA0` inside a name is a character, not padding — stripping it would
    /// turn two different files into one name.
    #[test]
    fn padding_is_only_stripped_from_the_end() {
        let field = [b'A', PAD, b'B', PAD, PAD];
        assert_eq!(decode_field(&field), "A·B");
    }

    #[test]
    fn a_field_that_is_all_padding_is_an_empty_name() {
        assert_eq!(decode_field(&[PAD; 16]), "");
        assert_eq!(decode_field(&[]), "");
    }

    /// Nothing panics, whatever the byte — every one of them came off a disk
    /// image somebody else wrote.
    #[test]
    fn every_possible_byte_decodes_to_something() {
        for b in 0u8..=255 {
            let decoded = decode(&[b]);
            assert_eq!(decoded.chars().count(), 1, "byte {b:#04x}");
        }
    }

    #[test]
    fn encoding_is_case_folded_and_never_invents_a_byte() {
        assert_eq!(encode("elite"), b"ELITE".to_vec());
        assert_eq!(encode("GAME 1"), b"GAME 1".to_vec());
        assert_eq!(encode("ünicode"), b"_NICODE".to_vec());
    }
}
