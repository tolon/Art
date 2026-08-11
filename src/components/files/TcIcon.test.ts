// Covers `classifyEntry` — the single judgement `iconKindFor` and
// `fileTextColorVar` both derive from (see `TcIcon.tsx`'s comment) — the
// same way `panelName.test.ts` covers `splitName` on its own.
import { describe, expect, it } from "vitest";

import { classifyEntry, fileTextColorVar, iconKindFor } from "./TcIcon";

describe("classifyEntry", () => {
  it("is always 'dir' for a directory, regardless of its name", () => {
    expect(classifyEntry({ name: "Amiga PiStrom", is_dir: true })).toBe("dir");
    expect(classifyEntry({ name: "Desktop.ini", is_dir: true })).toBe("dir");
    expect(classifyEntry({ name: ".git", is_dir: true })).toBe("dir");
    expect(classifyEntry({ name: "archive.zip", is_dir: true })).toBe("dir");
  });

  it("classifies a file with no extension as plain", () => {
    expect(classifyEntry({ name: "README", is_dir: false })).toBe("plain");
  });

  it("classifies by the last extension when there are several dots", () => {
    expect(classifyEntry({ name: "notes.backup.txt", is_dir: false })).toBe("text");
    expect(classifyEntry({ name: "archive.tar.gz", is_dir: false })).toBe("plain");
  });

  it("matches a text extension case-insensitively", () => {
    expect(classifyEntry({ name: "README.TXT", is_dir: false })).toBe("text");
    expect(classifyEntry({ name: "Notes.Md", is_dir: false })).toBe("text");
  });

  it("classifies the reference's own examples", () => {
    expect(classifyEntry({ name: "AmigaForever-DVD.iso", is_dir: false })).toBe("plain");
    expect(classifyEntry({ name: "ReadMe.txt", is_dir: false })).toBe("text");
    expect(classifyEntry({ name: "Desktop.ini", is_dir: false })).toBe("hidden");
  });

  it("treats a dotfile as hidden even with no other dot", () => {
    expect(classifyEntry({ name: ".gitignore", is_dir: false })).toBe("hidden");
  });

  it("classifies an ART-openable archive extension", () => {
    for (const name of ["game.lha", "GAME.LHA", "pack.zip", "disk.adf", "hd.hdf", "data.7z"]) {
      expect(classifyEntry({ name, is_dir: false })).toBe("archive");
    }
  });

  it("hidden-by-name wins over an archive/text extension", () => {
    expect(classifyEntry({ name: ".backup.zip", is_dir: false })).toBe("hidden");
  });
});

describe("iconKindFor", () => {
  it("maps each classification to its icon", () => {
    expect(iconKindFor({ name: "Tools", is_dir: true })).toBe("folder");
    expect(iconKindFor({ name: "game.lha", is_dir: false })).toBe("archive");
    expect(iconKindFor({ name: "Desktop.ini", is_dir: false })).toBe("hidden");
    expect(iconKindFor({ name: "ReadMe.txt", is_dir: false })).toBe("file");
    expect(iconKindFor({ name: "AmigaForever-DVD.iso", is_dir: false })).toBe("file");
  });
});

describe("fileTextColorVar", () => {
  it("is the plain white token for directories and ordinary files", () => {
    expect(fileTextColorVar({ name: "Tools", is_dir: true })).toBe("var(--tc-text)");
    expect(fileTextColorVar({ name: "AmigaForever-DVD.iso", is_dir: false })).toBe("var(--tc-text)");
  });

  it("is the light-blue token for a text file", () => {
    expect(fileTextColorVar({ name: "ReadMe.txt", is_dir: false })).toBe("var(--tc-text-file)");
  });

  it("is the dimmed token for a hidden/system file", () => {
    expect(fileTextColorVar({ name: "Desktop.ini", is_dir: false })).toBe("var(--tc-text-hidden)");
  });
});
