# SD-2 G5 — decisions taken during execution

**Date:** 2026-08-15 / 16
**Plan:** [2026-08-15-os-install.md](2026-08-15-os-install.md)
**Spec:** [2026-08-15-os-install-design.md](../specs/2026-08-15-os-install-design.md)

---

This plan ran under subagent-driven development: a fresh implementer per task,
a review after each, and a fix loop. Where a review, a brief or the code
disagreed with each other, somebody had to decide, and the decisions were taken
on the user's behalf rather than by asking. **This file is the exhaustive
record of them**, so they can be read, argued with and reversed.

They are not all right. Several were corrected later by the very implementers
they were given to — those corrections are recorded here too, next to the
ruling they overturned, because a decision that was wrong is worth more to a
later reader than one that was quietly dropped.

The four that changed the product most, for anybody reading only this far:

- **A `Subtree` destination is a merge point, not a claim** — the rule that
  makes fifteen locale disks legal, and the one that later needed an exception
  for the *files* inside a shared drawer (ART-112).
- **The journalling claim was withdrawn.** `Journalled::begin` needs its block
  set up front and libpfs3 decides as it goes, so the design's safety argument
  was replaced with the one that is true: writes are bounded to the partition.
- **`import_filesystem` refuses.** `create_rdb_layout` builds an RDB from
  scratch on geometry real cards do not share.
- **`excluded` moved into the engine.** Turning a component off client-side
  left the manifest describing media that contributed nothing.

---


## Pre-flight scan

1. `recipe::amigaos_32()` returns `CoreResult<Recipe>`, not `&'static Recipe` — the code block is the authority over the summary line, and validation has to be able to fail. Cost if wrong: a needless clone per call, trivially changed later.

2. modules are declared as they are created — Task 1's `mod.rs` declares only `recipe` (plus `fixtures` under `cfg(test)`), and each later task adds its own `pub mod` line in its own commit. Cost if wrong: none; this is strictly required for the tree to compile task by task.

3. the shared fixtures grow per task rather than landing whole in Task 1. Task 1 adds `scratch`, `media`, `workbench`, `CancelAfter`, `digest_of_folder`, `fake_rom`; Task 5 adds `planned_with`; Task 9 adds `rdb_image` and `partition_offset`. Cost if wrong: a little churn in one test module.

4. **no `tempfile` dependency.** Fixtures follow the project's own convention — `std::env::temp_dir().join(format!("art-osinstall-{tag}-{}", std::process::id()))`, as `core/archive/extract.rs::scratch` and `core/layout/apply.rs` already do, cleaning up on the way in. Cost if wrong: slightly more fixture code than `tempfile` would need, and no new crate in a project that keeps its dependency list deliberately short.

5. Task 5's tests use `fixtures::planned_with(chosen, present, rom_major)` and derive their variants from it; the names `plan_with`, `plan_with_rom`, `plan_where_extras_has_no_l` and `plan_with_colliding_recipe` in the plan text are thin wrappers over it, written in Task 5. Cost if wrong: naming churn only.

6. Task 7 also modifies `apply.rs` — writing the `S:User-Startup` block is a step *of* apply, and Task 6 ships without it. Task 6's manifest and file writing stand alone and are testable without it. Cost if wrong: Task 7 grows by one integration test.

7. `InstallRequest` and every type crossing the command boundary carry `#[serde(rename_all = "camelCase")]`, matching `src/lib/*.ts`'s existing convention. Task 12's wire test is the thing that proves it. Cost if wrong: the screen fails on first use — which is exactly what that test exists to prevent.

8. converting `MediaEntry.protection: u32` to libpfs3's `u8` is a **checked** narrowing in Task 9, not an `as` cast. AmigaDOS uses the low eight bits; anything above them means the source is not what we think it is and is worth an error rather than a silent truncation. Cost if wrong: an error on a file some exotic volume stores oddly, which is the safe direction.


## Task 1

9. **a `Subtree` destination is a merge point, not a claim.** The implementer found that fifteen locale disks all writing `Locale/Languages` breaks the collision test, and it is right — but the answer is not an override chain, it is that the test asks the wrong question of drawers. Amiga install disks are built to merge into one volume: Workbench, Extras and Classes all contribute to `Devs/`, and each language disk contributes a different `.language` file to the same drawer. The defect this design guards against is two components writing the same **file** — which is what taking ModulesA1200's `C/` wholesale would have done. So the recipe-level collision test applies to `File`-kind rules only; coinciding subtree destinations need no declaration. The expanded, file-level check is `plan()`'s job in Task 5, which is where the spec put it ("a collision inside a plan is a defect"). `no_toolkit_disk_takes_a_whole_drawer_the_base_already_owns` is untouched — that is the real ModulesA1200 guard and it is orthogonal. Cost if wrong: two components could merge conflicting files inside a shared drawer and only Task 5 would catch it — which is the right place anyway, one task later.

10. the `Devs/` three-way overlap between `classes`, `glowicons` and `workbench-base` is a merge under the ruling above, so the implementer's added overrides come out. The one genuine file-level collision is `C/LoadModule`, claimed by both `storage` and `modules-a1200`. **Measured: the two binaries are byte-identical, SHA-256 35acfea734816965d271352f59c3643963f69c7e4b2469e3473c5f5a8a60fc14**, so the direction cannot break anything; `modules-a1200` overrides `storage`, because the Modules disk ships LoadModule beside the modules it exists to load. Cost if wrong: nil in this release — the bytes are the same file.

11. **measured — `MMULibs.adf`'s drawer is `Libs`, not `LIBS`**; `ModulesA1200_3.2.adf` is the disk that uses `LIBS`. Each rule uses its own medium's casing. Entry lookup is case-insensitive so this is cosmetic, but "say what the release says" is the project's rule and a recipe that misquotes its media is a trap with a fuse. The `68020…60.library` shorthand expanding to four named files was correct.

