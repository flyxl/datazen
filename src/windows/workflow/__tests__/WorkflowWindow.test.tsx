import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, fireEvent, screen, within } from '@testing-library/react';
import { WorkflowWindow } from '../WorkflowWindow';
import type {
  StepExecutionResult,
  WorkflowExecutionResult,
  WorkflowListItem,
} from '../../../types';

const {
  loadWorkflowsMock,
  loadConfigMock,
  loadSettingsMock,
  executeWorkflowMock,
  clearWorkflowResultMock,
  setupAiListenersMock,
  workflowGetDirMock,
  workflowHistoryListMock,
  workflowHistoryGetMock,
  workflowSaveMock,
  workflowDeleteMock,
  workflowReloadMock,
  workflowGetMock,
  workflowHistoryClearMock,
  getConnectionsMock,
  openWorkflowsDirMock,
  openDocsWindowMock,
  aiStoreState,
} = vi.hoisted(() => {
  const aiStoreState = {
    workflows: [
      {
        id: 'wf-demo',
        name: 'Demo Workflow',
        description: 'A demo',
        variables: [],
      },
      {
        id: 'wf-vars',
        name: 'Vars Workflow',
        description: '',
        variables: [
          {
            name: 'q',
            type: 'string',
            description: 'query text',
            required: true,
            default: 'hello',
          },
          { name: 'db', type: 'connection', description: 'conn', required: false },
        ],
      },
    ] as WorkflowListItem[],
    workflowsLoading: false,
    loadWorkflows: vi.fn().mockResolvedValue(undefined),
    loadConfig: vi.fn().mockResolvedValue(undefined),
    executeWorkflow: vi.fn().mockResolvedValue(undefined),
    workflowError: null as string | null,
    clearWorkflowResult: vi.fn(),
    setupEventListeners: vi.fn().mockResolvedValue(() => {}),
    workflowExecutionResult: null as WorkflowExecutionResult | null,
  };
  return {
    loadWorkflowsMock: aiStoreState.loadWorkflows,
    loadConfigMock: aiStoreState.loadConfig,
    loadSettingsMock: vi.fn().mockResolvedValue(undefined),
    executeWorkflowMock: aiStoreState.executeWorkflow,
    clearWorkflowResultMock: aiStoreState.clearWorkflowResult,
    setupAiListenersMock: aiStoreState.setupEventListeners,
    workflowGetDirMock: vi.fn().mockResolvedValue('/tmp/workflows'),
    workflowHistoryListMock: vi.fn().mockResolvedValue([]),
    workflowHistoryGetMock: vi.fn(),
    workflowSaveMock: vi.fn().mockResolvedValue(undefined),
    workflowDeleteMock: vi.fn().mockResolvedValue(undefined),
    workflowReloadMock: vi.fn().mockResolvedValue(undefined),
    workflowGetMock: vi.fn(),
    workflowHistoryClearMock: vi.fn().mockResolvedValue(undefined),
    getConnectionsMock: vi
      .fn()
      .mockResolvedValue([
        { id: 'c1', name: 'PG', databaseType: 'postgresql', database: 'postgres' },
      ]),
    openWorkflowsDirMock: vi.fn().mockResolvedValue(undefined),
    openDocsWindowMock: vi.fn(),
    aiStoreState,
  };
});

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useSettings', () => ({
  useSettings: () => {},
}));

vi.mock('../../../hooks/useResizable', () => ({
  useResizable: () => ({ size: 256, handleRef: { current: null } }),
}));

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (sel: (s: { loadSettings: () => Promise<void> }) => unknown) =>
    sel({ loadSettings: loadSettingsMock }),
}));

vi.mock('../../../stores/aiStore', () => {
  const useAiStore = Object.assign(
    (sel: (s: typeof aiStoreState) => unknown) => sel(aiStoreState),
    { getState: () => aiStoreState },
  );
  return { useAiStore };
});

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

vi.mock('../../../commands/settings', () => ({
  settingsCommands: {
    openWorkflowsDir: (...a: unknown[]) => openWorkflowsDirMock(...a),
  },
}));

vi.mock('../../../lib/windowManager', () => ({
  openDocsWindow: (...a: unknown[]) => openDocsWindowMock(...a),
}));

