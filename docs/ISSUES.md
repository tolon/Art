# Known Issues & Technical Debt

Every defect found in ART gets an ID here — fixed or not. The ID is stable and
never reused, so a commit message, a code comment or an error message can point
at one and stay meaningful.

Spec §68 requires user-facing errors to carry an identifier rather than an
opaque code. These IDs are the registry those identifiers come from.

**Format:** `ART-NNN` · severity · one-line claim · where · how it fails ·
what fixed it (with the test that proves it).

**Severity:**

| | Meaning |
|---|---|
| 🔴 **Critical** | Can destroy or corrupt user data, or crash the application. |
| 🟠 **High** | Wrong results, or a spec rule broken in a way users will notice. |
| 🟡 **Medium** | Incorrect behaviour with limited blast radius. |
| 🔵 **Low** | Hygiene, dead code, developer-facing friction. |

A ✅ beside the severity marks an entry found and fixed within the same
pass — filed and closed together rather than sitting in Open in between.

---

## Open

**ART-125** 🔵 **A fallback copy reports zero bytes, and the screen prints
that as a fact** — *found 2026-08-16, in [ART-122](#fixed)'s own verification
run*
`src-tauri/src/tools/hst_imager.rs` (`parse_copy_summary`),
`src/i18n/*.json` (`preload.result.copied`) · The real run reported
`files=3933 directories=280 bytes=0`. The counts come from asking the volume
afterwards — `hst-imager fs dir -r` ends with *"280 directories, 3933 files,
12.2 MB"* — and `parse_copy_summary` reads the first two and drops the third,
because `"12.2"` is not a `u64`. So the result panel renders "Copied in: 3933
file(s), 280 folder(s), **0 bytes**".

The number is unrecoverable rather than merely unparsed: `12.2 MB` is a
rounded string, and turning it into a byte count would invent 12 782 141 bytes
that nothing measured. So the fix is *not* to parse it harder — it is to say
nothing where ART has nothing, the same rule G8's `not-checked` state follows
(§89). That needs `CopySummary::bytes` to distinguish "zero" from "not
answered", and a string for each. Native runs are unaffected: they count their
own bytes exactly.

**ART-124** 🟡 **`apply()` reports how many plan items it ran, not what the
tree holds — so the headline figure for the real 3.2 install is 98 files and
50 directories too high** — *found 2026-08-16, counting the tree the real run
had already written*
`src-tauri/src/core/osinstall/apply.rs` · `outcome.files += 1` fires once per
plan item and `outcome.directories += 1` once per directory item. Neither is
the number of entries that exist afterwards: a component that `overrides`
another writes the same destination twice by design (that is what an override
*is* — ART-112 was a missing one), and a directory named by two components is
created once by `create_dir_all` and counted twice.

Measured against the tree the real run produced, which has not been rebuilt
since: `apply()` reported **4030 files / 330 directories**;
`E:\amiga\ProjeART\dist-3.2` holds **3932 files** (plus `distribution.json`,
which `apply` does not count) **and 280 directories**. Confirmed independently
— `hst-imager fs dir -r`, after a full copy of that tree onto a PFS3 volume,
counts *"280 directories, 3933 files"*, the extra one being the manifest.

Nothing is missing or wrong on disk: every file the plan meant to place is
there, and `distribution.json` records each one once. What is wrong is the
number the run announces, which has since been quoted as the size of the tree
in `STATUS.md`, `FEATURES.md` and three issue entries. Not fixed. The fix is
to count destinations rather than items — and to decide, in the same pass,
whether the manifest counts as one of the tree's files (it is written by
`apply`, so probably yes).

**ART-119** 🔵 **Five minors deferred from Task 13's review, folded into one
entry — two closed, three still open** — *found 2026-08-15/16, Task 13's fix
round, filed at Task 14; #3 and #4 closed 2026-08-16*
`src/lib/osinstall.ts`, `src/components/osbuilder/OsInstall.tsx`,
`src-tauri/src/core/osinstall/plan.rs` · None promoted during Task 13's own
round because each is one line, harmless today, or both:

1. **Open — not a one-line fix, left as designed.** The two-plan design
   (`osinstall_plan` called once for the base plan and once more for
   `excludedConditional`) doubles the work even when `excludedConditional` is
   empty and the two requests are byte-identical. `OsInstall.tsx`'s own
   comment on `basePlanResult`/`effectivePlanResult` explains why there are
   two calls at all (a plan requested *with* a component excluded never
   carries it in `componentsOn`, so one call cannot answer both "is this
   condition satisfied" and "is this excluded"); skipping the second call
   when `excludedConditional` is empty is a real option but changes when a
   live network/IPC round-trip happens versus a cached one, which is more
   than a one-line change to reason about correctly. Left for whoever grows
   this screen next.
2. **Open — cosmetic today, not a one-line fix.** The JSX renders four
   `conditionalReason` kinds as independent guards rather than an exhaustive
   `switch`, so a fifth kind would render no reason at all. All four shipped
   kinds are covered today, and `conditionalReason`'s own return type is a
   discriminated union a `switch` would exhaustiveness-check — but rewriting
   four independent `&&` blocks into one `switch` inside this render is a
   structural change to the JSX, not a one-liner. Left as-is.
3. **Closed 2026-08-16.** The recipe-parity test (`src/lib/osinstall.test.ts`)
   did not assert a `Condition`'s **kind** string, only its `major` — a future
   condition variant other than `rom-older-than` carrying a `major` field
   would have passed the parity test while the screen still said "below
   Kickstart V47". → `"agrees on media, required, available, condition and
   exclusive_group for every id"` now also asserts
   `rc.condition.condition === "rom-older-than"` whenever a component carries
   a `condition` at all, since `ComponentDef.conditionMajor`'s own doc comment
   already says it mirrors only that one variant.
4. **Closed 2026-08-16.** The reason block lost its
   `!def.required && def.available` gate during a fix round; unreachable
   against the shipped recipe (nothing both `available: false` and
   non-required needs a reason shown), so not acted on at the time, but the
   gate's absence was not provably safe against a future recipe that combined
   the two. → Re-added to the single `reason = …` computation in
   `OsInstall.tsx` rather than to each of the four JSX guards separately, so
   there is one place, not four, that can drift out of sync again. No new
   test: unreachable against today's recipe, as the entry always said, and
   `OsInstall.tsx` has no render test to add one to yet (`ART-118` — the
   screen has never been seen rendering past its headings in a headless
   browser).
5. **Open — blocked, not judged.** *(Pre-existing, not introduced by
   Task 13.)* The base plan can hard-error on `AdfSource::open` for an
   **excluded** component's damaged or vanished disk, blanking both plans —
   `osinstall_plan` should probably treat a missing/corrupt medium for a
   component the caller excluded the same way `MediaMissing`/
   `MediaPathMissing` already treat one for a component the caller never
   asked about. Lives in `src-tauri/src/core/osinstall/plan.rs`, which another
   session was actively working in at the time of this pass — left alone
   rather than risking a collision. Still open; needs its own session.
Not fixed (#1, #2, #5) — none is data-unsafe; each is a real, small gap worth
someone's attention before the recipe or the screen grows past what today's
tests cover. #3 and #4 fixed and verified by `pnpm test` (`osinstall.test.ts`:
26 passed) and `pnpm lint`.

**ART-118** 🟠 **The OS Builder's install screen has not been seen rendering
beyond its headings** — *found 2026-08-15/16, Task 13's browser pass and
Task 14's real run*
`src/components/osbuilder/OsInstall.tsx` · A headless-Chrome probe confirmed
the route, the new `Install` kind, and five resolved `h2` strings with no raw
key and no `{{…}}`. Deeper interaction — filling the media/ROM/destination
fields, ticking a component, running Plan, reading the confirmation panel or
the refusals card, running Verify and reading its three states, switching to
Turkish — crashed the renderer reproducibly with an access violation
(`-1073741819`), in both Chrome and Edge and both headless modes, and was not
resolved. Task 14's real run (`run_the_real_engine_against_the_users_own_media_when_asked`)
exercised the same 26-component checklist and the modules-on-without-being-
chosen path **through the Rust engine directly**, never through this screen —
so the engine's own correctness is now evidenced far beyond the screen's own
verification. `ART-062` already names Turkish strings as substantially longer
than their English originals; the screen's tight controls (the component
checklist, the confirmation panel) are exactly where that would first show and
have never been seen with real Turkish content in them.
Not fixed. Needs a real `pnpm tauri dev` pass — or a working headless-browser
session — driving the screen against `E:\amiga\ProjeART\dist-3.2`.

**ART-117** 🟡 **`import_filesystem` refuses a foreign card's existing RDB —
by design, but the gap has no other path today** — *found 2026-08-16 (Task 9),
named for filing at Task 14*
`src-tauri/src/core/preload/native.rs`, `core/card/build.rs` · `create_rdb_layout`
builds an RDB **from scratch** on a fixed 16-head/63-sector LBA geometry; it
cannot edit one already on disk. Real cards disagree with that geometry —
CaffeineOS's RDB is 12 heads, 256 sectors — so writing a fresh RDB over an
existing area would invalidate every partition already in it, and
`NativeFormatter::import_filesystem` refuses by name rather than attempting
it. A card ART itself built already carries its drivers (`build.rs` lays an
RDB per area and embeds FSHD/LSEG at build time), and an FFS partition needs
no driver at all — Kickstart carries FFS — so the gap is narrower than it
first looks: embedding a PFS3 driver into a **foreign** card's existing RDB
(one ART did not build) is the one case with no path in ART today.
`hst-imager` does it; that is named in the refusal text. Not fixed — filed as
future work, not implied to already work.

**ART-115** 🔵 **A `core::iso` test flake, seen three times across this
session, never diagnosed** — *found 2026-08-15/16 (Tasks 3, 7, 8), filed at
Task 14*
`src-tauri/src/core/iso/mod.rs` ·
`extract_tree_does_not_follow_a_directory_that_points_back_at_the_root` failed
on one of several full-suite runs on three separate days of this session's
work (Task 3's second run, Task 7's first run — two failures at once — and
Task 8), always this one test, always in `core::iso`, always passing in
isolation, and never in code any of those tasks' diffs touched. **This is not
ART-059**, which is `net/`'s test-server race — a different module entirely.
The obvious cause was checked and ruled out: `core::iso`'s `tmp()` keys on
both the process id and a nanosecond timestamp, so a scratch-path collision
between parallel tests is not it. Two different modules flaking in one
session (this one and ART-059) may point at something environmental — this
machine, or `cargo test`'s default parallelism — rather than two unrelated
defects, but that is a guess, not a finding. Task 14's own two full-suite runs
(1382 passed, 0 failed, 3 ignored, both times) did **not** reproduce it.
**A deliberate reproduction attempt on 2026-08-16 failed to provoke it**: six
runs of `cargo test core::iso` in isolation and three of the full parallel
suite, all clean — nine consecutive green runs on the machine that produced
all three sightings, on the merged tree. So it is real (three independent
agents saw it, one of them twice in a single run) and it is rare enough that
nine runs do not catch it, which is the worst frequency for diagnosis and the
reason this entry records the negative result rather than quietly dropping it.

What is now known, so the next attempt does not start over: it is not a
scratch-path collision, it is not reproducible in isolation, it needs the
full suite's parallelism, and it survived the merge to `main`. What would
actually settle it is the failure output, which nobody has captured — every
sighting was reported second-hand as "2 failed, both `core::iso`". **The next
person to see it should save the panic message before re-running.** Until then
this stays open and undiagnosed; re-running until green is exactly what this
project's standing rule forbids.

**ART-110** 🔵 **A partial layout apply cannot be resumed, and the screen stays
busy** — *found 2026-08-15, the whole-branch review of SD-2 G11*
`src-tauri/src/core/layout/apply.rs` · `src/pages/ContentLayout.tsx` · Any
mid-run failure — item 5 of 10, or a `copy_tree` that hits the depth cap partway
down a tree — leaves what already landed on disk. `copy_tree_excluding` creates
the destination and then iterates, and files sort in with directories, so the
residue is real files and not merely empty folders. The next preview reports all
of it as ordinary collisions with nothing saying it is the wreckage of a failed
run, and there is no skip-existing and no resume: the only way forward is the
file manager.

Compounded on the screen, where `busy` is never cleared when a job fails or is
cancelled — the comment says "a cancelled or failed job is the job bar's to
report", but nothing clears the flag, so Preview and Apply both stay disabled
until the user navigates away and back. That half is inherited verbatim from
`VolumePreload.tsx` and is a pre-existing pattern rather than a new mistake, but
this is the screen where it bites, because this is the screen you need to re-run
after a failure.

Nothing is destroyed — `place()` refuses to overwrite, so a retry fails loudly
rather than replacing anything. Fixing it means deciding what a re-preview should
say about a destination that already holds exactly what this plan would put
there, which is a design question and not a patch.

**ART-109** 🔵 **`core/layout`'s WHDLoad tests never use LHA, and its `outside`
test does not discriminate** — *found 2026-08-15, the whole-branch review of
SD-2 G11*
`src-tauri/src/core/layout/{mod,apply}.rs` · Every WHDLoad fixture in the module
is a ZIP built at runtime; real packs are `.lha`. That is more than a fixture
nit, because the drawer's name is derived **twice, from two sources**:
`plan()` reads the archive's entry names through `archive::open`, and `apply()`
re-runs `analyse` over the *extracted* tree. They must agree, because the drawer
lands at the destination's leaf while the icon lands at
`parent.join(layout.icon_name())` — from the second answer. If the two ever
diverge for a backend, the icon lands under a name that does not match the
drawer and §82 fails silently, which is the one outcome that function exists to
prevent. One `.lha` fixture driven through `plan` → `apply` would pin it.

Separately, `a_file_outside_the_pack_is_dropped_rather_than_landing_in_the_drawer`
passes against the pre-fix code: the wrapped case never walked those paths and
the wrapper-less case always gets an empty `outside`, so the unification it was
written for has no observable behaviour to catch. The test documents real
behaviour and is not worthless, but it must not be counted as covering that
change.

**ART-108** 🔵 **Nothing you drop can reach the layout screen** — *found
2026-08-15, the whole-branch review of SD-2 G11*
`src-tauri/src/core/workflow/builtin.rs` · The module's own framing is "drop four
hundred files, get an organised card", and the only ways in are the sidebar and
a file dialog: the workflow catalogue has no entry pointing at `/layout`, so
ART's one drop pipeline cannot route anything there. The design doc does not
require it and `Navigate { route: "/layout" }` for a dropped `Directory` is its
own decision — filed because the gap is written down nowhere else.

**ART-107** 🔵 **`scan::gather` drops silently at the depth cap, and counts an
overlapping input twice** — *found 2026-08-15, the whole-branch review of SD-2
G11*
`src-tauri/src/core/layout/scan.rs` · Two ways the plan can quietly not describe
what the user dropped. `walk` returns `Ok(())` past `MAX_SCAN_DEPTH` and
`tree_bytes` returns `0`, so files below the cap are absent from the plan with
nothing on screen saying so, and a drawer's size can read low. The copy path
does this correctly — `copy_tree` **refuses** past the cap rather than
truncating — and the scan should at least count what it did not look at, so the
plan can say "n items were deeper than ART will look".

And `gather` does not dedupe: adding `E:\Games` and then
`E:\Games\Turrican.lha`, both of which the screen allows, yields the same file
twice and a self-collision the user can only resolve by removing a source.

**ART-106** 🔵 **A WHDLoad icon's destination is invisible to collision
analysis** — *found 2026-08-15, the whole-branch review of SD-2 G11*
`src-tauri/src/core/layout/mod.rs` · `collisions_in` walks `item.destination`
only, but applying an `UnpackWhdload` item also writes
`<parent>/<name>.info` beside the drawer (§82). So the preview can report no
collisions for a staging tree that already holds `Games/Turrican.info`; the
apply then silently no-ops the icon — `if !to.exists()` — and places a drawer
Workbench cannot see, which is the exact failure §82 exists to prevent, reached
from the other side.

The no-op is not the bug and does not need changing on its own: `place()`
already refuses when the *drawer's* destination exists, so an existing icon
beside a free drawer is a state only a previous partial run can produce. What is
missing is that the plan never considers the icon's path at all.

**ART-105** 🔵 **`size()` is written three times** — *found 2026-08-15, the
whole-branch review of SD-2 G11*
`src/pages/ContentLayout.tsx` · `src/components/osbuilder/VolumePreload.tsx` ·
`src/components/osbuilder/CardBuilder.tsx` · The same five-line `GIB` constant
and `size()` byte formatter, now in three screens and identical in all three.
Two copies was a judgement call; three is where it stops being one. One
`src/lib/size.ts` is smaller than the next reviewer noticing again.

**ART-104** 🟡 **The user's own A1200 Kickstart is not in the ROM database** —
*found 2026-08-14, planning a card with the real material*
`src-tauri/src/core/rom.rs` · `KNOWN_ROMS` holds one SHA-256 per ROM, and the
3.1 (40.068) A1200 entry is
`e40a5dfb3d017ba335127d85ea15c34cb27a2444230e963b7b6a1e378774d9b4`. The file the
project's own material list names —
`Kickstart v3.1 rev 40.68 (1993)(Commodore)(A1200).rom`, 524 288 bytes — hashes
to `6d43840d4099a74170ea0f0425b6257c3891ebcaa39c4d1840075a9ab22b5707`. Same ROM,
a different dump; there are several in circulation.

So `identify_rom` falls back to size and answers *Generic Amiga 512KB ROM
(Kickstart 2.x/3.x)*, and `card_plan_build` warns `RomUnrecognised` for the ROM
that is very probably right. Nothing is refused and nothing is substituted —
that part is by design — but two things are lost: the user is told ART does not
know their ROM every time they build a card, and `rom_suits` returns `None`, so
the *wrong machine* check can never fire for this file at all.

**The fix, settled the same day by prior art** ([sd0-prior-art.md §4.1](sd0-prior-art.md)).
The Emu68 Imager answers this with data rather than with code:
`Compare-KickstartHashes.ps1` compares against a **CSV carrying several hashes
per Kickstart revision**, with a `Sequence` field to choose between them, and
accepts a file of **524 288 *or* 524 299 bytes** — the second being a 512 KiB
ROM behind Amiga Forever's 11-byte header, which ART's size check rejects
outright today.

So: `KNOWN_ROMS` gains a **list** of hashes per entry rather than one, and the
headered size is accepted with the header skipped before hashing. Not the
header-parsing redesign this entry first proposed — that was a guess, and the
cheaper answer turned out to be the one a tool in the field already ships.
Reproduced by `plan_a_real_card_when_asked` with `ART_CARD_ROM` set.

**ART-101** 🔵 **The sidebar's collapse never fires under Application Size** — *open*
`src/components/layout/layout.css` · `@media (max-width: 1000px)` collapses the
sidebar to icons, "below this the sidebar's labels cost more than they give".
Media queries are evaluated against the **real viewport**, and Application Size
is a `zoom` on an element inside it — so the rule asks a question about a
window the layout does not live in. Measured while closing ART-099: in a
1258 px window the sidebar is 224 real px at 100 %, 291 at 130 % and **448 at
200 %** — over a third of the glass — while the layout itself has only 629 CSS
px to work with, which is well under the 1000 the design says it wants icons
at. So the breakpoint the design already agreed on is exactly the one that
cannot fire when it is most needed.

Not a defect in the sense of anything being wrong or unreachable: the sidebar
is merely wide, and every screen still fits (`over=0` at every size). Fixing it
means deciding *where* the breakpoint belongs — the honest answer is that it
should be asked of `innerWidth / zoom` rather than of `innerWidth`, which means
a class from `@/lib/appZoom` rather than a media query, and that is a design
choice worth making deliberately rather than while fixing something else.

**ART-093** 🟡 **ART cannot fetch an Emu68 kernel update; it can only tell you which one you need**
`core/pistorm/` · `net/` · The fix round's F4 asked for two things. The reading
half is built: the card's `Emu68.img` is identified from the `$VER:` string its
own build compiles in, and the hardware matrix names the archive that belongs on
it. The **fetching** half is not.

Not built rather than half-built, and the screen offers no button for it — the
same rule the WiFi panel follows. Three things make it more than an afternoon,
and each is a decision worth making deliberately:

- **The host policy.** `net/http_mirror.rs` refuses cross-host redirects on
  purpose (§41.5.7): a followed redirect is a fetch the user never configured.
  A GitHub release asset redirects to `objects.githubusercontent.com`, so this
  needs its own client with its own stated policy, not a relaxation of that one.
