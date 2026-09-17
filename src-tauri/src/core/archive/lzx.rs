//! Amiga LZX, as a backend for the shared gate.
//!
//! LZX was the Amiga's other archiver, and the one a good share of Aminet and
//! of the owner's own collections is packed with. Round 1 refused it (R1-2);
//! the owner's decision 6 overrides that refusal, so ART reads it — with the
//! container described in the round's research (R2 § A1,
//! `docs/superpowers/notes/2026-09-16-card-round-2-lzx-and-names.md`) and the
//! attribute rules in R3 § 4
//! (`docs/superpowers/notes/2026-09-16-card-round-2-attributes.md`).
//!
//! **Why ART has its own reader rather than a crate** (R2 § A5). The gate's
//! contract is a limit *inside* the decode loop, and neither Rust LZX crate
//! has one — both size their buffers from header values. A merged group (see
//! below) wants one forward pass per group, which one crate cannot give
//! without re-decoding per member. One crate decodes the date wrongly on real
//! files, the other is a same-day, one-author release under a licence
//! `deny.toml` does not allow, and `unlzx.c` states no licence at all. So the
//! code here is written from the format description, not translated from any
//! of them, and it reuses ART's one CRC-32 (`core::hashing`).
//!
//! ```text
//! info header   10 bytes: "LZX" + 7 bytes ART does not read
//! record        31 bytes, then name (len [30]), then comment (len [14]),
//!               then `packed` bytes of data
//!   [0]      attributes: 0x01 r · 0x02 w · 0x04 d · 0x08 e · 0x10 a · 0x20 h
//!            · 0x40 s · 0x80 p, set = granted
//!   [2..6]   unpacked size, u32 LE
//!   [6..10]  packed size, u32 LE
//!   [11]     pack mode: 0 stored, 2 LZX
//!   [12]     flags, bit 0 merged
//!   [14]     comment length (0..=79)
//!   [18..22] date — NOT read (R2 § A3: three decoders, three years)
//!   [22..26] data CRC-32 LE, of this member's unpacked bytes
//!   [26..30] header CRC-32 LE, over the 31 bytes with [26..30] zeroed,
//!            then name, then comment
//!   [30]     name length
//! group         records with packed == 0 wait; the next record with
//!               packed > 0 closes the group and its data is the whole
//!               group's stream (members concatenated in record order)
//! names         Latin-1, `/` between components, no directory records
//!               (drawers are implied)
//! ```
//!
//! **A merged group is decoded whole, once**, which is why
//! [`read_selected`](ArchiveBackend::read_selected) is overridden the way
//! 7z's is: pulling members by index would decode the group once per member.
//! A group whose *declared* total passes [`MAX_GROUP_OUTPUT`] is refused per
//! member before a byte of it is decoded. Both CRCs the format stores are
//! checked: a header that fails its CRC refuses the archive at open, and a
//! member that fails its data CRC is refused by name and never written.
//!
//! **Dates are not claimed.** The three decoders R2 compared read three
//! different years from the same field, so `amiga.date` is always `None`.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::ops::Range;
use std::path::{Path, PathBuf};

use super::extract::{MAX_ENTRIES, MAX_ENTRY_OUTPUT};
use super::{AmigaAttributes, ArchiveBackend, ArchiveEntry};
use crate::core::error::{CoreError, CoreResult};
use crate::core::hashing::{crc32_ieee, Crc32};

const INFO_HEADER_LEN: u64 = 10;
const RECORD_LEN: usize = 31;
const METHOD_STORED: u8 = 0;
const METHOD_LZX: u8 = 2;

/// The most one merged group may unpack to. A group is decoded whole — its
/// members share one stream — so the per-entry cap is the group's cap.
pub const MAX_GROUP_OUTPUT: u64 = MAX_ENTRY_OUTPUT;

fn malformed(detail: impl Into<String>) -> CoreError {
    CoreError::Malformed {
        format: "lzx".into(),
        detail: detail.into(),
    }
}

#[derive(Debug, Clone)]
struct Record {
    entry: ArchiveEntry,
    data_crc: u32,
}

/// Records `members` share the stream of `packed` bytes at `data_offset`.
#[derive(Debug, Clone)]
struct Group {
    members: Range<usize>,
    method: u8,
    packed: u32,
    data_offset: u64,
}

#[derive(Debug)]
pub struct LzxBackend {
    path: PathBuf,
    records: Vec<Record>,
    groups: Vec<Group>,
    /// Records after the last group: nothing follows them.
    trailing: Range<usize>,
}

/// XADMaster `XADLZXParser.m:126-137`, the same bit order `unlzx.c:1101-1108`
/// prints: LZX stores "granted", AmigaDOS stores RWED inverted.
pub(crate) fn protection_from_lzx(attributes: u8) -> u32 {
    let a = u32::from(attributes);
    let mut p = 0;
    if a & 0x01 == 0 {
        p |= 0x08;
    }
    if a & 0x02 == 0 {
        p |= 0x04;
    }
    if a & 0x04 == 0 {
        p |= 0x01;
    }
    if a & 0x08 == 0 {
        p |= 0x02;
    }
    if a & 0x10 != 0 {
        p |= 0x10;
    }
    if a & 0x20 != 0 {
        p |= 0x80;
    }
    if a & 0x40 != 0 {
        p |= 0x40;
    }
    if a & 0x80 != 0 {
        p |= 0x20;
    }
    p
}

fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

fn read_exact_or(
    reader: &mut impl Read,
    buf: &mut [u8],
    what: impl FnOnce() -> String,
) -> CoreResult<()> {
    reader.read_exact(buf).map_err(|e| match e.kind() {
        std::io::ErrorKind::UnexpectedEof => malformed(what()),
        _ => CoreError::Io(e),
    })
}

/// A group's decode error, restated for one member — keeping its kind: a
/// pack mode ART does not read is not a damaged archive.
fn for_member(name: &str, error: &CoreError) -> CoreError {
    match error {
        CoreError::UnsupportedFormat(detail) => {
            CoreError::UnsupportedFormat(format!("'{name}': {detail}"))
        }
        CoreError::Malformed { detail, .. } => malformed(format!("'{name}': {detail}")),
        // A read error is the disk's, not the archive's (final review M1).
        CoreError::Io(io) => {
            CoreError::Io(std::io::Error::new(io.kind(), format!("'{name}': {io}")))
        }
        other => malformed(format!("'{name}': {other}")),
    }
}

/// A stored group: the bytes as they are, up to `stop_at`. The buffer grows
/// with the bytes present, never from a header value.
fn read_stored(packed: impl Read, stop_at: u64) -> CoreResult<Vec<u8>> {
    let mut out = Vec::new();
    packed.take(stop_at).read_to_end(&mut out)?;
    if out.len() as u64 != stop_at {
        return Err(malformed("stored data ends early"));
    }
    Ok(out)
}

