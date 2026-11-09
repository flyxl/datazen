import { describe, expect, it } from 'vitest';

import {
  createBackendClient,
  toTimestamp,
  type MethodMap,
  type WorkflowDefinition,
} from '../src/index';
import { createFakeTransport } from './client.test';

/**
 * The Workflow domain contract table (§6.14, W8 刀3c): method × request shape
 * × response shape, one row per method, through the same table-driven runner
 * as `domainContractTables.test.ts`.
 *
 * The runner asserts the three contract facts that matter on the wire:
 * 1. the **method name** the transport receives (the MethodMap key);
 * 2. the **request envelope** (id-addressed bags; the token-folded
 *    `{ ...request, submissionToken }` shape on the three body-carrying write
 *    faces; `{}` on the no-arg faces);
 * 3. the **response passthrough** (the facade adds nothing, drops nothing —
 *    the web transport's `{ yaml }` / `{ cleared }` unwrapping happens below
 *    this boundary, so the domain client sees the bare scalar).
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

function mustNarrow<T>(narrowed: T | undefined): T {
  if (narrowed === undefined) throw new Error('fixture did not narrow');
  return narrowed;
}

const token = {
  idempotencyKey: 'idem-wf-1',
  expiresAt: mustNarrow(toTimestamp(1_700_000_000_000)),
};

const definition: WorkflowDefinition = {
  id: 'wf-1',
  name: 'Nightly checksum',
  description: 'Recompute per-table checksums',
  variables: [{ name: 'table', type: 'string', description: 'Which table to check' }],
  steps: [{ type: 'query', id: 's1', sql: 'SELECT count(*) FROM users' }],
};

const executionResult = {
  success: true,
  finalOutput: 'ok',
  steps: [],
  totalTimeMs: 12,
};

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

runContractTable('Workflow', [
  {
    call: (c) => c.listWorkflows(),
    method: 'listWorkflows',
    payload: {},
    response: [{ id: 'wf-1', name: 'Nightly checksum', description: '', variables: [] }],
  },
  {
    call: (c) => c.getWorkflow({ workflowId: 'wf-1' }),
    method: 'getWorkflow',
    payload: { workflowId: 'wf-1' },
    response: definition,
  },
  {
    call: (c) => c.getWorkflowYaml({ workflowId: 'wf-1' }),
    method: 'getWorkflowYaml',
    payload: { workflowId: 'wf-1' },
    response: 'id: wf-1\nname: Nightly checksum\n',
  },
  {
    // The definition rides inside the `{ workflow }` bag (IPC-shaped domain
    // form); the web route unfolds it into the flat contract body.
    call: (c) => c.saveWorkflow({ workflow: definition }, token),
    method: 'saveWorkflow',
    payload: { workflow: definition, submissionToken: token },
    response: undefined,
  },
  {
    call: (c) => c.saveWorkflowYaml({ yaml: 'id: wf-2\nname: Imported\n' }, token),
    method: 'saveWorkflowYaml',
    payload: { yaml: 'id: wf-2\nname: Imported\n', submissionToken: token },
    response: { ...definition, id: 'wf-2', name: 'Imported' },
  },
  {
    call: (c) => c.deleteWorkflow({ workflowId: 'wf-1' }),
    method: 'deleteWorkflow',
    payload: { workflowId: 'wf-1' },
    response: undefined,
  },
  {
    call: (c) =>
      c.executeWorkflow(
        { workflowId: 'wf-1', variables: { table: 'users' }, connectionId: 'conn-1' },
        token,
      ),
    method: 'executeWorkflow',
    payload: {
      workflowId: 'wf-1',
      variables: { table: 'users' },
      connectionId: 'conn-1',
      submissionToken: token,
    },
    response: executionResult,
  },
  {
    call: (c) => c.reloadWorkflows(),
    method: 'reloadWorkflows',
    payload: {},
    response: undefined,
  },
  {
    call: (c) => c.listWorkflowRuns({ workflowId: 'wf-1' }),
    method: 'listWorkflowRuns',
    payload: { workflowId: 'wf-1' },
    response: [
      {
        id: 'run-1',
        workflowId: 'wf-1',
        workflowName: 'Nightly checksum',
        success: true,
        totalTimeMs: 12,
        createdAt: '2026-11-07T00:00:00Z',
      },
    ],
  },
  {
    // The parameter keeps the desktop IPC bag's `historyId` name even though
    // the contract renamed the noun (run, not history).
    call: (c) => c.getWorkflowRun({ historyId: 'run-1' }),
    method: 'getWorkflowRun',
    payload: { historyId: 'run-1' },
    response: {
      id: 'run-1',
      workflowId: 'wf-1',
      workflowName: 'Nightly checksum',
      variables: { table: 'users' },
      result: executionResult,
      createdAt: '2026-11-07T00:00:00Z',
    },
  },
  {
    // No argument = the whole recorded history; the facade normalizes the
    // absent filter to `{}` rather than leaking a `workflowId: null` key.
    call: (c) => c.clearWorkflowRuns(),
    method: 'clearWorkflowRuns',
    payload: {},
    response: 3,
  },
]);

describe('workflow domain optional-argument defaults', () => {
  it('listWorkflowRuns without arguments sends the empty bag (absent workflowId = the whole history)', async () => {
    const fake = createFakeTransport();
    fake.responses.set('listWorkflowRuns', []);
    const client = createBackendClient('desktop', fake.transport);
    await expect(client.listWorkflowRuns()).resolves.toEqual([]);
    expect(fake.calls[0]?.method).toBe('listWorkflowRuns');
    expect(fake.calls[0]?.payload).toEqual({});
  });
});
