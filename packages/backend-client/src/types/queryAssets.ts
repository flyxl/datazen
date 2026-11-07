/**
 * Query-assets wire vocabulary (contract §6.12): the favorite statements a
 * principal keeps and the paged history of what they ran.
 *
 * The shapes are the shared Rust vocabulary (`datazen::history::domain` —
 * `FavoriteQuery`, `QueryHistoryEntry`, `QueryHistoryPage`) serialized
 * camelCase; both the desktop IPC command rail and the server HTTP surface
 * answer with this same shape, which is what lets one client type serve both.
 *
 * `get_favorites_root` / `refresh_favorites` stay OFF this surface by
 * contract: they are filesystem semantics (the desktop favorites directory),
 * and the server stores rows, not files.
 */

/** One recorded execution of a statement, as the paged history read returns it. */
export interface QueryHistoryEntry {
  id: string;
  connectionId: string;
  database: string;
  /** Schema namespace when known (PG search_path is not session-tracked yet → usually absent). */
  schema?: string | null;
  sql: string;
  executedAt: string;
  executionTimeMs: number;
  rowsAffected?: number;
  success: boolean;
  errorMessage?: string;
}

/** Backend sort orders for the paged history read. */
export type HistorySort = 'recent' | 'oldest' | 'slowest';

/**
 * One page of query history plus the number of rows the filter matched
 * *before* the page was cut.
 *
 * `total` is what lets the UI admit it is showing a slice: without it, a
 * `limit`-ed page is indistinguishable from the whole table.
 */
export interface QueryHistoryPage {
  entries: QueryHistoryEntry[];
  total: number;
}

/**
 * A saved statement. On the desktop this is one `.sql` file under the
 * favorites root named by `id` with these fields in `--` front-matter; on the
 * server it is a row. The wire shape is the same either way.
 */
export interface FavoriteQuery {
  id: string;
  connectionId: string;
  title: string;
  sql: string;
  createdAt: string;
  /** Absent when the record carries no `updatedAt`. */
  updatedAt?: string | null;
  /** Target database the statement was saved against, for later re-binding. */
  database?: string | null;
  /** Reserved for the keyword track; parsed and round-tripped, not yet bound. */
  keyword?: string | null;
  /** Directory relative to the favorites root, `/`-separated. Absent = root. */
  folder?: string | null;
}

/** `listQueryFavorites`: optional connection filter. */
export interface QueryFavoritesListRequest {
  connectionId?: string | null;
}

/**
 * `createQueryFavorite`: the draft the backend mints an id and timestamps for.
 * Only `connectionId`/`title`/`sql` are required; the optional fields are the
 * server row shape's extras (the desktop file front-matter carries
 * `keyword`/`database` too, but its IPC command takes the three required
 * ones and ignores the rest).
 */
export interface FavoriteCreateRequest {
  connectionId: string;
  title: string;
  sql: string;
  databaseName?: string | null;
  keyword?: string | null;
  folder?: string | null;
}

/** `deleteQueryFavorite`: the id from the listing. */
export interface QueryFavoriteDeleteRequest {
  id: string;
}

/**
 * `getQueryHistoryPage`: every predicate is applied before the page is cut,
 * and `total` reports the whole match set.
 */
export interface QueryHistoryPageRequest {
  limit: number;
  connectionId?: string | null;
  database?: string | null;
  schema?: string | null;
  search?: string | null;
  since?: string | null;
  /** Upper bound; server surface supports it, the desktop command ignores it. */
  until?: string | null;
  order?: HistorySort;
}

/** `deleteQueryHistory`: resolves to the number of rows removed (0 = no-op). */
export interface QueryHistoryDeleteRequest {
  id: string;
}

/** Which family of recorded history a purge addresses. */
export type HistoryPurgeScope = 'query' | 'workflow' | 'all';

/**
 * `purgeHistory`: bulk retention policy. `retainDays` absent = purge the
 * whole scope regardless of age.
 */
export interface HistoryPurgeRequest {
  scope: HistoryPurgeScope;
  retainDays?: number;
}
