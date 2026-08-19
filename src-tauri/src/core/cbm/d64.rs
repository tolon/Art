//! Reading a Commodore disk image: the header, the directory, and a file.
//!
//! Read-only, and every number in here came out of a file somebody else wrote.
//! Three bounds do the work, and none of them is optional:
//!
//! - **A track/sector pair is never an index.** It goes through
//!   [`Geometry::offset_of`](super::geometry::Geometry::offset_of), which
//!   refuses a pair the disk does not have. `panic = "abort"` in release turns
//!   one bad index into a dead application.
//! - **Every chain has a step limit *and* a visited set.** A crafted image can
//!   point a sector at itself, or at one it has already been through. A step
//!   limit alone lets a cycle run to the limit on every file; a visited set
//!   alone lets a long enough chain run for a long time. Both, or neither
//!   works.
//! - **Entry counts are capped**, because a directory chain can claim more
//!   entries than the disk has sectors.

use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use super::geometry::{Drive, Geometry, SECTOR_SIZE};
use super::petscii;
use crate::core::error::{CoreError, CoreResult};

/// How many sectors one chain may walk before ART calls it malformed. A
/// 1581's whole surface is 3,200 sectors, so nothing legitimate comes close.
pub const MAX_CHAIN: usize = 4_096;

/// How many directory entries ART will read. A 1541 directory holds 144; the
/// cap is generous enough for the extended directories some DOSes wrote and
/// still bounded.
pub const MAX_ENTRIES: usize = 4_096;

/// What a directory entry says a file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    Del,
    Seq,
    Prg,
    Usr,
    Rel,
    /// A type byte no Commodore DOS defines, kept as itself rather than
    /// guessed at.
    Unknown(u8),
}

impl FileType {
    fn from_byte(b: u8) -> Self {
        match b & 0x0F {
            0 => Self::Del,
            1 => Self::Seq,
            2 => Self::Prg,
            3 => Self::Usr,
            4 => Self::Rel,
            other => Self::Unknown(other),
        }
    }

    /// The three-letter name a `LOAD"$"` listing shows.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Del => "DEL",
            Self::Seq => "SEQ",
            Self::Prg => "PRG",
            Self::Usr => "USR",
            Self::Rel => "REL",
            Self::Unknown(_) => "???",
        }
    }
}

/// One file in the directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CbmEntry {
    pub name: String,
    pub file_type: FileType,
    /// Where the file's first sector is.
    pub track: u8,
    pub sector: u8,
    /// Size in 254-byte blocks, as the directory claims it. A claim, not a
    /// measurement — [`D64Image::read_file`] reports what it actually read.
    pub blocks: u16,
    /// A file that was never closed (`*PRG` in a listing). Its chain may end
    /// nowhere.
    pub closed: bool,
    /// Write-protected (`<` in a listing).
    pub locked: bool,
}

/// A Commodore disk image, opened but not loaded.
#[derive(Debug, Clone)]
pub struct D64Image {
    path: PathBuf,
    geometry: Geometry,
}

impl D64Image {
    /// Open an image, deciding its shape from the file's length.
    pub fn open(path: &Path) -> CoreResult<Self> {
        let len = std::fs::metadata(path)?.len();
        let geometry = Geometry::from_len(len)?;
        Ok(Self {
            path: path.to_path_buf(),
            geometry,
        })
    }

    pub fn geometry(&self) -> Geometry {
        self.geometry
    }

    /// Read one whole sector.
    fn sector(&self, track: u8, sector: u8) -> CoreResult<[u8; SECTOR_SIZE]> {
        let offset = self.geometry.offset_of(track, sector)?;
        let mut file = File::open(&self.path)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut buf = [0u8; SECTOR_SIZE];
        file.read_exact(&mut buf)
            .map_err(|e| CoreError::Malformed {
                format: "cbm".into(),
                detail: format!(
                    "track {track} sector {sector} runs past the end of the image: {e}"
                ),
            })?;
        Ok(buf)
    }

    /// The disk's name and id, from the header sector.
    ///
    /// The 1541 keeps both in the BAM sector (18/0); the 1581 keeps them in
    /// its header sector (40/0) at the same offsets a 1541 uses for its BAM
    /// entries, which is why the offset depends on the drive rather than being
    /// one constant.
    pub fn disk_name(&self) -> CoreResult<(String, String)> {
        let (name_at, id_at) = match self.geometry.drive {
            Drive::D64 | Drive::D71 => (0x90, 0xA2),
            Drive::D81 => (0x04, 0x16),
        };
        let header = self.sector(self.geometry.directory_track(), 0)?;
        let name = petscii::decode_field(&header[name_at..name_at + 16]);
        let id = petscii::decode_field(&header[id_at..id_at + 2]);
        Ok((name, id))
    }

