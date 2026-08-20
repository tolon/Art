// The wire between `commands/amigainstall.rs` and this module.
//
// A TypeScript type is erased at run time, so nothing in the build notices
// when a Rust enum gains a variant and the union here does not — the screen
// would then fall through every branch it knows and say nothing at all about
// an ending that really happened. This reads both sources as text and checks
// the two lists against each other, the same way `osinstall.test.ts` checks
// the screen's assumptions against every shipped recipe.
//
// It deliberately does **not** re-derive the JSON shape: the Rust side pins
// that exactly (`every_run_outcome_has_the_shape_the_frontend_reads`,
// `settlement_is_camel_case_on_the_wire_including_inside_a_variant`). What
// only this side can see is whether the frontend knows about all of it.

import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { AMIGA_INSTALL_EVENT } from "@/lib/amigainstall";

const SRC = resolve(__dirname, "..", "..", "src-tauri", "src");
const CORE = readFileSync(resolve(SRC, "core", "amigainstall", "mod.rs"), "utf8");
const COMMAND = readFileSync(resolve(SRC, "commands", "amigainstall.rs"), "utf8");
const WRAPPER = readFileSync(resolve(__dirname, "amigainstall.ts"), "utf8");

/** The body of a `pub enum <name> { … }`, up to its closing brace at column 0. */
function enumBody(source: string, name: string): string {
  const start = source.indexOf(`pub enum ${name} {`);
  expect(start, `${name} must exist in the Rust`).toBeGreaterThan(-1);
  const end = source.indexOf("\n}", start);
  return source.slice(start, end);
}

/** Variant identifiers, ignoring doc comments and attributes. */
function variants(body: string): string[] {
  return body
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => /^[A-Z][A-Za-z0-9]*\s*(\{|,|$)/.test(line))
    .map((line) => line.replace(/[^A-Za-z0-9].*$/, ""));
}

/** `EmulatorClosed` → `emulator-closed`, which is what `rename_all` does. */
function kebab(variant: string): string {
  return variant
    .replace(/([a-z0-9])([A-Z])/g, "$1-$2")
    .toLowerCase();
}

/** Every `kind: "…"` the TypeScript union declares, between two markers. */
function kindsBetween(from: string, to: string): string[] {
  const start = WRAPPER.indexOf(from);
  const end = WRAPPER.indexOf(to);
  expect(start, `${from} must exist`).toBeGreaterThan(-1);
  expect(end).toBeGreaterThan(start);
  return [...WRAPPER.slice(start, end).matchAll(/kind: "([a-z-]+)"/g)].map((m) => m[1]);
}

describe("the four endings reach the frontend", () => {
  it("declares every RunOutcome the Rust has, under the same tag", () => {
    const rust = variants(enumBody(CORE, "RunOutcome")).map(kebab);
    const ts = kindsBetween("export type RunOutcome", "export type SettlementReport");

    expect(rust).toEqual(["succeeded", "failed", "timed-out", "emulator-closed"]);
    expect([...ts].sort()).toEqual([...rust].sort());
  });

  it("declares every SettlementReport the Rust has", () => {
    const rust = variants(enumBody(COMMAND, "SettlementReport")).map(kebab);
    const ts = kindsBetween("export type SettlementReport", "export interface AmigaInstallRequest");

    expect(rust).toEqual(["promoted", "kept"]);
    expect([...ts].sort()).toEqual([...rust].sort());
  });

  it("names the same event as the Rust", () => {
    const declared = COMMAND.match(/AMIGA_INSTALL_EVENT: &str = "([^"]+)"/);
    expect(declared?.[1]).toBe(AMIGA_INSTALL_EVENT);
  });

  it("asks for the package's own archive, which ART-185 made required", () => {
    // The Rust field is not optional — no `#[serde(default)]`, no
    // `Option` — so a request without it fails to deserialise and the
    // command rejects before anything runs. TypeScript is the only place
    // that can say so *before* the call, and only if the field is declared
    // required here too. Without the archive the installer is on no mounted
    // volume and the run reports that it said no about a program that never
    // started, which is the whole of ART-185.
    expect(COMMAND).toMatch(/pub package_archive: PathBuf,/);
    expect(COMMAND).not.toMatch(/#\[serde\(default\)\]\s*\r?\n\s*pub package_archive/);
    expect(WRAPPER).toContain("packageArchive: string;");
    expect(WRAPPER).not.toContain("packageArchive?");

    // And the third volume the run mounts is named on both sides, because
    // the user will see it on the Workbench (design §4).
    const declared = CORE.match(/PACKAGE_VOLUME: &str = "([^"]+)"/);
    expect(declared?.[1]).toBeTruthy();
    expect(WRAPPER).toContain("packageVolume: string;");
  });

  it("keeps the fields a struct variant carries, which serde does not rename for free", () => {
    // `#[serde(rename_all)]` on an enum renames variants, not the fields of a
    // struct variant, so `leftBehind` only arrives camelCased because the
    // variant carries its own attribute. If that attribute is ever dropped
    // the Rust test fails; if the field is dropped here, the screen silently
    // stops telling the user about a tree it could not remove.
    expect(COMMAND).toMatch(/#\[serde\(rename_all = "camelCase"\)\]\s*\r?\n\s*Promoted \{/);
    expect(WRAPPER).toContain("leftBehind: string | null");
  });
});
