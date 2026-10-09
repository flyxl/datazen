import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type MutableRefObject,
  type Ref,
} from 'react';
import {
  Bookmark,
  Check,
  CirclePlay,
  Clock,
  FileSearch,
  Play,
  Save,
  Sparkles,
  Undo2,
  Wand2,
  WandSparkles,
} from 'lucide-react';
import { ToolbarShell } from '../../../components/ui/ToolbarShell';
import { ToolbarButton } from '../../../components/ui/ToolbarButton';
import { SqlEditor } from '../../../components/SqlEditor';
import type { SqlEditorHandle } from '../../../components/SqlEditor';
import type { EditorMetadataSnapshot } from '../../../components/sql-editor/metadata/types';
import { SnippetMenuButton } from './toolbar/SnippetMenuButton';
import { ExecutionStrategySelect } from './toolbar/ExecutionStrategySelect';
import { RefreshCompletionButton } from './toolbar/RefreshCompletionButton';
import { QueryContextSelectors } from '../../../components/query/QueryContextSelectors';
import { QueryExecutionStatus } from '../../../components/query/QueryExecutionStatus';
import { Nl2SqlPanel } from '../../../components/ai/Nl2SqlPanel';
import { QueryBuilderHostAdapter } from './QueryBuilderHostAdapter';
import { useQueryBuilderContribution } from './useQueryBuilderContribution';
import { sqlEditorEnhancedEP, useExtension } from '@datazen/extension-points';
import { cn } from '../../../lib/cn';
import { useI18n } from '../../../hooks/useI18n';
import { usePlatform } from '../../../hooks/usePlatform';
import { useSettingsStore } from '../../../stores/settingsStore';
import { tid } from '../../../lib/tid';
import { formatShortcutForDisplay, getActionShortcut } from '../../../lib/keymap';
import { showNativeContextMenu } from '../../../lib/nativeContextMenu';
import { buildSqlEditorContextMenuItems } from '../../../lib/sqlEditorContextMenu';
import type { QueryExecutionViewModel } from '../../../lib/queryExecutionViewModel';
import type { SqlNamespace } from '../../../lib/sqlNamespace';
import type { SqlParam } from '../../../lib/sqlBindParams';
import { Spinner } from '../../../components/ui/Spinner';

export interface QueryEditorSectionProps {
  /** Owning query panel; scopes the visual builder's open state. */
  panelId: string;
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
  /**
   * `true` pins the editor to `editorHeight` because the user dragged the
   * splitter. `false` (default) lets the editor absorb whatever height the
   * column has left — before the first execution, and while the result pane
   * sizes itself to its content.
   */
  editorHeightPinned?: boolean;
  /** Rendered only while nothing has been executed yet. */
  showIdleHint?: boolean;
  /** Hidden before the first execution: there is nothing to split against. */
  showResizeHandle?: boolean;
  /**
   * The editor host. The splitter reads its rendered height so taking over
   * from automatic sizing starts exactly where the user grabbed it.
   */
  editorViewportRef?: Ref<HTMLDivElement>;
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
  /** Reports the completion-cache refresh result to the panel's message surface. */
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
  panelId,
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
  editorHeightPinned = false,
  showIdleHint = false,
  showResizeHandle = true,
  editorViewportRef,
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
  const completionIncludeTablePrefix = useSettingsStore(
    (s) => s.settings.editorCompletionIncludeTablePrefix ?? true,
  );
  const completionQuotePolicy = useSettingsStore(
    (s) => s.settings.editorCompletionQuotePolicy ?? 'unquoted',
  );
  const keymapPreset = useSettingsStore((s) => s.settings.keymapPreset);
  const customKeymap = useSettingsStore((s) => s.settings.customKeymap);
  const enhanced = useExtension(sqlEditorEnhancedEP);
  const { contribution: queryBuilder, openPanelId } = useQueryBuilderContribution();
  const editorExtensionSettings = useSettingsStore(
    (s) =>
      (s.settings.driverSettings?.['sql-editor-enhanced'] ??
        s.settings.driverSettings?.['sql-editor-pro']) as Record<string, unknown> | undefined,
  );
  const bindParamPanelEnabled = editorExtensionSettings?.bindParamPanel !== false;

