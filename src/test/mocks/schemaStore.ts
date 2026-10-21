import { useMemo } from 'react';
import type { SchemaStore } from '../../stores/schemaStore';
import {
  createEmptyConnectionSchema,
  type ConnectionSchemaState,
} from '../../stores/schemaStoreState';
import { projectRelationColumns } from '../../stores/schemaColumnLoader';

/**
 * Test double for `src/stores/schemaStore`.
 *
 * `vi.mock(path, factory)` replaces the WHOLE module, so a factory that returns
 * only a subset of the real exports makes any other import of the missing ones
 * throw `No "<name>" export is defined on the "<path>" mock` at render time.
 * `schemaStoreMockModule()` therefore mirrors every value export the real
 * module has, so adding a new store export cannot silently break a suite.
 *
 * The store is keyed per runtime DB session (`schemas: Map<dbSessionId, …>`),
 * and a flat `currentDatabase` / `tables` / `views` mock drifts from production
 * the moment a consumer reads the map instead of the hook (`query/contracts.ts`
 * does). So this double keeps exactly ONE source of truth — the map — and the
 * hooks reproduce the real ones' contract: a map hit returns that session's
 * field, a miss returns the empty snapshot, never another session's data.
 */
/** The only slice the mocked hooks read; suites add the action spies they assert on. */
export type SchemaStoreCache = Pick<SchemaStore, 'schemas' | 'activeDbSessionId'>;

/** Seed one session's cache the way `patchConnectionSchema` would. */
export function seedConnectionSchema(
  state: SchemaStoreCache,
  dbSessionId: string,
  patch: Partial<ConnectionSchemaState>,
): void {
  const prev = state.schemas.get(dbSessionId) ?? createEmptyConnectionSchema();
  state.schemas.set(dbSessionId, { ...prev, ...patch });
}

/** The value exports of `src/stores/schemaStore` backed by `state`. */
export function schemaStoreMockModule(state: SchemaStoreCache) {
  const emptyConnectionSchema = createEmptyConnectionSchema();
  return {
    useSchemaStore: Object.assign((selector: (s: typeof state) => unknown) => selector(state), {
      getState: () => state,
    }),
    /** Hit → that session's field; miss → the empty snapshot. Never another session's data. */
    useConnectionSchemaField: <K extends keyof ConnectionSchemaState>(
      dbSessionId: string,
      field: K,
    ): ConnectionSchemaState[K] => {
      const entry = state.schemas.get(dbSessionId);
      if (entry) return entry[field];
      return emptyConnectionSchema[field];
    },
    useConnectionColumnMaps: (dbSessionId: string) => {
      const entry = state.schemas.get(dbSessionId);
      return useMemo(
        () =>
          projectRelationColumns(
            entry ?? emptyConnectionSchema,
            dbSessionId,
            entry?.currentDatabase ?? '',
            entry?.currentSchema ?? undefined,
          ),
        [entry, dbSessionId],
      );
    },
  };
}
