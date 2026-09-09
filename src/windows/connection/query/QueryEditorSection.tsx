import { useCallback, useState, type MutableRefObject, type Ref } from 'react';
import { Bookmark, Check, Clock, Loader2, Play, Save, Sparkles, Undo2 } from 'lucide-react';
import { ToolbarShell } from '../../../components/ui/ToolbarShell';
import { ToolbarButton } from '../../../components/ui/ToolbarButton';
import { SqlEditor } from '../../../components/SqlEditor';
import type { SqlEditorHandle } from '../../../components/SqlEditor';
import type { EditorMetadataSnapshot } from '../../../components/sql-editor/metadata/types';
import { SnippetMenuButton } from './toolbar/SnippetMenuButton';
import { ExecutionStrategySelect } from './toolbar/ExecutionStrategySelect';
import { QueryToolbarMoreMenu } from './QueryToolbarMoreMenu';
import { metadataCache } from '../../../components/sql-editor/metadata/metadataCache';
import { invalidateSchemaCache } from '../../../lib/schemaCache';
import { useSchemaStore } from '../../../stores/schemaStore';
import { QueryContextSelectors } from '../../../components/query/QueryContextSelectors';
import { QueryExecutionStatus } from '../../../components/query/QueryExecutionStatus';
import { Nl2SqlPanel } from '../../../components/ai/Nl2SqlPanel';
import { sqlEditorEnhancedEP, useExtension } from '@datazen/extension-points';
import { useI18n } from '../../../hooks/useI18n';
import { useOnboardingStore } from '../../../stores/onboardingStore';
import { usePlatform } from '../../../hooks/usePlatform';
import { useSettingsStore } from '../../../stores/settingsStore';
import { tid } from '../../../lib/tid';
import { showNativeContextMenu } from '../../../lib/nativeContextMenu';
import { buildSqlEditorContextMenuItems } from '../../../lib/sqlEditorContextMenu';
import type { QueryExecutionViewModel } from '../../../lib/queryExecutionViewModel';
import type { SqlNamespace } from '../../../lib/sqlNamespace';
import type { SqlParam } from '../../../lib/sqlBindParams';

export interface QueryEditorSectionProps {
  dbSessionId: string;
  databaseType?: string;
  editorRef: MutableRefObject<SqlEditorHandle | null>;
  toolbarRef: Ref<HTMLDivElement>;
  compactToolbar: boolean;
  sql: string;
  running: boolean;
  executionTimeMs: number | null | undefined;
  executionViewModel: QueryExecutionViewModel;
  sqlParams: SqlParam[];
  paramValues: Record<string, string>;
  paramLabels?: Record<string, string>;
  paramHistory?: Record<string, any[]>;
  onParamChange: (name: string, value: string) => void;
  onClearParamHistory?: (stableId: string) => void;
  onApplyParamHistory?: (stableId: string, value: string) => void;
  editorHeight: number;
  editorResizeRef: Ref<HTMLDivElement>;
  editorSchema: SqlNamespace;
  editorDefaultSchema: string | undefined;
  editorDefaultTable: string | undefined;
  namespaceLoading: boolean;
  supportsExplain: boolean;
  safeMode: boolean;
  inTransaction: boolean;
  txBusy: boolean;
  isMultiDb: boolean;
  isPathHierarchy: boolean;
  hasContextSelectors: boolean;
  databases: string[];
  selectedDatabase?: string | null;
  selectedSchema?: string | null;
  namespaceTree: SqlNamespace;
  pathAliases: Record<string, string>;
  contextPath: string[];
  nl2sqlVisible: boolean;
  onToggleNl2sql: () => void;
  historyVisible: boolean;
  favoritesVisible: boolean;
  onToggleHistory: () => void;
  onToggleFavorites: () => void;
  onUpdateSql: (sql: string) => void;
  onExecute: () => void;
  onExecuteSelection: (sql: string) => void;
  onCancel: () => void;
  onFormat: () => void;
  /** §4.3 Reports the completion-cache refresh result to the panel's message surface. */
  onCompletionRefreshed: (message: string) => void;
  onExplain: () => void;
  onBeginTx: () => void;
  onCommitTx: () => void;
  onRollbackTx: () => void;
  onApplyAiSql: (sql: string) => void;
  onOpenAddFavoriteDialog: (sql: string) => void;
  onQualifiedPath: (parents: string[]) => void;
  onSelectContextLevel: (index: number, value: string) => void;
  onDropTable: (
    payload: import('../../../components/SqlEditor').DroppedTablePayload,
    pos: number | null,
  ) => void;

