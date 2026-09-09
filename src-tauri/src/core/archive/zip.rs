//! ZIP, as a backend for the shared gate.
//!
//! The `zip` crate does the central directory and the deflate; ART does what
//! it always does with somebody else's file — decides nothing from it that it
//! can check for itself. In particular the *names* come back raw and go
//! straight to [`extract`](super::extract), which is the only thing allowed to
//! turn one into a path.
//!
//! Two things worth knowing about ZIP specifically:
//!
//! - **`mangled_name` is not used.** The crate offers a "sanitised" name, and
//!   taking it would move the traversal defence out of the gate and into a
//!   dependency, which is precisely the arrangement `core/archive` exists to
//!   avoid. `raw_name`/`name` is what ART asks for, and `safe_join` decides.
//! - **Encrypted entries are refused, not skipped quietly.** A ZIP can be
//!   partly encrypted; an entry ART cannot decrypt is reported by name so the
//!   user knows something did not arrive.
//!
//! ## A password ART was given, never one ART goes looking for
//!
//! [`ZipBackend::open_with_password`] exists for exactly one shape: an update
//! package whose payload the package's *publisher* locked and whose key two
//! MIT projects publish in their own source (Emu68 Hatcher, Emu68-Imager).
//! ART carries such a key as **recipe data** — see
//! `core::osinstall::package::Package::payload_password` — and hands it here.
//! Nothing in this module derives, guesses, searches for or brute-forces a
//! password: a wrong one is one refusal with one sentence, and that is the
//! whole of ART's behaviour when a key does not fit.
//!
//! The key is checked **at open**, not at the first read.
//! [`ZipBackend::open_with_password`] decrypts the first encrypted entry's
//! 12-byte ZipCrypto header there and then, so a mismatch is a refusal
//! before any caller has written a byte. ZipCrypto's own check is one byte
//! wide (1/256 of wrong passwords pass it), which the crate's own
//! documentation says and this module does not pretend otherwise — the
//! read that follows would then fail its CRC, which is a `Malformed`, not a
//! silent half-file.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use zip::ZipArchive;

use super::{ArchiveBackend, ArchiveEntry};
use crate::core::error::{CoreError, CoreResult};

pub struct ZipBackend {
    path: PathBuf,
    archive: ZipArchive<BufReader<File>>,
    /// The key a recipe handed ART for this archive's encrypted entries, or
    /// `None` for the ordinary case. See the module doc comment.
    password: Option<Vec<u8>>,
}

impl std::fmt::Debug for ZipBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ZipBackend")
            .field("path", &self.path)
            .finish()
    }
}

fn malformed(detail: impl Into<String>) -> CoreError {
    CoreError::Malformed {
        format: "zip".into(),
        detail: detail.into(),
    }
}

impl ZipBackend {
    pub fn open(path: &Path) -> CoreResult<Self> {
        Self::open_with_password(path, None)
    }

    /// [`open`](Self::open), with a key for the archive's encrypted entries.
    ///
    /// `None` is exactly [`open`](Self::open). `Some` also **verifies the key
    /// here**, against the first encrypted entry, so a wrong one is
    /// [`CoreError::PayloadPasswordRefused`] before the caller has done
    /// anything with the archive — see the module doc comment.
    ///
    /// An archive with a password given and **nothing encrypted in it** is
    /// not an error: `by_index_with_options` discards a password no entry
    /// needs, and a package whose publisher shipped one build locked and
    /// another in clear is a fact about the archive, not a fault.
    pub fn open_with_password(path: &Path, password: Option<&str>) -> CoreResult<Self> {
        let file = BufReader::new(File::open(path)?);
        let archive = ZipArchive::new(file)
            .map_err(|e| malformed(format!("failed to read the ZIP directory: {e}")))?;
        let mut backend = Self {
            path: path.to_path_buf(),
            archive,
            password: password.map(|p| p.as_bytes().to_vec()),
        };
        if password.is_some() {
            backend.verify_password()?;
        }
        Ok(backend)
    }

