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

const tableState = vi.hoisted(() => ({
  activeTable: 'users',
  columns: [{ name: 'name' }],
  setFilters: vi.fn(),
  clearFilters: vi.fn(),
}));

vi.mock('../../../stores/aiStore', () => ({
  useAiStore: (sel: (s: typeof aiState) => unknown) => sel(aiState),
}));

vi.mock('../../../stores/tableDataStore', () => ({
  useTableDataStore: Object.assign((sel: (s: typeof tableState) => unknown) => sel(tableState), {
    getState: () => tableState,
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
  tableState.activeTable = 'users';
  tableState.columns = [{ name: 'name' }];
});

describe('NlFilterInput', () => {
  it('shows not configured button', async () => {
    aiState.isConfigured = false;
    const { getByText } = render(
      <NlFilterInput dbSessionId="c1" database="db" tableName="users" />,
    );
    fireEvent.click(getByText('common.aiNotConfigured'));
    await waitFor(() => {
      expect(openSettingsWindow).toHaveBeenCalledWith('ai');
    });
  });

  it('expands, parses, and applies filters', async () => {
    const { container, getByText, rerender } = render(
      <NlFilterInput dbSessionId="c1" database="db" tableName="users" />,
    );
    fireEvent.click(container.querySelector('button')!);
    const input = container.querySelector('input')!;
    fireEvent.change(input, { target: { value: 'active users' } });
    rerender(<NlFilterInput dbSessionId="c1" database="db" tableName="users" />);
    fireEvent.click(getByText('smartFilter.parse'));
    await waitFor(() => {
      expect(aiState.parseFilter).toHaveBeenCalledWith({
        dbSessionId: 'c1',
        database: 'db',
        table: 'users',
      });
      expect(tableState.setFilters).toHaveBeenCalled();
    });
  });

  it('shows parsing, error, and parsed states', () => {
    aiState.isParsingFilter = true;
    const { container, rerender, getByText } = render(
      <NlFilterInput dbSessionId="c1" database="db" tableName="users" />,
    );
    fireEvent.click(container.querySelector('button')!);
    expect(getByText('smartFilter.parsing')).toBeInTheDocument();

    aiState.isParsingFilter = false;
    aiState.nlFilterError = 'bad prompt';
    rerender(<NlFilterInput dbSessionId="c1" database="db" tableName="users" />);
    expect(getByText('bad prompt')).toBeInTheDocument();

    aiState.nlFilterError = null;
    aiState.parsedFilters = [];
    rerender(<NlFilterInput dbSessionId="c1" database="db" tableName="users" />);
    expect(getByText('smartFilter.noFilters')).toBeInTheDocument();

    aiState.parsedFilters = [{ column: 'a' }, { column: 'b' }];
    rerender(<NlFilterInput dbSessionId="c1" database="db" tableName="users" />);
    expect(getByText('smartFilter.parsed')).toBeInTheDocument();
  });

  it('does not apply filters that reference unknown columns', async () => {
    aiState.parseFilter.mockResolvedValueOnce([
      { column: 'category', operator: 'eq', value: 'shipped' },
    ]);
    const { container, getByText, rerender } = render(
      <NlFilterInput dbSessionId="c1" database="db" tableName="users" />,
    );
    fireEvent.click(container.querySelector('button')!);
    fireEvent.change(container.querySelector('input')!, { target: { value: 'shipped' } });
    rerender(<NlFilterInput dbSessionId="c1" database="db" tableName="users" />);
    fireEvent.click(getByText('smartFilter.parse'));

    await waitFor(() => {
      expect(getByText('smartFilter.invalidColumns')).toBeInTheDocument();
    });
    expect(tableState.setFilters).not.toHaveBeenCalled();
  });

  it('clears and collapses on X click', () => {
    aiState.nlFilterInput = 'test';
    const { container } = render(
      <NlFilterInput dbSessionId="c1" database="db" tableName="users" />,
    );
    fireEvent.click(container.querySelector('button')!);
    const closeBtn = Array.from(container.querySelectorAll('button')).pop()!;
    fireEvent.click(closeBtn);
    expect(aiState.clearNlFilter).toHaveBeenCalled();
    expect(tableState.clearFilters).toHaveBeenCalled();
  });
});
