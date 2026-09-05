//! `igame.data` — telling the Amiga's own launcher what ART already knows.
//!
//! SD-2 G10 wave 2. ART's catalogue holds a title, a year, a genre, a player
//! count and a chipset for every game it has read; iGame, the launcher most
//! WHDLoad collections are browsed through on the Amiga, reads exactly those
//! from a small file in each game's drawer. Writing it is how the metadata
//! ART worked out on the host survives onto the machine.
//!
//! # The format was read out of iGame, not recalled
//!
//! `MrZammler/iGame`, `src/fsfuncs.c::getIGameDataInfo` and `src/funcs.c`'s
//! calls to it, read 2026-08-24. **Five of the six rules below would have been
//! got wrong by writing what the format looks like from the outside**, and
//! each one produces a file iGame silently ignores rather than an error
//! anybody would see:
//!
//! 1. **The line buffer is 64 bytes.** `int lineSize = 64;` and
//!    `FGets(fp, line, lineSize)`, which reads at most `lineSize - 1`
//!    characters *including* the newline. A longer line is not truncated — it
//!    is **split**, and the tail comes back as the next line, where a `=`
//!    inside a long title would be parsed as a key. [`LINE_BYTES`].
//! 2. **Keys are matched with `strcmp`**, so they are case-sensitive and must
//!    be exactly `title`, `chipset`, `genre`, `year`, `players`, `exe`.
//! 3. **Nothing is trimmed.** `strtok(line, "=")` makes `title = X` into the
//!    key `"title "`, which matches nothing. No spaces around the `=`.
//! 4. **`year` and `players` are only taken when `isNumeric(value)`.** A
//!    written "unknown" is read and discarded, so it is not written at all.
//! 5. **`exe` is rejected when it contains `.slave`** (`strcasestr`), which is
//!    the trap: the obvious thing to put there is the slave, and iGame will
//!    refuse exactly that. `exe` is for the non-WHDLoad case.
//! 6. **An empty value is skipped** (`strlen(value) > 0`), so a key with
//!    nothing after it is noise rather than a statement that ART knows
//!    nothing.
//!
//! One more, worth knowing rather than acted on: **`title` is only used when
//! the user has `useIgameDataTitle` switched on** in iGame. ART writing a
//! title is a suggestion the user's own setting may decline, and no sentence
//! anywhere may promise otherwise.
//!
//! # An existing file is edited, never regenerated
//!
//! CLAUDE.md's rule for `FF.CFG`, `config.txt` and `cmdline.txt`, and it
//! applies here for the same reason: an `igame.data` may be one somebody
//! curated by hand, and iGame **silently ignores keys it does not know**, so a
//! user may keep their own in it. [`merge_into`] rewrites the keys ART manages
//! and passes every other line through verbatim — comments, ordering and
//! unknown keys included.
//!
//! # Where it can go, measured on the owner's own catalogue
//!
//! **Not one title in it has a drawer.** Counted 2026-08-24 over their 2 815
//! catalogued titles: 1 697 `whdload-hardfile`, 1 016 `floppies`, 102
//! `hardfile`, and **zero** drawers — `Media::WhdloadDrawer` was removed by
//! [ART-147] precisely because the shape was wrong. So *“`igame.data` beside
//! each slave”*, which is how the work list describes this, cannot be done for
//! a single one of their games: the slaves are **inside** the hardfiles.
//!
//! That is not a reason to leave the format unwritten — it is the prerequisite
//! for both of the ways it can be reached — but it is a reason no sentence
//! anywhere may say ART writes `igame.data` for a collection like this one
//! yet. The two routes, neither built:
//!
//! 1. **Into the hardfile**, beside the slave, through `core/volume/write`.
//!    A `whdload-hardfile` is a bare FFS volume ART can already write.
//!
//!    **Measured on all 1 697 of the owner's own images**
//!    ([`free_bytes_in_hardfile`], 2026-08-24): every one read without error,
//!    the median has **43 008 bytes free** against a file of a few dozen, and
//!    **148 have less than a kilobyte** — many of them exactly zero. So the
//!    route is open for roughly 1 549 titles and closed for 148, which is a
//!    distinct ending rather than a failure, and a number worth having before
//!    anybody decides whether ART should write into somebody's game files at
//!    all.
//! 2. **Into a distribution's own Games drawer**, for a build that unpacks
//!    WHDLoad archives rather than carrying self-booting images — the
//!    ClassicWB/HstWB shape, which this owner's collection is not.
//!
//! [ART-147]: ../../../../docs/ISSUES.md
//!
//! # What is omitted is reported
//!
//! A value that will not fit the 64-byte line is **left out and named**,
//! never truncated. Truncating a title produces a wrong title on the Amiga's
//! screen, which is this project's own worst shape; leaving it out makes iGame
//! fall back to the drawer name, which is what it would have shown anyway.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::core::error::CoreResult;

