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

**ART-187** 🔵 **A cancelled Amiga-side install leaves the last phase line
on screen under a badge that says nothing about it** — *found 2026-08-21 in
Task 6's review, ruled shippable*
`src/components/osbuilder/AmigaInstallPanel.tsx` ·
`src-tauri/src/commands/amigainstall.rs`

Cancel an install and the panel keeps showing whatever the run last reported
— "Unpacking…", "Copying…" — beneath a badge that correctly claims nothing
about the copy. Stale, not false: the line is visibly a phase rather than a
verdict, which is why it was ruled acceptable rather than held.

It is filed because the fix is small and the right one: Rust reports every
other outcome and says nothing on a *successful* discard, so the screen has
nothing to replace the last phase with. A `report` there lets the badge fall
silent instead of leaving a sentence that has stopped being true.

Worth keeping in the same family as the three defects this round was really
about — a screen that keeps saying something after it stopped being so. The
sibling found the same day was worse and is fixed: the cancelled badge used
to assert *"the copy has been discarded"* over the top of ART's own *"could
not be removed"*, so the screen contradicted the core.

**ART-184** 🟠 **The test fixtures leak a scratch directory per run, for
ever, and filled a 2 TB drive** — *found 2026-08-20 when the suite began
failing with `StorageFull`*
`src-tauri/src/core/osinstall/mod.rs::fixtures::scratch` and every helper
shaped like it

**Measured, not inferred.** `%TEMP%` held **169,291** `art-*` directories
averaging **6.1 MB** — roughly **987 GB**, on a system drive with 70 MB left.
The oldest was stamped 01:49 and the newest 19:22 the same day, so this is one
session's output. Hundreds of tests then failed with
`Os { code: 112, kind: StorageFull }`, and every suite measurement taken that
evening was worthless until the cause was found.

Three things compound:

1. **Every run gets a new name.** `fixtures::scratch` builds
   `art-osinstall-{tag}-{pid}-{counter}`, then calls `remove_dir_all` on
   *that* name — a name that by construction has never existed. Nothing
   touches the previous run's directories.
2. **Not every test removes its own.** `core/osinstall/apply.rs` creates
   twelve and removes three. One measured run of its 49 tests left **41**
   directories behind.
3. **Nothing sweeps.** `core/osinstall`'s *production* code has
   `sweep_stale_preview_scratch_dirs`, which removes its own scratch
   directories after an hour. The test fixtures have no equivalent.

A missing `remove_dir_all` is also skipped whenever a test panics, so a red
suite leaks far more than a green one — which is the worst time for it.

**What would close it.** Not a bigger disk and not a manual clean: a fixture
that removes its directory on `Drop` rather than at the end of the happy path,
so a panicking test cleans up too, plus the same hourly sweep the production
side already has. Both are the shapes this codebase already uses elsewhere.

**Worked around, not fixed:** `src-tauri/.cargo/config.toml` (git-ignored,
machine-local) points `TMP`/`TEMP` at the project disk, because the owner's
standing rule is that nothing deletes from `C:` — so `C:` must not be where
this piles up. The leak is unchanged; it now accumulates somewhere that does
not stop the machine.

**ART-183** 🔵 **A misspelled key in a release recipe is still dropped in
silence** — *found while fixing the same hole in packages, 2026-08-20*
`src-tauri/src/core/osinstall/recipe.rs`

`package.rs` now refuses an unrecognised key **by name** (`serde(flatten)`
catch-all plus `check_unknown_keys`), because a recipe written
`"amigaInstaller"` instead of `"amiga_installer"` used to parse to `Ok(None)`
— silently dropped, and the symptom was "this package is not one this round
can run", a lie that reads like a design decision.

`recipe.rs`'s release recipes have exactly the same exposure and were left
alone deliberately: wave D's Task 2 does not touch them, and changing a parser
nobody measured is how a debt round becomes a defect round. The reasoning is
recorded in `RawPackage`'s doc comment, where the next editor of either file
will meet it.

Note `deny_unknown_fields` is **not** the fix here and was tried: every
shipped recipe carries `_why_…` documentation blocks, which it would reject.
Whatever closes this has to allow a leading `_` and refuse the rest.

**ART-179** 🔵 **Twenty-eight catalogue keys nothing renders** — *found
2026-08-20 by `src/i18n/dead-keys.test.ts` on the day that check was written
(ART-080 review, F2); allow-listed there rather than deleted*
`src/i18n/en.json`, `src/i18n/tr.json` ·
`src/i18n/dead-keys.test.ts::KEPT_WITHOUT_A_READER`

A dead key is worse than a missing one: the parity test is satisfied, both
languages agree, the string reads correctly to anyone grepping the JSON, and
the screen says nothing. `files.hostDelete.confirm` was exactly that — written
in both languages, never rendered, and a report claimed the user saw it. The
check that now exists to prevent a repeat found twenty-eight more on its first
run.

They belong to features outside the round that found them, and each was
checked by hand: none appears anywhere under `src/` in any form.

| feature | keys |
|---|---|
| the home screen's statistics panel | `dashboard.statistics`, `dashboard.noStats` |
| SD-2's per-distro note panel | ten under `distro.note.*` |
| artwork wave B | `artwork.enabled`, `artwork.outcome.cachedBefore_one/_other` |
| G10's empty states | `gameindex.empty`, `gameindex.noMatch`, `gameindex.statedBy` |
| commander chrome | `files.pane.copyTitle`, `files.pane.deleteTitle`, `files.pane.folderSuffix` |
| PiStorm card panel | `pistorm.card.configSets`, `pistorm.card.kernelFound` |
| preload screen | `preload.card.heading`, `preload.tool.heading` |
| miscellaneous | `app.name`, `common.continue`, `collection.status.indexed` |

**Why they were not simply deleted.** Removing another feature's translated
sentence — written in two languages, by someone, for a screen that was
designed — is not a debt round's call to make in passing. Several read like a
panel that was specified and then cut, and the right answer for those is
probably "build the panel", not "lose the strings".

**What would close it:** one pass per feature, deciding *render it* or *remove
it*, and emptying `KEPT_WITHOUT_A_READER` as it goes. The list is closed in the
meantime: a **new** dead key fails `dead-keys.test.ts`, which is the whole
point — that cannot happen again without someone adding a line to the
allow-list on purpose.


