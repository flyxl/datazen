import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { AlertTriangle, Clock, Loader2, Play, Square } from 'lucide-react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { Button } from '../../components/ui/Button';
import { SqlEditor } from '../../components/SqlEditor';
import type { SqlSchema } from '../../components/SqlEditor';
import { useQueryStore } from '../../stores/queryStore';
import { useSchemaStore } from '../../stores/schemaStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useColumnResize } from '../../hooks/useColumnResize';
import { databaseCommands } from '../../commands/database';
import { cn } from '../../lib/cn';
import type { StatementResult } from '../../types';

interface QueryPanelProps {
  connectionId: string;
  queryTabId: string;
}

export function QueryPanel({ connectionId, queryTabId }: QueryPanelProps) {
  const tab = useQueryStore((s) => s.tabs.find((t) => t.id === queryTabId));
  const historyVisible = useQueryStore((s) => s.historyVisible);
  const history = useQueryStore((s) => s.history);
  const setConnectionId = useQueryStore((s) => s.setConnectionId);
  const updateSql = useQueryStore((s) => s.updateSql);
  const setActiveResult = useQueryStore((s) => s.setActiveResult);
  const executeQuery = useQueryStore((s) => s.executeQuery);
  const cancelQuery = useQueryStore((s) => s.cancelQuery);
  const loadHistory = useQueryStore((s) => s.loadHistory);
  const toggleHistory = useQueryStore((s) => s.toggleHistory);

  const tables = useSchemaStore((s) => s.tables);
  const views = useSchemaStore((s) => s.views);

  const [columnCache, setColumnCache] = useState<Record<string, string[]>>({});

  const allTableNames = useMemo(
    () => [...tables, ...views].map((t) => t.name),
    [tables, views],
  );

  useEffect(() => {
    if (!connectionId || allTableNames.length === 0) return;
    let cancelled = false;

    const missing = allTableNames.filter((name) => !(name in columnCache));
    if (missing.length === 0) return;

    Promise.all(
      missing.map((name) =>
        databaseCommands
          .getTableSchema(connectionId, name)
          .then((schema) => [name, schema.columns.map((c) => c.name)] as const)
          .catch(() => [name, [] as string[]] as const),
      ),
    ).then((entries) => {
      if (cancelled) return;
      setColumnCache((prev) => {
        const next = { ...prev };
        for (const [name, cols] of entries) {
          next[name] = cols;
        }
        return next;
      });
    });

    return () => { cancelled = true; };
  }, [connectionId, allTableNames, columnCache]);

  const editorSchema: SqlSchema = useMemo(() => {
    const result: SqlSchema = {};
    for (const name of allTableNames) {
      result[name] = columnCache[name] ?? [];
    }
    return result;
  }, [allTableNames, columnCache]);

  useEffect(() => {
    setConnectionId(connectionId);
    void loadHistory();
  }, [connectionId, setConnectionId, loadHistory]);

  const handleExecute = useCallback(() => {
    if (tab) void executeQuery(tab.id);
  }, [tab, executeQuery]);

  const handleCancel = useCallback(() => {
    if (tab) void cancelQuery(tab.id);
  }, [tab, cancelQuery]);

  if (!tab) return null;

  const { results, activeResultIdx } = tab;
  const activeResult: StatementResult | undefined = results[activeResultIdx];

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* Toolbar */}
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-edge bg-surface-alt px-3">
        <Button
          variant="primary"
          className="h-7 gap-1 px-2 text-xs"
          onClick={handleExecute}
          disabled={tab.running}
        >
          <Play className="h-3.5 w-3.5" />
          执行
        </Button>
        {tab.running && (
          <Button variant="danger" className="h-7 gap-1 px-2 text-xs" onClick={handleCancel}>
            <Square className="h-3.5 w-3.5" />
            停止
          </Button>
        )}
        <span className="text-[11px] text-fg-muted">⌘+Enter 执行</span>
        <div className="flex-1" />
        {tab.executionTimeMs != null && (
          <span className="text-[11px] text-fg-muted">总耗时 {tab.executionTimeMs} ms</span>
        )}
        <Button
          variant={historyVisible ? 'secondary' : 'ghost'}
          className="h-7 gap-1 px-2 text-xs"
          onClick={toggleHistory}
        >
          <Clock className="h-3.5 w-3.5" />
          历史
        </Button>
      </div>

      {/* Editor + results (vertical split) */}
      <div className="flex min-h-0 flex-1">
        <div className="flex min-w-0 flex-1 flex-col">
          {/* SQL editor */}
          <div className="min-h-[100px] border-b border-edge" style={{ height: '35%' }}>
            <SqlEditor
              value={tab.sql}
              onChange={(v) => updateSql(tab.id, v)}
              onExecute={handleExecute}
              placeholder="输入 SQL 查询语句…"
              schema={editorSchema}
            />
          </div>

          {/* Results area */}
          <div className="flex min-h-0 flex-1 flex-col">
            {tab.running && (
              <div className="flex flex-1 items-center justify-center gap-2 text-fg-muted">
                <Loader2 className="h-5 w-5 animate-spin" />
                执行中…
              </div>
            )}

            {tab.error && !tab.running && (
              <div className="flex-1 overflow-auto p-4">
                <div className="rounded-md border border-red-500/20 bg-red-500/10 px-4 py-3 text-sm text-red-400">
                  {tab.error}
                </div>
              </div>
            )}

            {results.length > 0 && !tab.running && (
              <>
                {/* Result tabs — only show when there are multiple results */}
                {results.length > 1 && (
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
                        结果 {idx + 1}
                        <span className="ml-1.5 text-[10px] text-fg-muted">
                          ({r.rows.length} 行, {r.executionTimeMs}ms)
                        </span>
                        {idx === activeResultIdx && (
                          <span className="absolute bottom-0 left-0 right-0 h-0.5 bg-accent" />
                        )}
                      </button>
                    ))}
                  </div>
                )}

                {/* Active result table */}
                {activeResult && <ResultTable result={activeResult} />}
              </>
            )}

            {results.length === 0 && !tab.running && !tab.error && (
              <div className="flex flex-1 items-center justify-center text-sm text-fg-muted">
                按 ⌘+Enter 执行查询
              </div>
            )}
          </div>
        </div>

        {/* History panel */}
        {historyVisible && (
          <aside className="w-64 shrink-0 overflow-y-auto border-l border-edge bg-surface-alt">
            <div className="border-b border-edge px-3 py-2 text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
              查询历史
            </div>
            {history.length === 0 ? (
              <div className="px-3 py-4 text-center text-xs text-fg-muted">暂无历史记录</div>
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
                      {h.success ? '成功' : '失败'}
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

const ROW_HEIGHT = 32;

function ResultTable({ result }: { result: StatementResult }) {
  const { columns, rows } = result;
  const queryResultLimit = useSettingsStore((s) => s.settings.queryResultLimit);
  const scrollRef = useRef<HTMLDivElement>(null);
  const { columnWidths, onResizeStart } = useColumnResize({ count: columns.length });

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 20,
  });

  return (
    <>
      <div className="flex items-center gap-3 border-b border-edge bg-surface-alt px-3 py-1.5 text-xs text-fg-secondary">
        <span>{rows.length} 行</span>
        <span className="text-edge">|</span>
        <span>{columns.length} 列</span>
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
              结果已截断，仅显示前 {queryResultLimit} 条
            </span>
          </>
        )}
      </div>
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-auto">
        <div className="min-w-max text-[13px]">
          {/* Sticky header */}
          <div className="sticky top-0 z-10 flex bg-surface-alt">
            <div className="w-[60px] shrink-0 border-b border-r border-edge px-3 py-2 text-left text-xs font-medium text-fg-muted">
              #
            </div>
            {columns.map((col, ci) => (
              <div
                key={col.name}
                className="relative shrink-0 border-b border-r border-edge px-3 py-2 text-left text-xs font-medium text-fg-secondary"
                style={{ width: columnWidths[ci] }}
              >
                {col.name}
                <div
                  className="absolute right-0 top-0 z-20 h-full w-[5px] cursor-col-resize hover:bg-accent/40 active:bg-accent/60"
                  onPointerDown={(e) => {
                    e.preventDefault();
                    onResizeStart(ci, e.clientX);
                  }}
                />
              </div>
            ))}
          </div>
          {/* Virtualized rows */}
          <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
            {virtualizer.getVirtualItems().map((vRow) => {
              const row = rows[vRow.index];
              return (
                <div
                  key={vRow.index}
                  className={cn(
                    'absolute left-0 flex w-full border-b border-edge',
                    vRow.index % 2 === 0 ? 'bg-surface' : 'bg-surface-raised/50',
                  )}
                  style={{ top: vRow.start, height: ROW_HEIGHT }}
                >
                  <div className="flex w-[60px] shrink-0 items-center border-r border-edge px-3 text-xs text-fg-muted">
                    {vRow.index + 1}
                  </div>
                  {row.map((cell, ci) => (
                    <div
                      key={columns[ci]?.name ?? ci}
                      className="flex shrink-0 items-center overflow-hidden border-r border-edge px-3 font-mono"
                      style={{ width: columnWidths[ci] }}
                    >
                      {cell === null || cell === undefined ? (
                        <span className="text-fg-muted italic">NULL</span>
                      ) : (
                        <span className="truncate text-fg-secondary">{String(cell)}</span>
                      )}
                    </div>
                  ))}
                </div>
              );
            })}
          </div>
        </div>
      </div>
    </>
  );
}
