import { Component, type ErrorInfo, type ReactNode } from 'react';
import { useI18n } from '../hooks/useI18n';
import { TitleBar } from './TitleBar';

interface Props {
  children: ReactNode;
}

interface BoundaryProps extends Props {
  t: (key: 'common.error' | 'common.close' | 'common.retry' | 'backend.unknownError') => string;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

class ErrorBoundaryInner extends Component<BoundaryProps, State> {
  state: State = { hasError: false, error: null };

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('[ErrorBoundary]', {
      name: error.name,
      messageLength: error.message.length,
      componentStack: info.componentStack,
    });
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
      <div className="flex h-screen min-h-0 w-screen flex-col bg-surface">
        <TitleBar />
        <div className="flex flex-1 items-center justify-center p-8">
          <div className="max-w-md space-y-4 text-center">
            <div className="text-4xl">⚠️</div>
            <h2 className="text-lg font-semibold text-fg">{this.props.t('common.error')}</h2>
            <p className="text-sm text-fg-secondary break-all">
              {this.state.error?.message || this.props.t('backend.unknownError')}
            </p>
            <div className="flex justify-center gap-3">
              <button
                type="button"
                onClick={this.handleDismiss}
                className="rounded-lg border border-border px-4 py-2 text-sm text-fg-secondary hover:bg-surface-alt"
              >
                {this.props.t('common.close')}
              </button>
              <button
                type="button"
                onClick={this.handleReload}
                className="rounded-lg bg-accent px-4 py-2 text-sm text-white hover:bg-accent/90"
              >
                {this.props.t('common.retry')}
              </button>
            </div>
          </div>
        </div>
      </div>
    );
  }
}

export function ErrorBoundary({ children }: Props) {
  const { t } = useI18n();
  return <ErrorBoundaryInner t={t} children={children} />;
}
