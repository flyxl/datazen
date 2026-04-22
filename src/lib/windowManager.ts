/**
 * Multi-window manager.
 *
 * In Tauri runtime, creates real OS windows via WebviewWindow.
 * In browser dev mode, opens new browser tabs with query params.
 */

import { t } from '../locales/t';
import { getPlatformSync } from '../hooks/usePlatform';

let counter = 0;

function nextLabel(prefix: string) {
  counter += 1;
  return `${prefix}-${Date.now()}-${counter}`;
}

interface OpenWindowOptions {
  /** URL query params to pass context to the new window. */
  params?: Record<string, string>;
  /** Window width (default 800). */
  width?: number;
  /** Window height (default 640). */
  height?: number;
  /** Window title. */
  title?: string;
  /** Whether to center the window (default true). */
  center?: boolean;
}

function currentBgColor(): string {
  return document.documentElement.classList.contains('dark') ? '#0f172a' : '#ffffff';
}

async function openTauriWindow(label: string, options: OpenWindowOptions) {
  const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
  const isMac = getPlatformSync() === 'macos';

  const qs = new URLSearchParams(options.params ?? {}).toString();
  const url = qs ? `index.html?${qs}` : 'index.html';

  new WebviewWindow(label, {
    url,
    title: options.title ?? 'DataZen',
    width: options.width ?? 800,
    height: options.height ?? 640,
    center: options.center ?? true,
    decorations: false,
    transparent: isMac,
    minWidth: 600,
    minHeight: 480,
    visible: false,
    ...(isMac ? {} : { backgroundColor: currentBgColor() }),
  });
}

function openBrowserWindow(options: OpenWindowOptions) {
  const qs = new URLSearchParams(options.params ?? {}).toString();
  const url = qs ? `/?${qs}` : '/';
  window.open(url, '_blank', `width=${options.width ?? 800},height=${options.height ?? 640}`);
}

function isTauri(): boolean {
  return '__TAURI_INTERNALS__' in window;
}

export function openNewConnectionWindow(editId?: string) {
  const params: Record<string, string> = { window: 'new-connection' };
  if (editId) params.editId = editId;

  const opts: OpenWindowOptions = {
    params,
    width: 800,
    height: 680,
    title: editId ? t('win.editConnection') : t('win.newConnection'),
  };

  if (isTauri()) {
    void openTauriWindow(nextLabel('new-connection'), opts);
  } else {
    openBrowserWindow(opts);
  }
}

export function openConnectionWindow(connectionId: string, connectionName: string, database?: string, databaseType?: string) {
  const params: Record<string, string> = { window: 'connection', connectionId, connectionName };
  if (database) params.database = database;
  if (databaseType) params.databaseType = databaseType;

  const opts: OpenWindowOptions = {
    params,
    width: 1200,
    height: 800,
    title: `${connectionName} - DataZen`,
  };

  if (isTauri()) {
    void openTauriWindow(nextLabel('connection'), opts);
  } else {
    openBrowserWindow(opts);
  }
}

export function openQueryWindow(connectionId: string, database: string) {
  const opts: OpenWindowOptions = {
    params: { window: 'query', connectionId, database },
    width: 1000,
    height: 700,
    title: t('win.query', { db: database }),
  };

  if (isTauri()) {
    void openTauriWindow(nextLabel('query'), opts);
  } else {
    openBrowserWindow(opts);
  }
}

export function openDataSyncWindow() {
  const opts: OpenWindowOptions = {
    params: { window: 'data-sync' },
    width: 1000,
    height: 700,
    title: t('win.dataSync'),
  };

  if (isTauri()) {
    void openTauriWindow(nextLabel('data-sync'), opts);
  } else {
    openBrowserWindow(opts);
  }
}

export function openBackupWindow() {
  const opts: OpenWindowOptions = {
    params: { window: 'backup' },
    width: 750,
    height: 520,
    title: t('win.backup'),
  };

  if (isTauri()) {
    void openTauriWindow(nextLabel('backup'), opts);
  } else {
    openBrowserWindow(opts);
  }
}

const SETTINGS_LABEL = 'settings-singleton';

export function openSettingsWindow() {
  if (isTauri()) {
    void (async () => {
      const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
      const existing = await WebviewWindow.getByLabel(SETTINGS_LABEL);
      if (existing) {
        await existing.setFocus();
        return;
      }
      const qs = new URLSearchParams({ window: 'settings' }).toString();
      const isMac = getPlatformSync() === 'macos';
      new WebviewWindow(SETTINGS_LABEL, {
        url: `index.html?${qs}`,
        title: t('win.settings'),
        width: 600,
        height: 560,
        center: true,
        decorations: false,
        transparent: isMac,
        minWidth: 480,
        minHeight: 400,
        visible: false,
        ...(isMac ? {} : { backgroundColor: currentBgColor() }),
      });
    })();
  } else {
    openBrowserWindow({
      params: { window: 'settings' },
      width: 600,
      height: 560,
      title: t('win.settings'),
    });
  }
}
