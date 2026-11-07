import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, waitFor } from '@testing-library/react';
import { Nl2SqlPanel } from '../Nl2SqlPanel';
import type { AiMemoryReference } from '../../../types';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

const openSettingsWindow = vi.fn();
vi.mock('../../../lib/windowManager', () => ({
  openSettingsWindow: (...args: unknown[]) => openSettingsWindow(...args),
}));

const aiState = vi.hoisted(() => ({
  isConfigured: true,
  settingsConfig: null as {
    memory?: {
      enabled: boolean;
      autoLearn: boolean;
      cloudPolicy: 'normal_only' | 'allow_internal' | 'disable_cloud';
      privacyAcknowledged: boolean;
    };
  } | null,
  nl2sql: {
    input: '',
    isGenerating: false,
    generatedSql: '',
    memoryUsed: [] as AiMemoryReference[],
  },
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
      <button type="button" data-testid="nl2sql-submit" onClick={onSubmit}>
        submit
      </button>
    </div>
  ),
}));

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  aiState.isConfigured = true;
  aiState.nl2sql = { input: 'list users', isGenerating: false, generatedSql: '', memoryUsed: [] };
  aiState.nl2sqlError = null;
  aiState.settingsConfig = {
    memory: {
      enabled: true,
      autoLearn: false,
      cloudPolicy: 'normal_only',
      privacyAcknowledged: true,
    },
  };
});

describe('Nl2SqlPanel', () => {
  it('shows not configured state', () => {
    aiState.isConfigured = false;
    const { getByText } = render(
      <Nl2SqlPanel dbSessionId="c1" database="mydb" onSqlChange={vi.fn()} />,
    );
    expect(getByText('common.aiNotConfigured')).toBeInTheDocument();
    fireEvent.click(getByText('settings.ai.goToConfigure'));
    expect(openSettingsWindow).toHaveBeenCalledWith('ai');
  });

  it('generates SQL and writes to editor when complete', async () => {
    const onSqlChange = vi.fn();
    aiState.nl2sql.generatedSql = 'SELECT 1';
    const { rerender, getByText } = render(
      <Nl2SqlPanel
        dbSessionId="c1"
        connectionId="conn-1"
        database="mydb"
        onSqlChange={onSqlChange}
      />,
    );
    fireEvent.click(getByText('nl2sql.generate'));
    expect(aiState.generateSql).toHaveBeenCalledWith({
      dbSessionId: 'c1',
      connectionId: 'conn-1',
      database: 'mydb',
      currentTable: undefined,
      contextFiles: undefined,
      contextTables: undefined,
      memoryMode: 'auto',
      memoryScope: 'user',
    });

    aiState.nl2sql = { ...aiState.nl2sql, isGenerating: false, generatedSql: 'SELECT 1' };
    rerender(
      <Nl2SqlPanel
        dbSessionId="c1"
        connectionId="conn-1"
        database="mydb"
        onSqlChange={onSqlChange}
      />,
    );
    await waitFor(() => expect(onSqlChange).toHaveBeenCalledWith('SELECT 1'));
  });

  it('does not request memory when the setting is disabled', () => {
    aiState.settingsConfig = null;
    const { getByText } = render(
      <Nl2SqlPanel dbSessionId="c1" connectionId="conn-1" database="mydb" onSqlChange={vi.fn()} />,
    );
    fireEvent.click(getByText('nl2sql.generate'));
    expect(aiState.generateSql).toHaveBeenCalledWith(
      expect.objectContaining({ memoryMode: 'off' }),
    );
  });

  it('clears input and shows error', () => {
    // Trash button now renders when generatedSql exists (not input).
    aiState.nl2sql.input = 'query';
    aiState.nl2sql.generatedSql = 'SELECT 1';
    aiState.nl2sqlError = 'failed';
    const { getByText } = render(
      <Nl2SqlPanel dbSessionId="c1" database="mydb" onSqlChange={vi.fn()} />,
    );
    expect(getByText('failed')).toBeInTheDocument();
    const trashBtn = Array.from(document.querySelectorAll('button')).find((b) =>
      b.querySelector('.lucide-trash2'),
    )!;
    expect(trashBtn).toBeDefined();
    fireEvent.click(trashBtn);
    expect(aiState.clearNl2Sql).toHaveBeenCalled();
  });

  it('shows injected memories as an expandable chip with citation references', () => {
    aiState.nl2sql = {
      ...aiState.nl2sql,
      memoryUsed: [
        {
          reference: 'M1',
          id: 'mem-1',
          subject: 'Timezone preference',
          kind: 'preference',
          scope: 'user',
          score: 0.9,
        },
      ],
    };
    const { getByTestId, queryByTestId, getByText } = render(
      <Nl2SqlPanel dbSessionId="c1" connectionId="conn-1" database="mydb" onSqlChange={vi.fn()} />,
    );
    expect(getByTestId('ai-memory-chip')).toBeInTheDocument();
    expect(getByText('chat.memory.used')).toBeInTheDocument();
    expect(queryByTestId('ai-memory-expanded')).toBeNull();
    fireEvent.click(getByTestId('ai-memory-chip'));
    expect(getByTestId('ai-memory-expanded')).toBeInTheDocument();
    expect(getByText('Timezone preference')).toBeInTheDocument();
    fireEvent.click(getByTestId('ai-memory-chip'));
    expect(queryByTestId('ai-memory-expanded')).toBeNull();
    fireEvent.click(getByTestId('ai-memory-citation-M1'));
    expect(getByTestId('ai-memory-expanded')).toBeInTheDocument();
  });

  it('disables generate without database', () => {
    const { getByText } = render(
      <Nl2SqlPanel dbSessionId="c1" database="" onSqlChange={vi.fn()} />,
    );
    expect(getByText('nl2sql.generate').closest('button')).toBeDisabled();
  });
});
