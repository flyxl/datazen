/**
 * The Query-assets domain client (form-split F3, contract §6.12).
 *
 * Everything a principal keeps about their own querying: favorite statements
 * and the paged history of what they ran. The two desktop-only filesystem
 * commands (`get_favorites_root` / `refresh_favorites`) are deliberately not
 * methods here — they never ride the HTTP surface, and the desktop keeps
 * calling them through the legacy rail.
 *
 * Write faces carry a `SubmissionToken` the same way every other write does;
 * the desktop transport drops it before invoking (the IPC rail needs no
 * idempotency receipt inside one trusted process).
 */

import type { BackendGuard } from './guard';
import type { BackendTransport } from './transport';
import type {
  FavoriteCreateRequest,
  FavoriteQuery,
  HistoryPurgeRequest,
  QueryFavoriteDeleteRequest,
  QueryFavoritesListRequest,
  QueryHistoryDeleteRequest,
  QueryHistoryPage,
  QueryHistoryPageRequest,
} from './types/queryAssets';
import type { SubmissionToken, SubmitRequest } from './types/session';

/** The Query-assets slice of `MethodMap`. */
export interface QueryAssetsMethodMap {
  listQueryFavorites: { request: QueryFavoritesListRequest; response: FavoriteQuery[] };
  createQueryFavorite: {
    request: FavoriteCreateRequest & SubmitRequest;
    response: FavoriteQuery;
  };
  deleteQueryFavorite: { request: QueryFavoriteDeleteRequest & SubmitRequest; response: void };
  getQueryHistoryPage: { request: QueryHistoryPageRequest; response: QueryHistoryPage };
  deleteQueryHistory: {
    request: QueryHistoryDeleteRequest & SubmitRequest;
    /** Rows removed; 0 means the id was already gone (the caller rejects no-ops). */
    response: number;
  };
  clearQueryHistory: { request: SubmitRequest; response: void };
  purgeHistory: { request: HistoryPurgeRequest & SubmitRequest; response: number };
}

/** The Query-assets domain client surface. */
export interface QueryAssetsClient {
  listQueryFavorites(request?: QueryFavoritesListRequest): Promise<FavoriteQuery[]>;
  createQueryFavorite(
    request: FavoriteCreateRequest,
    token: SubmissionToken,
  ): Promise<FavoriteQuery>;
  deleteQueryFavorite(request: QueryFavoriteDeleteRequest, token: SubmissionToken): Promise<void>;
  getQueryHistoryPage(request: QueryHistoryPageRequest): Promise<QueryHistoryPage>;
  deleteQueryHistory(request: QueryHistoryDeleteRequest, token: SubmissionToken): Promise<number>;
  clearQueryHistory(token: SubmissionToken): Promise<void>;
  purgeHistory(request: HistoryPurgeRequest, token: SubmissionToken): Promise<number>;
}

/** Build the Query-assets domain client over a transport. */
export function createQueryAssetsMethods(
  transport: BackendTransport,
  guard: BackendGuard,
): QueryAssetsClient {
  return {
    listQueryFavorites: (request) =>
      guard('listQueryFavorites', () => transport.call('listQueryFavorites', request ?? {})),

    createQueryFavorite: (request, submissionToken) =>
      guard('createQueryFavorite', () =>
        transport.call('createQueryFavorite', { ...request, submissionToken }),
      ),

    deleteQueryFavorite: (request, submissionToken) =>
      guard('deleteQueryFavorite', () =>
        transport.call('deleteQueryFavorite', { ...request, submissionToken }),
      ),

    getQueryHistoryPage: (request) =>
      guard('getQueryHistoryPage', () => transport.call('getQueryHistoryPage', request)),

    deleteQueryHistory: (request, submissionToken) =>
      guard('deleteQueryHistory', () =>
        transport.call('deleteQueryHistory', { ...request, submissionToken }),
      ),

    clearQueryHistory: (submissionToken) =>
      guard('clearQueryHistory', () => transport.call('clearQueryHistory', { submissionToken })),

    purgeHistory: (request, submissionToken) =>
      guard('purgeHistory', () => transport.call('purgeHistory', { ...request, submissionToken })),
  };
}