    /// Every file in the directory.
    ///
    /// Walks the directory chain from the drive's own starting sector. A
    /// sector that points back at one already visited ends the walk — that is
    /// a cycle, not more directory — and the walk is capped either way.
    pub fn list(&self) -> CoreResult<Vec<CbmEntry>> {
        let mut entries = Vec::new();
        let mut visited: HashSet<(u8, u8)> = HashSet::new();
        let mut next = Some((
            self.geometry.directory_track(),
            self.geometry.directory_sector(),
        ));
        let mut steps = 0usize;

        while let Some((track, sector)) = next {
            if track == 0 {
                break;
            }
            steps += 1;
            if steps > MAX_CHAIN {
                return Err(malformed("the directory chain is longer than this disk"));
            }
            if !visited.insert((track, sector)) {
                // A cycle. Everything read so far is real; stopping here is
                // what makes the listing finite.
                break;
            }

            let data = self.sector(track, sector)?;
            for slot in 0..8 {
                let at = slot * 32;
                let entry = &data[at..at + 32];
                let type_byte = entry[2];
                // A type byte of zero is an empty slot — a deleted file, or a
                // slot never used. Not an error, and not a file.
                if type_byte == 0 {
                    continue;
                }
                if entries.len() >= MAX_ENTRIES {
                    return Err(malformed(
                        "this directory claims more entries than ART reads",
                    ));
                }
                entries.push(CbmEntry {
                    name: petscii::decode_field(&entry[5..21]),
                    file_type: FileType::from_byte(type_byte),
                    track: entry[3],
                    sector: entry[4],
                    blocks: u16::from_le_bytes([entry[30], entry[31]]),
                    closed: type_byte & 0x80 != 0,
                    locked: type_byte & 0x40 != 0,
                });
            }

            // The link to the next directory sector lives in the first two
            // bytes of the sector, which are also the first two bytes of slot
            // zero — the same bytes, read for a different purpose.
            next = if data[0] == 0 {
                None
            } else {
                Some((data[0], data[1]))
            };
        }

        Ok(entries)
    }

    /// Read a file by walking its sector chain.
    ///
    /// The first two bytes of each sector are the next track and sector. A
    /// next-track of zero means this is the last one, and then the second byte
    /// is **the offset of the last used byte**, so the sector carries
    /// `second - 1` bytes of data.
    pub fn read_file(&self, entry: &CbmEntry) -> CoreResult<Vec<u8>> {
        if entry.track == 0 {
            return Err(malformed(format!(
                "'{}' does not say where its first sector is",
                entry.name
            )));
        }

        let mut out = Vec::new();
        let mut visited: HashSet<(u8, u8)> = HashSet::new();
        let mut next = Some((entry.track, entry.sector));
        let mut steps = 0usize;

        while let Some((track, sector)) = next {
            steps += 1;
            if steps > MAX_CHAIN {
                return Err(malformed(format!(
                    "'{}' is a chain longer than this disk",
                    entry.name
                )));
            }
            if !visited.insert((track, sector)) {
                return Err(malformed(format!(
                    "'{}' has a sector chain that loops back on itself",
                    entry.name
                )));
            }

            let data = self.sector(track, sector)?;
            if data[0] == 0 {
                // Last sector: byte 1 is the offset of the last used byte, so
                // the data runs from 2 up to it. A value below 2 means the
                // sector claims to hold less than nothing.
                let used = data[1] as usize;
                if used < 2 {
                    return Err(malformed(format!(
                        "'{}' ends with a sector claiming {used} used bytes",
                        entry.name
                    )));
                }
                out.extend_from_slice(&data[2..=used.min(SECTOR_SIZE - 1)]);
                break;
            }
            out.extend_from_slice(&data[2..]);
            next = Some((data[0], data[1]));
        }

        Ok(out)
    }
}

fn malformed(detail: impl Into<String>) -> CoreError {
    CoreError::Malformed {
        format: "cbm".into(),
        detail: detail.into(),
    }
}

/// Building disk images byte by byte, for the tests. ART ships no fixtures.
#[cfg(test)]
pub mod fixture {
    use super::*;

    /// A 1541 image under construction.
    pub struct D64Builder {
        pub geometry: Geometry,
        data: Vec<u8>,
    }

