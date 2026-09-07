import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { TableSchema } from '../../../../types';
import type { QualifiedRelationId } from '../../semantic/types';
import { createMetadataCache, schemaToMetadata } from '../metadataCache';
import type { EditorRelationRequest } from '../types';

function ident(namespace: string[], name: string): QualifiedRelationId {
  return {
    namespacePath: namespace.map((n) => ({ name: n, quoted: false })),
    name: { name, quoted: false },
  };
}

function makeSchema(name: string, comment?: string): TableSchema {
  return {
    tableName: name,
    columns: [{ name: 'id', dataType: 'int', nullable: false, isPrimaryKey: true }],
    primaryKeys: ['id'],
    indexes: [{ name: 'idx', columns: ['id'], isUnique: false, isPrimary: true }],
    foreignKeys: [
      { name: 'fk', columns: ['id'], referencedTable: 'other', referencedColumns: ['id'] },
    ],
    ...(comment != null ? { comment } : {}),
  } as TableSchema;
}

const ctx = { database: 'demo', schema: 'public', dialectId: 'postgresql' };

describe('metadataCache', () => {
  let now: number;
  let loadTableSchema: ReturnType<typeof vi.fn>;
  let cache: ReturnType<typeof createMetadataCache>;

  beforeEach(() => {
    now = 1000;
    loadTableSchema = vi.fn();
    cache = createMetadataCache({
      loadTableSchema: loadTableSchema as never,
      debounceMs: 100_000,
      errorTtlMs: 100,
      now: () => now,
    });
  });

  it('loads a relation and exposes it in the immutable snapshot', async () => {
    loadTableSchema.mockResolvedValue(makeSchema('users', 'a table'));
    cache.ensureRelations('s1', [{ identity: ident(['public'], 'users'), kind: 'table' }], ctx);
    expect(loadTableSchema).not.toHaveBeenCalled(); // debounced
    await cache.flushNow();

    const snapshot = cache.getSnapshot('s1');
    expect(snapshot.epoch).toBeGreaterThan(0);
    expect(snapshot.database).toBe('demo');
    expect(snapshot.schema).toBe('public');
    expect(snapshot.relations.size).toBe(1);
    const meta = snapshot.relations.get('s1::public.users');
    expect(meta!.comment).toBe('a table');
    expect(meta!.primaryKey).toEqual(['id']);
    expect(meta!.columns[0]!.name).toBe('id');
    expect(meta!.foreignKeys[0]!.referencedTable).toBe('other');
    expect(loadTableSchema).toHaveBeenCalledWith('s1', 'public.users', 'demo');
  });

  it('dedupes identical in-flight requests to one IPC call', async () => {
    let resolveFn: (value: TableSchema) => void = () => {};
    loadTableSchema.mockReturnValue(new Promise((r) => (resolveFn = r)));
    const req: EditorRelationRequest = { identity: ident(['public'], 'users'), kind: 'table' };
    cache.ensureRelations('s1', [req], ctx);
    const inflight = cache.flushNow(); // starts an in-flight load, returns pending promise
    cache.ensureRelations('s1', [req], ctx); // while in-flight → deduped
    resolveFn(makeSchema('users'));
    await inflight;
    expect(loadTableSchema).toHaveBeenCalledTimes(1);
    expect(cache.getSnapshot('s1').relations.size).toBe(1);
  });

  it('batches multiple relations in one flush', async () => {
    loadTableSchema.mockResolvedValue(makeSchema('x'));
    cache.ensureRelations(
      's1',
      [
        { identity: ident(['public'], 'a'), kind: 'table' },
        { identity: ident(['public'], 'b'), kind: 'table' },
      ],
      ctx,
    );
    await cache.flushNow();
    expect(loadTableSchema).toHaveBeenCalledTimes(2);
    expect(cache.getSnapshot('s1').relations.size).toBe(2);
  });

  it('skips cte/subquery and cross-namespace when allowQualified=false', async () => {
    loadTableSchema.mockResolvedValue(makeSchema('x'));
    const qualified = createMetadataCache({
      loadTableSchema: loadTableSchema as never,
      debounceMs: 100_000,
      errorTtlMs: 100,
      now: () => now,
      allowQualified: false,
    });
    qualified.ensureRelations(
      's1',
      [
        { identity: ident(['public'], 'a'), kind: 'table' },
        { identity: ident([], 'bare'), kind: 'table' },
        { identity: ident([], 'cte'), kind: 'cte' },
        { identity: ident([], 'sub'), kind: 'subquery' },
      ],
      ctx,
    );
    await qualified.flushNow();
    expect(qualified.getSnapshot('s1').relations.size).toBe(1); // only bare loaded
    expect(loadTableSchema).toHaveBeenCalledTimes(1);
    expect(loadTableSchema).toHaveBeenCalledWith('s1', 'bare', 'demo');
  });

  it('caches a load error for the TTL, then retries after it expires', async () => {
    loadTableSchema.mockRejectedValueOnce(new Error('boom'));
    const req: EditorRelationRequest = { identity: ident([], 'users'), kind: 'table' };
    cache.ensureRelations('s1', [req], ctx);
    await cache.flushNow();
    expect(loadTableSchema).toHaveBeenCalledTimes(1);

    // Within TTL → no retry, stays absent.
    cache.ensureRelations('s1', [req], ctx);
    await cache.flushNow();
    expect(loadTableSchema).toHaveBeenCalledTimes(1);

    // After TTL expires → retried.
    now = 1101;
    cache.ensureRelations('s1', [req], ctx);
    await cache.flushNow();
    expect(loadTableSchema).toHaveBeenCalledTimes(2);
  });

  it('exposes loaded relations via getRelation and returns undefined for missing', async () => {
    loadTableSchema.mockResolvedValue(makeSchema('users'));
    cache.ensureRelations('s1', [{ identity: ident(['public'], 'users'), kind: 'table' }], ctx);
    await cache.flushNow();
    const meta = cache.getRelation('s1', 's1::public.users');
    expect(meta?.kind).toBe('table');
    expect(cache.getRelation('s1', 's1::public.missing')).toBeUndefined();
  });

  it('returns a referentially stable snapshot until the epoch bumps', async () => {
    loadTableSchema.mockResolvedValue(makeSchema('users'));
    cache.ensureRelations('s1', [{ identity: ident(['public'], 'users'), kind: 'table' }], ctx);
    const before = cache.getSnapshot('s1');
    expect(cache.getSnapshot('s1')).toBe(before); // same ref within epoch
    await cache.flushNow();
    const after = cache.getSnapshot('s1');
    expect(after).not.toBe(before); // epoch bumped → new ref
  });

  it('invalidates a relation by identity', async () => {
    loadTableSchema.mockResolvedValue(makeSchema('users'));
    cache.ensureRelations('s1', [{ identity: ident(['public'], 'users'), kind: 'table' }], ctx);
    await cache.flushNow();
    expect(cache.getRelation('s1', 's1::public.users')).toBeDefined();
    cache.invalidateRelation('s1', ident(['public'], 'users'), 'postgresql');
    expect(cache.getRelation('s1', 's1::public.users')).toBeUndefined();
  });

  it('invalidates a whole session and all sessions', async () => {
    loadTableSchema.mockResolvedValue(makeSchema('u'));
    cache.ensureRelations('s1', [{ identity: ident([], 'a'), kind: 'table' }], ctx);
    cache.ensureRelations('s2', [{ identity: ident([], 'b'), kind: 'table' }], ctx);
    await cache.flushNow();
    expect(cache.getSnapshot('s1').relations.size).toBe(1);
    expect(cache.getSnapshot('s2').relations.size).toBe(1);

    cache.invalidateSession('s1');
    expect(cache.getSnapshot('s1').relations.size).toBe(0);
    expect(cache.getSnapshot('s2').relations.size).toBe(1);

    cache.invalidateAll();
    expect(cache.getSnapshot('s2').relations.size).toBe(0);
  });

  it('switchContext resets relations and bumps epoch', async () => {
    loadTableSchema.mockResolvedValue(makeSchema('u'));
    cache.ensureRelations('s1', [{ identity: ident([], 'a'), kind: 'table' }], {
      ...ctx,
      schema: 'public',
    });
    await cache.flushNow();
    const firstEpoch = cache.getSnapshot('s1').epoch;

    cache.switchContext('s1', { database: 'demo', schema: 'other', dialectId: 'postgresql' });
    const s = cache.getSnapshot('s1');
    expect(s.epoch).toBeGreaterThan(firstEpoch);
    expect(s.schema).toBe('other');
    expect(s.relations.size).toBe(0);
  });

  it('notifies subscribers on relation change and supports unsubscribe', async () => {
    loadTableSchema.mockResolvedValue(makeSchema('u'));
    const listener = vi.fn();
    const unsubscribe = cache.subscribe(listener);
    cache.ensureRelations('s1', [{ identity: ident([], 'a'), kind: 'table' }], ctx);
    expect(listener).not.toHaveBeenCalled();
    await cache.flushNow();
    expect(listener).toHaveBeenCalled();
    unsubscribe();
    cache.invalidateSession('s1');
    const calls = listener.mock.calls.length;
    expect(listener).toHaveBeenCalledTimes(calls); // not called again after unsubscribe
  });

  it('returns an empty snapshot for unknown sessions', () => {
    const s = cache.getSnapshot('none');
    expect(s.epoch).toBe(0);
    expect(s.relations.size).toBe(0);
  });

  it('schemaToMetadata maps columns/PK/index/FK and keeps comment optional', () => {
    const withComment = schemaToMetadata('k', ident([], 't'), 'table', makeSchema('t', 'hi'), 5);
    expect(withComment.comment).toBe('hi');
    const noComment = schemaToMetadata('k', ident([], 't'), 'table', makeSchema('t'), 6);
    expect(noComment.comment).toBeUndefined();
    expect(Object.isFrozen(noComment)).toBe(true);
  });

  it('invalidates a relation from an external schema-cache invalidation', async () => {
    let externalListener: ((s: string, t?: string) => void) | undefined;
    const wired = createMetadataCache({
      loadTableSchema: loadTableSchema as never,
      debounceMs: 100_000,
      subscribeInvalidation: (l) => {
        externalListener = l;
        return () => {
          externalListener = undefined;
        };
      },
    });
    loadTableSchema.mockResolvedValue(makeSchema('users'));
    wired.ensureRelations('s1', [{ identity: ident(['public'], 'users'), kind: 'table' }], ctx);
    await wired.flushNow();
    expect(wired.getRelation('s1', 's1::public.users')).toBeDefined();

    // External DDL-mutation invalidation (name form) drops the relation.
    externalListener!('s1', 'public.users');
    expect(wired.getRelation('s1', 's1::public.users')).toBeUndefined();

    // Whole-session invalidation drops the session entirely.
    wired.ensureRelations('s2', [{ identity: ident([], 'a'), kind: 'table' }], ctx);
    await wired.flushNow();
    expect(wired.getSnapshot('s2').relations.size).toBe(1);
    externalListener!('s2');
    expect(wired.getSnapshot('s2').relations.size).toBe(0);
  });

  describe('[tester] metadataCache coverage gaps', () => {
    it('loads a view relation and exposes it with kind view', async () => {
      loadTableSchema.mockResolvedValue(makeSchema('v'));
      cache.ensureRelations('s1', [{ identity: ident(['public'], 'v'), kind: 'view' }], ctx);
      await cache.flushNow();
      expect(loadTableSchema).toHaveBeenCalledWith('s1', 'public.v', 'demo');
      const meta = cache.getRelation('s1', 's1::public.v');
      expect(meta?.kind).toBe('view');
    });

    it('keeps a table and a view sharing a name in different namespaces distinct', async () => {
      loadTableSchema.mockResolvedValue(makeSchema('users'));
      cache.ensureRelations(
        's1',
        [
          { identity: ident(['public'], 'users'), kind: 'table' },
          { identity: ident(['audit'], 'users'), kind: 'view' },
        ],
        ctx,
      );
      await cache.flushNow();
      expect(loadTableSchema).toHaveBeenCalledTimes(2);
      expect(cache.getSnapshot('s1').relations.size).toBe(2);
      expect(cache.getRelation('s1', 's1::public.users')?.kind).toBe('table');
      expect(cache.getRelation('s1', 's1::audit.users')?.kind).toBe('view');
    });

    it('auto-flushes queued requests once the debounce window elapses', async () => {
      vi.useFakeTimers();
      loadTableSchema.mockClear();
      loadTableSchema.mockResolvedValue(makeSchema('users'));
      const debounced = createMetadataCache({
        loadTableSchema: loadTableSchema as never,
        debounceMs: 40,
        now: () => 0,
      });
      expect(loadTableSchema).not.toHaveBeenCalled();
      debounced.ensureRelations(
        's1',
        [{ identity: ident(['public'], 'users'), kind: 'table' }],
        ctx,
      );
      expect(loadTableSchema).not.toHaveBeenCalled(); // still debounced
      await vi.advanceTimersByTimeAsync(50);
      await vi.waitFor(() => expect(debounced.getSnapshot('s1').relations.size).toBe(1));
      expect(loadTableSchema).toHaveBeenCalledWith('s1', 'public.users', 'demo');
      vi.useRealTimers();
    });
  });
});
