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

// The drawer is reduced to what the test needs to observe: whether it is
// editable, and the edit callback it would call.
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

/** One result with a runtime binding, so the drawer may be editable at all. */
const RESULT = {
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
  runtimeBinding: {
    executionId: 'exec-1',
    resourceBindingId: 'res-1',
    handle: { dbSessionId: 'sess-1', runtimeEpoch: 0 },
  },
} as unknown as StatementResult;

function seedPanel(result: StatementResult = RESULT) {
  usePanelStore.setState({
    panels: [PANEL],
    activePanelId: PANEL.id,
    queryExec: new Map([
      [
        PANEL.id,
        {
          ...EMPTY_QUERY_EXEC,
          sql: result.sql,
          results: [result],
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

/** The read-only reason the drawer shows, as a data attribute. */
function reasonAttribute(container: HTMLElement): string | null {
  return container.querySelector('[data-dt-grid-readonly-reason]')?.getAttribute(
    'data-dt-grid-readonly-reason',
  ) ?? null;
}

beforeEach(() => {
  resolveResultOrigin.mockReset();
  usePanelResultEdit.setState({ verdicts: {}, resolving: {}, errors: {} });
  seedPanel();
});

afterEach(() => {
  cleanup();
  usePanelStore.setState({ panels: [], activePanelId: null, queryExec: new Map() });
});

describe('the result grid drawer when the driver refuses the result', () => {
  it('refuses the edit and never enters the write path', async () => {
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

    expect(screen.getByTestId('mock-detail-panel')).toHaveAttribute('data-editable', 'false');

    fireEvent.click(screen.getByTestId('mock-edit-field'));

    expect(updateResultCell).not.toHaveBeenCalled();
  });

  it('stays read-only while the verdict is still being fetched', async () => {
    const updateResultCell = vi.fn(async () => undefined);
    usePanelStore.setState({ updateResultCell });
    // A verdict that never arrives: the grid must not become writable.
    resolveResultOrigin.mockReturnValue(new Promise(() => undefined));

    const { container } = renderDrawers();

    await waitFor(() => expect(resolveResultOrigin).toHaveBeenCalledTimes(1));
    expect(reasonAttribute(container)).toBe('unknownOrigin');
    expect(screen.getByTestId('mock-detail-panel')).toHaveAttribute('data-editable', 'false');

    fireEvent.click(screen.getByTestId('mock-edit-field'));

    expect(updateResultCell).not.toHaveBeenCalled();
  });

  it('keeps the grid read-only when the driver says the result may be written', async () => {
    const updateResultCell = vi.fn(async () => undefined);
    usePanelStore.setState({ updateResultCell });
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

    // The origin verdict alone must not open the grid: the write path that
    // could carry such a write is not connected, so an opened drawer would
    // claim an edit that is silently refused.
    await waitFor(() => expect(reasonAttribute(container)).not.toBeNull());
    expect(screen.getByTestId('mock-detail-panel')).toHaveAttribute('data-editable', 'false');

    fireEvent.click(screen.getByTestId('mock-edit-field'));

    expect(updateResultCell).not.toHaveBeenCalled();
  });

  it('keeps a result the narrow mapping check verified editable', async () => {
    const updateResultCell = vi.fn(async () => undefined);
    usePanelStore.setState({ updateResultCell });
    // The narrow verdict says the mapping is verified; the driver cannot tell
    // where the rows came from. The narrow verdict is not withdrawn.
    seedPanel({
      ...RESULT,
      source: { writableMapping: 'verified' },
    } as unknown as StatementResult);
    resolveResultOrigin.mockResolvedValue({
      writable: false,
      identityColumns: [],
      editableColumns: [],
      table: null,
      reason: 'unknownOrigin',
    });

    const { container } = renderDrawers();

    await waitFor(() => expect(reasonAttribute(container)).toBeNull());
    fireEvent.click(screen.getByTestId('mock-edit-field'));

    await waitFor(() => expect(updateResultCell).toHaveBeenCalledTimes(1));
  });
});