/// Extra bits per offset/length slot (the "footer").
const ONE: [u8; 32] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13, 14, 14,
];
/// Base value per offset/length slot.
const TWO: [u32; 32] = [
    0, 1, 2, 3, 4, 6, 8, 12, 16, 24, 32, 48, 64, 96, 128, 192, 256, 384, 512, 768, 1024, 1536,
    2048, 3072, 4096, 6144, 8192, 12288, 16384, 24576, 32768, 49152,
];
const MAX_CODE_LEN: usize = 16;
const LITERAL_SYMBOLS: usize = 768;

/// The compressed stream's bits: 16-bit big-endian words, each taken least
/// significant bit first. The reader holds one word; nothing is sized from
/// the stream.
struct Bits<R> {
    inner: R,
    word: u16,
    left: u8,
}

impl<R: Read> Bits<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            word: 0,
            left: 0,
        }
    }

    fn bit(&mut self) -> CoreResult<u32> {
        if self.left == 0 {
            let mut two = [0u8; 2];
            let mut got = 0;
            // `got < 2` keeps the slice in range; `read` returns at most its length.
            while got < 2 {
                let n = self.inner.read(&mut two[got..])?;
                if n == 0 {
                    break;
                }
                got += n;
            }
            if got == 0 {
                return Err(malformed("the compressed stream ends before its data does"));
            }
            self.word = u16::from_be_bytes(two);
            self.left = 16;
        }
        let b = u32::from(self.word & 1);
        self.word >>= 1;
        self.left -= 1;
        Ok(b)
    }

    /// An n-bit value, its least significant bit read first.
    fn bits(&mut self, n: u8) -> CoreResult<u32> {
        let mut value = 0;
        for i in 0..n {
            value |= self.bit()? << i;
        }
        Ok(value)
    }
}

