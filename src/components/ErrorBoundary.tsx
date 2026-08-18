import { Component, type ErrorInfo, type ReactNode } from "react";

import i18n from "@/i18n";

interface Props {
  children: ReactNode;
}
interface State {
  hasError: boolean;
  error?: Error;
  info?: string;
}

// This screen is what a user sees when something else in ART has already
// broken — and i18n itself is one of the things that can break before it
// gets this far. A bare t() call here could produce a blank screen at
// exactly the moment an explanation matters most, so the title falls back to
// hardcoded English whenever i18next is not ready or throws. This is the one
// place in ART where untranslated text beats no text.
const FALLBACK_TITLE = "ART failed to render";

function titleText(): string {
  try {
    if (!i18n.isInitialized) return FALLBACK_TITLE;
    const key = "components.errorBoundary.title";
    const translated = i18n.t(key);
    return translated && translated !== key ? translated : FALLBACK_TITLE;
  } catch {
    return FALLBACK_TITLE;
  }
}

/**
 * Catches render-time errors so we can see *why* the UI is blank instead of a
 * silent white screen. Especially useful when a third-party component throws
 * during initial render.
 */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { hasError: false };

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    this.setState({ info: info.componentStack ?? "" });
    console.error("[ART ErrorBoundary]", error, info);
  }

  render() {
    if (this.state.hasError) {
      // The dark theme's own hex values used to be baked in here, so the one
      // screen that appears when everything else has failed was unreadable in
      // the light theme — 2.3:1 for the stack trace (ART-140).
      return (
        <div style={{ padding: 24, color: "var(--err-text)", fontFamily: "monospace", whiteSpace: "pre-wrap" }}>
          <h2 style={{ color: "var(--err-text)" }}>{titleText()}</h2>
          <pre>{this.state.error?.toString()}</pre>
          {this.state.info && (
            <pre style={{ marginTop: 12, color: "var(--text-muted)" }}>{this.state.info}</pre>
          )}
        </div>
      );
    }
    return this.props.children;
  }
}