12. the ⚠️ (mmulibs' library names were an expansion of the brief's ellipsis, not a measurement) is resolved by measuring. The eight names are **correct**. But `Libs/` holds a **ninth entry the implementer missed — `mmu`, a directory**. Real gap; it enters the fix round. Cost if wrong: MuTools installed without their own data drawer.

13. Minors 7 and 8 are promoted into the fix round rather than deferred. `digest_of_folder` is the *instrument* Task 6's "the media is byte-for-byte unchanged" proof depends on, it hashes absolute paths so it cannot compare two trees, and it has no coverage — the implementer wrote an end-to-end fixture exercise, watched it pass, then deleted it. This project's own second lesson is that a screenshot is not a measurement and the instrument gets checked before the picture is believed (ART-099). Six untested helpers carrying Tasks 2–10's evidence is that lesson pointed at ourselves. Cost if wrong: a little test code nobody needed.

14. Minors 5, 6 and 9 fold into the Important 2 fix — they are the same code area (validate's coverage and the recipe's data assertions) and cost nothing extra once someone is there. Minor 5 in particular is a test that *cannot fail*, which the review rubric treats as a defect regardless of its label.

15. Minor 10 parked — `exclusive_group: "modules"` has one member because v1 targets the A1200 Kickstart the user's PiStorm A500 runs, which is the machine in the room. The group is inert now and load-bearing the moment a second Modules disk is added. Cost if wrong: nil; it is a correct mechanism with one user.

16. Important 4 is not a defect — it is correct under ruling 5 — but it is carried into Task 5's dispatch verbatim, so Task 5 does not inherit the impression that a green recipe says anything about file-level collisions. Every File-inside-another-component's-Subtree overwrite is unguarded until Task 5 builds that check.


## Task 2

17. the implementer's divergence from the brief's suggested `VolumeWriter::attributes` is **endorsed, not a finding**. `VolumeWriter::open` demands `FileRegionMut`, so following the brief literally would have required write access to open install media that nothing mutates — and the spec's own safety table says the media folder is opened read-only and never written. Using the read-only `FileRegion` plus the same free functions `VolumeWriter` is built on is the better answer and the brief was wrong. Cost if wrong: nil; it is strictly less privilege.

18. the second concern is a real latent defect and is fixed before review, not carried. `entry_at` on the root block reads `PROTECT_OFFSET`/`COMMENT_OFFSET`, which on a root block alias the bitmap-page table — bounds-safe but semantically garbage. The implementer judged it unreachable because "the recipe never exercises `entry("")`", but the recipe has `from: ""` for `fonts` and `backdrops`, and Task 5 resolves every rule's `from` against its media to decide `MediaPathMissing`. So `entry("")` is reached, and it would hand back protection bits invented out of a bitmap table — which `apply` could then write into a `.uaem`. This is lesson 3 of this project's own list, a field promising one thing and delivering another. Cost if wrong: a two-line guard nobody needed.

19. the Critical stands and I verified its premise in the code. `core/cbm/d64.rs`'s module doc states it as project policy in so many words — "Every chain has a step limit *and* a visited set… A step limit alone lets a cycle run to the limit… Both, or neither" — and `core/adf/fs.rs::walk_and_count_on` keeps a `HashSet` for this very walk over these very structures. `core/layout/scan.rs` gets away with depth-only because it walks a real filesystem and skips symlinks; that does not transfer to an arbitrary block graph. Enters the fix round. Cost if wrong: a HashSet per walk, which the codebase already pays everywhere else.

20. Important 2 is sharper than its label. `0x20` (`--p-rwed`) exercises the pure bit with every RWED granted; `0x42` (`-s--rw-d`) is the only fixture case where a **denied** bit has to survive the inversion, and RWED-inversion is the single easiest thing in this format to get backwards. Fix.

21. the ⚠️ about `docs/STATUS.md` and `FEATURES.md` is **not a finding — parked**. CLAUDE.md's "when work lands" means when the *work* lands, and this plan makes that Task 14 explicitly. Flipping a FEATURES row now would claim a feature that is two files into a fourteen-task build. Cost if wrong: nil; Task 14 owns it and the plan says so.