/// What the file is called, in the game's own drawer beside its `.slave`.
pub const FILE_NAME: &str = "igame.data";

/// iGame's own `FGets` buffer, and the reason this module counts bytes.
///
/// `FGets` reads at most `lineSize - 1` characters and the newline is one of
/// them, so a line's `key=value` may occupy at most `LINE_BYTES - 2` bytes.
pub const LINE_BYTES: usize = 64;

/// The keys ART writes. Everything else in an existing file is passed through.
///
/// Exactly as `getIGameDataInfo` spells them: lower case, matched with
/// `strcmp`.
pub const MANAGED_KEYS: [&str; 6] = ["title", "chipset", "genre", "year", "players", "exe"];

/// What ART knows about one title, in iGame's own vocabulary.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IGameData {
    pub title: Option<String>,
    pub chipset: Option<String>,
    pub genre: Option<String>,
    pub year: Option<u16>,
    pub players: Option<u8>,
    /// The program to run, for a title that is **not** WHDLoad.
    ///
    /// iGame refuses a value containing `.slave`, so a slave path put here is
    /// silently dropped by the launcher. [`render`] refuses it here instead,
    /// where somebody can read the reason.
    pub exe: Option<String>,
}

/// Why a field ART knows did not reach the file.
///
/// **Reported rather than swallowed**: a title missing from `igame.data` is a
/// title iGame shows by its drawer name, and somebody should be able to find
/// out why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "why", rename_all = "kebab-case")]
pub enum Omitted {
    /// `key=value` would not fit iGame's 64-byte line, and truncating a title
    /// puts a wrong one on the Amiga's screen.
    TooLong { key: String, bytes: usize },
    /// iGame itself would discard it — today only an `exe` naming a slave.
    RefusedByIGame { key: String, reason: String },
    /// Nothing to say. Written down so "ART knows no year" and "ART could not
    /// fit the year" are not one sentence.
    Empty { key: String },
}

impl Omitted {
    /// The managed key this omission is about, whichever reason it carries.
    pub fn key(&self) -> &str {
        match self {
            Omitted::TooLong { key, .. }
            | Omitted::RefusedByIGame { key, .. }
            | Omitted::Empty { key } => key,
        }
    }

    /// Whether this is worth telling somebody about.
    ///
    /// **`Empty` is not.** Most titles have no genre, no year and no chipset
    /// ART could state — that is the ordinary case `render`'s own doc calls
    /// "ART knows no year", not a failure of anything. Surfacing it on every
    /// write would bury the two omissions somebody can actually act on
    /// (`TooLong`, `RefusedByIGame`) in noise that says nothing wrong
    /// happened.
    fn is_notable(&self) -> bool {
        !matches!(self, Omitted::Empty { .. })
    }
}

impl std::fmt::Display for Omitted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Omitted::TooLong { key, bytes } => write!(
                f,
                "{key} is {bytes} bytes — too long for iGame's 64-byte line"
            ),
            Omitted::RefusedByIGame { key, reason } => write!(f, "{key}: {reason}"),
            Omitted::Empty { key } => write!(f, "ART knows no {key}"),
        }
    }
}

/// The omissions worth telling a user about, in the order `render` found
/// them. English (ART-060), the same as a `CoreError`'s own message — the
/// screen shows it beside the translated sentence rather than inside it.
pub fn notable_omissions(omitted: &[Omitted]) -> Vec<String> {
    omitted
        .iter()
        .filter(|o| o.is_notable())
        .map(|o| o.to_string())
        .collect()
}

/// The file's text, and what did not go into it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rendered {
    pub text: String,
    pub omitted: Vec<Omitted>,
}

/// Would this `key=value` survive iGame's `FGets`?
fn fits(key: &str, value: &str) -> bool {
    // key + '=' + value, plus the newline FGets counts against the buffer.
    key.len() + 1 + value.len() + 1 < LINE_BYTES
}

