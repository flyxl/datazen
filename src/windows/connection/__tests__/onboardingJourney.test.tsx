/**
 * Onboarding Continuous Lifecycle Journey Tests.
 *
 * Validates the full first-run wizard state machine per
 * `docs/development/interaction-and-testing-principles.md` (Principle 3):
 * 1. Complete lifecycle: welcome -> sample init -> query -> insights -> completion
 * 2. Premature skip at step 2 leaves store cleanly skipped
 * 3. DOM-level guide bar interactions (quick run -> complete -> unmount)
 * 4. End-to-end: WelcomePage click -> guide bar DOM journey -> completion
 * 5. Persistence recovery: completed/skipped status survives module reload
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  render,
  screen,
  fireEvent,
  cleanup,
  waitFor,
  renderHook,
  act,
} from '@testing-library/react';
import { useOnboardingStore } from '../../../stores/onboardingStore';
import { usePanelStore } from '../../../stores/panelStore';
import { OnboardingGuideBar } from '../OnboardingGuideBar';
import { WelcomePage } from '../../welcome/WelcomePage';
import { QueryEditorSection } from '../query/QueryEditorSection';
import { ResultWorkspace } from '../result-workspace/ResultWorkspace';
import { useQueryExecutionGate } from '../query/useQueryExecutionGate';
import { sampleDataCommands } from '../../../commands/sampleData';
import { PENDING_CONNECTION_KEY } from '../../../lib/windowManager';
import { EMPTY_QUERY_EXEC } from '../../../stores/queryExecActions';
import { toQueryExecutionViewModel } from '../../../lib/queryExecutionViewModel';
import type { StatementResult } from '../../../types';

const STORAGE_KEY = 'datazen:onboarding-state-v1';

const fetchConnectionsMock = vi.fn();

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../commands/sampleData', () => ({
  sampleDataCommands: {
    initSampleDatabase: vi.fn(),
  },
}));

vi.mock('../../../stores/connectionStore', () => ({
  useConnectionStore: {
    getState: () => ({
      fetchConnections: fetchConnectionsMock,
      connections: [{ id: 'conn-1', name: 'Test', databaseType: 'sqlite' }],
    }),
  },
}));

vi.mock('../../../lib/windowManager', () => ({
  PENDING_CONNECTION_KEY: 'datazen:pending-connection',
  openNewConnectionDialog: vi.fn(),
}));

vi.mock('../../../lib/connectionShare', () => ({
  openConnectionShareDialog: vi.fn(),
}));

vi.mock('../../../components/TitleBar', () => ({
  TitleBar: ({ title }: { title: string }) => <div data-testid="title-bar">{title}</div>,
}));

vi.mock('../../../components/MenuBar', () => ({
  MenuBar: () => <div data-testid="menu-bar">menu</div>,
}));

vi.mock('../../../components/ThemeToggle', () => ({
  ThemeToggle: () => <div data-testid="theme-toggle">theme</div>,
}));

vi.mock('../../../hooks/usePlatform', () => ({
  usePlatform: () => 'macos',
}));

vi.mock('../../../hooks/useConfirmDialog', () => ({
  useConfirmDialog: () => [vi.fn().mockResolvedValue(true), null],
}));

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (sel: (s: Record<string, unknown>) => unknown) =>
    sel({
      settings: {
        autoCommit: true,
        safeMode: false,
        editorCompletionQuotePolicy: 'unquoted',
        pluginSettings: {},
      },
    }),
}));

vi.mock('../../../components/SqlEditor', async () => {
  const { forwardRef, useImperativeHandle } = await import('react');
  return {
    SqlEditor: forwardRef((_props: unknown, ref) => {
      useImperativeHandle(ref, () => ({
        getSelection: () => '',
        insertAt: vi.fn(),
        toggleLineComment: vi.fn(),
      }));
      return <div data-testid="mock-sql-editor" />;
    }),
  };
});

vi.mock('../../../components/query/QueryContextSelectors', () => ({
  QueryContextSelectors: () => null,
}));

vi.mock('../../../components/query/QueryExecutionStatus', () => ({
  QueryExecutionStatus: () => null,
}));

vi.mock('../../../components/ai/Nl2SqlPanel', () => ({
  Nl2SqlPanel: () => <div data-testid="nl2sql-panel" />,
}));

vi.mock('../../../lib/nativeContextMenu', () => ({
  showNativeContextMenu: vi.fn(),
}));

vi.mock('../../../lib/schemaCache', () => ({
  invalidateSchemaCache: vi.fn(),
}));

vi.mock('../../../components/sql-editor/metadata/metadataCache', () => ({
  metadataCache: { invalidateSession: vi.fn() },
}));

vi.mock('../../../stores/schemaStore', () => ({
  useSchemaStore: { getState: () => ({ loadTables: vi.fn().mockResolvedValue(undefined) }) },
}));

vi.mock('../../../commands/query', () => ({
  queryCommands: {
    beginSessionTransaction: vi.fn().mockResolvedValue(undefined),
  },
}));

vi.mock('@datazen/extension-points', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@datazen/extension-points')>();
  return {
    ...actual,
    useExtension: () => ({}),
  };
});

vi.mock('../result-workspace/ResultTableView', () => ({
  ResultTableView: () => <div role="grid">table</div>,
}));

vi.mock('../../../components/chart/ChartView', () => ({
  ChartView: () => <div role="img" aria-label="mock chart" />,
}));

describe('Onboarding Continuous Journey Test', () => {
  beforeEach(() => {
    localStorage.clear();
    useOnboardingStore.getState().resetOnboarding();
    vi.clearAllMocks();
    fetchConnectionsMock.mockResolvedValue(undefined);
  });

  afterEach(() => {
    cleanup();
  });

  it('runs complete lifecycle: welcome -> sample init -> step 2 query -> step 3 insights -> completion', () => {
    // 1. Initial state
    expect(useOnboardingStore.getState().status).toBe('not_started');
    expect(useOnboardingStore.getState().step).toBe(1);

    // 2. User clicks "Open Sample SQLite"
    useOnboardingStore.getState().startOnboarding('sample_sqlite');
    expect(useOnboardingStore.getState().status).toBe('active');
    expect(useOnboardingStore.getState().step).toBe(2);

    // 3. User executes the preset query -> success with rows
    useOnboardingStore.getState().markQueryExecuted();
    expect(useOnboardingStore.getState().queryExecuted).toBe(true);
    expect(useOnboardingStore.getState().step).toBe(3);

    // 4. User explores chart or AI action
    useOnboardingStore.getState().markAiOrChartExplored();
    expect(useOnboardingStore.getState().aiOrChartExplored).toBe(true);

    // 5. Completion
    useOnboardingStore.getState().completeOnboarding();
    expect(useOnboardingStore.getState().status).toBe('completed');
  });

  it('allows premature skip at step 2 and leaves store cleanly skipped', () => {
    useOnboardingStore.getState().startOnboarding('sample_sqlite');
    expect(useOnboardingStore.getState().status).toBe('active');

    useOnboardingStore.getState().skipOnboarding();
    expect(useOnboardingStore.getState().status).toBe('skipped');

    // Subsequent query execution does not reactivate guide bar
    useOnboardingStore.getState().markQueryExecuted();
    expect(useOnboardingStore.getState().status).toBe('skipped');
  });

  describe('DOM-level guide bar journey', () => {
    it('advances from step 2 quick run to step 3 complete and unmounts on completion', () => {
      useOnboardingStore.getState().startOnboarding('sample_sqlite');
      const onRun = vi.fn();

      const { rerender, container } = render(<OnboardingGuideBar onExecuteSampleQuery={onRun} />);

      expect(screen.getByTestId('onboarding-guide-bar')).toBeInTheDocument();
      expect(screen.getByTestId('onboarding-quick-run-btn')).toBeInTheDocument();
      expect(screen.queryByTestId('onboarding-complete-btn')).not.toBeInTheDocument();

      fireEvent.click(screen.getByTestId('onboarding-quick-run-btn'));
      expect(onRun).toHaveBeenCalledOnce();
      expect(useOnboardingStore.getState().step).toBe(3);
      expect(useOnboardingStore.getState().queryExecuted).toBe(true);

      rerender(<OnboardingGuideBar onExecuteSampleQuery={onRun} />);
      expect(screen.queryByTestId('onboarding-quick-run-btn')).not.toBeInTheDocument();
      expect(screen.getByTestId('onboarding-complete-btn')).toBeInTheDocument();

      fireEvent.click(screen.getByTestId('onboarding-complete-btn'));
      expect(useOnboardingStore.getState().status).toBe('completed');

      rerender(<OnboardingGuideBar onExecuteSampleQuery={onRun} />);
      expect(container.firstChild).toBeNull();
    });
  });

  describe('End-to-end multi-step continuous journey', () => {
    it('WelcomePage sample click -> guide bar quick run -> explore -> complete unmounts', async () => {
      const mockConn = { id: 'sample_sqlite', name: 'Sample E-Commerce' };
      vi.mocked(sampleDataCommands.initSampleDatabase).mockResolvedValueOnce(mockConn);

      // 1. Welcome page: user opens sample database
      render(<WelcomePage />);
      expect(useOnboardingStore.getState().status).toBe('not_started');
      expect(useOnboardingStore.getState().step).toBe(1);

      fireEvent.click(screen.getByTestId('welcome-open-sample'));

      await waitFor(() => {
        expect(sampleDataCommands.initSampleDatabase).toHaveBeenCalled();
        expect(fetchConnectionsMock).toHaveBeenCalled();
        expect(useOnboardingStore.getState().status).toBe('active');
        expect(useOnboardingStore.getState().step).toBe(2);
        expect(useOnboardingStore.getState().sampleConnectionId).toBe('sample_sqlite');
        expect(localStorage.getItem(PENDING_CONNECTION_KEY)).toBe(
          JSON.stringify({ connectionId: 'sample_sqlite' }),
        );
      });

      cleanup();

      // 2. Connection workspace: guide bar at step 2
      const onRun = vi.fn();
      const { rerender } = render(<OnboardingGuideBar onExecuteSampleQuery={onRun} />);

      expect(screen.getByTestId('onboarding-guide-bar')).toBeInTheDocument();
      expect(screen.getByTestId('onboarding-quick-run-btn')).toBeInTheDocument();
      expect(screen.queryByTestId('onboarding-complete-btn')).not.toBeInTheDocument();

      // 3. Quick run advances to step 3
      fireEvent.click(screen.getByTestId('onboarding-quick-run-btn'));
      expect(onRun).toHaveBeenCalledOnce();
      expect(useOnboardingStore.getState().step).toBe(3);
      expect(useOnboardingStore.getState().queryExecuted).toBe(true);

      rerender(<OnboardingGuideBar onExecuteSampleQuery={onRun} />);
      expect(screen.queryByTestId('onboarding-quick-run-btn')).not.toBeInTheDocument();
      expect(screen.getByTestId('onboarding-complete-btn')).toBeInTheDocument();

      // 4. Explore insights via NL2SQL toggle at step 3
      cleanup();
      const onToggleNl2sql = vi.fn();
      render(
        <QueryEditorSection
          dbSessionId="sess-1"
          editorRef={{ current: null }}
          toolbarRef={{ current: null }}
          compactToolbar={false}
          sql="SELECT 1"
          running={false}
          executionTimeMs={null}
          executionViewModel={toQueryExecutionViewModel(EMPTY_QUERY_EXEC, {
            supportsCancelQuery: false,
            supportsQueryExecutionCancel: false,
          })}
          sqlParams={[]}
          paramValues={{}}
          onParamChange={vi.fn()}
          editorHeight={200}
          editorResizeRef={{ current: null }}
          editorSchema={[]}
          namespaceLoading={false}
          supportsExplain
          safeMode={false}
          inTransaction={false}
          txBusy={false}
          isMultiDb={false}
          isPathHierarchy={false}
          hasContextSelectors={false}
          databases={[]}
          namespaceTree={[]}
          pathAliases={{}}
          contextPath={[]}
          nl2sqlVisible={false}
          onToggleNl2sql={onToggleNl2sql}
          historyVisible={false}
          favoritesVisible={false}
          onToggleHistory={vi.fn()}
          onToggleFavorites={vi.fn()}
          onUpdateSql={vi.fn()}
          onExecute={vi.fn()}
          onExecuteSelection={vi.fn()}
          onCancel={vi.fn()}
          onFormat={vi.fn()}
          onCompletionRefreshed={vi.fn()}
          onExplain={vi.fn()}
          onBeginTx={vi.fn()}
          onCommitTx={vi.fn()}
          onRollbackTx={vi.fn()}
          onApplyAiSql={vi.fn()}
          onOpenAddFavoriteDialog={vi.fn()}
          onQualifiedPath={vi.fn()}
          onSelectContextLevel={vi.fn()}
          onDropTable={vi.fn()}
        />,
      );
      fireEvent.click(screen.getByRole('button', { name: 'nl2sql.title' }));
      expect(onToggleNl2sql).toHaveBeenCalled();
      expect(useOnboardingStore.getState().aiOrChartExplored).toBe(true);

      // 5. Complete onboarding and guide bar unmounts
      cleanup();
      const { rerender: rerenderGuide, container: guideContainer } = render(
        <OnboardingGuideBar onExecuteSampleQuery={onRun} />,
      );
      fireEvent.click(screen.getByTestId('onboarding-complete-btn'));
      expect(useOnboardingStore.getState().status).toBe('completed');

      rerenderGuide(<OnboardingGuideBar onExecuteSampleQuery={onRun} />);
      expect(guideContainer.firstChild).toBeNull();
    });
  });

  describe('Wired onboarding hooks', () => {
    const chartableResult: StatementResult = {
      sql: 'SELECT label, amount FROM metrics',
      columns: [
        { name: 'label', dataType: 'text', nullable: true },
        { name: 'amount', dataType: 'int4', nullable: true },
      ],
      rows: [['one', 1]],
      executionTimeMs: 1,
    };

    it('query success via execution gate marks step 2 -> 3', async () => {
      useOnboardingStore.getState().startOnboarding('sample_sqlite');

      usePanelStore.setState({
        executeQuery: vi.fn().mockImplementation(async () => {
          usePanelStore.setState({
            queryExec: new Map([
              [
                'p1',
                {
                  ...EMPTY_QUERY_EXEC,
                  sql: 'SELECT 1',
                  results: [
                    {
                      sql: 'SELECT 1',
                      columns: [],
                      rows: [[1]],
                      executionTimeMs: 1,
                    },
                  ],
                  activeResultIdx: 0,
                  error: null,
                },
              ],
            ]),
          });
        }),
        executeSelection: vi.fn().mockResolvedValue(undefined),
        queryExec: new Map([['p1', { ...EMPTY_QUERY_EXEC, sql: 'SELECT 1' }]]),
      });

      const editorRef = {
        current: {
          getSelection: () => '',
          insertAt: vi.fn(),
          toggleLineComment: vi.fn(),
        },
      };

      const { result } = renderHook(() =>
        useQueryExecutionGate({
          panelId: 'p1',
          dbSessionId: 'sess-1',
          databaseType: 'sqlite',
          connectionId: 'conn-1',
          editorRef,
          sql: 'SELECT 1',
          boundPayload: undefined,
          inTransaction: false,
          setInTransaction: vi.fn(),
          refreshTxStatus: vi.fn().mockResolvedValue(undefined),
          maybeOfferAbortedDialog: vi.fn().mockResolvedValue(undefined),
          syncContextFromSql: vi.fn().mockResolvedValue(undefined),
          showMessageDialog: vi.fn(),
          onExecutionComplete: vi.fn(),
        }),
      );

      await act(async () => {
        await result.current.runExecute('full');
      });

      expect(useOnboardingStore.getState().queryExecuted).toBe(true);
      expect(useOnboardingStore.getState().step).toBe(3);
    });

    it('chart view switch at step 3 marks aiOrChartExplored', () => {
      useOnboardingStore.getState().startOnboarding('sample_sqlite');
      useOnboardingStore.getState().markQueryExecuted();

      const onSetResultViewMode = (mode: 'table' | 'chart') => {
        if (mode === 'chart') {
          const ob = useOnboardingStore.getState();
          if (ob.status === 'active' && ob.step === 3) {
            ob.markAiOrChartExplored();
          }
        }
      };

      render(
        <ResultWorkspace
          result={chartableResult}
          view="table"
          onViewChange={onSetResultViewMode}
        />,
      );

      fireEvent.click(screen.getByRole('button', { name: 'chart.viewChart' }));
      expect(useOnboardingStore.getState().aiOrChartExplored).toBe(true);
    });
  });

  describe('Persistence recovery journey', () => {
    it('preserves completed status after module reload and does not re-open wizard', async () => {
      useOnboardingStore.getState().startOnboarding('sample_sqlite');
      useOnboardingStore.getState().markQueryExecuted();
      useOnboardingStore.getState().completeOnboarding();
      expect(localStorage.getItem(STORAGE_KEY)).toContain('"status":"completed"');

      vi.resetModules();
      const storeMod = await import('../../../stores/onboardingStore');
      const guideBarMod = await import('../OnboardingGuideBar');
      const reloaded = storeMod.useOnboardingStore.getState();

      expect(reloaded.status).toBe('completed');
      expect(reloaded.step).toBe(3);

      const { container } = render(<guideBarMod.OnboardingGuideBar />);
      expect(container.firstChild).toBeNull();
    });

    it('preserves skipped status after module reload and does not re-open wizard', async () => {
      useOnboardingStore.getState().startOnboarding('sample_sqlite');
      useOnboardingStore.getState().skipOnboarding();
      expect(localStorage.getItem(STORAGE_KEY)).toContain('"status":"skipped"');

      vi.resetModules();
      const storeMod = await import('../../../stores/onboardingStore');
      const guideBarMod = await import('../OnboardingGuideBar');
      const reloaded = storeMod.useOnboardingStore.getState();

      expect(reloaded.status).toBe('skipped');

      const { container } = render(<guideBarMod.OnboardingGuideBar />);
      expect(container.firstChild).toBeNull();
    });
  });
});
