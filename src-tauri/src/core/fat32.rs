//! The FAT32 boot partition of a PiStorm card (SD-1 · G2).
//!
//! The one filesystem ART **writes** that is not an Amiga one. A PiStorm card
//! boots the Raspberry Pi first: its firmware reads a FAT32 partition for the
//! Emu68 kernel, a Kickstart image, `config.txt` and `cmdline.txt`, and only
//! then does anything Amiga-shaped happen. So a card ART builds has to carry a
//! real FAT32 filesystem, made by something that has been read by other
//! people's firmware — which is why this is `fatfs` and not a formatter of
//! ART's own (see `Cargo.toml` for that decision).
//!
//! ## Everything here is bounded to the partition
//!
//! The partition is a *region inside a much larger file*, and the Amiga's own
//! disks begin immediately after it. A formatter that ran past its end would
//! write over the first area's RDB — the one structure the whole card depends
//! on — so nothing here is handed the file. [`Region`] is: it maps offset zero
//! to the partition's first byte and refuses, rather than truncates, anything
//! that would reach past its last. That refusal is a test of its own.

use std::io::{self, Read, Seek, SeekFrom, Write};

use crate::core::error::{CoreError, CoreResult};

/// A bounded window onto something bigger.
///
/// `Read`, `Write` and `Seek` over `[start, start + len)` of an underlying
/// file, with offsets relative to `start`. Everything the FAT32 writer does
/// goes through this, so "the formatter cannot escape its partition" is a
/// property of the type rather than of anybody's care.
pub struct Region<T> {
    inner: T,
    start: u64,
    len: u64,
    /// Where the caller thinks it is, relative to `start`.
    pos: u64,
}

impl<T: Read + Write + Seek> Region<T> {
    pub fn new(inner: T, start: u64, len: u64) -> Self {
        Self {
            inner,
            start,
            len,
            pos: 0,
        }
    }

    /// How long the window is, in bytes.
    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn seek_inner(&mut self) -> io::Result<()> {
        self.inner.seek(SeekFrom::Start(self.start + self.pos))?;
        Ok(())
    }
}

impl<T: Read + Write + Seek> Read for Region<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // A read past the end is short rather than an error: that is what a
        // reader expects at the end of a file, and FAT32's own structures never
        // ask for more than they wrote.
        let left = self.len.saturating_sub(self.pos);
        if left == 0 {
            return Ok(0);
        }
        let take = buf.len().min(left as usize);
        self.seek_inner()?;
        let read = self.inner.read(&mut buf[..take])?;
        self.pos += read as u64;
        Ok(read)
    }
}

impl<T: Read + Write + Seek> Write for Region<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // A write past the end is **refused**, not shortened. A short write
        // would leave the filesystem's own bookkeeping describing bytes that
        // never landed, and the bytes past the end belong to the Amiga's first
        // disk.
        if self.pos.saturating_add(buf.len() as u64) > self.len {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                format!(
                    "a write of {} bytes at {} would run past the end of a {}-byte partition",
                    buf.len(),
                    self.pos,
                    self.len
                ),
            ));
        }
        self.seek_inner()?;
        let written = self.inner.write(buf)?;
        self.pos += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<T: Read + Write + Seek> Seek for Region<T> {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let next = match to {
            SeekFrom::Start(n) => n as i64,
            SeekFrom::Current(n) => self.pos as i64 + n,
            SeekFrom::End(n) => self.len as i64 + n,
        };
        if next < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot seek before the start of the partition",
            ));
        }
        // Seeking past the end is allowed and writing there is not — the same
        // shape every file has, and the check that matters is on the write.
        self.pos = next as u64;
        Ok(self.pos)
    }
}

/// A file to put on the boot partition.
#[derive(Debug, Clone)]
pub struct BootFile {
    /// The name as it will appear — `Emu68.img`, `config_caffeineos.txt`.
    pub name: String,
    pub bytes: Vec<u8>,
}

/// The volume label a freshly built card carries.
///
/// `NO NAME` is what both real cards have, being what every tool writes when
/// nobody says otherwise. ART says otherwise: a card with ART's name on it is
/// one whose origin is obvious months later, when it is one of four on a desk.
pub const DEFAULT_LABEL: &str = "ART CARD";