vi.mock('../../../components/ui/Select', () => ({
  Select: ({
    value,
    options,
    onChange,
    className,
  }: {
    value: string;
    options: { value: string; label: string }[];
    onChange: (v: string) => void;
    className?: string;
  }) => (
    <select
      data-testid="var-select"
      className={className}
      value={value}
      onChange={(e) => onChange(e.target.value)}
    >
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  ),
}));

vi.mock('../../../components/TitleBar', () => ({
  TitleBar: ({ title, leftContent }: { title: string; leftContent?: React.ReactNode }) => (
    <div data-testid="title-bar">
      <span>{title}</span>
      {leftContent}
    </div>
  ),
}));

vi.mock('../../../components/StatusBar', () => ({
  StatusBar: () => <div data-testid="status-bar" />,
}));

vi.mock('../../../components/DataTable/DataTable', () => ({
  DataTable: () => <div data-testid="data-table" />,
}));

vi.mock('../../../components/chart/ChartView', () => ({
  ChartView: ({
    onDataPointClick,
    onConfigChange,
  }: {
    onDataPointClick?: () => void;
    onConfigChange?: (cfg: unknown) => void;
  }) => (
    <div data-testid="chart-view">
      <button type="button" onClick={() => onDataPointClick?.()}>
        chart-point
      </button>
      <button type="button" onClick={() => onConfigChange?.({ type: 'bar' })}>
        chart-config
      </button>
    </div>
  ),
}));

vi.mock('../../../components/ai/WorkflowChatPanel', () => ({
  WorkflowChatPanel: ({ onSaved, onBack }: { onSaved: () => void; onBack: () => void }) => (
    <div data-testid="wf-chat">
      <button type="button" onClick={onSaved}>
        chat-saved
      </button>
      <button type="button" onClick={onBack}>
        chat-back
      </button>
    </div>
  ),
}));

vi.mock('../WorkflowForm', async () => {
  const actual = await vi.importActual<typeof import('../WorkflowForm')>('../WorkflowForm');
  return {
    ...actual,
    WorkflowForm: ({ onSave, onCancel }: { onSave: () => void; onCancel: () => void }) => (
      <div data-testid="workflow-form">
        <button type="button" onClick={onSave}>
          form-save
        </button>
        <button type="button" onClick={onCancel}>
          form-cancel
        </button>
      </div>
    ),
  };
});

const confirmMock = vi.fn(() => true);
const askMock = vi.fn().mockResolvedValue(true);

vi.mock('@tauri-apps/plugin-dialog', () => ({
  ask: (...args: unknown[]) => askMock(...args),
}));

function makeResult(steps: StepExecutionResult[], success = true): WorkflowExecutionResult {
  return { success, finalOutput: '', steps, totalTimeMs: 42 };
}

const chartableStep: StepExecutionResult = {
  stepId: 'q1',
  stepType: 'query',
  status: 'success',
  executionTimeMs: 12,
  connectionName: 'PG',
  sqlExecuted: 'SELECT name, val FROM t',
  result: {
    columns: [
      { name: 'name', dataType: 'text' },
      { name: 'val', dataType: 'int4' },
    ],
    rows: [
      ['a', 1],
      ['b', 2],
    ],
    rows_count: 2,
    execution_time_ms: 12,
  },
};

async function renderAndLoad() {
  render(<WorkflowWindow />);
  await waitFor(() => expect(loadWorkflowsMock).toHaveBeenCalled());
}

async function selectWorkflow(name: string) {
  fireEvent.click(screen.getByText(name));
  await waitFor(() => expect(clearWorkflowResultMock).toHaveBeenCalled());
}

async function executeWithResult(result: WorkflowExecutionResult | null) {
  executeWorkflowMock.mockImplementation(async () => {
    aiStoreState.workflowExecutionResult = result;
  });
  fireEvent.click(screen.getByText('workflows.execute'));
  await waitFor(() => expect(executeWorkflowMock).toHaveBeenCalled());
}

