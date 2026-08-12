//! T64 — a tape *archive*, despite the name.
//!
//! It has a real header and a real directory, which is what makes it
//! browsable where a TAP is not. What it does **not** have is a header worth
//! trusting: T64s were written by a generation of tools that got their own
//! counts wrong, and the file's own fields are wrong often enough that reading
//! them literally produces empty archives and truncated files out of images
//! that other tools open perfectly (plan amendment A4).
//!
//! So this reader is written the other way round: **the records are the
//! truth, the header is a hint.**
//!
//! - `used entries` is frequently `0` while records exist. ART scans the
//!   record table instead of believing the count, and says so in the listing
//!   rather than silently disagreeing with the file.
//! - `max entries` can be larger than the file, or zero. It bounds the scan
//!   only after being clamped to what the file can actually hold.
//! - **End addresses are frequently wrong**, so a declared length is not a
//!   length. Every entry's data range is clamped to the file; an end address
//!   below the start address means "work the length out from the container",
//!   not an error and never a negative length.
//!
//! ```text
//! 0x00  32  signature, e.g. "C64 tape image file"
//! 0x20   2  version
//! 0x22   2  max entries
//! 0x24   2  used entries
//! 0x26   2  unused
//! 0x28  24  container name
//! 0x40      the record table, 32 bytes per entry
//! ```
//!
//! Each record: type at 0 (1 = normal file), C64 start address at 1..3, end
//! address at 3..5, two unused, the offset of the data in the file at 8..12,
//! then a 16-byte name.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use super::petscii;
use crate::core::error::{CoreError, CoreResult};

const HEADER_LEN: usize = 0x40;
const RECORD_LEN: usize = 32;

/// How many records ART will look at, however many the header claims.
pub const MAX_RECORDS: usize = 4_096;

/// The signature every T64 starts with. Tools wrote several variants — "C64
/// tape image file", "C64S tape file", "C64 tape file" — so only the first
/// three bytes are load-bearing, and the rest is shown, not checked.
pub const T64_MAGIC: &[u8; 3] = b"C64";

/// One entry of the archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct T64Entry {
    pub name: String,
    /// Where the C64 would have loaded it.
    pub load_address: u16,
    /// Bytes of data ART can actually read for this entry, after clamping to
    /// the file. Not the declared length — see the module comment.
    pub bytes: u32,
    /// Offset of the data within the file.
    pub offset: u32,
    /// True when the entry's declared end address disagreed with what the
    /// file can hold, and ART used the container instead. Surfaced so a
    /// listing can say the archive is one of the badly written ones.
    pub length_was_repaired: bool,
}

/// A T64 archive, opened but not loaded.
#[derive(Debug, Clone)]
pub struct T64Archive {
    path: PathBuf,
    len: u64,
    container_name: String,
    entries: Vec<T64Entry>,
    /// The header said one thing about how many entries there are and the
    /// records said another. Kept for the pane to report.
    pub header_disagreed: bool,
}

impl T64Archive {
    pub fn open(path: &Path) -> CoreResult<Self> {
        let len = std::fs::metadata(path)?.len();
        if len < HEADER_LEN as u64 {
            return Err(malformed(format!(
                "{len} bytes is too short to be a T64: the header alone is {HEADER_LEN}"
            )));
        }

        let mut file = File::open(path)?;
        let mut header = [0u8; HEADER_LEN];
        file.read_exact(&mut header)?;
        if &header[0..3] != T64_MAGIC {
            return Err(CoreError::UnsupportedFormat(
                "this file does not carry a T64 signature".to_string(),
            ));
        }

        let declared_max = u16::from_le_bytes([header[0x22], header[0x23]]) as usize;
        let declared_used = u16::from_le_bytes([header[0x24], header[0x25]]) as usize;
        let container_name = petscii::decode_field(&header[0x28..0x40]);

        // What the file can actually hold, whatever the header says.
        let room = ((len as usize).saturating_sub(HEADER_LEN)) / RECORD_LEN;
        let scan = declared_max.max(declared_used).min(room).min(MAX_RECORDS);

        let mut table = vec![0u8; scan * RECORD_LEN];
        file.seek(SeekFrom::Start(HEADER_LEN as u64))?;
        file.read_exact(&mut table)?;

        let mut entries = Vec::new();
        for record in table.chunks_exact(RECORD_LEN) {
            // Type 0 is a free slot. Anything else is a used one — the
            // "normal file" type is 1, but tools wrote other values and the
            // data is still there, so the type is not a gate.
            if record[0] == 0 {
                continue;
            }

            let load = u16::from_le_bytes([record[1], record[2]]);
            let end = u16::from_le_bytes([record[3], record[4]]);
            let offset = u32::from_le_bytes([record[8], record[9], record[10], record[11]]);

            // A record whose data starts past the end of the file describes
            // nothing. Skipped rather than listed as an empty file.
            if offset as u64 >= len {
                continue;
            }

            let available = (len - offset as u64) as u32;
            let declared = end.checked_sub(load).map(u32::from);
            let (bytes, repaired) = match declared {
                // The ordinary case, and still clamped: an end address inside
                // the C64's address space says nothing about this file's size.
                Some(declared) if declared > 0 && declared <= available => (declared, false),
                // End before start, zero length, or a range past the end of
                // the file: use what the container actually has.
                _ => (available, true),
            };

            entries.push(T64Entry {
                name: petscii::decode_field(&record[16..32]),
                load_address: load,
                bytes,
                offset,
                length_was_repaired: repaired,
            });
        }

        let header_disagreed = declared_used != entries.len();
        Ok(Self {
            path: path.to_path_buf(),
            len,
            container_name,
            entries,
            header_disagreed,
        })
    }