    /// Open the first encrypted entry with the key this backend was given,
    /// and answer whether ZipCrypto's own header check accepted it.
    ///
    /// Nothing is decompressed: `by_index_decrypt` reads and validates the
    /// 12-byte header and returns a reader, which is dropped. So the cost is
    /// one seek and twelve bytes however large the payload is, and the answer
    /// arrives before a caller has committed to anything.
    fn verify_password(&mut self) -> CoreResult<()> {
        let Some(password) = self.password.clone() else {
            return Ok(());
        };
        let count = self.archive.len();
        for index in 0..count {
            let encrypted = {
                let entry = self
                    .archive
                    .by_index_raw(index)
                    .map_err(|e| malformed(format!("entry {index} could not be read: {e}")))?;
                entry.encrypted()
            };
            if !encrypted {
                continue;
            }
            return match self.archive.by_index_decrypt(index, &password) {
                Ok(_) => Ok(()),
                Err(zip::result::ZipError::InvalidPassword) => {
                    Err(CoreError::PayloadPasswordRefused {
                        archive: self.path.display().to_string(),
                    })
                }
                Err(other) => Err(malformed(format!(
                    "entry {index} could not be opened: {other}"
                ))),
            };
        }
        // Nothing in it is encrypted. See `open_with_password`'s own comment.
        Ok(())
    }
}

