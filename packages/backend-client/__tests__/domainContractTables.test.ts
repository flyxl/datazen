import { describe, expect, it } from 'vitest';

import {
  createBackendClient,
  toId,
  toTimestamp,
  type BackendTransport,
  type MethodMap,
} from '../src/index';
import { createFakeTransport } from './client.test';

/**
 * The F5 per-domain contract tables: method × request shape × response shape,
 * one table per domain client, all through one table-driven runner.
 *
 * The runner asserts the three contract facts that matter on the wire:
 * 1. the **method name** the transport receives (the MethodMap key);
 * 2. the **request envelope** (flat vs `{request}`-nested vs token-folded);
 * 3. the **response passthrough** (the facade adds nothing, drops nothing).
 *
 * A new domain method that drifts on any of the three fails here before it
 * can fail in a window.
 */

interface ContractRow {
  /** Facade method exercised. */
  call: (client: ReturnType<typeof createBackendClient>) => Promise<unknown>;
  /** Method name the transport must receive. */
  method: keyof MethodMap;
  /** The exact payload the transport must receive. */
  payload: unknown;
  /** Canned response served for `method` (also asserted as the resolution). */
  response: unknown;
}

const token = {
  idempotencyKey: 'idem-1',
  expiresAt: mustNarrow(toTimestamp(1_700_000_000_000)),
};
const handle = {
  dbSessionId: mustNarrow(toId('sess-1')),
  runtimeEpoch: mustNarrow(toId('epoch-1')),
};
const openRequest = {
  initialTarget: {
    connectionId: mustNarrow(toId('conn-1')),
    namespace: { database: null, catalog: null, schema: 'public', path: [] },
    object: null,
  },
  owner: {
    kind: 'editor',
    clientInstanceId: mustNarrow(toId('c-1')),
    editorSessionId: mustNarrow(toId('e-1')),
  },
  idempotencyKey: 'idem-open-1',
};

function mustNarrow<T>(narrowed: T | undefined): T {
  if (narrowed === undefined) throw new Error('fixture did not narrow');
  return narrowed;
}

function runContractTable(domain: string, rows: ContractRow[]): void {
  describe(`${domain} domain contract table`, () => {
    for (const row of rows) {
      it(`${String(row.method)}: name, envelope and response are the contract`, async () => {
        const fake = createFakeTransport();
        fake.responses.set(row.method, row.response);
        const client = createBackendClient('desktop', fake.transport);

        await expect(row.call(client)).resolves.toEqual(row.response);
        expect(fake.calls).toHaveLength(1);
        expect(fake.calls[0]?.method).toBe(row.method);
        expect(fake.calls[0]?.payload).toEqual(row.payload);
      });
    }
  });
}

runContractTable('Settings', [
  {
    call: (c) => c.getSettings(),
    method: 'getSettings',
    payload: undefined,
    response: {
      preferences: {},
      execution: {},
      monitor: {},
      onboarding: null,
      rowCountMode: 'auto',
    },
  },
  {
    call: (c) =>
      c.saveSettings({
        preferences: {},
        execution: {},
        monitor: {},
        onboarding: null,
        rowCountMode: 'auto',
      } as never),
    method: 'saveSettings',
    payload: {
      settings: {
        preferences: {},
        execution: {},
        monitor: {},
        onboarding: null,
        rowCountMode: 'auto',
      },
    },
    response: {
      preferences: {},
      execution: {},
      monitor: {},
      onboarding: null,
      rowCountMode: 'auto',
    },
  },
]);

runContractTable('Identity', [
  {
    call: (c) => c.getPlatformIdentity(),
    method: 'getPlatformIdentity',
    payload: undefined,
    response: { clientInstanceId: 'c-1', organizationId: 'o-1', principalId: 'p-1' },
  },
  {
    call: (c) => c.issueSubmissionToken('session.open', handle),
    method: 'issueSubmissionToken',
    payload: { operation: 'session.open', handle },
    response: token,
  },
]);

