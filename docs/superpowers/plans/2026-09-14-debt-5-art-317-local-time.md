# Debt round 5 — ART-317: Amiga dates in local time — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every Amiga date ART makes from an instant — "now", a host file's mtime, a disc's recording date —
and every instant it reads back from an Amiga date, goes through local wall time, as AmigaDOS `DateStamp()`
does. The offset is the one in force on that date, obtained from Windows outside `core/`. A `.uaem` date is
already wall time and passes through untouched.

**Architecture:** `core/clock.rs` declares `AmigaClock`; `tools/local_time.rs` implements it with
`chrono::Local`. Every writer, copy source and reader that turns an instant into an Amiga date, or back, takes
the clock explicitly. Vendored libpfs3 gains an optional date on `FormatOptions` and `Writer` (`+art.4`). The
no-clock conveniences are `#[cfg(test)]`, so the product cannot compile without choosing a clock.

**Tech Stack:** Rust 1.93, `chrono` 0.4.45 (`clock` feature, outside `core/`), vendored `libpfs3`, Cargo, cargo-deny.

**Spec:** `docs/superpowers/specs/2026-09-14-art-317-local-time-design.md`

## Global Constraints

- Branch `art-debt-0914`. Run `git branch --show-current` before every commit. Never `git add -A`: name the files.
- **This plan runs after plan 3.** Plan 3 takes libpfs3 `+art.3`; this plan takes `+art.4`. Plans 1–4 may have
  moved the lines below, so re-find every `file:line` with `rg` before editing. Line numbers are as measured
  on 2026-09-14.
- ISSUES id **ART-317** was filed by plan 1. Do not renumber it.
- `src-tauri/src/core/` may use only std, serde, serde_json, sha2, log, thiserror, delharc, zip, sevenz-rust2,
  quick-xml, fatfs and libpfs3. **`chrono` is used only in `src-tauri/src/tools/local_time.rs`.**
- Product call sites pass `&crate::tools::local_time::LOCAL_TIME`. Tests pass `&crate::core::clock::UtcClock`
  when the date is not under test, or a `FixedClock`/`SeasonalClock` `static` when it is.
- Every Rust command runs as:
  `$env:TMP='E:\amiga\ProjeART\build\tmp'; $env:TEMP=$env:TMP; cd D:\Projeler\Amiga\amiga-retro-toolkit\src-tauri; cargo test --lib <name>`
- A test run is finished when it prints `test result:`, not when it exits 0. Never pipe a command that can fail.
  Write output to a file under `E:\amiga\ProjeART\build\tmp\` and `Select-String` the file.
- Mutation protocol: first `Copy-Item <absolute file> E:\amiga\ProjeART\build\tmp\art317-backup\` (create the
  folder once). Then mutate, run, and restore with `Copy-Item` from the backup. Never `git checkout --`.
- Commit messages are written with the Write tool to `E:\amiga\ProjeART\build\tmp\art317-commit.txt` and
  committed with `git commit -F E:\amiga\ProjeART\build\tmp\art317-commit.txt`.
- Test instants, used verbatim throughout:
  - `1_768_478_400` = 2026-01-15 12:00:00 UTC. Amiga day 17 546; at UTC 720 min, at +2 h 840 min, at +3 h 900 min.
  - `1_784_116_800` = 2026-07-15 12:00:00 UTC. Amiga day 17 727.
  - `1_774_746_000` = 2026-03-29 01:00:00 UTC, the EU spring switch.

## Call-site inventory (measured 2026-09-14)

| Where a date is made or read | Today | Task |
|---|---|---|
| `core/volume/write/layout.rs:104-110` `amiga_now` (callers `dir.rs:397`, `file.rs:208`) | UTC now | 3 |
| `core/volume/write/layout.rs:114-121` `amiga_from_unix` (callers `copy.rs:388`, `iso/mod.rs:545`, `iso/mod.rs:810`, `commands/iso.rs:175`, `osinstall/source_cd.rs:156`) | UTC instant → Amiga | 5, 7, 8 |
| `core/adf/create.rs:192-204` `get_current_amiga_date` (caller `create.rs:86`) | UTC now | 4 |
| `core/preload/native.rs:642-655` `current_amiga_date` (caller `native.rs:573`) | UTC now | 6 |
| `vendor/libpfs3/src/util.rs:163-172` (callers `format.rs:114`, `writer.rs:571`, `writer.rs:1178`) | UTC now | 6 |
| `core/adf/bcpl.rs:76-81` `AmigaDate::to_unix` (caller `core/adf/fs.rs:87`) | Amiga → UTC instant | 7 |

| Product site that constructs a writer or a dated source | Task |
|---|---|
| `commands/volume_write.rs:171` (`with_volume`, BlockJournal), `:288` (`WholeFileVolume::writer`), `:1470` (`run_copy_in_folder_with`), `:2067` (`run_copy_in_staged_with`), `:2135` (`volume_attributes`) | 3 |
| `commands/adf.rs:176` `save_new_adf` | 4 |
| `core/volume/write/copy.rs:214` `source.metadata` (reaches every `HostFolder`/`HostSelection`/`IsoSource` built in `commands/archives.rs:176,308`, `commands/archive.rs:396`, `commands/sources.rs:660`, `commands/volume_write.rs:943,979,1210,1277`, `commands/iso.rs:357,363`, `core/whdload/install.rs:646`) — the constructors do not change | 5 |
| `commands/preload.rs:611` `NativeFormatter`; `core/preload/native.rs:189` `FormatOptions`, `:895` `Writer::open`, `:997` `VolumeWriter::open` | 6 |
| `commands/volume.rs:65` and `commands/adf.rs:137-138` (listings); `commands/iso.rs:175` (single-file sidecar) and `:262` (`extract_tree`) | 7 |
| `commands/osinstall.rs:394` `plan_with_cache_in`, `:1992` `open_media`, `:2791` `apply_staging_in`; `core/osinstall/apply.rs:1328`, `plan.rs:1705`, `scan.rs:137,163,166` | 8 |

`core/whdload/install.rs:287` calls only `entries()` (no date), and `:842` is inside its tests. `core/card/sizing.rs:661,717`
is inside tests (from `:564`). `core/adf/mutate.rs`, named in the research note and in ART-317, does not exist
(`Glob src-tauri/src/**/mutate*.rs` finds nothing).

**Dates that must pass through untouched — they are already wall time, and applying an offset would be the bug:**

| Where | Why no offset | Guard |
|---|---|---|
| `.uaem` in: `core/volume/write/uaem.rs:206` `amiga_from_civil` → `copy.rs:372-373` → the writer; `core/preload/native.rs:1066-1078` (FFS `copy_in`) | zone-less text straight to an Amiga triplet, no UTC step | Task 5 `a_uaem_date_round_trips_without_an_offset`; Task 6 FFS test |
| `.uaem` out: `copy.rs::extract_from_volume` → `sidecar_for` → `uaem::render` (`uaem.rs:193`) | the volume's triplet rendered as text | Task 5 `a_uaem_date_round_trips_without_an_offset` |
| `commands/volume_write.rs:2152` `date_text` | the same renderer | none new (no conversion to guard) |
| `core/osinstall/source.rs` (ADF install media) | an Amiga date copied to an Amiga date | none new |

---

### Task 1: The clock trait in `core/`

**Files:**
- Create: `src-tauri/src/core/clock.rs`
- Modify: `src-tauri/src/core/mod.rs` (module list, `:9-60`)
- Modify: `src-tauri/src/core/adf/bcpl.rs:75-91`

**Interfaces:**
- Produces: `crate::core::clock::{AmigaClock, UtcClock, FixedClock, amiga_from_wall, system_now_unix}`,
  `#[cfg(test)] crate::core::clock::SeasonalClock`, and `AmigaDate::to_wall_seconds(self) -> i64`.
  - `trait AmigaClock: Send + Sync { fn now_unix(&self) -> i64; fn offset_at(&self, unix: i64) -> i32; fn amiga_from_unix(&self, unix: i64) -> AmigaDate; fn amiga_now(&self) -> AmigaDate; fn unix_from_amiga(&self, date: AmigaDate) -> i64; }`
  - `pub struct FixedClock { pub now: i64, pub offset: i32 }`
  - `pub struct SeasonalClock { pub now: i64, pub switch_at: i64, pub before: i32, pub after: i32 }`

- [ ] **Step 1: Write the failing tests.** Create `src-tauri/src/core/clock.rs` holding only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::adf::bcpl::{AmigaDate, AMIGA_EPOCH_UNIX};

    const JAN_15_NOON_UTC: i64 = 1_768_478_400;
    const JUL_15_NOON_UTC: i64 = 1_784_116_800;
    const EU_SPRING_SWITCH: i64 = 1_774_746_000;

    fn seasons() -> SeasonalClock {
        SeasonalClock { now: JUL_15_NOON_UTC, switch_at: EU_SPRING_SWITCH, before: 7_200, after: 10_800 }
    }

    #[test]
    fn utc_changes_nothing() {
        let date = UtcClock.amiga_from_unix(JAN_15_NOON_UTC);
        assert_eq!(date, AmigaDate { days: 17_546, mins: 720, ticks: 0 });
        assert_eq!(UtcClock.unix_from_amiga(date), JAN_15_NOON_UTC);
    }

    #[test]
    fn a_fixed_offset_moves_the_wall_clock_and_comes_back() {
        let clock = FixedClock { now: JAN_15_NOON_UTC, offset: 10_800 };
        assert_eq!(clock.amiga_now(), AmigaDate { days: 17_546, mins: 900, ticks: 0 });
        assert_eq!(clock.unix_from_amiga(clock.amiga_now()), JAN_15_NOON_UTC);
    }

    /// ART-317's DST rule: the offset in force on the date itself, not today's.
    /// `FileTimeToLocalFileTime` gets exactly this wrong (Microsoft Learn, "File Times").
    #[test]
    fn the_offset_is_the_one_in_force_at_the_date_not_now() {
        let clock = seasons();
        assert_eq!(clock.amiga_from_unix(JAN_15_NOON_UTC).mins, 14 * 60, "winter, UTC+2");
        assert_eq!(clock.amiga_from_unix(JUL_15_NOON_UTC).mins, 15 * 60, "summer, UTC+3");
    }

    #[test]
    fn a_round_trip_survives_both_sides_of_the_switch() {
        let clock = seasons();
        for unix in [
            EU_SPRING_SWITCH - 1_800,
            EU_SPRING_SWITCH,
            EU_SPRING_SWITCH + 1_800,
            JAN_15_NOON_UTC,
            JUL_15_NOON_UTC,
        ] {
            assert_eq!(clock.unix_from_amiga(clock.amiga_from_unix(unix)), unix, "at {unix}");
        }
    }

    #[test]
    fn a_date_before_the_amiga_epoch_clamps_after_the_offset() {
        let clock = FixedClock { now: 0, offset: -3_600 };
        assert_eq!(clock.amiga_from_unix(AMIGA_EPOCH_UNIX), AmigaDate::default());
    }
}
```

Add `pub mod clock;` to `src-tauri/src/core/mod.rs` in alphabetical order, after `pub mod cbm;`.

- [ ] **Step 2: Run to see it fail**

Run: `$env:TMP='E:\amiga\ProjeART\build\tmp'; $env:TEMP=$env:TMP; cd D:\Projeler\Amiga\amiga-retro-toolkit\src-tauri; cargo test --lib core::clock::`
Expected: compile error `cannot find type SeasonalClock` / `UtcClock` / `FixedClock`.

- [ ] **Step 3: Implement.** In `core/adf/bcpl.rs`, add inside `impl AmigaDate`, above `to_unix`:

```rust
    /// Seconds since 1970-01-01 **as the wall clock reads**, with no zone —
    /// what AmigaDOS stores (ART-317). Not an instant: turning it into one
    /// needs the offset in force on that date, which is
    /// `core::clock::AmigaClock::unix_from_amiga`'s job.
    pub fn to_wall_seconds(self) -> i64 {
        AMIGA_EPOCH_UNIX
            + (self.days as i64) * 86_400
            + (self.mins as i64) * 60
            + (self.ticks as i64) / TICKS_PER_SEC
    }
