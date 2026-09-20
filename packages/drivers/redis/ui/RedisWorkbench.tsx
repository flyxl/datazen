import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useState,
  type MouseEvent as ReactMouseEvent,
} from 'react';
import { Database, FolderInput, Loader2, Plus, RefreshCw, Search, X } from 'lucide-react';
import { Button } from '@datazen/ui';
import { Input } from '@datazen/ui';
import { useSchemaStore } from '../../../../src/stores/schemaStore';
import { useSettingsStore } from '../../../../src/stores/settingsStore';
import { useI18n } from '../../../../src/hooks/useI18n';
import { cn } from '../../../../src/lib/cn';
import { showNativeContextMenu } from '../../../../src/lib/nativeContextMenu';
import { readBooleanField } from '../../../../src/lib/driverSettings';
import { invokeGetKey } from './redisInvoke';
import type { KeyDetail } from '../../../../src/types';
import { BatchBar } from './BatchBar';
import { hasRedisJson } from './hasRedisJson';
import { ImportExport } from './ImportExport';
import { invokeModulesList } from './JsonEditor';
import { KeyDetailEditor } from './KeyEditors';
import { buildRedisKeyContextMenuItems } from './redisKeyContextMenu';
import { KeyBrowserControls } from './KeyBrowserControls';
import { KeyTable } from './KeyTable';
import { useRedisKeyScan } from './useRedisKeyScan';
import {
  KeyWorkbenchDialogs,
  openKeyCtxDelete,
  openKeyCtxRename,
  openKeyCtxTtl,
  type KeyCtxDialog,
} from './KeyWorkbenchDialogs';

const REDIS_DB_COUNT = 16;

export interface RedisWorkbenchProps {
  dbSessionId: string;
  initialDatabase?: string;
  hideSidebar?: boolean;
  onDbIndexChange?: (dbIndex: number) => void;
  onDatabaseChange?: (database: string) => void;
  onKeysChange?: (keys: string[]) => void;
}

export interface RedisWorkbenchHandle {
  refreshKeys: () => void;
  selectDatabase: (db: string) => void;
}

function allRedisDbs(): string[] {
  return Array.from({ length: REDIS_DB_COUNT }, (_, i) => `db${i}`);
}

function mergeDatabases(fromServer: string[]): string[] {
  const extras = fromServer.filter((db) => !/^db(\d+)$/.test(db));
  return [...allRedisDbs(), ...extras];
}

