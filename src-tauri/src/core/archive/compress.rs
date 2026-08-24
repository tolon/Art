//! Unix `compress(1)` — the `.Z` format, LZW — read only.
//!
//! ART-228. AmigaOS 3.2's install media ships most of its Locale content
//! compressed: **3 263 files** in a tree ART built, the whole help system
//! among them (215 of 215 in the English branch), plus the Turkish
//! ISO-8859-9 fonts and catalogs. The release's own Installer decompresses
//! them during the copy and drops the suffix —
//! `(copyfiles … (compression) (newname #target))`, read out of
//! `Install3.2.adf`'s own script — and no decompressor exists anywhere on the
//! 35-ADF set, because the Amiga `Installer` does it itself. So ART has to as
//! well, and it has to do it here rather than by launching anything.
//!
//! ## Written rather than depended on
//!
//! `.Z` is small and completely specified, and the alternative was a crate
//! whose licence and advisories would join `cargo deny`'s surface for about a
//! hundred lines of LZW. What matters more is that a decompressor is a thing
//! pointed at untrusted bytes, and every bound in here is explicit: the code
//! width is checked against the header's own maximum, the dictionary cannot
//! grow past it, a code that has not been defined is refused rather than
//! read, and the output is capped. `core/archive`'s other three readers are
//! third-party and sit behind one gate for exactly that reason; this one is
//! ART's and is written to the same rule.
//!
//! ## The format, as implemented
//!
//! - Three-byte header: `1f 9d`, then a flags byte. The low five bits are the
//!   maximum code width (9..=16); bit `0x80` is *block mode*, in which code
//!   256 means "reset the dictionary".
//! - Codes follow, packed **least-significant-bit first**, starting at nine
//!   bits and widening by one each time the dictionary fills.
//! - In block mode the encoder pads to a whole group of eight codes after a
//!   clear, so the reader must skip the same padding.
//!
//! ## The one path with no test, said out loud
//!
//! **The dictionary reset is implemented from the specification and is
//! exercised by nothing.** A `compress` encoder emits a clear when the
//! dictionary fills, which at the 16-bit maximum these files declare means
//! 65 536 entries; the largest `.Z` on the owner's AmigaOS media is a few
//! kilobytes and never gets near it, so no real file reaches that branch. A
//! fixture was attempted — a hand-written encoder emitting a clear mid-stream
//! — and **7-Zip refused to agree with it**, which means the encoder's
//! padding was wrong, not that the decoder's is right. Tuning the encoder
//! against 7-Zip and then testing this decoder against the tuned encoder
//! would be a circle wearing an oracle's clothes, so it was not done.
//!
//! What that leaves: the reset branch is unverified, and getting it wrong
//! produces plausible-looking bytes rather than an error. It is written here
//! rather than left out because a stream that resets and is silently
//! truncated would be worse. If a file ever reaches it, this paragraph is
//! where to start.

use crate::core::error::{CoreError, CoreResult};

/// `1f 9d`, the two bytes every `.Z` file begins with.
pub const MAGIC: [u8; 2] = [0x1f, 0x9d];

const MIN_BITS: u32 = 9;
const MAX_BITS_LIMIT: u32 = 16;
const CLEAR_CODE: u16 = 256;
const FIRST_FREE: u16 = 257;

/// The largest thing ART will decompress out of one entry.
///
/// A `.Z` file is a stream, so the size it expands to is not knowable until
/// it has expanded — which is the shape that lets a small hostile file ask
/// for an unbounded allocation. AmigaOS's own compressed content is measured
/// in kilobytes; 64 MB is far above anything a locale drawer holds and far
/// below anything that hurts.
pub const MAX_OUTPUT: usize = 64 * 1024 * 1024;

/// Whether a name is one this module should be asked about.
///
/// Case-sensitive on purpose. The media spells it `.Z`, and a `.z` would be
/// gzip's own lower-case spelling for something else entirely — guessing
/// between them is how a reader ends up confidently wrong about a file.
pub fn is_compressed_name(name: &str) -> bool {
    name.ends_with(".Z") && name.len() > 2
}

/// `name` without its `.Z`, or `None` when it does not have one.
pub fn name_without_suffix(name: &str) -> Option<&str> {
    is_compressed_name(name).then(|| &name[..name.len() - 2])
}