    /// The archive's own name, from its header.
    pub fn container_name(&self) -> &str {
        &self.container_name
    }

    pub fn entries(&self) -> &[T64Entry] {
        &self.entries
    }

    /// Read one entry's bytes.
    ///
    /// Bounded by the entry's clamped length, which was already reconciled
    /// with the file at open time, so this cannot be asked for more than the
    /// file holds.
    pub fn read(&self, entry: &T64Entry) -> CoreResult<Vec<u8>> {
        if entry.offset as u64 >= self.len {
            return Err(malformed(format!(
                "'{}' starts past the end of the archive",
                entry.name
            )));
        }
        let mut file = File::open(&self.path)?;
        file.seek(SeekFrom::Start(entry.offset as u64))?;
        let mut out = vec![0u8; entry.bytes as usize];
        file.read_exact(&mut out)
            .map_err(|e| malformed(format!("'{}' could not be read whole: {e}", entry.name)))?;
        Ok(out)
    }
}

fn malformed(detail: impl Into<String>) -> CoreError {
    CoreError::Malformed {
        format: "t64".into(),
        detail: detail.into(),
    }
}

/// Building T64s byte by byte, for the tests — including the broken ones the
/// world is full of.
#[cfg(test)]
pub mod fixture {
    use super::*;

    /// One record's worth of intent, before it becomes bytes.
    pub struct Record {
        pub name: &'static str,
        pub load: u16,
        /// Written into the record as-is, however wrong it is.
        pub end: u16,
        pub data: Vec<u8>,
    }

    /// Build a T64. `used` is written into the header verbatim, so a test can
    /// say `0` while handing over real records — which is what half the T64s
    /// in circulation do.
    pub fn build(records: &[Record], used: u16) -> Vec<u8> {
        let max = records.len() as u16;
        let mut out = vec![0u8; HEADER_LEN + records.len() * RECORD_LEN];
        out[0..19].copy_from_slice(b"C64 tape image file");
        out[0x20] = 1;
        out[0x21] = 1;
        out[0x22..0x24].copy_from_slice(&max.to_le_bytes());
        out[0x24..0x26].copy_from_slice(&used.to_le_bytes());
        for byte in out[0x28..0x40].iter_mut() {
            *byte = petscii::PAD;
        }
        for (i, b) in petscii::encode("TAPE").into_iter().take(24).enumerate() {
            out[0x28 + i] = b;
        }

        // The data follows the record table; each record points at where its
        // own bytes landed.
        let mut offsets = Vec::new();
        for record in records {
            offsets.push(out.len() as u32);
            out.extend_from_slice(&record.data);
        }

        for (i, record) in records.iter().enumerate() {
            let at = HEADER_LEN + i * RECORD_LEN;
            out[at] = 1;
            out[at + 1..at + 3].copy_from_slice(&record.load.to_le_bytes());
            out[at + 3..at + 5].copy_from_slice(&record.end.to_le_bytes());
            out[at + 8..at + 12].copy_from_slice(&offsets[i].to_le_bytes());
            for byte in out[at + 16..at + 32].iter_mut() {
                *byte = petscii::PAD;
            }
            for (j, b) in petscii::encode(record.name)
                .into_iter()
                .take(16)
                .enumerate()
            {
                out[at + 16 + j] = b;
            }
        }
        out
    }

    pub fn record(name: &'static str, load: u16, data: &[u8]) -> Record {
        Record {
            name,
            load,
            end: load + data.len() as u16,
            data: data.to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixture::{build, record, Record};
    use super::*;

    fn write(bytes: &[u8]) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "art-t64-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tape.t64");
        std::fs::write(&path, bytes).unwrap();
        (dir, path)
    }

    #[test]
    fn a_well_formed_archive_lists_and_reads() {
        let bytes = build(
            &[
                record("GAME", 0x0801, b"program bytes"),
                record("DATA", 0x1000, b"more"),
            ],
            2,
        );
        let (d, p) = write(&bytes);

        let archive = T64Archive::open(&p).unwrap();
        assert_eq!(archive.container_name(), "TAPE");
        assert!(!archive.header_disagreed);
        let entries = archive.entries();
        assert_eq!(entries.len(), 2, "{entries:?}");
        assert_eq!(entries[0].name, "GAME");
        assert_eq!(entries[0].load_address, 0x0801);
        assert!(!entries[0].length_was_repaired);
        assert_eq!(archive.read(&entries[0]).unwrap(), b"program bytes");
        assert_eq!(archive.read(&entries[1]).unwrap(), b"more");

        std::fs::remove_dir_all(&d).ok();
    }

