/**
 * The real grid with the real store action: **a narrow-verified result is
 * edited and the write reaches the database.**
 *
 * This file is the counterpart of the defect it pins down. The front end
 * keeps the narrow mapping verdict (a result the narrow host-side check
 * verified is editable, and the origin verdict does not withdraw it), and the
 * store's guard used to refuse **every** write — because the back end refuses
 * every result on the *origin* channel today, no driver implementing the
 * origin contract. The two halves therefore disagreed: the grid opened an
 * editor, the commit was refused, and the write never reached the database
 * (PRD §10 row 11: "前端说能写、后端说不能，用户看到可编辑却总是失败").
 *
 * The channels are separate now, and this file drives the **real** grid with
 * the **real** store action over a **real** transport — so the write is
 * counted where it really happens.
 */

import { cleanup, fireEvent, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  createBackendClient,
  setBackendClient,
  type BackendTransport,
} from '@datazen/backend-client';
import { toId } from '@datazen/backend-client';
import type { StatementResult } from '../../../../types';
import { ResultTableView } from '../ResultTableView';
import { usePanelResultEdit } from '../../../../stores/panelResultEdit';
import { EMPTY_QUERY_EXEC, usePanelStore } from '../../../../stores/panelStore';
import type { Panel } from '../../../../stores/panelStore';

const { calls } = vi.hoisted(() => ({ calls: [] as string[] }));

const transport: BackendTransport = {
  call: (async (method: string) => {
    calls.push(method);
    return undefined;
  }) as BackendTransport['call'],
};

vi.mock('../../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

const settingsState = vi.hoisted(() => ({ queryResultLimit: 5000, safeMode: false }));

vi.mock('../../../../stores/settingsStore', () => ({
  useSettingsStore: (selector: (state: { settings: typeof settingsState }) => unknown) =>
    selector({ settings: settingsState }),
}));

vi.mock('../../../../hooks/useVirtualTable', () => ({
  useVirtualTable: ({ rows }: { rows: unknown[][] }) => ({
    virtualRows: rows.map((_, index) => ({ index, key: String(index), start: index * 40 })),
    totalHeight: rows.length * 40,
  }),
}));

vi.mock('../../../../hooks/useColumnResize', () => ({
  useColumnResize: () => ({
    columnWidths: [160, 200],
    onResizeStart: vi.fn(),
  }),
  adjustWidthsForSort: (widths: number[]) => widths,
}));

vi.mock('../../../../lib/nativeContextMenu', () => ({
  showNativeContextMenu: vi.fn().mockResolvedValue(undefined),
}));

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
function verifiedResult(): StatementResult {
  const executionId = toId('execution-1')!;
  return {
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
      executionId,
      resourceBindingId: toId('resource-1')!,
      handle: { dbSessionId: toId('session-1')!, runtimeEpoch: toId('epoch-1')! },
    },
    source: {
      executionId,
      statementIndex: 0,
      writableMapping: 'verified',
      relation: {
        connectionId: toId('connection-1')!,
        namespace: { database: 'app', catalog: null, schema: 'public', path: [] },
        object: { kind: 'table', name: 'orders', signature: null },
      },
      context: {
        namespace: { database: 'app', catalog: null, schema: 'public', path: [] },
        searchPath: null,
        effectiveIdentity: null,
        transactionState: 'none',
        autocommit: true,
        confidence: 'confirmed',
      },
    },
  } as unknown as StatementResult;
}

function editors(container: HTMLElement): HTMLElement[] {
  return Array.from(
    container.querySelectorAll(
      '[data-testid="data-table-cell"] input, [data-testid="data-table-cell"] textarea',
    ),
  );
}

beforeEach(() => {
  calls.length = 0;
  settingsState.safeMode = false;
  setBackendClient('desktop', createBackendClient('desktop', transport));
  usePanelResultEdit.setState({
    verdicts: {
      'sess-1#0': {
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
  usePanelStore.setState({
    panels: [PANEL],
    activePanelId: PANEL.id,
    queryExec: new Map([
      [
        PANEL.id,
        {
          ...EMPTY_QUERY_EXEC,
          sql: 'SELECT * FROM orders',
          results: [verifiedResult()],
          activeResultIdx: 0,
        },
      ],
    ]),
  });
});

afterEach(() => {
  cleanup();
  usePanelStore.setState({ panels: [], activePanelId: null, queryExec: new Map() });
  vi.clearAllMocks();
});

describe('the grid and the store on a narrow-verified result', () => {
  it('opens an editor and the write reaches the database', async () => {
    const { container } = render(
      <ResultTableView
        result={verifiedResult()}
        writeBackKey="sess-1#0"
        onCellEdit={(row, column, value) =>
          usePanelStore.getState().updateResultCell(PANEL.id, 0, row, column, value)
        }
      />,
    );

    const cell = container.querySelector('[data-testid="data-table-cell"]');
    fireEvent.doubleClick(cell!);

    // The grid says the cell is editable: the narrow verdict is not withdrawn.
    expect(editors(container).length).toBeGreaterThan(0);

    const editor = editors(container)[0];
    fireEvent.change(editor, { target: { value: '11' } });
    fireEvent.keyDown(editor, { key: 'Enter' });

    // The write was produced: the narrow channel is governed by the narrow
    // mapping verdict, which the back end re-verifies against the live
    // catalog — not by the origin verdict, which governs the other channel.
    await vi.waitFor(() => {
      expect(calls).toContain('commitQueryResultCell');
    });
    expect(container.querySelector('[role="alert"]')).toBeNull();
  });
});
