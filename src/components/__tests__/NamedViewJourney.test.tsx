/**
 * The saved-views journey (08 §9.3).
 *
 * One journey, not a set of isolated legal states: open the menu → save a
 * view under a name → see it in the list → rename it in place → make it the
 * default → import a file that carries the same name → delete it behind a
 * confirmation → and finally watch a failing command land in the error state
 * with the classified text.
 *
 * The menu takes its three commands as props, so the journey runs against
 * fakes: what is asserted here is the menu's own behaviour, not the IPC.
 */
import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup, screen, waitFor } from '@testing-library/react';
import { NamedViewMenu } from '../filter/NamedViewMenu';
import type { NamedView, NamedViewScope } from '../../lib/namedViews';

vi.mock('../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string, params?: Record<string, string | number>) =>
      params
        ? key.replace(/\{(\w+)\}/g, (_match, name: string) => String(params[name] ?? ''))
        : key,
  }),
}));

const SCOPE: NamedViewScope = {
  connectionId: 'conn-1',
  database: 'app',
  schema: null,
  table: 'orders',
};

const APPLIED = {
  scope: SCOPE,
  filters: [{ column: 'id', operator: 'eq' as const, value: 1 }],
  filterLogic: 'and' as const,
  sorts: [],
  visibleColumns: null,
  columnOrder: null,
};

/** A stand-in for the host's three commands. */
function fakeOperations(seed: NamedView[] = []) {
  let views: NamedView[] = [...seed];
  const list = vi.fn(async () => views);
  const save = vi.fn(async (view: NamedView) => {
    const saved: NamedView = {
      ...view,
      id: view.id || `view-${views.length + 1}`,
      createdAt: '2026-01-01T00:00:00.000Z',
      updatedAt: null,
    };
    views = [...views.filter((entry) => entry.id !== saved.id), saved];
    return saved;
  });
  const remove = vi.fn(async (id: string) => {
    views = views.filter((entry) => entry.id !== id);
    return views;
  });
  return {
    operations: { list, save, remove },
    list,
    save,
    remove,
    stored: () => views,
  };
}

const FILES = {
  write: vi.fn(async () => true),
  read: vi.fn(async () => null as { fileName: string; content: string } | null),
};