/// Whether these bytes begin with the `.Z` magic.
pub fn looks_compressed(bytes: &[u8]) -> bool {
    bytes.len() >= 3 && bytes[0] == MAGIC[0] && bytes[1] == MAGIC[1]
}

fn malformed(detail: impl Into<String>) -> CoreError {
    CoreError::Malformed {
        format: "compress".into(),
        detail: detail.into(),
    }
}

/// Decompress one `.Z` stream.
pub fn decompress(input: &[u8]) -> CoreResult<Vec<u8>> {
    if input.len() < 3 {
        return Err(malformed(format!(
            "a compressed file needs at least a three-byte header; got {} byte(s)",
            input.len()
        )));
    }
    if input[0] != MAGIC[0] || input[1] != MAGIC[1] {
        return Err(malformed(format!(
            "expected the magic 1f 9d, found {:02x} {:02x}",
            input[0], input[1]
        )));
    }

    let flags = input[2];
    let max_bits = u32::from(flags & 0x1f);
    let block_mode = flags & 0x80 != 0;
    if !(MIN_BITS..=MAX_BITS_LIMIT).contains(&max_bits) {
        return Err(malformed(format!(
            "the header asks for {max_bits}-bit codes; compress uses {MIN_BITS} to \
             {MAX_BITS_LIMIT}"
        )));
    }

    // The dictionary. `prefix`/`suffix` are the classic pair: entry `c` is
    // entry `prefix[c]` followed by the byte `suffix[c]`. Capacity is the
    // header's own maximum, so it cannot grow past what the file declared.
    let capacity = 1usize << max_bits;
    let mut prefix = vec![0u16; capacity];
    let mut suffix = vec![0u8; capacity];
    let mut next_free = if block_mode { FIRST_FREE } else { CLEAR_CODE };
    let mut code_bits = MIN_BITS;

    let mut out: Vec<u8> = Vec::new();
    // Reused across codes so a long chain does not allocate per byte.
    let mut stack: Vec<u8> = Vec::with_capacity(capacity);

    let body = &input[3..];
    let mut bit_pos: usize = 0;
    let total_bits = body.len() * 8;
    let mut previous: Option<u16> = None;
    // Codes read since the last reset, to find the encoder's padding.
    let mut since_reset: usize = 0;

    loop {
        if bit_pos + code_bits as usize > total_bits {
            break;
        }
        let code = read_code(body, bit_pos, code_bits);
        bit_pos += code_bits as usize;
        since_reset += 1;

        if block_mode && code == CLEAR_CODE {
            // The encoder pads the stream so that the codes between resets
            // fill a whole group of eight. Skipping the same padding is not
            // optional: without it every following code is misaligned, and
            // the result is bytes rather than an error.
            let group = (code_bits as usize) * 8;
            let consumed = since_reset * code_bits as usize;
            let padding = (group - (consumed % group)) % group;
            bit_pos += padding;
            next_free = FIRST_FREE;
            code_bits = MIN_BITS;
            since_reset = 0;
            previous = None;
            continue;
        }

        let mut current = code;
        stack.clear();

        // The one self-referential case LZW allows: a code defined by the
        // sequence it is about to emit.
        let deferred = if usize::from(code) >= usize::from(next_free) {
            let Some(prev) = previous else {
                return Err(malformed(format!(
                    "code {code} is used before anything defines it"
                )));
            };
            if usize::from(code) > usize::from(next_free) {
                return Err(malformed(format!(
                    "code {code} is beyond the next free entry {next_free}"
                )));
            }
            current = prev;
            Some(first_byte(&prefix, &suffix, prev))
        } else {
            None
        };

        while current >= CLEAR_CODE {
            let index = usize::from(current);
            if index >= capacity {
                return Err(malformed(format!(
                    "code {current} is outside the dictionary"
                )));
            }
            stack.push(suffix[index]);
            let next = prefix[index];
            if next == current {
                return Err(malformed(format!("code {current} refers to itself")));
            }
            current = next;
            if stack.len() > capacity {
                return Err(malformed("a dictionary chain longer than the dictionary"));
            }
        }
        stack.push(current as u8);

        if out.len() + stack.len() + 1 > MAX_OUTPUT {
            return Err(malformed(format!(
                "the stream expands past ART's {MAX_OUTPUT}-byte limit for one file"
            )));
        }
        out.extend(stack.iter().rev());
        if let Some(byte) = deferred {
            out.push(byte);
        }

        if let Some(prev) = previous {
            if usize::from(next_free) < capacity {
                let first = if let Some(byte) = deferred {
                    byte
                } else {
                    first_byte(&prefix, &suffix, code)
                };
                prefix[usize::from(next_free)] = prev;
                suffix[usize::from(next_free)] = first;
                next_free += 1;
                if usize::from(next_free) == (1usize << code_bits) && code_bits < max_bits {
                    code_bits += 1;
                }
            }
        }
        previous = Some(code);
    }

    Ok(out)
}