/// Sectors per cluster, matching both real cards: 8 × 512 = 4 KiB.
const SECTORS_PER_CLUSTER: u8 = 8;

/// Create the FAT32 filesystem and put `files` in its root.
///
/// `start` and `len` describe the partition inside `image` — the same numbers
/// `core::mbr::plan_card` put in the partition table, and the caller is
/// expected to pass exactly those rather than work them out a second way.
///
/// A file's name may be a **relative path** — `overlays/emu68.dtbo` — and the
/// directories in it are created as needed. That is not a convenience: the
/// Emu68 release carries an `overlays/` folder, and a real card's boot
/// partition has eighteen of them. Every path goes through [`checked_path`],
/// which refuses what `safe_join` refuses for an archive entry.
pub fn create_boot_partition<T: Read + Write + Seek>(
    image: T,
    start: u64,
    len: u64,
    label: &str,
    files: &[BootFile],
) -> CoreResult<()> {
    let mut region = Region::new(image, start, len);

    let options = fatfs::FormatVolumeOptions::new()
        // Forced rather than inferred. `fatfs` picks a FAT width from the
        // partition's size, and the Pi's firmware wants FAT32 — a card that
        // came out FAT16 because it was small would be a card that does not
        // boot, discovered on the Amiga rather than here.
        .fat_type(fatfs::FatType::Fat32)
        .bytes_per_cluster(SECTORS_PER_CLUSTER as u32 * 512)
        .volume_label(volume_label(label));

    fatfs::format_volume(&mut region, options).map_err(|err| CoreError::Malformed {
        format: "FAT32".into(),
        detail: format!("the boot partition could not be created: {err}"),
    })?;

    let fs = fatfs::FileSystem::new(&mut region, fatfs::FsOptions::new()).map_err(|err| {
        CoreError::Malformed {
            format: "FAT32".into(),
            detail: format!("the boot partition was created and cannot be opened: {err}"),
        }
    })?;

    {
        let root = fs.root_dir();
        for file in files {
            let name = checked_path(&file.name)?;

            // Parents first: `fatfs` will not create a file in a directory
            // that is not there, and `Emu68-pistorm.zip`'s `overlays/` is a
            // directory before it is a file. `create_dir` opens an existing
            // one rather than failing, so several files in the same folder
            // cost nothing extra.
            if let Some((parents, _)) = name.rsplit_once('/') {
                let mut walked = String::new();
                for part in parents.split('/') {
                    if !walked.is_empty() {
                        walked.push('/');
                    }
                    walked.push_str(part);
                    root.create_dir(&walked)
                        .map_err(|err| CoreError::Malformed {
                            format: "FAT32".into(),
                            detail: format!(
                                "'{walked}' could not be created on the boot partition: {err}"
                            ),
                        })?;
                }
            }

            let mut handle = root
                .create_file(&name)
                .map_err(|err| CoreError::Malformed {
                    format: "FAT32".into(),
                    detail: format!("'{name}' could not be created on the boot partition: {err}"),
                })?;
            handle
                .write_all(&file.bytes)
                .and_then(|()| handle.flush())
                .map_err(|err| CoreError::Malformed {
                    format: "FAT32".into(),
                    detail: format!("'{name}' could not be written: {err}"),
                })?;
        }
    }

    fs.unmount().map_err(|err| CoreError::Malformed {
        format: "FAT32".into(),
        detail: format!("the boot partition could not be closed cleanly: {err}"),
    })?;

    // Two defects `fatfs` 0.3.6 leaves in every directory it creates. See
    // `repair_directories` — they were found by an independent reader, which
    // is the only way a writer's own mistakes ever are.
    repair_directories(&mut region)?;

    Ok(())
}