function renderMenu(overrides: Partial<React.ComponentProps<typeof NamedViewMenu>> = {}) {
  const fake = fakeOperations();
  const onApply = vi.fn();
  const props = {
    scope: SCOPE,
    applied: APPLIED,
    operations: fake.operations,
    files: FILES,
    onApply,
    ...overrides,
  };
  render(<NamedViewMenu {...props} />);
  return { ...fake, onApply, props };
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('the saved-views journey', () => {
  it('goes from an empty menu to a saved, renamed, default, imported and deleted view', async () => {
    const { operations, save, remove, onApply } = renderMenu();

    // 1. The menu opens: `loading` first, then the list.
    fireEvent.click(screen.getByTestId('named-view-trigger'));
    expect(screen.getByTestId('named-view-panel')).toHaveAttribute('data-state', 'loading');
    await waitFor(() =>
      expect(screen.getByTestId('named-view-panel')).toHaveAttribute('data-state', 'list'),
    );
    expect(operations.list).toHaveBeenCalledWith(SCOPE);
    expect(screen.getByTestId('named-view-empty')).toBeInTheDocument();

    // 2. `Save as…` asks for a name.
    fireEvent.click(screen.getByTestId('named-view-save-as'));
    expect(screen.getByTestId('named-view-panel')).toHaveAttribute('data-state', 'naming');
    fireEvent.change(screen.getByTestId('named-view-name-input'), {
      target: { value: 'Open orders' },
    });

    // 3. Saving hands the **applied** filters to the host.
    fireEvent.click(screen.getByTestId('named-view-save'));
    await waitFor(() => expect(save).toHaveBeenCalledTimes(1));
    const saved = save.mock.calls[0]?.[0] as NamedView;
    expect(saved.name).toBe('Open orders');
    expect(saved.filters).toEqual(APPLIED.filters);
    expect(saved.scope).toEqual(SCOPE);
    expect(saved.id).toBe('');

    // 4. The new entry is in the list.
    await waitFor(() => expect(screen.getByTestId('named-view-item')).toBeInTheDocument());
    expect(screen.getByText('Open orders')).toBeInTheDocument();

    // 5. Renaming keeps the id.
    fireEvent.click(screen.getByTestId('named-view-rename'));
    fireEvent.change(screen.getByTestId('named-view-rename-input'), {
      target: { value: 'Open orders this week' },
    });
    fireEvent.keyDown(screen.getByTestId('named-view-rename-input'), { key: 'Enter' });
    await waitFor(() => expect(save).toHaveBeenCalledTimes(2));
    const renamed = save.mock.calls[1]?.[0] as NamedView;
    expect(renamed.name).toBe('Open orders this week');
    expect(renamed.id).toBe('view-1');

    // 6. The default badge appears on the default view.
    fireEvent.click(screen.getByTestId('named-view-set-default'));
    await waitFor(() =>
      expect(screen.getByTestId('named-view-default-badge')).toBeInTheDocument(),
    );
    expect(save.mock.calls[2]?.[0]).toMatchObject({ isDefault: true, id: 'view-1' });

    // 7. An import that carries the same name gets the `(2)` suffix.
    const imported = JSON.stringify({
      version: 1,
      exportedAt: '2026-01-02T00:00:00.000Z',
      views: [
        {
          id: 'other',
          name: 'Open orders this week',
          scope: { connectionId: 'conn-1', database: 'app', schema: null, table: 'orders' },
          filters: [],
          filterLogic: 'and',
          sorts: [],
          visibleColumns: null,
          columnOrder: null,
          isDefault: false,
          createdAt: '2026-01-02T00:00:00.000Z',
          updatedAt: null,
        },
      ],
    });
    FILES.read.mockResolvedValueOnce({ fileName: 'views.json', content: imported });
    fireEvent.click(screen.getByTestId('named-view-import'));
    await waitFor(() =>
      expect(screen.getByText('Open orders this week (2)')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('named-view-notice')).toHaveTextContent(
      'filter.view.importDone',
    );

    // 8. Applying a view hands it to the table and closes the menu.
    fireEvent.click(screen.getAllByTestId('named-view-apply')[0]);
    expect(onApply).toHaveBeenCalledTimes(1);
    expect(screen.queryByTestId('named-view-panel')).not.toBeInTheDocument();

    // 9. Deleting asks first, and the entry is gone once confirmed.
    fireEvent.click(screen.getByTestId('named-view-trigger'));
    await waitFor(() =>
      expect(screen.getByTestId('named-view-panel')).toHaveAttribute('data-state', 'list'),
    );
    fireEvent.click(screen.getAllByTestId('named-view-delete')[0]);
    const confirm = await screen.findByTestId('confirm-dialog-ok');
    fireEvent.click(confirm);
    await waitFor(() => expect(remove).toHaveBeenCalled());
  });

  it('shows the classified text when a command fails', async () => {
    const fake = fakeOperations();
    fake.list.mockRejectedValueOnce(
      new Error('grid.view.nameExists: a saved view named \'Users\' already exists for this table'),
    );
    render(
      <NamedViewMenu
        scope={SCOPE}
        applied={APPLIED}
        operations={fake.operations}
        files={FILES}
        onApply={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByTestId('named-view-trigger'));
    await waitFor(() =>
      expect(screen.getByTestId('named-view-panel')).toHaveAttribute('data-state', 'error'),
    );
    // The message is the classified one, not the raw wrapper text.
    expect(screen.getByTestId('named-view-error')).toHaveTextContent(
      'filter.view.errorNameExists',
    );

    // And the error state can be retried.
    fireEvent.click(screen.getByTestId('named-view-retry'));
    await waitFor(() =>
      expect(screen.getByTestId('named-view-panel')).toHaveAttribute('data-state', 'list'),
    );
  });

  it('refuses a name that is already taken, before the host is asked', async () => {
    const fake = fakeOperations([
      {
        id: 'view-1',
        name: 'Open orders',
        scope: SCOPE,
        filters: [],
        filterLogic: 'and',
        sorts: [],
        visibleColumns: null,
        columnOrder: null,
        isDefault: false,
        createdAt: '2026-01-01T00:00:00.000Z',
        updatedAt: null,
      },
    ]);
    render(
      <NamedViewMenu
        scope={SCOPE}
        applied={APPLIED}
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
      target: { value: 'open ORDERS' },
    });

    expect(screen.getByTestId('named-view-name-taken')).toBeInTheDocument();
    expect(screen.getByTestId('named-view-save')).toBeDisabled();
    expect(fake.save).not.toHaveBeenCalled();
  });
});
