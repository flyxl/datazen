import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, waitFor } from '@testing-library/react';
import { Nl2SqlPanel } from '../Nl2SqlPanel';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

const openSettingsWindow = vi.fn();
vi.mock('../../../lib/windowManager', () => ({
  openSettingsWindow: (...args: unknown[]) => openSettingsWindow(...args),
}));

const aiState = vi.hoisted(() => ({
  isConfigured: true,
  nl2sql: { input: '', isGenerating: false, generatedSql: '' },
  nl2sqlError: null as string | null,
  setNl2SqlInput: vi.fn(),
  generateSql: vi.fn().mockResolvedValue(undefined),
  clearNl2Sql: vi.fn(),
}));

vi.mock('../../../stores/aiStore', () => ({
  useAiStore: (sel: (s: typeof aiState) => unknown) => sel(aiState),
}));

vi.mock('../AiInput', () => ({
  AiInput: ({
    value,
    onChange,
    onSubmit,
    disabled,
    placeholder,
  }: {
    value: string;
    onChange: (v: string) => void;
    onSubmit: () => void;
    disabled?: boolean;
    placeholder?: string;
  }) => (
    <div data-testid="ai-input">
      <textarea
        data-testid="nl2sql-textarea"
        value={value}
        placeholder={placeholder}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value)}
      />
      <button type="button" data-testid="nl2sql-submit" onClick={onSubmit}>submit</button>
    </div>
  ),
}));

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  aiState.isConfigured = true;
  aiState.nl2sql = { input: 'list users', isGenerating: false, generatedSql: '' };
  aiState.nl2sqlError = null;
});

describe('Nl2SqlPanel', () => {
  it('shows not configured state', () => {
    aiState.isConfigured = false;
    const { getByText } = render(
      <Nl2SqlPanel connectionId="c1" database="mydb" onSqlChange={vi.fn()} />,
    );
    expect(getByText('nl2sql.notConfigured')).toBeInTheDocument();
    fireEvent.click(getByText('settings.ai.goToConfigure'));
    expect(openSettingsWindow).toHaveBeenCalledWith('ai');
  });

  it('generates SQL and writes to editor when complete', async () => {
    const onSqlChange = vi.fn();
    aiState.nl2sql.generatedSql = 'SELECT 1';
    const { rerender, getByText } = render(
      <Nl2SqlPanel connectionId="c1" database="mydb" onSqlChange={onSqlChange} />,
    );
    fireEvent.click(getByText('nl2sql.generate'));
    expect(aiState.generateSql).toHaveBeenCalledWith({
      connectionId: 'c1',
      database: 'mydb',
      currentTable: undefined,
      contextFiles: undefined,
      contextTables: undefined,
    });

    aiState.nl2sql = { ...aiState.nl2sql, isGenerating: false, generatedSql: 'SELECT 1' };
    rerender(<Nl2SqlPanel connectionId="c1" database="mydb" onSqlChange={onSqlChange} />);
    await waitFor(() => expect(onSqlChange).toHaveBeenCalledWith('SELECT 1'));
  });

  it('clears input and shows error', () => {
    aiState.nl2sql.input = 'query';
    aiState.nl2sqlError = 'failed';
    const { getByText } = render(
      <Nl2SqlPanel connectionId="c1" database="mydb" onSqlChange={vi.fn()} />,
    );
    fireEvent.click(getByText('nl2sql.generate'));
    expect(getByText('failed')).toBeInTheDocument();
    const trashBtn = Array.from(document.querySelectorAll('button')).find(
      (b) => b.querySelector('.lucide-trash2'),
    )!;
    fireEvent.click(trashBtn);
    expect(aiState.clearNl2Sql).toHaveBeenCalled();
  });

  it('disables generate without database', () => {
    const { getByText } = render(
      <Nl2SqlPanel connectionId="c1" database="" onSqlChange={vi.fn()} />,
    );
    expect(getByText('nl2sql.generate').closest('button')).toBeDisabled();
  });
});
