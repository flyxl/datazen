import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { AlertTriangle, BarChart3, Bookmark, Clock, Database, FileSearch, Loader2, Play, Sparkles, Square, Stethoscope, TableProperties, Trash2 } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { SqlEditor } from '../../components/SqlEditor';
import type { SqlEditorHandle, SqlSchema } from '../../components/SqlEditor';
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
import { cn } from '../../lib/cn';
import { queryCommands } from '../../commands/query';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import type { ExplainResult, StatementResult } from '../../types';

interface QueryPanelProps {
  connectionId: string;
  queryTabId: string;
  databaseType?: string;
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
  const resultViewMode = tab?.resultViewMode ?? 'table';
  const setResultViewModeStore = useQueryStore((s) => s.setResultViewMode);
  const setResultViewMode = useCallback(
    (mode: 'table' | 'chart') => { if (tab) setResultViewModeStore(tab.id, mode); },
    [tab, setResultViewModeStore],
  );

  const { size: editorHeight, handleRef: editorResizeRef } = useResizable({
    direction: 'vertical',
    initialSize: 280,
    minSize: 100,
    maxSize: 900,
    storageKey: 'query-editor-height',
  });

  const tables = useSchemaStore((s) => s.tables);
  const views = useSchemaStore((s) => s.views);
  const columnMap = useSchemaStore((s) => s.columnMap);
  const databases = useSchemaStore((s) => s.databases);
  const currentDatabase = useSchemaStore((s) => s.currentDatabase);
  const isMultiDb = useSchemaStore((s) => s.isMultiDatabase);
  const loadColumnMap = useSchemaStore((s) => s.loadColumnMap);
  const loadTables = useSchemaStore((s) => s.loadTables);

  const dbMeta = databaseType ? DB_REGISTRY[databaseType as keyof typeof DB_REGISTRY] : undefined;
  const supportsExplain = dbMeta?.supportsExplain !== false;

  const editorSchema: SqlSchema = useMemo(() => {
    const result: SqlSchema = {};
    for (const t of [...tables, ...views]) {
      result[t.name] = columnMap[t.name] ?? [];
    }
    return result;
  }, [tables, views, columnMap]);

  useEffect(() => {
    setConnectionId(connectionId);
    void loadHistory();
    void loadFavorites();
  }, [connectionId, setConnectionId, loadHistory, loadFavorites]);

  useEffect(() => {
    if (tables.length > 0 && Object.keys(columnMap).length === 0) {
      void loadColumnMap();
    }
  }, [tables, columnMap, loadColumnMap]);

  const handleExecute = useCallback(() => {
    if (!tab) return;
    const sel = editorRef.current?.getSelection()?.trim();
    if (sel) {
      void executeSelection(tab.id, sel);
    } else {
      void executeQuery(tab.id);
    }
  }, [tab, executeQuery, executeSelection]);

  const handleExecuteSelection = useCallback((sql: string) => {
    if (tab) void executeSelection(tab.id, sql);
  }, [tab, executeSelection]);

  const handleCancel = useCallback(() => {
    if (tab) void cancelQuery(tab.id);
  }, [tab, cancelQuery]);

  const handleApplyAiSql = useCallback((sql: string) => {
    if (tab) updateSql(tab.id, sql);
  }, [tab, updateSql]);

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

  const handleEditorContextMenu = useCallback((_e: MouseEvent, sqlText: string) => {
    const lang = useSettingsStore.getState().settings.language || 'en';
    pendingFavSqlRef.current = sqlText;
    void invoke('show_editor_context_menu', { lang: lang === 'en' ? 'en' : 'zh' });
  }, []);

  useEffect(() => {
    const unlisten = listen('menu:add-favorite', () => {
      const sql = pendingFavSqlRef.current;
      if (sql) {
        setFavoriteDialogSql(sql);
        setFavoriteName('');
        setShowFavoriteDialog(true);
      }
    });
    return () => { void unlisten.then((fn) => fn()); };
  }, []);

  if (!tab) return null;

