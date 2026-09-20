import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import type { SqlEditorHandle } from '../../components/SqlEditor';
import { buildEditorSchema } from '../../lib/buildEditorSchema';
import {
  inferDefaultSchema,
  inferDefaultTable,
  tableClauseFingerprint,
  tablesReferencedInSql,
} from '../../lib/sqlEditorDefaults';
import { usePanelStore } from '../../stores/panelStore';
import { useQueryBuilderStore } from '../../stores/queryBuilderStore';
import { useActiveConnectionStore } from '../../stores/activeConnectionStore';
import { useQueryExec } from '../../hooks/useQueryExec';
import { useSchemaStore } from '../../stores/schemaStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useI18n } from '../../hooks/useI18n';
import { useResizable } from '../../hooks/useResizable';
import { useCompactToolbar } from '../../hooks/useCompactToolbar';
import { queryToolbarExpandedMinWidth } from './queryToolbarWidth';
import { formatSql } from '../../lib/sqlFormat';
import { paramsToPayload } from '../../lib/sqlBindParams';
import { sqlEditorEnhancedEP, useExtension } from '@datazen/extension-points';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import { resolveExportScope } from '../../lib/exportCapability';
import { toQueryExecutionViewModel } from '../../lib/queryExecutionViewModel';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { ResultMessageDialog } from '../../components/ui/ResultMessageDialog';
import {
  useMetadataSnapshot,
  ensureMetadataRelations,
  resolveEditorDialectId,
} from '../../stores/schemaStoreSelectors';
import { metadataCache } from '../../components/sql-editor/metadata/metadataCache';
import type { QueryPanelProps } from './query/contracts';
import { QuerySidebarSection, useQueryContextPath } from './query/QuerySidebarSection';
import { QueryEditorSection } from './query/QueryEditorSection';
import {
  QueryTransactionModals,
  useQueryTransaction,
  FavoriteNameDialog,
  QueryResultsPane,
} from './query/QueryTransactionModals';
import { useQueryExecutionGate } from './query/useQueryExecutionGate';
import { createQueryDropHandler, useQueryPanelWorkflows } from './query/queryDropHandler';
import {
  estimateQueryResultPaneHeight,
  queryResultPaneSizing,
} from './query/queryResultPaneHeight';
import { resolveResultWorkspaceView } from './result-workspace/resultWorkspaceHelpers';
import { cn } from '../../lib/cn';
import { sendQueryErrorChatDraft } from './query/queryErrorChatPrompt';

export type { QueryPanelProps } from './query/contracts';

/**
 * Persisted editor height. Presence of the stored value is also what marks the
 * split as user-controlled rather than automatic.
 */
const EDITOR_HEIGHT_STORAGE_KEY = 'query-editor-height';