  // ── S6-D: Metadata & execution wiring ──────────────────────────
  /** Immutable metadata snapshot for completion/hover/navigation. */
  metadataSnapshot?: EditorMetadataSnapshot;
  /** Connection identity for cross-connection drop validation. */
  connectionId?: string;

  // ── S6-D: Navigation & DDL callbacks ───────────────────────────
  /** Navigate to a table data page (Mod+Click on table name). */
  onNavigateToTable?: (target: { database?: string; schema?: string; name: string }) => void;
  /** Navigate to a table structure page (Mod+Click on column name). */
  onNavigateToStructure?: (target: {
    database?: string;
    schema?: string;
    name: string;
    columnName?: string;
  }) => void;
  /** Navigate to a relation DDL page (hover action). */
  onNavigateToDdl?: (target: {
    database?: string;
    schema?: string;
    name: string;
    kind: 'table' | 'view';
  }) => void;
}

export function QueryEditorSection({
  dbSessionId,
  databaseType,
  editorRef,
  toolbarRef,
  compactToolbar,
  sql,
  running,
  executionTimeMs,
  executionViewModel,
  sqlParams,
  paramValues,
  paramLabels,
  paramHistory,
  onParamChange,
  onClearParamHistory,
  onApplyParamHistory,
  editorHeight,
  editorResizeRef,
  editorSchema,
  editorDefaultSchema,
  editorDefaultTable,
  namespaceLoading,
  supportsExplain,
  safeMode,
  inTransaction,
  txBusy,
  isMultiDb,
  isPathHierarchy,
  hasContextSelectors,
  databases,
  selectedDatabase,
  selectedSchema,
  namespaceTree,
  pathAliases,
  contextPath,
  nl2sqlVisible,
  onToggleNl2sql,
  historyVisible,
  favoritesVisible,
  onToggleHistory,
  onToggleFavorites,
  onUpdateSql,
  onExecute,
  onExecuteSelection,
  onCancel,
  onFormat,
  onCompletionRefreshed,
  onExplain,
  onBeginTx,
  onCommitTx,
  onRollbackTx,
  onApplyAiSql,
  onOpenAddFavoriteDialog,
  onQualifiedPath,
  onSelectContextLevel,
  onDropTable,
  // S6-D new props
  metadataSnapshot,
  connectionId,
  onNavigateToTable,
  onNavigateToStructure,
  onNavigateToDdl,
}: QueryEditorSectionProps) {
  const { t } = useI18n();
  const platform = usePlatform();
  const isMac = platform === 'macos';
  const executeShortcutLabel = isMac ? '⌘ Enter' : 'Ctrl+Enter';
  const completionQuotePolicy = useSettingsStore(
    (s) => s.settings.editorCompletionQuotePolicy ?? 'unquoted',
  );
  const enhanced = useExtension(sqlEditorEnhancedEP);
  const editorExtensionSettings = useSettingsStore(
    (s) =>
      (s.settings.pluginSettings?.['sql-editor-enhanced'] ??
        s.settings.pluginSettings?.['sql-editor-pro']) as Record<string, unknown> | undefined,
  );
  const bindParamPanelEnabled = editorExtensionSettings?.bindParamPanel !== false;
  const [isRefreshingCompletion, setIsRefreshingCompletion] = useState(false);

  /**
   * Prefer the editor's own selection-aware formatter (§4.2); `onFormat` stays
   * as the fallback for the rare case the editor has not mounted yet.
   */
  const handleFormatClick = useCallback(() => {
    if (editorRef.current?.formatDocument) {
      editorRef.current.formatDocument();
      return;
    }
    onFormat();
  }, [editorRef, onFormat]);

  const handleRefreshCompletion = useCallback(async () => {
    if (!dbSessionId || isRefreshingCompletion) return;
    setIsRefreshingCompletion(true);
    try {
      invalidateSchemaCache(dbSessionId);
      metadataCache.invalidateSession(dbSessionId);
      if (selectedDatabase) {
        await useSchemaStore.getState().loadTables(selectedDatabase, dbSessionId);
      }
      onCompletionRefreshed(t('query.refreshCompletionDone'));
    } finally {
      setIsRefreshingCompletion(false);
    }
  }, [dbSessionId, selectedDatabase, isRefreshingCompletion, onCompletionRefreshed, t]);

  const handleToggleNl2sql = useCallback(() => {
    const ob = useOnboardingStore.getState();
    if (ob.status === 'active' && ob.step === 3) {
      ob.markAiOrChartExplored();
    }
    onToggleNl2sql();
  }, [onToggleNl2sql]);

  const handleEditorContextMenu = useCallback(
    (e: MouseEvent, sqlText: string) => {
      const selection = editorRef.current?.getSelection() ?? '';
      const hasSelection = selection.length > 0;
      void showNativeContextMenu(
        buildSqlEditorContextMenuItems({
          labels: {
            run: t('query.run'),
            runSelection: t('query.runSelection'),
            format: t('query.format'),
            comment: t('query.comment'),
            addFavorite: t('common.addToFavorites'),
          },
          handlers: {
            onRun: onExecute,
            onRunSelection: () => {
              if (selection.trim()) onExecuteSelection(selection);
            },
            onFormat: handleFormatClick,
            onComment: () => editorRef.current?.toggleLineComment(),
            onAddFavorite: onOpenAddFavoriteDialog,
          },
          sqlText,
          hasSelection,
        }),
        { x: e.clientX, y: e.clientY },
      );
    },
    [editorRef, onExecute, onExecuteSelection, handleFormatClick, onOpenAddFavoriteDialog, t],
  );

  return (
    <>
      <ToolbarShell
        ref={toolbarRef}
        className="h-9 flex-nowrap overflow-x-hidden px-3"
        {...tid('query-editor-toolbar')}
      >
        {hasContextSelectors && (
          <QueryContextSelectors
            isMultiDb={isMultiDb}
            isPathHierarchy={isPathHierarchy}
            databases={databases}
            currentDatabase={selectedDatabase ?? null}
            contextSchema={selectedSchema}
            namespaceTree={namespaceTree}
            pathAliases={pathAliases}
            contextPath={contextPath}
            namespaceLoading={namespaceLoading}
            onSelectLevel={onSelectContextLevel}
          />
        )}
        {running ? (
          <QueryExecutionStatus viewModel={executionViewModel} onCancel={onCancel} />
        ) : (
          <ToolbarButton
            compact={compactToolbar}
            variant="run"
            label={t('query.execute')}
            title={`${t('query.execute')} (${executeShortcutLabel})`}
            icon={
              running ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Play className="h-3.5 w-3.5" />
              )
            }
            onClick={onExecute}
            disabled={running}
            {...tid('editor-execute-button')}
          />
        )}
        <ExecutionStrategySelect compact={compactToolbar} disabled={running} />
        <ToolbarButton
          compact={compactToolbar}
          variant="ghost"
          label={t('common.save')}
          icon={<Save className="h-3.5 w-3.5" />}
          onClick={() => onOpenAddFavoriteDialog(sql)}
          disabled={running || !sql.trim()}
          {...tid('editor-save-button')}
        />
        <ToolbarButton
          compact={compactToolbar}
          variant={nl2sqlVisible ? 'secondary' : 'ghost'}
          label={t('nl2sql.title')}
          icon={<Sparkles className="h-3.5 w-3.5" />}
          onClick={handleToggleNl2sql}
        />
        <QueryToolbarMoreMenu
          compact={compactToolbar}
          disabled={running}
          supportsExplain={supportsExplain}
          explainDisabled={running || !sql.trim()}
          formatDisabled={running || !sql.trim()}
          refreshCompletionDisabled={isRefreshingCompletion}
          inTransaction={inTransaction}
          txBusy={txBusy}
          onFormat={handleFormatClick}
          onExplain={() => void onExplain()}
          onBeginTx={() => void onBeginTx()}
          onCommitTx={() => void onCommitTx()}
          onRollbackTx={() => void onRollbackTx()}
          onRefreshCompletion={() => void handleRefreshCompletion()}
          renderSnippetButton={() => (
            <SnippetMenuButton editorRef={editorRef} compact={compactToolbar} disabled={running} />
          )}
        />
        {inTransaction && (
          <>
            <div className="mx-1 h-4 w-px shrink-0 bg-edge" />
            <ToolbarButton
              compact={compactToolbar}
              variant="ghost"
              label={t('query.commitTx')}
              icon={<Check className="h-3.5 w-3.5 text-success" />}
              onClick={() => void onCommitTx()}
              disabled={running || txBusy}
              data-testid="query-commit-tx"
            />
            <ToolbarButton
              compact={compactToolbar}
              variant="ghost"
              label={t('query.rollbackTx')}
              icon={<Undo2 className="h-3.5 w-3.5 text-danger" />}
              onClick={() => void onRollbackTx()}
              disabled={running || txBusy}
              data-testid="query-rollback-tx"
            />
            <span
              className="shrink-0 rounded bg-accent/15 px-1.5 py-0.5 text-[10px] font-medium text-accent"
              title={t('query.inTransaction')}
            >
              {compactToolbar ? 'TX' : t('query.inTransaction')}
            </span>
          </>
        )}
        <div className="min-w-0 flex-1" />
        {safeMode && (
          <span
            className="shrink-0 rounded bg-warning/15 px-1.5 py-0.5 text-[10px] font-medium text-warning"
            title={t('settings.safeMode')}
            data-testid="query-safe-mode"
          >
            {compactToolbar ? 'Safe' : t('settings.safeMode')}
          </span>
        )}
        {!compactToolbar && (
          <span className="shrink-0 whitespace-nowrap text-[11px] text-fg-muted">
            {isMac ? '⌘+Enter' : 'Ctrl+Enter'} {t('query.execute')}
          </span>
        )}
        {executionViewModel.rowCount != null && (
          <span className="shrink-0 whitespace-nowrap text-[11px] text-fg-muted">
            {compactToolbar
              ? `${executionViewModel.rowCount} ${t('common.rows')}`
              : t('query.historyRows', { count: executionViewModel.rowCount })}
          </span>
        )}
        {executionViewModel.affectedRows != null && (
          <span className="shrink-0 whitespace-nowrap text-[11px] text-fg-muted">
            {compactToolbar
              ? `${executionViewModel.affectedRows} ${t('query.affectedRows')}`
              : t('query.rowsAffectedCount', { count: executionViewModel.affectedRows })}
          </span>
        )}
        {executionTimeMs != null && (
          <span className="shrink-0 whitespace-nowrap text-[11px] text-fg-muted">
            {compactToolbar
              ? `${executionTimeMs} ms`
              : `${t('query.totalTime')} ${executionTimeMs} ms`}
          </span>
        )}
        <ToolbarButton
          compact={compactToolbar}
          variant={historyVisible ? 'secondary' : 'ghost'}
          label={t('query.history')}
          icon={<Clock className="h-3.5 w-3.5" />}
          onClick={onToggleHistory}
          {...tid('editor-history-toggle')}
        />
        <ToolbarButton
          compact={compactToolbar}
          variant={favoritesVisible ? 'secondary' : 'ghost'}
          label={t('query.favorites')}
          icon={<Bookmark className="h-3.5 w-3.5" />}
          onClick={onToggleFavorites}
          {...tid('editor-favorites-toggle')}
        />
      </ToolbarShell>

      {bindParamPanelEnabled && enhanced.renderBindParamPanel && (
        <div data-testid="bind-param-panel">
          {enhanced.renderBindParamPanel({
            params: sqlParams,
            values: paramValues,
            labels: paramLabels,
            history: paramHistory,
            onChange: onParamChange,
            onClearHistory: onClearParamHistory,
            onApplyHistory: onApplyParamHistory,
          })}
        </div>
      )}

      <div className="flex min-w-0 flex-col">
        {nl2sqlVisible && (
          <Nl2SqlPanel
            dbSessionId={dbSessionId}
            database={selectedDatabase ?? ''}
            onSqlChange={onApplyAiSql}
          />
        )}

        <div className="relative shrink-0 border-b border-edge" style={{ height: editorHeight }}>
          <SqlEditor
            ref={editorRef}
            value={sql}
            onChange={onUpdateSql}
            onExecute={onExecute}
            onExecuteSelection={onExecuteSelection}
            onExecuteAll={onExecute}
            onSaveQuery={() => onOpenAddFavoriteDialog(sql)}
            onContextMenu={handleEditorContextMenu}
            onQualifiedPath={onQualifiedPath}
            placeholder={t('query.placeholder')}
            schema={editorSchema}
            databaseType={databaseType}
            database={selectedDatabase ?? undefined}
            namespaceLoading={namespaceLoading}
            defaultSchema={editorDefaultSchema}
            defaultTable={editorDefaultTable}
            onDropTable={onDropTable}
            // S6-D: metadata & execution wiring
            metadataSnapshot={metadataSnapshot}
            executionStatus={running ? 'running' : 'idle'}
            executingRange={null}
            connectionId={connectionId}
            // S6-D: navigation & DDL callbacks
            onNavigateToTable={onNavigateToTable}
            onNavigateToStructure={onNavigateToStructure}
            onNavigateToDdl={onNavigateToDdl}
            completionQuotePolicy={completionQuotePolicy}
          />
        </div>
        <div
          ref={editorResizeRef}
          className="h-1.5 shrink-0 cursor-row-resize bg-transparent hover:bg-accent/30 active:bg-accent/40"
          title="Drag to resize editor"
        />
      </div>
    </>
  );
}
