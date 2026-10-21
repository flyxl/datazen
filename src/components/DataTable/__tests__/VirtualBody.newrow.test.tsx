import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, cleanup } from '@testing-library/react';
import { VirtualBody, type DraftRowViewModel } from '../VirtualBody';
import enQuery from '../../../locales/en/query';

/**
 * The marker of a freshly inserted row.
 *
 * The background highlight of the grid is shared with the detail panel, so a
 * highlighted row could be either. The new row therefore gets a marker **with
 * a word on it** — and the marker must stay apart from the badge of a draft
 * row ("NEW", not saved yet).
 *
 * The real i18n hook is used here on purpose: the wording is part of the
 * contract, so the test reads the English text out of the domain pack instead
 * of asserting a key.
 */

vi.mock('../../../hooks/useVirtualTable', () => ({
  useVirtualTable: ({ rows }: { rows: unknown[][] }) => ({
    virtualRows: rows.map((_, index) => ({ index, key: String(index), start: index * 40 })),
    totalHeight: rows.length * 40,
  }),
}));

const COLS = [
  { id: 'id', name: 'id', type: 'integer' },
  { id: 'name', name: 'name', type: 'varchar' },
];

const ROWS = [
  [1, 'Alice'],
  [2, 'Bob'],
  [3, 'Carol'],
];

const DRAFTS: DraftRowViewModel[] = [{ draftId: 'draft-a', cells: { name: 'Dave' } }];

afterEach(cleanup);

function renderBody(props: Partial<React.ComponentProps<typeof VirtualBody>> = {}) {
  const scrollEl = document.createElement('div');
  return render(
    <VirtualBody
      columns={COLS}
      rows={ROWS}
      rowHeight={40}
      editingCell={null}
      selectedRows={new Set()}
      scrollElement={scrollEl}
      onCellDoubleClick={vi.fn()}
      onCellEdit={vi.fn()}
      onCellEditCancel={vi.fn()}
      onRowSelect={vi.fn()}
      {...props}
    />,
  );
}

/** The marker of a row, if that row carries one. */
function markerOf(container: HTMLElement, rowIndex: number): Element | null {
  return container
    .querySelector(`[data-dt-row-view="${rowIndex}"]`)
    ?.querySelector('[data-testid="new-row-badge"]') ?? null;
}

describe('VirtualBody (the marker of a new row)', () => {
  it('marks the flagged row with the wording of a new row', () => {
    const { container } = renderBody({ newRowIndex: 1 });

    const marker = markerOf(container, 1);
    expect(marker).not.toBeNull();
    // The wording is the point: the background highlight alone cannot tell a
    // new row from the row the detail panel highlights.
    expect(marker).toHaveTextContent(enQuery['tableData.highlightNewRow']);
    expect(marker).toHaveTextContent('New row');
    // Only the flagged row carries it.
    expect(markerOf(container, 0)).toBeNull();
    expect(markerOf(container, 2)).toBeNull();
  });

  it('renders no marker when no row is flagged', () => {
    const { container } = renderBody();

    // The default is "off": the DOM of a grid that does not use the feature
    // is byte for byte the DOM of today.
    expect(container.querySelector('[data-testid="new-row-badge"]')).toBeNull();
  });

  it('keeps the draft badge apart from the new-row marker', () => {
    const { container } = renderBody({ newRowIndex: 0, draftRows: DRAFTS });

    // The new row: the committed one, with its own wording.
    const marker = markerOf(container, 0);
    expect(marker).toHaveTextContent('New row');
    expect(container.querySelector('[data-dt-row-view="0"] [data-testid="draft-row-badge"]')).toBeNull();

    // The draft row: "NEW", not saved yet — a different marker and a
    // different wording.
    const draftBadge = container.querySelector('[data-testid="draft-row-badge"]');
    expect(draftBadge).not.toBeNull();
    expect(draftBadge).toHaveTextContent(enQuery['tableData.draftRowBadge']);
    expect(draftBadge).not.toHaveTextContent('New row');
    // …and it carries no new-row marker.
    const draftRow = container.querySelector('[data-testid="draft-row-0"]')!;
    expect(draftRow.querySelector('[data-testid="new-row-badge"]')).toBeNull();
  });
});
