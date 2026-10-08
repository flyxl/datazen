import { invoke } from '@tauri-apps/api/core';
import type {
  ExplainResult,
  FavoriteQuery,
  HistorySort,
  MultiQueryResult,
  QueryHistoryEntry,
  QueryHistoryPage,
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
      // The statement's target. The driver never issues `USE`: it either
      // inlines the qualifier (MySQL family) or routes to that database's pool
      // (PostgreSQL family), so the session is never re-pointed.
      database: database ?? null,
      // PG-family schema target — inlined into relation names by the driver.
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
      /** Database this stream reads from. Never a session `USE`. */
      database?: string | null;
      /** PG-family schema target for the stream. */
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

  /** `database` is the explain's target; no session switch is involved. */
  getExplain: (dbSessionId: string, sql: string, database?: string | null) =>
    invoke<ExplainResult>('get_explain', { dbSessionId, sql, database: database ?? null }),

  cancelQuery: (dbSessionId: string, executionId: string) =>
    invoke<void>('cancel_query', { dbSessionId, executionId }),

  /** connectionId = 持久化配置连接 id（历史按连接分组）。 */
  getQueryHistory: (limit: number, connectionId?: string, database?: string, schema?: string) =>
    invoke<QueryHistoryEntry[]>('get_query_history', { limit, connectionId, database, schema }),

  clearQueryHistory: () => invoke<void>('clear_query_history'),

  /**
   * Paged history with an honest count.
   *
   * `getQueryHistory` cannot express search or a time range, and a caller that
   * filters its `limit`-ed array in JS silently loses anything past the limit.
   * Here the backend applies every predicate *before* the page is cut and
   * reports `total` for the whole match set, so the UI can say "N of M" rather
   * than imply the page is everything.
   */
  getQueryHistoryPage: (opts: {
    limit: number;
    connectionId?: string | null;
    database?: string | null;
    schema?: string | null;
    search?: string | null;
    since?: string | null;
    order?: HistorySort;
  }) =>
    invoke<QueryHistoryPage>('get_query_history_page', {
      limit: opts.limit,
      connectionId: opts.connectionId ?? null,
      database: opts.database ?? null,
      schema: opts.schema ?? null,
      search: opts.search ?? null,
      since: opts.since ?? null,
      order: opts.order ?? 'recent',
    }),

  /** Removes one row. Resolves to 0 when the id was already gone. */
  deleteQueryHistoryEntry: (id: string) => invoke<number>('delete_query_history', { id }),

  /** Ask for a save path, then write `content` to it. Resolves false if cancelled. */
  saveSqlFile: (defaultFileName: string, content: string) =>
    invoke<boolean>('save_sql_file', { defaultFileName, content }),

  getFavoriteQueries: (connectionId?: string) =>
    invoke<FavoriteQuery[]>('get_favorite_queries', { connectionId }),

  addFavoriteQuery: (connectionId: string, title: string, sql: string) =>
    invoke<FavoriteQuery>('add_favorite_query', { connectionId, title, sql }),

  deleteFavoriteQuery: (id: string) => invoke<void>('delete_favorite_query', { id }),

  /**
   * Resolved favorites directory, default or configured. The panel shows this
   * so the user knows which folder to point a sync service at.
   */
  getFavoritesRoot: () => invoke<string>('get_favorites_root'),

  /**
   * Re-scan the directory, dropping the backend cache first. The plain
   * `getFavoriteQueries` is the cached fast path and will not see a `.sql` file
   * that a sync client wrote while the app was running.
   */
  refreshFavorites: (connectionId?: string) =>
    invoke<FavoriteQuery[]>('refresh_favorites', { connectionId }),

  beginSessionTransaction: (dbSessionId: string) =>
    invoke<void>('begin_session_transaction', { dbSessionId }),

  commitSessionTransaction: (dbSessionId: string) =>
    invoke<void>('commit_session_transaction', { dbSessionId }),

  rollbackSessionTransaction: (dbSessionId: string) =>
    invoke<void>('rollback_session_transaction', { dbSessionId }),

  sessionTransactionStatus: (dbSessionId: string) =>
    invoke<boolean>('session_transaction_status', { dbSessionId }),
};