function clickStepTab(stepId: string) {
  const btn = screen
    .getAllByRole('button')
    .find((b) => b.textContent?.includes(stepId) && b.textContent?.includes('['));
  expect(btn).toBeTruthy();
  fireEvent.click(btn!);
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal('confirm', confirmMock);
  confirmMock.mockReturnValue(true);
  askMock.mockResolvedValue(true);
  aiStoreState.workflowsLoading = false;
  aiStoreState.workflowError = null;
  aiStoreState.workflowExecutionResult = null;
  aiStoreState.workflows = [
    {
      id: 'wf-demo',
      name: 'Demo Workflow',
      description: 'A demo',
      variables: [],
    },
    {
      id: 'wf-vars',
      name: 'Vars Workflow',
      description: '',
      variables: [
        { name: 'q', type: 'string', description: 'query text', required: true, default: 'hello' },
        { name: 'db', type: 'connection', description: 'conn', required: false },
      ],
    },
  ];
  loadWorkflowsMock.mockResolvedValue(undefined);
  workflowGetDirMock.mockResolvedValue('/tmp/workflows');
  getConnectionsMock.mockResolvedValue([
    { id: 'c1', name: 'PG', databaseType: 'postgresql', database: 'postgres' },
  ]);
  workflowHistoryListMock.mockResolvedValue([]);
  executeWorkflowMock.mockResolvedValue(undefined);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe('WorkflowWindow', () => {
  it('loads workflows and shows empty hint', async () => {
    await renderAndLoad();
    expect(screen.getByText('Demo Workflow')).toBeInTheDocument();
    expect(screen.getByText('workflows.emptyHint')).toBeInTheDocument();
    expect(screen.getByTitle('workflows.create')).toBeInTheDocument();
    expect(screen.getByTitle('workflows.openDir')).toBeInTheDocument();
  });

  it('shows loading and empty workflow list states', async () => {
    aiStoreState.workflowsLoading = true;
    aiStoreState.workflows = [];
    await renderAndLoad();
    expect(screen.getByText('workflows.loading')).toBeInTheDocument();

    cleanup();
    aiStoreState.workflowsLoading = false;
    await renderAndLoad();
    expect(screen.getByText('workflows.noWorkflows')).toBeInTheDocument();
  });

  it('switches to history tab, shows empty, views entry, and clears history', async () => {
    workflowHistoryListMock.mockResolvedValue([
      {
        id: 'h1',
        workflowId: 'wf-demo',
        workflowName: 'Demo Workflow',
        success: true,
        totalTimeMs: 10,
        createdAt: '2026-01-01T12:00:00.000Z',
      },
    ]);
    workflowHistoryGetMock.mockResolvedValue({
      id: 'h1',
      workflowId: 'wf-demo',
      workflowName: 'Demo Workflow',
      variables: {},
      createdAt: '2026-01-01T12:00:00.000Z',
      result: makeResult([chartableStep]),
    });

    await renderAndLoad();
    fireEvent.click(screen.getByText('workflows.history.title'));
    await waitFor(() => expect(workflowHistoryListMock).toHaveBeenCalled());

    fireEvent.click(screen.getByText('Demo Workflow'));
    await waitFor(() => expect(screen.getByTestId('data-table')).toBeInTheDocument());

    fireEvent.click(screen.getByText('workflows.history.clear'));
    expect(confirmMock).toHaveBeenCalledWith('workflows.history.clearConfirm');
    await waitFor(() => expect(workflowHistoryClearMock).toHaveBeenCalled());

    cleanup();
    workflowHistoryListMock.mockResolvedValue([]);
    await renderAndLoad();
    fireEvent.click(screen.getByText('workflows.history.title'));
    await waitFor(() => expect(screen.getByText('workflows.history.empty')).toBeInTheDocument());
  });

  it('shows feedback when history load fails', async () => {
    workflowHistoryListMock.mockResolvedValue([
      {
        id: 'h-bad',
        workflowId: 'wf-demo',
        workflowName: 'Bad History',
        success: false,
        totalTimeMs: 1,
        createdAt: '2026-01-01T12:00:00.000Z',
      },
    ]);
    workflowHistoryGetMock.mockRejectedValue(new Error('history fetch failed'));

    await renderAndLoad();
    fireEvent.click(screen.getByText('workflows.history.title'));
    await waitFor(() => expect(workflowHistoryListMock).toHaveBeenCalled());
    fireEvent.click(screen.getByText('Bad History'));
    await waitFor(() =>
      expect(screen.getByText('Error: history fetch failed')).toBeInTheDocument(),
    );
  });

  it('reuses existing history panel when clicking same entry twice', async () => {
    workflowHistoryListMock.mockResolvedValue([
      {
        id: 'h1',
        workflowId: 'wf-demo',
        workflowName: 'Demo Workflow',
        success: true,
        totalTimeMs: 10,
        createdAt: '2026-01-01T12:00:00.000Z',
      },
    ]);
    workflowHistoryGetMock.mockResolvedValue({
      id: 'h1',
      workflowId: 'wf-demo',
      workflowName: 'Demo Workflow',
      variables: {},
      createdAt: '2026-01-01T12:00:00.000Z',
      result: makeResult([chartableStep]),
    });

    await renderAndLoad();
    fireEvent.click(screen.getByText('workflows.history.title'));
    await waitFor(() => expect(workflowHistoryListMock).toHaveBeenCalled());
    fireEvent.click(screen.getByText('Demo Workflow'));
    await waitFor(() => expect(workflowHistoryGetMock).toHaveBeenCalledTimes(1));
    fireEvent.click(screen.getByText('Demo Workflow'));
    expect(workflowHistoryGetMock).toHaveBeenCalledTimes(1);
  });

  it('opens create, edit, and ai-create panels with tab reuse', async () => {
    await renderAndLoad();

    fireEvent.click(screen.getByTitle('workflows.create'));
    await waitFor(() => expect(screen.getByTestId('workflow-form')).toBeInTheDocument());

    fireEvent.click(screen.getByTitle('workflows.create'));
    expect(screen.getAllByTestId('workflow-form')).toHaveLength(1);

    fireEvent.click(screen.getByTitle('workflows.aiCreate.title'));
    await waitFor(() => expect(screen.getByTestId('wf-chat')).toBeInTheDocument());

    fireEvent.click(screen.getByTitle('workflows.aiCreate.title'));
    expect(screen.getAllByTestId('wf-chat')).toHaveLength(1);
  });

  it('reloads workflows, opens dir, and opens docs', async () => {
    await renderAndLoad();
    fireEvent.click(screen.getByTitle('workflows.reload'));
    await waitFor(() => expect(workflowReloadMock).toHaveBeenCalled());

    fireEvent.click(screen.getByTitle('workflows.openDir'));
    await waitFor(() => expect(openWorkflowsDirMock).toHaveBeenCalled());

    fireEvent.click(screen.getByTitle('docs.openWorkflowHelp'));
    expect(openDocsWindowMock).toHaveBeenCalledWith('workflows');
  });

  it('selects workflow, fills variables, executes, and shows step table/chart', async () => {
    await renderAndLoad();
    await selectWorkflow('Vars Workflow');

    expect(screen.getByDisplayValue('hello')).toBeInTheDocument();
    fireEvent.change(screen.getByDisplayValue('hello'), { target: { value: 'world' } });
    fireEvent.change(screen.getByTestId('var-select'), { target: { value: 'c1' } });

    await executeWithResult(makeResult([chartableStep]));
    expect(screen.getByTestId('data-table')).toBeInTheDocument();

    fireEvent.click(screen.getByText('chart.viewChart'));
    await waitFor(() => expect(screen.getByTestId('chart-view')).toBeInTheDocument());

    fireEvent.click(screen.getByText('chart-point'));
    await waitFor(() => expect(screen.getByTestId('data-table')).toBeInTheDocument());

    fireEvent.click(screen.getByText('chart.viewChart'));
    fireEvent.click(screen.getByText('chart-config'));
    fireEvent.click(screen.getByText('chart.viewTable'));
  });

  it('reuses run panel and closes panels via tab X', async () => {
    await renderAndLoad();
    await selectWorkflow('Demo Workflow');
    await selectWorkflow('Demo Workflow');

    const tabClose = Array.from(document.querySelectorAll('button')).find((b) =>
      b.querySelector('svg')?.classList.contains('lucide-x'),
    );
    expect(tabClose).toBeTruthy();
    fireEvent.click(tabClose!);
    await waitFor(() => expect(screen.queryByText('workflows.execute')).not.toBeInTheDocument());
  });

  it('handles execute failure and workflow error display', async () => {
    await renderAndLoad();
    await selectWorkflow('Demo Workflow');

    executeWorkflowMock.mockRejectedValue(new Error('exec failed'));
    fireEvent.click(screen.getByText('workflows.execute'));
    await waitFor(() => expect(executeWorkflowMock).toHaveBeenCalled());

    cleanup();
    aiStoreState.workflowError = 'Workflow boom';
    await renderAndLoad();
    await selectWorkflow('Demo Workflow');
    expect(screen.getByText('Workflow boom')).toBeInTheDocument();
  });

  it('shows select-step placeholder when result has no steps', async () => {
    await renderAndLoad();
    await selectWorkflow('Demo Workflow');
    await executeWithResult(makeResult([]));
    expect(screen.getByText('workflows.selectStep')).toBeInTheDocument();
  });

  it('renders step variants: object rows, ai output, error, empty query, statuses', async () => {
    const manyRows = Array.from({ length: 1001 }, (_, i) => ['x', i]);
    const steps: StepExecutionResult[] = [
      {
        stepId: 'obj',
        stepType: 'query',
        status: 'success',
        executionTimeMs: 5,
        result: { rows: [{ name: 'a', val: 1 }] },
      },
      {
        stepId: 'ai-step',
        stepType: 'ai',
        status: 'success',
        executionTimeMs: 3,
        result: { result: 'generated text' },
      },
      {
        stepId: 'err-step',
        stepType: 'query',
        status: 'failed',
        executionTimeMs: 1,
        error: 'bad sql',
        sqlExecuted: 'SELECT bad',
      },
      {
        stepId: 'empty-q',
        stepType: 'query',
        status: 'success',
        executionTimeMs: 2,
        sqlExecuted: 'SELECT 1 WHERE false',
      },
      { stepId: 'skip', stepType: 'query', status: 'skipped', executionTimeMs: 0 },
      { stepId: 'timeout', stepType: 'query', status: 'timed_out', executionTimeMs: 0 },
      {
        stepId: 'big',
        stepType: 'query',
        status: 'success',
        executionTimeMs: 99,
        result: {
          columns: [
            { name: 'name', dataType: 'text' },
            { name: 'val', dataType: 'int4' },
          ],
          rows: manyRows,
          rows_count: 1001,
        },
      },
    ];

    await renderAndLoad();
    await selectWorkflow('Demo Workflow');
    await executeWithResult(makeResult(steps, false));

    for (const id of ['obj', 'ai-step', 'err-step', 'empty-q', 'skip', 'timeout', 'big']) {
      clickStepTab(id);
    }

    clickStepTab('big');
    fireEvent.click(screen.getByText('chart.viewChart'));
    await waitFor(() => expect(screen.getByText('chart.sampledWarning')).toBeInTheDocument());

    clickStepTab('empty-q');
    expect(screen.getByText('workflows.noQueryResult')).toBeInTheDocument();

    clickStepTab('ai-step');
    expect(screen.getByText('generated text')).toBeInTheDocument();

    clickStepTab('err-step');
    expect(screen.getByText('bad sql')).toBeInTheDocument();
  });

  it('edits and deletes workflows from sidebar actions', async () => {
    workflowGetMock.mockResolvedValue({
      id: 'wf-demo',
      name: 'Demo Workflow',
      description: 'd',
      variables: [{ name: 'v1', type: 'string', description: '', required: true }],
      steps: [{ type: 'query', id: 's1', sql: 'SELECT 1', connection: 'c1' }],
    });

    await renderAndLoad();
    const row = screen.getByText('Demo Workflow').closest('div.group')!;
    const buttons = within(row as HTMLElement).getAllByRole('button');
    fireEvent.click(buttons[0]);
    await waitFor(() => expect(workflowGetMock).toHaveBeenCalledWith('wf-demo'));
    expect(screen.getByTestId('workflow-form')).toBeInTheDocument();

    fireEvent.click(buttons[1]);
    await waitFor(() =>
      expect(askMock).toHaveBeenCalledWith('workflows.deleteConfirm', expect.anything()),
    );
    await waitFor(() => expect(workflowDeleteMock).toHaveBeenCalledWith('wf-demo'));
  });

  it('shows feedback on edit failure and saves workflow from edit panel', async () => {
    workflowGetMock.mockRejectedValue(new Error('load failed'));
    await renderAndLoad();
    const row = screen.getByText('Demo Workflow').closest('div.group')!;
    fireEvent.click(within(row as HTMLElement).getAllByRole('button')[0]);
    await waitFor(() => expect(screen.getByText('Error: load failed')).toBeInTheDocument());

    cleanup();
    workflowGetMock.mockResolvedValue({
      id: 'wf-demo',
      name: 'Demo Workflow',
      description: '',
      variables: [],
      steps: [{ type: 'ai', id: 's1', prompt: 'hi' }],
    });
    workflowSaveMock.mockResolvedValue(undefined);
    await renderAndLoad();
    fireEvent.click(
      within(screen.getByText('Demo Workflow').closest('div.group')!).getAllByRole('button')[0],
    );
    await waitFor(() => expect(screen.getByTestId('workflow-form')).toBeInTheDocument());
    fireEvent.click(screen.getByText('form-save'));
    await waitFor(() => expect(workflowSaveMock).toHaveBeenCalled());
    await waitFor(() => expect(screen.getByText('workflows.saved')).toBeInTheDocument());
  });

  it('shows save error feedback and cancels edit panel', async () => {
    workflowGetMock.mockResolvedValue({
      id: 'wf-demo',
      name: 'Demo',
      description: '',
      variables: [],
      steps: [{ type: 'query', id: 's1', sql: '' }],
    });
    workflowSaveMock.mockRejectedValue(new Error('save failed'));

    await renderAndLoad();
    fireEvent.click(
      within(screen.getByText('Demo Workflow').closest('div.group')!).getAllByRole('button')[0],
    );
    await waitFor(() => expect(screen.getByTestId('workflow-form')).toBeInTheDocument());
    fireEvent.click(screen.getByText('form-save'));
    await waitFor(() => expect(screen.getByText('Error: save failed')).toBeInTheDocument());

    fireEvent.click(screen.getByText('form-cancel'));
    await waitFor(() => expect(screen.queryByTestId('workflow-form')).not.toBeInTheDocument());
  });

  it('ai-create panel callbacks reload workflows and close panel', async () => {
    await renderAndLoad();
    fireEvent.click(screen.getByTitle('workflows.aiCreate.title'));
    await waitFor(() => expect(screen.getByTestId('wf-chat')).toBeInTheDocument());
    fireEvent.click(screen.getByText('chat-saved'));
    await waitFor(() => expect(loadWorkflowsMock).toHaveBeenCalledTimes(2));
    fireEvent.click(screen.getByTitle('workflows.aiCreate.title'));
    fireEvent.click(screen.getByText('chat-back'));
    await waitFor(() => expect(screen.queryByTestId('wf-chat')).not.toBeInTheDocument());
  });

  it('skips delete and clear when confirm is cancelled', async () => {
    confirmMock.mockReturnValue(false);
    askMock.mockResolvedValue(false);
    workflowHistoryListMock.mockResolvedValue([
      {
        id: 'h1',
        workflowId: 'wf-demo',
        workflowName: 'Demo Workflow',
        success: true,
        totalTimeMs: 1,
        createdAt: '2026-01-01T12:00:00.000Z',
      },
    ]);

    await renderAndLoad();
    const row = screen.getByText('Demo Workflow').closest('div.group')!;
    fireEvent.click(within(row as HTMLElement).getAllByRole('button')[1]);
    expect(workflowDeleteMock).not.toHaveBeenCalled();

    fireEvent.click(screen.getByText('workflows.history.title'));
    await waitFor(() => expect(workflowHistoryListMock).toHaveBeenCalled());
    fireEvent.click(screen.getByText('workflows.history.clear'));
    expect(workflowHistoryClearMock).not.toHaveBeenCalled();
  });

  it('switches between panel tabs', async () => {
    await renderAndLoad();
    fireEvent.click(screen.getByTitle('workflows.create'));
    await waitFor(() => expect(screen.getByTestId('workflow-form')).toBeInTheDocument());

    await selectWorkflow('Demo Workflow');
    expect(screen.getByText('workflows.execute')).toBeInTheDocument();

    const createTab = screen
      .getAllByRole('button')
      .find((b) => b.textContent?.includes('workflows.create'));
    expect(createTab).toBeTruthy();
    fireEvent.click(createTab!);
    await waitFor(() => expect(screen.getByTestId('workflow-form')).toBeInTheDocument());
  });

  it('shows execution spinner while running', async () => {
    await renderAndLoad();
    await selectWorkflow('Demo Workflow');

    let resolveExec: () => void = () => {};
    executeWorkflowMock.mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          resolveExec = resolve;
        }),
    );
    fireEvent.click(screen.getByText('workflows.execute'));
    await waitFor(() => expect(document.querySelector('.animate-spin')).toBeTruthy());
    resolveExec();
  });
});
