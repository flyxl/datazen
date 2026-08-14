import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
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
import { SqlEditor } from '../../components/SqlEditor';
import type { SqlEditorHandle } from '../../components/SqlEditor';
import { buildEditorSchema } from '../../lib/buildEditorSchema';
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
import { DataTable } from '../../components/DataTable/DataTable';
import type { ColumnDef } from '../../components/DataTable/TableHeader';
import { ChartView } from '../../components/chart/ChartView';
import { Nl2SqlPanel } from '../../components/ai/Nl2SqlPanel';
import { DiagnosisPanel } from '../../components/ai/DiagnosisPanel';
import { ExplainPanel } from '../../components/ai/ExplainPanel';
import { useQueryStore } from '../../stores/queryStore';
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
import { createEmptyDashboard } from '../dashboard/DashboardWindow';
import { AddToDashboardDialog } from '../dashboard/AddToDashboardDialog';
import { formatSql } from '../../lib/sqlFormat';
import { parseSqlParams, paramsToPayload } from '../../lib/sqlBindParams';
import { BindParamPanel } from '../../components/query/BindParamPanel';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import type { ExplainResult, StatementResult } from '../../types';
import type { ButtonProps } from '../../components/ui/Button';
import { Dialog } from '../../components/ui/Dialog';
import { analyzeTransactionSql, isAbortedTransactionError } from '../../lib/sqlTransactionGuard';

interface QueryPanelProps {
  connectionId: string;
  queryTabId: string;
  databaseType?: string;
}

interface ToolbarButtonProps extends ButtonProps {
  compact: boolean;
  label: string;
  icon: ReactNode;
}

function ToolbarButton({ compact, label, icon, className, title, ...props }: ToolbarButtonProps) {
  return (
    <Button
      {...props}
      title={title ?? label}
      aria-label={label}
      className={cn('shrink-0', compact ? 'h-7 px-1.5' : 'h-7 gap-1 px-2 text-xs', className)}
    >
      {icon}
      <span className={cn(compact && 'sr-only')}>{label}</span>
    </Button>
  );
}

