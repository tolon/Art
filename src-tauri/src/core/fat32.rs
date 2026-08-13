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
/// Names go through [`bare_name`], which is **stricter than `safe_join`** on
/// purpose: that gate asks "does this stay inside the root", and `Sub/dir.txt`
/// does. Here a name is a name — the boot partition's root is flat, and a
/// separator in it would be a directory nobody asked for.
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
            let name = bare_name(&file.name)?;
            let mut handle = root.create_file(name).map_err(|err| CoreError::Malformed {
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

    Ok(())
}

/// A file name, and nothing that is secretly a path.
///
/// Stricter than `core::security::safe_join`, and for a different question.
/// That one asks whether an archive entry stays inside a root, and
/// `Sub/dir.txt` legitimately does. The boot partition's root is flat: every
/// name here comes from a list ART or the user wrote, and a separator, a drive
/// letter or a `..` in one is a mistake or an attempt, never an intention.
fn bare_name(name: &str) -> CoreResult<&str> {
    let refused = |why: &str| {
        CoreError::SafetyRefused(format!(
            "'{name}' cannot be a file on the boot partition: {why}"
        ))
    };

    if name.is_empty() {
        return Err(refused("it has no name"));
    }
    if name.contains(['/', '\\']) {
        return Err(refused(
            "a name on the boot partition cannot contain a path",
        ));
    }
    if name.contains(':') {
        return Err(refused("a name cannot carry a drive or a stream"));
    }
    if name == "." || name == ".." {
        return Err(refused("that is a directory, not a file"));
    }
    if name.chars().any(|c| c.is_control()) {
        return Err(refused("it contains a control character"));
    }
    Ok(name)
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

    /// A name is a name, not a path. Anything that would place a file
    /// somewhere the caller did not ask for is refused at the door — the same
    /// rule `safe_join` applies to archive entries.
    #[test]
    fn a_name_that_is_really_a_path_is_refused() {
        for hostile in ["../escape.txt", "sub/dir.txt", "C:\\absolute.txt"] {
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

        let mut image = Cursor::new(vec![0u8; PARTITION as usize]);
        create_boot_partition(
            &mut image,
            0,
            PARTITION,
            DEFAULT_LABEL,
            &[
                file("Emu68.img", b"kernel bytes, for the oracle"),
                file("config.txt", b"initramfs kick.rom\narm_64bit=1\n"),
                file("cmdline.txt", b"sd.unit0=ro\n"),
                file("config_caffeineos.txt", b"a long name, for the oracle\n"),
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