**ART-178** 🔵 **`useRemembered` hands back a fresh array identity when the
persisted value lands, so every effect that depends on one runs twice with an
identical request** — *found 2026-08-20 while measuring [ART-119](#fixed) #1 on
`debt-wave-c2`; filed rather than fixed there, because it is not that screen's
defect*
`src/lib/useRemembered.ts` · `src/lib/remembered.ts` · every screen that reads
an array or object through `useRemembered`

ART-119 #1 was "the OS Builder plans the same thing twice", and fixing it
halved the count. Measured, in `OsInstall.test.tsx`: the old code submitted
**4** byte-identical `osinstallPlan` requests for one settled render, and the
fix took it to **2**. The remaining factor of two is a different cause and is
this entry.

`useRemembered`'s read is asynchronous. On the first render it returns the
default — a **fresh** `[]` — and when the persisted value lands it returns
another array, structurally equal and referentially different. Any `useEffect`
listing that value as a dependency therefore runs a second time with a request
that is byte-identical to the first. For the OS Builder that is a full
re-plan: `plan()` opens and walks every switched-on component's disc image.

**Why this is not ART-119's to fix and not a one-line change.** It is not one
screen: `useRemembered` is how this project keeps its promise that nothing
changes unless the user changes it, and it is read for the component
checklist, the file manager's remembered paths, the collection's source list
and more. Memoizing inside `useRemembered` would make the identity stable but
would also make "the value has not arrived yet" and "the value is the default"
indistinguishable at the point of use, which is [ART-089](#fixed)'s own
hazard from the other side. Comparing structurally at each call site puts the
same reasoning in a dozen places. Neither is obviously right, and picking one
inside a debt round would have hidden the measurement that found it.

**What would close it:** a decision about where the stabilisation lives —
inside `useRemembered` (one place, but it has to keep "not yet loaded"
distinguishable) or in a shared dependency-comparison hook the screens opt
into — and then the measurement re-run. The count is the acceptance test: the
OS Builder's settled render should submit **1** request, not 2.

*Not data-unsafe.* Nothing is written twice; the duplicate is a read. What it
costs is real work on every keystroke in a field, on a screen whose plan reads
real disc images.


**ART-171** 🟠 **The content layer's spec §8.3 hazard — `WBStartup` and
`Devs` arriving on a tree for the first time — was never exercised, because
no package file ever reached a tree** — *filed 2026-08-19 by the final
whole-branch review (m4), deliberately not built there*
`src-tauri/src/core/osinstall/recipes/packages/boingbag-39-1.json` ·
`src-tauri/src/core/osinstall/apply.rs`

The design spec predicted it: a BoingBag's payload carries `WBStartup` and
`Devs` drawers that a base 3.9 tree may not have at all, so applying one is
the first time `apply` creates a **new top-level drawer** on an existing
tree rather than writing into one the release already made — including its
`.uaem` sidecar, its manifest rows, and whatever `S:User-Startup` expects to
find beside it.

Nothing about that was measured, and the reason is ART-166: both BoingBag
payloads are password-encrypted, so **not one file of either package has
ever been placed**. The synthetic tests write into drawers the fixture tree
already has; the one real run got as far as opening the payload.

This is the same bookkeeping [ART-159](#open) does for the *previous*
round's unexercised §5 hazards, and it is filed for the same reason: a
predicted hazard that produced no component, no test and no issue is
indistinguishable, six months later, from one that was handled.

**What would close it.** Not a synthetic fixture — one exists and proves
nothing about the real payload's shape. Either the Amiga-side install round
ART-166 names, or a package whose files ART *can* read that genuinely
introduces a top-level drawer (`locale-turkish` does not; it lands inside
`Locale/`, which `locale-base` already makes).

**The Amiga-side round succeeded on 2026-08-21 and still did not close it,
which settles what closing it means.** Both BoingBags installed
([ART-193](#fixed)) and BoingBag 1 really did create new top-level content on
the tree — `WBStartup/ASyncWB`, `WBStartup/BenchTrash`,
`Utilities/AMPlifier/…`, `Devs/NSDPatch.cfg-BB3.9-1`. **Every one of them was
written by the Amiga's own `Updater` inside the emulator, and none by
`apply`.** So the first of the two routes above is now known, by measurement
rather than by prediction, not to exercise `apply` at all: this needs the
second route — a package whose files ART can read that genuinely introduces a
top-level drawer — or an explicit test against a real payload's real shape.


**ART-172** 🟠 **The content layer's spec §8.4 hazard — a language pack
colliding with the base `Locale` — was never exercised either, and the run
that looked like it did was measuring a mangled name** — *filed 2026-08-19
by the final whole-branch review (m4), deliberately not built there*
`src-tauri/src/core/osinstall/recipes/packages/locale-turkish.json` ·
`src-tauri/src/core/osinstall/collide.rs`

The spec predicted that a language pack lands on top of catalogs the base
release already placed: `AmigaOS39.iso`'s own
`OS-Version3.9/Locale/Catalogs/türkçe` carries roughly the same ~34 catalog
names `BoingBag39-2-turkce.lha` updates, measured with 7-Zip, and
`locale-turkish` declares `overrides: ["locale-base"]` precisely because of
it.

The real run reported **0 rows of every collision class** — `rows=0
upgrade=0 downgrade=0 same-version=0 unversioned=0` — and that number is not
evidence that the collision does not happen. It is [ART-168](#fixed): the
drawer name arrived as `t<U+FFFD>rk<U+FFFD>e`, so the incoming files were
compared against a destination nothing had ever written, and every one of
them classified as *new*. **No comparison against the base `Locale` ever
took place.**

So the hazard stands unexercised. **ART-168 is now fixed (2026-08-20), so
this is the next thing to re-measure**, because the answer flips from "0 collisions" to "~34 of them" and the
`declared` column, the `overrides` declaration and the whole five-class
preview all get their first test against real material with a real overlap.

**Why this is filed separately from ART-168** rather than folded into it:
ART-168 is a defect in one function in `core/lha` with a known fix and a
known blast radius. This is a *verification* gap in the content layer, it
outlives that fix, and nothing in the record would otherwise say that the
cleanest-looking number the round produced was measuring the wrong thing.

**Re-measured 2026-08-21 and still not exercised.** The owner's built tree
`E:\amiga\ProjeART\dist-3.9-bb` was read directly: `locale-turkish`'s **36**
files sit under `Locale/Catalogs/t<U+FFFD>rk<U+FFFD>e` while `locale-base`'s
**597** sit under `Locale/Catalogs/TÜRKÇE`, and **not one of the 36 carries an
`overwrote` record**. That tree predates ART-168's fix, so the number is the
old one and the collision has still never happened. Closing this needs a tree
**rebuilt** with the fixed reader — the fix is in, the measurement is not.


**ART-166** 🔴 **Both BoingBag payload archives are password-encrypted ZIPs, so
neither BoingBag recipe can place a single file** — *found 2026-08-19 by Task
8's real run, on `content-layer`*
`src-tauri/src/core/osinstall/recipes/packages/boingbag-39-1.json` ·
`…/boingbag-39-2.json`

Both recipes name `member: "AmigaOS-Update"` — the payload archive stored
inside the wrapper LHA. That member is a ZIP, and every entry in it is
**ZipCrypto-encrypted**: 233 of 233 entries in BoingBag 3.9-1's payload
(210 files, 23 folders) and 147 of 147 in 3.9-2's (121 files, 26 folders).
Confirmed three independent ways — ART's own reader
(`entry 42 of this ZIP cannot be read: Password required to decrypt file`;
entry 128 for 3.9-2), 7-Zip 26.02 (`Encrypted = +`, `Method = ZipCrypto
Deflate`, and `ERROR: Wrong password` on extraction), and the raw local file
header, whose general-purpose flag word reads `0x0003` with bit 0 set.

The password belongs to the BoingBag's own `Updater`, which the wrapper LHA
carries beside the payload (`BoingBag3.9-1/C/Updater`, `C/GetLocale`) — an
Amiga executable that has to run *on* an Amiga. Nothing about this is a bug in
`core/archive`: the reader is right and the recipes are asking for bytes that
are not readable on the host.

Task 4 measured these recipes from the payload's **listing**, which ZipCrypto
leaves in clear, and every test since has been synthetic — so the first time
anything asked for the bytes was this run. That is exactly the gap Task 8
exists to close.

Left open, and deliberately not "fixed": circumventing the encryption is not
ART's business, and the honest options are all design decisions rather than
code changes — place the wrapper's loose files and let the Amiga's own
`Updater` run at first boot, or withdraw both BoingBag recipes until there is
a path that works. Whichever is chosen, spec §10/§89 says ART must not offer a
package it cannot apply, and today it offers two.

**The owner's decision, taken 2026-08-19 after external research and recorded
here so nobody re-opens it by accident: no bypass of the password will be
written.** The research is what settled it rather than taste — every
established distribution builder (HstWB Installer, AmiKit, AmigaSYS,
ClassicWB) installs a BoingBag by running the package's **own `Updater` inside
an emulator**, where the password already lives, rather than by decrypting
anything; HstWB's own README says so outright (*"HstWB Installer uses WinUAE
or FS-UAE emulator to run the installation process"*). So the supported path
exists and it is an Amiga-side one. That becomes **its own round**, not a
continuation of this one, and it is cheaper than it sounds because
`core/winuae::launch_winuae` already exists — what is missing is running it
unattended and reading the result back.

**What the screen does today, checked in the code rather than assumed
(corrected 2026-08-19 by the final whole-branch review's M3 — the sentence
that stood here said "the screen says so" and the screen did not).** When
this was first written, `osinstall_packages` set `available` from
`found.iter().any(|f| f.media == p.media)` alone, so a user with the real
`BoingBag39-1.lha` in the folder got a live checkbox, no warning, and — on
confirming — the reader's own raw English sentence, `entry 42 of this ZIP
cannot be read: Password required to decrypt file`, whatever language they
had chosen. The entry was right in its body about §10/§89 and wrong in its
last line.

Both are now true instead:

- Both BoingBag recipes declare `"host_placement_block": "encrypted-payload"`
  in their own JSON — data, not code, so the day the Amiga-side round lands
  the block is deleted rather than an `if` hunted down.
- The checklist gives such a package its own badge and its own sentence,
  in both catalogues, naming what it needs (*its own Amiga-side Updater*)
  rather than reporting that ART failed — and the row is **untickable**.
  "Archive not found" is deliberately not reused: the archive is right
  there.
- A pick remembered from an earlier run still arrives checked, so the
  preview is suppressed for that selection and the Add button is disabled;
  the row stays tickable *off*, which is F3's rule and is not weakened.
- `plan()` refuses the selection by type
  (`RefusalReason::PackageNotPlaceableOnHost`) before the package folder is
  even scanned, and `osinstall_collisions` refuses before opening an
  archive — so a caller reaching the commands directly gets the same
  answer, not the ZIP reader's.

So the shipped BoingBag recipes stay, unplaceable and **said to be** so.

Also worth recording as *my own* mistake rather than the material's: the
payload's 234 entries were listed with 7-Zip early in the round and read as
plain files. **ZipCrypto does not encrypt names** — listing works, extraction
does not — so the first fact was true and the second was inferred from it. The
first thing that ever asked for bytes was Task 8's run.

**The Amiga-side route works, and this entry is now the record of why the
host-side one still cannot** — *2026-08-21*. [ART-193](#fixed) is fixed, and
both BoingBags installed on the owner's own material through ART's own
`compose` → `install` path: BoingBag 1 in 169.1 s (3 795 → 3 859 files),
BoingBag 2 on that result in 138.1 s, and the tree booted and answered
`Workbench 45.3 (07-Dec-01)` where it used to answer `Workbench 45.1`.

**So the files reach a tree — and not one of them was placed by `apply`.**
The Amiga's own `Updater` writes them, inside the emulator, after the
package's own code decrypts its own payload. Nothing here decrypts anything
and no password is bypassed; the payload is as encrypted as it ever was, and
host-side placement is as impossible as it ever was. This entry therefore
stays open as the reason a BoingBag's rows are refused on the host screen —
`host_placement_block: "encrypted-payload"` is still correct and still
shipped. What has changed is that the sentence ART tells the user now has an
answer to give: the package installs, through the emulator, the way every
established distribution builder installs one.


**ART-159** 🟠 **Two of spec §5's three predicted hazards for AmigaOS 3.9 —
`SetPatch`/the boot sequence, and the three language-variant trees — went
untouched by every task on the branch and were recorded nowhere** — *found
2026-08-19 by the whole-branch review (findings 8 and its §5 notes); filed in
the fix pass, deliberately not built there*
`src-tauri/src/core/osinstall/recipes/amigaos-3.9.json` ·

The design spec named three hazards a 3.9 recipe would hit. One was met
honestly and filed (ART-157, the Kickstart minimum). The other two produced
no component, no test, no issue and no line in FEATURES/STATUS:

1. **`SetPatch` and the boot sequence.** The spec named the disc's own
   `First-Install` tree as carrying `c/SetPatch`, `loadwb`, `iprefs` and
   `mount`, and warned that "the boot path is part of what must be placed".
   The shipped recipe places `OS-VERSION3.9/WORKBENCH3.5/S` — the
   Startup-Sequence itself — and nothing from `First-Install`. A
   Startup-Sequence whose first line runs a `SetPatch` that is not in `C:`
   is the single most likely reason a 3.9 tree fails to boot when somebody
   finally tries, which is why this is High and not Low.
2. **Language variants.** `Locale`, `Locale.Euro` and `Special-Locale` are
   unaddressed. Whatever locale content lands does so incidentally, inside
   the `STORAGE` subtree. AmigaOS 3.2's real run already showed this is the
   class of mistake only a running system reveals (ART-127: a tree that
   built and verified clean, and was missing `icon.library`).

Not fixed here. Both belong to the recipe's second and third steps, which
the spec itself makes conditional on the boot test in §4 — and no 3.9 tree
has been booted (see FEATURES.md's 🟡 row). The point of this entry is that
"blocked on a boot" is a reason to record something as owed, not a reason to
leave it unrecorded: a hazard predicted before the work and untouched after
it should be visible in the register of what ART owes, beside ART-157, rather
than only in a review nobody reads again.

**Hazard 1 is now measured on a running system, and stays open only for
hazard 2** — *2026-08-21, the Amiga-side round's real run*. The tree really
does carry `C/SETPATCH` (placed by `workbench-base`) and
`Devs/AMIGAOS ROM UPDATE` (127 956 bytes), and running `C:SetPatch QUIET`
against it under the owner's Kickstart 40.68 demonstrably applies the update:
the booted system then answers `workbench.library 45.102` and
`version.library 45.1`, and its banner changes from *1985-1993
Commodore-Amiga* to *1985-2000 Amiga International*. So the boot path is
placed and works. What the entry above got right is that **nothing was
running it** — ART's own generated boot did not, which is
[ART-189](#fixed). **Hazard 2 — `Locale`, `Locale.Euro` and `Special-Locale`
— is still untouched**, and that is why this stays open.

**ART-130** 🔵 **A game can name the Kickstart it needs, and nothing offers to
supply it** — *filed 2026-08-17, out of G10's design round; the reading half is
built by G10, this is the half that was deliberately left out*
`src-tauri/src/core/gameindex/`, `src-tauri/src/core/rom/`, ROM Manager ·
A WHDLoad slave at `ws_Version >= 16` declares the Kickstart image it needs by
name (`kick34005.A500`, which WHDLoad loads from `DEVS:Kickstarts/`), by size
and by CRC16 — documented in whdload.de's autodoc under
`WHDLoad.Slave/--Overview--`. G10 reads all three fields, computes WHDLoad's
own CRC-16/ARC (`core/hashing`), and **reports** which declared images are
missing from the tree it is building. What it does not do is close the loop:
ART holds a 154-dump Kickstart table, verified against amitools' Remus database
on every CI run, so in many cases it could *identify* the needed image and
offer to place it under the name the slave asks for.

Left out on purpose rather than forgotten. Putting a user's ROM onto their card
on their behalf reaches ROM Manager, the licensed-Amiga-Forever decode path
([ART-128](#fixed)) and the card's own layout — decisions of theirs, not a side
effect of a metadata pass. It is the same question G9 answers for the OS side
("does this Kickstart suit this volume?") arriving from the games side, and it
belongs beside G9/G16 rather than inside a launcher-metadata round.

Design: [2026-08-17-g10-launcher-metadata-design.md](superpowers/specs/2026-08-17-g10-launcher-metadata-design.md) §6.

**Decided 2026-08-21 by the owner: yes, ART should offer it — but in its own
round, and always as a proposal.** Never as a side effect of a metadata pass,
and never placing a ROM without the user agreeing to that specific placement.
The reasoning that kept it out of G10 still holds and is the reason for the
shape: putting a user's ROM onto their card touches ROM Manager, the licensed
Amiga Forever decode path and the card's own layout, and those are the
owner's decisions rather than something a scan does on their behalf. So the
loop closes as *"this title asks for `kick34005.A500`; ART recognises it in
your collection — place it?"*, never as a silent copy.

**ART-118** 🟠 **The OS Builder's install screen has never been driven in a
real browser past its headings — jsdom now covers what a browser could not,
the crash itself is still unresolved** — *found 2026-08-15/16, Task 13's
browser pass and Task 14's real run; narrowed 2026-08-19*
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
verification.

**2026-08-19: `src/components/osbuilder/OsInstall.test.tsx` added — five jsdom
component tests, the first automated coverage of this screen at all.**
Mocked at the `@/lib/osinstall` / `@/lib/pistorm` / `@/lib/settings` boundary
(the house pattern — see `useRomPairing.test.tsx`), not deeper, and the real
component is rendered directly rather than a proxy harness. What is now
covered:
- The screen mounts **past its headings** with the media/ROM/destination
  fields, the 26-entry component checklist, and the Build and Verify actions
  all present and reachable — the thing no browser session could get past.
- The whole rendered tree carries no raw i18next key shape and no literal
  `{{…}}`, in **both English and Turkish** — the first time any language has
  been checked against a running instance of this screen (`ART-062`).
- Ticking a component in the checklist reaches the request `osinstallPlan` is
  asked to plan and changes what the plan section shows — the checklist's
  own wiring had never been exercised by anything before this.
- A refusal renders as the real, translated sentence, not a blank card.

What is still **not** covered, and why this stays open rather than closing:
jsdom does no layout at all, so it cannot reproduce the access violation
itself (a native renderer crash) or measure whether a long Turkish string
overflows its container — that half of `ART-062` is unchanged and still a
real-screen job. The crash's root cause is still unknown; a real
`pnpm tauri dev` pass by a human, driving the screen against a real media
folder (e.g. `E:\amiga\ProjeART\dist-3.2`), is still owed and is what would
actually close this.

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

**Decided 2026-08-21 by the owner: leave it. `hst-imager` stays the named
fallback for this one gap.** Editing an existing RDB in place is the kind of
operation that takes every partition on the card with it when it goes wrong,
and the geometry evidence says it would go wrong: `create_rdb_layout` assumes
16 heads / 63 sectors and a real CaffeineOS card is 12 / 256. There is no
measured demand for it either — the case is narrow (embedding a PFS3 driver
into a card **ART did not build**; ART's own cards already carry their
drivers, and FFS needs none because Kickstart carries it). The refusal names
`hst-imager` by name, which is what makes this a signposted boundary rather
than a dead end.

Revisit only if someone actually meets the case and `hst-imager` cannot serve
it — not before.

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

**ART-152** 🔵 ✅ **ART sized a WHDLoad launch's machine from the catalogue's
own chipset — the Amiga the *game* was written for, not the Amiga WHDLoad
runs on. Closed by one named, known-good WHDLoad machine profile instead of
the per-title `ws_ExpMem` reading it was filed suggesting** — *filed
2026-08-18 alongside ART-151; the owner ruled on the design 2026-08-21 and it
was built the same day*
`src-tauri/src/core/profile.rs::AmigaProfile::whdload_a1200` ·
`src-tauri/src/core/launch/mod.rs::machine_for_request` ·
`src-tauri/src/commands/launch.rs::profile_for_request`

## What was filed, and why it was not built

The original entry proposed reading `ws_ExpMem` out of each slave — WHDLoad's
autodoc (<https://www.whdload.de/docs/autodoc.html>, Overview) describes it as
"the expansion memory, an extra memory area which can optionally requested by
the Slave-structure, it may be Chip- or Fast-memory dependently on what is
available". The owner ruled against it: `ws_ExpMem` names what a slave *may*
request rather than a title's whole requirement, reading it would cost a
catalogue schema bump and a rescan of every user's collection, and it answers
a smaller question than the one that was actually wrong.

## What was actually wrong

The memory was the symptom; the **machine** was the defect. A WHDLoad launch
took its machine from `machine_for(catalogue chipset, user default)`, so an
OCS title planned an A500 — 68000, OCS, 512 KB Chip, 512 KB Slow — with
ART-151's Fast RAM bolted on top. But a WHDLoad title does not boot the game;
it boots AmigaDOS, which starts WHDLoad, which patches the game and then runs
it. Sizing that machine from the original game's chipset means as many launch
configurations as there are catalogue entries, each of which has to be right
on its own.

## The fix

One named profile, `AmigaProfile::whdload_a1200`: **68EC020, AGA, 2 MB Chip,
8 MB Fast, Kickstart 3.x** — the owner's decision, and the machine WHDLoad's
own community guidance is written around.
`core::launch::machine_for_request` routes every WHDLoad-shaped request to
`WHDLOAD_PROFILE_MACHINE` (`Machine::A1200`) whatever the catalogue says;
floppies and plain hardfiles still follow the catalogue exactly as before.
`DEFAULT_WHDLOAD_FAST_RAM_MB` is now *defined as* the profile's own
`WHDLOAD_PROFILE_FAST_RAM_MB`, so the shipped default, the profile and the
Settings control are one number rather than three that agree today.

**The user's Settings value is untouched.** `launch.whdloadFastRamMb` is still
remembered and still applied — as `.max()`, so raising it adds headroom and
lowering it never shrinks the profile's own 8 MB. The per-title machine picker
(`LaunchArgs::machine_override`) still outranks the whole decision.

**Sources, and what they actually say.** whdload.de's requirements page
(<https://www.whdload.de/docs/en/need.html>, read 2026-08-21) states only a
**floor** — 68000, "a minimum of 1.0 MiB RAM (sometimes more, it depends on
the installed program)", Kickstart 2.0 — and gives no recommended
configuration and no chip-versus-fast guidance at all. So this profile is
community consensus sitting well above a floor the page does state, and the
code says so rather than dressing it as vendor doctrine. The floor itself is
still enforced separately, on the ROM, by `WHDLOAD_MIN_KICKSTART_MAJOR` — an
A1200 profile does not silently accept a Kickstart 1.3 dump, and a user with
no 3.x ROM gets `NoRomMeetsWhdloadMinimum { machine: A1200 }`.

## What is *not* claimed

**Whether an OCS-only title runs as well here as on the A500 machine ART used
to pick has not been measured.** AGA is backwards compatible with OCS in
hardware and WHDLoad slaves are written to be installed on this machine, but
that is an argument, not a measurement. The one title ART has measured
(`1000 Miglia`, ART-151) reached its own logo on A500/OCS/68000 with 8 MB
Fast; it has not been run on this profile. The caveat is recorded in
`whdload_a1200`'s own doc comment, and the escape hatch is deliberate and
visible: the per-title machine picker puts a title back on an A500 for that
title alone, and the confirmation screen names the machine
(`preview.plan.machine`) before anything starts. There is **no** hidden
catalogue-derived fallback.

An earlier draft of this entry carried a claim that WinUAE's 68020 "is not
cycle-accurate and never will be". It was traced to a MiSTer FPGA forum
thread — an aside about a different platform — and **removed** rather than
written into the code: WinUAE's own release notes describe a maintained 68020
cycle-exact mode (4.3.0 removed its internal idle cycles as a refinement
toward real-world accuracy). Nothing supportable was found to replace it, so
nothing was written.

Regression tests, all failing against the pre-fix code:
`commands::launch::tests::a_whdload_launch_of_an_ocs_title_writes_the_68020_2mb_chip_8mb_fast_config`,
`commands::launch::tests::a_floppy_launch_of_an_ocs_title_still_writes_a_68000_ocs_config`,
`commands::launch::tests::a_per_title_machine_choice_still_beats_the_whdload_profile_in_the_config`,
`commands::launch::tests::a_raised_settings_value_reaches_the_generated_whdload_config`,
`commands::launch::tests::a_settings_value_below_the_profile_never_shrinks_the_generated_config`,
`commands::launch::tests::a_whdload_launch_refuses_a_kickstart_below_the_minimum_on_the_a1200`,
`core::launch::tests::a_whdload_request_ignores_the_catalogues_chipset_and_plans_the_a1200`,
`core::launch::tests::a_non_whdload_request_still_follows_the_catalogues_chipset`,
`core::launch::tests::the_default_fast_ram_is_the_whdload_profiles_own`.

The five config-level ones assert the **generated `.uae` text**, not the
profile struct: `cpu_model=68020` is derived from `CpuModel::M68EC020` and
`chipmem_size=4` is a 512 KB-unit conversion of 2048 KB, so the struct and the
file are two different claims. Whole-line matching, not `contains` —
`fastmem_size=8` is a prefix of `fastmem_size=80`. Eleven mutations were run
against them (the rule removed, the rule over-applied, the named profile
swapped for the Profile Studio's A1200 preset, `.max()` turned into an
assignment, the Settings value dropped, the machine guard dropped, the
Kickstart floor set to 0, and the profile's Fast RAM / Chip RAM / CPU /
chipset each changed); every one was caught.

**ART-193** 🔴 ✅ **A BoingBag's `Updater` started, printed nothing, opened
nothing and never returned — because ART's script had never run the tree's own
`AddDataTypes`** — *found 2026-08-21 by Task 7's run; its stated cause (the
missing AmigaOS 3.9 CD-ROM) falsified by Task 9's; cause found and fixed by
Task 10's bisection the same day*
`src-tauri/src/core/amigainstall/workvol.rs::startup_sequence` ·
`src-tauri/src/core/winuae.rs::real_version_hook::probe_script`

## What was wrong

The tree's own `S/Startup-Sequence` runs `C:AddDataTypes REFRESH QUIET`, which
registers the descriptors in `DEVS:DataTypes` with `datatypes.library`.
Nothing in ART's generated script had ever run it, so every install ART
attempted ran against a `datatypes.library` with an **empty** descriptor list
— a state a booted AmigaOS 3.9 is never in.

The owner's own `Updater` 45.15 then started, opened no window, printed not one
byte, wrote none of the 3 795 files and never returned. `Status`, run on the
machine while it hung, showed the process alive. Four runs across two rounds
ended by terminating the emulator, at 180 s, 400 s, 414 s and 1 200 s.

## How it was found, and what it cost to look in the wrong places first

**One line fixes it, and four plausible stories did not.** Each row is one
WinUAE run against a fresh copy of the same tree with the same package and
disc; *installed* means `S/Startup-Sequence-BB3.9-1` present on the Amiga and
the host tree grown from 3 795 files / 19 563 933 bytes to 3 859 / 20 135 854:

| Added to ART's script | Result |
|---|---|
| the tree's whole boot: assigns + `SetEnv Language` + `AddDataTypes` + `IPrefs` + `ConClip` + `Path` + `LoadWB` | **installed** |
| `REXX:`/`PRINTERS:`/`KEYMAPS:`/`LOCALE:`/`HELP:` + `SetEnv Language` only | hung |
| `LoadWB` only — Workbench really running, confirmed by `Status` | hung |
| `IPrefs` + `ConClip` + `LoadWB` (the set Task 9 tried) | hung |
| `AddDataTypes` + `IPrefs` + `ConClip` + `LoadWB` | **installed** |
| **`AddDataTypes REFRESH QUIET` alone** | **installed**, byte-identical result |

So a missing `LOCALE:` assign is not it, a missing Workbench is not it, and
`IPrefs` is not it. Task 9's negative on `IPrefs`/`ConClip`/`LoadWB` was
re-run here and is confirmed.

Two earlier hypotheses were also tested and are negatives with evidence:

- **Stack.** `Status` showed the `Updater` running on AmigaDOS's default
  **4 096** bytes. Raised to **65 536** (confirmed by `Status`: `stk 65536`),
  it hung exactly as before and wrote nothing.
- **The program never starts.** It does. Invoked with no arguments it returns
  at once and prints `Updater: required argument missing` — read on the host —
  so the binary loads, runs, parses and can write to a redirected stream.

## The fix

Two lines in `startup_sequence`, guarded exactly as ART-189's `SetPatch` is:

```text
  If EXISTS {sys}:C/AddDataTypes
    {sys}:C/AddDataTypes REFRESH QUIET
  EndIf
```

placed below `SetPatch` (which resets the machine, so anything above it runs
twice), below the `DEVS:` and `LIBS: … Classes ADD` assigns (the descriptors
live in one and the handlers they name in the other), and above the installer.
That is the tree's own order. `core::winuae::real_version_hook::probe_script`
gained the same lines, because an instrument that sets up a different machine
from the product answers questions about a machine nobody runs.

## What it produced

**Both BoingBags installed, through the product `compose` → `install` path, on
the owner's own material:**

| Package | End to end | Tree after |
|---|---|---|
| `BoingBag39-1 (1).lha` (`Updater` 45.15) | **169.1 s**, `Succeeded`, `Promoted` | 3 859 files, 20 135 997 bytes |
| `BoingBag39-2.lha` (`Updater` 45.19), on that result | **138.1 s**, `Succeeded`, `Promoted` | 3 868 files, 20 533 434 bytes |

and the result was **booted and asked**, not inferred:

```text
  Kickstart 40.68, Workbench 45.3 (07-Dec-01)
  version.library 45.3 (07-Dec-01)
  workbench.library 45.127 (21-Feb-01)
  resource.library 44.103 (28-Nov-01)
```

where the same tree answered `Workbench 45.1 (13-Nov-00)` before.

## What is *not* claimed

*Observed:* with the line the install happens and without it the `Updater`
hangs for ever — six runs, same tree, same package. *Not observed:* which call
inside the program blocks. Read from the binary, the `Updater` does
`ReadArgs("UPDATEFILE/A, TARGETDIR/A")`, then `LockPubScreen("INSTALLER")`
falling back to `LockPubScreen(NULL)`, `CreateMsgPort`,
`OpenCatalog("Updater.catalog")` and its `resource.library` window, all
**before** it opens `xadmaster.library` — so the wait is somewhere in that GUI.
Saying more would be a story rather than a measurement.

## The CD half, built in Task 9, and now measured rather than assumed

`LaunchMedia` carries a `cd_image_path` and emits `cdimage0=`, `scsi=true` and
`win32.map_cd_drives=true`; a recipe declares the medium and the request
supplies the file; the running Amiga reports
`CD0: 467M ... Read Only AmigaOS3.9`. Task 9 correctly reported that mounting
it changed nothing — because the machine never reached the check. **It does
now.** Run with the fix and *without* the disc, the `Updater` opens its window,
reaches its own *"Checking AmigaOS 3.9 CD-R…"* line, and AmigaDOS puts up

```text
  Please insert volume AmigaOS3.9 in any drive     [ Retry ] [ Cancel ]
```

which nobody is there to answer; the process stays alive and nothing is
written. So the refusal ART already emits without a disc is right for a
measured reason instead of an inferred one, and its sentence was rewritten to
say what actually happens.

Tests: `the_trees_own_datatypes_are_registered_before_the_installer` ·
`the_whole_script_is_what_it_is` (exact-match) ·
`commands::amigainstall::real_install_hook::install_a_real_package_when_asked`
(gated, `#[ignore]`d) · `core::winuae::real_version_hook::ask_a_tree_its_version_when_asked`
(gated, `#[ignore]`d).

**ART-192** 🟠 ✅ **The run built no `ENV:`, so a real installer stopped on a
System Request nobody could answer** — *found 2026-08-21 by Task 7's run
against the owner's own `BoingBag39-1 (1).lha`, fixed the same day*
`src-tauri/src/core/amigainstall/workvol.rs::startup_sequence`

Design §6's first hazard, met in the flesh. With ART-191 fixed the owner's
`Updater` 45.15 got past `resource.library` and put this on the emulator's
screen:

```text
  Please insert volume ENV in any drive     [ Retry ] [ Cancel ]
```

`startup_sequence` had assigned `SYS:`, `C:`, `S:`, `L:`, `LIBS:`, `DEVS:`,
`FONTS:` and `T:`, and deliberately no `ENV:` — its own note said so and said
why: *"If an installer turns out to need `ENV:`, the run will say so and the
fix belongs with that measurement."* That was the right call, and this is the
measurement it asked for.

Fixed with the four lines a real `Startup-Sequence` uses, in its order:
`MakeDir RAM:T RAM:Clipboards RAM:ENV RAM:ENV/Sys`, `Assign ENV: RAM:ENV`
(with `T:` and `CLIPS:` beside it), and `Copy ENVARC: RAM:ENV ALL QUIET
NOREQ` to populate it from the tree's own `Prefs/Env-Archive`. **`NOREQ` is
load-bearing**: a missing `ENVARC:` must return a code, not open a second
requester behind the first.

`T:` moved from `RAM:` to `RAM:T` in the same change. It pointed at the root
only to avoid a `MakeDir` whose return code could abort the script under
`FailAt 21`; with ART-188 that objection is gone, and the conventional target
is what an `Installer` writing to `T:` was tested against.

Test: `the_whole_script_is_what_it_is` (exact-match) ·
`commands::amigainstall::real_install_hook::install_a_real_package_when_asked`
(gated, `#[ignore]`d).

**ART-191** 🟠 ✅ **`LIBS:` carried only the tree's `Libs`, so
`resource.library` could not initialise and a BoingBag `Updater` refused to
start** — *found 2026-08-21 by Task 7's run, fixed the same day*
`src-tauri/src/core/amigainstall/workvol.rs::startup_sequence`

The owner's own `Updater` 45.15 ended immediately with

```text
  Cannot open "resource.library", version 44.
```

and every obvious reading of that sentence was wrong. The tree carries the
library (`Libs/RESOURCE.LIBRARY`, `$VER: resource.library 44.102
(29-Sep-99)`), so the version asked for is the version present, and `LIBS:`
was pointed at the tree's own drawer — `List LIBS: PAT #?resource#?`, run on
the Amiga through `core::winuae::real_version_hook`, listed it.

**The library says why itself.** Its printable strings name five BOOPSI
classes it opens — `gadgets/chooser.gadget`, `gadgets/clicktab.gadget`,
`gadgets/listbrowser.gadget`, `gadgets/radiobutton.gadget`,
`gadgets/speedbar.gadget` — which live in `SYS:Classes/Gadgets`. The tree
carries them and **nothing had put them on `LIBS:`**. A class that will not
open makes the library's initialisation fail, and a library that fails to
initialise is `OpenLibrary` returning `NULL`.

The tree's own `S/Startup-Sequence` does it in one line, and that line is now
in ART's: `Assign LIBS: {sys}:Classes ADD`. `ADD` rather than a second
assign, so `LIBS:` becomes both drawers in the order a real boot leaves them.

**The proof is the change in behaviour, not the diagnosis.** With that one line
added and nothing else changed, the same run stopped saying *"Cannot open
resource.library"* and got as far as [ART-192](#fixed)'s requester. Two
`Version` probes taken while diagnosing are deliberately **not** cited as
evidence either way: AmigaDOS `Version` falls back to reading a file when it
cannot open a library, so *OPENABLE at 44* was consistent with the library
being unopenable, and it briefly read as a contradiction.

**Why 116 unit tests could not catch it**: every one of them asserts what ART
*writes*, and the script ART wrote was internally consistent. What was wrong
was a fact about AmigaOS 3.9 that only a running AmigaOS 3.9 states.

Test: `the_whole_script_is_what_it_is` ·
`install_a_real_package_when_asked` (gated).

**ART-190** 🔴 ✅ **The re-run guard read a marker written above `SetPatch`,
and `SetPatch` reboots — so the installer was never invoked at all, and the
run was on its way to being reported as a timeout** — *found 2026-08-21 by
Task 7's run, fixed the same day*
`src-tauri/src/core/amigainstall/workvol.rs` ·
`src-tauri/src/core/amigainstall/mod.rs::INVOKED_FILE`

The guard existed for design §6's third hazard — *"a package that reboots the
Amiga … the work volume has to survive a reboot and not re-run the
installation on the second pass"* — and it was right about that reboot and
blind to the other one.

`If EXISTS ARTWork:art-result.txt` tested the file the script writes as
`started` near the top. Once ART-189 put the tree's own `SetPatch` into the
script, and `SetPatch` **resets the machine** after loading a ROM update — the
reason an AmigaOS 3.9 system appears to boot twice — the second pass found
`started` already written, printed *"this install already ran. Not repeating
it."* and stopped. Measured on the owner's own tree: the Amiga's screen said
exactly that, and the `Updater` had never been invoked. The host would then
have polled out its deadline and reported **timed out** for a run that was
never made.

Fixed by splitting the two questions the one file was answering.
`art-result.txt` stays what the host reads; a second marker,
`art-invoked.txt`, is written **below** `SetPatch` and directly above the
installer, and the guard reads that. A reset `SetPatch` caused leaves it
absent and the second pass carries on; a reset the installer caused leaves it
present and the second pass stops, which is the case the guard was written
for.

Test: `a_reboot_before_the_installer_lets_the_second_pass_do_the_work` ·
`a_second_boot_does_not_run_the_installer_again` ·
`the_host_polls_the_name_the_script_redirects_to`, which now accounts for
**every** redirection rather than only the ones it expected.

**ART-189** 🟠 ✅ **ART's generated boot never ran the tree's own `SetPatch`,
so an AmigaOS 3.9 tree met a 3.1 ROM** — *found 2026-08-21 while diagnosing
ART-191, fixed the same day*
`src-tauri/src/core/amigainstall/workvol.rs::startup_sequence`

AmigaOS 3.5 and 3.9 are a disk-based operating system over a V40 or older
Kickstart, and the thing that reconciles the two runs first in the tree's own
`S/Startup-Sequence`, ahead of every assign it makes: `C:SetPatch QUIET`,
which loads `Devs/AmigaOS ROM Update` (127 956 bytes in the owner's tree).
ART's script boots ART's own volume, so nothing had run it.

Fixed with `If EXISTS {sys}:C/SetPatch` + `{sys}:C/SetPatch QUIET`, below
every assign so `DEVS:` resolves, above the installer so the libraries it
opens are the updated ones. Guarded, because a tree carrying no `SetPatch`
has no ROM update to load, and a missing-command failure directly above the
installer would be a return code this script reserves for the installer.

**Reported honestly: this did not fix ART-191** — the class assign did. What
it demonstrably does is apply the update. With this line the booted tree
answers `Kickstart 40.68, Workbench 45.1`, `workbench.library 45.102`,
`version.library 45.1`, and its own banner changes from *1985-1993
Commodore-Amiga* to *1985-2000 Amiga International*. Whether a given
installer would fail without it has not been measured; what has is that the
tree is now in the state its own boot puts it in. It also brought ART-190
with it, which is the cost of the change and is recorded above.

This is also the first half of [ART-159](#open)'s hazard 1 measured on a
running system rather than predicted.

Test: `the_trees_own_rom_update_is_loaded_before_the_installer` ·
`the_whole_script_is_what_it_is`.

**ART-188** 🔴 ✅ **A return code of 900 aborted ART's own script before it
could report, so an installer that refused was on its way to being reported
as a timeout** — *found 2026-08-21 by Task 7's first run against the owner's
own material, fixed the same day*
`src-tauri/src/core/amigainstall/workvol.rs::FAIL_AT`

Design §6 named it: *"whatever writes it has to run even when the installer
fails, or a failure and a hang look identical."* The script set `FailAt 21`,
reasoned from the convention that AmigaDOS return codes are 0, 5, 10 and 20.

**The convention is not a rule.** The owner's own `Updater` 45.15 ended with

```text
  Cannot open "resource.library", version 44.
  ARTPkg:BoingBag3.9-1/C/Updater failed returncode 900
```

900 is far above 21, so AmigaDOS aborted the script at that line, the
`If Warn` branch never ran, and the work volume was left holding exactly
`started` — confirmed on the host by reading the file. The run then polled for
the remainder of its twenty-minute deadline; left alone it would have reported
**timed out**, *"nobody answered a question it asked"*, about an installer
that had answered immediately and in plain words.

Fixed by setting the fail level above any return code a program can express
(`FailAt 2000000000`, a `LONG`) rather than above the ones convention says it
will use. `If Warn` tests against `WARN` (5) and is unaffected by the fail
level, so a non-zero code still reports `failed` — it now reports it instead
of killing the script.

**Verified against the same material**: the identical run then ended in
16.0 s with `Failed`, not a timeout.

Test: `a_failing_installer_cannot_abort_the_script_before_it_reports`, which
now asserts the level against the measured 900 and names the number, so an
edit that lowers it back into range fails there rather than on a desktop.


**ART-186** 🟠 **Nothing enforced the BoingBag order, and nothing refused
the `Updater` that cannot run under an emulator** — *both established
2026-08-21 from the owner's own material and sources, fixed the same day on
`amiga-side-install`*
`src-tauri/src/core/osinstall/chain.rs` · `core/osinstall/apply.rs` ·
`core/amigainstall/packagevol.rs` · `recipes/packages/boingbag-39-1.json` ·
`src-tauri/src/commands/amigainstall.rs`

Two requirements found together, both by opening the material rather than
reasoning about it.

**1. The chain is mandatory.** A clean AmigaOS 3.9, then BoingBag 1, then
BoingBag 2, then optionally the community BoingBag 3 and 4 — which state
their own requirement as "AmigaOS 3.9+BB2". Nothing in ART enforced it: a run
of BoingBag 2 against a tree BoingBag 1 never touched was accepted, and the
result would boot and be quietly wrong. That is the same failure the AmigaOS
3.9 round already produced once, when a tree that booted cleanly turned out
to be 3.5.

`core/osinstall/chain.rs` reads the `distribution.json` the OS-install engine
already writes at the tree's root, and `compose` refuses a run whose
prerequisite is missing **before anything is copied** — naming what is missing
and in what order. A tree with no manifest at all is refused too, with its own
sentence: ART cannot say what is in it, which is a different fix for the user
than "install BoingBag 1 first".

**The other half, without which the refusal would have been worse than no
refusal.** A BoingBag cannot be placed from the host by any route (ART-166),
so BoingBag 1 never appears among a tree's file records and BoingBag 2 would
have been refused for ever on a tree that really had it. So a successful
Amiga-side run records itself: `DistributionManifest::amiga_installed`, a
`#[serde(default)]` list carrying only what ART can vouch for — which package
ran, and the AmigaDOS line it ran as. No invented `FileRecord`s: an Amiga
Installer is a program ART did not write and cannot supervise per file, and a
manifest claiming a provenance nobody measured is the very failure it exists
to prevent.

**2. BoingBag 1's `Updater` predates emulator support.** Measured with 7-Zip
26.02 against the owner's downloads in `E:\amiga\Amigatolon\os39`, and then
by reading each extracted file's own `$VER:` marker rather than its length:

| Archive | `C/Updater` | Dated | States |
|---|---|---|---|
| `BoingBag39-1.lha` | 25,588 | 2001-04-03 | `Updater 45.13 (3.4.2001)` |
| `BoingBag39-1-UAE.lha` | 25,732 | 2001-04-17 | `Updater 45.15 (17.4.2001)` |
| `BoingBag39-2.lha` | 42,676 | 2001-11-09 | `Updater 45.19 (9.11.2001)` |

The UAE archive's readme names the build and what it fixes — *"This archive
contains a file, Updater 45.15, that fixes the following problem: You can
install the BoingBag on UAE now."* — and says a download made after
2001-04-20 already carries it. The owner's is dated 2001-04-03. This round
launches that installer inside an emulator, so left alone BoingBag 1 would
fail, the script's `If Warn` would write `failed`, and ART would report that
the installer ran and refused — about a program that could not work (§89).

**The size is not the signal; the `$VER:` string is.** 25,588 bytes is
consistent with any build that happens to be that long, and reading a
coincidence as proof is how this project once shipped an AmigaOS 3.5 tree
under the name 3.9. `boingbag-39-1.json` declares `minimum_version: "45.15"`
and one overlay medium; `packagevol::unpack` takes a **list** of archives,
copies the declared subtree over the package's drawer, then looks for the
installer, then asks it what it is — and refuses an older or silent build by
name, saying which archive to supply.

**One thing the scouted design got wrong, and only reading the archive
found it.** `BoingBag39-1-UAE.lha` nests its payload one drawer deeper than
the package it patches — `BoingBag3.9-1-UAE\BoingBag3.9-1\C\Updater`, not
`BoingBag3.9-1\C\Updater` — so extracting it over the package with
`OverwritePolicy::Overwrite` would have written a parallel drawer and left
45.13 exactly where it was. Each overlay is therefore extracted into a scratch
directory of its own, through the same one gate, and the declared subtree
copied over the drawer through `safe_join` and `core::safety::atomic`.

Closed by **`boingbag_two_is_refused_on_a_tree_without_boingbag_one`** and
**`an_overlay_replaces_the_packages_own_updater_and_the_older_build_loses`**,
with `a_refused_chain_copies_nothing_at_all`,
`a_tree_with_no_manifest_is_a_different_refusal`,
`only_a_successful_run_records_anything`,
`an_amiga_side_install_is_recorded_and_read_back`,
`the_overlays_own_drawer_never_reaches_the_mount`,
`the_stock_updater_alone_is_refused_and_the_message_says_what_to_supply`,
`the_installer_is_looked_for_after_the_overlay_not_before`,
`an_overlay_destination_that_leaves_the_drawer_is_refused` and
`an_overlay_source_that_leaves_its_own_archive_is_refused` beside them. The
measurement itself is re-runnable:
`the_owners_real_updaters_state_the_versions_this_recipe_relies_on`, `#[ignore]`d
behind `ART_OS39_FOLDER`, reads all three real archives through the production
`unpack` and was run — 45.13, 45.15, 45.19, the stock archive alone refused,
the pair accepted.

Twenty-five mutations were put back. Twenty-three failed a named test; **one
survived the first round and was disclosed and fixed** — the overlay's
traversal test asserted only `is_err()`, and with `safe_join` swapped for
`Path::join` the copy landed outside while the *version gate* refused the run
a moment later, so the test passed against the defect it was written for. It
now asserts which refusal fired and that nothing at all was written outside.
**Two mutations are recorded as surviving on purpose**: the two `resolve`
calls inside `copy_over`, whose input comes from `std::fs::read_dir` and so
can never be a traversal. They are defence in depth, no test can reach them,
and `copy_over`'s own documentation says so rather than letting a reader
assume coverage.

**Fix round 1** closed a Major that was in neither half but in the gap
between them, and it is the same failure class a third time in two days: **a
true outcome reported as its opposite.** `record_amiga_install` cannot write
into a tree with no `distribution.json`, but `missing_prerequisites`
deliberately did not read the tree for a package that requires nothing — so
BoingBag 1 against a hand-made folder was an explicitly *permitted* run, the
installer would have **worked**, the recording would have failed, the copy
would never have been promoted, and the user would have been told the install
failed after it succeeded. ART-185 would have said "the installer ran and
refused" about a program that never started; the stock `Updater` would have
said it about one that could not work; this said "it failed" about one that
did the job. §89 forbids all three.

Fixed by closing the gap rather than patching a side: `applied` is now read
**unconditionally**, so "ART can account for this tree" and "ART can record a
success into this tree" are the same question asked once, and
`refuse_unless_prerequisites_met` was renamed `refuse_unless_installable`
because the old name made a manifest-less tree read as one with no
prerequisites to fail. The alternative — inventing a manifest when there is
none — was rejected for the reason that ruled out synthesising `FileRecord`s:
it would carry a `release` ART does not know and an empty `files[]` into
`verify`, `collide` and `apply::classify_incoming`, which today refuses
outright on a manifest-less tree because it cannot say what adding a component
would replace. That decision is **not observable from the outcome** — both
ways make the two halves agree — so it is asserted directly by
`recording_never_creates_a_manifest_for_a_tree_art_did_not_build`, without
which a mutation swapping one for the other left the suite green.

Closed by **`every_run_that_is_allowed_can_also_record_that_it_worked`** (the
property over five tree shapes, not one example) and
**`a_run_compose_accepts_always_reaches_a_promotion_when_the_installer_succeeds`**
(the ending that had no test at all), with
`a_package_that_requires_nothing_still_needs_a_tree_art_can_account_for` and
`boingbag_one_is_refused_on_a_tree_with_no_manifest` — the two tests that
asserted the **opposite** until 2026-08-21 — beside them. Nine composition
tests that had used a tree path pointing nowhere were given real distribution
trees in the same round: three of them asserted only `is_err()` and would
otherwise have gone on passing on the *chain* refusal instead of the guard
each is named for.

Still open from the same readme, and not this round's: UAE may not present the
AmigaOS 3.9 CD under the name the installer expects, whose documented
workaround is a manual `Assign AmigaOS3.9:`. Task 7 measures it.

**ART-185** 🔴 **Nothing mounted the package, so the installer would
never have started — and ART would have reported that it ran and refused** —
*found 2026-08-21 by wave D's Task 5 implementer, confirmed in code by its
reviewer, fixed the same day on `amiga-side-install`*
`src-tauri/src/core/amigainstall/run.rs::media_for`

`media_for` built exactly two `DirMount`s: the tree copy and ART's own work
volume. The generated script then emitted `CD DH0:BoingBag3.9-1` and
`DH0:BoingBag3.9-1/C/Updater`, which resolve only if the package sits inside
the distribution tree. For a BoingBag it never can — not being placeable on
the host is precisely what this round exists to work around (ART-166).

It would not have failed loudly. The command runs, `CD` fails, the shell
cannot find the program, the script's `If Warn` writes `failed`, and ART tells
the user **"the installer ran and said no"** about a program that never
started. §89 forbids claiming what is not there, and a confident wrong
sentence is worse than an error.

A **spec** defect, not an implementation slip: §3 said what to run and never
said where it runs from, and four tasks passed review because each was locally
correct. The design was amended on 2026-08-21 with a section of its own,
*"Where the package's own files come from"*.

**A third mount was necessary and not sufficient** — nothing anywhere unpacked
the plain-LHA wrapper to a host directory in the first place. Three things
landed together:

- `core/amigainstall/packagevol.rs` unpacks the wrapper through
  `core::archive`'s one security gate (`safe_join`, the output caps, the entry
  cap) into an empty scratch directory, and then **asks what arrived**: the
  package's drawer must be there and the recipe's installer must be inside it,
  or the run is refused by name before an emulator starts. **Nothing decrypts
  anything** — the wrapper is plain LHA and the ZipCrypto payload inside is
  copied out as an opaque blob for the Amiga-side `Updater`, which is this
  design's arrangement from the start.
- `RunRequest::package_volume_dir` and the third `DirMount`, under
  `PACKAGE_VOLUME` (`ARTPkg`), mounted as data at the tree's boot priority.
  `claims_package_volume` refuses a tree that would shadow it, and `media_for`
  refuses a package directory that is not there.
- `commands/amigainstall.rs::compose` roots the installer's path in
  `ARTPkg:` rather than the tree's volume, and the drawer defaults to the
  package's own recipe `media` instead of the volume root.

Closed by **`the_installer_is_reached_through_the_package_volume_and_not_the_tree`**,
with `the_package_is_mounted_as_its_own_volume_beside_the_other_two`,
`a_run_whose_package_was_not_unpacked_is_refused_before_launching` and
`a_wrong_archive_is_refused_before_the_tree_is_copied` beside it.

Sixteen mutations were put back and every one failed a named test. **Four
survived the first round**, all four for the reason this session keeps
finding: an assertion that the defect also satisfies. The drawer check was
covered only by "it errored", and deleting it left the *installer* check to
error instead; the traversal test aimed at a path that did not exist, so
`Path::join` refused it too; the boot-priority assertion compared a constant
with itself; and the ordering mutation was written as a no-op. All four tests
were rewritten to assert the property rather than the symptom — the refusal
now has to *list what the archive really held*, the traversal target is a real
furnished directory, and the priority is compared against ART's own volume.

**ART-182** 🟡 **A blocking-CI flake: sixteen tests shared one staging
namespace** — *reported by wave D's Task 2 implementer 2026-08-20, fixed the
same day on `amiga-side-install`*
`src-tauri/src/commands/osinstall.rs`

`staging_is_removed_however_the_preview_ends` counted directories matching
`art-osinstall-collisions-component-<pid>-`. `cargo test` runs the whole
binary in **one process** across many threads, and sixteen tests reach
`preview_component_collisions` — so the count picked up other tests' in-flight
directories, and the assertion compared two numbers that were never about the
same work. It failed **three runs in six** on the machine that found it and
**none in six** here — `cargo test -- --test-threads=1` passed
deterministically while the parallel suite did not, and six runs at the base
commit passed, which is what proved the race was in the test rather than in
the code under it — which is exactly why CLAUDE.md forbids shipping it: a
blocking CI that fails at random trains people to re-run until green.

`scratch_root_for` now names the thread as well as the process. The per-call
counter already made every root unique, so this is not about collisions — it
makes a directory left under `%TEMP%` after a crash attributable to the work
that left it.

The guard could not be "the suite is green now", because it was green here
before the fix. It is `two_threads_never_stage_into_one_namespace`, which
asserts the invariant the race violates — the same move as ART-181's frozen
clock. Removing the thread hash fails it every run.

**ART-181** 🔴 **Every user file in ART was written through a temp name that
two threads could share** — *found by wave D's Task 1 implementer while being
told not to add an eighth instance, fixed 2026-08-20*
`src-tauri/src/core/safety/atomic.rs`

`core::safety::atomic::temp_path_for` named its temp file with a bare
nanosecond stamp and opened it with a **truncating** `create`. Two threads
that read the same nanosecond wrote into one file and both renamed it over the
destination. This is the single path every write in ART goes through, so the
outcome is a corrupted ADF, HDF or config — not a flaky test.

Seventh instance of the counter defect this week (ART-164, ART-173, the
26-site sweep, `open_nested`, `launch_winuae`, the preview staging root) and
the **first in production code** rather than test scaffolding. The doc comment
was itself the false claim: *"Nanosecond stamp keeps concurrent writers from
colliding."*

The counter makes the name unique inside the process; `create_new` makes it
unique against anything outside.

Worth preserving: **the first two tests written for this both passed against
the defect.** A threaded stressor passed five runs out of five, and a
1000-call uniqueness loop passed because a real clock advances between calls.
Both were announced as the guard before being measured. The third works
because the clock became a parameter (`temp_path_at`) and the test holds it
still; both weaker ones are kept and labelled as stressors.

**ART-180** 🔵 **The dead-key allow-list could not tell an excuse from a
stale one** — *found 2026-08-20 while checking ART-179's own guard, fixed the
same day*
`src/i18n/dead-keys.test.ts`

`dead-keys.test.ts`'s third test is called "the allow-list has no stale
entries" and its comment says it catches an entry that *"has since gained a
reader"*. It asserted something else: that the key was still in the catalogue.
Proved by giving the allow-listed `common.continue` a real reader in
`CopyPlanDialog.tsx` — **all three tests passed**, and the entry's written
reason ("a generic affirmative no dialog in ART uses") was now false with
nothing noticing.

The first half of that test file is sound: a genuinely new dead key is caught
by name. This was the half that was not. Both are asserted now.

Same shape as ART-181 and the Task 2 Major: **a comment claiming more than its
assertion does.** Found by mutating the guard, not by reading it.

**ART-080** 🔵 **ART cannot delete a file on the user's own disk, so nothing
can be moved *off* a host folder** — *found in phase 2b task 3; **the owner
decided it 2026-08-20** and it was fixed the same day on `debt-wave-c2`
(fix round 1)*
`src-tauri/src/core/hostfs.rs` (new) ·
`src-tauri/src/tools/recycle_bin.rs` (new) ·
`src-tauri/src/commands/panel.rs` · `src/lib/panel.ts` ·
`src/lib/movePlan.ts` · `src/pages/FileManager.tsx`

This was never open for want of work. It was open on one question — **where a
deleted host file goes** — and the owner has answered it: **the Windows
Recycle Bin.**

**The reasoning, because it is the part worth keeping.** ART invents no
recovery mechanism of its own and uses the one the operating system already
has — the one place a user already knows to look. That beats a `.art-backup/`
directory beside the file, which nobody discovers and which duplicates a
multi-gigabyte ISO in order to move it, and it beats a permanent delete, which
nobody can undo.

**The architectural consequence, and it was not optional.** Sending a file to
the Recycle Bin is `IFileOperation`, a Windows API, and `core/` is
platform-independent and must not call one. So it takes the shape this project
already uses for exactly this: **`core/hostfs.rs` declares the
`HostRecycler` trait** and carries every rule about which files may be named
and what a partial failure has to say; **`tools/recycle_bin.rs` implements
it**, outside `core/`, and knows only how to hand one path to the shell. That
is `core::preload::VolumeFormatter` and `tools/hst_imager.rs` again, and both
doc comments say so by name. The trait's own doc comment says *why* it is a
trait rather than leaving the next reader to infer it.

**What it refuses rather than guesses.**

- A **folder and names, never paths**. `recycle_many` resolves each through
  `safe_join`, so nothing the frontend sends — however assembled — can name a
  file outside the folder the pane is showing. A name that escapes, or one
  that is not there, refuses the **whole** pass before one file is touched.
- A **drive root** is still refused (`files.move.refuseLocalRoot`). `C:\` is
  where `Windows` and `Program Files` live, and the two confirmations a user
  learns to click through for a game are the same two here.
- A volume with **no Recycle Bin** — a network share, some removable media —
  is a failure ART reports by name, never a silent fall back to a permanent
  delete. Falling back would be ART picking, on the user's behalf, the one
  option the owner ruled out.

**It is not all-or-nothing, and the outcome says so.** `delete_many`'s
guarantee is real because a disk image has a journal; a host filesystem has
none. Twelve files sent one by one are twelve completed operations and the
thirteenth failing cannot undo them. Claiming otherwise would be a promise ART
cannot keep (§89), so `HostDeleteOutcome` carries **every name and what became
of it**, and the screen names the ones that did not go — "eleven of twelve" is
not something a user can act on; the twelfth's name is.

**And it says where the file went.** A delete the user cannot find is the same
as one they cannot undo, so the destination is in every sentence that reports
a removal — and only in those: nothing removed names nowhere. `RecycleTarget`
is a value the UI translates, not an English string from Rust (ART-060), and
both catalogues carry it.

Previewed by the confirmation the screen already shows before a move, and
logged through `commands/oplog.rs` with the folder, the count asked for, the
count removed and the count that failed — a partial result is the one a log
most needs to carry, and it is recorded as `verified(false)` rather than a
plain success.

Tests: eight in `core::hostfs::tests`, all against a fake recycler because
that is what the trait is for —
`a_partial_failure_says_exactly_what_went_and_what_did_not`,
`a_recycler_that_claims_success_and_leaves_the_file_is_not_believed` (the
outcome is verified against the filesystem, not against the recycler's word),
`a_name_that_escapes_the_folder_refuses_the_whole_pass`,
`an_absolute_path_is_refused_the_same_way`,
`a_name_that_is_not_there_refuses_before_anything_is_touched`,
`cancelling_reports_what_had_already_gone`, `nothing_removed_names_nowhere`;
six in `src/lib/hostDelete.test.ts` for the sentence, including that the
catalogue string really interpolates `{{target}}` rather than the parameter
being carried and never used; and four new `planMove` cases.
`tools::recycle_bin::tests::a_real_file_really_goes_to_the_real_bin` drives
the actual shell and is `#[ignore]`d, because it puts something in the
machine's real Recycle Bin — a side effect outside the tempdir every other
test confines itself to.

**Vacuity checked**, and the first attempt was wrong in the way ART-144 #5
had just taught: both traversal tests called `.unwrap_err()` before their
filesystem assertion, so removing `safe_join` failed on the unwrap and the
security claim was never reached. Reordered, and with the guard removed they
now fail by name:

```
an unguarded join recycles exactly this: C:\Users\...\art-hostfs-escape-target.adf
assertion failed: bin.seen.borrow().is_empty()      # kernel32.dll was attempted
```

**Fix round 2 — what a review found in it.** Four Majors, all of them the
same shape in different places: a rule stated in one layer and enforced in
another, or a report saying more than it knew.

- **The drive-root refusal lived only in TypeScript** (F5). `movePlan.ts`
  refused it and `panel_delete_many` validated only `is_dir()` — so
  containment was relative to a root the *caller* chose, and the command is
  reachable without the screen. `core::hostfs::refuse_drive_root` is the rule
  now, called first inside `recycle_many` (before containment, since
  containment is relative to the parent) **and** on the command thread before
  a job starts. The screen keeps its copy as an early answer, never as the
  guarantee. `a_drive_root_is_refused_before_anything_is_resolved` covers
  `C:\`, `C:`, a UNC share root and `/`;
  `the_root_refusal_comes_before_the_containment_check` pins the order.
- **The confirmation never said where the file goes** (F2).
  `files.hostDelete.confirm` was written into both catalogues and **nothing
  rendered it** — so the sentence naming the Recycle Bin was never on screen,
  and this entry claimed it was. Rendered now, as its own question before the
  copy half. And because a dead key satisfies every check ART had, a new one
  was added: `src/i18n/dead-keys.test.ts`. It fails on
  `files.hostDelete.confirm` the moment the render is removed, and it found
  twenty-eight more on its first run ([ART-179](#open)).
- **A cancelled delete reported a success it did not have** (F1). A twelve-name
  request stopped after three had three rows, all successful — so the log said
  `verified(true)` and the screen said "3 item(s) went to the Recycle Bin",
  which is also what a *complete* three-file delete says.
  `HostDeleteOutcome` carries `asked` and `cancelled` now, `complete()` is what
  the log records, `untouched` is logged separately from `failed` (a name never
  attempted is not a name that failed), and the screen has its own sentence:
  *"Stopped. 3 of 12 went to the Recycle Bin; the other 9 are still there."*
- **The oplog's destination was a literal** (F10). `"Recycle Bin"`, typed, so a
  second recycler would have had the first one's destination logged against it.
  It comes from `outcome.target.log_label()` now, and is `-` when nothing was
  removed.

Two minors taken with them: a host-to-host move was **planned as allowed and
then refused by the page after both confirmations** (F8) — refused in
`planMove` now, because a refusal a plan can reach has to be reached in the
plan; and `sameImageAsEachOther` compared the image path alone (F9), so `DH0:`
to `DH1:` of one HDF — two volumes that share a file, and the commonest move
on a real PiStorm card — was refused as a relink. It compares `volumeIndex`
too.

**What is guarded and what is not** is now written into `core/hostfs`'s module
doc rather than left to be discovered (F7): `..`, absolute, UNC and drive
prefixes are refused for the whole pass; `safe_join`'s containment is
**lexical**, so a symlink or junction inside the folder whose target is outside
it passes — bounded by the shell recycling the *link* rather than its target,
which is a lost shortcut and not a lost tree; `exists()` follows links; and
there is a check-to-call window, which is why each entry's result is asked of
the filesystem afterwards rather than assumed.

**The dependency**, since it is the first ART has taken that touches the
user's own files: `trash` 5.2.6 (MIT), `default-features = false` to drop
`chrono` — which it needs only to *read* the bin back, something ART never
does. Its four new transitive crates are MIT or MIT OR Apache-2.0, so
`cargo deny check` passes with no exception (`advisories ok, bans ok, licenses
ok, sources ok`), unlike `libpfs3`, which needed one. Recorded in
`THIRD_PARTY_LICENSES.md` in the same commit, as that file's own rule
requires.

**ART-081** 🟡 **A single file cannot be moved between two images, because
the primitive underneath addresses a directory** — *found in phase 2b task 3;
fixed 2026-08-20 on `debt-wave-c2`*
`src/lib/movePlan.ts` · `src/lib/deletePlan.ts` (new) ·
`src/pages/FileManager.tsx` · `src/lib/volumeWrite.ts` ·
`src-tauri/src/commands/volume_write.rs`

The copy half was already built ([ART-064](#fixed), [ART-176](#fixed)); the
**delete** half is what kept this open, and the decision was in the sequencing
rather than in either half. Both are settled now, and the ruling behind them
is one sentence: **a single-entry operation goes through the route the batch
uses, never a second path with its own promises.**

**What the second route cost.** `volume_delete` committed per call;
`volume_delete_many` accumulates the whole set into one `BlockSet` and one
journalled commit that rolls back whole on either write strategy
([ART-073](#fixed)). So the *commoner* case — one file — took the weaker
guarantee. The place that hurt most was F8 on a file that has a Workbench
icon: the screen deleted the file, **then** asked about `Turrican.info`, then
deleted it in a second committed operation. A failure between the two left the
file gone and an icon that opens nothing — the exact §7.1 clutter the icon
question exists to prevent. The icon question is asked before anything is
deleted now, and both names go in one batch. `volume_delete` is deleted, root
and branch: the Rust command, its `invoke_handler` registration and its TS
wrapper, the same way wave C1 removed the copy path's second route.

**What F6 can do now.** `planMove`'s two "between two images" refusals are
gone — a selection between two images is a batch like any other, files
included. The sequencing that made this a decision is unchanged and is what
makes it safe: the destination is **re-listed** and every name looked for
before a single delete runs, so the worst a failed move can leave is a
duplicate, never a gap. What the batch adds is that the delete cannot
half-happen either.

**A hazard the lifted restriction created, and its guard.** F6 could now reach
a move between two directories of the **same image** — which is a relink, not
a copy-and-delete: doing it the long way stages the tree out, writes it back
into the same volume and then removes the original, needing twice the free
space and losing the only copy if it fails between the halves. F5 has always
refused this (`files.err.sameImage`); F6 could reach it and did not, which
only stopped mattering because F6 was restricted to one directory. `planMove`
takes `sameImage` and refuses on it.

`files.move.refuseFileBetweenImages` and `files.err.batchBetweenVolumes` are
removed from both catalogues: both said "not supported yet", and both are now
false.

Tests: `src/lib/deletePlan.test.ts` — nine, of which
`"puts the file and its icon in ONE batch — ART-081"` is the defect itself and
`"a one-entry selection is exactly a one-entry batch"` is the ruling; and
`src/lib/movePlan.test.ts`'s `"allows several entries between two images —
ART-081"`, `"allows a lone file between two images — ART-081, and by the same
route"` and `"refuses a move between two directories of the *same* image"`.
Vacuity checked both ways: dropping the icon from the batch fails one, and
removing the same-image guard fails two (the second in
`phrase-keys.test.ts`, which enumerates every refusal `planMove` can produce).

**The other half of the original ask** — moving *off* a host folder — was
still refused when this landed, because ART could not delete a file on the
user's own disk. The owner decided that the same day: a host file goes to the
Windows Recycle Bin, and [ART-080](#fixed) is fixed too. The two share their
sequencing exactly — copy, re-list the destination, look for every name, and
only then remove the source — and differ in the one place they must: an image
delete is all-or-nothing because it has a journal, and a host delete reports
per entry because it has none.

**ART-175** 🟡 **The OS Builder can preview what a package would replace and
still cannot preview what switching a recipe component on would replace** —
*found 2026-08-20 by the wave-B1 review (F3); fixed 2026-08-20 on
`debt-wave-c2`*
`src-tauri/src/commands/osinstall.rs` (`preview_component_collisions`,
`osinstall_component_collisions`) · `src/lib/osinstall.ts` ·
`src/components/osbuilder/OsInstall.tsx`

[ART-170](#fixed) made `collide::preview` able to answer for a release
recipe's component, and nothing asked it. This is the ask.

**What had to be built, and why it is not the package shape.** A BoingBag is
previewed against a tree that already exists, so `preview` has real files to
compare and a `distribution.json` saying which component owns each. A release
component has neither: `apply` is `SAFE_CREATE` and refuses an existing root,
so what `workbench-39` would replace is not a file on disk at all — it is
`workbench-base`'s own item, in the same plan, not yet written anywhere.

So the tree is **staged, and only the part that is actually in the way**: for
each destination the previewed component claims that an earlier component in
the same plan also claims, the earlier component's bytes are written into a
scratch root at that destination, with a `distribution.json` naming its owner.
That is exactly what `classify_incoming` needs to read both sides honestly and
what `declared_override` needs to answer — and it is a few dozen files rather
than the six hundred a fully staged tree would cost. `collide::preview` itself
is untouched.

**A defect the work found, in itself.** "Earlier" first meant "not in the
selection", which made previewing the *base* component report it as
**downgrading** `C/Format` from 45.1 to 44.5 — the overlay, which writes
after it, staged as though it were already there. A component that writes
later is not in the way; it is on top. Now one pass in plan order: an
unselected item records itself as the current owner, a selected item takes
whatever owner had been recorded by then.
`a_later_component_is_not_what_is_being_replaced` is the test that caught it.

**A claim in this entry that turned out to be wrong, corrected rather than
repeated.** This entry said `workbench-39` is "the only one in shipped data
that layers over another". It is not: AmigaOS **3.2** has four of its own —
`extras`, `modules-a1200`, `classes`, and `glowicons`, which layers over four
components at once. Found by the recipe-parity test failing on its first run
against the real JSON; the assertion now pins all five, and the doc comments
that had repeated the claim say so.

**What the screen shows.** `ComponentSummary` gained `overrides`, so the
screen can ask about exactly the switched-on components that can be in
another's way — previewing all of them would mean reading a whole AmigaOS
install off media to answer a question about a few dozen files. A "What this
would replace" section above the plan's own file list, with the same grouped
rows `PackagePanel` already uses, in both catalogues. It states **placed, new
and replaced** rather than only the collision rows, because an empty report
for a component that places six hundred files means "nothing is in the way",
not "nothing happens" (§89) — and a preview that *failed* says so rather
than looking like one that found nothing.

**Fix round 2 — two Majors a review found in it.**

- **Two previews of the same thing corrupted each other** (F3). The staging
  root was a *deterministic* hash of the plan and the components, and the
  preview `remove_dir_all`s it on entry — so two concurrent previews shared a
  root and the second wiped the first's staged files. The first then found
  nothing at the destination and **every Replace row degraded silently to
  "new"**: a wrong preview, in the screen a user reads before ticking the
  component that decides which operating system they end up with. And it is not
  a rare interleaving — [ART-178](#open) makes the plan effect settle twice
  with an identical request, so two concurrent identical previews are the
  *normal* case. The root is per call now (process id plus a counter that never
  repeats — the sixth instance of that class in this codebase and the first
  outside a test), and a `StagingDir` guard removes it however the call ends,
  which also stops staged AmigaOS bytes outliving a cancelled preview (F6).

  *A note on the test.* The threaded reproduction **passes against the defect**
  — the two previews are fast enough that one often finishes before the other
  clears the directory — so it is not the guard. The guard is
  `two_previews_never_share_a_staging_root`, which fails deterministically the
  moment the root goes back to a hash.

- **"New" counted identical files as new** (F4). `collide::preview` drops
  `Identical` rows before returning — its own rule, and a good one — so
  `placed - reports.len()` calls a file landing byte-for-byte on another
  component's copy *new*. On AmigaOS 3.9's overlay that is **130 files**, and
  it is exactly why this preview's numbers and the 622 that
  `layer_the_real_39_overlay_when_asked` measured off the real disc could never
  have matched. `ComponentPreview` carries `contested` now, so the three counts
  partition what would be placed — `new = placed - contested`,
  `unchanged = contested - reports.len()`, `replaced = reports.len()` — and the
  screen says all three. The census hook counts the same way and asserts the
  sum, so the two hooks are comparable rather than merely both printed.

**Still true afterwards**, and recorded in ART-170's entry already:
`Libs/WORKBENCH.LIBRARY` carries no `$VER:` marker, so 3.9's replacement of it
(193,400 → 199,852 bytes) classifies as `Unversioned` — a size comparison —
while being the single change that turns `Workbench 44.5` into `Workbench
45.1`. The preview shows the file and cannot say what the change is.

Tests: six in `commands::osinstall::tests` —
`switching_a_component_on_previews_what_it_would_replace` (the issue itself,
asserting an `Upgrade` read off both files' own `$VER:` off real ADFs, and
that a file landing on nothing produces no row while still counting in
`placed`), `a_component_that_declared_no_override_cannot_be_reported_as_declaring_one`,
`a_component_that_replaces_a_file_with_the_same_bytes_reports_nothing`,
`previewing_no_components_opens_no_media`,
`a_later_component_is_not_what_is_being_replaced` and
`a_cancelled_component_preview_stops`; the recipe-parity test
`"declares its overrides where the screen can see them (ART-175)"` in
`src/lib/osinstall.test.ts`; and four jsdom tests in `OsInstall.test.tsx`
covering the request that goes out, the rows that come back, the silence when
nothing layering is on, and the failed-preview sentence. Vacuity checked:
stubbing the screen's layering detection to `[]` fails three of the four.

**ART-143** 🟡 **A hand-attached picture is not re-materialised if the
artwork cache is deleted** — *filed 2026-08-18 (collection-wave-c's own design
§2 promised this and no task built it); fixed 2026-08-20 on `debt-wave-c2`*
`src-tauri/src/core/artwork/rebind.rs` (new) ·
`src-tauri/src/commands/artwork.rs` · `src/lib/artwork.ts` ·
`src/pages/CollectionStudio.tsx`

`ArtBinding::chosen` — the file the user originally picked — was kept, per its
own doc comment, "so the binding can be re-materialised if the cache is ever
cleared", and nothing ever read it back. The artwork cache is a **sibling** of
the catalogue directory precisely so a user can delete 1.6 GB of pictures and
keep the index that took minutes to build; doing so lost every hand-attached
picture, with the exact file it came from still named in the override right
beside it.

**What it is now.** `core/artwork/rebind.rs`: for each title the screen is
showing that carries an `art` override, if the cache has no entry — **or has
a row whose file is gone**, which is what deleting the pictures and leaving
`index.json` produces — read `chosen` and put it back. A job
(`artwork_rebind_manual`), because a full cache deletion can mean hundreds of
file reads and `atomic_write`s (§54), cancelled only between whole bindings,
with the index saved before `Cancelled` is returned so what finished is
genuinely finished. Run from `loadArtwork` on every catalogue load rather than
behind a button: there is nothing for the user to decide, the choice is
already theirs and already recorded, and a pass over an intact cache is one
metadata call per binding and no writes. That is why it is not
[ART-132](#fixed)'s rule — nothing is fetched and nothing leaves the machine.

**The measured fact from the collection round is the load-bearing one.** The
cache does **not** normalise internally: `Cache::store`/`Cache::get` take
whatever key they are handed, and every reader folds through
`core/artwork/key.rs::normalise` first. A pass that skipped the fold once
wrote 242 pictures under keys nothing read. So the title arrives from the
screen as the screen holds it — the *applied* title, which is what
`attach_picture` was given — and is folded here, exactly once.

**What it refuses rather than guesses.** A `chosen` file that has gone, is no
longer a PNG/JPEG, has grown past `MAX_PREVIEW_BYTES`, or cannot be read is
reported as a typed `RebindProblem` and **left alone** — the override is not
deleted, because the drive may simply not be plugged in today, and quietly
discarding the user's choice is the one outcome "nothing changes unless the
user changes it" forbids. Size is checked on the metadata, before the read.
The screen names each one, with the file it looked for, in both catalogues.

Tests: nine in `core::artwork::rebind::tests` —
`a_deleted_cache_is_rebuilt_from_the_files_the_user_chose` is the issue
itself, `the_picture_lands_under_the_key_the_screen_reads_by` is the 242-
pictures rule, `an_intact_binding_is_left_exactly_as_it_was` is what makes
running it on every load acceptable, `a_row_whose_file_has_gone_is_rebuilt_too`
is the half-deleted cache, `one_missing_source_does_not_stop_the_others`,
`an_oversized_source_is_refused_by_its_size`,
`a_source_that_is_not_a_drawable_picture_is_refused` and
`cancelling_keeps_what_it_already_restored`; plus
`commands::artwork::tests::a_hand_attached_picture_survives_the_artwork_cache_being_deleted`,
the round trip through the real `attach_picture` override, which is the join
the unit tests cannot see. Vacuity checked: replacing `normalise(&binding.title)`
with the raw title fails **six** of them, the command-level one included.

**ART-144** 🔵 **Five minors deferred across collection-wave-c's own review
rounds, folded into one entry — all five now closed** — *found 2026-08-18; #4
closed by the whole-branch review; #1, #2, #3 and #5 closed 2026-08-20 on
`debt-wave-c2`*
`src/lib/collectionDetail.ts` · `src-tauri/src/commands/artwork.rs` ·
`src/lib/artwork.ts` · `src/components/collection/TitleDetail.tsx` ·
`src-tauri/src/core/launch/extract.rs`

**#1 — already true, and checked rather than assumed.** `mediaPhrase`'s
`default` arm is gone (it went with the WHDLoad reclassification, `acf73b5`);
it has three explicit cases like its sibling `mediaKind`. Verified by adding a
fourth `Media` variant and compiling: `tsc` reports **both** functions —
`src/lib/collectionDetail.ts(13,44): error TS2366` and
`src/lib/gameindex.ts(301,42): error TS2366` — "Function lacks ending return
statement and return type does not include 'undefined'." Nothing to change.

**#2 — the backup path is surfaced instead of discarded.** `set_override`
writes through `core/safety`'s `guarded_write` and returns where it preserved
the previous `overrides.json`; `artwork_attach` and `artwork_detach` dropped
it, which made them the only override writers in the collection that did
(`catalogue_set_override` has always returned it). They return `AttachOutcome
{ art, backup }` and `DetachOutcome { backup }` now, and `TitleDetail` shows
`collection.detail.art.backedUp` — both catalogues — with the path. That file
holds every correction the user has ever made to their catalogue, so
rewriting it silently is precisely the case CLAUDE.md's rule exists for. The
note clears on the next action and on every title change, so it can never
describe a different game's write.

**#3 — `loadPictures()` takes a ticket.** Two round trips with nothing
serialising them, so clicking down a list faster than the artwork cache
answers let a slow reply for the *previous* title land last and paint another
game's box art — no error, nothing to retry. A request counter rather than an
`AbortController`, because there is nothing to abort: `invoke` has no
cancellation, so the work happens either way and the only question is whether
its answer is still wanted. The `catch` arm is guarded too — a stale
*failure* blanking the current title is the same bug wearing the other face.

**#5 — the traversal test asserted something that could not fail.** It
unpacked into `dir/out` and checked `!dir.join("evil.adf").exists()`, but
`../../` from `dir/out` resolves to `dir`'s **parent** — so the assertion
looked where an unguarded join would never write. It bit only because
`.unwrap_err()` panicked first, which is a different claim. Two things
changed: the destination is two levels down, so the escape lands inside the
test's own scratch directory and is checkable; and **the filesystem is
asserted before the error is**, so removing `safe_join` now fails the security
line by name rather than the unwrap. Verified: with `safe_join` replaced by
`into.join(name)` the test fails with `an unguarded join writes exactly here:
C:\Users\…\art-launch-unpack-traversal-…\evil.adf`.

Tests: `core::launch::extract::tests::an_entry_that_escapes_the_destination_is_refused`
(#5, rewritten — the vacuity check above *is* its proof);
`commands::artwork::tests::a_successful_attach_leaves_the_picture_in_the_cache`
asserts the first attach reports `backup: None` and the second reports a path
that is really on disk (#2, Rust side); and a new
`src/components/collection/TitleDetail.test.tsx` — five jsdom tests, the
panel's first automated coverage — covering #2 on screen (attach, detach, and
saying nothing when there was nothing to back up) and #3 both ways (a late
success and a late failure for the previous title). Vacuity checked: removing
the two ticket checks fails 2, removing the two `setArtBackup` calls fails a
different 2.

**ART-119** 🔵 **Five minors deferred from Task 13's review, folded into one
entry — all five now closed** — *found 2026-08-15/16; #3 and #4 closed
2026-08-16; #1, #2 and #5 closed 2026-08-20 on `debt-wave-c2`*
`src/components/osbuilder/OsInstall.tsx` · `src/lib/osinstall.ts` ·
`src-tauri/src/core/osinstall/plan.rs` · `src-tauri/src/core/osinstall/mod.rs`

**#1 — the screen planned the same thing twice.** The two-plan design is
right and stays: a plan requested *with* a component excluded never carries it
in `componentsOn`, so one call cannot answer both "is this condition
satisfied" and "is this excluded". But with nothing excluded the two requests
are byte-identical, and `plan()` opens and walks every switched-on component's
disc image, so the second call was a full re-plan thrown away — on every
keystroke in the media, ROM and destination fields. One call now answers both
when the requests match; both are still made the moment they differ. Safe
because nothing mutates a `PlanResult` and nothing compares the two by
identity, and it does not move *when* a round trip happens — the remaining
call is in the same effect, the same tick, on the same dependency change.
**Measured, not reasoned about:** reverting to the old `Promise.all` makes
`OsInstall.test.tsx`'s `renderFull` submit **4** requests, all four
byte-identical; it submits **2** now.

*Found while measuring, and left alone deliberately:* the remaining factor of
two is a different duplication — `useRemembered` hands back a fresh array
identity when the persisted value lands, so the effect settles twice with the
identical request. That is dependency-identity churn across the screens that
use `useRemembered`, not this screen's two-plan design, and pinning it to this
entry would have hidden it. Recorded here and in the test's own comment.

**#2 — four independent guards became one exhaustive `switch`.** A fifth
`ConditionalReason` kind would have rendered *nothing at all* — a conditional
row ticked with no explanation, which is the defect a review already found on
this screen once. `conditionalReasonText` in `@/lib/osinstall` is a `switch`
over the union with a `never` fallthrough, so a fifth kind is a compile error;
it lives in `src/lib` rather than the component so its keys are visible to
`phrase-keys.test.ts`, which is the only thing that catches a `Phrase` pointing
at a key nobody added. Four literal `t("…")` sites traded for one dynamic one;
`literal-keys.test.ts`'s ledger records 114 → 115 and why.

**#5 — an unreadable disk is a refusal now, not a `CoreError`.** `plan()` did
`open_media(found_media)?`, so one damaged disk failed the whole plan. The OS
Builder made that worse than it sounds: it asks for two plans through one
`Promise.all`, one deliberately with nothing excluded, so a disk the user had
*already excluded* blanked both plans — including the one it was excluded
from — and the screen showed a raw English `CoreError` sentence instead of a
refusal card (ART-060). New `RefusalReason::MediaUnreadable { component,
volume_name, path, reason }`, the third face of the per-component fact
`MediaMissing` and `MediaPathMissing` already carry. It still blocks the
install; it no longer takes the screen with it. Both catalogues carry
`osinstall.refusal.mediaUnreadable`.

*Stated rather than implied:* an unexcluded plan's `items` are empty either
way — **any** refusal empties the preview, which is `plan.rs`'s own
pre-existing rule and applies to `MediaMissing` identically. What changed is
that there now *is* a plan, carrying a named refusal, rather than no plan.

*The fixture is a real gap, not a contrived one:* `identify` reads a disc's
name off its volume descriptor and stops (ART-161) while `open_media` walks the
tree, so a disc past `MAX_WALK_DEPTH` (ART-158) is genuinely found, genuinely
named from inside itself, and genuinely refused when read — the same fixture
`scan.rs`'s `a_disc_is_identified_from_its_descriptor_without_walking_its_tree`
already uses to prove that gap exists.

Tests: `core::osinstall::plan::plan_tests::an_unreadable_disk_is_a_refusal_and_excluding_it_still_plans`
(#5 — vacuity checked: restoring the two `?`s makes it panic on the
`LimitExceeded` the walk raises);
`commands::osinstall::…::refusal_reason_tag_and_field_spellings_for_every_variant`
(the wire spelling, and its exhaustive `match` is what caught the new variant
at compile time);
`OsInstall.test.tsx`'s `"asks once for a request shape it has already asked
for"` and `"still asks twice when the two requests genuinely differ"` (#1);
`phrase-keys.test.ts`'s `"conditionalReasonText: every ConditionalReason
variant resolves"` (#2), which also asserts all four kinds are reachable from
`conditionalReason` itself rather than enumerating a union the screen cannot
produce.

**ART-174** 🔵 **Two more breakpoints ask the real viewport a question about
the zoomed layout** — *found 2026-08-20 on `debt-wave-a`; fixed 2026-08-20 on
`debt-wave-c2`*
`src/pages/CollectionStudio.tsx` · `src/pages/FileManager.css` ·
`src/lib/dockLayout.ts` · `src/pages/FileManager.tsx` ·
`scripts/zoom-check.py`

Both rules were `@media (max-width: 1000px)` and both sat inside `.app-shell`,
which carries `zoom` — so each asked about a viewport its layout does not live
in, exactly as [ART-101](#fixed) did. They are fixed differently because they
are not the same question.

**The Collection Studio's detail split** is the shell's own question, so it
gets the shell's own answer: `.app-shell-narrow .collection-with-detail`.
`shellWidthClasses` already computes that class from `innerWidth / zoom` and
already carries the identical 1000 px threshold (`SIDEBAR_ICONS_BELOW`), so the
grid now stacks exactly when the sidebar collapses — which is what the original
rule meant by 1000 px. Verified in headless Chrome at a 1400×900 window,
driving the real Ctrl+= gesture rather than setting the variable by hand:
`100% → shellClass=wide cols=712.656px 356.344px`, `180% → shellClass=NARROW
cols=669.236px` (one track). The old rule could not have fired at any
Application Size there — the real viewport is 1400 px throughout.

**The commander's row is a different question**, which is why the original
entry said so. Its columns are `em`, and the screen has a *second* zoom of its
own (Ctrl+wheel over the listing, `@/lib/dockLayout`), so "does this row still
fit" has two inputs and a media query can see neither. `scripts/zoom-check.py`
grew a `--files` pass that measures the pane in **`em` of its own text**, and
the threshold is that measurement rather than a number picked near it: at the
Application Size that puts the shell at exactly the old rule's 1000 px
(zoom 2.575 of a 2575 px window) one pane measures `pane=355px em=12
pane_in_em=29.58`, so `PANE_NARROW_BELOW_EM = 29.6` — the next tenth above the
last narrow width. `paneWidthClasses` puts `tc-commander-narrow` on
`.tc-commander` and the stylesheet's rules moved under it.

**A second measurement changed the implementation.** A `ResizeObserver` alone
does not see an Application Size change: asked directly, the browser reported
`after zoom=2 offsetWidth=496 fires=[]` — the pane went from 1131 px to 496 px
and the observer said nothing. An observer-only fix would have left the
commander wide at exactly the sizes this issue is about. So the observer stays
(window resizes, sidebar collapse) and the Application Size gets its own
re-read on the next frame.

End-to-end in headless Chrome, both real gestures, 2575×1407:

```
start                      zoom=1   pane=1131 class=wide   fnkeyLabel=block
after 6x Ctrl+=            zoom=1.6 pane=653  class=wide   fnkeyLabel=block
+ 10x Ctrl+wheel (text up) zoom=1.6 pane=648  class=NARROW fnkeyLabel=none
back to 100%, text big     zoom=1   pane=1127 class=wide   fnkeyLabel=block
```

The third line is the case no media query could ever have reached: same
2575 px window, same pixels, and the row is genuinely out of room.

**Still true afterwards, and recorded rather than fixed:** the wide row's fixed
columns total 34.9 em, so at 29.6 em they have already stopped fitting — the
agreed breakpoint is late, not early. Moving it changes *when* the screen
degrades, which is a design decision and not this issue's, so it is left. It is
written into `PANE_NARROW_BELOW_EM`'s own doc comment so the next person meets
it there.

Tests: `src/lib/dockLayout.test.ts` — six new, of which
`"degrades at exactly the point the media query it replaces did"` pins the
measured boundary (355 px narrow, 356 px not) and
`"sees the case the media query was blind to: the text grew, not the window"`
pins the case the old rule could not answer. Vacuity checked both ways:
stubbing `paneWidthClasses` to always-wide fails 2, to always-narrow fails 4,
and the six together fail under one mutation or the other.

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

**Decided by the owner 2026-08-20 (wave-C1 fix round 1): ART does not fetch
it, and this is closed as answered rather than left as owed work.**

ART keeps doing the part the user cannot do for themselves — reading the
card's `Emu68.img` `$VER:` string, naming the archive that belongs on that
board and saying which release line it comes from — and then stops. There is
no download button and there will not be one.

**The boundary, written down so it is not re-opened by accident.**
`net/http_mirror.rs` refuses cross-host redirects on purpose (§41.5.7): a
followed redirect is a fetch the user never configured, and the guarantee that
ART only ever reaches configured mirrors is worth exactly as much as that
refusal. A GitHub release asset redirects to `objects.githubusercontent.com`,
so fetching one means either relaxing that rule or writing a second client
with a weaker policy beside it. Relaxing it for convenience would remove the
reason it exists, and a second client with its own rules is the same
relaxation wearing a different name.

The other two obstacles recorded above stand as further reasons rather than as
the deciding one: an archive name means a different board in each release line
([ART-091](#open)), so a fetch that resolved "latest" without the line would
be that defect with a network connection; and unpacking an archive onto a card
that boots somebody's machine is a multi-file write wanting the same
preview → backup → verify every other write in ART has.

Not deferred, not "later" — this is the answer. If it is ever revisited it
should be revisited as a decision about §41.5.7, not as an afternoon's work on
a screen.

**ART-176** 🔵 **F5 between two images means two different things for one
entry and for several** — *found 2026-08-20 while closing ART-064*
`src/pages/FileManager.tsx::copyTo` · `src-tauri/src/commands/volume_write.rs::copy_between_volumes`

Copying **several** entries between two images now keeps each one's own name:
`Game/` and `Readme.txt` arrive as `Game/` and `Readme.txt`, the shape
`HostSelection` gives the local→volume direction and the one
`volume_copy_between_many` was built to match.

Copying **one** folder between two images does not. `copy_between_volumes`
stages `from_dir`'s *contents* and copies those into `to_dir`, so F5 on
`Tools` lands `Editor` and `Readme` loose in the destination directory and no
`Tools` drawer at all. That is the behaviour
`a_tree_copies_between_two_images_through_the_command_pipeline` has asserted
since phase 1a — it is tested, not accidental — and it is also what
[ART-081](#fixed) means by "F5 on a lone file … copies the whole folder the
file happens to be in", seen from the folder end.

So the two paths disagree, and the disagreement is new: before ART-064 the
multi-entry case simply refused, so there was nothing to disagree with.
Nothing is lost either way and nothing is destroyed — F5 deletes nothing — but
a user who selects one drawer and a user who selects two get different shapes.

Not fixed here, and deliberately: making the single-entry case keep the
drawer's name is a **product** decision about what F5 means between two
images, it contradicts a test that was written on purpose, and it is the same
call ART-081 has to make about a lone file. Both should be answered in one
pass, by whoever owns that decision, rather than settled as a side effect of
a batching task.

**Decided by the owner and fixed 2026-08-20 on `debt-wave-c1` (fix round 1):
the drawer is preserved.** `Games/Turrican/Turrican.slave` arrives as
`DH1:Games/Turrican/Turrican.slave` whether one row is marked or ten.

Fixed by **removing the second route** rather than by teaching it to agree.
`volume_copy_between` and `copy_between_volumes` are gone; there is one
command between two images now, `volume_copy_between_many`, and a single pick
is a one-entry batch. Two routes through one operation cannot give two results
if there is one route.

The frontend's F5 and F6 single-entry paths both call it with
`selectedEntriesForBatch([entry])`.

**It also fixes the lone-*file* case**, which was the same defect wearing
different clothes: F5 on one file passed the pane's own `dirBlock` and copied
the whole folder that file happened to be in. It now copies the file.

`a_tree_copies_between_two_images_through_the_command_pipeline` asserted the
old flattening and was right about the code at the time; it is replaced by
`commands::volume_write::tests::one_folder_between_two_images_keeps_its_drawer`
and `…::one_file_between_two_images_copies_that_file_and_nothing_else`.

**ART-177** 🔵 **A layout apply still cannot be resumed — the residue is
reported, and there is no way to carry on from it** — *found 2026-08-20 while
closing [ART-110](#fixed), which this is the undecided half of*
`src-tauri/src/core/layout/apply.rs` · `src/pages/ContentLayout.tsx`

A run that fails part way now says so and names what landed
(`ART-APPLY-PARTIAL`), and the screen no longer stays busy. What it still
cannot do is carry on: there is no skip-existing, no resume, and the next
preview reports the residue as ordinary collisions — true, and unhelpful,
because they are collisions with the user's own interrupted run.

Nothing is destroyed. `place()` refuses to overwrite, so a retry fails loudly
rather than replacing anything, and the only way forward is the file manager.

Fixing it means answering a question ART has not answered anywhere else: what
should a preview *say* about a destination that already holds exactly what
this plan would put there? "Already done, skip it" is only safe if ART can
tell that the file on disk is the file this item would write — same bytes, not
merely the same name — and for an `UnpackWhdload` item that means comparing a
whole extracted drawer against an archive. "Collision, refuse" is what happens
today and is honest. "Overwrite" contradicts §93 and the applier's own stated
rule. Which of those the screen offers, and whether resuming is a mode or a
per-row choice, is a product decision.

Recorded rather than answered, because answering it as a side effect of a
debt-clearing pass is how a safety rule gets changed by accident.

**Decided by the owner and fixed 2026-08-20 on `debt-wave-c1` (fix round 1):
skip, and say so.** A destination that already holds **exactly what this plan
would put there** is stepped over, and the preview counts it. Re-running a
half-finished apply therefore resumes by itself — no "continue" button, no
resume mode, no new state to get out of step.

`core/layout/presence.rs` answers the one question that makes it safe, and its
rule is the whole design: **when ART cannot be sure, the answer is
`Different`.** A false "already in place" is a file the user asked for that
silently never arrives; a false "different" is a collision they have to look
at. Only the second is survivable, so every branch ends in `Different` unless
it has positively established sameness.

| placement | "the same" means |
|---|---|
| `CopyFile` | same length, then **byte for byte**, streamed in 64 KB chunks |
| `CopyTree` | the same relative paths both ways, no extras either way, every file compared as above |
| `UnpackWhdload` | the archive's **entry list** — every entry inside the pack has a file at the matching path whose length is the entry's declared size, and the drawer holds nothing else. No decompression. |

That last row uses a declared size, which ART treats as an adversarial claim
everywhere else. It is one here too; what differs is which way a lie pushes
the answer. A lie that makes the check **fail** costs a collision report. A
lie that makes it **pass** causes ART to write nothing at all, leaving what is
on disk untouched. Neither writes attacker-chosen bytes, which is why the
cheap check is right here and wrong at the extraction gate.

**Nothing is overwritten, and that has not moved** (§93). `place()` refuses a
destination holding anything else, exactly as before; what changed is that it
recognises its own output. The check is re-asked at apply time rather than
taken from the plan, because the plan was computed before the confirmation and
the disk can have moved on in between.

**The screen says three numbers**, and the third is a guarantee rather than a
statistic: `new 847 · already in place 12 (skipped) · overwrites 0`. ART never
overwrites, so `overwrites 0` is what that promise looks like on screen. The
result panel adds "N were already in place and were left alone", so "nothing
happened" and "it was already done" never read the same.

`layout_recheck` now returns both answers from one walk (`RecheckResult`),
because they are the same question asked of the same paths — returning only
collisions is how the screen came to call a stopped run's own output a clash.

Covered by `core::layout::apply::tests::re_running_a_half_finished_plan_finishes_it`
(plan, place one item by hand, re-plan, and assert **no collisions**, one
`already_in_place`, then `placed: 1, skipped: 1`),
`…::a_destination_holding_something_else_is_still_refused` (which asserts the
bytes on disk are untouched), and six unit tests in
`core::layout::presence::tests` — of which
`a_different_file_of_the_same_length_is_different` is the one that matters
most: a check that stopped at the size would call a different disk image of
the same size "already done" and silently never copy the real one.

**Fix round 2 (wave-C1 re-review, G1 — Critical): the first implementation did
not keep the rule this entry states, and the review broke it three times.**

The rule was right and the code inferred certainty from **length**:

- A WHDLoad drawer with the **right lengths and the wrong bytes** was judged
  already-in-place. `same_pack` compared the archive's *declared* sizes
  against what was on disk and skipped on a match.
- **Two trees differing only by an empty directory** compared equal, because
  the map `walk` built held files alone.
- A resumed apply **never restored a missing `.info`** — presence looked at
  the drawer and not at the icon ART-106 had just made a destination in this
  same batch — so a resume produced a tree Workbench cannot see and reported
  it finished. §82, reached from the resume side.

All three are fixed by comparing **content, never size**:

| placement | what is compared now |
|---|---|
| `CopyFile` | length as a cheap *reject*, then byte for byte |
| `CopyTree` | the same entries both ways, **directories included**, then every file byte for byte |
| `UnpackWhdload` | every entry inside the pack **decompressed** and compared against the file on disk, in one forward pass through `read_selected` (index-at-a-time is quadratic on a solid 7z) |

A declared size is now used for one thing only: bounding the read. Where the
content cannot be read in full — an archive that will not open, a file that
will not read — the answer is `Different` and the item is **written**. That
asymmetry is the whole design and is now stated at the top of the module: a
wrong `Different` costs one unnecessary write; a wrong `AlreadyInPlace` leaves
a wrong file on the user's volume and tells nobody.

`Presence` gained a fourth answer, `IconMissing`: the drawer is exactly right
and its `.info` is not there. Not a collision — nothing is in the way — and
not settled either, so it counts as work on screen and `apply` writes the icon
alone (`Parts::IconOnly`), leaving the verified drawer untouched. An icon that
is present and is *somebody else's* is `Different`, because writing over it
would be an overwrite (§93).

The review's three cases are the three new tests:
`core::layout::presence::tests::a_drawer_with_the_right_lengths_and_the_wrong_bytes_is_different`,
`…::two_trees_differing_only_by_an_empty_directory_are_different`,
`…::a_drawer_whose_icon_is_missing_asks_for_the_icon_and_not_a_collision`,
plus `…::a_drawer_whose_icon_is_someone_elses_is_different`,
`…::an_archive_that_will_not_open_is_different`, and the end-to-end
`core::layout::apply::tests::a_resumed_apply_restores_an_icon_the_first_run_never_wrote`.

Mutation-checked against all three: restoring the shape-only pack comparison,
dropping directories from the walk, and treating a missing icon as
already-in-place fails exactly the corresponding tests (5 failed / 57 passed);
restored, 62 pass.

**Fix round 3 (wave-C1 re-review): measured, and it is a job now.**

Comparing content is not free, and "if it proves too slow" is how a freeze
ships. Measured on the owner's own collection —
`E:\amiga\Amigatolon\WHDload`, **1 697 WHDLoad HDFs, 3.74 GB**, release
build, by `core::layout::tests::layout_plan_timing_over_a_real_collection`
(checked in and `#[ignore]`d, so the number can be re-run rather than
re-trusted):

| | items | time |
|---|---|---|
| first plan, nothing at the destination | 1 702 | **797 ms** (2 804 ms with a cold file cache) |
| apply | 1 698 placed | 58 978 ms |
| plan over a staging tree that already holds it — the **resume** | 1 702 | **138 898 ms** |

81.6 ms an item on the resume, because every destination that exists is read
in full and matched against its source: 3.74 GB read twice. Two and a quarter
minutes on the command thread is a frozen window, and even the *first* plan at
797 ms is past the point where §54 applies.

So `layout_plan` is a job (`spawn_job`), like `archives_plan_install` before
it (ART-066): it reports progress per item, the plan arrives on
`layout-plan-result`, and a cancelled or failed preview releases the screen
through `onJobProgress` exactly as a failed apply does. `plan_with` and
`settled_in_with` take a `&dyn ProgressSink` with thin `NoProgress` wrappers —
the `scan_collection_directory` / `_with` shape — and **cancellation is
checked between whole items, never inside a comparison**, which would mean
deciding on a half-read file.

The comparison was not weakened to buy the time back. A cheaper one is how a
wrong file gets skipped, which is the defect G1 was filed for.

Covered by `core::layout::tests::a_plan_stops_when_asked_and_between_whole_items`
(and it asserts the staging root does not exist afterwards: planning writes
nothing, cancelled or not). The measurement lives in `plan_with`'s own doc
comment as well, so the next person meets the number rather than the worry.

**A second folder, for shape rather than for the headline:**
`E:\amiga\Amigatolon\paketler` — 5 652 items, 0.93 GB of mostly small
files — planned in 9 417 ms cold and 7 550 ms on the re-plan, 1.3–1.7 ms an
item either way. The resume cost is dominated by **bytes**, not by item count:
3.74 GB of HDFs costs 81.6 ms an item and 0.93 GB of small files costs 1.3 ms.

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

**Fixed 2026-08-20 on `debt-wave-c1`.** `src/pages/FileManager.test.tsx` is the
first test that renders the real page. The mock is the one the rest of this
suite already uses — the `@/lib/*` wrappers around `invoke`/`listen`, never
`@tauri-apps/api` itself — plus `@tauri-apps/plugin-dialog`,
`@crabnebula/tauri-plugin-drag`, and `@/lib/settings` one layer further down,
for the reason `OsInstall.test.tsx` records: `useRemembered` writes through
`saveSettings`, the real `tauri-plugin-store` boundary, which rejects in jsdom
with nothing to catch it.

The page was rendered rather than split, on the evidence that a mock of that
surface turned out to be about eighty lines.

What it establishes, which is what the entry above asks for:

- **The screen mounts, with two real panes and a real listing in each** —
  `useSettingsStore` is seeded `loaded: true` with both default folders set,
  because the cold-start effect is gated on the settings having arrived
  (ART-089) and would otherwise leave both panes empty.
- **Both write-result listeners are subscribed at mount**, with a handler and
  not merely called — `listen(event, undefined)` would satisfy a bare
  "was it called".
- **A click reaches `@/lib/selection`**: clicking a row selects it, Ctrl+click
  adds to it, and the *other* pane stays unselected. Read off the row's own
  text colour rather than a test-only attribute, because an attribute added
  for the test would be a thing the test keeps true rather than a thing the
  user sees.
- **An F-key's `run` acts on the pane its enablement was computed from**: F5
  with two local panes produces the "Both panes are local folders" sentence,
  which is unreachable unless `run` read the same focused pane.
- **No raw key and no unrendered `{{interpolation}}`**, in English and in
  Turkish — ART-062's automatable half. Its other half, whether a longer
  Turkish label still fits, is not touched: jsdom does no layout.

**Mutation-checked, both wiring tests.** With
`const isSelected = selectedNames.has(entry.name)` forced to `false` and
`copyTo`'s `setHint(t("files.err.bothLocal"))` replaced by `setHint(null)`,
the two wiring tests failed and the other four passed. Restored, all six pass.

**Fix round 1 (wave-C1 review, F1): two of the six tests could not fire, and
this entry claimed they did.** The raw-key guard was
`/\bfiles\.[a-z][a-zA-Z]*\.[a-z]/`, and the `\b` made it dead: the
function-key bar renders its label immediately after the key name, so the
screen's text reads `…F3files.functionKeys.view…`, and `3` to `f` is not a
word boundary. The reviewer set `t("files.functionKeys.viewMUTANT")`, watched
the literal key render on screen, and both language tests still passed.

The anchor is gone and the namespaces are listed instead —
`/(files|common|components)\.[a-zA-Z]+\.[a-zA-Z]/`, listed rather than
matched by a generic `a.b.c` shape, which would fire on a filename or a path.
Re-verified with the reviewer's own mutation: with
`files.functionKeys.viewMUTANT` in place both string tests fail and the other
four pass; restored, all six pass.

Test 2 (both listeners subscribed at mount, with a handler) was confirmed real
by the reviewer, so the shape of these tests was right — only the anchor was
not.

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

**Partly fixed 2026-08-20 on `debt-wave-c1`. The resume half is a product
decision and was left open — see ART-177.**

Two things were closed, both of which were plain defects rather than
questions:

- **The screen no longer stays busy.** `onLayoutResult` fires on success
  alone, so a job that failed or was cancelled left `busy` set for ever and
  Preview and Apply both disabled — on the one screen you most need to re-run
  after a failure. `ContentLayout.tsx` now keeps the apply job's id and clears
  the flag from `onJobProgress` when that job reaches any terminal state,
  surfacing the message and its `ART-*` id on a failure. The pattern is
  `FileManager.tsx`'s, which had it already.
- **A failed run says how much of it landed.** `CoreError::PartiallyApplied
  { placed, item, reason }` (`ART-APPLY-PARTIAL`) is the sibling of
  `CancelledPartway`: cancelling has reported its count since ART-058 and
  failing did not, which is why the residue was invisible. A run that fails on
  its fifth item now names the item that refused and the four already on disk,
  at the moment the user can act on it. A run that fails on its *first* item
  reports the plain reason — dressing that up as a partial apply would send
  the user looking for a mess that is not there.

Covered by `core::layout::apply::tests::a_run_that_fails_partway_says_how_much_of_it_landed`
and `…::a_run_that_fails_on_its_first_item_reports_the_plain_reason`.

**What is still open** is what the entry above calls a design question, and it
is: skip-existing, resume, and what a re-preview should say about a
destination that already holds exactly what this plan would put there. That
decides what "already done" means for a staging tree, and it is not a call to
make as a side effect of a debt-clearing pass. Filed as ART-177.

**That half is now answered and closed — see [ART-177](#fixed).** The owner's
decision was *skip, and say so*, so a half-finished apply resumes by itself.

**Fix round 1 (wave-C1 review, F7 and F8).**

- **F7: the icon's name was normalised two ways.** ART-109 unified *which
  field* both sides read; it did not unify *how they read it*. The plan side
  split the raw `destination` string and the apply side used the `safe_join`ed
  target's leaf, so `Games/Turrican/` — which is what a person types into the
  retarget box — gave `Games/Turrican/.info` on one side and `Turrican.info`
  on the other. `core::layout::icon_stem` is now the single normalisation both
  use. Covered by
  `core::layout::tests::a_destination_with_a_trailing_separator_names_the_icon_the_same_way`,
  which asks it of `Games/Turrican/`, `Games/Turrican//` and
  `Games\Turrican\`.
- **F8: `placed` counts whole items.** An item that fails *part way* — a tree
  copy that stopped halfway down, an unpack that got most of a drawer out —
  can leave some of itself behind, and no count describes that. Stated on the
  `PartiallyApplied` variant rather than papered over: the number says what
  finished, and `item` says where to look for what did not.

**ART-108** 🔵 **Nothing you drop can reach the layout screen** — *found
2026-08-15, the whole-branch review of SD-2 G11*
`src-tauri/src/core/workflow/builtin.rs` · The module's own framing is "drop four
hundred files, get an organised card", and the only ways in are the sidebar and
a file dialog: the workflow catalogue has no entry pointing at `/layout`, so
ART's one drop pipeline cannot route anything there. The design doc does not
require it and `Navigate { route: "/layout" }` for a dropped `Directory` is its
own decision — filed because the gap is written down nowhere else.

**Fixed 2026-08-20 on `debt-wave-c1`.** `dir.organise` → `Navigate { route:
"/layout" }` for a dropped `Directory`, and `ContentLayout.tsx` reads the
dropped path out of router state and adds it to its source list — added, not
acted on: a drop says what to lay out, not where to lay it out, and the
staging root stays the user's to choose.

**Registered below `dir.scan_collection`, not above it.** Both are starred, so
neither folder action is a dead end (§46), but *which* of "catalogue this
folder" and "lay this folder out onto a card" should be the first thing a
dropped folder offers is a product judgement. The gap this entry names is that
the layout screen could not be reached by dropping anything at all; opening
that door is not the same as reordering the room, and the second was not
decided here.

Covered by `core::workflow::builtin::tests::a_dropped_folder_can_reach_the_layout_screen`,
which asserts the **route** and not merely that an entry with that id exists —
one pointing somewhere else would satisfy a bare registration check and leave
the screen just as unreachable. `docs/drag-drop-workflows.md`'s folder row
updated in the same commit.

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

**Fixed 2026-08-20 on `debt-wave-c1`**, in both halves, and they turned out to
be the same half.

**The LHA fixture found a real divergence — by removing the possibility of
one.** `core::lha::tests::make_lha_with` builds the fixture, and
`an_lha_whdload_pack_plans_and_applies_under_one_name` drives one `.lha`
through `plan` → `apply`: the name the entry list gives and the name the
extracted tree gives have to be the same name. Writing it exposed that the two
answers were *already* free to diverge without any backend disagreeing at all
— the drawer lands at the destination's leaf, and the icon used to land at
`PackLayout::icon_name()`, the pack's own name. Retarget a row from
`Games/Turrican` to `Games/TurricanII`, which is the one thing that screen
exists to let you do, and the icon lands as `Games/Turrican.info`: attached to
no drawer, silently, which is exactly what §82 exists to prevent.

So both sides now derive the icon's name from the **destination** —
`apply::unpack_whdload` from the target path, `core::layout::icon_destination`
from the same field — and `a_retargeted_whdload_row_takes_its_icon_with_it`
pins it. That test fails against the previous code.

**The `outside` test is not counted, and the reason is written down.** Its doc
comment now says plainly what it does not prove, and the `skip_from_drawer`
call site carries the argument for why nothing can reach that exclusion today:
with a wrapper, `layout.outside` holds only paths that are not under
`layout.root`, and the copy walks `layout.root`, so it never meets them;
without a wrapper, `is_inside` returns true for everything, so `outside` is
empty by construction. The list stays as belt and braces — the day `analyse`
marks a file *inside* the pack as outside it, this is what stops it riding
along — but it is stated as unreachable rather than presented as tested.

**Fix round 1 (wave-C1 review, F6).** The `.lha` fixture was stored `-lh0-` at
**level 0**, and Wave A measured 914 level-1 and 2 259 level-2 entries in the
owner's own archives — so the one format the tests exercised was the one real
packs least often use, which is the same shape this entry was filed for.
`core::lha::tests::make_level1_archive` builds a multi-entry level-1 archive
(the drawer lives in an extension header, `0xFF`-separated rather than `/` —
a different reader path, and therefore a different answer to "what is this
pack called"), and
`core::layout::apply::tests::a_level_one_lha_pack_plans_and_applies_under_one_name`
drives one through `plan` → `apply` alongside the level-0 case.

**What that still does not cover** (G5 of the re-review): the fixture carries
a level-1 *header* and its payload is still stored `-lh0-`. **No `-lh5-`
compression path is exercised anywhere in `core/layout`'s tests** — the level
and the method are independent, and only the level moved. Reading a real
compressed pack end to end is `core/lha`'s own coverage, not this module's,
and if that ever stops being true this is the fixture to grow.

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

**Fixed 2026-08-20 on `debt-wave-c1`.** `LayoutItem` gained `writes_icon`, and
`core::layout::icon_destination` derives the icon's path from the item's own
`destination`. `collisions_in` now asks about both, so a staging tree already
holding `Games/Turrican.info` is reported as a collision instead of coming back
clean and letting `apply` silently no-op the icon.

A **flag** rather than a stored second path, deliberately: the whole point of
that screen is that the user retargets rows, and a path recorded at plan time
would answer about where a row used to point. Derived from `destination`, the
icon follows the row — which is what `layout_recheck` re-asks after every
retarget.

And it is asked of the archive, not assumed: a pack that ships without an icon
sets `writes_icon: false`, because claiming a collision for a file that will
never be written is as wrong as missing one that will.

Covered by `core::layout::tests::an_icon_already_in_the_staging_tree_is_a_collision`
(whose drawer destination is deliberately *free*, so the icon is the only
thing that can produce the finding),
`…::a_pack_with_no_icon_claims_no_icon_destination`, and
`…::a_retargeted_row_moves_its_icon_destination_with_it`.

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

**Fixed 2026-08-20 on `debt-wave-c1`.** Made true for both strategies rather
than narrowed in the doc comment: an API that says all-or-nothing and means it
for one code path is a defect in the promise, not in the path.

`VolumeWriter::delete_many` replaces the loop over `delete_with`. The whole
batch accumulates into **one** `BlockSet` and **one** `Allocator`, and a single
`commit` journals it — so a failure anywhere, at any image size, rolls the
journal back and leaves the file byte-for-byte as it was. What made that
possible without a redesign was already there: every read inside the writer
goes through `set.view(device, block)`, so each entry's unlink sees the hash
chain as the previous unlinks left it, and a directory emptied earlier in the
same batch reads as empty when its own turn comes. The allocator is loaded
once for the batch and stamped once at the end — loading it per entry would
read the bitmap back off the device and lose every block the earlier deletes
had freed.

The pre-check (`check_batch_deletable`) stays and is not redundant: it runs
against a read-only mount before the writer session opens, so the ordinary
refusals cost no journal at all. What changed is the guarantee for whatever
gets past it.

**Reproduced before it was fixed, on the path that had the defect.** The
pre-check does not look at protection bits, so a delete-protected entry is a
failure that reaches the writer. With the old loop restored and the new test
run against a 16.9 MB image (33 000 blocks — just past
`WHOLE_FILE_LIMIT_BYTES`, the smallest image that takes the journalled path at
all), the batch `["First.txt", "Second.txt", "Locked.txt"]` left exactly
`["Locked.txt"]` on the volume: the two entries before the refusal were
already durable in the user's file. Restored, all three survive.

Covered by `commands::volume_write::tests::a_batch_delete_that_fails_inside_the_writer_deletes_nothing_on_a_journalled_image`
(which asserts the image's **bytes**, the listing, and that no rolled-back
journal is left behind), with
`…::a_batch_delete_that_can_succeed_still_removes_everything_on_a_journalled_image`
as the other half — a gate that refused everything would satisfy the first
test and destroy the feature — and
`…::a_batch_delete_that_fails_inside_the_writer_deletes_nothing_on_a_floppy_either`
so that closing this cannot quietly cost the strategy that was already
correct.

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

**Fixed 2026-08-20 on `debt-wave-c1`, together.** They are one gap seen from
two directions, and the thing missing was the same in both: a way to say
"these entries, out of this volume, as one operation".

`core::volume::write::copy::extract_selection_from_volume` is that primitive.
It takes a `&[SelectedEntry]` — header block, name, is-dir, exactly what the
pane has on screen — mounts once, and writes every picked entry into one host
folder under one `ExtractReport`. Cancellation is checked between whole
entries, never inside one.

Both directions are that function with a different destination:

| direction | command | destination |
|---|---|---|
| volume → local | `volume_extract_many` | the folder the user picked |
| volume → volume | `volume_copy_between_many` | a temp folder, then `run_copy_in_staged_with` into the other image |

**What each gained.** ART-065's volume→local path was a `Promise.all` of one
`volumeCopyOut` job per folder and one bare `volumeExtractTo` per file; each
was safe alone and the batch had no guarantee at all. It is now one job, one
walk, one report, one Stop — and a stopped run says so
(`files.status.copyOutStopped`) rather than reporting the files it happened to
reach as a finished job. ART-064's volume→volume path refused by name; it now
stages the whole selection into one temp folder and inserts it in one
operation, so the destination takes **one** backup and one commit, and
`OnCancel::Abandon` means a stopped batch commits nothing — the same promise
`volume_copy_in_many` already gave the opposite direction.

Two smaller things fell out and are worth naming:

- **`run_copy_in_staged` gained an `on_cancel`**, so the staged insert can
  abandon the way the host-folder insert already could. The single-tree
  `volume_copy_between` keeps `KeepWhatLanded`; only the batch abandons.
- **A collision no volume can have and one host folder can.** `Prices: 1993`
  and `Prices? 1993` are two entries there and one file here, so
  `check_escaped_name_collisions` refuses the selection up front, naming both
   — the same shape and the same case-insensitive comparison
  `HostSelection::check_for_name_collisions` uses (ART-072).

**What did *not* change here, deliberately.** F6 (Move) still refused several
entries between two images at the time: a move is a copy *and* a delete, and
the delete half was ART-081's, not this one. `files.err.batchBetweenVolumes`
survived for that refusal and its text was rewritten in both catalogues to
talk about **moving** rather than copying. *(Both are gone as of
[ART-081](#fixed), 2026-08-20: `volumeDeleteMany` is that delete half, F6
takes a selection like F5 does, and a key saying "not supported yet" would now
be false.)*

Covered by `commands::volume_write::tests::a_selection_copies_out_of_a_volume_as_one_operation`,
`…::a_cancelled_selection_copy_out_says_so_in_one_report`,
`…::a_selection_whose_names_collide_on_the_host_is_refused_before_anything_is_written`,
`…::a_selection_copies_between_two_images_as_one_batch`, and
`…::a_cancelled_selection_between_two_images_leaves_the_destination_untouched`
(which asserts the destination's **bytes**, not merely that an `Err` came
back). The frontend half — refuse the whole selection rather than silently
dropping a row ART cannot address — is `src/lib/selection.test.ts`'s
`selectedEntriesForBatch` block.

**Fix round 1 (wave-C1 review, F4 and F5).**

- **F4: a failed volume→local batch dropped its report.** Every `?` in
  `extract_selection_from_volume`'s loop threw the `ExtractReport` away, which
  is exactly the situation `CoreError::PartiallyApplied` had been added for
  one issue earlier. A batch that fails on its seventh entry now names the
  entry that refused and the count already on disk; one that fails before
  writing anything still reports the plain reason, because dressing that up
  would send the user looking for a mess that is not there. Covered by
  `commands::volume_write::tests::a_selection_copy_out_that_fails_partway_says_how_much_landed`
  and `…::a_selection_copy_out_that_fails_at_once_reports_the_plain_reason`.
- **F5: "a stopped batch commits nothing" is a whole-file promise.** On the
  block-journal strategy — an image past 16 MB — each file copied in is its
  own committed, journalled operation, durable before the next one starts.
  `OnCancel::Abandon` there buys **honesty, not atomicity**: the job ends
  `CancelledPartway { files }` rather than reporting a successful install of a
  package that is missing most of itself. `run_copy_in_staged_with` says so at
  both branches, as `run_copy_in_folder_with` already did; the claim in the
  wave report has been corrected to match.

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

**Fixed 2026-08-20 on `debt-wave-c1`** — see ART-065 immediately above, which
this was closed with and by the same primitive.

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

**Fixed 2026-08-20 on `debt-wave-c1`.** The deepening is a new module rather
than a bigger `validate_image`: `core/volume/integrity.rs` walks the volume
from the root block outwards, claims every block each file and directory
occupies, and compares that with the free-space map. It lives in `core/volume`
and not in `core/adf` because it needs `core/volume/write/bitmap`'s `Allocator`
to read a **multi-page** bitmap — `core/adf`'s own `Bitmap::parse` reads one
block and stops, which is enough for a floppy and wrong for everything else —
and `core/adf` may not import upwards.

Four things it now catches that nothing did:

| finding | severity | why |
|---|---|---|
| `blocks.crosslinked` | Problem | two files own one block; writing one destroys the other |
| `hashchain.bucket` | Problem | an entry linked in a bucket its name does not hash to — on the disk, and invisible to every `Dir` |
| `bitmap.in_use_but_free` | Problem | a block a file occupies that the map calls free: the next allocation hands it out |
| `bitmap.leaked` | Warning | a block marked used that nothing references — wasteful, and reads perfectly |

**The gate compares; it does not judge.** A volume the user has carried since
1993 may already leak a block or hold an entry in the wrong bucket, and
refusing every write to it on that ground would take their disk away from them
rather than protect it (§89 — the same rule that already made a bad bootblock
checksum a warning and not a refusal). So `WholeFileVolume::commit` walks the
volume **twice**, as it was and as the operation left it, and only a `Problem`
finding the operation *introduced* refuses the write. `integrity::newly_broken`
compares on `(code, message)` rather than on code alone, so a volume that had
one cross-link and now has two still refuses.

Scope is stated rather than implied: this is the **whole-file** strategy's
gate. The block-journal strategy has no whole-image validation at all — it
exists for images too large to hold in memory — and gains none here; the
journal and `validate_touched` remain what protect it, exactly as
[`with_volume`]'s doc comment already said.

Covered by `commands::volume_write::tests::a_write_that_would_cross_link_two_files_never_reaches_the_file`
and `…::a_write_that_would_hide_a_file_from_amigados_never_reaches_the_file`
(both assert the file is byte-for-byte unchanged, not merely that an `Err` came
back), by `…::a_volume_that_was_already_broken_is_still_writable` for the §89
half, and by ten unit tests in `core::volume::integrity::tests` — including
`a_clean_volume_has_nothing_to_report` and
`a_clean_volume_with_a_multi_page_bitmap_has_nothing_to_report`, without which
none of the corruption tests would mean anything. Reverting `deep_check`'s call
site makes the first two fail; that was checked, not assumed.

**Fix round 1 (wave-C1 review, F2 and F3).** Two holes, both in the same
direction — the gate seeing less than the walk did.

- **F2: a cap on reporting had become a cap on detecting.** Past
  `MAX_PER_CODE` (8) the overflow folded into a `Warning` `.more` line, so a
  **ninth** cross-link on a volume that already had eight was invisible to
  `newly_broken` — eight `Problem`s before, eight after, write allowed. The
  overflow line now carries the **worst severity folded into it** and its
  message carries the count, so nine produce a `Problem` saying "1 further"
  where eight produce none and ten say "2 further" — three different
  `(code, message)` pairs, which is what `newly_broken` compares on. Covered
  by `core::volume::integrity::tests::a_finding_past_the_reporting_cap_still_reaches_the_gate`,
  mutation-checked by forcing the overflow back to `Warning`.
- **F3: proceeding is not the same as saying nothing.** "Refuse only what the
  operation introduced" is the right rule for *refusing*, and it was never a
  reason to write into a volume ART had just found cross-linked and tell
  nobody. `deep_check` now returns the pre-existing `Problem`s and logs each
  one; `WholeFileVolume::commit` returns a `Committed { backup, pre_existing }`
  instead of a bare backup path; and the damage reaches the user twice — as
  `pre_existing_damage` on `MutationResult`, `DeleteManyResult` and
  `MutationOutcome`, and as a `Pre-existing damage` detail in the operation
  log (§53). Covered by
  `commands::volume_write::tests::a_write_into_an_already_damaged_volume_says_so_while_going_through`
  (the write still lands — that is the §89 half) and
  `…::a_write_into_a_sound_volume_reports_no_damage`, without which a field
  that was always populated would satisfy the first and mean nothing.

**Measured by the reviewer, and worth keeping:** the walk costs **1.85 ms** at
the 16 MB whole-file cap and **214 ms** on a 2 GB partition with 20 000 files.
Wiring it into the health badge is affordable on time; the watch item is
memory, not time.

**Fix round 2 (wave-C1 re-review, G2): F3's fix was still the silence it was
filed about.** `pre_existing_damage` reached the operation log, the
application log and the frontend's types — and **nothing drew it**. A field no
screen renders is the same silence with more code behind it.

`src/components/files/DamageRow.tsx` renders it, in both catalogues
(`files.damage.foundBeforeWriting`), and says all three things the user needs:
ART found this damage **before** it wrote, the write still landed, and here is
what was already wrong. Its own component rather than a fifth thing competing
for the status line — it is not an error (nothing failed) and not a hint
(nothing was declined) — and its own component so it can be *rendered in a
test* without standing a two-pane commander up around it, which is how "this
reaches the user" gets checked instead of asserted.

Covered by `src/components/files/DamageRow.test.tsx`: the sentence renders in
English and Turkish with no raw key and no unrendered interpolation, at most
three findings are shown, and — the half that stops it crying wolf — an empty
list renders **nothing at all**.

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

**Fixed 2026-08-20 on `debt-wave-b2`.** `core/iso/susp.rs` reads the System Use
Area of every directory record; `core/iso/directory.rs` lets the Rock Ridge
`NM` name win over the ISO9660 identifier and hangs the `AS` protection long
and comment off `IsoEntry`; `IsoImage::list` follows a record's `CE`
continuation chain, which `directory.rs` cannot because it has no I/O.

**Measured before it was designed.** `scripts/iso-susp-census.py` (checked in,
so the answer can be re-run rather than re-trusted) walks every directory
record of every disc in the owner's ISO folder and counts SUSP signatures:

| disc | descriptors | signatures |
|---|---|---|
| `AmigaOS39.iso` | `[Primary, Primary, Terminator]` | `RR` 10536 · `PX` 10536 · `NM` 8584 · **`AS` 8584** · `SP` 1 |
| `Amiga Developer CD v2.1.iso` | `[Primary, Primary, Terminator]` | `RR` 41050 · `PX` 41050 · `NM` 36212 · **`AS` 36212** · `SP` 1 |
| `Amiga Developer CD v1.1 …iso` | `[Primary, Primary, Terminator]` | `RR` 10451 · `PX` 10451 · `TF` 10451 · `NM` 9111 · `SP` 1 · `CE` 1 · `ER` 1 (**no `AS`**) |
| `AmigaOS3.2CD(ZaP).iso` | `[Primary, Primary, Terminator]` | **none** — 6963 records, not one System Use Area |

So two of the four discs carry `AS`, one carries POSIX Rock Ridge without it,
and one carries no System Use data at all. All four ISO9660 discs and none
Joliet, which is why the names and the bits are only in the primary tree.

**The byte layout came from the disc that describes it.** Amiga Developer CD
v2.1 carries `Rock_Ridge_Amiga_Specific` v2.4 (1996-12-05, Angela Schmidt with
Andrew Young) at `/Contributions/Angela_Schmidt/Reference/` — the
specification itself, read off the owner's own medium. It was cross-checked
against `ODFileSystem`'s `rr_parse_as` (BSD-2-Clause,
<https://github.com/reinauer/ODFileSystem>), whose two unit vectors are
reproduced byte for byte in `core/iso/susp.rs`'s tests, and against all 44 796
real `AS` entries: every one accounts for its payload exactly, none with bytes
left over and none the layout fails to fit.

**What the bits turned out to be.** On the 3.9 CD: `0x20` (`p`, pure) 106
times, `0x40` (`s`, script) 6 times, `0x60` once; on Developer CD v2.1, `0x20`
39 times, `0x22` 216, `0x42` 14, `0x46` 3. ART reading the real disc now
reports `OS-Version3.9/Workbench3.5/C/Assign` as `--p-rwed` and
`S/Start-ACTION.rexx` as `-s--rwed` — the two bits this entry was filed about.
33 files across the two discs carry a comment (AWeb's cache records the URL it
fetched each file from; the Developer CD's copy of the specification records
where it came from).

**What ART does with them.** A disc-sourced file now writes a `.uaem` sidecar
when — and only when — its record carried an `AS` entry, through the same
`copy::sidecar_for` an ADF extraction uses, so one file and a whole folder
dragged off the same disc produce the same sidecars. A recording date alone is
deliberately *not* enough: writing one for that would put a second file beside
every file on every plain ISO9660 disc ART has ever extracted, which this entry
does not ask for. `IsoSource::metadata` hands the same bits to the shared copy
engine, so a WHDLoad slave copied off a CD onto an HDF keeps its `S` and `P`.
`osinstall::source_cd` propagates them into `MediaEntry`, so `distribution.json`
records what the medium said rather than a declared default — that module's own
doc used to assert ISO9660 "has nowhere to keep" them, and now says why that is
true of ISO9660 and false of an Amiga-mastered disc. `iso_list` fills the Attr
column from them through `uaem::format_bits`, never a second formatter.

**Verified by two implementations that are not ART.** 7-Zip is now handed a
Rock Ridge fixture as well as the plain and Joliet ones
(`scripts/iso-oracle-check.py`) and agrees on all four mixed-case names — which
matters because ART's `NM` reader and ART's own fixture builder were written
from the same reading of SUSP and could have agreed with each other and both
been wrong. The `CE` case is deliberately *not* handed to 7-Zip: given one it
lists `Games/Game` where ART lists `Games/Game.slave`, and its own source says
why (`CPP/7zip/Archive/Iso/IsoItem.h`, `FindSuspRecord` returns the first
matching inline entry and never looks for `CE`). That case is checked instead
by a third implementation, `scripts/iso-susp-census.py`, which follows
continuations and joins the same halves the same way.

**Tests.** `core::iso::susp::tests` (17, including ODFileSystem's own two
vectors, the shape every real `AS` entry has, a zero-length entry, an entry
running past its area, a comment length byte of zero, and the entry cap) and,
in `core::iso::tests`:
`a_rock_ridge_disc_reads_its_real_names_not_its_8_3_ones`,
`a_rock_ridge_disc_carries_its_amiga_protection_bits_and_comment`,
`a_name_and_comment_split_across_a_ce_continuation_are_joined`,
`a_disc_with_no_sp_entry_is_not_parsed_as_rock_ridge`,
`the_sp_skip_is_honoured_rather_than_assumed_to_be_zero`,
`a_disc_sourced_file_carries_its_bits_into_a_uaem_sidecar`,
`a_disc_sourced_file_carries_its_bits_into_an_amiga_volume`,
`a_disc_with_no_amiga_metadata_still_writes_no_sidecars`.

The last two exist to stop the others passing vacuously: the first pins that
the bits reach the *volume* writer and not only the sidecar writer, and the
second that an ordinary disc still produces no `.uaem` at all.

**What is deliberately still not read**, all four measured against the same
four discs rather than assumed:

- **`SL`, `RE`, `CL`, `PL`** — Rock Ridge symlinks and deep-directory
  relocation. No disc in the folder carries one (the signatures present are
  `SP`, `CE`, `ER`, `RR`, `PX`, `TF`, `NM`, `AS`), and ART has no symlink to
  map them onto. Untested code for an unmeasured case is worse than none —
  the same call `core/archive/lha.rs` made about the `0x3F` comment header.
- **`PX`** — POSIX mode. The specification's TABLE 6 is not a suggestion:
  *"Filesystems **shall** make use of the required 'PX' entry if no 'AS' entry
  or no protection bits in that entry are present for an object"*, and it gives
  the exact mapping. ODFileSystem implements it. **ART deliberately does not**,
  and the deviation rests on what was measured rather than on reading the
  requirement down. Exactly one disc here carries `PX` with no `AS`: Developer
  CD v1.1, whose own `ER` entry declares `RRIP_1991A` — POSIX Rock Ridge, so
  its modes are the *mastering host's* file permissions and not an Amiga's
  intent. Deriving from them would manufacture Amiga bits the disc never
  recorded and present them as read, which §89 forbids more strongly than the
  specification requires the mapping. If a disc turns up whose `PX` is
  Amiga-derived, this is the entry to reopen.
- **A disc carrying both Joliet and Rock Ridge** would still be read as Joliet
  and so lose its `AS` entries. None of the four does — all are ISO9660-only —
  so the preference is untouched rather than reversed on a guess.
- **`AS` on a Joliet tree**. Not looked for: the `SP` probe asks the tree ART
  actually reads, so a Joliet root answers `None` and no System Use Area is
  parsed there.

**ART-161** 🔵 **The same disc is fully walked three to four times per
install, because `scan::identify` opens a `CdSource` to read one string and
then drops it** — *found 2026-08-19 by the whole-branch review (finding 12)*
`src-tauri/src/core/osinstall/scan.rs:148` (`identify`) ·
`src-tauri/src/core/osinstall/apply.rs:301` ·

`CdSource::open` walks a disc's entire directory tree once, at open, and
holds the result. `scan::identify` opens one purely to read the volume name —
the one thing `find_media` needs to match a recipe's `media` — and then drops
the whole walk. `open_media` immediately re-opens and re-walks the same file.
`find_media` does this for **every** candidate in the media folder (the
owner's holds four ISOs), and `apply()` does it again per medium, so the
owner's own 469 MiB disc is walked three to four times in one install.

Not a memory problem and not a correctness problem: every walk is bounded
(`MAX_WALK_ENTRIES` 100,000, `MAX_WALK_DEPTH` 16, refused rather than
truncated — ART-158), and the measured cost is inside the ~20 seconds a real
3.9 build already takes. Low, and recorded rather than fixed because the
honest fix — letting `identify` answer from a descriptor read alone, or
handing the already-open source forward instead of a path — is a change to
`scan.rs`'s contract with both of its callers.

**Fixed 2026-08-20 on `debt-wave-b1`**, by the first of the two options the
entry named — `identify` answers from a descriptor read alone — because it is
the one that does not change `scan.rs`'s contract with either caller.

`IsoImage::open` reads the volume descriptors and stops; it is what
`CdSource::open` itself calls first, before the walk. So the volume name still
comes from **inside** the medium, which is this module's whole rule and is not
weakened by reading a smaller part of the inside. What is dropped is only the
walk — and it still happens, exactly once, in `open_media`, for the disc an
install actually reads.

**Measured against the owner's own ISO folder** (four discs, warm cache,
2026-08-20, `identify` timed *first* on each file so the walk never inherits a
cache the descriptor read paid for):

| disc | size | `CdSource::open` | descriptor only |
|---|---|---|---|
| `Amiga Developer CD v1.1 …iso` | 58.6 MiB | 146.4 ms | 1.4 ms |
| `Amiga Developer CD v2.1.iso` | 258.0 MiB | 525.9 ms | 0.8 ms |
| `AmigaOS3.2CD(ZaP).iso` | 74.3 MiB | 147.8 ms | 0.8 ms |
| `AmigaOS39.iso` | 468.1 MiB | 188.7 ms | 0.9 ms |
| **all four** | | **1,008.8 ms** | **3.9 ms** |

Both columns are ART's own code on the owner's own discs, run once and then
deleted rather than left as a machine-specific test. Note that `AmigaOS39.iso`
is the *largest* file and not the slowest walk: the cost is the shape of the
directory tree, not the size of the disc, which is worth knowing before anyone
optimises the wrong half of it.

**One consequence, deliberately taken and now pinned by a test.** A disc that
exceeds ART's own walk bounds (`MAX_WALK_ENTRIES`, `MAX_WALK_DEPTH` — ART-158)
used to be refused inside `identify`, so it vanished from the scan with nothing
said about it at all. It now identifies normally and is refused **by name**,
with the same unchanged `LimitExceeded` sentence, at the point something
actually tries to read its tree. A named refusal a user can read beats a disc
that silently is not there (§89); only where it surfaces has moved.

**Test:** `a_disc_is_identified_from_its_descriptor_without_walking_its_tree`
— a synthetic ISO nested seventeen levels deep, one past `MAX_WALK_DEPTH`. It
asserts first that `CdSource::open` really does refuse it (without that line
the test would be measuring a disc ART was perfectly happy with, and proving
nothing), then that `identify` succeeds anyway with the descriptor's own volume
name, then that the disc reaches `find_media`. Reverting `identify` to
`CdSource::open` fails it.


**ART-170** 🟡 **`collide::preview` can only be asked about a **package**,
so a release recipe's own layering cannot be previewed at all** — *found
2026-08-19 while measuring ART-169's fix; filed rather than fixed, Task 8 fix
round 2 (F2)*
`src-tauri/src/core/osinstall/collide.rs` (`declared_override`)

`preview` builds each row's `declared` column through `declared_override`,
which resolves the incoming item's component id with
`package::by_id(component)` and **refuses by name** when it resolves to
nothing:

> `'{component}' does not name a shipped package, so ART cannot say whether it
> declared an override for '{to}'`

That is right for what the function was built for (a package's `overrides` is
the only thing that can declare an intent to replace a *tree's* file, and
answering `false` for an unknown id would make every row read "nothing
declared" for a reason unrelated to what was declared). The consequence is
that the preview §3 requires cannot be shown for a recipe component — and
AmigaOS 3.9 now has one that genuinely layers over another, `workbench-39`
over `workbench-base`, replacing 40 real files.

Measured around it rather than through it: `layer_the_real_39_overlay_when_asked`
calls `collide::classify` directly — the same function `preview` calls once it
has both sides' bytes — and reports the four classes plus `Identical`. Only the
`declared` column is missing, and for a layer *inside one recipe*
`plan::detect_collisions` has already enforced the same thing at plan time, so
nothing is unguarded today.

What it costs: the OS Builder can show a user what a BoingBag would replace and
cannot show the same user what switching a component on would replace. Fixing
it means resolving `component` against the union of every shipped id — releases
and packages together, the way `recipe.rs::all_shipped_component_ids` already
does for `every_override_names_a_component_that_exists` — rather than against
`package::by_id` alone.

**A second, harder limit sits beside this one, and widening `declared` will
not touch it.** `Libs/WORKBENCH.LIBRARY` carries no `$VER:` marker at all, so
the 3.9 overlay's replacement of it (193,400 → 199,852 bytes) classifies as
`Unversioned` — a size comparison — while being the single change that turns
`Workbench 44.5` into `Workbench 45.1`. **The boot proved the decisive change
and the classifier could not.** Generalised: if `workbench.library` carries no
readable marker, other decisive files will not either, so **any future "did
the update take?" check built on collision classes alone will be confidently
wrong about the one file that matters.** Reading the version off a booted
system is not a nicety — see the method note in
`layer_the_real_39_overlay_when_asked` and in [STATUS.md](STATUS.md).

**Fixed 2026-08-20 on `debt-wave-b1`**, exactly as the entry scoped it:
`declared_override` now resolves the incoming item's component id against the
union of every shipped id — releases and packages together — instead of
against `package::by_id` alone.

`recipe::shipped_component_overrides(id)` is the resolver, and it is the
runtime counterpart of the union `every_override_names_a_component_that_exists`
already used for validation. `overrides` crosses the boundary in **both**
directions in shipped data — `locale-turkish` (a package) declares
`locale-base` (a release component), `workbench-39` (a release component)
declares `workbench-base` — so reading it out of one catalogue was never the
whole answer, and that test knew it before the code did.

The refusal is unchanged in kind: an id in neither catalogue is still refused
by name, because an id that resolves to nothing is an inconsistency in
whatever built the item rather than a fact about the file. Only the sentence
widened, from "does not name a shipped package" to "does not name a shipped
component or package".

**Tests:** `a_release_recipes_own_component_can_be_asked_about_too` (the real
pair — `workbench-39` over `workbench-base` — asserted in both directions of
the `declared` column, so an answer that were simply always true fails),
`shipped_component_overrides_reads_releases_and_packages_alike` (the resolver
over both catalogues, over a component that declares no overrides, and over an
id in neither), and `no_id_is_claimed_by_both_a_release_and_a_package` (the
precondition the resolver's release-then-package order rests on). The first
fails with `declared_override` put back on `package::by_id`.

**What this does not do**, said plainly rather than left to be discovered: the
*command* layer's own incoming-builder (`commands::osinstall::extract_incoming_for_preview`)
still assembles rows from chosen packages only, so the OS Builder screen has
nothing to hand `preview` for a recipe component yet. What is removed is the
block that made it impossible in the core — the layer above can now be widened
without touching `collide`.

**That half is now [ART-175](#fixed), filed in fix round 1** rather than left
as a sentence inside a closed entry. A half-closed issue with no successor is
how work disappears, and this one is the user-facing half of what ART-170 was
about.

**Fix round 1 (2026-08-20) — two defects in the resolver itself.**

**F10 — `panic!`/`expect` inside `core/`, on a per-row path.** The first
version reasoned that a shipped recipe that will not parse is ART's own bug
rather than a user situation, and panicked. That does not survive the release
profile: `panic = "abort"` makes it take the whole application down, and
`declared_override` runs for every collision row — the exact shape CLAUDE.md's
bounds-checking rule names. `shipped_component_overrides` now returns
`CoreResult<Option<Vec<String>>>`; broken shipped data is still a bug, and it
is now a bug that produces a refusal a user can read.

**F6 — it re-parsed every shipped recipe per row, and nothing said so.**
Benchmarked on this machine (release profile, 20,000 calls):

| path | per call |
|---|---|
| building the map uncached — what the first version did per row | **116.1 µs** |
| `package::by_id` alone — what it replaced | 18.5 µs |
| the cached map, as it now stands | **0.161 µs** |

which reproduces the review's own 110 µs / 16.5 µs and then removes it: the
`id -> overrides` map is built once per process in a `OnceLock`. The shipped
JSON is `include_str!`-ed into the binary and cannot change under a running
process, so the cache can never go stale; the parse **result** is cached, so a
broken recipe is reported identically on the first call and the thousandth.
`CoreError` is not `Clone`, so the failure is held as its own text and a
`CoreError::Malformed` is rebuilt from it unchanged.

**The second, harder limit recorded above is untouched and stands**: a file
carrying no `$VER:` marker at all — `Libs/WORKBENCH.LIBRARY` is the measured
one — classifies as `Unversioned` whatever it actually changes, so no "did the
update take?" check built on collision classes alone can be trusted about the
one file that matters. Widening `declared` was never going to reach that, and
this did not.


**ART-157** 🟡 **The recipe format cannot state a Kickstart *minimum*, only a
maximum, so AmigaOS 3.9's real requirement (V40 or newer) goes unstated and
unchecked** — *found 2026-08-19, Task 3, answering the plan's own instruction
to report this rather than invent an encoding for it; recorded here per the
plan's ruling at the time (written when this documentation task was numbered
differently)*
`src-tauri/src/core/osinstall/mod.rs:77-85` (`Condition`) ·

`Condition` has exactly one variant, `RomOlderThan { major: u16 }` — "switch
this component on when the paired Kickstart's own stated major is *below*
this," which the 3.2 recipe uses to add `modules-a1200` back for a pre-V47
ROM. AmigaOS 3.9 needs the opposite fact stated — "this release requires *at
least* a 3.1/V40 ROM" — and there is no condition kind, recipe field, or
`PairedRom` field that reads a stated minimum today. `workbench-base`, 3.9's
only component, carries no `condition` at all, so G9's pairing check has
nothing to enforce for this release: a plan built against it will never raise
`RomUnknown` or any Kickstart-related refusal for this reason, which is
`NotChecked`, not a pass.

Not fixed, and nothing was repurposed to approximate it — asserting a false
minimum through `rom-older-than` would be worse than stating none. The honest
fix is a new condition kind (`Condition::RomAtLeast { major: u16 }`, or a
release-wide `minimum_rom_major` outside any single component's on/off
switch, since this is a fact about the release rather than one component) plus
the G9 pairing wiring to enforce it — engine code touching `core/osinstall/mod.rs`
(`Condition`, `condition_holds`), `plan.rs` (`rom_requirement`), and the wire
types (`PairedRom`, `src/lib/osinstall.ts`), its own change, not built here.

**Fixed 2026-08-20 on `debt-wave-b1`**, as the entry itself scoped it: the new
condition kind plus the G9 wiring, and nothing repurposed to approximate it.

**The number was read off the artefact, not recalled.** AmigaOS 3.9's own
Installer script — `OS-Version3.9/OS3.9Install` on the owner's `AmigaOS39.iso`,
`$VER: Install 45.0 (24.11.2000)`, extracted with 7-Zip 2026-08-20 — carries
the refusal string it exits with:

> `You have to install Kickstart 3.1 ROMs before installing Workbench 3.9.`

and, in the German strings beside it, `Sie müssen Kickstart 3.1 und min.
Workbench 3.0 zur Nutzung von Workbench 3.9 vorinstallieren.` Kickstart 3.1
is V40. The manual on the same disc (`installation/book-main1.html`) states
only the *hardware* floor — 68020, 6 MB Chip and 4 MB Fast on pre-A1200 models
— and names no Kickstart version at all, which is why the installer and not
the manual is the citation in the recipe.

`Condition::RomAtLeast { major }` is the mirror of `RomOlderThan`, evaluated in
the same `condition_holds` and by the same rule: the ROM's own header
(`core::rom::stated_version`), never `KNOWN_ROMS` (ART-104).

**Where the two kinds differ is `rom_requirement`, and that is the whole
mechanism.** They contribute from opposite sides of the switch and mean the
same thing about the finished tree:

- `RomOlderThan` contributes when its component is **off** — the fallback
  (`LIBS:Modules`) is absent, so the tree needs the ROM the condition would
  have fired for.
- `RomAtLeast` contributes when its component is **on** — the component's own
  files are what need the newer ROM.

Declared on 3.9's `workbench-base`, which is `required`, so it is a *statement*
and not a switch: a `Condition` can only ever turn a component on, and this one
is on regardless. What it changes is what the tree records —
`PairedRom::requires_major`, which `commands::preload::rom_pairing_for` maps to
`core::rom::pairing::TreeRom` and G9 puts to the card's own Kickstart before
the destructive step.

**One thing was found while wiring it and fixed in the same commit.**
`resolve_components_on` evaluated a `required` component's condition, which
could push `RefusalReason::RomUnknown` and refuse the whole plan. For a
component that is on unconditionally the evaluation can change nothing — the
`Ok(false)` arm is a no-op — so without the skip, building a 3.9 tree would
have started demanding a paired ROM purely because the recipe now says out
loud what the release needs to *boot*. The requirement is still recorded
(`rom_requirement` reads the recipe, not the resolved set) and still checked,
where it can be acted on. Building the tree is deliberately not refused: a 3.9
tree is assembled on the host and the Kickstart it will meet is the card's.

**On screen**, `ComponentSummary` grew `requires_rom_major` beside
`condition_major` rather than reusing it. The two numbers read alike and say
opposite things, and `conditionalReason`'s four-branch vocabulary is written
entirely for `rom-older-than` — projecting a floor through it would have told
the user the reverse of the truth. One new key in both catalogues,
`osinstall.components.reason.romAtLeast`, renders it on the row.

**Tests, and what each one actually guards** (re-measured in fix round 1 —
the claim "all five fail" was written, not run; three Rust and one frontend
did, and the fifth, `a_rom_at_least_condition_holds_from_its_own_major_upwards`,
tests `condition_holds` directly and does not depend on the recipe declaring
anything):

| test | fails with the recipe's declaration removed? |
|---|---|
| `each_condition_kind_contributes_its_requirement_from_its_own_side` | yes |
| `a_39_tree_reports_unsuitable_against_a_card_carrying_a_pre_v40_rom` | yes |
| `a_39_trees_recorded_minimum_survives_the_manifest_and_reaches_g9` (new, F2) | yes |
| `component_summary_serializes_with_the_keys_the_checklist_reads` | yes |
| frontend `projects each condition kind into its own field and never the other` | yes |
| `a_rom_at_least_condition_holds_from_its_own_major_upwards` | **no** |

Measured: `1947 passed; 4 failed` in Rust plus one frontend failure. The last
row is the unit test for the new variant itself — it constructs its own
`Condition::RomAtLeast { major: 40 }` and asserts both edges of the boundary,
so no recipe has to declare anything for it to be meaningful. It guards the
comparison, not the declaration.

**What the chain test proves, and what it does not** (fix round 1, F2 — the
claim "the whole chain" was not supported). Two tests now cover two different
lengths of it:

- `a_39_tree_reports_unsuitable_against_a_card_carrying_a_pre_v40_rom` covers
  shipped recipe → `rom_requirement` → a **hand-built** `TreeRom` →
  `pairing::compare`. The manifest is not in it.
- `a_39_trees_recorded_minimum_survives_the_manifest_and_reaches_g9`, added in
  fix round 1, closes that hop: the number goes from the shipped recipe
  through `rom_requirement` into a real `distribution.json` as a `PairedRom`,
  and comes back through `commands::preload::rom_pairing_for`'s own narrow
  reader and mapping, against a card manifest carrying V37 and then V40.
  (`plan::rom_requirement` became `pub(crate)` for this, because the test has
  to live in `commands/` — `core/` may not depend on it.)

**Still unproven, and recorded rather than implied:** `apply` actually writing
that `PairedRom` into a real tree's manifest (the test writes the JSON
itself), and everything past the comparison — no card has been written with a
3.9 tree and no 3.9 tree has been booted. See ART-159 and FEATURES.md's 🟡
row; this entry claims a recorded, checked requirement, not a working system.


**ART-167** 🟠 **Eight of the owner's archives claim the top-level directory
`LocaleUpdate` and two claim `BoingBag3.9-2`, so `scan::package_for` correctly
refuses two of the three shipped packages and nothing in the product can pick
between the candidates** — *found 2026-08-19 by Task 8's real run, on
`content-layer`*
`src-tauri/src/core/osinstall/scan.rs:329` (`package_for`) ·
`src-tauri/src/commands/osinstall.rs` (`resolve_package_archive`)

`find_packages` identifies 27 archives out of the 58 entries in
`E:\amiga\Amigatolon\paketler` (0.30 s; the 171 MB `.rar` and both `.7z` files
among them, so the "skip what cannot be opened" rule holds). Of those:

| package | `media` | `package_for` |
|---|---|---|
| `boingbag-39-1` | `BoingBag3.9-1` | `Found` |
| `boingbag-39-2` | `BoingBag3.9-2` | `Ambiguous` — `BoingBag39-2.lha`, `BoingBag39-2-Contribution.lha` |
| `locale-turkish` | `LocaleUpdate` | `Ambiguous` — all eight `BoingBag39-2-<language>.lha` |

The refusal itself is right, and `package_for`'s own doc comment predicted this
exact folder. What is missing is the other half: a package's identity is its
archive's single top-level directory, and eight different language packs share
one. Nothing in `plan()` or in the Produce screen lets a user say *which*
`LocaleUpdate` they mean, so `locale-turkish` cannot be selected at all against
the owner's real folder — the run only measured it by naming the archive
outright, which `add_package`'s own contract allows ("`archive` is given, not
looked up") but no user-facing path offers.

**State: open, and it is the cheapest of this round's four to close** —
`add_package` already takes a named archive, so what is missing is a way for
the *user* to name one when `package_for` answers `Ambiguous`: the refusal
already carries the candidate list, and the screen needs to offer it as a
choice rather than render it as a dead end.

**What the screen does today, checked in the code rather than assumed:**
`osinstall_packages` sets `available` with `found.iter().any(|f| f.media ==
p.media)` (`commands/osinstall.rs`), and all eight language archives *do*
carry `media == "LocaleUpdate"` — so the package is offered as available, the
tick is accepted, and the `PackageArchiveAmbiguous` refusal fires when the
preview or the add resolves the archive. Nothing is ever placed from the
wrong archive, which is the safer half of getting this wrong, but the user
meets the dead end one step later than they could.

**Fixed 2026-08-20 on `debt-wave-b1`**, and the measurement came first.

**The census, in full** — every `.lha` in the owner's package folder, walked
by a parser that is not ART's (`scripts/lha-package-identity.py`, new, sharing
`lha-header-census.py`'s header reader), asked what top-level directory ART's
rule reads from inside it:

| archive | top-level directory read from inside |
|---|---|
| `3.2/AmigaOs 3.2/NDK3.2/…/wget-1.10.1/doc.lha` | `doc` |
| `3.2/Update3.2.2.lha` | `Update3.2.2` |
| `Amelinium.lha` | *refused: no top-level directory* |
| `AmiSSL-v5-OS3.lha` | `AmiSSL` |
| `AmiSpeedTest.lha` | `AmiSpeedTest` |
| `BoingBag39-1.lha` | `BoingBag3.9-1` |
| `BoingBag39-2-Contribution.lha` | **`BoingBag3.9-2`** |
| `BoingBag39-2-deutsch.lha` | **`LocaleUpdate`** |
| `BoingBag39-2-francais.lha` | **`LocaleUpdate`** |
| `BoingBag39-2-greek.lha` | **`LocaleUpdate`** |
| `BoingBag39-2-italiano.lha` | **`LocaleUpdate`** |
| `BoingBag39-2-polski.lha` | **`LocaleUpdate`** |
| `BoingBag39-2-portugues-brasil.lha` | **`LocaleUpdate`** |
| `BoingBag39-2-portugues.lha` | **`LocaleUpdate`** |
| `BoingBag39-2-turkce.lha` | **`LocaleUpdate`** |
| `BoingBag39-2.lha` | **`BoingBag3.9-2`** |
| `Dopus5_MagellanII/DopusPDF/dopus_5_pdf_manual.lha` | `dopus_5_pdf_manual` |
| `Emu68MeterAlpha2.lha` | `Fonts` |
| `Ethernet.lha` | *refused: no top-level directory* |
| `Euro-Update.lha` | `Euro-Update` |
| `IconLib_46.4.lha` | `IconLib_46.4` |
| `JanoEditor.lha` | `Jano_v1.01` |
| `LayersBenchMark.lha` | `LayersBenchMark` |
| `MCP/mcp1_48.lha` | `MCP1.48` |
| `MCP/mcp1_49cu.lha` | *refused: no top-level directory* |
| `MUI-5.0-20210831-os3-contrib.lha` | *refused: 4 (Documentation, MUI, ReleaseNotes, SDK)* |
| `MUI-5.0-20210831-os3-debug.lha` | *refused: no top-level directory* |
| `MUI-5.0-20210831-os3.lha` | *refused: 2 (MUI, SDK)* |
| `MUI-5.0-20210831-os4-contrib.lha` | *refused: 4 (Documentation, MUI, ReleaseNotes, SDK)* |
| `MUI-5.0-20210831-os4-debug.lha` | *refused: no top-level directory* |
| `MUI-5.0-20210831-os4.lha` | *refused: 2 (MUI, SDK)* |
| `NDK39.lha` | `NDK_3.9` |
| `OS39FAQ-english.lha` | `Archives` |
| `Picasso96/Picasso96-2_1b.lha` | `picasso96install` |
| `PoseidonV45.lha` | `PoseidonV4` |
| `SnoopDos.lha` | *refused: no top-level directory* |
| `amipkg.lha` | *refused: no top-level directory* |
| `bebbossh.lha` | `bebbossh` |
| `changebootpri.lha` | *refused: no top-level directory* |
| `hippoplayer.lha` | *refused: 2 (HippoPlayer, HippoSupport)* |
| `mui38usr.lha` | `MUI` |
| `netio-1.33r4.lha` | `netio` |
| `pfs3aio.lha` | *refused: no top-level directory* |
| `zenPrismWifi_v0.4.lha` | `zenPrismWifi` |

44 archives, and exactly two names are claimed more than once: `LocaleUpdate`
by eight, `BoingBag3.9-2` by two. Everything else is unique, which is why the
identity rule was never in doubt — it is right, and it is not enough.

**What actually distinguishes the eight, and it is not the filename.** The
same walk compared them entry by entry. Exactly **four** entries are common to
all eight — `LocaleUpdate.info`, `LocaleUpdate/Install Locale` and its icon,
and `LocaleUpdate/getlocale` — and each archive carries **one distinct**
`locale/catalogs/<language>` drawer: `türkçe`, `deutsch`, `français`,
`italiano`, `polski`, `português`, `português-brasil`, `greek`. (Two of them
also bring `devs`, `fonts` and a `wbstartup` drawer — greek and polski — but
the language drawer is the fact common to all eight.) The two `BoingBag3.9-2`
claimants separate the same way: `BoingBag39-2.lha`'s second level holds
`AmigaOS-Update`, `C`, `Install`, `Installer`, `Manuals` and `XAD-Update`;
`BoingBag39-2-Contribution.lha`'s holds `Contribution` and the readmes, and no
payload member at all.

So **every collision in the owner's real folder is resolvable from inside the
archive**, and the tiebreak keeps the whole point of the identity rule rather
than trading it away for a filename match.

**The fix.** A package recipe may declare a second identity fact —
`Package::distinguished_by`, a path that must exist inside the archive below
its top-level directory — and `scan::package_for` takes it as a second filter.
`locale-turkish` declares `locale/catalogs/türkçe`, `boingbag-39-2` declares
`AmigaOS-Update`, and `boingbag-39-1` declares none, because its top-level name
is unique across all 44 archives and a condition that separates nothing can
only refuse an archive that would have worked.

Three properties that are the design rather than the implementation, each with
its own doc comment in `package.rs`:

1. **The second filter runs whether or not `media` was ambiguous.** That is
   the worse half of the same bug: with only `BoingBag39-2-deutsch.lha` in the
   folder there was exactly *one* candidate, so `package_for` answered `Found`
   and the Turkish package resolved to a German archive with no ambiguity to
   warn anybody. That case is now `Missing`, which is true.
2. **Narrowing never picks a winner.** More than one survivor is still
   `MediaMatch::Ambiguous` over the narrowed list, and the caller still turns
   it into a refusal naming exactly those candidates.
3. **`member` is deliberately not reused as the distinguisher**, even though
   `boingbag-39-2` declares the same string twice. The same census shows why:
   `AmigaOS-Update` is carried by `BoingBag39-1.lha` *and* `BoingBag39-2.lha`,
   so as an identity it separates nothing — it settles this pair only because
   the other claimant of `BoingBag3.9-2` happens to lack it. Folding the
   fields would encode a coincidence as a rule.

Matching is whole-path and case-insensitive, through `MediaSource::entry` so
there is one rule rather than a second copy of it — never a prefix, because
`português` and `português-brasil` are two of the real eight.

**Tests, and what each one actually guards** (re-measured in fix round 1 —
the claim that stood here, "all four fail", was wrong, and the review was
right about which one):

| test | fails with the filter reverted? |
|---|---|
| `eight_archives_claiming_localeupdate_are_separated_by_what_is_inside_them` | yes |
| `a_single_candidate_of_the_wrong_variant_is_missing_not_found` | yes |
| `an_ambiguity_the_distinguisher_cannot_settle_still_names_both` | yes |
| `a_drawer_spelled_in_upper_case_is_the_same_drawer` (F1) | yes |
| `an_archive_that_cannot_be_reopened_does_not_satisfy_the_distinguisher` (F7) | yes |
| `an_empty_declared_drawer_does_not_satisfy_the_distinguisher` (F8) | yes |
| `only_the_packages_with_a_shared_top_level_directory_declare_a_distinguisher` | **no** |

Measured by replacing the second filter with a no-op and running the whole
suite: `1945 passed; 6 failed`. The seventh row guards the **shipped JSON** —
that `locale-turkish` and `boingbag-39-2` declare a distinguisher and
`boingbag-39-1` does not — and a data assertion cannot fail when the code
that reads the data is disabled. It is still worth having, and it is not
evidence about the filter; counting it as such was the same mistake as
writing a number nobody re-ran.

One of these was vacuous when first written:
`an_ambiguity_the_distinguisher_cannot_settle_still_names_both` used two
identical Turkish archives, which are ambiguous with or without the filter.
A German pack was added beside them and the assertion widened to require it
*not* be named.

**Fix round 1 (2026-08-20) — three defects the review found in the fix
itself, all of which let an archive pass a check it should not have.**

**F1, and it is the fifth exact-case defect in two days.** The fold both
halves of the identity comparison used was `eq_ignore_ascii_case`, and the
distinction is not ASCII: **four of the owner's eight language drawers are
non-ASCII** (`türkçe`, `français`, `português`, `português-brasil`). The
reviewer's case — an archive spelling its drawer `TÜRKÇE` against a recipe
spelling it `türkçe` — answered `Missing`, while the ASCII pair
`POLSKI`/`polski` answered `Found`.

**This is not a hypothetical spelling, and finding that out corrected an
earlier measurement too.** Read with ART's own `CdSource` rather than with
7-Zip, `AmigaOS39.iso` carries **no Joliet descriptor at all** — its
descriptor chain is `[Primary, Primary, Terminator]` — so ART reads the
Primary tree and the disc answers `OS-VERSION3.9/LOCALE/CATALOGS/TÜRKÇE`,
upper case. `locale-turkish.json`'s own note said the disc spelled it
`türkçe`, "measured with 7z"; 7-Zip lower-cases plain ISO9660 names as a
display convention, so the first measurement disagreed with the reader that
actually does the work. That note is corrected in the same commit.

The consequence reached further than ART-167: `destination_key` folded
ASCII-only too, so the base component's `Locale/Catalogs/TÜRKÇE/x.catalog`
and this package's `Locale/Catalogs/türkçe/x.catalog` were **two different
destinations**. The ~34 overlapping catalogs [ART-169](#fixed) predicted would
appear once ART-168 was fixed were still going to read as zero, for a second
and unrelated reason.

Fixed with **one** fold, `core::osinstall::fold_amiga_case` /
`amiga_names_equal`, applied at every site in this module tree that compares
one AmigaDOS name to another: `same_identity`, `ArchiveSource::find_by_path`
and its implicit-directory dedup, `CdSource::find_by_path`,
`starts_with_ignoring_case`, `plan::relative_to`, and `destination_key` /
`same_destination`. It is the inverse of `hash::intl_to_upper` — ASCII plus
`0xC0..=0xDE` except `0xD7` — expressed over `char`, because Unicode's first
256 code points *are* Latin-1, which is the same identity `core::lha`'s
ART-168 fix rests on. **International was chosen, not read**: there is no
bootblock behind an archive entry name to ask, the owner's own names need the
fold, a modern AmigaOS volume is an INTL one, and folding more can only merge
names AmigaDOS already treats as one. The reasoning is in that function's own
doc comment, which is what the old ASCII-only choice never had.

**F7 — the distinguisher check failed open.** An archive that would not
re-open answered `true`, on the reasoning that leaving a candidate standing
keeps a *pair* `Ambiguous`. That only held for a pair: with a single
candidate it restored ART-167 exactly, resolving a package to a file nobody
could show was its own. It fails closed now — the package reports `Missing`,
a refusal that names it.

**F8 — an empty declared drawer satisfied the distinguisher.** A repacked
archive declaring `locale/catalogs/türkçe/` and holding nothing under it
passed a check that exists to say the archive carries that language's
catalogs. A directory must now hold at least one entry beneath it.

**F1's own doc-comment claim was false and is corrected**: `same_identity`
said "ASCII-only, matching `eq_ignore_ascii_case`", which was true of the
code and wrong about every case that mattered.

**A fixture was corrected in the same commit, and it is why the bug could hide.**
`commands::osinstall::tests::write_locale_turkish_archive` wrote
`LocaleUpdate/locale/catalogs/x.catalog` — a catalog sitting directly in
`catalogs`, with no language drawer at all. No real language pack looks like
that, and a fixture without the drawer could never have shown the collision.
It is now a real level-0 LHA with Latin-1 names, the shape all eight of the
owner's archives actually have (40 entries, 40 files, **no directory entries
at all**, so the drawer exists only because
`ArchiveSource::with_implicit_directories` synthesises it).

**What is unchanged:** no user-visible string was added, because the refusals
this narrows to (`PackageArchiveMissing`, `PackageArchiveAmbiguous`) already
existed with their own translated sentences.


**ART-087** 🔵 **Space marks a row but does not compute a directory's size** —
*found while building phase 2b task 5; fixed 2026-08-20 on `debt-wave-a`*
`src-tauri/src/core/dirsize.rs` (new) ·
`src-tauri/src/commands/panel.rs` (`panel_directory_size`) ·
`src-tauri/src/commands/volume.rs` (`volume_directory_size`) ·
`src/lib/panel.ts` · `src/pages/FileManager.tsx`

The brief (§3.2) asks for Total Commander's `CountSpace=1`: Space on a
**directory** marks it *and* walks it, replacing the `<DIR>` in the Size column
with the real total. ART marked and did not count, because there was no
primitive to count with — `panel_list_local` lists one level,
`scan_collection_directory` looks for Amiga files rather than totalling bytes,
and `volume_plan_copy` computes a size only against a destination volume.

**Fixed 2026-08-20**, as the entry itself scoped it: one primitive, one command
per side of the fence, both as jobs, and a third state in the Size column.

`core::dirsize` is the primitive — `host_total` and `volume_total`, one module
because the *answer* is the same shape whichever side asked. Both are bounded
at 32 levels and neither follows a symlink (`symlink_metadata`, the ART-028
rule), both take a `ProgressSink` and check `is_cancelled` between whole
entries, and neither writes anything.

**A total that stopped short says so.** `DirTotal::partial` is set when the
walk hit its depth cap or could not read a directory it met, and the number is
then a **floor**, not a total — rendered as `≥ 1,234` with a tooltip rather
than as the answer. A Size column printing a floor as a total would be quietly
wrong by however much it did not look at, which is exactly the silence ART-107
was about on the layout side. A subdirectory ART cannot read makes the count
partial rather than failing the whole drawer: a floor that admits it is one
beats refusing to count a tree because one corner is locked.

`panel_directory_size` and `volume_directory_size` return a job id (§54) and
answer on one shared `dir-size-result` event. Neither writes, so neither
touches the operation log. The volume one resolves the image and index
*before* opening the job, so a bad image is an error the caller sees rather
than a job that opens and immediately fails.

The screen keys a count by pane **and** row (`dirSizeKey`), and matches the
answer back by **job id**, not by key — a user who walked into another folder
while the count ran must not have it land on whatever row now holds that key. A
cancelled or failed job emits no result, so the job's own terminal state is
what drops the row back to `<DIR>` instead of leaving it saying "counting…" for
the rest of the session.

**F4, fix round 1 — the job id was registered after the answer could already
have arrived.** The screen invoked the command, and registered the returned job
id inside `.then()`. Rust's `spawn_job` starts its thread **before** the command
returns, so a small folder finishes and emits while the frontend is still inside
`await invoke(...)`: the listener saw an id it did not know yet, dropped the
event, and the row said "counting…" for the rest of the session. That is the
same race `src/lib/jobs.ts::awaitJobResult`'s own doc comment records finding in
`osinstallCollisions`, in a new place, and the helper exists precisely for it.

`countHostDirectory` / `countVolumeDirectory` now wrap `awaitJobResult`, which
subscribes *first*, buffers anything arriving before the id is known, and
matches retroactively. They reject on a failed or cancelled job, which retired
both of the screen's hand-rolled listeners **and** the job-id map: the component
awaits an ordinary promise and never sees a job id. The Rust payload lost its
`rename_all` so `job_id` travels snake_case, matching `LayoutResult` and the
`TPayload extends { job_id: number }` bound the helper is written against.

**ISO and archive panes are not counted, and say nothing rather than
pretending.** `dirSizeKey` returns `null` for a row with neither a host path
nor a header block, so Space there does exactly what it did before — marks, and
nothing else. There is no command that counts one, and a key for a job that
will never start would leave the row counting forever (§89).

Tests: seven in `core::dirsize::tests` — a host tree totalled at every depth, a
tree past the cap reporting a floor with `partial` set, a file refused as not a
folder, cancellation returning `Cancelled` rather than a short number, a real
FFS volume built in a tempdir totalled whole and by subdirectory, and a block
that is not a directory coming back partial rather than fatal. On the frontend,
`src/lib/panel.test.ts` covers `dirSizeKey` (per-pane keying, files refused,
uncountable rows refused, block `0` not confused with no block) and
`dirSizeCell`'s three states including `partial`. New keys `files.tc.counting`
and `files.tc.partialCount` in both catalogues.

**ART-101** 🔵 **The sidebar's collapse never fires under Application Size** —
*found 2026-08-13 while closing ART-099; fixed 2026-08-20 on `debt-wave-a`*
`src/lib/appZoom.ts` (`shellWidth`, `shellWidthClasses`) ·
`src/components/layout/layout.css` · `src/components/layout/Layout.tsx`

`@media (max-width: 1000px)` collapsed the sidebar to icons, "below this the
sidebar's labels cost more than they give". Media queries are evaluated against
the **real viewport**, and Application Size is a `zoom` on an element inside it
— so the rule asked a question about a window the layout does not live in.
Measured while closing ART-099: in a 1258 px window the sidebar is 224 real px
at 100 %, 291 at 130 % and **448 at 200 %** — over a third of the glass — while
the layout itself has only 629 CSS px to work with, well under the 1000 the
design says it wants icons at. The breakpoint the design already agreed on was
exactly the one that could not fire when it was most needed.

**Fixed 2026-08-20** the way the entry itself said it should be: the question
is asked of `innerWidth / zoom`, and the answer arrives as a class rather than
a media query. `shellWidthClasses` (pure, in `@/lib/appZoom`, so it is pinned
by a test rather than by squinting at a screenshot) returns
`app-shell-narrow` below 1000 and `app-shell-tight` below 760; `Layout.tsx`
tracks `innerWidth` on `resize` and puts them on `.app-shell`, with `zoom` as a
dependency so changing the Application Size re-evaluates immediately — the case
the whole issue is about. **Both thresholds are unchanged**; only the width
they are compared against is, so this changes when the design's own decision
fires and never what it decides.

Tests: `src/lib/appZoom.test.ts` — `shellWidth` (including that an impossible
zoom is clamped rather than divided by) and `shellWidthClasses`, whose central
case is the measured 1258 px window: nothing at 100 %, `app-shell-narrow` at
130 % and both classes at 200 %, where the old media query said "no" at all
three.

Two other rules in the codebase have the same defect at the same threshold;
filed as [ART-174](#fixed) rather than changed here, because the file manager's
is not the same question (see that entry).

**ART-107** 🔵 **`scan::gather` drops silently at the depth cap, and counts an
overlapping input twice** — *found 2026-08-15, the whole-branch review of SD-2
G11; fixed 2026-08-20 on `debt-wave-a`*
`src-tauri/src/core/layout/scan.rs` · `src-tauri/src/core/layout/mod.rs` ·
`src/lib/layout.ts` · `src/pages/ContentLayout.tsx`

Two ways the plan could quietly not describe what the user dropped. `walk`
returned `Ok(())` past `MAX_SCAN_DEPTH` and `tree_bytes` returned `0`, so files
below the cap were absent from the plan with nothing on screen saying so, and a
drawer's size could read low. And `gather` did not dedupe: adding a folder and
then a file inside it — both of which the screen allows — yielded the same file
twice and a self-collision the user could only resolve by removing a source.

**Fixed 2026-08-20**, both halves.

`gather` now returns `Gathered { found, too_deep, duplicates }`. Each of the
last two is a `Dropped { paths, more }` — the first twenty named, the rest
counted, the same "name what you can act on, count the rest" shape
`CoreError::NonAsciiPfs3Names` already uses, so a tree with nine thousand deep
branches does not put nine thousand paths on screen and does not claim there
were twenty. `LayoutPlan` carries both through to the UI, which renders them as
two new sections beside the existing Collisions and Not-included ones.

`walk` and `tree_bytes` both record the folder they stopped at, so a drawer
whose size reads low says why — the number on screen is what a user decides
against. It still does **not** refuse: a scan is a preview, and refusing to
preview a whole folder because one corner of it is 33 levels down would be
worse than showing the rest and naming the corner. (The *copy* path still
refuses, which is right — `apply::copy_tree` was always correct here.)

Deduping is by `std::fs::canonicalize`, so a folder and a file inside it are
recognisably one thing whatever spelling reached the drop; a path the OS will
not canonicalize falls back to itself, which is no worse than the old
behaviour for that one entry and never an error for a scan that is otherwise
fine. The **first** sighting is the one kept, and the tests ask it in both
orders, because a dedupe that only worked one way round would pass a single
test.

**Neither report blocks apply.** A plan short in one corner, or one whose
source list named something twice, is still worth applying — the defect was the
silence, not the event. `layoutBlocker` is unchanged and there is a test saying
so, so nobody "tidies" these into blockers later by accident.

Tests: `core::layout::scan::tests::scanning_stops_at_the_depth_limit` (extended
to assert the report, not just that it returned),
`::a_drawer_deeper_than_the_cap_says_its_size_is_short`,
`::a_file_inside_a_folder_that_was_also_added_is_kept_once`,
`::the_same_overlap_the_other_way_round_is_also_kept_once`,
`::an_ordinary_scan_reports_nothing_dropped`,
`::the_report_is_bounded_and_counts_the_rest`, and on the frontend
`src/lib/layout.test.ts`'s `droppedTotal` and
`layoutBlocker and the ART-107 reports` blocks. New keys `layout.tooDeep.*`
and `layout.duplicates.*` in both catalogues, covered by
`src/i18n/literal-keys.test.ts`.

**ART-105** 🔵 **`size()` is written three times** — *found 2026-08-15, the
whole-branch review of SD-2 G11; fixed 2026-08-20 on `debt-wave-a`*
`src/lib/size.ts` (new) · `src/pages/ContentLayout.tsx` ·
`src/components/osbuilder/VolumePreload.tsx` ·
`src/components/osbuilder/CardBuilder.tsx`

The same five-line `GIB` constant and `size()` byte formatter sat in three
screens — identical in `ContentLayout` and `VolumePreload`, and split into
`gb()`/`mib()` halves in `CardBuilder`, which prints the bare number and lets
a translated sentence supply the unit.

**Fixed 2026-08-20**: one `src/lib/size.ts` exporting `GIB`, `gibNumber`,
`mibNumber` and `size`. `CardBuilder` imports the two bare-number helpers
under its own names, so its call sites are untouched; the other two import
`size`. Nothing about what any screen prints changed — the test pins the
values the three copies produced, not the values the new function happens to.

**Not folded into `panel.ts::formatBytes`, and the module comment says why.**
That one prints a *file's* size for a directory listing (B / KB / MB); these
print a *volume's*, where the interesting range starts in the hundreds of
megabytes and runs to hundreds of gigabytes. Two formatters because there are
two questions.

Test: `src/lib/size.test.ts` — the first test any of the three copies has had,
covering the GB/MB boundary at exactly one gibibyte, both roundings, and that
the bare-number helpers carry no unit.

**ART-158** 🔵 **`CoreError::Malformed` covered two different failure classes
for an ISO9660 disc — a corrupt structure, and a disc merely larger than
`CdSource`'s own walk limits** — *found 2026-08-19, Task 1, ruled parked
rather than fixed; fixed 2026-08-20 on `debt-wave-a`*
`src-tauri/src/core/error.rs` (`CoreError::LimitExceeded`) ·
`src-tauri/src/core/osinstall/source_cd.rs` (`CdSource::open`)

`IsoImage::walk()` can stop short of the whole disc and still return `Ok` with
whatever it found, because its other caller — a file-manager listing — would
rather show a partial tree than nothing. An install source must not, so
`CdSource::open` checks `walk.truncated`/`walk.depth_limited` and refuses. It
refused with `CoreError::Malformed { format: "iso9660", .. }`, so
`ART-FORMAT-MALFORMED` was the identifier a user saw for both a genuinely
damaged disc and one that merely holds more than `CdSource` will read (100,000
entries / 16 levels of nesting). The disc that refusal exists for is **valid**,
and the two failures want opposite answers from whoever reads them: "this
medium cannot be used" against "ART's own limit, which could be raised".

**Fixed 2026-08-20** with a new variant, `CoreError::LimitExceeded { subject,
detail }` → **`ART-LIMIT-EXCEEDED`**. `subject` names the limit in ART's own
terms (`"iso9660 walk"`) and `detail` says what it is and what the consequence
would have been. **Only the two limit cases moved.** Nothing existing was
renumbered — `code()`'s ids are user-facing and stable, so `Malformed` keeps
`ART-FORMAT-MALFORMED` and every other caller of it is untouched, including
`IsoImage::open`'s own errors one line above the two that moved.

Still scoped to `CdSource`. `core/iso`'s other caller (`IsoSource`) does not
refuse on these flags at all, which is a different gap in a different module's
contract and is deliberately still not this issue's.

Tests: `core::osinstall::source_cd::tests::a_disc_deeper_than_art_will_walk_is_a_limit_not_a_malformed_disc`
(a real 20-level ISO built in a tempdir, asserting `code() ==
"ART-LIMIT-EXCEEDED"` — the identifier, not just the variant, since the id is
what a user quotes) and
`core::osinstall::source_cd::tests::a_disc_that_is_damaged_is_still_malformed`
(the other side of the boundary: a truncated disc is still
`ART-FORMAT-MALFORMED`, so one class has not swallowed the other).
`core::error::tests::every_variant_has_a_distinct_code` covers the new id.

**ART-173** ✅ 🟡 **`core::cbm`'s and `core::detect`'s test scratch
directories can be shared by two threads, so one test reads another's
fixture — measured at 4 failures in 40 runs** — *found and fixed 2026-08-20 on
`debt-wave-a`, while verifying ART-115*
`src-tauri/src/core/cbm/d64.rs` (`write`) · `src-tauri/src/core/cbm/t64.rs`
(`write`) · `src-tauri/src/core/detect.rs` (`tmp`)

This is [ART-164](#fixed) again, in three more modules, and it was found the
way that one was: a full-suite run failed on
`core::cbm::d64::tests::a_file_that_is_not_a_disk_image_is_refused_at_open`
while closing ART-156, in code that run's diff did not touch.

`write()` keyed its directory on process id plus a nanosecond timestamp and
every caller then wrote to the same `disk.d64` inside it. Cargo runs these
tests in parallel threads of one process and `SystemTime::now()` on Windows
does not advance between two calls landing in the same clock tick, so two
tests share one file.

**Measured, not inferred: 4 failures in 40 runs of `cargo test core::cbm::`,
across two different tests**, with the decisive one naming the mechanism
outright —

```
---- core::cbm::d64::tests::a_disk_reports_its_name_and_id stdout ----
called `Result::unwrap()` on an `Err` value: UnsupportedFormat("1000 bytes is
not a Commodore disk image ART recognises. …")
```

— 1000 bytes being exactly the fixture
`a_file_that_is_not_a_disk_image_is_refused_at_open` writes, read under
another test's name. The other three sightings were that test failing on its
own assertion, which is the same collision from the other side.

`core/cbm/t64.rs::write` has the identical shape (`tape.t64` in a directory
keyed the same way) and `core/detect.rs::tmp()` takes no tag at all, so pid
plus a timestamp was the only thing keeping its twenty-odd callers apart.
Both were fixed with the same line rather than waiting for them to be seen.

**Fixed 2026-08-20** by adding an atomic counter to all three, the same shape
`core/iso`, `core/gameindex` and `core/layout` already use. Verified the way
the defect was found: **40 consecutive runs of `cargo test core::cbm::`, zero
failures** (against 4 in the 40 that measured it), and 20 of
`cargo test core::detect::`, zero failures.

**F8/F9, fix round 1 — the sweep is finished, and this branch had been adding
to the problem while fixing it.** Round 0 fixed three helpers and filed the
rest as owed; it had also *introduced* six new pid-keyed scratch directories of
its own, in the same round. Both halves are now closed.

`crate::core::test_scratch_id()` (test-only, in `core/mod.rs`) returns the
process id **plus** a process-wide atomic counter, as a `String`. Every scratch
helper already formatted `std::process::id()` with `{}`, so the sweep is a
one-token substitution at each site — no format string changed, and every edit
reads as a one-word diff.

**How the sites were found, so the next person can re-run it rather than
re-trust it** (`scripts/scratch-counter-sweep.py`; `--apply` rewrites, no flag
reports): walk every `.rs` file under
`src-tauri/src`; find every `std::process::id()`; treat a site as test code
when it falls after the file's first `#[cfg(test)]`; treat it as already safe
when `fetch_add` appears inside the enclosing `fn`. Result:

| | sites |
|---|---|
| total | 82 |
| production (skipped — all five already carry their own counter) | 5 |
| already had a counter | 7 |
| **rewritten** | **70** |

**R2, fix round 2 — that zero was not what it sounded like.** The script
grepped `std::process::id()` and nothing else, so roughly twenty helpers keyed
on `SystemTime::now()…as_nanos()` **alone** were invisible to it — and that is
the *worse* shape, because two threads can genuinely share a nanosecond
reading (the Windows clock is coarse) where they can never have different pids,
and unlike a bare pid it *looks* unique. `core/lha/safe_extract.rs`'s own
`scratch` was one. A script reporting zero while blind to the worse half is a
guard that passes vacuously, which is the class this round has now filed three
times.

The script searches both shapes now, and also refuses to touch an `as_nanos()`
that is not building a path (a timing measurement or a seed). Re-run after
widening:

| | sites |
|---|---|
| total | 55 |
| production (never touched) | 5 |
| already had a counter | 24 |
| **rewritten** | **26** |

All 26 were `as_nanos()`-only. Re-running now reports **0**, and that zero
covers both shapes.

One behaviour change fell out and is worth naming: `osinstall::fixtures::scratch`
used to be *stable* per tag — two calls with one tag returned the same
directory, cleared on entry — and a test asserted exactly that. That contract
**is** the ART-164 hazard, and `apply.rs`'s `planned()` already appended its own
counter to the tag to work around it. The helper is now unique per call and the
test asserts the safe property instead
(`scratch_gives_every_call_its_own_empty_directory`). Only that test reused a
tag; every other call site passes a distinct one.

**ART-115** 🔵 **A `core::iso` test flake, seen three times across this
session, never diagnosed** — *found 2026-08-15/16 (Tasks 3, 7, 8), filed at
Task 14; closed 2026-08-20 on `debt-wave-a` as fixed by commit `7e77609`*
`src-tauri/src/core/iso/mod.rs`

`extract_tree_does_not_follow_a_directory_that_points_back_at_the_root` failed
on one of several full-suite runs on three separate days, always in
`core::iso`, always passing in isolation, and never in code any of those
tasks' diffs touched. Two deliberate reproduction attempts (nine consecutive
green runs on the machine that produced all three sightings) failed to provoke
it, and the entry recorded that negative result rather than dropping the
issue.

**It was ART-164 all along**, filed later and diagnosed properly: `core::iso`'s
`tmp()` keyed its scratch directory on pid plus a nanosecond timestamp, two
parallel tests could get the same one, and *any* test in the module was
exposed — re-measuring ART-164 found 5 failures across 40 runs and **four
different tests**, one of them comparing an accented volume name against
another fixture's disc. That is exactly this entry's symptom: a single named
test was only ever the one that happened to lose the race, which is why
quarantining it and running it in isolation both said nothing.

**Closed 2026-08-20 as fixed by `7e77609`** ("fix(iso): give each test its own
scratch directory"), which gave `tmp()` an atomic counter. Verified here with
the same measurement that closed ART-164: **40 consecutive runs of
`cargo test core::iso::`, zero failures**, on the branch this wave is built
on. No separate fix was needed and none was invented.

Test: the module's own 56 tests, run 40 times — the measurement *is* the
verification for a race, and there is no single test that can assert one.

**ART-156** 🟡 **`plan()`'s `total_bytes` counts a CD-sourced directory's own
ISO9660 extent length as if it were file content, so it overstates what
`apply()` actually writes to disk** — *found 2026-08-19, the Task 5 real run
that fixed ART-155's real cause and let `apply()` finish for the first time;
fixed 2026-08-20 on `debt-wave-a`*
`src-tauri/src/core/osinstall/plan.rs` (`content_bytes`)

`total_bytes` was `items.iter().map(|item| item.bytes).sum()`, unconditional
over every `PlanItem`, files and directories alike. `PlanItem::bytes` for a
directory sourced from an ADF is `0` (an AmigaDOS directory block carries no
byte-length field the way a file does), so the sum was never visibly wrong
before — but on a CD a directory *is* an extent, and `IsoEntry::bytes` hands
back its declared, sector-rounded length. `apply()` turns such an item into a
plain host folder with no content of its own, so those bytes are real to the
disc and imaginary to the distribution tree.

Measured against the owner's real AmigaOS 3.9 CD: `plan()` predicted
`total_bytes: 6,108,319` and `apply()` wrote `6,054,225` bytes of file
content — a difference of exactly `54,094`, the sum of `PlanItem::bytes` over
the plan's 75 directory items, with all 588 file items byte-exact.

**Fixed 2026-08-20.** `total_bytes` is the sum over items that are **not**
directories, computed by a named `content_bytes(&items)` rather than an inline
`sum`.

**F7, fix round 1 — the test that was supposed to escape the tautology was
still one.** The rewritten
`the_total_is_the_sum_of_what_will_actually_be_written` asserted
`plan.total_bytes == content_bytes(&plan.items)`, which evaluates the identical
expression `plan()` had just evaluated: it agreed with a broken `plan()` exactly
as readily as with a correct one, and both this entry and the round-0 report
claimed it asked "real arithmetic". It now asserts against the fixture's own
constant — every fixture file is `b"data"`, 4 bytes (`fixtures::entries_for`) —
times the number of *file* items, and pins the literal `44` across `11` files so
a recipe change is a failure somebody looks at. Finding the real number was
itself the point: the tautological version passed while the true total was 44
and nobody knew. The field's doc comment now says what the number is *for*: a
progress bar's total, measuring progress through bytes written, so it counts
what gets written.

`build_the_real_39_tree_when_asked` (the disc-gated hook) asserted against the
sum of *file* items only, with a comment saying why; that work-around is gone
and it now asserts `outcome.bytes == planned.total_bytes` directly.
`find_the_directory_byte_overcount_when_asked` is kept as the evidence it
always was — re-running it should now print `total_bytes` equal to
`sum_file_bytes` with `sum_dir_bytes=54094` beside it.

Tests: `core::osinstall::plan::tests::a_directory_item_s_own_extent_length_is_not_content`
(a hand-built item list carrying the shape a disc produces, so the arithmetic
is pinned without the owner's 469 MiB disc) and
`core::osinstall::plan::tests::the_total_is_the_sum_of_what_will_actually_be_written`,
now asking `content_bytes` rather than restating it. The disc-gated
`core::osinstall::apply::tests::build_the_real_39_tree_when_asked` carries the
direct comparison against the real disc.

**ART-160** 🟡 **`osinstall::apply()` writes host filenames without going
through `windows_safe_name`, and the one machine that measured a reserved
device name is not every machine** — *found 2026-08-19 by the whole-branch
review (finding 15); fixed 2026-08-20 on `debt-wave-a`*
`src-tauri/src/core/osinstall/mod.rs` (`host_destination`, `host_relative`) ·
`src-tauri/src/core/osinstall/apply.rs` (`FileRecord::host_path`) ·
`src-tauri/src/core/preload/amiga_names.rs` · `src-tauri/src/tools/hst_imager.rs`

`copy.rs` holds the list of 22 names Windows reserves for devices (`AUX`,
`CON`, `NUL`, `COM1`…) and a function that renames around them. `apply()` did
not call it: every destination went through `safe_join` — the security
boundary, never in question — and then straight to the filesystem under
whatever name the media carried.

**What the round trip turned out to be, measured before anything changed.**
The entry warned that a rename "has to be recorded, not just performed", and
the reason is sharper than it first looked: `core/preload/native.rs`'s
`collect_into` builds the AmigaDOS path it copies onto the card **out of the
host filenames it walks**, so a tree that quietly stored `_AUX` would have put
`_AUX` on the Amiga volume — and `verify_volume`, which looks the manifest's
`path` up on that volume, would then fail a file that is really there under
the wrong name. `verify` reads the *volume*, not the host tree, so the
manifest's `path` must stay the AmigaDOS name; nothing about verify wanted the
host name.

**Fixed 2026-08-20**, in three parts:

1. **One placer, one rule.** `osinstall::host_destination` is now the only way
   a destination becomes a host path, and `apply()`, `undeclared_overwrites`
   and `collide::classify_incoming` all go through it (the last two mattered:
   both ask `target.is_file()` about a tree `apply` wrote, and asking under
   the un-escaped name would have reported "nothing there" for a file that
   is). It asks the two questions in the order
   `commands/volume_write.rs::folder_destination` already established —
   containment of the **raw** name first, because `windows_safe_name` turns
   `..\..\Startup` into a name that passes containment trivially, then host
   legality of the escaped one — and escapes **segment by segment**, since
   `windows_safe_name` maps `/` to `_` and would otherwise flatten
   `Storage/DOSDrivers/AUX` into one filename.
2. **The Amiga name is recorded, not lost.** `FileRecord::path` is still
   always the AmigaDOS path; `FileRecord::host_path` (`hostPath`,
   `#[serde(default, skip_serializing_if)]`) records where the file actually
   landed, and only when the two differ — a tree with nothing to escape
   carries no `hostPath` at all, pinned by a test.
3. **The name reaches the volume.** `core/preload/amiga_names.rs` reads that
   pairing back and hands `collect_into` the AmigaDOS name per node, so a
   renamed *drawer* translates once for everything beneath it. It declares its
   own two-field record rather than importing `core/osinstall`'s manifest type
   — the lower module reads only what it needs and serde ignores the rest,
   the shape CLAUDE.md prescribes. A folder with no manifest, or one that will
   not parse, renames nothing.

**F5/F6, fix round 1 — escaping was many-to-one, and the second write won
silently.** The round-0 fix protected a name the host cannot store, and in
doing so created a way to *merge* two names into one. `windows_safe_name` maps
every refused character onto the single replacement `_`, and prefixes a
reserved device name with one, so a medium holding two genuinely different
names escapes both onto one host file:

| on the medium | on the host |
|---|---|
| `Devs/Prices: 1993` | `Devs/Prices_ 1993` |
| `Devs/Prices? 1993` | `Devs/Prices_ 1993` |
| `Storage/DOSDrivers/AUX` | `Storage/DOSDrivers/_AUX` |
| `Storage/DOSDrivers/_AUX` | `Storage/DOSDrivers/_AUX` |

`apply` writes items in order and `atomic_write` replaces, so the second of a
colliding pair **silently overwrote the first**: the tree held one file where
the media held two, and `distribution.json` recorded both, each claiming the
same `hostPath`. F6 is that pair's other end — `core/preload` resolves the map
by host path, so the single survivor would then be copied onto the volume under
whichever AmigaDOS name won, renaming a genuine `_AUX` to `AUX`.

Fixed by `osinstall::host_name_collisions`, called from **both** entry points
(`apply` and `add_package`) before a byte is written or a medium is opened. It
refuses by name — "'Devs/Prices: 1993' and 'Devs/Prices? 1993' both become
'Devs/Prices_ 1993'" — the same shape an undeclared overwrite is refused in.
There is no correct silent answer: renaming further would invent a name no
medium carried, so the plan is refused and the user is told which two files
clash. Two spellings of the *same* destination are not a collision (the 3.9
disc spells `C/ASSIGN`, a BoingBag `C/Assign` — that is the ordinary overwrite
`FileRecord::overwrote` already records), and directories are merge points, not
claims, the same rule `detect_collisions` and `undeclared_overwrites` follow.

**R1, fix round 2 — the refusal was keyed exact-case while the comparison
beside it folds case, so the narrowest pair walked straight through.**
`host_name_collisions` compared destinations with `same_destination`
(`eq_ignore_ascii_case`) but keyed its map on the host path **as spelled**.
`Devs/Prices: 1993` claimed `Devs/Prices_ 1993` and `Devs/prices? 1993` claimed
`Devs/prices_ 1993` — two keys for one file on a case-insensitive filesystem —
so the pair never met in the map at all. Reproduced end to end by the reviewer:
`apply` returned `Ok`, wrote **one** file and recorded **two**. Three-way,
escaped-against-literal and subdirectory collisions were all caught; this was
the one hole.

Keyed on [`destination_key`](#) now, which is what the comparison already uses
and what that function's own doc comment warns about: "a `HashMap` or
`BTreeMap` keyed on a raw destination is the same defect in a quieter form".
This round was bitten by an exact-case key **four separate times**; using the
existing helper rather than a fresh fold is what stops a fifth.

**And its corollary: two different drawers merging is refused too, reversing a
test that had blessed it.** The first version skipped directory items, quoting
the rule `detect_collisions` and `undeclared_overwrites` follow — "directories
are merge points, not claims" — and shipped a test asserting that
`Storage/AUX` beside `Storage/_AUX` is fine. That rule does not transfer:
those two ask whether *the same drawer* is claimed twice, which is ordinary;
this asks whether **two different drawers become one**, which is the same loss
as two files becoming one and reaches further — every file under both lands in
one host drawer, and `core/preload` resolves that drawer to a single AmigaDOS
name, so one whole subtree arrives on the volume under the other's name. The
`is_dir` flag is gone entirely: `same_destination` already tells "the same
drawer twice" from "two drawers becoming one", which is the question that
actually matters.

Round-2 tests: `::two_destinations_differing_only_in_case_still_collide` and
`::a_case_differing_pair_is_refused_by_apply_not_written_once_and_recorded_twice`
(the reviewer's own case, through `apply`, asserting the tree is never
created), `::two_different_drawers_that_escape_to_one_are_refused` (replacing
the test that asserted the opposite),
`::two_components_creating_the_same_drawer_is_not_a_clash` (the case the old
test was reaching for) and `::a_drawer_and_a_file_that_escape_to_one_name_are_refused`.
Both case tests were checked against the old key by reverting it: both fail,
both pass with `destination_key`.

Round-1 tests: `core::osinstall::apply::tests::two_destinations_that_escape_to_one_host_name_are_refused`
(end to end through `apply`, and asserting the tree is *not created at all* —
refused before anything is written, not an error after a partial write),
`::a_reserved_name_and_a_real_underscore_name_are_refused_together` (F6's own
pair), `::destinations_that_escape_to_nothing_do_not_clash`,
`::the_same_destination_spelled_twice_is_not_a_clash` and
`::two_components_claiming_one_drawer_is_not_a_clash`.

**And the fallback refuses rather than getting it wrong.** `hst-imager` copies
a folder exactly as it finds it and cannot be told a file's real name, so
`HstImager::copy_in` now refuses a tree whose manifest records any escaped
name — before the tool is launched, naming every `host → amiga` pair. That is
the same typed-capability-gap shape as `NonAsciiPfs3Names` and
`ForeignRdbEmbedNotSupported`, with the default and the fallback the other way
round: here it is the *fallback* that cannot. New error id
`ART-ESCAPED-NAME-NEEDS-NATIVE`; nothing existing was renumbered.

Behaviour change worth stating: `Storage/DOSDrivers/AUX` off the owner's real
AmigaOS 3.9 disc now appears in the tree as `Storage/DOSDrivers/_AUX`, and
`Devs/Prices: 1993` — a legal AmigaDOS name that NTFS refuses outright, so
`apply()` used to fail on it with a raw OS error partway through building a
tree — now lands as `Devs/Prices_ 1993`. Both reach the card under their
AmigaDOS names.

Tests: `core::osinstall::apply::tests::a_name_windows_reserves_is_escaped_on_disk_and_recorded_in_the_manifest`,
`core::osinstall::apply::tests::the_amiga_name_is_recoverable_from_the_tree_apply_wrote`
(the round trip, asked against a manifest `apply()` really wrote),
`core::osinstall::apply::tests::an_ordinary_tree_records_no_host_path`,
`core::preload::native::tests::an_escaped_host_name_is_copied_under_its_amiga_name`,
`core::preload::native::tests::a_folder_without_a_manifest_is_copied_verbatim`,
the five in `core::preload::amiga_names::tests`, and
`tools::hst_imager::tests::a_tree_with_escaped_names_is_refused_before_the_tool_runs`.

**ART-168** 🔴 **An LHA entry name's non-ASCII bytes are replaced with U+FFFD
rather than decoded, so a real Amiga drawer name becomes a name no Amiga can
see** — *found 2026-08-19 by Task 8's real run and confirmed on the booted
system, on `content-layer`; fixed 2026-08-20 on `debt-wave-a`*
`src-tauri/src/core/lha/mod.rs` (`entry_path`)

`entry_path` read a level-0/1 LHA header's name with
`String::from_utf8_lossy(&header.filename)`. Amiga archive names are Latin-1,
not UTF-8, so every high-bit byte became U+FFFD. Measured against the owner's
own `BoingBag39-2-turkce.lha`, whose payload sits at
`LocaleUpdate/locale/catalogs/türkçe/…` (bytes `74 FC 72 6B E7 65`): ART wrote
its 36 catalogs into `Locale/Catalogs/t<U+FFFD>rk<U+FFFD>e`, **beside** the
disc's own `TÜRKÇE` rather than onto it. `collide::preview` reported
`rows=0 upgrade=0 downgrade=0 same-version=0 unversioned=0` — a clean install
by every number the round measures, and wrong.

Confirmed from the booted system, not inferred: `dir` on `SYS:Locale/Catalogs`
listed **20** drawers where the host directory held 21. AmigaDOS could not see
the drawer at all, so all 36 catalogs were invisible to the system that was
supposed to receive them.

This was ART-155 again in the other reader: `core/iso/descriptor.rs::decode_iso646`
was corrected in Task 5 to decode a high-bit byte as ISO-8859-1 instead of `?`,
and the LHA path was never given the same treatment.

**Fixed 2026-08-20** by decoding the raw `filename` field as ISO-8859-1
(`b as char` over the whole 0x80..=0xFF range — Unicode's first 256 code
points *are* Latin-1, so no table is needed), the same decision and the same
reasoning `decode_iso646` already carries: AmigaDOS's own native character set
is ISO-8859-1. `entry_path`'s doc comment now says why, and says what U+FFFD
cost beyond looking wrong — it **merged distinct names**, since every high-bit
byte folded to the same replacement character.

**The header-level census — corrected in fix round 1, and the correction
matters more than the original claim did.** The first version of this entry
said "every non-ASCII name sits in a level-0 header, so this branch is the one
that carries them", and gave per-archive counts adding to roughly 120. That was
measured with a parser that read only the **first 200 KB** of each archive and
stopped after **200 entries** per file, and that desynchronised on level-2
headers (it added a level-2 header's size without its compressed data). It
therefore never saw most of the collection. Re-measured over every byte of
every entry:

| level | entries | non-ASCII name | `name\0comment` | drawer in a `0x02` header |
|---|---|---|---|---|
| 0 | 4,843 | 483 | 126 | — (the field holds the whole path) |
| 1 | 914 | 0 | 0 | **880** |
| 2 | 2,259 | 0 | 0 | 2,252 |

**8,016 entries across 44 archives** — 38 in the folder itself and 6 in
subfolders, which is the script's own output rather than a remembered figure;
the "41" carried through this round's earlier text came from an older task and
nobody had re-measured it (R3). The *conclusion* survived — all 483
non-ASCII names really are level-0, so the Latin-1 fix does cover every one of
them — but the evidence behind it did not, and it hid two further defects in
the levels the census had skipped ([F1](#fixed) and [F3](#fixed), both fixed in
round 1 and described under this entry). One number was also a false positive
worth recording: an early re-run reported 880 non-ASCII level-1 names, which
was the `0xFF` **separator** inside every `0x02` directory header being counted
as an accented character.

The census is reproducible rather than quoted: `scripts/lha-header-census.py`
walks all three header levels over a folder of archives and prints the table
above.

**Level 2/3 no longer percent-encodes either.** The original entry recorded
that `delharc`'s `parse_pathname_to_str` maps any byte outside 0x20..0x7E to
`%fc`-style escapes and left it alone as unmeasured. Reading the extension
headers for F1 made the same code path serve every level, so all of them now
decode Latin-1 through one function.

**F1 — a level-1 entry's drawer lives in extension header `0x02`, and was
being thrown away.** A level-1 header's `filename` field holds the **base name
only**; the directory is in a `0x02` extension header, separated by `0xFF`.
`entry_path` returned as soon as the raw field was non-empty, so it never
looked. **880 of the 914 level-1 entries** in the owner's collection lose their
drawer that way — all 316 of `AmiSSL-v5-OS3.lha`, all 283 of
`Update3.2.2.lha` (an AmigaOS update this engine exists to install), all 228 of
`IconLib_46.4.lha` — every one of them flattened into the archive root, on top
of each other. Pre-existing, and made *worse* by round 0's doc comment, which
described the level-0/1 branch as understood and correct.

Fixed by reading the extension headers for every level that has them, and
prepending the `0x02` directory. A level-1 entry with **no** `0x02` header is
not an error — it is a file at the archive root, which 34 of the 914 genuinely
are.

There is deliberately **no** "cannot resolve the drawer" refusal, and the
reason is recorded rather than assumed: `delharc`'s parser walks the whole
extension chain before building a header, validates each declared length
against the level-1 skip size or the level-2/3 long header length, and
propagates a short read (`parser.rs:316-329`), while `ExtraHeaderIter::next`
returns `None` only at length zero (`parser.rs:71-88`). A header that claims
extension headers and carries none cannot reach ART. Probed as well as read —
three mutations of a level-1 fixture are all rejected by `delharc`'s own
base-header checksum, since the next-header-size field sits inside the
checksummed region — so the guard that was written first was deleted as
unreachable rather than shipped as untestable.

**F3 — a level-0 name can carry an Amiga comment after a NUL.** Amiga LhA
stores `name\0comment` in the one field; `delharc` truncates at the NUL and
round 0's whole-field decode did not, so **126 entries** came back with a NUL
and the comment glued on — `BoingBag3.9-1\…\spatch` + `6.50 (26.8.93)`, and
similar throughout `BoingBag39-1`, `Euro-Update`, `hippoplayer` and
`mui38usr`. The name is now cut at the NUL, and the tail is **kept** rather
than discarded: an AmigaDOS file comment is real metadata, and losing it
quietly is what [ART-078](#fixed) was filed about on the ISO9660 side, and now
reads there too. It travels
as `LhaEntry::comment`. The `0x3F` comment *extension* header is deliberately
not read — no archive in the measured collection carries one (the types
present are `0x00`, `0x01`, `0x02`, `0x50`, `0x51`, `0x54`), and untested code
for an unmeasured case is worse than none.

**Two drafts of the path assembly were wrong the same way, and the tests
caught both.** The first split on every separator and dropped `.`/`..`
components the way `delharc` does — which turned `../../evil.txt` into
`evil.txt`: still contained by `safe_join`, but reported as a **successful
extraction** rather than a refused traversal. The second kept `..` and still
dropped *empty* components, which turned the absolute
`/art-oracle-root-escape.txt` into a relative name `safe_join` accepted, and
cost the archives oracle one of its three expected refusals. Both are the same
mistake — **normalising a hostile name into a benign one destroys the
report** — so the final version changes exactly one thing, `0xFF` to `/`, and
lets `safe_join` refuse everything else by name.

**The other two archive backends were checked the same way and are genuinely
different** — neither guesses a charset, because their formats state one:

- `core/archive/zip.rs` reads `entry.name()` from the `zip` crate, which
  follows APPNOTE.TXT: general-purpose bit 11 set means the name is UTF-8,
  clear means **CP437**, and the crate implements both branches
  (`zip-8.6.0/src/read.rs:526-530`, `from_cp437`). A replacement character can
  only appear when an archive *declares* UTF-8 and then stores invalid UTF-8,
  which is the archive lying, not ART guessing.
- `core/archive/sevenz.rs` reads `file.name`, which `sevenz-rust2` decodes
  with `String::from_utf16` (`sevenz-rust2-0.21.4/src/reader.rs:1229`) — 7z
  stores names as UTF-16LE by format definition, and a malformed one is an
  `Err`, not a mangled name.

Tests, round 0: `core::lha::tests::a_level_zero_name_s_high_bit_bytes_decode_as_latin1`
(the real `türkçe` bytes, end to end through `open_archive`),
`::two_names_differing_only_above_ascii_stay_two_names` (the collision the old
decode caused), and
`core::osinstall::source_archive::tests::art_168_an_lha_name_s_latin_1_bytes_arrive_decoded`
— the test that used to assert the *wrong* answer on purpose, rewritten to
assert the decoded name as its own doc comment demanded.

Tests, round 1: `core::lha::tests::a_level_one_entry_keeps_the_drawer_from_its_extension_header`,
`::a_level_one_drawer_is_split_on_0xff_and_decoded_as_latin1` (the two fixes
composing, not merely coexisting), `::a_level_one_entry_with_no_directory_header_sits_at_the_root`
(the legitimate 34-of-914 case), `::a_level_one_header_with_a_damaged_extension_area_is_refused`,
`::a_name_carrying_an_amiga_comment_is_split_at_the_nul`,
`::an_amiga_comment_is_latin1_too`, `::an_ordinary_entry_carries_no_comment`,
`::a_traversal_component_survives_assembly_for_safe_join_to_refuse` and
`::an_absolute_name_survives_assembly_too` (the two wrong drafts, pinned so
neither can come back). New fixture `make_level1_lha`, built byte-exact from
the level-1 layout.

**ART-164** ✅ **`core::iso`'s test scratch directory can be shared by two
threads, so *any* test in the module can read another's fixture — first
measured at about one full-suite run in thirty, and re-measured at four
different tests failing this way** — *found 2026-08-19 on `main`; re-measured
2026-08-19 while closing the content-layer round*
`src-tauri/src/core/iso/mod.rs:1316`

`tmp()` keys its directory on process id plus a nanosecond timestamp, and two
threads entering it close enough together get the same name, so one test reads
the other's fixture. This is ART-059 again in a different module —
`core/osinstall`'s own fixtures already solved it with an atomic counter, and
the fix here is the same one line.

**The blast radius is wider than this entry first said, and that is the part
worth correcting.** It was filed naming one test,
`a_mode2_form2_track_is_refused_rather_than_misread`. Re-measured by running
`cargo test core::iso::` forty times in a row, **five runs failed and four
*different* tests were the one that failed** — `a_descriptor_that_loses_its_identifier_is_an_error`
(twice), `a_directory_claiming_a_length_past_the_end_of_the_file_is_an_error`
(twice), and `a_minimal_iso_reports_its_volume_name_and_root`, which failed
with the clearest possible symptom of the mechanism:

```
assertion `left == right` failed
  left: "Amiga Tëst"
 right: "AMIGA_TEST"
```

— one test's disc read under another test's name. So there is no single
"flaky test" to quarantine; every test in the module that writes a fixture is
exposed, and which one loses the race is arbitrary. The rate depends on how
much other work fills the runner: a whole-suite run failed once in eleven,
while the module on its own — 56 tests and little else to interleave with —
failed five times in forty.

**Fixed 2026-08-19** by giving `tmp()` an atomic counter, the same shape
`core/gameindex` and `core/layout` already use. Verified the way the defect
was found: **40 consecutive runs of `cargo test core::iso::`, zero failures**,
against 5 failures across 4 tests in the 40 runs that measured it. The full
suite then ran three times at 1875 passed.
**It is now also the one thing standing between this branch and an honest
"the suite is green"**, so it should be the next thing fixed rather than the
smallest.

**ART-169** 🔴 ✅ **`workbench-base` placed only the disc's `Workbench3.5`
half, never its `Workbench3.9` overlay, so the tree ART called "AmigaOS 3.9"
booted as Workbench 44.5 with a Startup-Sequence that failed on its first
command** — *found 2026-08-19 by Task 8's real run and its boot; fixed the
same day, in Task 8's fix round 1*
`src-tauri/src/core/osinstall/recipes/amigaos-3.9.json` ·
`core::osinstall::apply::tests::layer_the_real_39_overlay_when_asked`

The owner's own `AmigaOS39.iso` carries **two** sibling install trees under
`OS-Version3.9`, and the ruling that fixed this came from measuring both
rather than from assuming the recipe pointed at the wrong one:

| | rows | top-level drawers |
|---|---:|---|
| `Workbench3.5` | 673 | `C Classes Devs Expansion Fonts L Libs Prefs Rexxc S Storage System T Tools Utilities WBStartup` |
| `Workbench3.9` | 854 | `C Classes Devs Fonts Libs Locale Prefs S Storage System Tools Utilities WBStartup` |

`Workbench3.9` has no `L`, no `Expansion`, no `Rexxc` and no `T`; its own `S`
holds `bg`, `fg`, `fork`, `History`, `SetFont`, `SetKeyboard` and three ARexx
scripts and **no `Startup-Sequence`**; there is no `Workbench3.9/C/Version`
at all; its `C` holds 29 commands where 3.5's holds 52, 12 of them
replacements and 17 new; and its `Libs/workbench.library` is 199,852 bytes
against 3.5's 193,400 — a different, larger file. It ships only what changed.

So it is an **overlay over `Workbench3.5`**, which is why a 3.9 CD carries a
`Workbench3.5` drawer at all, and the recipe was never installing the wrong
tree — it was stopping after the first layer. Fixed by adding a second
component, `workbench-39`, declared **last** (`plan()` emits items in recipe
order and the last writer wins), `required: true`, with
`overrides: ["workbench-base", "locale-base"]` and thirteen rules read off
the disc's own `Workbench3.9` listing rather than copied from
`workbench-base`'s.

**Measured, release build, against the owner's own disc** — the overlay is
854 items (792 files, 62 drawers), and the tree it produces:

| | files | drawers | bytes | elapsed |
|---|---:|---:|---:|---:|
| before (3.5 layer only) | 1257 | 156 | 10,003,017 | 10.57 s |
| after (both layers) | 1879 | 181 | 18,813,726 | 17.90 s |
| delta | +622 | +25 | +8,810,709 | |

**What the layer does to the files already there**, classified with
`collide::classify` — the same function `collide::preview` calls once it has
both sides' bytes:

```
upgrade=19  downgrade=0  same-version=0  unversioned=21
identical=130 (excluded by preview's own rule)  new files=622
```

`C/IPREFS 44.23 -> 45.9`, `Prefs/ICONTROL 44.11 -> 45.2`,
`Prefs/INPUT 44.19 -> 45.1`, `Prefs/OVERSCAN 44.13 -> 45.1`,
`Prefs/REACTION 44.10 -> 45.1` — 44.x to 45.x, read out of the files' own
`$VER:` strings by ART's own classifier. Zero downgrades.

**And the booted system says so.** Same WinUAE profile, same licensed
Kickstart 3.1 (V40), same `generate_uae_config`:

```
1> version full
Kickstart 40.68, Workbench 45.1 (13-Nov-00)
```

against `Kickstart 40.68, Workbench 44.5 (18-Aug-00)` before. `C:LoadMonDrvs:
Unknown command` is gone from the boot console, Workbench reaches a clean
screen with no requester, and its icons are the 3.9 ones where the 3.5-only
tree showed generic floppies.

**One thing left open by the fix, deliberately.** The rule places `Locale`
whole, so `Locale/Flags` (61 rows, Countries *and* Keymaps) and
`Locale/Providers` (4) now reach the volume, which `locale-base`'s own
`_why_locale_base_ART_162` note left off as cosmetic. That judgement was made
about a different source tree; this is the 3.9 layer as the disc ships it. If
the owner wants them off it is a rule to narrow, with a run as the reason —
see the component's own `_why_overrides_and_why_last` note.

**ART-165** 🟡 ✅ **Every `on*` subscription wrapper's fire-and-forget
`.then((fn) => { unlisten = fn })` pattern (or an async-IIFE variant of the
same shape) could leak a live Tauri listener, surface an unhandled promise
rejection, or both — the product-code defect ART-163's own test symptom was
standing in front of** — *found reviewing ART-163's root cause, 2026-08-19;
fixed the same round, in two passes (Task 7's own fix round, packages
screen, then its re-review — the first pass converted two screens and
closed this entry on that partial scope, which the re-review correctly
called over-closed; the second pass finished the sweep)*
`src/lib/jobs.ts` · `src/components/osbuilder/OsInstall.tsx` ·
`src/components/osbuilder/PackagePanel.tsx` ·
`src/components/osbuilder/CardBuilder.tsx` ·
`src/components/osbuilder/VolumePreload.tsx` · `src/components/JobBar.tsx` ·
`src/components/layout/Layout.tsx` · `src/pages/ContentLayout.tsx` ·
`src/pages/CollectionStudio.tsx`

Neither half was hypothetical, and not every site had both. **The leak:** an
effect's cleanup function is only ever the `unlisten` closure captured *at
the moment the subscribe promise resolves* — a component unmounted before
that (a fast navigation, or a test's own `cleanup()`) runs the returned
cleanup with `unlisten` still `undefined`, so the real listener already
registered with Tauri is never removed. Some sites (`Layout.tsx`,
`CollectionStudio.tsx`) already guarded this by hand, correctly. **The
rejection:** `listen()`'s own promise rejects whenever there is no IPC
bridge to reach it through, and nothing downstream of the bare `.then()` (or
the un-`try`/`catch`ed `await` inside a `void`-discarded async IIFE) ever
added a `.catch()` — this is production code, not only a test artefact;
ART-163 only ever showed the test-environment symptom of one instance of it.
`JobBar.tsx` had only this half (its own leak guard was already correct);
`CardBuilder.tsx`, `VolumePreload.tsx`, `ContentLayout.tsx` had both, in the
same shape `OsInstall.tsx` originally did.

**The fix.** `subscribeSafely()` (`src/lib/jobs.ts`) wraps any
`() => Promise<UnlistenFn>` subscribe call with a `cancelled` flag — a
listener that resolves after teardown is unregistered the instant it
arrives, never stored — and a `.catch()` that absorbs a failed subscribe
instead of leaving it unhandled. Every subscription site the codebase had at
the time of the second pass now goes through it — `OsInstall.tsx`,
`PackagePanel.tsx`, `CardBuilder.tsx`, `VolumePreload.tsx`, `JobBar.tsx`,
`Layout.tsx`'s drag-and-drop listener, `ContentLayout.tsx`, and
`CollectionStudio.tsx`'s three (catalogue refresh, artwork enrichment,
local-artwork). A grep for the two shapes this entry names
(`let unlisten` / `.then((fn) => {`) across `src/**/*.{ts,tsx}` finds no
further site outside `subscribeSafely`'s own implementation and one
documentation comment.

Covered indirectly by every component test that mounts an affected screen
without tripping an unhandled-rejection failure — the same evidence ART-163's own
fix below cites. There is no dedicated unit test for the leak half
specifically: proving a real Tauri listener handle was released needs a seam
into Tauri's own internal listener registry that this test suite does not
have yet.

**ART-163** 🟠 ✅ **`pnpm test` exited non-zero while every test passed,
because ten jsdom-rendered tests never mocked the Tauri `listen()` shim their
components subscribed through, and each one left an unhandled promise
rejection behind** — *found 2026-08-19 during the content-layer round,
present on `main`; fixed the same round*
`src/components/osbuilder/OsInstall.test.tsx` · `src/lib/jobs.ts`

**The cause**, not just the symptom. `OsInstall.tsx` subscribes to
`onJobProgress` on mount, which calls `@tauri-apps/api/event`'s real
`listen()`. A jsdom test has no Tauri IPC bridge for that call to reach, so
the promise rejects — and nothing in `OsInstall.test.tsx`'s own mocks ever
intercepted `@/lib/jobs`, so the rejection was real and uncaught, once per
render, across every test in the file (ten in total). Vitest counts an
unhandled rejection as a failure regardless of what the tests themselves
asserted, which is why the run's own summary line (`Tests 619 passed`)
disagreed with its exit code. This issue was **first reported correctly by
an implementer, and told the report was wrong** on a first pass that read
the summary line and not the exit code — a true finding withdrawn on that
say-so, then restored.

**The fix.** `OsInstall.test.tsx` now mocks `@/lib/jobs`'s `onJobProgress`
the same way it already mocks every other Tauri-backed wrapper the screen
calls (`osinstallPlan`, `osinstallApply`, …), so the real `listen()` — and
its rejecting promise — is never reached at all during the test run. This
closes the *symptom*; ART-165 above is the underlying defect in the
subscription pattern itself, filed and fixed separately so closing this one
could not also, quietly, close that.

Verified by the full `pnpm test` run itself: zero unhandled-rejection errors
and exit code `0`, confirmed on repeated runs — there is no narrower unit
test for "an exit code agrees with a summary line."

**ART-162** 🟠 ✅ **The AmigaOS 3.9 recipe placed nothing from the disc's own
`Locale` drawer, which made the same task's `locale-turkish` package inert —
no `.language`/`.country` file existed anywhere on the built tree for
`locale.library` to select *any* non-English locale by, Turkish included** —
*found 2026-08-19 by Task 4's code review; fixed the same day*
`src-tauri/src/core/osinstall/recipes/amigaos-3.9.json` ·
`src-tauri/src/core/osinstall/recipes/packages/locale-turkish.json`

The original round of Task 4 (the three curated package recipes: two
BoingBags and a Turkish catalog pack) treated the base 3.9 recipe as
out of scope — it was the previous round's file. But shipping
`locale-turkish` on top of a tree with no Locale selection mechanism at all
is exactly a §89 violation by omission: 36 catalog files that install cleanly
and can never be opened, because nothing on the volume can ever select the
language they translate into.

Measured against the owner's own `AmigaOS39.iso` (`7z l -slt`):
`OS-Version3.9/Locale` is a real top-level drawer, sibling to `Workbench3.5`,
with six of its own — `Catalogs`, `Countries`, `Flags`, `Help`, `Languages`,
`Providers` — none of which the shipped recipe placed. `Languages` carries
`türkçe.language`, `Countries` carries `türkiye.country`; without either on
the volume, Locale prefs has no Turkish entry to offer at all, independent of
whatever catalogs `locale-turkish` supplies.

**The fix.** `amigaos-3.9.json` gained a `locale-base` component, `required:
false` (matching the shipped AmigaOS 3.2 recipe's own `locale-base` — a tree
with no Locale drawer still boots and runs, in English, so nothing forces
this one on the way `install-libs`'s missing `icon.library` does). It takes
four of the six drawers — `Catalogs`, `Countries`, `Languages`, `Help`, the
set `locale.library`'s own selection actually reads — and leaves `Flags`
(country-picker globe icons) and `Providers` (dial-up ISP presets for
software this project does not install) out on purpose, as an unmeasured
guess ART-127 already argues against. `locale-turkish` then genuinely
collides with `locale-base` at the file level — both place
`Locale/Catalogs/türkçe/*.catalog`, and the base disc's own copy already
carries roughly the same ~34 names this update replaces (measured, same
`7z` pass) — so `locale-turkish` now declares `overrides: ["locale-base"]`,
resolved by the same `all_shipped_component_ids` fix ART-162's own review
round made to `recipe.rs` (see the review's other finding on `overrides`,
folded into this same fix round rather than filed separately).

Covered by `core::osinstall::recipe::tests::the_39_recipe_places_a_locale_component_art_162`
(the four rules, verbatim, and `required == false`) and the existing
`every_destination_is_a_name_amigados_can_store` /
`no_two_components_claim_one_destination_without_declaring_it` /
`every_override_names_a_component_that_exists` / `no_rule_escapes_the_tree`
invariant tests, which reach the new component automatically — it is just
another entry in the same shipped 3.9 recipe, not a new list to remember.

**ART-155** 🟠 ✅ **A real AmigaOS 3.9 disc names files `apply()` could not
write as literal Windows path segments — three accented letters ART's own
ISO9660 reader turned into `?`, a character Windows refuses in a path** —
*found 2026-08-19 by Task 4's real run; corrected and fixed 2026-08-19 by
Task 5, one day later*

**This entry's own diagnosis was half wrong, and that half is corrected here
rather than deleted.** It originally named two causes: a reserved DOS device
name (`Storage/DOSDrivers/AUX`) and the three accented `.country` files.
Task 5 measured the first claim directly, on this machine (Windows 11 Pro
26200): a file named literally `AUX` inside a directory writes and lists back
fine, by a plain path and by a `\\?\` path alike. `Storage/DOSDrivers/AUX`
was never what failed — this task's own real re-run confirms it: the file
is present on disk (`Storage/DOSDrivers/AUX`, `AUX.INFO`, `AUX-HANDLER` under
`L/`) in the same tree the `?` fix completed. **This does not mean reserved
device names are a non-issue everywhere** — only that they did not bite on
this one real machine and API; an older Windows or a different write path
could still refuse one, and that would be a new, separately-measured finding,
not a reason to doubt this correction.

The real cause was the second half: `Storage/Locale/Countries-Euro/*.country`
— `core/iso/descriptor.rs::decode_iso646` mapped every byte with the high bit
set to `?`, and `?` is one of the characters (`< > : " | ? *`) Windows
refuses in a path, regardless of what the byte actually meant.
`src-tauri/src/core/iso/descriptor.rs` ·

**The fix.** The three real bytes behind the `?`s — measured verbatim from
the disc by a throwaway diagnostic,
`core::osinstall::apply::tests::read_the_raw_country_name_bytes_when_asked`
— are `0xD6`, `0xCB`, `0xD1`: exactly the ISO-8859-1 (Latin-1) code points for
`Ö`, `Ë`, `Ñ`, the accented letters `Österreich`, a Belgian name and `España`
need. That is not a coincidence: AmigaDOS's own native character set is
ISO-8859-1 (<https://en.wikipedia.org/wiki/AmigaDOS>), the same
byte-transparent choice `write_bcpl_string` already makes in the ADF core.
A second implementation sharing no code with ART's own — 7-Zip, this
project's own ISO9660 oracle (`scripts/iso-oracle-check.py`) — reads this
exact disc's Rock Ridge alternate names for the same three files as
lower-case `ö`/`ë`/`ñ`, the lower-case Latin-1 pairing of the identical
bytes; stated honestly, that shows 7-Zip *agrees* with Latin-1 rather than
independently deriving it (its own Rock Ridge reader may itself default to a
Latin-1-like page), and these three bytes happen to coincide across Latin-1
and Windows-1252/1254 too — so it corroborates without ruling those out, and
the conclusion rests on AmigaDOS's documented charset, not on this
cross-check. (`amitools`, ART's chosen *ADF/HDF* oracle, has no ISO9660
support at all to consult — checked directly.) ECMA-119 itself restricts a
Primary-tree identifier to upper-case
letters, digits, underscore and a dot
(<https://en.wikipedia.org/wiki/ISO_9660>), so a high-bit byte there is
genuinely out of spec — this disc's mastering tool wrote one anyway, real
material a spec citation does not make go away. `decode_iso646` now decodes
every byte as ISO-8859-1 (`b as char`, since Unicode's first 256 code points
are Latin-1 by construction) instead of masking anything above ASCII to `?`
— see the function's own doc comment for the full case, including what this
changes for a non-Amiga disc that happens to carry the same kind of
out-of-spec byte.

Covered by `core::iso::descriptor::tests::a_real_disc_countries_euro_name_decodes_as_latin1`
(the verbatim bytes above) and `a_byte_above_ascii_decodes_as_latin1` (every
byte 0x80..=0xFF, the general case). Reproduced end to end by
`core::osinstall::apply::tests::build_the_real_39_tree_when_asked`, which now
completes against the real disc: 588 files, 75 directories, all 663 planned
items, `Storage/DOSDrivers/AUX` included. (That same real run found a
second, distinct defect once it could finally run to completion — ART-156,
filed separately, not fixed here.)

**ART-154** 🟠 ✅ **`apply()` hashed a whole medium into memory just to record
its SHA-256 — 469 MB for the real AmigaOS 3.9 disc, against CLAUDE.md's own
rule that ART never reads a whole user file into memory** — *found and fixed
2026-08-19, alongside ART-153, in the same three-line loop the coordinator's
review of Task 4 pointed at directly*
`src-tauri/src/core/osinstall/apply.rs` (the `for (volume, path) in
&plan.media_paths` loop) ·

`built_from.push(MediaRecord { sha256: sha256_bytes(&raw), .. })` needed
`raw`, and the only way `raw` existed was `let raw = std::fs::read(path)?;` —
harmless for an 880 KB ADF, and exactly the whole-file read
`core/hashing.rs::sha256_file` exists to avoid for anything bigger. Nothing
else in scope used `raw`. → replaced with `sha256_file(path)`, which streams
in 64 KiB chunks (`core/hashing.rs`) and never holds the medium whole.
Covered by the same real run as ART-153 —
`core::osinstall::apply::tests::build_the_real_39_tree_when_asked` — which
now hashes the real 469 MiB disc without the process's memory tracking it.

**ART-153** 🟠 ✅ **`apply()` cannot build a distribution tree from disc
media — it opens every medium `plan.media_paths` names through
`AdfSource::open` unconditionally, never `scan::open_media`** — *found
2026-08-19, the 3.9 recipe's Task 4 real run against the owner's own
`AmigaOS39.iso`; fixed the same day after the coordinator overruled the
task's own "stop at engine code" brief on this one point*
`src-tauri/src/core/osinstall/apply.rs`, `src-tauri/src/core/osinstall/scan.rs` ·

`plan()` was fixed for CD media in an earlier task
(`a_component_whose_media_is_a_disc_is_planned_from_the_disc`,
`core/osinstall/plan.rs`) and correctly resolves a `Subtree` rule against a
disc through `scan::open_media`, which dispatches on `FoundMedia::kind`
(`MediaKind::Floppy` → `AdfSource`, `MediaKind::Disc` → `CdSource`). `apply()`
never received the same fix: it built its `sources` map with
`Box::new(AdfSource::open(path)?)` for every volume in `plan.media_paths`,
whatever kind of medium it actually is. A plan built entirely from CD content
therefore planned cleanly (0 refusals) and then failed at the first byte
written, with `CoreError::UnsupportedFormat("… does not start with a
recognisable AmigaDOS signature")` — a real ISO9660 image is never a bare
AmigaDOS volume, so `AdfSource::open` always refused it.

The obstacle to a one-line fix was that `plan.media_paths` is a bare
`BTreeMap<String, PathBuf>`, carrying no `MediaKind`, while `scan::open_media`
takes a `&FoundMedia`. → the floppy-then-disc identification `find_media`'s
own loop already did was pulled out into a named function,
`scan::identify(path: &Path) -> Option<FoundMedia>` — the one place that
decision is made, called by both `find_media` (its loop body, unchanged in
behaviour) and `apply()` (per medium in `plan.media_paths`, re-identifying it
rather than trusting a stale assumption). A path that no longer identifies as
anything at apply time — moved, replaced, removed since the plan was made —
is now a typed `CoreError::InvalidInput` naming the path, not a silent skip.

Reproduced and now covered by
`core::osinstall::apply::tests::build_the_real_39_tree_when_asked`: `apply()`
opens the real 469 MiB `AmigaOS39.iso` through `CdSource` and proceeds into
the real write loop, 1,020 files and 71 directories past where it used to
fail immediately, before running into the unrelated ART-155.

**ART-151** 🟠 ✅ **A WHDLoad launch got the stock A500 profile completely
unmodified — 512 KB Chip, 512 KB Slow, no Fast RAM at all — and WHDLoad itself
refused to load the game for want of memory** — *found and fixed 2026-08-18,
running the real application against the user's own `1000 Miglia` after
ART-148/ART-149 got it past "not a DOS disk" and into WHDLoad itself, which
then printed:*
```
WHDLoad 18.7 ©1994-2021 Wepl
DOS-Error #103
(not enough memory available)
on loading "1000Miglia.Slave"
```
`src-tauri/src/core/launch/mod.rs`, `src-tauri/src/commands/launch.rs`,
`src/lib/launch.ts`, `src/components/collection/TitleDetail.tsx`,
`src/pages/Settings.tsx` ·

`1000 Miglia`'s catalogue record states no chipset, so `machine_for` fell back
to the user's own default machine — `a500` — and `commands/launch.rs::
profile_for` handed that plan `AmigaProfile::a500_ocs` exactly as
`core/profile.rs` defines it: a stock 1987 machine, `chip_kb: 512, slow_kb:
512, fast_mb: 0, z3_fast_mb: 0`. That is 1 MB total, none of it Fast — and
WHDLoad's own requirements page (<https://www.whdload.de/docs/en/need.html>)
states its minimum as "a minimum of 1.0 MiB RAM (sometimes more, it depends on
the installed program)". A stock A500 meets that floor with nothing left over
for the game itself, which is exactly the DOS-Error #103 above: WHDLoad
started, tried to load `1000Miglia.Slave`, and there was nowhere left to put
it.

**The fix.** `core::launch::DEFAULT_WHDLOAD_FAST_RAM_MB` (8) is folded into a
WHDLoad launch's profile by a new `commands/launch.rs::profile_for_request`,
which wraps the existing `profile_for` and raises `.memory.fast_mb` (never
lowers it — `.max(...)`) whenever the request is WHDLoad-shaped, read from the
same `core::launch::is_whdload_shaped` predicate `WHDLOAD_MIN_KICKSTART_MAJOR`
already uses for the Kickstart floor (the old private `needs_whdload_floor`
is now this public, shared predicate). A floppy or a plain (non-WHDLoad)
hardfile is never touched. 8 MB is not invented for this fix: it reuses the
number `AmigaProfile::a1200_aga` already settled on, describing itself as
"the ideal WHDLoad setup" — and it is Fast RAM specifically because
WHDLoad's own autodoc (<https://www.whdload.de/docs/autodoc.html>, Overview)
states the installed program's `BaseMem` "is always Chip-memory" while the one
optional extra a slave may request, `ExpMem`, "may be Chip- or Fast-memory
dependently on what is available" — so Fast RAM headroom gives WHDLoad and
AmigaDOS somewhere to live without changing what the emulated game's own
Chip RAM budget looks like.

**Never touches the shared presets.** `profile_for` already returns a fresh
`AmigaProfile` per call — `a500_ocs()`/`a1200_aga()` build a new struct every
time rather than handing back a shared instance — so `profile_for_request`
raising `.memory.fast_mb` on its result only ever changes the one launch in
progress. `core/profile.rs` itself, and every other screen that reads those
presets (the Profile Studio among them), are unmodified; a test
(`the_presets_cover_the_classic_line_with_unique_ids`'s neighbours in
`core/profile.rs`) was already pinning `AmigaProfile::a500_ocs`'s numbers and
still passes unchanged.

**Exposed as a setting, not a buried constant.** `launch.whdloadFastRamMb`
sits alongside `launch.romDir`/`launch.defaultMachine`/`launch.systemVolume`
in `Settings.tsx`'s `PlaySettingsSection`, remembered through `useRemembered`
and guarded by `isWhdloadFastRamMb` (`isWholeNumberBetween(0, 8)`, matching
WinUAE's own 24-bit Fast RAM ceiling — the same ceiling
`AmigaProfile::a1200_aga`'s stock preset already uses) so a hand-edited or
stale `settings.json` falls back to the default rather than reaching a
launch. `commands/launch.rs::LaunchArgs::whdload_fast_ram_mb` carries it to
the backend, `#[serde(default = "default_whdload_fast_ram_mb")]` for a screen
still running an older bundled frontend against a rebuilt backend.

**Said on the confirmation screen, not only on failure.** `LaunchPreview`
gained a `memory: Option<MemorySummary>` — `Some` exactly when a plan
settled, `None` on a refusal — and `TitleDetail.tsx`'s "will use" sentence
now reads `{{machine}} · {{rom}} · {{memory}}` (`en.json`/`tr.json`, both
updated in this commit) with `memoryLabel()` formatting the four numbers,
because DOS-Error #103 is exactly this fact and the user must be able to see
what will be tried before pressing Start, not learn it a second time from
WHDLoad's own error screen.

**`ws_ExpMem` is deliberately not read.** WHDLoad's autodoc describes it as an
*optional* extra memory area a slave may request — see the ExpMem quote
above — so it names a **request**, not the title's total requirement, and
reading it would need another catalogue schema bump and another rescan of
every user's collection. Filed separately as ART-152 rather than built
here — and closed there on 2026-08-21 by a named WHDLoad machine profile
rather than by the per-title reading it suggested.

Regression tests, all failing against the pre-fix code:
`commands::launch::tests::a_whdload_hardfile_plans_the_configured_fast_ram`,
`commands::launch::tests::a_whdload_drawer_plans_the_configured_fast_ram`,
`commands::launch::tests::a_floppy_title_does_not_get_whdload_memory_silently_applied`,
`commands::launch::tests::a_plain_hardfile_does_not_get_whdload_memory_applied`,
`commands::launch::tests::a_configured_value_lower_than_the_stock_a1200_preset_never_shrinks_it`,
`commands::launch::tests::preview_for_states_the_memory_a_whdload_launch_will_use`,
`commands::launch::tests::preview_for_states_no_memory_on_a_refusal`, and, on
the frontend, `src/lib/launch.test.ts`'s `isWhdloadFastRamMb` guard suite and
`memoryLabel` suite.

**Retried against the real emulator, and it is what got `1000 Miglia` past
DOS-Error #103.** Read off disk after the retry, the only line that changed
between the failing and the succeeding configuration was `fastmem_size=0` →
`fastmem_size=8` — exactly the value this fix adds. WHDLoad loaded
`1000Miglia.Slave` and Simulmondo's own title logo appeared in the WinUAE
window. DOS-Error #103 is the only symptom this fix addresses; a game that
needs more than 8 MB of Fast RAM, or fails for an unrelated reason once
memory is no longer the blocker, is still not covered by it.

**ART-149** 🔴 ✅ **A fix built on a correct mechanism and a wrong inference
changed the bare-hardfile geometry from `sectors=32` to `sectors=1`, and it
broke the very titles it meant to fix — reverted, and the correct geometry
restored with a measurement to settle it for good** — *the wrong fix found
and fixed 2026-08-18, running the real application against the user's own
WHDLoad hardfiles after the `sectors=1` change had already landed on `main`
and produced "not a DOS disk" on titles that had worked before it*
`src-tauri/src/core/winuae.rs::generate_uae_config` ·
The mechanism behind the original change is real and stays documented:
WinUAE derives the cylinder count as `(filesize / blocksize) / (sectors ×
surfaces)` (`hardfile.cpp::getchs2`) with integer division, so under the old
`32,1,2,512` geometry (one cylinder = 32×512 = 16,384 bytes) any file whose
size is not a whole multiple of 16,384 bytes has its last partial cylinder
rounded away when *presented* to AmigaDOS. `1000 Miglia v1.2.hdf` is
1,195,008 bytes = 2334 blocks = 72.9375 of those cylinders. The inference
drawn from that mechanism was wrong: that the missing blocks were *lost data*
that a different geometry could recover. They are not. The bare WHDLoad
hardfiles in this collection were themselves **built** at 32-sectors/1-surface
geometry, and the filesystem written inside each one is sized to the
truncated whole-cylinder block count — the truncation is the geometry the
volume expects, not a bug in how ART presents it.

**The measurement that settles it.** An FFS volume's root block sits at half
the volume's block count (`core::volume::VolumeGeometry::root_block_for`), so
the root block's actual position, read out of the image itself, tells you
what block count the filesystem inside was built for. Measured across six
images from this user's collection
(`E:\amiga\Amigatolon\WHDload\HDF_Games_WHDLoad_by_Enzo_[A]\`), by scanning
for the block whose first longword is `2` (`T_SHORT`) and whose last is `1`
(`ST_ROOT`):

| file blocks | root block | 2 × root | blocks truncated to a multiple of 32 |
|---|---|---|---|
| 1843 | 912 | 1824 | 1824 |
| 1331 | 656 | 1312 | 1312 |
| 3993 | 1984 | 3968 | 3968 |
| 5734 | 2864 | 5728 | 5728 |
| 5038 | 2512 | 5024 | 5024 |

Six for six: `2 × root_block` always equals the file's raw block count
truncated down to a multiple of 32, never the raw block count itself.
Presenting the full, untruncated block count instead (which is what
`sectors=1` does) makes AmigaDOS compute the root block at the *raw*
half-count — for `1000 Miglia` (2334 blocks) that is 1167, where the real
root block, per the table's method, is at 1152 (half of 2304, the truncated
count) — so the volume fails to validate and AmigaDOS reports "not a DOS
disk in device DH0", which is exactly what the user saw after the
`sectors=1` change shipped. `reserved=2` was never the problem and stays: in
the Amiga RDB scheme those two blocks are the partition's own boot blocks,
which is precisely what the `DOS\1` signature at block 0 of these images is.

**What actually happened, plainly.** `81e3162` ("Fix bare-hardfile geometry
rounding down the last cylinder", filed as ART-148) changed `sectors=32` to
`sectors=1` on the strength of the `getchs2` reading above, without checking
what block count the filesystem *inside* these images was actually built
for. It was reverted with `git reset --hard` on a mistaken stand-down —
weaker forum evidence about round-sized images was weighed against it — then
restored as ART-149 in `f36cfe5` on the reasoning that the forum evidence
was weaker than the source reading. The source reading about `getchs2` was
correct; the conclusion that changing the geometry would fix anything was
not, and restoring it broke every one of this collection's titles that
ART-149's own restore commit claimed to fix, reproducing "not a DOS disk" on
`1000 Miglia` itself. This entry is the correction: `sectors=32` is restored,
and the doc comment and test above now carry the six-image measurement so
the wrong inference is not rediscovered independently of the mechanism that
looks like it supports it.

**ART-148 is unaffected and remains correct.** It is a separate, genuine
cause of the same "not a DOS disk" symptom on the same file — no Kickstart
floor was enforced on a WHDLoad launch, so a name-sorted ROM scan could hand
a title a Kickstart 1.x machine that has no hard-disk filesystem in ROM at
all, regardless of hardfile geometry. Nothing about this entry touches that
fix.

Tests (`src-tauri/src/core/winuae.rs`): every bare-geometry assertion
restored to `32,1,2,512` (`generate_a1200_config_with_aros_fallback`,
`each_hardfile_gets_its_own_device`, `a_missing_shape_entry_defaults_to_bare_geometry`).
The regression test that had asserted `sectors=1` was replaced —
`bare_geometry_truncates_to_the_built_cylinder_count` — built on the same
real 1,195,008-byte `1000 Miglia` measurement, but now pinning the *correct*
line, `hardfile2=rw,DH0:...,32,1,2,512,0,,uae`, with the six-image table
above carried in its doc comment as the evidence. **Retried against the real
emulator, and confirmed by the working run's own configuration.** The
successful launch's `hardfile2=` line, read off disk, is exactly
`rw,DH0:E:\amiga\Amigatolon\WHDload\HDF_Games_WHDLoad_by_Enzo_[#]\1000
Miglia v1.2.hdf,32,1,2,512,0,,uae` — the geometry this entry restores.
`1000 Miglia` reached Simulmondo's title logo on it. The other 1697 titles
built to the same shape were not each individually retried; one title
booting on this geometry is a strong signal for the rest, not proof of
every one.

**ART-148** 🔴 ✅ **A WHDLoad title's machine was chosen with no floor on the
Kickstart it boots, so a name-sorted ROM-folder scan could hand it one older
than WHDLoad itself requires** — *found and fixed 2026-08-18, running the real
application against the user's own `1000 Miglia` (a self-booting WHDLoad
hardfile), which the emulator reported as "not a DOS disk", and confirmed
against whdload.de's own requirements page*
`src-tauri/src/core/launch/mod.rs`, `src-tauri/src/core/rom/mod.rs`,
`src-tauri/src/commands/launch.rs` ·

`1000 Miglia`'s catalogue record states no chipset and no Kickstart (both
`null`). `machine_for(None, default)` therefore fell back to the user's own
default machine — never set, so `a500` — and `plan_for` took the *first*
A500-suitable ROM in `scan_rom_directory`'s name-sorted order, which in this
user's folder is Kickstart 1.3. Two independent facts make that fatal on its
own, not merely unlucky: WHDLoad's own requirements page
(<https://www.whdload.de/docs/en/need.html>) states its minimum as "Kickstart
2.0 (version 37)", so a 1.3 machine cannot run WHDLoad at all; and Kickstart
1.x carries no hard-disk filesystem in ROM — Workbench 1.3 shipped
FastFileSystem as a driver for the RDB, and a self-booting WHDLoad hardfile is
a bare `DOS\1` FFS volume with **no** RDB, so a 1.x ROM has nothing to mount it
with regardless of geometry. That second fact is "not a DOS disk", by itself.

**A geometry theory for this same symptom, on this same file, was raised,
shipped, and then corrected — see ART-149; this entry is unaffected.** An
earlier pass through this same investigation traced "not a DOS disk" to
`hardfile2=`'s forced bare-image geometry (`81e3162`, filed at the time as
ART-148, ahead of this entry taking that number), changed `sectors=32` to
`sectors=1`, reverted that with `git reset --hard` on a mistaken stand-down,
then restored it as ART-149 — and the restored change turned out to be
wrong: the WinUAE integer-division mechanism it relied on was real, but the
truncated block count is what the filesystem *inside* these hardfiles was
actually built for, not lost data, so presenting the untruncated count broke
titles that had booted before. ART-149 now records the correction, with a
six-image measurement pinning `sectors=32` as correct. This entry — the
Kickstart floor — was never in question and needed no change: a 1.x ROM has
no hard-disk filesystem to mount a bare `DOS\1` volume with regardless of
geometry, so it is "not a DOS disk" on its own, independent of whatever
`hardfile2=` geometry is in play.

**The fix.** `core::launch::WHDLOAD_MIN_KICKSTART_MAJOR` (37) is enforced as a
floor on the *booted machine's* chosen ROM whenever a request is
WHDLoad-shaped — `RequestKind::Whdload`, or `RequestKind::Hardfile { whdload:
true }`, the shape `Media::WhdloadHardfile` (self-booting) takes. A plain
hardfile (`whdload: false`) and a floppy set are never held to it: a
hand-installed AmigaOS hardfile may legitimately need an old Kickstart, and a
bare `.adf` must keep booting on any Kickstart. `core::rom::RomInfo` gained a
`major: Option<u16>` field (mirrored onto `core::launch::LaunchRom`) so the
floor is checked against a number, not re-derived from display text. Choosing
among several suitable ROMs is now deterministic and no longer "first in
scan order": `core::launch::best_rom` takes the highest known major, ties
broken by name. A folder with nothing meeting the floor now refuses with a
new `LaunchRefusal::NoRomMeetsWhdloadMinimum { machine }`, naming the actual
requirement, rather than leaving the user at an AmigaDOS prompt to work out
why.

**The conceptual error this closes off.** A WHDLoad slave's own declared
Kickstart (`SlaveFacts.kickstart` / a catalogue record's `KickstartNeed`) is
the ROM image *WHDLoad itself* loads from `DEVS:Kickstarts` for the game, on a
machine already running something modern — it is not what the machine should
boot, and the floor above must never read it. `core/launch/mod.rs`'s own doc
comment on `WHDLOAD_MIN_KICKSTART_MAJOR` and `commands/launch.rs::
request_kind_from`'s doc comment both say so at the point the choice is made,
since a field named `kickstart` sitting right next to a real launch decision
is exactly the kind of thing that gets re-broken by someone reading the name
instead of the comment.

Regression tests, all failing against the pre-fix code:
`core::launch::tests::a_whdload_hardfile_with_a_1_3_and_a_3_1_available_plans_the_3_1`,
`core::launch::tests::a_whdload_hardfile_with_only_kickstart_1_x_refuses_with_the_floor`,
`core::launch::tests::a_whdload_drawer_with_only_kickstart_1_x_refuses_with_the_floor`,
`core::launch::tests::a_plain_hardfile_is_not_held_to_the_whdload_floor`,
`core::launch::tests::a_floppy_set_is_not_held_to_the_whdload_floor`, and
`core::rom::tests::major_from_revision_reads_the_leading_number_and_nothing_else`.

**Retried against the real emulator, and proven directly.** The successful
run's configuration names `kickstart_rom_file=E:\amiga\Amigatolon\kickstart\
Kickstart 3.1.rom`, read off disk — the highest-major ROM in the user's
folder, exactly what `best_rom`'s "highest major wins" rule picks where the
old first-alphabetical scan had handed this same title Kickstart 1.3.
`1000 Miglia` reached Simulmondo's own title logo on that machine.

**ART-147** 🔴 ✅ **A self-booting WHDLoad hardfile was catalogued as an
unpacked drawer, sending Play looking for a system volume the file never
needed — and shipping the fix broke every catalogue that already existed**
— *found and fixed 2026-08-18, running the real application against
`E:\amiga\Amigatolon\WHDload\...\1000 Miglia v1.2.hdf` (the same title
ART-146 left unretried), then again immediately after landing the fix, when
the Collection screen itself came up showing
`ART-FORMAT-MALFORMED: unknown variant 'whdload-drawer'` instead of any
title at all*
`src-tauri/src/core/gameindex/scan.rs::from_hardfile`,
`src-tauri/src/core/gameindex/record.rs::Media`,
`src-tauri/src/core/gameindex/store.rs::{read_root,read_overrides}`,
`src-tauri/src/commands/launch.rs` ·
1698 of this user's 2787 catalogued titles are the shape
`core::gameindex::readers::whdhdf`'s own header already documented: a bare
`DOS\1` FFS volume, no RDB, holding a WHDLoad drawer and an
`S/startup-sequence` that runs it — verified independently against
`1000 Miglia v1.2.hdf` (1.1 MB, begins `DOS\1`, carries the strings `WHDLoad`
×40 and `1000Miglia.Slave` ×3). `from_hardfile` recorded every one of them as
`Media::WhdloadDrawer { slave }` anyway — the shape for an *unpacked* drawer
that needs a separate bootable system — which sent Play down the WHDLoad
(Y1/Y2) path: ask for a system volume, mount the game as a plain directory,
write ART's own boot directory. The user hit exactly this: chose a plain
Workbench 3.0 image (no WHDLoad on it at all) as the "system", landed at an
AmigaDOS CLI, and reasonably concluded they had to install WHDLoad. They did
not — the file boots itself.

`Media::WhdloadDrawer` was checked and found to have exactly one producer,
`scan.rs::from_hardfile` — nothing else in the codebase ever constructed it
(`core::layout::ItemKind::WhdloadDrawer` is a same-named but unrelated type,
for laying files onto a card during an OS install, not for the catalogue).
Removed rather than left beside its replacement: a variant nothing produces
is a trap for the next reader, and repurposing it as
`Media::WhdloadHardfile { file, slave }` says what these 1698 files actually
are — `file` is the image ART now mounts and boots directly, `slave` is kept
rather than discarded, since it is what named the title and carries its
chipset and declared Kickstart. `GAMEINDEX_SCHEMA` moved 2 → 3 so an Update
re-reads every one of them, the same way ART-137's bump did.
`commands/launch.rs::request_kind_from` now maps `WhdloadHardfile` onto the
same `RequestKind::Hardfile` a plain `Hardfile` takes — mount the image, boot
it, no system volume, no boot directory, no Y1/Y2. `core::launch`'s
`RequestKind::Whdload`/`LaunchKind::Whdload` (drawer + separate system) stays
in place: it is the real shape for an *already-unpacked* WHDLoad pack that
needs a separate bootable system, the same shape `core::whdload` installs
onto a card during an OS install — no `Media` variant reaches it today because
nothing in `core::gameindex` catalogues a loose drawer or an `.lha` archive as
a title, not because the shape is imaginary.

The other half of this fix is a conflict the previous wave's own review
created: ART-141's whole-branch review made a plain hardfile mount
**read-only** on spec §93 ("originals are immutable by default"), correctly —
but a self-booting WHDLoad hardfile is the one shape where the game itself
writes its saves back into that exact image, so read-only-by-default silently
throws them away with no error and no message. §93 stands: the default is
still read-only. `LaunchArgs` gained `allow_write` (`#[serde(default)]`, so a
frontend build ahead of a not-yet-rebuilt backend still gets the safe
default rather than a failed deserialize), a per-title, off-by-default
opt-in `TitleDetail.tsx` remembers through `useRemembered` keyed by
`launch.allowWrite.<record.id>`. The confirmation screen states which side of
that choice is in effect before Start is reached: a new `MountNote::
WhdloadHardfile { read_only }`, distinct from the plain `Hardfile` note, so
the sentence can say plainly "any save this game makes will not be kept"
rather than reusing wording written for a different shape.

**A third bug, found live, the moment the first two landed.** Every one of
this user's WHDLoad titles had already been catalogued once, under the old
`Media::WhdloadDrawer { slave }` shape — so the instant `Media` changed
shape, `store.rs::read_root` hit `serde_json::from_slice::<CatalogueRoot>`
failing on `unknown variant 'whdload-drawer'` and turned that into a hard
`CoreError::Malformed`, which `store::load` propagated straight up through
`commands::gameindex::catalogue_load` to the screen. `GAMEINDEX_SCHEMA`'s
2 → 3 bump — the mechanism ART-137 built for exactly "an older reader wrote
this, re-read it" — never got a chance to run: `index_schema` lives *inside*
the same document that failed to parse, so a shape change that breaks
deserialization defeats its own re-read signal. The Collection screen came
up refusing outright, for every catalogue this user had.

**The fix turns on one distinction: a root catalogue file is derived data;
the overrides file is not.** A root file's every entry comes from a file
under the user's own folder — one Update reproduces it exactly — so
`read_root` now distinguishes three outcomes (`StoredRoot::Absent` /
`Unreadable` / `Found`) instead of collapsing "never scanned" and "cannot be
parsed" into the same `None`, or "cannot be parsed" into an error.
`Unreadable` (any raw `serde_json` parse failure — an old shape, truncation,
anything) folds into `RootView::stale`, the exact signal an `index_schema`
mismatch already used, rather than inventing a second one; `load()` returns
that root with `stale: true` and no entries instead of failing, and the
Collection screen's existing stale badge ("These titles were read by an
older version of ART. An update would improve them.") already says why and
already points at Update — no frontend change was needed. `refresh_root`
(what Update actually runs) reads the same file through the same
`read_root`, so it had to change too: `Unreadable` is now treated exactly
like `Absent` — nothing to reuse, every file re-read fresh — so pressing
Update is what actually rebuilds the file, rather than failing on the exact
document it exists to replace. A **newer**-schema root (one this build's
structs *can* parse, but whose `CatalogueRoot.schema` number is higher than
`CATALOGUE_SCHEMA`) is kept a hard error, unchanged: rescanning with an
*older* build cannot fix a file a newer one wrote, and `GAMEINDEX_SCHEMA`'s
own doc comment is explicit that a newer shape is refused rather than
half-read.

`read_overrides` was deliberately left exactly as strict as before. The
user's own title corrections and hand-attached pictures are not rebuildable
by rescanning — silently discarding a corrupt overrides file would replace
one bug (a screen that refuses to open) with a worse, quieter one (the
user's own edits gone with no message at all). This is the load-bearing
half of the fix: not "make parsing lenient," but "know which of these two
files can be forgiven and which cannot."

**Checked for the same latent bug elsewhere.** Every other derived-data
reader already found in `core/` — `artwork::cache` (its own doc comment
already says "derived data, refusing to open would strand the user"),
`oplog::jsonl`, `sources::catalog::jsonl`, `sources::installed` — already
tolerates a parse failure per-record or per-line rather than failing the
whole file; `gameindex::store::read_root` was the one reader still using an
all-or-nothing parse. Not checked further in this pass: `read_roots`
(`roots.json`, the list of catalogued folder paths) goes through the same
strict `read_json` helper and would fail the same way if its own shape ever
changed — lower risk, since it holds no `GameRecord`/`Media` and is far less
likely to change shape, but the same class of bug and not fixed here.
`core::card::manifest::read_manifest` is strict by the same pattern too, but
is arguably a different case: it describes what ART itself already wrote
onto a specific physical card, not something rebuildable by rescanning a
folder, so refusing rather than guessing may be the right call there — worth
a second look, not reopened as a defect.

Test: `core::gameindex::scan`'s `a_mixed_folder_yields_one_record_per_title`
and `a_stated_chipset_beats_a_guessed_one` (both already exercise
`from_hardfile`, now against the new `Media` shape);
`commands/launch.rs`'s `media_whdload_hardfile_becomes_request_kind_hardfile`,
`mount_notes_state_a_whdload_hardfile_as_read_only_by_default`,
`mount_notes_state_a_whdload_hardfile_as_writable_when_the_user_opts_in`,
`a_whdload_hardfile_mounts_read_only_by_default`,
`a_whdload_hardfile_mounts_writable_when_the_user_allows_it`, and
`allow_write_is_ignored_for_a_plain_hardfile` (the field must not leak onto a
shape the screen never offered it for);
`mount_note_wire_shape_is_what_the_frontend_reads` pinning the new variant's
JSON. Frontend: `src/lib/collectionDetail.test.ts`'s `mediaPhrase`/`diskList`/
`canLaunch` cases against the renamed kind, and `src/i18n/phrase-keys.test.ts`'s
`mediaPhrase`/`mountNotePhrase` variant-resolution tests extended with the new
tag. `core::gameindex::store`'s
`a_corrupt_root_file_is_read_as_unreadable_rather_than_erroring`,
`a_root_file_with_an_unknown_media_variant_is_read_as_unreadable`,
`load_reads_a_root_with_an_unknown_media_variant_as_stale_not_an_error` (the
exact end-to-end shape the live bug took),
`refresh_self_heals_a_root_file_this_build_cannot_parse` (proves Update
itself does not fail on the file it is meant to replace), and
`an_overrides_file_with_an_unknown_field_still_errors` (pins that the user
layer did **not** become lenient along with the root layer) — the last two
are the pair the fix's split is checked against; `a_root_file_from_a_newer_art_is_refused`
still passes unchanged, proving the newer-schema case is still a hard error.

**Retried, and the classification itself is now proven.** `1000 Miglia`
launched as `Media::WhdloadHardfile` — mounted and booted directly, no
system-volume prompt, no ART boot directory — and reached Simulmondo's own
title logo, which is only reachable if Play took the hardfile path this fix
put it on rather than the old drawer-plus-system path. What the
classification fix does **not** cover is still open: whether a save
survives with `allow_write` turned on has not been retried, and this one
title's launch does not stand in for the other 1696 records this fix
reclassified the same way.


**And the recovery was then verified on the user's own files, not simulated.**
After the fix, they pressed Update once in the running application. Read back
off disk afterwards: the WHDLoad root was rewritten with `index_schema: 3` and
**1697 records carrying `whdload-hardfile`** (plus the one floppy title that
root has always held), while `overrides.json` was left untouched at its
earlier timestamp with the user's own `1869 AGAdeneme` title correction intact
— which is the whole point of the derived/user-data split this fix turns on.
`1000 Miglia` itself has since launched from this reclassified record and
reached the game (ART-148/ART-149/ART-151, retried the same day, same
session). Still unproven: whether a save survives with `allow_write` turned
on.

**ART-146** 🔴 ✅ **`hardfile2=` forced bare-image geometry onto every hard
drive image, including a VHD container — WinUAE reported "Not a DOS disk in
unit 0"** — *found and fixed 2026-08-18, retrying Y1 against the user's own
`E:\amiga\amikit\AmiKit.hdf` after ART-145's fix landed*
`src-tauri/src/core/winuae.rs::generate_uae_config`,
`src-tauri/src/core/hdf.rs::detect_hardfile_shape` ·
`core/winuae.rs` always emitted
`hardfile2={access},DH{i}:{path},32,1,2,512,{bootpri},,uae` — forced 32
sectors, 1 surface, 2 reserved, 512-byte blocks — for every hardfile,
regardless of what the file actually was. That is correct for the bare
filesystem images ART itself creates (`create_hdf` with `is_rdb: false`),
whose first four bytes are `DOS\0`. Read directly off the user's disk,
`AmiKit.hdf` is not that: its first eight bytes are the ASCII `conectix`, the
Microsoft/Connectix VHD container signature, and the Amiga `RDSK` block sits
at block 67 behind the VHD header. Mounting it with forced geometry made the
emulated Amiga read VHD header bytes where AmigaDOS expected a filesystem —
exactly "Not a DOS disk in unit 0". The e-uae configuration syntax WinUAE
inherits (`docs/configuration.txt`) already states the fix: blocksize `0`
marks an RDB hard file, and "all other components ... will be ignored apart
from `<path>` and `<access>`" — its own example
(`hardfile2=rw,:/path,0,0,0,0,0,`) leaves the device name empty too, since a
forced `DH{i}:` is meaningless once the disk carries its own device names in
its own RDB.

Fixed by deciding the image's shape from the file itself before saying how to
mount it, rather than assuming one shape for all of them. `core/hdf.rs` gains
`HardfileShape` (`Bare` / `Rdb` / `Unknown`) and `detect_hardfile_shape`,
which reuses `find_rdb_location` (`core/rdb.rs`) for the RDB case rather than
adding a second RDB detector, and otherwise checks for the same bare
signatures `core/detect.rs`'s drop-pipeline classification already knows
(`DOS\0`..`DOS\7`, `PFS\3`, `PDS\3`, `SFS\0`) — anything else, VHD included,
comes back `Unknown`. `core/winuae.rs::generate_uae_config` stays
platform-independent and file-free: `LaunchMedia` grew a `hardfile_shapes:
Vec<HardfileShape>` field (`#[serde(default)]`, so a stored configuration
from before this field existed still deserialises, and a short vector leaves
later hardfiles at `HardfileShape::Bare` — the geometry every hardfile got
before this fix, so the WinUAE screen's own already-working path does not
regress) that the *caller* fills in from the real file, and `Bare` still
emits the forced-geometry line while `Rdb`/`Unknown` both emit
`hardfile2={access},:{path},0,0,0,0,{bootpri},,uae` — empty device, zeroed
geometry. The decision is made in `commands/launch.rs::media_for_plan` (both
the plain-hardfile and WHDLoad-system branches) and
`commands/winuae.rs::winuae_launch` (WinUAE Studio's own manual launch),
since that is where the image is actually available to read.

Test: `core/winuae.rs`'s `an_rdb_hardfile_gets_no_forced_geometry`,
`an_unrecognised_hardfile_shape_gets_no_forced_geometry` and
`a_missing_shape_entry_defaults_to_bare_geometry`, all pinning complete
`hardfile2=` lines with `assert!`/`assert_eq!`; `core/hdf.rs`'s
`detect_shape_of_a_bare_dos_image`, `detect_shape_of_a_bare_pds3_image`,
`detect_shape_of_an_rdb_image` and `detect_shape_of_a_vhd_image_is_unknown`;
`commands/launch.rs`'s `a_plain_hardfile_shape_is_detected_from_its_own_bytes`
and `a_whdload_systems_vhd_shape_is_detected`; `commands/winuae.rs`'s
`detects_the_shape_of_a_manually_selected_hardfile` and
`no_hardfiles_means_no_shapes_to_detect`.

**Still unproven — deliberately not exercised by the `1000 Miglia` run.**
This closes the defect the second real run hit, found by reading the image's
own bytes (`conectix` at offset 0, confirmed against the user's actual
`AmiKit.hdf`) rather than by re-running WinUAE with the fix applied. The
`1000 Miglia` run that later reached the game (ART-148/ART-149/ART-151) is a
bare `DOS\1` hardfile with no RDB and no VHD container — the `Bare` branch
this entry leaves unchanged, not the `Rdb`/`Unknown` branch this entry adds.
No VHD or RDB image was involved, so whether `AmiKit.hdf`, or any other VHD
or RDB image, now mounts correctly has still **not** been retried against
the real emulator.

**ART-145** 🔴 ✅ **The one-click WHDLoad launch never got past the CLI: the
generated startup-sequence could not run its own first line** — *found and
fixed 2026-08-18, by running Y2 against a real title (`1000 Miglia`) in
WinUAE*
`src-tauri/src/core/launch/whdload_boot.rs::startup_sequence` · Wave C's
headline feature is one click from drop to game. The first real run of it
put the user at an AmigaDOS CLI instead. Read off disk, ART's generated
`S/Startup-Sequence` was:

```
Assign C: DH0:C
Assign LIBS: DH0:Libs
Assign DEVS: DH0:Devs
CD DH1:
WHDLoad 1000Miglia.Slave
```

— and ART's boot directory, mounted at the highest boot priority so AmigaDOS
boots from it, contains no `C` drawer at all. Per the AmigaOS documentation,
`SYS:` becomes that boot volume and AmigaDOS auto-assigns `C:` only when a
`C` directory actually exists there. It does not on ART's, so `C:` was never
assigned — and `Assign` is itself a command that lives in `C:`. The script's
own first line could not run; AmigaDOS reported the command not found and
left the user at the prompt, exactly as read off disk.

Fixed by invoking that first `Assign` through an explicit path on the
mounted system volume (`DH0:C/Assign C: DH0:C`) rather than relying on `C:`
to already resolve — once that line runs, `C:` exists and every following
`Assign` resolves normally. Also added `SYS:` and `S:`, alongside the
existing `LIBS:` and `DEVS:`, so ART's boot directory presents the assigns a
real system boot would; `S:` matters concretely because WHDLoad reads
`S:WHDLoad.prefs`, and without it `S:` would have silently resolved to ART's
own `S` drawer instead of the user's system.

Test: `the_startup_sequence_assigns_from_the_system_and_runs_the_slave` and
`the_boot_directory_is_written_where_art_owns_it`, both updated to pin the
complete new script text with `assert_eq!`.

**Still unproven — and unexercised by the `1000 Miglia` run for a structural
reason, not an oversight.** The fixed script has not yet been run against the
real AmiKit image. `1000 Miglia`'s later successful run (ART-148/ART-149/
ART-151) is a self-booting `Media::WhdloadHardfile` — ART-147's fix routes
that shape through `RequestKind::Hardfile`, mounting and booting the image's
own filesystem directly, with no ART-generated boot directory and no
`S/Startup-Sequence` of ART's own in play at all. Only `RequestKind::Whdload`
(an unpacked drawer paired with a separate system volume, Y1/Y2) ever reaches
`whdload_boot::startup_sequence`, and no title in this run took that path.
Whether the fixed script now runs is still open.

**ART-142** 🟠 ✅ **A comma in a mounted folder's path shifts every field
after it in the generated WinUAE configuration** — *filed 2026-08-18,
collection-wave-c Task 12, out of Task 9's own deferred finding; fixed in the
whole-branch review's fix pass, same day*
`src-tauri/src/core/winuae.rs::checked_config_value` ·
`filesystem2=` and `hardfile2=` are both comma-delimited WinUAE directives —
`hardfile2=<rw|ro>,<device>:<path>,<sectors>,<surfaces>,<reserved>,
<blocksize>,<bootpri>,<filesystem>,<controller>` and
`filesystem2=<rw|ro>,<device>:<volume label>:<host path>,<bootpri>` both put
unrelated fields after the path — but `checked_config_value` only rejected a
line break, the corruption its own doc comment named. A Windows folder named
`Games, Amiga`, mounted as a directory volume, turned
`filesystem2=rw,DH1:Game:D:\Games, Amiga,0` into six comma-separated fields
instead of four, so WinUAE would have read the boot priority wrong rather
than the line being refused outright. Pre-existing in the `hardfile2=` loop
since before this wave — an ADF filename rarely carries a comma — but
collection-wave-c's directory mounts (§4.3 of its design) made it far more
likely to fire, since a Windows folder is a name the user chose, not one ART
generated.

Fixed the same way the newline case already was: `checked_config_value` now
refuses a value containing a comma with the same `CoreError::InvalidInput`
shape, covered by two new tests,
`a_comma_in_a_directory_mount_path_is_rejected` and
`a_comma_in_a_hardfile_path_is_rejected` (`core/winuae.rs`).

**ART-141** 🔴 ✅ **Play mounted an `.rp9`'s hardfile as the zip package
itself, writable** — *found in code review of Task 11 (Play, collection-wave-c),
fixed the same day*
`src-tauri/src/commands/launch.rs`, `src-tauri/src/core/launch/extract.rs` ·
`commands/launch.rs::request_kind_from`'s `Media::Hardfile` arm discarded the
record's `file` field — which for an `.rp9` is the zip entry name a
`<harddrive>` tag names, e.g. `af-application.hdf` — and used `args.path`, the
`.rp9` package itself. `media_for_plan`'s `Hardfile` arm then had no
`is_rp9`/extraction branch at all, unlike the `Floppies` arm beside it, so
`hardfile_paths` ended up holding the path to the zip, mounted **writable**
(this branch never sets `write_protect_hardfiles`) via `hardfile2=`. WinUAE
would have opened the user's own `.rp9` archive as a raw block device and
could have written AmigaDOS filesystem structures over it the moment the
"game" tried to save — not a failed launch, a corrupted package, hence 🔴
rather than 🟠. Not hypothetical: the user's catalogue holds 242 `.rp9`
packages, and a hardfile-based one (Enzo's collection) is an ordinary shape.

Fixed with the same treatment the `Floppies` arm already had:
`request_kind_from` now carries `Media::Hardfile { file }`'s `file` through
untouched (mirroring how `Floppies { ordered }` was already handled), and
`core/launch/extract.rs` gained `unpack_hardfile` — `unpack_floppies`'
counterpart, sharing its entry-resolution logic through a new private
`unpack_named` but with its own ceiling (`MAX_HARDFILE_BYTES`, 512 MB, the
same one `core::gameindex::scan::MAX_TITLE_BYTES` already treats as "one
catalogued title" — `unpack_floppies`'s 8 MB floppy ceiling would have
refused any real hardfile). `media_for_plan`'s `Hardfile` arm now branches on
`is_rp9(&request.path)` exactly as `Floppies` does: extract-then-mount for a
`.rp9`, mount the catalogued path directly otherwise.

Also fixed, filed against the same review:

- `detect_winuae(None)` ignored the user's configured WinUAE path
  (`settings.winuaePath`) — the same path `commands/winuae.rs::winuae_launch`
  already honours. `launch_title` now takes a `winuae_path: Option<String>`
  sibling argument, the same shape `winuae_launch` uses, and
  `src/lib/launch.ts::launchTitle` takes it from the caller; `TitleDetail.tsx`
  reads it from `useSettingsStore`.
- The three settings Play needs — `launch.romDir`, `launch.defaultMachine`,
  `launch.systemVolume` — existed only inside an already-open title's Play
  panel, so a user who had never opened one (in particular, never set the
  bootable system a WHDLoad launch cannot work without) had no way to find
  them. `src/pages/Settings.tsx` gained a Play section reusing the exact same
  `useRemembered` keys, so both surfaces read and write the same values.
- `media_for_plan` — the highest-risk function in the task and the one with
  no direct test, which is how the mounting bug got through — now takes plain
  `launch_dir`/`boot_dir` paths instead of an `AppHandle`, making it callable
  from a plain unit test with no running Tauri app. Ten new tests cover it
  directly: a plain floppy, an `.rp9` floppy set, a plain hardfile, an `.rp9`
  hardfile (`an_rp9_hardfile_is_extracted_not_the_package_itself` — fails
  against the pre-fix code), and WHDLoad in both one-click and
  mount-and-hand-over modes, asserting the system image stays read-only, the
  game drawer stays writable, and the boot directory's priority outranks
  both. `core/launch/extract.rs` gained four more:
  `the_hardfile_comes_out_from_under_its_entry_name`,
  `a_hardfile_the_package_does_not_carry_is_an_error`,
  `a_hardfile_larger_than_a_floppy_ceiling_still_unpacks`.

**ART-140** 🟡 ✅ **The palette was chosen by eye, and the light theme put
2.20:1 text inside its own success badge** — *found and fixed 2026-08-18, the
contrast pass the user asked for after [ART-139](#fixed)*
`src/styles/theme.css`, `src/styles/global.css`, `scripts/contrast-check.py` ·
The light theme had already been widened once by looking at it. Measured
against WCAG — 4.5:1 for text this size — it still failed in 23 places, and the
dark theme in 9:

| | light | dark | needs |
|---|---|---|---|
| `.badge-ok` text on its own tint | **2.20** | 4.29 | 4.5 |
| `.badge-warn` text on its own tint | **2.07** | 4.97 → 3.75 hovered | 4.5 |
| `.badge-err` text on its own tint | **2.85** | **3.40** | 4.5 |
| `--text-faint` (file paths) | **2.85** | **3.54** | 4.5 |
| white on `.btn-primary` | **3.94** | 6.42 | 4.5 |
| `--border-strong` (input edges, drop zone) | 2.23 | **1.95** | 3.0 |

**The cause is one token doing two jobs.** `.badge-ok` sets
`background: color-mix(in srgb, var(--ok) 22%, transparent)` and
`color: var(--ok)` — the same colour as its own background, 22% apart. No value
of `--ok` fixes that: darkening it to clear the tint takes the fill down with
it, and solving both at once produces `#614710` for a warning, which is mud.

So each meaning now has **two** tokens: `--ok` / `--warn` / `--err` / `--accent`
stay the mark (fills, borders, `accent-color`, the tint), and `--ok-text` /
`--warn-text` / `--err-text` / `--accent-text` are the same hue moved in
lightness until they clear 4.5:1 against all four surfaces *and* against their
own badge tint on each. The dark theme's identity colours are untouched; the
light theme's accent moved `#2d8aab` → `#297e9c`, which is what the primary
button's white label needed.

Two more from the same sweep, both from hard-coded colour rather than tokens:
`ErrorBoundary` had the dark theme's hex values baked in, so the crash screen —
the one you see when everything else has failed — showed its stack trace at
2.3:1 on a light page; and the Hex viewer's header read `var(--text-muted)`
inside a hard-coded `#0d1117` panel, which the light theme turned into dark
grey on near-black.

**The check is now a script, not a judgement.** `scripts/contrast-check.py`
reads `theme.css` itself, computes all 90 pairs ART actually renders — including
each badge's tint composited over each surface — and exits non-zero below
threshold. It needs no browser and no dev server, so unlike `zoom-check.py` it
**runs in CI**. All 90 pass.

Not covered, deliberately: `--border` (a panel edge is not the only thing
separating a card from the page) and the File Manager's `--tc-*` palette, which
is Total Commander's own, taken from the user's config and already theme-aware.

**ART-138** 🟡 ✅ **ROM Manager said `CRC ERR` about ROMs it simply did not
recognise** — *found 2026-08-18 photographing the screens for the README; fixed
the same day*
`src-tauri/src/core/rom/mod.rs`, `src/lib/rom.ts`, `src/pages/RomStudio.tsx` ·
Scanning a real ROM folder marked accelerator and SCSI-controller ROMs —
`A2630_390282-06.bin`, `A4091.rom`, `apollo_12xx_v560.bin`,
`Blizzard_1230-IV.rom` — `CRC ERR`. Not recognising them is correct; calling
them damaged is not, and ART had no basis for it.

**The cause was a two-valued field.** `RomInfo.checksum_valid` was a `bool`, so
"this is a Kickstart and its checksum does not verify" and "there is no
Kickstart checksum here to verify" arrived at the screen as the same `false`.
It is now `RomChecksum::{Valid, Invalid, NotChecked}`, and the badge for
`NotChecked` says *not a Kickstart* — or *encrypted*, when the file is a
licensed Amiga Forever dump with no `rom.key` beside it (ART-128), which is a
Kickstart ART cannot read rather than a file that is not one.

**How ART tells them apart, measured rather than assumed.** Two structural
marks Commodore's build leaves and damage between them does not touch: the
opening `$11xx 4EF9`, and the eight bytes `00 1C 00 1D 00 1E 00 1F` that end
the table after the stored checksum. Over ~150 real files — the 76 in this
project's ROM folder, the AmigaOS 3.2/3.2.1/3.2.2 releases and an Amiga Forever
export, Kickstart 0.7 through 47.111 — carrying both marks and summing
correctly agreed **exactly**: every image with both summed, and no accelerator,
SCSI or split half-image carried either. Re-measured through `identify_rom`
itself, the user's folder now reports `valid=30 invalid=0 not-checked=46`
where it previously accused 46 files of damage.

The size-based fallback name was the same unfounded claim from the other side:
a 256 KB accelerator ROM was called *Generic Amiga 256KB ROM (Kickstart 1.x)*.
It now reads `Not a Kickstart image (256 KB)`, and the generic Kickstart names
are kept for images that actually are one.

Second half of the report, and a different thing: `Compatible Amiga Models:`
was blank for the CDTV Extended 2.30 because the Remus database names no
machine for it — correct data, rendered as an empty gap that read as a missing
feature. The screen now says so in words.

Tests: `a_rom_that_is_not_a_kickstart_is_not_accused_of_a_bad_checksum`,
`a_kickstart_whose_body_changed_still_reports_a_bad_checksum`,
`a_kickstart_is_recognised_by_its_opening_and_its_tail`,
`an_encrypted_rom_with_no_key_says_nothing_about_its_checksum`, plus
`checksumBadge`'s four keys in `src/i18n/phrase-keys.test.ts`. The
`ART_ROM_DIR` hook prints the verdict per file and the three totals.

**ART-139** 🟡 ✅ **Aminet's text inputs rendered white in the dark theme** —
*found 2026-08-18, photographing the screens for the README; fixed the same day*
`src/styles/global.css` · The catalogue search box, the subfolder field and two
dropdowns were bare `<input>`/`<select>` elements, so they came from the
browser's own stylesheet: white boxes with black text on a dark page. The
Collection's search box beside them was dark only because that screen styles it
inline.

Fixed where it belongs — a theme rule for `input`, `select` and `textarea` in
`global.css` rather than another inline style on one screen. Written inside
`:where()` so it carries zero specificity: every existing inline style and
class rule still wins, so nothing that already looked right changed.
Checkboxes, radios, ranges and colour swatches keep their native shape and take
`accent-color`. Verified in both themes in a real browser (headless Chrome
against `pnpm dev`, the same approach `scripts/zoom-check.py` uses).

**ART-137** 🔴 ✅ **99 of 758 records reported a Kickstart image whose name was
68000 machine code — `ws_kickname` is a list when `ws_kickcrc` is `$ffff`** —
*found 2026-08-18 photographing the Collection for the README; fixed the same
day*
`src-tauri/src/core/gameindex/readers/slave.rs`,
`src-tauri/src/core/gameindex/record.rs` · Two cards on screen read
`Needs Kickstart ÔöÇÔûêÔûêÔûêÔûêÔûêÔûê` where their neighbours read
`Needs Kickstart 34005.a500`. The bytes behind it were not a mangled string,
they were code — and the shape repeated across every affected title with only
three bytes changing.

**The cause, decoded from the bytes rather than guessed at.** `ws_kickname` was
being read as a string when it is really a **list**: `(crc16, rptr-to-name)`
entries ended by a zero word, with the names laid out immediately after it.
What says which shape it is is `ws_kickcrc` — `$ffff` is not a checksum, it is
the marker for "the name field is a list". Every affected record had it.

```
rptr 0x1330 →  9f f5 | 13 49      crc 0x9ff5 → name at 0x1349
               75 d3 | 13 55      crc 0x75d3 → name at 0x1355
               97 0c | 13 3e      crc 0x970c → name at 0x133e
               00 00              end
               "40063.a600"       0x1330 + 14 = 0x133e ✓
```

The decode holds up three independent ways, which is why it was trusted without
the autodoc text (whdload.de could not be retrieved through a fetch that
session): **two real slaves carry the same three CRCs**; every entry's pointer
lands exactly on a name; and each name ends one byte before the next entry's
pointer. It then produces names that exist in the world — `40068.a1200`,
`40068.a4000`, `40063.a600`, which is precisely what a game that runs on an
A1200, an A4000 or an A600 would ask for.

`KickstartNeed` gained `alternatives`; `image` holds the first so a screen
written before this keeps working, and **`crc16` is left `None` rather than
recording `$ffff`** — a sentinel stored as a checksum is the same class of lie
as the garbage name it came with. `GAMEINDEX_SCHEMA` moved 1 → 2, which is what
that number is for: an Update re-reads every record written by the older reader
without the user having to know why.

Tests: four synthetic (the list, the sentinel not becoming a checksum, a single
name still reading as one, a truncated list, an entry pointing nowhere) plus
`one_real_slaves_kickstart` — `#[ignore]`d and env-gated — which asserts the
invariant rather than a value somebody's disk happens to hold: **nothing ART
reports as a Kickstart image may be unprintable.**

**ART-136** 🟠 ✅ **The ADF path assumed TOSEC filenames; of 847 real ADFs,
none are TOSEC — so one game became five entries and artwork matched 3 %** —
*found 2026-08-18, indexing a second real folder; the tool that answers it
landed the same day*
`src-tauri/src/core/gameindex/cleanup.rs`,
`src-tauri/src/commands/gameindex.rs` · Adding a second folder made an
assumption visible that a WHDLoad-only library had hidden. `readers/tosec.rs`
parses TOSEC names — `Title (1990)(Publisher)(Disk 1 of 3).adf` — and the real
material is hand-named: `A-Train Disk 1.adf`, `ADPro_D3.adf`,
`CaptiveII_Disk1.adf`, `dune2-2.adf`. **Zero of 847 matched the convention.**

Three consequences, measured rather than supposed:

- **The disk number lands in the title.** `A-Train Disk 1` and `A-Train Disk 2`
  are two entries for one game; `ADPro_D1` … `_D5` are five.
- **Artwork cannot match.** 3 % for the hand-named ADFs against 60 % for the
  WHDLoad folder beside them, because `A-Train Disk 1` is in no thumbnail index
  anywhere.
- **The parser does not merely fail, it mangles.** `(c) 1990 Svein Berge.adf`
  came out as `1990 Svein Berge` — a parenthesised group stripped as though it
  were a TOSEC field. Provenance then records `tosec-name`, claiming a TOSEC
  name was parsed when the filename was taken raw.

**The fix is a tool, not a guess**, which was the project owner's call: ART
proposes and the user accepts. `cleanup::suggest_title` removes an explicit disk
marker (`Disk 3`, `_D3`, `-Disk^1`, `disc2`) and `suggest_stem` keeps it in one
form, because a title and a filename want opposite things from a disk number —
two disks are one game, but they must stay two files.

A bare trailing number is the hard case and needs the neighbours: nothing in
`dune2-2` says whether the 2 is a disk or a sequel, and `dune2` itself ends in a
digit. `disk_sets` reads every name at once and accepts a set only when it
**begins at disk one** — 163 of the 174 numbered groups in the real collection
do. That rule earns its keep in both directions: it groups `4D Driving 1/2` and
`apoc1/2/3`, and it leaves alone both `Turrican 2` beside `Turrican 3` and
`LSD_042` … `LSD_064`, which are eighteen issues of a disk magazine that share a
base, are numbered, and would otherwise have collapsed into one title.

What no rule can settle — `brian the lion 2`, with no disk one anywhere — is
typed by the user. Applying a title writes a `UserEdit` override (top
provenance, undoable); renaming the file is a separate button that confirms
first, refuses an existing target rather than replacing it, and is logged. The
catalogue follows the renamed file by its content-derived id, verified on real
material.

847 files now resolve to **523 distinct titles**, 606 with a suggestion. The
real-material hook is checked in, `#[ignore]`d and env-gated, and asserts the
property that matters rather than a count that belongs to somebody's disk: the
magazine issues stay as many titles as there are issues.

**ART-135** 🟠 ✅ **One politeness rule for every host, and four pictures fetched
for the one the screen shows: a one-minute job took forty** — *found 2026-08-17,
driving the artwork run against 1700 real titles; fixed the same day*
`src-tauri/src/core/artwork/enrich.rs`,
`src-tauri/src/core/artwork/sources/` · The user asked whether waiting an hour
for cover art was right. It was not, and both halves were the engine's own
doing.

`REQUESTS_PER_SECOND` was a single constant applied to every source. It was
chosen for **whdload.de**, which volunteers run on a small server and where four
requests a second is right — and then applied unchanged to **libretro's
pictures, which come off GitHub's CDN**. Holding a CDN to a volunteer server's
pace is not politeness, it is a mistake with a courteous name. The rate is now
stated per source ([`ArtSource::requests_per_second`]) under a ceiling in
`enrich`: whdload.de 4, libretro 16.

The larger half: libretro publishes **four** kinds per title — boxart, snap,
title screen, logo — and the run fetched all four, while the Collection renders
exactly one. Three-quarters of the wait bought pictures nothing displays.
`EnrichRequest::wanted` now carries what the caller will render, and
`commands/artwork.rs::DISPLAYED_KINDS` names the two the screen actually shows.
Wave C widens that list when it has somewhere to put the rest.

Measured against the user's real library: about forty minutes to about one.
Tests: `only_the_wanted_kinds_are_fetched` (fails if a kind nobody asked for is
requested) and `each_source_states_its_own_rate_and_none_exceeds_the_ceiling`
(fails if a CDN and a volunteer's server are asked at the same pace).

**ART-134** 🔴 ✅ **The artwork index was written only at the end of an
hour-long run, so an interruption orphaned every picture it had fetched** —
*found 2026-08-17, driving the first real artwork run; fixed the same day*
`src-tauri/src/core/artwork/enrich.rs`,
`src-tauri/src/core/artwork/cache.rs` · The user reported that downloaded
artwork did not appear on screen. It had downloaded: **790 pictures were on
disk**. What was missing was `index.json` — the record of what had been fetched
— because `cache.save()` ran once, after the last title of a run over 1700 of
them. The run was interrupted, so nothing knew those 790 files existed: the
screen reads the index and saw an empty cache, and the next run began
downloading all of them again.

Two fixes, and the second is what recovers work already done:

- **The index is written whatever happens.** `enrich` now runs the work in an
  inner function and saves unconditionally on the way out — cancelled, failed
  or finished — plus every three seconds during the run. Time rather than a
  title count, because the screen reads the same file to show pictures as they
  arrive, and a count would save every thirty seconds when titles match and
  every few when they do not.
- **A picture already on disk is adopted, not fetched again.** `Cache::adopt`
  takes an existing file into the index without touching its bytes, which is
  what rescued the 790 orphans rather than re-downloading them.

The tests that existed did not catch this because every one of them ran a
handful of titles to completion; a long run that stops half way had never been
exercised. Both fixes were mutation-checked: disabling adoption fails
`pictures_on_disk_without_an_index_are_adopted_not_refetched`, and saving only
on success fails `a_cancelled_run_keeps_its_record_and_the_next_run_refetches_nothing`.
Stated precisely, because the distinction matters: the real failure was the
**process ending**, which no in-process test reproduces — the cancellation path
already saved. The adoption test is the one that covers what actually happened.

**ART-133** 🔴 ✅ **`window.confirm` asked nothing, so thirteen confirmations
never fired — four of them in front of a delete** — *found 2026-08-17, driving
the catalogue screen; the `confirm` half fixed the same day, the `prompt` half
open below*
`src/pages/FileManager.tsx`, `src/pages/PistormStudio.tsx`,
`src/components/files/CheckoutPanel.tsx`, `src/pages/CollectionStudio.tsx` ·
Removing a folder from the catalogue removed it **with no dialog shown at all**.
wry disables WebView2's own script dialogs, so the browser's `window.confirm`
returns without asking, and every guard shaped like
`if (!window.confirm(…)) return;` was a guard that never fired.

**Thirteen of them, and the four that matter most stood in front of a
deletion**: `deleteEntry`'s two (whose own comment explains why a second was
needed — *"the first one is the reflex the user has already learned to click
through"* — while neither was asked), the delete-protected guard, and
`deleteMany`'s. Also silent: discarding a modified checkout, and deleting a
named PiStorm firmware set, which [ART-092](#fixed) had built that confirmation
for on purpose.

All thirteen now call `confirm` from `@tauri-apps/plugin-dialog`, which shows a
real dialog; it is async, so every call site is awaited. `dialog:default` was
already in `capabilities/default.json`, so no new permission was needed.

**The evidence is one observation, and the entry says so rather than
generalising.** What was seen is the folder removal. The other twelve are the
same API in the same webview, which is why they were fixed together — but
nobody has watched them fail.

**Still open — `window.prompt`, four sites**: new folder and rename in the file
manager, mark-by-mask, and Aminet's partition picker. A suppressed `prompt`
returns `null`, so those features would silently do nothing, and those screens
*have* been driven against real material before — so the inference is weaker
here, not stronger. There is also no drop-in replacement: the dialog plugin has
`confirm`, `ask` and `message` but no text input, so fixing them means building
an input dialog. Not a change to make on a guess.

Test: `src/i18n/no-window-dialogs.test.ts` bans `window.confirm` and
`window.alert` outright and **counts** the four `window.prompt` sites, so the
number moves only when somebody has looked. It earned its place immediately —
written to guard the eleven found by hand, it found six more the manual grep
had missed.

**ART-132** 🟠 ✅ **Three things the Collection screen got wrong, all found in
the first minute anyone looked at it** — *found 2026-08-17, driving G10's
screen in `pnpm tauri dev` against the real collection; fixed the same day*
`src-tauri/src/core/gameindex/scan.rs`, `src/pages/CollectionStudio.tsx`,
`src/components/JobBar.tsx` · The screen was opened, pointed at
`E:\amiga\Amigatolon\WHDload`, and reported as sitting at "100%" doing
nothing. It was doing plenty, and three separate defects were stacked on top
of each other:

1. **The index hashed a file before deciding whether it was a title.** The
   collection folder holds `WHDLoadPiStorm-180224.img` — a **29 GB** card
   image beside 1697 two-megabyte games — and `.img` is a hardfile extension,
   so the scan took its SHA-256. `read_one` identifies first and hashes second
   now, and refuses anything past `MAX_TITLE_BYTES` (512 MiB) outright: the
   largest real single-game hardfile here is 93 MB, and a file past that
   ceiling is a container, not a game. This is ART's own "never read a whole
   user file when only its header is needed" rule, which the first cut of this
   scan broke.
2. **Two scans ran at once.** `startScan` remembers the folder, which changes
   `rememberedDir`, which re-runs the resume-where-you-left-off effect — whose
   guard still held the *previous* folder. So requesting a scan started a
   second one of the same folder. Two "Indexing titles" jobs, side by side, on
   the same 1699 files. **This predates G10**; with the old scanner's
   seconds-long run nobody could see it, and a ten-minute run made it obvious.
   `startScan` claims the folder for both guards before its first `await`.
3. **The progress bar rounded 99.6% up to 100%.** `Math.round` → `Math.floor`.
   A bar may say 99% while the last item finishes; it must never say 100% with
   work left, because that reads as finished-and-stuck.

Test: `a_file_too_large_for_a_title_is_skipped_without_being_read`, which
builds a sparse file past the ceiling and asserts the scan stays fast — a
reader that still hashed it would take visibly longer than the test allows.
The other two are screen behaviour and were verified by opening it again.

**ART-131** 🔴 ✅ **A bare hardfile whose filesystem is smaller than the file
would not mount — 1456 of the user's 1697 WHDLoad hardfiles** — *found
2026-08-17 while building G10's hardfile reader; fixed the same day*
`src-tauri/src/core/volume/mount.rs`, `src-tauri/src/core/gameindex/readers/whdhdf.rs` ·
A bare hardfile records its own extent nowhere: there is no RDB to ask, so ART
placed the root block from the file's own length (`total_blocks / 2`). Amiga
partitions are whole cylinders and a hardfile's *file* need not be, so a volume
of 1824 blocks lives happily in a file of 1843 — and the root block is at 912
while that calculation says 921. Everything read from 921 is somebody's game
data, which is why the failures came back as `header block has type
-1409280683, expected 2` rather than as anything mentioning geometry.

**Measured across the user's whole collection**: computing from the file's
length finds the root block in **241** of 1697; rounding down to a whole
cylinder (32 blocks) first finds it in **1697**. Nothing about this is specific
to the game index — ADF Studio and the Files screen could not open those 1456
images either. G10 is only what pointed at it.

`mount()` now probes rather than assumes: the file's own count is tried first,
so every image that already worked reads exactly as before, and the
cylinder-aligned count is tried only when that block is not a root block. When
neither looks right the original count is kept, so a corrupt image still fails
where and how it failed before. The geometry is rebuilt with the volume's real
extent, not just its root — otherwise a write could allocate a block the
bitmap does not cover.

Two smaller findings from the same sweep, both in `whdhdf.rs`:
**AmigaDOS's thirty-character filename limit truncates the extension too**, so
`20000MeilenUnterDemMeerDe.Slave` is `…MeerDe.Slav` on disk — thirty-two real
images, and a whole-extension match found none of them. A name now gets a file
onto a shortlist and `read_slave` decides from the bytes, which is
`core/detect`'s own rule one level down. And the entry ceiling started at 4096;
`Beneath A Steel Sky v2.2 CD32` reached it, so it matches
`core::whdload::MAX_ENTRIES` at 20 000 now.

Tests: `a_volume_shorter_than_its_file_still_mounts`,
`a_volume_that_fills_its_file_is_left_alone`,
`an_image_with_no_root_block_anywhere_keeps_its_own_count`,
`a_name_truncated_by_amigados_is_still_a_slave`,
`a_truncated_name_without_a_slave_inside_is_not_a_game`. Real material:
`read_the_real_hardfiles_when_asked` reads **1697 of 1697, 0 refused** (was
249 of 1697).

**ART-129** 🟠 ✅ **The ROM pairing check stayed silent for the pairing it
exists to warn about — twice over** — *found 2026-08-17 in G9's final review;
fixed 2026-08-17*
`src-tauri/src/core/rom/pairing.rs`, `src/components/osbuilder/VolumePreload.tsx` ·
Two independent silences, either of which put a V47 system volume onto a V40
card with nothing at all rendered above the destructive confirmation. Both are
the §89 failure — a missing answer read as a pass — one level up from where
G9's design was written to prevent it.

1. **`compare` returned `Paired` before the requirement was evaluated.** A
   tree records both the ROM it was planned against *and* what it needs, and
   the two can disagree: plan AmigaOS 3.2 against a real Kickstart 40.68 with
   `modules-a1200` excluded — a supported choice, with a shipped test of its
   own — and `plan()` writes `{ statedMajor: 40, requiresMajor: 47 }`. Build
   the card with that same ROM and the hashes match, so identity answered a
   question it was never asked. Fixed by asking the recipe's question first;
   identity now only chooses between `Paired` and `Suitable` once the answer
   holds. Test: `identity_does_not_excuse_a_requirement_the_rom_never_met`,
   with `identity_still_pairs_when_the_requirement_holds` and
   `two_empty_hashes_are_not_the_same_rom` beside it.
2. **Only the first filled partition was asked about.** The preload screen
   takes a content folder per partition and the plan emits a `copy-in` for
   each, but the pairing did `picks.find(…)` and rendered an unqualified
   sentence. A staging folder on DH0 made ART talk about the one folder not at
   risk; a paired DH0 made it say nothing while DH1's tree went on unwarned.
   Fixed by asking about every chosen folder and rendering one line per folder
   that has something to say, each named by its drive. Tests:
   `useRomPairing.test.tsx`'s "asks about every chosen folder, not just the
   first" and "keeps a warning that a paired folder used to swallow, named by
   drive".

Three smaller wrongs from the same review, fixed in the same pass. **Silence
meant four things**, one of which was the check failing: `paired` renders
nothing by design, and so did a verdict still in flight and a rejecting
command. A third `NotCheckedReason`, `check-failed`, is held on the rejection —
per folder — and a muted "checking…" line sits above the confirmation while
answers are outstanding; the checkbox is never disabled, because this warns and
never blocks. **The `unsuitable` sentence hard-coded V47** beside its own
interpolated `{{needs}}`, so a recipe naming 45 would have printed "built for
V45 … the Amiga will stop asking for V47"; the observed AmigaOS message now
lives in `unsuitable47` and is used only where it is true. And
`notChecked.card` asserted a cause it had not established — a corrupt or
too-new manifest is neither of the two it named — so it now says only that the
card's manifest did not answer.

**ART-128** 🟠 ✅ **A licensed Amiga Forever ROM went onto the card
encrypted, and the card could not boot** — *found and fixed 2026-08-17, from
the user saying they own Cloanto ROMs too*
`src-tauri/src/commands/card.rs`, `src-tauri/src/core/rom/mod.rs`,
`src/pages/RomStudio.tsx` · An Amiga Forever ROM is the same Kickstart behind
an `AMIROMTYPE1` header and a repeating XOR against the buyer's own
`rom.key`. `payload_for` read it with a plain `std::fs::read` and handed the
bytes straight to the boot partition, so the card carried eleven bytes of
header and half a megabyte of ciphertext where its Kickstart should be. The
Amiga would not start, and nothing on the way there said why: the build's only
note was `RomUnrecognised` — the same one any uncatalogued dump gets, which
reads as *probably fine*.

Two smaller wrongs sat beside it. `identify_rom` stripped the header and then
described the ciphertext, so a licensed ROM came back as *Generic Amiga 512KB
ROM* with no version and no machine — ART's weakest answer for a file it could
have read exactly. And the ROM screen showed a green **✓ Cloanto Encrypted
Header Stripped** badge for every such file, which is true and useless: the
header was off, the image was still encrypted.

→ **Fixed by treating these as first-class input, which is what they are** —
the user's community owns a mix of bare dumps and licensed Amiga Forever ROMs,
and uses ART for real Amigas rather than emulators. `decode_cloanto` undoes the
XOR; the key is looked for as `rom.key` **beside the ROM**, which is where
Amiga Forever puts it and where `amitools`' own loader looks (the algorithm was
read from that implementation rather than remembered). Decoded, the image goes
through the ordinary identification — stored checksum, name, machine — so a
licensed A1200 dump is now named and placed exactly like a bare one.

Without the key ART says so and stops: the ROM is named *Amiga Forever ROM
(encrypted, needs rom.key)*, claims no machine, and a card build is **refused**
rather than warned — refused because this is a certainty, not a risk, and
ART-103 is the precedent for stopping at one. The badge now splits into the
decoded case and an amber "no rom.key beside it" case.

Tests: `a_cloanto_rom_is_decoded_with_the_key_beside_it_and_then_identified`
(a synthetic ROM carrying a catalogued stored checksum, encrypted with a
synthetic key — recovering `Kickstart 40.68 (A1200)` proves the decode
produced the original bytes, not merely different ones),
`a_cloanto_rom_with_no_key_is_named_as_one_rather_than_guessed_at`,
`an_encrypted_rom_with_no_key_is_refused_rather_than_written_to_a_card`,
`an_encrypted_rom_reaches_the_card_decoded`.

**Then verified against genuine material, which took a turn worth recording.**
The user's Amiga Forever ROMs turned out to be on this machine after all, at
`E:\amiga\shared\rom` — and they are **not encrypted**: every one is a plain
262 144 or 524 288 bytes, as are the 39 on the original Amiga Forever DVD
image (`E:\amiga\AmigaForever-DVD.iso`). So this user's licensed ROMs already
work through the ordinary path, and the identification table proves it: **25
of the 41 files in that folder are named with their machine**, every one
agreeing with Cloanto's own filename — `amiga-os-310-a4000t.rom` gives
`Kickstart 40.70 (A4000T)`, `amiga-os-310-a600.rom` gives
`Kickstart 40.63 (A500/A600/A2000)`, and the CD32 main and extended ROMs are
told apart — while the boot ROMs, the keyboard MCU and the bonus ROMs in the
same folder correctly claim nothing. That is a **second, independent
collection** (Cloanto's own dumps, a different provenance from the TOSEC-named
set [ART-104](#fixed) was measured on) agreeing with the Remus table.

The DVD does carry a real `rom.key` (1426 bytes), which made the decode
testable against genuine material rather than a synthetic key: that key, the
user's own plain `amiga-os-310-a1200.rom` encrypted with it (524 299 bytes —
exactly Amiga Forever's encrypted form), and `identify_rom` answering
`Kickstart 40.68 (A1200)`. The original bytes, recovered with the licence key
the user owns. The extracted key and the ROM copy were deleted afterwards;
neither is in the repository and neither ever will be.

**ART-104** 🟡 ✅ **ART's ROM database matched none of the user's 29 Kickstart
dumps** — *found 2026-08-14 planning a card with the real material; fixed
2026-08-16*
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

**Measured before fixing, and it was worse than the entry said.** Not one
dump: `KNOWN_ROMS`'s ten hand-listed hashes matched **0 of the 29** Kickstart
files in the project's own collection. So `compatible_models` was empty for
every one of them, `rom_suits` returned `None` every time, and the
wrong-machine check had never fired for real material in its life. The ten
hashes also had no recorded provenance — nothing said where they came from or
against what they had been checked.

→ **Fixed by identifying a dump the way the ecosystem does.** Every Kickstart
stores a checksum 24 bytes before its end, and that value is unique *per
build*: `Kickstart 40.68 (A1200)` and `Kickstart 40.68 (A4000)` share a
revision and differ here, which is the distinction a revision alone can never
make (this entry's own earlier note explains why borrowing a same-revision
entry's machines was rejected). `identify_rom` now asks three questions in
order — the stored checksum, then the old SHA-256 table, then what the ROM
says about its own version — and only the first can name a machine.

The table behind it is **generated, not hand-listed**:
`scripts/rom-table-check.py` reads the Remus split database shipped with
`amitools` (GPL-2.0-or-later, compatible with ART's GPL-3.0-or-later; recorded
in `THIRD_PARTY_LICENSES.md`) and emits `core/rom/remus.rs` — 154 dumps with
their sizes, names and machines. CI runs the same script in verify mode, so
the committed table cannot drift from its source without the build saying so.

**It cannot grow a claim quietly.** Machine lists come from an explicit map of
the database's own 44 parenthetical strings, not from splitting them: the
obvious tokeniser read `A500/2000` as A500 alone and `A1200_R2` as nothing,
and a *partial* machine list is worse than none — `rom_suits` would then warn
"wrong machine" about a ROM that suits it, which is this entry in reverse. A
string the map has never seen stops the script rather than producing a guess
(one already did: `Kickstart 45.61 AmigaForever (1200)`).

Measured after: **24 of the collection's Kickstarts named with their machine**,
the two 40.68 builds correctly told apart, and the 52 accelerator and
diagnostic ROMs in the same folder claiming nothing at all. Six Kickstart
dumps the database does not carry fall back to what they state about
themselves, exactly as before — no regression, no invention.

**Its mirror, fixed in the same pass**: the size-based fallback used to name
machines from a file's *length* — a 256 KB image was "A500, A2000", which is
what ART told the user about the CDTV extended ROM in their own collection,
and anything unrecognised was given the model `"Unknown"`. `rom_suits` never
acted on those (it declines when `version` is `Custom`), so nothing was
refused wrongly, but the screen showed a claim nothing measured. The size
still names the *shape*; it no longer names a machine
(`a_size_names_the_shape_and_not_the_machine`).

Tests: `a_catalogued_dump_is_named_and_placed_by_the_checksum_it_stores` (the
two 40.68 builds, distinguished by nothing but the stored value),
`an_entry_that_names_no_machine_claims_none`,
`a_size_names_the_shape_and_not_the_machine`, and
`identify_the_real_rom_collection_when_asked` — an `ART_ROM_DIR`-gated hook
that prints what ART makes of a real collection, since ART ships no ROM and
never will.

**ART-124** 🟡 ✅ **`apply()` reported how many plan items it ran, not what the
tree holds — and the manifest carried 94 paths twice** — *found 2026-08-16,
counting the tree the real run had already written; fixed the same day*
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

**The manifest was the worse half, found while fixing the count.** Every item
pushed a `FileRecord`, so `distribution.json` — which its own doc comment
calls "the only record… because the media itself is gone by then" — held 4047
records for 3950 paths, each duplicate claiming a *different* component put
the file there. One of the two claims was always false.

→ Fixed: destinations are tracked as they are written (keyed by `item.to`, the
same key `plan::detect_collisions` pairs claimants by, so the two cannot
disagree about what "the same destination" means). An override **replaces**
its predecessor's record instead of adding one, and `bytes` follows the
surviving file rather than summing every write that landed on the path.
Directories are deduped the same way — and ancestors no rule names
(`Prefs/Presets` on the way to `Prefs/Presets/Backdrops`) are now counted,
which was a second, smaller undercount hiding behind the first.

Measured against the real trees, both ROMs: 3950 files / 278 directories
(V47) and 3954 / 281 (V40), each matching a filesystem walk exactly, with no
duplicate path in either manifest. Test:
`the_report_counts_what_the_tree_holds_not_what_the_plan_did`, which counts
the tree on disk rather than deriving a number from `plan.items` — the
derivation being exactly what was wrong. Mutation-checked twice: reverting
either half fails it.

**ART-125** 🔵 ✅ **A fallback copy reported zero bytes, and the screen printed
that as a fact** — *found 2026-08-16 in [ART-122](#fixed)'s own verification
run; fixed the same day*
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

→ Fixed as the entry proposed: `CopySummary::bytes` is `Option<u64>`, where
`None` means *not answered* and `Default` is a **known** zero, so an
accumulator can start there without the two meanings colliding.
`CopySummary::absorb` folds a step into a run and one unanswered step makes
the total unanswerable — a sum missing an addend is not a sum. `hst-imager`'s
parser answers `None` unless it reads an integer byte count, and the screen
picks a sentence without the byte clause (`preload.result.copiedNoBytes`)
rather than printing a zero. The real run now says `bytes=not answered`
where it used to say `bytes=0` against 12 MB. Tests:
`a_rounded_size_is_not_answered_rather_than_answered_wrongly`,
`an_unreadable_listing_answers_no_byte_count_either`,
`an_unreadable_summary_is_zero_rather_than_an_error` (updated), and
`copiedPhrase`'s three cases — including that a **real** zero still prints,
since that is a different answer.

**ART-126** 🔴 ✅ **Every RDB filesystem ART has ever embedded was ignored by
AmigaOS: `PatchFlags` named the wrong field** — *found and fixed 2026-08-16,
by booting what ART built*
`src-tauri/src/core/rdb.rs` (`create_rdb_layout`, the FSHD block) · The
`FileSysEntry`'s `PatchFlags` says which of its fields AmigaOS copies into the
device node — one bit per field, **in structure order**: `Type`(0), `Task`(1),
`Lock`(2), `Handler`(3), `StackSize`(4), `Priority`(5), `Startup`(6),
`SegListBlock`(7), `GlobalVec`(8). ART wrote `0x10`, with a comment stating
that bit 4 was `dn_SegListBlock`. Bit 4 is `StackSize`, and the value beside
it was zero. So every disk ART wrote asked AmigaOS to patch a stack size to
nothing and said nothing at all about the driver it had just embedded.

**The consequence is the whole of G4**: the partition mounted with no handler,
which on a `PDS3` volume means it does not mount at all, and a *bootable* one
sent the machine into `Software Failure 8000 0008` — a privilege violation,
from jumping into a seg list that was never installed. [ART-084](#fixed)
closed with the words "a PFS3 disk ART makes now mounts". It did not. Nothing
ART built had ever been mounted by an Amiga; the claim rested on `rdbtool`
extracting the driver back SHA-256-identical, `hst-imager` listing it, and
ART's own parser reporting it — **none of which acts on `PatchFlags`**. It is
the project's own recurring shape (ART-032 … 035, ART-075, ART-079) arriving
one layer higher: every reader agreed, and the only thing that could
contradict them was a Kickstart.

Fixed to `0x180` — `SegListBlock` and `GlobalVec`, and nothing ART has no
opinion about. Not a guess: both of the user's real, booting PiStorm cards
were read for it. CaffeineOS 9317 writes `0x180` with `StackSize` unpatched;
MultibootOS 2.2 writes `0x190` for its PFS3 (the same two bits plus a
2048-byte stack) and `0x180` for its FFS.

Proved by the thing that found it — WinUAE, licensed Kickstart, the user's own
material, one variable changed at a time:

| Run | Kickstart | What happened |
|---|---|---|
| before the fix, booting the volume | 3.1 (V40) | `Software Failure 8000 0008` |
| before the fix, mounted beside a boot floppy | 3.1 (V40) | no volume icon — it never mounted |
| after the fix, same floppy | 3.1 (V40) | the volume appears on Workbench |
| after the fix, ART's *own* PFS3 format, booted | 3.1 (V40) | `hello from ART` at the `1>` prompt |

That last one is the first hard disk ART has ever made that an Amiga booted,
and it settles [ART-122](#fixed)'s open half as well: the real PFS3 driver
accepts the reserved-area layout `NativeFormatter` writes, which is what
`pfs3aio`'s own algorithm produces.

**Emulated is not provisional here.** WinUAE did not model any of this: the
ROM's own `expansion.library` parsed the RDB and the real `pfs3aio` 68k
binary mounted the volume. The evidence is the same code a real Amiga runs,
which is why one variable at a time through it was enough to find a defect
four independent readers had missed.

Test: `the_seg_list_and_global_vec_are_the_fields_amigados_is_told_to_take`,
which asserts the two bits are claimed *and* that `StackSize` and `Priority`
are not — the defect was a claimed field, so an unclaimed one is what the test
pins.

**ART-127** 🟠 ✅ **The tree G5 builds could not start Workbench: two
libraries missing, and the wallpapers left off on an assumption** — *found and
fixed 2026-08-16, by booting the tree once ART-126 let it boot at all*
`src-tauri/src/core/osinstall/recipes/amigaos-3.2.json` · With ART-126 fixed,
the real AmigaOS 3.2 tree booted far enough to speak for itself, and it asked
for three things in turn:

1. *"Please insert a volume containing LIBS/icon.library in any drive."*
   AmigaOS 3.2's A1200 ROM does not carry `icon.library`, and neither does
   `Workbench3.2` — whose `Libs` drawer holds 23 libraries, not this one. It
   is on `Install3.2`, a disk the recipe named **no component for at all**:
   it had been written off as "the OS's own boot floppy, not component media"
   when `find_media` reported 35 of 36 volumes and nobody asked what the 36th
   held.
2. The same, for `LIBS/workbench.library` — 185 KB, same disk, same reason.
3. *"ERROR: can't load picture Sys:Prefs/Presets/Backdrops/default_pal.iff"*.
   The `backdrops` component shipped `available: false` with a comment saying
   it would stay off "until somebody measures where the real installer places
   wallpapers". That was the right call while the answer was a guess; the
   running system has now named the path itself.

Fixed: a new required `install-libs` component takes those two libraries off
`Install3.2` (and only those two — `iffparse`, `locale` and `version` are on
that disk as well and `Workbench3.2` already ships them, so taking them would
be the collision ART-112 was), and `backdrops` is available, aimed at
`Prefs/Presets/Backdrops`. Tests:
`the_libraries_workbench_does_not_carry_come_off_the_install_disk`,
`backdrops_go_where_the_running_system_asked_for_them`.

**The result, which is what this entry exists for**: AmigaOS 3.2 boots to a
clean Workbench — wallpaper and all, no requesters — from a PFS3 volume ART
prepared, under WinUAE with the user's own licensed V47 A1200 ROM. The code
that read the disk and started the system is AmigaOS's own, executing, so
this is not a weaker claim than a hardware one *for what it covers*. What it
does not cover is the card path — MBR, an Amiga disk 1.1 GB in, Emu68's SD
driver, the FAT32 boot partition — which no emulator can answer.

Two things this dragged in with it, both worth keeping: the real-media hook
had one ROM's answers hard-coded (`modules-a1200` asserted on, and one exact
file count), so it failed the moment it was pointed at the user's *other* real
ROM — it now asserts the rule the condition encodes (on exactly when the
paired ROM's stated major is below 47) and pins a set of counts per ROM. And
`fixtures::required_media` exists because eight fixtures broke at once when a
second required component appeared: a required component's media is a
precondition of any plan, not something a test chooses.

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

**A fresh, concrete instance (Task 4 review, 2026-08-19).** The
`locale-turkish` package recipe (`recipes/packages/locale-turkish.json`)
places a `türkçe` catalog drawer — the exact non-ASCII AmigaDOS name this
entry's own measurement already lists among the 24 that tripped it on
`dist-3.2`. Applying this package onto a PFS3 volume natively will hit the
same `CoreError::NonAsciiPfs3Names` refusal and route through the
`hst-imager` fallback (`commands/preload.rs::run_with_fallback`) — this
package is real, current confirmation that the gap this entry describes is
still live, not merely historical. No code changed for this note; recorded
so the Turkish package's own PFS3-native limitation is written down
somewhere rather than only implied by this entry's general description.

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

**ART-084** 🟠 **An HDF created as PFS3 or SFS is a DosType with no filesystem behind it, and an Amiga cannot mount it** — *fixed 2026-08-12; **the fix was half a fix and nobody knew until 2026-08-16** — the driver was embedded correctly and then advertised with the wrong `PatchFlags`, so AmigaOS ignored it and the disk still did not mount. See [ART-126](#fixed): this entry's closing claim was verified only by tools that do not read that field.*
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