export function QueryPanel({ connectionId, queryTabId, databaseType }: QueryPanelProps) {
  const { t } = useI18n();
  const tab = useQueryStore((s) => s.tabs.find((t) => t.id === queryTabId));
  const historyVisible = useQueryStore((s) => s.historyVisible);
  const history = useQueryStore((s) => s.history);
  const setConnectionId = useQueryStore((s) => s.setConnectionId);
  const updateSql = useQueryStore((s) => s.updateSql);
  const setActiveResult = useQueryStore((s) => s.setActiveResult);
  const executeQuery = useQueryStore((s) => s.executeQuery);
  const executeSelection = useQueryStore((s) => s.executeSelection);
  const cancelQuery = useQueryStore((s) => s.cancelQuery);
  const loadHistory = useQueryStore((s) => s.loadHistory);
  const toggleHistory = useQueryStore((s) => s.toggleHistory);
  const favorites = useQueryStore((s) => s.favorites);
  const favoritesVisible = useQueryStore((s) => s.favoritesVisible);
  const loadFavorites = useQueryStore((s) => s.loadFavorites);
  const addFavorite = useQueryStore((s) => s.addFavorite);
  const deleteFavorite = useQueryStore((s) => s.deleteFavorite);
  const toggleFavorites = useQueryStore((s) => s.toggleFavorites);
  const setResultDetailRow = useQueryStore((s) => s.setResultDetailRow);
  const setChartConfig = useQueryStore((s) => s.setChartConfig);

  // AI entry points are always visible; panels handle unconfigured state internally

  const editorRef = useRef<SqlEditorHandle>(null);
  const pendingFavSqlRef = useRef('');
  const [favoriteName, setFavoriteName] = useState('');
  const [showFavoriteDialog, setShowFavoriteDialog] = useState(false);
  const [favoriteDialogSql, setFavoriteDialogSql] = useState('');
  const [nl2sqlVisible, setNl2sqlVisible] = useState(false);
  const [diagnosisVisible, setDiagnosisVisible] = useState(false);
  const [explainResult, setExplainResult] = useState<ExplainResult | null>(null);
  const [explainLoading, setExplainLoading] = useState(false);
  const [explainError, setExplainError] = useState<string | null>(null);
  const [showExplain, setShowExplain] = useState(false);
  const [paramValues, setParamValues] = useState<Record<string, string>>({});
  const [inTransaction, setInTransaction] = useState(false);
  const [txBusy, setTxBusy] = useState(false);
  const [txUnclosedOpen, setTxUnclosedOpen] = useState(false);
  const [txAbortedOpen, setTxAbortedOpen] = useState(false);
  const [txAbortedDetail, setTxAbortedDetail] = useState<string | null>(null);
  const pendingExecuteRef = useRef<null | { kind: 'full' | 'selection'; sql?: string }>(null);
  const [addToDashboardOpen, setAddToDashboardOpen] = useState(false);
  const safeMode = useSettingsStore((s) => s.settings.safeMode);
  const autoCommit = useSettingsStore((s) => s.settings.autoCommit);
  const resultViewMode = tab?.resultViewMode ?? 'table';
  const setResultViewModeStore = useQueryStore((s) => s.setResultViewMode);
  const setResultViewMode = useCallback(
    (mode: 'table' | 'chart') => {
      if (tab) setResultViewModeStore(tab.id, mode);
    },
    [tab, setResultViewModeStore],
  );

  const { size: editorHeight, handleRef: editorResizeRef } = useResizable({
    direction: 'vertical',
    initialSize: 280,
    minSize: 100,
    maxSize: 900,
    storageKey: 'query-editor-height',
  });
  const { ref: toolbarRef, compact: compactToolbar } = useCompactToolbar();

  const tables = useSchemaStore((s) => s.tables);
  const views = useSchemaStore((s) => s.views);
  const columnMap = useSchemaStore((s) => s.columnMap);
  const namespaceTree = useSchemaStore((s) => s.namespaceTree);
  const pathAliases = useSchemaStore((s) => s.pathAliases);
  const databases = useSchemaStore((s) => s.databases);
  const currentDatabase = useSchemaStore((s) => s.currentDatabase);
  const isMultiDb = useSchemaStore((s) => s.isMultiDatabase);
  const ensureColumns = useSchemaStore((s) => s.ensureColumns);
  const loadTables = useSchemaStore((s) => s.loadTables);
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
  const editorDefaultTable = useMemo(() => inferDefaultTable(tab?.sql ?? ''), [tab?.sql]);

  const ensureTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(
    () => () => {
      if (ensureTimer.current) clearTimeout(ensureTimer.current);
    },
    [],
  );

  useEffect(() => {
    void ensureNamespacePath([]);
  }, [connectionId, currentDatabase, ensureNamespacePath]);

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
      if (db && db !== currentDatabase) await loadTables(db);
    },
    [currentDatabase, ensureNamespacePath, isPathHierarchy, loadTables],
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
    const sql = tab?.sql ?? '';
    if (!sql.trim()) return;
    const timer = setTimeout(() => {
      void syncContextFromSql(sql);
    }, 50);
    return () => clearTimeout(timer);
  }, [tab?.sql, syncContextFromSql]);

  useEffect(() => {
    setConnectionId(connectionId);
    void loadHistory();
    void loadFavorites();
  }, [connectionId, setConnectionId, loadHistory, loadFavorites]);

  useEffect(() => {
    useSchemaStore.setState({ connectionId, databaseType: databaseType ?? null });
  }, [connectionId, databaseType]);

  useEffect(() => {
    const names = tablesReferencedInSql(tab?.sql ?? '');
    if (names.length === 0) return;
    const timer = setTimeout(() => {
      void ensureColumns(names);
    }, 120);
    return () => clearTimeout(timer);
  }, [tab?.sql, ensureColumns, namespaceTree, tables, views]);

  const sqlParams = useMemo(() => parseSqlParams(tab?.sql ?? ''), [tab?.sql]);
  const boundPayload = useMemo(
    () => (sqlParams.length > 0 ? paramsToPayload(sqlParams, paramValues) : undefined),
    [sqlParams, paramValues],
  );

  const refreshTxStatus = useCallback(async () => {
    try {
      setInTransaction(await queryCommands.sessionTransactionStatus(connectionId));
    } catch {
      setInTransaction(false);
    }
  }, [connectionId]);

  useEffect(() => {
    void refreshTxStatus();
  }, [refreshTxStatus]);

  const handleBeginTx = useCallback(async () => {
    setTxBusy(true);
    try {
      await queryCommands.beginSessionTransaction(connectionId);
      await refreshTxStatus();
    } catch (e) {
      console.warn(e);
    } finally {
      setTxBusy(false);
    }
  }, [connectionId, refreshTxStatus]);

  const handleCommitTx = useCallback(async () => {
    setTxBusy(true);
    try {
      await queryCommands.commitSessionTransaction(connectionId);
      await refreshTxStatus();
    } catch (e) {
      console.warn(e);
    } finally {
      setTxBusy(false);
    }
  }, [connectionId, refreshTxStatus]);

  const handleRollbackTx = useCallback(async () => {
    setTxBusy(true);
    try {
      await queryCommands.rollbackSessionTransaction(connectionId);
      await refreshTxStatus();
    } catch (e) {
      console.warn(e);
    } finally {
      setTxBusy(false);
    }
  }, [connectionId, refreshTxStatus]);

  const maybeOfferAbortedDialog = useCallback(
    async (error: string | null | undefined) => {
      await refreshTxStatus();
      const stillInTx = await queryCommands
        .sessionTransactionStatus(connectionId)
        .catch(() => false);
      if (stillInTx || isAbortedTransactionError(error)) {
        setTxAbortedDetail(error ?? null);
        setTxAbortedOpen(true);
      }
    },
    [connectionId, refreshTxStatus],
  );

  const runExecute = useCallback(
    async (kind: 'full' | 'selection', selectionSql?: string) => {
      if (!tab) return;
      await syncContextFromSql(
        kind === 'selection' && selectionSql != null ? selectionSql : tab.sql,
      );
      if (!autoCommit && !inTransaction) {
        try {
          await queryCommands.beginSessionTransaction(connectionId);
          setInTransaction(true);
        } catch {
          /* driver may not support transactions; continue */
        }
      }
      if (kind === 'selection' && selectionSql != null) {
        await executeSelection(tab.id, selectionSql, boundPayload);
      } else {
        const sel = editorRef.current?.getSelection()?.trim();
        if (sel) {
          await executeSelection(tab.id, sel, boundPayload);
        } else {
          await executeQuery(tab.id, boundPayload);
        }
      }
      const err = useQueryStore.getState().tabs.find((item) => item.id === tab.id)?.error ?? null;
      if (err) {
        await maybeOfferAbortedDialog(err);
      } else {
        await refreshTxStatus();
      }
    },
    [
      tab,
      autoCommit,
      inTransaction,
      connectionId,
      executeSelection,
      executeQuery,
      boundPayload,
      maybeOfferAbortedDialog,
      refreshTxStatus,
      syncContextFromSql,
    ],
  );

  const requestExecute = useCallback(
    (kind: 'full' | 'selection', selectionSql?: string) => {
      if (!tab) return;
      const sqlForCheck =
        kind === 'selection' && selectionSql != null
          ? selectionSql
          : editorRef.current?.getSelection()?.trim() || tab.sql;
      if (analyzeTransactionSql(sqlForCheck).hasUnclosedBegin) {
        pendingExecuteRef.current = { kind, sql: selectionSql };
        setTxUnclosedOpen(true);
        return;
      }
      void runExecute(kind, selectionSql);
    },
    [tab, runExecute],
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
      await queryCommands.rollbackSessionTransaction(connectionId);
      await refreshTxStatus();
    } catch (e) {
      console.warn(e);
    } finally {
      setTxBusy(false);
      setTxAbortedOpen(false);
      setTxAbortedDetail(null);
    }
  }, [connectionId, refreshTxStatus]);

  const handleAbortedSkip = useCallback(() => {
    setTxAbortedOpen(false);
    setTxAbortedDetail(null);
  }, []);

  const handleFormat = useCallback(() => {
    if (!tab?.sql.trim()) return;
    try {
      updateSql(tab.id, formatSql(tab.sql, databaseType));
    } catch {
      /* keep original SQL if formatter rejects dialect-specific syntax */
    }
  }, [tab, databaseType, updateSql]);

  const handleCancel = useCallback(() => {
    if (tab) void cancelQuery(tab.id);
  }, [tab, cancelQuery]);

  const handleApplyAiSql = useCallback(
    (sql: string) => {
      if (tab) updateSql(tab.id, sql);
    },
    [tab, updateSql],
  );

  const handleExplain = useCallback(async () => {
    if (!tab?.sql.trim()) return;
    setExplainLoading(true);
    setExplainError(null);
    setShowExplain(true);
    try {
      const result = await queryCommands.getExplain(connectionId, tab.sql);
      setExplainResult(result);
    } catch (e) {
      setExplainResult(null);
      setExplainError(e instanceof Error ? e.message : String(e));
    } finally {
      setExplainLoading(false);
    }
  }, [connectionId, tab]);

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
      if (!tab) return;
      void showNativeContextMenu(
        buildFavoriteSidebarContextMenuItems({
          labels: {
            applySql: t('query.applySql'),
            copySql: t('query.copySql'),
            delete: t('common.delete'),
          },
          handlers: {
            onApplySql: () => updateSql(tab.id, favorite.sql),
            onCopySql: () => copySqlToClipboard(favorite.sql),
            onDelete: () => {
              void deleteFavorite(favorite.id);
            },
          },
        }),
        { x: e.clientX, y: e.clientY },
      );
    },
    [tab, t, updateSql, copySqlToClipboard, deleteFavorite],
  );

  const handleHistoryContextMenu = useCallback(
    (e: ReactMouseEvent, sql: string) => {
      e.preventDefault();
      e.stopPropagation();
      if (!tab) return;
      void showNativeContextMenu(
        buildHistorySidebarContextMenuItems({
          labels: {
            applySql: t('query.applySql'),
            copySql: t('query.copySql'),
          },
          handlers: {
            onApplySql: () => updateSql(tab.id, sql),
            onCopySql: () => copySqlToClipboard(sql),
          },
        }),
        { x: e.clientX, y: e.clientY },
      );
    },
    [tab, t, updateSql, copySqlToClipboard],
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
                await loadHistory();
              })();
            },
          },
        }),
        { x: e.clientX, y: e.clientY },
      );
    },
    [t, loadHistory],
  );

  // Keep event bridge for E2E / menubar emit compatibility.
  useEffect(() => {
    const unlisten = listen('menu:add-favorite', () => {
      const sql =
        pendingFavSqlRef.current ||
        useQueryStore.getState().tabs.find((t) => t.id === queryTabId)?.sql ||
        '';
      openAddFavoriteDialog(sql);
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [openAddFavoriteDialog, queryTabId]);

  if (!tab) return null;

  const { results, activeResultIdx } = tab;
  const activeResult: StatementResult | undefined = results[activeResultIdx];

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* Toolbar */}
      <div
        ref={toolbarRef}
        className="flex h-9 shrink-0 flex-nowrap items-center gap-2 overflow-x-auto border-b border-edge bg-surface-alt px-3"
      >
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
        <ToolbarButton
          compact={compactToolbar}
          variant="primary"
          label={t('query.execute')}
          icon={<Play className="h-3.5 w-3.5" />}
          onClick={handleExecute}
          disabled={tab.running}
        />
        {tab.running && (
          <ToolbarButton
            compact={compactToolbar}
            variant="danger"
            label={t('query.stop')}
            icon={<Square className="h-3.5 w-3.5" />}
            onClick={handleCancel}
          />
        )}
        {supportsExplain && (
          <ToolbarButton
            compact={compactToolbar}
            variant="ghost"
            label={t('explain.title')}
            icon={<FileSearch className="h-3.5 w-3.5" />}
            onClick={() => void handleExplain()}
            disabled={tab.running || !tab.sql.trim()}
          />
        )}
        <ToolbarButton
          compact={compactToolbar}
          variant="ghost"
          label={t('query.format')}
          icon={<Wand2 className="h-3.5 w-3.5" />}
          onClick={handleFormat}
          disabled={tab.running || !tab.sql.trim()}
        />
        <div className="mx-1 h-4 w-px shrink-0 bg-edge" />
        <ToolbarButton
          compact={compactToolbar}
          variant="ghost"
          label={t('query.beginTx')}
          icon={<CirclePlay className="h-3.5 w-3.5" />}
          onClick={() => void handleBeginTx()}
          disabled={tab.running || txBusy || inTransaction}
        />
        <ToolbarButton
          compact={compactToolbar}
          variant="ghost"
          label={t('query.commitTx')}
          icon={<Check className="h-3.5 w-3.5" />}
          onClick={() => void handleCommitTx()}
          disabled={tab.running || txBusy || !inTransaction}
        />
        <ToolbarButton
          compact={compactToolbar}
          variant="ghost"
          label={t('query.rollbackTx')}
          icon={<Undo2 className="h-3.5 w-3.5" />}
          onClick={() => void handleRollbackTx()}
          disabled={tab.running || txBusy || !inTransaction}
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
        {tab.executionTimeMs != null && (
          <span className="shrink-0 whitespace-nowrap text-[11px] text-fg-muted">
            {compactToolbar
              ? `${tab.executionTimeMs} ms`
              : `${t('query.totalTime')} ${tab.executionTimeMs} ms`}
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
      </div>

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
              connectionId={connectionId}
              database={currentDatabase ?? ''}
              onSqlChange={handleApplyAiSql}
            />
          )}

          {/* SQL editor — height adjustable via bottom drag handle */}
          <div className="relative shrink-0 border-b border-edge" style={{ height: editorHeight }}>
            <SqlEditor
              ref={editorRef}
              value={tab.sql}
              onChange={(v) => updateSql(tab.id, v)}
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
            <div
              className="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
              onClick={() => setShowFavoriteDialog(false)}
            >
              <div
                className="w-[400px] rounded-lg border border-edge bg-surface p-4 shadow-xl"
                onClick={(e) => e.stopPropagation()}
              >
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
                        void addFavorite(favoriteName.trim(), favoriteDialogSql);
                        setFavoriteName('');
                        setShowFavoriteDialog(false);
                      }
                      if (e.key === 'Escape') {
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
                        void addFavorite(favoriteName.trim(), favoriteDialogSql);
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
            {showExplain && !tab.running && (
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
                    <div className="rounded-md border border-red-500/20 bg-red-500/10 px-4 py-3 text-sm text-red-400">
                      {explainError}
                    </div>
                  </div>
                )}
                {!explainLoading && explainResult && (
                  <ExplainPanel
                    connectionId={connectionId}
                    sql={tab.sql}
                    explainOutput={explainResult.planText}
                    planJson={explainResult.planJson}
                    onApplySql={handleApplyAiSql}
                  />
                )}
              </div>
            )}

            {!showExplain && tab.running && results.length === 0 && (
              <div className="flex flex-1 items-center justify-center gap-2 text-fg-muted">
                <Loader2 className="h-5 w-5 animate-spin" />
                {t('query.executing')}
              </div>
            )}

            {!showExplain && tab.error && !tab.running && (
              <div className="flex-1 overflow-auto">
                <div className="p-4">
                  <QueryErrorPanel
                    message={tab.error}
                    onDiagnose={currentDatabase ? () => setDiagnosisVisible(true) : undefined}
                  />
                </div>
                {diagnosisVisible && currentDatabase && (
                  <DiagnosisPanel
                    connectionId={connectionId}
                    database={currentDatabase}
                    sql={tab.sql}
                    errorMessage={tab.error}
                    onApplySql={handleApplyAiSql}
                    onClose={() => setDiagnosisVisible(false)}
                  />
                )}
              </div>
            )}

            {!showExplain && !tab.error && results.length > 0 && (
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
                        onClick={() => setActiveResult(tab.id, idx)}
                      >
                        {t('query.result')} {idx + 1}
                        <span className="ml-1.5 text-[10px] text-fg-muted">
                          ({r.rows.length} {t('common.rows')}
                          {tab.running ? '' : `, ${r.executionTimeMs}ms`})
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
                    {tab.running && (
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
                    {tab.running || resultViewMode === 'table' ? (
                      <ResultTable result={activeResult} />
                    ) : (
                      <ChartView
                        key={tab.id}
                        result={activeResult}
                        savedConfig={tab.chartConfig}
                        onConfigChange={(cfg) => setChartConfig(tab.id, cfg)}
                        onDataPointClick={(rowIndex) => {
                          setResultViewMode('table');
                          setResultDetailRow(rowIndex);
                        }}
                      />
                    )}
                  </>
                )}
              </>
            )}

            {!showExplain && results.length === 0 && !tab.running && !tab.error && (
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
                    onClick={() => updateSql(tab.id, f.sql)}
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
              className="border-b border-edge px-3 py-2 text-[11px] font-semibold uppercase tracking-wider text-fg-muted"
              onContextMenu={handleHistoryHeaderContextMenu}
            >
              {t('query.historyTitle')}
            </div>
            {history.length === 0 ? (
              <div className="px-3 py-4 text-center text-xs text-fg-muted">
                {t('query.noHistory')}
              </div>
            ) : (
              history.map((h) => (
                <button
                  key={h.id}
                  type="button"
                  className="w-full border-b border-edge px-3 py-2 text-left hover:bg-surface-raised"
                  onClick={() => updateSql(tab.id, h.sql)}
                  onContextMenu={(e) => handleHistoryContextMenu(e, h.sql)}
                >
                  <div className="truncate font-mono text-xs text-fg-secondary">{h.sql}</div>
                  <div className="mt-1 flex items-center gap-2 text-[11px] text-fg-muted">
                    <span className={h.success ? 'text-green-400' : 'text-red-400'}>
                      {h.success ? t('common.success') : t('common.failed')}
                    </span>
                    <span>{h.executionTimeMs}ms</span>
                  </div>
                </button>
              ))
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
          <pre className="max-h-40 overflow-auto whitespace-pre-wrap rounded border border-edge bg-surface p-2 font-mono text-[11px] text-red-400">
            {txAbortedDetail}
          </pre>
        ) : null}
      </Dialog>

      <AddToDashboardDialog
        open={addToDashboardOpen}
        onClose={() => setAddToDashboardOpen(false)}
        onConfirm={(dashboardId, newName) => {
          void (async () => {
            if (!tab?.sql.trim() || !activeResult?.rows.length) return;
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
                configId: connectionId,
                sql: tab.sql,
                title: tab.title || undefined,
                viewMode: resultViewMode,
                chartConfig: tab.chartConfig,
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

function ResultTable({ result }: { result: StatementResult }) {
  const { t } = useI18n();
  const queryResultLimit = useSettingsStore((s) => s.settings.queryResultLimit);
  const setResultDetailRow = useQueryStore((s) => s.setResultDetailRow);
  const resultDetailRowIndex = useQueryStore((s) => s.resultDetailRowIndex);

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
      setResultDetailRow(row);
      setEditingCell({ row, col });
    },
    [setResultDetailRow],
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
      onRowClick={setResultDetailRow}
      highlightedRow={resultDetailRowIndex}
      exportTableName="query_result"
    />
  );
}