export function QueryPanel({
  panelId,
  dbSessionId,
  connectionId,
  databaseType,
  connectionName,
  database,
  schema,
  namespacePath: panelNamespacePath,
  callbacks,
}: QueryPanelProps) {
  const { t } = useI18n();
  const [confirmRetry, confirmRetryDialog] = useConfirmDialog();
  const exec = useQueryExec(panelId);
  // While the visual builder is up it replaces the whole query content area,
  // so the result pane yields its height to the canvas (PRD §6.4 / G2).
  const qbOpenHere = useQueryBuilderStore((s) =>
    s.openPanelId ? s.openPanelId === panelId : s.isOpen,
  );
  // NOTE: the builder is torn down when the panel *tab* closes, not when this
  // component unmounts — switching tabs unmounts the inactive panel too, and
  // that must not throw its builder away. See ContentView's panel-diff effect.
  const safeMode = useSettingsStore((s) => s.settings.safeMode);
  const driverCapabilities = useActiveConnectionStore(
    (s) => s.connections[connectionId]?.capabilities,
  );
  const activeConnectionEntry = useActiveConnectionStore((s) => s.connections[connectionId]);
  const executionViewModel = useMemo(
    () => toQueryExecutionViewModel(exec, driverCapabilities),
    [exec, driverCapabilities],
  );

  const historyVisible = usePanelStore((s) => s.historyVisible);
  const updateSql = usePanelStore((s) => s.updateSql);
  const setActiveResult = usePanelStore((s) => s.setActiveResult);
  const cancelQuery = usePanelStore((s) => s.cancelQuery);
  const loadHistory = usePanelStore((s) => s.loadHistory);
  const toggleHistory = usePanelStore((s) => s.toggleHistory);
  const favoritesVisible = usePanelStore((s) => s.favoritesVisible);
  const loadFavorites = usePanelStore((s) => s.loadFavorites);
  const storeAddFavorite = usePanelStore((s) => s.addFavorite);
  const toggleFavorites = usePanelStore((s) => s.toggleFavorites);
  const setResultDetailRow = usePanelStore((s) => s.setResultDetailRow);
  const setChartConfig = usePanelStore((s) => s.setChartConfig);
  const setResultViewModeStore = usePanelStore((s) => s.setResultViewMode);

  const editorRef = useRef<SqlEditorHandle>(null);
  const enhanced = useExtension(sqlEditorEnhancedEP);
  const bindState = enhanced.useBindParameters?.(exec.sql) ?? {
    params: [],
    values: {},
    labels: {},
    activeParamIds: [],
    paramHistory: {},
    setValue: () => {},
    applyHistoryEntry: () => {},
    clearHistory: () => {},
    markSubmitted: () => {},
    getHistory: () => [],
  };
  const sqlParams = bindState.params;
  const paramValues = bindState.values;
  const paramLabels = bindState.labels;
  const paramValuesRef = useRef<Record<string, string>>({});
  paramValuesRef.current = paramValues;

  const paramHistory = useMemo(() => {
    const map: Record<string, any[]> = {};
    for (const p of sqlParams) {
      map[p.stableId] = bindState.getHistory(p.stableId);
    }
    return map;
  }, [sqlParams, bindState]);

  const [nl2sqlVisible, setNl2sqlVisible] = useState(false);
  const [messageDialogOpen, setMessageDialogOpen] = useState(false);
  const [messageDialogText, setMessageDialogText] = useState('');
  const [messageDialogKind, setMessageDialogKind] = useState<'error' | 'success'>('error');
  const [executionSeq, setExecutionSeq] = useState(0);
  const resultViewMode = exec.resultViewMode ?? 'table';

  /**
   * The editor/results split starts out automatic: the editor owns whatever
   * height the result pane does not need, and the pane itself only claims the
   * height its content needs (capped at half by CSS). Dragging the splitter
   * pins the editor to an explicit height for good — `editorResizeManual`
   * remembers that choice, including across restarts via the same storage key
   * `useResizable` persists to.
   */
  const [editorResizeManual, setEditorResizeManual] = useState(() => {
    try {
      return localStorage.getItem(`resize:${EDITOR_HEIGHT_STORAGE_KEY}`) != null;
    } catch {
      return false;
    }
  });
  const editorViewportRef = useRef<HTMLDivElement | null>(null);

  const { size: editorHeight, handleRef: editorResizeRef } = useResizable({
    direction: 'vertical',
    initialSize: 280,
    minSize: 100,
    maxSize: 900,
    storageKey: EDITOR_HEIGHT_STORAGE_KEY,
    onResizeStart: () => setEditorResizeManual(true),
    // Measured at grab time: automatic sizing is CSS-driven, so `size` would
    // otherwise still hold a stale value and the splitter would jump.
    getStartSize: () => editorViewportRef.current?.offsetHeight,
  });

  const tables = useSchemaStore((s) => s.tables);
  const views = useSchemaStore((s) => s.views);
  const columnMap = useSchemaStore((s) => s.columnMap);
  const namespaceTree = useSchemaStore((s) => s.namespaceTree);
  const pathAliases = useSchemaStore((s) => s.pathAliases);
  const databases = useSchemaStore((s) => s.databases);
  const currentDatabase = useSchemaStore((s) => s.currentDatabase);
  const currentSchema = useSchemaStore((s) => s.currentSchema);
  const isMultiDb = useSchemaStore((s) => s.isMultiDatabase);
  const ensureColumns = useSchemaStore((s) => s.ensureColumns);
  const loadColumnMap = useSchemaStore((s) => s.loadColumnMap);
  const namespaceLoading = useSchemaStore((s) => s.ensuringCount > 0);
  const schemaEpoch = useSchemaStore((s) => s.schemaEpoch);

  const metadataSnapshot = useMetadataSnapshot(dbSessionId);

  const onNavigateToTable = useCallback(
    (target: { database?: string; schema?: string; name: string }) => {
      callbacks?.openRelation(
        target.name,
        target.schema ?? null,
        target.database ?? database,
        'data',
      );
    },
    [callbacks, database],
  );

  const onNavigateToStructure = useCallback(
    (target: { database?: string; schema?: string; name: string; columnName?: string }) => {
      callbacks?.openRelation(
        target.name,
        target.schema ?? null,
        target.database ?? database,
        'structure',
        target.columnName,
      );
    },
    [callbacks, database],
  );

  const onNavigateToDdl = useCallback(
    (target: { database?: string; schema?: string; name: string; kind: 'table' | 'view' }) => {
      callbacks?.openRelation(
        target.name,
        target.schema ?? null,
        target.database ?? database,
        'ddl',
      );
    },
    [callbacks, database],
  );

  const dbMeta = databaseType ? DB_REGISTRY[databaseType as keyof typeof DB_REGISTRY] : undefined;
  const isPathHierarchy = dbMeta?.namespaceEnsure === 'path-hierarchy';
  const selectedDatabase = isPathHierarchy
    ? (currentDatabase ?? database)
    : (database ?? currentDatabase);
  const selectedSchema = schema ?? currentSchema;
  const supportsExplain = dbMeta?.supportsExplain === true;
  const hasContextSelectors = isPathHierarchy || (isMultiDb && databases.length > 0);
  const queryResultExportCapability = resolveExportScope(dbMeta);
  const schemaState = useMemo(
    () => ({ currentDatabase, currentSchema, tables, views, columnMap }),
    [columnMap, currentDatabase, currentSchema, tables, views],
  );

  const contextPathState = useQueryContextPath({
    panelId,
    dbSessionId,
    panelNamespacePath,
    isPathHierarchy,
    selectedDatabase: selectedDatabase ?? undefined,
    namespaceTree,
    pathAliases,
    databases,
    currentDatabase,
  });

  const tx = useQueryTransaction({ dbSessionId });

  const { ref: toolbarRef, compact: compactToolbar } = useCompactToolbar(
    useMemo(
      () =>
        queryToolbarExpandedMinWidth({
          hasContextSelectors,
          isPathHierarchy,
          isMultiDb,
          inTransaction: tx.inTransaction,
          contextSchema: selectedSchema,
          namespaceTree,
          pathAliases,
          databases,
          contextPath: contextPathState.contextPath,
          currentDatabase: selectedDatabase,
        }),
      [
        hasContextSelectors,
        isPathHierarchy,
        isMultiDb,
        tx.inTransaction,
        selectedSchema,
        namespaceTree,
        pathAliases,
        databases,
        contextPathState.contextPath,
        selectedDatabase,
      ],
    ),
  );

  const editorSchema = useMemo(
    () =>
      buildEditorSchema({
        namespaceTree,
        tables,
        views,
        columnMap,
        currentDatabase: selectedDatabase,
        hoistPath: contextPathState.contextPath,
      }),
    [namespaceTree, tables, views, columnMap, selectedDatabase, contextPathState.contextPath],
  );
  const editorDefaultSchema = useMemo(() => inferDefaultSchema(tables, views), [tables, views]);
  // Table-clause fingerprint of the live SQL: stable while only column names
  // change (e.g. deleting a column), changes when tables/clauses change.
  // Drives two keystroke-cost optimizations below (defaultTable + workflowSql).
  const tableFingerprint = useMemo(() => tableClauseFingerprint(exec.sql), [exec.sql]);
  // `defaultTable` feeds the CodeMirror SQL compartment: a new reference
  // reconfigures lang-sql (schema reindex, O(tables)). Only recompute when
  // the table fingerprint changes, and only publish when the table changes.
  const defaultTableRef = useRef<string | undefined>(undefined);
  const defaultTableFpRef = useRef('');
  const defaultTableInit = useRef(false);
  if (!defaultTableInit.current || tableFingerprint !== defaultTableFpRef.current) {
    defaultTableInit.current = true;
    defaultTableFpRef.current = tableFingerprint;
    const next = inferDefaultTable(exec.sql);
    if (next !== defaultTableRef.current) {
      defaultTableRef.current = next;
    }
  }
  const editorDefaultTable = defaultTableRef.current;
  // Snapshot of the SQL for diagnosis/retry/explain workflows. These only
  // need the SQL at error/execution time; keeping a stable reference across
  // column-name keystrokes skips the whole workflow memo chain
  // (diagnosis context build + retry action) per keystroke. Refreshes when
  // the table fingerprint changes, on error change, or after an execution.
  const workflowSqlRef = useRef(exec.sql);
  const workflowFpRef = useRef(tableFingerprint);
  const workflowErrRef = useRef(exec.error);
  const workflowSeqRef = useRef(executionSeq);
  if (
    tableFingerprint !== workflowFpRef.current ||
    exec.error !== workflowErrRef.current ||
    executionSeq !== workflowSeqRef.current
  ) {
    workflowFpRef.current = tableFingerprint;
    workflowErrRef.current = exec.error;
    workflowSeqRef.current = executionSeq;
    workflowSqlRef.current = exec.sql;
  }
  const workflowSql = workflowSqlRef.current;
  const boundPayload = useMemo(
    () => (sqlParams.length > 0 ? paramsToPayload(sqlParams, paramValues) : undefined),
    [sqlParams, paramValues],
  );

  const showMessageDialog = useCallback((text: string, kind: 'error' | 'success' = 'error') => {
    setMessageDialogText(text);
    setMessageDialogKind(kind);
    setMessageDialogOpen(true);
  }, []);

  const executionGate = useQueryExecutionGate({
    panelId,
    dbSessionId,
    databaseType,
    connectionId,
    editorRef,
    sql: exec.sql,
    boundPayload,
    paramValues,
    inTransaction: tx.inTransaction,
    setInTransaction: tx.setInTransaction,
    refreshTxStatus: tx.refreshTxStatus,
    maybeOfferAbortedDialog: tx.maybeOfferAbortedDialog,
    syncContextFromSql: contextPathState.syncContextFromSql,
    showMessageDialog,
    onExecutionComplete: () => {
      bindState.markSubmitted(bindState.values);
      setExecutionSeq((seq) => seq + 1);
    },
  });

  const workflows = useQueryPanelWorkflows({
    panelId,
    connectionId,
    dbSessionId,
    databaseType,
    connectionName,
    database,
    schema,
    // NOTE: workflows (diagnosis/retry/explain) only need the SQL snapshot at
    // error/execution time. Passing live `exec.sql` re-runs the whole workflow
    // memo chain (diagnosis context build + retry action) on every keystroke
    // including column-name deletions. `workflowSql` below stays referentially
    // stable while only column names change (see tableClauseFingerprint) and
    // refreshes on table changes, errors, or executions.
    sql: workflowSql,
    error: exec.error,
    chartConfig: exec.chartConfig,
    resultViewMode,
    activeResultRowsLength: exec.results[exec.activeResultIdx]?.rows.length ?? 0,
    boundPayload,
    paramValuesRef,
    schemaState,
    serverVersion: activeConnectionEntry?.serverInfo?.serverVersion,
    selectedDatabase: selectedDatabase ?? undefined,
    runExecute: executionGate.runExecute,
    confirmRetry,
    updateSql,
    showMessageDialog,
    t,
  });

  const handleDropTable = useMemo(
    () =>
      createQueryDropHandler({
        connectionId,
        dbSessionId,
        databaseType,
        database: selectedDatabase ?? '',
        editorRef,
      }),
    [connectionId, dbSessionId, databaseType, selectedDatabase],
  );

  // S6-A: Build sanitized error context and send as AI chat draft.
  const handleAskInChat = useCallback(() => {
    if (!exec.error || !exec.sql.trim()) return;
    sendQueryErrorChatDraft(
      {
        panelId,
        connectionId,
        dbSessionId,
        database: selectedDatabase ?? database,
        schema: selectedSchema,
        sql: exec.sql,
        error: exec.error,
        databaseType,
        connectionName,
        serverVersion: activeConnectionEntry?.serverInfo?.serverVersion,
        schemaState: {
          currentDatabase: selectedDatabase,
          currentSchema: selectedSchema,
          tables,
          views,
          columnMap,
        },
      },
      callbacks,
    );
  }, [
    activeConnectionEntry?.serverInfo?.serverVersion,
    callbacks,
    columnMap,
    connectionId,
    connectionName,
    database,
    databaseType,
    dbSessionId,
    exec.error,
    exec.sql,
    panelId,
    selectedDatabase,
    selectedSchema,
    tables,
    views,
  ]);

  useEffect(() => {
    if (!exec.sql.trim()) return;
    const timer = setTimeout(() => void contextPathState.syncContextFromSql(exec.sql), 50);
    return () => clearTimeout(timer);
  }, [exec.sql, contextPathState.syncContextFromSql]);

  useEffect(() => {
    void loadHistory(connectionId);
    void loadFavorites(connectionId);
  }, [connectionId, loadHistory, loadFavorites]);

  // Eagerly load column metadata so SQL autocomplete can suggest columns
  // even without a FROM clause (e.g. typing "SELECT na" shows matching columns).
  //
  // Phase 1: For multi-DB / path-hierarchy drivers, loadForConnection skips
  // loadTables, so `tables` stays empty. Trigger loadTables for the current
  // database so that namespaceTree gets table entries and columnMap can be built.
  const loadTablesFn = useSchemaStore((s) => s.loadTables);
  useEffect(() => {
    if (!dbSessionId || !isMultiDb || !currentDatabase) return;
    if (tables.length > 0) return; // already loaded
    void loadTablesFn(currentDatabase, dbSessionId);
  }, [dbSessionId, loadTablesFn, isMultiDb, currentDatabase, tables.length]);

  // Phase 2: Once tables are available (or for non-multi-DB), load columns.
  // Trigger on schemaEpoch (bumped by loadTables) and namespaceTree structural
  // changes (bumped via namespaceFingerprint). Also depends on tables.length so
  // it re-fires after Phase 1 populates tables.
  const namespaceFingerprint = useSchemaStore((s) => {
    const tree = s.namespaceTree;
    if (Array.isArray(tree)) return String(tree.length);
    return Object.keys(tree).sort().join(',');
  });
  useEffect(() => {
    if (!dbSessionId || !selectedDatabase) return;
    void loadColumnMap(dbSessionId, selectedDatabase);
  }, [
    dbSessionId,
    loadColumnMap,
    selectedDatabase,
    schemaEpoch,
    namespaceFingerprint,
    tables.length,
  ]);

  // Keep the editor metadata cache pinned to the tab's database. Only
  // switch when the bound database actually changes — switchContext drops
  // all loaded relations, so calling it per keystroke would thrash the cache.
  useEffect(() => {
    if (!dbSessionId || !selectedDatabase) return;
    const snapshot = metadataCache.getSnapshot(dbSessionId);
    if (snapshot.database !== selectedDatabase) {
      metadataCache.switchContext(dbSessionId, {
        database: selectedDatabase,
        schema: selectedSchema ?? undefined,
        dialectId: resolveEditorDialectId(databaseType),
      });
    }
  }, [dbSessionId, selectedDatabase, selectedSchema, databaseType]);

  useEffect(() => {
    const names = tablesReferencedInSql(exec.sql);
    if (names.length === 0) return;
    const timer = setTimeout(() => {
      if (!dbSessionId || !selectedDatabase) return;
      void ensureColumns(names, dbSessionId, selectedDatabase);
      {
        const dialectId = resolveEditorDialectId(databaseType);
        const requests = names.map((name) => ({
          identity: {
            namespacePath: selectedSchema ? [{ name: selectedSchema, quoted: false }] : [],
            name: { name, quoted: false },
          },
          kind: 'table' as const,
        }));
        ensureMetadataRelations(dbSessionId, requests, {
          database: selectedDatabase,
          schema: selectedSchema ?? undefined,
          dialectId,
        });
      }
    }, 120);
    return () => clearTimeout(timer);
  }, [
    exec.sql,
    ensureColumns,
    namespaceTree,
    tables,
    views,
    dbSessionId,
    selectedSchema,
    selectedDatabase,
    databaseType,
  ]);

  useEffect(() => {
    const unlisten = listen('menu:add-favorite', () => {
      workflows.openAddFavoriteDialog(
        workflows.pendingFavSqlRef.current ||
          usePanelStore.getState().queryExec.get(panelId)?.sql ||
          '',
      );
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [panelId, workflows.openAddFavoriteDialog, workflows.pendingFavSqlRef]);

  const handleFormat = useCallback(() => {
    if (!exec.sql.trim()) return;
    try {
      const options = useSettingsStore.getState().settings.sqlFormatOptions;
      updateSql(panelId, formatSql(exec.sql, databaseType, options));
    } catch {
      /* keep original SQL if formatter rejects dialect-specific syntax */
    }
  }, [exec.sql, panelId, databaseType, updateSql]);

  const { results, activeResultIdx } = exec;
  const activeResult = results[activeResultIdx];

  // Nothing has been executed yet: the editor owns the whole panel and the
  // result pane is not mounted at all.
  const resultsIdle =
    !exec.running &&
    !exec.error &&
    !workflows.showExplain &&
    !workflows.diagnosisVisible &&
    results.length === 0;

  // A pinned editor height only makes sense while there is something to split
  // against; before the first execution the editor always fills the panel.
  const editorHeightPinned = editorResizeManual && !resultsIdle;

  const resultsPaneHeight = useMemo(
    () =>
      estimateQueryResultPaneHeight({
        result: activeResult,
        view: resolveResultWorkspaceView(activeResult, resultViewMode, exec.chartConfig).view,
        hasResultTabs: results.length > 1 || workflows.explainResult != null,
        hasError: !!exec.error,
        showExplain: workflows.showExplain,
        showDiagnosis: workflows.diagnosisVisible,
      }),
    [
      activeResult,
      exec.chartConfig,
      exec.error,
      resultViewMode,
      results.length,
      workflows.diagnosisVisible,
      workflows.explainResult,
      workflows.showExplain,
    ],
  );

  return (
    <div
      className="flex min-h-0 flex-1 flex-col"
      data-testid="query-panel"
      data-execution-seq={executionSeq}
      data-query-running={exec.running ? 'true' : 'false'}
    >
      <div className="flex min-h-0 flex-1">
        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
          <QueryEditorSection
            panelId={panelId}
            dbSessionId={dbSessionId}
            databaseType={databaseType}
            editorRef={editorRef}
            toolbarRef={toolbarRef}
            compactToolbar={compactToolbar}
            sql={exec.sql}
            running={exec.running}
            executionTimeMs={exec.executionTimeMs}
            executionViewModel={executionViewModel}
            sqlParams={sqlParams}
            paramValues={paramValues}
            paramLabels={paramLabels}
            paramHistory={paramHistory}
            onParamChange={bindState.setValue}
            onApplyParamHistory={bindState.setValue}
            onClearParamHistory={bindState.clearHistory}
            editorHeight={editorHeight}
            editorResizeRef={editorResizeRef}
            editorHeightPinned={editorHeightPinned}
            showIdleHint={resultsIdle}
            showResizeHandle={!resultsIdle}
            editorViewportRef={editorViewportRef}
            editorSchema={editorSchema}
            editorDefaultSchema={editorDefaultSchema}
            editorDefaultTable={editorDefaultTable}
            namespaceLoading={namespaceLoading}
            supportsExplain={supportsExplain}
            safeMode={safeMode}
            inTransaction={tx.inTransaction}
            txBusy={tx.txBusy}
            isMultiDb={isMultiDb}
            isPathHierarchy={isPathHierarchy}
            hasContextSelectors={hasContextSelectors}
            databases={databases}
            selectedDatabase={selectedDatabase}
            selectedSchema={selectedSchema}
            namespaceTree={namespaceTree}
            pathAliases={pathAliases}
            contextPath={contextPathState.contextPath}
            nl2sqlVisible={nl2sqlVisible}
            onToggleNl2sql={() => setNl2sqlVisible((v) => !v)}
            historyVisible={historyVisible}
            favoritesVisible={favoritesVisible}
            onToggleHistory={toggleHistory}
            onToggleFavorites={toggleFavorites}
            onUpdateSql={(v) => updateSql(panelId, v)}
            onExecute={executionGate.handleExecute}
            onExecuteSelection={executionGate.handleExecuteSelection}
            onCancel={() => void cancelQuery(panelId)}
            onFormat={handleFormat}
            onCompletionRefreshed={(message) => showMessageDialog(message, 'success')}
            onExplain={workflows.handleExplain}
            onBeginTx={tx.handleBeginTx}
            onCommitTx={tx.handleCommitTx}
            onRollbackTx={tx.handleRollbackTx}
            onApplyAiSql={(v) => updateSql(panelId, v)}
            onOpenAddFavoriteDialog={workflows.openAddFavoriteDialog}
            onQualifiedPath={contextPathState.handleQualifiedPath}
            onSelectContextLevel={contextPathState.handleSelectContextLevel}
            onDropTable={handleDropTable}
            // S6-D: metadata & navigation wiring
            metadataSnapshot={metadataSnapshot}
            connectionId={connectionId}
            onNavigateToTable={onNavigateToTable}
            onNavigateToStructure={onNavigateToStructure}
            onNavigateToDdl={onNavigateToDdl}
          />
          {!qbOpenHere && !resultsIdle && (
            /*
             * Automatic sizing: the pane claims the height its content needs,
             * capped at half of the query content area so the editor keeps the
             * other half. Once the user has dragged the splitter the pane simply
             * fills what is left.
             */
            <div
              className={cn('flex min-h-0 flex-col', editorHeightPinned ? 'flex-1' : 'shrink')}
              style={queryResultPaneSizing(editorHeightPinned, resultsPaneHeight)}
              data-testid="query-results-host"
            >
              <QueryResultsPane
                dbSessionId={dbSessionId}
                databaseType={databaseType}
                sql={exec.sql}
                running={exec.running}
                error={exec.error}
                results={results}
                activeResultIdx={activeResultIdx}
                activeResult={activeResult}
                resultViewMode={resultViewMode}
                chartConfig={exec.chartConfig}
                resultDetailRowIndex={exec.resultDetailRowIndex}
                queryResultExportCapability={queryResultExportCapability}
                selectedDatabase={selectedDatabase}
                showExplain={workflows.showExplain}
                explainLoading={workflows.explainLoading}
                explainError={workflows.explainError}
                explainResult={workflows.explainResult}
                diagnosisVisible={workflows.diagnosisVisible}
                diagnosisContext={workflows.diagnosisContext}
                onExplainError={workflows.handleExplainError}
                retryActionEnabled={workflows.retryAction.enabled}
                addToDashboardOpen={workflows.addToDashboardOpen}
                onApplyAiSql={(v) => updateSql(panelId, v)}
                onApplyFixSql={workflows.handleApplyFixSql}
                onRetry={workflows.handleRetry}
                onSetActiveResult={(idx) => setActiveResult(panelId, idx)}
                onTogglePinResult={(idx) => usePanelStore.getState().togglePinResult(panelId, idx)}
                onSetResultViewMode={(mode) => {
                  setResultViewModeStore(panelId, mode);
                }}
                onChartConfigChange={(cfg) => setChartConfig(panelId, cfg)}
                onRowDetail={(rowIndex) => setResultDetailRow(panelId, rowIndex)}
                onShowExplain={workflows.setShowExplain}
                onDiagnosisVisible={workflows.setDiagnosisVisible}
                onAddToDashboardOpen={workflows.setAddToDashboardOpen}
                onAddToDashboardConfirm={workflows.handleAddToDashboardConfirm}
                onAskInChat={handleAskInChat}
              />
            </div>
          )}
          <FavoriteNameDialog
            open={workflows.showFavoriteDialog}
            favoriteName={workflows.favoriteName}
            favoriteDialogSql={workflows.favoriteDialogSql}
            onFavoriteNameChange={workflows.setFavoriteName}
            onClose={() => workflows.setShowFavoriteDialog(false)}
            onSave={() => {
              if (!workflows.favoriteName.trim()) return;
              void storeAddFavorite(
                workflows.favoriteName.trim(),
                workflows.favoriteDialogSql,
                connectionId,
              );
              workflows.setFavoriteName('');
              workflows.setShowFavoriteDialog(false);
            }}
          />
        </div>
        <QuerySidebarSection
          panelId={panelId}
          connectionId={connectionId}
          selectedDatabase={selectedDatabase ?? undefined}
          favoritesVisible={favoritesVisible}
          historyVisible={historyVisible}
        />
      </div>

      <QueryTransactionModals
        txUnclosedOpen={executionGate.txUnclosedOpen}
        txAbortedOpen={tx.txAbortedOpen}
        txAbortedDetail={tx.txAbortedDetail}
        txBusy={tx.txBusy}
        onConfirmUnclosedTx={() => void executionGate.handleConfirmUnclosedTx()}
        onCancelUnclosedTx={executionGate.handleCancelUnclosedTx}
        onAbortedRollback={tx.handleAbortedRollback}
        onAbortedSkip={tx.handleAbortedSkip}
      />

      {confirmRetryDialog}
      {executionGate.confirmDangerousDialog}
      {executionGate.executionStrategyAskModal}
      <ResultMessageDialog
        open={messageDialogOpen}
        kind={messageDialogKind}
        message={messageDialogText}
        onClose={() => setMessageDialogOpen(false)}
      />
    </div>
  );
}
