import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, fireEvent, screen } from '@testing-library/react';
import { WorkflowPanel } from '../WorkflowPanel';
import type { StepExecutionResult, WorkflowExecutionResult, WorkflowListItem } from '../../../types';

const {
  aiStoreState,
  loadWorkflowsMock,
  executeWorkflowMock,
  clearWorkflowResultMock,
  workflowGetDirMock,
  workflowHistoryListMock,
  workflowHistoryGetMock,
  workflowSaveMock,
  workflowDeleteMock,
  workflowReloadMock,
  workflowGetMock,
  workflowHistoryClearMock,
  getConnectionsMock,
  getDatabasesMock,
  openDocsWindowMock,
} = vi.hoisted(() => {
  const aiStoreState = {
    workflows: [
      { id: 'wf1', name: 'Workflow One', description: 'First', variables: [] },
      {
        id: 'wf2',
        name: 'With Vars',
        description: '',
        variables: [
          { name: 'q', type: 'string', description: 'query', required: true, default: 'x' },
          { name: 'conn', type: 'connection', description: 'db', required: false },
        ],
      },
    ] as WorkflowListItem[],
    workflowsLoading: false,
    loadWorkflows: vi.fn().mockResolvedValue(undefined),
    executeWorkflow: vi.fn().mockResolvedValue(undefined),
    workflowError: null as string | null,
    clearWorkflowResult: vi.fn(),
    workflowExecutionResult: null as WorkflowExecutionResult | null,
    isExecutingWorkflow: false,
  };
  return {
    aiStoreState,
    loadWorkflowsMock: aiStoreState.loadWorkflows,
    executeWorkflowMock: aiStoreState.executeWorkflow,
    clearWorkflowResultMock: aiStoreState.clearWorkflowResult,
    workflowGetDirMock: vi.fn().mockResolvedValue('/tmp/workflows'),
    workflowHistoryListMock: vi.fn().mockResolvedValue([]),
    workflowHistoryGetMock: vi.fn(),
    workflowSaveMock: vi.fn().mockResolvedValue(undefined),
    workflowDeleteMock: vi.fn().mockResolvedValue(undefined),
    workflowReloadMock: vi.fn().mockResolvedValue(undefined),
    workflowGetMock: vi.fn(),
    workflowHistoryClearMock: vi.fn().mockResolvedValue(undefined),
    getConnectionsMock: vi.fn().mockResolvedValue([
      { id: 'c1', name: 'PG', databaseType: 'postgresql', database: 'postgres' },
      { id: 'c2', name: 'My', databaseType: 'mysql' },
    ]),
    getDatabasesMock: vi.fn().mockResolvedValue(['db1', 'db2']),
    openDocsWindowMock: vi.fn(),
  };
});

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../stores/aiStore', () => ({
  useAiStore: (sel: (s: typeof aiStoreState) => unknown) => sel(aiStoreState),
}));

vi.mock('../../../commands/ai', () => ({
  aiCommands: {
    workflowGetDir: (...a: unknown[]) => workflowGetDirMock(...a),
    workflowHistoryList: (...a: unknown[]) => workflowHistoryListMock(...a),
    workflowHistoryGet: (...a: unknown[]) => workflowHistoryGetMock(...a),
    workflowHistoryClear: (...a: unknown[]) => workflowHistoryClearMock(...a),
    workflowSave: (...a: unknown[]) => workflowSaveMock(...a),
    workflowDelete: (...a: unknown[]) => workflowDeleteMock(...a),
    workflowReload: (...a: unknown[]) => workflowReloadMock(...a),
    workflowGet: (...a: unknown[]) => workflowGetMock(...a),
  },
}));

vi.mock('../../../commands/connection', () => ({
  connectionCommands: {
    getConnections: (...a: unknown[]) => getConnectionsMock(...a),
  },
}));

vi.mock('../../../commands/database', () => ({
  databaseCommands: {
    getDatabases: (...a: unknown[]) => getDatabasesMock(...a),
  },
}));

vi.mock('../../../lib/windowManager', () => ({
  openDocsWindow: (...a: unknown[]) => openDocsWindowMock(...a),
}));