  const { results, activeResultIdx } = tab;
  const activeResult: StatementResult | undefined = results[activeResultIdx];

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* Toolbar */}
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-edge bg-surface-alt px-3">
        {isMultiDb && databases.length > 0 && (
          <div className="flex items-center gap-1.5 shrink-0">
            <Database className="h-3.5 w-3.5 text-fg-muted" />
            <Select
              value={currentDatabase ?? ''}
              options={databases.map((db) => ({ value: db, label: db }))}
              onChange={(db) => void loadTables(db)}
              className="!h-6 !text-[11px] max-w-[180px]"
            />
          </div>
        )}
        <Button
          variant="primary"
          className="h-7 gap-1 px-2 text-xs"
          onClick={handleExecute}
          disabled={tab.running}
        >
          <Play className="h-3.5 w-3.5" />
          {t('query.execute')}
        </Button>
        {tab.running && (
          <Button variant="danger" className="h-7 gap-1 px-2 text-xs" onClick={handleCancel}>
            <Square className="h-3.5 w-3.5" />
            {t('query.stop')}
          </Button>
        )}
        {supportsExplain && (
          <Button
            variant="ghost"
            className="h-7 gap-1 px-2 text-xs"
            onClick={() => void handleExplain()}
            disabled={tab.running || !tab.sql.trim()}
          >
            <FileSearch className="h-3.5 w-3.5" />
            {t('explain.title')}
          </Button>
        )}
        <span className="text-[11px] text-fg-muted">⌘+Enter {t('query.execute')}</span>
        <div className="flex-1" />
        {tab.executionTimeMs != null && (
          <span className="text-[11px] text-fg-muted">{t('query.totalTime')} {tab.executionTimeMs} ms</span>
        )}
        <Button
          variant={historyVisible ? 'secondary' : 'ghost'}
          className="h-7 gap-1 px-2 text-xs"
          onClick={toggleHistory}
        >
          <Clock className="h-3.5 w-3.5" />
          {t('query.history')}
        </Button>
        <Button
          variant={favoritesVisible ? 'secondary' : 'ghost'}
          className="h-7 gap-1 px-2 text-xs"
          onClick={toggleFavorites}
        >
          <Bookmark className="h-3.5 w-3.5" />
          {t('query.favorites')}
        </Button>
        <Button
          variant={nl2sqlVisible ? 'secondary' : 'ghost'}
          className="h-7 gap-1 px-2 text-xs"
          onClick={() => setNl2sqlVisible((v) => !v)}
        >
          <Sparkles className="h-3.5 w-3.5" />
          {t('nl2sql.title')}
        </Button>
      </div>

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
          <div
            className="relative shrink-0 border-b border-edge"
            style={{ height: editorHeight }}
          >
            <SqlEditor
              ref={editorRef}
              value={tab.sql}
              onChange={(v) => updateSql(tab.id, v)}
              onExecute={handleExecute}
              onExecuteSelection={handleExecuteSelection}
              onContextMenu={handleEditorContextMenu}
              placeholder={t('query.placeholder')}
              schema={editorSchema}
              databaseType={databaseType}
            />
          </div>
          <div
            ref={editorResizeRef}
            className="h-1.5 shrink-0 cursor-row-resize bg-transparent hover:bg-accent/30 active:bg-accent/40"
            title="Drag to resize editor"
          />

          {showFavoriteDialog && (
            <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" onClick={() => setShowFavoriteDialog(false)}>
              <div
                className="w-[400px] rounded-lg border border-edge bg-surface p-4 shadow-xl"
                onClick={(e) => e.stopPropagation()}
              >
                <div className="mb-3 text-sm font-medium text-fg">{t('query.addFavorite')}</div>
                <div className="mb-2">
                  <label className="mb-1 block text-xs text-fg-muted">{t('query.favoriteTitle')}</label>
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
                    <span className="absolute bottom-0 left-0 right-0 h-0.5 bg-accent" />
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
                    onApplySql={handleApplyAiSql}
                  />
                )}
              </div>
            )}

            {!showExplain && tab.running && (
              <div className="flex flex-1 items-center justify-center gap-2 text-fg-muted">
                <Loader2 className="h-5 w-5 animate-spin" />
                {t('query.executing')}
              </div>
            )}

            {!showExplain && tab.error && !tab.running && (
              <div className="flex-1 overflow-auto">
                <div className="p-4">
                  <div className="flex items-start gap-2 rounded-md border border-red-500/20 bg-red-500/10 px-4 py-3">
                    <span className="flex-1 text-sm text-red-400">{tab.error}</span>
                    {currentDatabase && (
                      <button
                        type="button"
                        className="shrink-0 rounded px-2 py-0.5 text-[11px] text-blue-400 hover:bg-blue-500/10"
                        onClick={() => setDiagnosisVisible(true)}
                      >
                        <span className="flex items-center gap-1">
                          <Stethoscope className="h-3 w-3" />
                          {t('diagnosis.diagnose')}
                        </span>
                      </button>
                    )}
                  </div>
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

            {!showExplain && results.length > 0 && !tab.running && (
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
                          ({r.rows.length} {t('common.rows')}, {r.executionTimeMs}ms)
                        </span>
                        {idx === activeResultIdx && (
                          <span className="absolute bottom-0 left-0 right-0 h-0.5 bg-accent" />
                        )}
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
                    </div>
                    {resultViewMode === 'table' ? (
                      <ResultTable result={activeResult} />
                    ) : (
                      <ChartView
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
              <div className="px-3 py-4 text-center text-xs text-fg-muted">{t('query.noFavorites')}</div>
            ) : (
              favorites.map((f) => (
                <div
                  key={f.id}
                  className="group flex w-full items-start border-b border-edge px-3 py-2 hover:bg-surface-raised"
                >
                  <button
                    type="button"
                    className="min-w-0 flex-1 text-left"
                    onClick={() => updateSql(tab.id, f.sql)}
                  >
                    <div className="truncate text-xs font-medium text-fg">{f.title}</div>
                    <div className="mt-0.5 truncate font-mono text-[11px] text-fg-muted">{f.sql}</div>
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
            <div className="border-b border-edge px-3 py-2 text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
              {t('query.historyTitle')}
            </div>
            {history.length === 0 ? (
              <div className="px-3 py-4 text-center text-xs text-fg-muted">{t('query.noHistory')}</div>
            ) : (
              history.map((h) => (
                <button
                  key={h.id}
                  type="button"
                  className="w-full border-b border-edge px-3 py-2 text-left hover:bg-surface-raised"
                  onClick={() => updateSql(tab.id, h.sql)}
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
        <span>{result.rows.length} {t('common.rows')}</span>
        <span className="text-edge">|</span>
        <span>{result.columns.length} {t('common.columns')}</span>
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
      onRowClick={setResultDetailRow}
      highlightedRow={resultDetailRowIndex}
      exportTableName="query_result"
    />
  );
}