22. Minors fold into the fix round where they are one line each and touch code already being changed (the `>=` depth boundary, the pin covering `date`, the duplicated `fixture` helper, `walk`'s absent-vs-empty semantics made explicit). `open` taking `volumes.first()` without checking the layout is bare is promoted — `AdfSource::open` is public API and an HDF silently becoming partition 0 is the kind of quiet wrong answer this project files issues about. `read`'s formatted not-found sentence is parked: consistent with the codebase, and ART-060 already tracks Rust-side strings as a whole.

23. the total-entry cap gets its test. The re-reviewer found the route the implementer could not: `entries_in` walks one hash bucket's chain to `MAX_CHAIN_STEPS` (65,536) **inside a single directory**, so pointing one already-written file block's `NEXT_HASH_OFFSET` at itself yields thousands of duplicate non-directory entries with zero recursion — the visited set (keyed on directory blocks) and the depth cap are both untouched, and only the entry cap fires. That makes the cap the sole guard on that route rather than a belt-and-braces backstop, and the fixture is a single-field splice, smaller than the two already in the file. The implementer's "disproportionate fixture" reasoning was honest but mistaken about the mechanism. Cost if wrong: one small test.

24. **my previous ruling was wrong and the implementer proved it by running it.** The route the re-reviewer proposed — a self-referencing `NEXT_HASH_OFFSET` on a file block — does not reach `walk_dir`'s total-entry cap, because `dir::entries_in` never returns a `Vec` to `walk_dir` until its own walk finishes, so its pre-existing `MAX_CHAIN_STEPS` guard fires first with its own message. The implementer built the splice, ran it, captured the actual error text, and reported the contradiction instead of writing an assertion around whatever happened. That is the behaviour this project's first lesson asks for, applied against its coordinator. The original "disproportionate fixture" judgement was closer to right than my ruling was. Cost of the correction: nil — the splice test was kept, re-attributed to the guard it actually exercises, which is coverage of a pre-existing guard that had none.

25. the white-box test for the total-entry cap is accepted. Its real route (one block aliased across many never-repeated directories) is genuinely large to build black-box, the cap is reachable in principle rather than dead, and a test that calls `walk_dir` with `out` pre-seeded proves the guard fires. Cost if wrong: the cap is proven to work but not proven to be reachable by a real image — recorded here so nobody later reads it as fully exercised.


## Task 3

26. **the duplicate-volume-name refusal is right in kind and wrong in scope.** Hard-failing the whole folder scan means a user with a stray backup copy of one Locale disk cannot install anything at all, even when installing only English. Silently taking the first is worse — that installs unknown bytes — so the implementer's instinct was correct. The answer is the shape this project already uses everywhere: the scan **reports** the ambiguity as data, and the **refusal is per component, at plan time, by name**, like `MediaMissing` and `MediaPathMissing`. A duplicate of a disk nobody selected is not a reason to stop. Enters a fix round. Cost if wrong: a plan can be built while an unused duplicate sits in the folder, which is the outcome I want.

27. the second concern corrects *my* dispatch and is accepted with thanks — I asserted `AdfSource::open` windows rather than slurps; the implementer checked and found an 880 KB floppy ADF sits under the 1 MiB window cap, so it is a full read in practice. ~31 MB across 36 files, milliseconds. No action, but the report now says the true thing instead of repeating mine. Twice now an implementer has corrected me by measuring rather than deferring.

28. the misnamed symlink test gets **renamed, not backfilled**. Checked the precedent: `core/layout/scan.rs` and `core/collection.rs` both skip symlinks via `symlink_metadata` and neither has a direct test, because creating a symlink on Windows needs privileges CI does not have. Demanding a test here the rest of the codebase does not have would be inconsistent; a test whose *name* lies about its coverage is the real defect, and that is what gets fixed. Cost if wrong: symlink skipping stays covered by reading, as everywhere else in this codebase.

29. **a second flake, and it is not the documented one — carried to Task 14 to be filed, not swallowed.** The implementer saw `core::iso::extract_tree_does_not_follow_a_directory_that_points_back_at_the_root` fail on one of three full-suite runs and pass in isolation. That is *not* ART-059, which is `net/`'s test server. I checked the obvious cause and ruled it out: `core/iso`'s `tmp()` keys on PID **and** a nanosecond timestamp, so it is not a scratch-path collision. I could not diagnose it further and it is outside this task's code, so it goes to Task 14 as an `ART-NNN` to file rather than a line in a scratch file. Two different modules flaking in one session may be environmental (parallel execution on this machine) rather than two defects — worth saying in the issue rather than asserting either. Cost if wrong: an issue filed for noise, which is the cheap direction; this project's own rule is that a suite failing at random trains people to re-run until green.

30. both Importants are real and enter the fix round. The first is subtle and good: `MediaAmbiguous.paths` is `Vec<PathBuf>` where every sibling variant uses `String`, and serde's `PathBuf` impl **errors** on a non-UTF-8 path — so on Windows the refusal built to explain a problem could itself fail to cross the command boundary, blowing up instead of displaying. The second is the round-1 fix undone from the other side: the doc says only a directory-listing failure propagates, but `symlink_metadata(&path)?` propagates too, so one unreadable entry fails the whole scan — exactly the blunt outcome I removed. Cost if wrong: nil, both are strictly narrowing.

31. Minor 3 is promoted to Important. It answers the question I explicitly set the reviewer — whether the new enum can be misused — and the answer is yes: `if let MediaMatch::Found(..)` silently reads Ambiguous as Missing, which is the arbitrary-winner failure wearing a different hat. I considered `Result<Option<_>, Ambiguity>` instead and rejected it: Task 5 collects refusals rather than short-circuiting, so `?` would be wrong there too, and the implementer's reasoning for the enum stands. The proportionate protection is `#[must_use]`, a doc line naming `match` over `if let`, and a requirement in Task 5's own dispatch that its review will check. Recorded here because a type would have been better than a convention and this is a convention. Cost if wrong: Task 5 ignores an ambiguity and installs the wrong bytes — which is why its review gets told to look.

32. Minors 4 and 6 fold in — both are tests that cannot fail for the property they claim. The duplicate test's ordering assertion passes with unsorted directory order because the fixture names happen to sort the way they were created (delete `entries.sort()` and it stays green), and test 1 asserts `Found(_)` without checking *which* path, which is the entire point of a test about identifying media by content rather than filename. Same family as the two unfalsifiable tests Task 1's review caught. Minor 5 (an HDF at scan level) folds in at one line.

33. the reviewer is right that nothing establishes the `core::iso` flake's lineage. That is already carried to Task 14 to be filed as its own `ART-NNN`, with the honest statement that it is not ART-059 and was not diagnosed.

34. the residual sort coverage is **accepted as documented**, and the re-reviewer's suggested alternative is declined because it does not do what it appears to. Extracting the sort into a pure helper and unit-testing it would prove the helper sorts; it would not prove `find_media` calls it, which is the actual uncovered property — and on NTFS that difference is unobservable because the filesystem already enumerates alphabetically. A test that looks like coverage without being it is the exact defect class this session has spent three rounds removing, so adding one here to close a cosmetic gap would be a step backwards. The sort makes `MediaAmbiguous.paths` deterministic across filesystems; it stays, with the test's comment saying plainly that it cannot fail on this platform. Cost if wrong: on a filesystem that enumerates arbitrarily, a refusal could list its two paths in an unstable order — visible, harmless, and not something a wrong order could turn into wrong bytes.


## Task 4

35. the plan file itself is corrected rather than ruled around a third time. `tempfile::tempdir()` appeared 12 times in the Shared test fixtures section and tripped implementers on Tasks 1 and 4; I had ruled it at pre-flight and carried it in dispatches, but the source text kept saying the wrong thing. Ten tasks remain, so fixing the plan beats correcting it ten more times. Committed separately from any code. Cost if wrong: a plan edit mid-execution, which the ledger records.

36. the Critical is real and is the most user-specific finding of the session. Deleting `strip_cloanto_header` from `rom_facts` leaves all six tests green, because no fixture carries the 11-byte `AMIROMTYPE1` prefix. **The user has licensed Amiga Forever** (recorded in STATUS), so Cloanto-headered dumps are ordinary input here, not an edge case — and without the strip, ART reads bytes 12..16 eleven bytes early, `stated_version` returns `None`, and a perfectly good ROM is refused. That is ART-104's shape arriving at the user's machine instead of in CI. One `Vec` concat closes it. Cost if wrong: nil.

37. Important 2 enters the fix round. `an_unreadable_rom_is_a_core_error_not_a_panic` asserts only `is_err()` on a *missing* file, so it cannot tell "states no version" from "could not be opened" and would pass for any error from any cause. Same defect class this session has now found five times.

38. Important 3 is a carry-forward, not a fix here. The `RomUnknown` seam is asserted (`condition_holds(_, None)`) but nothing yet produces the `None`, so today a non-ROM aborts planning with a `CoreError` sentence rather than the typed refusal decision 2 requires. Task 5 owns that join; it goes into Task 5's dispatch verbatim and its review will check the mapping rather than assume it. Cost if wrong: the UI gets an English sentence where it expected a translatable value — ART-060 from a new direction.

39. the near-duplicate tests fold in (keep one), and the report's "ten tests" wording slip gets corrected. Neither is worth a round on its own; both are in code being touched anyway.


## Task 5

40. the `camelCase` serde attributes are **endorsed, not scope creep** — that is pre-flight ruling 7 arriving early, and earlier is better than in Task 12 where a rename would already have shipped.

41. the substituted collision mutation is accepted. The brief's suggested one ("consider only the first two items") was a genuine no-op against a design that groups by destination, and the implementer substituted a mutation that exercises the override-resolution logic and said why. That is the right response to a brief that is wrong about a detail.

42. **`exclusive_group` gets enforced here rather than parked again.** Task 1's review flagged it as inert (one member) and I parked it; Task 5 now reports it is not enforced at all. With one member it cannot be violated, so nothing is broken today — but a field named `exclusive_group` that nothing checks is a claim the code does not keep, and CLAUDE.md's rule is not to claim support that is not implemented and tested. `plan()` is where component selection is resolved, so it is where the check belongs, and it is cheap: one refusal variant, one check, one test over a synthetic recipe with two members. The alternative was deleting the field, which is churn — a second Modules disk (A500, A600) is exactly what v1's A1200-only scope defers, not what it rules out. Cost if wrong: one refusal variant nobody raises until the recipe grows.

43. the fixture's default ROM major of 47 is accepted with a note. It keeps tests that do not care about Modules simple, and both branches are covered explicitly elsewhere. Worth recording, though, that **the user's own machine runs Kickstart 3.1 rev 40.68**, so "Modules on" is the real-world path and "Modules off" is the fixture default — the review is being asked to confirm the on-path is not the thinner of the two.

44. the Important enters the fix round and it is the one that matters. Carried item 4's whole point is that a *file* landing inside another component's *subtree* must be caught, and the reviewer showed no test pins it: exclude the walk output from the collision input and every current test still passes. `plan_with_colliding_recipe` uses File→File, which `recipe.rs` already covers statically, and `a_declared_override_is_not_a_collision` proves only the no-refusal direction. This is the sixth unfalsifiable-test finding of the plan and the first one hiding inside a check I explicitly carried forward across three tasks. Cost if wrong: nil.

45. Minor 3 is promoted. A `File` rule whose `from` resolves to a directory is emitted `is_dir: true`, filtered out of `detect_collisions`, and silently escapes the check; a `Subtree` rule over a file gets `bytes: 0`. Unreachable from the shipped recipe — but recipes are *data* and the design's whole promise is that 3.9 and CaffeineOS arrive as new files rather than new code. Task 1's `validate()` cannot catch this because it needs real media, so `plan()` is the only place it can be caught. A recipe-authoring footgun with no validation, in the one module whose job is to surface recipe defects, is not a Minor.

46. Minor 2 is carried to Task 12, not fixed here — `find_media`'s `CoreError` for a missing or unreadable media folder is the most likely user mistake after a bad ROM, and it reaches the UI as an English sentence (ART-060). The command layer is where that boundary is drawn, and the reviewer agrees. Cost if wrong: one untranslatable sentence on the most common mistake, for a few more tasks.

47. Minors 1 and 4 fold in (assert `items.is_empty()` on the two untested refusal paths; replace the `expect` in the hot loop with `continue`, since `panic = "abort"` makes any `expect` in `core/` an application kill even when provably unreachable). The reviewer's on-path observation folds in too: the Modules-on test asserts only the `File` rule's destination, none of the three `Subtree` rules — and that is the path the user's own 40.68 machine will run.


## Task 6

48. both Importants enter the fix round. The seam concern I asked about is real and worse than I framed it: `apply` is called from exactly one place in the repository (its own tests), all three hand-built plans set `is_dir: false`, so **the directory branch is untested code — and it is the branch a real plan hits first on almost every component**, since every `workbench-base` rule is a Subtree. The second is the same weakness the implementer correctly fixed for `MediaRecord.sha256` and left one field over: `record.sha256.len() == 64` would pass an implementation that hashed the path, the sidecar text, or a placeholder. One ~15-line test routing `fixtures::planned_with` through the real `plan()` closes both.

49. the `apply`-ignores-`plan.refusals` Minor is **promoted**. `plan()` empties `items` and `media_paths` when refusals exist, so `apply` on a refused plan cheerfully creates the root and writes a `distribution.json` with empty `files` — a manifest asserting a complete tree that holds nothing. That is requirement 5's failure mode arriving through a different door, and a manifest that lies about what it describes is the exact dishonesty this design forbids elsewhere (§89, G8's three states). One guard, one test. Outside the brief's literal scope and inside its intent.

