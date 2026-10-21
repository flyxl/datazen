/**
 * Permanent guard for the extraction of the panel toolbar out of `TableView`
 * (DB-06 track, GC budget).
 *
 * The golden file is the **pre-split** markup: it was captured from a verbatim
 * copy of `TableView` at commit 78197e830 (the temporary probe rendered the old
 * and the new component with the same props and compared the DOM byte for
 * byte — all ten scenarios came out `identical: true`). The probe is deleted;
 * this file is what keeps the conclusion alive.
 *
 * A byte-exact comparison is deliberate: the toolbar carries the E2E
 * `data-testid` anchors (`table-data-refresh`, `table-insert-row`,
 * `table-filter-toggle`, `table-quick-filter`) and the quick-filter error
 * strip, and every one of them is a contract the E2E specs look up.
 */
import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/react';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { TableToolbar } from '../TableToolbar';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../components/ai/NlFilterInput', () => ({
  NlFilterInput: () => <div data-testid="mock-nl-filter" />,
}));

const GOLDEN = readFileSync(
  resolve(__dirname, '__fixtures__/table-toolbar.golden.html'),
  'utf-8',
);

const COLUMNS = [
  { name: 'id', dataType: 'int', nullable: false, isPrimaryKey: true },
  { name: 'name', dataType: 'varchar', nullable: true, isPrimaryKey: false },
];

function renderToolbar(overrides: Partial<React.ComponentProps<typeof TableToolbar>> = {}) {
  return render(
    <TableToolbar
      panelId="panel-users"
      dbSessionId="c1"
      database="app"
      tableName="users"
      columns={COLUMNS}
      loading={false}
      pendingBusy={false}
      isEditable
      filterPanelOpen={false}
      appliedFilterCount={0}
      draftFilterCount={0}
      onRefresh={vi.fn()}
      onSetFilterPanelOpen={vi.fn()}
      onAddFilter={vi.fn()}
      onSetFilters={vi.fn()}
      onClearFilters={vi.fn()}
      onAddDraftRow={vi.fn()}
      {...overrides}
    />,
  );
}

afterEach(cleanup);

describe('TableToolbar', () => {
  it('renders the pre-split toolbar markup byte for byte', () => {
    const { container } = renderToolbar();
    // An unparsable quick filter puts the error strip right after the toolbar —
    // the golden covers that strip as well, so the comparison has to reproduce it.
    const quick = container.querySelector(
      '[data-testid="table-quick-filter"]',
    ) as HTMLInputElement;
    fireEvent.change(quick, { target: { value: 'id ===' } });
    fireEvent.keyDown(quick, { key: 'Enter' });

    expect(container.innerHTML).toBe(GOLDEN);
  });

  it('keeps the E2E anchors of the toolbar', () => {
    const { container } = renderToolbar();
    for (const testId of [
      'table-data-refresh',
      'table-insert-row',
      'table-filter-toggle',
      'table-quick-filter',
    ]) {
      expect(container.querySelector(`[data-testid="${testId}"]`)).not.toBeNull();
    }
  });

  it('hides the insert button on a read-only panel', () => {
    const { container } = renderToolbar({ isEditable: false });
    expect(container.querySelector('[data-testid="table-insert-row"]')).toBeNull();
  });

  it('opens the manual filter and seeds a default filter when none is drafted', () => {
    const onSetFilterPanelOpen = vi.fn();
    const onAddFilter = vi.fn();
    const { container } = renderToolbar({ onSetFilterPanelOpen, onAddFilter });

    fireEvent.click(container.querySelector('[data-testid="table-filter-toggle"]') as HTMLElement);

    expect(onSetFilterPanelOpen).toHaveBeenCalledWith(true);
    expect(onAddFilter).toHaveBeenCalledWith({ column: 'id', operator: 'eq', value: '' });
  });

  it('closes the manual filter instead of seeding a filter when it is open', () => {
    const onSetFilterPanelOpen = vi.fn();
    const onAddFilter = vi.fn();
    const { container } = renderToolbar({
      filterPanelOpen: true,
      onSetFilterPanelOpen,
      onAddFilter,
    });

    fireEvent.click(container.querySelector('[data-testid="table-filter-toggle"]') as HTMLElement);

    expect(onSetFilterPanelOpen).toHaveBeenCalledWith(false);
    expect(onAddFilter).not.toHaveBeenCalled();
  });

  it('applies a quick filter that mixes AND and OR with a connector per condition', () => {
    const onSetFilters = vi.fn();
    const { container } = renderToolbar({ onSetFilters });
    const quick = container.querySelector(
      '[data-testid="table-quick-filter"]',
    ) as HTMLInputElement;

    fireEvent.change(quick, { target: { value: "id = 1 OR name = 'a' AND id > 0" } });
    fireEvent.keyDown(quick, { key: 'Enter' });

    expect(onSetFilters).toHaveBeenCalledTimes(1);
    const [conditions, logic] = onSetFilters.mock.calls[0] as [
      Array<{ column: string; operator: string; connector?: string }>,
      string,
    ];
    expect(conditions.map((entry) => entry.column)).toEqual(['id', 'name', 'id']);
    // The connector belongs to the condition it joins to the previous one;
    // the first condition has no predecessor.
    expect(conditions.map((entry) => entry.connector)).toEqual([
      undefined,
      'or',
      'and',
    ]);
    expect(logic).toBe('or');
  });

  it('refuses a quick filter whose parentheses group a different operator', () => {
    const onSetFilters = vi.fn();
    const { container } = renderToolbar({ onSetFilters });
    const quick = container.querySelector(
      '[data-testid="table-quick-filter"]',
    ) as HTMLInputElement;

    fireEvent.change(quick, { target: { value: "id = 1 AND (name = 'a' OR id > 0)" } });
    fireEvent.keyDown(quick, { key: 'Enter' });

    // The expression cannot be written as a flat list, so it is refused
    // rather than flattened into a different question.
    expect(onSetFilters).not.toHaveBeenCalled();
    expect(container.textContent).toContain('filter.nestedLogic');
  });

  it('clears the filters when the quick filter is emptied', () => {
    const onClearFilters = vi.fn();
    const { container } = renderToolbar({ onClearFilters });
    const quick = container.querySelector(
      '[data-testid="table-quick-filter"]',
    ) as HTMLInputElement;

    fireEvent.change(quick, { target: { value: '   ' } });
    fireEvent.keyDown(quick, { key: 'Enter' });

    expect(onClearFilters).toHaveBeenCalled();
  });
});
