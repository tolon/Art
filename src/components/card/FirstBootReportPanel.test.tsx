// @vitest-environment jsdom
//
// A plain presentational read of `S:FirstBoot.log`'s parsed shape (Task 9 of
// the first-boot round; Task 11 mounts this on the card screen with a
// `source` prop this test does not cover).
//
// Endings stay distinct (CLAUDE.md): the wording carries which ending it is,
// and colour is never the only signal — every assertion below checks the
// *text*, and the tone assertions check a `data-tone` attribute alongside it
// rather than instead of it.

import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import i18n from "i18next";

// Side-effecting: a real, synchronously-initialised i18next instance, the
// way every other component test in this suite sets one up.
import "@/i18n";
import { useSettingsStore } from "@/stores/settingsStore";
import type { FirstBootReport } from "@/lib/firstboot";
import { FirstBootReportPanel } from "@/components/card/FirstBootReportPanel";

function report(over: Partial<FirstBootReport> = {}): FirstBootReport {
  return {
    version: 1,
    system: { system: "PiStorm", rpi: "RPi4", kick: "3.2" },
    steps: [
      { name: "10-hardware", outcome: { kind: "ok" }, details: ["sd0-mounted RPi4"] },
      { name: "20-aux", outcome: { kind: "skipped", reason: "not-3.9" }, details: [] },
      { name: "50-pkg-boingbag-39-1", outcome: { kind: "refused", rc: 20 }, details: [] },
    ],
    ending: "done-partial",
    fatCopyFailed: false,
    rebootRequestedBy: null,
    unknown: [],
    ...over,
  };
}

afterEach(() => {
  cleanup();
  useSettingsStore.setState((state) => ({ settings: { ...state.settings, uxMode: "beginner" } }));
});

describe("what the Amiga said it detected", () => {
  it("names the system, the Pi model and the Kickstart it booted", () => {
    render(<FirstBootReportPanel report={report()} />);
    const said = screen.getByTestId("firstboot-report-system").textContent ?? "";
    expect(said).toContain("PiStorm");
    expect(said).toContain("RPi4");
    expect(said).toContain("3.2");
  });

  it("says nothing about the system when the Amiga never wrote one", () => {
    render(<FirstBootReportPanel report={report({ system: null })} />);
    expect(screen.queryByTestId("firstboot-report-system")).toBeNull();
  });
});

describe("one row per step, wording and colour together", () => {
  it("says what happened to each step, in words", () => {
    render(<FirstBootReportPanel report={report()} />);
    const rows = screen.getAllByTestId("firstboot-report-step");
    expect(rows).toHaveLength(3);
    expect(rows[0].textContent).toContain("10-hardware");
    expect(rows[0].textContent).toContain(i18n.t("firstboot.step.ok"));
    expect(rows[1].textContent).toContain("20-aux");
    expect(rows[1].textContent).toContain(i18n.t("firstboot.step.skipped", { reason: "not-3.9" }));
    expect(rows[2].textContent).toContain("50-pkg-boingbag-39-1");
    expect(rows[2].textContent).toContain(i18n.t("firstboot.step.refused", { rc: 20 }));
  });

  it("carries the tone alongside the wording, never instead of it", () => {
    render(<FirstBootReportPanel report={report()} />);
    const rows = screen.getAllByTestId("firstboot-report-step");
    expect(rows[0].dataset.tone).toBe("ok");
    expect(rows[1].dataset.tone).toBe("muted");
    expect(rows[2].dataset.tone).toBe("warn");
  });

  it("shows a step's details only in power mode", () => {
    render(<FirstBootReportPanel report={report()} />);
    expect(screen.queryByText("sd0-mounted RPi4")).toBeNull();

    cleanup();
    useSettingsStore.setState((state) => ({ settings: { ...state.settings, uxMode: "power" } }));
    render(<FirstBootReportPanel report={report()} />);
    expect(screen.getByText("sd0-mounted RPi4")).toBeTruthy();
  });
});

describe("the ending, and the two things that ride beside it", () => {
  it("says the ending in its own sentence", () => {
    render(<FirstBootReportPanel report={report({ ending: "done-partial" })} />);
    expect(screen.getByTestId("firstboot-report-ending").textContent).toBe(
      i18n.t("firstboot.ending.donePartial")
    );
  });

  it("notes a failed FAT copy when the flag is set, and says nothing when it is not", () => {
    render(<FirstBootReportPanel report={report({ fatCopyFailed: true })} />);
    expect(screen.getByTestId("firstboot-report-fat-copy-failed")).toBeTruthy();

    cleanup();
    render(<FirstBootReportPanel report={report({ fatCopyFailed: false })} />);
    expect(screen.queryByTestId("firstboot-report-fat-copy-failed")).toBeNull();
  });

  it("carries a line from a newer ART verbatim, under its own heading", () => {
    render(<FirstBootReportPanel report={report({ unknown: ["frobnicate 3"] })} />);
    const unknown = screen.getByTestId("firstboot-report-unknown");
    expect(unknown.textContent).toContain("frobnicate 3");
  });

  it("says nothing about unknown lines when there are none", () => {
    render(<FirstBootReportPanel report={report({ unknown: [] })} />);
    expect(screen.queryByTestId("firstboot-report-unknown")).toBeNull();
  });
});

describe("which copy was read", () => {
  it("says the Amiga volume's own copy when source is amiga-volume", () => {
    render(<FirstBootReportPanel report={report()} source="amiga-volume" />);
    expect(screen.getByTestId("firstboot-report-source").textContent).toBe(
      i18n.t("firstboot.report.source.amigaVolume")
    );
  });

  it("says the FAT partition's copy when source is fat", () => {
    render(<FirstBootReportPanel report={report()} source="fat" />);
    expect(screen.getByTestId("firstboot-report-source").textContent).toBe(
      i18n.t("firstboot.report.source.fat")
    );
  });

  it("says nothing about a source when there is none, or none is given", () => {
    render(<FirstBootReportPanel report={report()} source="none" />);
    expect(screen.queryByTestId("firstboot-report-source")).toBeNull();

    cleanup();
    render(<FirstBootReportPanel report={report()} />);
    expect(screen.queryByTestId("firstboot-report-source")).toBeNull();
  });
});

describe("a card that has never been booted", () => {
  it("shows only the not-booted sentence and no table", () => {
    render(
      <FirstBootReportPanel
        report={report({
          system: null,
          steps: [],
          ending: "not-booted",
          unknown: [],
        })}
      />
    );
    expect(screen.getByTestId("firstboot-report-ending").textContent).toBe(
      i18n.t("firstboot.ending.notBooted")
    );
    expect(screen.queryAllByTestId("firstboot-report-step")).toHaveLength(0);
    expect(screen.queryByTestId("firstboot-report-system")).toBeNull();
    expect(screen.queryByTestId("firstboot-report-table")).toBeNull();
  });
});
