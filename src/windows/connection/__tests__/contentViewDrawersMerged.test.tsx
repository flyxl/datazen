/**
 * Tester's independent probe of the **hand-resolved merge** in
 * `src/windows/connection/ContentViewDrawers.tsx`.
 *
 * Two tracks edited that file's `<DetailPanel>` block: the result-grid track
 * (read-only reason banner + a verdict-driven `editable`) and the editors
 * track (the drawer's type label must read the driver's own declaration).
 * The conflict was resolved by hand, so this file checks the *result*:
 *
 * 1. the drawer's **read-only reason banner** is there (result-grid);
 * 2. the drawer's `editable` is **verdict-driven** (result-grid) — and not a
 *    hard-coded `writableMapping === 'verified'`;
 * 3. the drawer's **type label** reads the **same** `cellTypeHints` as the
 *    grid's header and cells (editors) — a MySQL `tinyint(1)` column is
 *    `text-dt-bool` in all three places;
 * 4. both tracks' changes are alive.
 *
 * D7: the reason is read from the contract attribute; no rendered copy is
 * pinned.
 */

import { cleanup, render, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ContentViewDrawers } from '../ContentViewDrawers';
import { EMPTY_QUERY_EXEC, usePanelStore } from '../../../stores/panelStore';
import { usePanelResultEdit } from '../../../stores/panelResultEdit';
import { DB_REGISTRY } from '../../../lib/databaseTypes';
import { DataTable } from '../../../components/DataTable/DataTable';
import { ResultTableView } from '../result-workspace/ResultTableView';
import { buildColumnEditorInfo } from '../../../lib/cellEditorKind';
import type { Panel } from '../../../stores/panelStore';
import type { StatementResult } from '../../../types';

const { resolveResultOrigin } = vi.hoisted(() => ({ resolveResultOrigin: vi.fn() }));

vi.mock('../../../hooks/useResizable', () => ({
  useResizable: () => ({ size: 320, handleRef: { current: null }, resize: vi.fn() }),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../commands/query', () => ({
  queryCommands: {
    resolveResultOrigin: (...args: unknown[]) => resolveResultOrigin(...args),
  },
}));

vi.mock('../../../hooks/useVirtualTable', () => ({
  useVirtualTable: ({ rows }: { rows: unknown[][] }) => ({
    virtualRows: rows.map((_, index) => ({ index, key: String(index), start: index * 40 })),
    totalHeight: rows.length * 40,
  }),
}));

vi.mock('../../../hooks/useColumnResize', () => ({
  useColumnResize: () => ({
    columnWidths: [160, 200],
    onResizeStart: vi.fn(),
  }),
  adjustWidthsForSort: (widths: number[]) => widths,
}));

vi.mock('../../../lib/nativeContextMenu', () => ({
  showNativeContextMenu: vi.fn().mockResolvedValue(undefined),
}));

const PANEL = {
  id: 'panel-query-1',
  type: 'query',
  connectionId: 'cfg-1',
  dbSessionId: 'sess-1',
  connectionName: 'App',
  databaseType: 'mysql',
  title: 'Query',
  database: 'app',
  schema: null,
} as unknown as Panel;

/** A MySQL result whose column is `tinyint(1)` — a boolean, only the driver knows. */
function mysqlResult(writableMapping?: 'verified'): StatementResult {
  return {
    sql: 'SELECT flag FROM orders',
    columns: [{ name: 'flag', dataType: 'tinyint(1)', nullable: true }],
    rows: [[1]],
    executionTimeMs: 3,
    runtimeBinding: {
      executionId: 'exec-1',
      resourceBindingId: 'res-1',
      handle: { dbSessionId: 'sess-1', runtimeEpoch: 0 },
    },
    ...(writableMapping
      ? { source: { writableMapping, statementIndex: 0 } }
      : {}),
  } as unknown as StatementResult;
}

function seed(result: StatementResult) {
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
      databaseType="mysql"
      onCloseDetail={() => undefined}
      pendingDraftRequest={null}
      onDraftConsumed={() => undefined}
    />,
  );
}

/** The drawer's inputs: the search box plus one per editable field. */
function inputs(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll('input, textarea'));
}

function readOnlyReason(container: HTMLElement): string | null {
  return (
    container
      .querySelector('[data-dt-grid-readonly-reason]')
      ?.getAttribute('data-dt-grid-readonly-reason') ?? null
  );
}

/** The class of the drawer's type label for the only column. */
function drawerTypeClass(container: HTMLElement): string {
  const label = Array.from(container.querySelectorAll('span')).find(
    (span) => span.textContent === 'tinyint(1)',
  );
  return label?.className ?? '';
}

beforeEach(() => {
  resolveResultOrigin.mockReset();
  resolveResultOrigin.mockResolvedValue({
    writable: false,
    identityColumns: [],
    editableColumns: [],
    table: null,
    reason: 'multiTable',
  });
  usePanelResultEdit.setState({ verdicts: {}, resolving: {}, errors: {}, generations: {} });
  seed(mysqlResult('verified'));
});

