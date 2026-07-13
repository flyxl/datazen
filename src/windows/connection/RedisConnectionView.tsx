import { useCallback, useEffect, useRef, useState } from 'react';
import {
  Database,
  Key,
  Loader2,
  RefreshCw,
  Search,
  X,
} from 'lucide-react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { Button } from '../../components/ui/Button';
import { Input } from '../../components/ui/Input';
import { SqlEditor } from '../../components/SqlEditor';
import { useSchemaStore } from '../../stores/schemaStore';
import { useQueryStore } from '../../stores/queryStore';
import { useColumnResize } from '../../hooks/useColumnResize';
import { useI18n } from '../../hooks/useI18n';
import { databaseCommands } from '../../commands/database';
import { cn } from '../../lib/cn';
import type { KeyEntry, KeyDetail as KeyDetailType, StatementResult } from '../../types';

const ROW_HEIGHT = 32;
const PAGE_SIZE = 200;

import type { ConnectionViewProps } from '../../lib/connectionViews/types';

type ActiveTab = 'items' | 'queries';

export function RedisConnectionView({
  connectionId,
  connectionName,
  initialDatabase,
}: ConnectionViewProps) {
  const { t } = useI18n();

  const databases = useSchemaStore((s) => s.databases);
  const loading = useSchemaStore((s) => s.loading);
  const loadForConnection = useSchemaStore((s) => s.loadForConnection);

  const [selectedDb, setSelectedDb] = useState<string | null>(null);
  const [dbIndex, setDbIndex] = useState(0);
  const [activeTab, setActiveTab] = useState<ActiveTab>('items');

  // Key browser state
  const [keys, setKeys] = useState<KeyEntry[]>([]);
  const [cursor, setCursor] = useState(0);
  const [dbSize, setDbSize] = useState(0);
  const [keysLoading, setKeysLoading] = useState(false);
  const [searchPattern, setSearchPattern] = useState('*');
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [keyDetail, setKeyDetail] = useState<KeyDetailType | null>(null);
  const [keyDetailLoading, setKeyDetailLoading] = useState(false);

  useEffect(() => {
    void loadForConnection(connectionId, { skipLoadTables: true });
  }, [connectionId, loadForConnection]);

  useEffect(() => {
    if (databases.length > 0 && !selectedDb) {
      const initial = initialDatabase
        ? databases.find((d) => d === initialDatabase)
        : databases[0];
      if (initial) handleSelectDb(initial);
    }
  }, [databases, initialDatabase, selectedDb]);

  const handleSelectDb = useCallback(
    (db: string) => {
      const idx = parseInt(db.replace('db', ''), 10) || 0;
      setSelectedDb(db);
      setDbIndex(idx);
      setKeys([]);
      setCursor(0);
      setDbSize(0);
      setSelectedKey(null);
      setKeyDetail(null);
      setSearchPattern('*');
      void loadKeys(idx, '*', 0, true);
    },
    [connectionId],
  );

  const loadKeys = useCallback(
    async (idx: number, pattern: string, cur: number, reset: boolean) => {
      setKeysLoading(true);
      try {
        const result = await databaseCommands.kvScanKeys(
          connectionId,
          idx,
          pattern || '*',
          cur,
          PAGE_SIZE,
        );
        if (reset) {
          setKeys(result.keys);
        } else {
          setKeys((prev) => [...prev, ...result.keys]);
        }
        setCursor(result.cursor);
        setDbSize(result.dbSize);
      } catch (e) {
        console.error('kvScanKeys failed:', e);
      } finally {
        setKeysLoading(false);
      }
    },
    [connectionId],
  );

  const handleLoadMore = useCallback(() => {
    if (cursor !== 0) {
      void loadKeys(dbIndex, searchPattern, cursor, false);
    }
  }, [dbIndex, searchPattern, cursor, loadKeys]);

  const handleSearch = useCallback(() => {
    setKeys([]);
    setCursor(0);
    setSelectedKey(null);
    setKeyDetail(null);
    void loadKeys(dbIndex, searchPattern, 0, true);
  }, [dbIndex, searchPattern, loadKeys]);

  const handleRefresh = useCallback(() => {
    void loadForConnection(connectionId);
    if (selectedDb) {
      setKeys([]);
      setCursor(0);
      setSelectedKey(null);
      setKeyDetail(null);
      void loadKeys(dbIndex, searchPattern, 0, true);
    }
  }, [connectionId, selectedDb, dbIndex, searchPattern, loadForConnection, loadKeys]);

  const handleSelectKey = useCallback(
    async (key: string) => {
      setSelectedKey(key);
      setKeyDetailLoading(true);
      try {
        const detail = await databaseCommands.kvGetKey(connectionId, dbIndex, key);
        setKeyDetail(detail);
      } catch (e) {
        console.error('kvGetKey failed:', e);
        setKeyDetail(null);
      } finally {
        setKeyDetailLoading(false);
      }
    },
    [connectionId, dbIndex],
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* Top tabs: Items | Queries */}
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
        <div className="flex min-h-0 flex-1">
          {/* Left sidebar: database list */}
          <aside className="flex w-48 shrink-0 flex-col overflow-y-auto border-r border-edge bg-surface-alt">
            {/* Search */}
            <div className="border-b border-edge p-2">
              <div className="relative">
                <Search className="pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-fg-muted" />
                <Input
                  value={searchPattern}
                  onChange={(e) => setSearchPattern(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') handleSearch();
                  }}
                  placeholder={t('redis.searchKeys')}
                  className="h-7 pl-7 text-xs"
                />
              </div>
            </div>

            {loading && (
              <div className="flex items-center gap-2 px-3 py-2 text-xs text-fg-muted">
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
                {t('common.loading')}
              </div>
            )}

            {databases.map((db) => (
              <button
                key={db}
                type="button"
                className={cn(
                  'flex items-center gap-2 px-3 py-2 text-left text-sm transition-colors',
                  selectedDb === db
                    ? 'bg-blue-500/10 text-blue-400 font-medium'
                    : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
                )}
                onClick={() => handleSelectDb(db)}
              >
                <Database className="h-4 w-4 shrink-0" />
                {db}
              </button>
            ))}
          </aside>

          {/* Right panel: key browser */}
          <div className="flex min-h-0 min-w-0 flex-1 flex-col">
            {selectedDb ? (
              <>
                {/* Info bar */}
                <div className="flex shrink-0 items-center gap-3 border-b border-edge bg-surface-alt px-3 py-1.5 text-xs text-fg-secondary">
                  <span>{selectedDb}</span>
                  <span className="text-edge">|</span>
                  <span>{t('redis.dbSize').replace('{count}', String(dbSize))}</span>
                  <span className="text-edge">|</span>
                  <span>
                    {keys.length} loaded
                    {cursor !== 0 && ` (${t('redis.loadMore')}…)`}
                  </span>
                </div>

                {/* Key table + detail split */}
                <div className="flex min-h-0 flex-1">
                  {/* Key list table */}
                  <div className="flex min-w-0 flex-1 flex-col">
                    <KeyTable
                      keys={keys}
                      selectedKey={selectedKey}
                      onSelectKey={handleSelectKey}
                      loading={keysLoading}
                      hasMore={cursor !== 0}
                      onLoadMore={handleLoadMore}
                    />
                  </div>

                  {/* Key detail panel */}
                  {selectedKey && (
                    <div className="flex w-[400px] shrink-0 flex-col border-l border-edge">
                      <div className="flex items-center justify-between border-b border-edge bg-surface-alt px-3 py-2">
                        <span className="truncate text-xs font-medium text-fg">
                          {selectedKey}
                        </span>
                        <button
                          type="button"
                          className="rounded p-1 text-fg-muted hover:bg-surface-raised hover:text-fg"
                          onClick={() => {
                            setSelectedKey(null);
                            setKeyDetail(null);
                          }}
                        >
                          <X className="h-3.5 w-3.5" />
                        </button>
                      </div>
                      <div className="flex-1 overflow-auto p-3">
                        {keyDetailLoading ? (
                          <div className="flex items-center gap-2 text-xs text-fg-muted">
                            <Loader2 className="h-3.5 w-3.5 animate-spin" />
                            {t('common.loading')}
                          </div>
                        ) : keyDetail ? (
                          <KeyDetailView detail={keyDetail} />
                        ) : null}
                      </div>
                    </div>
                  )}
                </div>
              </>
            ) : (
              <div className="flex flex-1 items-center justify-center text-fg-muted">
                <div className="text-center">
                  <Database className="mx-auto h-10 w-10 opacity-20" />
                  <div className="mt-3 text-sm">{t('redis.selectDb')}</div>
                </div>
              </div>
            )}
          </div>
        </div>
      ) : (
        <RedisQueryPanel connectionId={connectionId} />
      )}
    </div>
  );
}