export const RedisWorkbench = forwardRef<RedisWorkbenchHandle, RedisWorkbenchProps>(
  function RedisWorkbench(
    { dbSessionId, initialDatabase, hideSidebar, onDbIndexChange, onDatabaseChange, onKeysChange },
    ref,
  ) {
    const { t } = useI18n();
    const databasesFromStore = useSchemaStore((s) => s.databases);
    const loading = useSchemaStore((s) => s.loading);
    const loadForConnection = useSchemaStore((s) => s.loadForConnection);
    const driverSettings = useSettingsStore((s) => s.settings.driverSettings);
    const allowFlush = readBooleanField(
      (driverSettings?.redis ?? {}) as Record<string, unknown>,
      'allowFlush',
      false,
    );

    const databases = useMemo(() => mergeDatabases(databasesFromStore), [databasesFromStore]);

    const [selectedDb, setSelectedDb] = useState<string | null>(null);
    const [dbIndex, setDbIndex] = useState(0);
    const [selectedKey, setSelectedKey] = useState<string | null>(null);
    const [selectedKeys, setSelectedKeys] = useState<Set<string>>(new Set());
    const [keyDetail, setKeyDetail] = useState<KeyDetail | null>(null);
    const [keyDetailLoading, setKeyDetailLoading] = useState(false);
    const [batchSummary, setBatchSummary] = useState<string | null>(null);
    const [modules, setModules] = useState<string[] | null>(null);
    const [importExportOpen, setImportExportOpen] = useState(false);
    const [createOpen, setCreateOpen] = useState(false);
    const [flushDialog, setFlushDialog] = useState<'db' | 'all' | null>(null);
    const [keyCtxDialog, setKeyCtxDialog] = useState<KeyCtxDialog>(null);

    const {
      keys,
      cursor,
      dbSize,
      keysLoading,
      searchPattern,
      setSearchPattern,
      keyTypeFilter,
      setKeyTypeFilter,
      viewMode,
      setViewMode,
      withMemory,
      setWithMemory,
      expandedFolders,
      loadKeys,
      resetSelectionState,
      refresh: scanRefresh,
      loadMore,
      search: scanSearch,
      toggleFolder,
    } = useRedisKeyScan({
      dbSessionId,
      dbIndex,
      enabled: selectedDb !== null,
    });

    useEffect(() => {
      void loadForConnection(dbSessionId, { skipLoadTables: true });
    }, [dbSessionId, loadForConnection]);

    useEffect(() => {
      let cancelled = false;
      setModules(null);
      void invokeModulesList(dbSessionId)
        .then((list) => {
          if (!cancelled) setModules(list);
        })
        .catch(() => {
          if (!cancelled) setModules([]);
        });
      return () => {
        cancelled = true;
      };
    }, [dbSessionId]);

    const createTypes = useMemo(() => {
      const base = ['string', 'hash', 'list', 'set', 'zset'];
      if (modules && hasRedisJson(modules)) {
        return [...base, 'ReJSON'];
      }
      return base;
    }, [modules]);

    const handleSelectDb = useCallback(
      (db: string) => {
        const idx = parseInt(db.replace('db', ''), 10) || 0;
        setSelectedDb(db);
        setDbIndex(idx);
        onDbIndexChange?.(idx);
        onDatabaseChange?.(db);
        setSelectedKey(null);
        setSelectedKeys(new Set());
        setKeyDetail(null);
        setSearchPattern('*');
        resetSelectionState();
        void loadKeys(idx, '*', 0, true);
      },
      [loadKeys, resetSelectionState, setSearchPattern, onDatabaseChange, onDbIndexChange],
    );

    useEffect(() => {
      onKeysChange?.(keys.map((entry) => entry.key));
    }, [keys, onKeysChange]);

    useEffect(() => {
      if (databases.length > 0 && !selectedDb) {
        const initial = initialDatabase
          ? (databases.find((d) => d === initialDatabase) ?? initialDatabase)
          : databases[0];
        if (initial) handleSelectDb(initial);
      }
    }, [databases, initialDatabase, selectedDb, handleSelectDb]);

    const refreshKeys = useCallback(() => {
      if (selectedDb) {
        setSelectedKey(null);
        setSelectedKeys(new Set());
        setKeyDetail(null);
        scanRefresh();
      }
    }, [selectedDb, scanRefresh]);

    const handleRefresh = useCallback(() => {
      void loadForConnection(dbSessionId, { skipLoadTables: true });
      refreshKeys();
    }, [dbSessionId, loadForConnection, refreshKeys]);

    useImperativeHandle(ref, () => ({ refreshKeys, selectDatabase: handleSelectDb }), [
      refreshKeys,
      handleSelectDb,
    ]);

    const handleSearch = useCallback(() => {
      setSelectedKey(null);
      setSelectedKeys(new Set());
      setKeyDetail(null);
      scanSearch();
    }, [scanSearch]);

    const handleSelectKey = useCallback(
      async (key: string) => {
        setSelectedKey(key);
        setKeyDetailLoading(true);
        try {
          const detail = await invokeGetKey(dbSessionId, dbIndex, key);
          setKeyDetail(detail);
        } catch (e) {
          console.error('get_key failed:', e);
          setKeyDetail(null);
        } finally {
          setKeyDetailLoading(false);
        }
      },
      [dbSessionId, dbIndex],
    );

    const reloadDetail = useCallback(async () => {
      if (!selectedKey) return;
      await handleSelectKey(selectedKey);
      refreshKeys();
    }, [selectedKey, handleSelectKey, refreshKeys]);

    const toggleKeySelection = (key: string, checked: boolean) => {
      setSelectedKeys((prev) => {
        const next = new Set(prev);
        if (checked) next.add(key);
        else next.delete(key);
        return next;
      });
    };

    const toggleSelectAll = () => {
      if (selectedKeys.size === keys.length) {
        setSelectedKeys(new Set());
      } else {
        setSelectedKeys(new Set(keys.map((k) => k.key)));
      }
    };

    const handleKeyContextMenu = useCallback(
      (e: ReactMouseEvent, key: string) => {
        e.preventDefault();
        e.stopPropagation();
        void showNativeContextMenu(
          buildRedisKeyContextMenuItems({
            labels: {
              copyKey: t('common.copyName'),
              setTtl: t('redis.setTtl'),
              rename: t('redis.renameKey'),
              delete: t('common.delete'),
            },
            handlers: {
              onCopyKey: () => {
                void navigator.clipboard.writeText(key);
              },
              onSetTtl: () => setKeyCtxDialog(openKeyCtxTtl(key)),
              onRename: () => setKeyCtxDialog(openKeyCtxRename(key)),
              onDelete: () => setKeyCtxDialog(openKeyCtxDelete(key)),
            },
          }),
          { x: e.clientX, y: e.clientY },
        );
      },
      [t],
    );

    return (
      <div className="flex min-h-0 flex-1">
        {!hideSidebar && (
          <aside className="flex w-48 shrink-0 flex-col overflow-y-auto border-r border-edge bg-surface-alt">
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
                  data-testid="redis-search-input"
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
                    ? 'bg-accent/10 text-accent font-medium'
                    : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
                )}
                aria-current={selectedDb === db ? 'page' : undefined}
                data-testid={`redis-db-${db}`}
                onClick={() => handleSelectDb(db)}
              >
                <Database className="h-4 w-4 shrink-0" />
                {db}
              </button>
            ))}
          </aside>
        )}

        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
          {selectedDb ? (
            <>
              <div className="flex shrink-0 flex-wrap items-center gap-3 border-b border-edge bg-surface-alt px-3 py-1.5 text-xs text-fg-secondary">
                <span>{selectedDb}</span>
                <span className="text-edge">|</span>
                <span>{t('redis.dbSize').replace('{count}', String(dbSize))}</span>
                <span className="text-edge">|</span>
                <span>
                  {t('redis.loadedCount').replace('{count}', String(keys.length))}
                  {cursor !== 0 && ` (${t('redis.loadMore')}…)`}
                </span>
                <KeyBrowserControls
                  keyType={keyTypeFilter}
                  onKeyTypeChange={setKeyTypeFilter}
                  viewMode={viewMode}
                  onViewModeChange={setViewMode}
                  withMemory={withMemory}
                  onWithMemoryChange={setWithMemory}
                />
                <div className="flex-1" />
                <Button
                  variant="secondary"
                  className="h-7 gap-1 px-2 text-xs"
                  title={t('connWin.refresh')}
                  data-testid="redis-refresh"
                  onClick={handleRefresh}
                >
                  <RefreshCw className="h-3.5 w-3.5" />
                  {t('redis.refresh')}
                </Button>
                <Button
                  variant="secondary"
                  className="h-7 gap-1 px-2 text-xs"
                  data-testid="redis-create-key"
                  onClick={() => setCreateOpen(true)}
                >
                  <Plus className="h-3.5 w-3.5" />
                  {t('redis.createKey')}
                </Button>
                <Button
                  variant="secondary"
                  className="h-7 gap-1 px-2 text-xs"
                  onClick={() => setImportExportOpen(true)}
                >
                  <FolderInput className="h-3.5 w-3.5" />
                  {t('redis.importExportTitle')}
                </Button>
                {allowFlush && (
                  <>
                    <Button
                      variant="secondary"
                      className="h-7 px-2 text-xs text-danger"
                      onClick={() => setFlushDialog('db')}
                    >
                      {t('redis.flushDb')}
                    </Button>
                    <Button
                      variant="secondary"
                      className="h-7 px-2 text-xs text-danger"
                      onClick={() => setFlushDialog('all')}
                    >
                      {t('redis.flushAll')}
                    </Button>
                  </>
                )}
              </div>

              {batchSummary && (
                <div className="shrink-0 border-b border-edge bg-surface-alt px-3 py-1 text-xs text-fg-secondary">
                  {batchSummary}
                  <button
                    type="button"
                    className="ml-2 text-fg-muted hover:text-fg"
                    onClick={() => setBatchSummary(null)}
                  >
                    ×
                  </button>
                </div>
              )}

              <BatchBar
                dbSessionId={dbSessionId}
                dbIndex={dbIndex}
                selectedKeys={[...selectedKeys]}
                searchPattern={searchPattern}
                onClearSelection={() => setSelectedKeys(new Set())}
                onRefresh={refreshKeys}
                onSummary={setBatchSummary}
              />

              <div className="flex min-h-0 flex-1">
                <div className="flex min-w-0 flex-1 flex-col">
                  <KeyTable
                    keys={keys}
                    viewMode={viewMode}
                    expandedFolders={expandedFolders}
                    onToggleFolder={toggleFolder}
                    selectedKey={selectedKey}
                    selectedKeys={selectedKeys}
                    onSelectKey={handleSelectKey}
                    onToggleKey={toggleKeySelection}
                    onToggleSelectAll={toggleSelectAll}
                    onKeyContextMenu={handleKeyContextMenu}
                    loading={keysLoading}
                    hasMore={cursor !== 0}
                    onLoadMore={loadMore}
                  />
                </div>

                {selectedKey && (
                  <div className="flex w-[420px] shrink-0 flex-col border-l border-edge">
                    <div className="flex items-center justify-between border-b border-edge bg-surface-alt px-3 py-2">
                      <span className="truncate text-xs font-medium text-fg">{selectedKey}</span>
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
                        <KeyDetailEditor
                          dbSessionId={dbSessionId}
                          dbIndex={dbIndex}
                          detail={keyDetail}
                          modules={modules}
                          onRefresh={reloadDetail}
                          onRenamed={(newKey) => {
                            setSelectedKey(newKey);
                            refreshKeys();
                          }}
                        />
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

        <ImportExport
          dbSessionId={dbSessionId}
          dbIndex={dbIndex}
          selectedKeys={[...selectedKeys]}
          searchPattern={searchPattern}
          open={importExportOpen}
          onOpenChange={setImportExportOpen}
          onRefresh={refreshKeys}
          onSummary={setBatchSummary}
        />

        <KeyWorkbenchDialogs
          dbSessionId={dbSessionId}
          dbIndex={dbIndex}
          allowFlush={allowFlush}
          createTypes={createTypes}
          selectedKey={selectedKey}
          onRefreshKeys={refreshKeys}
          onSelectKey={handleSelectKey}
          onClearSelectedKey={() => {
            setSelectedKey(null);
            setKeyDetail(null);
          }}
          onUpdateSelectedKey={setSelectedKey}
          onUpdateSelectedKeys={setSelectedKeys}
          onBatchSummary={setBatchSummary}
          createOpen={createOpen}
          onCreateOpenChange={setCreateOpen}
          flushDialog={flushDialog}
          onFlushDialogChange={setFlushDialog}
          keyCtxDialog={keyCtxDialog}
          onKeyCtxDialogChange={setKeyCtxDialog}
        />
      </div>
    );
  },
);
