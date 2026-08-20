//! A Cloanto RetroPlatform `.rp9` package, read for what it **states**.
//!
//! An `.rp9` is a zip holding the media, `rp9-manifest.xml` and usually
//! `rp9-preview.png`. The manifest is curated: title, publisher, year, genre,
//! rating, required Kickstart, target machine, disk order and a screenshot —
//! **242 packages** of it on this machine, all offline, all of which this reader
//! parses. The gap analysis budgeted "an optional online fetch, off by default"
//! for exactly these fields; here no fetch is needed at all, which is the
//! stronger form of §60's offline-first rule rather than an exception to it.
//!
//! `<type>` is the one thing here that `core/layout` says it will never derive —
//! and it is right, because this is not derived. The packager wrote it down.
//! Measured across those 242: 111 `demo`, 96 `game`, 15 `system`, 10 `gallery`,
//! 10 `video`. Only the first two are things a game launcher should list.
//!
//! The bytes arrive through `core/archive`'s gate, never from a path, so the
//! manifest is already bounded before `quick-xml` sees it.

use std::path::Path;

use quick_xml::events::Event;

use crate::core::archive::open;
use crate::core::error::{CoreError, CoreResult};
use crate::core::gameindex::record::TitleKind;

/// The largest `rp9-manifest.xml` ART will parse.
///
/// The real ones here are 1.8–2.0 KB. A quarter of a megabyte is already absurd
/// for this format; the point of a ceiling is that a hostile file reaches it and
/// a real one never does.
const MAX_MANIFEST_BYTES: u64 = 256 * 1024;

/// What an `.rp9` manifest states.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rp9Facts {
    pub title: String,
    pub kind: Option<TitleKind>,
    pub year: Option<u16>,
    pub publisher: Option<String>,
    /// The manifest's own vocabulary (`driving-simulation`), not iGame's.
    /// [`map_genre`] translates.
    pub genre: Option<String>,
    pub rating: Option<u8>,
    /// `<systemrom>310</systemrom>` — the Kickstart the package wants.
    pub system_rom: Option<u16>,
    /// `<system>a-1200</system>`.
    pub machine: Option<String>,
    /// Floppy images in `priority` order, not document order.
    pub floppies: Vec<String>,
    /// A `<harddrive>` entry, which is what the Demoscene packages here carry
    /// instead of floppies.
    pub hardfile: Option<String>,
    pub preview: Option<String>,
}

fn rp9_malformed(detail: impl std::fmt::Display) -> CoreError {
    CoreError::Malformed {
        format: "rp9".into(),
        detail: detail.to_string(),
    }
}

/// Read an `.rp9`'s manifest.
pub fn read_rp9(path: &Path) -> CoreResult<Rp9Facts> {
    parse_manifest(&manifest_bytes(path)?)
}

/// Pull `rp9-manifest.xml` out of the package, bounded.
fn manifest_bytes(path: &Path) -> CoreResult<Vec<u8>> {
    let mut archive = open(path)?;
    let entries = archive.entries()?;
    let index = entries
        .iter()
        .position(|entry| {
            !entry.is_dir
                && entry
                    .name
                    .rsplit(['/', '\\'])
                    .next()
                    .is_some_and(|name| name.eq_ignore_ascii_case("rp9-manifest.xml"))
        })
        .ok_or_else(|| rp9_malformed("no rp9-manifest.xml inside, so this is not an .rp9"))?;
    archive.read(index, MAX_MANIFEST_BYTES)
}

