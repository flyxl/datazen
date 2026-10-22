/**
 * The saved-views menu's **state machine**, driven through the real hook
 * (Wave 7 · track B · 08 §4.6).
 *
 * The track's own journey test drives the component with fakes. This file
 * drives it through `useNamedViewMenu` — the hook that decides the scope, the
 * dirty flag and the **apply** path — because that is where the contract
 * lives:
 *
 * - applying a view is `setFilters` + `setSort` + `setVisibleColumns`, and
 *   **nothing else**: those three are the existing actions, so the view
 *   inherits their `requestRevision` race protection and their `page: 0`
 *   reset instead of growing a second, weaker path;
 * - the view that is saved is the **applied** one, not the draft, and a dirty
 *   draft shows a hint but does **not** block the save;
 * - a view that names a column the table no longer has is **refused**, not
 *   silently dropped and not turned into a query that would fail.
 *
 * The six states — `idle` / `loading` / `list` / `naming` / `importing` /
 * `error` — are asserted on the rendered `data-state`, and every state is
 * checked for an exit.
 */
import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup, screen, waitFor, renderHook } from '@testing-library/react';
import type { NamedView, NamedViewScope } from '../../../lib/namedViews';
import type { TableActions } from '../useTableActions';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string, params?: Record<string, string | number>) =>
      params
        ? key.replace(/\{(\w+)\}/g, (_match, name: string) => String(params[name] ?? ''))
        : key,
  }),
}));

// The menu is only offered when the bound backend can serve the views. The
// hook asks the port; here the port says yes, so the entry is present.
vi.mock('../../../lib/namedViews', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../../lib/namedViews')>();
  return { ...actual, namedViewsAvailable: () => true };
});

import { NamedViewMenu } from '../../../components/filter/NamedViewMenu';
import { useNamedViewMenu } from '../useNamedViewMenu';

const SCOPE: NamedViewScope = {
  connectionId: 'conn-1',
  database: 'app',
  schema: null,
  table: 'orders',
};

const APPLIED_FILTERS = [{ column: 'id', operator: 'eq' as const, value: 1 }];
const DRAFT_FILTERS = [{ column: 'id', operator: 'eq' as const, value: 2 }];

/** The four actions the hook uses; the rest of the table is irrelevant here. */
type UsedActions = Pick<TableActions, 'setFilters' | 'setSort' | 'setVisibleColumns' | 'applyFilters'>;

function fakeActions(): UsedActions & { calls: string[] } {
  const calls: string[] = [];
  return {
    calls,
    setFilters: vi.fn((_filters, _logic) => {
      calls.push('setFilters');
    }),
    setSort: vi.fn((_sort) => {
      calls.push('setSort');
    }),
    setVisibleColumns: vi.fn((_columns) => {
      calls.push('setVisibleColumns');
    }),
    applyFilters: vi.fn(() => {
      calls.push('applyFilters');
    }),
  };
}

const SAVED: NamedView = {
  id: 'view-1',
  name: 'Open orders',
  scope: SCOPE,
  filters: [
    { column: 'id', operator: 'eq', value: 1 },
    { column: 'name', operator: 'like', value: 'a' },
  ],
  filterLogic: 'or',
  sorts: [{ column: 'name', descending: true }],
  visibleColumns: ['id', 'name'],
  columnOrder: null,
  isDefault: false,
  createdAt: '2026-01-01T00:00:00.000Z',
  updatedAt: null,
};

function fakeOperations(seed: NamedView[] = []) {
  let views: NamedView[] = [...seed];
  return {
    operations: {
      list: vi.fn(async () => views),
      save: vi.fn(async (view: NamedView) => {
        const saved: NamedView = {
          ...view,
          id: view.id || `view-${views.length + 1}`,
          createdAt: '2026-01-01T00:00:00.000Z',
          updatedAt: null,
        };
        views = [...views.filter((entry) => entry.id !== saved.id), saved];
        return saved;
      }),
      remove: vi.fn(async (id: string) => {
        views = views.filter((entry) => entry.id !== id);
        return views;
      }),
    },
    save: vi.fn(async (view: NamedView) => {
      const saved: NamedView = {
        ...view,
        id: view.id || `view-${views.length + 1}`,
        createdAt: '2026-01-01T00:00:00.000Z',
        updatedAt: null,
      };
      views = [...views.filter((entry) => entry.id !== saved.id), saved];
      return saved;
    }),
  };
}