50. the mutation-evidence Minor is fixed rather than noted. Corrupting the ADF made `AdfSource::open` reject a 9-byte file, so the run died before the digest comparison was ever reached — the assertion was never shown to be load-bearing. That is the eighth instance in this plan of evidence not proving the claim attached to it, and the pattern is the reason to fix it rather than wave it through: a mutation check that passes for the wrong reason is worse than none, because it is recorded as proof.


## Task 7

51. adding `InstallPlan::user_startup` is **endorsed, not scope creep.** `apply()` consumes only the `InstallPlan` — a deliberate property, documented and carried since Task 5, and the alternative (passing a `Recipe` into `apply`) would break it. Putting the composed lines in the plan keeps "the plan previewed is the plan that runs" true for User-Startup too, which is the one file ART composes rather than copies. Cost if wrong: a field on the plan that only one consumer reads.

52. **the `core::iso` flake has now been seen twice in this session** (Task 3's run 2, Task 7's run 1 with two failures), always in the same module, never in code this plan touches. That is enough evidence to stop calling it noise. It stays carried to Task 14 to be filed as its own `ART-NNN` — distinct from ART-059, which is `net/` — with both sightings recorded and the honest note that it was not diagnosed. Cost if wrong: an issue filed for something environmental, which is the cheap direction; the expensive direction is a suite people re-run until green.

