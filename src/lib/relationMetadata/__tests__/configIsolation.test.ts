/**
 * P4 configuration isolation for the editor relation-metadata cache.
 *
 * The cache addresses its per-session state by (connectionId, configRevision,
 * dbSessionId) instead of by `dbSessionId` alone. These tests drive that
 * partition directly: the same session id, re-read under a different
 * configuration, must not observe anything read under the previous one.
 */

import { afterEach, describe, expect, it, vi } from 'vitest';
import type { TableSchema } from '../../../types';
import { createMetadataCache } from '../metadataCache';
import { registerEditorSessionConfigResolver } from '../sessionConfig';
import type { EditorMetadataContext, EditorRelationRequest } from '../types';

// Built structurally, not imported: shared library code (and its tests) must
// not depend on the consumers below it.
function ident(namespace: string[], name: string): EditorRelationRequest['identity'] {
  return {
    namespacePath: namespace.map((n) => ({ name: n, quoted: false })),
    name: { name, quoted: false },
  };
}

function makeSchema(name: string): TableSchema {
  return {
    tableName: name,
    columns: [{ name: 'id', dataType: 'int', nullable: false, isPrimaryKey: true }],
    primaryKeys: ['id'],
    indexes: [],
    foreignKeys: [],
  } as TableSchema;
}

const ctx: EditorMetadataContext = {
  database: 'demo',
  schema: 'public',
  dialectId: 'postgresql',
};
const request: EditorRelationRequest = {
  identity: ident(['public'], 'users'),
  kind: 'table',
};

describe('metadataCache configuration isolation', () => {
  const unregisters: Array<() => void> = [];

  afterEach(() => {
    while (unregisters.length > 0) unregisters.pop()?.();
  });

  function publishConfig(config: { connectionId: string; configRevision: number }): void {
    unregisters.push(registerEditorSessionConfigResolver(() => config));
  }

  function makeCache(loadTableSchema: ReturnType<typeof vi.fn>) {
    return createMetadataCache({
      loadTableSchema: loadTableSchema as never,
      debounceMs: 100_000,
      errorTtlMs: 100,
      now: () => 1000,
    });
  }

  it('does not serve a previous configuration snapshot to a rebind', async () => {
    const loadTableSchema = vi.fn().mockResolvedValue(makeSchema('users'));
    const cache = makeCache(loadTableSchema);

    publishConfig({ connectionId: 'conn-1', configRevision: 1 });
    cache.ensureRelations('s1', [request], ctx);
    await cache.flushNow();
    expect(cache.getSnapshot('s1').relations.get('s1::public.users')).toBeDefined();

    // Same runtime session, edited connection configuration.
    unregisters.pop()?.();
    publishConfig({ connectionId: 'conn-1', configRevision: 2 });

    const rebound = cache.getSnapshot('s1');
    expect(rebound.relations.size).toBe(0);

    cache.ensureRelations('s1', [request], ctx);
    await cache.flushNow();
    // Re-read, not served from the old partition's in-flight/loaded state.
    expect(loadTableSchema).toHaveBeenCalledTimes(2);
  });

  it('does not share an in-flight load across a rebind', async () => {
    let resolveFirst: (value: TableSchema) => void = () => {};
    const loadTableSchema = vi
      .fn()
      .mockReturnValueOnce(new Promise<TableSchema>((r) => (resolveFirst = r)))
      .mockResolvedValue(makeSchema('users'));
    const cache = makeCache(loadTableSchema);

    publishConfig({ connectionId: 'conn-1', configRevision: 1 });
    cache.ensureRelations('s1', [request], ctx);
    const firstFlush = cache.flushNow();

    unregisters.pop()?.();
    publishConfig({ connectionId: 'conn-1', configRevision: 2 });
    cache.ensureRelations('s1', [request], ctx);
    await cache.flushNow();

    resolveFirst(makeSchema('users'));
    await firstFlush;
    expect(loadTableSchema).toHaveBeenCalledTimes(2);
  });

  it('does not serve a connection config change as the same session', async () => {
    const loadTableSchema = vi.fn().mockResolvedValue(makeSchema('users'));
    const cache = makeCache(loadTableSchema);

    publishConfig({ connectionId: 'conn-a', configRevision: 1 });
    cache.ensureRelations('s1', [request], ctx);
    await cache.flushNow();

    unregisters.pop()?.();
    publishConfig({ connectionId: 'conn-b', configRevision: 1 });
    expect(cache.getSnapshot('s1').relations.size).toBe(0);
    expect(cache.getRelation('s1', 's1::public.users')).toBeUndefined();
  });

  it('still isolates two live sessions of the same config', async () => {
    const loadTableSchema = vi.fn().mockResolvedValue(makeSchema('users'));
    const cache = makeCache(loadTableSchema);
    publishConfig({ connectionId: 'conn-1', configRevision: 1 });

    cache.ensureRelations('s1', [request], ctx);
    await cache.flushNow();
    expect(cache.getRelation('s2', 's1::public.users')).toBeUndefined();
    expect(cache.getSnapshot('s2').relations.size).toBe(0);
  });

  it('invalidates every partition of a session, whatever its configuration', async () => {
    const loadTableSchema = vi.fn().mockResolvedValue(makeSchema('users'));
    const cache = makeCache(loadTableSchema);

    // Same session id, two configurations; the invalidation is config-agnostic.
    publishConfig({ connectionId: 'conn-1', configRevision: 1 });
    cache.ensureRelations('s1', [request], ctx);
    await cache.flushNow();
    unregisters.pop()?.();
    publishConfig({ connectionId: 'conn-1', configRevision: 2 });
    cache.ensureRelations('s1', [request], ctx);
    await cache.flushNow();
    expect(cache.getRelation('s1', 's1::public.users')).toBeDefined();

    cache.invalidateSession('s1');
    expect(cache.getRelation('s1', 's1::public.users')).toBeUndefined();
    expect(cache.getSnapshot('s1').relations.size).toBe(0);
  });

  it('keeps session-independent behaviour when no host publishes a configuration', async () => {
    const loadTableSchema = vi.fn().mockResolvedValue(makeSchema('users'));
    const cache = makeCache(loadTableSchema);

    cache.ensureRelations('s1', [request], ctx);
    await cache.flushNow();
    expect(cache.getRelation('s1', 's1::public.users')).toBeDefined();
  });
});