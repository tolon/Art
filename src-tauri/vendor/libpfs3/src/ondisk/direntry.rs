//! Directory block headers and directory entry parsing.
//!
//! Modified by ART on 2026-09-15 (ART-325): a directory entry's extra fields
//! are read and written in pfs3aio's layout — the flags word last, one bit
//! per 16-bit word of `struct extrafields`, the words before it — through
//! `ExtraFields::word_offsets`, `ExtraFields::read` and
//! `ExtraFields::encode`, which `DirEntry::parse` and the writer share.
//! 0.1.3 read the flags word first, one bit per field.
//! Modified by ART on 2026-09-15 (the scoped re-review's follow-ups 1 and 6,
//! ART-324 and ART-330): `entry_bounds` is the one rule for walking a
//! directory block, shared with the writer; `DirEntry::parse` refuses a
//! malformed entry rather than ending the walk there, and reads `fsizex` as
//! part of the size only on a largefile volume. `ART-PATCH.md` in this
//! crate's root says what and why.

use super::*;
use crate::error::{Error, Result};

/// Directory block header (0x14 bytes).
#[derive(Debug)]
pub struct DirBlockHeader {
    pub id: u16,
    pub datestamp: u32,
    pub anodenr: u32,
    pub parent: u32,
}

impl DirBlockHeader {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 0x14 {
            return Err(Error::TooShort("dir block header"));
        }
        let id = u16::from_be_bytes(data[0..2].try_into().unwrap());
        if id != DBLKID {
            return Err(Error::BadBlockId("dir block", DBLKID, id));
        }
        let datestamp = u32::from_be_bytes(data[4..8].try_into().unwrap());
        let anodenr = u32::from_be_bytes(data[0x0C..0x10].try_into().unwrap());
        let parent = u32::from_be_bytes(data[0x10..0x14].try_into().unwrap());
        Ok(Self {
            id,
            datestamp,
            anodenr,
            parent,
        })
    }
}

pub const DIR_BLOCK_HEADER_SIZE: usize = 0x14;

/// Variable-length directory entry.
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub entry_size: u8,
    pub entry_type: i8,
    pub anode: u32,
    pub fsize: u32,
    pub creation_day: u16,
    pub creation_minute: u16,
    pub creation_tick: u16,
    pub protection: u8,
    pub name: String,
    pub comment: String,
    pub extra: ExtraFields,
}

/// Extra fields appended after name+comment in a directory entry — pfs3aio's
/// `struct extrafields` (`blocks.h:342-353`, `tonioni/pfs3aio` `211f7f0`).
///
/// ART (ART-325): `prot` holds the entry's protection bits 8-31 with its
/// lower byte, the entry's own `protection`, OR-ed in, as `GetExtraFields`
/// returns it (`directory.c:3729-3730`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExtraFields {
    pub link: u32,
    pub uid: u16,
    pub gid: u16,
    pub prot: u32,
    pub virtualsize: u32,
    pub rollpointer: u32,
    pub fsizex: u16,
}

/// ART (ART-325): the number of 16-bit words in pfs3aio's
/// `struct extrafields`, and so the number of flag bits `GetExtraFields`
/// reads (`directory.c:3726`).
pub const EXTRA_FIELD_WORDS: usize = 11;

/// ART (ART-325): the word of `struct extrafields` that is `fsizex`, and its
/// flag bit.
pub const EXTRA_FSIZEX_WORD: usize = 10;

/// ART (ART-325): an entry whose flags word names more extra-field words
/// than lie between the start of its fields and the flags word itself.
/// pfs3aio would read the missing words out of the entry's comment or name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MalformedExtraFields {
    pub flags: u16,
}

/// ART (ART-325): where an entry's extra fields start, for a name of `nlen`
/// bytes and a comment of `clen`: `(sizeof(struct direntry) + nlength +
/// comment length) & 0xfffe`, `struct direntry` being 20 bytes
/// (`blocks.h:327-340`) — `AddExtraFields` (`directory.c:3773`) and
/// `RenameAndMove` (`:2114-2115`).
pub fn extra_fields_offset(nlen: usize, clen: usize) -> usize {
    (20 + nlen + clen) & !1
}

