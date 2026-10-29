/**
 * Production wiring of `MetadataBatcher` (P4 批量).
 *
 * The contract under test is that coalescing never crosses an isolation
 * dimension: two asks merge only when their connection config, config revision,
 * runtime session *and* target namespace all match, and every caller receives
 * only the columns of the relations it asked for.
 */

import { describe, expect, it, vi } from 'vitest';
import type { RelationColumns, RelationRef } from '@datazen/driver-sdk';
import {
  createSchemaColumnBatcher,
  DEFAULT_BATCH_DEBOUNCE_MS,
  type ColumnReadTransport,
} from '../schemaColumnBatcher';
import type { SessionMetadataIdentity } from '../schemaMetadataKeys';

const session: SessionMetadataIdentity = { connectionId: 'conn-1', metadataRevision: 1 };

/** Short but non-zero — the window is what makes coalescing happen. */
const WINDOW = 5;

function batcher(read: ColumnReadTransport, debounceMs = WINDOW) {
  return createSchemaColumnBatcher({ read, debounceMs });
}

/** Attach a handler up front so an expected rejection is never unhandled. */
function settle(
  promise: Promise<unknown>,
): Promise<{ ok: true; value: unknown } | { ok: false; error: unknown }> {
  return promise.then(
    (value) => ({ ok: true, value }),
    (error: unknown) => ({ ok: false, error }),
  );
}

function ref(name: string, over: Partial<RelationRef> = {}): RelationRef {
  return { database: 'demo', schema: 'public', name, ...over };
}

function columnsFor(relations: readonly RelationRef[]): RelationColumns[] {
  return relations.map((r) => ({
    ref: r,
    columns: [{ name: 'id', dataType: 'int', nullable: false, isPrimaryKey: true }],
    primaryKeys: ['id'],
  }));
}

function transport(): ColumnReadTransport & { calls: RelationRef[][] } {
  const calls: RelationRef[][] = [];
  const read = vi.fn(async (_s, _id, relations) => {
    calls.push([...relations]);
    return columnsFor(relations);
  });
  return Object.assign(read as unknown as ColumnReadTransport, { calls });
}

describe('schemaColumnBatcher', () => {
  it('coalesces within a window rather than reading every ask on its own', () => {
    // A zero window would make the batcher a pass-through; the default window
    // is what turns a burst into one round trip.
    expect(DEFAULT_BATCH_DEBOUNCE_MS).toBeGreaterThan(0);
  });

  it('resolves without a read when nothing was asked for', async () => {
    const read = transport();
    const b = batcher(read);
    await expect(b.ensure(session, 's1', [])).resolves.toEqual([]);
    expect(read).not.toHaveBeenCalled();
  });

  it('merges asks of one namespace into a single read', async () => {
    const read = transport();
    const b = batcher(read);
    await Promise.all([
      b.ensure(session, 's1', [ref('users')]),
      b.ensure(session, 's1', [ref('orders')]),
    ]);
    expect(read).toHaveBeenCalledTimes(1);
    expect(read.calls[0]?.map((r) => r.name).sort()).toEqual(['orders', 'users']);
  });

  it('gives every caller only the columns it asked for', async () => {
    const read = transport();
    const b = batcher(read);
    const [users, orders] = await Promise.all([
      b.ensure(session, 's1', [ref('users')]),
      b.ensure(session, 's1', [ref('orders')]),
    ]);
    expect(users.map((c) => c.ref.name)).toEqual(['users']);
    expect(orders.map((c) => c.ref.name)).toEqual(['orders']);
  });

  it('never merges across runtime sessions', async () => {
    const read = transport();
    const b = batcher(read);
    await Promise.all([
      b.ensure(session, 's1', [ref('users')]),
      b.ensure(session, 's2', [ref('users')]),
    ]);
    expect(read).toHaveBeenCalledTimes(2);
    expect(read.calls.map((c) => c[0]?.name)).toEqual(['users', 'users']);
  });

  it('never merges across config revisions of one session', async () => {
    const read = transport();
    const b = batcher(read);
    await Promise.all([
      b.ensure({ connectionId: 'conn-1', metadataRevision: 1 }, 's1', [ref('users')]),
      b.ensure({ connectionId: 'conn-1', metadataRevision: 2 }, 's1', [ref('users')]),
    ]);
    expect(read).toHaveBeenCalledTimes(2);
  });

  it('never merges across connection configs', async () => {
    const read = transport();
    const b = batcher(read);
    await Promise.all([
      b.ensure(session, 's1', [ref('users')]),
      b.ensure({ connectionId: 'conn-2', metadataRevision: 1 }, 's1', [ref('users')]),
    ]);
    expect(read).toHaveBeenCalledTimes(2);
  });

  it('never merges across target namespaces, and returns every group', async () => {
    const read = transport();
    const b = batcher(read);
    const values = await b.ensure(session, 's1', [
      ref('users'),
      ref('users', { schema: 'audit' }),
      ref('orders'),
    ]);
    // public.users + public.orders share one read; audit.users gets its own.
    expect(read).toHaveBeenCalledTimes(2);
    expect(read.calls.map((c) => c.length).sort()).toEqual([1, 2]);
    expect(values.map((v) => v.ref.name).sort()).toEqual(['orders', 'users', 'users']);
  });

  it('dedupes a repeated relation within one read', async () => {
    const read = transport();
    const b = batcher(read);
    const values = await b.ensure(session, 's1', [ref('users'), ref('users')]);
    expect(read).toHaveBeenCalledTimes(1);
    expect(read.calls[0]).toHaveLength(1);
    expect(values).toHaveLength(1);
  });

  it('drops queued reads of a closed session instead of hanging', async () => {
    const read = transport();
    const b = batcher(read, 10_000);
    const pending = settle(b.ensure(session, 's1', [ref('users')]));
    b.invalidateSession('s1');
    const result = await pending;
    expect(result.ok).toBe(false);
    expect(result.ok === false && result.error).toMatchObject({
      message: expect.stringContaining('session s1 closed'),
    });
    expect(read).not.toHaveBeenCalled();
  });

  it('leaves other sessions queued when one session is closed', async () => {
    const read = transport();
    const b = batcher(read);
    const kept = settle(b.ensure(session, 's2', [ref('users')]));
    const dropped = settle(b.ensure(session, 's1', [ref('users')]));
    b.invalidateSession('s1');
    expect((await dropped).ok).toBe(false);
    expect((await kept).ok).toBe(true);
    expect(read).toHaveBeenCalledTimes(1);
  });

  it('reports how many reads are queued', async () => {
    const read = transport();
    const b = batcher(read, 10_000);
    void settle(b.ensure(session, 's1', [ref('users')]));
    void settle(b.ensure(session, 's2', [ref('users')]));
    expect(b.size).toBe(2);
    b.invalidateSession('s1');
    expect(b.size).toBe(1);
  });

  it('propagates a transport failure to every waiter of the read', async () => {
    const read = vi.fn(async () => {
      throw new Error('ipc down');
    });
    const b = batcher(read);
    const results = await Promise.allSettled([
      b.ensure(session, 's1', [ref('users')]),
      b.ensure(session, 's1', [ref('orders')]),
    ]);
    expect(results.map((r) => r.status)).toEqual(['rejected', 'rejected']);
  });
});