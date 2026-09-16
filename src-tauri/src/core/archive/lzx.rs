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
/// compressed block ART does not read yet is not a damaged archive.
fn for_member(name: &str, error: &CoreError) -> CoreError {
    match error {
        CoreError::UnsupportedFormat(detail) => {
            CoreError::UnsupportedFormat(format!("'{name}': {detail}"))
        }
        CoreError::Malformed { detail, .. } => malformed(format!("'{name}': {detail}")),
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

/// A compressed (pack mode 2) group, decoded up to `stop_at` bytes.
fn decode_lzx<R: Read>(_packed: R, _stop_at: u64) -> CoreResult<Vec<u8>> {
    Err(CoreError::UnsupportedFormat(
        "compressed LZX blocks are not read yet".into(),
    ))
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
            let Some(last_wanted) = group.members.clone().rev().find(|i| is_wanted(*i)) else {
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
            let mut offset = 0u64;
            for i in group.members.clone() {
                let size = self.records[i].entry.declared_bytes;
                let range = offset..offset + size;
                offset += size;
                if !is_wanted(i) {
                    continue;
                }
                let name = &self.records[i].entry.name;
                let result = match &decoded {
                    Err(e) => Err(for_member(name, e)),
                    Ok(_) if size > limit => Err(CoreError::InvalidInput(format!(
                        "'{name}' is {size} bytes, past the {limit} bytes asked for"
                    ))),
                    Ok(bytes) => match bytes.get(range.start as usize..range.end as usize) {
                        None => Err(malformed(format!(
                            "'{name}' lies past the data its group decoded"
                        ))),
                        Some(slice) if crc32_ieee(slice) != self.records[i].data_crc => {
                            Err(malformed(format!("'{name}' fails its data CRC")))
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
}
