/**
 * ErrorBanner extraction parity — NlFilterInput, the one component with *two*
 * announcement bars.
 *
 * The parse failure moved onto `ErrorBanner`; the validation warning did not,
 * because it is a different thing: the input parsed, but the draft references
 * a column that does not exist. Rendering both is the point — it pins that the
 * red one is the shared component and the amber one stayed local, so neither
 * can quietly become the other.
 */
import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, screen, waitFor } from '@testing-library/react';
import { NlFilterInput } from '../NlFilterInput';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useAiKeyboard', () => ({
  useAiKeyboard: (onSubmit: () => void) => ({
    onKeyDown: (e: React.KeyboardEvent) => {
      if (e.key === 'Enter') onSubmit();
    },
  }),
}));

const aiState = vi.hoisted(() => ({
  isConfigured: true,
  nlFilterInput: '',
  setNlFilterInput: vi.fn((v: string) => {
    aiState.nlFilterInput = v;
  }),
  parsedFilters: null as unknown[] | null,
  isParsingFilter: false,
  nlFilterError: null as string | null,
  parseFilter: vi.fn().mockResolvedValue([{ column: 'name', operator: 'eq', value: 'x' }]),
  clearNlFilter: vi.fn(() => {
    aiState.nlFilterInput = '';
    aiState.parsedFilters = null;
    aiState.nlFilterError = null;
  }),
}));

const tableStore = vi.hoisted(() => ({
  byPanel: new Map<string, unknown>(),
  setFilters: vi.fn(),
  clearFilters: vi.fn(),
}));

const PANEL = 'panel-1';

vi.mock('../../../stores/aiStore', () => ({
  useAiStore: (sel: (s: typeof aiState) => unknown) => sel(aiState),
}));

vi.mock('../../../stores/tableDataStore', () => ({
  useTableDataStore: Object.assign((sel: (s: typeof tableStore) => unknown) => sel(tableStore), {
    getState: () => tableStore,
  }),
}));

vi.mock('../../../lib/windowManager', () => ({
  openSettingsWindow: vi.fn(),
}));

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  aiState.isConfigured = true;
  aiState.nlFilterInput = '';
  aiState.parsedFilters = null;
  aiState.isParsingFilter = false;
  aiState.nlFilterError = null;
  tableStore.byPanel = new Map();
  tableStore.byPanel.set(PANEL, {
    context: { dbSessionId: 'c1', table: 'users' },
    columns: [{ name: 'name' }],
  });
});

/** Expand, type a query, and press Parse. The store mock is not reactive. */
async function parse(query: string, result: unknown[]) {
  aiState.parseFilter.mockResolvedValue(result);
  const view = render(
    <NlFilterInput panelId={PANEL} dbSessionId="c1" database="db" tableName="users" />,
  );
  fireEvent.click(view.container.querySelector('button') as HTMLButtonElement);
  fireEvent.change(view.container.querySelector('input') as HTMLInputElement, {
    target: { value: query },
  });
  view.rerender(<NlFilterInput panelId={PANEL} dbSessionId="c1" database="db" tableName="users" />);
  fireEvent.click(screen.getByText('smartFilter.parse'));
  await waitFor(() => expect(aiState.parseFilter).toHaveBeenCalled());
  return view;
}

function renderInput() {
  return render(<NlFilterInput panelId={PANEL} dbSessionId="c1" database="db" tableName="users" />);
}

describe('NlFilterInput: the parse failure is the shared error bar', () => {
  it('is announced, red, and indented past the icon gutter', async () => {
    const view = renderInput();
    fireEvent.click(view.container.querySelector('button') as HTMLButtonElement);
    fireEvent.change(view.container.querySelector('input') as HTMLInputElement, {
      target: { value: 'active users' },
    });
    view.rerender(
      <NlFilterInput panelId={PANEL} dbSessionId="c1" database="db" tableName="users" />,
    );
    fireEvent.click(screen.getByText('smartFilter.parse'));
    await waitFor(() => expect(aiState.parseFilter).toHaveBeenCalled());

    // The store's copy is what gets announced — the component does not invent
    // its own wording for a failure it did not observe.
    aiState.nlFilterError = 'The AI service is unreachable';
    view.rerender(
      <NlFilterInput panelId={PANEL} dbSessionId="c1" database="db" tableName="users" />,
    );

    const banner = await screen.findByText('The AI service is unreachable');
    expect(banner.tagName).toBe('DIV');
    expect(banner).toHaveAttribute('role', 'alert');
    // The call site's `text-danger` overrides the variant's red outright; only
    // the shared type ramp and indent survive from the component.
    expect(banner).toHaveClass('text-xs', 'pl-9', 'text-danger');
    expect(banner).not.toHaveClass('text-red-400');
  });

  it('is absent while there is no parse failure', async () => {
    await parse('active users', [{ column: 'name', operator: 'eq', value: 'x' }]);
    expect(screen.queryByRole('alert')).toBeNull();
  });
});

describe('NlFilterInput: the validation warning stayed local', () => {
  it("is announced, amber, and carries none of ErrorBanner's chrome", async () => {
    // A result naming a column the table does not have: parsed, but unusable.
    await parse('shipped', [{ column: 'category', operator: 'eq', value: 'shipped' }]);

    const warning = await screen.findByText('smartFilter.invalidColumns');
    expect(warning.tagName).toBe('DIV');
    expect(warning).toHaveAttribute('role', 'alert');
    // Amber, and nothing from ErrorBanner's red or its boxed surface.
    expect(warning).toHaveClass('pl-9', 'text-xs', 'text-warning');
    expect(warning.className).not.toContain('text-red');
    expect(warning.className).not.toContain('text-danger');
    expect(warning.className).not.toContain('bg-red');
    // The unusable draft is not applied.
    expect(tableStore.setFilters).not.toHaveBeenCalled();
  });
});
