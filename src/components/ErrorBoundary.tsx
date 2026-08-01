import { Component, type ErrorInfo, type ReactNode } from 'react';

interface Props {
  children: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { hasError: false, error: null };

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('[ErrorBoundary]', error, info.componentStack);
  }

  handleReload = () => {
    window.location.reload();
  };

  handleDismiss = () => {
    this.setState({ hasError: false, error: null });
  };

  render() {
    if (!this.state.hasError) {
      return this.props.children;
    }

    return (
      <div className="flex h-screen w-screen items-center justify-center bg-surface p-8">
        <div className="max-w-md space-y-4 text-center">
          <div className="text-4xl">⚠️</div>
          <h2 className="text-lg font-semibold text-fg">Something went wrong</h2>
          <p className="text-sm text-fg-secondary break-all">
            {this.state.error?.message}
          </p>
          <div className="flex justify-center gap-3">
            <button
              onClick={this.handleDismiss}
              className="rounded-lg border border-border px-4 py-2 text-sm text-fg-secondary hover:bg-surface-alt"
            >
              Dismiss
            </button>
            <button
              onClick={this.handleReload}
              className="rounded-lg bg-accent px-4 py-2 text-sm text-white hover:bg-accent/90"
            >
              Reload
            </button>
          </div>
        </div>
      </div>
    );
  }
}