/// The first byte the sequence for `code` produces.
fn first_byte(prefix: &[u16], suffix: &[u8], code: u16) -> u8 {
    let mut current = code;
    let mut guard = 0usize;
    while current >= CLEAR_CODE {
        let index = usize::from(current);
        if index >= prefix.len() {
            return 0;
        }
        current = prefix[index];
        guard += 1;
        if guard > prefix.len() {
            return 0;
        }
    }
    let _ = suffix;
    current as u8
}

/// One code, least-significant-bit first.
fn read_code(body: &[u8], bit_pos: usize, width: u32) -> u16 {
    let mut value: u32 = 0;
    for i in 0..width {
        let bit = bit_pos + i as usize;
        let byte = body[bit / 8];
        if byte >> (bit % 8) & 1 == 1 {
            value |= 1 << i;
        }
    }
    value as u16
}

#[cfg(test)]
mod oracle_hook {
    //! ART's own `.Z` reader against 7-Zip's, on the owner's real material.
    //!
    //! `#[ignore]`d and env-gated like every hook of this shape: it needs a
    //! file off the owner's own AmigaOS media and a 7-Zip that is not in CI.
    //! Point `ART_Z_IN` at a `.Z` and `ART_Z_ORACLE` at what 7-Zip produced
    //! from it.
    use super::*;

