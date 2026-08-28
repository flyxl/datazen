/**
 * Multi-window manager.
 *
 * In Tauri runtime, creates real OS windows via Rust-side command
 * to ensure `accept_first_mouse` is applied at the native layer.
 * In browser dev mode, opens new browser tabs with query params.
 */

import { invoke } from '@tauri-apps/api/core';
import { settingsCommands } from '../commands/settings';
import { t } from '../locales/t';
import { useSettingsStore } from '../stores/settingsStore';
import { buildDocsUrl } from './docsUrls';
import { emitCrossWindow } from './crossWindowBus';
import { openNewConnectionDialog as openConnectionEditorDialog } from './connectionEditor';

/**
 * Representative window labels that must match
 * `src-tauri/capabilities/default.json` → `windows` globs.
 * Keep in sync when adding open*Window helpers.
 */
export const WINDOW_CAPABILITY_LABEL_SAMPLES = [
  'main',
  'data-sync-singleton',
  'data-transfer-singleton',
  'schema-diff-singleton',
  'backup-singleton',
  'backup-restore-singleton',
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

/** Menu item ids whose handlers live in the main workspace shell. */
const MAIN_SHELL_MENU_IDS = new Set([
  'open-settings',
  'new-connection',
  'workflow',
  'dashboard',
  'export-config',
  'import-config',
  'export-connections',
  'import-connections-file',
  'import-connections-dbx',
  'import-connections-navicat',
  'import-connections-datagrip',
  'import-connections-dbeaver',
  'import-connections-tableplus',
]);

async function focusMainWindow(): Promise<void> {
  if (!isTauri()) return;
  const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
  const main = await WebviewWindow.getByLabel('main');
  if (!main) return;
  await main.show();
  await main.unminimize();
  await main.setFocus();
}

/** True when any sub-window (backup, data-sync, …) is still open. */
export async function hasOpenChildWindows(): Promise<boolean> {
  if (!isTauri()) return false;
  const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
  const windows = await WebviewWindow.getAll();
  return windows.some((window) => window.label !== 'main');
}

/** Sub-window HTML entry — no splash (see `window.html`). Main stays on `index.html`. */
const SUB_WINDOW_ENTRY = 'window.html';

import { cssColorToHex } from './surfaceBgCache';

/** Coalesce concurrent singleton opens (duplicate menu listeners / double-click). */
const singletonOpenInFlight = new Map<string, Promise<void>>();

function resolveOpenerBackgroundColor(): string | undefined {
  if (typeof document === 'undefined') return undefined;
  const inline = document.documentElement.style.backgroundColor;
  const fromInline = inline ? cssColorToHex(inline) : null;
  if (fromInline) return fromInline;
  const computed = getComputedStyle(document.documentElement).backgroundColor;
  return cssColorToHex(computed) ?? undefined;
}

async function openTauriWindow(label: string, options: OpenWindowOptions) {
  const qs = new URLSearchParams(options.params ?? {}).toString();
  const url = qs ? `${SUB_WINDOW_ENTRY}?${qs}` : SUB_WINDOW_ENTRY;
  const backgroundColor = resolveOpenerBackgroundColor();

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
        ...(backgroundColor ? { backgroundColor } : {}),
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

function openSingletonWindow(label: string, options: OpenWindowOptions) {
  const inflight = singletonOpenInFlight.get(label);
  if (inflight) {
    void inflight.catch(() => {});
    return;
  }

  const task = (async () => {
    if (isTauri()) {
      await openTauriWindow(label, options);
    } else {
      openBrowserWindow(options);
    }
  })().finally(() => {
    singletonOpenInFlight.delete(label);
  });

  singletonOpenInFlight.set(label, task);
  void task.catch(() => {});
}

// ── In-app dialogs (main window) ────────────────────────────────────

/** Open the new/edit connection dialog in the main window. */
export function openNewConnectionDialog(editId?: string) {
  void focusMainWindow().then(() => openConnectionEditorDialog(editId));
}

// ── Singleton windows ───────────────────────────────────────────────

export function openDataSyncWindow() {
  openSingletonWindow('data-sync-singleton', {
    params: { window: 'data-sync' },
    width: 1000,
    height: 700,
    minWidth: 600,
    minHeight: 480,
    title: t('common.dataSyncTitle'),
  });
}

export function openDataTransferWindow() {
  openSingletonWindow('data-transfer-singleton', {
    params: { window: 'data-transfer' },
    width: 1000,
    height: 720,
    minWidth: 640,
    minHeight: 480,
    title: t('win.dataTransfer'),
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

export function openBackupWindow(
  mode: 'backup' | 'restore' = 'backup',
  prefill?: { connectionId?: string; database?: string },
) {
  const restore = mode === 'restore';
  // URL param `connectionId` = persistent config connection id.
  openSingletonWindow(restore ? 'backup-restore-singleton' : 'backup-singleton', {
    params: {
      window: 'backup',
      ...(restore ? { mode: 'restore' } : {}),
      ...(prefill?.connectionId ? { connectionId: prefill.connectionId } : {}),
      ...(prefill?.database ? { database: prefill.database } : {}),
    },
    width: 750,
    height: 520,
    minWidth: 600,
    minHeight: 400,
    title: restore ? t('win.restore') : t('win.backup'),
  });
}

export function openWorkflowWindow() {
  void focusMainWindow().then(() => emitCrossWindow('menu:workflow'));
}

export async function emitMenuAction(id: string, payload?: unknown): Promise<void> {
  if (MAIN_SHELL_MENU_IDS.has(id)) {
    await focusMainWindow();
  }
  await emitCrossWindow(`menu:${id}`, payload);
}

export function openSettingsWindow(section?: string) {
  void emitMenuAction('open-settings', section ? { section } : undefined);
}

/** Open official help docs in the system browser (GitHub Pages). */
export function openDocsWindow(section?: string) {
  const language = useSettingsStore.getState().settings.language;
  const url = buildDocsUrl(language, section);
  if (isTauri()) {
    void settingsCommands.openPath(url).catch(() => {
      window.open(url, '_blank', 'noopener');
    });
  } else {
    window.open(url, '_blank', 'noopener');
  }
}

// ── Multi-instance windows ──────────────────────────────────────────

export function openDashboardWindow(dashboardId?: string, dashboardName?: string) {
  void focusMainWindow().then(() => {
    if (dashboardId) {
      void emitCrossWindow('menu:open-dashboard', { dashboardId, dashboardName });
      return;
    }
    void emitCrossWindow('menu:dashboard');
  });
}

/**
 * Key used by {@link openConnectionWindow} to hand the first connection
 * payload to the main workspace ConnectionPage via `localStorage`.
 * Subsequent connections use the `datazen:open-connection` cross-window event.
 */
export const PENDING_CONNECTION_KEY = 'datazen:pending-connection';

/**
 * Open the main workspace and add a connection tab.
 *
 * Connection-specific params are NOT included in the URL (only `window=connection`)
 * so that the Rust-side `focus_existing_window` never re-navigates the webview
 * when a second connection is opened.
 *
 * Instead, the connection payload is delivered via:
 * 1. `localStorage` — read on first mount (handles the initial window creation race)
 * 2. `datazen:open-connection` event — handled by the existing listener
 */
export function openConnectionWindow(
  opts: { dbSessionId?: string; connectionId?: string },
  connectionName: string,
  database?: string,
  databaseType?: string,
  action?: string,
) {
  // Cross-window payload keys follow the W3 convention:
  // `connectionId` = persistent config connection id, `dbSessionId` = runtime session id.
  const payload: Record<string, string> = { connectionName };
  if (opts.dbSessionId) payload.dbSessionId = opts.dbSessionId;
  if (opts.connectionId) payload.connectionId = opts.connectionId;
  if (database) payload.database = database;
  if (databaseType) payload.databaseType = databaseType;
  if (action) payload.action = action;

  try {
    localStorage.setItem(PENDING_CONNECTION_KEY, JSON.stringify(payload));
  } catch {
    // localStorage unavailable; event-only fallback
  }

  void emitCrossWindow('datazen:open-connection', payload);
  void focusMainWindow();
}
