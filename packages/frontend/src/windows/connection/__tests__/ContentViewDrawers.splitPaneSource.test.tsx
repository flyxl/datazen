/**
 * Which result the row-detail drawer judges when the tab is split.
 *
 * The drawer draws the row the grid put it on, so it has to judge the **same**
 * result the grid shows. The drawer used to read the tab's *default* pane's
 * execution state while the grid showed the *focused* pane's: the two
 * surfaces judged two different result sets, so the drawer's read-only reason
 * came from the wrong verdict key and degraded to `unknownOrigin`.
 *
 * The observable is the result itself — the verdict request carries the
 * statement, and the drawer hands the row it shows to the detail panel.
 */
import { cleanup, render, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ContentViewDrawers } from '../ContentViewDrawers';
import { EMPTY_QUERY_EXEC, paneKey, usePanelStore } from '../../../stores/panelStore';
import { usePanelResultEdit } from '../../../stores/panelResultEdit';
import type { Panel } from '../../../stores/panelStore';
import type { StatementResult } from '../../../types';

const { resolveResultOrigin } = vi.hoisted(() => ({ resolveResultOrigin: vi.fn() }));

vi.mock('../../../hooks/useResizable', () => ({
  useResizable: () => ({ size: 320, handleRef: { current: null }, startResize: vi.fn() }),
}));

// The drawer is reduced to the row it shows: that row is the observable of
// which result set the drawer judged.
vi.mock('../../../components/DataTable/DetailPanel', () => ({
  DetailPanel: ({ row }: { row?: Record<string, unknown> | null }) => (
    <div data-testid="mock-detail-panel">{row ? JSON.stringify(row) : ''}</div>
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

/** The pane the tab is split into besides its default one. */
const SECOND_PANE = 'b';

function result(sql: string, id: number): StatementResult {
  return {
    sql,
    columns: [{ name: 'id', dataType: 'INTEGER', nullable: false }],
    rows: [[id]],
    executionTimeMs: 1,
    runtimeBinding: {
      executionId: `exec-${id}`,
      resourceBindingId: 'res-1',
      handle: { dbSessionId: 'sess-1', runtimeEpoch: 0 },
    },
  } as unknown as StatementResult;
}

function seedSplit(defaultPane: StatementResult, focusedPane: StatementResult) {
  usePanelStore.setState({
    panels: [PANEL],
    activePanelId: PANEL.id,
    focusedPaneId: SECOND_PANE,
    queryExec: new Map([
      [
        PANEL.id,
        {
          ...EMPTY_QUERY_EXEC,
          sql: defaultPane.sql,
          results: [defaultPane],
          activeResultIdx: 0,
          resultDetailRowIndex: 0,
        },
      ],
      [
        paneKey(PANEL.id, SECOND_PANE),
        {
          ...EMPTY_QUERY_EXEC,
          sql: focusedPane.sql,
          results: [focusedPane],
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

beforeEach(() => {
  resolveResultOrigin.mockReset();
  usePanelResultEdit.setState({ verdicts: {}, resolving: {}, errors: {} });
});

afterEach(() => {
  cleanup();
  usePanelStore.setState({
    panels: [],
    activePanelId: null,
    focusedPaneId: null,
    queryExec: new Map(),
  });
});

describe('the row-detail drawer of a split tab', () => {
  it('judges the focused pane’s result, not the default pane’s', async () => {
    const defaultPane = result('SELECT id FROM default_pane', 1);
    const focusedPane = result('SELECT id FROM focused_pane', 42);
    seedSplit(defaultPane, focusedPane);
    resolveResultOrigin.mockResolvedValue({
      writable: false,
      identityColumns: [],
      editableColumns: [],
      table: null,
      reason: 'multiTable',
    });

    const { container } = renderDrawers();

    // The verdict is asked for the statement the grid shows …
    await waitFor(() => expect(resolveResultOrigin).toHaveBeenCalledTimes(1));
    expect(resolveResultOrigin.mock.calls[0][0]).toMatchObject({ sql: focusedPane.sql });
    // … and the row the drawer shows comes out of that same result.
    const shown = container.querySelector('[data-testid="mock-detail-panel"]')?.textContent ?? '';
    expect(shown).toContain('42');
    expect(shown).not.toContain('"id":1');
  });
});
