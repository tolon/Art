# Lessons

**The incidents this project's rules were bought with.** Every rule in
`CLAUDE.md` that is not architecture and not testing mechanics was paid for
once, here. This file is the receipt: each section is a dated incident and ends
with the rule it produced, so a rule can be checked against what actually
happened rather than taken on faith.

It is history. A section describes the tree on the day it was written and is
never rewritten to match today's code — where a claim has since been overtaken,
say so in place rather than editing the story.

- Where the project is now: [STATUS.md](STATUS.md)
- What is broken or owed: [ISSUES.md](ISSUES.md)
- How the code is shaped: [architecture.md](architecture.md)
- Testing mechanics and the mutation discipline: [testing.md](testing.md)

---

## The failure that does not crash

**ART's most expensive defects do not crash. They tell the user a confident,
wrong sentence.** They pass every test, log nothing, and are invisible until
somebody reads the screen and acts on it.

One round produced five of them in three days:

- Nothing mounted the package, so ART would have said *"the installer ran and
  said no"* about a program that **never started**.
- The user's BoingBag carried an `Updater` that cannot run under an emulator —
  the same sentence, about a program that **could not work**.
- A recording failure turned a **successful** install into "it failed".
- A cancelled-run badge asserted *"the copy has been discarded"* over the top
  of ART's own *"could not be removed"* — the screen contradicting the core.
- A refusal told a user with a perfectly good Kickstart **3.1** that their ROM
  did not meet the minimum, sending them to look for a version that does not
  exist. The refusal was right; the sentence was not.