/// Walk the manifest.
///
/// Matches on **local names**, so the RetroPlatform namespace prefix never has
/// to be tracked.
///
/// Text is **accumulated and committed on the closing tag**, not assigned as it
/// arrives. quick-xml 0.41 splits a run of text at every entity reference —
/// `Rock &amp; Roll` comes through as `Text("Rock ")`, `GeneralRef("amp")`,
/// `Text(" Roll ")` — which is also why `unescape` is no longer a method on the
/// text event. Assigning per event keeps only the last fragment, and the title
/// `Rock &amp; Roll &lt;Deluxe&gt;` becomes `Deluxe`.
fn parse_manifest(xml: &[u8]) -> CoreResult<Rp9Facts> {
    let mut reader = quick_xml::Reader::from_reader(xml);
    // An unclosed element is a malformed manifest, not something to read as far
    // as it goes: without this, `<application><title>Half` parses "successfully"
    // and ART would treat a truncated download as a title.
    reader.config_mut().check_end_names = true;

    let mut facts = Rp9Facts::default();
    let mut floppies: Vec<(u32, String)> = Vec::new();
    let mut element: Option<Vec<u8>> = None;
    let mut text = String::new();
    let mut priority: u32 = u32::MAX;
    let mut entity_is_publisher = false;
    let mut image_is_preview = false;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.local_name().as_ref().to_vec();
                // Attributes are read while the element is open: a
                // `<floppy priority="1">` knows its order before its text
                // arrives, and an `<entity type="publisher">` knows whether its
                // text is a publisher at all.
                priority = attr(&e, b"priority")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(u32::MAX);
                if name == b"entity" {
                    entity_is_publisher = attr(&e, b"type").as_deref() == Some("publisher");
                }
                if name == b"image" {
                    image_is_preview = attr(&e, b"type").as_deref() == Some("screen-running");
                }
                element = Some(name);
                text.clear();
            }
            Ok(Event::Text(t)) if element.is_some() => {
                text.push_str(&t.decode().map_err(rp9_malformed)?);
            }
            Ok(Event::GeneralRef(r)) if element.is_some() => {
                // The event carries the reference's *name* (`amp`, `#38`).
                // Rebuilding `&name;` and handing it to `escape::unescape`
                // resolves both the predefined entities and numeric character
                // references in one place.
                let name = r.decode().map_err(rp9_malformed)?;
                let resolved = quick_xml::escape::unescape(&format!("&{name};"))
                    .map_err(rp9_malformed)?
                    .into_owned();
                text.push_str(&resolved);
            }
            Ok(Event::End(_)) => {
                let value = text.trim().to_string();
                if !value.is_empty() {
                    match element.as_deref() {
                        Some(b"title") => facts.title = value,
                        Some(b"year") => facts.year = value.parse().ok(),
                        Some(b"rating") => facts.rating = value.parse().ok(),
                        Some(b"systemrom") => facts.system_rom = value.parse().ok(),
                        Some(b"genre") => facts.genre = Some(value),
                        Some(b"system") => facts.machine = Some(value),
                        Some(b"type") => {
                            facts.kind = Some(match value.as_str() {
                                "game" => TitleKind::Game,
                                "demo" => TitleKind::Demo,
                                "system" => TitleKind::System,
                                "gallery" => TitleKind::Gallery,
                                "video" => TitleKind::Video,
                                // Kept verbatim rather than dropped: a word ART
                                // has not seen is still something the packager
                                // said.
                                other => TitleKind::Other(other.to_string()),
                            })
                        }
                        Some(b"entity") if entity_is_publisher => facts.publisher = Some(value),
                        Some(b"floppy") => floppies.push((priority, value)),
                        Some(b"harddrive") => facts.hardfile = Some(value),
                        Some(b"image") if image_is_preview => facts.preview = Some(value),
                        _ => {}
                    }
                }
                element = None;
                text.clear();
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(rp9_malformed(err)),
            _ => {}
        }
        buf.clear();
    }

    if facts.title.is_empty() {
        return Err(rp9_malformed("the manifest names no title"));
    }

    // `priority` decides the disk order, not document order. `sort_by_key` is
    // stable, so equal priorities keep the order they arrived in.
    floppies.sort_by_key(|(priority, _)| *priority);
    facts.floppies = floppies.into_iter().map(|(_, name)| name).collect();

    Ok(facts)
}

fn attr(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        (a.key.as_ref() == key).then(|| String::from_utf8_lossy(a.value.as_ref()).into_owned())
    })
}

