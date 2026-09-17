# Roadmap

**What happens next, and in what order, lives in [STATUS.md § Stage plan](STATUS.md#stage-plan)** —
the SD-0 … SD-5 stages built from [sd-appliance-gap-analysis.md](sd-appliance-gap-analysis.md)
and the PiStorm image-builder work. This file carries no phase list of its own.

The Phase 0–7 plan of 2026-08-09 (a dependency-ordered gap analysis against the
master spec) was overtaken by that stage plan and has been removed from the tree;
git history keeps it. Do not resurrect its phase numbering.

- Current position and stage ordering → [STATUS.md](STATUS.md)
- Whether a specific feature exists → [FEATURES.md](FEATURES.md)
- What a module is meant to do → the master spec, which is canonical for product behaviour

**Never proceed with a broken build.** Do not implement future-stage features
until the current stage is stable.

## Phase completion criteria

Each stage must satisfy:

- Build: PASS
- Tests: PASS
- No critical errors
- No obvious data-loss risk
- UI remains responsive
- Documentation updated
- CHANGELOG updated