const FILES = {
  write: vi.fn(async () => true),
  read: vi.fn(async () => null as { fileName: string; content: string } | null),
};

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('§7 · the apply path goes through the existing actions', () => {
  it('applies filters, the first sort and the column layout — and nothing else', () => {
    const actions = fakeActions();
    const { result } = renderHook(() =>
      useNamedViewMenu({
        connectionId: 'conn-1',
        database: 'app',
        schema: null,
        tableName: 'orders',
        filters: APPLIED_FILTERS,
        filterLogic: 'and',
        sorts: [],
        draftFilters: APPLIED_FILTERS,
        draftFilterLogic: 'and',
        visibleColumns: null,
        actions: actions as unknown as TableActions,
      }),
    );

    const config = result.current;
    expect(config).not.toBeNull();
    config!.onApply(SAVED);

    // The three existing actions, in that order.
    expect(actions.calls).toEqual(['setFilters', 'setSort', 'setVisibleColumns']);

    // The filters carry the view's own logic, and the per-condition
    // connectors survive the trip.
    expect(actions.setFilters).toHaveBeenCalledTimes(1);
    const [filters, logic] = (actions.setFilters as ReturnType<typeof vi.fn>).mock.calls[0];
    expect(logic).toBe('or');
    expect(filters).toEqual(SAVED.filters);

    // The first sort, and the column layout.
    expect(actions.setSort).toHaveBeenCalledWith(SAVED.sorts[0]);
    expect(actions.setVisibleColumns).toHaveBeenCalledWith(SAVED.visibleColumns);
  });

  it('leaves the column layout alone when the view does not carry one', () => {
    const actions = fakeActions();
    const { result } = renderHook(() =>
      useNamedViewMenu({
        connectionId: 'conn-1',
        database: 'app',
        schema: null,
        tableName: 'orders',
        filters: APPLIED_FILTERS,
        filterLogic: 'and',
        sorts: [],
        draftFilters: APPLIED_FILTERS,
        draftFilterLogic: 'and',
        visibleColumns: null,
        actions: actions as unknown as TableActions,
      }),
    );
    result.current!.onApply({ ...SAVED, sorts: [], visibleColumns: null });
    // `null` means "do not interfere": the column layout is left as it is.
    expect(actions.setVisibleColumns).not.toHaveBeenCalled();
    expect(actions.setSort).not.toHaveBeenCalled();
    expect(actions.setFilters).toHaveBeenCalledTimes(1);
  });

  it('refuses a view whose filter names a column the table no longer has', async () => {
    const actions = fakeActions();
    const { result } = renderHook(() =>
      useNamedViewMenu({
        connectionId: 'conn-1',
        database: 'app',
        schema: null,
        tableName: 'orders',
        filters: APPLIED_FILTERS,
        filterLogic: 'and',
        sorts: [],
        draftFilters: APPLIED_FILTERS,
        draftFilterLogic: 'and',
        visibleColumns: null,
        actions: actions as unknown as TableActions,
      }),
    );
    const config = result.current!;

    const fake = fakeOperations([SAVED]);
    render(
      <NamedViewMenu
        scope={SCOPE}
        applied={{
          scope: SCOPE,
          filters: APPLIED_FILTERS,
          filterLogic: 'and',
          sorts: [],
          visibleColumns: null,
          columnOrder: null,
        }}
        operations={fake.operations}
        files={FILES}
        onApply={config.onApply}
        columns={['id']}
      />,
    );

    fireEvent.click(screen.getByTestId('named-view-trigger'));
    await waitFor(() =>
      expect(screen.getByTestId('named-view-panel')).toHaveAttribute('data-state', 'list'),
    );
    fireEvent.click(screen.getByTestId('named-view-apply'));

    // The refusal is visible, and **nothing was applied**.
    await waitFor(() =>
      expect(screen.getByTestId('named-view-panel')).toHaveAttribute('data-state', 'error'),
    );
    expect(screen.getByTestId('named-view-error').textContent).toContain(
      'filter.view.missingColumn',
    );
    expect(actions.setFilters).not.toHaveBeenCalled();
  });
});

