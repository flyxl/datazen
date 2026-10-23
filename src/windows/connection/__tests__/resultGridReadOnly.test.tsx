/**
 * The result grid's read-only behaviour, with the **real** DetailPanel.
 *
 * The branch's other component test replaces the drawer with a mock, so it
 * cannot observe an edit gesture at all. This file drives the real one, and
 * pins three facts:
 *
 * 1. **§1.2.2** — while the verdict refuses the result, none of the edit
 *    gestures (double click, Enter, F2, direct typing, paste) produces a write
 *    intent, and no editor input is ever rendered.
 * 2. **the release channel** — the origin verdict alone must not open the
 *    grid: the write path that could carry such a write is not connected yet,
 *    so an opened grid would claim an edit that is silently refused. The grid
 *    therefore stays read-only **and says why**.
 * 3. **the verdict cache** — the verdict of the result that is no longer on
 *    screen is dropped together with the rows it was made for.
 *
 * D7: no rendered copy is pinned — the reason is read from the contract
 * attribute `data-dt-grid-readonly-reason`.
 */

import { cleanup, fireEvent, render, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ContentViewDrawers } from '../ContentViewDrawers';
import { EMPTY_QUERY_EXEC, usePanelStore } from '../../../stores/panelStore';
import { usePanelResultEdit } from '../../../stores/panelResultEdit';
import { useTableDataStore } from '../../../stores/tableDataStore';
import type { Panel } from '../../../stores/panelStore';
import type { StatementResult } from '../../../types';

const { resolveResultOrigin, commitQueryResultCell } = vi.hoisted(() => ({
  resolveResultOrigin: vi.fn(),
  commitQueryResultCell: vi.fn(async (..._args: unknown[]) => undefined),
}));

vi.mock('../../../hooks/useResizable', () => ({
  useResizable: () => ({ size: 320, handleRef: { current: null }, resize: vi.fn() }),
}));

vi.mock('../../../commands/query', () => ({
  queryCommands: {
    resolveResultOrigin: (...args: unknown[]) => resolveResultOrigin(...args),
  },
}));

// The write command is the only way a write statement can leave the app.
vi.mock('@datazen/backend-client', async (importOriginal) => {
  const original = await importOriginal<typeof import('@datazen/backend-client')>();
  return {
    ...original,
    useBackendClient: () => ({
      ...original.useBackendClient(),
      commitQueryResultCell: (...args: unknown[]) => commitQueryResultCell(...args),
    }),
  };
});

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

interface ResultOptions {
  /** The narrow host-side mapping verdict. */
  writableMapping?: 'verified' | 'readOnly';
  /** Whether the executed result still carries a runtime binding. */
  runtimeBinding?: boolean;
}

function result(options: ResultOptions = {}): StatementResult {
  const { writableMapping, runtimeBinding = true } = options;
  return {
    sql: 'SELECT id, total FROM orders',
    columns: [
      { name: 'id', dataType: 'INTEGER', nullable: false },
      { name: 'total', dataType: 'INTEGER', nullable: true },
    ],
    rows: [
      [1, 10],
      [2, 20],
    ],
    executionTimeMs: 3,
    ...(runtimeBinding
      ? {
          runtimeBinding: {
            executionId: 'exec-1',
            resourceBindingId: 'res-1',
            handle: { dbSessionId: 'sess-1', runtimeEpoch: 0 },
          },
        }
      : { runtimeBinding: null }),
    ...(writableMapping ? { source: { writableMapping, statementIndex: 0 } } : {}),
  } as unknown as StatementResult;
}

function seedResult(next: StatementResult) {
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
          resultDetailRowIndex: 0,
        },
      ],
    ]),
  });
}

function renderDrawers() {
  return render(
    <ContentViewDrawers
      activePanel={PANEL}
      detailOpen
      aiChatOpen={false}
      detailPanelApplicable
      dbSessionId="sess-1"
      connectionName="App"
      currentDatabase="app"
      databaseType="postgresql"
      onCloseDetail={() => undefined}
      pendingDraftRequest={null}
      onDraftConsumed={() => undefined}
    />,
  );
}

/** The read-only reason the drawer shows, as a contract attribute. */
function reasonAttribute(container: HTMLElement): string | null {
  return (
    container
      .querySelector('[data-dt-grid-readonly-reason]')
      ?.getAttribute('data-dt-grid-readonly-reason') ?? null
  );
}

