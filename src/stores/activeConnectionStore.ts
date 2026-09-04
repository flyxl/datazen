import { create } from 'zustand';
import { connectionCommands } from '../commands/connection';
import { emitCrossWindow } from '../lib/crossWindowBus';
import { t } from '../locales/t';
import type { ConnectionConfig, DriverCapabilities, ServerInfo } from '../types';

export type ConnectionStatus = 'idle' | 'connecting' | 'connected' | 'error';

function extractError(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return t('backend.unknownError');
}

export interface ConnectionEntry {
  /** Runtime database session id (empty while connecting/errored). */
  dbSessionId: string;
  /** Persistent connection id this entry was opened from (map key). */
  connectionId: string;
  status: ConnectionStatus;
  serverInfo: ServerInfo | null;
  /** Runtime driver capabilities; undefined while the session info is unknown. */
  capabilities?: DriverCapabilities;
  currentDatabase: string | null;
  error: string | null;
}

interface ActiveConnectionStore {
  /** All tracked connections, keyed by connectionId (persistent config connection id). */
  connections: Record<string, ConnectionEntry>;

  connect: (config: ConnectionConfig) => Promise<void>;
  /** Mark a connection as 'connecting' without triggering IPC. */
  markConnecting: (connectionId: string, database: string | null) => void;
  /** Mark a connection as 'connected' (called from cross-window event). */
  markConnected: (connectionId: string, dbSessionId: string) => void;
  /** Mark a connection as failed (called from cross-window event). */
  markError: (connectionId: string, error: string) => void;
  disconnect: (connectionId: string) => Promise<void>;
  removeByDbSessionId: (dbSessionId: string) => void;
  reset: () => void;
}

export const useActiveConnectionStore = create<ActiveConnectionStore>((set, get) => ({
  connections: {},

  connect: async (config) => {
    const connectionId = config.id;
    if (import.meta.env.DEV) {
      console.log('[connect] starting', connectionId, config.name);
    }

    set((s) => ({
      connections: {
        ...s.connections,
        [connectionId]: {
          dbSessionId: '',
          connectionId,
          status: 'connecting',
          serverInfo: null,
          capabilities: undefined,
          currentDatabase: config.database ?? null,
          error: null,
        },
      },
    }));

    try {
      const dbSessionId = await connectionCommands.connect(connectionId);
      if (import.meta.env.DEV) {
        console.log('[connect] pool created', dbSessionId);
      }

      const serverInfo = await connectionCommands.testConnection(config);
      if (import.meta.env.DEV) {
        console.log('[connect] server info', serverInfo);
      }

      // Capability discovery must not turn a successful database connection
      // into a failed connection. Older/headless backends may not return the
      // optional field, in which case the UI treats cancellation as unknown.
      let capabilities: DriverCapabilities | undefined;
      try {
        capabilities =
          (await connectionCommands.getConnectionInfo(dbSessionId)).capabilities ?? undefined;
      } catch (e) {
        console.warn('[connect] capability discovery failed', e);
      }

      set((s) => ({
        connections: {
          ...s.connections,
          [connectionId]: {
            dbSessionId,
            connectionId,
            status: 'connected',
            serverInfo,
            capabilities,
            currentDatabase: config.database ?? null,
            error: null,
          },
        },
      }));
      if (import.meta.env.DEV) {
        console.log('[connect] success', dbSessionId);
      }
      void emitCrossWindow('datazen:connection-ready', { connectionId, dbSessionId });
    } catch (e) {
      const msg = extractError(e);
      console.error('[connect] failed', msg);
      set((s) => ({
        connections: {
          ...s.connections,
          [connectionId]: {
            ...s.connections[connectionId],
            dbSessionId: '',
            connectionId,
            status: 'error',
            error: msg,
          },
        },
      }));
      void emitCrossWindow('datazen:connection-failed', { connectionId, error: msg });
    }
  },

  markConnecting: (connectionId, database) => {
    set((s) => ({
      connections: {
        ...s.connections,
        [connectionId]: {
          dbSessionId: '',
          connectionId,
          status: 'connecting',
          serverInfo: null,
          capabilities: undefined,
          currentDatabase: database,
          error: null,
        },
      },
    }));
    setTimeout(() => {
      const entry = get().connections[connectionId];
      if (entry?.status === 'connecting') {
        set((s) => ({
          connections: {
            ...s.connections,
            [connectionId]: {
              ...s.connections[connectionId],
              status: 'error',
              error: 'Connection timeout',
            },
          },
        }));
      }
    }, 30_000);
  },

  markConnected: (connectionId, dbSessionId) => {
    set((s) => ({
      connections: {
        ...s.connections,
        [connectionId]: {
          ...s.connections[connectionId],
          dbSessionId,
          connectionId,
          status: 'connected',
          // A direct ConnectionPage connect reaches this action without the
          // store's `connect` flow. Start capability discovery here too, but
          // keep the value unknown until the session-info IPC resolves.
          capabilities: undefined,
          error: null,
        },
      },
    }));

    void Promise.resolve(connectionCommands.getConnectionInfo(dbSessionId))
      .then((info) => {
        if (!info) return;
        set((s) => {
          const entry = s.connections[connectionId];
          if (!entry || entry.dbSessionId !== dbSessionId) return s;
          return {
            connections: {
              ...s.connections,
              [connectionId]: {
                ...entry,
                capabilities: info.capabilities ?? undefined,
              },
            },
          };
        });
      })
      .catch((error: unknown) => {
        // Capability discovery is advisory. A legacy/headless backend must
        // leave cancellation in the safe `unknown` state.
        console.warn('[connect] capability discovery failed', error);
      });
  },

  markError: (connectionId, error) => {
    set((s) => ({
      connections: {
        ...s.connections,
        [connectionId]: {
          ...s.connections[connectionId],
          dbSessionId: '',
          connectionId,
          status: 'error',
          error,
        },
      },
    }));
  },

  disconnect: async (connectionId) => {
    const entry = get().connections[connectionId];
    const dbSessionId = entry?.dbSessionId;
    if (import.meta.env.DEV) {
      console.log('[disconnect]', connectionId, dbSessionId);
    }

    if (!dbSessionId) {
      set((s) => {
        const { [connectionId]: _, ...rest } = s.connections;
        return { connections: rest };
      });
      return;
    }

    try {
      await connectionCommands.disconnect(dbSessionId);
      if (import.meta.env.DEV) {
        console.log('[disconnect] success');
      }
      await emitCrossWindow('datazen:disconnect-requested', { dbSessionId });
    } catch (e) {
      console.error('[disconnect] failed', extractError(e));
    } finally {
      set((s) => {
        const { [connectionId]: _, ...rest } = s.connections;
        return { connections: rest };
      });
    }
  },

  removeByDbSessionId: (dbSessionId) => {
    set((s) => {
      const next = { ...s.connections };
      for (const key of Object.keys(next)) {
        if (next[key].dbSessionId === dbSessionId) {
          delete next[key];
        }
      }
      return { connections: next };
    });
  },

  reset: () => set({ connections: {} }),
}));