53. the spec gap is real and is the project's own fourth lesson arriving again. `merge_user_startup` takes the first `;BEGIN <id>` and the first `;END <id>` after it, so an unterminated block is left alone on run one and **swallowed on run two** — the stray opener, its content, and the user's own line between them all vanish. The reviewer proved it by merging the output of the implementer's own unterminated-block test. Requirement 3 exists to prevent exactly that outcome; it was met for one run. "A guarantee that holds is not the same as a goal that works" is written in STATUS as a lesson from G11, and this is the same shape.

54. the CRLF Minor is **promoted to Important**. `find_marker_line` requires `bytes[end] == b'\n'`, so a `;BEGIN amissl\r\n` line matches nothing and every re-run appends instead of replacing. This is not an exotic case: the tree lives on Windows and the module doc itself describes `User-Startup` as a file the user hand-edits. Notepad is the default tool and it writes CRLF. Requirement 4 breaks in the one situation the file is documented to be in.

55. the Latin-1 Minor is **promoted to Important**. `String::from_utf8` on a media-copied starter turns any ISO-8859-1 byte into `CoreError::Malformed`, failing the install at the very last step after every file has landed. Amiga text is Latin-1, this user is Turkish, and the recipe ships a `Locale-TR` component — `ç`, `ş`, `ğ`, `ı` are one hand-edit away. The failure is safe (nothing destroyed, no manifest written) but it is an install that dies at the finish line for a reason the user cannot act on. Operating on bytes avoids it entirely.

56. both remaining Importants (no exact-equality test on the replace path; recipe order claimed everywhere and tested nowhere) enter the round as found. The order one matters beyond tidiness: these are `Assign` lines, and order changes behaviour on the Amiga.


## Task 8

57. **the spec's journalling claim is wrong and is corrected, not worked around.** `Journalled::begin(device, image, offset, description, blocks: &[u32])` requires the block set **up front** — it saves those blocks, fsyncs, and returns a guard that may write exactly those. That fits Stage W's `BlockSet` design, where the writer plans its blocks first. libpfs3 decides as it goes and cannot supply the list. So "every PFS3 block write goes into core/volume/journal.rs" was never achievable as written, and the design section arguing the adapter is "not optional" because of it argued from a false premise.

58. **the real safety property is bounded writes, and it is already built.** `FileRegionMut::open(path, offset, length, block_size)` refuses a region running past the end of the file rather than clamping — its own doc explains why — so nothing outside the target partition can be touched: not the other partitions, not the RDB, not the FAT32 boot partition. That is the property worth claiming and testing, and it is the same guarantee `core/fat32.rs::Region` gives the boot partition.

59. **G5's operation is format-then-fill, so the partition's prior contents are forfeit by the user's own confirmed choice.** `core/preload::plan` always emits `FormatPartition` before `CopyIn`, and the screen already names formatting as destructive. Journalling blocks the user has agreed to erase protects nothing. An interrupted run leaves an **incomplete** volume, which is reformatted and redone — not a corrupted one silently trusted. That is worth saying on screen and in the report, not hiding.

60. **writing into an existing populated PFS3 volume without formatting is out of scope and must be refused by name.** That is the one case where crash safety would genuinely matter and where ART has none, so ART does not offer it. Refusing rather than half-supporting is this project's standing pattern.

61. the `'static` blocker dissolves with the above. `FileRegionMut` owns its `File` and carries no lifetime, so `ArtBlockDevice` can **own** its device and satisfy `Box<dyn BlockDevice + 'static>`. The lifetime problem and the journal problem had one root and have one fix.