/// A relative path on the boot partition, checked the way an archive entry is.
///
/// **The boot partition is not flat, and finding that out was worth the
/// trouble.** `Emu68-pistorm.zip` carries an `overlays/` directory, and the
/// FAT32 of a real CaffeineOS card holds eighteen folders nested four deep. A
/// writer that only placed root-level names could not lay down the Emu68
/// payload at all.
///
/// So a path here may carry `/`, and everything that makes one dangerous is
/// refused on the same grounds `core::security::safe_join` refuses it for an
/// archive entry: absolute paths, `..`, drive letters, empty components. The
/// only difference is that there is no host filesystem to join against — the
/// question is the same one.
///
/// Returns the path with separators normalised to `/`, which is what `fatfs`
/// takes.
fn checked_path(name: &str) -> CoreResult<String> {
    let refused = |why: &str| {
        CoreError::SafetyRefused(format!("'{name}' cannot go on the boot partition: {why}"))
    };

    if name.is_empty() {
        return Err(refused("it has no name"));
    }
    if name.contains(':') {
        return Err(refused("a path cannot carry a drive or a stream"));
    }
    if name.chars().any(|c| c.is_control()) {
        return Err(refused("it contains a control character"));
    }

    let normalised = name.replace('\\', "/");
    if normalised.starts_with('/') {
        return Err(refused("it is an absolute path"));
    }
    for part in normalised.split('/') {
        if part.is_empty() {
            return Err(refused("it has an empty part"));
        }
        if part == "." || part == ".." {
            return Err(refused("it walks the directory tree"));
        }
    }

    Ok(normalised)
}

// ---------------------------------------------------------------------------
// Repairing what `fatfs` 0.3.6 gets wrong about a directory
// ---------------------------------------------------------------------------

/// Two defects in every directory `fatfs` 0.3.6 creates, found by pointing
/// 7-Zip at what ART wrote (`scripts/fat-oracle-check.py`).
///
/// 1. **`.` and `..` are given long-filename entries.** They are 8.3 names by
///    definition and must never carry one. This is what 7-Zip reports as
///    `Headers Error`, and it was isolated by deleting exactly those two
///    entries in a copy of the image and watching the complaint go away.
/// 2. **`..` points at the root's own cluster** — 2 — where the format says a
///    directory whose parent is the root must write **0**. 7-Zip does not
///    check this one; the specification is still the specification.
///
/// Neither stops `fatfs` reading its own output, which is exactly the shape of
/// ART-032..035: a writer and a reader that agree with each other and with
/// nothing else. The card's boot partition is read by the Raspberry Pi's
/// firmware, which nobody here can interrogate, so a defect a strict reader
/// objects to is not one to ship and hope about.
///
/// This runs after the filesystem is closed and touches nothing but the two
/// entries at the front of each directory.
fn repair_directories<T: Read + Write + Seek>(region: &mut Region<T>) -> CoreResult<()> {
    let bpb = Bpb::read(region)?;

    // Directories to visit, starting with the root. Bounded: a card's boot
    // partition holds tens of folders, and a chain that loops must not become
    // an infinite walk (the same rule every chain walk in ART follows).
    const MAX_DIRECTORIES: usize = 4096;
    let mut queue = vec![bpb.root_cluster];
    let mut visited = 0usize;

    while let Some(first_cluster) = queue.pop() {
        visited += 1;
        if visited > MAX_DIRECTORIES {
            return Err(CoreError::Malformed {
                format: "FAT32".into(),
                detail: "this boot partition has more directories than ART will walk".into(),
            });
        }

        for cluster in bpb.chain(region, first_cluster)? {
            let offset = bpb.cluster_offset(cluster);
            let mut bytes = vec![0u8; bpb.cluster_bytes()];
            region.seek(SeekFrom::Start(offset))?;
            region.read_exact(&mut bytes)?;

            let mut changed = false;
            let entries = bytes.len() / DIR_ENTRY_BYTES;
            for index in 0..entries {
                let at = index * DIR_ENTRY_BYTES;
                // Copied rather than borrowed: the fixes below write into
                // `bytes`, and one cannot hold a slice of it while doing so.
                let mut entry = [0u8; DIR_ENTRY_BYTES];
                entry.copy_from_slice(&bytes[at..at + DIR_ENTRY_BYTES]);

                if entry[0] == 0x00 {
                    break; // no entry here or after it
                }
                if entry[0] == 0xE5 || entry[ATTR] == ATTR_LFN {
                    continue;
                }

                let is_dot = &entry[..11] == b".          ";
                let is_dotdot = &entry[..11] == b"..         ";

                if entry[ATTR] & ATTR_DIRECTORY != 0 && !is_dot && !is_dotdot {
                    // A directory of its own. Its `.` and `..` need the same
                    // repair, however deep the payload nests.
                    queue.push(entry_cluster(&entry));
                }

                if !(is_dot || is_dotdot) {
                    continue;
                }

                // Defect 1: mark every long-filename entry immediately before
                // this one as deleted. `0xE5` in the first byte is what FAT
                // has always meant by "ignore this slot".
                let mut before = index;
                while before > 0 {
                    let prev = (before - 1) * DIR_ENTRY_BYTES;
                    if bytes[prev + ATTR] != ATTR_LFN || bytes[prev] == 0xE5 {
                        break;
                    }
                    bytes[prev] = 0xE5;
                    changed = true;
                    before -= 1;
                }

                // Defect 2: `..` in a directory whose parent is the root.
                if is_dotdot && entry_cluster(&entry) == bpb.root_cluster {
                    bytes[at + 20..at + 22].copy_from_slice(&0u16.to_le_bytes());
                    bytes[at + 26..at + 28].copy_from_slice(&0u16.to_le_bytes());
                    changed = true;
                }
            }

            if changed {
                region.seek(SeekFrom::Start(offset))?;
                region.write_all(&bytes)?;
            }
        }
    }

    region.flush()?;
    Ok(())
}

