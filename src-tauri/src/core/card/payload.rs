//! What goes on the boot partition, and which of it is right for this card
//! (SD-1 · G2).
//!
//! [`build_card`](super::build::build_card) places whatever files it is given.
//! This is what decides them: the Emu68 release the user downloaded, unpacked
//! and checked against the board they actually have, plus their Kickstart under
//! the name the firmware looks for, plus the two config files.
//!
//! ## The archive's name is a claim ART names, and the user decides
//!
//! `Emu68-pistorm.zip` means the **classic** board in the 1.0.x line and
//! PiStorm32-lite/PiStorm16 in the 1.1 line, and no release has ever shipped
//! `Emu68-pistorm16.zip` at all — that was ART-091, a name ART invented and
//! told people to download for months. So the archive a user hands over is
//! compared with the board and the release line before a byte of it reaches a
//! card. Until 2026-09-11 a difference was refused; by the owner's ruling
//! (ART-305) it is **written and said**: the people who build a PiStorm card
//! pick their own Emu68 — a prerelease, a nightly, a build they are testing —
//! so the payload carries an [`ArchiveMismatch`] with both names and the plan
//! shows it before the card is written.
//!
//! `Emu68-raspi.zip` is still refused. It is not a version of the PiStorm
//! firmware at all but Emu68 running on a Pi on its own, it sits in the same
//! release, and it is the commonest thing to pick by mistake.
//!
//! ## The config files are edited, never regenerated
//!
//! The Emu68 archive carries its own `config.txt` — the Raspberry Pi's, with
//! dozens of settings ART knows nothing about. §39/§40's rule holds here as it
//! does for `FF.CFG`: managed keys are rewritten and everything else passes
//! through, which is what [`merge_config_txt`] already does. `cmdline.txt` is
//! not in the archive at all, so that one ART writes from nothing.

use std::path::Path;

use crate::core::archive;
use crate::core::error::{CoreError, CoreResult};
use crate::core::fat32::BootFile;
use crate::core::pistorm::firmware::{merge_config_txt_with_overlays, FirmwareConfig};
use crate::core::pistorm::hardware::{
    kernel_archive, Emu68Line, KernelArchive, PistormHardware, PistormVariant,
};
use crate::core::pistorm::options::{merge_cmdline, storage_overlay_lines, Emu68Options};

/// The most one file in the Emu68 archive may decompress to.
///
/// `start.elf` is the biggest at about 3 MB; 64 is room for a release several
/// times larger without being a number that lets an archive exhaust memory.
const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;

/// The most a whole archive may expand to on its way to the card.
///
/// `MAX_ENTRY_BYTES` bounds one file; twenty files each just under it is a
/// gigabyte and a quarter, and the payload is held whole in memory before any
/// of it is written. A zip that compresses to nothing and expands to everything
/// is the oldest hostile archive there is, and `core/archive`'s gate does not
/// cover this path — that one is about extraction to *disk*, and nothing here
/// touches disk.
///
/// The real Emu68 release is about ten megabytes across twenty files, so this
/// is generous by a factor of twenty-five. The user's Kickstart is not counted:
/// it is a file on their own disk rather than something an archive can inflate.
const MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;

/// The archive nobody wants and everybody downloads once.
const RASPI_ARCHIVE: &str = "emu68-raspi.zip";

/// What the boot partition should end up holding.
pub struct PayloadSpec {
    pub hardware: PistormHardware,
    /// Which release line the archive came from. It decides what the file
    /// names mean, and ART cannot tell from the bytes.
    pub line: Emu68Line,
    /// The `config.txt` settings ART manages. `kickstart_file` is also the
    /// name the ROM is written under, so the two cannot disagree.
    pub firmware: FirmwareConfig,
    /// The `cmdline.txt` options ART manages.
    pub options: Emu68Options,
    /// The user's Kickstart. `None` builds a card with no ROM on it, which is
    /// a card that will not boot — allowed, because a caller may be adding it
    /// separately, and said plainly rather than substituted for.
    pub kickstart: Option<Vec<u8>>,
}

/// The Emu68 archive the user chose, where ART's table names another for this
/// board and release line (ART-305).
///
/// Written anyway and said, never refused — the owner's ruling of 2026-09-11:
/// the people who build a PiStorm card choose their Emu68 on purpose (a
/// prerelease, a nightly, a build they are testing), so the card is theirs and
/// the sentence is ART's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveMismatch {
    /// The chosen file's own name.
    pub given: String,
    /// What the table names for this board in this line — `None` when the line
    /// ships nothing for the board at all (PiStorm16 on stable, ART-091).
    pub expected: Option<String>,
    pub variant: PistormVariant,
    pub line: Emu68Line,
}

