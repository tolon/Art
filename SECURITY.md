# Security Policy

## Reporting a vulnerability

If you discover a security vulnerability in ART, please report it
**responsibly**:

1. **Do not** open a public GitHub issue.
2. Email the maintainers directly (see the project's contact info).
3. Include a clear description and, if possible, a reproduction.

We will acknowledge receipt within 72 hours and aim to issue a fix within
a reasonable timeframe, crediting you unless you prefer otherwise.

## Threat model

ART treats **all external files as untrusted input**. Disk images, archives,
and ROMs may be malformed or hostile. The core engine must never:

- execute arbitrary code from a file,
- follow path-traversal entries during archive extraction
  (e.g. `../../Windows/System32/...`),
- allocate unbounded memory in response to malformed headers,
- overwrite files outside the user's chosen destination,
- write to raw devices without explicit, double confirmation,
- launch external processes with unvalidated arguments.

See [docs/security-model.md](docs/security-model.md) for the full model,
including the destructive-operation classification
(READ ONLY / SAFE / REQUIRES BACKUP / DESTRUCTIVE / EXPERIMENTAL).

## Scope

This policy covers the ART application source. It does not cover
third-party tools (WinUAE, LHA, FlashFloppy utilities) that ART may invoke —
those have their own security policies.
