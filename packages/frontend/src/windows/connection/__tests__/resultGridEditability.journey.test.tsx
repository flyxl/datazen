/**
 * The result grid's write-back journey.
 *
 * This is a **journey**, not a snapshot: it walks the whole state machine of
 * one result — the verdict is fetched, the result is refused, the user tries to
 * edit anyway, the statement is executed again — and asserts the transition at
 * every step. The states it walks are the ones the interface implements:
 *
 * ```text
 *   Resolving ──(verdict)──▶ ReadOnly(reason) ──(new result)──▶ Resolving
 *        │
 *        └──────────────(verdict)──▶ Writable ──(edit)──▶ write path
 * ```
 *
 * The assertions use the contract attributes (`data-dt-grid-readonly-reason`)
 * and the i18n key, never the rendered sentence: a translation change must not
 * turn this journey red.
 */

import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ContentViewDrawers } from '../ContentViewDrawers';
import { EMPTY_QUERY_EXEC, usePanelStore } from '../../../stores/panelStore';
import { usePanelResultEdit } from '../../../stores/panelResultEdit';
import type { Panel } from '../../../stores/panelStore';
import type { StatementResult } from '../../../types';

const { resolveResultOrigin } = vi.hoisted(() => ({ resolveResultOrigin: vi.fn() }));

vi.mock('../../../hooks/useResizable', () => ({
  useResizable: () => ({ size: 320, handleRef: { current: null }, startResize: vi.fn() }),
}));

vi.mock('../../../components/DataTable/DetailPanel', () => ({
  DetailPanel: ({
    editable,
    onFieldEdit,
  }: {
    editable?: boolean;
    onFieldEdit?: (row: number, column: string, value: unknown) => void;
  }) => (
    <div data-testid="mock-detail-panel" data-editable={String(editable ?? false)}>
      <button data-testid="mock-edit-field" onClick={() => onFieldEdit?.(0, 'total', 11)} />
    </div>
  ),
}));

vi.mock('../../../components/ai/AiChatPanel', () => ({
  AiChatPanel: () => null,
}));