impl ExtraFields {
    /// ART (ART-325): the offset, inside `entry`, of each word of
    /// `struct extrafields` the entry carries, as pfs3aio's `GetExtraFields`
    /// finds them (`directory.c:3719-3731`): the flags word is the entry's
    /// last two bytes, bit `i` says word `i` is there, and each word that is
    /// there lies before the one found after it. `AddExtraFields` writes them
    /// that way (`:3764-3800`); hst-amiga reads them the same way
    /// (`DirEntryReader.ReadExtraFields`, `henrikstengaard/hst-amiga`
    /// `6b45584`).
    ///
    /// `entry` is one whole entry, its size byte long. An entry that ends
    /// before its fields' start plus a flags word has none — what pfs3aio
    /// writes without `MODE_DIR_EXTENSION` (`directory.c:3522-3524,2123-2124`).
    pub fn word_offsets(
        entry: &[u8],
    ) -> std::result::Result<[Option<usize>; EXTRA_FIELD_WORDS], MalformedExtraFields> {
        let mut at = [None; EXTRA_FIELD_WORDS];
        let Some(&nlen) = entry.get(17) else {
            return Ok(at);
        };
        let Some(&clen) = entry.get(18 + usize::from(nlen)) else {
            return Ok(at);
        };
        let fields = extra_fields_offset(usize::from(nlen), usize::from(clen));
        let end = entry.len();
        let Some(flags) = end
            .checked_sub(2)
            .filter(|&f| f >= fields)
            .and_then(|f| entry.get(f..end))
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
        else {
            return Ok(at);
        };
        let mut next = end - 2;
        for (i, slot) in at.iter_mut().enumerate() {
            if flags & (1 << i) != 0 {
                if next < fields + 2 {
                    return Err(MalformedExtraFields { flags });
                }
                next -= 2;
                *slot = Some(next);
            }
        }
        Ok(at)
    }

    /// ART (ART-325): `entry`'s extra fields as `GetExtraFields` reads them
    /// (`directory.c:3719-3731`), a word not there reading 0.
    pub fn read(entry: &[u8]) -> std::result::Result<Self, MalformedExtraFields> {
        let at = Self::word_offsets(entry)?;
        let word = |i: usize| {
            at[i]
                .and_then(|o| entry.get(o..o + 2))
                .map_or(0, |b| u16::from_be_bytes([b[0], b[1]]))
        };
        let long = |i: usize| (u32::from(word(i)) << 16) | u32::from(word(i + 1));
        Ok(Self {
            link: long(0),
            uid: word(2),
            gid: word(3),
            prot: long(4) | u32::from(entry.get(16).copied().unwrap_or(0)),
            virtualsize: long(6),
            rollpointer: long(8),
            fsizex: word(EXTRA_FSIZEX_WORD),
        })
    }

    /// ART (ART-325): what `AddExtraFields` puts at `extra_fields_offset`
    /// (`directory.c:3764-3800`): each non-zero word of `struct extrafields`,
    /// the highest first, then the flags word. `prot`'s lower byte is not
    /// stored — it is the entry's `protection` (`:3771-3772`).
    pub fn encode(&self) -> Vec<u8> {
        let prot = self.prot & 0xffff_ff00;
        let words: [u16; EXTRA_FIELD_WORDS] = [
            (self.link >> 16) as u16,
            self.link as u16,
            self.uid,
            self.gid,
            (prot >> 16) as u16,
            prot as u16,
            (self.virtualsize >> 16) as u16,
            self.virtualsize as u16,
            (self.rollpointer >> 16) as u16,
            self.rollpointer as u16,
            self.fsizex,
        ];
        let mut out = Vec::with_capacity(2 * EXTRA_FIELD_WORDS + 2);
        let mut flags = 0u16;
        for (i, word) in words.iter().enumerate().rev() {
            if *word != 0 {
                out.extend_from_slice(&word.to_be_bytes());
                flags |= 1 << i;
            }
        }
        out.extend_from_slice(&flags.to_be_bytes());
        out
    }
}

/// ART (2026-09-15, the scoped re-review's follow-up 6, ART-330): an entry a
/// directory-block walk cannot step over — smaller than its own header,
/// running past its block, or with a name running past the entry. `offset` is
/// where it starts in the block, `size` its size byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MalformedDirEntry {
    pub offset: usize,
    pub size: u8,
}

/// ART (2026-09-15, the scoped re-review's follow-up 6): the one rule for
/// walking a directory block, shared by the reader (`DirEntry::parse`) and the
/// writer (`Writer::dir_entry_at`). `Ok(None)` at the end of the block's
/// entries — a size byte of 0, as pfs3aio's `while (entry->next)` ends
/// (`directory.c:734`), or the end of the block. `Ok(Some((size, nlen)))` for
/// an entry whose header and name lie inside it and inside the block.
/// Anything else is `MalformedDirEntry`.
pub fn entry_bounds(
    data: &[u8],
    offset: usize,
) -> std::result::Result<Option<(usize, usize)>, MalformedDirEntry> {
    let Some(&size) = data.get(offset) else {
        return Ok(None);
    };
    if size == 0 {
        return Ok(None);
    }
    let malformed = MalformedDirEntry { offset, size };
    let len = usize::from(size);
    if len < 18 || offset + len > data.len() {
        return Err(malformed);
    }
    let nlen = usize::from(*data.get(offset + 17).ok_or(malformed)?);
    if 18 + nlen > len {
        return Err(malformed);
    }
    Ok(Some((len, nlen)))
}

