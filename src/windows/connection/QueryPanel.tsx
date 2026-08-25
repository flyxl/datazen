import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
} from 'react';
import {
  CirclePlay,
  AlertTriangle,
  BarChart3,
  Bookmark,
  Check,
  Clock,
  FileSearch,
  Gauge,
  Loader2,
  Play,
  Sparkles,
  Square,
  TableProperties,
  Trash2,
  Undo2,
  Wand2,
} from 'lucide-react';
import { listen } from '@tauri-apps/api/event';
import { Button } from '../../components/ui/Button';
import { ToolbarShell } from '../../components/ui/ToolbarShell';
import { ToolbarButton } from '../../components/ui/ToolbarButton';
import { SqlEditor } from '../../components/SqlEditor';
import type { SqlEditorHandle } from '../../components/SqlEditor';
import { buildEditorSchema } from '../../lib/buildEditorSchema';
import { findGroupForDatabase, groupQueryHistory } from '../../lib/historyGroups';
import { showNativeContextMenu } from '../../lib/nativeContextMenu';
import { buildSqlEditorContextMenuItems } from '../../lib/sqlEditorContextMenu';
import {
  buildFavoriteSidebarContextMenuItems,
  buildHistorySidebarContextMenuItems,
  buildHistorySidebarHeaderContextMenuItems,
} from '../../lib/querySidebarContextMenu';
import {
  inferDefaultSchema,
  inferDefaultTable,
  tablesReferencedInSql,
} from '../../lib/sqlEditorDefaults';
import {
  namespaceRootsFrom,
  pathsEqual,
  resolveQueryContextPath,
} from '../../lib/queryContextPath';
import { QueryContextSelectors } from '../../components/query/QueryContextSelectors';
import { QueryErrorPanel } from '../../components/query/QueryErrorPanel';
import { CopyableError } from '../../components/ui/CopyableError';
import { DataTable } from '../../components/DataTable/DataTable';
import type { ColumnDef } from '../../components/DataTable/TableHeader';
import { ChartView } from '../../components/chart/ChartView';
import { Nl2SqlPanel } from '../../components/ai/Nl2SqlPanel';
import { DiagnosisPanel } from '../../components/ai/DiagnosisPanel';
import { ExplainPanel } from '../../components/ai/ExplainPanel';
import { usePanelStore } from '../../stores/panelStore';
import { useQueryExec } from '../../hooks/useQueryExec';
import { useSchemaStore } from '../../stores/schemaStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useI18n } from '../../hooks/useI18n';
import { useResizable } from '../../hooks/useResizable';
import { useCompactToolbar } from '../../hooks/useCompactToolbar';
import { cn } from '../../lib/cn';
import { queryCommands } from '../../commands/query';
import { dashboardCommands } from '../../commands/dashboard';
import { openDashboardWindow } from '../../lib/windowManager';
import { emitCrossWindow } from '../../lib/crossWindowBus';
import { createEmptyDashboard } from '../dashboard/DashboardPanel';
import { AddToDashboardDialog } from '../dashboard/AddToDashboardDialog';
import { formatSql } from '../../lib/sqlFormat';
import { parseSqlParams, paramsToPayload } from '../../lib/sqlBindParams';
import { BindParamPanel } from '../../components/query/BindParamPanel';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import type { ExplainResult, StatementResult } from '../../types';
import { Dialog } from '../../components/ui/Dialog';
import { analyzeTransactionSql, isAbortedTransactionError } from '../../lib/sqlTransactionGuard';
import { formatLastConnected } from '../../lib/formatters';

interface QueryPanelProps {
  panelId: string;
  /** Live database session id used for every query this panel issues. */
  dbSessionId: string;
  /** Persistent saved-connection ID (stable across restarts). */
  connectionId: string;
  databaseType?: string;
}

function hasSuspiciousPostgresDoubleQuotedLiteral(sql: string): boolean {
  return /(?:=|<>|!=|\bLIKE\b|\bILIKE\b)\s*"[^"]+"/i.test(sql);
}