// ── Key Table ──

function KeyTable({
  keys,
  selectedKey,
  onSelectKey,
  loading,
  hasMore,
  onLoadMore,
}: {
  keys: KeyEntry[];
  selectedKey: string | null;
  onSelectKey: (key: string) => void;
  loading: boolean;
  hasMore: boolean;
  onLoadMore: () => void;
}) {
  const { t } = useI18n();
  const scrollRef = useRef<HTMLDivElement>(null);
  const { columnWidths, onResizeStart } = useColumnResize({
    count: 4,
  });

  const virtualizer = useVirtualizer({
    count: keys.length + (hasMore ? 1 : 0),
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 20,
  });

  const TYPE_COLORS: Record<string, string> = {
    string: 'text-green-400',
    hash: 'text-blue-400',
    list: 'text-orange-400',
    set: 'text-purple-400',
    zset: 'text-yellow-400',
    stream: 'text-cyan-400',
  };

  return (
    <div ref={scrollRef} className="min-h-0 flex-1 overflow-auto">
      <div className="min-w-max text-[13px]">
        {/* Header */}
        <div className="sticky top-0 z-10 flex bg-surface-alt">
          {[t('redis.key'), t('redis.type'), t('redis.ttl'), t('redis.preview')].map(
            (col, ci) => (
              <div
                key={col}
                className="relative shrink-0 border-b border-r border-edge px-3 py-2 text-left text-xs font-medium text-fg-secondary"
                style={{
                  width: columnWidths[ci],
                  ...(ci === 0 || ci === 3
                    ? { flex: '1 1 0', minWidth: 120 }
                    : {}),
                }}
              >
                {col}
                <div
                  className="absolute right-0 top-0 z-20 h-full w-[5px] cursor-col-resize hover:bg-accent/40 active:bg-accent/60"
                  onPointerDown={(e) => {
                    e.preventDefault();
                    onResizeStart(ci, e.clientX);
                  }}
                />
              </div>
            ),
          )}
        </div>

        {/* Rows */}
        <div
          style={{ height: virtualizer.getTotalSize(), position: 'relative' }}
        >
          {virtualizer.getVirtualItems().map((vRow) => {
            if (vRow.index >= keys.length) {
              return (
                <div
                  key="load-more"
                  className="absolute left-0 flex w-full items-center justify-center border-b border-edge"
                  style={{ top: vRow.start, height: ROW_HEIGHT }}
                >
                  <button
                    type="button"
                    className="text-xs text-blue-400 hover:underline"
                    onClick={onLoadMore}
                    disabled={loading}
                  >
                    {loading ? (
                      <Loader2 className="inline h-3.5 w-3.5 animate-spin" />
                    ) : (
                      t('redis.loadMore')
                    )}
                  </button>
                </div>
              );
            }

            const entry = keys[vRow.index];
            const isSelected = selectedKey === entry.key;

            return (
              <div
                key={entry.key}
                className={cn(
                  'absolute left-0 flex w-full cursor-pointer border-b border-edge',
                  isSelected
                    ? 'bg-blue-500/10'
                    : vRow.index % 2 === 0
                      ? 'bg-surface'
                      : 'bg-surface-raised/50',
                  'hover:bg-blue-500/5',
                )}
                style={{ top: vRow.start, height: ROW_HEIGHT }}
                onClick={() => onSelectKey(entry.key)}
              >
                <div
                  className="flex shrink-0 items-center overflow-hidden border-r border-edge px-3 font-mono"
                  style={{ flex: '1 1 0', minWidth: 120, width: columnWidths[0] }}
                >
                  <Key className="mr-1.5 h-3 w-3 shrink-0 text-fg-muted" />
                  <span className="truncate text-fg-secondary">{entry.key}</span>
                </div>
                <div
                  className="flex shrink-0 items-center overflow-hidden border-r border-edge px-3"
                  style={{ width: columnWidths[1] }}
                >
                  <span
                    className={cn(
                      'text-xs font-medium',
                      TYPE_COLORS[entry.keyType] ?? 'text-fg-muted',
                    )}
                  >
                    {entry.keyType}
                  </span>
                </div>
                <div
                  className="flex shrink-0 items-center overflow-hidden border-r border-edge px-3 text-xs text-fg-muted"
                  style={{ width: columnWidths[2] }}
                >
                  {entry.ttl < 0 ? '∞' : `${entry.ttl}s`}
                </div>
                <div
                  className="flex shrink-0 items-center overflow-hidden border-r border-edge px-3 font-mono text-fg-secondary"
                  style={{ flex: '1 1 0', minWidth: 120, width: columnWidths[3] }}
                >
                  <span className="truncate">{entry.preview}</span>
                </div>
              </div>
            );
          })}
        </div>

        {keys.length === 0 && !loading && (
          <div className="px-4 py-8 text-center text-xs text-fg-muted">
            {t('redis.noKeys')}
          </div>
        )}

        {loading && keys.length === 0 && (
          <div className="flex items-center justify-center gap-2 py-8 text-xs text-fg-muted">
            <Loader2 className="h-4 w-4 animate-spin" />
            {t('common.loading')}
          </div>
        )}
      </div>
    </div>
  );
}