impl DirEntry {
    /// Parse one direntry from `data` at `offset`.
    ///
    /// ART (2026-09-15, the scoped re-review's follow-ups 1 and 6):
    /// `Ok(Some((entry, next_offset)))`, or `Ok(None)` at the end of the
    /// block's entries, as `entry_bounds` says; a malformed entry is
    /// `Err(MalformedDirEntry)`, where 0.1.3 returned `None` — the end — so a
    /// name behind it read as not there and a listing ended short.
    /// `largefile` is the volume's `Rootblock::has_largefile`: without it
    /// `extra.fsizex` reads 0 and `file_size()` has no bits above 31, as
    /// pfs3aio's `GetDEFileSize` (`directory.c:3641-3652`) and this crate's
    /// deldir reader (`DelDirEntry::parse`) read it.
    ///
    /// ART (ART-325): `extra` is read by `ExtraFields::read`, pfs3aio's
    /// layout. An entry whose extra fields do not fit it is still listed, with
    /// no extra fields but its own protection byte in `prot`.
    pub fn parse(
        data: &[u8],
        offset: usize,
        largefile: bool,
    ) -> std::result::Result<Option<(Self, usize)>, MalformedDirEntry> {
        let Some((size, nlength)) = entry_bounds(data, offset)? else {
            return Ok(None);
        };
        let end = offset + size;
        let raw = &data[offset..end];
        let entry_size = raw[0];

        let entry_type = raw[1] as i8;
        let anode = u32::from_be_bytes(raw[2..6].try_into().unwrap());
        let fsize = u32::from_be_bytes(raw[6..10].try_into().unwrap());
        let creation_day = u16::from_be_bytes(raw[10..12].try_into().unwrap());
        let creation_minute = u16::from_be_bytes(raw[12..14].try_into().unwrap());
        let creation_tick = u16::from_be_bytes(raw[14..16].try_into().unwrap());
        let protection = raw[16];

        // `entry_bounds` holds the name inside the entry.
        let name = crate::util::latin1_to_string(&raw[18..18 + nlength]);

        let mut comment = String::new();
        let comment_off = 18 + nlength;
        if comment_off < raw.len() {
            let clen = raw[comment_off] as usize;
            let cstart = comment_off + 1;
            if cstart + clen <= raw.len() {
                comment = crate::util::latin1_to_string(&raw[cstart..cstart + clen]);
            }
        }

        let mut extra = ExtraFields::read(raw).unwrap_or_else(|_| ExtraFields {
            prot: u32::from(protection),
            ..ExtraFields::default()
        });
        if !largefile {
            extra.fsizex = 0;
        }

        Ok(Some((
            Self {
                entry_size,
                entry_type,
                anode,
                fsize,
                creation_day,
                creation_minute,
                creation_tick,
                protection,
                name,
                comment,
                extra,
            },
            end,
        )))
    }

    pub fn is_file(&self) -> bool {
        self.entry_type < 0 && self.entry_type != ST_ROLLOVERFILE
    }
    pub fn is_rollover(&self) -> bool {
        self.entry_type == ST_ROLLOVERFILE
    }
    pub fn is_dir(&self) -> bool {
        self.entry_type == ST_USERDIR
    }
    pub fn is_softlink(&self) -> bool {
        self.entry_type == ST_SOFTLINK
    }
    pub fn is_hardlink(&self) -> bool {
        self.entry_type == ST_LINKDIR || self.entry_type == ST_LINKFILE
    }

    /// Full file size including extended bits 32-47.
    ///
    /// ART (ART-325): `fsizex` as pfs3aio's layout places it. ART (2026-09-15,
    /// the scoped re-review's follow-up 1): part of the size only on a
    /// largefile volume, as pfs3aio's `GetDEFileSize` adds it
    /// (`directory.c:3641-3652`) — `DirEntry::parse` reads it as 0 elsewhere.
    pub fn file_size(&self) -> u64 {
        self.fsize as u64 | ((self.extra.fsizex as u64) << 32)
    }
}