```

Put this above the test module in `core/clock.rs`:

```rust
//! Which clock an Amiga date is read against (ART-317).
//!
//! AmigaDOS `DateStamp()` is local wall time with no zone, and pfs3aio stamps
//! with it. An instant — a host file's mtime, a disc's recording date, "now" —
//! becomes an Amiga date only through the offset **in force on that date**,
//! and that offset is a platform question. So `core/` declares the trait and
//! `tools/local_time.rs` implements it (CLAUDE.md's core-independence rule).
//! `UtcClock` is ART's behaviour before ART-317, kept for tests and for
//! callers that read no date.

use crate::core::adf::bcpl::{AmigaDate, AMIGA_EPOCH_UNIX, TICKS_PER_SEC};

/// Where "now" and the UTC offset come from.
pub trait AmigaClock: Send + Sync {
    /// Seconds since 1970-01-01 UTC.
    fn now_unix(&self) -> i64;

    /// Seconds east of UTC in force at the UTC instant `unix`.
    fn offset_at(&self, unix: i64) -> i32;

    /// The Amiga date a wall clock here showed at the instant `unix`.
    fn amiga_from_unix(&self, unix: i64) -> AmigaDate {
        amiga_from_wall(unix + i64::from(self.offset_at(unix)))
    }

    /// Now, as AmigaDOS would stamp it.
    fn amiga_now(&self) -> AmigaDate {
        self.amiga_from_unix(self.now_unix())
    }

    /// The instant an Amiga date names. Two passes: the first guesses the
    /// instant with the offset at the wall time, the second corrects with the
    /// offset at that guess, which is right on both sides of a DST switch. A
    /// wall time that happens twice (autumn) resolves to one of them, and one
    /// that never happens (spring) to the nearest instant.
    fn unix_from_amiga(&self, date: AmigaDate) -> i64 {
        let wall = date.to_wall_seconds();
        let first = wall - i64::from(self.offset_at(wall));
        wall - i64::from(self.offset_at(first))
    }
}

/// The Amiga triplet for wall-clock seconds since 1970. Anything before 1978
/// clamps to the epoch rather than wrapping (§4.1): a negative day count shows
/// as a date far in the future on a real Amiga.
pub fn amiga_from_wall(wall: i64) -> AmigaDate {
    let since = (wall - AMIGA_EPOCH_UNIX).max(0);
    AmigaDate {
        days: (since / 86_400) as u32,
        mins: ((since % 86_400) / 60) as u32,
        ticks: ((since % 60) * TICKS_PER_SEC) as u32,
    }
}

/// The system clock, in whole seconds since 1970 UTC. A clock set before
/// 1970 reads as the Amiga epoch, as ART's writers always did.
pub fn system_now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(AMIGA_EPOCH_UNIX)
}

/// UTC: ART's behaviour before ART-317.
#[derive(Debug, Default, Clone, Copy)]
pub struct UtcClock;

impl AmigaClock for UtcClock {
    fn now_unix(&self) -> i64 {
        system_now_unix()
    }
    fn offset_at(&self, _unix: i64) -> i32 {
        0
    }
}

/// A stopped clock with one offset — deterministic stamping.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock {
    pub now: i64,
    pub offset: i32,
}

impl AmigaClock for FixedClock {
    fn now_unix(&self) -> i64 {
        self.now
    }
    fn offset_at(&self, _unix: i64) -> i32 {
        self.offset
    }
}

/// A stopped clock with one DST switch: `before` until `switch_at`, `after`
/// from it on.
#[cfg(test)]
#[derive(Debug, Clone, Copy)]
pub struct SeasonalClock {
    pub now: i64,
    pub switch_at: i64,
    pub before: i32,
    pub after: i32,
}

#[cfg(test)]
impl AmigaClock for SeasonalClock {
    fn now_unix(&self) -> i64 {
        self.now
    }
    fn offset_at(&self, unix: i64) -> i32 {
        if unix >= self.switch_at {
            self.after
        } else {
            self.before
        }
    }
}
```

- [ ] **Step 4: Run to see it pass**

Run: `$env:TMP='E:\amiga\ProjeART\build\tmp'; $env:TEMP=$env:TMP; cd D:\Projeler\Amiga\amiga-retro-toolkit\src-tauri; cargo test --lib core::clock::`
Expected: `test result: ok. 5 passed`.

- [ ] **Step 5: Mutate the guard.** Back up `clock.rs`. In `unix_from_amiga`, replace the last line with
`first` (one pass). Run the same command. Expected: `a_round_trip_survives_both_sides_of_the_switch` FAILS
"at 1774744200". Restore from the backup and re-run: all 5 pass.

- [ ] **Step 6: Commit**

`git branch --show-current` → `art-debt-0914`. Message file: `core: AmigaClock, the clock an Amiga date is read against (ART-317)`.
`git add src-tauri/src/core/clock.rs src-tauri/src/core/mod.rs src-tauri/src/core/adf/bcpl.rs`
`git commit -F E:\amiga\ProjeART\build\tmp\art317-commit.txt`

---

### Task 2: The Windows clock in `tools/`

**Files:**
- Create: `src-tauri/src/tools/local_time.rs`
- Modify: `src-tauri/src/tools/mod.rs:11-13`, `src-tauri/Cargo.toml` (after the `trash` block, `:198`), `THIRD_PARTY_LICENSES.md` (after the `trash` entry, `:78`)
- Modify: `src-tauri/src/core/mod.rs` (the `independence` test module, after `core_never_spawns_a_process_outside_a_test`, `:858`)

**Interfaces:**
- Consumes: `crate::core::clock::{AmigaClock, system_now_unix}`.
- Produces: `crate::tools::local_time::{WindowsLocalTime, LOCAL_TIME}`, where `pub static LOCAL_TIME: WindowsLocalTime`.

- [ ] **Step 1: Write the core-side guard first** (it passes today and has to keep passing). In `core/mod.rs`'s `mod independence`, after `core_never_spawns_a_process_outside_a_test`:

```rust
    /// ART-317: the UTC offset is a platform question, answered by
    /// `tools/local_time.rs` through `core::clock::AmigaClock`. A time-zone
    /// crate named inside `core/` would put the answer back where it cannot
    /// be tested with a fixed clock.
    ///
    /// Mutate by adding `use chrono::Local;` to `core/clock.rs`; this fails.
    #[test]
    fn core_never_names_a_time_zone_crate() {
        // Built with `concat!` so this file's own source does not match.
        let needles = [concat!("chro", "no::"), concat!("use chro", "no"), concat!("iana_time", "_zone")];
        let mut offenders = Vec::new();
        for path in core_files() {
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|err| panic!("reading {}: {err}", path.display()));
            for (n, line) in text.lines().enumerate() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                if needles.iter().any(|needle| line.contains(needle)) {
                    offenders.push(format!("{}:{}: {}", path.display(), n + 1, line.trim()));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "core/ named a time-zone crate — the offset belongs in tools/local_time.rs (ART-317):\n{}",
            offenders.join("\n")
        );
    }
```

Run: `...; cargo test --lib core_never_names_a_time_zone_crate` → `test result: ok. 1 passed`.
Mutate: back up `core/clock.rs`, add `use chrono::Local;` at the top, run again → FAILS naming `clock.rs:…`. Restore.

- [ ] **Step 2: Write the Windows implementation's tests.** Create `src-tauri/src/tools/local_time.rs` with only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::clock::system_now_unix;

    /// What this can prove on CI, whose runners are UTC: the answer has the
    /// shape of a real offset. It cannot prove it is *this* machine's offset
    /// — the ignored test below does that.
    #[test]
    fn the_offset_is_a_whole_quarter_hour_inside_the_real_range() {
        for unix in [1_768_478_400, 1_784_116_800, LOCAL_TIME.now_unix()] {
            let offset = LOCAL_TIME.offset_at(unix);
            assert!((-14 * 3_600..=14 * 3_600).contains(&offset), "{offset} at {unix}");
            assert_eq!(offset % 900, 0, "{offset} at {unix}");
        }
    }

    #[test]
    fn now_is_the_system_clock() {
        let before = system_now_unix();
        let now = LOCAL_TIME.now_unix();
        assert!((before..=before + 2).contains(&now), "{before} vs {now}");
    }

    /// The real check, on the owner's machine: .NET's `TimeZoneInfo` is a
    /// separate implementation of the same registry rules. Istanbul observed
    /// DST until 2016, so 2015's January and July disagree there, and 2026's
    /// agree. Run with `ART_TZ_ORACLE=1`.
    #[test]
    #[ignore = "asks .NET TimeZoneInfo on this machine; set ART_TZ_ORACLE=1"]
    fn agrees_with_dotnet_timezoneinfo_across_seasons_and_years() {
        if std::env::var_os("ART_TZ_ORACLE").is_none() {
            return;
        }
        // 2015-01-01 12:00Z, 2015-07-15 12:00Z, 2026-01-15 12:00Z, 2026-07-15 12:00Z
        for unix in [1_420_113_600_i64, 1_436_961_600, 1_768_478_400, 1_784_116_800] {
            let script = format!(
                "[int][System.TimeZoneInfo]::Local.GetUtcOffset([DateTimeOffset]::FromUnixTimeSeconds({unix})).TotalSeconds"
            );
            let out = std::process::Command::new("powershell")
                .args(["-NoProfile", "-Command", &script])
                .output()
                .expect("powershell runs");
            let dotnet: i32 = String::from_utf8_lossy(&out.stdout).trim().parse().expect("an integer");
            println!("{unix}: chrono {} / .NET {dotnet}", LOCAL_TIME.offset_at(unix));
            assert_eq!(LOCAL_TIME.offset_at(unix), dotnet, "at {unix}");
        }
    }
}
```