vi.mock('../../../commands/query', () => ({
  queryCommands: {
    resolveResultOrigin: (...args: unknown[]) => resolveResultOrigin(...args),
  },
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

function result(sql: string): StatementResult {
  return {
    sql,
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

function reasonAttribute(container: HTMLElement): string | null {
  return (
    container
      .querySelector('[data-dt-grid-readonly-reason]')
      ?.getAttribute('data-dt-grid-readonly-reason') ?? null
  );
}

beforeEach(() => {
  resolveResultOrigin.mockReset();
  usePanelResultEdit.setState({ verdicts: {}, resolving: {}, errors: {} });
  seedResult(result('SELECT id, total FROM orders'));
});

afterEach(() => {
  cleanup();
  usePanelStore.setState({ panels: [], activePanelId: null, queryExec: new Map() });
});

describe('the write-back journey of one query result', () => {
  it('goes from resolving to refused, survives an edit attempt, and starts over on a new result', async () => {
    const updateResultCell = vi.fn(async () => undefined);
    usePanelStore.setState({ updateResultCell });

    // The verdict arrives late: the first fetch is still in flight when the
    // user clicks.
    let releaseVerdict: (value: unknown) => void = () => undefined;
    resolveResultOrigin.mockReturnValue(
      new Promise((resolve) => {
        releaseVerdict = resolve;
      }),
    );

    const { container } = renderDrawers();

    // ── Step 1: the result is on screen and nobody has judged it yet.
    await waitFor(() => expect(resolveResultOrigin).toHaveBeenCalledTimes(1));
    expect(reasonAttribute(container)).toBe('unknownOrigin');
    expect(screen.getByTestId('mock-detail-panel')).toHaveAttribute('data-editable', 'false');

    // ── Step 2: the user edits anyway. Nothing is written: the write path is
    // not entered at all.
    fireEvent.click(screen.getByTestId('mock-edit-field'));
    expect(updateResultCell).not.toHaveBeenCalled();

    // ── Step 3: the verdict arrives and refuses the result.
    releaseVerdict({
      writable: false,
      identityColumns: [],
      editableColumns: [],
      table: null,
      reason: 'aggregate',
    });
    await waitFor(() => expect(reasonAttribute(container)).toBe('aggregate'));
    expect(screen.getByTestId('mock-detail-panel')).toHaveAttribute('data-editable', 'false');

    // ── Step 4: the user edits again. Still nothing is written.
    fireEvent.click(screen.getByTestId('mock-edit-field'));
    expect(updateResultCell).not.toHaveBeenCalled();

    // ── Step 5: the statement is executed again. The old verdict belongs to
    // the old rows, so it is dropped and a new one is fetched.
    resolveResultOrigin.mockResolvedValue({
      writable: false,
      identityColumns: [],
      editableColumns: [],
      table: null,
      reason: 'view',
    });
    seedResult(result('SELECT id, total FROM orders_view'));
    await waitFor(() => expect(resolveResultOrigin).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(reasonAttribute(container)).toBe('view'));

    // ── Step 6: the reason travelled with the new result, and the grid is
    // still read-only.
    fireEvent.click(screen.getByTestId('mock-edit-field'));
    expect(updateResultCell).not.toHaveBeenCalled();
  });

  it('opens the grid through the narrow verdict and then enters the write path', async () => {
    const updateResultCell = vi.fn(async () => undefined);
    usePanelStore.setState({ updateResultCell });
    // The narrow host-side mapping verdict verified the relation — the one
    // channel that can open the grid today.
    seedResult({
      ...result('SELECT id, total FROM orders'),
      source: { writableMapping: 'verified', statementIndex: 0 },
    } as unknown as StatementResult);
    resolveResultOrigin.mockResolvedValue({
      writable: false,
      identityColumns: [],
      editableColumns: [],
      table: null,
      reason: 'unknownOrigin',
    });

    const { container } = renderDrawers();

    // ── Step 1: the verdict is on its way; the grid is still read-only.
    await waitFor(() => expect(resolveResultOrigin).toHaveBeenCalledTimes(1));
    await waitFor(() =>
      expect(screen.getByTestId('mock-detail-panel')).toHaveAttribute('data-editable', 'true'),
    );

    // ── Step 2: the origin verdict arrives and cannot take the grant away —
    // the narrow verdict is not withdrawn by it.
    await waitFor(() => expect(reasonAttribute(container)).toBeNull());
    expect(screen.getByTestId('mock-detail-panel')).toHaveAttribute('data-editable', 'true');

    // ── Step 3: the edit reaches the write path.
    fireEvent.click(screen.getByTestId('mock-edit-field'));
    await waitFor(() => expect(updateResultCell).toHaveBeenCalledTimes(1));
  });

  it('keeps the grid shut when only the origin verdict would grant the write', async () => {
    const updateResultCell = vi.fn(async () => undefined);
    usePanelStore.setState({ updateResultCell });
    // The narrow verdict did not verify the mapping; only the origin verdict
    // says the result may be written. The release channel is closed, so the
    // grid stays read-only — an open grid would claim an edit that the write
    // path silently refuses.
    seedResult({
      ...result('SELECT id, total FROM orders'),
      source: { writableMapping: 'readOnly', statementIndex: 0 },
    } as unknown as StatementResult);
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
    await waitFor(() => expect(reasonAttribute(container)).not.toBeNull());
    expect(screen.getByTestId('mock-detail-panel')).toHaveAttribute('data-editable', 'false');

    fireEvent.click(screen.getByTestId('mock-edit-field'));
    expect(updateResultCell).not.toHaveBeenCalled();
  });

  it('drops the verdict when the panel is closed', async () => {
    resolveResultOrigin.mockResolvedValue({
      writable: false,
      identityColumns: [],
      editableColumns: [],
      table: null,
      reason: 'distinct',
    });

    const { unmount } = renderDrawers();

    await waitFor(() => expect(resolveResultOrigin).toHaveBeenCalledTimes(1));
    expect(Object.keys(usePanelResultEdit.getState().verdicts)).toHaveLength(1);

    // Closing the panel destroys the verdict together with the rows it was
    // made for.
    unmount();
    expect(Object.keys(usePanelResultEdit.getState().verdicts)).toHaveLength(0);
  });
});