/// Everything that goes on the boot partition, and what the card will boot.
#[derive(Debug)]
pub struct Emu68Payload {
    /// The files, in the order they will be written.
    pub files: Vec<BootFile>,
    /// The file `config.txt` points the Pi's firmware at.
    ///
    /// Out here rather than left inside the config, because it is the answer
    /// ART-103 got wrong: a screen can show it before a card is written, and a
    /// name that is not among `files` is a card that fails on the Amiga where
    /// nobody can see why.
    pub kernel_file: String,
    /// Set when the archive is not the one ART's table names for this board
    /// and line — written anyway, and said (ART-305).
    pub archive_mismatch: Option<ArchiveMismatch>,
}

/// Everything that goes on the boot partition, in the order it will be written.
pub fn emu68_payload(archive_path: &Path, spec: &PayloadSpec) -> CoreResult<Emu68Payload> {
    payload_within(archive_path, spec, MAX_TOTAL_BYTES)
}

/// [`emu68_payload`] with the ceiling as a parameter, so a test can prove the
/// refusal without allocating a quarter of a gigabyte to do it.
fn payload_within(
    archive_path: &Path,
    spec: &PayloadSpec,
    max_total: u64,
) -> CoreResult<Emu68Payload> {
    let archive_mismatch = archive_mismatch_for(archive_path, spec)?;

    let mut backend = archive::open(archive_path)?;
    let entries = backend.entries()?;

    let mut files: Vec<BootFile> = Vec::with_capacity(entries.len() + 3);
    let mut existing_config: Option<String> = None;
    let mut total: u64 = 0;

    for (index, entry) in entries.iter().enumerate() {
        if entry.is_dir {
            // A directory is made by the file inside it — see
            // `core::fat32::create_boot_partition`. An empty one carries
            // nothing a Pi reads.
            continue;
        }

        let bytes = backend.read(index, MAX_ENTRY_BYTES)?;

        // On the running total of what was actually read, never on what the
        // archive declares — a declared length is the attacker's field.
        total = total
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| too_big(archive_path, max_total))?;
        if total > max_total {
            return Err(too_big(archive_path, max_total));
        }

        let name = entry.name.replace('\\', "/");

        // The archive's own `config.txt` is not placed as it stands: it is what
        // ART's managed keys are merged *into*.
        if name.eq_ignore_ascii_case("config.txt") {
            existing_config = Some(String::from_utf8_lossy(&bytes).into_owned());
            continue;
        }

        files.push(BootFile { name, bytes });
    }

    if files.is_empty() {
        return Err(CoreError::Malformed {
            format: "Emu68 archive".into(),
            detail: format!("'{}' holds no files", archive_path.display()),
        });
    }

    // Which file the firmware is to boot — **the release's own answer**, not
    // ART's (ART-103). `Emu68-pistorm.zip` carries `Emu68-pistorm.gz` and its
    // `config.txt` says `kernel=Emu68-pistorm.gz`; ART used to rewrite that to
    // `Emu68.img`, a file no card has, and the card would not boot. The Pi's
    // firmware decompresses a gzipped kernel itself, which is why the release
    // ships one and points straight at it.
    let mut firmware = spec.firmware.clone();
    firmware.kernel_file = kernel_named_by(existing_config.as_deref())
        .or_else(|| kernel_among(&files))
        .ok_or_else(|| CoreError::Malformed {
            format: "Emu68 archive".into(),
            detail: format!(
                "'{}' does not say which file is the kernel and carries nothing named like \
                     one, so ART cannot write a `config.txt` that boots",
                archive_path.display()
            ),
        })?;

    if !files.iter().any(|file| file.name == firmware.kernel_file) {
        return Err(CoreError::Malformed {
            format: "Emu68 archive".into(),
            detail: format!(
                "the archive's config.txt boots '{}' and the archive does not contain it",
                firmware.kernel_file
            ),
        });
    }

    // `config.txt`, edited rather than regenerated (§39/§40). A release that
    // stopped carrying one would get ART's managed keys and nothing else,
    // which is what `merge_config_txt(None)` already means.
    files.push(BootFile {
        name: "config.txt".into(),
        bytes: merge_config_txt_with_overlays(
            &firmware,
            &storage_overlay_lines(&spec.options),
            existing_config.as_deref(),
        )
        .into_bytes(),
    });

    // `cmdline.txt` is **not** in the Emu68 archive — checked against the real
    // release, whose nineteen files do not include one. So this is written
    // from nothing rather than merged into something.
    files.push(BootFile {
        name: "cmdline.txt".into(),
        bytes: merge_cmdline(&spec.options, spec.hardware, None).into_bytes(),
    });

    if let Some(rom) = &spec.kickstart {
        // Under the name `config.txt`'s `initramfs` line points at, so the two
        // cannot disagree about what the ROM is called.
        files.push(BootFile {
            name: firmware.kickstart_file.clone(),
            bytes: rom.clone(),
        });
    }

    Ok(Emu68Payload {
        files,
        kernel_file: firmware.kernel_file,
        archive_mismatch,
    })
}

