import { useEffect } from 'react';
import { TitleBar } from '../../components/TitleBar';
import { MenuBar } from '../../components/MenuBar';
import { ThemeToggle } from '../../components/ThemeToggle';
import { useI18n } from '../../hooks/useI18n';
import { listenCrossWindow } from '../../lib/crossWindowBus';
import { useConnectionStore } from '../../stores/connectionStore';
import { Button } from '../../components/ui/Button';
import { ConnectionPage } from '../connection/ConnectionPage';
import { WelcomePage } from '../welcome/WelcomePage';

/**
 * Main window entry: first-run welcome when no saved connections,
 * otherwise the unified connection workspace.
 */
export function MainPage() {
  const { t } = useI18n();
  const connections = useConnectionStore((s) => s.connections);
  const connectionsLoaded = useConnectionStore((s) => s.connectionsLoaded);
  const loadError = useConnectionStore((s) => s.error);
  const fetchConnections = useConnectionStore((s) => s.fetchConnections);
  const fetchGroups = useConnectionStore((s) => s.fetchGroups);

  useEffect(() => {
    void fetchConnections();
    void fetchGroups();
  }, [fetchConnections, fetchGroups]);

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    void listenCrossWindow('datazen:connections-changed', () => {
      void fetchConnections();
      void fetchGroups();
    }).then((fn) => {
      cleanup = fn;
    });
    return () => cleanup?.();
  }, [fetchConnections, fetchGroups]);

  if (!connectionsLoaded) {
    return (
      <div className="flex h-full min-h-0 flex-col bg-surface text-fg">
        <TitleBar
          title={t('menu.appName')}
          leftContent={<MenuBar />}
          rightContent={<ThemeToggle />}
        />
        <div
          className="flex flex-1 items-center justify-center"
          data-testid="main-connections-loading"
        >
          <div className="h-6 w-6 animate-spin rounded-full border-2 border-accent border-t-transparent" />
        </div>
      </div>
    );
  }

  if (connections.length === 0 && loadError) {
    return (
      <div className="flex h-full min-h-0 flex-col bg-surface text-fg">
        <TitleBar
          title={t('menu.appName')}
          leftContent={<MenuBar />}
          rightContent={<ThemeToggle />}
        />
        <div
          className="flex flex-1 flex-col items-center justify-center gap-4 px-6"
          data-testid="welcome-load-error"
        >
          <p className="text-center text-sm text-red-400">{loadError}</p>
          <Button data-testid="welcome-load-retry" onClick={() => void fetchConnections()}>
            {t('common.retry')}
          </Button>
        </div>
      </div>
    );
  }

  if (connections.length === 0) {
    return <WelcomePage />;
  }

  return <ConnectionPage />;
}
