/**
 * Review audit, round 2 — the marker of a freshly inserted row (04 §8
 * `tableData.highlightNewRow`, implemented in `ce57640fc`).
 *
 * The point of the feature is that the grid has **two** channels:
 *
 *   - `highlightedRow` — the background highlight, **shared** with the detail
 *     panel (a row the user opened in the detail view is highlighted the same
 *     way);
 *   - `newRowIndex` — the marker of the row the last insert produced, with a
 *     **word** on it.
 *
 * If the two were not independent, the detail panel would masquerade as a
 * fresh insert. The cases below therefore always assert the *other* channel
 * too, so that a case cannot pass because the highlight was simply absent.
 */
import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, cleanup } from '@testing-library/react';
import { VirtualBody, type DraftRowViewModel } from '../VirtualBody';
import enQuery from '../../../locales/en/query';

/**
 * The real i18n hook is used on purpose: the wording is part of the contract,
 * so the test reads the English text out of the domain pack instead of
 * asserting a key.
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

function rowOf(container: HTMLElement, index: number): HTMLElement {
  const row = container.querySelector(`[data-dt-row-view="${index}"]`);
  if (!row) throw new Error(`row ${index} is not rendered`);
  return row as HTMLElement;
}

function badgeOf(container: HTMLElement, index: number): Element | null {
  return rowOf(container, index).querySelector('[data-testid="new-row-badge"]');
}

describe('A · the marker of a new row is its own channel', () => {
  it('renders no marker and no new attribute when no row is flagged', () => {
    const { container } = renderBody();

    expect(container.querySelector('[data-testid="new-row-badge"]')).toBeNull();
    // The row container carries exactly the attributes it carried before the
    // feature existed — no marker hook, no extra attribute.
    expect(
      [...rowOf(container, 0).attributes].map((attribute) => attribute.name).sort(),
    ).toEqual(['class', 'data-dt-row-view', 'style', 'tabindex']);
    expect(rowOf(container, 0).getAttribute('class')).toBe(
      'absolute left-0 flex w-full cursor-pointer outline-none border-b border-edge/30 bg-surface hover:bg-surface-raised/50',
    );
  });

  it('marks the flagged row even when the background highlight is elsewhere', () => {
    // The two channels are independent: the background is on row 1 (the detail
    // panel), the marker is on row 0 (the inserted row).
    const { container } = renderBody({ newRowIndex: 0, highlightedRow: 1 });

    // The marker is on the flagged row …
    expect(badgeOf(container, 0)).not.toBeNull();
    expect(badgeOf(container, 1)).toBeNull();
    expect(badgeOf(container, 2)).toBeNull();
    // … and the background is still on the other row.
    expect(rowOf(container, 1).getAttribute('class')).toContain('bg-accent/8');
    expect(rowOf(container, 0).getAttribute('class')).not.toContain('bg-accent/8');
  });

  it('does not mark a row that is only highlighted by the detail panel', () => {
    // The background channel alone must never produce a marker: otherwise the
    // detail panel would impersonate a fresh insert.
    const { container } = renderBody({ highlightedRow: 0 });

    // The highlight really is there (so the case is not vacuous) …
    expect(rowOf(container, 0).getAttribute('class')).toContain('bg-accent/8');
    expect(rowOf(container, 1).getAttribute('class')).not.toContain('bg-accent/8');
    // … and it carries no marker.
    expect(badgeOf(container, 0)).toBeNull();
    expect(container.querySelector('[data-testid="new-row-badge"]')).toBeNull();
  });

  it('keeps the marker of a new row apart from the badge of a draft row', () => {
    const { container } = renderBody({ newRowIndex: 0, draftRows: DRAFTS });

    // The new row: its own test id, its own wording.
    const marker = badgeOf(container, 0);
    expect(marker).not.toBeNull();
    expect(marker?.getAttribute('data-testid')).toBe('new-row-badge');
    expect(marker).toHaveTextContent(enQuery['tableData.highlightNewRow']);
    expect(marker).toHaveTextContent('New row');
    // The draft badge is on the draft row, with a different wording.
    expect(rowOf(container, 0).querySelector('[data-testid="draft-row-badge"]')).toBeNull();

    const draftRow = container.querySelector('[data-testid="draft-row-0"]')!;
    const draftBadge = draftRow.querySelector('[data-testid="draft-row-badge"]');
    expect(draftBadge).not.toBeNull();
    expect(draftBadge).toHaveTextContent(enQuery['tableData.draftRowBadge']);
    expect(draftBadge).toHaveTextContent('NEW');
    expect(draftBadge).not.toHaveTextContent('New row');
    // The draft row carries no new-row marker.
    expect(draftRow.querySelector('[data-testid="new-row-badge"]')).toBeNull();
  });

  it('words the marker with the real English text of the domain pack', () => {
    // The wording comes from the i18n pack, not from a hard-coded string: the
    // key is the one the contract asks for and the pack is the only source.
    expect(enQuery['tableData.highlightNewRow']).toBe('New row');
    expect(enQuery['tableData.draftRowBadge']).toBe('NEW');
    // The two wordings are not the same string — that is the whole point of
    // the marker: "NEW" is a row that is not saved yet, "New row" is a row the
    // last commit produced.
    expect(enQuery['tableData.highlightNewRow']).not.toBe(enQuery['tableData.draftRowBadge']);

    const { container } = renderBody({ newRowIndex: 2 });
    expect(badgeOf(container, 2)).toHaveTextContent(enQuery['tableData.highlightNewRow']);
  });
});