62. `Cargo.lock` being gitignored in this repository is noted and left alone. It is the repo's existing choice, unrelated to this plan, and changing it mid-execution would be scope I was not given. Worth raising with the user separately — an application usually commits its lockfile, and `cargo deny`'s value depends on it.

63. **the core::iso flake has now been seen three times** (Task 3, Task 7, Task 8), always the same module, never in code this plan touches. Three sightings across three separate agents on three different days' worth of runs is not noise. It is filed at Task 14 as its own ART-NNN, distinct from ART-059 (which is net/), with all three sightings and the honest note that it was never diagnosed. If it appears again before Task 14, it is worth stopping to diagnose rather than counting.

64. the Important is real and lands in the place the review brief singled out. `flush_reaches_the_underlying_devices_sync` runs against `VecDevice`, whose `sync` is unconditionally `Ok(())`, and asserts only `is_ok()` — an adapter flush that never touches the device passes identically, and the test's own comment claims the opposite. Tenth instance of the pattern in this plan. The *design* decision (mapping flush to `BlockDeviceMut::sync` rather than no-oping) is right and stays; only the test is empty.

65. the "G5 detects" Minor is fixed rather than noted, and it matters more than its label. The doc says an incomplete volume is one "G5 detects and reformats and redoes from scratch" — no such detection exists. I mandated the reformat-and-redo framing; "detects" was added on top. That is the same over-claim disease I corrected in the spec one commit earlier, reappearing one level down, which is exactly how it spreads. Either the word goes or it points at where the detection will live.

66. the three remaining Minors fold in — they are one line each and in code being touched: assert both sides in the bulk bounded test (a `write_blocks` returning Err before writing anything currently passes it), check `data.len()` against `count * block_size`, and extend the u32 refusal assertion to `write_block`.


## Task 9

67. **`import_filesystem` refusing is correct, and my spec was wrong again.** I checked: `create_rdb_layout(total_bytes, partitions, file_systems)` builds an RDB from scratch on hardcoded 16-head/63-sector LBA geometry and does not edit an existing one. Real cards disagree — CaffeineOS's RDB is 12 heads, 256 sectors — so writing a fresh RDB over an existing area would invalidate every partition in it. The spec's "ART already does this — G4's create_rdb_layout" was the second false delegation claim in this design, after the journal. Spec corrected and committed.

68. **the gap is narrower than it looks and the boundary is now written down.** A card ART built already carries its drivers (build.rs lays an RDB per area, G4 embeds FSHD/LSEG at build time); an FFS partition needs no driver at all because Kickstart carries FFS — which is why the user's screen-test.img has a correct two-step plan with no embed step. What is genuinely unsupported is embedding a driver into a *foreign* card's existing RDB, and `hst-imager` is named in the refusal. Filed as future work rather than implied. Cost if wrong: a PFS3 partition on a card ART did not build still needs the external tool for one step, said out loud.

69. the hand-maintained `LIBPFS3_VERSION` constant needs a drift guard, not just a comment. A version string that silently disagrees with `Cargo.toml` puts a false claim in a user-facing report, which is the same class as everything else corrected today. A test that parses the pin out of `Cargo.toml` and compares is cheap.

70. the fit-check Important is the one that matters and enters the round. `needed` sums raw file sizes while `free_bytes` counts whole blocks, so real consumption is always larger — per-file block rounding plus directory, anode and header overhead. A tree at 95% of free space therefore passes the check and then dies inside libpfs3 partway through, **having already written**. That is precisely the outcome requirement 5 exists to prevent ("asked before the first byte"), and the code comment calls the arithmetic "conservative" when it is anti-conservative. A guard that fails open while documenting itself as failing closed is worse than no guard.

71. the `LIBPFS3_VERSION` Important is upheld and its justification is factually wrong, which I checked. `libpfs3 = "0.1.3"` is a caret range; `ureq` is `=3.2.1`. So the constant's doc claiming "the same trade-off ART already accepts for ureq's exact pin" is false — ureq cannot drift and this can, on any `cargo update`, into a user-facing report. Pin it and add the drift test.

72. both remaining Importants enter as found. Directory `.uaem` sidecars are silently dropped (neither applied nor copied) — harmless today because requirement 1's load-bearing case is a file, but silent loss is the thing this project files issues about. And requirement 1 has no FFS counterpart at all: `copy_in_ffs` uses a different mechanism (`FileMeta` rather than `update_dir_entry_protection`), so an FFS branch that dropped metadata entirely would pass every test in the diff.

73. the journal Minor is documented, not fixed. `format_ffs_volume` writing boot/root/bitmap raw is defensible for the same reason PFS3's lack of journalling is — a format's contents are forfeit by the user's confirmed choice — but the module doc explains that for PFS3 and says nothing here, which reads as an oversight rather than a decision. Cost if wrong: an IO failure mid-format leaves a volume whose signature satisfies ART-049's check over a garbage root, and nobody wrote down that this was considered.


## Task 10

74. extending `FileRecord` with `protection: Option<u32>` is **endorsed** — it closes a real gap in my plan. Task 6's manifest recorded path, component, media, sha256 and bytes but no protection, so the brief's "compare size and protection" had nothing to compare against. serde-defaulted `Option` keeps older manifests readable. Cost if wrong: one nullable field in a format nothing has shipped yet.

75. the implementer found the brief's own fourth test cannot fail for its property — the fixture is forced all-`Pass` by test 1, so no `NotChecked` entry ever exists for a mutation to bite — and added a genuinely-NotChecked PFS3 case that does catch it. Twelfth instance in this plan, and the third inside text I wrote.

76. `core/preload/mod.rs`'s module doc is now **false**, not merely stale — it says "ART has no PFS3 reader… not one file checked", which stopped being true when libpfs3 landed in Task 8. The implementer flagged it honestly and left it. It gets fixed in the fix round rather than deferred: after a session spent removing claims the code cannot keep, leaving one that the code has outgrown would be the same failure with the sign flipped. Cost if wrong: a doc edit.