/// Offset of the attribute byte in a directory entry.
const ATTR: usize = 11;
const ATTR_LFN: u8 = 0x0F;
const ATTR_DIRECTORY: u8 = 0x10;
const DIR_ENTRY_BYTES: usize = 32;

/// The cluster a directory entry points at: high word at 20, low word at 26.
fn entry_cluster(entry: &[u8]) -> u32 {
    let high = u16::from_le_bytes([entry[20], entry[21]]) as u32;
    let low = u16::from_le_bytes([entry[26], entry[27]]) as u32;
    (high << 16) | low
}

/// The handful of BPB fields needed to find a cluster.
struct Bpb {
    bytes_per_sector: u32,
    sectors_per_cluster: u32,
    data_start: u64,
    fat_start: u64,
    root_cluster: u32,
    total_clusters: u32,
}

impl Bpb {
    fn read<T: Read + Write + Seek>(region: &mut Region<T>) -> CoreResult<Self> {
        let mut boot = [0u8; 512];
        region.seek(SeekFrom::Start(0))?;
        region.read_exact(&mut boot)?;

        let malformed = |what: &str| CoreError::Malformed {
            format: "FAT32".into(),
            detail: format!("the boot partition ART just wrote has {what}"),
        };

        let bytes_per_sector = u16::from_le_bytes([boot[11], boot[12]]) as u32;
        let sectors_per_cluster = boot[13] as u32;
        let reserved = u16::from_le_bytes([boot[14], boot[15]]) as u32;
        let fats = boot[16] as u32;
        let fat_size = u32::from_le_bytes([boot[36], boot[37], boot[38], boot[39]]);
        let total_sectors = u32::from_le_bytes([boot[32], boot[33], boot[34], boot[35]]);
        let root_cluster = u32::from_le_bytes([boot[44], boot[45], boot[46], boot[47]]);

        if bytes_per_sector == 0 || sectors_per_cluster == 0 || fats == 0 || fat_size == 0 {
            return Err(malformed("no geometry"));
        }

        let fat_start = reserved as u64 * bytes_per_sector as u64;
        let first_data_sector = reserved + fats * fat_size;
        let data_start = first_data_sector as u64 * bytes_per_sector as u64;
        let total_clusters = total_sectors
            .saturating_sub(first_data_sector)
            .checked_div(sectors_per_cluster)
            .ok_or_else(|| malformed("no clusters"))?;

        Ok(Self {
            bytes_per_sector,
            sectors_per_cluster,
            data_start,
            fat_start,
            root_cluster,
            total_clusters,
        })
    }

