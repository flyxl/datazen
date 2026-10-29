import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/react';
import { VirtualBody, type DraftRowViewModel } from '../VirtualBody';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useVirtualTable', () => ({
  useVirtualTable: ({ rows }: { rows: unknown[][] }) => ({
    virtualRows: rows.map((_, index) => ({
      index,
      key: String(index),
      start: index * 40,
    })),
    totalHeight: rows.length * 40,
  }),
}));

const COLS = [
  { id: 'id', name: 'id', type: 'integer' },
  { id: 'name', name: 'name', type: 'varchar' },
];

const DRAFTS: DraftRowViewModel[] = [
  { draftId: 'draft-a', cells: { name: 'Alice' } },
  { draftId: 'draft-b', cells: {} },
];

afterEach(cleanup);

function renderDrafts(props: Partial<React.ComponentProps<typeof VirtualBody>> = {}) {
  const scrollEl = document.createElement('div');
  return render(
    <VirtualBody
      columns={COLS}
      rows={[
        [1, 'Bob'],
      ]}
      rowHeight={40}
      editingCell={null}
      selectedRows={new Set()}
      scrollElement={scrollEl}
      onCellDoubleClick={vi.fn()}
      onCellEdit={vi.fn()}
      onCellEditCancel={vi.fn()}
      onRowSelect={vi.fn()}
      draftRows={DRAFTS}
      {...props}
    />,
  );
}

describe('VirtualBody (new-row drafts)', () => {
  it('renders the drafts after the real rows with their own test ids', () => {
    const { container } = renderDrafts();

    const first = container.querySelector('[data-testid="draft-row-0"]')!;
    const second = container.querySelector('[data-testid="draft-row-1"]')!;
    expect(first).toBeInTheDocument();
    expect(second).toBeInTheDocument();
    // The draft rows are marked as such, so a test (or an E2E script) never has
    // to guess from the geometry.
    expect(first).toHaveAttribute('data-row-kind', 'draft');
    expect(first).toHaveAttribute('data-draft-id', 'draft-a');
    expect(second).toHaveAttribute('data-draft-id', 'draft-b');
    // The badge marks the row as "not yet saved".
    expect(container.querySelector('[data-testid="draft-row-badge"]')!).toHaveTextContent(
      'tableData.draftRowBadge',
    );
  });

  it('shows the DEFAULT placeholder for a column that is not written', () => {
    const { container } = renderDrafts();

    const written = container.querySelector('[data-draft-id="draft-a"] [data-draft-col="name"]')!;
    expect(written).toHaveAttribute('data-draft-state', 'value');
    expect(written).toHaveTextContent('Alice');

    // The unwritten column of the first draft and every column of the second
    // draft show the placeholder: the database keeps its own default there.
    const unwritten = container.querySelector('[data-draft-id="draft-a"] [data-draft-col="id"]')!;
    expect(unwritten).toHaveAttribute('data-draft-state', 'default');
    expect(unwritten.querySelector('[data-testid="draft-cell-default"]')).toHaveTextContent(
      'tableData.insertDefault',
    );
    const empty = container.querySelector('[data-draft-id="draft-b"] [data-draft-col="name"]')!;
    expect(empty).toHaveAttribute('data-draft-state', 'default');
  });

  it('marks an explicit NULL as a written value', () => {
    const { container } = renderDrafts({
      draftRows: [{ draftId: 'draft-a', cells: { name: null } }],
    });

    const cell = container.querySelector('[data-draft-col="name"]')!;
    expect(cell).toHaveAttribute('data-draft-state', 'null');
  });

  it('starts the cell editor on a double click and commits the typed value', () => {
    const onDraftCellEditStart = vi.fn();
    const onDraftCellEdit = vi.fn();
    const { container } = renderDrafts({ onDraftCellEditStart, onDraftCellEdit });

    const cell = container.querySelector('[data-draft-id="draft-a"] [data-draft-col="name"]')!;
    fireEvent.doubleClick(cell);
    expect(onDraftCellEditStart).toHaveBeenCalledWith('draft-a', 'name');
  });

  it('writes the value only when the editor commits, and clears the column on an empty commit', () => {
    const onDraftCellEdit = vi.fn();
    const { container } = renderDrafts({
      editingDraft: { draftId: 'draft-a', column: 'name' },
      onDraftCellEdit,
    });

    const input = container.querySelector('input')!;
    expect(input).toBeInTheDocument();
    // Typing alone does not write anything: the value lands once, when the
    // editor commits. That is what keeps an interrupted IME composition from
    // writing half a word.
    fireEvent.change(input, { target: { value: 'Alic' } });
    expect(onDraftCellEdit).not.toHaveBeenCalled();
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onDraftCellEdit).toHaveBeenCalledWith('draft-a', 'name', 'Alic');

    // Clearing the cell means "do not write this column" — not "write NULL".
    const cleared = renderDrafts({
      editingDraft: { draftId: 'draft-a', column: 'name' },
      onDraftCellEdit,
    });
    const clearedInput = cleared.container.querySelector('input')!;
    fireEvent.change(clearedInput, { target: { value: '' } });
    fireEvent.keyDown(clearedInput, { key: 'Enter' });
    expect(onDraftCellEdit).toHaveBeenLastCalledWith('draft-a', 'name', undefined);
  });

  it('discards a draft row from its row-number slot', () => {
    const onDraftRowRemove = vi.fn();
    const { container } = renderDrafts({ onDraftRowRemove });

    const remove = container.querySelector('[data-draft-id="draft-a"] [data-testid="draft-row-remove"]')!;
    fireEvent.click(remove);

    expect(onDraftRowRemove).toHaveBeenCalledWith('draft-a');
  });

  it('activates the draft row the pointer is on', () => {
    const onDraftRowActivate = vi.fn();
    const { container } = renderDrafts({ onDraftRowActivate });

    fireEvent.click(container.querySelector('[data-testid="draft-row-1"]')!);

    expect(onDraftRowActivate).toHaveBeenCalledWith('draft-b');
  });

  it('offers the "add a row" entry below the last row', () => {
    const onAddDraftRow = vi.fn();
    const { container } = renderDrafts({ onAddDraftRow });

    const spacer = container.querySelector('[data-testid="draft-add-row"]')!;
    expect(spacer).toBeInTheDocument();
    fireEvent.doubleClick(spacer);
    expect(onAddDraftRow).toHaveBeenCalled();
  });

  it('renders no draft markup when the grid has no drafts', () => {
    const { container } = renderDrafts({ draftRows: undefined });

    expect(container.querySelector('[data-row-kind="draft"]')).toBeNull();
    expect(container.querySelector('[data-testid="draft-add-row"]')).toBeNull();
  });
});