77. the read-only finding is real and reaches real volumes. `verify_ffs_files` opens `FileRegionMut` + `VolumeWriter::open`, which runs `write_refusal` — so a dircache volume (`DOS\4..\7`, all routed to Ffs by `family_of`) or a 1024-byte-block partition makes `verify_volume` return `Err` and produce **no report at all**, for files whose presence and size it could have read. STATUS already lists dircache volumes as deliberately read-only in ART, so this is not hypothetical. It is precisely G8's unverifiable case, which must be `NotChecked` with a reason rather than a failed run.

78. the overclaiming-detail finding is the one worth naming. `PFS3_CONTENT_NOT_CHECKED` says "presence, size and protection matched" **unconditionally** — including when the manifest carries no protection and when the expected value will not fit libpfs3's `u8`. In that last case protection was genuinely not checked, and the sentence claims it matched. A module whose entire purpose is keeping "did not look" apart from "fine" must not do that in its own explanatory text.

79. three claimed `Fail` paths having no test enters as found. The module doc asserts PFS3 reaches `Fail` on a size or protection disagreement and that an unknown DosType yields `NotChecked`; only PFS3 *presence* is pinned. Short-circuiting either comparison, or turning the `Other` arm into a pass, leaves the suite green. Thirteenth instance of the pattern.

80. the stale `core/preload/mod.rs` doc goes into this same round, as already ruled.

81. the residual is not a nice-to-have — it is **Task 2's decision, unlearned one task later**. Task 2's implementer found that `VolumeWriter::open` demands `FileRegionMut` even where nothing mutates, used the read-only `FileRegion` plus the free functions instead, and I endorsed that explicitly. Task 10 reaches for the write handle again. ART's FFS reading path is generic over `BlockDevice`, so a *verifier* needs no write access at all, and taking one means a write-protected image cannot be verified — an image on read-only media fails with a permission error instead of reporting what it could read. Consistency inside one feature matters more than the size of the diff. Cost if wrong: a small round for a privilege the operation never needed.


## Task 11

82. the implementer's note that **ART's PFS3 writer carries protection but not the `.uaem` comment or date** is recorded for Task 14 to file. `libpfs3` 0.1.3 exposes no setter for either (Task 9 established the date half). Protection is the load-bearing field — it is what makes `Resident C:Assign PURE` work — so nothing is broken, but a `.uaem` sidecar carrying a comment has it silently dropped on the PFS3 path while the FFS path keeps it. Silent divergence between two branches of one operation is worth an issue, not a code comment. Cost if wrong: an issue filed for cosmetic metadata.

83. Important 1 enters the round even though the reviewer notes it is not a compliance gap. Direction 1 compares name, kind, size and attributes but never **contents**, because `fs dir` yields no bytes — so ART-079's shape is guarded when ART *reads* and not when ART *writes*. For an OS install the writer is the side being trusted: wrong bytes at the right length is a card that does not boot. The script already drives `fs copy`; extracting the volume out and hashing closes it, and the docstring's "proves ART's PFS3 writer" overstates what direction 1 proves until it does.

84. Minor 5 is **promoted**. `parent = SCRATCH if SCRATCH.is_dir() else None` falls back silently to the system temp directory on `C:`, and then writes a 220 MB image there. The user said plainly on 2026-08-14 that C: and D: are not used, and it is in this project's memory as a standing rule. A documented silent fallback is still a silent fallback.

85. Importants 2 and 3 enter as found — direction 2 has no "nothing extra" check, and `parse_dir_listing` skipping rows it does not understand means an unexpected entry in a malformed row escapes the one assertion that depends on the parse being complete.

86. the ⚠️ about the 1203 -> 1357 snapshot correction is noted, not actioned. The implementer did run the suite and measure 1357, so the number is not invented; the drift it corrects predates this task, and STATUS is Task 14's job. Carried there.


## Task 12

87. Critical 1 is a live defect that would have shipped. `VerifyReport` carries no `rename_all`, so it serializes `not_checked` while `src/lib/osinstall.ts` reads `notChecked` — `undefined` on every report, `isVerified` false always, and any screen showing the third count rendering `undefined`. It lands in exactly the field §89 exists for.

88. Critical 2 confirms the hypothesis I put to the reviewer, and the lesson is worth keeping. The wire test **was** the mechanism designed to prevent Critical 1, and it covers only the inbound half — so the defect landed in the uncovered half. The mutation evidence (14 build errors from a Rust field rename) demonstrates the compiler, not the test: a rename cannot compile, while the failures that matter (`#[serde(rename)]`, dropping `rename_all`, a type change serde still accepts) all compile fine. One of the two tests is near-tautological — it serializes and deserializes the same Rust types, so any `rename_all` change moves both sides together and it cannot fail. Fifteenth instance of the pattern, and the costliest, because this one had a real bug behind it.

89. Critical 3 stands. `a_report_with_nothing_checked_is_not_verified` never calls `osinstall_verify` and its final assertion is entailed by the two above it; `verified(report.failed == 0)` passes the whole suite. It tests `verify_volume`, which Task 10 already covered.

90. the i18n gap is a **CLAUDE.md violation, not scope creep to wave through**. The new TS emits ~11 `Phrase` keys under `osinstall.*` and neither `en.json` nor `tr.json` holds a single one, and the mappers were never added to `src/i18n/phrase-keys.test.ts` — the guard that exists precisely because nothing else catches a `Phrase` pointing at a key nobody added. `pnpm test` passes only because the test does not know the mappers exist. Both catalogues, same commit, plus the mappers registered in the guard.


## Task 13

