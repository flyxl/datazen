import { create } from 'zustand';
import { connectionCommands } from '../commands/connection';
import { emitCrossWindow } from '../lib/crossWindowBus';
import { t } from '../locales/t';
import type { ConnectionConfig, ServerInfo } from '../types';

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
    console.log('[connect] starting', connectionId, config.name);

    set((s) => ({
      connections: {
        ...s.connections,
        [connectionId]: {
          dbSessionId: '',
          connectionId,
          status: 'connecting',
          serverInfo: null,
          currentDatabase: config.database ?? null,
          error: null,
        },
      },
    }));

    try {
      const dbSessionId = await connectionCommands.connect(connectionId);
      console.log('[connect] pool created', dbSessionId);

      const serverInfo = await connectionCommands.testConnection(config);
      console.log('[connect] server info', serverInfo);

      set((s) => ({
        connections: {
          ...s.connections,
          [connectionId]: {
            dbSessionId,
            connectionId,
            status: 'connected',
            serverInfo,
            currentDatabase: config.database ?? null,
            error: null,
          },
        },
      }));
      console.log('[connect] success', dbSessionId);
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
          error: null,
        },
      },
    }));
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
    console.log('[disconnect]', connectionId, dbSessionId);

    if (!dbSessionId) {
      set((s) => {
        const { [connectionId]: _, ...rest } = s.connections;
        return { connections: rest };
      });
      return;
    }

    try {
      await connectionCommands.disconnect(dbSessionId);
      console.log('[disconnect] success');
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
