//! The IFF `FORM`…`PREF` container every AmigaOS preferences file uses.
//!
//! Measured against real files rather than recalled: `WBPattern.prefs` from
//! ART's own built AmigaOS 3.2 tree and from 3.9 are byte-identical at 462
//! bytes, and both open `FORM` / size / `PREF` / `PRHD` / 6 / six zero bytes
//! before the first payload chunk (design doc §1.1).
//!
//! This module never rewrites a chunk it was not asked about and never
//! reorders one. That is not tidiness: a release's `WBPattern.prefs` carries
//! three `PTRN` chunks and a user who sets only the root backdrop must keep
//! the other two exactly as the release shipped them (§39/§40).

use crate::core::error::{CoreError, CoreResult};
use std::ops::Range;

/// Where one chunk's body lives inside the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkSpan {
    pub id: [u8; 4],
    pub body: Range<usize>,
}

/// A parsed preferences file. Holds the original bytes so anything this
/// module does not understand can be handed back untouched.
#[derive(Debug, Clone)]
pub struct PrefsFile {
    bytes: Vec<u8>,
    chunks: Vec<ChunkSpan>,
}

fn malformed(detail: &str) -> CoreError {
    CoreError::Malformed {
        format: "IFF PREF".to_string(),
        detail: detail.to_string(),
    }
}