/** Every input in the drawer: the search box plus one per editable field. */
function inputs(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll('input, textarea'));
}

beforeEach(() => {
  resolveResultOrigin.mockReset();
  commitQueryResultCell.mockReset();
  usePanelResultEdit.setState({ verdicts: {}, resolving: {}, errors: {} });
  seedResult(result());
});

afterEach(() => {
  cleanup();
  usePanelStore.setState({ panels: [], activePanelId: PANEL.id, queryExec: new Map() });
});

describe('a result the driver refuses', () => {
  it('produces no write intent for any edit gesture', async () => {
    const updateResultCell = vi.fn(async () => undefined);
    usePanelStore.setState({ updateResultCell });
    resolveResultOrigin.mockResolvedValue({
      writable: false,
      identityColumns: [],
      editableColumns: [],
      table: null,
      reason: 'multiTable',
    });

    const { container } = renderDrawers();
    await waitFor(() => expect(resolveResultOrigin).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(reasonAttribute(container)).toBe('multiTable'));

    // The search box is the only input: no field is an editor.
    const before = inputs(container).length;
    expect(before).toBe(1);

    const fields = Array.from(container.querySelectorAll('pre'));
    expect(fields.length).toBeGreaterThan(0);

    for (const field of fields) {
      fireEvent.doubleClick(field);
      fireEvent.keyDown(field, { key: 'Enter' });
      fireEvent.keyDown(field, { key: 'F2' });
      fireEvent.keyDown(field, { key: 'a' });
      fireEvent.paste(field, { clipboardData: { getData: () => '99' } });
    }
    fireEvent.keyDown(container, { key: 'Enter' });
    fireEvent.keyDown(container, { key: 'F2' });
    fireEvent.paste(container, { clipboardData: { getData: () => '99' } });

    expect(updateResultCell).not.toHaveBeenCalled();
    expect(commitQueryResultCell).not.toHaveBeenCalled();
    expect(inputs(container).length).toBe(before);
  });

  it('shows the reason the back end gave', async () => {
    resolveResultOrigin.mockResolvedValue({
      writable: false,
      identityColumns: [],
      editableColumns: [],
      table: null,
      reason: 'sessionReadOnly',
    });

    const { container } = renderDrawers();

    await waitFor(() => expect(reasonAttribute(container)).toBe('sessionReadOnly'));
  });

  it('never touches the table panel pending-change mechanism', async () => {
    const tableStore = useTableDataStore.getState();
    const stageCellChange = vi.spyOn(tableStore, 'stageCellChange');
    const applyColumnToRows = vi.spyOn(tableStore, 'applyColumnToRows');
    resolveResultOrigin.mockResolvedValue({
      writable: false,
      identityColumns: [],
      editableColumns: [],
      table: null,
      reason: 'multiTable',
    });

    const { container } = renderDrawers();
    await waitFor(() => expect(reasonAttribute(container)).toBe('multiTable'));

    const fields = Array.from(container.querySelectorAll('pre'));
    for (const field of fields) {
      fireEvent.doubleClick(field);
      fireEvent.keyDown(field, { key: 'Enter' });
      fireEvent.paste(field, { clipboardData: { getData: () => '99' } });
    }

    expect(stageCellChange).not.toHaveBeenCalled();
    expect(applyColumnToRows).not.toHaveBeenCalled();
    stageCellChange.mockRestore();
    applyColumnToRows.mockRestore();
  });
});