export function QueryPanel({ panelId, dbSessionId, connectionId, databaseType }: QueryPanelProps) {
  const { t } = useI18n();
  const exec = useQueryExec(panelId);
  const historyVisible = usePanelStore((s) => s.historyVisible);
  const history = usePanelStore((s) => s.queryHistory);
  const updateSql = usePanelStore((s) => s.updateSql);
  const setActiveResult = usePanelStore((s) => s.setActiveResult);
  const storeExecuteQuery = usePanelStore((s) => s.executeQuery);
  const storeExecuteSelection = usePanelStore((s) => s.executeSelection);
  const cancelQuery = usePanelStore((s) => s.cancelQuery);
  const loadHistory = usePanelStore((s) => s.loadHistory);
  const toggleHistory = usePanelStore((s) => s.toggleHistory);
  const favorites = usePanelStore((s) => s.queryFavorites);
  const favoritesVisible = usePanelStore((s) => s.favoritesVisible);
  const loadFavorites = usePanelStore((s) => s.loadFavorites);
  const storeAddFavorite = usePanelStore((s) => s.addFavorite);
  const deleteFavorite = usePanelStore((s) => s.deleteFavorite);
  const toggleFavorites = usePanelStore((s) => s.toggleFavorites);
  const setResultDetailRow = usePanelStore((s) => s.setResultDetailRow);
  const setChartConfig = usePanelStore((s) => s.setChartConfig);

  // AI entry points are always visible; panels handle unconfigured state internally

  const editorRef = useRef<SqlEditorHandle>(null);
  const pendingFavSqlRef = useRef('');
  const [favoriteName, setFavoriteName] = useState('');
  const [showFavoriteDialog, setShowFavoriteDialog] = useState(false);
  const [favoriteDialogSql, setFavoriteDialogSql] = useState('');
  const [nl2sqlVisible, setNl2sqlVisible] = useState(false);
  const [diagnosisVisible, setDiagnosisVisible] = useState(false);
  const [explainResult, setExplainResult] = useState<ExplainResult | null>(null);
  const [showCancel, setShowCancel] = useState(false);
  const [explainLoading, setExplainLoading] = useState(false);
  const [explainError, setExplainError] = useState<string | null>(null);
  const [showExplain, setShowExplain] = useState(false);
  const [paramValues, setParamValues] = useState<Record<string, string>>({});
  const [historySearch, setHistorySearch] = useState('');
  const [historyScopeMode, setHistoryScopeMode] = useState<'current' | 'all'>('current');
  const [inTransaction, setInTransaction] = useState(false);
  const [txBusy, setTxBusy] = useState(false);
  const [txUnclosedOpen, setTxUnclosedOpen] = useState(false);
  const [txAbortedOpen, setTxAbortedOpen] = useState(false);
  const [txAbortedDetail, setTxAbortedDetail] = useState<string | null>(null);
  const pendingExecuteRef = useRef<null | { kind: 'full' | 'selection'; sql?: string }>(null);
  const [addToDashboardOpen, setAddToDashboardOpen] = useState(false);
  const safeMode = useSettingsStore((s) => s.settings.safeMode);
  const autoCommit = useSettingsStore((s) => s.settings.autoCommit);
  const resultViewMode = exec.resultViewMode ?? 'table';
  const setResultViewModeStore = usePanelStore((s) => s.setResultViewMode);
  const setResultViewMode = useCallback(
    (mode: 'table' | 'chart') => {
      setResultViewModeStore(panelId, mode);
    },
    [panelId, setResultViewModeStore],
  );

  const { size: editorHeight, handleRef: editorResizeRef } = useResizable({
    direction: 'vertical',
    initialSize: 280,
    minSize: 100,
    maxSize: 900,
    storageKey: 'query-editor-height',
  });
  const { ref: toolbarRef, compact: compactToolbar } = useCompactToolbar();

  useEffect(() => {
    if (!exec.running) {
      setShowCancel(false);
      return;
    }
    const timer = window.setTimeout(() => setShowCancel(true), 300);
    return () => window.clearTimeout(timer);
  }, [exec.running]);

  const tables = useSchemaStore((s) => s.tables);
  const views = useSchemaStore((s) => s.views);
  const columnMap = useSchemaStore((s) => s.columnMap);
  const namespaceTree = useSchemaStore((s) => s.namespaceTree);
  const pathAliases = useSchemaStore((s) => s.pathAliases);
  const databases = useSchemaStore((s) => s.databases);
  const currentDatabase = useSchemaStore((s) => s.currentDatabase);
  const isMultiDb = useSchemaStore((s) => s.isMultiDatabase);
  const ensureColumns = useSchemaStore((s) => s.ensureColumns);
  const switchDatabase = useSchemaStore((s) => s.switchDatabase);
  const ensureNamespacePath = useSchemaStore((s) => s.ensureNamespacePath);
  const namespaceLoading = useSchemaStore((s) => s.ensuringCount > 0);

  const dbMeta = databaseType ? DB_REGISTRY[databaseType as keyof typeof DB_REGISTRY] : undefined;
  const supportsExplain = dbMeta?.supportsExplain === true;
  const isPathHierarchy = dbMeta?.namespaceEnsure === 'path-hierarchy';
  const [contextPath, setContextPath] = useState<string[]>([]);
  const editorSchema = useMemo(
    () =>
      buildEditorSchema({
        namespaceTree,
        tables,
        views,
        columnMap,
        currentDatabase,
        hoistPath: contextPath,
      }),
    [namespaceTree, tables, views, columnMap, currentDatabase, contextPath],
  );
  const editorDefaultSchema = useMemo(() => inferDefaultSchema(tables, views), [tables, views]);
  const editorDefaultTable = useMemo(() => inferDefaultTable(exec.sql), [exec.sql]);

  const ensureTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(
    () => () => {
      if (ensureTimer.current) clearTimeout(ensureTimer.current);
    },
    [],
  );

  useEffect(() => {
    void ensureNamespacePath([]);
  }, [dbSessionId, currentDatabase, ensureNamespacePath]);

  useEffect(() => {
    if (isPathHierarchy) return;
    setContextPath(currentDatabase ? [currentDatabase] : []);
  }, [currentDatabase, isPathHierarchy]);

  const applyContextPath = useCallback(
    async (next: string[]) => {
      setContextPath(next);
      if (isPathHierarchy) {
        if (next.length > 0) await ensureNamespacePath(next);
        return;
      }
      const db = next[0];
      if (db && db !== currentDatabase) {
        // Use switchDatabase (not loadTables) so the session + editor context
        // move to the target database without bumping schemaEpoch. Bumping
        // schemaEpoch treats the switch as a schema-wide change and would make
        // the sidebar (ConnectionNavigatorTree) wipe and reload every expanded
        // database, causing a full redraw and racing the session `useDatabase`
        // back to another database.
        await switchDatabase(db);
      }
    },
    [currentDatabase, ensureNamespacePath, isPathHierarchy, switchDatabase],
  );

  const handleQualifiedPath = useCallback(
    (parents: string[]) => {
      if (ensureTimer.current) clearTimeout(ensureTimer.current);
      ensureTimer.current = setTimeout(() => {
        void ensureNamespacePath(parents);
      }, 120);
      const roots = new Set(namespaceRootsFrom(namespaceTree, pathAliases, databases));
      if (parents[0] && roots.has(parents[0]) && !pathsEqual(parents, contextPath)) {
        void applyContextPath(parents);
      }
    },
    [applyContextPath, contextPath, databases, ensureNamespacePath, namespaceTree, pathAliases],
  );

  const syncContextFromSql = useCallback(
    async (sql: string) => {
      const resolved = resolveQueryContextPath(sql, {
        databases,
        namespaceRoots: namespaceRootsFrom(namespaceTree, pathAliases, databases),
      });
      if (!resolved || pathsEqual(resolved, contextPath)) return;
      await applyContextPath(resolved);
    },
    [applyContextPath, contextPath, databases, namespaceTree, pathAliases],
  );

  useEffect(() => {
    if (!exec.sql.trim()) return;
    const timer = setTimeout(() => {
      void syncContextFromSql(exec.sql);
    }, 50);
    return () => clearTimeout(timer);
  }, [exec.sql, syncContextFromSql]);

  useEffect(() => {
    void loadHistory(connectionId);
    void loadFavorites(connectionId);
  }, [connectionId, loadHistory, loadFavorites]);

  useEffect(() => {
    const names = tablesReferencedInSql(exec.sql);
    if (names.length === 0) return;
    const timer = setTimeout(() => {
      void ensureColumns(names);
    }, 120);
    return () => clearTimeout(timer);
  }, [exec.sql, ensureColumns, namespaceTree, tables, views]);

  const sqlParams = useMemo(() => parseSqlParams(exec.sql), [exec.sql]);
  const boundPayload = useMemo(
    () => (sqlParams.length > 0 ? paramsToPayload(sqlParams, paramValues) : undefined),
    [sqlParams, paramValues],
  );

  const refreshTxStatus = useCallback(async () => {
    try {
      setInTransaction(await queryCommands.sessionTransactionStatus(dbSessionId));
    } catch {
      setInTransaction(false);
    }
  }, [dbSessionId]);

  useEffect(() => {
    void refreshTxStatus();
  }, [refreshTxStatus]);

  const handleBeginTx = useCallback(async () => {
    setTxBusy(true);
    try {
      await queryCommands.beginSessionTransaction(dbSessionId);
      await refreshTxStatus();
    } catch (e) {
      console.warn(e);
    } finally {
      setTxBusy(false);
    }
  }, [dbSessionId, refreshTxStatus]);

  const handleCommitTx = useCallback(async () => {
    setTxBusy(true);
    try {
      await queryCommands.commitSessionTransaction(dbSessionId);
      await refreshTxStatus();
    } catch (e) {
      console.warn(e);
    } finally {
      setTxBusy(false);
    }
  }, [dbSessionId, refreshTxStatus]);

  const handleRollbackTx = useCallback(async () => {
    setTxBusy(true);
    try {
      await queryCommands.rollbackSessionTransaction(dbSessionId);
      await refreshTxStatus();
    } catch (e) {
      console.warn(e);
    } finally {
      setTxBusy(false);
    }
  }, [dbSessionId, refreshTxStatus]);

  const maybeOfferAbortedDialog = useCallback(
    async (error: string | null | undefined) => {
      await refreshTxStatus();
      const stillInTx = await queryCommands
        .sessionTransactionStatus(dbSessionId)
        .catch(() => false);
      if (stillInTx || isAbortedTransactionError(error)) {
        setTxAbortedDetail(error ?? null);
        setTxAbortedOpen(true);
      }
    },
    [dbSessionId, refreshTxStatus],
  );

  const runExecute = useCallback(
    async (kind: 'full' | 'selection', selectionSql?: string) => {
      const sqlToRun =
        kind === 'selection' && selectionSql != null
          ? selectionSql
          : editorRef.current?.getSelection()?.trim() || exec.sql;
      if (databaseType === 'postgresql' && hasSuspiciousPostgresDoubleQuotedLiteral(sqlToRun)) {
        window.alert(t('query.postgresDoubleQuoteHint'));
        return;
      }
      await syncContextFromSql(
        kind === 'selection' && selectionSql != null ? selectionSql : exec.sql,
      );
      if (!autoCommit && !inTransaction) {
        try {
          await queryCommands.beginSessionTransaction(dbSessionId);
          setInTransaction(true);
        } catch {
          /* driver may not support transactions; continue */
        }
      }
      if (kind === 'selection' && selectionSql != null) {
        await storeExecuteSelection(panelId, selectionSql, boundPayload);
      } else {
        const sel = editorRef.current?.getSelection()?.trim();
        if (sel) {
          await storeExecuteSelection(panelId, sel, boundPayload);
        } else {
          await storeExecuteQuery(panelId, boundPayload);
        }
      }
      const err = usePanelStore.getState().queryExec.get(panelId)?.error ?? null;
      if (err) {
        await maybeOfferAbortedDialog(err);
      } else {
        await refreshTxStatus();
      }
    },
    [
      exec.sql,
      databaseType,
      panelId,
      autoCommit,
      inTransaction,
      connectionId,
      storeExecuteSelection,
      storeExecuteQuery,
      boundPayload,
      maybeOfferAbortedDialog,
      refreshTxStatus,
      syncContextFromSql,
      t,
    ],
  );

  const requestExecute = useCallback(
    (kind: 'full' | 'selection', selectionSql?: string) => {
      const sqlForCheck =
        kind === 'selection' && selectionSql != null
          ? selectionSql
          : editorRef.current?.getSelection()?.trim() || exec.sql;
      if (analyzeTransactionSql(sqlForCheck).hasUnclosedBegin) {
        pendingExecuteRef.current = { kind, sql: selectionSql };
        setTxUnclosedOpen(true);
        return;
      }
      void runExecute(kind, selectionSql);
    },
    [exec.sql, runExecute],
  );

  const handleExecute = useCallback(() => {
    requestExecute('full');
  }, [requestExecute]);

  const handleExecuteSelection = useCallback(
    (sql: string) => {
      requestExecute('selection', sql);
    },
    [requestExecute],
  );

  const handleConfirmUnclosedTx = useCallback(() => {
    const pending = pendingExecuteRef.current;
    pendingExecuteRef.current = null;
    setTxUnclosedOpen(false);
    if (!pending) return;
    void runExecute(pending.kind, pending.sql);
  }, [runExecute]);

  const handleCancelUnclosedTx = useCallback(() => {
    pendingExecuteRef.current = null;
    setTxUnclosedOpen(false);
  }, []);

  const handleAbortedRollback = useCallback(async () => {
    setTxBusy(true);
    try {
      await queryCommands.rollbackSessionTransaction(dbSessionId);
      await refreshTxStatus();
    } catch (e) {
      console.warn(e);
    } finally {
      setTxBusy(false);
      setTxAbortedOpen(false);
      setTxAbortedDetail(null);
    }
  }, [dbSessionId, refreshTxStatus]);

  const handleAbortedSkip = useCallback(() => {
    setTxAbortedOpen(false);
    setTxAbortedDetail(null);
  }, []);

  const handleFormat = useCallback(() => {
    if (!exec.sql.trim()) return;
    try {
      updateSql(panelId, formatSql(exec.sql, databaseType));
    } catch {
      /* keep original SQL if formatter rejects dialect-specific syntax */
    }
  }, [exec.sql, panelId, databaseType, updateSql]);

  const handleCancel = useCallback(() => {
    void cancelQuery(panelId);
  }, [panelId, cancelQuery]);

  const handleApplyAiSql = useCallback(
    (sql: string) => {
      updateSql(panelId, sql);
    },
    [panelId, updateSql],
  );

  const handleExplain = useCallback(async () => {
    if (!exec.sql.trim()) return;
    setExplainLoading(true);
    setExplainError(null);
    setShowExplain(true);
    try {
      const result = await queryCommands.getExplain(dbSessionId, exec.sql, currentDatabase);
      setExplainResult(result);
    } catch (e) {
      setExplainResult(null);
      setExplainError(e instanceof Error ? e.message : String(e));
    } finally {
      setExplainLoading(false);
    }
  }, [dbSessionId, exec.sql, currentDatabase]);

  const openAddFavoriteDialog = useCallback((sql: string) => {
    const trimmed = sql.trim();
    if (!trimmed) return;
    pendingFavSqlRef.current = trimmed;
    setFavoriteDialogSql(trimmed);
    setFavoriteName('');
    setShowFavoriteDialog(true);
  }, []);

  const handleEditorContextMenu = useCallback(
    (e: MouseEvent, sqlText: string) => {
      pendingFavSqlRef.current = sqlText;
      const selection = editorRef.current?.getSelection() ?? '';
      const hasSelection = selection.length > 0;
      void showNativeContextMenu(
        buildSqlEditorContextMenuItems({
          labels: {
            run: t('query.run'),
            runSelection: t('query.runSelection'),
            format: t('query.format'),
            comment: t('query.comment'),
            addFavorite: t('query.addFavorite'),
          },
          handlers: {
            onRun: handleExecute,
            onRunSelection: () => {
              if (selection.trim()) handleExecuteSelection(selection);
            },
            onFormat: handleFormat,
            onComment: () => editorRef.current?.toggleLineComment(),
            onAddFavorite: openAddFavoriteDialog,
          },
          sqlText,
          hasSelection,
        }),
        { x: e.clientX, y: e.clientY },
      );
    },
    [openAddFavoriteDialog, t, handleExecute, handleExecuteSelection, handleFormat],
  );

  const copySqlToClipboard = useCallback((sql: string) => {
    void navigator.clipboard.writeText(sql);
  }, []);

  const handleFavoriteContextMenu = useCallback(
    (e: ReactMouseEvent, favorite: { id: string; sql: string }) => {
      e.preventDefault();
      e.stopPropagation();
      void showNativeContextMenu(
        buildFavoriteSidebarContextMenuItems({
          labels: {
            applySql: t('query.applySql'),
            copySql: t('query.copySql'),
            delete: t('common.delete'),
          },
          handlers: {
            onApplySql: () => updateSql(panelId, favorite.sql),
            onCopySql: () => copySqlToClipboard(favorite.sql),
            onDelete: () => {
              void deleteFavorite(favorite.id);
            },
          },
        }),
        { x: e.clientX, y: e.clientY },
      );
    },
    [panelId, t, updateSql, copySqlToClipboard, deleteFavorite],
  );

  const handleHistoryContextMenu = useCallback(
    (e: ReactMouseEvent, sql: string) => {
      e.preventDefault();
      e.stopPropagation();
      void showNativeContextMenu(
        buildHistorySidebarContextMenuItems({
          labels: {
            applySql: t('query.applySql'),
            copySql: t('query.copySql'),
          },
          handlers: {
            onApplySql: () => updateSql(panelId, sql),
            onCopySql: () => copySqlToClipboard(sql),
          },
        }),
        { x: e.clientX, y: e.clientY },
      );
    },
    [panelId, t, updateSql, copySqlToClipboard],
  );

  const handleHistoryHeaderContextMenu = useCallback(
    (e: ReactMouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      void showNativeContextMenu(
        buildHistorySidebarHeaderContextMenuItems({
          labels: { clearHistory: t('query.clearHistory') },
          handlers: {
            onClearHistory: () => {
              void (async () => {
                await queryCommands.clearQueryHistory();
                await loadHistory(connectionId);
              })();
            },
          },
        }),
        { x: e.clientX, y: e.clientY },
      );
    },
    [t, connectionId, loadHistory],
  );

  // Group by recorded session database; default scope shows only this panel's
  // database so applied entries re-run in context (no "table not exist").
  const historyGroups = useMemo(
    () => groupQueryHistory(history, t('query.historyUnknownDb')),
    [history, t],
  );
  const currentDbGroup = useMemo(
    () => findGroupForDatabase(historyGroups, currentDatabase),
    [historyGroups, currentDatabase],
  );
  // 'current' with no matching group falls back to all groups (single-db
  // drivers, or no history recorded for this database yet) — surface that.
  const historyScopeFallback = historyScopeMode === 'current' && !currentDbGroup;
  const scopedSections = useMemo(() => {
    const q = historySearch.trim().toLowerCase();
    const applyQ = (items: typeof history) =>
      q ? items.filter((h) => h.sql.toLowerCase().includes(q)) : items;
    if (historyScopeMode === 'current' && currentDbGroup) {
      return [
        {
          key: currentDbGroup.key,
          label: null as string | null,
          items: applyQ(currentDbGroup.entries),
        },
      ];
    }
    return historyGroups.map((g) => ({ key: g.key, label: g.label, items: applyQ(g.entries) }));
  }, [historyScopeMode, currentDbGroup, historyGroups, historySearch]);
  const filteredCount = scopedSections.reduce((n, s) => n + s.items.length, 0);

  const handleClearHistory = useCallback(() => {
    void (async () => {
      await queryCommands.clearQueryHistory();
      await loadHistory(connectionId);
    })();
  }, [connectionId, loadHistory]);

  // Keep event bridge for E2E / menubar emit compatibility.
  useEffect(() => {
    const unlisten = listen('menu:add-favorite', () => {
      const sql =
        pendingFavSqlRef.current || usePanelStore.getState().queryExec.get(panelId)?.sql || '';
      openAddFavoriteDialog(sql);
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [openAddFavoriteDialog, panelId]);

  const { results, activeResultIdx } = exec;
  const activeResult: StatementResult | undefined = results[activeResultIdx];

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* Toolbar */}
      <ToolbarShell ref={toolbarRef} className="h-9 flex-nowrap overflow-x-auto px-3">
        <QueryContextSelectors
          isMultiDb={isMultiDb}
          isPathHierarchy={isPathHierarchy}
          databases={databases}
          currentDatabase={currentDatabase}
          namespaceTree={namespaceTree}
          pathAliases={pathAliases}
          contextPath={contextPath}
          onSelectLevel={(index, value) => {
            void applyContextPath([...contextPath.slice(0, index), value]);
          }}
        />
        {exec.running && showCancel ? (
          <ToolbarButton
            compact={compactToolbar}
            variant="danger"
            label={t('query.stop')}
            icon={<Square className="h-3.5 w-3.5" />}
            onClick={handleCancel}
          />
        ) : (
          <ToolbarButton
            compact={compactToolbar}
            variant="run"
            label={t('query.execute')}
            icon={
              exec.running ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Play className="h-3.5 w-3.5" />
              )
            }
            onClick={handleExecute}
            disabled={exec.running}
          />
        )}
        {supportsExplain && (
          <ToolbarButton
            compact={compactToolbar}
            variant="ghost"
            label={t('explain.title')}
            icon={<FileSearch className="h-3.5 w-3.5" />}
            onClick={() => void handleExplain()}
            disabled={exec.running || !exec.sql.trim()}
          />
        )}
        <ToolbarButton
          compact={compactToolbar}
          variant="ghost"
          label={t('query.format')}
          icon={<Wand2 className="h-3.5 w-3.5" />}
          onClick={handleFormat}
          disabled={exec.running || !exec.sql.trim()}
        />
        <div className="mx-1 h-4 w-px shrink-0 bg-edge" />
        <ToolbarButton
          compact={compactToolbar}
          variant="ghost"
          label={t('query.beginTx')}
          icon={<CirclePlay className="h-3.5 w-3.5" />}
          onClick={() => void handleBeginTx()}
          disabled={exec.running || txBusy || inTransaction}
        />
        <ToolbarButton
          compact={compactToolbar}
          variant="ghost"
          label={t('query.commitTx')}
          icon={<Check className="h-3.5 w-3.5" />}
          onClick={() => void handleCommitTx()}
          disabled={exec.running || txBusy || !inTransaction}
        />
        <ToolbarButton
          compact={compactToolbar}
          variant="ghost"
          label={t('query.rollbackTx')}
          icon={<Undo2 className="h-3.5 w-3.5" />}
          onClick={() => void handleRollbackTx()}
          disabled={exec.running || txBusy || !inTransaction}
        />
        {inTransaction && (
          <span
            className="shrink-0 rounded bg-accent/15 px-1.5 py-0.5 text-[10px] font-medium text-accent"
            title={t('query.inTransaction')}
          >
            {compactToolbar ? 'TX' : t('query.inTransaction')}
          </span>
        )}
        {safeMode && (
          <span
            className="shrink-0 rounded bg-warning/15 px-1.5 py-0.5 text-[10px] font-medium text-warning"
            title={t('settings.safeMode')}
          >
            {compactToolbar ? 'Safe' : t('settings.safeMode')}
          </span>
        )}
        {!compactToolbar && (
          <span className="shrink-0 whitespace-nowrap text-[11px] text-fg-muted">
            ⌘+Enter {t('query.execute')}
          </span>
        )}
        <div className="min-w-0 flex-1" />
        {exec.executionTimeMs != null && (
          <span className="shrink-0 whitespace-nowrap text-[11px] text-fg-muted">
            {compactToolbar
              ? `${exec.executionTimeMs} ms`
              : `${t('query.totalTime')} ${exec.executionTimeMs} ms`}
          </span>
        )}
        <ToolbarButton
          compact={compactToolbar}
          variant={historyVisible ? 'secondary' : 'ghost'}
          label={t('query.history')}
          icon={<Clock className="h-3.5 w-3.5" />}
          onClick={toggleHistory}
        />
        <ToolbarButton
          compact={compactToolbar}
          variant={favoritesVisible ? 'secondary' : 'ghost'}
          label={t('query.favorites')}
          icon={<Bookmark className="h-3.5 w-3.5" />}
          onClick={toggleFavorites}
        />
        <ToolbarButton
          compact={compactToolbar}
          variant={nl2sqlVisible ? 'secondary' : 'ghost'}
          label={t('nl2sql.title')}
          icon={<Sparkles className="h-3.5 w-3.5" />}
          onClick={() => setNl2sqlVisible((v) => !v)}
        />
      </ToolbarShell>

      <BindParamPanel
        params={sqlParams}
        values={paramValues}
        onChange={(name, value) => setParamValues((prev) => ({ ...prev, [name]: value }))}
      />

      {/* Editor + results (vertical split) */}
      <div className="flex min-h-0 flex-1">
        <div className="flex min-w-0 flex-1 flex-col">
          {/* NL2SQL panel (collapsible, aligned with editor) */}
          {nl2sqlVisible && (
            <Nl2SqlPanel
              dbSessionId={dbSessionId}
              database={currentDatabase ?? ''}
              onSqlChange={handleApplyAiSql}
            />
          )}

          {/* SQL editor — height adjustable via bottom drag handle */}
          <div className="relative shrink-0 border-b border-edge" style={{ height: editorHeight }}>
            <SqlEditor
              ref={editorRef}
              value={exec.sql}
              onChange={(v) => updateSql(panelId, v)}
              onExecute={handleExecute}
              onExecuteSelection={handleExecuteSelection}
              onContextMenu={handleEditorContextMenu}
              onQualifiedPath={handleQualifiedPath}
              placeholder={t('query.placeholder')}
              schema={editorSchema}
              databaseType={databaseType}
              namespaceLoading={namespaceLoading}
              defaultSchema={editorDefaultSchema}
              defaultTable={editorDefaultTable}
            />
          </div>
          <div
            ref={editorResizeRef}
            className="h-1.5 shrink-0 cursor-row-resize bg-transparent hover:bg-accent/30 active:bg-accent/40"
            title="Drag to resize editor"
          />

          {showFavoriteDialog && (
            <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
              <div className="w-[400px] rounded-lg border border-edge bg-surface p-4 shadow-xl">
                <div className="mb-3 text-sm font-medium text-fg">{t('query.addFavorite')}</div>
                <div className="mb-2">
                  <label className="mb-1 block text-xs text-fg-muted">
                    {t('query.favoriteTitle')}
                  </label>
                  <input
                    type="text"
                    value={favoriteName}
                    onChange={(e) => setFavoriteName(e.target.value)}
                    placeholder={t('query.favoriteTitlePlaceholder')}
                    className="h-8 w-full rounded border border-edge bg-surface-alt px-2 text-sm text-fg focus:border-accent focus:outline-none"
                    autoFocus
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' && favoriteName.trim()) {
                        void storeAddFavorite(favoriteName.trim(), favoriteDialogSql, connectionId);
                        setFavoriteName('');
                        setShowFavoriteDialog(false);
                      }
                    }}
                  />
                </div>
                <div className="mb-3">
                  <label className="mb-1 block text-xs text-fg-muted">SQL</label>
                  <div className="max-h-[120px] overflow-auto rounded border border-edge bg-surface-alt p-2 font-mono text-xs text-fg-secondary">
                    {favoriteDialogSql}
                  </div>
                </div>
                <div className="flex justify-end gap-2">
                  <Button
                    variant="ghost"
                    className="h-7 px-3 text-xs"
                    onClick={() => setShowFavoriteDialog(false)}
                  >
                    {t('common.cancel')}
                  </Button>
                  <Button
                    variant="primary"
                    className="h-7 px-3 text-xs"
                    disabled={!favoriteName.trim()}
                    onClick={() => {
                      if (favoriteName.trim()) {
                        void storeAddFavorite(favoriteName.trim(), favoriteDialogSql, connectionId);
                        setFavoriteName('');
                        setShowFavoriteDialog(false);
                      }
                    }}
                  >
                    {t('common.save')}
                  </Button>
                </div>
              </div>
            </div>
          )}

          {/* Results area */}
          <div className="flex min-h-0 flex-1 flex-col">
            {/* EXPLAIN view */}
            {showExplain && !exec.running && (
              <div className="flex min-h-0 flex-1 flex-col">
                <div className="flex shrink-0 items-center gap-0 border-b border-edge bg-surface-alt px-1">
                  {results.length > 0 && (
                    <button
                      type="button"
                      className="relative px-3 py-1.5 text-xs text-fg-muted hover:text-fg-secondary transition-colors"
                      onClick={() => setShowExplain(false)}
                    >
                      {t('query.result')}
                    </button>
                  )}
                  <button
                    type="button"
                    className="relative px-3 py-1.5 text-xs text-fg font-medium transition-colors"
                  >
                    {t('explain.title')}
                    <span
                      className={cn('absolute bottom-0 left-0 right-0 h-0.5 bg-accent opacity-100')}
                    />
                  </button>
                </div>
                {explainLoading && (
                  <div className="flex flex-1 items-center justify-center gap-2 text-fg-muted">
                    <Loader2 className="h-5 w-5 animate-spin" />
                    {t('explain.loading')}
                  </div>
                )}
                {!explainLoading && explainError && (
                  <div className="p-4">
                    <CopyableError
                      message={explainError}
                      copyButton
                      className="rounded-md border border-red-500/20 bg-red-500/10 px-4 py-3 text-sm text-red-400"
                    />
                  </div>
                )}
                {!explainLoading && explainResult && (
                  <ExplainPanel
                    dbSessionId={dbSessionId}
                    sql={exec.sql}
                    explainOutput={explainResult.planText}
                    planJson={explainResult.planJson}
                    planTree={explainResult.planTree}
                    onApplySql={handleApplyAiSql}
                  />
                )}
              </div>
            )}

            {!showExplain && exec.running && results.length === 0 && (
              <div className="flex flex-1 items-center justify-center gap-2 text-fg-muted">
                <Loader2 className="h-5 w-5 animate-spin" />
                {t('query.executing')}
              </div>
            )}

            {!showExplain && exec.error && !exec.running && (
              <div className="flex-1 overflow-auto">
                <div className="p-4">
                  <QueryErrorPanel
                    message={exec.error}
                    onDiagnose={currentDatabase ? () => setDiagnosisVisible(true) : undefined}
                  />
                </div>
                {diagnosisVisible && currentDatabase && (
                  <DiagnosisPanel
                    dbSessionId={dbSessionId}
                    database={currentDatabase}
                    sql={exec.sql}
                    errorMessage={exec.error}
                    onApplySql={handleApplyAiSql}
                    onClose={() => setDiagnosisVisible(false)}
                  />
                )}
              </div>
            )}

            {!showExplain && !exec.error && results.length > 0 && (
              <>
                {/* Result tabs */}
                {(results.length > 1 || explainResult) && (
                  <div className="flex shrink-0 items-center gap-0 border-b border-edge bg-surface-alt px-1">
                    {results.map((r, idx) => (
                      <button
                        key={idx}
                        type="button"
                        className={cn(
                          'relative px-3 py-1.5 text-xs transition-colors',
                          idx === activeResultIdx
                            ? 'text-fg font-medium'
                            : 'text-fg-muted hover:text-fg-secondary',
                        )}
                        onClick={() => setActiveResult(panelId, idx)}
                      >
                        {t('query.result')} {idx + 1}
                        <span className="ml-1.5 text-[10px] text-fg-muted">
                          ({r.rows.length} {t('common.rows')}
                          {exec.running ? '' : `, ${r.executionTimeMs}ms`})
                        </span>
                        <span
                          className={cn(
                            'absolute bottom-0 left-0 right-0 h-0.5 bg-accent transition-opacity duration-300',
                            idx === activeResultIdx ? 'opacity-100' : 'opacity-0',
                          )}
                        />
                      </button>
                    ))}
                    {explainResult && (
                      <button
                        type="button"
                        className="relative px-3 py-1.5 text-xs text-fg-muted hover:text-fg-secondary transition-colors"
                        onClick={() => setShowExplain(true)}
                      >
                        {t('explain.title')}
                      </button>
                    )}
                  </div>
                )}

                {/* View mode toggle + active result */}
                {activeResult && (
                  <>
                    {exec.running && (
                      <div className="flex shrink-0 items-center gap-2 border-b border-edge bg-surface-alt px-3 py-1.5 text-xs text-fg-muted">
                        <Loader2 className="h-3 w-3 animate-spin" />
                        {t('query.streamingRows', { n: String(activeResult.rows.length) })}
                      </div>
                    )}
                    <div className="flex shrink-0 items-center border-b border-edge bg-surface-alt px-2">
                      <div className="flex items-center gap-0.5 rounded-md bg-surface p-0.5 my-1">
                        <button
                          type="button"
                          className={cn(
                            'flex items-center gap-1 rounded px-2 py-0.5 text-xs transition-colors',
                            resultViewMode === 'table'
                              ? 'bg-accent/20 text-accent font-medium'
                              : 'text-fg-muted hover:text-fg-secondary',
                          )}
                          onClick={() => setResultViewMode('table')}
                        >
                          <TableProperties className="h-3 w-3" />
                          {t('chart.viewTable')}
                        </button>
                        <button
                          type="button"
                          className={cn(
                            'flex items-center gap-1 rounded px-2 py-0.5 text-xs transition-colors',
                            resultViewMode === 'chart'
                              ? 'bg-accent/20 text-accent font-medium'
                              : 'text-fg-muted hover:text-fg-secondary',
                          )}
                          onClick={() => setResultViewMode('chart')}
                        >
                          <BarChart3 className="h-3 w-3" />
                          {t('chart.viewChart')}
                        </button>
                      </div>
                      {resultViewMode === 'chart' && activeResult.rows.length > 1000 && (
                        <span className="ml-2 flex items-center gap-1 text-[11px] text-yellow-400">
                          <AlertTriangle className="h-3 w-3" />
                          {t('chart.sampledWarning', { limit: '1000' })}
                        </span>
                      )}
                      <Button
                        variant="ghost"
                        className="ml-auto h-7 gap-1 px-2 text-xs"
                        data-testid="query-add-to-dashboard"
                        disabled={!activeResult.rows.length}
                        onClick={() => setAddToDashboardOpen(true)}
                      >
                        <Gauge className="h-3 w-3" />
                        {t('dashboard.addToDashboard')}
                      </Button>
                    </div>
                    {exec.running || resultViewMode === 'table' ? (
                      <ResultTable result={activeResult} panelId={panelId} />
                    ) : (
                      <ChartView
                        key={panelId}
                        result={activeResult}
                        savedConfig={exec.chartConfig}
                        onConfigChange={(cfg) => setChartConfig(panelId, cfg)}
                        onDataPointClick={(rowIndex) => {
                          setResultViewMode('table');
                          setResultDetailRow(panelId, rowIndex);
                        }}
                      />
                    )}
                  </>
                )}
              </>
            )}

            {!showExplain && results.length === 0 && !exec.running && !exec.error && (
              <div className="flex flex-1 items-center justify-center text-sm text-fg-muted">
                {t('query.shortcutHint')}
              </div>
            )}
          </div>
        </div>

        {/* History panel */}
        {favoritesVisible && (
          <aside className="w-64 shrink-0 overflow-y-auto border-l border-edge bg-surface-alt">
            <div className="border-b border-edge px-3 py-2 text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
              {t('query.favoritesTitle')}
            </div>
            {favorites.length === 0 ? (
              <div className="px-3 py-4 text-center text-xs text-fg-muted">
                {t('query.noFavorites')}
              </div>
            ) : (
              favorites.map((f) => (
                <div
                  key={f.id}
                  className="group flex w-full items-start border-b border-edge px-3 py-2 hover:bg-surface-raised"
                  onContextMenu={(e) => handleFavoriteContextMenu(e, f)}
                >
                  <button
                    type="button"
                    className="min-w-0 flex-1 text-left"
                    onClick={() => updateSql(panelId, f.sql)}
                  >
                    <div className="truncate text-xs font-medium text-fg">{f.title}</div>
                    <div className="mt-0.5 truncate font-mono text-[11px] text-fg-muted">
                      {f.sql}
                    </div>
                  </button>
                  <button
                    type="button"
                    className="ml-1 shrink-0 p-1 text-fg-muted opacity-0 hover:text-red-400 group-hover:opacity-100"
                    onClick={() => void deleteFavorite(f.id)}
                  >
                    <Trash2 className="h-3 w-3" />
                  </button>
                </div>
              ))
            )}
          </aside>
        )}
        {historyVisible && (
          <aside className="w-64 shrink-0 overflow-y-auto border-l border-edge bg-surface-alt">
            <div
              className="flex items-center justify-between border-b border-edge px-3 py-2"
              onContextMenu={handleHistoryHeaderContextMenu}
            >
              <span className="text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
                {t('query.historyTitle')}
              </span>
              {history.length > 0 && (
                <button
                  type="button"
                  className="p-1 text-fg-muted hover:text-red-400"
                  title={t('query.clearHistory')}
                  aria-label={t('query.clearHistory')}
                  onClick={handleClearHistory}
                >
                  <Trash2 className="h-3 w-3" />
                </button>
              )}
            </div>
            {history.length > 0 && (
              <div className="border-b border-edge px-2 py-1.5">
                <input
                  type="search"
                  value={historySearch}
                  onChange={(e) => setHistorySearch(e.target.value)}
                  placeholder={t('query.searchHistory')}
                  className="w-full rounded border border-edge bg-surface px-2 py-1 text-xs text-fg placeholder:text-fg-muted focus:border-accent focus:outline-none"
                  aria-label={t('query.searchHistory')}
                />
              </div>
            )}
            {history.length > 0 && (
              <div className="flex gap-1 border-b border-edge px-2 py-1.5">
                <button
                  type="button"
                  data-testid="history-scope-current"
                  aria-pressed={historyScopeMode === 'current'}
                  onClick={() => setHistoryScopeMode('current')}
                  className={`rounded px-2 py-0.5 text-[11px] ${historyScopeMode === 'current' ? 'bg-accent text-white' : 'border border-edge text-fg-muted hover:text-fg'}`}
                >
                  {t('query.historyScopeCurrent')}
                </button>
                <button
                  type="button"
                  data-testid="history-scope-all"
                  aria-pressed={historyScopeMode === 'all'}
                  onClick={() => setHistoryScopeMode('all')}
                  className={`rounded px-2 py-0.5 text-[11px] ${historyScopeMode === 'all' ? 'bg-accent text-white' : 'border border-edge text-fg-muted hover:text-fg'}`}
                >
                  {t('query.historyScopeAll')}
                </button>
              </div>
            )}
            {history.length > 0 && historyScopeFallback && (
              <div
                data-testid="history-scope-fallback-hint"
                className="border-b border-edge px-3 py-1.5 text-[11px] text-fg-muted"
              >
                {t('query.historyScopeFallbackHint')}
              </div>
            )}
            {history.length === 0 ? (
              <div className="px-3 py-4 text-center text-xs text-fg-muted">
                {t('query.noHistory')}
              </div>
            ) : filteredCount === 0 ? (
              <div className="px-3 py-4 text-center text-xs text-fg-muted">
                {t('query.noHistoryMatch')}
              </div>
            ) : (
              <>
                {historyScopeMode === 'current' && currentDbGroup && (
                  <div className="border-b border-edge px-3 py-1.5 text-[11px] text-fg-muted">
                    {t('query.database')}:
                    <span className="ml-1 font-medium text-fg">{currentDbGroup.label}</span>
                  </div>
                )}
                {scopedSections.map((section) => (
                  <div key={section.key}>
                    {section.label && (
                      <div
                        data-testid="history-group-label"
                        className="sticky top-0 z-10 border-b border-edge bg-surface-alt px-3 py-1 text-[11px] font-semibold text-fg-muted"
                      >
                        {section.label} ({section.items.length})
                      </div>
                    )}
                    {section.items.map((h) => (
                      <button
                        key={h.id}
                        type="button"
                        className="w-full border-b border-edge px-3 py-2 text-left hover:bg-surface-raised"
                        onClick={() => updateSql(panelId, h.sql)}
                        onContextMenu={(e) => handleHistoryContextMenu(e, h.sql)}
                      >
                        <div className="selectable truncate font-mono text-xs text-fg-secondary">
                          {h.sql}
                        </div>
                        <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[11px] text-fg-muted">
                          <span className={h.success ? 'text-green-400' : 'text-red-400'}>
                            {h.success ? t('common.success') : t('common.failed')}
                          </span>
                          <span>{h.executionTimeMs}ms</span>
                          {h.rowsAffected != null && (
                            <span>{t('query.historyRows', { count: h.rowsAffected })}</span>
                          )}
                          <span>{formatLastConnected(h.executedAt)}</span>
                        </div>
                      </button>
                    ))}
                  </div>
                ))}
              </>
            )}
          </aside>
        )}
      </div>

      <Dialog
        open={txUnclosedOpen}
        title={t('query.txUnclosedTitle')}
        description={t('query.txUnclosedBody')}
        onClose={handleCancelUnclosedTx}
        footer={
          <>
            <Button variant="ghost" onClick={handleCancelUnclosedTx}>
              {t('query.txUnclosedCancel')}
            </Button>
            <Button onClick={handleConfirmUnclosedTx}>{t('query.txUnclosedConfirm')}</Button>
          </>
        }
      >
        <p className="text-xs text-fg-muted">{t('query.inTransaction')}</p>
      </Dialog>

      <Dialog
        open={txAbortedOpen}
        title={t('query.txAbortedTitle')}
        description={t('query.txAbortedBody')}
        onClose={handleAbortedSkip}
        footer={
          <>
            <Button variant="ghost" onClick={handleAbortedSkip} disabled={txBusy}>
              {t('query.txAbortedSkip')}
            </Button>
            <Button variant="danger" onClick={() => void handleAbortedRollback()} disabled={txBusy}>
              {t('query.txAbortedRollback')}
            </Button>
          </>
        }
      >
        {txAbortedDetail ? (
          <pre className="copyable max-h-40 overflow-auto whitespace-pre-wrap break-words rounded border border-edge bg-surface p-2 font-mono text-[11px] text-red-400">
            {txAbortedDetail}
          </pre>
        ) : null}
      </Dialog>

      <AddToDashboardDialog
        open={addToDashboardOpen}
        onClose={() => setAddToDashboardOpen(false)}
        onConfirm={(dashboardId, newName) => {
          void (async () => {
            if (!exec.sql.trim() || !activeResult?.rows.length) return;
            setAddToDashboardOpen(false);
            try {
              let targetId = dashboardId;
              if (dashboardId === 'new') {
                const board = createEmptyDashboard(newName?.trim() || t('dashboard.defaultName'));
                await dashboardCommands.saveDashboard(board);
                targetId = board.id;
              }
              const created = await dashboardCommands.createWidgetFromSql({
                dashboardId: targetId,
                connectionId,
                sql: exec.sql,
                title:
                  (
                    usePanelStore.getState().panels.find((p) => p.id === panelId) as
                      | import('../../stores/panelStore').QueryPanel
                      | undefined
                  )?.title || undefined,
                viewMode: resultViewMode,
                chartConfig: exec.chartConfig,
              });
              void emitCrossWindow('dashboard:changed', { dashboardId: created.dashboard.id });
              openDashboardWindow(created.dashboard.id, created.dashboard.name);
            } catch (e) {
              window.alert(String(e));
            }
          })();
        }}
      />
    </div>
  );
}

