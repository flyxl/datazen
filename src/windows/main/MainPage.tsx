import { useEffect, useRef } from 'react';
import { TitleBar } from '../../components/TitleBar';
import { MenuBar } from '../../components/MenuBar';
import { ThemeToggle } from '../../components/ThemeToggle';
import { useI18n } from '../../hooks/useI18n';
import { listenCrossWindow } from '../../lib/crossWindowBus';
import { openConnectionShareDialog } from '../../lib/connectionShare';
import { hideSplash } from '../../lib/splash';
import type { ConnectionImportSource } from '../../components/connection/ConnectionShareDialog';
import { openNewConnectionDialog } from '../../lib/windowManager';
import { useConnectionStore } from '../../stores/connectionStore';
import { Button } from '../../components/ui/Button';
import { ConnectionEditorDialogHost } from '../../components/connection/NewConnectionDialog';
import { ConnectionShareDialogHost } from '../../components/connection/ConnectionShareDialogHost';
import { ConnectionPage } from '../connection/ConnectionPage';
import { RunningExecutionCloseHost } from './RunningExecutionCloseHost';

/**
 * Main window entry.
 *
 * This window is created by Rust **after** the onboarding wizard has finished
 * (or immediately on an upgrade/normal launch).  It never renders the wizard
 * itself — that lives in a separate Tauri window.
 *
 * The zero-connection empty state is owned by `ConnectionPage` (via
 * `ConnectionWorkspaceHome` state 1): no separate welcome page exists, so a
 * user who skips the wizard or deletes every connection lands straight in the
 * workspace with create/import CTAs.
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

  // Hide the splash screen once the first connection load completes.
  const splashHidden = useRef(false);
  useEffect(() => {
    if (connectionsLoaded && !splashHidden.current) {
      splashHidden.current = true;
      hideSplash(document.getElementById('splash'));
    }
  }, [connectionsLoaded]);

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

  if (!connectionsLoaded) return null;

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
          data-testid="main-load-error"
        >
          <p className="select-text text-center text-sm text-danger">{loadError}</p>
          <Button data-testid="main-load-retry" onClick={() => void fetchConnections()}>
            {t('common.retry')}
          </Button>
        </div>
        <ConnectionEditorDialogHost />
        <ConnectionShareDialogHost />
      </div>
    );
  }

  return (
    <>
      <ConnectionPage />
      <ConnectionEditorDialogHost />
      <ConnectionShareDialogHost />
      <RunningExecutionCloseHost />
    </>
  );
}
