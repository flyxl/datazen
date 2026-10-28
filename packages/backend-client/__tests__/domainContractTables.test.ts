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
