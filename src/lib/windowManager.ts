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

/**
 * Representative window labels that must match
 * `src-tauri/capabilities/default.json` → `windows` globs.
 * Keep in sync when adding open*Window helpers.
 */
export const WINDOW_CAPABILITY_LABEL_SAMPLES = [
  'main',
  'new-connection-singleton',
  'data-sync-singleton',
  'schema-diff-singleton',
  'backup-singleton',
  'workflow-singleton',
  'settings-singleton',
  'docs-singleton',
  'connection-0-1',
  'dashboard-sample-id',
] as const;

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

/** Sub-window HTML entry — no splash (see `window.html`). Main stays on `index.html`. */
const SUB_WINDOW_ENTRY = 'window.html';

async function openTauriWindow(label: string, options: OpenWindowOptions) {
  const qs = new URLSearchParams(options.params ?? {}).toString();
  const url = qs ? `${SUB_WINDOW_ENTRY}?${qs}` : SUB_WINDOW_ENTRY;

  try {
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
  } catch (e) {
    console.error(`[windowManager] failed to open window "${label}"`, e);
    // A rejected invoke (permission denied, build failure) is otherwise
    // indistinguishable from "the click did nothing", and devtools are not
    // available in release builds.
    try {
      const { message } = await import('@tauri-apps/plugin-dialog');
      await message(`${label}\n\n${e instanceof Error ? e.message : String(e)}`, {
        title: 'DataZen',
        kind: 'error',
      });
    } catch {
      // dialog unavailable; the console log above is the only trace
    }
    throw e;
  }
}

function openBrowserWindow(options: OpenWindowOptions) {
  const qs = new URLSearchParams(options.params ?? {}).toString();
  const url = qs ? `/window.html?${qs}` : '/window.html';
  window.open(url, '_blank', `width=${options.width ?? 800},height=${options.height ?? 640}`);
}

function openWindow(label: string, options: OpenWindowOptions) {
  if (isTauri()) {
    void openTauriWindow(label, options).catch(() => {});
  } else {
    openBrowserWindow(options);
  }
}

/**
 * Open a singleton window.
 *
 * Reuse is handled in Rust: `create_sub_window` shows and focuses an
 * existing window with the same label. Resolving it here instead would
 * only focus the window, which silently does nothing when the window
 * exists but never became visible.
 *
 * Native Help → Documentation opens docs from Rust (`open_docs_window`) so
 * it never fans out through every webview. In-app buttons still use this
 * helper via `invoke('create_sub_window')`.
 */
function openSingletonWindow(label: string, options: OpenWindowOptions) {
  openWindow(label, options);
}

// ── Singleton windows ───────────────────────────────────────────────

export function openNewConnectionWindow(editId?: string) {
  const params: Record<string, string> = { window: 'new-connection' };
  if (editId) params.editId = editId;

  openSingletonWindow('new-connection-singleton', {
    params,
    width: 800,
    height: 680,
    minWidth: 600,
    minHeight: 480,
    title: editId ? t('win.editConnection') : t('win.newConnection'),
  });
}

export function openDataSyncWindow() {
  openSingletonWindow('data-sync-singleton', {
    params: { window: 'data-sync' },
    width: 1000,
    height: 700,
    minWidth: 600,
    minHeight: 480,
    title: t('win.dataSync'),
  });
}

export function openSchemaDiffWindow() {
  openSingletonWindow('schema-diff-singleton', {
    params: { window: 'schema-diff' },
    width: 900,
    height: 640,
    minWidth: 560,
    minHeight: 420,
    title: t('win.schemaDiff'),
  });
}

export function openBackupWindow() {
  openSingletonWindow('backup-singleton', {
    params: { window: 'backup' },
    width: 750,
    height: 520,
    minWidth: 600,
    minHeight: 400,
    title: t('win.backup'),
  });
}

export function openWorkflowWindow() {
  openSingletonWindow('workflow-singleton', {
    params: { window: 'workflow' },
    width: 1100,
    height: 750,
    minWidth: 700,
    minHeight: 500,
    title: t('win.workflow'),
  });
}

export function openSettingsWindow(section?: string) {
  const params: Record<string, string> = { window: 'settings' };
  if (section) params.section = section;

  openSingletonWindow('settings-singleton', {
    params,
    title: t('win.settings'),
    width: 720,
    height: 560,
    minWidth: 560,
    minHeight: 400,
  });
}

export function openDocsWindow(section?: string) {
  const params: Record<string, string> = { window: 'docs' };
  if (section) params.section = section;

  openSingletonWindow('docs-singleton', {
    params,
    title: t('win.docs'),
    width: 920,
    height: 680,
    minWidth: 640,
    minHeight: 480,
  });
}

// ── Multi-instance windows ──────────────────────────────────────────

export function openDashboardWindow(dashboardId?: string, dashboardName?: string) {
  const params: Record<string, string> = { window: 'dashboard' };
  if (dashboardId) params.dashboardId = dashboardId;
  openWindow('dashboard-main', {
    params,
    width: 1200,
    height: 800,
    minWidth: 800,
    minHeight: 500,
    title: dashboardName ? `${dashboardName} - DataZen` : t('win.dashboard'),
  });
}

export function openConnectionWindow(
  opts: { connectionId?: string; configId?: string },
  connectionName: string,
  database?: string,
  databaseType?: string,
) {
  const params: Record<string, string> = { window: 'connection', connectionName };
  if (opts.connectionId) params.connectionId = opts.connectionId;
  if (opts.configId) params.configId = opts.configId;
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
