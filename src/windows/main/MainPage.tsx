import { useEffect } from 'react';
import { TitleBar } from '../../components/TitleBar';
import { MenuBar } from '../../components/MenuBar';
import { ThemeToggle } from '../../components/ThemeToggle';
import { useI18n } from '../../hooks/useI18n';
import { listenCrossWindow } from '../../lib/crossWindowBus';
import { openConnectionShareDialog } from '../../lib/connectionShare';
import type { ConnectionImportSource } from '../../components/connection/ConnectionShareDialog';
import { openNewConnectionDialog } from '../../lib/windowManager';
import { useConnectionStore } from '../../stores/connectionStore';
import { Button } from '../../components/ui/Button';
import { ConnectionEditorDialogHost } from '../../components/connection/NewConnectionDialog';
import { ConnectionShareDialogHost } from '../../components/connection/ConnectionShareDialogHost';
import { ConnectionPage } from '../connection/ConnectionPage';
import { WelcomePage } from '../welcome/WelcomePage';
import { useMigrationWindowMenuActions } from '../../hooks/useMigrationWindowMenuActions';

/**
 * Main window entry: first-run welcome when no saved connections,
 * otherwise the unified connection workspace.
 */
export function MainPage() {
  const { t } = useI18n();
  useMigrationWindowMenuActions();
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

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    void listenCrossWindow('menu:new-connection', () => {
      openNewConnectionDialog();
    }).then((fn) => {
      cleanup = fn;
    });
    return () => cleanup?.();
  }, []);

  useEffect(() => {
    const cleanups: Array<() => void> = [];
    const openImport = (source: ConnectionImportSource = 'file') => {
      openConnectionShareDialog('import', source);
    };

    void listenCrossWindow('menu:export-connections', () => {
      openConnectionShareDialog('export');
    }).then((fn) => cleanups.push(fn));
    void listenCrossWindow('menu:import-connections', () => {
      openImport();
    }).then((fn) => cleanups.push(fn));
    void listenCrossWindow('menu:import-connections-file', () => {
      openImport('file');
    }).then((fn) => cleanups.push(fn));
    void listenCrossWindow('menu:import-connections-dbx', () => {
      openImport('dbx');
    }).then((fn) => cleanups.push(fn));
    void listenCrossWindow('menu:import-connections-navicat', () => {
      openImport('navicat');
    }).then((fn) => cleanups.push(fn));
    void listenCrossWindow('menu:import-connections-datagrip', () => {
      openImport('datagrip');
    }).then((fn) => cleanups.push(fn));
    void listenCrossWindow('menu:import-connections-dbeaver', () => {
      openImport('dbeaver');
    }).then((fn) => cleanups.push(fn));
    void listenCrossWindow('menu:import-connections-tableplus', () => {
      openImport('tableplus');
    }).then((fn) => cleanups.push(fn));

    return () => cleanups.forEach((fn) => fn());
  }, []);

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
        <ConnectionEditorDialogHost />
        <ConnectionShareDialogHost />
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
        <ConnectionEditorDialogHost />
        <ConnectionShareDialogHost />
      </div>
    );
  }

  if (connections.length === 0) {
    return (
      <>
        <WelcomePage />
        <ConnectionEditorDialogHost />
        <ConnectionShareDialogHost />
      </>
    );
  }

  return (
    <>
      <ConnectionPage />
      <ConnectionEditorDialogHost />
      <ConnectionShareDialogHost />
    </>
  );
}