    #[test]
    #[ignore = "needs a real .Z and 7-Zip's answer to it; set ART_Z_IN and ART_Z_ORACLE"]
    fn matches_7zip_on_a_real_file() {
        let (Ok(input), Ok(oracle)) = (std::env::var("ART_Z_IN"), std::env::var("ART_Z_ORACLE"))
        else {
            return;
        };
        let packed = std::fs::read(&input).unwrap();
        let expected = std::fs::read(&oracle).unwrap();
        let got = decompress(&packed).expect("ART must read what 7-Zip read");
        println!(
            "{} bytes in, {} out; 7-Zip said {}",
            packed.len(),
            got.len(),
            expected.len()
        );
        assert_eq!(got.len(), expected.len(), "length");
        assert_eq!(got, expected, "bytes");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fixtures made by a throwaway encoder and then **checked with 7-Zip**
    // before being written down: 7-Zip decompresses each of these to exactly
    // the plaintext the test asserts. So what follows is agreed on by a tool
    // that is neither ART nor the script that produced it, which is the only
    // reason a hand-made fixture is worth anything.

    const SHORT_Z: [u8; 38] = [
        0x1f, 0x9d, 0x90, 0x41, 0xa4, 0x50, 0x01, 0x21, 0xa7, 0x4c, 0x18, 0x32, 0x73, 0x40, 0xdc,
        0x41, 0x13, 0x86, 0x0e, 0x08, 0x3a, 0x68, 0xca, 0x10, 0x2c, 0xc3, 0xc6, 0xe0, 0x1c, 0x89,
        0x77, 0xe4, 0xbc, 0xa1, 0x53, 0xc6, 0x85, 0x02,
    ];

    const WIDE_Z: [u8; 758] = [
        0x1f, 0x9d, 0x90, 0x00, 0x02, 0x08, 0x18, 0x40, 0xa0, 0x80, 0x81, 0x03, 0x08, 0x12, 0x28,
        0x58, 0xc0, 0xa0, 0x81, 0x83, 0x07, 0x10, 0x22, 0x48, 0x98, 0x40, 0xa1, 0x82, 0x85, 0x0b,
        0x18, 0x32, 0x68, 0xd8, 0xc0, 0xa1, 0x83, 0x87, 0x0f, 0x20, 0x42, 0x88, 0x18, 0x41, 0xa2,
        0x84, 0x89, 0x13, 0x28, 0x52, 0xa8, 0x58, 0xc1, 0xa2, 0x85, 0x8b, 0x17, 0x30, 0x62, 0xc8,
        0x98, 0x41, 0xa3, 0x86, 0x8d, 0x1b, 0x38, 0x72, 0xe8, 0xd8, 0xc1, 0xa3, 0x87, 0x8f, 0x1f,
        0x40, 0x82, 0x08, 0x19, 0x42, 0xa4, 0x88, 0x91, 0x23, 0x48, 0x92, 0x28, 0x59, 0xc2, 0xa4,
        0x89, 0x93, 0x27, 0x50, 0xa2, 0x48, 0x99, 0x42, 0xa5, 0x8a, 0x95, 0x2b, 0x58, 0xb2, 0x68,
        0xd9, 0xc2, 0xa5, 0x8b, 0x97, 0x2f, 0x60, 0xc2, 0x88, 0x19, 0x43, 0xa6, 0x8c, 0x99, 0x33,
        0x68, 0xd2, 0xa8, 0x59, 0xc3, 0xa6, 0x8d, 0x9b, 0x37, 0x70, 0xe2, 0xc8, 0x99, 0x43, 0xa7,
        0x8e, 0x9d, 0x3b, 0x78, 0xf2, 0xe8, 0xd9, 0xc3, 0xa7, 0x8f, 0x9f, 0x3f, 0x80, 0x02, 0x09,
        0x1a, 0x44, 0xa8, 0x90, 0xa1, 0x43, 0x88, 0x12, 0x29, 0x5a, 0xc4, 0xa8, 0x91, 0xa3, 0x47,
        0x90, 0x22, 0x49, 0x9a, 0x44, 0xa9, 0x92, 0xa5, 0x4b, 0x98, 0x32, 0x69, 0xda, 0xc4, 0xa9,
        0x93, 0xa7, 0x4f, 0xa0, 0x42, 0x89, 0x1a, 0x45, 0xaa, 0x94, 0xa9, 0x53, 0xa8, 0x52, 0xa9,
        0x5a, 0xc5, 0xaa, 0x95, 0xab, 0x57, 0xb0, 0x62, 0xc9, 0x9a, 0x45, 0xab, 0x96, 0xad, 0x5b,
        0xb8, 0x72, 0xe9, 0xda, 0xc5, 0xab, 0x97, 0xaf, 0x5f, 0xc0, 0x82, 0x09, 0x1b, 0x46, 0xac,
        0x98, 0xb1, 0x63, 0xc8, 0x92, 0x29, 0x5b, 0xc6, 0xac, 0x99, 0xb3, 0x67, 0xd0, 0xa2, 0x49,
        0x9b, 0x46, 0xad, 0x9a, 0xb5, 0x6b, 0xd8, 0xb2, 0x69, 0xdb, 0xc6, 0xad, 0x9b, 0xb7, 0x6f,
        0xe0, 0xc2, 0x89, 0x1b, 0x47, 0xae, 0x9c, 0xb9, 0x73, 0xe8, 0xd2, 0xa9, 0x5b, 0xc7, 0xae,
        0x9d, 0xbb, 0x77, 0xf0, 0xe2, 0xc9, 0x9b, 0x47, 0xaf, 0x9e, 0xbd, 0x7b, 0xf8, 0xf2, 0xe9,
        0xdb, 0xc7, 0xaf, 0x9f, 0xbf, 0x7f, 0x01, 0x0d, 0x54, 0xd0, 0x41, 0x09, 0x2d, 0xd4, 0xd0,
        0x43, 0x11, 0x4d, 0x54, 0xd1, 0x45, 0x19, 0x6d, 0xd4, 0xd1, 0x47, 0x21, 0x8d, 0x54, 0xd2,
        0x49, 0x29, 0xad, 0xd4, 0xd2, 0x4b, 0x31, 0xcd, 0x54, 0xd3, 0x4d, 0x39, 0xed, 0xd4, 0xd3,
        0x4f, 0x41, 0x0d, 0x55, 0xd4, 0x51, 0x49, 0x2d, 0xd5, 0xd4, 0x53, 0x51, 0x4d, 0x55, 0xd5,
        0x55, 0x59, 0x6d, 0xd5, 0xd5, 0x57, 0x61, 0x8d, 0x55, 0xd6, 0x59, 0x69, 0xad, 0xd5, 0xd6,
        0x5b, 0x71, 0xcd, 0x55, 0xd7, 0x5d, 0x79, 0xed, 0xd5, 0xd7, 0x5f, 0x81, 0x0d, 0x56, 0xd8,
        0x61, 0x89, 0x2d, 0xd6, 0xd8, 0x63, 0x91, 0x4d, 0x56, 0xd9, 0x65, 0x99, 0x6d, 0xd6, 0xd9,
        0x67, 0xa1, 0x8d, 0x56, 0xda, 0x69, 0xa9, 0xad, 0xd6, 0xda, 0x6b, 0xb1, 0xcd, 0x56, 0xdb,
        0x6d, 0xb9, 0xed, 0xd6, 0xdb, 0x6f, 0xc1, 0x0d, 0x57, 0xdc, 0x71, 0xc9, 0x2d, 0xd7, 0xdc,
        0x73, 0xd1, 0x4d, 0x57, 0xdd, 0x75, 0xd9, 0x6d, 0xd7, 0xdd, 0x77, 0xe1, 0x8d, 0x57, 0xde,
        0x79, 0xe9, 0xad, 0xd7, 0xde, 0x7b, 0xf1, 0xcd, 0x57, 0xdf, 0x7d, 0xf9, 0xed, 0xd7, 0xdf,
        0x7f, 0x01, 0x12, 0x64, 0x10, 0x42, 0x0a, 0x31, 0xe4, 0x10, 0x44, 0x12, 0x51, 0x64, 0x11,
        0x46, 0x1a, 0x71, 0xe4, 0x11, 0x48, 0x22, 0x91, 0x64, 0x12, 0x4a, 0x2a, 0xb1, 0xe4, 0x12,
        0x4c, 0x32, 0xd1, 0x64, 0x13, 0x4e, 0x3a, 0xf1, 0xe4, 0x13, 0x50, 0x42, 0x11, 0x65, 0x14,
        0x52, 0x4a, 0x31, 0xe5, 0x14, 0x54, 0x52, 0x51, 0x65, 0x15, 0x56, 0x5a, 0x71, 0xe5, 0x15,
        0x58, 0x62, 0x91, 0x65, 0x16, 0x5a, 0x6a, 0xb1, 0xe5, 0x16, 0x5c, 0x72, 0xd1, 0x65, 0x17,
        0x5e, 0x7a, 0xf1, 0xe5, 0x17, 0x60, 0x82, 0x11, 0x66, 0x18, 0x62, 0x8a, 0x31, 0xe6, 0x18,
        0x64, 0x92, 0x51, 0x66, 0x19, 0x66, 0x9a, 0x71, 0xe6, 0x19, 0x68, 0xa2, 0x91, 0x66, 0x1a,
        0x6a, 0xaa, 0xb1, 0xe6, 0x1a, 0x6c, 0xb2, 0xd1, 0x66, 0x1b, 0x6e, 0xba, 0xf1, 0xe6, 0x1b,
        0x70, 0xc2, 0x11, 0x67, 0x1c, 0x72, 0xca, 0x31, 0xe7, 0x1c, 0x74, 0xd2, 0x51, 0x67, 0x1d,
        0x76, 0xda, 0x71, 0xe7, 0x1d, 0x78, 0xe2, 0x91, 0x67, 0x1e, 0x7a, 0xea, 0xb1, 0xe7, 0x1e,
        0x7c, 0xf2, 0xd1, 0x67, 0x1f, 0x7e, 0xfa, 0xf1, 0xe7, 0xdf, 0x3f, 0x74, 0xa0, 0x51, 0x06,
        0x08, 0x71, 0xd4, 0x91, 0xc6, 0x18, 0x6b, 0x80, 0x20, 0x86, 0x1c, 0x6f, 0xdc, 0xe1, 0x06,
        0x08, 0x66, 0xbc, 0x81, 0x07, 0x08, 0x01, 0x0f, 0x5c, 0xf0, 0xc1, 0x09, 0x2f, 0xdc, 0xf0,
        0xc3, 0x11, 0x4f, 0x5c, 0x31, 0xc1, 0x06, 0x23, 0xac, 0x30, 0xc3, 0x0e, 0x43, 0x2c, 0x31,
        0xc5, 0x02, 0x83, 0x8c, 0xf1, 0xc8, 0x1b, 0x9b, 0xec, 0x71, 0xca, 0x17, 0x8b, 0xac, 0x71,
        0xc9, 0x1d, 0xa3, 0x6c, 0x71, 0xc8, 0x19, 0x93, 0xcc, 0xf1, 0xc9, 0x1f, 0xc7, 0x9c, 0x73,
        0xcb, 0x35, 0xf7, 0x8c, 0x33, 0xcb, 0x34, 0xf3, 0x0c, 0xf3, 0xd0, 0x33, 0xef, 0xfc, 0xf2,
        0xcd, 0x2b, 0x27, 0xed, 0xb2, 0xcd, 0x2a, 0xcb, 0xac, 0xf3, 0xd3, 0x42, 0x37, 0x3d, 0x75,
        0xd0, 0x47, 0x5b, 0x0d, 0xb4, 0xd1, 0x4c, 0x4b, 0xbd, 0xf5, 0xd2, 0x51, 0xff, 0x5c, 0x34,
        0xd8, 0x3e, 0x13, 0xad, 0x34, 0xd4, 0x65, 0x3b, 0x8d, 0x75, 0xd7, 0x62, 0x9f, 0x5d, 0xb5,
        0xd7, 0x63, 0xa3, 0x8d, 0xf4, 0xd5, 0x27, 0x03,
    ];

    // wide.plain: 1168 bytes = bytes(0..=255) three times, then
    //   b"the quick brown fox " twenty times.

    /// `bytes(0..=255)` three times, then a phrase twenty times — long enough
    /// that the code width grows past nine bits twice, which is where a
    /// reader that never widens still produces output and gets it wrong.
    fn wide_plain() -> Vec<u8> {
        let mut v: Vec<u8> = Vec::new();
        for _ in 0..3 {
            v.extend(0u8..=255);
        }
        for _ in 0..20 {
            v.extend_from_slice(b"the quick brown fox ");
        }
        v
    }

    #[test]
    fn a_short_stream_decompresses_to_its_plaintext() {
        assert_eq!(
            decompress(&SHORT_Z).unwrap(),
            b"ART reads what the release wrote.\n"
        );
    }

    #[test]
    fn a_stream_that_widens_past_nine_bits_decompresses_whole() {
        let got = decompress(&WIDE_Z).unwrap();
        let want = wide_plain();
        assert_eq!(got.len(), want.len(), "length");
        assert_eq!(got, want, "bytes");
    }

    #[test]
    fn the_header_is_checked_rather_than_assumed() {
        assert!(decompress(b"").is_err());
        assert!(decompress(b"\x1f").is_err());
        // Right length, wrong magic.
        let err = decompress(b"\x1f\x8b\x90").unwrap_err().to_string();
        assert!(err.contains("1f 9d"), "{err}");
    }

    /// A width outside 9..=16 is a refusal and not a clamp: clamping would
    /// read the file with a dictionary the encoder never used, and produce
    /// bytes.
    #[test]
    fn a_code_width_compress_never_uses_is_refused() {
        for bad in [0u8, 8, 17, 31] {
            let err = decompress(&[0x1f, 0x9d, bad]).unwrap_err().to_string();
            assert!(err.contains("9 to 16"), "width {bad}: {err}");
        }
    }

    /// The shape a truncated or hostile stream takes: a code the dictionary
    /// has not defined. Refused by name rather than read past the end of the
    /// table.
    #[test]
    fn a_code_nothing_has_defined_is_refused() {
        // 9-bit codes, LSB first: 0x1ff is far beyond the first free entry.
        let body = [0xff, 0x01];
        let err = decompress(&[0x1f, 0x9d, 0x90, body[0], body[1]])
            .unwrap_err()
            .to_string();
        assert!(err.contains("before anything defines it"), "{err}");
    }

    #[test]
    fn the_suffix_rules_are_the_ones_the_media_uses() {
        assert!(is_compressed_name("dos.catalog.Z"));
        assert!(!is_compressed_name(".Z"), "a bare suffix is not a name");
        // Lower case is gzip's, not compress's — guessing between them is how
        // a reader ends up confidently wrong about a file.
        assert!(!is_compressed_name("dos.catalog.z"));
        assert_eq!(name_without_suffix("dos.catalog.Z"), Some("dos.catalog"));
        assert_eq!(name_without_suffix("dos.catalog"), None);
    }

    #[test]
    fn the_magic_is_recognised_without_decompressing() {
        assert!(looks_compressed(&SHORT_Z));
        assert!(!looks_compressed(b"not compressed at all"));
        assert!(!looks_compressed(&[0x1f]), "two bytes are not a header");
    }
}