    impl D64Builder {
        pub fn new(tracks: u8) -> Self {
            let geometry = Geometry {
                drive: Drive::D64,
                tracks,
                error_bytes: false,
            };
            let sectors = geometry.total_sectors().unwrap() as usize;
            let mut builder = Self {
                geometry,
                data: vec![0u8; sectors * SECTOR_SIZE],
            };
            builder.write_header("TEST DISK", "01");
            builder
        }

        fn at(&mut self, track: u8, sector: u8) -> &mut [u8] {
            let offset = self.geometry.offset_of(track, sector).unwrap() as usize;
            &mut self.data[offset..offset + SECTOR_SIZE]
        }

        /// The BAM sector: a DOS version byte, the disk name and its id, all
        /// padded the way a drive pads them.
        pub fn write_header(&mut self, name: &str, id: &str) {
            let track = self.geometry.directory_track();
            let first_dir = self.geometry.directory_sector();
            let bam = self.at(track, 0);
            bam[0] = track;
            bam[1] = first_dir;
            bam[2] = b'A';
            for byte in bam[0x90..0xA0].iter_mut() {
                *byte = petscii::PAD;
            }
            for (i, b) in petscii::encode(name).into_iter().take(16).enumerate() {
                bam[0x90 + i] = b;
            }
            bam[0xA0] = petscii::PAD;
            bam[0xA1] = petscii::PAD;
            for (i, b) in petscii::encode(id).into_iter().take(2).enumerate() {
                bam[0xA2 + i] = b;
            }
        }

        /// Put a file on the disk: its data chained through `sectors`, and a
        /// directory entry pointing at the first of them.
        pub fn add_file(&mut self, name: &str, contents: &[u8], sectors: &[(u8, u8)]) {
            assert!(!sectors.is_empty(), "a file needs at least one sector");
            let needed = contents.len().div_ceil(254).max(1);
            assert_eq!(sectors.len(), needed, "{name}: wrong number of sectors");

            for (i, (track, sector)) in sectors.iter().enumerate() {
                let start = i * 254;
                let chunk = &contents[start.min(contents.len())..];
                let take = chunk.len().min(254);
                let last = i + 1 == sectors.len();
                let block = self.at(*track, *sector);
                if last {
                    block[0] = 0;
                    block[1] = (take + 1) as u8;
                } else {
                    block[0] = sectors[i + 1].0;
                    block[1] = sectors[i + 1].1;
                }
                block[2..2 + take].copy_from_slice(&chunk[..take]);
            }

            self.add_directory_entry(name, FileType::Prg, sectors[0], sectors.len() as u16);
        }

        /// A directory entry pointing wherever the caller says — including at
        /// nothing real, which is what the hostile fixtures need.
        pub fn add_directory_entry(
            &mut self,
            name: &str,
            file_type: FileType,
            first: (u8, u8),
            blocks: u16,
        ) {
            let track = self.geometry.directory_track();
            let sector = self.geometry.directory_sector();
            let type_byte = 0x80
                | match file_type {
                    FileType::Del => 0,
                    FileType::Seq => 1,
                    FileType::Prg => 2,
                    FileType::Usr => 3,
                    FileType::Rel => 4,
                    FileType::Unknown(b) => b,
                };

            let dir = self.at(track, sector);
            let slot = (0..8)
                .find(|slot| dir[slot * 32 + 2] == 0)
                .expect("the fixture directory sector is full");
            let at = slot * 32;
            dir[at + 2] = type_byte;
            dir[at + 3] = first.0;
            dir[at + 4] = first.1;
            for byte in dir[at + 5..at + 21].iter_mut() {
                *byte = petscii::PAD;
            }
            for (i, b) in petscii::encode(name).into_iter().take(16).enumerate() {
                dir[at + 5 + i] = b;
            }
            let size = blocks.to_le_bytes();
            dir[at + 30] = size[0];
            dir[at + 31] = size[1];
        }

        /// Point a directory sector at another one, for the cycle fixtures.
        pub fn link_directory(&mut self, from: (u8, u8), to: (u8, u8)) {
            let block = self.at(from.0, from.1);
            block[0] = to.0;
            block[1] = to.1;
        }

