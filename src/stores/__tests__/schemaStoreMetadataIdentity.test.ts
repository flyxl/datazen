import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { RelationRef } from '@datazen/driver-sdk';
import { relationColumnsCacheKey, type SessionMetadataIdentity } from '../schemaMetadataKeys';

/**
 * Relation columns are cached under identity (connection config + its
 * revision), runtime session and target. These tests pin the behaviour that
 * follows from that: a session bound to another connection config never serves
 * the columns read under the previous one, and neither does a request that
 * was already in flight when the binding changed.
 */

const { mockReadColumns } = vi.hoisted(() => ({
  mockReadColumns: vi.fn(async (_dbSessionId: string, refs: readonly RelationRef[]) => ({
    results: refs.map((ref) => ({
      status: 'ok' as const,
      value: {
        ref,
        columns: [{ name: 'id', dataType: 'integer', nullable: false }],
        primaryKeys: ['id'],
      },
    })),
  })),
}));

vi.mock('@datazen/driver-sdk', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/driver-sdk')>()),
  schemaClient: { readColumns: mockReadColumns },
}));

vi.mock('../../commands/database', () => ({
  databaseCommands: {
    getDatabases: vi.fn().mockResolvedValue(['testdb']),
    listTables: vi
      .fn()
      .mockResolvedValue([{ name: 'users', tableType: 'TABLE', schema: 'public', rowCount: null }]),
  },
}));

const mockConnectionCommands = {
  connect: vi.fn(),
  testConnection: vi.fn(),
  getConnectionInfo: vi.fn(),
  disconnect: vi.fn(),
};

vi.mock('../../commands/connection', () => ({
  connectionCommands: mockConnectionCommands,
}));

vi.mock('../../lib/crossWindowBus', () => ({
  emitCrossWindow: vi.fn().mockResolvedValue(undefined),
}));

const DB_SESSION = 'db-session-1';

type SchemaStoreModule = typeof import('../schemaStore');
type ActiveConnectionStoreModule = typeof import('../activeConnectionStore');

let useSchemaStore: SchemaStoreModule['useSchemaStore'];
let useActiveConnectionStore: ActiveConnectionStoreModule['useActiveConnectionStore'];

function entry() {
  return useSchemaStore.getState().schemas.get(DB_SESSION)!;
}

async function seedSession(): Promise<void> {
  await useSchemaStore.getState().loadForConnection(DB_SESSION);
  useSchemaStore.getState().bindMetadataIdentity(DB_SESSION, 'cfg-1');
}

beforeEach(async () => {
  vi.resetModules();
  vi.clearAllMocks();
  mockReadColumns.mockImplementation(async (_dbSessionId, refs) => ({
    results: refs.map((ref) => ({
      status: 'ok' as const,
      value: {
        ref,
        columns: [{ name: 'id', dataType: 'integer', nullable: false }],
        primaryKeys: ['id'],
      },
    })),
  }));
  mockConnectionCommands.getConnectionInfo.mockResolvedValue(null);
  ({ useSchemaStore } = await import('../schemaStore'));
  ({ useActiveConnectionStore } = await import('../activeConnectionStore'));
  useSchemaStore.getState().reset();
  useActiveConnectionStore.getState().reset();
});

describe('bindMetadataIdentity', () => {
  it('binds an entry to its connection config and bumps the config revision', async () => {
    await useSchemaStore.getState().loadForConnection(DB_SESSION);
    const before = entry();
    expect(before.connectionId).toBe('');
    expect(before.metadataRevision).toBe(0);

    useSchemaStore.getState().bindMetadataIdentity(DB_SESSION, 'cfg-1');

    expect(entry().connectionId).toBe('cfg-1');
    expect(entry().metadataRevision).toBe(before.metadataRevision + 1);
  });

  it('is idempotent for the same connection config so a cache survives', async () => {
    await seedSession();
    await useSchemaStore.getState().ensureColumns(['users'], DB_SESSION, 'testdb');
    const revision = entry().metadataRevision;
    expect(Object.keys(entry().relationColumns)).toHaveLength(1);

    useSchemaStore.getState().bindMetadataIdentity(DB_SESSION, 'cfg-1');

    expect(entry().metadataRevision).toBe(revision);
    expect(Object.keys(entry().relationColumns)).toHaveLength(1);
  });

  it('drops cached columns and in-flight markers when the config changes', async () => {
    await seedSession();
    await useSchemaStore.getState().ensureColumns(['users'], DB_SESSION, 'testdb');
    const revision = entry().metadataRevision;

    useSchemaStore.getState().bindMetadataIdentity(DB_SESSION, 'cfg-2');

    expect(entry().connectionId).toBe('cfg-2');
    expect(entry().metadataRevision).toBe(revision + 1);
    expect(entry().relationColumns).toEqual({});
    expect(entry().columnInflight.size).toBe(0);
  });

  it('ignores a bind without a session or a config id', async () => {
    await seedSession();
    const before = entry();

    useSchemaStore.getState().bindMetadataIdentity('', 'cfg-9');
    useSchemaStore.getState().bindMetadataIdentity(DB_SESSION, '');

    expect(entry().connectionId).toBe(before.connectionId);
    expect(entry().metadataRevision).toBe(before.metadataRevision);
  });
});

