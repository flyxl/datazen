import { legacyInvoke as invoke } from '@datazen/backend-client';
import { toIpcConnectionConfig } from '../lib/connectionConfig';
import type { ConnectionConfig, DriverCapabilities, ServerInfo } from '../types';

/**
 * Self-describing result of `connect` / `connect_dedicated` (mirrors the Rust
 * `ConnectionSessionRef`). `connectionId` is the persistent connection-config
 * id (survives restart); `dbSessionId` is the runtime database session id
 * (in-memory only, never persisted). See docs/architecture/naming.md.
 */
export interface ConnectionSessionRef {
  connectionId: string;
  dbSessionId: string;
}

async function waitForE2EConnectDelay(connectionId: string): Promise<void> {
  if (!import.meta.env.VITE_E2E || typeof window === 'undefined') return;
  const delays = (
    window as Window & {
      __DATAZEN_E2E_CONNECT_DELAY_MS__?: Record<string, number>;
    }
  ).__DATAZEN_E2E_CONNECT_DELAY_MS__;
  const delayMs = delays?.[connectionId] ?? 0;
  if (!Number.isFinite(delayMs) || delayMs <= 0) return;
  await new Promise<void>((resolve) => window.setTimeout(resolve, delayMs));
}

/**
 * Connects in flight, keyed by connectionId. The main window reaches one
 * connection's session through two independent front-end paths: the tab
 * (`useConnectionTabs.connectTab`, opened by selection/double-click) and the
 * store (`activeConnectionStore.connect`, e.g. the navigator's context-menu
 * "open" / "query history" on a not-yet-open connection). Both funnel through
 * `connectionCommands.connect`, so joining concurrent callers to the first
 * in-flight call HERE is the single guarantee of one backend session per
 * connection, no matter which UI entry point fired first. Without it the two
 * paths each issue `invoke('connect')`, the backend retires the first session,
 * and every reader of it fails with `sessionNotFound` (empty schema tree, no
 * visible error). The store-level dedup cannot see the tab path, and vice
 * versa; only this shared boundary covers both.
 *
 * `connectDedicated` is deliberately NOT deduped: sub-windows intentionally
 * open additional, isolated sessions.
 */
const inFlightConnects = new Map<string, Promise<ConnectionSessionRef>>();