afterEach(() => {
  cleanup();
  usePanelStore.setState({ panels: [], activePanelId: null, queryExec: new Map() });
});

describe('the hand-resolved merge in ContentViewDrawers', () => {
  it('keeps the read-only reason banner of the result-grid track', async () => {
    // A result no verdict vouches for: the banner is the only way the user
    // learns why the drawer is read-only.
    seed(mysqlResult());
    const { container } = renderDrawers();

    await waitFor(() => expect(resolveResultOrigin).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(readOnlyReason(container)).toBe('multiTable'));
    expect(readOnlyReason(container)).not.toBe('');
  });

  it("keeps the editors track's type hints: a MySQL tinyint(1) is a boolean", async () => {
    const { container } = renderDrawers();

    await waitFor(() => expect(resolveResultOrigin).toHaveBeenCalledTimes(1));
    // The drawer reads the driver's own declaration, not the raw type name.
    expect(drawerTypeClass(container)).toContain('text-dt-bool');
  });

  it('makes the drawer, the grid header and the grid cell agree on that type', async () => {
    // The grid next to the drawer reads the same declaration, so the three
    // sites cannot disagree.
    const hints = DB_REGISTRY.mysql.cellTypes;
    expect(hints).toBeDefined();

    const columns = [{ name: 'flag', dataType: 'tinyint(1)', nullable: true }];
    const columnDefs = [{ id: 'flag', name: 'flag', type: 'tinyint(1)' }];
    const columnEditorInfo = buildColumnEditorInfo(columns, hints);

    const { container } = render(
      <div>
        <DataTable
          columns={columnDefs}
          rows={[[1]]}
          cellTypeHints={hints}
          cellEditing={{
            panelId: 'panel-merge',
            columnEditorInfo,
            readOnlyReason: null,
          }}
          onCellEditCancel={() => undefined}
        />
        <ContentViewDrawers
          activePanel={PANEL}
          detailOpen
          aiChatOpen={false}
          detailPanelApplicable
          dbSessionId="sess-1"
          connectionName="App"
          currentDatabase="app"
          databaseType="mysql"
          onCloseDetail={() => undefined}
          pendingDraftRequest={null}
          onDraftConsumed={() => undefined}
        />
      </div>,
    );

    const headerLabel = container.querySelector('[title="tinyint(1)"]');
    const cell = container.querySelector('[data-dt-row="0"][data-dt-col="flag"]');
    const cellValue = cell?.querySelector('span');

    expect(headerLabel?.className).toContain('text-dt-bool');
    expect(cellValue?.className).toContain('text-dt-bool');
    expect(drawerTypeClass(container)).toContain('text-dt-bool');
  });

  it('makes the result grid, its header and the drawer agree on the same type', async () => {
    // The **result grid** is the result-grid track's surface, and it had never
    // been given the driver's type declaration: the grid coloured a MySQL
    // `tinyint(1)` column by its type name while the drawer next to it used
    // the driver's declaration — two colours for one column on one screen.
    //
    // The grid now reads the same declaration as the drawer and the table
    // panel (`DB_REGISTRY[databaseType]?.cellTypes`), so all three agree.
    const hints = DB_REGISTRY.mysql.cellTypes;
    const { container } = render(
      <div>
        <ResultTableView result={mysqlResult()} cellTypeHints={hints} />
        <ContentViewDrawers
          activePanel={PANEL}
          detailOpen
          aiChatOpen={false}
          detailPanelApplicable
          dbSessionId="sess-1"
          connectionName="App"
          currentDatabase="app"
          databaseType="mysql"
          onCloseDetail={() => undefined}
          pendingDraftRequest={null}
          onDraftConsumed={() => undefined}
        />
      </div>,
    );

    const headerLabel = container.querySelector('[title="tinyint(1)"]');
    const cell = container.querySelector('[data-dt-row="0"][data-dt-col="flag"]');
    const cellValue = cell?.querySelector('span');

    expect(headerLabel?.className).toContain('text-dt-bool');
    expect(cellValue?.className).toContain('text-dt-bool');
    expect(drawerTypeClass(container)).toContain('text-dt-bool');
  });

  it("keeps the drawer's editable state verdict-driven", async () => {
    // The narrow host-side check verified the result: the narrow verdict is
    // not withdrawn by the origin verdict, so the drawer stays editable.
    const { container } = renderDrawers();

    await waitFor(() => expect(resolveResultOrigin).toHaveBeenCalledTimes(1));
    // The search box plus the field's editor.
    await waitFor(() => expect(inputs(container).length).toBeGreaterThan(1));

    // A result no verdict vouches for is read-only: only the search box.
    cleanup();
    seed(mysqlResult());
    const second = renderDrawers();
    await waitFor(() => expect(resolveResultOrigin).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(inputs(second.container).length).toBe(1));
  });
});