Add `pub mod local_time;` to `tools/mod.rs` between `hst_imager` and `recycle_bin`.

- [ ] **Step 3: Run to see it fail**

Run: `...; cargo test --lib tools::local_time::`
Expected: compile error `cannot find value LOCAL_TIME`.

- [ ] **Step 4: Add the dependency.** In `src-tauri/Cargo.toml`, after the `trash = …` line (`:198`):

```toml

# The local UTC offset, for every Amiga date ART writes or shows (ART-317).
#
# AmigaDOS stamps local wall time with no zone. The offset in force on a given
# date is a Windows question, so it is asked in `tools/local_time.rs`, never in
# `core/` (which declares `core::clock::AmigaClock`). chrono's Windows backend
# calls `GetTimeZoneInformationForYear` for the year of the instant converted,
# so a file modified in winter keeps its winter offset when copied in summer —
# unlike `FileTimeToLocalFileTime`, which applies today's. Already in the build
# through `delharc` and `serde_with`, with `clock` on; this makes it a direct
# dependency instead of an incidental one. MIT OR Apache-2.0.
chrono = { version = "0.4.45", default-features = false, features = ["clock"] }
```

- [ ] **Step 5: Implement.** Above the test module in `tools/local_time.rs`:

```rust
//! The machine's own UTC offset, as ART's [`AmigaClock`] (ART-317).
//!
//! `core/clock.rs` declares the trait and this answers it, for the same reason
//! `recycle_bin.rs` beside it exists: the answer is Windows' (CLAUDE.md's
//! core-independence rule). `chrono::Local` asks `GetTimeZoneInformationForYear`
//! for the year of each instant, uncached, so the offset is the one in force on
//! that date and follows a time-zone change without a restart.

use chrono::{Local, TimeZone};

use crate::core::clock::{system_now_unix, AmigaClock};

/// Windows' time zone, per date. Stateless.
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsLocalTime;

/// The clock every product call site passes.
pub static LOCAL_TIME: WindowsLocalTime = WindowsLocalTime;

impl AmigaClock for WindowsLocalTime {
    fn now_unix(&self) -> i64 {
        system_now_unix()
    }

    /// `0` only for an instant outside chrono's ±262 000-year range, where no
    /// file or disc date can be.
    fn offset_at(&self, unix: i64) -> i32 {
        Local
            .timestamp_opt(unix, 0)
            .single()
            .map(|at| at.offset().local_minus_utc())
            .unwrap_or(0)
    }
}
```

- [ ] **Step 6: Run to see it pass, and check the build graph**

Run: `...; cargo test --lib tools::local_time::` → `test result: ok. 2 passed; 0 failed; 1 ignored`.
Run: `...; cargo tree -i chrono -e normal --depth 1` → `amiga-retro-toolkit` is listed among chrono's dependents.
Run: `cd D:\Projeler\Amiga\amiga-retro-toolkit; git diff --stat -- src-tauri/Cargo.lock` → at most one line
changed: `"chrono",` added to `amiga-retro-toolkit`'s dependency list. No new `[[package]]`.
Run: `cd D:\Projeler\Amiga\amiga-retro-toolkit; cargo deny --manifest-path src-tauri/Cargo.toml check` → `licenses ok`, `bans ok`, `advisories ok`, `sources ok`.

- [ ] **Step 7: Mutate.** Back up `local_time.rs`. Make `offset_at` return `1` → `the_offset_is_a_whole_quarter_hour…` FAILS.
Make it return `0` → that test **survives on CI** (a UTC runner answers 0 anyway). Disclose this survivor in the
commit message. On the owner's UTC+3 machine,
`$env:ART_TZ_ORACLE='1'; cargo test --lib agrees_with_dotnet -- --ignored --nocapture` FAILS on it. Restore.

- [ ] **Step 8: Record the licence.** In `THIRD_PARTY_LICENSES.md`, after the `trash` entry:

```markdown
- **chrono** — the local UTC offset in force on a given date (MIT / Apache-2.0), with `iana-time-zone`
  (MIT / Apache-2.0) and `windows-link` (MIT / Apache-2.0) beneath it, all already in the build through
  `delharc` and `serde_with`. **Outside `core/`**, in `tools/local_time.rs`: `core::clock::AmigaClock` declares
  the question and this answers it, so every Amiga date ART writes is local wall time, as AmigaDOS stamps it
  (ART-317). `core/` stays free of it, and `core::independence::core_never_names_a_time_zone_crate` keeps it so
```

- [ ] **Step 9: Commit** — `tools: the machine's own UTC offset through chrono::Local (ART-317)`. The body names
the CI survivor from Step 7.
`git add src-tauri/src/tools/local_time.rs src-tauri/src/tools/mod.rs src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/core/mod.rs THIRD_PARTY_LICENSES.md`

---

### Task 3: ART's FFS/OFS writer stamps with its clock

**Files:**
- Modify: `src-tauri/src/core/volume/write/mod.rs:181-248` (struct and `open`), `:474-483` (`add_file`), `:530` (`make_dir`); tests after `:1171`
- Modify: `src-tauri/src/core/volume/write/dir.rs:28` (import), `:389-404`; test calls `:580,631,659,690,731,778,822,843,867`
- Modify: `src-tauri/src/core/volume/write/file.rs:25` (import), `:112-121`, `:208`; test calls `:443,470,503,556,587,617,640,670,701,730,759`
- Modify: `src-tauri/src/core/volume/write/layout.rs:99-110` (delete `amiga_now`)
- Modify: `src-tauri/src/commands/volume_write.rs:171, :288, :1470, :2067, :2135`

**Interfaces:**
- Consumes: `AmigaClock`, `FixedClock`, `UtcClock` (Task 1); `LOCAL_TIME` (Task 2).
- Produces:
  - `VolumeWriter::open_with_clock(device: &'a mut dyn BlockDeviceMut, geometry: VolumeGeometry, image: &Path, volume_offset: u64, clock: &'static dyn AmigaClock) -> CoreResult<Self>`
  - `VolumeWriter::clock(&self) -> &'static dyn AmigaClock`
  - `VolumeWriter::open(...)` is unchanged in signature; it delegates with `&UtcClock` (Task 9 makes it test-only)
  - `dir::write_dir_header(set, block, parent, name, now: AmigaDate)`
  - `file::write_file_blocks(..., protection: u32, date: AmigaDate)`

- [ ] **Step 1: Write the failing test** in `core/volume/write/mod.rs`'s `mod tests`:

```rust
    static PLUS_THREE: crate::core::clock::FixedClock =
        crate::core::clock::FixedClock { now: 1_768_478_400, offset: 10_800 };

    /// ART-317: a drawer and a file ART creates carry the writer's local wall
    /// time — 15:00 at 12:00 UTC on a UTC+3 clock — not UTC.
    #[test]
    fn a_new_drawer_and_a_new_file_carry_the_writers_local_time() {
        let disk = floppy("local-time");
        let mut device = disk.device();
        let mut writer =
            VolumeWriter::open_with_clock(&mut device, disk.geometry, &disk.path, 0, &PLUS_THREE)
                .unwrap();
        let drawer = writer.make_dir(0, "Tools").unwrap().block.unwrap();
        let file = writer.add_file(0, "Readme", b"hi", FileMeta::default()).unwrap().block.unwrap();

        let local = AmigaDate { days: 17_546, mins: 15 * 60, ticks: 0 };
        assert_eq!(writer.attributes(drawer).unwrap().date, local, "drawer");
        assert_eq!(writer.attributes(file).unwrap().date, local, "file");
    }
```

- [ ] **Step 2: Add only the constructor, so the red is behavioural.** In `mod.rs`, add
`use crate::core::clock::AmigaClock;`, add the field `clock: &'static dyn AmigaClock` as the struct's last
field, rename `pub fn open(` to `pub fn open_with_clock(` with the new last parameter
`clock: &'static dyn AmigaClock`, and put `clock` in the final `Ok(Self { … })`. Then add:

```rust
    /// [`open_with_clock`](Self::open_with_clock) with UTC — ART's dates
    /// before ART-317. Task 9 makes this test-only.
    pub fn open(
        device: &'a mut dyn BlockDeviceMut,
        geometry: VolumeGeometry,
        image: &Path,
        volume_offset: u64,
    ) -> CoreResult<Self> {
        Self::open_with_clock(device, geometry, image, volume_offset, &crate::core::clock::UtcClock)
    }

    /// The clock this writer stamps and converts dates with (ART-317).
    pub fn clock(&self) -> &'static dyn AmigaClock {
        self.clock
    }
```

Run: `...; cargo test --lib a_new_drawer_and_a_new_file_carry_the_writers_local_time`
Expected: FAIL at `"drawer"`, where `left` is today's real UTC date rather than `days: 17546, mins: 900`.

- [ ] **Step 3: Route stamping through the clock.**
  - `dir.rs`: change the signature to
    `pub fn write_dir_header(set: &mut BlockSet, block: u32, parent: u32, name: &str, now: AmigaDate) -> CoreResult<()>`.
    Replace `set_date(dir, amiga_now())?;` with `set_date(dir, now)?;`. Remove `amiga_now` from the `:28`
    import and import `crate::core::adf::bcpl::AmigaDate` if it is not already imported. At each test call
    listed above, add a last argument `AmigaDate::default()`. At `:486`, `set_date(dir, amiga_now())` becomes
    `set_date(dir, AmigaDate::default())`.
  - `file.rs`: change the parameter `date: Option<crate::core::adf::bcpl::AmigaDate>` to
    `date: crate::core::adf::bcpl::AmigaDate`, `set_date(bytes, date.unwrap_or_else(amiga_now))?;` to
    `set_date(bytes, date)?;`, and remove `amiga_now` from the `:25` import. At each test call, `None` becomes
    `crate::core::adf::bcpl::AmigaDate::default()` and `Some(d)` becomes `d`.
  - `mod.rs:474-483`: `meta.date,` becomes `meta.date.unwrap_or_else(|| self.clock.amiga_now()),`.
  - `mod.rs:530`: `dir::write_dir_header(&mut set, block, parent, &checked)?;` becomes
    `dir::write_dir_header(&mut set, block, parent, &checked, self.clock.amiga_now())?;`.
  - `layout.rs`: delete `amiga_now` (`:99-110`).
  - `commands/volume_write.rs`: each of `:171`, `:288`, `:1470`, `:2067` and `:2135` changes
    `VolumeWriter::open(` to `VolumeWriter::open_with_clock(` and adds a last argument
    `&crate::tools::local_time::LOCAL_TIME`.

- [ ] **Step 4: Run to see it pass, then the module**

