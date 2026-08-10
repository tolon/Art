# Contributing to Amiga Retro Toolkit

Thank you for your interest in ART. This document covers the basics.

## Development environment

See [README.md](README.md) for setup instructions (Rust MSVC toolchain,
pnpm, MSVC Build Tools).

## Workflow

1. Read [docs/STATUS.md](docs/STATUS.md) to see where the project is and what
   the current stage covers.
2. Pick an issue from [docs/ISSUES.md](docs/ISSUES.md), or open one to discuss
   a change.
3. Branch from `master`.
4. Follow the stage plan — **do not implement future-stage features** until the
   current one is stable. [docs/roadmap.md](docs/roadmap.md) defines what each
   phase contains; STATUS.md defines the order.
5. Keep the build green:

   ```bash
   pnpm lint          # TypeScript type-check
   cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
   pnpm tauri build   # full build
   ```

6. Every feature must have tests. See [docs/testing.md](docs/testing.md).
7. Update the tracking docs: a session-log line in
   [docs/STATUS.md](docs/STATUS.md), a flipped row in
   [docs/FEATURES.md](docs/FEATURES.md) if you finished a feature, and an
   `ART-NNN` entry in [docs/ISSUES.md](docs/ISSUES.md) if you found or fixed a
   defect.
8. This repository has no GitHub remote — there is nowhere to open a hosted pull request.
   Merge your branch back to `master` locally (or hand it off for review) with a clear
   description of what changed and why.

## Architecture rules

- **No business logic in React components.** All Amiga-format handling lives
  in the Rust core (`src-tauri/src/core/`).
- **The core is platform-independent.** Never import `tauri` or Windows APIs
  inside `core/`.
- **Never modify user data silently.** Follow the safety pipeline:
  `SOURCE → ANALYZE → VALIDATE → PREVIEW → BACKUP → APPLY → VERIFY → REPORT`.
- **Never distribute copyrighted ROMs or commercial software.**

## Adding a translation

ART ships English (`src/i18n/en.json`) and Turkish (`src/i18n/tr.json`).
Adding or changing a UI string:

- Add the key to **both** files in the same commit. `pnpm test` runs a parity
  check that fails the build if the two catalogues' key sets differ, a value
  is empty, or an interpolation variable (`{{n}}`, `{{code}}`, …) present in
  one is missing from the other.
- `src/lib` helpers that build a message return a `Phrase { key, params? }`,
  not a rendered sentence — that keeps the helper itself free of the i18n
  singleton, and lets the calling component decide when to call `t()`.
- Rust-side strings (`CoreError` messages, `WhdloadRefusal.reason` /
  `.suggestion`) are not part of this system yet and stay English regardless
  of the chosen language — see [ISSUES.md](docs/ISSUES.md) (ART-060).

## Commit messages

Use clear, imperative-mood commit messages (e.g. "Add ADF bootblock validation").
Reference issues when relevant.

## Licensing

By contributing, you agree your contributions are licensed under the project's
[MIT license](LICENSE).