/// Turn what ART knows into the lines iGame reads.
pub fn render(data: &IGameData) -> Rendered {
    let mut lines: Vec<String> = Vec::new();
    let mut omitted: Vec<Omitted> = Vec::new();

    let mut put = |key: &str, value: Option<String>, refusal: Option<String>| {
        let Some(value) = value.filter(|v| !v.trim().is_empty()) else {
            omitted.push(Omitted::Empty {
                key: key.to_string(),
            });
            return;
        };
        if let Some(reason) = refusal {
            omitted.push(Omitted::RefusedByIGame {
                key: key.to_string(),
                reason,
            });
            return;
        }
        if !fits(key, &value) {
            omitted.push(Omitted::TooLong {
                key: key.to_string(),
                bytes: key.len() + 1 + value.len() + 1,
            });
            return;
        }
        lines.push(format!("{key}={value}"));
    };

    put("title", data.title.clone(), None);
    put("genre", data.genre.clone(), None);
    // Zero is not a year and not a player count; iGame would store the zero.
    put(
        "year",
        data.year.filter(|y| *y > 0).map(|y| y.to_string()),
        None,
    );
    put(
        "players",
        data.players.filter(|p| *p > 0).map(|p| p.to_string()),
        None,
    );
    put("chipset", data.chipset.clone(), None);
    put(
        "exe",
        data.exe.clone(),
        data.exe
            .as_deref()
            .filter(|exe| exe.to_ascii_lowercase().contains(".slave"))
            .map(|_| {
                "iGame discards an `exe` naming a slave (`strcasestr(value, \".slave\")`); \
                 the slave is what WHDLoad runs, and `exe` is for titles that are not \
                 WHDLoad"
                    .to_string()
            }),
    );

    let text = if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    };
    Rendered { text, omitted }
}

/// Rewrite the keys ART manages inside an existing file, keeping everything
/// else exactly as it was.
///
/// CLAUDE.md's config-file rule: **never regenerate a user's file from
/// scratch.** iGame ignores keys it does not know, so somebody may keep their
/// own in here, and the ordering and any comments are theirs.
///
/// A managed key ART has nothing to say about is **left alone** rather than
/// deleted — removing a line the user wrote is not an edit, it is a loss.
pub fn merge_into(existing: &str, data: &IGameData) -> Rendered {
    let fresh = render(data);
    let mut replacements: Vec<(String, String)> = Vec::new();
    for line in fresh.text.lines() {
        if let Some((key, value)) = line.split_once('=') {
            replacements.push((key.to_string(), value.to_string()));
        }
    }

    let mut out: Vec<String> = Vec::new();
    let mut written: Vec<String> = Vec::new();
    for line in existing.lines() {
        let key = line.split_once('=').map(|(k, _)| k.to_string());
        match key.as_deref() {
            Some(key) if MANAGED_KEYS.contains(&key) => {
                match replacements.iter().find(|(k, _)| k == key) {
                    Some((k, v)) => {
                        out.push(format!("{k}={v}"));
                        written.push(k.clone());
                    }
                    // ART has nothing to say about this one. The user's line
                    // stays: deleting it is a loss, not an edit.
                    None => out.push(line.to_string()),
                }
            }
            // Unknown keys, comments, blank lines — verbatim.
            _ => out.push(line.to_string()),
        }
    }
    for (key, value) in &replacements {
        if !written.contains(key) {
            out.push(format!("{key}={value}"));
        }
    }

    let text = if out.is_empty() {
        String::new()
    } else {
        format!("{}\n", out.join("\n"))
    };
    Rendered {
        text,
        omitted: fresh.omitted,
    }
}

/// What one write into `<dir>/igame.data` settled on.
///
/// Decided from a **single** read of the file that may already be there —
/// the second half of I3's fix. `apply_one` used to read and render the file
/// once to decide whether anything would change, discard that, then call
/// [`write_beside`] which read and rendered it again to actually write —
/// two disk reads and two renders to answer one question. This enum is what
/// lets a caller get that answer *and* the write in one call.
///
/// **`AlreadyCurrent` and `NothingFit` are not the same fact, even though
/// both leave the file untouched.** The first is a file that already said
/// exactly what ART would write — a real "nothing to do". The second is a
/// title with nothing ART could put in the file at all: every managed field
/// was empty, too long for iGame's line, or refused by iGame itself.
/// Writing an *empty* `igame.data` for the second case (I2) would look
/// exactly like [`WriteOutcome::Written`] to anything checking whether a
/// file exists or counting bytes copied, and would say nothing to iGame —
/// CLAUDE.md's own worst class of defect, arriving here as a written,
/// verified file with nothing in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WriteOutcome {
    /// Nothing was there before; a fresh file was written.
    Written,
    /// A file was already there and its managed keys were rewritten in place.
    Merged,
    /// A file was already there and already said exactly this. Nothing was
    /// touched.
    AlreadyCurrent,
    /// Nothing survived iGame's own rules to write — every field was empty,
    /// too long, or refused. Nothing was touched: an existing file (if any)
    /// is left exactly as it was rather than blanked.
    NothingFit,
}

