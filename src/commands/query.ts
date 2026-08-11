import { invoke } from '@tauri-apps/api/core';
import type { ExplainResult, FavoriteQuery, MultiQueryResult, QueryHistoryEntry } from '../types';
import { driverCommands } from './driver';

export const queryCommands = {
  executeQuery: async (connectionId: string, sql: string) => {
    const result = await driverCommands.execute({
      connectionId,
      command: 'query',
      input: { sql },
    });
    return result.data as MultiQueryResult;
  },

  getExplain: (connectionId: string, sql: string) =>
    invoke<ExplainResult>('get_explain', { connectionId, sql }),

  cancelQuery: (connectionId: string) =>
    invoke<void>('cancel_query', { connectionId }),

  getQueryHistory: (limit: number) =>
    invoke<QueryHistoryEntry[]>('get_query_history', { limit }),

  clearQueryHistory: () => invoke<void>('clear_query_history'),

  getFavoriteQueries: () =>
    invoke<FavoriteQuery[]>('get_favorite_queries'),

  addFavoriteQuery: (title: string, sql: string) =>
    invoke<FavoriteQuery>('add_favorite_query', { title, sql }),

  deleteFavoriteQuery: (id: string) =>
    invoke<void>('delete_favorite_query', { id }),
};
