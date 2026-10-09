import type { TableInfo } from './types/schemaMetadata';

export type SchemaSessionState = {
  pathItems: Record<string, TableInfo[]>;
  databases: string[];
  loading: boolean;
};

export type SchemaStoreState = {
  schemas: Map<string, SchemaSessionState>;
  loadForConnection: (dbSessionId: string, options?: { skipLoadTables?: boolean }) => Promise<void>;
  setLoadedTables: (database: string, tables: TableInfo[], dbSessionId: string) => void;
  mergeNamespace: (
    segments: string[],
    kind: 'branch' | 'tables',
    names: string[],
    dbSessionId: string,
  ) => void;
  registerPathAliases: (entries: { name: string; id: string }[], dbSessionId: string) => void;
  cachePathItems: (fetchPath: string, items: TableInfo[], dbSessionId: string) => void;
};

const EMPTY_SESSION: SchemaSessionState = { pathItems: {}, databases: [], loading: false };

export type BoundSchemaStore = {
  <T>(selector: (state: SchemaStoreState) => T): T;
  getState: () => SchemaStoreState;
  subscribe: (listener: (state: SchemaStoreState, prev: SchemaStoreState) => void) => () => void;
};

let boundStore: BoundSchemaStore | null = null;

export function bindSchemaStore(store: BoundSchemaStore): void {
  boundStore = store;
}

function getStore(): BoundSchemaStore {
  if (!boundStore) {
    throw new Error('SchemaStore has not been bound to driver-sdk yet.');
  }
  return boundStore;
}

/**
 * Reactive accessor for the bound host schema store (zustand-hook shape).
 * Driver trees read session entries and actions through it instead of
 * importing the host store module.
 */
export type UseBoundSchemaStore = {
  <T>(selector: (state: SchemaStoreState) => T): T;
  getState(): SchemaStoreState;
};

export const useBoundSchemaStore: UseBoundSchemaStore = Object.assign(
  <T>(selector: (state: SchemaStoreState) => T): T => getStore()(selector),
  {
    getState: (): SchemaStoreState => getStore().getState(),
  },
);

/**
 * Sync fetched tables into the host schema store (SQL editor autocomplete).
 * Pass `dbSessionId` for custom schema trees that never call `loadForConnection`.
 */
export function syncSchemaTables(database: string, tables: TableInfo[], dbSessionId: string): void {
  const store = getStore();
  store.getState().setLoadedTables(database, tables, dbSessionId);
}

export function syncSchemaNamespace(
  segments: string[],
  kind: 'branch' | 'tables',
  names: string[],
  options: { dbSessionId: string },
): void {
  const store = getStore();
  store.getState().mergeNamespace(segments, kind, names, options.dbSessionId);
}

/**
 * Register SQL display-name → fetch-path-root aliases and seed top-level namespace branches.
 * Drivers that use opaque path roots (e.g. numeric ids) call this after listing databases.
 */
export function registerPathAliases(
  entries: { name: string; id: string }[],
  dbSessionId: string,
): void {
  const store = getStore();
  store.getState().registerPathAliases(entries, dbSessionId);
}

/** Cached `get_tables` rows for a fetch path (`dbId` or `dbId/catalog[/schema]`). */
export function getCachedPathItems(
  fetchPath: string,
  dbSessionId: string,
): TableInfo[] | undefined {
  return boundStore?.getState().schemas.get(dbSessionId)?.pathItems[fetchPath];
}

/** Store `get_tables` rows so autocomplete and the schema tree share one fetch. */
export function cachePathItems(fetchPath: string, items: TableInfo[], dbSessionId: string): void {
  getStore().getState().cachePathItems(fetchPath, items, dbSessionId);
}

/** Subscribe to the shared path-item cache (custom trees hydrate from autocomplete). */
export function subscribeSchemaPathItems(
  listener: (items: Record<string, TableInfo[]>) => void,
  dbSessionId: string,
): () => void {
  const store = getStore();
  listener(store.getState().schemas.get(dbSessionId)?.pathItems ?? EMPTY_SESSION.pathItems);
  return store.subscribe((state, prev) => {
    const current = state.schemas.get(dbSessionId)?.pathItems ?? EMPTY_SESSION.pathItems;
    const previous = prev.schemas.get(dbSessionId)?.pathItems ?? EMPTY_SESSION.pathItems;
    if (current !== previous) listener(current);
  });
}

/** Read only the owning session, including stable defaults before it loads. */
export function useBoundConnectionSchemaField<K extends keyof SchemaSessionState>(
  dbSessionId: string,
  key: K,
): SchemaSessionState[K] {
  return getStore()((state) => (state.schemas.get(dbSessionId) ?? EMPTY_SESSION)[key]);
}