/// What writing one did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Written {
    pub path: String,
    pub outcome: WriteOutcome,
    /// Where the previous file went, taken only for [`WriteOutcome::Merged`]
    /// under a policy that keeps generations. `None` for every other
    /// outcome: nothing to preserve for a fresh file, and nothing changed for
    /// either `Unchanged`-shaped outcome.
    pub backup: Option<String>,
    pub omitted: Vec<Omitted>,
}

/// Put an `igame.data` beside a slave, editing one already there.
///
/// Reads the file **once** and decides everything from that one read:
/// whether it already says this ([`WriteOutcome::AlreadyCurrent`]), whether
/// there is nothing to say at all ([`WriteOutcome::NothingFit`]), or whether
/// to write — fresh or merged. Only the last case touches the file, and only
/// then through [`crate::core::safety::atomic_write`]: a truncated
/// `igame.data` is one iGame reads half of, and a half-written line is worse
/// than none.
///
/// `backup_policy` is the caller's own choice, because the two real callers
/// want different things from the exact same write: `commands/whdload.rs`'s
/// `run_install` passes [`crate::core::safety::BackupPolicy::NONE`] — nothing
/// of the user's is being touched, because ART unpacked this drawer moments
/// ago, so there is nothing to preserve — while `igamewrite::apply_one`
/// passes [`crate::core::safety::BackupPolicy::CONFIG`], because that
/// `igame.data` may be one the user (or another tool) wrote by hand. A backup
/// is only ever taken immediately before [`WriteOutcome::Merged`] actually
/// overwrites something; nothing is backed up for a fresh file or for either
/// outcome that leaves the file alone.
pub fn write_beside(
    drawer: &Path,
    data: &IGameData,
    backup_policy: crate::core::safety::BackupPolicy,
) -> CoreResult<Written> {
    let path = drawer.join(FILE_NAME);
    let existing = std::fs::read_to_string(&path).ok();
    let rendered = match &existing {
        Some(text) => merge_into(text, data),
        None => render(data),
    };

    if existing.as_deref() == Some(rendered.text.as_str()) {
        return Ok(Written {
            path: path.display().to_string(),
            outcome: WriteOutcome::AlreadyCurrent,
            backup: None,
            omitted: rendered.omitted,
        });
    }
    if rendered.text.is_empty() {
        return Ok(Written {
            path: path.display().to_string(),
            outcome: WriteOutcome::NothingFit,
            backup: None,
            omitted: rendered.omitted,
        });
    }

    let backup = if existing.is_some() {
        crate::core::safety::backup_file(&path, backup_policy)?
    } else {
        None
    };

    crate::core::safety::atomic_write(&path, rendered.text.as_bytes())?;
    Ok(Written {
        path: path.display().to_string(),
        outcome: if existing.is_some() {
            WriteOutcome::Merged
        } else {
            WriteOutcome::Written
        },
        backup: backup.map(|p| p.display().to_string()),
        omitted: rendered.omitted,
    })
}