describe('relation columns are cached per metadata identity', () => {
  it('re-reads columns after the connection config changed instead of reusing them', async () => {
    await seedSession();
    await useSchemaStore.getState().ensureColumns(['users'], DB_SESSION, 'testdb');
    expect(mockReadColumns).toHaveBeenCalledTimes(1);
    const first = { ...entry().relationColumns };
    expect(Object.keys(first)).toHaveLength(1);

    useSchemaStore.getState().bindMetadataIdentity(DB_SESSION, 'cfg-2');
    await useSchemaStore.getState().ensureColumns(['users'], DB_SESSION, 'testdb');

    expect(mockReadColumns).toHaveBeenCalledTimes(2);
    const second = entry().relationColumns;
    expect(Object.keys(second)).toHaveLength(1);
    // Different identity, so a different cache key: the old entry is gone, not
    // silently re-labelled.
    expect(Object.keys(second)[0]).not.toBe(Object.keys(first)[0]);
  });

  it('does not write a result whose request was issued under a superseded identity', async () => {
    await seedSession();
    let release: (() => void) | null = null;
    mockReadColumns.mockImplementationOnce(async (_dbSessionId, refs) => {
      await new Promise<void>((resolve) => {
        release = resolve;
      });
      return {
        results: refs.map((ref) => ({
          status: 'ok' as const,
          value: {
            ref,
            columns: [{ name: 'id', dataType: 'integer', nullable: false }],
            primaryKeys: ['id'],
          },
        })),
      };
    });

    const pending = useSchemaStore.getState().ensureColumns(['users'], DB_SESSION, 'testdb');
    useSchemaStore.getState().bindMetadataIdentity(DB_SESSION, 'cfg-2');
    release!();
    await pending;

    expect(entry().relationColumns).toEqual({});
    expect(entry().columnInflight.size).toBe(0);
  });

  it('keeps a failed read out of the cache so a later call can retry', async () => {
    await seedSession();
    mockReadColumns.mockRejectedValueOnce(new Error('ipc down'));

    await useSchemaStore.getState().ensureColumns(['users'], DB_SESSION, 'testdb');

    expect(entry().relationColumns).toEqual({});
    expect(entry().columnInflight.size).toBe(0);

    await useSchemaStore.getState().ensureColumns(['users'], DB_SESSION, 'testdb');
    expect(Object.keys(entry().relationColumns)).toHaveLength(1);
  });
});

describe('relationColumnsCacheKey', () => {
  const ref = (overrides: Partial<RelationRef> = {}): RelationRef => ({
    database: 'db1',
    schema: 'public',
    name: 'users',
    ...overrides,
  });
  const identity = (overrides: Partial<SessionMetadataIdentity> = {}): SessionMetadataIdentity => ({
    connectionId: 'cfg-1',
    metadataRevision: 1,
    ...overrides,
  });

  it('is stable for the same identity, session and relation', () => {
    expect(relationColumnsCacheKey(identity(), 's1', ref())).toBe(
      relationColumnsCacheKey(identity(), 's1', ref()),
    );
  });

  it('discriminates connection config, config revision, session and target', () => {
    const base = relationColumnsCacheKey(identity(), 's1', ref());
    const others = [
      relationColumnsCacheKey(identity({ connectionId: 'cfg-2' }), 's1', ref()),
      relationColumnsCacheKey(identity({ metadataRevision: 2 }), 's1', ref()),
      relationColumnsCacheKey(identity(), 's2', ref()),
      relationColumnsCacheKey(identity(), 's1', ref({ name: 'orders' })),
      relationColumnsCacheKey(identity(), 's1', ref({ schema: 'other' })),
      relationColumnsCacheKey(identity(), 's1', ref({ database: 'db2' })),
    ];
    expect(new Set([base, ...others]).size).toBe(others.length + 1);
  });

  it('treats a missing schema as its own target, not as the empty string', () => {
    expect(relationColumnsCacheKey(identity(), 's1', ref({ schema: null }))).not.toBe(
      relationColumnsCacheKey(identity(), 's1', ref({ schema: '' })),
    );
  });
});

describe('activeConnectionStore binds the schema identity', () => {
  it('binds on connect', async () => {
    mockConnectionCommands.connect.mockResolvedValueOnce(DB_SESSION);
    mockConnectionCommands.testConnection.mockResolvedValueOnce({ version: '16' });

    await useActiveConnectionStore.getState().connect({
      id: 'cfg-1',
      name: 'Test',
      databaseType: 'postgresql',
      sslMode: 'prefer',
      database: 'testdb',
    });

    expect(entry().connectionId).toBe('cfg-1');
    expect(entry().metadataRevision).toBe(1);
  });

  it('binds on an externally reported session (tab restore)', () => {
    useActiveConnectionStore.getState().markConnected('cfg-1', DB_SESSION);

    expect(entry().connectionId).toBe('cfg-1');
    expect(entry().metadataRevision).toBe(1);
  });
});
