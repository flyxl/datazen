/**
 * DB-03 §9.3 — the grid's binding of the clipboard, at the component level.
 *
 * Two things are asserted here and nowhere else:
 *
 * 1. **Degraded safety.** A `DataTable` without the `clipboard` prop must behave exactly as
 *    it does today: no interception, no `data-dt-cell-error`, nothing to explain. That is the
 *    contract every other panel depends on.
 * 2. **The gesture is only ours when it can be served** — a grid without a landing cell does
 *    not swallow the browser's paste, and an open editor keeps the native behaviour.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/react';
import { DataTable, type DataTableProps } from '../DataTable';
import type { GridClipboardHandlers } from '../gridClipboardBridge';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));
vi.mock('../../../hooks/useColumnResize', () => ({
  useColumnResize: () => ({ columnWidths: [160, 160], onResizeStart: vi.fn() }),
  adjustWidthsForSort: (widths: number[]) => widths,
}));
vi.mock('../../../hooks/useVirtualTable', () => ({
  useVirtualTable: ({ rows }: { rows: unknown[][] }) => ({
    virtualRows: rows.map((_, index) => ({ index, key: String(index), start: index * 40 })),
    totalHeight: rows.length * 40,
  }),
}));

const COLUMNS = [
  { id: 'id', name: 'id', type: 'integer' },
  { id: 'name', name: 'name', type: 'text' },
];
const ROWS: unknown[][] = [
  [1, 'Alice'],
  [2, 'Bob'],
];
const SELECTION = {
  mode: 'cell' as const,
  anchor: { rowIndex: 0, columnName: 'id' },
  focus: { rowIndex: 1, columnName: 'name' },
  extraRanges: [],
};

/** A `clipboardData` stand-in: jsdom has no `DataTransfer`. */
function clipboardData(initial = '') {
  const store: Record<string, string> = { 'text/plain': initial };
  return {
    getData: (type: string) => store[type] ?? '',
    setData: (type: string, value: string) => {
      store[type] = value;
    },
    read: (type: string) => store[type],
  };
}

/** Dispatch a native `copy` / `paste` with a working `clipboardData`. */
function dispatch(
  element: Element,
  type: 'copy' | 'paste',
  data: ReturnType<typeof clipboardData>,
): Event {
  const event = new Event(type, { bubbles: true, cancelable: true });
  Object.defineProperty(event, 'clipboardData', { value: data });
  fireEvent(element, event);
  return event;
}

function handlers(overrides: Partial<GridClipboardHandlers> = {}): GridClipboardHandlers {
  return {
    buildCopyText: vi.fn(() => 'ID\tNAME\r\n1\tAlice'),
    onCopied: vi.fn(),
    onPasteText: vi.fn(),
    onPasteFromClipboard: vi.fn(),
    canCopy: true,
    canPaste: true,
    ...overrides,
  };
}

function renderGrid(props: Partial<DataTableProps> = {}) {
  const utils = render(
    <DataTable
      columns={COLUMNS}
      rows={ROWS}
      selectedRows={new Set<number>()}
      gridSelection={SELECTION}
      onGridSelectionChange={vi.fn()}
      {...props}
    />,
  );
  const surface = utils.container.querySelector('[data-dt-surface]');
  if (!(surface instanceof HTMLElement)) throw new Error('grid surface not found');
  return { ...utils, surface };
}

beforeEach(() => {
  // `document.elementFromPoint` is used by the drag code; jsdom has no layout.
  Object.defineProperty(document, 'elementFromPoint', {
    configurable: true,
    writable: true,
    value: vi.fn(() => null),
  });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe('DataTable without a clipboard bridge (degraded safety)', () => {
  it('does not intercept a copy gesture', () => {
    const { surface } = renderGrid();
    const data = clipboardData('selection made by the browser');
    const event = dispatch(surface, 'copy', data);
    // The browser default stays in charge — this is what every un-migrated panel gets today.
    expect(event.defaultPrevented).toBe(false);
    expect(data.read('text/plain')).toBe('selection made by the browser');
  });

  it('does not intercept a paste gesture', () => {
    const { surface } = renderGrid();
    const event = dispatch(surface, 'paste', clipboardData('x'));
    expect(event.defaultPrevented).toBe(false);
  });

  it('renders no data-dt-cell-error attribute anywhere', () => {
    const { container } = renderGrid();
    expect(container.querySelectorAll('[data-dt-cell-error]')).toHaveLength(0);
  });
});

describe('DataTable with a clipboard bridge', () => {
  it('writes the serialized region into clipboardData and marks the gesture handled', () => {
    const buildCopyText = vi.fn(() => 'ID\tNAME\r\n1\tAlice');
    const onCopied = vi.fn();
    const { surface } = renderGrid({ clipboard: handlers({ buildCopyText, onCopied }) });
    const data = clipboardData();

    const event = dispatch(surface, 'copy', data);

    expect(buildCopyText).toHaveBeenCalledWith({
      mode: 'cell',
      range: { topRow: 0, bottomRow: 1, leftColumn: 'id', rightColumn: 'name' },
    });
    expect(data.read('text/plain')).toBe('ID\tNAME\r\n1\tAlice');
    expect(event.defaultPrevented).toBe(true);
    expect(onCopied).toHaveBeenCalledWith(expect.anything(), true);
  });

  it('pastes into the top-left of the region', () => {
    const onPasteText = vi.fn();
    const { surface } = renderGrid({ clipboard: handlers({ onPasteText }) });
    const event = dispatch(surface, 'paste', clipboardData('7\tZoe'));

    expect(onPasteText).toHaveBeenCalledWith('7\tZoe', { rowIndex: 0, columnName: 'id' });
    expect(event.defaultPrevented).toBe(true);
  });

  it('leaves the browser alone while a cell editor is open', () => {
    const onPasteText = vi.fn();
    const { surface } = renderGrid({
      clipboard: handlers({ onPasteText }),
      editingCell: { row: 0, col: 'name' },
    });
    const event = dispatch(surface, 'paste', clipboardData('typed into the editor'));

    expect(onPasteText).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(false);
  });

  it('does not intercept paste when there is no landing cell', () => {
    const onPasteText = vi.fn();
    const { surface } = renderGrid({
      clipboard: handlers({ onPasteText }),
      gridSelection: { mode: 'none', anchor: null, focus: null, extraRanges: [] },
    });
    const event = dispatch(surface, 'paste', clipboardData('x'));

    expect(onPasteText).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(false);
  });

  it('treats an empty rectangle as a failed copy, not a successful one', () => {
    const onCopied = vi.fn();
    const { surface } = renderGrid({
      clipboard: handlers({ buildCopyText: () => '', onCopied }),
    });
    const data = clipboardData();
    const event = dispatch(surface, 'copy', data);

    expect(onCopied).toHaveBeenCalledWith(expect.anything(), false);
    // Nothing was put on the clipboard: `setData` is never reached for an empty result.
    expect(data.read('text/plain')).toBe('');
    expect(event.defaultPrevented).toBe(true);
  });

  it('marks exactly the cells a refused paste reported', () => {
    const { container } = renderGrid({
      clipboard: handlers(),
      cellErrors: new Set(['1:name']),
    });

    const marked = Array.from(container.querySelectorAll('[data-dt-cell-error]')).map((element) =>
      element.getAttribute('data-dt-col'),
    );
    expect(marked).toEqual(['name']);
    const cell = container.querySelector('[data-dt-row="1"][data-dt-col="name"]');
    expect(cell?.getAttribute('data-dt-cell-error')).toBe('true');
  });
});