fn be_u32(bytes: &[u8], at: usize) -> CoreResult<u32> {
    let end = at
        .checked_add(4)
        .ok_or_else(|| malformed("offset overflow"))?;
    let slice = bytes
        .get(at..end)
        .ok_or_else(|| malformed("a length field runs past the end of the file"))?;
    Ok(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

/// Parse a `FORM`…`PREF`, indexing every chunk including `PRHD`.
pub fn parse(bytes: &[u8]) -> CoreResult<PrefsFile> {
    if bytes.len() < 12 {
        return Err(malformed("shorter than an IFF FORM header"));
    }
    if &bytes[0..4] != b"FORM" {
        return Err(malformed("does not begin with FORM"));
    }
    let form_size = be_u32(bytes, 4)? as usize;
    let form_end = form_size
        .checked_add(8)
        .ok_or_else(|| malformed("FORM size overflows"))?;
    if form_end > bytes.len() {
        return Err(malformed("the FORM size is larger than the file"));
    }
    if &bytes[8..12] != b"PREF" {
        return Err(malformed("the FORM type is not PREF"));
    }

    let mut chunks = Vec::new();
    let mut at = 12usize;
    while at < form_end {
        let header_end = at
            .checked_add(8)
            .ok_or_else(|| malformed("chunk header overflows"))?;
        if header_end > form_end {
            return Err(malformed("a chunk header runs past the end of the FORM"));
        }
        let mut id = [0u8; 4];
        id.copy_from_slice(&bytes[at..at + 4]);
        let size = be_u32(bytes, at + 4)? as usize;
        let body_start = header_end;
        let body_end = body_start
            .checked_add(size)
            .ok_or_else(|| malformed("chunk size overflows"))?;
        if body_end > form_end {
            return Err(malformed(
                "a chunk claims more bytes than the FORM contains",
            ));
        }
        chunks.push(ChunkSpan {
            id,
            body: body_start..body_end,
        });
        // IFF pads an odd-sized body with one byte that the size field
        // does not count.
        at = body_end
            .checked_add(size & 1)
            .ok_or_else(|| malformed("chunk padding overflows"))?;
    }

    Ok(PrefsFile {
        bytes: bytes.to_vec(),
        chunks,
    })
}

impl PrefsFile {
    pub fn chunks(&self) -> &[ChunkSpan] {
        &self.chunks
    }

    /// The body of chunk `index`. Refuses rather than panics on an index
    /// this file never produced — a caller-computed index is not trusted
    /// input, and the release profile aborts the whole process on an
    /// out-of-range slice.
    pub fn body(&self, index: usize) -> CoreResult<&[u8]> {
        let span = self
            .chunks
            .get(index)
            .ok_or_else(|| malformed("chunk index is not one this file produced"))?;
        Ok(&self.bytes[span.body.clone()])
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    /// Rebuild the file with the named chunks' bodies replaced. Every other
    /// chunk — `PRHD`, an unrecognised one, a `PTRN` the caller did not
    /// name — is copied verbatim, in its original order.
    pub fn replace_bodies(&self, edits: &[(usize, Vec<u8>)]) -> CoreResult<Vec<u8>> {
        for (index, _) in edits {
            if *index >= self.chunks.len() {
                return Err(malformed("edit names a chunk this file does not have"));
            }
        }
        let mut content = Vec::new();
        content.extend_from_slice(b"PREF");
        for (index, span) in self.chunks.iter().enumerate() {
            let body: &[u8] = match edits.iter().find(|(i, _)| *i == index) {
                Some((_, replacement)) => replacement,
                None => &self.bytes[span.body.clone()],
            };
            let size = u32::try_from(body.len())
                .map_err(|_| malformed("a replacement body does not fit an IFF chunk"))?;
            content.extend_from_slice(&span.id);
            content.extend_from_slice(&size.to_be_bytes());
            content.extend_from_slice(body);
            if body.len() % 2 == 1 {
                content.push(0);
            }
        }
        let form_size = u32::try_from(content.len())
            .map_err(|_| malformed("the rebuilt FORM does not fit an IFF size field"))?;
        let mut out = Vec::with_capacity(content.len() + 8);
        out.extend_from_slice(b"FORM");
        out.extend_from_slice(&form_size.to_be_bytes());
        out.extend_from_slice(&content);
        Ok(out)
    }
}

#[cfg(test)]
pub(crate) mod tests_support {
    /// Build a `FORM`…`PREF` by hand: a 6-byte `PRHD` then the given chunks.
    /// Odd-sized chunk bodies get the IFF pad byte, which is *not* counted in
    /// the chunk's own size field — the trap this builder exists to reproduce.
    pub(crate) fn synthetic_prefs(chunks: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut content = Vec::new();
        content.extend_from_slice(b"PREF");
        content.extend_from_slice(b"PRHD");
        content.extend_from_slice(&6u32.to_be_bytes());
        content.extend_from_slice(&[0u8; 6]);
        for (id, body) in chunks {
            content.extend_from_slice(id);
            content.extend_from_slice(&(body.len() as u32).to_be_bytes());
            content.extend_from_slice(body);
            if body.len() % 2 == 1 {
                content.push(0);
            }
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"FORM");
        out.extend_from_slice(&(content.len() as u32).to_be_bytes());
        out.extend_from_slice(&content);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tests_support::synthetic_prefs;

    #[test]
    fn a_prefs_file_round_trips_byte_for_byte() {
        let bytes = synthetic_prefs(&[(*b"PTRN", vec![1, 2, 3, 4]), (*b"SCRM", vec![9; 28])]);
        let parsed = parse(&bytes).unwrap();
        assert_eq!(parsed.to_bytes(), bytes);
    }

    #[test]
    fn every_chunk_is_indexed_including_the_header() {
        let bytes = synthetic_prefs(&[(*b"PTRN", vec![7; 24])]);
        let parsed = parse(&bytes).unwrap();
        let ids: Vec<[u8; 4]> = parsed.chunks().iter().map(|c| c.id).collect();
        assert_eq!(ids, vec![*b"PRHD", *b"PTRN"]);
        assert_eq!(parsed.body(1).unwrap(), &[7u8; 24]);
    }

    #[test]
    fn replacing_one_body_leaves_every_other_chunk_untouched() {
        // `synthetic_prefs` prepends PRHD, so the indices are
        // PRHD=0, PTRN=1, XXXX=2, PTRN=3. Replace the *second* PTRN.
        let bytes = synthetic_prefs(&[
            (*b"PTRN", vec![1; 24]),
            (*b"XXXX", vec![0xAB; 10]),
            (*b"PTRN", vec![2; 24]),
        ]);
        let parsed = parse(&bytes).unwrap();
        let out = parsed.replace_bodies(&[(3, vec![5; 40])]).unwrap();
        let after = parse(&out).unwrap();
        assert_eq!(
            after.body(0).unwrap(),
            &[0u8; 6],
            "PRHD must survive verbatim"
        );
        assert_eq!(
            after.body(1).unwrap(),
            &[1u8; 24],
            "the first PTRN must survive verbatim"
        );
        assert_eq!(
            after.body(2).unwrap(),
            &[0xABu8; 10],
            "the unknown chunk must survive verbatim"
        );
        assert_eq!(after.body(3).unwrap(), &[5u8; 40]);
    }

    #[test]
    fn an_odd_sized_body_is_padded_but_the_size_field_is_not() {
        let bytes = synthetic_prefs(&[(*b"PTRN", vec![1; 24])]);
        let parsed = parse(&bytes).unwrap();
        let out = parsed.replace_bodies(&[(1, vec![3; 5])]).unwrap();
        let after = parse(&out).unwrap();
        assert_eq!(
            after.body(1).unwrap().len(),
            5,
            "the size field states the real length"
        );
        assert_eq!(out.len() % 2, 0, "the file itself stays word-aligned");
    }

    #[test]
    fn a_file_that_is_not_a_form_pref_is_refused() {
        assert!(parse(b"FORM\x00\x00\x00\x04ILBM").is_err());
        assert!(parse(b"NOPE\x00\x00\x00\x04PREF").is_err());
        assert!(parse(b"FORM").is_err());
    }

    #[test]
    fn a_chunk_running_past_the_end_is_refused_not_clamped() {
        let mut bytes = synthetic_prefs(&[(*b"PTRN", vec![1; 24])]);
        let last = bytes.len();
        // the PTRN size field is the four bytes before its 24-byte body
        bytes[last - 28..last - 24].copy_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
        let err = parse(&bytes).unwrap_err();
        assert!(
            format!("{err}").contains("chunk"),
            "the refusal must name the chunk, got: {err}"
        );
    }

    #[test]
    fn a_form_size_larger_than_the_file_is_refused() {
        let mut bytes = synthetic_prefs(&[(*b"PTRN", vec![1; 24])]);
        bytes[4..8].copy_from_slice(&0x7FFF_FFFFu32.to_be_bytes());
        assert!(parse(&bytes).is_err());
    }

    #[test]
    fn a_body_index_this_file_never_produced_is_refused_not_panicked() {
        // PRHD=0, PTRN=1 — index 2 does not exist.
        let bytes = synthetic_prefs(&[(*b"PTRN", vec![1; 24])]);
        let parsed = parse(&bytes).unwrap();
        let err = parsed.body(2).unwrap_err();
        assert!(
            format!("{err}").contains("chunk"),
            "the refusal must name the chunk, got: {err}"
        );
    }
}