describe('the verdict cache', () => {
  it('drops the verdict of the result that is no longer on screen', async () => {
    resolveResultOrigin.mockResolvedValue({
      writable: false,
      identityColumns: [],
      editableColumns: [],
      table: null,
      reason: 'distinct',
    });

    // Two results in one panel: the second one is on screen.
    const first = result();
    const second = { ...result(), sql: 'SELECT id, total FROM orders_2' };
    usePanelStore.setState({
      panels: [PANEL],
      activePanelId: PANEL.id,
      queryExec: new Map([
        [
          PANEL.id,
          {
            ...EMPTY_QUERY_EXEC,
            sql: first.sql,
            results: [first, second],
            activeResultIdx: 0,
            resultDetailRowIndex: 0,
          },
        ],
      ]),
    });

    const { rerender } = renderDrawers();
    await waitFor(() => expect(resolveResultOrigin).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(Object.keys(usePanelResultEdit.getState().verdicts)).toHaveLength(1));

    // The user switches to the second result tab: the first verdict is
    // destroyed together with the rows it was made for.
    usePanelStore.setState({
      queryExec: new Map([
        [
          PANEL.id,
          {
            ...EMPTY_QUERY_EXEC,
            sql: second.sql,
            results: [first, second],
            activeResultIdx: 1,
            resultDetailRowIndex: 0,
          },
        ],
      ]),
    });
    rerender(
      <ContentViewDrawers
        activePanel={PANEL}
        detailOpen
        aiChatOpen={false}
        detailPanelApplicable
        dbSessionId="sess-1"
        connectionName="App"
        currentDatabase="app"
        databaseType="postgresql"
        onCloseDetail={() => undefined}
        pendingDraftRequest={null}
        onDraftConsumed={() => undefined}
      />,
    );

    await waitFor(() => expect(resolveResultOrigin).toHaveBeenCalledTimes(2));
    await waitFor(() => {
      const keys = Object.keys(usePanelResultEdit.getState().verdicts);
      expect(keys).toHaveLength(1);
      expect(keys[0].endsWith('#1')).toBe(true);
    });
  });
});

describe('the release channel, exercised against the narrow write path', () => {
  it('stays read-only and says why, because no write path can carry the grant', async () => {
    // The origin verdict says the result may be written back. The narrow
    // host-side check did not verify the mapping, so the narrow write path
    // (`commit_query_result_cell`) would refuse — and refuse *silently*.
    // The release channel is therefore closed: the grid must not claim the
    // cell is editable.
    const updateResultCell = vi.fn(async () => undefined);
    usePanelStore.setState({ updateResultCell });
    seedResult(result({ writableMapping: 'readOnly', runtimeBinding: true }));
    resolveResultOrigin.mockResolvedValue({
      writable: true,
      identityColumns: [{ column: 'id', sourceColumn: 'id' }],
      editableColumns: [
        { column: 'id', sourceColumn: 'id' },
        { column: 'total', sourceColumn: 'total' },
      ],
      table: { catalog: null, schema: null, name: 'orders', aliased: false },
      reason: null,
    });

    const { container } = renderDrawers();
    await waitFor(() => expect(resolveResultOrigin).toHaveBeenCalledTimes(1));

    // Only the search box: the grid does not claim to be editable.
    await waitFor(() => expect(inputs(container).length).toBe(1));
    // …and the reason is offered: a read-only cell that says nothing is the
    // "unresponsive grey cell" the contract forbids.
    expect(reasonAttribute(container)).not.toBeNull();

    // The user edits anyway: nothing is written, and nothing is claimed.
    const field = inputs(container)[0];
    fireEvent.change(field, { target: { value: '11' } });
    fireEvent.keyDown(field, { key: 'Enter' });

    expect(updateResultCell).not.toHaveBeenCalled();
    expect(commitQueryResultCell).not.toHaveBeenCalled();
  });

  it('stays read-only when the origin verdict grants write-back but no runtime binding exists', async () => {
    // The origin verdict is the only thing that could grant write-back here;
    // without a runtime binding the narrow write path has nothing to bind to.
    const updateResultCell = vi.fn(async () => undefined);
    usePanelStore.setState({ updateResultCell });
    seedResult(result({ writableMapping: 'readOnly', runtimeBinding: false }));
    resolveResultOrigin.mockResolvedValue({
      writable: true,
      identityColumns: [{ column: 'id', sourceColumn: 'id' }],
      editableColumns: [{ column: 'id', sourceColumn: 'id' }],
      table: { catalog: null, schema: null, name: 'orders', aliased: false },
      reason: null,
    });

    const { container } = renderDrawers();
    await waitFor(() => expect(resolveResultOrigin).toHaveBeenCalledTimes(1));

    // Only the search box: the grid is not editable …
    expect(inputs(container).length).toBe(1);
    // … and it says why: a silent grey cell is forbidden.
    expect(reasonAttribute(container)).not.toBeNull();

    // The drawer offers no editable field, so there is nothing to type into.
    for (const field of Array.from(container.querySelectorAll('pre'))) {
      fireEvent.doubleClick(field);
      fireEvent.keyDown(field, { key: 'Enter' });
    }
    expect(updateResultCell).not.toHaveBeenCalled();
  });
});