impl ArchiveBackend for ZipBackend {
    fn format(&self) -> &'static str {
        "zip"
    }

    fn entries(&mut self) -> CoreResult<Vec<ArchiveEntry>> {
        let count = self.archive.len();
        let mut entries = Vec::with_capacity(count);
        for index in 0..count {
            let entry = self
                .archive
                .by_index_raw(index)
                .map_err(|e| malformed(format!("entry {index} could not be read: {e}")))?;
            entries.push(ArchiveEntry {
                // The name exactly as stored. A ZIP writes `/` as its
                // separator whatever made it, and a hostile one writes
                // whatever it likes — neither is this module's problem.
                name: entry.name().to_string(),
                is_dir: entry.is_dir(),
                declared_bytes: entry.size(),
            });
        }
        Ok(entries)
    }

    fn read(&mut self, index: usize, limit: u64) -> CoreResult<Vec<u8>> {
        // The key, when a recipe gave one. `by_index_decrypt` discards it for
        // an entry that is not encrypted, so one call serves a mixed archive
        // as well as a wholly locked one — there is no second code path for
        // "this entry happens to be in clear" to be wrong about.
        let path = self.path.clone();
        let opened = match self.password.clone() {
            Some(password) => self.archive.by_index_decrypt(index, &password),
            None => self.archive.by_index(index),
        };
        let mut entry = opened.map_err(|e| match e {
            zip::result::ZipError::UnsupportedArchive(reason) => CoreError::UnsupportedFormat(
                format!("entry {index} of this ZIP cannot be read: {reason}"),
            ),
            // Reachable only for the 1-in-256 wrong key ZipCrypto's own
            // header check lets through at open — see the module doc comment.
            zip::result::ZipError::InvalidPassword => CoreError::PayloadPasswordRefused {
                archive: path.display().to_string(),
            },
            other => malformed(format!("entry {index} could not be opened: {other}")),
        })?;

        // `take(limit + 1)`: reading one byte past what is allowed is how the
        // difference between "exactly at the limit" and "more than the limit"
        // is discovered without ever holding the "more". A declared size is a
        // claim, and this is the claim being checked rather than trusted.
        let ceiling = limit.saturating_add(1);
        let mut out = Vec::new();
        entry
            .by_ref()
            .take(ceiling)
            .read_to_end(&mut out)
            .map_err(|e| malformed(format!("decompressing entry {index} failed: {e}")))?;

        if out.len() as u64 > limit {
            return Err(malformed(format!(
                "entry {index} decompresses to more than the {limit} bytes it was allowed"
            )));
        }
        Ok(out)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    /// A ZIP holding the given entries, built at runtime — ART ships no
    /// fixtures. Stored rather than deflated so the bytes in the file are the
    /// bytes the test wrote, which makes a failure readable.
    pub fn make_zip_with(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            for (name, data) in files {
                // `start_file` refuses some of the names this test suite exists
                // to try, so the raw entry API is used: the archive under test
                // has to be able to say `../../outside.txt`, because a real
                // hostile one can.
                writer
                    .start_file(*name, options)
                    .expect("the fixture writer must accept the name under test");
                writer.write_all(data).unwrap();
            }
            writer.finish().unwrap();
        }
        cursor.into_inner()
    }

    // ---- ZipCrypto, written here rather than by the crate under test -----
    //
    // `zip` 8.6 keeps `FileOptions::with_deprecated_encryption` `pub(crate)`,
    // so the crate's own writer cannot be reached from a test — and reaching
    // it would be the wrong fixture anyway: an archive encrypted by the same
    // code that decrypts it proves the two halves agree, not that either
    // matches the format. Python's `zipfile` cannot write one either (it
    // reads ZipCrypto and never writes it), so a checked-in fixture would
    // have to come from somewhere nobody in this repository could re-make.
    //
    // What follows is ~50 lines of PKWARE's own published algorithm, written
    // from the APPNOTE description: three 32-bit keys seeded from the
    // password, a 12-byte header whose last byte is the high byte of the
    // plaintext's CRC32 (PKZIP 2.0's one-byte check, which is what
    // `zipcrypto.rs::validate` compares against), and a stream cipher over
    // the bytes. Synthetic and generated at runtime, like every other fixture
    // here.

    /// The CRC32 table ZIP and ZipCrypto both use (polynomial `0xEDB88320`).
    fn crc_table() -> [u32; 256] {
        let mut table = [0u32; 256];
        for (n, slot) in table.iter_mut().enumerate() {
            let mut c = n as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
            *slot = c;
        }
        table
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let table = crc_table();
        let mut crc = 0xFFFF_FFFFu32;
        for &b in bytes {
            crc = table[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
        }
        crc ^ 0xFFFF_FFFF
    }

    struct ZipCryptoKeys {
        keys: [u32; 3],
        table: [u32; 256],
    }

    impl ZipCryptoKeys {
        fn new(password: &[u8]) -> Self {
            let mut me = Self {
                keys: [0x1234_5678, 0x2345_6789, 0x3456_7890],
                table: crc_table(),
            };
            for &b in password {
                me.update(b);
            }
            me
        }

        fn crc32_byte(&self, crc: u32, b: u8) -> u32 {
            (crc >> 8) ^ self.table[((crc ^ b as u32) & 0xFF) as usize]
        }

        fn update(&mut self, plain: u8) {
            self.keys[0] = self.crc32_byte(self.keys[0], plain);
            self.keys[1] = self.keys[1]
                .wrapping_add(self.keys[0] & 0xFF)
                .wrapping_mul(134_775_813)
                .wrapping_add(1);
            self.keys[2] = self.crc32_byte(self.keys[2], (self.keys[1] >> 24) as u8);
        }

        fn stream_byte(&self) -> u8 {
            let temp = (self.keys[2] | 2) & 0xFFFF;
            ((temp.wrapping_mul(temp ^ 1)) >> 8) as u8
        }

        fn encrypt(&mut self, plain: u8) -> u8 {
            let cipher = plain ^ self.stream_byte();
            self.update(plain);
            cipher
        }
    }

    /// `plain`, ZipCrypto-encrypted under `password`, with its 12-byte
    /// header. Deterministic: the first eleven header bytes are fixed, and
    /// only the twelfth carries meaning (the CRC check byte).
    fn zipcrypto_encrypt(plain: &[u8], password: &[u8]) -> Vec<u8> {
        let mut keys = ZipCryptoKeys::new(password);
        let mut header = [0x5Au8; 12];
        header[11] = (crc32(plain) >> 24) as u8;
        let mut out: Vec<u8> = header.iter().map(|&b| keys.encrypt(b)).collect();
        out.extend(plain.iter().map(|&b| keys.encrypt(b)));
        out
    }

    /// A ZIP whose every entry is ZipCrypto-encrypted under `password`,
    /// stored (never deflated) so the bytes in the file are the bytes the
    /// test wrote.
    ///
    /// The container is written by hand — local headers, central directory,
    /// end-of-central-directory — because the encryption has to be ART's
    /// fixture's and not the crate's; see this section's own comment.
    pub fn make_zipcrypto_zip_with(files: &[(&str, &[u8])], password: &[u8]) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        let mut directory: Vec<u8> = Vec::new();
        for (name, plain) in files {
            let name_bytes = name.as_bytes();
            let encrypted = zipcrypto_encrypt(plain, password);
            let offset = out.len() as u32;
            let crc = crc32(plain);

            // Local file header. Flag bit 0 set: this entry is encrypted.
            out.extend_from_slice(&0x0403_4B50u32.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes()); // version needed
            out.extend_from_slice(&1u16.to_le_bytes()); // general purpose flag
            out.extend_from_slice(&0u16.to_le_bytes()); // stored
            out.extend_from_slice(&0u16.to_le_bytes()); // time
            out.extend_from_slice(&0u16.to_le_bytes()); // date
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&(encrypted.len() as u32).to_le_bytes());
            out.extend_from_slice(&(plain.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // extra length
            out.extend_from_slice(name_bytes);
            out.extend_from_slice(&encrypted);

            directory.extend_from_slice(&0x0201_4B50u32.to_le_bytes());
            directory.extend_from_slice(&20u16.to_le_bytes()); // version made by
            directory.extend_from_slice(&20u16.to_le_bytes()); // version needed
            directory.extend_from_slice(&1u16.to_le_bytes());
            directory.extend_from_slice(&0u16.to_le_bytes());
            directory.extend_from_slice(&0u16.to_le_bytes());
            directory.extend_from_slice(&0u16.to_le_bytes());
            directory.extend_from_slice(&crc.to_le_bytes());
            directory.extend_from_slice(&(encrypted.len() as u32).to_le_bytes());
            directory.extend_from_slice(&(plain.len() as u32).to_le_bytes());
            directory.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            directory.extend_from_slice(&0u16.to_le_bytes()); // extra
            directory.extend_from_slice(&0u16.to_le_bytes()); // comment
            directory.extend_from_slice(&0u16.to_le_bytes()); // disk
            directory.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
            directory.extend_from_slice(&0u32.to_le_bytes()); // external attrs
            directory.extend_from_slice(&offset.to_le_bytes());
            directory.extend_from_slice(name_bytes);
        }

        let directory_offset = out.len() as u32;
        let directory_size = directory.len() as u32;
        out.extend_from_slice(&directory);
        out.extend_from_slice(&0x0605_4B50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // this disk
        out.extend_from_slice(&0u16.to_le_bytes()); // directory start disk
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&directory_size.to_le_bytes());
        out.extend_from_slice(&directory_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // comment length
        out
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-zip-{tag}-{}-{}",
            crate::core::test_scratch_id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn it_lists_and_reads_an_archive() {
        let dir = scratch("list");
        let archive = dir.join("test.zip");
        std::fs::write(
            &archive,
            make_zip_with(&[("readme.txt", b"hello"), ("data/file.bin", b"\x00\x01\x02")]),
        )
        .unwrap();

        let mut backend = ZipBackend::open(&archive).unwrap();
        let entries = backend.entries().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "readme.txt");
        assert_eq!(entries[0].declared_bytes, 5);
        assert_eq!(backend.read(0, 1024).unwrap(), b"hello");
        assert_eq!(backend.read(1, 1024).unwrap(), b"\x00\x01\x02");
        // Backwards, too: the gate reads in order, but a backend that could
        // only go forwards would be a trap for the next caller.
        assert_eq!(backend.read(0, 1024).unwrap(), b"hello");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_file_that_is_not_a_zip_fails_at_open() {
        let dir = scratch("not-zip");
        let bogus = dir.join("plain.zip");
        std::fs::write(&bogus, vec![0u8; 512]).unwrap();

        assert!(ZipBackend::open(&bogus).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The fixture writer must store the hostile names **verbatim**, or the
    /// shared gate test in `core::archive` would be proving nothing: an
    /// archive whose `../../outside.txt` was quietly turned into
    /// `outside.txt` on the way in cannot show that the gate refuses
    /// traversal — it would pass just as well with the defence removed.
    /// Asserted beside the writer, because that is where the sanitising
    /// would happen.
    #[test]
    fn the_fixture_writer_stores_hostile_names_exactly_as_given() {
        let dir = scratch("verbatim");
        let archive = dir.join("hostile.zip");
        // The same three names the shared gate test hands every backend.
        let names = [
            "../../outside.txt",
            "../sibling.txt",
            r"C:\Windows\Temp\absolute.txt",
        ];
        let files: Vec<(&str, &[u8])> = names.iter().map(|n| (*n, b"x" as &[u8])).collect();
        std::fs::write(&archive, make_zip_with(&files)).unwrap();

        let mut backend = ZipBackend::open(&archive).unwrap();
        let stored: Vec<String> = backend
            .entries()
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(stored, names, "the names must survive the round trip");

        std::fs::remove_dir_all(&dir).ok();
    }

    // ---- the password path (ART-166, 2026-09-08) -------------------------

    /// The fixture is only worth anything if a **reader that knows nothing
    /// about it** cannot get at the bytes without the key. Asserted first,
    /// because every test below it would pass just as well against an
    /// archive that was never really encrypted.
    #[test]
    fn the_encrypted_fixture_cannot_be_read_without_the_key() {
        let dir = scratch("crypto-premise");
        let archive = dir.join("locked.zip");
        std::fs::write(
            &archive,
            make_zipcrypto_zip_with(&[("C/WBRun", b"amiga program")], b"93ABDF11"),
        )
        .unwrap();

        let mut plain = ZipBackend::open(&archive).unwrap();
        // Listing still works — a ZIP's names are in clear even when its
        // contents are not, which is exactly why ART could count 233 entries
        // it could not read.
        assert_eq!(plain.entries().unwrap()[0].name, "C/WBRun");
        let err = plain.read(0, 1024).unwrap_err().to_string();
        assert!(
            err.to_lowercase().contains("password"),
            "an encrypted entry read without a key must say so: {err}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The right key gives the bytes back, whole and unchanged.
    #[test]
    fn the_right_password_reads_the_entry_back_exactly() {
        let dir = scratch("crypto-right");
        let archive = dir.join("locked.zip");
        // Long enough to cross the cipher's own state a few hundred times,
        // so a keystream that desynchronised after the header would show.
        let payload: Vec<u8> = (0..4096u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(
            &archive,
            make_zipcrypto_zip_with(
                &[("C/WBRun", b"amiga program"), ("Libs/x.library", &payload)],
                b"93ABDF11",
            ),
        )
        .unwrap();

        let mut backend = ZipBackend::open_with_password(&archive, Some("93ABDF11")).unwrap();
        assert_eq!(backend.read(0, 1024).unwrap(), b"amiga program");
        assert_eq!(backend.read(1, 8192).unwrap(), payload);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// **A wrong key is one refusal with one sentence, raised at open** —
    /// never a raw crate error reaching the user, and never after a caller
    /// has already started doing something with the archive.
    #[test]
    fn a_wrong_password_is_refused_at_open_with_arts_own_sentence() {
        let dir = scratch("crypto-wrong");
        let archive = dir.join("locked.zip");
        std::fs::write(
            &archive,
            make_zipcrypto_zip_with(&[("C/WBRun", b"amiga program")], b"93ABDF11"),
        )
        .unwrap();

        let err = ZipBackend::open_with_password(&archive, Some("NOTTHEKEY"))
            .expect_err("the key does not fit this archive");
        assert_eq!(
            err.code(),
            "ART-PAYLOAD-PASSWORD",
            "its own ending, not ART-FORMAT-MALFORMED — the file is not damaged"
        );
        let text = err.to_string();
        assert!(text.contains("did not open with the password"), "{text}");
        assert!(
            text.contains("locked.zip"),
            "the sentence names the file the user can go and look at: {text}"
        );
        assert!(
            !text.contains("provided password is incorrect"),
            "the crate's own words must not reach the user: {text}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A key handed to an archive with nothing encrypted in it is not an
    /// error: the crate discards a password no entry needs, and a package
    /// whose publisher shipped one build locked and another in clear is a
    /// fact about the archive rather than a fault.
    #[test]
    fn a_password_for_an_archive_in_clear_is_not_an_error() {
        let dir = scratch("crypto-unneeded");
        let archive = dir.join("plain.zip");
        std::fs::write(&archive, make_zip_with(&[("C/Assign", b"assign")])).unwrap();

        let mut backend = ZipBackend::open_with_password(&archive, Some("93ABDF11")).unwrap();
        assert_eq!(backend.read(0, 1024).unwrap(), b"assign");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A directory entry is a directory, not a zero-byte file — get this
    /// wrong and an archive's folders arrive as empty files with the folder's
    /// name, and every path under them is refused.
    #[test]
    fn a_directory_entry_is_reported_as_one() {
        let dir = scratch("dirs");
        let archive = dir.join("test.zip");
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            writer
                .add_directory("Tools", SimpleFileOptions::default())
                .unwrap();
            writer.finish().unwrap();
        }
        std::fs::write(&archive, cursor.into_inner()).unwrap();

        let mut backend = ZipBackend::open(&archive).unwrap();
        let entries = backend.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_dir, "{:?}", entries[0]);

        std::fs::remove_dir_all(&dir).ok();
    }
}