        pub fn build(self) -> Vec<u8> {
            self.data
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixture::D64Builder;
    use super::*;

    /// A fresh scratch directory per call, every time.
    ///
    /// **The counter is the whole point (ART-173).** Keying on pid plus a
    /// nanosecond timestamp is not enough: Cargo runs these tests in parallel
    /// threads of one process, `SystemTime::now()` on Windows does not
    /// advance between two calls that land in the same clock tick, and every
    /// caller writes to the same `disk.d64` inside the directory it gets. Two
    /// tests then share one file — measured at **4 failures in 40 runs** of
    /// `cargo test core::cbm::`, with
    /// `a_disk_reports_its_name_and_id` failing on
    /// `UnsupportedFormat("1000 bytes is not a Commodore disk image")`, which
    /// is `a_file_that_is_not_a_disk_image_is_refused_at_open`'s fixture read
    /// under this test's name. Same mechanism and same one-line fix as
    /// ART-164 in `core::iso`.
    fn write(bytes: &[u8]) -> (PathBuf, PathBuf) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "art-cbm-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("disk.d64");
        std::fs::write(&path, bytes).unwrap();
        (dir, path)
    }

    #[test]
    fn a_disk_reports_its_name_and_id() {
        let mut builder = D64Builder::new(35);
        builder.write_header("ELITE", "2A");
        let (d, p) = write(&builder.build());

        let image = D64Image::open(&p).unwrap();
        let (name, id) = image.disk_name().unwrap();
        assert_eq!(name, "ELITE");
        assert_eq!(id, "2A");

        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn the_directory_lists_what_is_on_the_disk() {
        let mut builder = D64Builder::new(35);
        builder.add_file("LOADER", b"hello", &[(17, 0)]);
        builder.add_file("DATA FILE", b"world", &[(17, 1)]);
        let (d, p) = write(&builder.build());

        let entries = D64Image::open(&p).unwrap().list().unwrap();
        assert_eq!(entries.len(), 2, "{entries:?}");
        assert_eq!(entries[0].name, "LOADER");
        assert_eq!(entries[0].file_type, FileType::Prg);
        assert_eq!(entries[0].file_type.as_str(), "PRG");
        assert!(entries[0].closed);
        assert_eq!(entries[1].name, "DATA FILE");

        std::fs::remove_dir_all(&d).ok();
    }

    /// The whole point of the sector chain: bytes back out in the order they
    /// went in, across more than one sector, with the last one truncated to
    /// what it actually holds.
    #[test]
    fn a_file_reads_back_byte_for_byte_across_several_sectors() {
        let contents: Vec<u8> = (0..600u32).map(|i| (i % 251) as u8).collect();
        let mut builder = D64Builder::new(35);
        builder.add_file("BIG", &contents, &[(17, 0), (17, 1), (17, 2)]);
        let (d, p) = write(&builder.build());

        let image = D64Image::open(&p).unwrap();
        let entries = image.list().unwrap();
        let read = image.read_file(&entries[0]).unwrap();

        assert_eq!(read.len(), contents.len(), "254 + 254 + 92");
        assert_eq!(read, contents);

        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_one_sector_file_reads_back_exactly_its_bytes() {
        let mut builder = D64Builder::new(35);
        builder.add_file("SMALL", b"hi", &[(17, 0)]);
        let (d, p) = write(&builder.build());

        let image = D64Image::open(&p).unwrap();
        let entries = image.list().unwrap();
        assert_eq!(image.read_file(&entries[0]).unwrap(), b"hi");

        std::fs::remove_dir_all(&d).ok();
    }

    /// A crafted image can point a sector at itself. Without the visited set
    /// this reads forever — or, with only a step limit, reads four thousand
    /// sectors before giving up, for every file on the disk.
    #[test]
    fn a_file_whose_chain_points_at_itself_is_an_error_not_a_hang() {
        let mut builder = D64Builder::new(35);
        builder.add_file("LOOP", &vec![b'x'; 300], &[(17, 0), (17, 1)]);
        // Make the second sector point back at the first.
        builder.link_directory((17, 1), (17, 0));
        let (d, p) = write(&builder.build());

        let image = D64Image::open(&p).unwrap();
        let entries = image.list().unwrap();
        let err = image.read_file(&entries[0]).unwrap_err();
        assert!(err.to_string().contains("loops back"), "{err}");

        std::fs::remove_dir_all(&d).ok();
    }

    /// The same for the *directory* chain: a sector that points back at one
    /// already read ends the listing rather than repeating it forever.
    #[test]
    fn a_directory_that_points_back_at_itself_still_lists_once() {
        let mut builder = D64Builder::new(35);
        builder.add_file("ONE", b"a", &[(17, 0)]);
        let dir = (18, builder.geometry.directory_sector());
        builder.link_directory(dir, dir);
        let (d, p) = write(&builder.build());

        let entries = D64Image::open(&p).unwrap().list().unwrap();
        assert_eq!(entries.len(), 1, "listed more than once: {entries:?}");

        std::fs::remove_dir_all(&d).ok();
    }

    /// A directory entry pointing at a track the disk does not have is an
    /// error from `offset_of`, not an index into the file.
    #[test]
    fn an_entry_pointing_outside_the_disk_is_an_error() {
        let mut builder = D64Builder::new(35);
        builder.add_directory_entry("GHOST", FileType::Prg, (99, 0), 1);
        let (d, p) = write(&builder.build());

        let image = D64Image::open(&p).unwrap();
        let entries = image.list().unwrap();
        assert_eq!(entries.len(), 1);
        let err = image.read_file(&entries[0]).unwrap_err();
        assert_eq!(err.code(), "ART-FORMAT-MALFORMED", "{err}");

        std::fs::remove_dir_all(&d).ok();
    }

    /// An entry that says its first sector is track 0 — which is not a track —
    /// is refused by name rather than read as something.
    #[test]
    fn an_entry_with_no_first_sector_is_refused() {
        let mut builder = D64Builder::new(35);
        builder.add_directory_entry("NOWHERE", FileType::Seq, (0, 0), 0);
        let (d, p) = write(&builder.build());

        let image = D64Image::open(&p).unwrap();
        let entries = image.list().unwrap();
        let err = image.read_file(&entries[0]).unwrap_err();
        assert!(err.to_string().contains("NOWHERE"), "{err}");

        std::fs::remove_dir_all(&d).ok();
    }

    /// The 40-track images amendment A3 added: the same reader, a bigger disk,
    /// and a file living on a track a 35-track image does not have.
    #[test]
    fn a_forty_track_image_reads_files_past_track_35() {
        let mut builder = D64Builder::new(40);
        builder.add_file("DEEP", b"past thirty-five", &[(38, 5)]);
        let bytes = builder.build();
        assert_eq!(bytes.len(), 196_608);

        let (d, p) = write(&bytes);
        let image = D64Image::open(&p).unwrap();
        assert_eq!(image.geometry().tracks, 40);
        let entries = image.list().unwrap();
        assert_eq!(image.read_file(&entries[0]).unwrap(), b"past thirty-five");

        std::fs::remove_dir_all(&d).ok();
    }

    /// A file type ART does not know keeps its byte rather than becoming a
    /// PRG by default.
    #[test]
    fn an_unknown_file_type_is_reported_as_unknown() {
        let mut builder = D64Builder::new(35);
        builder.add_directory_entry("ODD", FileType::Unknown(7), (17, 0), 1);
        let (d, p) = write(&builder.build());

        let entries = D64Image::open(&p).unwrap().list().unwrap();
        assert_eq!(entries[0].file_type, FileType::Unknown(7));
        assert_eq!(entries[0].file_type.as_str(), "???");

        std::fs::remove_dir_all(&d).ok();
    }

    /// Read a disk image ART did not write, and print what it made of it.
    ///
    /// The same shape `read_foreign_volume_for_oracle_when_asked` has for an
    /// Amiga volume, and for the same reason: every fixture above is built by
    /// ART's own builder from the same sector table ART's reader uses, so they
    /// can agree with each other and both be wrong — ART-032 … ART-035's
    /// shape. `scripts/make-c64-fixture.py` writes a disk from the published
    /// 1541 layout instead, and this is what points ART at it.
    ///
    /// ```text
    /// python scripts/make-c64-fixture.py test/sample.d64
    /// ART_C64_READ_IN=../test/sample.d64 cargo test read_foreign_c64_for_oracle_when_asked -- --nocapture
    /// ```
    #[test]
    fn read_foreign_c64_for_oracle_when_asked() {
        let Ok(source) = std::env::var("ART_C64_READ_IN") else {
            return;
        };
        let image = D64Image::open(std::path::Path::new(&source)).unwrap();
        let (name, id) = image.disk_name().unwrap();
        println!("disk={name}");
        println!("id={id}");
        println!("tracks={}", image.geometry().tracks);
        for entry in image.list().unwrap() {
            let data = image.read_file(&entry).unwrap();
            println!(
                "file={}|{}|{}|{}",
                entry.name,
                entry.file_type.as_str(),
                entry.blocks,
                data.len()
            );
        }
    }

    #[test]
    fn a_file_that_is_not_a_disk_image_is_refused_at_open() {
        let (d, p) = write(&vec![0u8; 1000]);
        assert!(D64Image::open(&p).is_err());
        std::fs::remove_dir_all(&d).ok();
    }
}
