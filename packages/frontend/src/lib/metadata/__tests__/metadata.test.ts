import { describe, expect, it } from 'vitest';
import type { Counter, ExecutionTarget, Id, ResultProvenance, SessionHandle, Timestamp } from '@datazen/backend-client';
import {
  checkTemporaryResult,
  isProvenanceCurrent,
  metadataBatchBucket,
  metadataCacheKey,
  metadataConfigBucket,
  MetadataBatcher,
  readTemporaryResult,
} from '../index';

const id = (s: string) => s as Id;
const counter = (n: number) => n as Counter;
const ts = (n: number) => n as Timestamp;

function handle(dbSession = 's1', epoch = 'e1'): SessionHandle {
  return { dbSessionId: id(dbSession), runtimeEpoch: id(epoch) };
}

function target(database: string | null = 'db', schema: string | null = 'public'): ExecutionTarget {
  return {
    connectionId: id('c1'),
    namespace: { database, catalog: null, schema, path: [] },
    object: null,
  };
}

function provenance(overrides: Partial<ResultProvenance> = {}): ResultProvenance {
  return {
    organizationId: id('org'),
    principalId: id('p1'),
    connectionId: id('c1'),
    configRevision: counter(3),
    contextBefore: {} as ResultProvenance['contextBefore'],
    contextAfter: {} as ResultProvenance['contextAfter'],
    requestedTarget: target(),
    capabilitySnapshot: {} as ResultProvenance['capabilitySnapshot'],
    executedAt: ts(0),
    ...overrides,
  };
}

describe('metadataCacheKey', () => {
  const input = { connectionId: id('c1'), configRevision: counter(3), dbSessionId: id('s1'), target: target() };

  it('isolates by connection / config / target / dbSession', () => {
    const base = metadataCacheKey(input);
    expect(metadataCacheKey({ ...input, connectionId: id('c2') })).not.toBe(base);
    expect(metadataCacheKey({ ...input, configRevision: counter(4) })).not.toBe(base);
    expect(metadataCacheKey({ ...input, dbSessionId: id('s2') })).not.toBe(base);
    expect(metadataCacheKey({ ...input, target: target('db2') })).not.toBe(base);
    expect(metadataCacheKey({ ...input, target: target('db', 'other') })).not.toBe(base);
  });

  it('is stable for identical identity/config/target', () => {
    expect(metadataCacheKey(input)).toBe(metadataCacheKey({ ...input }));
  });

  it('batch bucket isolates by target namespace', () => {
    expect(metadataBatchBucket(input)).not.toBe(
      metadataBatchBucket({ ...input, target: target('db2') }),
    );
    expect(metadataBatchBucket(input)).toBe(metadataBatchBucket({ ...input }));
  });

  it('config bucket ignores target', () => {
    expect(metadataConfigBucket(input)).toBe(metadataConfigBucket({ ...input, target: target('db2') }));
  });
});

describe('TemporaryResult invalidation', () => {
  const current = {
    handle: handle(),
    connectionId: 'c1',
    configRevision: 3,
    target: target(),
  };

  it('drops temp results when the runtime epoch changes', () => {
    const entry = { originalHandle: handle(), provenance: provenance(), value: 42 };
    expect(checkTemporaryResult(entry, { ...current, handle: handle('s1', 'e2') }).valid).toBe(false);
    expect(readTemporaryResult(entry, { ...current, handle: handle('s1', 'e2') })).toBeNull();
  });

  it('drops temp results when the db session changes', () => {
    const entry = { originalHandle: handle(), provenance: provenance(), value: 42 };
    expect(readTemporaryResult(entry, { ...current, handle: handle('s2', 'e1') })).toBeNull();
  });

  it('drops temp results when connection or config revision changes', () => {
    const entry = { originalHandle: handle(), provenance: provenance(), value: 42 };
    expect(readTemporaryResult(entry, { ...current, connectionId: 'c2' })).toBeNull();
    expect(readTemporaryResult(entry, { ...current, configRevision: 4 })).toBeNull();
  });

  it('drops temp results when the target namespace changes', () => {
    const entry = { originalHandle: verifiedProvenance(), provenance: provenance(), value: 42 };
    expect(readTemporaryResult(entry, { ...current, target: target('db2') })).toBeNull();
  });

  it('accepts entries that match the current handle + config + target', () => {
    const entry = { originalHandle: handle(), provenance: provenance(), value: 42 };
    expect(readTemporaryResult(entry, current)).toBe(42);
  });

  it('isProvenanceCurrent classifies old provenance as stale', () => {
    expect(isProvenanceCurrent(provenance(), { connectionId: 'c1', configRevision: 3, target: target() })).toBe(true);
    expect(isProvenanceCurrent(provenance(), { connectionId: 'c1', configRevision: 4, target: target() })).toBe(false);
    expect(isProvenanceCurrent(provenance(), { connectionId: 'c2', configRevision: 3, target: target() })).toBe(false);
    expect(isProvenanceCurrent(provenance(), { connectionId: 'c1', configRevision: 3, target: target('db2') })).toBe(false);
  });
});

describe('MetadataBatcher', () => {
  it('groups keys by identity/config/target bucket and dedupes', async () => {
    const batches: Array<{ bucket: string; keys: string[] }> = [];
    const batcher = new MetadataBatcher((bucket, keys) => {
      batches.push({ bucket, keys });
    });
    const a = { connectionId: id('c1'), configRevision: counter(1), dbSessionId: id('s1'), target: target() };
    const b = { ...a, target: target('db2') };
    const c = { ...a, dbSessionId: id('s2') };
    batcher.enqueue(a, 'k1');
    batcher.enqueue(a, 'k1');
    batcher.enqueue(a, 'k2');
    batcher.enqueue(b, 'k1');
    batcher.enqueue(c, 'k3');
    await batcher.flushNow();
    expect(batches.length).toBe(3);
    const aBatch = batches.find((x) => x.keys.length === 2);
    expect(aBatch?.keys.sort()).toEqual(['k1', 'k2']);
    expect(batches.some((x) => x.keys.join() === 'k1')).toBe(true);
    expect(batches.some((x) => x.keys.join() === 'k3')).toBe(true);
  });

  it('invalidate drops only matching buckets', async () => {
    const batches: Array<{ bucket: string; keys: string[] }> = [];
    const batcher = new MetadataBatcher((bucket, keys) => {
      batches.push({ bucket, keys });
    });
    const a = { connectionId: id('c1'), configRevision: counter(1), dbSessionId: id('s1'), target: target() };
    batcher.enqueue(a, 'k1');
    batcher.enqueue({ ...a, configRevision: counter(2) }, 'k2');
    batcher.invalidate((b) => b.includes(',2,'));
    await batcher.flushNow();
    expect(batches.length).toBe(1);
    expect(batches[0].keys).toEqual(['k1']);
  });
});

function verifiedProvenance(): SessionHandle {
  return handle();
}