Run: `...; cargo test --lib a_new_drawer_and_a_new_file_carry_the_writers_local_time` → `ok. 1 passed`.
Run: `...; cargo test --lib core::volume:: *> E:\amiga\ProjeART\build\tmp\art317-t3.txt; Select-String -Path E:\amiga\ProjeART\build\tmp\art317-t3.txt -Pattern "test result:"` → `0 failed`.

- [ ] **Step 5: Mutate.** Back up `mod.rs`. At `:530`, use `crate::core::clock::UtcClock.amiga_now()` instead
→ FAILS at `"drawer"`. Restore. Then at `:474-483`, use `UtcClock` instead → FAILS at `"file"`. Restore.

- [ ] **Step 6: Commit** — `volume/write: FFS/OFS entries stamped with the writer's clock (ART-317)`.
`git add` the six files above.

---

### Task 4: A new ADF is stamped with the clock

**Files:**
- Modify: `src-tauri/src/core/adf/create.rs:24-28, :85-97, :142-159, :191-205`; tests `:259, :274`
- Modify: `src-tauri/src/commands/adf.rs:176`

**Interfaces:**
- Produces:
  - `create::create_blank_adf_at(volume_name: &str, fs_type: FileSystemType, bootable: bool, now: AmigaDate) -> CoreResult<Vec<u8>>`
  - `#[cfg(test)] create::create_blank_adf(volume_name, fs_type, bootable)`
  - `save_new_adf(path: &Path, volume_name: &str, fs_type: FileSystemType, bootable: bool, clock: &dyn AmigaClock) -> CoreResult<AdfInfo>`

- [ ] **Step 1: Failing test** in `create.rs`'s `mod tests`:

```rust
    static PLUS_THREE: crate::core::clock::FixedClock =
        crate::core::clock::FixedClock { now: 1_768_478_400, offset: 10_800 };

    /// ART-317: both root-block dates of a new disk are the clock's local time.
    #[test]
    fn a_new_disk_is_stamped_with_the_clocks_local_time() {
        let (_guard, dir) = crate::core::ScratchDir::pair("art-adf-new", "local-time");
        let target = dir.join("Local.adf");
        save_new_adf(&target, "Local", FileSystemType::Ffs, false, &PLUS_THREE).unwrap();

        let image = std::fs::read(&target).unwrap();
        let root = 880 * 512;
        let word = |o: usize| u32::from_be_bytes(image[root + o..root + o + 4].try_into().unwrap());
        assert_eq!((word(420), word(424), word(428)), (17_546, 900, 0), "last-changed date");
        assert_eq!((word(472), word(476), word(480)), (17_546, 900, 0), "creation date");
    }
```

- [ ] **Step 2: Behavioural red.** Add `use crate::core::clock::AmigaClock;` and give `save_new_adf` a last
parameter `_clock: &dyn AmigaClock`, ignored. Update `create.rs:259` and `:274` to pass
`&crate::core::clock::UtcClock`, and `commands/adf.rs:176` to pass `&crate::tools::local_time::LOCAL_TIME`.
Run: `...; cargo test --lib a_new_disk_is_stamped_with_the_clocks_local_time` → FAIL at `"last-changed date"` (UTC now).

- [ ] **Step 3: Implement.** Rename `pub fn create_blank_adf(` to `pub fn create_blank_adf_at(` with a last
parameter `now: AmigaDate`. Delete `let now = get_current_amiga_date();` (`:86`). Delete
`get_current_amiga_date` (`:191-205`). Add:

```rust
/// [`create_blank_adf_at`] stamped with UTC now — test-only (ART-317). The
/// product goes through [`save_new_adf`], which takes the clock.
#[cfg(test)]
pub fn create_blank_adf(
    volume_name: &str,
    fs_type: FileSystemType,
    bootable: bool,
) -> CoreResult<Vec<u8>> {
    create_blank_adf_at(volume_name, fs_type, bootable, crate::core::clock::UtcClock.amiga_now())
}
```

In `save_new_adf`, rename `_clock` to `clock` and change the create call to
`create_blank_adf_at(volume_name, fs_type, bootable, clock.amiga_now())?`.

- [ ] **Step 4: Pass**

Run: `...; cargo test --lib core::adf:: *> E:\amiga\ProjeART\build\tmp\art317-t4.txt; Select-String -Path E:\amiga\ProjeART\build\tmp\art317-t4.txt -Pattern "test result:"` → `0 failed`.
Run: `...; cargo build` → finishes. This proves no product code calls the test-only `create_blank_adf`.

- [ ] **Step 5: Mutate.** In `save_new_adf`, pass `crate::core::clock::UtcClock.amiga_now()` → the new test FAILS. Restore.

- [ ] **Step 6: Commit** — `adf: a new disk is stamped with the clock (ART-317)`. `git add src-tauri/src/core/adf/create.rs src-tauri/src/commands/adf.rs`

---

### Task 5: A host mtime or a disc date becomes local time for its own date

**Files:**
- Modify: `src-tauri/src/core/volume/write/copy.rs:84-88` (trait), `:214`, `:349-352`, `:361-391`, `:679-682`; tests after `:1307`
- Modify: `src-tauri/src/core/iso/mod.rs:795-813`; test calls `:2101, :2104, :2563`

**Interfaces:**
- Consumes: `VolumeWriter::open_with_clock`, `VolumeWriter::clock` (Task 3); `SeasonalClock` (Task 1).
- Produces: `CopySource::metadata(&self, relative: &str, clock: &dyn AmigaClock) -> CoreResult<Option<Sidecar>>`. No constructor changes.

- [ ] **Step 1: The DST test** in `copy.rs`'s `mod tests`:

```rust
    /// ART-317, the season rule: a host file modified in January (UTC+2) and
    /// copied in July (UTC+3) carries 14:00 — its own day's offset. Today's
    /// offset would give 15:00, and UTC 12:00.
    #[test]
    fn a_winter_mtime_copied_in_summer_keeps_its_winter_local_time() {
        static SEASONS: crate::core::clock::SeasonalClock = crate::core::clock::SeasonalClock {
            now: 1_784_116_800,
            switch_at: 1_774_746_000,
            before: 7_200,
            after: 10_800,
        };
        let fixture = Fixture::new("dst-mtime");
        fixture.put("Winter.txt", b"written in January");
        let winter = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_768_478_400);
        std::fs::File::options()
            .write(true)
            .open(fixture.source.join("Winter.txt"))
            .unwrap()
            .set_modified(winter)
            .unwrap();

        let folder = HostFolder::new(&fixture.source, false);
        let mut device = fixture.device();
        let mut writer =
            VolumeWriter::open_with_clock(&mut device, fixture.geometry, &fixture.image, 0, &SEASONS)
                .unwrap();
        copy_into_volume(&mut writer, 0, &folder, OverwritePolicy::Skip, &NoProgress).unwrap();

        let block = writer.find(0, "Winter.txt").unwrap().unwrap().block;
        assert_eq!(
            writer.attributes(block).unwrap().date,
            crate::core::adf::bcpl::AmigaDate { days: 17_546, mins: 14 * 60, ticks: 0 },
            "UTC+2, the offset on the file's own date"
        );
    }
```

- [ ] **Step 2: Run to see it fail**

Run: `...; cargo test --lib a_winter_mtime_copied_in_summer_keeps_its_winter_local_time`
Expected: FAIL, `left: … mins: 720` (UTC).

- [ ] **Step 3: Implement**
  - `copy.rs`: add `use crate::core::clock::AmigaClock;`. Change the trait method to
    `fn metadata(&self, relative: &str, clock: &dyn AmigaClock) -> CoreResult<Option<Sidecar>>;`. Add a doc
    line: "A host mtime or a disc's recording date is an instant; `clock` turns it into the wall time AmigaDOS
    stores, with the offset in force on that date (ART-317). A `.uaem` date is already wall time and is used
    as written."
  - `:214`: `let sidecar = source.metadata(&entry.relative, writer.clock())?;`
  - `HostFolder::metadata` and `HostSelection::metadata` take `clock: &dyn AmigaClock` and call
    `host_metadata(&path, self.read_sidecars, clock)`.
  - `host_metadata(path: &Path, read_sidecars: bool, clock: &dyn AmigaClock)`: `date: amiga_from_unix(unix),`
    becomes `date: clock.amiga_from_unix(unix),`. Remove the `amiga_from_unix` import from copy.rs if nothing
    else uses it.
  - `iso/mod.rs:795`: `fn metadata(&self, relative: &str, clock: &dyn AmigaClock)`. At `:810`:
    `date: found.entry.date.map(|unix| clock.amiga_from_unix(unix)).unwrap_or_default(),`. Add
    `use crate::core::clock::AmigaClock;`.
  - `iso/mod.rs` tests `:2101`, `:2104` and `:2563`: add `&crate::core::clock::UtcClock` as the second argument.

- [ ] **Step 4: The disc half.** At the end of the `iso/mod.rs` test that contains `:2101`
(`source.metadata("Startup-Sequence", …)`), append:

```rust
        // ART-317: a disc's recording date is an instant, read as local wall time.
        static PLUS_TWO: crate::core::clock::FixedClock =
            crate::core::clock::FixedClock { now: 0, offset: 7_200 };
        use crate::core::clock::{AmigaClock, UtcClock};
        let utc = source.metadata("Startup-Sequence", &UtcClock).unwrap().unwrap();
        let local = source.metadata("Startup-Sequence", &PLUS_TWO).unwrap().unwrap();
        assert_ne!(utc.date, crate::core::adf::bcpl::AmigaDate::default(), "the fixture carries a date");
        assert_eq!(local.date, PLUS_TWO.amiga_from_unix(UtcClock.unix_from_amiga(utc.date)));
        assert_ne!(local.date, utc.date);
```

- [ ] **Step 5: Pass**

Run: `...; cargo test --lib a_winter_mtime_copied_in_summer_keeps_its_winter_local_time` → `ok`.
Run: `...; cargo test --lib core::iso:: *> E:\amiga\ProjeART\build\tmp\art317-t5a.txt; Select-String -Path E:\amiga\ProjeART\build\tmp\art317-t5a.txt -Pattern "test result:"` → `0 failed`.
Run: `...; cargo test --lib core::volume::write::copy:: *> E:\amiga\ProjeART\build\tmp\art317-t5b.txt; Select-String -Path E:\amiga\ProjeART\build\tmp\art317-t5b.txt -Pattern "test result:"` → `0 failed`.

- [ ] **Step 6: Mutate with the exact Windows defect** (today's offset instead of the date's). Back up
`copy.rs`. In `host_metadata`, write
`date: crate::core::clock::amiga_from_wall(unix + i64::from(clock.offset_at(clock.now_unix()))),`
→ FAILS with `mins: 900`. Restore.

- [ ] **Step 7: The `.uaem` guard — a date that is already wall time is never shifted.** This test passes
before and after this task. Its red is the mutation below, and it exists so a later "convert every date" change
fails. Add it to `copy.rs`'s `mod tests`:

