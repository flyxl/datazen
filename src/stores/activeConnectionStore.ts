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
  connectionId: string;
  configId: string;
  status: ConnectionStatus;
  serverInfo: ServerInfo | null;
  currentDatabase: string | null;
  error: string | null;
}

interface ActiveConnectionStore {
  /** All tracked connections, keyed by configId. */
  connections: Record<string, ConnectionEntry>;

  connect: (config: ConnectionConfig) => Promise<void>;
  /** Mark a connection as 'connecting' without triggering IPC. */
  markConnecting: (configId: string, database: string | null) => void;
  /** Mark a connection as 'connected' (called from cross-window event). */
  markConnected: (configId: string, connectionId: string) => void;
  /** Mark a connection as failed (called from cross-window event). */
  markError: (configId: string, error: string) => void;
  disconnect: (configId: string) => Promise<void>;
  removeByConnectionId: (connectionId: string) => void;
  reset: () => void;
}

export const useActiveConnectionStore = create<ActiveConnectionStore>((set, get) => ({
  connections: {},

  connect: async (config) => {
    const configId = config.id;
    console.log('[connect] starting', configId, config.name);

    set((s) => ({
      connections: {
        ...s.connections,
        [configId]: {
          connectionId: '',
          configId,
          status: 'connecting',
          serverInfo: null,
          currentDatabase: config.database ?? null,
          error: null,
        },
      },
    }));

    try {
      const connectionId = await connectionCommands.connect(configId);
      console.log('[connect] pool created', connectionId);

      const serverInfo = await connectionCommands.testConnection(config);
      console.log('[connect] server info', serverInfo);

      set((s) => ({
        connections: {
          ...s.connections,
          [configId]: {
            connectionId,
            configId,
            status: 'connected',
            serverInfo,
            currentDatabase: config.database ?? null,
            error: null,
          },
        },
      }));
      console.log('[connect] success', connectionId);
      void emitCrossWindow('datazen:connection-ready', { configId, connectionId });
    } catch (e) {
      const msg = extractError(e);
      console.error('[connect] failed', msg);
      set((s) => ({
        connections: {
          ...s.connections,
          [configId]: {
            ...s.connections[configId],
            connectionId: '',
            configId,
            status: 'error',
            error: msg,
          },
        },
      }));
      void emitCrossWindow('datazen:connection-failed', { configId, error: msg });
    }
  },

  markConnecting: (configId, database) => {
    set((s) => ({
      connections: {
        ...s.connections,
        [configId]: {
          connectionId: '',
          configId,
          status: 'connecting',
          serverInfo: null,
          currentDatabase: database,
          error: null,
        },
      },
    }));
  },

  markConnected: (configId, connectionId) => {
    set((s) => ({
      connections: {
        ...s.connections,
        [configId]: {
          ...s.connections[configId],
          connectionId,
          configId,
          status: 'connected',
          error: null,
        },
      },
    }));
  },

  markError: (configId, error) => {
    set((s) => ({
      connections: {
        ...s.connections,
        [configId]: {
          ...s.connections[configId],
          connectionId: '',
          configId,
          status: 'error',
          error,
        },
      },
    }));
  },

  disconnect: async (configId) => {
    const entry = get().connections[configId];
    const connectionId = entry?.connectionId;
    console.log('[disconnect]', configId, connectionId);

    if (!connectionId) {
      set((s) => {
        const { [configId]: _, ...rest } = s.connections;
        return { connections: rest };
      });
      return;
    }

    try {
      await connectionCommands.disconnect(connectionId);
      console.log('[disconnect] success');
      await emitCrossWindow('datazen:disconnect-requested', { connectionId });
    } catch (e) {
      console.error('[disconnect] failed', extractError(e));
    } finally {
      set((s) => {
        const { [configId]: _, ...rest } = s.connections;
        return { connections: rest };
      });
    }
  },

  removeByConnectionId: (connectionId) => {
    set((s) => {
      const next = { ...s.connections };
      for (const key of Object.keys(next)) {
        if (next[key].connectionId === connectionId) {
          delete next[key];
        }
      }
      return { connections: next };
    });
  },

  reset: () => set({ connections: {} }),
}));
