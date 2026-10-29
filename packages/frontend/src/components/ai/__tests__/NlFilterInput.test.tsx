import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, waitFor } from '@testing-library/react';
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

/** The parse result is only applied when the panel slice still points at that table. */
function seedPanelSlice(
  context: { table: string; dbSessionId?: string },
  columns: { name: string }[],
) {
  tableStore.byPanel.set(PANEL, { context: { dbSessionId: 'c1', ...context }, columns });
}

vi.mock('../../../stores/aiStore', () => ({
  useAiStore: (sel: (s: typeof aiState) => unknown) => sel(aiState),
}));

vi.mock('../../../stores/tableDataStore', () => ({
  useTableDataStore: Object.assign((sel: (s: typeof tableStore) => unknown) => sel(tableStore), {
    getState: () => tableStore,
  }),
}));

const openSettingsWindow = vi.fn();
vi.mock('../../../lib/windowManager', () => ({
  openSettingsWindow: (...args: unknown[]) => openSettingsWindow(...args),
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
  seedPanelSlice({ table: 'users' }, [{ name: 'name' }]);
});

function renderInput() {
  return render(<NlFilterInput panelId={PANEL} dbSessionId="c1" database="db" tableName="users" />);
}

describe('NlFilterInput', () => {
  it('shows not configured button', async () => {
    aiState.isConfigured = false;
    const { getByText } = renderInput();
    fireEvent.click(getByText('common.aiNotConfigured'));
    await waitFor(() => {
      expect(openSettingsWindow).toHaveBeenCalledWith('ai');
    });
  });

  it('expands, parses, and applies filters', async () => {
    const { container, getByText, rerender } = renderInput();
    fireEvent.click(container.querySelector('button')!);
    const input = container.querySelector('input')!;
    fireEvent.change(input, { target: { value: 'active users' } });
    rerender(<NlFilterInput panelId={PANEL} dbSessionId="c1" database="db" tableName="users" />);
    fireEvent.click(getByText('smartFilter.parse'));
    await waitFor(() => {
      expect(aiState.parseFilter).toHaveBeenCalledWith({
        dbSessionId: 'c1',
        database: 'db',
        table: 'users',
      });
      expect(tableStore.setFilters).toHaveBeenCalledWith(PANEL, [
        { column: 'name', operator: 'eq', value: 'x' },
      ]);
    });
  });

  it('shows parsing, error, and parsed states', () => {
    aiState.isParsingFilter = true;
    const { container, rerender, getByText } = renderInput();
    fireEvent.click(container.querySelector('button')!);
    expect(getByText('smartFilter.parsing')).toBeInTheDocument();

    aiState.isParsingFilter = false;
    aiState.nlFilterError = 'bad prompt';
    rerender(<NlFilterInput panelId={PANEL} dbSessionId="c1" database="db" tableName="users" />);
    expect(getByText('bad prompt')).toBeInTheDocument();

    aiState.nlFilterError = null;
    aiState.parsedFilters = [];
    rerender(<NlFilterInput panelId={PANEL} dbSessionId="c1" database="db" tableName="users" />);
    expect(getByText('smartFilter.noFilters')).toBeInTheDocument();

    aiState.parsedFilters = [{ column: 'a' }, { column: 'b' }];
    rerender(<NlFilterInput panelId={PANEL} dbSessionId="c1" database="db" tableName="users" />);
    expect(getByText('smartFilter.parsed')).toBeInTheDocument();
  });

  it('does not apply filters that reference unknown columns', async () => {
    aiState.parseFilter.mockResolvedValueOnce([
      { column: 'category', operator: 'eq', value: 'shipped' },
    ]);
    const { container, getByText, rerender } = renderInput();
    fireEvent.click(container.querySelector('button')!);
    fireEvent.change(container.querySelector('input')!, { target: { value: 'shipped' } });
    rerender(<NlFilterInput panelId={PANEL} dbSessionId="c1" database="db" tableName="users" />);
    fireEvent.click(getByText('smartFilter.parse'));

    await waitFor(() => {
      expect(getByText('smartFilter.invalidColumns')).toBeInTheDocument();
    });
    expect(tableStore.setFilters).not.toHaveBeenCalled();
  });

  it('ignores a parse result when the panel moved to another table', async () => {
    seedPanelSlice({ table: 'orders' }, [{ name: 'name' }]);
    const { container, getByText, rerender } = renderInput();
    fireEvent.click(container.querySelector('button')!);
    fireEvent.change(container.querySelector('input')!, { target: { value: 'active' } });
    rerender(<NlFilterInput panelId={PANEL} dbSessionId="c1" database="db" tableName="users" />);
    fireEvent.click(getByText('smartFilter.parse'));

    await waitFor(() => {
      expect(aiState.parseFilter).toHaveBeenCalled();
    });
    expect(tableStore.setFilters).not.toHaveBeenCalled();
  });

  it('clears and collapses on X click', () => {
    aiState.nlFilterInput = 'test';
    const { container } = renderInput();
    fireEvent.click(container.querySelector('button')!);
    const closeBtn = Array.from(container.querySelectorAll('button')).pop()!;
    fireEvent.click(closeBtn);
    expect(aiState.clearNlFilter).toHaveBeenCalled();
    expect(tableStore.clearFilters).toHaveBeenCalledWith(PANEL);
  });
});
