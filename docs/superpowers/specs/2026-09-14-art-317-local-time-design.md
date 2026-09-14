# ART-317 — Amiga dates in local time: design

*2026-09-14, on `art-debt-0914`. The owner's decision of the same day: **local time everywhere, the offset
obtained outside `core/`**. Research: `docs/superpowers/notes/2026-09-14-debt-round-research.md` § D7. Plan:
`docs/superpowers/plans/2026-09-14-debt-5-art-317-local-time.md`. Nothing below was run; every claim is read.*

**The defect.** AmigaDOS `DateStamp()` is local wall time with no zone (pfs3aio stamps with it,
`directory.c:3540`, `format.c:393`). ART turns **an instant** into an Amiga date as UTC − 252 460 800 s, with
no offset, so on a UTC+3 machine those dates show three hours early (measured on the Windows run: libpfs3 18:15
beside hst-imager 21:15). Not every date is an instant: a `.uaem` date is already wall time (see 4).

## Decisions

1. **The offset comes from `chrono::Local` (0.4.45, `clock` feature), called only in
   `src-tauri/src/tools/local_time.rs`.** Its Windows backend calls only `GetTimeZoneInformationForYear`, for
   the year of the instant being converted, uncached [1] — so a host file modified in winter and copied in
   summer gets the winter offset, the one in force on its own date. chrono 0.4.45 is already built, with
   `iana-time-zone` and `windows-link` (its `clock` dependencies), through `delharc` and `serde_with`
   (`Cargo.lock:476-485, 725, 3444`). MIT OR Apache-2.0.
   *Rejected:* `FileTimeToLocalFileTime` — "uses the current settings for the time zone and daylight saving
   time … even if the file time you are converting is in standard time" [2]. `windows-sys` +
   `SystemTimeToTzSpecificLocalTime` — date-aware, but "may calculate the local time incorrectly" when the
   offset changes between years and the two times fall in different years [3]; chrono deliberately avoids it
   [1]; and ART would own `unsafe` FFI. The webview's `Date.getTimezoneOffset` — writers run synchronously on
   Rust job threads, so `core/` cannot ask per date. A single value sent up front is the current-offset defect
   again.
2. **The seam is a trait, `core::clock::AmigaClock`, passed explicitly.** Required: `now_unix()` and
   `offset_at(unix) -> i32`. Provided: `amiga_now`, `amiga_from_unix` and `unix_from_amiga`. `core/` ships
   `UtcClock` (today's behaviour), `FixedClock` and a test-only `SeasonalClock`; the product uses
   `tools::local_time::LOCAL_TIME`. Writers hold a `&'static dyn AmigaClock`. The no-clock conveniences —
   `VolumeWriter::open`, `create_blank_adf`, `NativeFormatter::UTC`, `plan`/`apply` — are `#[cfg(test)]`, so
   product code that forgets the clock fails `cargo clippy --all-targets`. That is the ART-295 pattern already
   used for `plan`/`apply`. "Now" is part of the trait, so a stamping test pins both values: an invariant, not
   a wait.
   *Rejected:* a process-wide setter. Cargo runs tests on parallel threads of one process, so a test setting
   +3 h races one asserting UTC; and a setter nobody called silently means UTC, which is a confident wrong
   date.
3. **libpfs3 becomes `0.1.3+art.4`, still std-only.** Two additions: `FormatOptions::datestamp:
   Option<(u16, u16, u16)>` and `Writer::set_entry_date(Option<(u16, u16, u16)>)`. `None` keeps 0.1.3's UTC.
   ART sets the entry date before each entry it writes. `ART-PATCH.md` records it. **Plan 5 runs after plan 3,
   which takes `+art.3`.**
4. **Only an instant is converted. Wall time passes through untouched.** The instants are:
   (a) "now" from the host clock — libpfs3 `util.rs:163-172`, `layout.rs:104` `amiga_now`, `native.rs:642`
   `current_amiga_date` and `adf/create.rs:192`;
   (b) a host file's mtime — `copy.rs:380-388`, `amiga_from_unix`;
   (c) an ISO recording date, whose zone `iso/directory.rs:315` has already removed — `iso/mod.rs:545`,
   `iso/mod.rs:810`, `commands/iso.rs:175`, `osinstall/source_cd.rs:156`.
   Reading back applies the same clock in reverse to produce an instant: `core/adf/fs.rs:87`
   `FileEntry::unix_date` (read by `commands/volume.rs:92` and `commands/adf.rs:137-138`, shown in the browser's
   local time by `AdfBrowser.tsx:578` and `tcFormat.ts:23`).
   **Untouched, because it is already wall time:** a `.uaem` date. `uaem.rs:206` `amiga_from_civil` turns
   zone-less text straight into a triplet, which `copy.rs:372-373` and `native.rs:1066-1078` write as read. On
   the way out, `extract_from_volume` renders the volume's triplet as text through `uaem::render`. So
   `volume → .uaem → volume` is the identity on any clock, and a test on a UTC+3 clock fails if an offset is
   applied on either leg. `volume_attributes`' `date_text` (`commands/volume_write.rs:2152`) and Amiga→Amiga
   dates (`osinstall/source.rs`) are untouched for the same reason.
   *Not Amiga dates at all:* the game index's `mtime_ms`, the scan cache's `mtime_nanos` and the journal's
   mtime. `core/adf/mutate.rs`, named in the research note, does not exist.
5. **Dates on existing volumes stay as they were written.** The CHANGELOG line says so.

## Accepted limits

- chrono picks a year's rules by the year of the UTC instant [1]. That is wrong only within the offset of New
  Year, in a year whose rules changed.
- The disc scan cache stores converted dates. `SCAN_CACHE_SCHEMA` 2 → 3 makes UTC-era listings miss. A time
  zone changed while a listing is cached shows the old offset until the listing expires.
- The browser formats `unix_date` with its own ICU rules. For historical dates these can differ from Windows'
  registry rules.
- PFS3 `copy_in` still drops a sidecar's date (ART-116). Its entries get "now", in local time.

## Sources

[1] chrono v0.4.45 `src/offset/local/windows.rs` —
<https://raw.githubusercontent.com/chronotope/chrono/v0.4.45/src/offset/local/windows.rs>. `Local` requires
the `clock` feature: <https://docs.rs/chrono/0.4.45/chrono/offset/trait.TimeZone.html>.
[2] Microsoft Learn, *File Times* — <https://learn.microsoft.com/en-us/windows/win32/sysinfo/file-times>.
[3] Microsoft Learn, *SystemTimeToTzSpecificLocalTime* —
<https://learn.microsoft.com/en-us/windows/win32/api/timezoneapi/nf-timezoneapi-systemtimetotzspecificlocaltime>.
