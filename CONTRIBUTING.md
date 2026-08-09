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
3. Branch from `main`.
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
8. Open a pull request with a clear description.

## Architecture rules

- **No business logic in React components.** All Amiga-format handling lives
  in the Rust core (`src-tauri/src/core/`).
- **The core is platform-independent.** Never import `tauri` or Windows APIs
  inside `core/`.
- **Never modify user data silently.** Follow the safety pipeline:
  `SOURCE → ANALYZE → VALIDATE → PREVIEW → BACKUP → APPLY → VERIFY → REPORT`.
- **Never distribute copyrighted ROMs or commercial software.**

## Commit messages

Use clear, imperative-mood commit messages (e.g. "Add ADF bootblock validation").
Reference issues when relevant.

## Licensing

By contributing, you agree your contributions are licensed under the project's
[MIT license](LICENSE).
