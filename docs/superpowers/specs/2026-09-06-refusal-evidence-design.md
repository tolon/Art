# A refusal that shows its evidence

*Written 2026-09-06. **This is history**: it describes the tree on the day it
was written. Re-run the checks below rather than re-trusting them.*

The OS Builder's **"Why this cannot be built yet"** section already writes good
sentences. This round does not rewrite them. It gives them the one thing they
lack: **what ART actually found in the folder the user pointed at.**

The owner asked for this after driving the built application, which is the
second time this project has learnt more from one person at a screen than from
a suite — the WHDLoad round's own record says the same.

---

## 1. What was checked, and what it changed

### 1.1 The sentences are already good — the gap is evidence, not wording

All nineteen refusal strings were read (`src/i18n/en.json`, `osinstall.refusal.*`).
Several are already actionable in exactly the way `CLAUDE.md` demands — *"Untick
it, and install it on the Amiga with that Updater instead"*, *"give each layer
its own folder"*, *"Remove all but the one you want"*.

The gap is narrower and more useful than "reword them". Today:

> **Extras** needs **Extras3.2**, and no file in the media folder carries that
> volume name.

A user reading that cannot tell whether they pointed at the wrong folder, are
missing one disk, or have the whole set of a **different release**. ART knows
which, and does not say.

### 1.2 ART already has the answer and throws it away

`core/osinstall/plan.rs:1611` holds `found` — the complete scan of the media
folder — and the very next lines emit
`RefusalReason::MediaMissing { component, volume_name }`, carrying **none** of
it. The evidence is one variable away from the refusal that needs it.

### 1.3 …and it can already name the release, too

`core/osinstall/identify.rs` exists and does more than its callers use.
`identify(volume_names) -> MediaVerdict` returns a `ReleaseEvidence` carrying:

| Field | What it holds |
|---|---|
| `release` | which AmigaOS release this pile of media is |
| `distinguishing` | the volumes that identify it and no other release |
| `shared` | the volumes that could belong to several releases |
| `missing_required` | **the required volumes that are not there** |

`commands/osinstall.rs` calls `release_holding` and `layer_holding` for the
screen's own purposes. **Nothing carries any of it into a refusal.** So this
round is wiring and sentences, not new logic — which is why it is small.

### 1.4 Three things were dropped after checking the tree

The owner initially asked for content-hash identification as well. Three checks
removed it, and each is recorded so nobody re-proposes it from the same
starting point:

- **"A renamed file would be recognised."** Already true.
  `core/osinstall/scan.rs`'s own module doc: *"found by opening each candidate
  and reading its volume name from inside it, never by trusting its
  filename"* — `AdfSource::open` reads the label out of the root block.
- **"ART would know when material changed."** Already built. `scan_cache.rs`
  (ART-188) keys on `(path, size, mtime)`, and **its doc records that the
  owner's own earlier suggestion was a content hash** and why it was refused:
  hashing 468 MB costs about what the walk costs, so it trades the problem for
  itself. *"A hash answers 'are these two files the same'; this cache asks 'is
  this the same file I read last time', which is a different question and a
  much cheaper one."* It even handles a restored backup keeping its timestamps.
- **"A table would say which revision a disk is."** It would, and ART
  deliberately dropped one. `identify.rs`'s doc: a previous round *"dropped a
  186-row hash table that ART could not verify. So there is no table here."*
  Shipping Emu68 Hatcher's 186 MD5s would re-introduce exactly that.

**What survives of the idea is already in §1.3**: `missing_required` and
`distinguishing` answer "which release is this" from data ART generates itself,
with no table to verify and nothing new to ship.

---

## 2. What this round builds

### 2.1 One evidence block, not nineteen enriched refusals

The evidence is about **the folder**, not about each refusal, so it is gathered
once rather than duplicated into every variant. `InstallPlan` gains:

```rust
pub media_evidence: Option<MediaEvidence>,
```

carrying what the scan found, what `identify` made of it, and which required
volumes are absent. **`RefusalReason` does not change** — its variants stay as
they are, their sentences stay as they are, and nothing in the existing i18n
catalogue is rewritten. That keeps the diff small and keeps nineteen tested
sentences untouched.

### 2.2 What the screen gains

Above the existing list, in the section that already has a heading:

> The media folder holds **Workbench3.1, Extras3.1, Fonts, Locale, Install3.1**.
> That looks like **AmigaOS 3.1**, and this build wants **AmigaOS 3.2**.

and, when the release is right and only disks are missing:

> The media folder holds **Workbench3.2, Fonts, Locale, Install3.2**.
> **Extras3.2** and **Classes3.2** are not there.

### 2.3 The three states it must keep apart

`MediaVerdict` already distinguishes them and the screen must not collapse them —
this project's named failure class is exactly that:

- **Identified** — say which release, and whether it is the one being built.
- **Ambiguous** — say that the folder could be more than one release and name
  the candidates. Do **not** pick one.
- **Unknown** — say that nothing in the folder identifies a release. A folder
  holding only `Fonts` and `Locale` is genuinely ambiguous (both are unversioned
  across 3.1, 3.1.4 and 3.2 — `identify.rs` measured that), and claiming
  otherwise would be a confident wrong sentence.

And a fourth, distinct from all three: **no folder chosen yet**. "You have not
told me where to look" is not "I looked and found nothing".

### 2.4 What it must not do

- **Not guess.** Where `identify` says Ambiguous or Unknown, the screen says so.
- **Not fetch.** The refusals in this section are about AmigaOS install media
  and Kickstart ROMs — material the user must own. `CLAUDE.md`'s line stands:
  *"ART never downloads a distro image… That is a legal line, not a
  preference."* Emu68 Hatcher does not download install media either — measured:
  60 of its packages come from Aminet, 14 from web URLs, 6 from GitHub, but
  `os_install.yaml` has **no `download:` block at all** and `install_media.py`
  contains no download code. Add-on packages are a different question and a
  later round.
- **Not rewrite the nineteen sentences**, which are already good.

---

## 3. Verification

- Unit tests over `identify`'s three verdicts, asserting the **specific
  sentence** each produces and that no two are the same — a test asserting only
  "some evidence was shown" would pass with the distinction removed.
- A plan-level test that the evidence reaches `InstallPlan` from the same scan
  the refusals were built from, so the two cannot disagree about what the folder
  held.
- A frontend test per state, including **the empty one**: no folder chosen must
  read differently from a folder with nothing recognisable in it.
- Both i18n catalogues, in the same commit.
- **Mutation, per `CLAUDE.md`**: every guard has its defect put back and is
  watched to fail. This round has already found three trials that failed for the
  wrong reason; report the actual failure message for each, and a trial that
  fails on the wrong cause is not a pass.

---

## 4. Not in this round

- **Downloading anything** — §2.4.
- **Content hashing** — §1.4, dropped on three measurements.
- **The nineteen refusal sentences** — already good.
- **`Utilities`/`WBStartup` missing from the recipe** — ART-251, a
  recipe-content question.

## 5. Known risks

- **The evidence is only as good as the scan.** A folder of unreadable images
  produces an empty listing, which reads like an empty folder. The refusal for
  *that* is `MediaUnreadable`, which already exists and already names the file —
  so the evidence block must not imply the folder was empty when ART simply
  could not read what was in it.
- **`identify` is name-based, and two of the names carry no information.**
  `Fonts` and `Locale` are unversioned across releases — its own doc measured
  this against HstWB Installer's catalogue and two installation guides. So a
  folder holding only those is genuinely Unknown, and the screen must say so
  rather than reaching for a guess.