    /// Amendment A4's first case, and the common one: the header says there
    /// are no entries while the records are right there. Trusting the count
    /// gives an empty listing for an archive other tools open.
    #[test]
    fn a_used_count_of_zero_does_not_hide_the_records() {
        let bytes = build(&[record("REAL FILE", 0x0801, b"here")], 0);
        let (d, p) = write(&bytes);

        let archive = T64Archive::open(&p).unwrap();
        assert_eq!(archive.entries().len(), 1, "the record is what counts");
        assert_eq!(archive.entries()[0].name, "REAL FILE");
        assert!(
            archive.header_disagreed,
            "and the listing can say the header was wrong"
        );
        assert_eq!(archive.read(&archive.entries()[0]).unwrap(), b"here");

        std::fs::remove_dir_all(&d).ok();
    }

    /// A4's second case: an end address *below* the start address. Subtracting
    /// gives a negative length — which as unsigned arithmetic is an enormous
    /// one — so the container decides instead.
    #[test]
    fn an_end_address_before_the_start_uses_the_container_length() {
        let mut records = vec![record("BACKWARDS", 0x1000, b"eight!!!")];
        records[0].end = 0x0800;
        let bytes = build(&records, 1);
        let (d, p) = write(&bytes);

        let archive = T64Archive::open(&p).unwrap();
        let entry = &archive.entries()[0];
        assert!(entry.length_was_repaired);
        assert_eq!(entry.bytes, 8, "what the file actually holds");
        assert_eq!(archive.read(entry).unwrap(), b"eight!!!");

        std::fs::remove_dir_all(&d).ok();
    }

    /// A4's third case: a declared range that runs past the end of the file.
    /// Clamped, not refused — the data that is there is still readable.
    #[test]
    fn a_declared_range_past_the_end_of_the_file_is_clamped() {
        let mut records = vec![record("TOO BIG", 0x0801, b"only four")];
        records[0].end = 0xFFFF;
        let bytes = build(&records, 1);
        let (d, p) = write(&bytes);

        let archive = T64Archive::open(&p).unwrap();
        let entry = &archive.entries()[0];
        assert!(entry.length_was_repaired);
        assert_eq!(entry.bytes, 9);
        assert_eq!(archive.read(entry).unwrap(), b"only four");

        std::fs::remove_dir_all(&d).ok();
    }

    /// A header claiming thousands of entries in a file that holds two: the
    /// scan is bounded by the file, not by the claim.
    #[test]
    fn a_max_entry_count_larger_than_the_file_does_not_read_past_it() {
        let mut bytes = build(&[record("ONE", 0x0801, b"a")], 1);
        bytes[0x22..0x24].copy_from_slice(&60_000u16.to_le_bytes());
        let (d, p) = write(&bytes);

        let archive = T64Archive::open(&p).unwrap();
        assert_eq!(archive.entries().len(), 1);

        std::fs::remove_dir_all(&d).ok();
    }

    /// A record whose data offset is past the end of the file describes
    /// nothing at all, and is left out rather than listed as an empty file.
    #[test]
    fn a_record_pointing_past_the_end_of_the_file_is_left_out() {
        let mut bytes = build(&[record("GHOST", 0x0801, b"x")], 1);
        let at = HEADER_LEN + 8;
        bytes[at..at + 4].copy_from_slice(&999_999u32.to_le_bytes());
        let (d, p) = write(&bytes);

        let archive = T64Archive::open(&p).unwrap();
        assert!(archive.entries().is_empty(), "{:?}", archive.entries());

        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_file_that_is_not_a_t64_is_refused() {
        let (d, p) = write(&vec![0u8; 512]);
        assert!(T64Archive::open(&p).is_err());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_file_shorter_than_the_header_is_refused_rather_than_read() {
        let (d, p) = write(b"C64 tape");
        let err = T64Archive::open(&p).unwrap_err();
        assert!(err.to_string().contains("too short"), "{err}");
        std::fs::remove_dir_all(&d).ok();
    }

    /// Free slots between real records are skipped, not turned into files
    /// with empty names.
    #[test]
    fn free_slots_in_the_record_table_are_not_entries() {
        let records = vec![
            record("FIRST", 0x0801, b"one"),
            Record {
                name: "",
                load: 0,
                end: 0,
                data: Vec::new(),
            },
            record("THIRD", 0x0801, b"three"),
        ];
        let mut bytes = build(&records, 3);
        // Blank the middle record's type byte, the way a free slot is written.
        bytes[HEADER_LEN + RECORD_LEN] = 0;
        let (d, p) = write(&bytes);

        let archive = T64Archive::open(&p).unwrap();
        let names: Vec<&str> = archive.entries().iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["FIRST", "THIRD"]);

        std::fs::remove_dir_all(&d).ok();
    }
}