vi.mock('../../chart/ChartView', () => ({
  ChartView: ({ onDataPointClick }: { onDataPointClick?: () => void }) => (
    <div data-testid="chart-view">
      <button type="button" onClick={() => onDataPointClick?.()}>chart-click</button>
    </div>
  ),
}));

const confirmMock = vi.fn(() => true);

function makeResult(steps: StepExecutionResult[]): WorkflowExecutionResult {
  return { success: true, finalOutput: 'done', steps, totalTimeMs: 10 };
}

const chartableStep: StepExecutionResult = {
  stepId: 'q1',
  stepType: 'query',
  status: 'success',
  executionTimeMs: 5,
  connectionName: 'PG',
  sqlExecuted: 'SELECT n, v FROM t',
  result: {
    columns: [{ name: 'n', dataType: 'text' }, { name: 'v', dataType: 'int4' }],
    rows: [['a', 1], ['b', 2]],
    rows_count: 2,
    execution_time_ms: 5,
  },
};

beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal('confirm', confirmMock);
  confirmMock.mockReturnValue(true);
  aiStoreState.workflowsLoading = false;
  aiStoreState.workflowError = null;
  aiStoreState.workflowExecutionResult = null;
  aiStoreState.isExecutingWorkflow = false;
  aiStoreState.workflows = [
    { id: 'wf1', name: 'Workflow One', description: 'First', variables: [] },
    {
      id: 'wf2',
      name: 'With Vars',
      description: '',
      variables: [
        { name: 'q', type: 'string', description: 'query', required: true, default: 'x' },
        { name: 'conn', type: 'connection', description: 'db', required: false },
      ],
    },
  ];
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe('WorkflowPanel', () => {
  it('loads workflows and shows storage dir', async () => {
    render(<WorkflowPanel connectionId="c1" />);
    await waitFor(() => expect(loadWorkflowsMock).toHaveBeenCalled());
    expect(screen.getByText('Workflow One')).toBeInTheDocument();
    expect(screen.getByText('/tmp/workflows')).toBeInTheDocument();
  });

  it('shows loading state', () => {
    aiStoreState.workflowsLoading = true;
    render(<WorkflowPanel />);
    expect(screen.getByText('workflows.loading')).toBeInTheDocument();
  });

  it('shows empty list hint', async () => {
    aiStoreState.workflows = [];
    render(<WorkflowPanel />);
    await waitFor(() => expect(screen.getByText('workflows.empty')).toBeInTheDocument());
  });

  it('selects workflow and executes', async () => {
    render(<WorkflowPanel connectionId="c1" />);
    await waitFor(() => screen.getByText('Workflow One'));
    fireEvent.click(screen.getByText('Workflow One'));
    expect(clearWorkflowResultMock).toHaveBeenCalled();
    fireEvent.click(screen.getByText('workflows.run'));
    await waitFor(() => {
      expect(executeWorkflowMock).toHaveBeenCalledWith({
        workflowId: 'wf1',
        variables: {},
        connectionId: 'c1',
      });
    });
  });

  it('creates workflow via form and saves', async () => {
    render(<WorkflowPanel />);
    await waitFor(() => screen.getByText('workflows.create'));
    fireEvent.click(screen.getByTitle('workflows.create'));
    const idInput = screen.getByPlaceholderText('workflows.form.idPlaceholder');
    fireEvent.change(idInput, { target: { value: 'new-wf' } });
    fireEvent.change(screen.getByPlaceholderText('workflows.form.namePlaceholder'), { target: { value: 'New' } });
    fireEvent.click(screen.getByText('common.save'));
    await waitFor(() => expect(workflowSaveMock).toHaveBeenCalled());
    expect(screen.getByText('workflows.saved')).toBeInTheDocument();
  });

  it('adds variable and step types in form', async () => {
    render(<WorkflowPanel />);
    await waitFor(() => screen.getByText('workflows.create'));
    fireEvent.click(screen.getByTitle('workflows.create'));
    fireEvent.click(screen.getByText('workflows.form.addVariable'));
    fireEvent.click(screen.getByText(/workflows.form.addAiStep/));
    fireEvent.click(screen.getByText(/workflows.form.addConditionStep/));
    fireEvent.click(screen.getByText(/workflows.form.addForeachStep/));
    expect(screen.getByPlaceholderText('workflows.form.prompt')).toBeInTheDocument();
    expect(screen.getByText(/workflows.form.conditionHint/)).toBeInTheDocument();
    expect(screen.getByPlaceholderText('item')).toBeInTheDocument();
  });

  it('shows save error feedback', async () => {
    workflowSaveMock.mockRejectedValueOnce(new Error('save failed'));
    render(<WorkflowPanel />);
    await waitFor(() => screen.getByTitle('workflows.create'));
    fireEvent.click(screen.getByTitle('workflows.create'));
    fireEvent.change(screen.getByPlaceholderText('workflows.form.idPlaceholder'), { target: { value: 'wf-x' } });
    fireEvent.change(screen.getByPlaceholderText('workflows.form.namePlaceholder'), { target: { value: 'X' } });
    fireEvent.click(screen.getByText('common.save'));
    await waitFor(() => expect(screen.getByText('Error: save failed')).toBeInTheDocument());
  });

  it('shows running state while executing', async () => {
    aiStoreState.isExecutingWorkflow = true;
    render(<WorkflowPanel connectionId="c1" />);
    await waitFor(() => screen.getByText('Workflow One'));
    fireEvent.click(screen.getByText('Workflow One'));
    expect(screen.getByText('workflows.running')).toBeInTheDocument();
  });

  it('loads database picker for multi-db connection in query step form', async () => {
    render(<WorkflowPanel />);
    await waitFor(() => screen.getByTitle('workflows.create'));
    fireEvent.click(screen.getByTitle('workflows.create'));
    const connSelect = document.querySelector('select')!;
    fireEvent.change(connSelect, { target: { value: 'c2' } });
    await waitFor(() => expect(getDatabasesMock).toHaveBeenCalledWith('c2'));
    await waitFor(() => expect(screen.getByText('db1')).toBeInTheDocument());
  });

  it('edits workflow with nested step types', async () => {
    workflowGetMock.mockResolvedValue({
      id: 'wf1',
      name: 'Workflow One',
      description: 'First',
      variables: [],
      steps: [
        { type: 'condition', id: 'c1', if: 'true', thenSteps: [{ type: 'query', id: 'q1', sql: 'SELECT 1' }], elseSteps: [] },
        { type: 'foreach', id: 'f1', items: 'steps.q1.rows', asVar: 'row', steps: [{ type: 'ai', id: 'a1', prompt: 'hi' }], maxIterations: 50 },
      ],
    });
    render(<WorkflowPanel />);
    await waitFor(() => screen.getByText('Workflow One'));
    fireEvent.click(screen.getAllByTitle('workflows.edit')[0]);
    await waitFor(() => expect(workflowGetMock).toHaveBeenCalledWith('wf1'));
    await waitFor(() => expect(screen.getByText(/workflows.form.conditionHint/)).toBeInTheDocument());
    expect(screen.getByDisplayValue('row')).toBeInTheDocument();
    expect(screen.getByDisplayValue('50')).toBeInTheDocument();
  });

  it('skips delete when confirm cancelled', async () => {
    confirmMock.mockReturnValueOnce(false);
    render(<WorkflowPanel />);
    await waitFor(() => screen.getByText('Workflow One'));
    fireEvent.click(screen.getAllByTitle('workflows.delete')[0]);
    expect(workflowDeleteMock).not.toHaveBeenCalled();
  });

  it('edits and deletes workflow', async () => {
    workflowGetMock.mockResolvedValue({
      id: 'wf1',
      name: 'Workflow One',
      description: 'First',
      variables: [],
      steps: [{ type: 'query', id: 's1', sql: 'SELECT 1' }],
    });
    render(<WorkflowPanel />);
    await waitFor(() => screen.getByText('Workflow One'));
    const editBtn = screen.getAllByTitle('workflows.edit')[0];
    fireEvent.click(editBtn);
    await waitFor(() => expect(workflowGetMock).toHaveBeenCalledWith('wf1'));
    expect(screen.getByText('workflows.edit')).toBeInTheDocument();

    fireEvent.click(screen.getAllByTitle('workflows.delete')[0]);
    await waitFor(() => expect(workflowDeleteMock).toHaveBeenCalledWith('wf1'));
  });

  it('reloads workflows and opens docs', async () => {
    render(<WorkflowPanel />);
    await waitFor(() => screen.getByTitle('workflows.reload'));
    fireEvent.click(screen.getByTitle('workflows.reload'));
    await waitFor(() => expect(workflowReloadMock).toHaveBeenCalled());
    fireEvent.click(screen.getByTitle('docs.openWorkflowHelp'));
    expect(openDocsWindowMock).toHaveBeenCalledWith('workflows');
  });

  it('shows execution result with expandable step and chart via history', async () => {
    workflowHistoryListMock.mockResolvedValue([
      { id: 'h1', workflowName: 'Chart Run', createdAt: Date.now(), totalTimeMs: 99, success: true },
    ]);
    workflowHistoryGetMock.mockResolvedValue({
      result: makeResult([chartableStep]),
    });
    render(<WorkflowPanel />);
    await waitFor(() => screen.getByText('Workflow One'));
    fireEvent.click(screen.getByText('workflows.history.title'));
    await waitFor(() => screen.getByText('Chart Run'));
    fireEvent.click(screen.getByText('Chart Run'));
    await waitFor(() => expect(screen.getByText(/workflows\.result/)).toBeInTheDocument());
    fireEvent.click(screen.getByText('q1'));
    expect(screen.getByText('chart.viewChart')).toBeInTheDocument();
    fireEvent.click(screen.getByText('chart.viewChart'));
    fireEvent.click(screen.getByText('chart-click'));
    expect(screen.getByText('chart.viewTable')).toBeInTheDocument();
  });

  it('shows workflow error and failed execution via history', async () => {
    aiStoreState.workflowError = 'exec failed';
    workflowHistoryListMock.mockResolvedValue([
      { id: 'h1', workflowName: 'Failed Run', createdAt: Date.now(), totalTimeMs: 1, success: false },
    ]);
    workflowHistoryGetMock.mockResolvedValue({
      result: {
        success: false,
        finalOutput: '',
        error: 'boom',
        steps: [{ stepId: 's1', stepType: 'query', status: 'failed', executionTimeMs: 1, error: 'err' }],
        totalTimeMs: 1,
      },
    });
    render(<WorkflowPanel />);
    await waitFor(() => screen.getByText('Workflow One'));
    expect(screen.getByText('exec failed')).toBeInTheDocument();
    fireEvent.click(screen.getByText('workflows.history.title'));
    await waitFor(() => screen.getByText('Failed Run'));
    fireEvent.click(screen.getByText('Failed Run'));
    await waitFor(() => expect(screen.getByText(/workflows\.executionFailed/)).toBeInTheDocument());
    fireEvent.click(screen.getByText('s1'));
    expect(screen.getByText('err')).toBeInTheDocument();
  });

  it('history tab lists items and shows detail', async () => {
    workflowHistoryListMock.mockResolvedValue([
      { id: 'h1', workflowName: 'Past Run', createdAt: Date.now(), totalTimeMs: 99, success: true },
    ]);
    workflowHistoryGetMock.mockResolvedValue({
      result: makeResult([chartableStep]),
    });
    render(<WorkflowPanel />);
    await waitFor(() => screen.getByText('workflows.history.title'));
    fireEvent.click(screen.getByText('workflows.history.title'));
    await waitFor(() => screen.getByText('Past Run'));
    fireEvent.click(screen.getByText('Past Run'));
    await waitFor(() => expect(workflowHistoryGetMock).toHaveBeenCalledWith('h1'));
    expect(screen.getByText('workflows.history.back')).toBeInTheDocument();
    fireEvent.click(screen.getByText('workflows.history.back'));

    fireEvent.click(screen.getByText('workflows.history.clear'));
    await waitFor(() => expect(workflowHistoryClearMock).toHaveBeenCalled());
  });

  it('fills connection variable for workflow with vars', async () => {
    render(<WorkflowPanel connectionId="c1" />);
    await waitFor(() => screen.getByText('With Vars'));
    fireEvent.click(screen.getByText('With Vars'));
    const selects = document.querySelectorAll('select');
    expect(selects.length).toBeGreaterThan(0);
    fireEvent.change(selects[0], { target: { value: 'c1' } });
  });
});
