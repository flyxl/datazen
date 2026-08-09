import { useCallback, useEffect, useRef, useState } from 'react';
import { Loader2, RefreshCw } from 'lucide-react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { Button } from '../../components/ui/Button';
import { SqlEditor } from '../../components/SqlEditor';
import { useSchemaStore } from '../../stores/schemaStore';
import { useQueryStore } from '../../stores/queryStore';
import { useColumnResize } from '../../hooks/useColumnResize';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import type { ConnectionViewProps } from '../../lib/connectionViews/types';
import type { StatementResult } from '../../types';
import {
  RedisWorkbench,
  type RedisWorkbenchHandle,
} from '../../../packages/drivers/redis/ui/RedisWorkbench';

const ROW_HEIGHT = 32;

type ActiveTab = 'items' | 'queries';

export function RedisConnectionView({
  connectionId,
  connectionName,
  initialDatabase,
}: ConnectionViewProps) {
  const { t } = useI18n();
  const loadForConnection = useSchemaStore((s) => s.loadForConnection);
  const [activeTab, setActiveTab] = useState<ActiveTab>('items');
  const workbenchRef = useRef<RedisWorkbenchHandle>(null);

  const handleRefresh = useCallback(() => {
    void loadForConnection(connectionId, { skipLoadTables: true });
    workbenchRef.current?.refreshKeys();
  }, [connectionId, loadForConnection]);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-edge bg-surface-alt px-4">
        <Button
          variant="secondary"
          className="h-8 w-8 !px-0"
          title={t('connWin.refresh')}
          onClick={handleRefresh}
        >
          <RefreshCw className="h-4 w-4" />
        </Button>
        <div className="mx-1 h-6 w-px bg-edge" />
        {(['items', 'queries'] as const).map((tab) => (
          <button
            key={tab}
            type="button"
            className={cn(
              'relative px-4 py-3 text-sm transition-colors',
              activeTab === tab
                ? 'text-fg font-medium'
                : 'text-fg-secondary hover:text-fg',
            )}
            onClick={() => setActiveTab(tab)}
          >
            {tab === 'items' ? t('redis.items') : t('redis.queries')}
            {activeTab === tab && (
              <span className="absolute inset-x-0 bottom-0 h-0.5 bg-blue-500" />
            )}
          </button>
        ))}
        <div className="flex-1" />
        <span className="text-xs text-fg-muted">{connectionName}</span>
      </div>

      {activeTab === 'items' ? (
        <RedisWorkbench
          ref={workbenchRef}
          connectionId={connectionId}
          initialDatabase={initialDatabase}
        />
      ) : (
        <RedisQueryPanel connectionId={connectionId} />
      )}
    </div>
  );
}

function RedisQueryPanel({ connectionId }: { connectionId: string }) {
  const { t } = useI18n();
  const tab = useQueryStore((s) => s.tabs[0]);
  const setConnectionId = useQueryStore((s) => s.setConnectionId);
  const updateSql = useQueryStore((s) => s.updateSql);
  const executeQuery = useQueryStore((s) => s.executeQuery);
  const createTab = useQueryStore((s) => s.createTab);
  const setActiveResult = useQueryStore((s) => s.setActiveResult);

  useEffect(() => {
    setConnectionId(connectionId);
    if (!tab) createTab();
  }, [connectionId, setConnectionId, tab, createTab]);

  const handleExecute = useCallback(() => {
    if (tab) void executeQuery(tab.id);
  }, [tab, executeQuery]);

  if (!tab) return null;

  const { results, activeResultIdx } = tab;
  const activeResult: StatementResult | undefined = results[activeResultIdx];

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-edge bg-surface-alt px-3">
        <Button
          variant="primary"
          className="h-7 gap-1 px-2 text-xs"
          onClick={handleExecute}
          disabled={tab.running}
        >
          {t('query.execute')}
        </Button>
        <span className="text-[11px] text-fg-muted">
          ⌘+Enter — {t('redis.queries')}
        </span>
        <div className="flex-1" />
        {tab.executionTimeMs != null && (
          <span className="text-[11px] text-fg-muted">
            {tab.executionTimeMs} ms
          </span>
        )}
      </div>

      <div className="min-h-[100px] border-b border-edge" style={{ height: '30%' }}>
        <SqlEditor
          value={tab.sql}
          onChange={(v) => updateSql(tab.id, v)}
          onExecute={handleExecute}
          placeholder="GET key\nHGETALL user:1\nSET key value"
          schema={{}}
        />
      </div>

      <div className="flex min-h-0 flex-1 flex-col">
        {tab.running && (
          <div className="flex flex-1 items-center justify-center gap-2 text-fg-muted">
            <Loader2 className="h-5 w-5 animate-spin" />
            {t('query.executing')}
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
            {results.length > 1 && (
              <div className="flex shrink-0 items-center border-b border-edge bg-surface-alt px-1">
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
                      ({r.executionTimeMs}ms)
                    </span>
                    {idx === activeResultIdx && (
                      <span className="absolute bottom-0 left-0 right-0 h-0.5 bg-accent" />
                    )}
                  </button>
                ))}
              </div>
            )}
            {activeResult && <RedisResultTable result={activeResult} />}
          </>
        )}

        {results.length === 0 && !tab.running && !tab.error && (
          <div className="flex flex-1 items-center justify-center text-sm text-fg-muted">
            输入 Redis 命令并按 ⌘+Enter 执行
          </div>
        )}
      </div>
    </div>
  );
}

function RedisResultTable({ result }: { result: StatementResult }) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const { columns, rows } = result;
  const { columnWidths, onResizeStart } = useColumnResize({
    count: columns.length,
  });

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 20,
  });

  return (
    <>
      <div className="flex items-center gap-3 border-b border-edge bg-surface-alt px-3 py-1.5 text-xs text-fg-secondary">
        <span>{rows.length} rows</span>
        <span className="text-edge">|</span>
        <span>{result.executionTimeMs} ms</span>
        {result.sql && (
          <>
            <span className="text-edge">|</span>
            <span className="max-w-[400px] truncate font-mono text-fg-muted">
              {result.sql}
            </span>
          </>
        )}
      </div>
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-auto">
        <div className="min-w-max text-[13px]">
          <div className="sticky top-0 z-10 flex bg-surface-alt">
            {columns.map((col, ci) => (
              <div
                key={col.name}
                className="relative shrink-0 border-b border-r border-edge px-3 py-2 text-left text-xs font-medium text-fg-secondary"
                style={{ width: columnWidths[ci] }}
              >
                {col.name}
                <div
                  className="absolute right-0 top-0 z-20 h-full w-[5px] cursor-col-resize hover:bg-accent/40"
                  onPointerDown={(e) => {
                    e.preventDefault();
                    onResizeStart(ci, e.clientX);
                  }}
                />
              </div>
            ))}
          </div>
          <div
            style={{
              height: virtualizer.getTotalSize(),
              position: 'relative',
            }}
          >
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
                  {row.map((cell, ci) => (
                    <div
                      key={columns[ci]?.name ?? ci}
                      className="flex shrink-0 items-center overflow-hidden border-r border-edge px-3 font-mono"
                      style={{ width: columnWidths[ci] }}
                    >
                      {cell === null || cell === undefined ? (
                        <span className="text-fg-muted italic">NULL</span>
                      ) : (
                        <span className="truncate text-fg-secondary">
                          {String(cell)}
                        </span>
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