    fn cluster_bytes(&self) -> usize {
        (self.bytes_per_sector * self.sectors_per_cluster) as usize
    }

    fn cluster_offset(&self, cluster: u32) -> u64 {
        self.data_start + (cluster as u64 - 2) * self.cluster_bytes() as u64
    }

    /// Every cluster of one chain, bounded by the number the volume has.
    fn chain<T: Read + Write + Seek>(
        &self,
        region: &mut Region<T>,
        first: u32,
    ) -> CoreResult<Vec<u32>> {
        let mut out = Vec::new();
        let mut cluster = first;

        // Below 2 is reserved; 0x0FFFFFF8 and up is the end-of-chain marker.
        while (2..0x0FFF_FFF8).contains(&cluster) {
            if out.len() as u32 > self.total_clusters {
                return Err(CoreError::Malformed {
                    format: "FAT32".into(),
                    detail: "a directory's cluster chain loops".into(),
                });
            }
            out.push(cluster);

            let mut next = [0u8; 4];
            region.seek(SeekFrom::Start(self.fat_start + cluster as u64 * 4))?;
            region.read_exact(&mut next)?;
            cluster = u32::from_le_bytes(next) & 0x0FFF_FFFF;
        }

        Ok(out)
    }
}

/// A FAT volume label is eleven bytes, space-padded, upper case.
fn volume_label(label: &str) -> [u8; 11] {
    let mut out = [b' '; 11];
    for (slot, byte) in out.iter_mut().zip(
        label
            .bytes()
            .map(|b| b.to_ascii_uppercase())
            .filter(|b| b.is_ascii_graphic() || *b == b' '),
    ) {
        *slot = byte;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// FAT32 needs 65 525 clusters before it is FAT32 at all. At 4 KiB each
    /// that is a 256 MiB floor, so every fixture here is above it — a smaller
    /// one would silently be a different filesystem and prove nothing about
    /// the card.
    const PARTITION: u64 = 300 * 1024 * 1024;
    /// The image is bigger than the partition, and the extra is what the test
    /// watches: the Amiga's first disk lives there.
    const IMAGE: u64 = 400 * 1024 * 1024;
    const START: u64 = 1024 * 1024;

    fn blank_image() -> Cursor<Vec<u8>> {
        // 0xA5 rather than zeroes: an untouched byte is then visibly untouched
        // rather than indistinguishable from something written as zero.
        Cursor::new(vec![0xA5; IMAGE as usize])
    }

    fn file(name: &str, contents: &[u8]) -> BootFile {
        BootFile {
            name: name.into(),
            bytes: contents.to_vec(),
        }
    }

    #[test]
    fn a_card_boots_from_what_is_written_here() {
        let mut image = blank_image();
        create_boot_partition(
            &mut image,
            START,
            PARTITION,
            DEFAULT_LABEL,
            &[
                file("Emu68.img", b"kernel bytes"),
                file("config.txt", b"initramfs kick.rom\n"),
                file("cmdline.txt", b"sd.unit0=ro\n"),
            ],
        )
        .unwrap();

        // Read it back the way the Pi would: mount the partition, list it.
        let mut region = Region::new(&mut image, START, PARTITION);
        let fs = fatfs::FileSystem::new(&mut region, fatfs::FsOptions::new()).unwrap();
        assert_eq!(fs.fat_type(), fatfs::FatType::Fat32, "the Pi wants FAT32");

        let names: Vec<String> = fs
            .root_dir()
            .iter()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert!(names.contains(&"Emu68.img".to_string()), "{names:?}");
        assert!(names.contains(&"config.txt".to_string()), "{names:?}");
        assert!(names.contains(&"cmdline.txt".to_string()), "{names:?}");

        let mut read = Vec::new();
        fs.root_dir()
            .open_file("config.txt")
            .unwrap()
            .read_to_end(&mut read)
            .unwrap();
        assert_eq!(read, b"initramfs kick.rom\n");
    }

    /// **The property the whole module is arranged around.** The Amiga's first
    /// disk begins where this partition ends, and its RDB is the first thing
    /// in it. Not one byte outside the partition may move.
    #[test]
    fn nothing_outside_the_partition_is_touched() {
        let mut image = blank_image();
        create_boot_partition(
            &mut image,
            START,
            PARTITION,
            DEFAULT_LABEL,
            &[file("Emu68.img", &vec![7u8; 64 * 1024])],
        )
        .unwrap();

        let bytes = image.into_inner();
        assert!(
            bytes[..START as usize].iter().all(|&b| b == 0xA5),
            "the partition table's own sector is before this partition"
        );
        assert!(
            bytes[(START + PARTITION) as usize..]
                .iter()
                .all(|&b| b == 0xA5),
            "the Amiga's first disk starts here, and its RDB is the first block of it"
        );
    }

    /// A payload larger than the partition is refused rather than written up
    /// to the edge and stopped. A truncated kernel is a card that does not
    /// boot and says nothing about why.
    #[test]
    fn a_file_too_big_for_the_partition_is_refused() {
        let mut image = blank_image();
        let err = create_boot_partition(
            &mut image,
            START,
            PARTITION,
            DEFAULT_LABEL,
            &[file("Emu68.img", &vec![7u8; (PARTITION + 1) as usize])],
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("Emu68.img"),
            "the refusal has to name the file: {err}"
        );
    }

    /// Long names are why `alloc` is on: a multiboot card is a folder full of
    /// `config_<distro>.txt`, and 8.3 would mangle every one of them.
    #[test]
    fn a_long_name_survives_as_itself() {
        let mut image = blank_image();
        create_boot_partition(
            &mut image,
            START,
            PARTITION,
            DEFAULT_LABEL,
            &[file("config_caffeineos.txt", b"x")],
        )
        .unwrap();

        let mut region = Region::new(&mut image, START, PARTITION);
        let fs = fatfs::FileSystem::new(&mut region, fatfs::FsOptions::new()).unwrap();
        let names: Vec<String> = fs
            .root_dir()
            .iter()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert!(
            names.contains(&"config_caffeineos.txt".to_string()),
            "{names:?}"
        );
    }

    /// **The Emu68 payload is not flat**, and this is what made that clear:
    /// `Emu68-pistorm.zip` carries `overlays/emu68.dtbo`, and a real card's
    /// boot partition has eighteen folders in it. A writer that could only
    /// place root-level names could not lay the payload down at all.
    #[test]
    fn a_file_in_a_subdirectory_lands_in_it() {
        let mut image = blank_image();
        create_boot_partition(
            &mut image,
            START,
            PARTITION,
            DEFAULT_LABEL,
            &[
                file("overlays/emu68.dtbo", b"overlay"),
                // A second file in the same folder, and one nested deeper: the
                // folder is opened rather than created twice, and every level
                // of a new path is made.
                file("overlays/unicam.dtbo", b"another"),
                file("USER/WinUAE/Configurations/card.uae", b"deep"),
            ],
        )
        .unwrap();

        let mut region = Region::new(&mut image, START, PARTITION);
        let fs = fatfs::FileSystem::new(&mut region, fatfs::FsOptions::new()).unwrap();

        let mut read = Vec::new();
        fs.root_dir()
            .open_file("overlays/emu68.dtbo")
            .unwrap()
            .read_to_end(&mut read)
            .unwrap();
        assert_eq!(read, b"overlay");

        read.clear();
        fs.root_dir()
            .open_file("USER/WinUAE/Configurations/card.uae")
            .unwrap()
            .read_to_end(&mut read)
            .unwrap();
        assert_eq!(read, b"deep");

        let overlays: Vec<String> = fs
            .root_dir()
            .open_dir("overlays")
            .unwrap()
            .iter()
            .map(|entry| entry.unwrap().file_name())
            .filter(|name| name != "." && name != "..")
            .collect();
        assert_eq!(overlays.len(), 2, "{overlays:?}");
    }

    /// The two defects `fatfs` 0.3.6 leaves in every directory, checked in the
    /// bytes rather than through a reader that shares them.
    ///
    /// Found by pointing 7-Zip at what ART wrote: it reported `Headers Error`
    /// on any image with a folder in it, and deleting exactly the two
    /// long-filename entries in a copy made the complaint go away. The `..`
    /// cluster is the second defect and 7-Zip does not check it — the format
    /// is still the format.
    #[test]
    fn a_directory_is_written_the_way_the_format_says() {
        let mut image = blank_image();
        create_boot_partition(
            &mut image,
            START,
            PARTITION,
            DEFAULT_LABEL,
            &[BootFile {
                name: "overlays/emu68.dtbo".into(),
                bytes: b"overlay".to_vec(),
            }],
        )
        .unwrap();

        let bytes = image.into_inner();
        let fat = &bytes[START as usize..];

        // Walk to the directory the way `repair_directories` does.
        let bps = u16::from_le_bytes([fat[11], fat[12]]) as usize;
        let spc = fat[13] as usize;
        let reserved = u16::from_le_bytes([fat[14], fat[15]]) as usize;
        let fats = fat[16] as usize;
        let fat_size = u32::from_le_bytes([fat[36], fat[37], fat[38], fat[39]]) as usize;
        let root_cluster = u32::from_le_bytes([fat[44], fat[45], fat[46], fat[47]]);
        let data = (reserved + fats * fat_size) * bps;
        let cluster_at = |n: u32| data + (n as usize - 2) * bps * spc;

        // Find the folder in the root.
        let root = &fat[cluster_at(root_cluster)..][..bps * spc];
        let mut folder = None;
        for chunk in root.chunks_exact(32) {
            if chunk[0] == 0 {
                break;
            }
            if chunk[11] & 0x10 != 0 && chunk[11] != 0x0F && chunk[0] != 0xE5 {
                folder = Some(
                    ((u16::from_le_bytes([chunk[20], chunk[21]]) as u32) << 16)
                        | u16::from_le_bytes([chunk[26], chunk[27]]) as u32,
                );
            }
        }
        let folder = folder.expect("the folder is in the root");
        let inside = &fat[cluster_at(folder)..][..bps * spc];

        // The repair marks the spurious entries deleted (`0xE5`) and leaves the
        // slots where they are, which is what a deleted entry has always been
        // in FAT — and is exactly the edit 7-Zip accepted when it was made by
        // hand in a copy of the image.
        let mut dot = None;
        let mut dotdot = None;
        for (index, chunk) in inside.chunks_exact(32).enumerate() {
            if chunk[0] == 0 {
                break;
            }
            if chunk[0] == 0xE5 {
                continue; // a deleted slot, live to nobody
            }
            if &chunk[..11] == b".          " {
                dot = Some(index);
            }
            if &chunk[..11] == b"..         " {
                dotdot = Some(index);
            }
        }
        let dot = dot.expect("every directory has a `.`");
        let dotdot = dotdot.expect("every directory has a `..`");

        // Neither is preceded by a **live** long-filename entry.
        for entry in [dot, dotdot] {
            let previous = &inside[(entry - 1) * 32..entry * 32];
            assert!(
                previous[0] == 0xE5 || previous[11] != 0x0F,
                "`.`/`..` must carry no long-filename entry"
            );
        }

        // `..` in a directory whose parent is the root points at cluster 0.
        let at = dotdot * 32;
        let parent = ((u16::from_le_bytes([inside[at + 20], inside[at + 21]]) as u32) << 16)
            | u16::from_le_bytes([inside[at + 26], inside[at + 27]]) as u32;
        assert_eq!(
            parent, 0,
            "the root is written as 0, not as its own cluster"
        );
    }

    /// A subdirectory is legitimate; leaving the partition is not. The same
    /// question `safe_join` asks of an archive entry, asked where there is no
    /// host filesystem to join against.
    #[test]
    fn a_path_that_would_escape_the_partition_is_refused() {
        for hostile in [
            "../escape.txt",
            "sub/../../escape.txt",
            "/absolute.txt",
            "C:\\absolute.txt",
            "sub//empty.txt",
        ] {
            let mut image = blank_image();
            let err = create_boot_partition(
                &mut image,
                START,
                PARTITION,
                DEFAULT_LABEL,
                &[file(hostile, b"x")],
            )
            .unwrap_err();
            assert_eq!(err.code(), "ART-SAFETY-REFUSED", "{hostile}: {err}");
        }
    }

    #[test]
    fn the_volume_carries_arts_label() {
        let mut image = blank_image();
        create_boot_partition(&mut image, START, PARTITION, DEFAULT_LABEL, &[]).unwrap();

        let mut region = Region::new(&mut image, START, PARTITION);
        let fs = fatfs::FileSystem::new(&mut region, fatfs::FsOptions::new()).unwrap();
        assert_eq!(fs.volume_label().trim(), DEFAULT_LABEL);
    }

    /// Write a boot partition out for something that is not ART to read.
    ///
    /// ART's own tests cannot catch a mistake its writer and its reader share
    /// — the ADF oracle exists for exactly that reason (ART-032..035), and a
    /// filesystem whose entire purpose is to be read by *somebody else's*
    /// firmware deserves the same suspicion.
    ///
    /// ```text
    /// ART_FAT_OUT=F:\art-fat.img cargo test export_fat32_for_oracle_when_asked
    /// 7z l F:\art-fat.img
    /// ```
    ///
    /// The image is the partition alone, with no table around it, because that
    /// is what a FAT reader expects to be pointed at.
    #[test]
    fn export_fat32_for_oracle_when_asked() {
        let Ok(dest) = std::env::var("ART_FAT_OUT") else {
            return;
        };

        // `ART_FAT_MB` sizes the partition. The default is the small one;
        // 1150 is the 1.10 GiB a real card carries, which is the size worth
        // pointing an independent reader at.
        let size = std::env::var("ART_FAT_MB")
            .ok()
            .and_then(|mb| mb.parse::<u64>().ok())
            .map(|mb| mb * 1024 * 1024)
            .unwrap_or(PARTITION);

        let mut image = Cursor::new(vec![0u8; size as usize]);
        create_boot_partition(
            &mut image,
            0,
            size,
            DEFAULT_LABEL,
            &[
                file("Emu68.img", b"kernel bytes, for the oracle"),
                file("config.txt", b"initramfs kick.rom\narm_64bit=1\n"),
                file("cmdline.txt", b"sd.unit0=ro\n"),
                file("config_caffeineos.txt", b"a long name, for the oracle\n"),
                // A folder, because the Emu68 payload has one and because a
                // directory is the part of FAT a reader is most likely to
                // disagree about.
                file("overlays/emu68.dtbo", b"an overlay, in a folder"),
            ],
        )
        .unwrap();

        std::fs::write(dest, image.into_inner()).unwrap();
    }

    // ---- the window itself ----

    #[test]
    fn a_region_refuses_a_write_that_would_leave_it() {
        let mut backing = Cursor::new(vec![0u8; 100]);
        let mut region = Region::new(&mut backing, 10, 20);

        region.seek(SeekFrom::Start(18)).unwrap();
        assert!(region.write_all(&[1, 2, 3]).is_err(), "18 + 3 > 20");

        region.seek(SeekFrom::Start(17)).unwrap();
        region.write_all(&[1, 2, 3]).unwrap();

        let bytes = backing.into_inner();
        assert_eq!(&bytes[27..30], &[1, 2, 3], "written at 10 + 17");
        assert!(bytes[30..].iter().all(|&b| b == 0), "and nowhere else");
    }

    #[test]
    fn a_region_reads_short_at_its_end_rather_than_past_it() {
        let mut backing = Cursor::new((0u8..=255).collect::<Vec<u8>>());
        let mut region = Region::new(&mut backing, 10, 5);

        let mut buf = [0u8; 8];
        let read = region.read(&mut buf).unwrap();
        assert_eq!(read, 5, "the window is five bytes long");
        assert_eq!(&buf[..5], &[10, 11, 12, 13, 14]);
        assert_eq!(region.read(&mut buf).unwrap(), 0, "and then nothing");
    }
}