function ResultTable({ result, panelId }: { result: StatementResult; panelId: string }) {
  const { t } = useI18n();
  const queryResultLimit = useSettingsStore((s) => s.settings.queryResultLimit);
  const setResultDetailRow = usePanelStore((s) => s.setResultDetailRow);
  const resultDetailRowIndex = usePanelStore(
    (s) => s.queryExec.get(panelId)?.resultDetailRowIndex ?? null,
  );

  const [editingCell, setEditingCell] = useState<{ row: number; col: string } | null>(null);

  const columnDefs: ColumnDef[] = useMemo(
    () => result.columns.map((c) => ({ id: c.name, name: c.name, type: c.dataType })),
    [result.columns],
  );

  const statusBar = useMemo(
    () => (
      <div className="flex items-center gap-3 border-b border-edge bg-surface-alt px-3 py-1.5 text-xs text-fg-secondary">
        <span>
          {result.rows.length} {t('common.rows')}
        </span>
        <span className="text-edge">|</span>
        <span>
          {result.columns.length} {t('common.columns')}
        </span>
        <span className="text-edge">|</span>
        <span>{result.executionTimeMs} ms</span>
        {result.sql && (
          <>
            <span className="text-edge">|</span>
            <span className="max-w-[400px] truncate font-mono text-fg-muted" title={result.sql}>
              {result.sql}
            </span>
          </>
        )}
        {result.truncated && (
          <>
            <span className="text-edge">|</span>
            <span className="flex items-center gap-1 text-yellow-400">
              <AlertTriangle className="h-3 w-3" />
              {t('query.resultTruncated', { limit: queryResultLimit })}
            </span>
          </>
        )}
      </div>
    ),
    [result, queryResultLimit, t],
  );

  const handleCellDoubleClick = useCallback(
    (row: number, col: string) => {
      setResultDetailRow(panelId, row);
      setEditingCell({ row, col });
    },
    [panelId, setResultDetailRow],
  );

  return (
    <DataTable
      columns={columnDefs}
      rows={result.rows}
      statusBar={statusBar}
      rowHeight={32}
      editingCell={editingCell}
      onCellDoubleClick={handleCellDoubleClick}
      onCellEdit={(_row, _col, _value) => setEditingCell(null)}
      onCellEditCancel={() => setEditingCell(null)}
      enableSetNull={false}
      onRowClick={(idx) => setResultDetailRow(panelId, idx)}
      highlightedRow={resultDetailRowIndex}
      exportTableName="query_result"
    />
  );
}