runContractTable('Connection', [
  {
    call: (c) => c.listConnections(),
    method: 'listConnections',
    payload: undefined,
    response: [],
  },
  {
    call: (c) => c.createConnection({ name: 'pg', driverId: 'postgres' } as never, token),
    method: 'createConnection',
    payload: { name: 'pg', driverId: 'postgres', submissionToken: token },
    response: {},
  },
  {
    call: (c) => c.updateConnection(mustNarrow(toId('conn-2')), 4, { name: 'pg2' } as never),
    method: 'updateConnection',
    payload: {
      connectionId: mustNarrow(toId('conn-2')),
      expectedRevision: 4,
      patch: { name: 'pg2' },
    },
    response: {},
  },
  {
    call: (c) => c.disableConnection(mustNarrow(toId('conn-2')), 4),
    method: 'disableConnection',
    payload: { connectionId: mustNarrow(toId('conn-2')), expectedRevision: 4 },
    response: {},
  },
]);

runContractTable('Session', [
  {
    call: (c) => c.openSession(openRequest as never, token),
    method: 'openSession',
    payload: { ...openRequest, submissionToken: token },
    response: { session: handle },
  },
  {
    call: (c) => c.getSession(handle),
    method: 'getSession',
    payload: { handle },
    response: {},
  },
  {
    call: (c) => c.closeSession({ handle, mode: 'graceful' } as never),
    method: 'closeSession',
    payload: { handle, mode: 'graceful' },
    response: {},
  },
]);

runContractTable('Execution', [
  {
    call: (c) => c.getExecution(mustNarrow(toId('exec-1'))),
    method: 'getExecution',
    payload: { executionId: mustNarrow(toId('exec-1')) },
    response: {},
  },
]);

runContractTable('Artifact', [
  {
    call: (c) => c.readArtifact({ artifactId: mustNarrow(toId('art-1')), chunkIndex: 0 } as never),
    method: 'readArtifact',
    payload: { artifactId: mustNarrow(toId('art-1')), chunkIndex: 0 },
    response: { bytes: new Uint8Array(), eof: true },
  },
]);

runContractTable('DriverGateway', [
  {
    // The gateway rows ride the transport directly — the driver-sdk path.
    call: (c) => c.transport.call('get_driver_commands', { driverType: 'postgres' }),
    method: 'get_driver_commands',
    payload: { driverType: 'postgres' },
    response: [],
  },
]);

const userAssetToken = {
  idempotencyKey: 'idem-qa-1',
  expiresAt: mustNarrow(toTimestamp(1_700_000_000_000)),
};

runContractTable('QueryAssets', [
  {
    call: (c) => c.listQueryFavorites({ connectionId: 'conn-1' }),
    method: 'listQueryFavorites',
    payload: { connectionId: 'conn-1' },
    response: [{ id: 'fav-1', connectionId: 'conn-1', title: 'T', sql: 'SELECT 1' }],
  },
  {
    call: (c) =>
      c.createQueryFavorite(
        { connectionId: 'conn-1', title: 'T', sql: 'SELECT 1' },
        userAssetToken,
      ),
    method: 'createQueryFavorite',
    payload: {
      connectionId: 'conn-1',
      title: 'T',
      sql: 'SELECT 1',
      submissionToken: userAssetToken,
    },
    response: { id: 'fav-2', connectionId: 'conn-1', title: 'T', sql: 'SELECT 1' },
  },
  {
    call: (c) => c.deleteQueryFavorite({ id: 'fav-1' }, userAssetToken),
    method: 'deleteQueryFavorite',
    payload: { id: 'fav-1', submissionToken: userAssetToken },
    response: undefined,
  },
  {
    call: (c) => c.getQueryHistoryPage({ limit: 50, order: 'recent' }),
    method: 'getQueryHistoryPage',
    payload: { limit: 50, order: 'recent' },
    response: { entries: [], total: 0 },
  },
  {
    call: (c) => c.deleteQueryHistory({ id: 'h-1' }, userAssetToken),
    method: 'deleteQueryHistory',
    payload: { id: 'h-1', submissionToken: userAssetToken },
    response: 2,
  },
  {
    call: (c) => c.clearQueryHistory(userAssetToken),
    method: 'clearQueryHistory',
    payload: { submissionToken: userAssetToken },
    response: undefined,
  },
  {
    call: (c) => c.purgeHistory({ scope: 'query', retainDays: 30 }, userAssetToken),
    method: 'purgeHistory',
    payload: { scope: 'query', retainDays: 30, submissionToken: userAssetToken },
    response: 7,
  },
]);

