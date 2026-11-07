import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/react';
import { ExplainPanel } from '../ExplainPanel';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

const openSettingsWindow = vi.fn();
vi.mock('../../../lib/windowManager', () => ({
  openSettingsWindow: (...args: unknown[]) => openSettingsWindow(...args),
}));

vi.mock('../../query/ExplainPlanTree', () => ({
  ExplainPlanTree: ({ planTree }: { planTree?: unknown }) => (
    <div data-testid="explain-plan">{JSON.stringify(planTree)}</div>
  ),
}));

const dataTableCalls = vi.hoisted(() => ({
  last: null as { columns: { id: string; name: string }[]; rows: unknown[][] } | null,
}));

vi.mock('../../DataTable/DataTable', () => ({
  DataTable: (props: { columns: { id: string; name: string }[]; rows: unknown[][] }) => {
    dataTableCalls.last = { columns: props.columns, rows: props.rows };
    return <div data-testid="explain-datatable" />;
  },
}));

const aiState = vi.hoisted(() => ({
  explainAnalysis: null as {
    summary: string;
    bottlenecks: { severity: string; node: string; description: string }[];
    suggestions: { description: string; sql?: string; impact: string }[];
  } | null,
  isAnalyzingExplain: false,
  explainError: null as string | null,
  isConfigured: true,
  analyzeExplain: vi.fn().mockResolvedValue(undefined),
  clearExplainAnalysis: vi.fn(),
}));

vi.mock('../../../stores/aiStore', () => ({
  useAiStore: (sel: (s: typeof aiState) => unknown) => sel(aiState),
}));

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  aiState.explainAnalysis = null;
  aiState.isAnalyzingExplain = false;
  aiState.explainError = null;
  aiState.isConfigured = true;
});

describe('ExplainPanel', () => {
  it('renders raw explain output and plan tree', () => {
    const { getByText, getByTestId } = render(
      <ExplainPanel
        dbSessionId="c1"
        sql="SELECT 1"
        explainOutput="Seq Scan on users"
        planTree={{
          id: 'pg',
          label: 'Seq Scan',
          details: [],
          children: [],
        }}
      />,
    );
    expect(getByText('Seq Scan on users')).toBeInTheDocument();
    expect(getByTestId('explain-plan')).toBeInTheDocument();
    fireEvent.click(getByText('explain.analyze'));
    expect(aiState.analyzeExplain).toHaveBeenCalledWith({
      dbSessionId: 'c1',
      explainOutput: 'Seq Scan on users',
      originalSql: 'SELECT 1',
    });
  });

  it('renders raw explain output as a DataTable when planJson carries columns/rows', () => {
    const planJson = {
      query_block: { nested_loop: [{ table: { table_name: 'ot', access_type: 'ref' } }] },
      columns: ['id', 'select_type', 'table', 'type', 'key', 'rows'],
      rows: [['1', 'SIMPLE', 'ot', 'ref', 'idx_uid', '69']],
    };
    const { getByTestId, queryByText } = render(
      <ExplainPanel
        dbSessionId="c1"
        sql="SELECT 1"
        explainOutput="raw text fallback"
        planJson={planJson}
      />,
    );
    // DataTable is rendered with the raw EXPLAIN columns/rows.
    expect(getByTestId('explain-datatable')).toBeInTheDocument();
    expect(dataTableCalls.last?.columns.map((c) => c.name)).toEqual([
      'id',
      'select_type',
      'table',
      'type',
      'key',
      'rows',
    ]);
    expect(dataTableCalls.last?.rows).toEqual([['1', 'SIMPLE', 'ot', 'ref', 'idx_uid', '69']]);
    // The plain-text fallback is not rendered.
    expect(queryByText('raw text fallback')).toBeNull();
  });

  it('shows analyzing and error states', () => {
    aiState.isAnalyzingExplain = true;
    const { getByText, rerender } = render(
      <ExplainPanel dbSessionId="c1" sql="" explainOutput="out" />,
    );
    expect(getByText('explain.analyzing')).toBeInTheDocument();

    aiState.isAnalyzingExplain = false;
    aiState.explainError = 'AI down';
    rerender(<ExplainPanel dbSessionId="c1" sql="" explainOutput="out" />);
    expect(getByText('AI down')).toBeInTheDocument();
  });

  it('renders analysis with bottlenecks and apply SQL', () => {
    const onApplySql = vi.fn();
    aiState.explainAnalysis = {
      summary: 'Slow scan',
      bottlenecks: [
        { severity: 'high', node: 'Seq Scan', description: 'Full table scan' },
        { severity: 'low', node: 'Sort', description: 'In memory' },
      ],
      suggestions: [
        { description: 'Add index', sql: 'CREATE INDEX idx ON users(id)', impact: 'large' },
      ],
    };
    const { getByText } = render(
      <ExplainPanel
        dbSessionId="c1"
        sql="SELECT * FROM users"
        explainOutput="plan"
        onApplySql={onApplySql}
      />,
    );
    expect(getByText('Slow scan')).toBeInTheDocument();
    expect(getByText('Seq Scan')).toBeInTheDocument();
    const highRow = getByText('Seq Scan').closest('div.rounded.border');
    expect(highRow?.className).toMatch(/text-danger|border-danger/);
    const lowRow = getByText('Sort').closest('div.rounded.border');
    expect(lowRow?.className).toMatch(/text-accent|border-accent/);
    fireEvent.click(getByText('nl2sql.apply'));
    expect(onApplySql).toHaveBeenCalledWith('CREATE INDEX idx ON users(id)');
  });

  it('shows not configured footer', () => {
    aiState.isConfigured = false;
    const { getByText } = render(<ExplainPanel dbSessionId="c1" sql="" explainOutput="x" />);
    fireEvent.click(getByText('settings.ai.goToConfigure'));
    expect(openSettingsWindow).toHaveBeenCalledWith('ai');
  });
});