  // The toolbar is icon-only, so keymap hints live in the tooltips.
  const formatShortcut = formatShortcutForDisplay(
    getActionShortcut('formatSql', keymapPreset, customKeymap),
  );

  // The Pro contribution owns its private builder store; visibility remains
  // scoped to the query tab that opened it.
  const qbOpen = openPanelId === panelId;
  const qbContextKey = JSON.stringify([
    connectionId ?? '',
    dbSessionId,
    selectedDatabase ?? '',
    selectedSchema ?? '',
  ]);
  const boundQbContextRef = useRef<{
    contribution: typeof queryBuilder;
    panelId: string;
    contextKey: string;
  } | null>(null);
  const openQbForCurrentContext = useCallback(() => {
    if (!queryBuilder) return;
    boundQbContextRef.current = { contribution: queryBuilder, panelId, contextKey: qbContextKey };
    queryBuilder.openFor(panelId, qbContextKey);
  }, [queryBuilder, panelId, qbContextKey]);

  // The selectors remain available while Builder is open. Rebind synchronously
  // before paint so the Pro controller destroys the old draft before the panel
  // can render or commit it using the newly selected database/schema props.
  useLayoutEffect(() => {
    if (!qbOpen || !queryBuilder) return;
    const bound = boundQbContextRef.current;
    if (
      bound?.contribution === queryBuilder &&
      bound.panelId === panelId &&
      bound.contextKey === qbContextKey
    ) {
      return;
    }
    openQbForCurrentContext();
  }, [qbOpen, queryBuilder, panelId, qbContextKey, openQbForCurrentContext]);

  // Non-blocking confirmation that the SQL landed in the editor.
  const [qbToast, setQbToast] = useState<string | null>(null);
  // Focus the editor once it is visible again (after OK / Cancel).
  const pendingEditorFocusRef = useRef(false);

  useEffect(() => {
    if (!qbToast) return;
    const timer = window.setTimeout(() => setQbToast(null), 2500);
    return () => window.clearTimeout(timer);
  }, [qbToast]);

  useEffect(() => {
    if (qbOpen || !pendingEditorFocusRef.current) return;
    pendingEditorFocusRef.current = false;
    // The editor was kept mounted (CSS-hidden), so this only re-focuses it.
    editorRef.current?.focus?.();
  }, [qbOpen, editorRef]);

  const handleToggleQb = useCallback(() => {
    // The toolbar item is a visibility toggle, not a cancel: the canvas state
    // survives, so nothing is destroyed by collapsing the builder.
    if (qbOpen) {
      queryBuilder?.hideFor();
    } else {
      openQbForCurrentContext();
    }
  }, [qbOpen, queryBuilder, openQbForCurrentContext]);

  /** OK: write the generated SQL back, close, and focus the editor. */
  const handleQbCommit = useCallback(
    (newSql: string | null, mode: 'replace' | 'append') => {
      if (newSql !== null) {
        onUpdateSql(mode === 'append' ? `${sql.trimEnd()}\n${newSql}` : newSql);
      }
      queryBuilder?.closeFor('ok');
      pendingEditorFocusRef.current = true;
      setQbToast(t('query.visualBuilder.appliedToast'));
    },
    [onUpdateSql, sql, queryBuilder, t],
  );

  /** Cancel / ×: discard the canvas changes and go back to the editor. */
  const handleQbCancel = useCallback(() => {
    queryBuilder?.closeFor('cancel');
    pendingEditorFocusRef.current = true;
  }, [queryBuilder]);

