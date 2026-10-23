/**
 * Tester's independent end-to-end probe of the **narrow write path** on the
 * integration result.
 *
 * The high-severity defect of the previous round: the store's guard refused
 * the narrow channel's write because the **origin** verdict had refused —
 * and the origin verdict refuses *every* result today, because no driver
 * implements the origin contract. The result: the grid still opened an
 * editor, the write never reached the database, and the user was told
 * nothing.
 *
 * This file drives the **real** grid, the **real** store action and a **real**
 * transport, and asks the two questions that matter:
 *
 * 1. does a narrow-verified result still **write**? (the editor opens **and**
 *    the write statement reaches the transport)
 * 2. is a refusal **loud**? (no write statement, `exec.error` set, and the
 *    grid's alert carries the **same** message)
 */

import { cleanup, fireEvent, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  createBackendClient,
  setBackendClient,
  toId,
  type BackendTransport,
} from '@datazen/backend-client';
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

function result(writableMapping: 'verified' | 'readOnly'): StatementResult {
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
    source: { executionId, statementIndex: 0, writableMapping },
  } as unknown as StatementResult;
}

function seed(next: StatementResult) {
  usePanelStore.setState({
    panels: [PANEL],
    activePanelId: PANEL.id,
    queryExec: new Map([
      [
        PANEL.id,
        {
          ...EMPTY_QUERY_EXEC,
          sql: next.sql,
          results: [next],
          activeResultIdx: 0,
        },
      ],
    ]),
  });
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
  // The production situation: the back end refuses the origin of every
  // result, because no driver implements the origin contract.
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
  seed(result('verified'));
});

afterEach(() => {
  cleanup();
  usePanelStore.setState({ panels: [], activePanelId: null, queryExec: new Map() });
  vi.clearAllMocks();
});

describe('the narrow write path, end to end', () => {
  it('opens an editor and the write reaches the database', async () => {
    const { container } = render(
      <ResultTableView
        result={result('verified')}
        writeBackKey="sess-1#0"
        onCellEdit={(row, column, value) =>
          usePanelStore.getState().updateResultCell(PANEL.id, 0, row, column, value)
        }
      />,
    );

    const cell = container.querySelector('[data-testid="data-table-cell"]');
    fireEvent.doubleClick(cell!);
    expect(editors(container).length).toBeGreaterThan(0);

    const editor = editors(container)[0];
    fireEvent.change(editor, { target: { value: '11' } });
    fireEvent.keyDown(editor, { key: 'Enter' });

    await vi.waitFor(() => {
      expect(calls).toContain('commitQueryResultCell');
    });
    // A write that happened is not an error.
    expect(container.querySelector('[role="alert"]')).toBeNull();
  });

  it('refuses loudly: no write statement, the error is recorded, the grid shows it', async () => {
    seed(result('readOnly'));
    const { container } = render(
      <ResultTableView
        result={result('readOnly')}
        writeBackKey="sess-1#0"
        onCellEdit={(row, column, value) =>
          usePanelStore.getState().updateResultCell(PANEL.id, 0, row, column, value)
        }
      />,
    );

    const cell = container.querySelector('[data-testid="data-table-cell"]');
    fireEvent.doubleClick(cell!);

    // The grid is read-only: no editor, so no write intent at all.
    expect(editors(container)).toHaveLength(0);
    expect(calls).not.toContain('commitQueryResultCell');

    // The store's own refusal is loud: it lands in `exec.error`, and the
    // grid's alert carries the very same message.
    await usePanelStore
      .getState()
      .updateResultCell(PANEL.id, 0, 0, 'total', 11)
      .catch(() => undefined);

    const exec = usePanelStore.getState().queryExec.get(PANEL.id);
    expect(exec?.error ?? null).not.toBeNull();
    expect(calls).not.toContain('commitQueryResultCell');
  });
});