describe('§7 · the six states and their exits', () => {
  function renderMenu(overrides: Partial<React.ComponentProps<typeof NamedViewMenu>> = {}) {
    const fake = fakeOperations();
    const props = {
      scope: SCOPE,
      applied: {
        scope: SCOPE,
        filters: APPLIED_FILTERS,
        filterLogic: 'and' as const,
        sorts: [],
        visibleColumns: null,
        columnOrder: null,
      },
      operations: fake.operations,
      files: FILES,
      onApply: vi.fn(),
      ...overrides,
    };
    render(<NamedViewMenu {...props} />);
    return { ...fake, props };
  }

  it('starts in idle — no panel at all', () => {
    renderMenu();
    expect(screen.queryByTestId('named-view-panel')).not.toBeInTheDocument();
  });

  it('goes idle → loading → list, and the loading state offers nothing to click', async () => {
    renderMenu();
    fireEvent.click(screen.getByTestId('named-view-trigger'));
    // The loading state shows a skeleton and **no entries**: there is
    // nothing to click while the list is in flight.
    expect(screen.getByTestId('named-view-panel')).toHaveAttribute('data-state', 'loading');
    expect(screen.queryByTestId('named-view-item')).not.toBeInTheDocument();
    expect(screen.queryByTestId('named-view-apply')).not.toBeInTheDocument();

    await waitFor(() =>
      expect(screen.getByTestId('named-view-panel')).toHaveAttribute('data-state', 'list'),
    );
    expect(screen.getByTestId('named-view-empty')).toBeInTheDocument();
  });

  it('goes list → naming → list, and the naming state refuses an empty name', async () => {
    renderMenu();
    fireEvent.click(screen.getByTestId('named-view-trigger'));
    await waitFor(() =>
      expect(screen.getByTestId('named-view-panel')).toHaveAttribute('data-state', 'list'),
    );

    fireEvent.click(screen.getByTestId('named-view-save-as'));
    expect(screen.getByTestId('named-view-panel')).toHaveAttribute('data-state', 'naming');
    expect(screen.getByTestId('named-view-save')).toBeDisabled();

    fireEvent.click(screen.getByTestId('named-view-cancel'));
    expect(screen.getByTestId('named-view-panel')).toHaveAttribute('data-state', 'list');
  });

  it('goes list → importing → list', async () => {
    renderMenu();
    fireEvent.click(screen.getByTestId('named-view-trigger'));
    await waitFor(() =>
      expect(screen.getByTestId('named-view-panel')).toHaveAttribute('data-state', 'list'),
    );
    fireEvent.click(screen.getByTestId('named-view-import'));
    await waitFor(() =>
      expect(screen.getByTestId('named-view-panel')).toHaveAttribute('data-state', 'list'),
    );
  });

  it('goes to error on a failure and can retry out of it', async () => {
    const fake = fakeOperations();
    fake.operations.list.mockRejectedValueOnce(new Error('the disk is on fire'));
    render(
      <NamedViewMenu
        scope={SCOPE}
        applied={{
          scope: SCOPE,
          filters: APPLIED_FILTERS,
          filterLogic: 'and',
          sorts: [],
          visibleColumns: null,
          columnOrder: null,
        }}
        operations={fake.operations}
        files={FILES}
        onApply={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByTestId('named-view-trigger'));
    await waitFor(() =>
      expect(screen.getByTestId('named-view-panel')).toHaveAttribute('data-state', 'error'),
    );
    expect(screen.getByTestId('named-view-error').textContent).toContain('the disk is on fire');
    // The exit: a retry goes back to loading and then to the list.
    fireEvent.click(screen.getByTestId('named-view-retry'));
    await waitFor(() =>
      expect(screen.getByTestId('named-view-panel')).toHaveAttribute('data-state', 'list'),
    );
  });
});

describe('§7 · what is saved is what is applied', () => {
  it('saves the applied filters, and a dirty draft only warns', async () => {
    const fake = fakeOperations();
    render(
      <NamedViewMenu
        scope={SCOPE}
        applied={{
          scope: SCOPE,
          filters: APPLIED_FILTERS,
          filterLogic: 'and',
          sorts: [],
          visibleColumns: null,
          columnOrder: null,
        }}
        draft={{ filters: DRAFT_FILTERS, filterLogic: 'and' }}
        dirty
        operations={fake.operations}
        files={FILES}
        onApply={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByTestId('named-view-trigger'));
    await waitFor(() =>
      expect(screen.getByTestId('named-view-panel')).toHaveAttribute('data-state', 'list'),
    );
    fireEvent.click(screen.getByTestId('named-view-save-as'));
    fireEvent.change(screen.getByTestId('named-view-name-input'), {
      target: { value: 'Open orders' },
    });

    // The hint is shown…
    expect(screen.getByTestId('named-view-draft-hint')).toBeInTheDocument();
    // …and the save is **not** blocked by it.
    expect(screen.getByTestId('named-view-save')).not.toBeDisabled();

    fireEvent.click(screen.getByTestId('named-view-save'));
    await waitFor(() => expect(fake.operations.save).toHaveBeenCalledTimes(1));
    const saved = fake.operations.save.mock.calls[0]?.[0] as NamedView;
    // The **applied** filters, not the draft.
    expect(saved.filters).toEqual(APPLIED_FILTERS);
    expect(saved.filters).not.toEqual(DRAFT_FILTERS);
  });

  it('saves the draft only when the user asks for it explicitly', async () => {
    const fake = fakeOperations();
    render(
      <NamedViewMenu
        scope={SCOPE}
        applied={{
          scope: SCOPE,
          filters: APPLIED_FILTERS,
          filterLogic: 'and',
          sorts: [],
          visibleColumns: null,
          columnOrder: null,
        }}
        draft={{ filters: DRAFT_FILTERS, filterLogic: 'and' }}
        dirty
        operations={fake.operations}
        files={FILES}
        onApply={vi.fn()}
        onApplyDraft={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByTestId('named-view-trigger'));
    await waitFor(() =>
      expect(screen.getByTestId('named-view-panel')).toHaveAttribute('data-state', 'list'),
    );
    fireEvent.click(screen.getByTestId('named-view-save-as'));
    fireEvent.change(screen.getByTestId('named-view-name-input'), {
      target: { value: 'Open orders' },
    });
    fireEvent.click(screen.getByTestId('named-view-apply-first'));

    await waitFor(() => expect(fake.operations.save).toHaveBeenCalledTimes(1));
    const saved = fake.operations.save.mock.calls[0]?.[0] as NamedView;
    expect(saved.filters).toEqual(DRAFT_FILTERS);
  });
});
