/**
 * The **narrow write path** — the one that actually reaches the database
 * today — driven through the **real** store action (no spy).
 *
 * The two channels of the result grid are separate: the narrow path is
 * governed by the narrow host-side mapping verdict, the origin path by the
 * driver's origin verdict. The back end refuses **every** result on the origin
 * channel today, because no driver implements the origin contract — its
 * verdict is always "the driver cannot tell" (`unknownOrigin`).
 *
 * So the question this file answers is sharp: **does the narrow write path
 * still work at all?** A result the narrow host-side check verified
 * (`SELECT * FROM <relation>`) is writable today — that path re-verifies the
 * mapping against the live catalog and then writes.
 *
 * Two cases:
 *
 * 1. no verdict has been fetched yet (the window before the answer arrives);
 * 2. the verdict has been fetched and the back end **refused the origin** —
 *    the production situation, and it must not take the narrow path down.
 *
 * The write command is counted on the transport, so the test shows whether a
 * write statement was actually produced. This file is also the first test in
 * the tree that drives a **real** store action instead of a spy: the earlier
 * component tests all replaced `updateResultCell`, so nothing exercised the
 * store's own guards end to end.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  createBackendClient,
  setBackendClient,
  type BackendTransport,
} from '@datazen/backend-client';
import { EMPTY_QUERY_EXEC, usePanelStore } from '../panelStore';
import { resultEditKey, usePanelResultEdit } from '../panelResultEdit';
import type { Panel } from '../panelStore';
import type { StatementResult } from '../../types';

const { calls } = vi.hoisted(() => ({ calls: [] as string[] }));

const transport: BackendTransport = {
  call: (async (method: string) => {
    calls.push(method);
    return undefined;
  }) as BackendTransport['call'],
};

const PANEL = {
  id: 'panel-query-1',
  type: 'query',
  connectionId: 'cfg-1',
  dbSessionId: 'sess-1',
  connectionName: 'App',
  databaseType: 'postgres',
  title: 'Query',
  database: 'app',
  schema: 'public',
} as unknown as Panel;

/** A result the narrow host-side check verified, with a live runtime binding. */
const RESULT = {
  sql: 'SELECT * FROM orders',
  columns: [
    { name: 'id', dataType: 'INTEGER', nullable: false },
    { name: 'total', dataType: 'INTEGER', nullable: true },
  ],
  rows: [
    [1, 10],
    [2, 20],
  ],
  executionTimeMs: 3,
  runtimeBinding: {
    executionId: 'exec-1',
    resourceBindingId: 'res-1',
    handle: { dbSessionId: 'sess-1', runtimeEpoch: 0 },
  },
  source: { writableMapping: 'verified', statementIndex: 0 },
} as unknown as StatementResult;

function seed() {
  usePanelStore.setState({
    panels: [PANEL],
    activePanelId: PANEL.id,
    queryExec: new Map([
      [
        PANEL.id,
        {
          ...EMPTY_QUERY_EXEC,
          sql: RESULT.sql,
          results: [RESULT],
          activeResultIdx: 0,
        },
      ],
    ]),
  });
}

beforeEach(() => {
  calls.length = 0;
  setBackendClient('desktop', createBackendClient('desktop', transport));
  usePanelResultEdit.setState({ verdicts: {}, resolving: {}, errors: {}, generations: {} });
  seed();
});

afterEach(() => {
  usePanelStore.setState({ panels: [], activePanelId: null, queryExec: new Map() });
});

describe('the narrow write path', () => {
  it('writes when no verdict has been fetched yet', async () => {
    await usePanelStore.getState().updateResultCell(PANEL.id, 0, 0, 'total', 11);

    expect(calls).toContain('commitQueryResultCell');
  });

  it('writes when the back end refused the origin — the narrow channel is its own', async () => {
    // The production situation: the grid asked for the verdict, and the back
    // end answered "the driver cannot tell" — which is every answer today,
    // because no driver implements the origin contract.
    usePanelResultEdit.setState({
      verdicts: {
        [resultEditKey('sess-1', 0)]: {
          writable: false,
          identityColumns: [],
          editableColumns: [],
          table: null,
          reason: 'unknownOrigin',
        },
      },
      resolving: {},
      errors: {},
      generations: {},
    });

    let thrown: unknown = null;
    try {
      await usePanelStore.getState().updateResultCell(PANEL.id, 0, 0, 'total', 11);
    } catch (error) {
      thrown = error;
    }

    // The narrow path re-verifies the mapping against the live catalog and
    // then writes. A back-end refusal of the *origin* must not silently take
    // that away: the two channels are separate, and closing the narrow one
    // with the origin verdict would remove a feature that works.
    expect(thrown).toBeNull();
    expect(calls).toContain('commitQueryResultCell');
  });
});
