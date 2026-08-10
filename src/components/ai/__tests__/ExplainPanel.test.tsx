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
  ExplainPlanTree: ({ planJson }: { planJson: unknown }) => (
    <div data-testid="explain-plan">{JSON.stringify(planJson)}</div>
  ),
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
        connectionId="c1"
        sql="SELECT 1"
        explainOutput="Seq Scan on users"
        planJson={{ Plan: { 'Node Type': 'Seq Scan' } }}
      />,
    );
    expect(getByText('Seq Scan on users')).toBeInTheDocument();
    expect(getByTestId('explain-plan')).toBeInTheDocument();
    fireEvent.click(getByText('explain.analyze'));
    expect(aiState.analyzeExplain).toHaveBeenCalledWith({
      connectionId: 'c1',
      explainOutput: 'Seq Scan on users',
      originalSql: 'SELECT 1',
    });
  });

  it('shows analyzing and error states', () => {
    aiState.isAnalyzingExplain = true;
    const { getByText, rerender } = render(
      <ExplainPanel connectionId="c1" sql="" explainOutput="out" />,
    );
    expect(getByText('explain.analyzing')).toBeInTheDocument();

    aiState.isAnalyzingExplain = false;
    aiState.explainError = 'AI down';
    rerender(<ExplainPanel connectionId="c1" sql="" explainOutput="out" />);
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
        connectionId="c1"
        sql="SELECT * FROM users"
        explainOutput="plan"
        onApplySql={onApplySql}
      />,
    );
    expect(getByText('Slow scan')).toBeInTheDocument();
    expect(getByText('Seq Scan')).toBeInTheDocument();
    fireEvent.click(getByText('nl2sql.apply'));
    expect(onApplySql).toHaveBeenCalledWith('CREATE INDEX idx ON users(id)');
  });

  it('shows not configured footer', () => {
    aiState.isConfigured = false;
    const { getByText } = render(
      <ExplainPanel connectionId="c1" sql="" explainOutput="x" />,
    );
    fireEvent.click(getByText('settings.ai.goToConfigure'));
    expect(openSettingsWindow).toHaveBeenCalledWith('ai');
  });
});
