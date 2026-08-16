# Roadmap

**The phase-by-phase build order lives in
[docs/superpowers/specs/2026-08-09-art-roadmap-design.md](superpowers/specs/2026-08-09-art-roadmap-design.md).**
That document is a dependency-ordered gap analysis against all 96 sections of
the master spec — Phase 0 (ground repair) through Phase 7 (consolidation),
plus the §45.5 AI layer as a separate project. It supersedes the phase list
this file used to carry: the phase numbers and contents do not match — this
file's old "Phase 1 — ADF + LHA" and the spec's "Phase 0 — Ground repair" are
not the same plan, and only the spec is current. Do not resurrect the old
list; treat the spec document as the single source for what each phase
contains.

**That document is not the current scheduling order.** [STATUS.md](STATUS.md)'s
own "Stage plan" section (the SD-0 … SD-5 stages built from
[sd-appliance-gap-analysis.md](sd-appliance-gap-analysis.md), the PiStorm
image-builder work) supersedes the Phase 0–7 numbering above for *scheduling*
— STATUS.md says so explicitly. The two do not compete: the spec above still
defines what a phase *contains*, STATUS.md's stage plan defines what order the
remaining work actually happens in and why. Read STATUS.md's Stage plan
section for "what's next", not the Phase list here.

- Current position and stage ordering → [STATUS.md](STATUS.md)
- Whether a specific feature exists → [FEATURES.md](FEATURES.md)

**Never proceed with a broken build.** Do not implement future-phase features
until the current phase is stable.

## Phase completion criteria

Each phase must satisfy:

- Build: PASS
- Tests: PASS
- No critical errors
- No obvious data-loss risk
- UI remains responsive
- Documentation updated
- CHANGELOG updated
