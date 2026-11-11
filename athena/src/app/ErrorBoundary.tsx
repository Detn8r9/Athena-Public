import { Component, type ErrorInfo, type ReactNode } from "react";
import { athenaInvoke } from "../shared/ipc/athenaIpc";

interface ErrorBoundaryProps {
  children: ReactNode;
  /** Shown in the fallback for context (e.g. "map preview"). */
  sectionLabel?: string;
}

interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
}

/**
 * Catches render errors in children so a single bad message or map tile does not blank the whole app.
 */
export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error("[ErrorBoundary]", error, errorInfo.componentStack);
    // Record it where Settings → Troubleshooting can show it; never let logging itself throw.
    const where = this.props.sectionLabel ? ` (${this.props.sectionLabel})` : "";
    const stack = (errorInfo.componentStack ?? "").split("\n").slice(0, 6).join(" ").trim();
    void athenaInvoke("append_error_log", {
      message: `[render crash${where}] ${error.message} ${stack}`.slice(0, 1200),
    }).catch(() => {});
  }

  private handleReload = () => {
    this.setState({ hasError: false, error: null });
  };

  render() {
    if (this.state.hasError) {
      return (
        <div className="error-boundary-fallback" role="alert">
          <p className="error-boundary-title">
            {this.props.sectionLabel
              ? `${this.props.sectionLabel} ran into a problem`
              : "This screen ran into a problem"}
          </p>
          <p className="error-boundary-detail">
            The rest of ATHENA is still running and your data is safe. Try again, or switch to
            another screen. The details were saved to the error log in Settings → Troubleshooting.
          </p>
          {this.state.error ? (
            <pre className="error-boundary-message">{this.state.error.message}</pre>
          ) : null}
          <button type="button" className="error-boundary-reload" onClick={this.handleReload}>
            Try again
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
