// @vitest-environment jsdom
//
// The Kickstart agreement step (round 4, task 9): what the titles want, what
// ART found, and only what the user ticks — the owner's rule of 2026-08-21.
//
// **Presentational, so nothing here is mocked at the `@/lib/*` boundary.**
// The component takes a `KickstartProposal` as a prop and reports the agreed
// names upward through a callback; it makes no `invoke` call and subscribes
// to no job, so there is nothing to fake — the house pattern
// (`CardSection.test.tsx`) is for a component that talks to Tauri, and this
// one does not.

import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { changeLanguage } from "@/i18n";
import { KickstartAgreement } from "@/components/osbuilder/KickstartAgreement";
import type { KickstartProposal, ProposedKickstart } from "@/lib/cardOs";
import type { KickstartOffer } from "@/lib/gameindex";

afterEach(async () => {
  cleanup();
  await changeLanguage("en");
});

const SUPPLIED: KickstartOffer = {
  outcome: "supplied",
  wanted: { name: "kick40068.A1200", crc16: 1234, size: 524_288 },
  by: { path: "E:\\roms\\kick40068.A1200", name: "kick40068.A1200", sizeDisagrees: null },
};
const ENCRYPTED: KickstartOffer = {
  outcome: "encrypted",
  wanted: { name: "kick34005.A500", crc16: 4321, size: 262_144 },
  candidates: ["E:\\afe\\rom\\kick34005.A500"],
};
const NOT_HERE: KickstartOffer = {
  outcome: "not-here",
  wanted: { name: "kick39106.A600", crc16: 1, size: 524_288 },
};
const UNMATCHABLE: KickstartOffer = {
  outcome: "unmatchable",
  wanted: { name: "kick13.A500", crc16: null, size: null },
};

function readyItem(over: Partial<ProposedKickstart> = {}): ProposedKickstart {
  return {
    name: "kick40068.A1200",
    titles: ["Games/Turrican/Turrican.slave"],
    titlesMore: 0,
    offer: SUPPLIED,
    rtb: { kind: "loose", path: "E:\\material\\kick40068.A1200.RTB" },
    ...over,
  };
}

function proposal(items: ProposedKickstart[], over: Partial<KickstartProposal> = {}): KickstartProposal {
  return { items, unreadableSlaves: [], rtbMissing: false, ...over };
}

describe("KickstartAgreement", () => {
  it("lets a supplied item with an RTB be ticked, and reports its name", async () => {
    const user = userEvent.setup();
    const onAgreedChange = vi.fn();
    render(
      <KickstartAgreement proposal={proposal([readyItem()])} onAgreedChange={onAgreedChange} />
    );

    const checkbox = screen.getByTestId("kickstart-check-kick40068.A1200") as HTMLInputElement;
    expect(checkbox.disabled).toBe(false);
    await user.click(checkbox);

    expect(onAgreedChange).toHaveBeenLastCalledWith(["kick40068.A1200"]);

    await user.click(checkbox);
    expect(onAgreedChange).toHaveBeenLastCalledWith([]);
  });

  it("cannot tick a supplied item with a missing RTB, and says where to get it", () => {
    render(
      <KickstartAgreement
        proposal={proposal([
          readyItem({ rtb: { kind: "missing", getFrom: "aminet-skick346" } }),
        ])}
        onAgreedChange={vi.fn()}
      />
    );

    const checkbox = screen.getByTestId("kickstart-check-kick40068.A1200") as HTMLInputElement;
    expect(checkbox.disabled).toBe(true);
    expect(screen.getByTestId("kickstart-row-kick40068.A1200").textContent).toMatch(
      /skick346/
    );
  });

  it("an encrypted offer cannot be ticked and says rom.key", () => {
    render(
      <KickstartAgreement
        proposal={proposal([readyItem({ name: "kick34005.A500", offer: ENCRYPTED })])}
        onAgreedChange={vi.fn()}
      />
    );

    const checkbox = screen.getByTestId("kickstart-check-kick34005.A500") as HTMLInputElement;
    expect(checkbox.disabled).toBe(true);
    expect(screen.getByTestId("kickstart-row-kick34005.A500").textContent).toMatch(/rom\.key/);
  });

  it.each([
    ["not-here", NOT_HERE, "kick39106.A600"],
    ["unmatchable", UNMATCHABLE, "kick13.A500"],
  ])("cannot tick a %s offer", (_label, offer, name) => {
    render(
      <KickstartAgreement
        proposal={proposal([readyItem({ name, offer })])}
        onAgreedChange={vi.fn()}
      />
    );
    const checkbox = screen.getByTestId(`kickstart-check-${name}`) as HTMLInputElement;
    expect(checkbox.disabled).toBe(true);
  });

  it("lists unreadable slaves rather than dropping them", () => {
    render(
      <KickstartAgreement
        proposal={proposal([], { unreadableSlaves: ["Games/Broken/Broken.slave"] })}
        onAgreedChange={vi.fn()}
      />
    );
    expect(screen.getByTestId("kickstart-unreadable").textContent).toContain(
      "Games/Broken/Broken.slave"
    );
  });

  it("allows ticking nothing — the summary says so and nothing is forced", () => {
    render(
      <KickstartAgreement
        proposal={proposal([readyItem(), readyItem({ name: "kick34005.A500", offer: ENCRYPTED })])}
        onAgreedChange={vi.fn()}
      />
    );
    // Neither checkbox is checked by ART itself.
    const checkbox = screen.getByTestId("kickstart-check-kick40068.A1200") as HTMLInputElement;
    expect(checkbox.checked).toBe(false);
    const summary = screen.getByTestId("kickstart-summary").textContent ?? "";
    expect(summary).toMatch(/0/);
  });

  it("renders in Turkish with no raw key or unresolved interpolation left on screen", async () => {
    await changeLanguage("tr");
    render(
      <KickstartAgreement
        proposal={proposal([
          readyItem(),
          readyItem({
            name: "kick34005.A500",
            offer: ENCRYPTED,
            rtb: { kind: "missing", getFrom: "whdload-seven-cities-of-gold" },
          }),
        ], { unreadableSlaves: ["Games/Broken/Broken.slave"] })}
        onAgreedChange={vi.fn()}
      />
    );
    const text = document.body.textContent ?? "";
    expect(text).not.toMatch(/cardKickstart\./);
    expect(text).not.toMatch(/\{\{/);
  });
});