91. Critical 1 stands. `withExclusions` leaves `mediaPaths` stale, so `apply` records an excluded component's medium in `distribution.json`'s `built_from` — with its SHA-256 — while not one of its bytes was installed. That manifest is by its own doc "the only record, because the media itself is gone by then", and clean removal reads it back. It also still opens the excluded image, so a disk that moved between plan and build fails a job over a component the user turned off.

92. Critical 2 is the finding of the session. **The feature does not work in the exact case it exists for.** A pre-V47 ROM with no `ModulesA1200_3.2` disk is precisely the user the confirmation is written for; that plan carries a `media-missing` refusal, `componentsOn` still holds the id so the tick and the confirmation arm, the user confirms "it will not boot" — and `osinstallBlocker` still returns `blocked.refusals`. The exclusion is inert. Same shape as G11's stale flag and as Task 7's guard that held exactly once.

93. **the seam moves into the engine, and this leaves the brief's scope deliberately.** `InstallRequest` gains `excluded: Vec<String>`, subtracted inside `resolve_components_on`, after which items, `componentsOn`, `mediaPaths`, collisions, refusals and `total_bytes` are consistent by construction and the UI stops owning engine semantics. My brief was wrong to imply this was three frontend files; the implementer half-discovered that and worked around it client-side instead of reporting it, which is the one place it should have escalated. Cost if wrong: Task 5's module changes again, late.

94. the hand-typed mirror gets a **parity test now, not a fifth command**. Vitest runs in node and `amigaos-3.2.json` is in the repo, so exporting the list from `src/lib/osinstall.ts` and asserting it against the parsed recipe turns silent two-way drift into a red build in the same commit that causes it. A command exposing the recipe stays the right long-term answer and stops being urgent.

95. the pure logic (`withExclusions`, `isForcedOnByCondition`, the toggle state machine) moves to `src/lib/osinstall.ts` with tests beside it. That is the project's own convention, and the reviewer notes it is what would have caught Critical 1.

96. the browser pass is **accepted as partial, and recorded as a rung rather than a pass.** Confirmed by direct observation: navigation to `#/os-builder`, the new "Install AmigaOS" kind, and its top-level headings resolve with no raw keys and no `{{...}}`. Deeper interaction — form fills, checkbox toggles, a Verify run, the Turkish switch — reproducibly crashed the renderer with an access violation across both browsers and both headless modes; not resolved, not worked around, probe not committed. That is precisely the shape I asked for: a bounded attempt and a precisely stated gap beats an afternoon of browser flags. **Still unseen and going to Task 14's documentation: the component checklist with its reasons rendered, the confirmation panel, Turkish in tight controls (ART-062 names Turkish strings as substantially longer), and `found_one/_other` pluralisation actually selecting in `tr`.**


## Task 14

97. **ART-113 verified independently by me, and it is the most consequential finding of the plan.** `libpfs3` 0.1.3 writes a name with `name.as_bytes()` — UTF-8, by definition of a Rust String — and reads it with `latin1_to_string`, which maps each byte to one char. For any non-ASCII name the two disagree: `Türkçe` goes out as 8 UTF-8 bytes and comes back as `TÃ¼rkÃ§e`, and since the Amiga also reads Latin-1, it is mojibake on the real machine too. ART cannot work around it through the public API — a Rust `String` cannot carry pre-encoded Latin-1 bytes. **This user is Turkish and `Locale-TR` is in the shipped recipe**, so it is not hypothetical for them. The product answer is a refusal by name on the PFS3 path, not the silent 24-entry exclusion the run used to proceed; that exclusion was right for finishing the run and is disclosed, but it is not what ships. Filed open; upstream is the real fix.

98. **ART-112 is partly my fault and the ledger should say so.** Task 1's implementer originally added `classes` to `glowicons.overrides`; I made it revert that under my "a Subtree destination is a merge point" ruling. The ruling was right about drawers and wrong about what sits inside them — the real `Devs/DataTypes/*.info` entries are *files*, and files do collide. The rule needed the exception the run has now supplied.

99. fixing ART-111 and ART-112 rather than only filing them is accepted. The run could not proceed otherwise, both are data-only recipe corrections reproducible through the new test hooks, and the incompleteness of the `storage` fix (Printers/LIBS/Classes.DataTypes unaddressed, collision risk unmeasured) is disclosed rather than glossed.

100. Important 1 is the finding, and it is the failure mode this task existed to prevent leaking through in one place. "24 entries excluded" understates the exclusion by roughly forty times: the full tree is 4030 files / 330 directories and the witnessed volume is 3061 / 224, because the 24 *named* exclusions are directories whose whole subtrees went with them — about 969 files and 106 directories. An attentive reader can subtract, but no sentence does it for them, and CHANGELOG's "does not yet succeed for those files" reads as a handful. Roughly a quarter of the tree is absent from the volume that was independently witnessed, and that must be said in ART-113, the FEATURES row and the session log.

101. Important 2 stands against STATUS's own rule. The 3059/3061 comparison — Step 2's strongest evidence — came from an uncommitted scratchpad script and an ad hoc `fs copy` + SHA-256 run, unlike Task 11, which committed its oracle for exactly this reason. Numbers that were run but cannot be re-run are the thing that rule forbids.

102. Important 3 is small and non-negotiable. The new card hook does `let _ = std::fs::remove_file(&image)` on whatever `ART_CARD_OUT` names, with no existence refusal — inverting this project's SAFE_CREATE convention, which STATUS documents `build_card` as honouring a few hundred lines above. Env-gated and `#[ignore]`d, so the blast radius is small; the precedent is not.

103. Important 4 — a reproduce block whose whole purpose is proving a claim was run should itself be runnable. `^` is not a continuation inside a bash fence, `VAR="x" cmd` is not a cmd assignment, and the comment calls two hooks `#[ignore]`d when they are not.

