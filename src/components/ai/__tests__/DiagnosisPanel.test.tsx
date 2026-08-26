import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, waitFor } from '@testing-library/react';
import { DiagnosisPanel } from '../DiagnosisPanel';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

const openSettingsWindow = vi.fn();
vi.mock('../../../lib/windowManager', () => ({
  openSettingsWindow: (...args: unknown[]) => openSettingsWindow(...args),
}));

const aiState = vi.hoisted(() => ({
  diagnosis: null as {
    explanation: string;
    changes: string[];
    suggestedSql?: string;
  } | null,
  isDiagnosing: false,
  diagnosisError: null as string | null,
  isConfigured: true,
  diagnoseError: vi.fn().mockResolvedValue(undefined),
  clearDiagnosis: vi.fn(),
}));

vi.mock('../../../stores/aiStore', () => ({
  useAiStore: (sel: (s: typeof aiState) => unknown) => sel(aiState),
}));

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  aiState.diagnosis = null;
  aiState.isDiagnosing = false;
  aiState.diagnosisError = null;
  aiState.isConfigured = true;
});

describe('DiagnosisPanel', () => {
  it('auto-diagnoses on mount when configured', async () => {
    render(
      <DiagnosisPanel
        dbSessionId="c1"
        database="db"
        sql="SELECT bad"
        errorMessage="syntax error"
        onApplySql={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    await waitFor(() => {
      expect(aiState.diagnoseError).toHaveBeenCalledWith({
        dbSessionId: 'c1',
        database: 'db',
        sql: 'SELECT bad',
        errorMessage: 'syntax error',
      });
    });
  });

  it('shows not configured state with close', () => {
    aiState.isConfigured = false;
    const onClose = vi.fn();
    const { container } = render(
      <DiagnosisPanel
        dbSessionId="c1"
        database="db"
        sql=""
        errorMessage=""
        onApplySql={vi.fn()}
        onClose={onClose}
      />,
    );
    expect(container.textContent).toContain('common.aiNotConfigured');
    const closeBtn = [...container.querySelectorAll('button')].at(-1)!;
    fireEvent.click(closeBtn);
    expect(aiState.clearDiagnosis).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it('renders diagnosis result and applies suggested SQL', () => {
    const onApplySql = vi.fn();
    const onClose = vi.fn();
    aiState.diagnosis = {
      explanation: 'Missing column',
      changes: ['Use correct column name'],
      suggestedSql: 'SELECT id FROM users',
    };
    const { getByText } = render(
      <DiagnosisPanel
        dbSessionId="c1"
        database="db"
        sql="SELECT x"
        errorMessage="column x does not exist"
        onApplySql={onApplySql}
        onClose={onClose}
      />,
    );
    expect(getByText('Missing column')).toBeInTheDocument();
    expect(getByText('Use correct column name')).toBeInTheDocument();
    fireEvent.click(getByText('diagnosis.applySuggested'));
    expect(onApplySql).toHaveBeenCalledWith('SELECT id FROM users');
    expect(onClose).toHaveBeenCalled();
  });

  it('shows diagnosing and error states with manual retry', () => {
    aiState.isDiagnosing = true;
    const { getByText, rerender } = render(
      <DiagnosisPanel
        dbSessionId="c1"
        database="db"
        sql=""
        errorMessage=""
        onApplySql={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expect(getByText('diagnosis.diagnosing')).toBeInTheDocument();

    aiState.isDiagnosing = false;
    aiState.diagnosisError = 'timeout';
    rerender(
      <DiagnosisPanel
        dbSessionId="c1"
        database="db"
        sql=""
        errorMessage=""
        onApplySql={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expect(getByText('timeout')).toBeInTheDocument();
    fireEvent.click(getByText('diagnosis.diagnose'));
    expect(aiState.diagnoseError).toHaveBeenCalled();
  });
});
