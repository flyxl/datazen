/**
 * Multi-window manager.
 *
 * In Tauri runtime, creates real OS windows via Rust-side command
 * to ensure `accept_first_mouse` is applied at the native layer.
 * In browser dev mode, opens new browser tabs with query params.
 */

import { invoke } from '@tauri-apps/api/core';
import { t } from '../locales/t';

let counter = 0;

function nextLabel(prefix: string) {
  counter += 1;
  return `${prefix}-${Date.now()}-${counter}`;
}

interface OpenWindowOptions {
  params?: Record<string, string>;
  width?: number;
  height?: number;
  minWidth?: number;
  minHeight?: number;
  title?: string;
  center?: boolean;
}

function isTauri(): boolean {
  return '__TAURI_INTERNALS__' in window;
}

async function openTauriWindow(label: string, options: OpenWindowOptions) {
  const qs = new URLSearchParams(options.params ?? {}).toString();
  const url = qs ? `index.html?${qs}` : 'index.html';

  await invoke('create_sub_window', {
    options: {
      label,
      url,
      title: options.title ?? 'DataZen',
      width: options.width ?? 800,
      height: options.height ?? 640,
      minWidth: options.minWidth,
      minHeight: options.minHeight,
      center: options.center ?? true,
    },
  });
}

function openBrowserWindow(options: OpenWindowOptions) {
  const qs = new URLSearchParams(options.params ?? {}).toString();
  const url = qs ? `/?${qs}` : '/';
  window.open(url, '_blank', `width=${options.width ?? 800},height=${options.height ?? 640}`);
}

function openWindow(label: string, options: OpenWindowOptions) {
  if (isTauri()) {
    void openTauriWindow(label, options);
  } else {
    openBrowserWindow(options);
  }
}

export function openNewConnectionWindow(editId?: string) {
  const params: Record<string, string> = { window: 'new-connection' };
  if (editId) params.editId = editId;

  openWindow(nextLabel('new-connection'), {
    params,
    width: 800,
    height: 680,
    minWidth: 600,
    minHeight: 480,
    title: editId ? t('win.editConnection') : t('win.newConnection'),
  });
}

export function openConnectionWindow(connectionId: string, connectionName: string, database?: string, databaseType?: string) {
  const params: Record<string, string> = { window: 'connection', connectionId, connectionName };
  if (database) params.database = database;
  if (databaseType) params.databaseType = databaseType;

  openWindow(nextLabel('connection'), {
    params,
    width: 1200,
    height: 800,
    minWidth: 600,
    minHeight: 480,
    title: `${connectionName} - DataZen`,
  });
}

export function openQueryWindow(connectionId: string, database: string) {
  openWindow(nextLabel('query'), {
    params: { window: 'query', connectionId, database },
    width: 1000,
    height: 700,
    minWidth: 600,
    minHeight: 480,
    title: t('win.query', { db: database }),
  });
}

export function openDataSyncWindow() {
  openWindow(nextLabel('data-sync'), {
    params: { window: 'data-sync' },
    width: 1000,
    height: 700,
    minWidth: 600,
    minHeight: 480,
    title: t('win.dataSync'),
  });
}

export function openBackupWindow() {
  openWindow(nextLabel('backup'), {
    params: { window: 'backup' },
    width: 750,
    height: 520,
    minWidth: 600,
    minHeight: 400,
    title: t('win.backup'),
  });
}

const SETTINGS_LABEL = 'settings-singleton';

export function openSettingsWindow(section?: string) {
  if (isTauri()) {
    void (async () => {
      const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
      const existing = await WebviewWindow.getByLabel(SETTINGS_LABEL);
      if (existing) {
        await existing.setFocus();
        return;
      }
      const params: Record<string, string> = { window: 'settings' };
      if (section) params.section = section;
      await openTauriWindow(SETTINGS_LABEL, {
        params,
        title: t('win.settings'),
        width: 720,
        height: 560,
        minWidth: 560,
        minHeight: 400,
      });
    })();
  } else {
    const params: Record<string, string> = { window: 'settings' };
    if (section) params.section = section;
    openBrowserWindow({
      params,
      width: 720,
      height: 560,
      title: t('win.settings'),
    });
  }
}
