import { Component, type ErrorInfo, type ReactNode } from "react";

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
  }

  private handleReload = () => {
    this.setState({ hasError: false, error: null });
  };

  render() {
    if (this.state.hasError) {
      return (
        <div className="error-boundary-fallback" role="alert">
          <p className="error-boundary-title">Component crashed</p>
          {this.props.sectionLabel ? (
            <p className="error-boundary-detail">This part of the UI failed ({this.props.sectionLabel}).</p>
          ) : (
            <p className="error-boundary-detail">This part of the UI failed to render.</p>
          )}
          {this.state.error ? (
            <pre className="error-boundary-message">{this.state.error.message}</pre>
          ) : null}
          <button type="button" className="error-boundary-reload" onClick={this.handleReload}>
            Reload
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
