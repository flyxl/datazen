/**
 * Tester's independent probe of the **row-level refusal** on the real grid.
 *
 * The claim under test: PRD §1.3 §6 (the edited row's locating key value must
 * not be NULL) is now **executable on the narrow channel** — the locating key
 * columns come from the catalog, and the grid refuses to open an editor for a
 * row that cannot be addressed.
 *
 * Two things are pinned that the branch's own case does not pin:
 *
 * 1. a **control on the same verdict**: the very same result and the very same
 *    (read-only) verdict, with a row whose key is *not* NULL, **does** open an
 *    editor — so the refusal is caused by the NULL key and not by the verdict
 *    being read-only;
 * 2. the **duplicate identity** case (§1.3 §7), which is the other page-level
 *    fact and has no server-side counterpart at all.
 *
 * D7: the reason is read from the contract attribute; no rendered copy is
 * pinned.
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

/** A result the narrow host-side check verified, with a live runtime binding. */
function verifiedResult(rows: unknown[][]): StatementResult {
  const executionId = toId('execution-1')!;
  return {
    sql: 'select id, total from orders',
    columns: [
      { name: 'id', dataType: 'int4', nullable: false },
      { name: 'total', dataType: 'int4', nullable: true },
    ],
    rows,
    executionTimeMs: 2,
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

/**
 * The verdict the back end sends for a narrow-verified result: the relation
 * is known, so the catalog could be read and the locating key columns are
 * reported — even though the driver cannot describe the origin.
 */
function locatedVerdict(): void {
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

function editors(container: HTMLElement): HTMLElement[] {
  return Array.from(
    container.querySelectorAll(
      '[data-testid="data-table-cell"] input, [data-testid="data-table-cell"] textarea',
    ),
  );
}

function rowRefusal(container: HTMLElement): string | null {
  return (
    container
      .querySelector('[data-dt-grid-row-refusal]')
      ?.getAttribute('data-dt-grid-row-refusal') ?? null
  );
}

beforeEach(() => {
  settingsState.safeMode = false;
  usePanelResultEdit.setState({ verdicts: {}, resolving: {}, errors: {}, generations: {} });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('a row that cannot be addressed', () => {
  it('opens the editor for a row whose key is not NULL — the control on the same verdict', () => {
    locatedVerdict();
    const onCellEdit = vi.fn().mockResolvedValue(undefined);
    const { container } = render(
      <ResultTableView
        result={verifiedResult([
          [1, 10],
          [2, 20],
        ])}
        writeBackKey="session-1#0"
        onCellEdit={onCellEdit}
      />,
    );

    const cell = container.querySelector('[data-testid="data-table-cell"]');
    fireEvent.doubleClick(cell!);

    // The same read-only verdict, the same result shape — only the key value
    // differs. So the refusal below is caused by the NULL key.
    expect(editors(container).length).toBeGreaterThan(0);
    expect(rowRefusal(container)).toBeNull();
  });

  it('refuses a row whose locating key value is NULL', () => {
    locatedVerdict();
    const onCellEdit = vi.fn().mockResolvedValue(undefined);
    const { container } = render(
      <ResultTableView
        result={verifiedResult([
          [null, 10],
          [2, 20],
        ])}
        writeBackKey="session-1#0"
        onCellEdit={onCellEdit}
      />,
    );

    const cell = container.querySelector('[data-testid="data-table-cell"]');
    fireEvent.doubleClick(cell!);

    expect(editors(container)).toHaveLength(0);
    expect(rowRefusal(container)).toBe('keyValueNull');
    expect(onCellEdit).not.toHaveBeenCalled();
  });

  it('refuses a row whose identity is not unique on the page', () => {
    locatedVerdict();
    const onCellEdit = vi.fn().mockResolvedValue(undefined);
    const { container } = render(
      <ResultTableView
        result={verifiedResult([
          [1, 10],
          [1, 20],
        ])}
        writeBackKey="session-1#0"
        onCellEdit={onCellEdit}
      />,
    );

    const cell = container.querySelector('[data-testid="data-table-cell"]');
    fireEvent.doubleClick(cell!);

    expect(editors(container)).toHaveLength(0);
    expect(rowRefusal(container)).toBe('duplicateIdentity');
    expect(onCellEdit).not.toHaveBeenCalled();
  });

  it('cannot compute the facts when no locating key column is known — and does not refuse', () => {
    // The catalog was unreadable: no key columns, so the page-level facts
    // cannot be computed. The grid must not turn a writable result grey.
    usePanelResultEdit.setState({
      verdicts: {
        'session-1#0': {
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
    const onCellEdit = vi.fn().mockResolvedValue(undefined);
    const { container } = render(
      <ResultTableView
        result={verifiedResult([
          [null, 10],
          [2, 20],
        ])}
        writeBackKey="session-1#0"
        onCellEdit={onCellEdit}
      />,
    );

    const cell = container.querySelector('[data-testid="data-table-cell"]');
    fireEvent.doubleClick(cell!);

    // Fail open: the facts are unknown, so the grid does not refuse.
    expect(editors(container).length).toBeGreaterThan(0);
    expect(rowRefusal(container)).toBeNull();
  });
});