runContractTable('SchemaDiff', [
  {
    call: (c) => c.listSchemaDiffProfiles(),
    method: 'listSchemaDiffProfiles',
    payload: {},
    response: [],
  },
  {
    call: (c) =>
      c.saveSchemaDiffProfile(
        { profile: { version: 1, id: 'sd-1', name: 'Pair' } as never },
        userAssetToken,
      ),
    method: 'saveSchemaDiffProfile',
    payload: {
      profile: { version: 1, id: 'sd-1', name: 'Pair' },
      submissionToken: userAssetToken,
    },
    response: undefined,
  },
  {
    call: (c) => c.deleteSchemaDiffProfile({ profileId: 'sd-1' }, userAssetToken),
    method: 'deleteSchemaDiffProfile',
    payload: { profileId: 'sd-1', submissionToken: userAssetToken },
    response: undefined,
  },
  {
    call: (c) =>
      c.compareTableSchemas({
        sourceDbSessionId: 'sess-1',
        targetDbSessionId: 'sess-2',
        sourceTableName: 't',
        targetTableName: 't',
      }),
    method: 'compareTableSchemas',
    payload: {
      sourceDbSessionId: 'sess-1',
      targetDbSessionId: 'sess-2',
      sourceTableName: 't',
      targetTableName: 't',
    },
    response: { table: 't', added: [], removed: [], changed: [] },
  },
  {
    call: (c) =>
      c.prepareSchemaDiffPlan(
        {
          sourceDbSessionId: 'sess-1',
          targetDbSessionId: 'sess-2',
          tableNames: ['t'],
          allowDestructive: false,
        },
        userAssetToken,
      ),
    method: 'prepareSchemaDiffPlan',
    payload: {
      sourceDbSessionId: 'sess-1',
      targetDbSessionId: 'sess-2',
      tableNames: ['t'],
      allowDestructive: false,
      submissionToken: userAssetToken,
    },
    response: { jobId: 'j-1', kind: 'schemaDiffPrepare', state: 'queued' },
  },
  {
    call: (c) =>
      c.prepareSchemaUnifiedPlan(
        {
          sourceDbSessionId: 'sess-1',
          targetDbSessionId: 'sess-2',
          tableNames: ['t'],
          allowDestructive: false,
          sourceObjects: [],
          targetObjects: [],
        } as never,
        userAssetToken,
      ),
    method: 'prepareSchemaUnifiedPlan',
    payload: {
      sourceDbSessionId: 'sess-1',
      targetDbSessionId: 'sess-2',
      tableNames: ['t'],
      allowDestructive: false,
      sourceObjects: [],
      targetObjects: [],
      submissionToken: userAssetToken,
    },
    response: { jobId: 'j-2', kind: 'schemaDiffPrepare', state: 'queued' },
  },
  {
    call: (c) =>
      c.executeSchemaDiffDeploy(
        { targetDbSessionId: 'sess-2', planId: 'plan-1', selectionRevision: 3 },
        userAssetToken,
      ),
    method: 'executeSchemaDiffDeploy',
    payload: {
      targetDbSessionId: 'sess-2',
      planId: 'plan-1',
      selectionRevision: 3,
      submissionToken: userAssetToken,
    },
    response: { jobId: 'j-3', kind: 'schemaDiffApply', state: 'queued' },
  },
  {
    call: (c) => c.listSchemaDiffJobs({ states: ['queued'], limit: 100 }),
    method: 'listSchemaDiffJobs',
    payload: { states: ['queued'], limit: 100 },
    response: [],
  },
  {
    call: (c) => c.getSchemaDiffJobDetails({ jobId: 'j-1' }),
    method: 'getSchemaDiffJobDetails',
    payload: { jobId: 'j-1' },
    response: {
      details: { job: { jobId: 'j-1' } },
      prepared: null,
      planUnavailableAfterRestart: false,
      deployResult: null,
    },
  },
  {
    call: (c) => c.cancelSchemaDiffDeploy({ jobId: 'j-1' }, userAssetToken),
    method: 'cancelSchemaDiffDeploy',
    payload: { jobId: 'j-1', submissionToken: userAssetToken },
    response: true,
  },
  {
    call: (c) =>
      c.verifySchemaDiffJobRecovery({ jobId: 'j-1', targetDbSessionId: 'sess-2' }, userAssetToken),
    method: 'verifySchemaDiffJobRecovery',
    payload: { jobId: 'j-1', targetDbSessionId: 'sess-2', submissionToken: userAssetToken },
    response: {
      details: { job: { jobId: 'j-1' } },
      prepared: null,
      planUnavailableAfterRestart: false,
      deployResult: null,
    },
  },
]);

