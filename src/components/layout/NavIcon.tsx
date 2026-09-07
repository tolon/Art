// 16px stroke icons for the sidebar's nav list (Windows 11 / Fluent look).
// The shapes are the exact vocabulary the owner-approved design canvas draws
// with (the generator's `I` icon dict, 2026-09-07) — currentColor stroke, no
// fill, so each icon follows the sidebar's own text colour in both themes
// without a prop. Hand-drawn inline SVG for the same reason `TcIcon.tsx` is:
// no icon font/library dependency, and ART ships no third-party content.

export type NavIconName =
  | "home"
  | "folder"
  | "floppy"
  | "archive"
  | "gamepad"
  | "hdd"
  | "gotek"
  | "chip"
  | "monitor"
  | "bolt"
  | "blocks"
  | "layout"
  | "books"
  | "globe"
  | "wrench"
  | "gear";

const SIZE = 16;

function Svg({ children }: { children: React.ReactNode }) {
  return (
    <svg
      width={SIZE}
      height={SIZE}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.3"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {children}
    </svg>
  );
}

export function NavIcon({ name }: { name: NavIconName }) {
  switch (name) {
    case "home":
      return (
        <Svg>
          <path d="M2.5 7.5 8 3l5.5 4.5V13a.5.5 0 0 1-.5.5H9.5V10h-3v3.5H3a.5.5 0 0 1-.5-.5z" />
        </Svg>
      );
    case "folder":
      return (
        <Svg>
          <path d="M2 4.5A1.5 1.5 0 0 1 3.5 3h3l1.5 1.5H12.5A1.5 1.5 0 0 1 14 6v5.5a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 11.5z" />
        </Svg>
      );
    case "floppy":
      return (
        <Svg>
          <rect x="2.5" y="2.5" width="11" height="11" rx="1.5" />
          <path d="M5 2.5v3.5h5V2.5M4.5 13.5V9.5h7v4" />
        </Svg>
      );
    case "archive":
      return (
        <Svg>
          <rect x="2" y="3" width="12" height="3" rx="1" />
          <path d="M3 6v6.5a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V6M6.5 9h3" />
        </Svg>
      );
    case "gamepad":
      return (
        <Svg>
          <path d="M5 5h6a3.5 3.5 0 0 1 3.4 4.3l-.6 2.4a1.5 1.5 0 0 1-2.7.5L10 10.5H6L4.9 12.2a1.5 1.5 0 0 1-2.7-.5l-.6-2.4A3.5 3.5 0 0 1 5 5zM5 7.5v2M4 8.5h2M10.5 8h.01M12 9.5h.01" />
        </Svg>
      );
    case "hdd":
      return (
        <Svg>
          <rect x="2" y="4" width="12" height="8" rx="1.5" />
          <path d="M2 9.5h12M11 10.8h.5" />
        </Svg>
      );
    case "gotek":
      return (
        <Svg>
          <rect x="2.5" y="4" width="11" height="8" rx="1" />
          <path d="M5 6.5h6M5 9.5h3" />
        </Svg>
      );
    case "chip":
      return (
        <Svg>
          <rect x="4.5" y="4.5" width="7" height="7" rx="1" />
          <path d="M6.5 2v2.5M9.5 2v2.5M6.5 11.5V14M9.5 11.5V14M2 6.5h2.5M2 9.5h2.5M11.5 6.5H14M11.5 9.5H14" />
        </Svg>
      );
    case "monitor":
      return (
        <Svg>
          <rect x="2" y="3" width="12" height="8" rx="1.5" />
          <path d="M6 13.5h4M8 11v2.5" />
        </Svg>
      );
    case "bolt":
      return (
        <Svg>
          <path d="M9 2 3.5 9H8l-1 5 5.5-7H8z" />
        </Svg>
      );
    case "blocks":
      return (
        <Svg>
          <rect x="2.5" y="8.5" width="5" height="5" rx="1" />
          <rect x="8.5" y="8.5" width="5" height="5" rx="1" />
          <rect x="5.5" y="2.5" width="5" height="5" rx="1" />
        </Svg>
      );
    case "layout":
      return (
        <Svg>
          <rect x="2.5" y="2.5" width="11" height="11" rx="1.5" />
          <path d="M2.5 6.5h11M6.5 6.5v7" />
        </Svg>
      );
    case "books":
      return (
        <Svg>
          <rect x="2.5" y="6.5" width="9" height="7.5" rx="1" />
          <path d="M4.5 6.5V4.8a1 1 0 0 1 1-1h7a1 1 0 0 1 1 1v5.2M6.5 3.8V2.3a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v5.2M5 11.5h4M5 9.5h2" />
        </Svg>
      );
    case "globe":
      return (
        <Svg>
          <circle cx="8" cy="8" r="5.5" />
          <path d="M2.5 8h11M8 2.5c2 2 2 9 0 11M8 2.5c-2 2-2 9 0 11" />
        </Svg>
      );
    case "wrench":
      return (
        <Svg>
          <path d="M9.8 2.7a3.2 3.2 0 0 0-3.6 4.4L2.5 10.8 5.2 13.5l3.7-3.7a3.2 3.2 0 0 0 4.4-3.6L11 8.5 8.5 6l2.3-2.3z" />
        </Svg>
      );
    case "gear":
      return (
        <Svg>
          <circle cx="8" cy="8" r="2" />
          <path d="M8 2v1.5M8 12.5V14M2 8h1.5M12.5 8H14M3.8 3.8l1 1M11.2 11.2l1 1M3.8 12.2l1-1M11.2 4.8l1-1" />
        </Svg>
      );
  }
}