/// Translate an `.rp9` genre into one of iGame's own.
///
/// iGame ships a fixed 21-entry list in the `genres` file inside its release
/// archive (read from `iGame-v2.6.1`): `Action`, `Adult`, `Adventure`,
/// `Bat and ball`, `Beat 'em up`, `Board`, `Cards`, `Demo`, `Gambling`, `Maze`,
/// `Misc`, `Pinball`, `Platform`, `Puzzle`, `Quiz`, `Racing`, `RPG`,
/// `Shoot 'em up`, `Simulation`, `Sports`, `Strategy`.
///
/// `.rp9`'s vocabulary is different and finer (`driving-simulation`,
/// `intro-40k`), so this maps what maps confidently and returns `None` for the
/// rest. The caller writes `Unknown` — iGame's own default — rather than
/// inventing a category, because a game filed under the wrong genre is worse
/// than one filed under none.
///
/// **`<genre>`'s vocabulary depends on `<type>`, and some of its values are not
/// genres at all.** Measured across the 242 real packages here (2026-08-17),
/// every value this function deliberately refuses to map belongs to a non-game
/// type:
///
/// ```text
/// history                                   → only on gallery (10) and video (10)
/// original · enhanced · prototype
///   · third-party                           → only on system (15 between them)
/// ```
///
/// Those describe what a system package or a picture collection *is*, not how a
/// game plays. Mapping `history` to an iGame genre would put a video under a
/// game category, so it stays `None` on purpose rather than for want of a
/// table entry.
pub fn map_genre(rp9_genre: &str) -> Option<&'static str> {
    let g = rp9_genre.trim().to_ascii_lowercase();

    // Demos first: an `.rp9` demo's genre describes the demo's own form
    // (`intro-40k`, `demo`, `musicdisk`, `slideshow`), and all of them are
    // iGame's `Demo`.
    if g.starts_with("intro") || g.starts_with("demo") || g.contains("disk-mag") || g == "slideshow"
    {
        return Some("Demo");
    }

    Some(match g.as_str() {
        "driving-simulation" | "racing" | "racing-driving" => "Racing",
        "shoot-em-up" | "shootemup" | "shooter" | "action-shooting" => "Shoot 'em up",
        "beat-em-up" | "fighting" => "Beat 'em up",
        "platform" | "platformer" => "Platform",
        "puzzle" | "mind-puzzle" => "Puzzle",
        "adventure" | "graphic-adventure" | "text-adventure" => "Adventure",
        "rpg" | "role-playing" | "role-adventure" => "RPG",
        "strategy" | "wargame" => "Strategy",
        "simulation" | "flight-simulation" => "Simulation",
        "sport" | "sports" => "Sports",
        "board-game" | "board" => "Board",
        "card-game" | "cards" => "Cards",
        "quiz" => "Quiz",
        "pinball" => "Pinball",
        "maze" => "Maze",
        "action" => "Action",
        "gambling" => "Gambling",
        "miscellaneous" | "misc" => "Misc",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rp9 xmlns="http://www.retroplatform.com">
  <application>
    <description>
      <type>game</type>
      <entity oid="1.2.3" priority="1" type="publisher">Insane Software</entity>
      <title>Aerial Racers</title>
      <year>1996</year>
      <rating>4</rating>
      <systemrom>310</systemrom>
      <genre>driving-simulation</genre>
    </description>
    <configuration><system>a-1200</system></configuration>
    <media>
      <floppy priority="2">two.adf</floppy>
      <floppy priority="1">one.adf</floppy>
      <floppy priority="3">three.adf</floppy>
    </media>
    <extras>
      <image root="embedded" type="screen-running" priority="1">rp9-preview.png</image>
    </extras>
  </application>
</rp9>"#;

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("art-rp9-{tag}-{}", crate::core::test_scratch_id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Build a `.rp9`: a zip carrying the manifest, a preview and a disk.
    fn write_rp9(dir: &Path, manifest: &str) -> PathBuf {
        write_zip(
            dir,
            "game.rp9",
            &[
                ("rp9-manifest.xml", manifest.as_bytes()),
                ("rp9-preview.png", b"not really a png"),
                ("one.adf", b"not really an adf"),
            ],
        )
    }

    fn write_zip(dir: &Path, name: &str, files: &[(&str, &[u8])]) -> PathBuf {
        let path = dir.join(name);
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (entry, bytes) in files {
            zip.start_file(*entry, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    /// Every field the format carries, read back — including the disk order,
    /// which arrives out of sequence in the XML on purpose.
    #[test]
    fn an_rp9_reads_its_manifest() {
        let dir = scratch("read");
        let path = write_rp9(&dir, MANIFEST);

        let facts = read_rp9(&path).unwrap();
        assert_eq!(facts.title, "Aerial Racers");
        assert_eq!(facts.kind, Some(TitleKind::Game));
        assert_eq!(facts.year, Some(1996));
        assert_eq!(facts.publisher.as_deref(), Some("Insane Software"));
        assert_eq!(facts.rating, Some(4));
        assert_eq!(facts.system_rom, Some(310));
        assert_eq!(facts.machine.as_deref(), Some("a-1200"));
        assert_eq!(facts.preview.as_deref(), Some("rp9-preview.png"));
        assert_eq!(
            facts.floppies,
            vec!["one.adf".to_string(), "two.adf".into(), "three.adf".into()],
            "priority decides the order, not document order"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// `<type>demo</type>` is a statement by the packager. It is the only thing
    /// that tells a demo from a game here, and the Demoscene packages on this
    /// machine carry a hardfile rather than floppies.
    #[test]
    fn a_demo_says_so() {
        let dir = scratch("demo");
        let manifest = MANIFEST
            .replace("<type>game</type>", "<type>demo</type>")
            .replace(
                r#"<floppy priority="2">two.adf</floppy>"#,
                r#"<harddrive priority="1">af-application.hdf</harddrive>"#,
            );
        let path = write_rp9(&dir, &manifest);

        let facts = read_rp9(&path).unwrap();
        assert_eq!(facts.kind, Some(TitleKind::Demo));
        assert_eq!(facts.hardfile.as_deref(), Some("af-application.hdf"));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A `<type>` ART has not seen is kept verbatim, not dropped.
    ///
    /// The first cut returned `None` here, which made "the packager said
    /// `application`" indistinguishable from "the packager said nothing". The
    /// real collection then produced thirty-five such packages.
    #[test]
    fn an_unknown_type_is_kept_verbatim() {
        let dir = scratch("othertype");
        let manifest = MANIFEST.replace("<type>game</type>", "<type>application</type>");
        let path = write_rp9(&dir, &manifest);

        assert_eq!(
            read_rp9(&path).unwrap().kind,
            Some(TitleKind::Other("application".into()))
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Malformed XML is refused with a reason, not half-read.
    #[test]
    fn a_broken_manifest_is_refused() {
        let dir = scratch("broken");
        let path = write_rp9(&dir, "<rp9><application><title>Half");
        assert!(read_rp9(&path).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A manifest that parses but names no title is refused too — an empty
    /// record that looks like a real one is worse than a refusal.
    #[test]
    fn a_manifest_with_no_title_is_refused() {
        let dir = scratch("notitle");
        let path = write_rp9(
            &dir,
            "<rp9><application><year>1996</year></application></rp9>",
        );
        let err = read_rp9(&path).unwrap_err();
        assert!(err.to_string().contains("no title"), "{err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A zip with no manifest is not an `.rp9`.
    #[test]
    fn a_zip_without_a_manifest_is_not_an_rp9() {
        let dir = scratch("nomanifest");
        let path = write_zip(&dir, "plain.rp9", &[("readme.txt", b"hello")]);
        let err = read_rp9(&path).unwrap_err();
        assert!(err.to_string().contains("not an .rp9"), "{err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Entity escapes are unescaped, which is half the reason this is a crate
    /// and not a hand-rolled reader.
    #[test]
    fn escaped_text_is_unescaped() {
        let dir = scratch("escapes");
        let manifest = MANIFEST.replace(
            "<title>Aerial Racers</title>",
            "<title>Rock &amp; Roll &lt;Deluxe&gt;</title>",
        );
        let path = write_rp9(&dir, &manifest);

        assert_eq!(read_rp9(&path).unwrap().title, "Rock & Roll <Deluxe>");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// iGame's genre list is fixed, so a genre outside it is `None` and the
    /// caller writes `Unknown` — iGame's own default.
    #[test]
    fn genres_map_onto_igames_own_vocabulary() {
        assert_eq!(map_genre("driving-simulation"), Some("Racing"));
        assert_eq!(map_genre("intro-40k"), Some("Demo"));
        assert_eq!(map_genre("shoot-em-up"), Some("Shoot 'em up"));
        assert_eq!(map_genre("Puzzle"), Some("Puzzle"), "case is folded");
        assert_eq!(map_genre("a-genre-nobody-has-heard-of"), None);

        // Every game genre the 242 real packages actually use.
        assert_eq!(map_genre("mind-puzzle"), Some("Puzzle"));
        assert_eq!(map_genre("action-shooting"), Some("Shoot 'em up"));
        assert_eq!(map_genre("role-adventure"), Some("RPG"));
        assert_eq!(map_genre("miscellaneous"), Some("Misc"));
        assert_eq!(map_genre("slideshow"), Some("Demo"));
    }

    /// The values that are **not** genres stay unmapped, deliberately.
    ///
    /// Measured on the 242 real packages: `history` appears only on `gallery`
    /// and `video` entries, and `original` / `enhanced` / `prototype` /
    /// `third-party` only on `system` ones. They describe what a package *is*,
    /// not how a game plays. This test exists so a later hand reading the
    /// unmapped-genre count does not "finish the table" and file a video under
    /// a game category.
    #[test]
    fn a_non_game_type_carries_words_that_are_not_genres() {
        for not_a_genre in [
            "history",
            "original",
            "enhanced",
            "prototype",
            "third-party",
        ] {
            assert_eq!(
                map_genre(not_a_genre),
                None,
                "{not_a_genre} is a non-game classification, not a genre"
            );
        }
    }

    /// The types measured across the real collection all survive the read, and
    /// the ones a launcher should not list say so.
    #[test]
    fn every_measured_type_is_kept() {
        let dir = scratch("types");
        for (word, expected, playable) in [
            ("game", TitleKind::Game, true),
            ("demo", TitleKind::Demo, true),
            ("system", TitleKind::System, false),
            ("gallery", TitleKind::Gallery, false),
            ("video", TitleKind::Video, false),
        ] {
            let manifest = MANIFEST.replace("<type>game</type>", &format!("<type>{word}</type>"));
            let path = write_rp9(&dir, &manifest);
            let facts = read_rp9(&path).unwrap();
            assert_eq!(facts.kind, Some(expected.clone()), "{word}");
            assert_eq!(expected.is_playable(), playable, "{word}");
            std::fs::remove_file(&path).ok();
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Real `.rp9` packages, read through the product's own path. `#[ignore]`d
    /// and env-gated: ART ships no copyrighted content.
    ///
    /// ```text
    /// cd src-tauri && ART_RP9_DIR="E:\amiga\Titles" \
    ///   cargo test read_the_real_rp9s_when_asked -- --nocapture --ignored
    /// ```
    ///
    /// **The number that matters is the unmapped-genre count**: it is what says
    /// whether [`map_genre`]'s table needs extending before anything writes an
    /// `igame.data`.
    #[test]
    #[ignore]
    fn read_the_real_rp9s_when_asked() {
        let Ok(dir) = std::env::var("ART_RP9_DIR") else {
            eprintln!("ART_RP9_DIR unset — skipping");
            return;
        };

        let found = walk_for_rp9(Path::new(&dir));
        let mut read = 0usize;
        let mut refused = 0usize;
        let mut unmapped: std::collections::BTreeMap<String, usize> = Default::default();
        let mut kinds: std::collections::BTreeMap<String, usize> = Default::default();
        let mut with_rom = 0usize;

        for entry in &found {
            match read_rp9(entry) {
                Ok(facts) => {
                    read += 1;
                    if read <= 5 {
                        println!(
                            "{}\n    {:?} {:?} {:?} genre={:?} rom={:?} disks={}",
                            entry.file_name().unwrap_or_default().to_string_lossy(),
                            facts.title,
                            facts.year,
                            facts.publisher,
                            facts.genre,
                            facts.system_rom,
                            facts.floppies.len()
                        );
                    }
                    *kinds.entry(format!("{:?}", facts.kind)).or_default() += 1;
                    if facts.system_rom.is_some() {
                        with_rom += 1;
                    }
                    if let Some(genre) = &facts.genre {
                        if map_genre(genre).is_none() {
                            *unmapped.entry(genre.clone()).or_default() += 1;
                        }
                    }
                }
                Err(err) => {
                    refused += 1;
                    println!("{}\n    REFUSED: {err}", entry.display());
                }
            }
        }

        println!("\n{} packages, {read} read, {refused} refused", found.len());
        println!("kinds: {kinds:?}");
        println!("{with_rom} state a systemrom");
        println!("{} unmapped genres:", unmapped.len());
        for (genre, count) in &unmapped {
            println!("    {genre} x{count}");
        }
        assert!(!found.is_empty(), "no .rp9 files found under {dir}");
    }

    fn walk_for_rp9(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return out;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(walk_for_rp9(&path));
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("rp9"))
            {
                out.push(path);
            }
        }
        out.sort();
        out
    }
}
