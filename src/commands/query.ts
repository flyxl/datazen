import { Channel, invoke } from '@tauri-apps/api/core';
import type {
  ExplainResult,
  FavoriteQuery,
  MultiQueryResult,
  QueryHistoryEntry,
  QueryStreamEvent,
} from '../types';
import { driverCommands } from './driver';

export const queryCommands = {
  executeQuery: async (
    connectionId: string,
    sql: string,
    params?: Record<string, string | number | boolean | null>,
  ) => {
    const result = await driverCommands.execute({
      connectionId,
      command: 'query',
      input: params && Object.keys(params).length > 0 ? { sql, params } : { sql },
    });
    return result.data as MultiQueryResult;
  },

  executeQueryStream: async (
    connectionId: string,
    sql: string,
    onEvent: (event: QueryStreamEvent) => void,
    options?: { applyResultLimit?: boolean; recordHistory?: boolean },
  ) => {
    const onEventChannel = new Channel<QueryStreamEvent>();
    onEventChannel.onmessage = onEvent;
    await invoke<void>('execute_query_stream', {
      connectionId,
      sql,
      onEvent: onEventChannel,
      applyResultLimit: options?.applyResultLimit,
      recordHistory: options?.recordHistory,
    });
  },

  getExplain: (connectionId: string, sql: string) =>
    invoke<ExplainResult>('get_explain', { connectionId, sql }),

  cancelQuery: (connectionId: string) => invoke<void>('cancel_query', { connectionId }),

  getQueryHistory: (limit: number) => invoke<QueryHistoryEntry[]>('get_query_history', { limit }),

  clearQueryHistory: () => invoke<void>('clear_query_history'),

  getFavoriteQueries: () => invoke<FavoriteQuery[]>('get_favorite_queries'),

  addFavoriteQuery: (title: string, sql: string) =>
    invoke<FavoriteQuery>('add_favorite_query', { title, sql }),

  deleteFavoriteQuery: (id: string) => invoke<void>('delete_favorite_query', { id }),

  beginSessionTransaction: (connectionId: string) =>
    invoke<void>('begin_session_transaction', { connectionId }),

  commitSessionTransaction: (connectionId: string) =>
    invoke<void>('commit_session_transaction', { connectionId }),

  rollbackSessionTransaction: (connectionId: string) =>
    invoke<void>('rollback_session_transaction', { connectionId }),

  sessionTransactionStatus: (connectionId: string) =>
    invoke<boolean>('session_transaction_status', { connectionId }),
};
