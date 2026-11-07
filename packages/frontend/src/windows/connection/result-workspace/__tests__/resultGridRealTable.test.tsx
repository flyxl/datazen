/**
 * The **real** result grid: the real `ResultTableView` with the real
 * `DataTable`, driven the way a user drives it — a double click on a real
 * cell.
 *
 * The mock-based grid test proves the wiring but not the behaviour: it cannot
 * show that a double click on a real cell leaves the grid without an editor,
 * nor that the grid says why.
 *
 * Four facts are pinned here:
 *
 * 1. a result the driver refuses does not open an editor, says why through
 *    the contract attribute, and produces **no write intent**;
 * 2. a result the narrow host-side check verified **does** open an editor —
 *    the control case, without which "no editor" would prove nothing;
 * 3. a result nobody has judged yet is read-only too;
 * 4. **the row-level guard really refuses**: a result the narrow check
 *    verified, whose clicked row has a NULL primary key, does not open an
 *    editor — the page-level fact of PRD §1.3 §6, reachable on the narrow
 *    channel because the locating key columns come from the catalog.
 *
 * D7: the reason is read from `data-dt-grid-readonly-reason`; no rendered
 * copy is pinned.
 */

import { cleanup, fireEvent, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { toId } from '@datazen/backend-client';
import type { StatementResult } from '../../../../types';
import { ResultTableView } from '../ResultTableView';
import { usePanelResultEdit } from '../../../../stores/panelResultEdit';

vi.mock('../../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

const settingsState = vi.hoisted(() => ({ queryResultLimit: 5000, safeMode: false }));

vi.mock('../../../../stores/settingsStore', () => ({
  useSettingsStore: (selector: (state: { settings: typeof settingsState }) => unknown) =>
    selector({ settings: settingsState }),
}));

// The host's own virtual-row hook: jsdom has no layout, so the real one would
// render zero rows and the double click would never reach a cell.
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

function result(): StatementResult {
  return {
    sql: 'select id, total from orders',
    columns: [
      { name: 'id', dataType: 'int4', nullable: false },
      { name: 'total', dataType: 'int4', nullable: true },
    ],
    rows: [
      [1, 10],
      [2, 20],
    ],
    executionTimeMs: 2,
  };
}

/** A result the narrow host-side check verified, with a live runtime binding. */
function verifiedResult(): StatementResult {
  const executionId = toId('execution-1')!;
  return {
    ...result(),
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

/** The editors the grid body rendered. */
function editors(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll('[data-testid="data-table-cell"] input, [data-testid="data-table-cell"] textarea'));
}

function readOnlyReason(container: HTMLElement): string | null {
  return (
    container
      .querySelector('[data-dt-grid-readonly-reason]')
      ?.getAttribute('data-dt-grid-readonly-reason') ?? null
  );
}

/** A result whose first row has a NULL primary key. */
function nullableKeyResult(): StatementResult {
  return {
    ...verifiedResult(),
    rows: [
      [null, 10],
      [2, 20],
    ],
  } as unknown as StatementResult;
}

/** The verdict the back end sends when it knows the catalog's key columns. */
function locatedResult(): void {
  usePanelResultEdit.setState({
    verdicts: {
      'session-1#0': {
        writable: false,
        identityColumns: [{ column: 'id', sourceColumn: 'id' }],
        editableColumns: [],
        table: { catalog: null, schema: null, name: 'orders', aliased: false },
        reason: 'unknownOrigin',
      },
    },
    resolving: {},
    errors: {},
    generations: {},
  });
}

function refuseResult(): void {
  usePanelResultEdit.setState({
    verdicts: {
      'session-1#0': {
        writable: false,
        identityColumns: [],
        editableColumns: [],
        table: null,
        reason: 'multiTable',
      },
    },
    resolving: {},
    errors: {},
    generations: {},
  });
}

beforeEach(() => {
  settingsState.safeMode = false;
  usePanelResultEdit.setState({ verdicts: {}, resolving: {}, errors: {}, generations: {} });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('the real result grid', () => {
  it('opens an editor for a result the narrow check verified (the control case)', () => {
    const onCellEdit = vi.fn().mockResolvedValue(undefined);
    const { container } = render(
      <ResultTableView
        result={verifiedResult()}
        writeBackKey="session-1#0"
        onCellEdit={onCellEdit}
      />,
    );

    const cell = container.querySelector('[data-testid="data-table-cell"]');
    expect(cell).not.toBeNull();
    expect(editors(container)).toHaveLength(0);

    fireEvent.doubleClick(cell!);

    // The editor is open: the control case proves the probe can see one.
    expect(editors(container).length).toBeGreaterThan(0);
  });

  it('refuses to open an editor when the driver refuses the result, and says why', () => {
    refuseResult();
    const onCellEdit = vi.fn().mockResolvedValue(undefined);
    const { container } = render(
      <ResultTableView
        result={result()}
        writeBackKey="session-1#0"
        onCellEdit={onCellEdit}
      />,
    );

    const cell = container.querySelector('[data-testid="data-table-cell"]');
    expect(cell).not.toBeNull();

    fireEvent.doubleClick(cell!);

    // No editor anywhere in the grid body.
    expect(editors(container)).toHaveLength(0);
    // The reason is present and non-empty.
    const reason = readOnlyReason(container);
    expect(reason).toBe('multiTable');
    expect(reason).not.toBe('');
    // No write intent at all.
    expect(onCellEdit).not.toHaveBeenCalled();
  });

  it('refuses to open an editor for a row whose primary key is NULL', () => {
    // The narrow host-side check verified the relation, so the result is
    // writable — but the clicked row's locating key is NULL, and a NULL cannot
    // address a row (PRD §1.3 §6). The locating key columns come from the
    // catalog, which is why the fact is computable on the narrow channel.
    locatedResult();
    const onCellEdit = vi.fn().mockResolvedValue(undefined);
    const { container } = render(
      <ResultTableView
        result={nullableKeyResult()}
        writeBackKey="session-1#0"
        onCellEdit={onCellEdit}
      />,
    );

    const cell = container.querySelector('[data-testid="data-table-cell"]');
    expect(cell).not.toBeNull();

    fireEvent.doubleClick(cell!);

    // No editor: the row cannot be addressed.
    expect(editors(container)).toHaveLength(0);
    // And the reason is not swallowed.
    expect(
      container.querySelector('[data-dt-grid-row-refusal]')?.getAttribute('data-dt-grid-row-refusal'),
    ).toBe('keyValueNull');
    expect(onCellEdit).not.toHaveBeenCalled();
  });

  it('is read-only, and says so, for a result nobody has judged yet', () => {
    const onCellEdit = vi.fn().mockResolvedValue(undefined);
    const { container } = render(
      <ResultTableView
        result={result()}
        writeBackKey="session-1#0"
        onCellEdit={onCellEdit}
      />,
    );

    const cell = container.querySelector('[data-testid="data-table-cell"]');
    fireEvent.doubleClick(cell!);

    expect(editors(container)).toHaveLength(0);
    expect(readOnlyReason(container)).toBe('unknownOrigin');
    expect(onCellEdit).not.toHaveBeenCalled();
  });
});