  /**
   * Prefer the editor's own selection-aware formatter; `onFormat` stays
   * as the fallback for the rare case the editor has not mounted yet.
   */
  const handleFormatClick = useCallback(() => {
    if (editorRef.current?.formatDocument) {
      editorRef.current.formatDocument();
      return;
    }
    onFormat();
  }, [editorRef, onFormat]);

  const handleToggleNl2sql = useCallback(() => {
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
        className="scrollbar-hide h-9 flex-nowrap overflow-x-hidden px-3"
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
            iconOnly
            variant="run"
            label={t('query.execute')}
            title={`${t('query.execute')} (${executeShortcutLabel})`}
            icon={running ? <Spinner size="md" /> : <Play className="h-3.5 w-3.5" />}
            onClick={onExecute}
            disabled={running}
            {...tid('editor-execute-button')}
          />
        )}
        <ExecutionStrategySelect iconOnly disabled={running} />
        <ToolbarButton
          iconOnly
          variant="ghost"
          label={t('common.save')}
          icon={<Save className="h-3.5 w-3.5" />}
          onClick={() => onOpenAddFavoriteDialog(sql)}
          disabled={running || !sql.trim()}
          {...tid('editor-save-button')}
        />
        <ToolbarButton
          iconOnly
          variant={nl2sqlVisible ? 'secondary' : 'ghost'}
          label={t('nl2sql.title')}
          icon={<Sparkles className="h-3.5 w-3.5" />}
          onClick={handleToggleNl2sql}
          {...tid('editor-nl2sql-button')}
        />
        {/*
         * Every action lives directly in the toolbar: the window has room for
         * the whole set, and an overflow menu would hide two-click actions
         * behind an extra hop. Names and shortcuts moved to the tooltips.
         */}
        <div className="mx-0.5 h-4 w-px shrink-0 bg-edge" />
        <ToolbarButton
          iconOnly
          variant="ghost"
          label={t('query.format')}
          title={`${t('query.format')} (${formatShortcut})`}
          icon={<Wand2 className="h-3.5 w-3.5" />}
          onClick={handleFormatClick}
          disabled={running || !sql.trim()}
          {...tid('editor-format-button')}
        />
        {supportsExplain && (
          <ToolbarButton
            iconOnly
            variant="ghost"
            label={t('explain.title')}
            icon={<FileSearch className="h-3.5 w-3.5" />}
            onClick={() => void onExplain()}
            disabled={running || !sql.trim()}
            {...tid('editor-explain-button')}
          />
        )}
        <SnippetMenuButton editorRef={editorRef} iconOnly disabled={running} />
        <RefreshCompletionButton
          dbSessionId={dbSessionId}
          database={selectedDatabase}
          iconOnly
          disabled={running}
          onRefreshed={onCompletionRefreshed}
        />
        {queryBuilder && (
          <ToolbarButton
            iconOnly
            variant={qbOpen ? 'secondary' : 'ghost'}
            label={t('query.visualBuilder.title')}
            icon={<WandSparkles className="h-3.5 w-3.5" />}
            onClick={handleToggleQb}
            {...tid('editor-visual-builder-button')}
          />
        )}
        {inTransaction ? (
          <>
            <div className="mx-0.5 h-4 w-px shrink-0 bg-edge" />
            <ToolbarButton
              iconOnly
              variant="ghost"
              label={t('query.commitTx')}
              icon={<Check className="h-3.5 w-3.5 text-success" />}
              onClick={() => void onCommitTx()}
              disabled={running || txBusy}
              data-testid="query-commit-tx"
            />
            <ToolbarButton
              iconOnly
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
        ) : (
          <ToolbarButton
            iconOnly
            variant="ghost"
            label={t('query.beginTx')}
            icon={<CirclePlay className="h-3.5 w-3.5" />}
            onClick={() => void onBeginTx()}
            disabled={running || txBusy}
            {...tid('editor-begin-tx-button')}
          />
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
          iconOnly
          variant={historyVisible ? 'secondary' : 'ghost'}
          label={t('query.history')}
          icon={<Clock className="h-3.5 w-3.5" />}
          onClick={onToggleHistory}
          {...tid('editor-history-toggle')}
        />
        <ToolbarButton
          iconOnly
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

      {/*
       * Editor column — the region the result pane splits against.
       *
       * It only shrinks to the editor's own height (`flex-initial`) once the
       * user has pinned a height by dragging the splitter; otherwise it absorbs
       * every pixel the result pane does not claim, which is what makes the
       * editor fill the panel before anything has been executed. With the
       * builder open the canvas owns the column (`flex-1`) because the result
       * pane is unmounted in that mode.
       */}
      <div
        className={cn(
          'relative flex min-h-0 min-w-0 flex-col',
          editorHeightPinned && !qbOpen ? 'flex-initial' : 'flex-1',
        )}
      >
        {nl2sqlVisible && (
          <Nl2SqlPanel
            dbSessionId={dbSessionId}
            database={selectedDatabase ?? ''}
            onSqlChange={onApplyAiSql}
          />
        )}

        {/*
         * The builder replaces the editor area rather than stacking above it,
         * so the canvas owns the panel height.
         */}
        {qbOpen && queryBuilder && (
          <QueryBuilderHostAdapter
            contribution={queryBuilder}
            panelId={panelId}
            connectionId={connectionId ?? ''}
            dbSessionId={dbSessionId}
            databaseType={databaseType ?? ''}
            database={selectedDatabase ?? ''}
            schema={selectedSchema ?? null}
            currentSql={sql}
            onCommit={handleQbCommit}
            onCancel={handleQbCancel}
          />
        )}

        {/*
         * Kept mounted while the builder is open (CSS-hidden only): unmounting
         * would drop the CodeMirror undo stack, the bind-param values (Pro EP)
         * and the metadata cache.
         *
         * Unpinned it fills the column (`flex-1`); pinned it takes the user's
         * height. `min-h-0 shrink` is load-bearing in the pinned case: a
         * persisted `editorHeight` larger than the space this column actually
         * has must shrink to fit. While it overflowed, the editor's opaque
         * gutter and bottom border painted on top of the result pane's toolbar
         * (the editor host is `relative`) and hid the 表格 / 图表 view toggle.
         */}
        <div
          ref={editorViewportRef}
          className={cn(
            'relative min-h-0 border-b border-edge',
            editorHeightPinned ? 'shrink' : 'flex-1',
            qbOpen && 'hidden',
          )}
          style={editorHeightPinned ? { height: editorHeight } : undefined}
          data-testid="query-editor-host"
        >
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
            completionIncludeTablePrefix={completionIncludeTablePrefix}
          />
        </div>

        {/* Idle hint. It used to live in the result pane, which kept a whole
            half of the panel reserved before anything had been executed; it is
            a one-line footer now so the editor can own the full height. */}
        {showIdleHint && !qbOpen && (
          <div
            className="flex shrink-0 items-center justify-center px-3 py-1.5 text-xs text-fg-muted"
            data-testid="query-idle-hint"
          >
            {t('query.shortcutHint')}
          </div>
        )}

        <div
          ref={editorResizeRef}
          className={cn(
            'h-1.5 shrink-0 cursor-row-resize bg-transparent hover:bg-accent/30 active:bg-accent/40',
            (qbOpen || !showResizeHandle) && 'hidden',
          )}
          title="Drag to resize editor"
        />
      </div>

      {qbToast && (
        <div
          className="pointer-events-none fixed bottom-6 left-1/2 z-50 -translate-x-1/2 rounded-lg border border-edge bg-surface-raised px-3 py-1.5 text-xs text-fg shadow-lg"
          role="status"
          data-testid="qb-toast"
        >
          {qbToast}
        </div>
      )}
    </>
  );
}
