/**
 * Query Execution & Asset Lifecycle Journey Tests.
 *
 * Implements continuous state machine / user journey tests according to
 * `docs/development/interaction-and-testing-principles.md` (Principle 3):
 * 1. Execution strategy lifecycle (cursor -> selection -> ask modal -> choices).
 * 2. Bind parameter gate & remediation lifecycle (missing param -> block -> partial fill -> pass).
 * 3. Result tab pin & multi-stream preservation lifecycle (run 1 -> pin -> run 2 -> append -> unpin).
 * 4. Query favorite asset journey (click -> open new tab without overwriting current panel).
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useQueryExecutionGate } from '../query/useQueryExecutionGate';
import { usePanelStore } from '../../../stores/panelStore';
import { useSettingsStore } from '../../../stores/settingsStore';
import { useConnectionStore } from '../../../stores/connectionStore';
import { applyQueryStreamEvent } from '../../../lib/queryStream';
import type { StatementResult } from '../../../types';
import type { QueryPanel } from '../../../stores/panelTypes';

describe('Query Execution & Asset Experience Journeys', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useSettingsStore.setState({
      settings: {
        ...useSettingsStore.getState().settings,
        safeMode: false,
        autoCommit: true,
        sqlExecutionStrategy: 'current_statement',
      },
    });
    useConnectionStore.setState({
      connections: [
        {
          id: 'conn-1',
          name: 'Test DB',
          driver: 'postgres',
          database: 'app',
          readOnly: false,
        } as any,
      ],
    });
  });

  describe('Journey 1: Execution Strategy Continuous Evolution Journey', () => {
    const multiSql = 'SELECT 1 AS first;\nSELECT 2 AS second;\nSELECT 3 AS third;';

    it('navigates from cursor execution -> selection override -> ask modal choice -> whole script', async () => {
      const executeQuery = vi.fn().mockResolvedValue(undefined);
      const executeSelection = vi.fn().mockResolvedValue(undefined);
      usePanelStore.setState({
        executeQuery,
        executeSelection,
      } as any);

      let currentSelection = '';
      let cursorOffset = multiSql.indexOf('SELECT 2 AS second;');

      const editorRef = {
        current: {
          getSelection: () => currentSelection,
          getCursorOffset: () => cursorOffset,
          toggleLineComment: vi.fn(),
          insertAt: vi.fn(),
        },
      };

      const showMessageDialog = vi.fn();
      const { result, rerender } = renderHook(() =>
        useQueryExecutionGate({
          panelId: 'p1',
          dbSessionId: 'sess-1',
          databaseType: 'postgresql',
          connectionId: 'conn-1',
          editorRef: editorRef as any,
          sql: multiSql,
          boundPayload: undefined,
          inTransaction: false,
          setInTransaction: vi.fn(),
          refreshTxStatus: vi.fn().mockResolvedValue(undefined),
          maybeOfferAbortedDialog: vi.fn().mockResolvedValue(undefined),
          syncContextFromSql: vi.fn().mockResolvedValue(undefined),
          showMessageDialog,
          onExecutionComplete: vi.fn(),
        }),
      );

      // Step 1: In 'current_statement' mode, execute statement containing cursor (statement 2)
      await act(async () => {
        result.current.handleExecute();
      });
      expect(executeSelection).toHaveBeenCalledWith('p1', 'SELECT 2 AS second;', undefined);
      expect(executeQuery).not.toHaveBeenCalled();
      executeSelection.mockClear();

      // Step 2: User makes an active text selection -> selection MUST take absolute precedence
      currentSelection = 'SELECT 1 AS first;';
      await act(async () => {
        result.current.handleExecute();
      });
      expect(executeSelection).toHaveBeenCalledWith('p1', 'SELECT 1 AS first;', undefined);
      executeSelection.mockClear();
      currentSelection = ''; // Clear selection

      // Step 3: User switches execution strategy to 'ask'
      useSettingsStore.setState({
        settings: {
          ...useSettingsStore.getState().settings,
          sqlExecutionStrategy: 'ask',
        },
      });
      cursorOffset = multiSql.indexOf('SELECT 3 AS third;');
      rerender();

      // Trigger execution -> should not execute immediately, must open ask modal
      await act(async () => {
        result.current.handleExecute();
      });
      expect(executeQuery).not.toHaveBeenCalled();
      expect(executeSelection).not.toHaveBeenCalled();
      expect(result.current.executionStrategyAskModal).not.toBeNull();

      // Step 4: In Ask modal, user chooses "Execute Current Statement"
      const modalProps = (result.current.executionStrategyAskModal as any).props;
      expect(modalProps.currentStatement.sql).toBe('SELECT 3 AS third;');
      await act(async () => {
        modalProps.onExecuteCurrent();
      });
      expect(executeSelection).toHaveBeenCalledWith('p1', 'SELECT 3 AS third;', undefined);
      executeSelection.mockClear();

      // Step 5: User triggers execution again in 'ask' mode, this time chooses "Execute Entire Script"
      await act(async () => {
        result.current.handleExecute();
      });
      const modalProps2 = (result.current.executionStrategyAskModal as any).props;
      await act(async () => {
        modalProps2.onExecuteEntire();
      });
      expect(executeQuery).toHaveBeenCalledWith('p1', undefined);
      executeQuery.mockClear();

      // Step 6: User switches strategy to 'entire_script' -> executes without modal
      useSettingsStore.setState({
        settings: {
          ...useSettingsStore.getState().settings,
          sqlExecutionStrategy: 'entire_script',
        },
      });
      rerender();

      await act(async () => {
        result.current.handleExecute();
      });
      expect(executeQuery).toHaveBeenCalledWith('p1', undefined);
      expect(result.current.executionStrategyAskModal).toBeNull();
    });
  });

  describe('Journey 2: Bind Param Security Gate & Remediation Journey', () => {
    const parameterizedSql = 'SELECT * FROM orders WHERE user_id = :uid AND status = :st;';

    it('blocks execution when unassigned, guides remediation, and passes when complete', async () => {
      const executeQuery = vi.fn().mockResolvedValue(undefined);
      const executeSelection = vi.fn().mockResolvedValue(undefined);
      usePanelStore.setState({
        executeQuery,
        executeSelection,
      } as any);

      let paramValues: Record<string, string> = {};
      const showMessageDialog = vi.fn();

      const { result, rerender } = renderHook(() =>
        useQueryExecutionGate({
          panelId: 'p1',
          dbSessionId: 'sess-1',
          databaseType: 'postgresql',
          connectionId: 'conn-1',
          editorRef: { current: null },
          sql: parameterizedSql,
          boundPayload: undefined,
          paramValues,
          inTransaction: false,
          setInTransaction: vi.fn(),
          refreshTxStatus: vi.fn().mockResolvedValue(undefined),
          maybeOfferAbortedDialog: vi.fn().mockResolvedValue(undefined),
          syncContextFromSql: vi.fn().mockResolvedValue(undefined),
          showMessageDialog,
          onExecutionComplete: vi.fn(),
        }),
      );

      // Step 1: Execute with 0 params filled -> hard blocked to prevent silent NULL
      await act(async () => {
        result.current.handleExecute();
      });
      expect(showMessageDialog).toHaveBeenCalledWith('Missing value for :uid', 'error');
      expect(executeQuery).not.toHaveBeenCalled();
      expect(executeSelection).not.toHaveBeenCalled();
      showMessageDialog.mockClear();

      // Step 2: User fills only :uid, leaves :st blank
      paramValues = { 'named:uid': '1001' };
      rerender();

      await act(async () => {
        result.current.handleExecute();
      });
      expect(showMessageDialog).toHaveBeenCalledWith('Missing value for :st', 'error');
      expect(executeQuery).not.toHaveBeenCalled();
      expect(executeSelection).not.toHaveBeenCalled();
      showMessageDialog.mockClear();

      // Step 3: User fills both :uid and :st -> gate opens and executes cleanly
      paramValues = { 'named:uid': '1001', 'named:st': 'active' };
      rerender();

      await act(async () => {
        result.current.handleExecute();
      });
      expect(showMessageDialog).not.toHaveBeenCalled();
      expect(executeSelection).toHaveBeenCalledWith(
        'p1',
        "SELECT * FROM orders WHERE user_id = 1001 AND status = 'active';",
        undefined,
      );
    });
  });

  describe('Journey 3: Result Tab Pin & Streaming Preservation Journey', () => {
    it('pins result, preserves it across new executions via baseOffset, and clears on unpin', () => {
      // Step 1: Initial query result arrives
      let tabState = {
        results: [] as StatementResult[],
        running: true,
        error: null as string | null,
        executionTimeMs: null as number | null,
        executionId: 'exec-1',
      };

      tabState = applyQueryStreamEvent(tabState, {
        type: 'statementStart',
        index: 0,
        sql: 'SELECT 1 AS original;',
        columns: [{ name: 'original', dataType: 'int', nullable: false }],
      });
      tabState = applyQueryStreamEvent(tabState, {
        type: 'rows',
        index: 0,
        rows: [[100]],
      });
      tabState = applyQueryStreamEvent(tabState, {
        type: 'statementEnd',
        index: 0,
        rowsAffected: 1,
        executionTimeMs: 15,
        truncated: false,
      });
      tabState = applyQueryStreamEvent(tabState, { type: 'done', totalTimeMs: 15 });

      expect(tabState.results).toHaveLength(1);
      expect(tabState.results[0].rows).toEqual([[100]]);

      // Step 2: User pins this result tab
      tabState.results[0].pinned = true;

      // Step 3: User executes a second query while tab 0 is pinned
      // Pinned results are preserved at the beginning; new results use baseOffset = 1
      const pinnedResults = tabState.results.filter((r) => r.pinned);
      const baseOffset = pinnedResults.length;
      expect(baseOffset).toBe(1);

      let nextRunState = {
        results: pinnedResults,
        running: true,
        error: null,
        executionTimeMs: null,
        executionId: 'exec-2',
      };

      // Stream events for the new query arrive (stream event index = 0, target index = 1)
      nextRunState = applyQueryStreamEvent(
        nextRunState,
        {
          type: 'statementStart',
          index: 0,
          sql: 'SELECT 2 AS fresh;',
          columns: [{ name: 'fresh', dataType: 'int', nullable: false }],
        },
        baseOffset,
      );
      nextRunState = applyQueryStreamEvent(
        nextRunState,
        {
          type: 'rows',
          index: 0,
          rows: [[200]],
        },
        baseOffset,
      );
      nextRunState = applyQueryStreamEvent(
        nextRunState,
        {
          type: 'statementEnd',
          index: 0,
          rowsAffected: 1,
          executionTimeMs: 8,
          truncated: false,
        },
        baseOffset,
      );
      nextRunState = applyQueryStreamEvent(nextRunState, { type: 'done', totalTimeMs: 8 });

      // Step 4: Verify Tab 0 was preserved untouched with its data and pinned flag
      expect(nextRunState.results).toHaveLength(2);
      expect(nextRunState.results[0].sql).toBe('SELECT 1 AS original;');
      expect(nextRunState.results[0].rows).toEqual([[100]]);
      expect(nextRunState.results[0].pinned).toBe(true);

      // Verify Tab 1 is the newly appended result
      expect(nextRunState.results[1].sql).toBe('SELECT 2 AS fresh;');
      expect(nextRunState.results[1].rows).toEqual([[200]]);
      expect(nextRunState.results[1].pinned).toBeFalsy();

      // Step 5: User unpins Tab 0 -> next run cleans all unpinned tabs
      nextRunState.results[0].pinned = false;
      const unpinnedPreserved = nextRunState.results.filter((r) => r.pinned);
      expect(unpinnedPreserved).toHaveLength(0);
    });
  });

  describe('Journey 4: Query Favorite to New Tab Journey', () => {
    it('opens favorite in an isolated new panel tab without overwriting current editor state', () => {
      const initialPanel: QueryPanel = {
        id: 'panel-qry-main',
        type: 'query',
        title: 'Work in progress',
        sql: 'SELECT * FROM drafts;',
        dbSessionId: 'sess-1',
        connectionId: 'conn-1',
        results: [],
        activeResultIdx: 0,
        resultViewMode: 'table',
        chartConfig: { chartType: 'bar', xColumn: null, yColumns: [] },
        historySearch: '',
        historyScopeMode: 'current',
        pinned: false,
      };

      usePanelStore.setState({
        panels: [initialPanel],
        activePanelId: 'panel-qry-main',
        queryFavorites: [
          {
            id: 'fav-monthly-report',
            title: 'Monthly Sales Report',
            sql: "SELECT date_trunc('month', created_at), sum(total) FROM orders GROUP BY 1;",
            connectionId: 'conn-1',
            createdAt: '2026-09-08T00:00:00Z',
          },
        ],
      });

      // Simulate clicking on the favorite in QuerySidebarSection
      const favorite = usePanelStore.getState().queryFavorites[0];
      const stateBefore = usePanelStore.getState();
      const currentPanel = stateBefore.panels.find((p) => p.id === 'panel-qry-main')!;

      // Click action creates a new panel tab with the favorite's title & SQL
      const newPanelId = 'panel-qry-fav-report';
      const newPanel: QueryPanel = {
        ...currentPanel,
        id: newPanelId,
        title: favorite.title,
        sql: favorite.sql,
      };

      act(() => {
        usePanelStore.getState().addPanel(newPanel, true);
        usePanelStore.getState().setActivePanel(newPanelId);
      });

      const stateAfter = usePanelStore.getState();
      expect(stateAfter.panels).toHaveLength(2);

      // Verify the original panel's work in progress was not modified or overwritten
      const originalPanelAfter = stateAfter.panels.find((p) => p.id === 'panel-qry-main')!;
      expect(originalPanelAfter.sql).toBe('SELECT * FROM drafts;');
      expect(originalPanelAfter.title).toBe('Work in progress');

      // Verify the new panel has the favorite SQL and title
      const newlyOpenedPanel = stateAfter.panels.find((p) => p.id === newPanelId)!;
      expect(newlyOpenedPanel.sql).toBe(favorite.sql);
      expect(newlyOpenedPanel.title).toBe('Monthly Sales Report');
      expect(stateAfter.activePanelId).toBe(newPanelId);
    });
  });
});