fn too_big(archive_path: &Path, max_total: u64) -> CoreError {
    CoreError::Malformed {
        format: "Emu68 archive".into(),
        detail: format!(
            "'{}' expands to more than {max_total} bytes, which is far more than any Emu68 \
             release — ART will not hold that much of an archive in memory",
            archive_path.display()
        ),
    }
}

/// The kernel the archive's own `config.txt` names, if it names one.
///
/// The authoritative answer, and the reason ART does not have to recognise a
/// kernel by its name: the release says which file it boots.
fn kernel_named_by(config: Option<&str>) -> Option<String> {
    let text = config?;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("kernel=") {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.replace('\\', "/"));
            }
        }
    }
    None
}

/// The kernel by its name, for a release whose `config.txt` does not say.
///
/// A fallback, and a narrow one: `Emu68` at the front is what every release
/// has called it. Guessing more widely would be inventing a name, which is
/// exactly what ART-091 and ART-103 were.
fn kernel_among(files: &[BootFile]) -> Option<String> {
    files
        .iter()
        .map(|file| &file.name)
        .find(|name| {
            let base = name.rsplit('/').next().unwrap_or(name);
            base.to_lowercase().starts_with("emu68")
        })
        .cloned()
}

/// How the chosen archive stands against ART's table for this board and line.
///
/// `Emu68-raspi.zip` is the one refusal: it is not a version of the PiStorm
/// firmware at all but Emu68 for a bare Raspberry Pi, and a card built from it
/// cannot boot a PiStorm. Everything else the user chose is theirs to choose
/// (ART-305, the owner's ruling) — a name the table does not expect comes back
/// as an [`ArchiveMismatch`] for the screen to say, never as an error.
fn archive_mismatch_for(
    archive_path: &Path,
    spec: &PayloadSpec,
) -> CoreResult<Option<ArchiveMismatch>> {
    let given = archive_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();

    if given.to_lowercase() == RASPI_ARCHIVE {
        return Err(CoreError::InvalidInput(
            "'Emu68-raspi.zip' is Emu68 running on a Raspberry Pi by itself, not the firmware \
             for a PiStorm. The card needs the archive for your board."
                .into(),
        ));
    }

    let mismatch = |expected: Option<&str>| ArchiveMismatch {
        given: given.clone(),
        expected: expected.map(str::to_string),
        variant: spec.hardware.variant,
        line: spec.line,
    };
    Ok(match kernel_archive(spec.hardware.variant, spec.line) {
        KernelArchive::Named(expected) if given.to_lowercase() == expected.to_lowercase() => None,
        KernelArchive::Named(expected) => Some(mismatch(Some(expected))),
        KernelArchive::Absent => Some(mismatch(None)),
        // The release exists for this board and its notes do not say which
        // asset covers it. ART has no better answer than the user's, and
        // inventing one is what ART-091 was — so there is nothing to compare.
        KernelArchive::Unstated => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::pistorm::hardware::{AmigaTarget, PistormVariant};
    use crate::core::pistorm::options::Emu68Options;

    fn scratch(name: &str) -> (crate::core::ScratchDir, std::path::PathBuf) {
        crate::core::ScratchDir::pair("art-payload", name)
    }

    /// A zip with the shape the real Emu68 release has: files at the root, a
    /// folder, and a `config.txt` of the Pi's own.
    fn emu68_zip(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
        use std::io::Write as _;
        let path = dir.join(name);
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);

        for (entry, contents) in [
            ("Emu68-pistorm.gz", &b"kernel"[..]),
            ("start.elf", b"firmware"),
            ("overlays/emu68.dtbo", b"overlay"),
            (
                "config.txt",
                b"# the Pi's own\narm_64bit=1\nsomething_art_knows_nothing_about=1\n",
            ),
        ] {
            zip.start_file(entry, options).unwrap();
            zip.write_all(contents).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    fn classic() -> PistormHardware {
        PistormHardware {
            amiga: AmigaTarget::A500,
            variant: PistormVariant::Classic,
            pi: crate::core::pistorm::hardware::PiModel::Pi3APlus,
        }
    }

    fn spec(hardware: PistormHardware, line: Emu68Line) -> PayloadSpec {
        PayloadSpec {
            hardware,
            line,
            firmware: FirmwareConfig::default(),
            options: Emu68Options::default(),
            kickstart: Some(vec![0xAB; 512 * 1024]),
        }
    }

    #[test]
    fn every_file_in_the_archive_reaches_the_card() {
        let (_guard, dir) = scratch("all-files");
        let archive = emu68_zip(&dir, "Emu68-pistorm.zip");

        let files = emu68_payload(&archive, &spec(classic(), Emu68Line::Stable))
            .unwrap()
            .files;
        let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();

        assert!(names.contains(&"Emu68-pistorm.gz"), "{names:?}");
        assert!(names.contains(&"start.elf"), "{names:?}");
        assert!(
            names.contains(&"overlays/emu68.dtbo"),
            "the folder survives: {names:?}"
        );
        assert!(names.contains(&"kick.rom"), "the Kickstart: {names:?}");
        assert!(names.contains(&"cmdline.txt"), "{names:?}");
        assert!(names.contains(&"config.txt"), "{names:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// §39/§40: the Pi's own `config.txt` is **edited**, not replaced. A line
    /// ART knows nothing about has to come out the other side.
    #[test]
    fn the_pis_own_config_settings_survive() {
        let (_guard, dir) = scratch("config-merge");
        let archive = emu68_zip(&dir, "Emu68-pistorm.zip");

        let files = emu68_payload(&archive, &spec(classic(), Emu68Line::Stable))
            .unwrap()
            .files;
        let config = files.iter().find(|f| f.name == "config.txt").unwrap();
        let text = String::from_utf8(config.bytes.clone()).unwrap();

        assert!(
            text.contains("something_art_knows_nothing_about=1"),
            "a setting ART does not manage must pass through: {text}"
        );
        assert!(
            text.contains("initramfs kick.rom"),
            "and ART's own line has to be there: {text}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **Which file the card boots is an answer, not a detail.** ART-103 was
    /// ART writing a kernel name no release has ever shipped, and the card
    /// failed on the Amiga where nobody could see why — so the name the payload
    /// settled on comes back out where a screen can put it in front of the user
    /// before the card is written.
    #[test]
    fn the_payload_says_which_file_the_card_boots() {
        let (_guard, dir) = scratch("kernel-answer");
        let archive = emu68_zip(&dir, "Emu68-pistorm.zip");

        let payload = emu68_payload(&archive, &spec(classic(), Emu68Line::Stable)).unwrap();

        assert_eq!(payload.kernel_file, "Emu68-pistorm.gz");
        assert!(
            payload.files.iter().any(|f| f.name == payload.kernel_file),
            "and it is one of the files being placed"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A payload is held whole in memory before it reaches the card, so the
    /// archive needs a **total** ceiling and not only a per-file one: twenty
    /// entries each just under `MAX_ENTRY_BYTES` is a gigabyte and a quarter,
    /// and a zip that compresses to nothing and expands to everything is the
    /// oldest hostile archive there is.
    ///
    /// Driven through the internal entry point with a small budget so the test
    /// costs kilobytes rather than a quarter of a gigabyte; the public
    /// [`emu68_payload`] passes [`MAX_TOTAL_BYTES`], which the next test pins.
    #[test]
    fn an_archive_that_expands_past_the_budget_is_refused() {
        let (_guard, dir) = scratch("budget");
        let archive = emu68_zip(&dir, "Emu68-pistorm.zip");

        let err = payload_within(&archive, &spec(classic(), Emu68Line::Stable), 8).unwrap_err();

        assert_eq!(err.code(), "ART-FORMAT-MALFORMED", "{err}");
        assert!(
            err.to_string().contains("8 bytes"),
            "the ceiling it broke has to be in the sentence: {err}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The budget the real entry point uses. A release of about ten megabytes
    /// has to fit with room to spare, and a card's worth of anything must not.
    #[test]
    fn the_payload_budget_is_generous_and_finite() {
        assert_eq!(MAX_TOTAL_BYTES, 256 * 1024 * 1024);
    }

    /// ART-103, at the point where it would cost a card that does not boot.
    ///
    /// The release's own `config.txt` says which file the firmware loads —
    /// `kernel=Emu68-pistorm.gz`, a gzipped kernel the Pi's firmware
    /// decompresses itself. ART used to rewrite that line to `Emu68.img`,
    /// which is a name from older material and a file no release ships.
    #[test]
    fn the_kernel_the_release_names_is_the_one_the_card_boots() {
        use std::io::Write as _;

        let (_guard, dir) = scratch("kernel-name");
        let path = dir.join("Emu68-pistorm.zip");
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (entry, contents) in [
            ("Emu68-pistorm.gz", &b"kernel"[..]),
            ("config.txt", b"kernel=Emu68-pistorm.gz\narm_64bit=1\n"),
        ] {
            zip.start_file(entry, options).unwrap();
            zip.write_all(contents).unwrap();
        }
        zip.finish().unwrap();

        let files = emu68_payload(&path, &spec(classic(), Emu68Line::Stable))
            .unwrap()
            .files;
        let config = files.iter().find(|f| f.name == "config.txt").unwrap();
        let text = String::from_utf8(config.bytes.clone()).unwrap();

        assert!(
            text.contains("kernel=Emu68-pistorm.gz"),
            "the release's own kernel line has to survive: {text}"
        );
        assert!(
            !text.contains("Emu68.img"),
            "and ART must not put its own name over it: {text}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The check that makes "this card boots" answerable before it is written:
    /// a `config.txt` naming a kernel the archive does not carry is refused,
    /// rather than built into a card that fails on the Amiga.
    #[test]
    fn a_config_naming_a_kernel_that_is_not_there_is_refused() {
        use std::io::Write as _;

        let (_guard, dir) = scratch("kernel-missing");
        let path = dir.join("Emu68-pistorm.zip");
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (entry, contents) in [
            ("start.elf", &b"firmware"[..]),
            ("config.txt", b"kernel=Emu68-somethingelse.gz\n"),
        ] {
            zip.start_file(entry, options).unwrap();
            zip.write_all(contents).unwrap();
        }
        zip.finish().unwrap();

        let err = emu68_payload(&path, &spec(classic(), Emu68Line::Stable)).unwrap_err();
        assert!(
            err.to_string().contains("Emu68-somethingelse.gz"),
            "the refusal names the file that is missing: {err}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The Kickstart is written under the name `config.txt` points at, so the
    /// two cannot disagree about what the ROM is called.
    #[test]
    fn the_kickstart_is_named_what_the_config_asks_for() {
        let (_guard, dir) = scratch("rom-name");
        let archive = emu68_zip(&dir, "Emu68-pistorm.zip");

        let mut wanted = spec(classic(), Emu68Line::Stable);
        wanted.firmware.kickstart_file = "kick31.rom".into();

        let files = emu68_payload(&archive, &wanted).unwrap().files;
        let rom = files.iter().find(|f| f.name == "kick31.rom").unwrap();
        assert_eq!(rom.bytes.len(), 512 * 1024);

        let config = files.iter().find(|f| f.name == "config.txt").unwrap();
        let text = String::from_utf8(config.bytes.clone()).unwrap();
        assert!(text.contains("initramfs kick31.rom"), "{text}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ART-091 made this a refusal; the owner's ruling of 2026-09-11 (ART-305)
    /// made it a sentence. The people who build a PiStorm card choose their
    /// Emu68 on purpose — a prerelease, a nightly, another board's build to
    /// test — so ART writes what it was given and names it against its own
    /// table. `Emu68-pistorm.zip` still means another board in the other line
    /// (ART-091); that is exactly what the sentence carries now.
    #[test]
    fn an_archive_for_another_board_is_written_and_named() {
        let (_guard, dir) = scratch("wrong-board");
        let archive = emu68_zip(&dir, "Emu68-pistorm32lite.zip");

        let payload = emu68_payload(&archive, &spec(classic(), Emu68Line::Stable))
            .expect("the user's choice is written, not refused");
        assert_eq!(
            payload.archive_mismatch,
            Some(ArchiveMismatch {
                given: "Emu68-pistorm32lite.zip".into(),
                expected: Some("Emu68-pistorm.zip".into()),
                variant: PistormVariant::Classic,
                line: Emu68Line::Stable,
            })
        );
    }

    /// **The owner's own card, 2026-09-11:** the 1.1 beta's classic archive
    /// with the release line left on stable. Refused until ART-305 with *"the
    /// PiStorm needs 'Emu68-pistorm.zip' from the stable release line, and
    /// this is 'Emu68-pistorm-classic.zip'"*.
    #[test]
    fn a_prerelease_archive_on_the_stable_line_is_written_and_named() {
        let (_guard, dir) = scratch("prerelease-on-stable");
        let archive = emu68_zip(&dir, "Emu68-pistorm-classic.zip");

        let payload = emu68_payload(&archive, &spec(classic(), Emu68Line::Stable))
            .expect("the owner's Emu68 is written");
        assert!(payload.files.iter().any(|f| f.name == payload.kernel_file));
        assert_eq!(
            payload.archive_mismatch.map(|m| (m.given, m.expected)),
            Some((
                "Emu68-pistorm-classic.zip".to_string(),
                Some("Emu68-pistorm.zip".to_string())
            ))
        );
    }

    /// A nightly carries its date and commit in its name
    /// (`api.github.com/repos/michalsc/Emu68/releases`, tag `nightly`), so no
    /// table entry can ever equal it — and it is what somebody testing the
    /// project picks.
    #[test]
    fn a_nightly_archive_is_written_and_named() {
        let (_guard, dir) = scratch("nightly");
        let archive = emu68_zip(&dir, "Emu68-pistorm-classic-20260728-614794.zip");

        let payload = emu68_payload(&archive, &spec(classic(), Emu68Line::Stable))
            .expect("a nightly is written");
        assert_eq!(
            payload.archive_mismatch.map(|m| m.given),
            Some("Emu68-pistorm-classic-20260728-614794.zip".to_string())
        );
    }

    /// PiStorm16 has no stable release at all (ART-091's table). That was a
    /// refusal too; it is now the same sentence with nothing to name.
    #[test]
    fn a_line_with_nothing_for_the_board_is_written_and_said() {
        let (_guard, dir) = scratch("absent");
        let archive = emu68_zip(&dir, "Emu68-pistorm.zip");
        let mut hardware = classic();
        hardware.variant = PistormVariant::Pistorm16;

        let payload = emu68_payload(&archive, &spec(hardware, Emu68Line::Stable))
            .expect("written, not refused");
        let mismatch = payload.archive_mismatch.expect("and said");
        assert_eq!(mismatch.expected, None);
        assert_eq!(mismatch.variant, PistormVariant::Pistorm16);
    }

    /// The commonest mistake in the whole release, and it is in the same zip
    /// listing as the right one.
    #[test]
    fn the_raspi_build_is_refused_for_what_it_is() {
        let (_guard, dir) = scratch("raspi");
        let archive = emu68_zip(&dir, "Emu68-raspi.zip");

        let err = emu68_payload(&archive, &spec(classic(), Emu68Line::Stable)).unwrap_err();
        assert!(
            err.to_string().contains("by itself"),
            "the refusal has to say what it is, not just that it is wrong: {err}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The classic board's archive is named differently in the two lines: the
    /// table's own name raises nothing, and the other line's is named — the
    /// same name means another board in the 1.1 line (ART-091).
    #[test]
    fn the_release_line_decides_what_the_name_means() {
        let (_guard, dir) = scratch("lines");

        let stable = emu68_zip(&dir, "Emu68-pistorm.zip");
        let alpha = emu68_zip(&dir, "Emu68-pistorm-classic.zip");

        let mismatch = |archive: &std::path::Path, line| {
            emu68_payload(archive, &spec(classic(), line))
                .unwrap()
                .archive_mismatch
        };
        assert_eq!(mismatch(&stable, Emu68Line::Stable), None);
        assert_eq!(mismatch(&alpha, Emu68Line::Alpha11), None);
        assert_eq!(
            mismatch(&stable, Emu68Line::Alpha11).and_then(|m| m.expected),
            Some("Emu68-pistorm-classic.zip".to_string()),
            "the same name means another board in the 1.1 line"
        );
    }

    /// A card with no ROM will not boot, and ART says so by leaving it out
    /// rather than by inventing one.
    #[test]
    fn no_kickstart_means_no_rom_on_the_card() {
        let (_guard, dir) = scratch("no-rom");
        let archive = emu68_zip(&dir, "Emu68-pistorm.zip");

        let mut wanted = spec(classic(), Emu68Line::Stable);
        wanted.kickstart = None;

        let files = emu68_payload(&archive, &wanted).unwrap().files;
        assert!(!files.iter().any(|f| f.name == "kick.rom"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