- **Which release.** The archive name depends on the release line, and one name
  means a different board in each ([ART-091](#open)). A fetch that resolves
  "latest" without the line would be the same defect with a network connection.
- **Writing it.** Unpacking an archive onto a card that boots somebody's machine
  is a multi-file write and wants the same preview → backup → verify every other
  write in ART has, per file.

Until then the screen tells the user exactly which archive to download and from
which release line, which is the part they cannot work out for themselves.

Recorded 2026-08-13 as owed work, not a defect.

**ART-087** 🔵 **Space marks a row but does not compute a directory's size**
`src/pages/FileManager.tsx` · `src/lib/selection.ts::spaceToggle` · The brief
(§3.2) asks for Total Commander's `CountSpace=1`: Space on a **directory**
marks it *and* walks it, replacing the `<DIR>` in the Size column with the real
total. ART marks; it does not count.

The reason is the phase's own rule. There is no primitive to count with:
`panel_list_local` lists one level, `scan_collection_directory` looks for Amiga
files rather than totalling bytes, and `volume_plan_copy` computes a size only
against a destination volume. A recursive walk is new engine capability — small
and read-only, but new — and phase 2b's plan says a gap found on the way is
filed rather than smuggled in.

Fixing it means one command per side of the fence: a depth-limited local walk
(the same guards `scan_collection_directory` already has — bounded depth, no
symlink following) and a volume-side directory total, both as jobs, because a
directory of forty thousand files must not block the command thread and must
be stoppable. The Size column then needs a third state — not just a number or
`<DIR>`, but "counting…".

Found while building phase 2b task 5.

**ART-081** 🟡 **A single file cannot be moved between two images, because the primitive underneath addresses a directory**
`src-tauri/src/commands/volume_write.rs::volume_copy_between` ·
`src/lib/movePlan.ts` · The command takes a *directory* block at both ends and
copies that directory's tree. F5 on a lone file between two images already
passes the pane's own `dirBlock` and therefore copies the whole folder the file
happens to be in — noisy, but harmless, because F5 deletes nothing. F6 (Move)
cannot use that: it would copy twenty files, delete the one that was marked, and
report a move.

So `planMove` refuses it by name (`files.move.refuseFileBetweenImages`), and
the test `refuses a lone file between two images` pins the refusal. Folders
move between images; files move out to a host folder; a file between two images
is copied with F5 for now.

Fixing it means a command that stages one *entry* — extract to a scratch path,
then `put_file` into the destination volume, inside one write session at each
end so the backup and journal guarantees hold — which is the same missing
primitive ART-064 needs for batching, and it should be built once for both.
The F5 whole-folder surprise above is worth fixing in the same pass.

Found while building F6 in phase 2b task 3; recorded rather than smuggled into
a UI task, which is what that task's plan requires.

**ART-080** 🔵 **ART cannot delete a file on the user's own disk, so nothing can be moved *off* a host folder**
`src/lib/movePlan.ts` · `src-tauri/src/commands/panel.rs` · Every delete ART
owns goes *into* a disk image through `core/volume/write`; there is no command
that removes a file from the user's own filesystem, and that is deliberate —
Explorer does that job and a two-pane commander that can silently delete host
files is a much larger safety surface than one that cannot.

The consequence, once F6 became Move: the most obvious move of all — drag a
game off `D:\downloads` and onto a floppy image — is a copy, and the original
stays. F6 says so (`files.move.refuseLocalSource`) instead of being disabled
with no explanation, and points at F5.

Fixing it is not a line of code but a decision: a host-side delete needs its own
`Safety` class, its own confirmation, its own oplog entry, and a policy on
recycle bin versus unlink. Worth doing deliberately, if at all.

Found while building F6 in phase 2b task 3.

**ART-078** 🟡 **An AmigaOS CD's protection bits and file comments are lost, because Rock Ridge and the Amiga `AS` entry are not read**
`core/iso/` · ART reads ISO9660 and prefers Joliet when a disc carries it.
Neither carries what an Amiga CD actually says about its files: protection bits
(`HSPARWED`) and the file comment live in the **Amiga `AS` System Use entry**, a
Rock Ridge-style extension, and ART reads no System Use area at all. Two
consequences, both quiet:

- **A WHDLoad-era disc loses its slave's `S` and `P` bits on the way out.** A
  game copied off a CD onto an HDF can arrive with the right bytes and the
  wrong protection, which is a game that starts and then does not work — the
  same class of failure §7.2 records for archives, where ART *does* carry the
  bits through `.uaem` sidecars.
- **A Unix-mastered disc with no Joliet descriptor falls back to uppercase
  8.3 names.** Rock Ridge is where its real names are, so `MyGame.info`
  becomes `MYGAME.INF` and the icon stops matching the drawer.

Neither is a regression: nothing ever claimed to read them, `FEATURES.md` and
`format-support-matrix.md` both say so, and the disc still copies. Fixing it
means reading the System Use area after each directory record, handling the
`SP`/`CE`/`NM` continuation entries, and mapping `AS` onto the same
`Protection` type `core/volume/write` already has — at which point the
existing `.uaem` writer carries the bits out to a folder for free.

Found while closing Phase 2a; recorded rather than fixed because it is a
format layer of its own, not an omission in the one that landed.

**ART-073** 🟡 **`delete_many`'s all-or-nothing guarantee only holds for the whole-file strategy**
`src-tauri/src/commands/volume_write.rs::delete_many` (line ~505) · The
pre-check (`check_batch_deletable`) runs once, against a read-only listing,
before the writer session opens — for a floppy-sized image (the whole-file
strategy) that is enough: nothing is written until the whole in-memory
result validates, so a batch that cannot fully succeed leaves the file
untouched, and every test in this module exercises that path. On a
block-journal image (a large HDF) each `writer.delete(...)` inside the
session's loop is its own committed, journalled operation, already durable
in the file the instant it returns — there is no whole-image commit step to
refuse at. An error partway through the loop after the pre-check passed
(a name resolving differently a moment later, say) leaves the earlier
deletes in the batch standing rather than none of them, breaking the
all-or-nothing promise the doc comment now qualifies. Reachable in
principle whenever a batch delete runs against an HDF rather than an ADF.
Not reachable through the case-different-name path any more —
`dedupe_case_insensitive` (added in the same pass that found this) closes
that specific trigger — but the underlying strategy gap is still open. Fix
would need the block-journal strategy to buffer its own generation of
deletes behind one commit point the way the whole-file strategy already
does, which is a real design change, not a one-line fix.

**ART-069** 🔵 **No frontend test renders `FileManager.tsx`**
`src/pages/FileManager.tsx` · It calls Tauri commands (`onVolumeWriteResult`,
`onJobProgress`, panel listing, …) on mount, which is why every phase-1a
frontend test extracts a pure function or hook instead of rendering the
page — `@/lib/selection`, `@/lib/functionKeyPlan` (added closing finding 4
of the phase-1a whole-branch review), `usePaneTab`/`isShortcutBlocked` in
`FunctionKeys.tsx`, and so on. Each extraction is real, tested logic, but
none of them proves the page actually *wires* the extracted piece
correctly — that an F-key's `run` reads the same `target` its `enabled`
was computed from, that a click handler calls the selection function it
looks like it calls, that the two `useEffect` result listeners registered
at mount really are registered before any button can start a job. Closing
this needs either a mock of the Tauri IPC surface (`@tauri-apps/api/core`'s
`invoke`, `@tauri-apps/api/event`'s `listen`) sufficient to render the page
in a test, or splitting `FileManager.tsx` into smaller components each
small enough to mock individually — a real task, not a quick fix.

**ART-065** 🟡 **Volume→local multi-select is several concurrent operations, not one**
`src/pages/FileManager.tsx::copySelectionTo` (line ~1090) · When the source
pane is a volume and more than one entry is selected for extraction to a
local folder, each entry becomes its own concurrent operation inside a
single `Promise.all` — a subdirectory goes through its own `volumeCopyOut`
job (awaited individually inside the map callback), a plain file through its
own direct `volumeExtractTo` call — rather than the one atomic, one-job
operation `volumeCopyInMany` (local→volume) and `volumeCopyBetween`
(volume→volume, staged) both give their directions. Each individual
extraction is still safe on its own — every write is the same
backup-and-validate pipeline as ever — but the *batch* has none of the
all-or-nothing guarantee the other two directions do: a selection of ten
entries where the seventh fails to extract leaves the first six on disk and
the last three silently never attempted, with no report tying the partial
result back to "this was one selection." Needs a batched extract primitive
(`volume_extract_many`, mirroring `volume_copy_in_many`'s shape) rather than
fixing the concurrency at the call site.

**ART-064** 🟡 **Volume→volume multi-select refuses rather than batching**
`src/pages/FileManager.tsx::copySelectionTo` (line ~1124) · "Two volumes and
more than one entry: not supported yet" — `setError(t("files.err.batchBetweenVolumes"))`
("Copying several entries between two images at once is not supported yet —
copy them one at a time."). Not a defect in the sense of wrong behaviour: the
refusal is explicit, immediate, and names the reason, which is exactly what
§89 asks for when a case is not handled. It is recorded here because it is
the one direction of the four (local→volume, volume→local, volume→volume
single-entry, volume→volume batch) Task 8's roadmap self-review calls out by
name as deliberately not built: there is no `volume_copy_between_many`
primitive to build a batch on top of — `volume_copy_between` (the command
`e3035cf` added end-to-end coverage for) stages exactly one directory tree
through a temp folder per call, and doing several would mean either several
separate stage-and-insert round trips (no shared atomicity, the same
weakness as ART-065) or a `HostSelection`-shaped staging step that does not
exist yet on the extract side. Needs its own task, not a quick fix — see
`ART-065` for the sibling gap it would need to close at the same time.

**ART-062** 🔵 **No language has been checked on screen**
`src/i18n/tr.json`, `src/i18n/en.json` · Every Turkish string landed this phase
was verified by `pnpm test`'s key-parity check and by reading the JSON — never
by opening the running application and looking at a screen. Several Turkish
strings are substantially longer than their English originals and sit in tight
controls, so the check that remains is visual, not automatable:

| Key | English | Turkish | Growth |
|---|---|---|---|
| `pistorm.saveSync` | "Save & Sync PiStorm SD" | "PiStorm SD'yi Kaydet ve Eşitle" | +36% |
| `hardDisk.bootablePri` | "Bootable (Pri {{n}})" | "Önyüklenebilir (Öncelik {{n}})" | +50% |
| `pistorm.profile.classic.badge` | "Cycle-Exact & Demos" | "Çevrim Hassasiyetli ve Demolar" | +58% |
| FileManager function-key label | "View" | "Görüntüle" | 4 → 9 characters |
| FileManager function-key label | "Grid" | "Izgara" | +50% |
| job status | "Done" | "Tamamlandı" | +150% |
| job status | "Failed" | "Başarısız" | +50% |

The function-key bar (`src/components/files/FunctionKeys.tsx`) was inspected
in source rather than run: its container carries `flexWrap: "wrap"` and each
button `flex: "1 1 90px"`, and `.btn` in `src/styles/global.css` carries no
`text-overflow` rule (only `.file-row-name` does), so the bar should wrap
rather than clip. That makes it the most likely of the rows above to look
merely cramped rather than the most likely to break outright — but nobody has
looked at it in Turkish. Needs an actual run of `pnpm tauri dev` with the
language switched to Turkish, working through PiStorm, the hard disk screen,
and the Files function-key bar at a few window widths.

**ART-060** 🔵 **Rust-side error sentences do not translate**
`core/error.rs::CoreError`, `commands/whdload.rs::WhdloadRefusal` · Every
`CoreError` variant's `Display` implementation, and `WhdloadRefusal { reason,
suggestion }`'s two fields, are English sentences written in `core/` and
`commands/`, reaching the UI verbatim regardless of the language chosen in
Settings. `CoreError::user_message()` appends the stable `ART-*` id from
`code()` to the English sentence for exactly this reason (§68) — the id was
always meant to be the stable, quotable part — but nothing today keys off it
to show a translated sentence instead. This is a design question worth
recording, not answering here: `core/` may not depend on the frontend's
`react-i18next` catalogue (CLAUDE.md's core-independence rule), so there are at
least two ways forward — move the sentences into the frontend catalogue, keyed
by `CoreError::code()` / a `WhdloadRefusal` reason code, and have the UI look
them up instead of rendering the string Rust sent; or give `core/` its own
minimal, dependency-free catalogue and have `Display` consult it. The first
keeps `core/` exactly as independent as it is today but means every error path
has to carry a stable key instead of (or alongside) a sentence, and duplicates
some phrasing decisions between Rust and the JSON catalogues. The second keeps
the sentence and its translation next to each other but adds a resource-lookup
concept to a crate that currently has none. Neither is decided here.

**ART-050** 🟡 **The §92 pre-flight gate does not check bitmap consistency or hash-chain integrity**
`core/adf/validate.rs::validate_image` · `commit_whole_file` (ART-042) refuses
a write when `validate_image` finds a `Problem`, but `validate_image` only
covers the bootblock signature, its checksum, the block count and the root
block's type. It does not walk the bitmap against what is actually allocated
and does not walk a hash chain for consistency, so an operation that leaves
two files owning the same block, or an entry linked into the wrong bucket,
still passes the gate and commits. `CHANGELOG.md`'s entry for ART-042 was
corrected in this pass to stop implying broader coverage than this. Deepening
`validate_image` to catch these is real work, not a flag to flip.

Missing features are not defects — see [FEATURES.md](FEATURES.md) for what is
not built yet, and [STATUS.md](STATUS.md) for what is scheduled.

Every module with working logic has now been audited. The remaining `core`
modules are stubs that only return `NotImplemented` (`recovery.rs`,
`conversion.rs`, `binary.rs`, `validation.rs`) or hold types with no logic
(`compatibility.rs`) — see [FEATURES.md](FEATURES.md) for their planned state.

Two areas were reviewed and found sound, and are recorded here so nobody
re-audits them without reason:

- `core/analysis.rs` — the hex reader clamps both offset and length, and the
  signature scan guards its window.
- `core/profile.rs` — preset data only, no parsing of untrusted input.

---

## Fixed

**ART-122** 🟠 ✅ **`hst-imager` cannot write into a volume `NativeFormatter`
just formatted — the first copy dies `ERROR_DISK_FULL`, and it dies *after*
the destructive format has already run** — *found 2026-08-16, measuring the
real AmigaOS 3.2 tree through the ART-120 fallback path; fixed 2026-08-16*
`src-tauri/src/core/preload/native.rs` (`format_partition`, PFS3 branch),
`src-tauri/src/commands/preload.rs` (`run_with_fallback`) · **This is the
exact combination ART-120 built.** The native path formats every partition —
that step never falls back — and hands the copy to `hst-imager` only when the
tree carries a non-ASCII AmigaDOS name (ART-113). So the fallback's own
working case is: *ART formats, `hst-imager` fills*. It does not work.

Minimal reproduction, no ART process involved after the format — two files,
15 bytes, into a **400 MB** freshly formatted PFS3 partition:

```text
# an ART-formatted PFS3 volume (any empty tree through the hook below)
hst.imager fs copy <tree> <image>\rdb\dh0 --recursive --makedir
  → System.IO.IOException: ERROR_DISK_FULL
     at Hst.Amiga.FileSystems.Pfs3.Directory.NewFile(...)
# the identical command, run a second time
  → 1 directory, 2 files, 15 B copied
```

**The first attempt repairs what the second one needs.** Byte-diffing the
image before and after the failed run: it changed three blocks inside the
rootblock cluster's **reserved bitmap** — most visibly 120 bytes at the tail
of one block, which ART's format left as `FF` (free) and the failed attempt
zeroed. Formatting the same partition with `hst-imager` instead writes those
same bytes as zero from the start, and it stops four bytes earlier still: the
two implementations disagree about how much of the reserved area exists, not
about one bit. Counted over the rootblock cluster, ART's reserved bitmap
marks **14 684** blocks free where `hst-imager`'s marks **11 188**.

**Whose arithmetic is right: settled, and it is ART's.** Parsing both
rootblocks field by field puts the disagreement in three coupled numbers —
`lastreserved` 29 569 vs 22 401, so `numreserved` **14 784** vs **11 200**;
`rblkcluster` 6 vs 4; `blocksfree` 789 934 vs 797 102, the difference being
exactly the 3 584 extra reserved blocks × 2 sectors. Both rootblocks are
internally consistent and self-describing; neither is corrupt. The tie is
broken by the reference implementation — `pfs3aio`'s own `format.c`, whose
`CalcNumReserved` is:

```c
taken = 32;
for (ULONG i = 2048; i && i / 2 < temp; i <<= 1)
    taken += taken * (i >= 512 * 2048 ? 10 : 14) / 16;
taken /= resblocksize / 1024;
taken = min(MAXNUMRESERVED, taken - 1);
taken = (taken + 31) & ~0x1f;
```

Worked through by hand for this partition (819 504 blocks, 1 KB reserved
blocks) that yields **14 784** — ART's number, to the block. `libpfs3` 0.1.3's
`calc_num_reserved` is a faithful port of it, and the reserved-bitmap size
loop matches `MakeReservedBitmap` as well. So **ART's format is what the
implementation an Amiga actually runs would have written**, and this is *not*
the "ART agrees only with itself" shape of ART-032 … 035, ART-075 and
ART-079 — the third implementation was consulted and it sides with ART.
What `hst-imager` cannot do is write into a volume laid out that way; it
writes into its own fine (checked: the same tree, the same command, into an
`hst-imager`-formatted volume, first try).

Severity is High rather than Critical because nothing is destroyed that the
user did not already confirm losing — the format step is the `Destructive` one
and it succeeded. But the user is left with a formatted, empty partition and
an error, which is the worst moment for the operation to stop.

**Fixed 2026-08-16 — a partition is formatted and filled by one tool.** The
defect ART can fix is its own design: `run_with_fallback` chose per *step*, so
a format ran natively while the copy into the volume it had just made fell
back, which is precisely the combination that fails. It now chooses per
**partition**. `VolumeFormatter::can_copy_in` (new, default `Ok(())`, native's
implementation sharing `plan_copy` and `non_ascii_refusal` with `copy_in` so
the two cannot drift) answers *before* the destructive step whether the copy
that follows will fall back; when it will, the format goes to the fallback
with it, reported as `FallbackReason::PairedWithFallbackCopy { drive }` rather
than by repeating the copy's own reason against a step that reason is not
about. When no fallback tool is configured, the run now refuses **before** the
partition is erased instead of after.

Verified on the real thing: the 3933-file / 280-directory AmigaOS 3.2 tree,
one run, no manual retry — `hst-imager`'s own listing counts *280 directories,
3933 files, 12.2 MB* with `türkiye.country` and its siblings by name
(`carry_the_real_dist_tree_through_the_fallback_path_when_asked`). Unit tests:
`a_partition_whose_copy_must_fall_back_is_formatted_by_the_same_tool`,
`one_partitions_fallback_does_not_pull_another_partition_with_it` (the choice
is still per partition, not per run),
`a_partition_whose_copy_needs_a_missing_tool_is_not_formatted_first`,
`a_copy_with_no_format_of_its_own_still_falls_back_by_itself`. Mutation-checked
twice: disabling the pairing fails three of them, and pairing on `slot` **or**
drive instead of both fails the per-partition one. The preview follows the same
rule (`plannedToolPhrase` now takes the plan, so a format paired with a copy is
labelled as conditional as that copy — ART-121's rule kept true).

**What is still open, and belongs to `hst-imager` rather than to ART**: its
PFS3 writer cannot write into a `pfs3aio`-shaped volume it did not format. ART
no longer asks it to. Worth reporting upstream, and worth remembering if a
future ART feature wants to fill a volume some *other* tool formatted.

Two directions were considered, and the second one's investigation is what
made the first one the *right* fix rather than a workaround:

1. **One tool per partition** — taken. Once the arithmetic was settled, "never
   mix two formatters inside one volume" stopped being a way of dodging the
   question and became the correct rule: ART's format is right, the external
   tool cannot fill it, so it must not be asked to.
2. **Change ART's own PFS3 arithmetic to match `hst-imager`'s** — rejected on
   the evidence above. It would move ART *away* from what `pfs3aio` writes, to
   please a tool that is the fallback rather than the target. The target is an
   Amiga.

Reproduced by `commands::preload::tests::carry_the_real_dist_tree_through_the_fallback_path_when_asked`
(`#[ignore]`d, env-gated), which is also what measured the tree. The two
images the byte-diff above came from are kept on the scratch drive rather than
described only in prose: `E:\amiga\ProjeART\exp-fmt-before.hdf` (the same
partition formatted by `NativeFormatter`) and `E:\amiga\ProjeART\exp-hstfmt.hdf`
(formatted by `hst-imager`), identical in every other respect —
`E:\amiga\ProjeART\exp-tree` is the two-file non-ASCII tree that triggers the
fallback in seconds.

**ART-123** 🔵 ✅ **A failed `hst-imager` command reported a stack frame
instead of what went wrong** — *found and fixed 2026-08-16, while diagnosing
[ART-122](#open)*
`src-tauri/src/tools/hst_imager.rs` · `last_meaningful_line` took the last
non-empty line of the tool's output, which is right for the handled case —
`hst-imager` prints one `[ERR] Partition 'dh9' not found` and stops — and
wrong for an **unhandled** exception, which prints the message and then eight
stack frames. ART showed the user
`at Hst.Imager.ConsoleApp.CommandHandler.Execute(CommandBase command)`, which
says nothing at all; the sentence they needed
(`System.IO.IOException: ERROR_DISK_FULL`) was twelve lines above it. ART-122
was invisible for two runs because of this. → Frames (`at Namespace.Method(…)`,
matched narrowly so a message merely beginning with "at" is not swallowed) are
skipped; a trace with no message at all still reports its last frame rather
than nothing. Three tests, the first built from the real captured output:
`an_unhandled_exception_reports_its_message_and_not_a_stack_frame`,
`a_handled_error_is_still_its_last_line`,
`a_trace_with_no_message_falls_back_to_its_last_frame`.

**ART-121** 🟡 ✅ **ART-120's own fix wave, reviewed — four findings, folded
into one entry** — *found 2026-08-16 in review of ART-120; fixed 2026-08-16*
`src/i18n/en.json`, `src/i18n/tr.json`, `src/lib/preload.ts`,
`src/components/osbuilder/VolumePreload.tsx`,
`src-tauri/src/commands/preload.rs`, `docs/STATUS.md`, `CHANGELOG.md` ·
ART-120 made `NativeFormatter` reachable and the engineering held up under
review, but the wave that landed it left the product saying the opposite of
what it now does, and left the record of the wave itself half-written.

1. **Fixed — two shipped strings told the user the opposite of what the code
   does.** `preload.scope` (the warning badge at the top of the very screen
   ART-120 changed) still read *"ART does not write PFS3 itself. hst-imager
   does the formatting…"*, and `layout.scope` still read *"A real PiStorm
   card is PFS3, which ART cannot write directly."* Both false since
   `NativeFormatter` became reachable. Rewritten in both catalogues to say
   what is true now — native by default, `hst-imager` a named fallback for
   the two known gaps — while keeping each string's own role (a scope badge,
   not a changelog entry).
2. **Fixed — `preload.result.notVerified` was inverted whenever the native
   path did the work.** It read *"what is inside is the tool's word and not
   ART's"*, which stopped being true the moment ART started writing the
   volume itself. Reading `preload_run`'s own job closure settled which of
   the two honest framings applies: it does not run any readback/verify pass
   after either writer finishes — the record's own comment ("ART has no
   PFS3 reader here") is about *this operation*, not about whether `libpfs3`
   can read at all (`core/osinstall/verify.rs` uses the same crate to do
   exactly that, for a different screen). So "not verified" is true for a
   native run and a fallback run alike, for the same reason in both cases —
   not "the tool's word", but "nobody re-read what either writer wrote here".
   The string now says that.
3. **Fixed — the writer for a destructive operation changed and the screen
   never said so.** Formatting a partition is `Destructive`, its writer
   moved from an externally-validated tool to ART's own (still 🟡 in
   `FEATURES.md` — 3061 of 4030 files proven, no Amiga has booted it), and
   nothing on the preview named which one was about to run. No Settings
   toggle was added — the user's decision was "native by default, named
   fallback", not a choice to expose. Instead `src/lib/preload.ts::
   plannedToolPhrase` labels each planned step before the confirmation
   checkbox: `import-filesystem` and `format-partition` are static facts
   (ART-117 always needs `hst-imager`; a format never does), and `copy-in`
   names the *possibility* of ART-113 rather than a verdict, since whether a
   source tree carries a non-ASCII name is a fact about that step's own
   content the plan does not scan for — the same reason `needsExternalTool`
   already draws that line. `VolumePreload.tsx` renders it beside
   `stepPhrase` in the plan list, so the reason (if any applies) is visible
   before Format and Fill is pressed, not only after in the result panel.
4. **Fixed — the result panel's tool label was wrong when every step fell
   back.** `run_with_fallback` set `outcome.tool = native.probe().ok()`
   **before the loop ran**, so a run where every single step actually went
   through `hst-imager` still printed "By libpfs3 … (native)" — contradicting
   the per-step list rendered directly beneath it. `outcome.tool` is now
   computed from what actually ran: `native`'s version when every step used
   it, the fallback's when every step did, and nothing at all for a mixed
   run — the summary line is honest about not being able to speak for a
   disagreement the per-step list already shows.
   `commands::preload::tests::
   every_step_falling_back_makes_the_summary_follow_the_fallback_tool`
   mutation-checks it: a two-step plan where the real `NativeFormatter`
   refuses both (ART-117, unconditionally) asserts `outcome.tool` names the
   fallback, not `native` — the exact case the bug shipped with.
5. **Fixed — `preload.fallback.nonAsciiPfs3Names` passed `{{count}}` with no
   plural forms**, unlike every other count-bearing key on the same screen
   (`preload.confirm_one`/`_other`, `preload.plan.willErase_one`/`_other`).
   Split into `_one`/`_other` in both catalogues; `fallbackPhrase`'s own
   return shape did not need to change, since i18next resolves the suffixed
   keys from `{{count}}` at `t()`-time.
6. **Fixed — `docs/STATUS.md` and `CHANGELOG.md` carried no record of ART-120
   at all.** Six commits landed "hst-imager is no longer required to prepare
   a card" — the most user-visible change in a while — and neither file
   said so. Both now carry it, and `STATUS.md`'s own entry states plainly
   what was *not* re-run: the real 4030-file `dist-3.2` tree was not carried
   through the new fallback path end to end, so the 969/106 non-ASCII
   figures in `FEATURES.md` remain the prior, separate measurement.

**Noted, no action — recorded rather than fixed:** `core::preload::run` (the
single-formatter runner `run_with_fallback` was built beside) is now reached
only by its own tests and the `#[ignore]`d real-card hook; kept because it is
still what a caller with exactly one formatter in hand — a test, or
`hst-imager` alone — uses, and CLAUDE.md's core-independence rule wants it
untouched by the fallback choice. `is_windows_reserved_component`
(`core/preload/native.rs`) does not match a trailing-dot reserved form
(`AUX.`) — irrelevant here since AmigaDOS names never carry a trailing dot,
so there is nothing this check is failing to catch on the data ART actually
handles.

**ART-120** 🟠 ✅ **`NativeFormatter` was unreachable from the application —
every preload a user ran still shelled out to `hst-imager`** — *found
2026-08-16, after G5 merged; fixed 2026-08-16*
`src-tauri/src/commands/preload.rs` · SD-2's Phase B was built so that ART
could format an Amiga volume and copy files into it **without launching
anything**: `core/preload/native.rs` implements the `VolumeFormatter` trait
over `libpfs3` and ART's own FFS writer, it is covered by its own tests, it is
checked in both directions by an independent `hst-imager` oracle, and it was
run against the user's real AmigaOS 3.2 media. **None of that was reachable
from the product.** `commands/preload.rs` constructed `HstImager::at(...)`
unconditionally — the formatter was not a choice, a setting or a fallback —
and outside its own file `NativeFormatter` appeared only in test modules.

**The decision taken: native by default, `hst-imager` a named fallback —
chosen per step, not per run.** `commands/preload.rs::run_with_fallback` tries
`NativeFormatter` first for every step in a plan and only reaches the
configured `hst-imager` for the two known capability gaps, both typed values
`core::error::CoreError` already carries: ART-113's
`NonAsciiPfs3Names` (a `copy-in` whose source tree has a non-ASCII AmigaDOS
name onto a PFS3 partition) and ART-117's new
`ForeignRdbEmbedNotSupported` (`import-filesystem` — refused unconditionally,
for every card, so this one is known before the plan even runs). Both are
safe to retry with the other tool because both are refused **before anything
is written** — `import_filesystem` never opens the image, and the ART-113
check runs before `FileRegionMut::open` — so trying native first never leaves
a half-written step behind. `FallbackReason::from_native_error`'s match is the
whole policy; nothing else falls back, so a real failure (a full volume, a
malformed image) surfaces as-is rather than being silently retried on another
tool.

**Per step, not per run, because the three kinds of step have different
needs.** `import-filesystem` always needs `hst-imager`; `format-partition`
and almost every `copy-in` run natively; only a `copy-in` whose content has a
non-ASCII name needs the fallback too, and that is a fact about that step's
own content. A run-level choice would have forced every step onto
`hst-imager` because of one accented folder name elsewhere in the tree, or
wasted the native path on everything after a driver import it could have done
fine.

**Never silent.** `StepReport { step, tool, fallback_reason }` — one per step,
always present, a plain `"native"` reported exactly as deliberately as a
fallback is — travels on `PreloadResult.steps`, is written into the operation
log's `"Fallback"` detail when any step used one, and is rendered per step in
`VolumePreload.tsx`'s result panel via `src/lib/preload.ts::fallbackPhrase`
(new `preload.fallback.*` keys, both languages). When the fallback is needed
and no `hst.imager.exe` is configured, `missing_tool_error` refuses before
that step's formatter call — naming the step, that `hst-imager` is needed,
and why — never a partial attempt. `preloadBlocker` on the frontend no longer
requires a tool path at all except when the plan already shows an
`import-filesystem` step (`needsExternalTool`); a `copy-in`'s non-ASCII gap is
a fact about content the plan does not scan for, so it can only be reported
after a run tries it, same as the in-app answer above.

**`core/` stayed free of the choice**, per its own rule: `VolumeFormatter` is
unchanged, `core::preload::run` (the single-formatter runner) is untouched,
and `run_with_fallback` — the thing that picks between two formatters — lives
entirely in `commands/preload.rs`. The one `core/` change is
`CoreError::ForeignRdbEmbedNotSupported`, a dedicated variant replacing the
generic `NotImplemented` `NativeFormatter::import_filesystem` used to return —
needed because the fallback choice has to tell "known capability gap, safe to
retry" apart from every other unbuilt corner of the engine that also returns
`NotImplemented`.

`docs/FEATURES.md`'s OS-install row is corrected alongside this entry: it no
longer claims no command calls `NativeFormatter`, and says plainly that the
969/106 non-ASCII figures from the real-media measurement were not re-proven
against the new fallback path end to end.

Mutation-checked, the three properties the fix is *for*: **native chosen by
default** — `commands::preload::tests::native_is_chosen_by_default_over_a_configured_but_unreachable_tool`
points the fallback at an `HstImager` whose executable does not exist, so any
code path that used it even once for a step native can do would fail with an
I/O error rather than pass; the same property is pinned on the frontend by
`src/lib/preload.test.ts`'s `"does not require the tool when the plan does not
need it"` and `phrase-keys.test.ts`'s explicit `toolPath: null` assertion.
**The fallback fires only for the step that needs it** —
`commands::preload::tests::a_non_ascii_source_tree_falls_back_only_for_the_step_that_needs_it`
runs a real `NativeFormatter` against a two-step plan (one ASCII
`format-partition`, one PFS3 `copy-in` with a `español` directory) and asserts
the first step's report says `"native"` while only the second falls back to a
recorder — a run-level rewrite would have failed the first assertion.
**A missing tool refuses rather than half-running** —
`commands::preload::tests::a_missing_fallback_tool_refuses_before_the_rest_of_the_plan_runs`
uses a recorder that fails `import_filesystem` with
`ForeignRdbEmbedNotSupported` and asserts both that the run errors and that
`format`/`copy` never ran. A fourth,
`commands::preload::tests::a_real_failure_is_not_treated_as_a_reason_to_fall_back`,
is the negative control every one of those needs: an `Io` failure is
surfaced as-is and the fallback recorder is never touched. `core::preload::
native::tests::import_filesystem_refuses_rather_than_guess` now asserts the
specific `ForeignRdbEmbedNotSupported` variant, not just its error code, so a
regression back to the generic `NotImplemented` fails it even though the code
string alone would not have caught that. Wire shapes are pinned in
`commands::preload::tests::wire_shapes` (`StepReport`, `FallbackReason`,
`PreloadResult`'s new `steps` field) the same way `commands/osinstall.rs`'s
own `wire_shapes` module already does.

Also closed in the same commit: `scripts/pfs3-oracle-check.py`'s ART-114
skip-path (Windows-reserved basenames) had never been exercised end to end,
because the agent that fixed ART-114 was forbidden from touching `native.rs`'s
fixture. `build_pfs3_volume_for_oracle_when_asked` now writes a real
`DOSDrivers/AUX` entry — `AUX` is the real case, a genuine AmigaOS serial-port
DOSDriver name, the same one `Storage3.2.adf` and `GlowIcons3.2.adf` carry —
and a live run against `E:\amiga\Amigatolon\hstimager\hst.imager.exe`
confirmed the script reports it as `1 file(s) skipped … not a failure` rather
than an unexplained shortfall, with every other check (names, sizes, hashes,
protection bits, both directions) passing.

**ART-116** 🔵 ✅ **ART's PFS3 writer carries protection bits but drops a
`.uaem`'s comment and date; the FFS branch keeps both** — *found 2026-08-16
(Task 11), named for filing at Task 14, made visible 2026-08-16*
`src-tauri/src/core/preload/native.rs` (`copy_in_pfs3` vs `copy_in_ffs`) ·
`libpfs3` 0.1.3 exposes `update_dir_entry_protection` and nothing else for a
directory entry — no setter for a comment or a date — so the PFS3 branch
applies a sidecar's protection bits and silently drops its comment and date,
while the FFS branch (`FileMeta`) carries all three. Protection is the
load-bearing field (`Resident C:Assign PURE` is why this phase exists) and
nothing was broken — G5 verified end to end on PFS3 without it — but one
operation's two branches diverging silently on metadata a user could have set
by hand is not something to leave unmarked. `libpfs3` is still pinned at
`=0.1.3` with no upstream answer, so the loss itself is **not fixable** —
what changed is that it is no longer silent.
→ [`CopySummary`](../src-tauri/src/core/preload/mod.rs) gained
`comments_lost`/`dates_lost` (`#[serde(default)]`, so an older reader still
deserializes a summary without them). `copy_in_pfs3` counts an entry toward
each the moment its `.uaem` sidecar carries a non-empty comment, or a date
other than `AmigaDate::default()`, that `update_dir_entry_protection` cannot
carry — the same "is this actually worth mentioning" rule
`core/volume/write/copy.rs::sidecar_for` already uses to decide whether a
sidecar is worth writing at all, so a sidecar that exists only for its
protection bits counts as losing neither. `copy_in_ffs` never increments
either field, because `FileMeta` genuinely keeps both. This is information
for the caller to report, not a refusal — nothing about G5's own PFS3 path
changed behaviour. `src/lib/preload.ts`'s `CopySummary` interface carries the
two fields too, so a future screen reading `PreloadResult` has them on the
wire already.
Proved by `core::preload::native::tests::copy_in_pfs3_counts_a_dropped_comment_and_a_dropped_date`
(a sidecar with both a real comment and a real date counts one of each),
`copy_in_pfs3_does_not_count_a_sidecar_with_no_comment_or_date_to_lose` (the
negative control: protection-only sidecar, nothing counted) and
`copy_in_ffs_never_counts_anything_lost` (the identical sidecar, on FFS,
counts nothing because FFS actually keeps it). Mutation-checked by hand:
unconditionally incrementing both counters (dropping the "is this worth
mentioning" gate) fails the negative control; temporarily adding the same
counting to `copy_in_ffs` fails `copy_in_ffs_never_counts_anything_lost` —
both reverted afterwards.

**ART-113** 🟠 ✅ **`libpfs3` 0.1.3 writes an entry's name as UTF-8 and reads it
back as Latin-1 — any non-ASCII AmigaDOS name fails to copy in** — *found
2026-08-16, Task 14's real run; refused by name 2026-08-16*
Vendored dependency (`libpfs3 = "=0.1.3"`), not ART code, but it blocked real
content on the user's own media. `writer.rs::create_dir_in`/`write_file_in`
encode the given name with `name.as_bytes()` — UTF-8 — while
`ondisk/direntry.rs`'s `DirEntry` decodes a stored name with
`util::latin1_to_string`. For any name outside ASCII (where UTF-8 and Latin-1
coincide byte-for-byte) the two disagree: a name like `español` writes as two
UTF-8 bytes for `ñ` (`0xC3 0xB1`) and reads back mis-decoded, so
`NativeFormatter::copy_in`'s own "was created and is not listed back" sanity
check (`native.rs:742` at the time, `.find(|e| e.name.eq_ignore_ascii_case(name))`)
fired and the whole copy aborted loudly — never silent corruption, but a hard
stop that named nothing the user could act on. Found via the real `dist-3.2`
tree: 24 **directories** across `Locale/Catalogs`, `Locale/Countries`,
`Locale/Help` and `Locale/Languages` carry accented AmigaDOS names —
`español`, `français`, `português`, `türkçe`, `österreich`,
`canada_français` — real content from `Locale-ES/-FR/-PT/-TR` and the base
`Locale` disk. Excluding those 24 directories to get a real card built at all
removed **969 of 4030 files and 106 of 330 directories — about a quarter of
the whole tree** (Task 14's own measurement; not reproducible by a committed
script, so recorded here rather than left silently unreproducible). `libpfs3`
is pinned and exposes no name-encoding option — a Rust `String` cannot carry
pre-encoded Latin-1 bytes through its public API — so this can only ever be
fixed upstream; what ART can do, and now does, is refuse by name before the
bad byte pattern ever reaches the crate.
→ `core::preload::native::non_ascii_entries` walks the already-flattened
`entries` list — directories included, not only files, since a directory's
own name goes through the identical `name.as_bytes()` write path — and
`copy_in_pfs3` checks it **before `FileRegionMut::open`**, so a bad name is
refused before the volume is even opened, let alone written to.
`CoreError::NonAsciiPfs3Names { paths, more }` names up to
`MAX_NAMED_NON_ASCII` (20) offending paths and folds the rest into `more`,
with a message that says which crate and which two encodings disagree and
names FFS as the way out — `core/volume/write` encodes these names
correctly, so this check runs on the PFS3 branch only. Superseded but not
replaced: the old "was created and is not listed back" sanity check at
`native.rs:742` stays as the safety net for whatever else might trip it.
Proved by `core::preload::native::tests::a_non_ascii_pfs3_file_name_is_refused_before_anything_is_written`
(byte-for-byte image comparison, not just the error's type),
`a_non_ascii_pfs3_directory_name_is_refused_even_when_its_contents_are_ascii`
(the shape that actually mattered: a bad directory name with pure-ASCII
contents), `more_offending_names_than_the_bound_are_folded_into_a_count` (25
offending names → 20 named + `more: 5`), `the_same_non_ascii_name_copies_in_fine_on_ffs`
(the same tree, FFS, succeeds) and two message-format tests in
`core::error::tests` pinning the sentence itself (every bounded path named,
the true total, "and N more", and "FFS" as the actionable advice). Mutation-
checked by hand: filtering only files (not directories) out of
`non_ascii_entries` fails the directory test; unconditionally incrementing
the bound by one off fails the "more" count test; running the same check on
`copy_in_ffs` fails the FFS test — each reverted afterwards.

**ART-114** 🔵 ✅ **`hst-imager`'s `fs copy` extraction silently drops any entry
whose name matches a Windows/MS-DOS reserved device basename, and the oracle
that depends on it reported the drop as an unexplained shortfall** — *found
2026-08-16, Task 14's real run; oracle fixed 2026-08-16*
External tool, not ART code — recorded because it is the independent witness
Task 14's own Step 2 depends on. The real `Storage3.2.adf` and
`GlowIcons3.2.adf` each carry a DOSDriver definition named `AUX`
(`DOSDrivers/AUX`, `DOSDrivers/AUX.info` — a real Amiga serial-port device
name that happens to collide with Windows' reserved `AUX` device). ART wrote
both correctly: `distribution.json` records real, distinct SHA-256 hashes and
byte counts for both (`Storage/DOSDrivers/AUX`, 119 bytes;
`Storage/DOSDrivers/AUX.info`, 481 bytes), and directory enumeration
(`Get-ChildItem`, Python's `os.listdir`) confirms both exist on the NTFS
distribution tree with the right sizes. But `hst.imager.exe fs copy … -r`,
extracting a PFS3 volume ART built back out to an NTFS folder, produced no
error and silently omitted both files — found by hashing every extracted file
against its source: 3059 of 3061 matched byte-for-byte, and the two misses
were exactly `Storage\DOSDrivers\AUX` and `AUX.info`. Windows-specific
(`Test-Path`/`Get-Item` on the exact same path fail the same way outside
`hst-imager` entirely; plain `os.listdir`/`Get-ChildItem` enumeration does
not), and it only matters for an **extraction to an NTFS path** — nothing
inside PFS3 itself is Windows-shaped, so ART's own reader is not known to be
affected. Not `hst-imager`'s to fix here, and not ART's tool at all — but
`scripts/pfs3-oracle-check.py`'s own job is to make a wrong volume loud, and
presenting a known, explainable absence as "3059 of 3061 matched" makes a
reader investigate by hand exactly the way this entry's own discovery did,
which is part of the job of the bug the oracle exists to catch.
→ `pfs3-oracle-check.py` now recognises Windows/MS-DOS reserved device
basenames (`CON`, `PRN`, `AUX`, `NUL`, `COM1`-`COM9`, `LPT1`-`LPT9`, matched
case-insensitively on the part before the first `.`, so `AUX` and `AUX.info`
both collide, and checked on every `/`-separated path segment, not just the
last). `check_art_writes_hst_reads` now returns `(checks, skipped)`:
a file whose path collides is named in `skipped`, `main()` prints it in its
own section with the reason, and it counts toward neither the pass list nor
the failure list — the exit status is unaffected by a Windows-reserved name.
Anything else missing from extraction still fails exactly as before.
Verified by running the real oracle
(`python scripts/pfs3-oracle-check.py`, `ART_HST_IMAGER` at
`E:\amiga\Amigatolon\hstimager\hst.imager.exe`) end to end: it still reports
"ART and hst-imager agree, both directions" (the synthetic fixture
`build_pfs3_volume_for_oracle_when_asked` carries no reserved name, so the new
skip path was not exercised by that run) and separately confirmed by hand —
`is_windows_reserved_component`/`path_has_reserved_component` invoked directly
against `AUX`, `AUX.info`, `aux`, `com3.txt` (True), `COM10`, `AUX2` (False,
correctly not reserved), and `Storage/DOSDrivers/AUX` (True, the real
colliding path). Extending the Rust-side synthetic fixture to include a
reserved name so the oracle's own run exercises the skip path was not done —
that fixture lives in `core/preload/native.rs`, out of scope for this fix
because another session is actively working in that file.

**ART-112** 🟠 ✅ **`glowicons` did not declare an override over `classes`, so a real
card refused to build** — *found and closed 2026-08-16, Task 14's run against
the user's own 3.2 media*
`src-tauri/src/core/osinstall/recipes/amigaos-3.2.json` · Both `Classes3.2.adf`
and `GlowIcons3.2.adf` ship `Devs/DataTypes/{8SVX,ACBM,AIFF,ANIM,BMP,CDXL,GIF,
ILBM,JPEG,PNG,WAVE}.info` — the DataType icons GlowIcons exists to re-skin —
and `workbench-base` ships three of the same names itself
(`AmigaGuide/FTXT/ILBM.info`). `glowicons`'s `overrides` list named
`workbench-base`, `extras` and `storage`, but not `classes`, so `plan()`'s
own `detect_collisions` (Task 5) correctly found no single winner and refused
the whole plan with eleven `DestinationCollision`s — every synthetic fixture
in Tasks 1–13 used ASCII-only single-disk data and never exercised two real
disks claiming the same nested file inside two overlapping `Subtree` rules.
→ `"classes"` added to `glowicons`'s `overrides`. Not a behaviour change for
any content already tested — the fixture recipes never modelled this overlap
— and correct in direction: GlowIcons is the icon-theme disk, so it wins.
Reproduced (and now passes) by
`run_the_real_engine_against_the_users_own_media_when_asked`
(`core/osinstall/apply.rs`, `#[ignore]`, `ART_OSINSTALL_MEDIA`/`_ROM`/`_DEST`
set) — `refusals` is empty and `classes`/`glowicons` are both in
`components_on`.

**ART-111** 🟡 ✅ **The `storage` component's rules named a `Storage/` drawer the
real disk does not have** — *found and closed 2026-08-16, Task 14's run
against the user's own 3.2 media*
`src-tauri/src/core/osinstall/recipes/amigaos-3.2.json` · Six `Subtree` rules
read `from: "Storage/DOSDrivers"`, `"Storage/Keymaps"`, `"Storage/Monitors"`,
`"Storage/Presets"`, `"Storage/DefIcons"`, `"Storage/Env-Archive"` — but the
real `Storage3.2.adf` carries `DOSDrivers/`, `Keymaps/`, `Monitors/`,
`Presets/`, `DefIcons/` and `Env-Archive/` at its **root**, with no `Storage/`
wrapper at all (confirmed by walking the real disk with `AdfSource::open` +
`MediaSource::walk("")`, the same way ART-013's era found `MMULibs.adf`'s
`Libs` casing). Every synthetic fixture that exercised `storage` built its own
tree from the recipe's own (wrong) assumption, so nothing caught the mismatch
before real media did — `plan()` correctly answered six
`MediaPathMissing` refusals, one per rule, rather than installing nothing
silently.
→ The six `from` values now name the real root-level paths; `to` is
unchanged (`Storage/DOSDrivers`, …), which is where AmigaOS expects them on
the installed volume. **Left deliberately incomplete, and said so here rather
than guessed at**: `Storage3.2.adf`'s real content also includes `Printers/`,
`LIBS/` (five libraries) and `Classes/DataTypes/{icon,jpeg}.datatype`, none of
which any `storage` rule reaches. Adding them was out of this fix's scope —
`Classes/DataTypes/*.datatype` already exists on the `classes` disk too, an
unmeasured third collision surface this session did not verify — so the
`storage` component ships today exactly as complete as it was before this fix,
minus the one bug that made it unusable at all.
Reproduced (and now passes) the same way as ART-112, above.

**ART-099** 🔵 ✅ **Application Size cut the right-hand edge off every screen** — *closed 2026-08-14: the real window was measured and nothing is clipped. The third wrong diagnosis was mine, and it lasted an hour.*

**2026-08-14, morning: a fourth wrong diagnosis, recorded because the pattern
is the point.** I screenshotted the running window, read text as cut off at
the right edge and a dead strip on the left, and committed an entry saying the
real window "does clip". It does not. The capture was made by a
**DPI-unaware** process on a 3840×2160 display at 150 % scaling, so
`CopyFromScreen` returned the left **two thirds** of the window and nothing
said so. The "cut" text was the edge of my picture.

**2026-08-14, afternoon: measured, in the real window, by the window.** An
overlay printed its own rects into the page:

```
inner 2560x1370  dpr 1.5   --app-zoom "1.3"   shell zoom "1.3"
shell   L0   R2560 W2560 H1370 | client 1969x1054 scroll 1969x1054
main    L291 R2560 W2269 H1370 | client 1745x1054 scroll 1745x1054
content L291 R2560 W2269 H1308 | client 1733x1006 scroll 1733x1182
kid     L651 R2185 W1534 H1454 | client 1180x1118 scroll 1180x1118
```

`scroll == client` horizontally at every level: **nothing overflows.**
`.app-shell` is exactly one window wide, which is what the CSS comment claims
and what `scripts/zoom-check.py` measured all along. The left-hand strip is
`.app-content > *`'s `max-width: 1180px; margin-inline: auto` — the design,
doing its job, with an equal strip on the right that my truncated capture
never showed.

**So the entry closes with the shell exonerated.** Its one real defect —
`overflow-x: hidden` making anything genuinely too wide unreachable — was
fixed on 2026-08-13 and stands. What is worth keeping is the method, now
proved four times over: **a screenshot is not a measurement.** Three attempts
were reasoned from pictures and all three were wrong; both times the answer
came in minutes once something was asked to report its own numbers. The tool
that does it lives on in `scripts/zoom-check.py`, and the lesson has a second
half now — *check the instrument before believing the picture it produces*.

The text below is what was written while the diagnosis was still open.
`src/components/layout/layout.css` · The Application Size feature applies CSS
`zoom` to `.app-shell`, and `zoom` does not scale viewport units — so the shell
divides first: `height: calc(100vh / var(--app-zoom))`, rendered back to exactly
one viewport by the zoom.

**The width was never given the same treatment.** A block-level shell takes its
parent's width, and `zoom` then renders it `z` times wider than the window. At
100 % nothing showed. At the 130 % this machine was actually set to, every
screen lost its right-hand edge: the Settings cards' input fields and the
Operation Log's rows ran off the side of the window with no scrollbar to reach
them, because `.app-shell` is `overflow: hidden` by design.

Found by screenshotting the running application to check something else — the
same way [ART-082](#fixed) was, and for the same reason: the tests were all
green, and no test looks at a window.

**Two attempted fixes, both wrong, and the second one shipped for an hour.**

- `width: calc(100% / z)` left the shell at `1/z` of the window — a dead strip
  down the right-hand side with the scrollbar stranded in the middle of the
  glass. That is what the user saw and reported.
- `width: calc(100vw / z)`, by symmetry with the height, did not correct the
  overflow either.

Both are reverted; the width rule is gone and the shell is back to the
behaviour it had before this entry was opened.

What went wrong in the diagnosis is worth keeping: the first screenshot showed
content running off the right edge, and I read that as a CSS bug without first
establishing that the window was on the screen at all. It was not —
`FindWindow` never located it, and the geometry only came out later
(2575×1407 at −7,−7, i.e. maximised). Editing one line and taking another
screenshot is not a reproduction, and three rounds of it produced two
regressions and no fix.

**Measured at last, 2026-08-13** — `scripts/zoom-check.py`, which drives the
running application in headless Chrome and reads the numbers out of the page.
Seven screens, three sizes, in a window the size of the user's own:

```
#/settings  z=1    window=2538  shell=2538  client=2299  scroll=2299  over=0
#/settings  z=1.3  window=2538  shell=2538  client=1717  scroll=1717  over=0
#/settings  z=2    window=2538  shell=2538  client=1038  scroll=1038  over=0
```

Two things follow, and the first one **disproves this entry's own diagnosis**:

- **`.app-shell` renders exactly one window wide at every size.** It is not
  drawn `z` times too wide and never was. `width: auto` resolves against
  `#root` in the parent's coordinate space, and the zoom applies to both alike;
  only `100vh` needed dividing, because viewport units are real pixels no
  coordinate space touches. That is why `calc(100% / z)` produced a shell at
  `1/z` with a dead strip, and why `calc(100vw / z)` changed nothing: both were
  corrections to something that was not happening.
- **Nothing on any of the seven screens overflows its column at 130 %**, at
  2538 px or at 1258. `scroll == client` everywhere. The reported symptom does
  not reproduce on the code as it stands — which, given that the first
  screenshot may not have been of the running window at all, is the likeliest
  reading of how it arose.

**What was real, and is fixed:** `.app-content` carried `overflow-x: hidden`.
Zoom buys size by spending width — the column measures 2299 CSS px at 100 %,
1717 at 130 % and 1038 at 200 % — so anything that *did* exceed it would be
clipped with **no way for the user to reach it**. Not merely invisible: in the
reproduction the box could still be scrolled by script (`scrollLeft` moved to
396) while offering no scrollbar, no wheel and no drag. That is precisely the
shape of the original report, and it is one character to fix. `.app-content`
scrolls sideways now when it has to, and `.scroll-x` still carries the cases
that are *meant* to (a hex dump row, a block table).

**Left open on purpose**, and only for this: everything above was measured in
Chrome against the dev server, and the application ships in **WebView2** with
real data — long paths, real log rows — that a dev server cannot produce. The
one check that remains is the user's own window at 130 %. If it is clean, this
closes; if it is not, `scripts/zoom-check.py` is where the numbers come from,
not another screenshot.

### SD-1 · G2 (2026-08-13/14)

**ART-103** 🟠 **ART wrote `kernel=Emu68.img` over the release's own line, and the card would not boot** — *fixed 2026-08-14*
`core/pistorm/firmware.rs` · `core/card/payload.rs` · `merge_config_txt` had
`kernel` among the keys it manages and wrote `KERNEL_IMAGE` — the constant
`"Emu68.img"` — into every `config.txt` it touched. The real
`Emu68-pistorm.zip` carries **`Emu68-pistorm.gz`** and its own `config.txt`
says `kernel=Emu68-pistorm.gz`; the Raspberry Pi's firmware decompresses a
gzipped kernel itself, which is why the release ships one and points straight
at it. No release has ever contained a file called `Emu68.img`, and the real
CaffeineOS card keeps three kernels in a `KERNEL/` folder with no extension at
all.

So a card ART built carried the right kernel under its right name and a
`config.txt` telling the firmware to load a file that was not there. **It would
not have booted**, and the failure would have appeared on the Amiga rather than
anywhere ART could see it.

Found by driving the real archive through `emu68_payload` and reading the
`config.txt` off the card that came out — the same way ART-090 and ART-091 were
found, which is to say by looking at what the real thing says rather than at
what ART believes.

→ The kernel's name is a **field**, not a constant: `FirmwareConfig.kernel_file`,
set by `emu68_payload` to what the archive's own `config.txt` names, and
verified to exist among the files being placed — an archive whose config points
at a kernel it does not carry is refused before a card is written rather than
after it fails to boot. `parse_config_txt` reads the line back, so the round
trip is honest. `KERNEL_IMAGE` survives as what the *reading* side looks for on
a card it did not build, and says in its own doc comment that it is not what a
release ships.

**ART-102** 🟡 **`fatfs` writes two things wrong in every directory it creates** — *worked around 2026-08-14*
`core/fat32.rs::repair_directories` · Found by pointing 7-Zip at a boot
partition ART had just written: any image with a folder in it came back
`Headers Error`. Isolated by editing a copy of the image by hand and watching
the complaint go away — the size was not the cause and neither was the payload;
one directory was enough.

The crate (`fatfs` 0.3.6) gets two things wrong in every directory:

1. **`.` and `..` are given long-filename entries.** They are 8.3 names by
   definition and must never carry one. This is what 7-Zip reports.
2. **`..` points at the root's own cluster** — 2 — where the format says a
   directory whose parent is the root writes **0**. 7-Zip does not check this
   one; the format is still the format.

Neither stops `fatfs` reading its own output, which is the exact shape of
ART-032..035: a writer and a reader that agree with each other and with nothing
else. This one matters because the partition is read by the **Raspberry Pi's
firmware**, which nobody here can interrogate, and because the Emu68 payload
lives partly in `overlays/` — a folder the firmware does read.

→ `repair_directories` walks the finished filesystem and fixes both: the
spurious long-filename entries are marked deleted (`0xE5`, which is what FAT
has always meant by an ignored slot — and is exactly the edit 7-Zip accepted
when it was made by hand), and `..` is set to 0 where the parent is the root.
`a_directory_is_written_the_way_the_format_says` checks the bytes rather than
asking a reader that would share the defect, and the oracle
(`scripts/fat-oracle-check.py`) now writes a folder for the same reason: without
one it would have gone on passing while the card carried the fault.

**Revisit when `fatfs` is upgraded.** If a later release writes directories
correctly, `repair_directories` becomes dead weight — and the way to find out is
to delete it and run the oracle.

### The open-list sweep (2026-08-13)

**ART-043** 🟠 **A partition inside a small image was written at the wrong offset** — *fixed 2026-08-13*
`commands/volume_write.rs` · The whole-file strategy is chosen by the *file's*
size — read it whole, mutate in memory, validate, one atomic write — and that
part was right. What was wrong was what it handed the writer: the whole file,
opened at offset `0`, while the geometry it was given described a **partition**
that may start megabytes in. For any RDB image of 16 MiB or less, volume-relative
block numbers were read and written as if they were file-absolute, so the root
block landed in the middle of the partition's data. Above the limit the
block-journal strategy takes over, and it has always opened the volume at its
own offset — the bug lived only in the small case, and no fixture in the suite
was a small RDB image with a formatted partition, which is why it survived.

**Nothing was ever at risk, and that is now measured rather than assumed.** The
gate ran `validate_image` over the whole file, which stops at the signature:
`RDSK`, not `DOS`. So a small RDB image could not be committed at all — this
was a strategy that could not succeed, not one that could corrupt. The first
read usually failed earlier still, with "block N is not a directory", because
block 880 of the *file* is not that volume's root.

→ `WholeFileVolume`, one session type replacing the three copies of the
whole-file branch, so the fix cannot be applied to two of them and missed in
the third. It gives the writer the **volume's own bytes**, opens it at the
volume's offset, and on commit validates the volume and splices it back into
the file for one atomic guarded write. Everything around the partition — the
RDB, another partition, trailing bytes — survives byte-for-byte, and for a bare
ADF (offset 0, length the whole file) the slice is the file and nothing changed.
The gate is now `validate_volume` and is asked about the volume, which is the
thing that has to be one.

Two tests, and the fixture the entry said nothing constructed:
`a_partition_inside_a_small_image_is_written_where_it_lives` writes into a
12 MB RDB image with a formatted 4 MB partition, reads the file back through
the volume's own geometry, and asserts every byte **before** the partition is
unchanged — the RDB is in those bytes, and a write that addressed the file
would have gone straight through it. `the_gate_asks_about_the_volume_not_the_file`
pins both halves of the validation question. Mutation-checked: putting the
session back on the whole file at offset zero fails the first outright.

**Not the whole of the "one fix" this was paired with.** Writing an *RDB* at an
offset — SD-1's G4 on a card, where each `0x76` area carries its own table
inside the card — is the other half, and it has no caller yet: G2 builds the
card. What is fixed here is writing *into* a volume that starts at an offset,
which is what exists today.


Six of the open entries closed in two passes, chosen because none of them
needed a decision first — the ones that do are still open and say so.

**ART-066** 🟡 **`archives_plan_install` unpacked the whole batch on the Tauri command thread** — *fixed 2026-08-13*
`commands/archives.rs` · `src/lib/archives.ts` · `src/pages/FileManager.tsx` ·
Planning a batch of archives is not the cheap arithmetic every other plan in
ART is: it has to **unpack every archive** to know what each one contains. It
did that straight in the command handler, so a plan over several large archives
froze the window with no progress and no way to stop it — in the one step whose
whole purpose is to let the user change their mind before anything is written.
Every other multi-step operation in the module already ran through `spawn_job`.

→ It is a job now, returning a job id; the plan arrives on a new
`archives-plan-result` event carrying its `job_id`. `FileManager` keeps the
sources, names and destination side in `pendingPlan` until it lands, and
`busy` stays set until the plan arrives *or* the job ends some other way — the
job-progress listener answers a cancelled or failed plan, which is the case the
old code could not have at all. The pending state is set **before** the call,
because a small batch can finish before `invoke` resolves with the id; only one
plan is ever in flight, so a result arriving early is still unambiguous.
`planning_answers_stop` covers the engine half. `NoProgress` is now a test-only
import in that module — every caller in the application runs on a real sink.

**ART-058** 🔵 **A cancelled block-journal copy did not tell the user files had already landed** — *fixed 2026-08-13*
`core/error.rs` · `core/jobs/mod.rs` · `commands/volume_write.rs` ·
`src/lib/jobs.ts` · Above the whole-file limit (16 MiB) each file a copy or
install writes is its own committed, journalled, verified operation, durable
before the next one starts, and cancelling correctly leaves them in place. Both
strategies came back as plain `Cancelled`, so somebody who stopped a large HDF
install part way through had no way to learn that some of it is on the volume —
while the whole-file strategy really does leave nothing at all.

→ `CoreError::CancelledPartway { files }` (`ART-CANCELLED-PARTWAY`), mapped by
the job runner to `JobState::Cancelled { files_landed: Some(n) }` — still a
cancellation, never a failure: the job bar must not go red for something the
user asked for. The count travels as a **number**, not inside the sentence,
because the sentence is English and the UI's is not (§68); `jobStatusLabel`
picks `cancelledPartway` and passes it as `count`, which is the half that makes
i18next actually pluralise (the ART-061 lesson). Zero landed is still plain
`Cancelled`. Four tests: both strategies end-to-end in Rust — the large one
asserts the count matches what is actually in the volume's listing, the small
one that the image is byte-for-byte unchanged — and the label's two branches in
`jobs.test.ts`.

**ART-070** 🔵 **`refresh(side)` moved keyboard focus to the pane it refreshed** — *fixed 2026-08-13*
`src/pages/FileManager.tsx` · `openLocal`, `openAdf`, `openHdf` and
`openVolume` each end with `setFocused(side)`, and `refresh(side)` calls
whichever matches the pane's kind. F5's copy path refreshes the **destination**
once the job result arrives, so focus jumped silently out of the pane the user
was working in and the next F-key press landed on the pane they were not
looking at. Total Commander leaves focus on the source.

→ `refresh` reads the focused side from a ref before re-opening and puts the
keyboard back afterwards. A ref rather than a dependency: taking `focused` as
one would rebuild every callback downstream of it on each Tab. Fixed once in
`refresh` rather than teaching six `open*` functions a flag — *opening* is the
user's instruction and should move focus; *refreshing* is ART's own and should
not. `FileManagerFocus.test.tsx` pins all three cases, the last of them by
running the harness with the old behaviour and asserting the bug.

**ART-068** 🔵 **The filter box told "empty" from "no match" by comparing entry counts** — *fixed 2026-08-13*
`src/lib/mask.ts` · `src/pages/FileManager.tsx` · The message picked between
`files.pane.filterNoMatch` and `files.pane.empty` with `filter.trim() !== "" &&
state.entries.length > 0` — correct only because `filterEntries` never changes
the unfiltered count, which is a property of *that* module the call site had no
way to depend on. Showing "this folder is empty" for a folder that only looks
empty because of a mask reads as ART having failed to open the disk.

→ `filterEntriesReporting` returns `{ entries, hidEverything }`, answered by the
code that did the removing, from the list it removed from; `filterEntries` stays
as a wrapper for the callers that do not need the reason. Five cases in
`mask.test.ts`, including the one the two counts could not tell apart: an empty
folder with an active mask is still an empty folder.

**ART-067** 🔵 **A batch archive install could not be stopped mid-archive** — *fixed 2026-08-13*
`commands/archives.rs::prepare_archives` · Unpacking used `&NoProgress`, so
`is_cancelled()` answered `false` all the way down and Stop was honoured
*between* archives but not during one: a batch of five whose third is large
left Stop unresponsive for the whole of that extraction.

→ A `BatchStep` sink forwards the real cancel flag and keeps the batch's own
counts. Forwarding the sink raw would have let the extractor's per-entry counts
(142 of 2000 files) overwrite the per-archive ones (3 of 5), so the bar would
leap inside an archive and fall back at each boundary; the message keeps its
`"Unpacking …"` prefix because
`cancelling_during_the_copy_phase_writes_nothing` tells the phases apart by it.
§54 is not weakened: what a cancelled unpack leaves half-finished is a scratch
directory ART owns and drops, and nothing reaches the volume until every archive
is staged. `stop_is_heard_inside_an_archive_not_only_between_them` uses **one**
archive on purpose — with no second iteration to reach, the only way it can come
back `Cancelled` is if the cancellation travelled into the unpack.
Mutation-checked against `&NoProgress`.

**ART-049** 🟡 **`create.rs`'s oracle hook and `VolumeWriter::open` agreed by hand, not by a check** — *fixed 2026-08-13*
`core/volume/write/mod.rs::VolumeWriter::open` · The oracle export hook formats
with `FileSystemType::Ffs` and then names `DOS\x01` geometry by hand, and the
two agreed only because whoever wrote it chose them to. Nothing at the writer's
boundary cross-checked the geometry it was handed against the dostype the
image's own bootblock declares — and the flavour byte decides whether data
blocks carry OFS's 24-byte header and whether names hash in international mode,
so a writer told the wrong one produces a disk that is internally coherent and
unreadable to the machine it was made for.

→ `open` reads block 0 and refuses (`ART-SAFETY-REFUSED`) when the bootblock
carries a **real** DOS signature that differs from the geometry's. A blank
bootblock is deliberately not a contradiction: an unformatted volume is a
different complaint with its own failure further in, and refusing it here would
swap a confusing error for a confusing refusal. Two tests, one for each half;
the refusal also proves the image is left byte-for-byte unchanged. No existing
caller was refused, which is the evidence that everything already agreed.

### Phase 2b, the PiStorm rebuild and the first real cards (2026-08-12 → 08-13)

**ART-085** 🟡 **A studio forgot the image it had open the moment you left the screen** — *fixed 2026-08-13*
`src/stores/openObjectStore.ts` · `AdfBrowser.tsx`, `HardDiskStudio.tsx`,
`LhaBrowser.tsx`, `HexTools.tsx`, `WhdloadInstall.tsx`, `WinuaeStudio.tsx` ·
Each held its open file in a local `useState`. Navigating away unmounted the
component and the state went with it, so coming back gave the empty
"open an .adf to begin" page again — while the Dashboard's Recent list, which
*is* persisted (SQLite `recent_files`), still showed the file that was open a
second ago. That contrast is what made it read as a fault rather than as a
design.

Found by the user in the running application, 2026-08-12.

→ `useOpenObject(kind)`, a drop-in for the `useState<string | null>(null)` each
studio held, backed by one small Zustand store. Four decisions in it are worth
keeping:

- **Session-scoped, and that was the user's call** (asked, 2026-08-13). It is
  not `@/lib/remembered` and never reaches `settings.json`: closing ART forgets
  what was open. A path that outlives the run can name a file since deleted,
  moved, or unplugged with the drive it was on, and answering for that is a
  bigger design than this issue asked for. The choices a studio *makes* — view
  mode, filesystem, folder — are still `useRemembered`'s and still survive a
  restart.
- **Per kind, not one global object.** Nine slots, because opening an ADF must
  not change what the Hard Disk studio is looking at, and WinUAE's attached
  media are not the same thing as the image a studio is inspecting.
- **Only the path is held; nothing parsed.** A studio re-reads its file on the
  way back in, so a file that changed on disk meanwhile cannot come back stale.
  `info === null` (or `chunk`, or `volumes`) is what "nothing is loaded *here*"
  looks like on a fresh mount, and that is what the effect tests.
- **Router state still wins.** Arriving from the drop panel or the Dashboard
  with a file opens that file and makes it the open one — the whole point of
  arriving that way.

Two things the fix deliberately does not do, named rather than smuggled:
`HexTools` comes back at the start of the file rather than where you were
reading, and `WhdloadInstall` restores the image but re-picks the partition
(automatically, when there is only one usable one).

Also fixed on the way past: `loadDisk` in ADF Studio reset the hex panel every
time it ran, which on a *reopen* would have switched off a remembered choice
the user had made — a setting changing without the user changing it. It now
resets only when a different disk is opened.

Tests: `openObjectStore.test.ts` (per-kind independence, close, replace) and
`openObjectSurvivesNavigation.test.tsx`, which mounts a harness wired the way
the studios are, unmounts it — that *is* navigating away — and mounts it again.
Mutation-checked: putting the harness back on `useState` fails two of its five
cases, including the one this issue is about.

**ART-096** 🟡 **ART wrote `MaxTransfer` and `Mask` as zero, and 100 buffers** — *fixed 2026-08-13*
`core/rdb.rs::create_rdb_layout` · `src/pages/HardDiskStudio.tsx` · The
DosEnvec's `MaxTransfer` (longword 45) and `Mask` (longword 46) were left at
zero, with a comment saying so. Every partition on both real cards — seventeen
of them across three RDBs, without exception — carries:

```
maxtransfer = 0x0001FE00    mask = 0x7FFFFFFE    buffers = 600
```

A mask of zero says no memory is acceptable for a transfer, which is not what
anybody means by it; ART's default of 100 buffers against the field's 600 was a
performance choice made by not making one. Neither is dangerous — Emu68 is
forgiving — but they are the fields that are cheap to get right and very hard
to diagnose when wrong, and ART now has measured values rather than a guess.

Found 2026-08-13 while reading two real cards. See
[sd2-card-layout.md](sd2-card-layout.md).

→ `core/rdb.rs` writes both fields and defaults to `DEFAULT_NUM_BUFFERS` (600);
`ParsedPartition` reads them back. Closed 2026-08-13 with the two halves that
were still owed:

- **A test of its own.** `dosenv_offsets_match_the_amiga_layout` pins the
  offsets, which is not the same as pinning the intent, and it reads bytes
  rather than the round trip. `a_created_partition_carries_the_measured_dosenv_values`
  creates an image, parses it back and asserts all three values off the parsed
  partition; `an_explicit_buffer_count_survives_the_round_trip` proves the
  default is not a ceiling. Mutation-checked both ways — restoring either old
  value (`Mask` to zero, buffers to 100) fails them.
- **The UI stopped naming a number it never asked for.** `HardDiskStudio.tsx`
  hard-coded `num_buffers: 100` in three places, so the new default reached
  nothing a user created — the core's measured value was silently outvoted by
  a literal in a component. The three are gone; `PartitionSpec.num_buffers` is
  `#[serde(default)]` on the Rust side and optional in `src/lib/hdf.ts`, so
  absent and zero both mean "the core decides".
  `a_spec_without_a_buffer_count_deserialises_to_the_default` covers the wire
  format, which is the join the type system does not.

**`SDH0` naming is deliberately not part of this.** It is a card convention,
and an HDF that WinUAE mounts wants `DH0`; it belongs to SD-1, where the card
is built.

**ART-100** 🟡 **The PiStorm screen went grey without saying why** — *fixed 2026-08-13*
`src/pages/PistormStudio.tsx` · Every control on that screen edits files on a
card, so every one of them is disabled until a card folder is chosen — the ROM
picker, the preview, the save. They said so **only by being grey**, which reads
as a broken screen rather than as a first step. The user's words on finding it:
the button is there but it does not work.

Fixed by saying it, once where the folder is chosen and again on each disabled
button's tooltip. The prerequisite was always right; it was simply never
spoken.

Worth generalising: a disabled control with no explanation is a defect of the
same kind as a control that does nothing (ART-090), and the screen had four of
them.

**ART-098** 🟠 **CI's licence gate could never pass, and the build and the installer never ran** — *fixed 2026-08-13*
`.github/workflows/ci.yml` · The `Dependency licences & advisories` step used
`EmbarkStudios/cargo-deny-action@v2`. That is a **container** action, container
actions only run on Linux runners, and this job runs on `windows-latest`. So the
step failed with

```text
##[error]Container action is only supported on Linux
```

on **every push since it was added** — and because it sits before them, the
`Build application` and `Upload MSI artifact` steps never ran either. Three red
runs in a row on `main` before anybody looked, and the reason was never in the
code being pushed.

Two things worth naming beyond the fix:

- **`docs/licenses.md` claims cargo-deny runs on every push.** It has not. The
  check itself is sound — it passes locally, and did today — but the claim was
  not true, which is the same class of thing as ART-084 and ART-090 turned
  inward.
- **The frontend tests were never in CI at all.** `pnpm lint` type-checks and
  stops there; the four hundred tests, including the en/tr parity check and the
  two that prove every `Phrase` and every registry key resolves to a real
  catalogue leaf, ran only on a developer's machine. A missing i18n key renders
  a raw dotted string on screen and nothing in CI would have seen it.

Fixed by installing `cargo-deny` and running the binary, and by adding a
`Frontend tests` step. Found while pushing 28 commits of `main` that had never
reached the remote.

**ART-097** 🟡 **A card may carry several RDBs, and ART models one — so it would report fifteen working partitions as broken** — *fixed 2026-08-13*
`core/rdb.rs` · `@/lib/rdbDrivers` · MultibootOS 2.2 has **two** `0x76` areas,
each with its own `RDSK`, its own geometry and its own partition list. ART
returns `Option<usize>` from `find_rdb_location` and stops at the first.

The consequence is worse than a missing half. That card's **second** RDB carries
a `DOS` driver at version 45.16 and **no PFS3** — while all fifteen of its
partitions are `PDS`. The card works, because the drivers a partition may use
are the **union across every RDB on the disk**. `partitionsMissingDriver` looks
at one RDB, so on this card it would name fifteen partitions as unmountable when
none of them is — the same false confidence in reverse that ART-084 was.

Fix: model a card as a list of Amiga areas, and take the driver set as the union
before deciding anything is missing. Depends on [ART-095](#open).

**Fixed 2026-08-13** with it. `CardImage::partitions_missing_driver` asks the
whole card, and on the real MultibootOS image reports **nothing** where the
per-area question would have named fifteen. The guard against over-correcting is
`a_partition_with_no_driver_anywhere_is_still_reported`: the union must not turn
ART-084's real finding into silence.

Still owed: the Hard Disk studio and `@/lib/rdbDrivers` ask the old question
against a single RDB. They keep working on HDFs, which is what they are pointed
at today; moving them onto `CardImage` is the follow-up that makes a card
openable from the UI rather than only from the core.

Found 2026-08-13. See [sd2-card-layout.md](sd2-card-layout.md).

**ART-095** 🟠 **ART cannot open a real PiStorm card image at all** — *fixed 2026-08-13*
`core/rdb.rs::find_rdb_location` · `core/hdf.rs` · `find_rdb_location` scans the
first **16 blocks of the file** for `RDSK`. On every real PiStorm card those
blocks are the MBR and the beginning of the FAT32 boot partition; the Amiga's
RDB sits about 1.1 GB in, at the start of a partition of type `0x76`.

Measured on two real distributions (2026-08-13, headers only, read-only):

| | CaffeineOS 9317 | MultibootOS 2.2 |
|---|---|---|
| FAT32 | 1.10 GiB at byte 1 048 576 | the same |
| RDB | byte **1 178 599 424** | byte **1 178 599 424** and byte **50 570 723 328** |

So ART finds no RDB on either, and the Hard Disk studio, the Files screen and
the workflow engine all see a file they cannot open. Nothing is at risk — ART
reads and refuses — but this is the single thing standing between ART and the
cards it exists to build.

**ART has no MBR awareness anywhere.** That is the fix: parse the partition
table, treat each `0x76` area as an Amiga disk with its own base offset, and
give the RDB reader that base. `core/volume` already works through a
`BlockDevice` at an offset, so the pieces below this are in place.

Found by reading the two images the user supplied — the inspection
`ART-research-distro-profiles.md` §8.2 parked SD-2a on. See
[sd2-card-layout.md](sd2-card-layout.md).

**Fixed 2026-08-13.** `core/mbr.rs` reads the partition table — four primary
entries and deliberately nothing else, since a PiStorm card has never needed
extended partitions or GPT and a parser with untested branches is worse than a
narrow one. `core/card/mod.rs` turns a file into the Amiga disks in it, and answers
for a plain HDF too: no MBR means one disk at offset zero, so callers do not
branch on what kind of file they were handed.

**Verified against both real cards**, which is the point of having them:

```
CaffeineOS 9317   1 area at 1178599424   SDH0, SDH1        PFS3 19.2
MultibootOS 2.2   2 areas                SDH0/SDH1 + 15    2 drivers
```

Every number agrees with an independent hand-decode of the same bytes, and with
what the distributions' own documentation says about themselves.

**ART-094** 🟡 **Overwriting a write-protected file is not checked either** — *fixed 2026-08-13*
`core/volume/write/mod.rs` · `commands/volume_write.rs::replace_file` · The
delete half of [ART-088](#open) is fixed: the writer honours the `d` bit and
refuses unless the user has been asked. The **overwrite** half is not.

AmigaDOS governs replacing a file's contents with the `w` bit, and a file with
it withheld is one the Amiga will not let you write to. ART's `replace_file`
deletes and re-adds — an implementation detail — and passes
`DeleteProtection::Override` precisely because that delete is not a deletion
the user asked for. Which is right, and leaves the real question unasked:
nothing anywhere checks `w` before overwriting.

The same shape as the fix that just landed: honour the bit by default, and let
a caller that has shown the question say so. The Files screen's copy dialog is
where that question already belongs — it asks about collisions there, and
"this one is write-protected" is the same conversation.

**Fixed the same day**, and it caught a side effect of the fix that created
it: ART-088 made `delete` honour the `d` bit, and the three overwrite paths all
reach `delete` — so copying over a delete-protected-but-writable file started
being refused for the wrong reason. `ensure_overwritable` is the right question
(`w`), asked before the replace begins; the delete underneath is
`DeleteProtection::Override`, because it is how ART performs a replace and not
a deletion the user asked for. Two bits, two guards, and
`a_delete_protected_file_may_still_be_overwritten` is the test that keeps them
apart.

All three paths honour it: the copy engine (a refusal becomes that entry's line
in `skipped`), `volume_put_file`, and check-in — where refusing is simply
correct, since putting an edit back into a file the Amiga would not let you
write to is what the bit exists to stop.

**The remaining half is the question itself.** `volume_put_file` takes an
override so the copy dialog can offer it, and nothing sends it yet: the refusal
names the remedies that exist today (clear the W bit in Attributes, or copy in
under another name) rather than promising a confirmation that is not there.
Adding it to the copy dialog, beside the collision question it already asks, is
the follow-up.

Split out of ART-088 on 2026-08-13 rather than left as a sentence inside it.

**ART-092** 🔵 **A named PiStorm firmware set cannot be deleted from ART** — *fixed 2026-08-13*
`core/pistorm/mod.rs` · `src/pages/PistormStudio.tsx` · Named sets can be
created, duplicated, renamed and activated. Deleting one is deliberately absent
from that list: removing a user's configuration is destructive, and destructive
actions in ART carry their own confirmation design (§92) rather than a bare
button. The screen says so rather than leaving a gap the user has to discover.

Not urgent — a set is a file on a card the user can already delete in Files, or
in Explorer. Worth doing properly when the confirm shape for "delete a thing
the user made" is settled, which is the same question a future *delete a ROM
from the card* will ask.

**Fixed 2026-08-13**, and the shape is the answer: the set is **backed up
before it goes**, so "deleted" means moved out of the way and recoverable, and
the screen says so rather than implying otherwise. Two things it refuses: a
name that would reach outside the card (`config_set_path` is the only spelling,
and there is none that produces the plain `config.txt`), and the set the card
is *currently running* — whose text matches `config.txt` byte for byte, so
deleting it would take away the only copy of the configuration it boots from.
Make another active first.

Tests: `a_set_is_deleted_and_kept`, `the_set_the_card_is_running_is_not_deletable`,
`deleting_cannot_reach_the_active_config_or_anything_outside_the_card`,
`deleting_a_set_that_is_not_there_says_so`.

Recorded 2026-08-13 as a deferral, not a defect.

**ART-091** 🟠 **ART named an Emu68 archive that has never existed, and the name that does exist means a different board in each release line** — *fixed 2026-08-13*
`core/pistorm/hardware.rs` · `PistormVariant::kernel_archive()` returned
`"Emu68-pistorm16.zip"` for the PiStorm16. **No Emu68 release has ever shipped a
file by that name.** It was invented to fill a cell in the table — in a module
whose own doc comment says "named rather than guessed" — and it survived the
ART-090 rebuild and its review.

Verified 2026-08-13 against `api.github.com/repos/michalsc/Emu68/releases` and
`pistorm.github.io/tutorials/sd_setup/`, the answer is worse than a wrong
filename:

| | 1.0.7 (latest stable) | 1.1.0-alpha.1 (prerelease) |
|---|---|---|
| PiStorm (classic) | `Emu68-pistorm.zip` | `Emu68-pistorm-classic.zip` |
| PiStorm600 | `Emu68-pistorm.zip` | not stated |
| PiStorm32-lite | `Emu68-pistorm32lite.zip` | `Emu68-pistorm.zip` |
| PiStorm16 | **no asset at all** | `Emu68-pistorm.zip` |

Two things follow, and neither can be said by a single string:

- **PiStorm16 has no stable release.** `v1.1.0-alpha.1` is the first Emu68 to
  support it — a GitHub prerelease. "Which zip" has no honest stable answer.
- **`Emu68-pistorm.zip` changes meaning between the lines.** In 1.0.x it is the
  classic board's firmware; in 1.1 alpha it is the PiStorm32-lite's and
  PiStorm16's. A user told "download Emu68-pistorm.zip", who then lands on the
  latest *stable* release — the obvious thing to do — flashes firmware for
  another board entirely.

Fixed by making the release line a field of its own and the answer a type
rather than a string: `KernelArchive::{Named, Absent, Unstated}`. `Absent` and
`Unstated` are both real cells in that table, and a plausible filename in
either is the slip this type exists to prevent. Three new notes carry it to the
screen — no stable release, the name means another board in the other line, and
the notes do not say. `no_archive_name_is_one_no_release_has_ever_contained`
pins the whole table against the three assets those releases actually ship.

Found in review of the ART-090 work, 2026-08-13.

**ART-090** 🟠 **The PiStorm screen offered controls Emu68 does not have, and wrote tokens it does not read** — *fixed 2026-08-12*
`core/pistorm.rs` · `src/pages/PistormStudio.tsx` · `src/i18n/*.json` · Four
things on that screen were not merely wrong, they were invented:

- **"Enable JIT Dynamic Recompiler."** Emu68 *is* a JIT engine. The official
  FAQ's words are "Emu68 is exclusively a Just-In-Time (JIT) engine" — it
  cannot be turned off short of powering the machine down. The real, adjacent
  option is `enable_cache` (JIT cache active from startup).
- **"Enable 68040 MMU Emulation — required for WHDLoad MuForce."** Emu68
  documents no MMU emulation at all; the MMU is famously not emulated and
  WHDLoad runs in NOMMU mode. This one actively misled: a user chasing a
  WHDLoad problem would have turned it on and believed something had changed.
- **A Fast RAM slider**, 1 GB by default. Emu68 maps RAM automatically. There
  is no size to set. The real memory knobs are `limit_2g`, `z2_ram_size`, the
  `enable_c*_slow` family and `move_slow_to_chip`.
- **`emu68-sd.device`**, named in the UI as the "high-speed MicroSD" driver.
  The real driver is `brcm-sdhc.device` on the Pi 3 family and
  `brcm-emmc.device` on the Pi 4 / CM4. The `emu68-` prefix belongs to the RTG
  card (`emu68-vc4.card`). This name reaches a user's mountlist, where a wrong
  one mounts nothing.

What was written to the card followed: `emu68.jit`, `emu68.mmu` and
`buptest.fastram_size` — three tokens Emu68 has never read — plus `sd.unit0=0`,
when that option takes `off`, `ro` or `rw`. Also `hdmi_cvt` and three
`framebuffer_*` keys in `config.txt`, for an "RTG resolution" Emu68 does not
take from there.

The profile cards claimed "99 % WHDLoad compatibility", "~800+ MIPS", "512 MB
Fast RAM" and "20+ MB/s" — none measured, none reproducible.

Spec §10 and §89 in the one place a user is most likely to trust ART, and worse
than the ADF equivalents: this screen's output goes onto a card that boots
somebody's real machine.

**Fixed 2026-08-12.** `core/pistorm/` is three modules with 58 tests:

- `hardware.rs` — the matrix the screen models. A setup is **three** choices,
  not one: Amiga → PiStorm board → Raspberry Pi, each filtering the next.
  Everything downstream derives from it — the Emu68 release archive, the
  storage device name in every generated hint, which tokens are meaningful, and
  the notes (CM4 eMMC, 3B/3B+ physical fit, power supply, a Pi that is reported
  working rather than guaranteed).
- `options.rs` — one field per documented `cmdline.txt` token and nothing else.
  `every_profile_is_made_of_real_tokens_only` is the test that keeps it that
  way. Slow-RAM tokens are **dropped**, not hidden, on an A600/A1200: the Emu68
  FAQ's own answer to "my A1200 reports the wrong RAM" is to remove them.
- `firmware.rs` — `config.txt`: kernel, `initramfs`, display presets with their
  real `hdmi_group`/`hdmi_mode` numbers shown on screen, and an overclock that
  is opt-in only and never part of a profile.

Both files are still merged, never regenerated, and
`saving_never_loses_the_parameter_the_pi_boots_by` pins the one that matters.
A card written by an older ART has the three fictional tokens removed on its
next save.

The screen prints the token beside every control and the whole `cmdline.txt`
line beneath them, plus the user's own boot parameters read-only — which is the
only way somebody can see for themselves that a control does what it says.

Source: `ART-brief-pistorm-studio-v2.md` (Emu68 Options and SD_Preparation
docs, pistorm.github.io hardware page and Emu68 FAQ, wiki.amiga.org
Pistorm-500 / Pistorm32-Lite, MultibootOS 2.2). Found by the user, 2026-08-12.

**Closed 2026-08-13 after the fix round** (`ART-prompt-pistorm-studio-fixes.md`),
which reviewed the above and closed what it left:

- **The Kickstart goes through ROM Manager.** Every ROM-shaped file on the card
  is identified by checksum and labelled with its version and the machines it is
  for; a ROM can be picked from anywhere on the PC, identified, and copied onto
  the card under a confirmed name. Unrecognised stays a label, never a refusal —
  a custom or byte-swapped image copies like any other. The picker is shown to
  *everyone*; Power Mode adds free typing rather than replacing it, which is the
  way round the first version had it.
- **The kernel states its version.** `$VER: Emu68 <version> <date> git:<hash>`,
  which Emu68's own build assembles in `cmake/verstring.cmake` and compiles in.
  A card whose image says nothing reports "unknown" rather than a guess.
- **Named firmware sets are managed, not just listed**: create, duplicate,
  rename and activate, each through preview → backup → write. Deleting one is
  [ART-092](#open), deliberately.
- ART-091 above, found in the same review.

Still owed and named as such: fetching a kernel update from GitHub is not built
(F4's second half), and the WiFi panel stays declared rather than offered until
G14 can edit the volumes it would write into.

**Not yet verified on real hardware.** No card built by this screen has been
booted. The tokens are what the documentation says; whether a given machine
likes a given set of them is a separate claim ART is not making.

**ART-089** 🟠 **Session restore could not work, and destroyed the session it was meant to restore** — *fixed 2026-08-12*
`src/pages/FileManager.tsx` · `src/App.tsx` · `App` starts the settings store
loading and deliberately does not await it — "non-blocking async
initializations", which is right for a theme and a language. The Files screen
mounts in parallel and reads `filesSession` off the store's **defaults**, which
is `null`.

So the cold start found no session, opened the first enumerated drive, and then
the persistence effect — doing exactly its job — wrote *that* over the real
session. Two tabs on `D:\…\test` became one tab on `C:\`, on disk, before the
user had touched anything.

Both halves of that are worth naming: the feature could not work at all, and it
was **destructive** — the failure erased the thing it failed to read. A
restore that merely did nothing would have been a bug; this lost data.

Found by reading `%APPDATA%\com.amiga-retro-toolkit.desktop\settings.json`
after the application had been reloaded, and noticing the saved session had
shrunk. No test could have caught it: the round-trip is correct
(`paneSession.test.ts` proves it), the tab model is correct, and the defect
lives entirely in *when* the screen asked.

Fixed by keying the cold start on the store's own `loaded` flag rather than on
mount, with `sessionRestored` keeping it once-only. **This is the second time
in one day that a defect survived a green suite and was found only by running
the application** — see [ART-082](#open).

**ART-088** 🟡 **The volume writer deletes a delete-protected entry without noticing the bit** — *fixed 2026-08-13*
`src-tauri/src/core/volume/write/mod.rs::delete` · AmigaDOS refuses to delete a
file whose `d` protection bit is clear — that is what the bit is *for*, and
WHDLoad slaves and system files routinely have it set that way. ART's `delete`
checks that a directory is empty and nothing else; the protection field is read
for the attributes dialog and for `.uaem` sidecars, and ignored here.

So a `Delete` in the file manager removes an entry that the Amiga itself would
have protected, and the user's own `[Confirmation]` settings — which keep
"overwrite read-only" on — have nothing to attach to.

**Half-fixed 2026-08-12** (phase 2b task 7): the file manager now reads the bit
off `PanelEntry.attrs` and asks a third time before deleting a protected entry,
naming it (`isDeleteProtected` in `@/lib/protection`, 6 tests). That is a
confirmation, not a guard — anything that calls `delete` without going through
that screen still deletes silently.

The real fix belongs in the writer: refuse unless an explicit override is
passed, the same shape `SAFE_CREATE` already has for "creating never replaces".
Worth doing alongside the same question for **overwrite** — `volume_put_file`
does not check the `w` bit either.

**Fixed 2026-08-13.** `VolumeWriter::delete` honours the bit and refuses,
naming the entry; `delete_with(.., DeleteProtection::Override)` is the way past
it, so a caller has to *ask* rather than get there by not thinking about it.
The default is `Honour`, so anything that has not been taught the question gets
the safe answer.

The file manager's three confirmations now reach the writer: `volume_delete`
and `volume_delete_many` take the answer as an argument, and the Files screen
sends it only where it has actually shown the question. Move asks **before**
the copy half rather than after — a move is a copy and then a delete, so a
refusal at the end would have left the user with both a duplicate and an error.
An icon deleted alongside the object it belongs to inherits the answer, because
asking again about `Turrican.info` after the user has agreed to delete
`Turrican` is a question with no new information in it.

`replace_file` passes `Override` deliberately: it deletes only so the same name
can carry new bytes, and AmigaDOS governs overwriting with the `w` bit rather
than `d`. **ART does not check `w` yet either** — that half is now
[ART-094](#open) rather than a sentence at the bottom of this one.

Tests: `a_delete_protected_entry_is_refused_by_name` (and the entry is still
there afterwards), `a_delete_protected_entry_goes_when_the_user_has_been_asked`,
`an_ordinary_file_still_deletes_without_being_asked_twice`, and
`the_delete_bit_is_read_the_way_amigados_stores_it` — RWED are stored inverted,
and getting that backwards would refuse every ordinary delete and allow every
protected one.

Found while auditing the brief's §3.4 confirmations against what ART actually
does.

**ART-086** 🔵 **Every path in Settings had to be typed by hand** — *fixed 2026-08-12*
`src/pages/Settings.tsx` · "WinUAE Path" and "Collection Directory" were plain
`<input>`s with a placeholder. ART already opens native pickers everywhere else
(`@tauri-apps/plugin-dialog`'s `open`, used by the Files screen, both studios
and the WHDLoad installer); these were simply the fields that never got it
wired.

Fixed with a `PathField` — the box, a **Browse…** button that opens the right
kind of picker (file for an executable, folder for a directory), and a clear
button. The box stays editable, because a path can be pasted and somebody who
knows where they are going types faster than they click. Empty is stored as
`null` rather than `""`, so "cleared" and "never set" stay the same thing,
which is what every caller's `?? fallback` already assumed.

The same component now serves the two **default folders** the Files screen
opens in, which is what made the fix worth doing today rather than eventually:
a setting the user is expected to fill in is a setting that needs a picker.

Found by the user in the running application, 2026-08-12.

**ART-084** 🟠 **An HDF created as PFS3 or SFS is a DosType with no filesystem behind it, and an Amiga cannot mount it** — *fixed 2026-08-12*
`core/hdf.rs::create_hdf` · `core/rdb.rs::create_rdb_layout` ·
`src/pages/HardDiskStudio.tsx` · The New HDF wizard's third step is "Choose
Amiga Filesystem", and it offers **PFS3-AIO as the default, badged
"⭐ Recommended (Fast & Safe)"**. What `create_hdf` actually writes is the RDSK
block, the PART blocks, and nothing else:

- **No filesystem is created**, for any choice. The partition has no root
  block and no bitmap. For DOS\1 / DOS\3 that is correct and normal — a real
  disk is partitioned on one machine and `Format`ted on the Amiga — but the
  wizard's wording does not say so.
- **No driver is embedded in the RDB.** PFS3 and SFS are not in Kickstart;
  they are loaded from FSHD + LSEG blocks inside the RDB, which ART
  deliberately does not write ([ART-025](#fixed), and G4 of the
  [SD gap analysis](sd-appliance-gap-analysis.md)). `IDNAME_FSHD` and
  `IDNAME_LSEG` exist in `core/rdb.rs` as constants and are referenced by
  nothing.

So the recommended, default option produces an image whose partition an Amiga
does not see at all. That is exactly the "don't claim support that isn't
implemented" rule (spec §10, §89) broken in the one place a user is most
likely to trust ART.

**Half-fixed 2026-08-12:** the wizard now shows the limitation in the dialog,
in both languages, before the image is made
(`hardDisk.modal.warnNoDriver`, `hdfSizeWarning` in `@/lib/hdfSize`, tested).
That turns a false claim into a stated one; it is not the fix.

**And confirmed from outside, the same day.** `hst-imager` — the tool both
existing PiStorm imagers stand on — **refuses** to do what ART's wizard does:

```text
[ERR] File system with DOS type 'PDS3' not found in Rigid Disk Block
```

It will not add a `PDS` partition until the driver is in the RDB. ART is not
being conservative in calling this a defect; it is being late.

**Reading half now built (SD-1, 2026-08-12).** `core/rdb.rs` reads the
FSHD/LSEG chain, and `partitionsMissingDriver` (`@/lib/rdbDrivers`, 6 tests)
answers the question this issue is about: the Hard Disk studio now *names the
partition* that will not mount, instead of warning in general. Verified against
an image `hst-imager` built, agreeing with `rdbtool` on version, size and
seg-list block. The writing half — putting a driver *into* an RDB ART creates —
is still owed. The fix is G4 —
segment-splitting a user-supplied `pfs3aio` binary into an LSEG chain,
checksumming it and wiring `DosType → SegListBlocks` — which is scheduled as
SD-1 and verifiable against `rdbtool`, an oracle that already exists.

**Fixed 2026-08-12 — the writing half (G4 complete).** `create_rdb_layout` now
takes `&[FileSystemSpec]` and writes, per driver, an FSHD block
(`DosType`, `Version`, `PatchFlags = 0x10`, `dn_SegListBlock`,
`dn_GlobalVec = -1`) followed by its own LSEG chain, 492 bytes a block, with
`SummedLongs` declaring how much of the last block is real — the same field the
reader already relied on from the other side. `create_hdf` and the `hdf_create`
command thread it through; the wizard's step 4 asks for the driver when the
chosen filesystem is one Kickstart does not have (`@/lib/fsDriver`, 8 tests).

Rust tests: `a_written_driver_reads_back_with_its_version_and_exact_size`,
`the_drivers_bytes_survive_the_trip_verbatim`, `two_drivers_are_chained_and_both_come_back`,
`a_disk_with_no_drivers_says_so_rather_than_pointing_at_block_zero`,
`a_driver_too_big_for_the_reserved_area_is_refused_with_the_numbers`.

**Verified from outside, both directions.** `rdbtool` reads an image ART built
carrying the real `pfs3aio`, reporting `PDS3/0x50445303 version=19.2
size=59120` — the same three numbers `hst-imager`'s own image reports — and
`rdbtool fsget` extracts the driver back out **SHA-256-identical** to the file
that went in. `hst.imager info` reads the same image and lays out the RDB and
the `PDS\3` partition correctly. ART's reader and writer agreeing with each
other would have proved nothing; this is the half that answers to somebody else.

The version is no longer asked for: `version_from_ver_string` reads it out of
the driver's own `$VER:` string, and refuses rather than defaulting when the
binary says nothing. 0.0 is not a harmless default — AmigaOS keeps the higher
of the version in the RDB and the one already loaded, so a driver claiming 0.0
is one that never runs.

`hardDisk.modal.warnNoDriver` is gone with the limitation it described.
`hdfSizeWarning` no longer answers the driver question at all — it is a size
function, and the answer now depends on what the user picked.

Found by the user in the running application, 2026-08-12.

**ART-083** 🟡 **The New HDF wizard capped disk size at 8 GB, and nothing in the engine asked it to** — *fixed 2026-08-12*
`src/pages/HardDiskStudio.tsx` · The five size buttons (500 MB … 8 GB) were
the whole of what could be chosen. `create_rdb_layout` refuses below 10 MB and
then only when the cylinder count will not fit a `u32` — at 516,096 bytes per
cylinder, a limit measured in petabytes. The 8 GB ceiling was a list of five
numbers in a component.

Fixed by a "Custom…" size that is typed, parsed and validated in
`@/lib/hdfSize` (13 tests): a comma decimal separator (the user's own locale),
the engine's own 10 MB floor as the only lower bound, no upper bound, and a
fraction of a megabyte **refused rather than rounded away** — this is the one
number in the dialog that cannot be changed after the image exists. A
partition past 4 GB now carries a warning about TD64/NSD rather than a
refusal: the image may be for an emulator, or for a machine that has both.

Found by the user in the running application, 2026-08-12.

**ART-082** 🟠 **The Files panes filled the window; their listings did not** — *fixed 2026-08-12*
`src/pages/FileManager.css` · Phase 2b task 1 made the panes fill the window
by construction and was checked by tests, which cannot see a pane. A
`max-height: 420px` on `.tc-row-list` — left from when the commander was a
widget on a page — survived it, so the *pane* grew to the window and the
*listing inside it* stayed 420 CSS pixels tall. On a maximized 4K window that
is twenty rows above six hundred pixels of dead pane, with the status line
stranded in the middle of it: acceptance point 1 of the phase brief
("panes always filling; no dead space") failing in the most visible way
possible.

Fixed by deleting the `max-height`; the flex rule further down the file was
always the thing that should have decided the height.

**Worth more than the fix:** this is the first defect [ART-062](#open) has
actually cost. Two tasks' worth of layout work went in green on 178 frontend
tests, and the first human look at the running screen found this in seconds.
Nothing in a jsdom test has a viewport height.

**ART-072** 🟡 **Selection collision checks compare names case-sensitively, so `Docs` and `docs` are not caught** — *fixed 2026-08-13*
`src-tauri/src/core/volume/write/copy.rs::HostSelection::check_for_name_collisions`
(line ~489) and `src-tauri/src/commands/archives.rs::prepare_archives`
(line ~350) both keep a `BTreeMap<String, _>` keyed on the name exactly as
given, so two roots (or two archives) that would land under the same drawer
name only collide when they are byte-for-byte identical. AmigaDOS is
case-preserving but case-*insensitive* (the same rule `hash::name_hash`
respects, ART-009/ART-010), so `Docs` and `docs` are two different keys
here but the one directory entry there. In `copy.rs` this means the clean,
named refusal `check_for_name_collisions` exists to give never fires for a
case-different pair; the second root is instead silently skipped later,
along with its whole subtree, replacing one clear "rename one of these"
message with a pile of unexplained "skipped" lines. In `archives.rs` the
same gap means two archives that both unpack to a `Docs`/`docs` drawer are
not refused up front either — `std::fs::rename(&item.content_root,
&destination)` is what fails instead, surfacing a raw OS rename error
rather than the friendly named-collision sentence the exact-match case
already gets. Fix is to key both maps on `name.to_lowercase()` instead of
the name itself, the same change `dedupe_case_insensitive`
(`commands/volume_write.rs`) just made for batch deletes.

**Fixed 2026-08-13**, both maps, keeping the name as *typed* for the message
so the sentence still says `Docs` rather than `docs`. Test:
`two_roots_differing_only_in_case_are_refused_too`.

**ART-071** 🟡 **A selection of only symlinks copies nothing and reports success** — *fixed 2026-08-13*
`src-tauri/src/core/volume/write/copy.rs::HostSelection::entries_capped`
(line ~527) · A root that is a symlink is skipped with a bare `continue` —
correct on its own (a link out of the pick would copy in something the user
did not select, the same rule `HostFolder`'s own `walk` applies mid-tree at
line ~414), but nothing records that it happened. If every root in a
selection is a symlink, `entries()` returns an empty `Vec` with no entry in
`CopyReport.skipped`, so `copy_into_volume` runs its loop zero times and
returns a report where `cancelled` is false and `skipped` is empty —
`CopyReport::is_complete()` reads that as a clean success. The user picked
one or more things, ART copied none of them, and every signal available to
the UI says the copy worked. Fix is for `entries_capped` to push a skipped
entry (as `walk`'s sibling check already could, but currently does not
either) rather than silently dropping the root.

**Fixed 2026-08-13** by a `CopySource::skipped_sources` the engine merges into
the report *before* the loop — which is the point, since a source that declined
everything runs the loop zero times. Default-empty on the trait, so no other
source is affected. Tests:
`a_selection_of_nothing_but_shortcuts_says_so_rather_than_reporting_success`
and `a_selection_of_ordinary_files_declines_nothing`.

**ART-061** 🟡 **`formatAge` is always plural in English** — *fixed 2026-08-13*
`src/lib/sources.ts::formatAge` · Returns `{ key: "aminet.age.weeksAgo", params:
{ n } }` (and the `monthsAgo` sibling) for any `n`, and `aminet.age.weeksAgo`'s
English text is the fixed template `"{{n}} weeks ago"` — so a package uploaded
exactly one week ago reads "1 weeks ago", and one month old reads "1 months
ago". Predates Phase 0b: task 7b's job was to move this string to the `Phrase`
layer faithfully, not to redesign it, and reproducing a known wart rather than
quietly fixing it mid-refactor was the right call — but the wart is real and
now has a name. The Turkish side is unaffected: `"{{n}} hafta önce"` is correct
at any `n`, because Turkish does not inflect the noun after a number. Fixing it
means either a plural-aware key pair (`weekAgo` / `weeksAgo`, chosen by `n
=== 1`) or i18next's own plural key suffix (`_one` / `_other`), English-only —
Turkish needs no equivalent change.

**Fixed 2026-08-13** with `_one`/`_other`, and the part that is easy to miss:
i18next picks the plural form from **`count`** and from nothing else, so
`formatAge` returns `{ count }` rather than `{ n }` now. A phrase that kept `n`
would have rendered the `_other` form at every number — the same bug with more
keys. `yearsAgo` is left alone deliberately: it carries a decimal, and "1.0
year ago" is worse English than what it replaces. Both catalogues carry the
pair; the two Turkish forms are identical to each other, which is correct.

### Phase 2a

**ART-079** 🔴 **A 7z archive from any real tool gave one entry another entry's bytes**
`core/archive/sevenz.rs` · Two defects in one read path, both invisible to
ART's own tests and both producing **wrong data with no error**:

1. **Index drift.** `sevenz-rust2`'s `for_each_entries` walks the archive's
   compressed *blocks* first — the entries that carry data, in block order —
   and the streamless ones (directories, empty files) after them. ART counted
   the yields as though they matched `archive().files`, so the moment an
   archive held a directory every index after it pointed at the wrong entry.
   Every archive a real tool writes holds directories; none of ART's fixtures
   did.
2. **A skipped entry was not drained.** A 7z block is one compressed stream
   holding several files end to end, so a file's data begins where the
   previous one's ended. Returning from an unwanted entry without consuming
   its bytes left the block reader short and decoded the *next* wanted file
   from the wrong place — right length, wrong contents. A partial selection is
   the normal case: the gate skips entries that already exist and refuses
   hostile names.

The two together are why `ReadMe.txt` came back holding `Notes.txt`'s text.

→ Entries are matched to indices **by the name the archive stores**, which is
stable under both orderings, and every entry the pass skips has its stream
drained. Verified against an archive the 7-Zip application wrote: ART's
SHA-256 for every entry now equals `7z e -so`'s, through both read paths.

**How it was found, because that is the lesson.** ART's 7z fixtures are built
by ART's own writer, which produces one block per file and no directories —
so they exercised neither defect and passed throughout. Pointing the reader at
a file *another tool* wrote found both in one run. That is the same failure
mode as ART-032…035 and ART-075, for the third time in this project:
`read_foreign_archive_for_oracle_when_asked` and
`read_foreign_c64_for_oracle_when_asked` exist now so it is one command rather
than an idea.

**ART-077** 🟠 **The file manager ignored the object a workflow sent it, so "Open in the file manager" opened nothing**
`src/pages/FileManager.tsx` · Every `Navigate` workflow hands its object over
the same way — a route plus `{ state: { path } }` — and every other studio
reads it on mount (`AdfBrowser`, `CollectionStudio`, `HardDiskStudio`, …).
This screen never did. `iso.browse` (Task 3) pointed at `/files` precisely
because the commander is where a disc belongs, and choosing it left the user
on the file manager with whatever panes they already had and the disc they
dropped nowhere in sight. Nothing failed and nothing said so, which is why it
survived Task 3's review: the route existed, the test asserting
`every_workflow_route_is_a_real_app_route` passed, and the action did nothing.

→ The screen now reads `location.state.path` and opens it in the left pane,
choosing the pane kind from `analyze_paths` — the same detection that offered
the action — rather than from the extension. Found while adding
`archive.browse`, the second workflow to depend on it.

**ART-076** 🟠 **Content-first detection never actually recognised an LHA, and its test could not tell**
`core/detect.rs` · An LHA header carries its compression-method field
(`-lh5-`, `-lhd-`, …) at **offset 2**, after the header length and its
checksum. Detection matched `-l` at offset **0**, which no LHA tool has ever
written — so a real archive fell through to the extension fallback, and one
renamed to `.dat` came back `Unknown`. The whole point of Phase 2a Task 1 was
that a file's name stops deciding what it is; for the format ART was built
around, it still did.

The test that was supposed to cover this, `detects_lha_by_signature`, wrote
five bytes — `-lh5-` — into a file and asserted they were detected. That is
not an archive; it is the method field with nothing around it. Fixture and
code agreed with each other and with no LHA in the world, which is
ART-032…035's shape once again, and the reason the fix is filed rather than
quietly applied.

→ `is_lha_header` checks the field where it belongs: dash, family (`lh`,
`lz`, `pm`), level, dash. The test now builds a real archive with the same
`make_lha_with` the LHA tests use and gives it a `.dat` extension, so it can
only pass on the strength of the signature. `a_method_field_at_offset_zero_is_not_an_lha`
pins the old fixture as *not* an archive. Found while adding ZIP and 7z
signatures next to it (Task 4).

**ART-075** 🟡 **A raw CD image in Mode 2 Form 1 would be misread, and two layers would be wrong together**
`core/detect.rs`, `core/iso/` · ART found a raw 2352-byte-sector track by
probing `CD001` at `0x9311`, which assumes **Mode 1**: 12 bytes of sync, a
4-byte header, then 2048 bytes of user data. A **Mode 2/XA Form 1** sector
carries an 8-byte subheader as well, so its user data starts at 24 and the
signature sits at `0x9319`. ART did not recognise such an image at all — and
worse, the reader took its data offset from the same assumption detection
made, so had one ever been recognised, both layers would have been wrong
*together*. That is the shape behind ART-032…035, and it is the one thing a
green test suite cannot show you. CD32 and mixed-mode discs are written this
way.

→ `SectorLayout::Raw2352Xa` carries the data offset (24) the way the existing
variants carry 0 and 16, so Mode 1 and Mode 2 Form 1 differ in one number
rather than in a branch at every read. Detection probes `0x9319` and reports
`iso9660-raw-xa`; `from_format_hint` turns it back into the layout, pinned by
`detections_format_hints_open_the_right_layout`. **Mode 2 Form 2** — 2324
bytes of audio or video and no filesystem — is refused by reading the submode
byte rather than misread as Form 1 (`refuse_form2`).

Pinned by `a_raw_mode2_xa_track_is_detected_at_0x9319`,
`a_mode1_raw_track_is_not_reported_as_xa`,
`an_xa_form1_disc_reads_exactly_as_a_mode1_one_does` (listing *and* bytes: an
eight-byte slip gives a file that is almost right, which is worse than one
that fails) and `a_mode2_form2_track_is_refused_rather_than_misread`.

The issue also recorded *why* ART's own tests could not close this: no host
mounts a raw track dump, so the 2352 path rested entirely on ART agreeing with
itself. That half is closed too, and separately —
`scripts/iso-oracle-check.py` strips both raw layouts down to 2048-byte
sectors from the *layout's* documented offsets (16 and 24, written in the
script, never read from `core::iso`) and diffs ART's listing and every file's
SHA-256 against 7-Zip's. Both raw fixtures now have an independent
implementation agreeing with them.

### Phase 1a

**ART-074** 🟠 **An accented filename came back corrupted**
`core/adf/bcpl.rs` · AmigaDOS stores strings as Latin-1, one byte per
character. `core/volume/write/dir.rs::put_name` knew that and encoded
correctly — its doc comment even said `write_bcpl_string` "would store `ü` as
two characters and the name would come back wrong on a real Amiga" — but it
fixed the problem locally instead of at the source, and left the read path
alone. `read_bcpl_string` used `String::from_utf8_lossy`, so `Grüße` (bytes
`47 72 FC DF 65`) read back with two replacement characters: `FC` and `DF` are
not valid UTF-8 lead bytes. `dir.rs::name_of` sits three lines from
`put_name` and disagreed with it about the encoding.

Two more callers were wrong in the other direction: `create.rs`'s volume name
and `rdb.rs`'s drive name went through `write_bcpl_string`, which used
`s.as_bytes()` — UTF-8 — so a non-ASCII volume name was written as byte pairs
a real Amiga renders as mojibake. Those two were *self*-consistent with the
old reader, which is why nothing failed: the same shape as ART-032 through
ART-035, where the reader and the writer share a mistake and the suite stays
green. Every name in every test was ASCII. ART-041 had already established
that Latin-1 names are supported, with `Grüße vom Süden` as its own example.

→ Both directions of `bcpl.rs` are now Latin-1, a plain cast each way because
Latin-1 is exactly the first 256 Unicode code points. A character above
`U+00FF` has no byte and becomes `?` rather than a pair an Amiga would show as
two wrong characters. `write_bcpl_string` also encodes before truncating, so a
field limit can no longer cut a character in half. Pinned by
`an_accented_name_survives_a_round_trip`,
`a_name_written_as_latin1_elsewhere_reads_back_intact` and
`truncation_counts_characters_and_never_splits_one`. One existing test,
`bcpl_string_lossy_on_non_utf8`, asserted only `chars().count() == 3` and so
passed under either encoding while claiming to prove replacement characters —
it now asserts the actual string. Found by the whole-branch reviewer chasing a
case-folding question in `delete_many`.


### Phase 0b

**ART-047** 🔵 **Dead code that clippy cannot see**
`core/adf/blocks.rs:81-99` · `block_slice`, `block_slice_mut`, `read_u32_at`
and `write_u32_at` had no callers left outside their own tests —
`core/adf/mutate.rs` was the last production caller, and task 10 (`fbd35ef`)
deleted it. `lib.rs`'s `#![allow(dead_code)]` (CLAUDE.md's one permitted
blanket allow) meant clippy did not flag them. Either give them a real caller
or remove them; audited bounds-checking code that production no longer reaches
is exactly the kind of gap ART-020 exists to stop CI from hiding.
→ Split the decision by what each helper is for. `core/adf/validate.rs::validate`
(~line 191) used to bounds-check `root_off` by hand and then index
`image[root_off..]` directly — the exact pattern this module exists to
replace — so it now calls `blocks::read_u32_at(image, root_block, 0)`, which
bounds-checks through `block_slice` internally. That gives both a real,
reachable production caller: `commands/volume_write.rs`'s §92 pre-flight gate
calls `validate_image` → `validate` before every whole-file commit.
`block_slice_mut` and `write_u32_at`, the write-side pair, are deleted rather
than kept for a caller that does not exist: `core/adf/mutate.rs` was their
only one, and the writer that replaced it, `core/volume/write`'s `BlockSet`
(`layout.rs`), stages each touched block in a `BTreeMap<u32, Vec<u8>>` for the
journal instead of slicing a whole-image buffer — a genuinely different
shape, not an oversight, and no surviving write path indexes a whole image by
block number. No bounds check was loosened to make anything fit; the deleted
functions' own bounds-checking arithmetic (`checked_mul`, the containment
check) is unchanged in the two that remain. `CLAUDE.md`'s bounds-checking
paragraph still names `block_slice_mut` and `write_u32_at` and needs a reword
to drop them. Pinned by `block_access_rejects_out_of_range_numbers` and
`block_word_access_is_bounds_checked` (`core/adf/blocks.rs`, the latter now
read-only), plus `validate.rs`'s existing `wrong_root_type_is_problem` and
`tiny_image_is_problem`, which exercise the new call path.

**ART-048** 🔵 **A source comment still described a module that no longer exists**
`commands/adf.rs:369` (a test's doc comment) · Written for task 8's routing
tests, before `core/adf/mutate` was deleted in task 10 (`fbd35ef`): "a later
task deleting `core/adf/mutate` would be unsafe" described something already
done. `docs/architecture.md`'s reference to `mutate_disk_file` carried the
same staleness and was corrected in an earlier pass — the one exception to
"intent docs are not rewritten" that CLAUDE.md names, because the module it
pointed at is gone.
→ Reworded to state what the test actually guards now: that all four
commands land on `core/volume`, ART's only filesystem writer, now that
`core/adf/mutate` is gone. A tree-wide search
(`grep -rn "mutate_disk_file\|adf/mutate" src src-tauri/src`) turned up no
other production code depending on the retired module — every remaining hit
is a past-tense comment recalling what it replaced.

**ART-051** 🟡 **`FEATURES.md` carries raw control bytes and git treats it as binary**
`docs/FEATURES.md` (the DosType write-matrix table, ~line 140) · Eight bytes
in the range `0x00`–`0x07` were embedded in the table as literal control
characters where the escaped text `\x00` … `\x07` was intended — the same
escaped style the rest of the file uses. They rendered as empty backticks, and
git classified the file as binary, so every change to it showed as
`Bin N -> M bytes` with no reviewable diff.
→ Each of the eight raw bytes replaced with its escaped text
(`python -c "d=open('docs/FEATURES.md','rb').read(); ..."` found the exact
offsets). A byte-for-byte alignment of the old and new file confirms those
eight positions are the *only* difference — everything else, including the
CRLF line endings, is untouched. `git diff --stat` for *this* commit still
prints `Bin 14510 -> 14261 bytes`: git classifies either side of a diff pair
as binary if it contains one raw NUL byte, regardless of what the other side
looks like, and the pre-fix blob has exactly one (the `DOS\x00` cell) —
confirmed by reproducing the same `Bin` notation on a two-line synthetic
before/after pair with `git diff --no-index`, including with `-a`/`--text`
forced. That is a property of diffing *against the old, binary-flagged blob*,
not of the fixed file: every diff of `docs/FEATURES.md` from this commit
onward, once neither side has a raw NUL, renders as ordinary text.

**ART-063** 🟠 **ART could not write a disk an Amiga would boot from**
`core/adf/create.rs`, `core/adf/bootcode.rs` · The `bootable` flag wrote
`0x4E 0x75` — a bare `RTS` — at offset 12 and nothing else. Kickstart's `strap`
validates the boot block's `DOS` signature and checksum, jumps to offset 12,
and requires the code there to return `D0 = 0` with `A0` holding an address to
jump to; **a non-zero `D0` raises a system alert and reboots**. An `RTS`
returned with whatever `D0` already held, so nothing was ever loaded. Invisible
to every test ART had, and to the amitools oracle, because both only ask
whether the boot block is *well-formed* — `xdftool` reported `bootable: True`
for the RTS stub too. Only Kickstart can answer whether the code runs.
→ `core/adf/bootcode.rs` assembles what the contract asks for: read `ExecBase`
from absolute address 4 (guaranteed, unlike `A6` on entry, which is convention),
`FindResident("dos.library")` at exec LVO −96, take `rt_Init` at offset 22 of
the returned `struct Resident`, return `D0 = 0` with `A0` pointing at it. ART's
own implementation, written from the documented contract and the published LVO
table — Commodore's boot block is copyrighted and ART ships no Amiga content,
ever. The two relative displacements are computed from the layout rather than
hand-counted, because miscounting either produces a disk that hangs a real
machine while every test still passes; seven tests in `bootcode::tests` pin
each landmark and both displacements independently. **Verified by booting
`test/art-bootable-test.adf` on 2026-08-11.** Note the scope: a disk that
boots is not a disk that boots to Workbench — DOS then wants
`S/Startup-Sequence` and the `c/`, `libs/` and `l/` contents behind it, which
are AmigaOS content ART cannot supply. Reaching a CLI prompt is the whole
claim, and `info.bootable` now means what it says.

### Phase 0a

**ART-059** 🔵 **A flaky test could fail CI at random**
`net/http_mirror.rs` (test helpers) · `a_plain_download_reports_what_it_wrote`
and `a_206_is_a_resume_and_carries_the_whole_size` read
`requests.lock().unwrap()[0]` immediately after `fetch` returned, but the test
server thread records the request *after* `handle` has written the response.
The client only needs those bytes, so it can return first, leaving the vector
empty and the index panicking. It lost roughly one full-suite run in five —
found by running the suite five times before merging Phase 0a rather than
once. A test that fails at random in a blocking CI trains people to re-run
until green, which is how a real failure gets waved through.
→ `wait_served(&served, n)` blocks on the `served` counter, which the thread
stores with `SeqCst` *after* the push, so it is a real happens-before edge
rather than a sleep. Applied at all three sites that read the recorded
requests. Verified by five consecutive clean full-suite runs.

**ART-037** 🔴 **ADF Studio could not open any bootable ADF**
`core/adf/bootblock.rs`, `core/adf/mod.rs` · The parser read bytes 8..11 of the
boot block as a "root block pointer". An AmigaDOS boot block has no such field:
0..3 is the DOS type, 4..7 the checksum, and 8 onwards is boot code. ART
therefore read 68000 machine code as a block number and refused every bootable
disk with `root block has type <nonsense>`. Invisible to ART's own tests
because every fixture ART builds has zeros where a real disk has boot code —
and invisible in day-to-day use because the file manager, which runs on
`core/volume`, opened the same disks perfectly.
→ The root block is computed, `total_blocks / 2` via `root_block_of()`, the
way `VolumeGeometry::root_block_for` already did. Pinned by
`a_bootable_image_opens_because_the_root_block_is_computed` and by an oracle
check that has `xdftool` write a bootable floppy — its own boot code,
checksum-legal — for ART to open.

**ART-038** 🟡 **HD ADFs reported half their capacity**
`core/adf/mod.rs` · `info()` computed capacity and parsed the bitmap with
`DD_TOTAL_BLOCKS`, so every number it reported for a 1.76 MB image was a
floppy-shaped guess. Same root cause as ART-037: `core/adf` predated the
Stage R geometry and never adopted it.
→ Both come from the image's own length, via `total_blocks_of()`. Pinned by
`an_hd_image_reports_its_real_capacity`.

**ART-039** 🟡 **Disabled controls were indistinguishable from active ones**
`src/styles/global.css` · The word `disabled` appeared in no stylesheet, so a
disabled primary button kept its accent fill. On the WHDLoad screen a
correctly-refused install showed a solid blue "Install to the disk" button
that did nothing when clicked.
→ `:disabled` and `:focus-visible` styles for buttons and form controls.

**ART-040** 🟡 **Content was clipped instead of scrolled, and nothing scaled**
`src/components/layout/layout.css` · `.app-main` is a grid item with the
default `min-height: auto`, so it grew to fit its content and `.app-content`'s
`overflow: auto` never engaged; the shell's `overflow: hidden` then clipped the
excess. `min-height: 0` appeared nowhere in the codebase. Separately, the
application had zero `@media` queries and eleven pages carried hand-written
`maxWidth` values.
→ The `min-height: 0` chain, breakpoints, and one central content width.

**ART-044** 🟠 **WHDLoad and Aminet installs could land a package partially with no warning**
`commands/sources.rs` · `sources_install_adf` and `sources_install_volume`
both wrote through `run_copy_in_folder`, which lands what fits and reports the
rest as `skipped` — the right contract for the general-purpose F5 copy, wrong
for an install. Neither install command ran a pre-flight fit check before
writing, so a package that did not fit was discovered mid-loop: a WHDLoad pack
whose `.slave` ended up in `skipped` is a broken, non-bootable result the user
was never warned about (§92: explain before modify).
→ `plan_copy_in_folder` (`commands/volume_write.rs`) mirrors
`volume_plan_copy`'s own pre-flight against an already-built `HostFolder` and
refuses atomically, with the real block numbers, before either install command
writes anything. Pinned by
`installing_a_package_that_does_not_fit_is_refused_without_touching_the_image`
and `installing_a_package_that_fits_installs_completely`.

**ART-045** 🟠 **A transient re-read after a successful write could report it as failed**
`commands/adf.rs` · `MutationOutcome::from_write` re-opened the image with
`AdfImage::open()` after `with_volume` had already committed the write and
closed its handle. That second, independent open could race an external lock
(antivirus scan-on-close, a search indexer) taken the moment the first handle
released, turning an already-durable, successful write into a reported
failure — and losing the backup path along with it, contradicting the
guarantee that the user is always told where the previous version went.
→ `VolumeWriter::all_bytes()` (`core/volume/write/mod.rs`) reads the volume
block by block through the session's own still-open device; `info_from_session`
builds `AdfInfo` from that, called from *inside* every `with_volume` closure,
so a failure there aborts before any byte reaches disk rather than after.
`MutationOutcome::from_write` is deleted, not disarmed. Pinned by
`a_failure_after_the_mutation_never_reaches_the_disk` plus per-command routing
tests (`creating_a_directory_goes_through_the_volume_writer`,
`deleting_an_entry_goes_through_the_volume_writer`,
`renaming_an_entry_goes_through_the_volume_writer`).

**ART-052** 🟠 **A cancelled install committed half a package and reported success**
`commands/sources.rs`, `commands/volume_write.rs` · `copy_into_volume` stops
between files and returns `Ok(report)` with `cancelled: true` — the right
contract for the general-purpose F5 copy. `run_copy_in_folder` then went
straight on to `commit_whole_file`, and neither install command ever read
`report.cancelled`: they read `files_copied`, `bytes_copied` and `skipped`, and
the files that were never reached appear in none of those, because the loop
`break`s. Cancelling an install therefore wrote half the package to the disk,
finished the job as **Completed**, and recorded a successful install in the
operation log — with the `.slave` quite possibly among the files that never
landed. The retired `install_archive_into_adf` had checked `is_cancelled()`
inside its `mutate_disk_file` closure and returned `CoreError::Cancelled`, so
nothing was written at all; that guarantee was lost in the move to the volume
writer.
→ `run_copy_in_folder_with(.., OnCancel::Abandon, ..)` returns
`CoreError::Cancelled` *before* `commit_whole_file` is reached, so the user's
image never leaves the state it was in. F5 keeps `OnCancel::KeepWhatLanded`
deliberately. Pinned by
`a_cancelled_install_writes_nothing_and_does_not_report_success`, which
compares the image's bytes before and after.

**ART-053** 🟠 **`VolumeWriter::all_bytes()` allocated from an unchecked block count**
`core/volume/write/mod.rs` · Introduced with ART-045's fix and documented as
"only sensible for a volume small enough to hold in memory", with nothing
enforcing it — while `commands/adf.rs::info_from_session` calls it inside
every `with_volume` closure and every ADF command takes an arbitrary image
path from the frontend. Pointed at an image over 16 MiB, `with_volume` takes
the block-journal branch: the write lands *in the file*, the journal is
deleted, and only then does `all_bytes()` try to read `total_blocks ×
block_size` into memory — a multi-gigabyte allocation in a profile that sets
`panic = "abort"`, or an `Err` returned after the write is already durable and
its journal gone.
→ `all_bytes()` computes the size with `checked_mul` and refuses anything over
`WHOLE_FILE_LIMIT_BYTES` — the same constant `WriteStrategy::for_image` splits
on, so "small enough to read whole" means one thing in ART. Pinned by
`all_bytes_refuses_a_volume_too_large_to_hold_in_memory`.

**ART-046** 🔵 **A doc comment claims a guarantee the public API does not give**
`core/volume/write/mod.rs` · The comment above the test-only `commit_blocks`
said "every public operation here deliberately cannot get into that state" — a
volume whose touched blocks are each well-formed but whose structure is wrong.
`add_file` on an image whose bitmap is flagged valid but marks the root block
free reaches exactly that state through the public API: `bitmap.rs`'s allocator
hands out anything free in `reserved..total_blocks` and the root block is not
inside `reserved` (2, on a floppy), so a plain `add_file` allocates block 880
and overwrites the root, with every touched block internally well-formed. The
claim was the argument for deleting ART-042's whole-image gate as redundant.
→ The comment now says what is true — no public operation can reach that state
on a *well-formed* volume, the flagged-valid-but-wrong bitmap can, and that is
what the gate above catches. Narrowed rather than "proved", because the public
API really can reach it.

**ART-054** 🟡 **The WHDLoad refusal panel contradicted itself**
`src/pages/WhdloadInstall.tsx`, `commands/whdload.rs` · On the "not a WHDLoad
pack" path Rust deliberately blanks the layout and zeroes the cost, and the UI
rendered them anyway: "Create the drawer `[blank]` and write 0 files, 0 B ·
0 blocks needed · 0 free" printed directly above the amber refusal, an empty
pack name badged *"no icon — it will not show up on Workbench"*, and an install
button reading "Install to " because `?? "the disk"` catches `null` but not the
empty string Rust sends. The panel's suggestion was a hardcoded "You can still
copy it by hand from the Files screen", which is wrong or duplicated for five
of the six refusals — a full disk, a name already taken and unsupported names
all fail the same way by hand, `needs_installer` needs WinUAE, and the
low-confidence reason already ends with that same sentence.
→ `hasPack()` (`src/lib/whdload.ts`) gates the cost paragraph and the
pack-name/icon block; the button falls back with `||`. The remedy is now data:
`WhdloadRefusal { reason, suggestion }` carries it from `refuse()`, so the hand
copy is suggested for exactly the one refusal it answers. Refusals still arrive
as `plan.refusal` inside an `Ok` and exceptions still reach the red banner —
`a_missing_pack_is_a_refusal_and_a_broken_archive_is_still_an_error` pins both,
and now also pins which suggestion travels with which reason.

**ART-055** 🔵 **The install pre-flight guard had no test that would fail if it were deleted**
`commands/sources.rs` · ART-044's fix put `if !plan.fits() { return
Err(SafetyRefused) }` inline inside `sources_install_adf`'s and
`sources_install_volume`'s `spawn_job` closures, which no test can call. The
test named after it called `plan_copy_in_folder` directly and asserted the
image was unchanged — trivially true, since only a read-only planner had run.
Deleting either guard left the suite green.
→ Both command bodies are factored into `install_archive_into_volume`, the way
task 8 factored the ADF commands, and the test drives that. Verified by
mutation: reverting the guard fails the test.

**ART-056** 🟡 **The sidebar clipped the page on any window shorter than its own nav list**
`src/components/layout/layout.css` · ART-040 applied `min-height: 0` to
`.app-main` and not to `.sidebar`, the other item in the same implicit grid
row. With fourteen nav entries the sidebar's min-content height is about
665px, so on a shorter viewport the auto row grew past `100vh`, `.app-main`
stretched with it and the shell's `overflow: hidden` clipped the bottom of
`.app-content` — including the bottom of its own scrollbar. A 1366×768 laptop
at 125% scaling is about 584 CSS px, so this was the ordinary case.
→ `.sidebar { min-height: 0; overflow-y: auto; }`.

**ART-057** 🟡 **Two more controls looked enabled while disabled**
`src/styles/global.css` · ART-039's fix keyed on `.btn`, and
`.breadcrumb-item` and `.file-row-dir-btn` (both `disabled` while an operation
runs, `src/pages/AdfBrowser.tsx`) carry neither — so both kept `cursor:
pointer` and their live colour. Also found in the same file: the
`:focus-visible` block ART-039 added was byte-identical to a universal
`:focus-visible` rule already 60 lines below it, and `CHANGELOG.md` claimed a
focus ring was "visible again" when it had never been missing.
→ The `.btn:disabled` treatment applied to both controls (the last breadcrumb
keeps full contrast — it is disabled because it is where you already are), the
duplicate rule removed, and the CHANGELOG line corrected.

### Stage W

**ART-042** 🔴 **A write that produced an invalid volume was committed anyway**
`commands/volume_write.rs` · The retired `core/adf/mutate.rs::mutate_disk_file`
validated the finished image in memory before writing it. `with_volume`, which
replaced it, ran the operation and went straight to `guarded_write` — under a
comment that said the opposite. The only surviving check was
`core/volume/write::validate_touched`, which sees just the blocks the operation
touched and, of those, only their checksums. A write that left every touched
block well-formed and the volume structurally wrong — a root block that is no
longer a header block, an image that stopped being an AmigaDOS volume —
replaced the user's file silently, and the backup it took was of the last good
version, so the damage was recoverable only if the user noticed.
→ `commit_whole_file` validates the whole in-memory image at all three
whole-file commit points and refuses before `guarded_write` is reached. Only
`Problem` findings refuse; a warning describes an image that was already like
that. Pinned by `a_write_that_would_not_validate_never_reaches_the_file`
(refused, and the file byte-for-byte unchanged) and
`a_valid_write_still_commits_through_the_gate`.

**ART-041** 🟠 **Validation measured every image against a DD floppy**
`core/adf/validate.rs` · `validate` compared `image.len()` against
`DD_TOTAL_BLOCKS * BLOCK_SIZE` and warned on anything else, so every HD floppy
and every hard-disk image was reported as suspect. Harmless while validation
was only a report; a wall the moment it became the gate in front of every write
(ART-042), which would have refused writes to every image that is not a DD ADF.
→ The block count comes from the image's own length and the root block from
that count (`VolumeGeometry::root_block_for`). Pinned by
`an_hd_floppy_image_is_healthy_at_its_own_geometry`,
`a_hard_disk_sized_image_is_healthy_at_its_own_geometry` and, end to end,
`an_hd_floppy_is_written_through_the_gate` /
`a_hard_disk_image_is_written_through_the_gate`.

**ART-036** 🟡 **Names with accented characters were refused as too long**
`core/volume/write/dir.rs` · `check_name` compared `str::len()` against the
30-character AmigaDOS limit. That counts **UTF-8 bytes**, not characters, so
every Latin-1 character above `~` counted double: `Grüße vom Süden` is 15
characters and 18 bytes, and a name of thirty accented characters was rejected
as sixty. AmigaDOS stores one byte per Latin-1 character, so all thirty fit.
The more accents a name had, the more wrong the answer — which is why it went
unnoticed in ASCII-only testing.
→ The check counts characters, and `plan.rs` shortens by characters too:
byte-slicing a Latin-1 stem would cut a character in half and panic. Pinned by
`the_length_limit_counts_characters_not_utf_8_bytes` and
`shortening_a_latin_1_name_does_not_split_a_character`.

### Amiga format compatibility (Stage R oracle)

All four were found the same way: by pointing **amitools** — an independent
implementation — at images ART had written. Every one of them was invisible to
ART's own test suite, because ART's reader and writer agreed with each other.
That is the project's oldest failure pattern, and this is the first time an
outside implementation has been asked.

**ART-033** 🔴 **Every ADF ART wrote had invalid block checksums**
`core/adf/checksum.rs` · AmigaDOS uses **two** checksum algorithms: the boot
block sums with end-around carry and stores the bitwise NOT, while root, header,
OFS data and bitmap blocks use a plain wrapping sum stored as its two's
complement. ART applied the boot block algorithm to everything. `core/rdb.rs`
had the correct one all along, which is why RDB blocks always validated
elsewhere and ADFs never did.
Every disk ART created or modified was rejected by AmigaOS, WinUAE and every
other Amiga tool — while ART reported it healthy, because its verifier used the
same wrong algorithm. Confirmed: `xdftool` refused an ART-made blank ADF with
"Invalid Root Block(2)".
→ `block_checksum` is now the plain sum; the boot block keeps its own
implementation in `bootblock.rs`. Verified: `xdftool` now reads an ART-made ADF
and lists its volume.
Tests: `a_checksummed_block_sums_to_zero`,
`the_algorithm_matches_the_reference_implementation`,
`checksum_all_zero_block` (which had been pinning the bug).

**ART-034** 🔴 **Files ART added to a disk were zero bytes on a real Amiga**
`core/adf/blocks.rs`, `core/adf/mutate.rs` *(mutate.rs deleted when task 10
retired it — fixed, not reopened; see ART-047/048)* · The file header's `byte_size` was
read and written at longword 79, and the comment at longword 80. The real
layout is `byte_size` at longword 81 (offset 324) and the comment at longword 82
(offset 328) — 80 is the protection bits. Both halves of ART used the same wrong
offsets, so a file round-tripped through ART perfectly and appeared as an empty
file to AmigaOS. The root block's bitmap-extension pointer was one longword
early too (412 instead of 416), which would matter on any volume large enough to
need extension blocks — that is, exactly the HDF partitions Stage R adds.
→ Offsets corrected against `amitools` (`EntryBlock._read_nac_modts`,
`FileHeaderBlock._read`). Verified: `xdftool` lists a file ART wrote with the
right size and `xdftool type` returns its contents.

**ART-035** 🔴 **The free-space bitmap was laid out the wrong way round**
`core/adf/blocks.rs`, `core/adf/create.rs`, `core/adf/mutate.rs` *(mutate.rs
deleted when task 10 retired it — fixed, not reopened; see ART-047/048)* ·
ART recorded
a block's bit at longword `block / 32`, counting from the **most** significant
bit. AmigaDOS records it at `(block - reserved) / 32`, counting from the
**least** significant bit — the bitmap does not describe the boot blocks at all.
ART was wrong on both axes, so every block it marked used landed on some other
block's bit. AmigaOS would have seen ART's occupied blocks as free and handed
them out to the next file written, destroying data on a real machine.
Invisible from inside ART: the allocator, the free-space count and the parser
all shared the same arithmetic, and a permutation of bit positions leaves the
*count* of free blocks unchanged, so even the totals looked right.
→ One `bit_position()` helper now owns the arithmetic and all four call sites
go through it. Confirmed against ADFlib (`adf_bitm.c`: `bitMask[i] == 1 << i`,
`sectOfMap = nSect - 2`) and amitools (`ADFSBitmap.get_bit`). Verified by
decoding an ART-written disk with the reference mapping: the blocks marked used
are exactly the root, the bitmap and the file that was added.
Tests: `bit_positions_match_the_reference_implementations`,
`bitmap_marks_used_and_free` (which had been pinning the bug),
`a_large_volume_spills_into_more_bitmap_blocks`.

**ART-032** 🟠 **RDB partitions described the wrong DosType and boot priority**
`core/rdb.rs` · `BootPri` was read and written at longword 46 and `DosType` at
47. They belong at 47 and 48; longword 46 is `Mask`. An ART-created hard disk
image therefore told AmigaOS its partitions had DosType 0 — no filesystem —
while ART read its own images back correctly. Found by `rdbtool`, which showed
`pri=1146049281` (the ASCII for `DOS`) sitting in the priority field.
→ Offsets corrected and pinned.
Tests: `dosenv_offsets_match_the_amiga_layout`,
`a_partition_reads_back_the_filesystem_it_was_written_with`.

### Software Sources (Aminet, §41.5)

Both found the same way: by running the finished engine against real Aminet
mirrors instead of only against its own fixtures. Neither was reachable from
the test suite as written, which is the lesson worth keeping.

**ART-030** 🔴 **Mirror failover concatenated a dead mirror's bytes onto the next one's**
`core/sources/mirror.rs` · `fetch_with_failover` passed the same `Write` to
every attempt. A mirror that failed *after* sending part of the body left those
bytes in the destination, and the next mirror's response was appended to them.
The result was one file assembled from two mirrors — which parses, hashes and
caches perfectly happily while being wrong.
Observed live: `aminet.net` dropped the connection 519 416 bytes into `INDEX`,
and the catalog came back with 91 413 entries instead of the 85 435 the file
actually holds. No error, no skipped line, no warning.
→ The destination is now a `FetchTarget`, wound back before every attempt.
`HttpMirrorClient` also refuses a body shorter than an announced
`Content-Length`, so a silently truncated response cannot pass as complete.
Tests: `a_mirror_that_dies_mid_body_does_not_pollute_the_next_one`,
`a_file_destination_is_wound_back_too`,
`a_dirty_destination_is_wound_back_before_the_first_attempt`,
`a_body_shorter_than_the_announced_length_is_never_a_success`.

**ART-031** 🟠 **LHA archives with level 2 or 3 headers could not be opened at all**
`core/lha/mod.rs`, `core/lha/safe_extract.rs` · both read `header.filename`
directly. That field is only populated for level 0 and 1 headers; levels 2 and
3 — what modern tools write, and what Aminet hosts — leave it empty and carry
the name in an extended header. Every such archive failed with
`ART-FORMAT-MALFORMED: empty entry name`, so LHA Studio could not list or
extract a typical Aminet download.
→ `entry_path()` falls back to `parse_pathname_to_str()` when the raw field is
empty. Levels 0 and 1 deliberately keep using the raw field: `delharc`
percent-encodes non-ASCII bytes, and switching those levels over would rename
the Latin-1 Amiga filenames that extract correctly today. `safe_join` remains
the choke point for both.
Tests: `a_level_two_header_is_read_from_its_extended_header`,
`a_level_zero_name_still_comes_from_the_raw_field`.

All fixed 2026-08-09 unless noted. Test names are in `src-tauri/`.

### Hard disk images (Stage 3 audit)

**ART-021** 🔴 **Creating or opening an HDF allocated the whole image in memory**
`core/hdf.rs`, `core/rdb.rs` · `create_rdb_image` built the entire image as a
`Vec<u8>` before writing it, and `open_hdf` read the whole file back to report
its geometry. A 4 GB hard disk image therefore needed 4 GB of RAM — on a machine
that could not spare it, ART died. Violates §56 (never allocate from an
unchecked length).
→ `create_rdb_layout` returns only the structured leading blocks; the file is
created sparsely with `set_len`. `open_hdf` reads a 1 MB header window and takes
the size from the file's metadata. Tests: `large_images_are_created_sparsely`,
`create_and_parse_rdb_image_with_pfs3_and_ffs`.

**ART-022** 🔴 **Creating an image silently destroyed an existing one**
`core/hdf.rs`, `core/adf/create.rs` · Both creation paths called
`std::fs::write`, which replaces whatever is already there. Typing the name of
an existing `Workbench.hdf` destroyed it — gigabytes, with no backup and no
prompt. Creation is a `SAFE_CREATE` operation and may only ever write something
new (§57).
→ Both refuse when the target exists; the HDF path uses `create_new` and removes
a partially written file if anything fails. Tests:
`create_refuses_to_replace_an_existing_image`,
`save_refuses_to_replace_an_existing_file`.

**ART-023** 🟠 **A tiny requested size aborted the application**
`core/hdf.rs` · A plain image smaller than four bytes was indexed at `[0..4]` to
write the bootblock signature, panicking — and with `panic = "abort"` that ends
the process.
→ A minimum size is enforced and reported as an error.
Test: `absurdly_small_sizes_error_rather_than_panic`.

**ART-024** 🔴 **The RDSK block described a zero-capacity disk to AmigaOS**
`core/rdb.rs` · The logical-drive fields were written at the wrong longwords.
`HiCylinder` (LW 35) and `CylBlocks` (LW 36) were never written at all, so they
stayed zero; `RDBBlocksHi` got the value meant for another field, and
`cylinders - 1` landed in a reserved longword. ART read its own disks back
correctly because the parser only reads geometry from LW 16–18 — the same shape
of defect as ART-009: self-consistent, and wrong for every other Amiga tool.
Verified against `amitools`' `RDBlock.py` field indices.
→ Fields written at their documented offsets.
Test: `rdsk_logical_drive_fields_are_written`.

**ART-025** 🟠 **Empty RDB block lists pointed at block 0**
`core/rdb.rs` · `BadBlockList`, `FileSysHeaderList` and `DriveInit` were left at
zero. Zero is a valid block number, so AmigaOS would follow them into block 0 —
the RDSK block itself. `BlockBytes` was also zero rather than 512.
→ Absent lists are written as `-1` (`NO_BLOCK`) and `BlockBytes` is set.
Test: `rdsk_absent_lists_use_the_no_block_sentinel`.

**ART-026** 🟠 **Oversized partitions were silently truncated**
`core/rdb.rs` · Each partition's end was clamped with `.min(cylinders - 1)`. Ask
for two 500 MB partitions on a 100 MB disk and the first was quietly shrunk to
whatever fit, the rest were skipped mid-loop — leaving the previous partition's
`next` pointer aimed at a block that was never written. The user was told
nothing. Violates §89.
→ The total is checked up front and an impossible layout is refused with the
numbers involved. Tests: `oversized_partitions_are_refused_not_truncated`,
`partitions_do_not_overlap_and_stay_inside_the_disk`,
`too_many_partitions_are_refused`.

**ART-027** 🟡 **RDB checksums ignored the block's own `SummedLongs`**
`core/rdb.rs` · The checksum summed a fixed 128 longwords instead of the count
the block declares (LW 1, conventionally 64). ART's own images happened to pass
because their later longwords are zero, but a real Amiga disk carrying vendor
strings there would be reported as corrupt.
→ Both compute and verify honour `SummedLongs`, with a guard against a
nonsensical declared value. Tests: `checksum_honours_summed_longs`,
`checksum_survives_a_nonsense_summed_longs_field`.

### Collection & ROM (Stage 3 audit)

**ART-028** 🟠 **A folder scan could be sent into unbounded recursion**
`core/collection.rs` · `collect_files_recursive` followed every subdirectory
with no depth limit and no symlink check. A Windows junction pointing back up
its own tree recursed until the stack overflowed, which aborts the process.
Dropping such a folder onto ART closed the application.
→ Depth is capped and symlinks are not followed.
Tests: `scanning_stops_at_the_depth_limit`, `scanning_finds_images_within_the_limit`.

**ART-029** 🟡 **Any file could be read whole as a "ROM"**
`core/rom.rs` · `identify_rom` read the entire file before checking anything.
Kickstarts are at most 1 MB, but the user picks the file — a mistaken pick of a
large image pulled all of it into memory.
→ Size is checked against a 4 MB ceiling before reading.

### Data safety

**ART-001** 🔴 **ADF edits overwrote the original with no backup and no atomicity**
`core/adf/mod.rs` · `mutate_disk_file` called `std::fs::write` straight onto the
user's file. An interrupted write left a truncated — that is, destroyed — disk
image, and there was no previous version to fall back on. Violates §57, §93.
→ Pipeline is now `read → mutate → validate → backup → atomic commit` via
`core/safety`. Tests: `mutation_backs_up_the_previous_version`,
`failed_mutation_leaves_the_original_untouched`,
`mutation_that_corrupts_the_image_is_not_committed`.

**ART-002** 🔴 **LHA extraction silently overwrote existing files**
`core/lha/safe_extract.rs` · `File::create` replaced whatever was already at the
destination. Extracting an archive into a working folder destroyed the user's
files without a word. Violates §89.
→ `OverwritePolicy::{Skip,Overwrite,Rename}`, defaulting to `Skip`.
Test: `existing_files_are_skipped_by_default`.

**ART-003** 🔴 **FlashFloppy `FF.CFG` was regenerated from scratch**
`core/gotek.rs` · Saving rewrote the whole file from ART's own fields, deleting
every setting it does not model — `pin02`, `head-settle-ms`, `display-order` and
dozens more that Gotek owners tune by hand. Directly violates §39
("Unknown settings must be preserved").
→ The file is now edited in place; unmanaged keys and comments pass through
verbatim. Test: `round_trip_preserves_unknown_settings`.

**ART-004** 🔴 **PiStorm `cmdline.txt` was regenerated, leaving the SD card unbootable**
`core/pistorm.rs` · Same defect, worse consequence: regenerating the Raspberry Pi
kernel command line dropped `root=` and `console=`, so the Pi would not boot at
all after ART "saved" the configuration. Violates §40.
→ `config.txt` and `cmdline.txt` are merged, not rewritten.
Test: `cmdline_round_trip_preserves_boot_parameters`.

**ART-005** 🟠 **Config files were replaced without a backup**
`core/gotek.rs`, `core/pistorm.rs` · No previous version was kept.
→ Both go through `guarded_write` with `BackupPolicy::CONFIG` (5 generations).
Tests: `saving_backs_up_the_previous_config`,
`saving_backs_up_the_previous_sd_configuration`.

**ART-006** 🟡 **Aborted extractions left truncated files behind**
`core/lha/safe_extract.rs` · A file that failed to decompress, or was cut short
by the bomb guard, stayed on disk looking like a successful extraction.
→ Partial output is removed and the entry is reported as skipped with a reason.

### Crashes

**ART-007** 🔴 **An invalid block number from the UI killed the whole application**
`core/adf/blocks.rs`, `core/volume/write/mod.rs` · Block numbers arrive from
the frontend (`dirBlock`, `headerBlock`) and were used to index the image
directly. Out of range meant a panic, and the release profile sets
`panic = "abort"` — so the entire app died.
→ All access goes through `blocks::block_slice`/`block_slice_mut`/`read_u32_at`/
`write_u32_at`, which return a `CoreError`. *(Location updated: the fix and its
tests originally lived in `core/adf/mutate.rs`, deleted when task 10 retired it
in favour of the single volume writer — fixed, not reopened; see ART-047,
where `block_slice_mut`/`write_u32_at` were later deleted for good — no write
path indexes a whole image by block number any more — and `block_slice`/
`read_u32_at` gained a new caller in `core/adf/validate.rs`.)* Tests:
`block_access_rejects_out_of_range_numbers` (`core/adf/blocks.rs`),
`out_of_range_directory_block_is_an_error` (`core/volume/write/mod.rs`).

**ART-008** 🟠 **Malformed images could hang the UI forever**
`core/volume/write/dir.rs`, `core/volume/write/file.rs` · Hash-bucket and
file-extension chain walks had no step limit; a chain pointing back at itself
looped indefinitely.
→ Both walks are bounded and report a malformed image rather than looping.
*(Location updated: originally `core/adf/mutate.rs`, deleted when task 10
retired it — fixed, not reopened.)* Tests:
`a_hash_chain_that_loops_is_an_error_not_a_hang` (`dir.rs`),
`an_extension_chain_that_loops_is_an_error_not_a_hang` (`file.rs`).

### Filesystem correctness

**ART-009** 🔴 **The ADF hash function was not AmigaDOS-compatible**
`core/adf/hash.rs` · `name_hash` omitted the `& 0x7ff` mask that the reference
algorithm applies after *every character*. Entries went into the wrong hash
buckets. ART could read its own images back — it hashed consistently — but
AmigaDOS, WinUAE and every other Amiga tool could not find the files. Any image
ART wrote before this fix is suspect.
→ Matches `adfGetHashValue`, with reference values pinned.
Tests: `matches_the_amigados_reference_values`,
`the_mask_is_applied_every_iteration`.

**ART-010** 🟠 **International volumes hashed names with the wrong case folding**
`core/adf/hash.rs` · `name_hash` took no `international` flag, so INTL volumes
(which fold the Latin-1 224–254 range) used plain ASCII rules.
→ The flag now comes from the volume's own bootblock.
Test: `international_folding_differs_for_accented_names`.

**ART-011** 🔴 **`rename_entry` corrupted directories when the chain was inconsistent**
`core/volume/write/dir.rs` · If the entry was not found in its parent's hash
chain the function carried on silently and linked it into the new bucket
anyway, leaving it referenced from two places. `delete_entry` had the check;
rename did not.
→ Missing entries now error out via `predecessor_of`, which ends its walk with
`Malformed { "block N is not in the bucket its name hashes to" }` rather than
returning "no predecessor". *(Location updated: originally `core/adf/mutate.rs`,
deleted when task 10 retired it — fixed, not reopened; the moved test also now
asserts the image is byte-for-byte unchanged after the refusal, which the
in-memory original could not check.)*
Test: `rename_of_an_unlinked_entry_reports_an_error` (`core/volume/write/mod.rs`).

**ART-012** 🟠 **Duplicate names could be created in one directory**
`core/volume/write/dir.rs` · AmigaDOS compares names case-insensitively and
cannot hold two entries with the same name; nothing checked.
→ Name collisions are rejected, case-insensitively, including a directory
shadowing an existing file's name. *(Location updated: originally
`core/adf/mutate.rs`, deleted when task 10 retired it — fixed, not reopened.)*
Tests: `a_name_that_differs_only_in_case_is_already_taken` (`dir.rs`),
`a_name_already_taken_is_refused_and_nothing_is_written`,
`a_directory_cannot_shadow_an_existing_file_name`,
`rename_onto_an_existing_name_is_rejected` (`core/volume/write/mod.rs`).

**ART-013** 🟠 **A file header could be used as a directory**
`core/volume/write/mod.rs` · Passing a file's header block as the insert
target wrote hash-table entries into it, corrupting the file.
→ `VolumeWriter::resolve_directory` verifies the block is `ST_ROOT` or
`ST_USERDIR` via `dir::is_directory`. *(Location updated: originally
`core/adf/mutate.rs`, deleted when task 10 retired it — fixed, not reopened.)*
Test: `writing_into_a_file_header_instead_of_a_directory_is_refused`.

### Security

**ART-014** 🟠 **The zip-bomb guard could be bypassed by integer overflow**
`core/lha/safe_extract.rs` · `total_written + header.original_size` was unchecked,
so an archive declaring a huge size could wrap the total past the limit.
→ `checked_add`; a size that cannot be added aborts the extraction.

**ART-015** 🟡 **Media paths could inject arbitrary WinUAE directives**
`core/winuae.rs` · `.uae` files are line-oriented and paths were written into
them unescaped, so a newline in a path became further configuration.
→ `checked_config_value` rejects line breaks.
Test: `line_breaks_in_paths_are_rejected`.

### Emulator integration

**ART-016** 🟠 **Only the first of several HDFs was reachable**
`core/winuae.rs` · Multiple hard drives produced repeated bare `hardfile=` lines
with no device names, and an invalid `hardfile_type{i}` key.
→ One `hardfile2=rw,DH<n>:…` line per image, per WinUAE `cfgfile.cpp`.
Test: `each_hardfile_gets_its_own_device`.

**ART-017** 🟡 **WinUAE detection used hard-coded drive letters**
`core/winuae.rs` · The candidate list included `D:\WinUAE\…`, left over from a
developer's machine, and assumed Windows lives on `C:`.
→ Paths come from the `ProgramFiles*` environment variables.

**ART-018** 🔵 **Concurrent launches clobbered each other's configuration**
`core/winuae.rs` · Every launch wrote the same `art_launch.uae` temp file.
→ The filename now carries a nanosecond stamp.

### Analysis

**ART-019** 🟠 **HD floppy geometry was never reported**
`core/analysis.rs` · The check read `size == 901_120 || size == 880 * 1024` —
the same number twice — so the HD branch was unreachable. Clippy's `eq_op` lint
caught this, but CI ran clippy with `continue-on-error`, so nobody saw it.
→ Explicit `DD_IMAGE_SIZE` / `HD_IMAGE_SIZE` constants, 11 vs 22 sectors.

### Build hygiene

**ART-020** 🟠 **CI hid real correctness errors**
`.github/workflows/ci.yml` · Clippy ran with `continue-on-error: true`, and
`lib.rs` carried a blanket `allow(dead_code, unused_imports, unused_variables,
unused_assignments)`. Between them, ART-019 stayed invisible for months.
→ Clippy is blocking; the blanket allow is narrowed to `dead_code`.

---

## Adding an entry

1. Take the next free `ART-NNN`.
2. State the defect as a claim, not a symptom — what is wrong, not what looked odd.
3. Name the file, and say **how it fails for a user**. If you cannot describe a
   way it hurts someone, it is a preference, not a defect.
4. Cite the spec section when a rule is broken.
5. When fixing it, add a regression test and name it here. A fix without a test
   is not fixed — it is untested.