// ── Key Detail View ──

function KeyDetailView({ detail }: { detail: KeyDetailType }) {
  const { t } = useI18n();
  const ttlText =
    detail.ttl < 0
      ? t('redis.noExpiry')
      : `${detail.ttl} ${t('redis.seconds')}`;

  return (
    <div className="space-y-3 text-xs">
      <div className="flex items-center gap-2">
        <span className="font-medium text-fg-muted">{t('redis.type')}:</span>
        <span className="rounded bg-blue-500/10 px-1.5 py-0.5 text-blue-400">
          {detail.keyType}
        </span>
        <span className="font-medium text-fg-muted">TTL:</span>
        <span className="text-fg-secondary">{ttlText}</span>
      </div>

      {detail.keyType === 'string' && (
        <div className="rounded-md border border-edge bg-surface-alt p-3">
          <pre className="whitespace-pre-wrap break-all font-mono text-fg-secondary">
            {typeof detail.value === 'object'
              ? JSON.stringify(detail.value, null, 2)
              : String(detail.value)}
          </pre>
        </div>
      )}

      {detail.keyType === 'hash' &&
        typeof detail.value === 'object' &&
        detail.value !== null && (
          <table className="w-full border-collapse">
            <thead>
              <tr className="border-b border-edge bg-surface-alt text-left">
                <th className="px-2 py-1.5 font-medium text-fg-muted">
                  {t('redis.field')}
                </th>
                <th className="px-2 py-1.5 font-medium text-fg-muted">
                  {t('redis.value')}
                </th>
              </tr>
            </thead>
            <tbody>
              {Object.entries(
                (detail.value as Record<string, Record<string, string>>)
                  .fields ?? detail.value,
              ).map(([k, v]) => (
                <tr key={k} className="border-b border-edge">
                  <td className="px-2 py-1.5 font-mono text-fg-secondary">
                    {k}
                  </td>
                  <td className="px-2 py-1.5 font-mono text-fg-secondary">
                    {String(v)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}

      {(detail.keyType === 'list' || detail.keyType === 'set') &&
        Array.isArray(
          (detail.value as Record<string, unknown>)?.items ??
            (detail.value as Record<string, unknown>)?.members,
        ) && (
          <table className="w-full border-collapse">
            <thead>
              <tr className="border-b border-edge bg-surface-alt text-left">
                <th className="w-16 px-2 py-1.5 font-medium text-fg-muted">#</th>
                <th className="px-2 py-1.5 font-medium text-fg-muted">
                  {t('redis.value')}
                </th>
              </tr>
            </thead>
            <tbody>
              {(
                ((detail.value as Record<string, unknown>)?.items ??
                  (detail.value as Record<string, unknown>)?.members) as string[]
              ).map((item, i) => (
                <tr key={i} className="border-b border-edge">
                  <td className="px-2 py-1.5 text-fg-muted">{i}</td>
                  <td className="px-2 py-1.5 font-mono text-fg-secondary">
                    {String(item)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}

      {detail.keyType === 'zset' &&
        Array.isArray(
          (detail.value as Record<string, unknown>)?.members,
        ) && (
          <table className="w-full border-collapse">
            <thead>
              <tr className="border-b border-edge bg-surface-alt text-left">
                <th className="px-2 py-1.5 font-medium text-fg-muted">
                  {t('redis.score')}
                </th>
                <th className="px-2 py-1.5 font-medium text-fg-muted">
                  {t('redis.member')}
                </th>
              </tr>
            </thead>
            <tbody>
              {(
                (detail.value as Record<string, { member: string; score: number }[]>)
                  .members
              ).map((item, i) => (
                <tr key={i} className="border-b border-edge">
                  <td className="px-2 py-1.5 text-fg-secondary">
                    {item.score}
                  </td>
                  <td className="px-2 py-1.5 font-mono text-fg-secondary">
                    {item.member}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}

      {!['string', 'hash', 'list', 'set', 'zset'].includes(detail.keyType) && (
        <div className="rounded-md border border-edge bg-surface-alt p-3">
          <pre className="whitespace-pre-wrap break-all font-mono text-fg-secondary">
            {JSON.stringify(detail.value, null, 2)}
          </pre>
        </div>
      )}
    </div>
  );
}

// ── Redis Query Panel ──

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
      {/* Toolbar */}
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

      {/* Editor */}
      <div className="min-h-[100px] border-b border-edge" style={{ height: '30%' }}>
        <SqlEditor
          value={tab.sql}
          onChange={(v) => updateSql(tab.id, v)}
          onExecute={handleExecute}
          placeholder="GET key\nHGETALL user:1\nSET key value"
          schema={{}}
        />
      </div>

      {/* Results */}
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
                    vRow.index % 2 === 0
                      ? 'bg-surface'
                      : 'bg-surface-raised/50',
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
