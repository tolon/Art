// 24px two-tone icons for the dashboard's quick-action tiles (Windows 11 /
// Fluent look) — the exact shapes the owner-approved design canvas draws
// with (the generator's `qic()` function, 2026-09-07): a stroke in the
// current text colour, one accent-tinted plane. `fill="var(--accent)"` and
// `fill="var(--accent-tint)"` rather than props, so the icon follows the
// theme with no plumbing.

export type QuickIconName = "files" | "collection" | "winuae" | "card" | "install" | "whdload";

const ACCENT = "var(--accent)";
const TINT = "var(--accent-tint)";
// The two short lines punched through the card icon's accent-filled slot —
// drawn in the page background so they read as a light channel against the
// fill, the same trick `gen.py`'s `qic("card", ...)` uses with `t["mica"]`.
const PUNCH = "var(--bg)";

function Svg({ children }: { children: React.ReactNode }) {
  return (
    <svg
      width={28}
      height={28}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.4"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {children}
    </svg>
  );
}

export function QuickIcon({ name }: { name: QuickIconName }) {
  switch (name) {
    case "files":
      // Two-pane commander: two listings, the left one active.
      return (
        <Svg>
          <rect x="2.5" y="4.5" width="19" height="15" rx="2" />
          <path d="M12 4.5v15" />
          <rect x="4.5" y="7" width="5.5" height="2.2" rx="0.6" fill={ACCENT} stroke="none" />
          <path d="M4.5 12h5.5M4.5 15h5.5M14 8h5.5M14 11h5.5M14 14h5.5M14 17h3.5" />
        </Svg>
      );
    case "collection":
      // A stack of three 3.5" floppies, fanned, with shutters.
      return (
        <Svg>
          <rect x="3" y="9" width="14" height="12" rx="1.5" fill={TINT} />
          <path d="M6.5 9V6.6a1.2 1.2 0 0 1 1.2-1.2h11.6a1.2 1.2 0 0 1 1.2 1.2v9.8" />
          <path d="M9.5 5.4V3.2a1.2 1.2 0 0 1 1.2-1.2h9.1a1.2 1.2 0 0 1 1.2 1.2v9.8" />
          <rect x="6" y="9" width="7" height="3.2" rx="0.6" fill={ACCENT} stroke="none" />
          <path d="M5.5 16.5h9M5.5 18.8h5" />
        </Svg>
      );
    case "winuae":
      // A CRT on an A500 wedge, a checkmark-shaped cursor on the screen.
      return (
        <Svg>
          <rect x="4" y="2.5" width="16" height="12" rx="1.6" />
          <rect x="6" y="4.5" width="12" height="8" rx="0.8" fill={TINT} />
          <path d="M9 9.2l2 2 4-4.4" stroke={ACCENT} strokeWidth="1.8" />
          <path d="M2.5 20.5 4.5 16h15l2 4.5z" fill={TINT} />
          <path d="M8 18.3h8" />
        </Svg>
      );
    case "card":
      // A microSD with the Amiga area marked.
      return (
        <Svg>
          <path
            d="M8.5 2.5h9A1.5 1.5 0 0 1 19 4v16a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 5 20V7z"
            fill={TINT}
          />
          <path d="M9.5 4v3M12 4v3M14.5 4v3M17 4v3" />
          <rect x="7.5" y="11" width="9" height="7.5" rx="0.8" fill={ACCENT} stroke="none" />
          <path d="M9.5 13.5h5M9.5 16h3" stroke={PUNCH} strokeWidth="1.2" />
        </Svg>
      );
    case "install":
      // A floppy going down into a drive slot.
      return (
        <Svg>
          <rect x="6" y="2.5" width="12" height="10" rx="1.2" fill={TINT} />
          <rect x="8.5" y="2.5" width="6" height="3" rx="0.5" fill={ACCENT} stroke="none" />
          <path d="M12 12.5v5M9.5 15l2.5 2.5 2.5-2.5" />
          <rect x="2.5" y="17.5" width="19" height="4" rx="1.2" />
          <path d="M5.5 19.5h5" />
        </Svg>
      );
    case "whdload":
      // A ball-top joystick with a fire button.
      return (
        <Svg>
          <circle cx="12" cy="5" r="2.6" fill={ACCENT} stroke="none" />
          <path d="M12 7.6v6.4" />
          <path
            d="M3.5 21a3 3 0 0 1-1-2.3c0-2.4 4.3-4.7 9.5-4.7s9.5 2.3 9.5 4.7A3 3 0 0 1 20.5 21z"
            fill={TINT}
          />
          <circle cx="17" cy="17" r="1.3" fill={ACCENT} stroke="none" />
          <path d="M3.5 21h17" />
        </Svg>
      );
  }
}