describe('SchemaDiff tagged-enum folding (wire ↔ normalized)', () => {
  const taggedDetails = {
    details: { job: { jobId: 'j-1' } },
    prepared: {
      plan: {
        planId: 'plan-1',
        table: 't',
        tables: ['t'],
        sourceDialect: 'postgresql',
        targetDialect: 'sqlite',
        sameDialect: false,
        statements: [],
        warnings: [],
        requirements: [
          { backfill: { table: 't', column: 'c', reason: 'needs backfill' } },
          { unsupported: { operation: 't.c2', reason: 'unsupported kind' } },
        ],
        rollbackCompleteness: { complete: true, missing: [] },
      },
      planId: 'plan-1',
      selectionRevision: 1,
      planVersion: 1,
      handlerVersion: 1,
      checkpointVersion: 1,
      expiresAt: '2026-01-01T00:00:00Z',
      recoveryPolicy: 'verify',
    },
    planUnavailableAfterRestart: false,
    deployResult: null,
  };

  it('getSchemaDiffJobDetails folds tagged requirements into the discriminated union', async () => {
    const fake = createFakeTransport();
    fake.responses.set('getSchemaDiffJobDetails', taggedDetails);
    const client = createBackendClient('desktop', fake.transport);

    const details = await client.getSchemaDiffJobDetails({ jobId: 'j-1' });
    expect(details.prepared?.plan.requirements).toEqual([
      { kind: 'Backfill', table: 't', column: 'c', reason: 'needs backfill' },
      { kind: 'Unsupported', table: 't', column: 'c2', reason: 'unsupported kind' },
    ]);
  });

  it('executeSchemaDiffDeploy unfolds a normalized plan back into the tagged wire shape', async () => {
    const fake = createFakeTransport();
    fake.responses.set('executeSchemaDiffDeploy', {
      jobId: 'j-9',
      kind: 'schemaDiffApply',
      state: 'queued',
    });
    const client = createBackendClient('desktop', fake.transport);

    await client.executeSchemaDiffDeploy(
      {
        targetDbSessionId: 'sess-2',
        plan: {
          planId: 'plan-1',
          table: 't',
          tables: ['t'],
          sourceDialect: 'postgresql',
          targetDialect: 'sqlite',
          sameDialect: false,
          statements: [],
          warnings: [],
          requirements: [{ kind: 'Backfill', table: 't', column: 'c', reason: 'needs backfill' }],
          rollbackCompleteness: { complete: true, missing: [] },
        },
      },
      userAssetToken,
    );

    expect(fake.calls).toHaveLength(1);
    const plan = (fake.calls[0]!.payload as { plan: { requirements: unknown[] } }).plan;
    expect(plan.requirements).toEqual([
      { backfill: { table: 't', column: 'c', reason: 'needs backfill' } },
    ]);
  });
});

describe('per-domain guard contract (unbound → explicit error, bound → behavior)', () => {
  it('a declared capability gap fails on use with the §13 code, not a bare error', async () => {
    const transport: BackendTransport = {
      async call() {
        throw new Error('must not be reached');
      },
      unsupportedMethods: { listNamedViews: 'capability' },
    };
    const client = createBackendClient('web', transport);

    await expect(client.listNamedViews({ scope: 'connection' } as never)).rejects.toMatchObject({
      code: 'CapabilityUnsupported',
    });
  });

  it('a notImplemented gap reports itself as such — a different fact from never', async () => {
    const transport: BackendTransport = {
      async call() {
        throw new Error('must not be reached');
      },
      unsupportedMethods: { estimateTableRows: 'notImplemented' },
    };
    const client = createBackendClient('desktop', transport);

    await expect(client.estimateTableRows({} as never)).rejects.toMatchObject({
      code: 'CapabilityUnsupported',
      unsupportedKind: 'notImplemented',
    });
  });
});
