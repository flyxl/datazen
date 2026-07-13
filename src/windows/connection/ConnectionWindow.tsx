import { useEffect } from 'react';
import { TitleBar } from '../../components/TitleBar';
import { useI18n } from '../../hooks/useI18n';
import { useThemeListener } from '../../hooks/useThemeListener';
import { useSettingsStore } from '../../stores/settingsStore';
import { connectionCommands } from '../../commands/connection';
import { emitCrossWindow, listenCrossWindow } from '../../lib/crossWindowBus';
import { getUrlParam } from '../../lib/windowKind';
import { DB_REGISTRY, getDbLabel } from '../../lib/databaseTypes';
import { getConnectionView } from '../../lib/connectionViews';
import type { DatabaseType } from '../../types';

export function ConnectionWindow() {
  useThemeListener();

  const { t } = useI18n();
  const loadSettings = useSettingsStore((s) => s.loadSettings);

  const connectionId = getUrlParam('connectionId') ?? '';
  const connectionName = getUrlParam('connectionName') ?? t('connWin.connected');
  const databaseType = getUrlParam('databaseType') ?? 'postgresql';
  const initialDatabase = getUrlParam('database') ?? undefined;

  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  useEffect(() => {
    if (!connectionId) return;
    let unlisten: (() => void) | undefined;
    let isClosing = false;

    (async () => {
      if (!('__TAURI_INTERNALS__' in globalThis)) return;
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      const win = getCurrentWindow();
      unlisten = await win.onCloseRequested(async (event) => {
        if (isClosing) return;
        isClosing = true;
        event.preventDefault();
        try {
          await connectionCommands.disconnect(connectionId);
        } catch (e) {
          console.error('[ConnectionWindow] disconnect on close failed', e);
        }
        await emitCrossWindow('datazen:connection-closed', { connectionId });
        await win.close();
      });
    })();

    return () => unlisten?.();
  }, [connectionId]);

  useEffect(() => {
    if (!connectionId) return;
    let cleanup: (() => void) | undefined;
    listenCrossWindow('datazen:disconnect-requested', async (payload) => {
      const data = payload as { connectionId?: string } | undefined;
      if (data?.connectionId !== connectionId) return;
      if (!('__TAURI_INTERNALS__' in globalThis)) return;
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      await getCurrentWindow().destroy();
    }).then((unlisten) => { cleanup = unlisten; });
    return () => cleanup?.();
  }, [connectionId]);

  useEffect(() => {
    if (!connectionId) return;
    const HEARTBEAT_MS = 5 * 60 * 1000;
    const timer = setInterval(() => {
      connectionCommands.pingConnection(connectionId).catch(() => {});
    }, HEARTBEAT_MS);
    return () => clearInterval(timer);
  }, [connectionId]);

  if (!connectionId) {
    return (
      <div className="flex h-screen items-center justify-center bg-surface text-fg">
        <div className="text-sm text-fg-muted">{t('connWin.missingParams')}</div>
      </div>
    );
  }

  const dbType = databaseType as DatabaseType;
  const viewMode = DB_REGISTRY[dbType]?.connectionView ?? 'sql';
  const ViewComponent = getConnectionView(viewMode);
  const centerTitle = `${connectionName} - ${getDbLabel(dbType)} - DataZen`;

  return (
    <div className="flex h-screen min-h-0 flex-col bg-surface text-fg">
      <TitleBar
        title={centerTitle}
        leftContent={
          <div className="flex items-center gap-2">
            <span className="inline-flex h-2 w-2 rounded-full bg-green-500" />
            <span className="text-xs text-fg-secondary">{connectionName}</span>
          </div>
        }
      />

      <ViewComponent
        connectionId={connectionId}
        connectionName={connectionName}
        databaseType={dbType}
        initialDatabase={initialDatabase}
      />
    </div>
  );
}