Earlier and worst: a tree that booted cleanly was shipped as AmigaOS 3.9 and
was **3.5**, because a copyright line was read as proof
([ART-169](ISSUES.md#fixed)).

**The rules that came out of it** — kept in CLAUDE.md, in full: endings stay
distinct; a refusal must be actionable; never claim what you did not do; the
screen may not out-claim the core; ask the artefact. And a progress bar showing
a fixed width when the total is unknown is the same defect wearing a different
hat — it looks like progress and carries no information.

---

## Research before design

**Verify from outside before you commit to a design.** Breaking this has cost
this project more time than every bug in it. Three stories, one per step of the
rule.

### 1. Run the thing you are about to depend on

A `7z l` on `BoingBag39-1.lha`'s payload showed 234 plain-looking entries, so
the content-layer spec was written around reading them. They are
ZipCrypto-encrypted: **listing works, extraction does not**, because the format
does not encrypt names. One `7z x` before the spec would have redirected the
whole round.

**Rule:** run it, do not merely list it.

### 2. Find out how the established projects solve it

HstWB Installer, AmiKit, AmigaSYS and ClassicWB all run the install *inside an
emulator* using the user's own OS files — which is how they install BoingBags
at all, and why ART's host-side placement has a ceiling. Cloanto's own
knowledge base names ART's non-ASCII locale-name problem as a known one for
directory-based distributions. All of it was found *after* the design, because
it was looked for after.

**Rule:** the prior art is research input, not a postscript. STATUS.md's
"What ART is, and the ceiling that comes with it" is where that finding now
lives as a standing constraint.

### 3. Read the medium's own root listing, and put it in the report

The AmigaOS 3.9 spec asserted that `Workbench3.5` is the CD's own name for the
3.9 tree "and not a mistake". `OS-Version3.9` also holds `Workbench3.9`. Eight
tasks and a merged branch later, the shipped tree was AmigaOS **3.5** — it
booted cleanly, and the copyright line was read as proof.

**Rule:** read the medium's own listing, quote it in the report, and ask the
artefact what it is rather than inferring it from a name.

---

## The controlled experiment

**2026-08-21.** One defect drew four explanations before the measured one, and
**all four were wrong**:

- a missing cache (true, but small);
- four jobs contending for the disk (measured at 1.18x, so contention was never
  the cost);
- a broken stop button (it called `jobCancel` correctly all along);
- and *"ART-178 is not the cause"* — the expensive one, because ART-178 **was**
  the cause and it had been eliminated by reading a stabiliser that the
  fallback path bypassed.

Reading the code produced three plausible wrong answers. An experiment produced
the right one. But the experiment worked because it was **controlled and
written down**, not because it was an experiment:

- **One variable.** Only `main`'s `useRemembered.ts` was swapped into the fixed
  tree, so the result could be attributed to that file.
- **A counted result, not an impression.** Not "it got faster": **27 plans at
  settle, 88 a quarter-second later**; with the fix, **27 and 27**.
- **Both arms reported.** The defective state beside the fixed one. A one-armed
  experiment says "I tried something and it improved" and cannot say what
  improved it.
- **The control measured too.** `catalogue` was identical across both runs, so
  it was *shown* not to be the churn rather than assumed innocent.
- **What would be measured, decided first**, so the result was read against an
  expectation instead of a story assembled afterwards.

**Rule:** record the eliminations as well as the finding. A refuted hypothesis
is written down so nobody walks that road again — and if an elimination later
turns out to be wrong, correct it in place and say so, because a confident
wrong elimination costs more than no elimination at all.

This is the same discipline as the mutation rule in
[testing.md](testing.md#a-test-is-not-a-guard-until-the-defect-has-been-put-back):
put the defect back and watch the test fail. Both measure the **difference**
rather than the claim.

---

## A one-off verification answers for the day it happened

The Aminet mirrors were checked live on **2026-08-09** and that sentence sat in
a doc comment for fifteen days meaning less each one — they are somebody else's
machines.

Where a check can be made re-runnable, make it re-runnable: `net/live_aminet.rs`
asks **each shipped mirror separately**, because failover stops at the first
that answers and a dead one in position two is invisible in an ordinary sync.

Same shape for real material rather than fixtures:
[ART-146](ISSUES.md) was *found* by reading `AmiKit.hdf`'s bytes and *tested*
against a synthetic header carrying those eight bytes — the right unit test, and
not the same claim, which is why the real 1.2 GB image is now asked directly.

**Say what is still not proven in the same breath**: that one measures the
configuration ART writes, not what WinUAE does with it.

**Rule:** cite what you checked and when; make the check re-runnable where you
can; and name what the check does *not* prove.

---

## A work list decays

**2026-08-24.** Five entries in one work list described work that was already
shipped — "Add to Collection", the editable mirror list, the install to HDF,
and two whole items before them. [FEATURES.md](FEATURES.md) said *"Stage A is
complete"* while the work list said three things were missing; the two
disagreed and the documents were right.

Two of the five were caught only because checking had become a habit — the
other three cost real work.

**Rule:** a work list is a claim about the repository, and it decays. Check the
entry against the tree before building against it, and when it turns out to be
stale, correct it in place with what was actually found.

---

## Shell traps

Four incidents, one family: **whenever a shell is between you and the text, the
shell gets a vote.**

### A path through a heredoc loses its backslashes

**2026-08-22.** A Windows path in a `<<'EOF'` block loses its backslash escapes
— `E:\amiga` arrives as `E:` plus a BEL byte, `\test\art-...` as a TAB and a
BEL. This produced corrupt paths in **three** documents, one of them committed
ten days earlier and unnoticed until a control-byte sweep found it. Nothing
fails; the text is simply wrong.

**Rule:** write paths through a file, never through a heredoc — the `Write`
tool for a script, the editing tools for a document. `scripts/control-byte-sweep.py`
is the standing guard and is blocking in CI.

### A commit message is a path too

**2026-08-24.** Inside `"..."` bash runs backticks as command substitution and
eats the quotes, so a message written with `` `to` `` and `` `from` `` in it
committed as *"A File rule's  is whatever the author typed"* — with two
`command not found` lines scrolling past above the success message. The commit
was already merged and pushed before it was noticed; history was not rewritten
for it, because the durable record is [ISSUES.md](ISSUES.md) and that was right.

**Rule:** write the message to a file and use `git commit -F`.

### A pipe reports the pipe's status

Never pipe a `git merge` into `tail`: the pipeline's exit code is `tail`'s, so a
conflicted merge reads as success and everything after it runs against a
half-merged tree.

**The same trap is not limited to `git`.** On **2026-08-22** a
`pnpm tauri build | tail` was reported as exit code 0 while the build had
actually failed (`os error 5`, the running `.exe` locked). The same applies to
`pnpm lint`, which is why STATUS.md says to run it unpiped.

**Rule:** whenever a command that can fail is piped, read its output, not the
pipeline's status.

### A relative `cd` that stuck, and a restore that threw the work away

**2026-09-07.** A mutation test on `src/lib/firstboot.ts` was "restored" with
`git checkout -- <file>` while the file carried 123 uncommitted lines; the
checkout put it back to `HEAD` and the work was gone (rebuilt afterwards from a
diff that happened to be in context).

Two traps in one: the backup `cp` had silently failed, because a `cd` from an
earlier command had stuck to the shell and its relative path pointed at the
wrong directory.

**Rule:** start every shell command from the repository root with an absolute
`cd`, copy the file to the scratchpad **by absolute path**, and restore from
that copy. Never restore a mutated file with `git checkout --`.

---

## The branch a subagent moved

A subagent may switch the working tree under you. This happened **three times in
one session**, and twice a `git push origin main` reported success while nothing
of the intended work had reached `main`.

**Rule:** run `git branch --show-current` before you commit. When an agent is
working in the checkout, give it a `git worktree` rather than changing branches
under it.