```rust
    /// ART-317, the other half. A `.uaem` date is zone-less wall time
    /// (`uaem.rs:206` `amiga_from_civil`, no UTC step), so it goes in as read.
    /// Extracted back out, it renders as the same text. On a UTC+3 clock, an
    /// offset applied on either leg moves 12:00 to 15:00 or 09:00.
    #[test]
    fn a_uaem_date_round_trips_without_an_offset() {
        static PLUS_THREE: crate::core::clock::FixedClock =
            crate::core::clock::FixedClock { now: 1_784_116_800, offset: 10_800 };
        let written = crate::core::adf::bcpl::AmigaDate { days: 17_546, mins: 12 * 60, ticks: 0 };

        let fixture = Fixture::new("uaem-no-offset");
        fixture.put("Game.slave", b"slave bytes");
        fixture.put("Game.slave.uaem", b"-sp-rwed 2026-01-15 12:00:00.00 keep me\n");
        {
            let folder = HostFolder::new(&fixture.source, true);
            let mut device = fixture.device();
            let mut writer = VolumeWriter::open_with_clock(
                &mut device, fixture.geometry, &fixture.image, 0, &PLUS_THREE,
            )
            .unwrap();
            copy_into_volume(&mut writer, 0, &folder, OverwritePolicy::Skip, &NoProgress).unwrap();
            let block = writer.find(0, "Game.slave").unwrap().unwrap().block;
            assert_eq!(writer.attributes(block).unwrap().date, written, "in: the sidecar's text, unshifted");
        }

        let out = fixture.dir.join("out");
        {
            let device = fixture.device();
            extract_from_volume(&device, &fixture.geometry, 0, &out, true, OverwritePolicy::Overwrite, &NoProgress)
                .unwrap();
        }
        let text = std::fs::read_to_string(out.join("Game.slave.uaem")).unwrap();
        assert!(text.starts_with("-sp-rwed 2026-01-15 12:00:00.00 "), "out: {text:?}");

        let second = Fixture::new("uaem-no-offset-2");
        let folder = HostFolder::new(&out, true);
        let mut device = second.device();
        let mut writer =
            VolumeWriter::open_with_clock(&mut device, second.geometry, &second.image, 0, &PLUS_THREE).unwrap();
        copy_into_volume(&mut writer, 0, &folder, OverwritePolicy::Skip, &NoProgress).unwrap();
        let block = writer.find(0, "Game.slave").unwrap().unwrap().block;
        assert_eq!(writer.attributes(block).unwrap().date, written, "back in: still unshifted");
    }
```

Run: `...; cargo test --lib a_uaem_date_round_trips_without_an_offset` → `ok. 1 passed`.
Mutate: back up `copy.rs`. In `host_metadata`'s sidecar branch (`:372-373`), replace `return Ok(Some(parsed));` with
`return Ok(Some(Sidecar { date: clock.amiga_from_unix(crate::core::clock::UtcClock.unix_from_amiga(parsed.date)), ..parsed }));`
→ FAILS at `"in:"` with `mins: 900`. Restore and re-run: passes.

- [ ] **Step 8: Commit** — `copy: host mtimes and disc dates become local time for their own date; .uaem dates stay as written (ART-317)`.
`git add src-tauri/src/core/volume/write/copy.rs src-tauri/src/core/iso/mod.rs`

---

### Task 6: libpfs3 `0.1.3+art.4`, and `NativeFormatter` takes the clock

**Files:**
- Modify: `src-tauri/vendor/libpfs3/src/format.rs` (header, `FormatOptions` `:24-37`, `:114`)
- Modify: `src-tauri/vendor/libpfs3/src/writer.rs` (header, struct `:18-36`, `open` `:40-68`, `:571`, `:1178`)
- Modify: `src-tauri/vendor/libpfs3/Cargo.toml`, `src-tauri/vendor/libpfs3/ART-PATCH.md`, `src-tauri/Cargo.lock`
- Modify: `src-tauri/Cargo.toml` (the libpfs3 comment naming the vendored version, `:164`)
- Modify: `src-tauri/src/core/preload/native.rs:128, :132-267, :513-517, :573, :639-655, :827-836, :895, :978-997`; tests
- Modify: `src-tauri/src/commands/preload.rs:611`; tests in `native.rs`, `core/firstboot/cardread.rs`, `core/osinstall/verify.rs`, `commands/preload.rs`
- Modify: `THIRD_PARTY_LICENSES.md:65`

**Interfaces:**
- Consumes: `AmigaClock`, `FixedClock`, `UtcClock`, `LOCAL_TIME`, `VolumeWriter::open_with_clock`.
- Produces:
  - libpfs3 `FormatOptions { volume_name, enable_deldir, datestamp: Option<(u16, u16, u16)> }` — plus whatever plan 3 added; keep it
  - `Writer::set_entry_date(&mut self, date: Option<(u16, u16, u16)>)`
  - `NativeFormatter::new(clock: &'static dyn AmigaClock) -> NativeFormatter` (a `const fn`)
  - `#[cfg(test)] NativeFormatter::UTC`

- [ ] **Step 0: Precondition.** `rg -n "^version" D:\Projeler\Amiga\amiga-retro-toolkit\src-tauri\vendor\libpfs3\Cargo.toml`
must print `0.1.3+art.3`. If it prints `art.2`, **stop**: plan 3 has not landed. Re-read `format.rs` and
`writer.rs` whole before editing, because plan 3 changed both.

- [ ] **Step 1: Failing tests** in `native.rs`'s `mod tests`:

```rust
    static PLUS_THREE: crate::core::clock::FixedClock =
        crate::core::clock::FixedClock { now: 1_768_478_400, offset: 10_800 };

    /// ART-317 on PFS3: the root date the format writes and the entry date
    /// `copy_in` writes are the clock's local time (15:00 at 12:00 UTC, UTC+3).
    #[test]
    fn a_pfs3_format_and_copy_in_carry_the_clocks_local_time() {
        let (_guard, image) = rdb_image_with_one_pds3_partition();
        let formatter = NativeFormatter::new(&PLUS_THREE);
        formatter.format_partition(&image, None, 1, "Work", &NoProgress).unwrap();
        let (_guard, tree) = fixtures::scratch("pfs3-local-time");
        std::fs::write(tree.join("Readme"), b"hello\n").unwrap();
        formatter.copy_in(&image, None, "DH0", &tree, &NoProgress).unwrap();

        let mut vol = libpfs3::volume::Volume::open(&image, partition_offset(&image)).unwrap();
        let local = (17_546u16, 900u16, 0u16);
        let rext = vol.rootblock_ext.as_ref().expect("a rootblock extension");
        assert_eq!(rext.root_date, local, "root date");
        let entry = vol.list_dir("").unwrap().into_iter().find(|e| e.name == "Readme").unwrap();
        assert_eq!((entry.creation_day, entry.creation_minute, entry.creation_tick), local, "entry date");
    }

    /// ART-317 on FFS: the formatted root and a copied file.
    #[test]
    fn an_ffs_format_and_copy_in_carry_the_clocks_local_time() {
        let (_guard, image) = rdb_image_with_one_dos3_partition();
        let formatter = NativeFormatter::new(&PLUS_THREE);
        formatter.format_partition(&image, None, 1, "Work", &NoProgress).unwrap();
        let (_guard, tree) = fixtures::scratch("ffs-local-time");
        std::fs::write(tree.join("Readme"), b"hello\n").unwrap();
        // A sidecar date is wall time already: it must land unshifted (12:00), not 15:00.
        std::fs::write(tree.join("Old"), b"old\n").unwrap();
        std::fs::write(tree.join("Old.uaem"), b"----rwed 2026-01-15 12:00:00.00 \n").unwrap();
        formatter.copy_in(&image, None, "DH0", &tree, &NoProgress).unwrap();

        let card = read_card(&image).unwrap();
        let area = &card.areas[0];
        let part = &area.rdb.partitions[0];
        let (offset, length, block_size) = partition_region(area, part).unwrap();
        let mut region = FileRegionMut::open(&image, offset, length, block_size).unwrap();
        let dos = DosType::new(part.dostype.to_be_bytes());
        let geometry = VolumeGeometry::new(block_size, region.total_blocks(), part.reserved, dos).unwrap();

        let mut root = vec![0u8; block_size];
        region.read_block(geometry.root_block, &mut root).unwrap();
        let word = |o: usize| u32::from_be_bytes(root[o..o + 4].try_into().unwrap());
        assert_eq!((word(420), word(424), word(428)), (17_546, 900, 0), "root date");

        let writer = VolumeWriter::open_with_clock(&mut region, geometry, &image, offset, &PLUS_THREE).unwrap();
        let block = writer.find(0, "Readme").unwrap().unwrap().block;
        assert_eq!(
            writer.attributes(block).unwrap().date,
            AmigaDate { days: 17_546, mins: 900, ticks: 0 },
            "file date"
        );
        let old = writer.find(0, "Old").unwrap().unwrap().block;
        assert_eq!(
            writer.attributes(old).unwrap().date,
            AmigaDate { days: 17_546, mins: 720, ticks: 0 },
            ".uaem date, unshifted"
        );
    }
```

- [ ] **Step 2: The clock field, stamping still UTC, test churn by script.** In `native.rs`:

```rust
/// A [`VolumeFormatter`] backed by `libpfs3` and ART's own FFS writer.
/// Launches nothing; see the module docs for what each method actually does.
/// Every date it writes comes from `clock` (ART-317).
pub struct NativeFormatter {
    clock: &'static dyn AmigaClock,
}

impl NativeFormatter {
    pub const fn new(clock: &'static dyn AmigaClock) -> Self {
        Self { clock }
    }

    /// UTC, for tests whose subject is not the date.
    #[cfg(test)]
    pub const UTC: Self = Self::new(&crate::core::clock::UtcClock);
}
```

Add `use crate::core::clock::AmigaClock;`. Change `commands/preload.rs:611` to
`let native = NativeFormatter::new(&crate::tools::local_time::LOCAL_TIME);`.

Write this script with the Write tool to `E:\amiga\ProjeART\build\tmp\art317_native_utc.py`:

```python
import pathlib, re
root = pathlib.Path(r"D:\Projeler\Amiga\amiga-retro-toolkit\src-tauri\src")
files = ["core/preload/native.rs", "core/firstboot/cardread.rs", "core/osinstall/verify.rs", "commands/preload.rs"]
bare = re.compile(r"\bNativeFormatter\b(?![:`\]])(?=\s*(?:$|[.;,)]))")
total = 0
for rel in files:
    path = root / rel
    with open(path, "r", encoding="utf-8", newline="") as f:
        lines = f.read().split("\n")
    start = next(i for i, l in enumerate(lines) if l.strip() == "#[cfg(test)]")
    changed = 0
    for i in range(start, len(lines)):
        s = lines[i].lstrip()
        if s.startswith("//") or s.startswith("use "):
            continue
        lines[i], n = bare.subn("NativeFormatter::UTC", lines[i])
        changed += n
    with open(path, "w", encoding="utf-8", newline="") as f:
        f.write("\n".join(lines))
    print(rel, changed)
    total += changed
