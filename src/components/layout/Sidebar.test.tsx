// @vitest-environment jsdom
//
// Regression guard for the sidebar's version display. It used to be a
// literal `v0.1` baked into the JSX — a second source of truth that was
// already wrong once the project passed 0.1.0 in substance if not in name.
// It now renders `import.meta.env.VITE_APP_VERSION`, set by vite.config.ts
// from package.json's own `version` field, so this test fails the moment
// the interface goes back to any hardcoded string: it does not know today's
// number in advance, it only knows the rendered text must match whatever
// `VITE_APP_VERSION` carries.
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";

import { Sidebar } from "./Sidebar";

afterEach(cleanup);

describe("Sidebar version", () => {
  it("renders VITE_APP_VERSION, not a hardcoded literal", () => {
    render(
      <MemoryRouter>
        <Sidebar />
      </MemoryRouter>,
    );

    expect(screen.getByText(`v${import.meta.env.VITE_APP_VERSION}`)).toBeTruthy();
  });
});