export const connectionCommands = {
  getConnections: () => invoke<ConnectionConfig[]>('get_connections'),

  saveConnection: (config: ConnectionConfig, clearMissingCredentials = false) =>
    invoke<void>('save_connection', {
      config: toIpcConnectionConfig(config),
      clearMissingCredentials,
    }),

  duplicateConnection: (sourceId: string, newId: string, newName: string) =>
    invoke<void>('duplicate_connection', { sourceId, newId, newName }),

  deleteConnection: (id: string) => invoke<void>('delete_connection', { id }),

  reorderConnections: (orderedIds: string[]) => invoke<void>('reorder_connections', { orderedIds }),

  testConnection: (config: ConnectionConfig) =>
    invoke<ServerInfo>('test_connection', { config: toIpcConnectionConfig(config) }),

  /**
   * connectionId = 持久化配置连接 id；返回值携带 connectionId + 运行时 dbSessionId。
   * Concurrent calls for the same connectionId join the first in-flight call
   * (see `inFlightConnects`); a settled call frees the slot for the next one.
   */
  connect: (connectionId: string): Promise<ConnectionSessionRef> => {
    const running = inFlightConnects.get(connectionId);
    if (running) return running;
    const attempt = (async () => {
      await waitForE2EConnectDelay(connectionId);
      return invoke<ConnectionSessionRef>('connect', { connectionId });
    })().finally(() => {
      inFlightConnects.delete(connectionId);
    });
    inFlightConnects.set(connectionId, attempt);
    return attempt;
  },

  /** Sub-window only: always opens a new db session, optionally pinned to `database`. */
  connectDedicated: (connectionId: string, database?: string): Promise<ConnectionSessionRef> =>
    invoke<ConnectionSessionRef>('connect_dedicated', {
      connectionId,
      database: database ?? null,
    }),

  pingConnection: (dbSessionId: string) => invoke<boolean>('ping_connection', { dbSessionId }),

  releaseConnection: (dbSessionId: string) =>
    invoke<boolean>('release_connection', { dbSessionId }),

  disconnect: (dbSessionId: string) => invoke<void>('disconnect', { dbSessionId }),

  /**
   * Releases one database's backend resources without closing the session.
   * The session stays connected; a later read of that database re-opens a pool
   * on demand. Returns whether anything was actually released (drivers without
   * per-database resources answer `false`).
   */
  closeDatabase: (dbSessionId: string, database: string) =>
    invoke<boolean>('close_database', { dbSessionId, database }),

  /**
   * Databases the session currently holds an open resource for. Drives the
   * navigator's "open" marker. Drivers without per-database resources answer
   * an empty list.
   */
  getOpenDatabases: (dbSessionId: string) =>
    invoke<string[]>('get_open_databases', { dbSessionId }),

  getConnectionInfo: (dbSessionId: string) =>
    invoke<{
      databaseType: string;
      driverCategory: string;
      name: string;
      host?: string;
      port?: number;
      database?: string;
      schema?: string;
      serverVersion?: string;
      /** Null/absent means the backend or legacy driver did not advertise it. */
      capabilities?: DriverCapabilities | null;
    }>('get_connection_info', { dbSessionId }),

  getAvailableDrivers: () => invoke<string[]>('get_available_drivers'),

  getGroups: () => invoke<string[]>('get_groups'),

  saveGroups: (groups: string[]) => invoke<void>('save_groups', { groups }),

  /**
   * Decision 3 merged IPC: native save dialog + encrypted export.
   * The wire-level `override_path` escape hatch is webdriver/E2E-only and is
   * never sent from production code. Returns the connection count, or null
   * when the dialog was dismissed.
   */
  exportConnections: (password: string, defaultFileName: string) =>
    invoke<number | null>('export_connections', { password, defaultFileName }),

  importConnectionsPreview: (password: string) =>
    invoke<{
      connections: ConnectionConfig[];
      groups: string[];
    } | null>('import_connections_preview', { password }),

  /**
   * Legacy merged IPC (password then file). Prefer pickConnectionsImportFile +
   * importConnectionsAtPath for the TablePlus-style file-first flow.
   */
  importConnections: (password: string) =>
    invoke<{
      imported: number;
      overwritten: number;
      groupsAdded: number;
      skipped?: string[];
      sourceFormat?: string;
    } | null>('import_connections_with_dialog', { password }),

  /** Native open dialog only; returns the picked path or null if cancelled.
   * Form-split F4c: the three import pickers merged into one host command —
   * the filter table stays host-side, the webview only names the kind. */
  pickConnectionsImportFile: () => invoke<string | null>('pick_import_file', { kind: 'connections' }),

  /** Import after the file has already been chosen in the UI. */
  importConnectionsAtPath: (password: string, path: string) =>
    invoke<{
      imported: number;
      overwritten: number;
      groupsAdded: number;
      skipped?: string[];
      sourceFormat?: string;
    }>('import_connections_at_path', { password, path }),

  detectConnectionImportPath: (source: string) =>
    invoke<{ path: string; found: boolean }>('detect_connection_import_path', { source }),

  pickConnectionImportPathWithDialog: (mode: 'file' | 'folder', source: string) =>
    invoke<string | null>('pick_import_file', { kind: 'competitor-path', mode, source }),

  importConnectionsFromApp: (source: string, password: string, dataPath: string) =>
    invoke<{
      imported: number;
      overwritten: number;
      groupsAdded: number;
      skipped?: string[];
      sourceFormat?: string;
    }>('import_connections_from_app', { source, password, dataPath }),
};