print("total", total)
```

Back up the four files. Run `python E:\amiga\ProjeART\build\tmp\art317_native_utc.py` and record the
per-file counts in the commit message. Then
`rg -n "NativeFormatter\s*$|NativeFormatter[.;,)]" D:\Projeler\Amiga\amiga-retro-toolkit\src-tauri\src`
must print only lines that are comments, `use`, or the product line `:611`.

Run: `...; cargo test --lib an_ffs_format_and_copy_in_carry_the_clocks_local_time` → FAIL at `"root date"` (UTC).

- [ ] **Step 3: libpfs3.** In `format.rs`, add to `FormatOptions`, and `datestamp: None,` in `Default`:

```rust
    /// Root and volume datestamp as (days, minutes, ticks) since 1978-01-01.
    /// `None` stamps the current time as 0.1.3 did, which is UTC. Added by ART
    /// for ART-317: AmigaDOS reads it as local time.
    pub datestamp: Option<(u16, u16, u16)>,
```

At `:114`: `let (cday, cmin, ctick) = opts.datestamp.unwrap_or_else(current_amiga_datestamp);`

In `writer.rs`, add the struct field `entry_date: Option<(u16, u16, u16)>,` (after `pending_writes`),
`entry_date: None,` in `open`, and, after `into_volume`:

```rust
    /// The date new and rewritten directory entries carry, as (days, minutes,
    /// ticks). `None` is the current time, as 0.1.3 stamped it (UTC). Added by
    /// ART for ART-317, whose caller sets local time before each entry.
    pub fn set_entry_date(&mut self, date: Option<(u16, u16, u16)>) {
        self.entry_date = date;
    }

    fn entry_datestamp(&self) -> (u16, u16, u16) {
        self.entry_date.unwrap_or_else(crate::util::current_amiga_datestamp)
    }
```

At `:571` and `:1178`: `let (cday, cmin, ctick) = self.entry_datestamp();`. Extend each file's
`//! Modified by ART` header with one line: `//! Modified by ART on 2026-09-14 (ART-317): an optional caller-supplied datestamp.`

In `vendor/libpfs3/Cargo.toml`, set `version = "0.1.3+art.4"` and change `(+art.N)` in the header comment to
`(+art.4)`. Then:
Run: `...; cargo update -p libpfs3 --precise 0.1.3+art.4`
Run: `...; cargo test --lib the_pinned_version_constant_matches_cargo_toml` → FAIL (constant still `art.3`).
Set `LIBPFS3_VERSION` (`native.rs:128`) to `"0.1.3+art.4"` and the `probe_names_libpfs3` expected string to
`"libpfs3 0.1.3+art.4 (native, no external tool)"`. Change the vendored-version mention in `src-tauri/Cargo.toml`
(the libpfs3 comment) and in `THIRD_PARTY_LICENSES.md:65` to `0.1.3+art.4`, adding `ART-317` to that line's
list. `rg -n "art\.[0-3]\b" D:\Projeler\Amiga\amiga-retro-toolkit --glob "!**/superpowers/**" --glob "!**/Cargo.lock"`
must then print only history (CHANGELOG, ISSUES, session-log).

- [ ] **Step 4: `NativeFormatter` stamps with its clock.** In `native.rs`:

```rust
/// An [`AmigaDate`] as libpfs3's (days, minutes, ticks). PFS3 stores days as
/// a `u16`, which lasts until 2157, so a later day clamps rather than wraps.
fn pfs3_datestamp(date: AmigaDate) -> (u16, u16, u16) {
    (u16::try_from(date.days).unwrap_or(u16::MAX), date.mins as u16, date.ticks as u16)
}
```

  - `format_partition`, the PFS3 arm: add `datestamp: Some(pfs3_datestamp(self.clock.amiga_now())),` to the
    `FormatOptions` literal. The FFS arm:
    `format_ffs_volume(&mut region, &geometry, &checked_name, self.clock.amiga_now())?;`.
  - `format_ffs_volume(device, geometry, volume_name, now: AmigaDate)`: delete `let now = current_amiga_date();` (`:573`), and delete `current_amiga_date` (`:639-655`).
  - `copy_in_pfs3` and `copy_in_ffs` get a last parameter `clock: &'static dyn AmigaClock`; `copy_in` passes `self.clock`.
  - `copy_in_pfs3`: first line of the loop body, before the cancellation check:
    `writer.set_entry_date(Some(pfs3_datestamp(clock.amiga_now())));`
  - `copy_in_ffs:997`: `let mut writer = VolumeWriter::open_with_clock(&mut region, geometry, image, offset, clock)?;`

- [ ] **Step 5: Pass**

Run: `...; cargo test --lib core::preload:: *> E:\amiga\ProjeART\build\tmp\art317-t6a.txt; Select-String -Path E:\amiga\ProjeART\build\tmp\art317-t6a.txt -Pattern "test result:"` → `0 failed`, including both new tests, `the_pinned_version_constant_matches_cargo_toml` and `probe_names_libpfs3`.
Run: `...; cargo test --lib commands::preload:: *> E:\amiga\ProjeART\build\tmp\art317-t6b.txt; Select-String -Path E:\amiga\ProjeART\build\tmp\art317-t6b.txt -Pattern "test result:"` → `0 failed`.
Run: `...; cargo test --lib core::firstboot:: core::osinstall::verify *> E:\amiga\ProjeART\build\tmp\art317-t6c.txt; Select-String -Path E:\amiga\ProjeART\build\tmp\art317-t6c.txt -Pattern "test result:"` → `0 failed`.

- [ ] **Step 6: Mutate, three guards.** Back up `native.rs`.
  1. Delete the `set_entry_date` line → FAILS at `"entry date"`. Restore.
  2. Use `datestamp: None` → FAILS at `"root date"` (PFS3). Restore.
  3. In `copy_in_ffs`, pass `&crate::core::clock::UtcClock` → FAILS at `"file date"`. Restore.

- [ ] **Step 7: `ART-PATCH.md`.**
  - Title → `` # `libpfs3` 0.1.3+art.4 — ART's vendored copy ``.
  - Append to the "Modified" row: `; 2026-09-14, src/format.rs and src/writer.rs for ART-317`. Change the
    "Carried" row's version to `0.1.3+art.4`.
  - Add a section after the last numbered change:

```markdown
**`src/format.rs`, `src/writer.rs`** ([ART-317](../../../docs/ISSUES.md); design
`docs/superpowers/specs/2026-09-14-art-317-local-time-design.md`):

N. **The caller may supply the datestamp.** `FormatOptions::datestamp` sets the root and volume date, and
   `Writer::set_entry_date` the date of each new or rewritten directory entry. `None` keeps 0.1.3's
   `current_amiga_datestamp()`, which is UTC. AmigaDOS `DateStamp()` is local time with no zone, and pfs3aio
   stamps with it; ART passes the local wall time. Std-only: the crate still asks no clock but `SystemTime`.
```

  (Number `N` after plan 3's last item.) Regenerate "Diff against 0.1.3" against the pristine release:

Run: `New-Item -ItemType Directory -Force E:\amiga\ProjeART\build\tmp\libpfs3-orig`
Run: `curl.exe -L -o E:\amiga\ProjeART\build\tmp\libpfs3-orig\libpfs3-0.1.3.crate https://static.crates.io/crates/libpfs3/libpfs3-0.1.3.crate`
Run: `(Get-FileHash -Algorithm SHA256 E:\amiga\ProjeART\build\tmp\libpfs3-orig\libpfs3-0.1.3.crate).Hash` → `02F457EF99A09DDEBF56E454C6A25DC3A6860A602C878489F132A4CA3EED4317`
Run: `tar -xzf E:\amiga\ProjeART\build\tmp\libpfs3-orig\libpfs3-0.1.3.crate -C E:\amiga\ProjeART\build\tmp\libpfs3-orig`
Run: `cd D:\Projeler\Amiga\amiga-retro-toolkit; git diff --no-index E:/amiga/ProjeART/build/tmp/libpfs3-orig/libpfs3-0.1.3/src src-tauri/vendor/libpfs3/src > E:\amiga\ProjeART\build\tmp\libpfs3.diff`
Replace the section's fenced diff with that file's content, with the `a/`/`b/` path prefixes shortened to `src/…` as the existing section spells them.

- [ ] **Step 8: Commit** — `libpfs3 0.1.3+art.4: caller-supplied datestamps; NativeFormatter stamps local time (ART-317)`. The body carries the script's per-file counts.
`git add` the vendor files, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `native.rs`, `commands/preload.rs`, `core/firstboot/cardread.rs`, `core/osinstall/verify.rs`, `THIRD_PARTY_LICENSES.md`.

---

### Task 7: Reading a date back, and the disc's own sidecars

**Files:**
- Modify: `src-tauri/src/core/adf/bcpl.rs:76-81` (delete `to_unix`), tests `:133-153`
- Modify: `src-tauri/src/core/adf/fs.rs:57, :87, :114-123, :178-195`, tests
- Modify: `src-tauri/src/core/adf/mod.rs:147-154`; `src-tauri/src/core/dirsize.rs:173`; `src-tauri/src/core/gameindex/readers/whdhdf.rs:195, :283`
- Modify: `src-tauri/src/commands/volume.rs:65`, `src-tauri/src/commands/adf.rs:137-138`
- Modify: `src-tauri/src/core/iso/mod.rs:419-460, :545`; `src-tauri/src/commands/iso.rs:175, :262`
- Modify: `src-tauri/src/core/volume/write/layout.rs:112-121` (delete `amiga_from_unix`), tests `:423-441`

**Interfaces:**
- Produces:
  - `fs::list_directory_on(device: &dyn BlockDevice, dir_block: u32, clock: &dyn AmigaClock)`
  - `fs::list_directory(image, dir_block, clock)`
  - `fs::list_root(image, root_block, clock)`
  - `AdfImage::list_root(&self, clock)` and `AdfImage::list_dir(&self, dir_block, clock)`
  - `IsoImage::extract_tree(&self, extent, length, dest, policy, clock: &dyn AmigaClock, sink)`

- [ ] **Step 1: Failing test** in `fs.rs`'s `mod tests`:

```rust
    /// ART-317, reading back: a date written on a UTC+3 clock lists as the
    /// instant it was stamped at. A UTC reader is three hours late — the
    /// defect's own shape, kept here as the control.
    #[test]
    fn a_listed_date_is_the_instant_the_writer_stamped() {
        use crate::core::clock::{FixedClock, UtcClock};
        use crate::core::volume::device::FileRegionMut;
        use crate::core::volume::write::{FileMeta, VolumeWriter};
        use crate::core::volume::DosType;
        static PLUS_THREE: FixedClock = FixedClock { now: 1_768_478_400, offset: 10_800 };

        let (bytes, geometry) = crate::core::volume::fixture::ffs_volume(1760, DosType::new(*b"DOS\x01"));
        let (_guard, dir) = crate::core::ScratchDir::pair("art-adf-fs", "local-time");
        let path = dir.join("disk.adf");
        std::fs::write(&path, &bytes).unwrap();
        {
            let mut device = FileRegionMut::open(&path, 0, geometry.total_bytes(), 512).unwrap();
            let mut writer =
                VolumeWriter::open_with_clock(&mut device, geometry, &path, 0, &PLUS_THREE).unwrap();
            writer.add_file(0, "Readme", b"hi", FileMeta::default()).unwrap();
        }
        let device = FileRegionMut::open(&path, 0, geometry.total_bytes(), 512).unwrap();
        let local = list_directory_on(&device, geometry.root_block, &PLUS_THREE).unwrap();
        assert_eq!(local[0].unix_date, 1_768_478_400, "same clock: the instant itself");
        let utc = list_directory_on(&device, geometry.root_block, &UtcClock).unwrap();
        assert_eq!(utc[0].unix_date, 1_768_478_400 + 10_800, "a UTC reader is three hours late");
    }
```

- [ ] **Step 2: Behavioural red.** Add the parameter `_clock: &dyn AmigaClock` (ignored) to
`list_directory_on`, `list_directory` and `list_root` in `fs.rs`, and to `AdfImage::list_root`/`list_dir` in
`adf/mod.rs`. Thread the clock through each:
  - `fs.rs:123` (`walk_and_count_on`) and `dirsize.rs:173` pass `&crate::core::clock::UtcClock`, with the
    comment `// counts only; no date is read`.
  - `whdhdf.rs:195, :283` pass `&crate::core::clock::UtcClock`, with `// names only; no date is read`.
  - `commands/volume.rs:65` and `commands/adf.rs:137-138` pass `&crate::tools::local_time::LOCAL_TIME`.
  - Test callers (`cargo test --lib --no-run` names each) pass `&crate::core::clock::UtcClock`.
Run: `...; cargo test --lib a_listed_date_is_the_instant_the_writer_stamped` → FAIL at `"same clock"` (left `1768489200`).

- [ ] **Step 3: Implement.** In `fs.rs`, rename `_clock` to `clock` and change `:87` to
`unix_date: clock.unix_from_amiga(hdr.date),`. Update the `unix_date` doc (`:23`) to: "Unix timestamp (seconds
since 1970 UTC), from the Amiga date read as local time through the caller's clock (ART-317)." Delete
`AmigaDate::to_unix` in `bcpl.rs`, and in its test `amiga_date_to_unix` rename every `to_unix()` to
`to_wall_seconds()`.

