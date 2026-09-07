import { beforeEach, describe, expect, it, vi } from 'vitest';
import { renderHook } from '@testing-library/react';
import type { TableSchema } from '../../types';
import type {
  QualifiedRelationId,
  SqlRelationBinding,
} from '../../components/sql-editor/semantic/types';
import type { EditorMetadataContext } from '../../components/sql-editor/metadata/types';
import { createMetadataCache } from '../../components/sql-editor/metadata/metadataCache';
import {
  bindingToRelationRequests,
  ensureMetadataRelations,
  getMetadataRelation,
  readMetadataSnapshot,
  relationCacheKey,
  resolveEditorDialectId,
  useActiveSessionMetadataSnapshot,
  useMetadataSnapshot,
} from '../schemaStoreSelectors';
import { useSchemaStore } from '../schemaStore';

function ident(name: string): QualifiedRelationId {
  return { namespacePath: [{ name: 'public', quoted: false }], name: { name, quoted: false } };
}

function makeSchema() {
  return {
    tableName: 'users',
    columns: [],
    primaryKeys: [],
    indexes: [],
    foreignKeys: [],
  } as TableSchema;
}

const ctx: EditorMetadataContext = { database: 'demo', schema: 'public', dialectId: 'postgresql' };

describe('schemaStoreSelectors', () => {
  let cache: ReturnType<typeof createMetadataCache>;
  let loadTableSchema: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    loadTableSchema = vi.fn();
    cache = createMetadataCache({
      loadTableSchema: loadTableSchema as never,
      debounceMs: 100_000,
    });
  });

  it('reads snapshot and relation synchronously', async () => {
    loadTableSchema.mockResolvedValue(makeSchema());
    ensureMetadataRelations('s1', [{ identity: ident('users'), kind: 'table' }], ctx, cache);
    await cache.flushNow();
    const snapshot = readMetadataSnapshot('s1', cache);
    expect(snapshot.relations.size).toBe(1);
    const rel = getMetadataRelation('s1', 's1::public.users', cache);
    expect(rel?.columns).toEqual([]);
  });

  it('normalizes relation cache key', () => {
    expect(relationCacheKey('s1', ident('users'), 'postgresql')).toBe('s1::public.users');
  });

  it('maps semantic bindings to requests, dropping cte/subquery', () => {
    const bindings: SqlRelationBinding[] = [
      {
        relation: ident('t1'),
        sourceRange: { from: 0, to: 2 },
        sourceKind: 'table',
      },
      {
        relation: ident('v1'),
        sourceRange: { from: 0, to: 2 },
        sourceKind: 'view',
      },
      {
        relation: ident('cte'),
        sourceRange: { from: 0, to: 3 },
        sourceKind: 'cte',
      },
      {
        relation: ident('sub'),
        sourceRange: { from: 0, to: 3 },
        sourceKind: 'subquery',
      },
    ];
    const requests = bindingToRelationRequests(bindings);
    expect(requests).toHaveLength(2);
    expect(requests[0]).toMatchObject({ kind: 'table' });
    expect(requests[1]).toMatchObject({ kind: 'view' });
  });

  it('resolves editor dialect id from databaseType', () => {
    expect(resolveEditorDialectId()).toBe('standard');
    expect(resolveEditorDialectId('postgresql')).toBe('postgresql');
    expect(resolveEditorDialectId('unknown-db-type')).toBe('unknown-db-type');
  });

  it('useMetadataSnapshot hook subscribes to the cache', async () => {
    loadTableSchema.mockResolvedValue(makeSchema());
    const { result } = renderHook(() => useMetadataSnapshot('s1', cache));
    expect(result.current.relations.size).toBe(0);
    ensureMetadataRelations('s1', [{ identity: ident('users'), kind: 'table' }], ctx, cache);
    await cache.flushNow();
    await vi.waitFor(() => expect(result.current.relations.size).toBe(1));
  });

  it('useActiveSessionMetadataSnapshot reads the active session (empty cache)', () => {
    useSchemaStore.setState({ activeDbSessionId: 'act-sess' });
    const { result } = renderHook(() => useActiveSessionMetadataSnapshot());
    expect(result.current.dbSessionId).toBe('act-sess');
    expect(result.current.relations.size).toBe(0);
  });
});