/// How much room a WHDLoad hardfile has left, so a route can be judged rather
/// than guessed at.
///
/// **Read-only**, and deliberately so: whether ART should write into somebody's
/// 1 697 game images at all is a decision, and a decision wants a number in
/// front of it. Opens the volume through the reading half of
/// [`crate::core::volume`] — a `&dyn BlockDevice`, which the type system will
/// not let write.
pub fn free_bytes_in_hardfile(image: &std::path::Path) -> CoreResult<u64> {
    use crate::core::adf::blocks::{Bitmap, RootBlock};
    use crate::core::volume::mount::{mount, scan_image};
    use crate::core::volume::read_block_vec;

    let scanned = scan_image(image)?;
    let entry = scanned
        .volumes
        .iter()
        .find(|volume| volume.is_mountable())
        .ok_or_else(|| crate::core::error::CoreError::Malformed {
            format: "hardfile".into(),
            detail: "no mountable volume".into(),
        })?;
    let (device, geometry) = mount(image, entry)?;

    let root = RootBlock::parse(&read_block_vec(&device, geometry.root_block)?)?;
    let bitmap = Bitmap::parse(
        &read_block_vec(&device, root.bitmap_block)?,
        geometry.total_blocks as usize,
    )?;
    Ok(bitmap.free_count() as u64 * geometry.block_size as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ScratchDir;

    /// **Is there room to write into these at all?**
    ///
    /// Permanent and `#[ignore]`d, like every hook that needs the owner's own
    /// material. The question it answers is not about code: writing
    /// `igame.data` *inside* each hardfile is the only route this collection
    /// has, and a route into a volume with no free blocks is not a route.
    ///
    /// ```text
    /// cd src-tauri && ART_HDF_DIR="E:\amiga\Amigatolon\WHDload"     ///   cargo test how_much_room_the_hardfiles_have -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "needs the owner's own hardfiles; set ART_HDF_DIR"]
    fn how_much_room_the_hardfiles_have() {
        let Ok(dir) = std::env::var("ART_HDF_DIR") else {
            return;
        };

        fn walk(at: &Path, into: &mut Vec<std::path::PathBuf>) {
            let Ok(entries) = std::fs::read_dir(at) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, into);
                } else if path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("hdf"))
                {
                    into.push(path);
                }
            }
        }
        let mut images = Vec::new();
        walk(Path::new(&dir), &mut images);
        images.sort();
        println!("{} hardfiles", images.len());

        let mut free: Vec<u64> = Vec::new();
        let mut unreadable = 0usize;
        // A rendered file is a few dozen bytes, but AmigaDOS allocates whole
        // blocks: a header block plus at least one data block.
        let needed = 2 * 512u64;
        let mut too_tight = 0usize;
        for image in &images {
            match free_bytes_in_hardfile(image) {
                Ok(bytes) => {
                    if bytes < needed {
                        too_tight += 1;
                        if too_tight <= 5 {
                            println!("  tight: {} has {bytes} bytes free", image.display());
                        }
                    }
                    free.push(bytes);
                }
                Err(err) => {
                    unreadable += 1;
                    if unreadable <= 5 {
                        println!("  unreadable: {} - {err}", image.display());
                    }
                }
            }
        }
        free.sort_unstable();
        println!("read {}, unreadable {unreadable}", free.len());
        if !free.is_empty() {
            println!(
                "  free bytes: smallest {} median {} largest {}",
                free[0],
                free[free.len() / 2],
                free[free.len() - 1]
            );
            println!(
                "  with less than {needed} bytes free: {too_tight} of {}",
                free.len()
            );
        }
    }

    fn data() -> IGameData {
        IGameData {
            title: Some("Turrican II".into()),
            chipset: Some("ECS".into()),
            genre: Some("Shoot'em up".into()),
            year: Some(1991),
            players: Some(1),
            exe: None,
        }
    }

    #[test]
    fn it_writes_the_six_keys_igame_reads() {
        let out = render(&data());
        assert_eq!(
            out.text,
            "title=Turrican II\ngenre=Shoot'em up\nyear=1991\nplayers=1\nchipset=ECS\n"
        );
    }

    /// **No spaces around the `=`.** `strtok(line, "=")` makes `title = X`
    /// into the key `"title "`, which `strcmp` matches against nothing — a
    /// file that looks right and does nothing.
    #[test]
    fn there_is_no_space_around_the_separator() {
        assert!(!render(&data()).text.contains(" ="));
        assert!(!render(&data()).text.contains("= "));
    }

    /// **Keys are matched with `strcmp`**, so they are case-sensitive.
    #[test]
    fn the_keys_are_lower_case_exactly() {
        let text = render(&data()).text;
        for key in MANAGED_KEYS {
            if text.contains(key) {
                assert!(
                    text.contains(&format!("{key}=")),
                    "'{key}' must appear as a key, lower case: {text}"
                );
            }
        }
        assert!(!text.contains("Title="), "iGame would not match it");
    }

    /// **The one nobody would guess.** iGame reads with a 64-byte `FGets`, so
    /// a longer line is not truncated — it is split, and the tail arrives as
    /// the next line. A title with an `=` in it would then be read as a key.
    ///
    /// So it is left out and **named**, never shortened: a truncated title is
    /// a wrong title on the Amiga's screen, where an omitted one makes iGame
    /// fall back to the drawer name it would have shown anyway.
    #[test]
    fn a_value_that_would_not_survive_fgets_is_left_out_and_said() {
        let long = "A".repeat(80);
        let out = render(&IGameData {
            title: Some(long.clone()),
            ..data()
        });
        assert!(!out.text.contains(&long), "not written");
        assert!(!out.text.contains("title="), "and not truncated either");
        assert!(
            out.omitted
                .iter()
                .any(|o| matches!(o, Omitted::TooLong { key, .. } if key == "title")),
            "and reported: {:?}",
            out.omitted
        );
        // The rest of the file is unaffected.
        assert!(out.text.contains("year=1991"));
    }

    /// The boundary itself, since the whole rule is an arithmetic one.
    #[test]
    fn the_longest_line_that_fits_is_sixty_three_bytes() {
        // "title=" is 6 bytes; 56 more plus the newline makes 63.
        let just_fits = "B".repeat(56);
        assert!(render(&IGameData {
            title: Some(just_fits.clone()),
            ..IGameData::default()
        })
        .text
        .contains(&just_fits));

        let one_too_many = "B".repeat(57);
        assert!(!render(&IGameData {
            title: Some(one_too_many),
            ..IGameData::default()
        })
        .text
        .contains("title="));
    }

    /// **The trap.** The obvious thing to put in `exe` is the slave, and iGame
    /// throws away exactly that (`strcasestr(value, ".slave")`). Refusing it
    /// here means somebody can read the reason.
    #[test]
    fn an_exe_naming_a_slave_is_refused_where_the_reason_can_be_read() {
        let out = render(&IGameData {
            exe: Some("Turrican2/Turrican2.Slave".into()),
            ..data()
        });
        assert!(!out.text.contains("exe="));
        let Some(Omitted::RefusedByIGame { reason, .. }) = out
            .omitted
            .iter()
            .find(|o| matches!(o, Omitted::RefusedByIGame { key, .. } if key == "exe"))
        else {
            panic!("must be refused with a reason: {:?}", out.omitted);
        };
        assert!(reason.contains(".slave"), "{reason}");
    }

    /// An `exe` that is not a slave is ordinary and goes in.
    #[test]
    fn an_exe_that_is_not_a_slave_is_written() {
        let out = render(&IGameData {
            exe: Some("Game/StartGame".into()),
            ..IGameData::default()
        });
        assert!(out.text.contains("exe=Game/StartGame"));
    }

    /// iGame skips an empty value, so writing one is noise. And "ART knows no
    /// year" is reported as its own thing rather than as a failure to fit.
    #[test]
    fn nothing_known_is_nothing_written_and_says_which() {
        let out = render(&IGameData::default());
        assert_eq!(out.text, "");
        assert_eq!(
            out.omitted.len(),
            6,
            "one per managed key: {:?}",
            out.omitted
        );
        assert!(out
            .omitted
            .iter()
            .all(|o| matches!(o, Omitted::Empty { .. })));
    }

    /// `atoi` on a zero gives a zero, and iGame would store it. A year of 0 is
    /// not a year.
    #[test]
    fn a_zero_year_or_player_count_is_not_a_fact() {
        let out = render(&IGameData {
            year: Some(0),
            players: Some(0),
            ..IGameData::default()
        });
        assert!(!out.text.contains("year="));
        assert!(!out.text.contains("players="));
    }

    // -- editing somebody's own file ------------------------------------

    /// **CLAUDE.md's config rule.** iGame ignores keys it does not know, so a
    /// user may keep their own in here — and the ordering and any comments are
    /// theirs.
    #[test]
    fn an_existing_file_is_edited_and_everything_else_passes_through() {
        let existing = "; my own notes\ntitle=Old Name\nfavourite=yes\nyear=1990\n";
        let out = merge_into(existing, &data());

        assert!(out.text.contains("; my own notes"), "a comment is theirs");
        assert!(
            out.text.contains("favourite=yes"),
            "an unknown key is theirs"
        );
        assert!(
            out.text.contains("title=Turrican II"),
            "managed keys are rewritten"
        );
        assert!(!out.text.contains("Old Name"));
        assert!(out.text.contains("year=1991"));
        // Their ordering is kept: the comment first, then title, then
        // favourite.
        let lines: Vec<&str> = out.text.lines().collect();
        assert_eq!(lines[0], "; my own notes");
        assert_eq!(lines[1], "title=Turrican II");
        assert_eq!(lines[2], "favourite=yes");
    }

    /// A managed key ART has nothing to say about is **left alone**. Removing
    /// a line the user wrote is not an edit, it is a loss.
    #[test]
    fn a_managed_key_art_knows_nothing_about_is_not_deleted() {
        let existing = "genre=Puzzle\nplayers=4\n";
        let out = merge_into(
            existing,
            &IGameData {
                genre: Some("Shoot'em up".into()),
                ..IGameData::default()
            },
        );
        assert!(out.text.contains("genre=Shoot'em up"), "rewritten");
        assert!(
            out.text.contains("players=4"),
            "ART knows no player count, so theirs stands: {}",
            out.text
        );
    }

    /// A key ART knows and the file does not is appended rather than lost.
    #[test]
    fn something_new_is_added_at_the_end() {
        let out = merge_into("favourite=yes\n", &data());
        assert!(out.text.starts_with("favourite=yes"));
        assert!(out.text.contains("title=Turrican II"));
        assert!(out.text.contains("chipset=ECS"));
    }

    // -- putting it in the drawer ---------------------------------------

    #[test]
    fn it_lands_beside_the_slave() {
        let dir = ScratchDir::new("art-igame", "write");
        let drawer = dir.join("Turrican2");
        std::fs::create_dir_all(&drawer).unwrap();
        std::fs::write(drawer.join("Turrican2.slave"), b"not really a slave").unwrap();

        let done =
            write_beside(&drawer, &data(), crate::core::safety::BackupPolicy::CONFIG).unwrap();
        assert_eq!(
            done.outcome,
            WriteOutcome::Written,
            "there was nothing there"
        );
        // iGame looks in the same directory as the slave.
        let written = std::fs::read_to_string(drawer.join(FILE_NAME)).unwrap();
        assert!(written.contains("title=Turrican II"));
        assert_eq!(done.path, drawer.join(FILE_NAME).display().to_string());
    }

    #[test]
    fn writing_over_a_file_edits_it_and_says_so() {
        let dir = ScratchDir::new("art-igame", "merge");
        let drawer = dir.join("Lotus");
        std::fs::create_dir_all(&drawer).unwrap();
        std::fs::write(drawer.join(FILE_NAME), b"favourite=yes\ntitle=Mine\n").unwrap();

        let done =
            write_beside(&drawer, &data(), crate::core::safety::BackupPolicy::CONFIG).unwrap();
        assert_eq!(
            done.outcome,
            WriteOutcome::Merged,
            "and the screen can say the file was edited"
        );
        let written = std::fs::read_to_string(drawer.join(FILE_NAME)).unwrap();
        assert!(written.contains("favourite=yes"));
        assert!(written.contains("title=Turrican II"));
    }

    /// Every line iGame will read has to survive its buffer, whatever route it
    /// took into the file. A merge that let a long value through would be the
    /// same defect by another door.
    #[test]
    fn a_merged_file_still_has_no_line_igame_would_split() {
        let out = merge_into(
            "favourite=yes\n",
            &IGameData {
                title: Some("Z".repeat(200)),
                ..data()
            },
        );
        for line in out.text.lines() {
            assert!(
                line.len() + 1 < LINE_BYTES,
                "iGame would split this: {line}"
            );
        }
    }

    // -- the default path: a tree ART just built -------------------------

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-igame-beside-{tag}-{}",
            crate::core::test_scratch_id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn igame_data_lands_beside_the_slave() {
        let root = scratch("igame-beside");
        let dir = root.join("Games/Turrican");
        std::fs::create_dir_all(&dir).unwrap();
        let data = IGameData {
            title: Some("Turrican".into()),
            ..Default::default()
        };
        write_beside(&dir, &data, crate::core::safety::BackupPolicy::NONE).unwrap();
        let text = std::fs::read_to_string(dir.join(FILE_NAME)).unwrap();
        assert!(text.contains("title=Turrican"));

        std::fs::remove_dir_all(&root).ok();
    }

    /// The FF.CFG rule: somebody may have curated theirs by hand, and iGame
    /// silently ignores keys it does not know.
    #[test]
    fn an_existing_file_is_edited_and_its_own_keys_survive() {
        let root = scratch("igame-merge");
        let dir = root.join("Games/Tag");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE_NAME), "; mine\nfavourite=yes\ntitle=Old\n").unwrap();
        let data = IGameData {
            title: Some("Tag".into()),
            ..Default::default()
        };
        write_beside(&dir, &data, crate::core::safety::BackupPolicy::CONFIG).unwrap();
        let text = std::fs::read_to_string(dir.join(FILE_NAME)).unwrap();
        assert!(text.contains("; mine"), "a comment is not ART's to delete");
        assert!(
            text.contains("favourite=yes"),
            "an unknown key is not ART's to delete"
        );
        assert!(text.contains("title=Tag"));
        assert!(!text.contains("title=Old"));

        std::fs::remove_dir_all(&root).ok();
    }

    /// A title too long to fit, but a genre that does: the file is still
    /// written, the title is left out and named, and nothing is truncated.
    #[test]
    fn a_value_that_will_not_fit_is_left_out_and_named() {
        let root = scratch("igame-long");
        let dir = root.join("Games/Long");
        std::fs::create_dir_all(&dir).unwrap();
        let data = IGameData {
            title: Some("x".repeat(200)),
            genre: Some("Puzzle".into()),
            ..Default::default()
        };
        let written = write_beside(&dir, &data, crate::core::safety::BackupPolicy::NONE).unwrap();
        assert_eq!(written.outcome, WriteOutcome::Written);
        assert!(
            written.omitted.iter().any(|o| o.key() == "title"),
            "a truncated title is a wrong title on the Amiga's screen"
        );
        let text = std::fs::read_to_string(dir.join(FILE_NAME)).unwrap();
        assert!(!text.contains("title="));
        assert!(
            text.contains("genre=Puzzle"),
            "what does fit is still written"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    /// **I2's own fix.** A title too long to fit and nothing else known at
    /// all: before this fix, `render` returned an empty string and
    /// `write_beside` wrote it anyway — an `igame.data` with nothing in it,
    /// reported the same as a real write. Now nothing is written at all, the
    /// outcome says so by name, and the omission is still reported.
    #[test]
    fn a_title_alone_that_does_not_fit_writes_nothing_at_all() {
        let root = scratch("igame-nothing-fits");
        let dir = root.join("Games/NothingFits");
        std::fs::create_dir_all(&dir).unwrap();
        let data = IGameData {
            title: Some("x".repeat(200)),
            ..Default::default()
        };
        let written = write_beside(&dir, &data, crate::core::safety::BackupPolicy::NONE).unwrap();
        assert_eq!(
            written.outcome,
            WriteOutcome::NothingFit,
            "nothing survived to write, and that must not read as Written"
        );
        assert!(written.omitted.iter().any(|o| o.key() == "title"));
        assert!(
            !dir.join(FILE_NAME).exists(),
            "an empty igame.data is worse than no file: it looks written and says nothing"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    /// A file that already exists can never come back empty when nothing new
    /// fits. `merge_into` only ever appends to or rewrites an existing line —
    /// it never drops one — so its output cannot be shorter than the file it
    /// started from, and an existing non-empty file can never merge down to
    /// nothing. That is *why* this case surfaces as `AlreadyCurrent` rather
    /// than `NothingFit` (which can only happen for a **fresh** file — see
    /// `a_title_alone_that_does_not_fit_writes_nothing_at_all`): either way,
    /// nothing is touched, and somebody's own hand-curated `igame.data`
    /// survives ART having nothing new to add.
    #[test]
    fn nothing_fitting_does_not_blank_an_existing_file() {
        let root = scratch("igame-nothing-fits-existing");
        let dir = root.join("Games/Kept");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE_NAME), "; mine\nfavourite=yes\n").unwrap();
        let data = IGameData {
            title: Some("x".repeat(200)),
            ..Default::default()
        };
        let written = write_beside(&dir, &data, crate::core::safety::BackupPolicy::CONFIG).unwrap();
        assert_eq!(
            written.outcome,
            WriteOutcome::AlreadyCurrent,
            "nothing new fits, so the file merges back to exactly what it was"
        );
        assert!(
            written.omitted.iter().any(|o| o.key() == "title"),
            "the title that did not fit is still named, even though nothing was written"
        );
        assert_eq!(written.backup, None, "nothing changed, nothing to back up");
        assert_eq!(
            std::fs::read_to_string(dir.join(FILE_NAME)).unwrap(),
            "; mine\nfavourite=yes\n",
            "ART had nothing new to say, so the user's own file is untouched"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    /// A second write with nothing new is `AlreadyCurrent`, takes no backup,
    /// and touches nothing — `write_beside`'s own version of the idempotency
    /// `igamewrite::apply_one` used to compute for itself with a second read.
    #[test]
    fn a_second_identical_write_is_already_current_and_touches_nothing() {
        let root = scratch("igame-already-current");
        let dir = root.join("Games/Twice");
        std::fs::create_dir_all(&dir).unwrap();
        let first = write_beside(&dir, &data(), crate::core::safety::BackupPolicy::CONFIG).unwrap();
        assert_eq!(first.outcome, WriteOutcome::Written);
        let before = std::fs::read_to_string(dir.join(FILE_NAME)).unwrap();

        let second =
            write_beside(&dir, &data(), crate::core::safety::BackupPolicy::CONFIG).unwrap();
        assert_eq!(second.outcome, WriteOutcome::AlreadyCurrent);
        assert_eq!(second.backup, None, "nothing changed, nothing to back up");
        assert_eq!(
            std::fs::read_to_string(dir.join(FILE_NAME)).unwrap(),
            before,
            "the file itself is untouched"
        );

        std::fs::remove_dir_all(&root).ok();
    }
}