- [ ] **Step 4: The disc's sidecars.** `IsoImage::extract_tree` and `extract_dir` take
`clock: &dyn AmigaClock` before `sink`. `extract_dir`'s recursive call passes it on. At `:545`:
`entry.date.map(|unix| clock.amiga_from_unix(unix)).unwrap_or_default(),`.
`commands/iso.rs:262` passes `&crate::tools::local_time::LOCAL_TIME`. At `commands/iso.rs:175`, replace
`.map(crate::core::volume::write::layout::amiga_from_unix)` with
`.map(|unix| crate::tools::local_time::LOCAL_TIME.amiga_from_unix(unix))` and import
`crate::core::clock::AmigaClock`. Test callers of `extract_tree` (`iso/mod.rs:2076, 2121, 2372, 2399, 2431, 2445, 2503`)
pass `&crate::core::clock::UtcClock`.

In the test that holds `:2121`, after its existing extraction, extract a second time into `out.join("plus-two")`
through the same receiver, with `&PLUS_TWO` (`static PLUS_TWO: crate::core::clock::FixedClock = crate::core::clock::FixedClock { now: 0, offset: 7_200 };`).
Then parse both `Startup-Sequence.uaem` sidecars with `crate::core::volume::write::uaem::parse` and assert:

```rust
        use crate::core::clock::{AmigaClock, UtcClock};
        assert_ne!(utc.date, local.date);
        assert_eq!(PLUS_TWO.unix_from_amiga(local.date), UtcClock.unix_from_amiga(utc.date), "same instant");
```

- [ ] **Step 5: Remove the UTC converter.** Delete `layout::amiga_from_unix` (`:112-121`). In layout.rs's tests,
`amiga_from_unix(x)` becomes `crate::core::clock::amiga_from_wall(x)` and `date.to_unix()` becomes
`date.to_wall_seconds()`. `rg -n "amiga_from_unix\b" D:\Projeler\Amiga\amiga-retro-toolkit\src-tauri\src` must
then print only `.amiga_from_unix(` method calls, plus `source_cd.rs:61`/`:156` (Task 8).
Task 8 removes those last two; until then keep a `use crate::core::clock::amiga_from_wall as amiga_from_unix;`
line in `source_cd.rs:61`, so this commit compiles unchanged in behaviour.

- [ ] **Step 6: Pass**

Run: `...; cargo test --lib *> E:\amiga\ProjeART\build\tmp\art317-t7.txt; Select-String -Path E:\amiga\ProjeART\build\tmp\art317-t7.txt -Pattern "test result:"` → `0 failed`.

- [ ] **Step 7: Mutate.** Back up `fs.rs`. At `:87`, `unix_date: UtcClock.unix_from_amiga(hdr.date)` → FAILS at `"same clock"`. Restore.
Back up `iso/mod.rs`. At `:545`, `UtcClock.amiga_from_unix(unix)` → the extended test FAILS. Restore.

- [ ] **Step 8: Commit** — `adf/iso: dates read back and disc sidecars through the clock (ART-317)`. `git add` the files listed.

---

### Task 8: Install discs read their recording dates as local time

**Files:**
- Modify: `src-tauri/src/core/osinstall/source_cd.rs:47-53` (doc), `:61`, `:68-74`, `:100-131`, `:150-159`, `:229, :239, :297`; tests
- Modify: `src-tauri/src/core/osinstall/scan.rs:134-139, :155-166`
- Modify: `src-tauri/src/core/osinstall/plan.rs:1504-1586, :1705`
- Modify: `src-tauri/src/core/osinstall/apply.rs:1226-1240, :1328`
- Modify: `src-tauri/src/core/osinstall/scan_cache.rs:119-127`; `src-tauri/src/core/osinstall/source.rs:47`
- Modify: `src-tauri/src/commands/osinstall.rs:394, :1992, :2791`
- Test callers: `scan.rs:1479`, `scan_cache.rs:660, 999, 1003, 1041, 1114, 1121, 1125, 1144, 1150, 1157, 1169, 1171`, `apply.rs:4964, 5021, 5701, 5796, 7924`, `source_contract.rs:151`, `source_cd.rs` tests

**Interfaces:**
- Produces:
  - `CdSource::open(path: &Path, clock: &'static dyn AmigaClock)`
  - `scan::open_media(found: &FoundMedia, clock: &'static dyn AmigaClock)`
  - `scan::open_media_cached(found, cache, clock: &'static dyn AmigaClock)`
  - `plan_with_cache_in(request, recipe, cache, scratch_root, clock: &'static dyn AmigaClock)`
  - `apply_staging_in(plan, root, scratch_root, clock: &'static dyn AmigaClock, sink)`
  - The `#[cfg(test)]` wrappers `plan`, `plan_with_cache`, `plan_over` and `apply` pass `&UtcClock`

- [ ] **Step 1: Failing test** in `source_cd.rs`'s `mod tests`. Build `path` with the same fixture lines as the
test around `:439` (the first one that opens a disc successfully):

```rust
    /// ART-317: a disc's recording date is an instant; an install tree gets it
    /// as local wall time.
    #[test]
    fn a_disc_recording_date_is_read_as_local_wall_time() {
        use crate::core::clock::{AmigaClock, FixedClock, UtcClock};
        static PLUS_TWO: FixedClock = FixedClock { now: 0, offset: 7_200 };
        // `path`: the same fixture lines the test at :439 uses.
        let mut utc = CdSource::open(&path, &UtcClock).unwrap();
        let mut local = CdSource::open(&path, &PLUS_TWO).unwrap();
        let file = utc.walk("").unwrap().into_iter().find(|e| !e.is_dir).expect("a file on the disc");
        assert_ne!(file.date, AmigaDate::default(), "the fixture carries a recording date");
        let same = local.entry(&file.path).unwrap().unwrap();
        assert_eq!(PLUS_TWO.unix_from_amiga(same.date), UtcClock.unix_from_amiga(file.date), "same instant");
        assert_ne!(same.date, file.date);
    }
```

If that fixture's files carry no date (the `assert_ne!` fails), set the date bytes of one record in the fixture
builder to a real value. Do not delete the assertion.

- [ ] **Step 2: Behavioural red.** Add `clock: &'static dyn AmigaClock` to `CdSource` (field), `CdSource::open`,
`open_media`, `open_media_cached` (its closure becomes `move || open_media(&found, clock)`),
`plan_with_cache_in`, `plan_over_with_cache` (`:1705` passes it on) and `apply_staging_in` (`:1328` passes it
on). Leave `to_media_entry` unchanged, still converting as UTC.
  - Test wrappers: `plan_with_cache`, `plan_over` and `apply` pass `&crate::core::clock::UtcClock`.
  - Product: `commands/osinstall.rs:394` and `:2791` pass `&crate::tools::local_time::LOCAL_TIME` (`:2791` as
    the fourth argument, before `progress`), and `:1992` does the same for `open_media`.
  - Every test caller the compiler names passes `&crate::core::clock::UtcClock`.
Run: `...; cargo test --lib a_disc_recording_date_is_read_as_local_wall_time` → FAIL at `"same instant"`.

- [ ] **Step 3: Implement.** Make `to_media_entry` `fn to_media_entry(clock: &dyn AmigaClock, walked: &IsoWalkEntry) -> MediaEntry`
with `date: walked.entry.date.map(|unix| clock.amiga_from_unix(unix)).unwrap_or_default(),`. Its three callers
become `.map(|walked| Self::to_media_entry(self.clock, walked))`. Delete the temporary `amiga_from_unix` alias
import (`:61`). In the module doc (`:47-53`), "through the same `amiga_from_unix` conversion
`IsoSource::metadata` already uses" becomes "through the clock the source was opened with, as local wall time
(ART-317), the same conversion `IsoSource::metadata` makes".

In `scan_cache.rs`, set `const SCAN_CACHE_SCHEMA: u32 = 3;` and add to its doc:
`` /// `3`: a disc's `MediaEntry::date` is local wall time (ART-317); a schema-2 listing holds UTC and must miss. ``

