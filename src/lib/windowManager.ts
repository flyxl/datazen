/**
 * The shared window surface (form-split F6 prep).
 *
 * The high-level named-window helpers (open*Window, emitMenuAction, the
 * pending-connection keys) are shared code: they reason over URL params,
 * cross-window events, localStorage and i18n, and reach the actual window
 * machinery through the `WindowRouter` port (`requirePlatform().windows`) —
 * never through the desktop zone. The F2 re-export shim to
 * `platform/desktop/windowRouter.ts` is gone; the low-level desktop
 * implementation (create_sub_window, WebviewWindow focus, the browser-tab
 * fallback) stays app-owned in the zone.
 */

import { IS_DESKTOP_BUILD } from '../platform/env';
import { requirePlatform } from '../platform/contracts';
import { emitCrossWindow } from './crossWindowBus';
import {
  buildMigrationWindowUrlParams,
  type MigrationWindowPrefill,
} from './migrationWindowPrefill';
import { openNewConnectionDialog as openConnectionEditorDialog } from './connectionEditor';
import { buildDocsUrl } from './docsUrls';
import { settingsCommands } from '../commands/settings';
import { t } from '../locales/t';
import { useSettingsStore } from '../stores/settingsStore';

export type { MigrationWindowPrefill };

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

/** True when any sub-window (backup, data-sync, …) is still open. */
export function hasOpenChildWindows(): Promise<boolean> {
  return requirePlatform().windows.hasOpenChildWindows();
}

export function openNewConnectionDialog(editId?: string, defaultGroup?: string) {
  void requirePlatform()
    .windows.focusMainWindow()
    .then(() => openConnectionEditorDialog(editId, defaultGroup));
}

// ── Singleton windows ───────────────────────────────────────────────

export function openDataSyncWindow(prefill?: MigrationWindowPrefill) {
  requirePlatform().windows.openSingletonWindow('data-sync-singleton', {
    params: buildMigrationWindowUrlParams('data-sync', prefill),
    width: 1000,
    height: 700,
    minWidth: 600,
    minHeight: 480,
    title: t('common.dataSyncTitle'),
  });
}

export function openDataTransferWindow(prefill?: MigrationWindowPrefill) {
  requirePlatform().windows.openSingletonWindow('data-transfer-singleton', {
    params: buildMigrationWindowUrlParams('data-transfer', prefill),
    width: 1000,
    height: 720,
    minWidth: 640,
    minHeight: 480,
    title: t('win.dataTransfer'),
  });
}

export function openSchemaDiffWindow(prefill?: MigrationWindowPrefill) {
  requirePlatform().windows.openSingletonWindow('schema-diff-singleton', {
    params: buildMigrationWindowUrlParams('schema-diff', prefill),
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
  requirePlatform().windows.openSingletonWindow(
    restore ? 'backup-restore-singleton' : 'backup-singleton',
    {
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
    },
  );
}

export function openWorkflowWindow() {
  void requirePlatform()
    .windows.focusMainWindow()
    .then(() => emitCrossWindow('menu:workflow'));
}

export async function emitMenuAction(id: string, payload?: unknown): Promise<void> {
  if (MAIN_SHELL_MENU_IDS.has(id)) {
    await requirePlatform().windows.focusMainWindow();
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
  if (IS_DESKTOP_BUILD) {
    void settingsCommands.openPath(url).catch(() => {
      window.open(url, '_blank', 'noopener');
    });
  } else {
    window.open(url, '_blank', 'noopener');
  }
}

// ── Multi-instance windows ──────────────────────────────────────────

export function openDashboardWindow(dashboardId?: string, dashboardName?: string) {
  void requirePlatform()
    .windows.focusMainWindow()
    .then(() => {
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
 * Key used by the onboarding wizard to signal ConnectionPage to auto-open
 * the sample connection and pre-fill a query panel with a preset SQL statement.
 * Consumed once on mount and immediately removed.
 */
export const POST_ONBOARDING_SAMPLE_KEY = 'datazen:post-onboarding-sample';

export interface PostOnboardingSample {
  connectionName: string;
  sql: string;
}

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
  void requirePlatform().windows.focusMainWindow();
}

// Re-export the open-options shape for callers that build custom windows
// through the port directly (kept from the F2 surface).
export type { OpenWindowOptions } from '../platform/contracts';