/// A canonical Huffman table: codes assigned in (length, symbol) order. Only
/// a complete table (Kraft sum exactly 1) is accepted.
struct Huffman {
    counts: [u16; MAX_CODE_LEN + 1],
    symbols: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8]) -> CoreResult<Self> {
        let mut counts = [0u16; MAX_CODE_LEN + 1];
        for &len in lengths {
            let slot = counts
                .get_mut(usize::from(len))
                .ok_or_else(|| malformed(format!("a code length of {len}")))?;
            *slot += 1;
        }
        counts[0] = 0;
        let mut left: i64 = 1;
        for &count in &counts[1..] {
            left = (left << 1) - i64::from(count);
            if left < 0 {
                return Err(malformed("a Huffman table is over-subscribed"));
            }
        }
        if left != 0 {
            return Err(malformed("a Huffman table is incomplete"));
        }
        // `lengths` is a slice this module owns: 8, 20 or 768 entries.
        let mut symbols = Vec::with_capacity(lengths.len());
        for len in 1..=MAX_CODE_LEN {
            for (symbol, &l) in lengths.iter().enumerate() {
                if usize::from(l) == len {
                    symbols.push(symbol as u16);
                }
            }
        }
        Ok(Self { counts, symbols })
    }

    /// One symbol, the code read most significant bit first.
    fn decode<R: Read>(&self, bits: &mut Bits<R>) -> CoreResult<u16> {
        let (mut code, mut first, mut index) = (0i64, 0i64, 0i64);
        for &count in &self.counts[1..] {
            code |= i64::from(bits.bit()?);
            let count = i64::from(count);
            if code - count < first {
                return usize::try_from(index + code - first)
                    .ok()
                    .and_then(|i| self.symbols.get(i).copied())
                    .ok_or_else(|| malformed("a Huffman code names no symbol"));
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err(malformed("a Huffman code runs past 16 bits"))
    }
}

/// A code length is stored as a change to the one before it, modulo 17.
fn delta(previous: u8, symbol: u16) -> u8 {
    ((u16::from(previous) + 17 - symbol) % 17) as u8
}

/// The 768 literal code lengths, in two passes (0..256 with `fix` 1, then
/// 256..768 with `fix` 0), each through its own 20-symbol pretree. The
/// lengths are deltas against the previous block's, so `lengths` persists.
fn read_literal_lengths<R: Read>(
    bits: &mut Bits<R>,
    lengths: &mut [u8; LITERAL_SYMBOLS],
) -> CoreResult<()> {
    let mut pos = 0usize;
    for (fix, end) in [(1u32, 256usize), (0u32, LITERAL_SYMBOLS)] {
        let mut pre = [0u8; 20];
        for l in pre.iter_mut() {
            *l = bits.bits(4)? as u8;
        }
        let pretree = Huffman::new(&pre)?;
        while pos < end {
            let symbol = pretree.decode(bits)?;
            let (run, value) = match symbol {
                17 => (3 + bits.bits(4)? + fix, 0),
                18 => (19 + bits.bits((6 - fix) as u8)? + fix, 0),
                19 => {
                    let run = 3 + bits.bits(1)? + fix;
                    let next = pretree.decode(bits)?;
                    if next > 16 {
                        return Err(malformed("a length repeat names another repeat"));
                    }
                    (run, delta(lengths[pos], next))
                }
                s => {
                    lengths[pos] = delta(lengths[pos], s);
                    pos += 1;
                    continue;
                }
            };
            for _ in 0..run {
                if pos >= end {
                    break;
                }
                lengths[pos] = value;
                pos += 1;
            }
        }
    }
    Ok(())
}

/// A compressed (pack mode 2) group, decoded up to `stop_at` bytes — exactly
/// `stop_at` or an error. The caller keeps `stop_at` within
/// [`MAX_GROUP_OUTPUT`], and that is the only bound `out` grows to.
///
/// ```text
/// block     method = bits(3): 1 reuse the literal table, 2 new table,
///           3 new table + aligned offsets (8 × bits(3) lengths first);
///           then length = bits(8)<<16 | bits(8)<<8 | bits(8);
///           then, for 2 and 3, the literal lengths
/// symbol    < 256 a literal; else slot = s & 31 gives the offset
///           (TWO + extra bits, the low 3 from the aligned table in a
///           method-3 block when ONE ≥ 3; 0 means the last offset) and
///           (s >> 5) & 15 the length (TWO + 3 + extra bits)
/// ```
///
/// State resets per group: lengths zero, last offset 1, no table, and a
/// window of zeros that a match may reach back into.
fn decode_lzx<R: Read>(packed: R, stop_at: u64) -> CoreResult<Vec<u8>> {
    let mut bits = Bits::new(packed);
    let mut out: Vec<u8> = Vec::new();
    let mut literal_len = [0u8; LITERAL_SYMBOLS];
    let mut literal: Option<Huffman> = None;
    let mut aligned: Option<Huffman> = None;
    let mut method = 0u32;
    let mut block_left = 0u64;
    let mut last_offset = 1usize;

    while (out.len() as u64) < stop_at {
        if block_left == 0 {
            method = bits.bits(3)?;
            if !(1..=3).contains(&method) {
                return Err(malformed(format!(
                    "block type {method} is not one Amiga LZX writes"
                )));
            }
            if method == 3 {
                let mut offset_len = [0u8; 8];
                for l in offset_len.iter_mut() {
                    *l = bits.bits(3)? as u8;
                }
                aligned = Some(Huffman::new(&offset_len)?);
            }
            block_left = u64::from(bits.bits(8)?) << 16;
            block_left |= u64::from(bits.bits(8)?) << 8;
            block_left |= u64::from(bits.bits(8)?);
            if method != 1 {
                read_literal_lengths(&mut bits, &mut literal_len)?;
                literal = Some(Huffman::new(&literal_len)?);
            }
            if literal.is_none() {
                return Err(malformed(
                    "a block reuses a literal table no earlier block built",
                ));
            }
            continue;
        }
        let table = literal
            .as_ref()
            .ok_or_else(|| malformed("no literal table"))?;
        let symbol = usize::from(table.decode(&mut bits)?);
        if symbol < 256 {
            out.push(symbol as u8);
            block_left = block_left.saturating_sub(1);
            continue;
        }
        let symbol = symbol - 256;
        let slot = symbol & 31;
        let footer = ONE[slot];
        let mut offset = TWO[slot] as usize;
        if method == 3 && footer >= 3 {
            offset += (bits.bits(footer - 3)? as usize) << 3;
            let aligned = aligned
                .as_ref()
                .ok_or_else(|| malformed("an aligned block has no offset table"))?;
            offset += usize::from(aligned.decode(&mut bits)?);
        } else {
            offset += bits.bits(footer)? as usize;
            if offset == 0 {
                offset = last_offset;
            }
        }
        last_offset = offset;
        let len_slot = (symbol >> 5) & 15;
        let length = TWO[len_slot] as usize + 3 + bits.bits(ONE[len_slot])? as usize;
        for _ in 0..length {
            if out.len() as u64 >= stop_at {
                break;
            }
            // The window starts as 64 KiB of zeros, and the archiver's matches
            // reach into it; an offset is at most 65 535 (`TWO` + `ONE`), so
            // it never reaches past that window.
            let byte = match out.len().checked_sub(offset) {
                None => 0,
                Some(at) => *out
                    .get(at)
                    .ok_or_else(|| malformed("a match reads past what was decoded"))?,
            };
            out.push(byte);
        }
        block_left = block_left.saturating_sub(length as u64);
    }
    Ok(out)
}

impl LzxBackend {
    /// Walk every record header, checking each one's CRC and that its data
    /// lies inside the file. Nothing is decoded here.
    pub fn open(path: &Path) -> CoreResult<Self> {
        let file = File::open(path)?;
        let file_len = file.metadata()?.len();
        let mut reader = BufReader::new(file);
        let mut info = [0u8; INFO_HEADER_LEN as usize];
        read_exact_or(&mut reader, &mut info, || {
            "shorter than the 10-byte LZX info header".into()
        })?;
        if &info[..3] != b"LZX" {
            return Err(malformed("does not start with 'LZX'"));
        }
        let mut at = INFO_HEADER_LEN;
        let mut records = Vec::new();
        let mut groups = Vec::new();
        let mut open_from = 0usize;
        // One past the gate's cap is enough for the gate to refuse the archive.
        while at < file_len && records.len() <= MAX_ENTRIES {
            let index = records.len();
            let mut head = [0u8; RECORD_LEN];
            read_exact_or(&mut reader, &mut head, || {
                format!("record {index} is cut short")
            })?;
            // At most 255 bytes each: a u8, not a size claim.
            let mut name = vec![0u8; usize::from(head[30])];
            read_exact_or(&mut reader, &mut name, || {
                format!("record {index}'s name is cut short")
            })?;
            let mut comment = vec![0u8; usize::from(head[14])];
            read_exact_or(&mut reader, &mut comment, || {
                format!("record {index}'s comment is cut short")
            })?;

            let stored = u32::from_le_bytes([head[26], head[27], head[28], head[29]]);
            let mut zeroed = head;
            zeroed[26..30].fill(0);
            let mut crc = Crc32::new();
            crc.update(&zeroed);
            crc.update(&name);
            crc.update(&comment);
            if crc.finish() != stored {
                return Err(malformed(format!(
                    "record {index} ('{}') fails its header CRC",
                    latin1(&name)
                )));
            }

            let unpacked = u32::from_le_bytes([head[2], head[3], head[4], head[5]]);
            let packed = u32::from_le_bytes([head[6], head[7], head[8], head[9]]);
            at += (RECORD_LEN + name.len() + comment.len()) as u64;
            let data_offset = at;
            let end = at
                .checked_add(u64::from(packed))
                .filter(|end| *end <= file_len)
                .ok_or_else(|| {
                    malformed(format!(
                        "record {index} says {packed} packed bytes follow and the file ends first"
                    ))
                })?;

            records.push(Record {
                entry: ArchiveEntry {
                    name: latin1(&name),
                    is_dir: false,
                    declared_bytes: u64::from(unpacked),
                    amiga: AmigaAttributes {
                        protection: Some(protection_from_lzx(head[0])),
                        comment: (!comment.is_empty()).then(|| latin1(&comment)),
                        date: None,
                    },
                },
                data_crc: u32::from_le_bytes([head[22], head[23], head[24], head[25]]),
            });

            if packed > 0 {
                groups.push(Group {
                    members: open_from..index + 1,
                    method: head[11],
                    packed,
                    data_offset,
                });
                open_from = index + 1;
                reader.seek(SeekFrom::Start(end))?;
                at = end;
            }
        }
        let trailing = open_from..records.len();
        Ok(Self {
            path: path.to_path_buf(),
            records,
            groups,
            trailing,
        })
    }
}

impl ArchiveBackend for LzxBackend {
    fn format(&self) -> &'static str {
        "lzx"
    }

    fn entries(&mut self) -> CoreResult<Vec<ArchiveEntry>> {
        Ok(self.records.iter().map(|r| r.entry.clone()).collect())
    }

    fn read(&mut self, index: usize, limit: u64) -> CoreResult<Vec<u8>> {
        let count = self.records.len();
        if index >= count {
            return Err(malformed(format!(
                "this archive has no entry {index}; it holds {count}"
            )));
        }
        let mut wanted = vec![false; count];
        wanted[index] = true;
        let mut found: Option<CoreResult<Vec<u8>>> = None;
        self.read_selected(&wanted, limit, &mut |_, data| {
            found = Some(data);
            Ok(())
        })?;
        found.unwrap_or_else(|| Err(malformed(format!("entry {index} carries no data stream"))))
    }

    /// One decode per merged group, stopping after its last wanted member.
    fn read_selected(
        &mut self,
        wanted: &[bool],
        limit: u64,
        sink: &mut dyn FnMut(usize, CoreResult<Vec<u8>>) -> CoreResult<()>,
    ) -> CoreResult<()> {
        let is_wanted = |i: usize| wanted.get(i).copied().unwrap_or(false);
        let mut reader = BufReader::new(File::open(&self.path)?);
        for group in &self.groups {
            // Final review M2: a wanted member past `limit` is refused from its
            // declared size before the group is decoded, and does not make the
            // decode run on to reach it.
            let mut fits = Vec::new();
            for i in group.members.clone().filter(|i| is_wanted(*i)) {
                let entry = &self.records[i].entry;
                if entry.declared_bytes > limit {
                    sink(
                        i,
                        Err(CoreError::InvalidInput(format!(
                            "'{}' is {} bytes, past the {limit} bytes asked for",
                            entry.name, entry.declared_bytes
                        ))),
                    )?;
                } else {
                    fits.push(i);
                }
            }
            let Some(&last_wanted) = fits.last() else {
                continue;
            };
            // ≤ 100 001 × u32::MAX: no overflow in u64.
            let total: u64 = group
                .members
                .clone()
                .map(|i| self.records[i].entry.declared_bytes)
                .sum();
            if total > MAX_GROUP_OUTPUT {
                for i in group.members.clone().filter(|i| is_wanted(*i)) {
                    sink(
                        i,
                        Err(CoreError::InvalidInput(format!(
                            "'{}' is inside a merged LZX group that unpacks to {total} bytes, \
                             past the {MAX_GROUP_OUTPUT} byte limit ART decodes at once",
                            self.records[i].entry.name
                        ))),
                    )?;
                }
                continue;
            }
            let stop_at: u64 = group
                .members
                .clone()
                .take_while(|i| *i <= last_wanted)
                .map(|i| self.records[i].entry.declared_bytes)
                .sum();
            reader.seek(SeekFrom::Start(group.data_offset))?;
            let packed = (&mut reader).take(u64::from(group.packed));
            let decoded = match group.method {
                METHOD_STORED => read_stored(packed, stop_at),
                METHOD_LZX => decode_lzx(packed, stop_at),
                other => Err(CoreError::UnsupportedFormat(format!(
                    "LZX pack mode {other} is not one ART reads"
                ))),
            };
            let mut decoded = decoded;
            let mut offset = 0u64;
            for i in group.members.clone() {
                let size = self.records[i].entry.declared_bytes;
                let range = offset as usize..(offset + size) as usize;
                offset += size;
                if !fits.contains(&i) {
                    continue;
                }
                let name = &self.records[i].entry.name;
                let result = match &mut decoded {
                    Err(e) => Err(for_member(name, e)),
                    Ok(bytes) => match bytes.get(range.clone()) {
                        None => Err(malformed(format!(
                            "'{name}' lies past the data its group decoded"
                        ))),
                        Some(slice) if crc32_ieee(slice) != self.records[i].data_crc => {
                            Err(malformed(format!("'{name}' fails its data CRC")))
                        }
                        // The last member moves out of the buffer rather than
                        // being copied beside it.
                        Some(_) if i == last_wanted => {
                            let mut owned = std::mem::take(bytes);
                            owned.truncate(range.end);
                            owned.drain(..range.start);
                            Ok(owned)
                        }
                        Some(slice) => Ok(slice.to_vec()),
                    },
                };
                sink(i, result)?;
                if i == last_wanted {
                    break;
                }
            }
        }
        for i in self.trailing.clone().filter(|i| is_wanted(*i)) {
            let entry = &self.records[i].entry;
            let result = if entry.declared_bytes == 0 {
                Ok(Vec::new())
            } else {
                Err(malformed(format!(
                    "'{}' has no packed data after it — the archive ends first",
                    entry.name
                )))
            };
            sink(i, result)?;
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::core::archive::extract::{extract_with_backend, OverwritePolicy};
    use crate::core::jobs::NoProgress;

    fn scratch(tag: &str) -> (crate::core::ScratchDir, PathBuf) {
        crate::core::ScratchDir::pair("art-lzx", tag)
    }

    /// One LZX record as a test wants it written. `packed` empty means the
    /// record waits for a later one to close its merged group.
    pub(crate) struct Rec<'a> {
        pub name: &'a str,
        pub comment: &'a str,
        pub attrs: u8,
        pub unpacked: u32,
        pub method: u8,
        pub data_crc: u32,
        pub packed: &'a [u8],
    }

    /// An LZX archive, byte for byte as the container description in the
    /// module doc comment.
    pub(crate) fn archive(records: &[Rec]) -> Vec<u8> {
        let mut out = b"LZX".to_vec();
        out.extend_from_slice(&[0u8; 7]);
        for r in records {
            let mut h = [0u8; 31];
            h[0] = r.attrs;
            h[2..6].copy_from_slice(&r.unpacked.to_le_bytes());
            h[6..10].copy_from_slice(&(r.packed.len() as u32).to_le_bytes());
            h[10] = 10;
            h[11] = r.method;
            h[12] = u8::from(r.packed.is_empty());
            h[14] = r.comment.len() as u8;
            h[15] = 10;
            h[22..26].copy_from_slice(&r.data_crc.to_le_bytes());
            h[30] = r.name.len() as u8;
            let mut crc = crate::core::hashing::Crc32::new();
            crc.update(&h);
            crc.update(r.name.as_bytes());
            crc.update(r.comment.as_bytes());
            h[26..30].copy_from_slice(&crc.finish().to_le_bytes());
            out.extend_from_slice(&h);
            out.extend_from_slice(r.name.as_bytes());
            out.extend_from_slice(r.comment.as_bytes());
            out.extend_from_slice(r.packed);
        }
        out
    }

    fn crc(bytes: &[u8]) -> u32 {
        crate::core::hashing::crc32_ieee(bytes)
    }

    /// The shared gate tests' builder (`core/archive/mod.rs::backends`): every
    /// entry in **one stored merged group**, so the gate's selections go
    /// through the group path rather than one record per stream.
    pub(crate) fn make_lzx_with(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let stream: Vec<u8> = entries
            .iter()
            .flat_map(|(_, d)| d.iter().copied())
            .collect();
        assert!(!stream.is_empty(), "a group with no bytes never closes");
        let last = entries.len() - 1;
        let records: Vec<Rec> = entries
            .iter()
            .enumerate()
            .map(|(i, (name, data))| Rec {
                name,
                comment: "",
                attrs: 0x0F,
                unpacked: data.len() as u32,
                method: 0,
                data_crc: crc(data),
                packed: if i == last { &stream } else { b"" },
            })
            .collect();
        archive(&records)
    }

    /// Three members in one stored group: listed with their Amiga attributes,
    /// written with the right bytes, drawers created from the paths alone.
    #[test]
    fn a_stored_merged_group_lists_and_writes_every_member() {
        let (_guard, dir) = scratch("stored-group");
        let path = dir.join("group.lzx");
        std::fs::write(
            &path,
            archive(&[
                Rec {
                    name: "C/One",
                    comment: "",
                    attrs: 0x0F,
                    unpacked: 3,
                    method: 0,
                    data_crc: crc(b"aaa"),
                    packed: b"",
                },
                Rec {
                    name: "C/Two",
                    comment: "Prism2",
                    attrs: 0x4F,
                    unpacked: 2,
                    method: 0,
                    data_crc: crc(b"bb"),
                    packed: b"",
                },
                Rec {
                    name: "Three",
                    comment: "",
                    attrs: 0x0F,
                    unpacked: 4,
                    method: 0,
                    data_crc: crc(b"cccc"),
                    packed: b"aaabbcccc",
                },
            ]),
        )
        .unwrap();

        let mut backend = LzxBackend::open(&path).unwrap();
        let entries = backend.entries().unwrap();
        assert_eq!(
            entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            ["C/One", "C/Two", "Three"]
        );
        assert_eq!(entries[1].amiga.protection, Some(0x40));
        assert_eq!(entries[1].amiga.comment.as_deref(), Some("Prism2"));
        assert_eq!(
            entries[1].amiga.date, None,
            "LZX dates are not claimed (R2 § A3)"
        );

        let out = dir.join("out");
        let outcome =
            extract_with_backend(&mut backend, &out, OverwritePolicy::Skip, &NoProgress).unwrap();
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(std::fs::read(out.join("C/One")).unwrap(), b"aaa");
        assert_eq!(std::fs::read(out.join("C/Two")).unwrap(), b"bb");
        assert_eq!(std::fs::read(out.join("Three")).unwrap(), b"cccc");
    }

    #[test]
    fn the_protection_byte_is_converted_to_the_amigados_order() {
        assert_eq!(protection_from_lzx(0x0F), 0x00); // ----rwed
        assert_eq!(protection_from_lzx(0x00), 0x0F); // --------
        assert_eq!(protection_from_lzx(0x0E), 0x08); // r not granted
        assert_eq!(protection_from_lzx(0x0D), 0x04); // w
        assert_eq!(protection_from_lzx(0x0B), 0x01); // d
        assert_eq!(protection_from_lzx(0x07), 0x02); // e
        assert_eq!(protection_from_lzx(0x1F), 0x10); // a
        assert_eq!(protection_from_lzx(0x2F), 0x80); // h
        assert_eq!(protection_from_lzx(0x4F), 0x40); // s
        assert_eq!(protection_from_lzx(0x8F), 0x20); // p
    }

    #[test]
    fn a_record_whose_header_crc_does_not_match_is_refused_by_number() {
        let (_guard, dir) = scratch("header-crc");
        let mut bytes = archive(&[Rec {
            name: "Two",
            comment: "",
            attrs: 0x0F,
            unpacked: 1,
            method: 0,
            data_crc: crc(b"x"),
            packed: b"x",
        }]);
        bytes[10 + 31] ^= 0x20; // "Two" → "two" after the CRC was computed
        let path = dir.join("bad.lzx");
        std::fs::write(&path, bytes).unwrap();
        let err = LzxBackend::open(&path).expect_err("refused");
        assert!(
            err.to_string().contains("record 0") && err.to_string().contains("header CRC"),
            "{err}"
        );
    }

    #[test]
    fn a_member_whose_data_crc_does_not_match_is_not_written() {
        let (_guard, dir) = scratch("data-crc");
        let path = dir.join("bad.lzx");
        std::fs::write(
            &path,
            archive(&[
                Rec {
                    name: "Good",
                    comment: "",
                    attrs: 0x0F,
                    unpacked: 2,
                    method: 0,
                    data_crc: crc(b"gg"),
                    packed: b"",
                },
                Rec {
                    name: "Bad",
                    comment: "",
                    attrs: 0x0F,
                    unpacked: 2,
                    method: 0,
                    data_crc: crc(b"XX"),
                    packed: b"ggbb",
                },
            ]),
        )
        .unwrap();
        let out = dir.join("out");
        let mut backend = LzxBackend::open(&path).unwrap();
        let outcome =
            extract_with_backend(&mut backend, &out, OverwritePolicy::Skip, &NoProgress).unwrap();
        assert!(out.join("Good").is_file());
        assert!(!out.join("Bad").exists());
        assert!(
            outcome
                .errors
                .iter()
                .any(|e| e.contains("Bad") && e.contains("data CRC")),
            "{:?}",
            outcome.errors
        );
    }

    #[test]
    fn an_archive_cut_short_is_refused() {
        let (_guard, dir) = scratch("cut");
        let whole = archive(&[Rec {
            name: "File",
            comment: "",
            attrs: 0x0F,
            unpacked: 4,
            method: 0,
            data_crc: crc(b"abcd"),
            packed: b"abcd",
        }]);
        let mid_record = dir.join("mid-record.lzx");
        std::fs::write(&mid_record, &whole[..10 + 20]).unwrap();
        assert!(LzxBackend::open(&mid_record)
            .err()
            .unwrap()
            .to_string()
            .contains("cut short"));
        let mid_data = dir.join("mid-data.lzx");
        std::fs::write(&mid_data, &whole[..whole.len() - 2]).unwrap();
        assert!(LzxBackend::open(&mid_data)
            .err()
            .unwrap()
            .to_string()
            .contains("file ends first"));
    }

    /// Refused on the declared sizes, before a byte is decoded: the packed
    /// bytes here are not a valid stream, so any attempt to decode would say
    /// something else.
    #[test]
    fn a_group_past_the_limit_is_refused_before_anything_is_decoded() {
        let (_guard, dir) = scratch("big-group");
        let path = dir.join("big.lzx");
        std::fs::write(
            &path,
            archive(&[
                Rec {
                    name: "A",
                    comment: "",
                    attrs: 0x0F,
                    unpacked: 200 << 20,
                    method: 2,
                    data_crc: 0,
                    packed: b"",
                },
                Rec {
                    name: "B",
                    comment: "",
                    attrs: 0x0F,
                    unpacked: 100 << 20,
                    method: 2,
                    data_crc: 0,
                    packed: &[0xFF; 8],
                },
            ]),
        )
        .unwrap();
        let mut backend = LzxBackend::open(&path).unwrap();
        let mut seen = Vec::new();
        backend
            .read_selected(&[true, true], MAX_ENTRY_OUTPUT, &mut |i, r| {
                seen.push((i, r.err().map(|e| e.to_string()).unwrap_or_default()));
                Ok(())
            })
            .unwrap();
        assert_eq!(seen.len(), 2);
        assert!(
            seen.iter()
                .all(|(_, e)| e.contains("merged LZX group") && e.contains("limit")),
            "{seen:?}"
        );
    }

    #[test]
    fn a_member_larger_than_the_limit_asked_is_refused() {
        let (_guard, dir) = scratch("limit");
        let path = dir.join("l.lzx");
        std::fs::write(
            &path,
            archive(&[Rec {
                name: "F",
                comment: "",
                attrs: 0x0F,
                unpacked: 3,
                method: 0,
                data_crc: crc(b"abc"),
                packed: b"abc",
            }]),
        )
        .unwrap();
        let mut backend = LzxBackend::open(&path).unwrap();
        assert!(backend.read(0, 2).is_err());
        assert_eq!(backend.read(0, 3).unwrap(), b"abc");
    }

    /// Final review M2: a member past the limit asked is refused from its
    /// declared size, before its group is decoded — a pack mode ART does not
    /// read would otherwise answer first.
    #[test]
    fn a_member_past_the_limit_is_refused_before_its_group_is_decoded() {
        let (_guard, dir) = scratch("limit-first");
        let path = dir.join("l.lzx");
        std::fs::write(
            &path,
            archive(&[Rec {
                name: "F",
                comment: "",
                attrs: 0x0F,
                unpacked: 3,
                method: 7,
                data_crc: 0,
                packed: b"abc",
            }]),
        )
        .unwrap();
        let mut backend = LzxBackend::open(&path).unwrap();
        let err = backend.read(0, 2).unwrap_err();
        assert!(matches!(err, CoreError::InvalidInput(_)), "{err:?}");
        assert!(
            err.to_string()
                .contains("'F' is 3 bytes, past the 2 bytes asked for"),
            "{err}"
        );
    }

    /// Final review M1: a read error while decoding is still a read error —
    /// a failing disk is not a damaged archive — and it names the member.
    #[test]
    fn an_io_error_for_a_member_stays_an_io_error() {
        let io = CoreError::Io(std::io::Error::other("the device is not ready"));
        let restated = for_member("C/Assign", &io);
        assert!(matches!(restated, CoreError::Io(_)), "{restated:?}");
        let text = restated.to_string();
        assert!(
            text.contains("'C/Assign'") && text.contains("the device is not ready"),
            "{text}"
        );
    }

    #[test]
    fn members_left_waiting_at_the_end_are_refused_unless_empty() {
        let (_guard, dir) = scratch("trailing");
        let path = dir.join("t.lzx");
        std::fs::write(
            &path,
            archive(&[
                Rec {
                    name: "Empty",
                    comment: "",
                    attrs: 0x0F,
                    unpacked: 0,
                    method: 0,
                    data_crc: 0,
                    packed: b"",
                },
                Rec {
                    name: "Lost",
                    comment: "",
                    attrs: 0x0F,
                    unpacked: 5,
                    method: 0,
                    data_crc: 0,
                    packed: b"",
                },
            ]),
        )
        .unwrap();
        let out = dir.join("out");
        let mut backend = LzxBackend::open(&path).unwrap();
        let outcome =
            extract_with_backend(&mut backend, &out, OverwritePolicy::Skip, &NoProgress).unwrap();
        assert_eq!(std::fs::read(out.join("Empty")).unwrap(), b"");
        assert!(!out.join("Lost").exists());
        assert!(
            outcome
                .errors
                .iter()
                .any(|e| e.contains("Lost") && e.contains("no packed data")),
            "{:?}",
            outcome.errors
        );
    }

    /// The bytes decide, not the name (`core/archive/mod.rs::open`'s rule).
    #[test]
    fn an_lzx_is_opened_as_lzx_whatever_it_is_called() {
        let (_guard, dir) = scratch("dispatch");
        let path = dir.join("really.lha");
        std::fs::write(
            &path,
            archive(&[Rec {
                name: "F",
                comment: "",
                attrs: 0x0F,
                unpacked: 1,
                method: 0,
                data_crc: crc(b"z"),
                packed: b"z",
            }]),
        )
        .unwrap();
        assert_eq!(
            crate::core::detect::detect(&path).unwrap().format_hint,
            "lzx"
        );
        assert_eq!(crate::core::archive::open(&path).unwrap().format(), "lzx");
    }

    struct BitWriter {
        bytes: Vec<u8>,
        word: u16,
        used: u32,
    }

    impl BitWriter {
        fn new() -> Self {
            Self {
                bytes: Vec::new(),
                word: 0,
                used: 0,
            }
        }
        fn bit(&mut self, b: u32) {
            self.word |= ((b & 1) as u16) << self.used;
            self.used += 1;
            if self.used == 16 {
                self.bytes.extend_from_slice(&self.word.to_be_bytes());
                self.word = 0;
                self.used = 0;
            }
        }
        /// An n-bit value, least significant bit first.
        fn put(&mut self, value: u32, n: u32) {
            for i in 0..n {
                self.bit(value >> i);
            }
        }
        /// A Huffman code, most significant bit first.
        fn code(&mut self, code: u32, len: u32) {
            for i in (0..len).rev() {
                self.bit(code >> i);
            }
        }
        fn finish(mut self) -> Vec<u8> {
            if self.used > 0 {
                self.bytes.extend_from_slice(&self.word.to_be_bytes());
            }
            self.bytes
        }
    }

    /// One block whose literal table gives symbols 0..512 nine-bit codes
    /// (code = symbol) and 512..768 none. The pretree gives symbol 0 and 8
    /// one-bit codes (0 → `0`, 8 → `1`): 8 turns a 0 into 9, 0 leaves a 0.
    fn nine_bit_table(w: &mut BitWriter) {
        let pretree = |w: &mut BitWriter| {
            for s in 0..20 {
                w.put(if s == 0 || s == 8 { 1 } else { 0 }, 4);
            }
        };
        pretree(w); // pass A: 256 × "9"
        for _ in 0..256 {
            w.code(1, 1);
        }
        pretree(w); // pass B: 256 × "9", then 256 × "0"
        for _ in 0..256 {
            w.code(1, 1);
        }
        for _ in 0..256 {
            w.code(0, 1);
        }
    }

    fn put_len(w: &mut BitWriter, length: u32) {
        w.put((length >> 16) & 0xFF, 8);
        w.put((length >> 8) & 0xFF, 8);
        w.put(length & 0xFF, 8);
    }

    fn one_member(path: &Path, data: &[u8], stream: &[u8]) {
        std::fs::write(
            path,
            archive(&[Rec {
                name: "Out",
                comment: "",
                attrs: 0x0F,
                unpacked: data.len() as u32,
                method: 2,
                data_crc: crc(data),
                packed: stream,
            }]),
        )
        .unwrap();
    }

    /// Literals, a match with three LSB-first extra offset bits (value 6, not a
    /// palindrome, so a reversed read gives 3), then a repeat of the last offset.
    #[test]
    fn a_compressed_block_of_literals_and_matches_decodes() {
        let expected = b"0123456789ABCDEFGHIJKLMN2345678";
        let mut w = BitWriter::new();
        w.put(2, 3);
        put_len(&mut w, expected.len() as u32);
        nine_bit_table(&mut w);
        for &b in &expected[..24] {
            w.code(u32::from(b), 9);
        }
        w.code(256 + 8, 9); // offset slot 8: 16 + bits(3); length slot 0: 3
        w.put(6, 3); // offset 22 → "234"
        w.code(256 + 32, 9); // offset slot 0 → last offset 22; length slot 1: 4 → "5678"
        let (_guard, dir) = scratch("lzx-block");
        let path = dir.join("b.lzx");
        one_member(&path, expected, &w.finish());
        assert_eq!(
            LzxBackend::open(&path).unwrap().read(0, 1 << 20).unwrap(),
            expected
        );
    }

    /// The same table as [`nine_bit_table`], built from the run symbols. The
    /// pretree gives 8, 17, 18 and 19 two-bit codes (`00`, `01`, `10`, `11`).
    /// Pass A (fix 1): one 8, then 51 × symbol 19 with bits(1) = 1 → runs of
    /// 3 + 1 + 1 = 5 nines. Pass B (fix 0): 64 × symbol 19 → runs of 4 nines,
    /// then symbol 18 × 3 with bits(6) = 63 → 82 zeros each, then symbol 17
    /// with bits(4) = 7 → 10 zeros.
    fn nine_bit_table_from_runs(w: &mut BitWriter) {
        let pretree = |w: &mut BitWriter| {
            for s in 0..20 {
                w.put(if [8, 17, 18, 19].contains(&s) { 2 } else { 0 }, 4);
            }
        };
        pretree(w);
        w.code(0b00, 2);
        for _ in 0..51 {
            w.code(0b11, 2);
            w.put(1, 1);
            w.code(0b00, 2);
        }
        pretree(w);
        for _ in 0..64 {
            w.code(0b11, 2);
            w.put(1, 1);
            w.code(0b00, 2);
        }
        for _ in 0..3 {
            w.code(0b10, 2);
            w.put(63, 6);
        }
        w.code(0b01, 2);
        w.put(7, 4);
    }

    /// Runs carry `fix` in their length (1 in pass A, 0 in pass B): read with
    /// the wrong one, the table's positions shift and the text does not decode.
    #[test]
    fn a_literal_table_built_from_length_runs_decodes() {
        let expected = b"0123456789ABCDEFGHIJKLMN2345678";
        let mut w = BitWriter::new();
        w.put(2, 3);
        put_len(&mut w, expected.len() as u32);
        nine_bit_table_from_runs(&mut w);
        for &b in &expected[..24] {
            w.code(u32::from(b), 9);
        }
        w.code(256 + 8, 9);
        w.put(6, 3);
        w.code(256 + 32, 9);
        let (_guard, dir) = scratch("lzx-runs");
        let path = dir.join("runs.lzx");
        one_member(&path, expected, &w.finish());
        assert_eq!(
            LzxBackend::open(&path).unwrap().read(0, 1 << 20).unwrap(),
            expected
        );
    }

    #[test]
    fn an_aligned_offset_block_decodes() {
        let expected = b"0123456789ABCDEFGHIJKLMN234";
        let mut w = BitWriter::new();
        w.put(3, 3);
        for _ in 0..8 {
            w.put(3, 3); // eight 3-bit aligned codes: code = symbol
        }
        put_len(&mut w, expected.len() as u32);
        nine_bit_table(&mut w);
        for &b in &expected[..24] {
            w.code(u32::from(b), 9);
        }
        w.code(256 + 8, 9); // slot 8 (footer 3): 16 + (bits(0) << 3) + aligned
        w.code(6, 3); // aligned symbol 6 (code 110, not a palindrome) → offset 22 → "234"
        let (_guard, dir) = scratch("lzx-aligned");
        let path = dir.join("a.lzx");
        one_member(&path, expected, &w.finish());
        assert_eq!(
            LzxBackend::open(&path).unwrap().read(0, 1 << 20).unwrap(),
            expected
        );
    }

    #[test]
    fn a_first_block_that_reuses_a_table_is_refused() {
        let mut w = BitWriter::new();
        w.put(1, 3);
        put_len(&mut w, 1);
        w.put(0, 16);
        let (_guard, dir) = scratch("lzx-reuse");
        let path = dir.join("r.lzx");
        one_member(&path, b"x", &w.finish());
        let err = LzxBackend::open(&path).unwrap().read(0, 16).unwrap_err();
        assert!(err.to_string().contains("no earlier block built"), "{err}");
    }

    /// The Amiga archiver starts every group with a 64 KiB window of zeros,
    /// and matches reach into it: `boingbag1.lzx` opens one group with a
    /// repeat of the initial last offset (1) at byte 0, and another with a
    /// match reaching one byte before the start (found by
    /// `read_the_owners_lzx_archives_when_asked`, 150 members refused). XADMaster
    /// zero-fills its window the same way (`LZSS.c`, `RestartLZSS`).
    #[test]
    fn a_match_before_the_first_byte_reads_the_zeroed_window() {
        // Byte 0: a repeat of the initial last offset, three bytes long.
        let mut w = BitWriter::new();
        w.put(2, 3);
        put_len(&mut w, 4);
        nine_bit_table(&mut w);
        w.code(256, 9); // slot 0, length slot 0: last offset (1), length 3
        w.code(u32::from(b'x'), 9);
        let (_guard, dir) = scratch("lzx-before");
        let path = dir.join("m.lzx");
        one_member(&path, b"\0\0\0x", &w.finish());
        assert_eq!(
            LzxBackend::open(&path).unwrap().read(0, 16).unwrap(),
            b"\0\0\0x"
        );

        // A match starting one byte before the data and running into it.
        let mut w = BitWriter::new();
        w.put(2, 3);
        put_len(&mut w, 6);
        nine_bit_table(&mut w);
        w.code(u32::from(b'a'), 9);
        w.code(u32::from(b'b'), 9);
        w.code(256 + 3 + (1 << 5), 9); // slot 3: offset 3; length slot 1: length 4
        let path = dir.join("n.lzx");
        one_member(&path, b"ab\0ab\0", &w.finish());
        assert_eq!(
            LzxBackend::open(&path).unwrap().read(0, 16).unwrap(),
            b"ab\0ab\0"
        );
    }

    #[test]
    fn a_stream_that_ends_early_is_refused() {
        let mut w = BitWriter::new();
        w.put(2, 3);
        put_len(&mut w, 10);
        nine_bit_table(&mut w);
        w.code(u32::from(b'a'), 9);
        let (_guard, dir) = scratch("lzx-short");
        let path = dir.join("s.lzx");
        one_member(&path, b"aaaaaaaaaa", &w.finish());
        let err = LzxBackend::open(&path).unwrap().read(0, 16).unwrap_err();
        assert!(
            err.to_string().contains("ends before its data does"),
            "{err}"
        );
    }

    #[test]
    fn an_incomplete_pretree_is_refused() {
        let mut w = BitWriter::new();
        w.put(2, 3);
        put_len(&mut w, 1);
        for s in 0..20 {
            w.put(if s == 0 { 1 } else { 0 }, 4); // one 1-bit code: Kraft ½
        }
        w.put(0, 16);
        let (_guard, dir) = scratch("lzx-incomplete");
        let path = dir.join("i.lzx");
        one_member(&path, b"x", &w.finish());
        let err = LzxBackend::open(&path).unwrap().read(0, 16).unwrap_err();
        assert!(err.to_string().contains("incomplete"), "{err}");
    }

    /// Real LZX archives, read and unpacked through the product's own gate.
    /// `#[ignore]`d and env-gated: ART ships no copyrighted content.
    ///
    /// ```text
    /// cd src-tauri && ART_LZX_DIR="E:\amiga\Amigatolon\paketler" \
    ///   ART_LZX_OUT="E:\amiga\ProjeART\build\tmp\card-r2\lzx-oracle\art" \
    ///   cargo test --lib read_the_owners_lzx_archives_when_asked -- --ignored --nocapture
    /// ```
    ///
    /// Every `*.lzx` directly in `ART_LZX_DIR` is unpacked into
    /// `ART_LZX_OUT/<stem>` (a scratch folder when `ART_LZX_OUT` is unset),
    /// which must not already hold anything: a file skipped because it exists
    /// would read as a member not written. The bytes are compared with an
    /// independent decoder by `scripts/lzx-oracle-check.py`; this hook proves
    /// only that every member decodes, passes the CRC the archiver stored,
    /// and is written.
    #[test]
    #[ignore]
    fn read_the_owners_lzx_archives_when_asked() {
        let Ok(dir) = std::env::var("ART_LZX_DIR") else {
            eprintln!("ART_LZX_DIR unset — skipping");
            return;
        };
        let (_guard, scratch_out) = scratch("owners-lzx");
        let out_root = std::env::var("ART_LZX_OUT")
            .map(PathBuf::from)
            .unwrap_or(scratch_out);

        let mut archives: Vec<PathBuf> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.is_file()
                    && p.extension()
                        .is_some_and(|x| x.to_string_lossy().eq_ignore_ascii_case("lzx"))
            })
            .collect();
        archives.sort();
        assert!(!archives.is_empty(), "no .lzx directly in {dir}");

        for path in &archives {
            let file = path.file_name().unwrap().to_string_lossy().into_owned();
            let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
            let dest = out_root.join(&stem);
            if let Ok(mut existing) = std::fs::read_dir(&dest) {
                assert!(
                    existing.next().is_none(),
                    "{} already holds files; give ART_LZX_OUT an empty folder",
                    dest.display()
                );
            }

            let mut backend = LzxBackend::open(path).unwrap();
            let entries = backend.entries().unwrap();
            let groups = backend.groups.len();
            let non_ascii = entries.iter().filter(|e| !e.name.is_ascii()).count();
            let comments = entries.iter().filter(|e| e.amiga.comment.is_some()).count();
            let odd_streams = backend.groups.iter().filter(|g| g.packed % 2 == 1).count();

            // The largest group, decoded alone and timed: the reader takes a
            // bit at a time, and this is where that would show.
            let largest = backend
                .groups
                .iter()
                .max_by_key(|g| {
                    g.members
                        .clone()
                        .map(|i| backend.records[i].entry.declared_bytes)
                        .sum::<u64>()
                })
                .cloned()
                .unwrap();
            let largest_bytes: u64 = largest
                .members
                .clone()
                .map(|i| entries[i].declared_bytes)
                .sum();
            let mut wanted = vec![false; entries.len()];
            for i in largest.members.clone() {
                wanted[i] = true;
            }
            let mut largest_ok = 0usize;
            let started = std::time::Instant::now();
            backend
                .read_selected(&wanted, MAX_ENTRY_OUTPUT, &mut |i, data| {
                    match data {
                        Ok(_) => largest_ok += 1,
                        Err(e) => eprintln!("  largest group: {}: {e}", entries[i].name),
                    }
                    Ok(())
                })
                .unwrap();
            let largest_time = started.elapsed();

            let started = std::time::Instant::now();
            let outcome =
                extract_with_backend(&mut backend, &dest, OverwritePolicy::Skip, &NoProgress)
                    .unwrap();
            let whole_time = started.elapsed();
            for error in outcome.errors.iter() {
                eprintln!("  error: {error}");
            }
            let errors = outcome.errors.len() + usize::from(outcome.aborted);
            let written = outcome.total_files;
            println!(
                "{file}: entries {} groups {groups} non-ascii {non_ascii} comments {comments} \
                 written {written} errors {errors}",
                entries.len()
            );
            println!(
                "  largest group: {} members, {largest_bytes} bytes, method {}, decoded {largest_ok} \
                 in {:.2} s; whole archive written in {:.2} s; odd-length streams {odd_streams}",
                largest.members.len(),
                largest.method,
                largest_time.as_secs_f64(),
                whole_time.as_secs_f64(),
            );
            assert_eq!(errors, 0, "{file}: {:?}", outcome.abort_reason);
            assert_eq!(written, entries.len(), "{file}");
            assert_eq!(largest_ok, largest.members.len(), "{file}");
        }
    }
}