- [ ] **Step 4: Pass**

Run: `...; cargo test --lib core::osinstall:: *> E:\amiga\ProjeART\build\tmp\art317-t8a.txt; Select-String -Path E:\amiga\ProjeART\build\tmp\art317-t8a.txt -Pattern "test result:"` → `0 failed`.
Run: `...; cargo test --lib commands::osinstall:: *> E:\amiga\ProjeART\build\tmp\art317-t8b.txt; Select-String -Path E:\amiga\ProjeART\build\tmp\art317-t8b.txt -Pattern "test result:"` → `0 failed`.
Run: `...; cargo build` → finishes, so no product code calls a test-only wrapper.

- [ ] **Step 5: Mutate.** Back up `source_cd.rs`. In `to_media_entry`, use `UtcClock.amiga_from_unix(unix)` → FAILS. Restore.

- [ ] **Step 6: Commit** — `osinstall: install discs read recording dates as local time; scan cache schema 3 (ART-317)`. `git add` the files listed.

---

### Task 9: Close the door — product code cannot forget the clock

**Files:**
- Modify: `src-tauri/src/core/volume/write/mod.rs` (`VolumeWriter::open` → `#[cfg(test)]`)
- Modify: `src-tauri/src/core/mod.rs` (`mod independence`)

- [ ] **Step 1: Sweep test, first seen failing.** In `mod independence`, after `core_never_names_a_time_zone_crate`:

```rust
    /// ART-317: an Amiga date is made from an instant only through
    /// `core::clock::AmigaClock`. Production code naming the epoch constant is
    /// doing that arithmetic by hand, which is how every writer came to stamp
    /// UTC. `bcpl.rs` defines it and `clock.rs` is the one converter.
    ///
    /// Mutate by pasting create.rs's old `get_current_amiga_date` above its
    /// `#[cfg(test)]`; this fails.
    #[test]
    fn core_makes_amiga_dates_only_through_the_clock() {
        let needle = concat!("AMIGA_EPOCH", "_UNIX");
        let allowed = ["adf/bcpl.rs", "clock.rs"];
        let mut offenders = Vec::new();
        for path in core_files() {
            let shown = path.display().to_string().replace('\\', "/");
            if allowed.iter().any(|ok| shown.ends_with(ok)) {
                continue;
            }
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|err| panic!("reading {}: {err}", path.display()));
            let lines: Vec<&str> = text.lines().collect();
            let regions = test_regions(&path.display().to_string(), &lines);
            for (n, line) in lines.iter().enumerate() {
                if line.trim_start().starts_with("//") || is_test_line(&regions, n) {
                    continue;
                }
                if line.contains(needle) {
                    offenders.push(format!("{shown}:{}: {}", n + 1, line.trim()));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "an Amiga date made outside core::clock (ART-317):\n{}",
            offenders.join("\n")
        );
    }
```

Run: `...; cargo test --lib core_makes_amiga_dates_only_through_the_clock` → `ok`. If it names a product line,
that line is a missed conversion: fix it through the clock, don't allow-list it.
Mutate: back up `create.rs` and paste the deleted `get_current_amiga_date` (from `git show HEAD~6:src-tauri/src/core/adf/create.rs`)
above `#[cfg(test)]` with `#[allow(dead_code)]` → FAILS naming `adf/create.rs`. Restore.

- [ ] **Step 2: Gate `VolumeWriter::open`.** Put `#[cfg(test)]` on it and change its doc to "Tests only (ART-317):
the product passes a clock through [`open_with_clock`](Self::open_with_clock)."
Run: `rg -n "VolumeWriter::open\(" D:\Projeler\Amiga\amiga-retro-toolkit\src-tauri\src\commands` → nothing above any `#[cfg(test)]`.
Run: `...; cargo clippy --all-targets -- -D warnings` → finishes with no errors.

- [ ] **Step 3: Mutate the gate.** Back up `commands/volume_write.rs`. At the `with_volume` BlockJournal site,
write `VolumeWriter::open(&mut device, geometry, image, entry.byte_offset)?` again.
Run: `...; cargo clippy --all-targets -- -D warnings` → `error[E0599]: no function or associated item named `open``. Restore and re-run: clean.

- [ ] **Step 4: Commit** — `core: product code cannot make an Amiga date without a clock (ART-317)`.
`git add src-tauri/src/core/volume/write/mod.rs src-tauri/src/core/mod.rs`

---

### Task 10: Records, full verification, review

**Files:**
- Modify: `docs/ISSUES.md`, `docs/architecture.md:69-70, :142-155`, `CHANGELOG.md:8`, `docs/STATUS.md` (Snapshot and "Picking up next session", `:190`), `docs/session-log.md:12`

- [ ] **Step 1: Full suite, twice (ART-059)**

Run: `...; cargo fmt --check` → no output.
Run: `...; cargo clippy --all-targets -- -D warnings` → no errors.
Run: `...; cargo test --lib *> E:\amiga\ProjeART\build\tmp\art317-full-1.txt; Select-String -Path E:\amiga\ProjeART\build\tmp\art317-full-1.txt -Pattern "test result:"`
Run: `...; cargo test --lib *> E:\amiga\ProjeART\build\tmp\art317-full-2.txt; Select-String -Path E:\amiga\ProjeART\build\tmp\art317-full-2.txt -Pattern "test result:"`
Expected: both `0 failed`, with identical passed/ignored counts. Record the counts.
Run: `cd D:\Projeler\Amiga\amiga-retro-toolkit; cargo deny --manifest-path src-tauri/Cargo.toml check` → all ok.
Run each, unpiped: `python scripts/control-byte-sweep.py`, `python scripts/scratch-root-sweep.py`, `python scripts/scratch-guard-sweep.py`, `python scripts/scratch-counter-sweep.py` → each clean.
`pnpm lint` / `pnpm test`: only if `git diff --stat main -- src/` shows a frontend file. This plan changes none.
On the owner's machine: `$env:ART_TZ_ORACLE='1'; cargo test --lib agrees_with_dotnet -- --ignored --nocapture` → passes, and prints 2015's January/July offsets. On an Istanbul zone they differ.

- [ ] **Step 2: `docs/ISSUES.md`.** Move ART-317 from `## Open` to the top of `## Fixed`. Make the title line
`**ART-317** 🟡 ✅ …` and add `fixed 2026-09-14 on art-debt-0914` to its italic line. The coordinator rescoped the
entry to instants only; keep that scope. If it still names `core/adf/mutate.rs`, add "(does not exist; no such
file on 2026-09-14)" after it. Append:
"**Fixed:** every writer, copy source and reader takes a `core::clock::AmigaClock`. The product's is
`tools::local_time::LOCAL_TIME` (`chrono::Local`, the offset in force on each date), and libpfs3 is
`0.1.3+art.4`. Tests: `core::clock::tests::the_offset_is_the_one_in_force_at_the_date_not_now`,
`a_new_drawer_and_a_new_file_carry_the_writers_local_time`, `a_new_disk_is_stamped_with_the_clocks_local_time`,
`a_winter_mtime_copied_in_summer_keeps_its_winter_local_time`, `a_uaem_date_round_trips_without_an_offset` (a
`.uaem` date is wall time and is never shifted), `a_pfs3_format_and_copy_in_carry_the_clocks_local_time`,
`an_ffs_format_and_copy_in_carry_the_clocks_local_time`, `a_listed_date_is_the_instant_the_writer_stamped`,
`a_disc_recording_date_is_read_as_local_wall_time`, `core_makes_amiga_dates_only_through_the_clock`; each seen
failing with its defect put back. Survivor: on a UTC CI runner, `offset_at` returning 0 passes; the ignored
`agrees_with_dotnet_timezoneinfo_across_seasons_and_years` catches it on a non-UTC machine. Dates already on
volumes stay as written."

- [ ] **Step 3: `docs/architecture.md`.** At `:142`, "There are five live instances" → "six". After the
`EmulatorLauncher` sentence, add: "`AmigaClock` (`core/clock.rs` → `tools/local_time.rs`, ART-317), the local UTC
offset in force on a given date, so every Amiga date ART writes or shows is local wall time as AmigaDOS stamps
it;". At `:69-70`, add a tree line
`│   │   │   ├── local_time.rs   #     the AmigaClock: chrono::Local, also outside core/`. In `:151`,
"unlike the other four" → "unlike the other five".

- [ ] **Step 4: `CHANGELOG.md`**, under `## [Unreleased]`:

```markdown
### Fixed

- **Dates on the Amiga now show your local time.** Files, drawers, disks and partitions ART creates or copies
  onto an Amiga volume carry the time your PC's clock shows — a file last changed in winter keeps its winter
  time — where they used to carry UTC and showed early or late by your time zone. Dates ART shows for files
  inside a disk image are read the same way. Dates carried in `.uaem` files were always your local time and
  are copied as written. Dates already on existing disks and cards stay as they were written.
```

- [ ] **Step 5: `docs/STATUS.md`.** Update the Snapshot numbers (test counts from Step 1, libpfs3 `0.1.3+art.4`).
Update the "Picking up next session" block **in place**: plan 5 done, ART-317 fixed, and the owner's two
follow-ups — run the ignored `.NET` oracle test on the owner's machine, and add the CLAUDE.md trait-table row
(Step 7). `docs/session-log.md`: a new top row, date 2026-09-14, "ART-317: Amiga dates in local time
(`AmigaClock`, `tools/local_time.rs`, libpfs3 `+art.4`, scan cache schema 3)", with the Step 1 counts ×2.

- [ ] **Step 6: Commit** — `docs: ART-317 fixed — local time everywhere`.
`git add docs/ISSUES.md docs/architecture.md CHANGELOG.md docs/STATUS.md docs/session-log.md`

- [ ] **Step 7: CLAUDE.md is outside the repository — do not edit it.** Put this row for the owner in the final
report, for `D:\Projeler\Amiga\CLAUDE.md`'s trait table:
`` | `AmigaClock` | `core/clock.rs` | `tools/local_time.rs` | the local UTC offset per date is a Windows question (ART-317) | ``

- [ ] **Step 8: Final review.** Use superpowers:requesting-code-review over `git diff main...art-debt-0914 -- src-tauri docs THIRD_PARTY_LICENSES.md CHANGELOG.md`
restricted to this plan's commits. Checklist:
  - every inventory row above is converted or explicitly justified;
  - no `.uaem` path (`uaem::parse`/`render`, `copy.rs:372-373`, `native.rs:1066-1078`, `extract_from_volume`) gained
    an offset, and `a_uaem_date_round_trips_without_an_offset` still passes;
  - no `UtcClock` in product code except the three "no date is read" comments (`rg -n "UtcClock" src-tauri/src --glob "!**/clock.rs"`, then check each hit is in a test or carries that comment);
  - `ART-PATCH.md`'s diff matches the vendored tree;
  - `Cargo.lock` added no `[[package]]`.
