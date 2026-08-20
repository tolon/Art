// @vitest-environment jsdom
//
// G2 of the wave-C1 re-review: `pre_existing_damage` reached the operation
// log, the application log and the frontend's types, and **nothing drew it**.
// A field nothing renders is the same silence the finding was filed about,
// with more code behind it.
//
// So this asserts the drawing, in both catalogues: the sentence appears, it
// says the write went through, and it names what was found. And it asserts
// the ordinary case renders nothing at all — a row that was always there
// would satisfy the first half and cry wolf on every healthy volume.

import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

import { changeLanguage } from "@/i18n";
import { DamageRow } from "@/components/files/DamageRow";

afterEach(async () => {
  cleanup();
  await changeLanguage("en");
});

const FINDINGS = [
  "Block 900 belongs to 'Game.exe' (block 880) and to block 884; writing to one would destroy the other. (blocks.crosslinked)",
];

describe("DamageRow", () => {
  it("renders nothing when the volume was sound", () => {
    const { container } = render(<DamageRow findings={[]} />);
    expect(container.textContent).toBe("");
  });

  it.each(["en", "tr"] as const)("says what was found, and when, in %s", async (language) => {
    await changeLanguage(language);
    render(<DamageRow findings={FINDINGS} />);

    const row = screen.getByRole("status");
    // The finding itself — the part that is not translated, because
    // `CoreError` sentences are English until ART-060 is answered.
    expect(row.textContent).toContain("blocks.crosslinked");
    // …and no raw key or unrendered interpolation around it.
    expect(row.textContent).not.toMatch(/files\.[a-zA-Z]+\.[a-zA-Z]/);
    expect(row.textContent).not.toMatch(/\{\{[a-zA-Z]/);
    // The count reached the sentence.
    expect(row.textContent).toContain("1");
  });

  it("shows at most three findings, so a wrecked volume cannot fill the shell", () => {
    render(<DamageRow findings={["one", "two", "three", "four"]} />);
    const row = screen.getByRole("status");
    expect(row.textContent).toContain("three");
    expect(row.textContent).not.toContain("four");
  });
});
