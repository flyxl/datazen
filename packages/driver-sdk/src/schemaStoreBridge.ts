import type { TableInfo } from '../../../src/types';

export type SchemaStoreState = {
  pathItems: Record<string, TableInfo[]>;
  /** Active-session database list (flattened from the per-session cache). */
  databases: string[];
  loading: boolean;
  loadForConnection: (dbSessionId: string, options?: { skipLoadTables?: boolean }) => Promise<void>;
  setLoadedTables: (database: string, tables: TableInfo[]) => void;
  mergeNamespace: (segments: string[], kind: 'branch' | 'tables', names: string[]) => void;
  registerPathAliases: (entries: { name: string; id: string }[]) => void;
  cachePathItems: (fetchPath: string, items: TableInfo[]) => void;
};

export type BoundSchemaStore = {
  <T>(selector: (state: SchemaStoreState) => T): T;
  getState: () => SchemaStoreState;
  setState: (partial: Record<string, unknown>) => void;
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
 * Driver trees read `databases` / `loading` / actions through it instead of
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
export function syncSchemaTables(
  database: string,
  tables: TableInfo[],
  dbSessionId?: string,
): void {
  const store = getStore();
  if (dbSessionId) {
    store.setState({ dbSessionId });
  }
  store.getState().setLoadedTables(database, tables);
}

export function syncSchemaNamespace(
  segments: string[],
  kind: 'branch' | 'tables',
  names: string[],
  options?: { dbSessionId?: string },
): void {
  const store = getStore();
  if (options?.dbSessionId) {
    store.setState({ dbSessionId: options.dbSessionId });
  }
  store.getState().mergeNamespace(segments, kind, names);
}

/**
 * Register SQL display-name → fetch-path-root aliases and seed top-level namespace branches.
 * Drivers that use opaque path roots (e.g. numeric ids) call this after listing databases.
 */
export function registerPathAliases(
  entries: { name: string; id: string }[],
  dbSessionId?: string,
): void {
  const store = getStore();
  if (dbSessionId) {
    store.setState({ dbSessionId });
  }
  store.getState().registerPathAliases(entries);
}

/** Cached `get_tables` rows for a fetch path (`dbId` or `dbId/catalog[/schema]`). */
export function getCachedPathItems(fetchPath: string): TableInfo[] | undefined {
  return boundStore?.getState().pathItems[fetchPath];
}

/** Store `get_tables` rows so autocomplete and the schema tree share one fetch. */
export function cachePathItems(fetchPath: string, items: TableInfo[]): void {
  getStore().getState().cachePathItems(fetchPath, items);
}

/** Subscribe to the shared path-item cache (custom trees hydrate from autocomplete). */
export function subscribeSchemaPathItems(
  listener: (items: Record<string, TableInfo[]>) => void,
): () => void {
  const store = getStore();
  listener(store.getState().pathItems);
  return store.subscribe((state, prev) => {
    if (state.pathItems !== prev.pathItems) listener(state.pathItems);
  });
}
