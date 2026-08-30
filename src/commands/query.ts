import { invoke } from '@tauri-apps/api/core';
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
    dbSessionId: string,
    sql: string,
    params?: Record<string, string | number | boolean | null>,
    database?: string | null,
    schema?: string | null,
  ) => {
    const result = await driverCommands.execute({
      dbSessionId,
      command: 'query',
      input: params && Object.keys(params).length > 0 ? { sql, params } : { sql },
      // F1: pin the session to the panel's selected database before running.
      database: database ?? null,
      // F7: PG-family schema target — rewrite-capable drivers inline it.
      schema: schema ?? null,
    });
    return result.data as MultiQueryResult;
  },

  executeQueryStream: async (
    dbSessionId: string,
    sql: string,
    onEvent: (event: QueryStreamEvent) => void,
    options?: {
      applyResultLimit?: boolean;
      recordHistory?: boolean;
      /** F1: pin the session to this database before streaming. */
      database?: string | null;
      /** F7: PG-family schema target for the stream. */
      schema?: string | null;
      /** Bound values are sent through the same cancellable stream path. */
      params?: Record<string, string | number | boolean | null>;
    },
  ) => {
    await driverCommands.executeStream({
      dbSessionId,
      command: 'query_stream',
      input:
        options?.params && Object.keys(options.params).length > 0
          ? { sql, params: options.params }
          : { sql },
      onEvent,
      applyResultLimit: options?.applyResultLimit,
      recordHistory: options?.recordHistory,
      database: options?.database ?? null,
      schema: options?.schema ?? null,
    });
  },

  /** `database` pins the session to a database for this explain (F1: no use_database IPC). */
  getExplain: (dbSessionId: string, sql: string, database?: string | null) =>
    invoke<ExplainResult>('get_explain', { dbSessionId, sql, database: database ?? null }),

  cancelQuery: (dbSessionId: string, executionId: string) =>
    invoke<void>('cancel_query', { dbSessionId, executionId }),

  /** connectionId = 持久化配置连接 id（历史按连接分组）。 */
  getQueryHistory: (limit: number, connectionId?: string, database?: string, schema?: string) =>
    invoke<QueryHistoryEntry[]>('get_query_history', { limit, connectionId, database, schema }),

  clearQueryHistory: () => invoke<void>('clear_query_history'),

  getFavoriteQueries: (connectionId?: string) =>
    invoke<FavoriteQuery[]>('get_favorite_queries', { connectionId }),

  addFavoriteQuery: (connectionId: string, title: string, sql: string) =>
    invoke<FavoriteQuery>('add_favorite_query', { connectionId, title, sql }),

  deleteFavoriteQuery: (id: string) => invoke<void>('delete_favorite_query', { id }),

  beginSessionTransaction: (dbSessionId: string) =>
    invoke<void>('begin_session_transaction', { dbSessionId }),

  commitSessionTransaction: (dbSessionId: string) =>
    invoke<void>('commit_session_transaction', { dbSessionId }),

  rollbackSessionTransaction: (dbSessionId: string) =>
    invoke<void>('rollback_session_transaction', { dbSessionId }),

  sessionTransactionStatus: (dbSessionId: string) =>
    invoke<boolean>('session_transaction_status', { dbSessionId }),
};
