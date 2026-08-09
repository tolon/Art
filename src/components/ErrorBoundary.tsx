import { Component, type ErrorInfo, type ReactNode } from "react";

interface Props {
  children: ReactNode;
}
interface State {
  hasError: boolean;
  error?: Error;
  info?: string;
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
      return (
        <div style={{ padding: 24, color: "#d65a5a", fontFamily: "monospace", whiteSpace: "pre-wrap" }}>
          <h2 style={{ color: "#d65a5a" }}>ART failed to render</h2>
          <pre>{this.state.error?.toString()}</pre>
          {this.state.info && (
            <pre style={{ marginTop: 12, color: "#9aa3b2" }}>{this.state.info}</pre>
          )}
        </div>
      );
    }
    return this.props.children;
  }
}
