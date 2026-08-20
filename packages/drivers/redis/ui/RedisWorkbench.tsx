import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
} from 'react';
import { Database, FolderInput, Key, Loader2, Plus, RefreshCw, Search, X } from 'lucide-react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { Button } from '../../../../src/components/ui/Button';
import { Input } from '../../../../src/components/ui/Input';
import { Select } from '../../../../src/components/ui/Select';
import { Dialog } from '../../../../src/components/ui/Dialog';
import { useSchemaStore } from '../../../../src/stores/schemaStore';
import { useSettingsStore } from '../../../../src/stores/settingsStore';
import { useColumnResize } from '../../../../src/hooks/useColumnResize';
import { useI18n } from '../../../../src/hooks/useI18n';
import { cn } from '../../../../src/lib/cn';
import { showNativeContextMenu } from '../../../../src/lib/nativeContextMenu';
import { readBooleanField } from '../../../../src/plugin-sdk/settings';
import { invokeGetKey, invokeScanKeys, redisCommandInvoke } from './redisInvoke';
import type { KeyDetail, KeyEntry } from '../../../../src/types';
import { BatchBar, invokeDeleteKeys } from './BatchBar';
import { hasRedisJson } from './hasRedisJson';
import { ImportExport } from './ImportExport';
import { invokeModulesList } from './JsonEditor';
import { KeyDetailEditor, invokeCreateKey, invokeRename, invokeSetTtl } from './KeyEditors';
import { buildRedisKeyContextMenuItems } from './redisKeyContextMenu';

type KeyCtxDialog =
  | { mode: 'ttl'; key: string }
  | { mode: 'rename'; key: string }
  | { mode: 'delete'; key: string }
  | null;

const ROW_HEIGHT = 32;
const PAGE_SIZE = 200;
const REDIS_DB_COUNT = 16;

export interface RedisWorkbenchProps {
  connectionId: string;
  initialDatabase?: string;
  hideSidebar?: boolean;
  onDbIndexChange?: (dbIndex: number) => void;
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

function formatSize(size: number): string {
  if (!size || size < 0) return '—';
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}

export const RedisWorkbench = forwardRef<RedisWorkbenchHandle, RedisWorkbenchProps>(
  function RedisWorkbench(
    { connectionId, initialDatabase, hideSidebar, onDbIndexChange, onKeysChange },
    ref,
  ) {
    const { t } = useI18n();
    const databasesFromStore = useSchemaStore((s) => s.databases);
    const loading = useSchemaStore((s) => s.loading);
    const loadForConnection = useSchemaStore((s) => s.loadForConnection);
    const pluginSettings = useSettingsStore((s) => s.settings.pluginSettings);
    const allowFlush = readBooleanField(
      (pluginSettings?.redis ?? {}) as Record<string, unknown>,
      'allowFlush',
      false,
    );

    const databases = useMemo(() => mergeDatabases(databasesFromStore), [databasesFromStore]);

    const [selectedDb, setSelectedDb] = useState<string | null>(null);
    const [dbIndex, setDbIndex] = useState(0);
    const [keys, setKeys] = useState<KeyEntry[]>([]);
    const [cursor, setCursor] = useState(0);
    const [dbSize, setDbSize] = useState(0);
    const [keysLoading, setKeysLoading] = useState(false);
    const [searchPattern, setSearchPattern] = useState('*');
    const [selectedKey, setSelectedKey] = useState<string | null>(null);
    const [selectedKeys, setSelectedKeys] = useState<Set<string>>(new Set());
    const [keyDetail, setKeyDetail] = useState<KeyDetail | null>(null);
    const [keyDetailLoading, setKeyDetailLoading] = useState(false);
    const [batchSummary, setBatchSummary] = useState<string | null>(null);
    const [createOpen, setCreateOpen] = useState(false);
    const [createName, setCreateName] = useState('');
    const [createType, setCreateType] = useState('string');
    const [createValue, setCreateValue] = useState('');
    const [createBusy, setCreateBusy] = useState(false);
    const [createError, setCreateError] = useState<string | null>(null);
    const [flushDialog, setFlushDialog] = useState<'db' | 'all' | null>(null);
    const [flushConfirm, setFlushConfirm] = useState('');
    const [flushBusy, setFlushBusy] = useState(false);
    const [flushError, setFlushError] = useState<string | null>(null);
    const [modules, setModules] = useState<string[] | null>(null);
    const [importExportOpen, setImportExportOpen] = useState(false);
    const [keyCtxDialog, setKeyCtxDialog] = useState<KeyCtxDialog>(null);
    const [keyCtxTtlInput, setKeyCtxTtlInput] = useState('');
    const [keyCtxRenameInput, setKeyCtxRenameInput] = useState('');
    const [keyCtxBusy, setKeyCtxBusy] = useState(false);
    const [keyCtxError, setKeyCtxError] = useState<string | null>(null);

    useEffect(() => {
      void loadForConnection(connectionId, { skipLoadTables: true });
    }, [connectionId, loadForConnection]);

    useEffect(() => {
      let cancelled = false;
      setModules(null);
      void invokeModulesList(connectionId)
        .then((list) => {
          if (!cancelled) setModules(list);
        })
        .catch(() => {
          if (!cancelled) setModules([]);
        });
      return () => {
        cancelled = true;
      };
    }, [connectionId]);

    const createTypes = useMemo(() => {
      const base = ['string', 'hash', 'list', 'set', 'zset'];
      if (modules && hasRedisJson(modules)) {
        return [...base, 'ReJSON'];
      }
      return base;
    }, [modules]);

    const loadKeys = useCallback(
      async (idx: number, pattern: string, cur: number, reset: boolean) => {
        setKeysLoading(true);
        try {
          const result = await invokeScanKeys(connectionId, idx, pattern || '*', cur, PAGE_SIZE);
          if (reset) {
            setKeys(result.keys);
          } else {
            setKeys((prev) => [...prev, ...result.keys]);
          }
          setCursor(result.cursor);
          setDbSize(result.dbSize);
        } catch (e) {
          console.error('scan_keys failed:', e);
        } finally {
          setKeysLoading(false);
        }
      },
      [connectionId],
    );

    const handleSelectDb = useCallback(
      (db: string) => {
        const idx = parseInt(db.replace('db', ''), 10) || 0;
        setSelectedDb(db);
        setDbIndex(idx);
        onDbIndexChange?.(idx);
        setKeys([]);
        setCursor(0);
        setDbSize(0);
        setSelectedKey(null);
        setSelectedKeys(new Set());
        setKeyDetail(null);
        setSearchPattern('*');
        void loadKeys(idx, '*', 0, true);
      },
      [loadKeys, onDbIndexChange],
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
        setKeys([]);
        setCursor(0);
        setSelectedKey(null);
        setSelectedKeys(new Set());
        setKeyDetail(null);
        void loadKeys(dbIndex, searchPattern, 0, true);
      }
    }, [selectedDb, dbIndex, searchPattern, loadKeys]);

    const handleRefresh = useCallback(() => {
      void loadForConnection(connectionId, { skipLoadTables: true });
      refreshKeys();
    }, [connectionId, loadForConnection, refreshKeys]);

    useImperativeHandle(ref, () => ({ refreshKeys, selectDatabase: handleSelectDb }), [
      refreshKeys,
      handleSelectDb,
    ]);

    const handleLoadMore = useCallback(() => {
      if (cursor !== 0) {
        void loadKeys(dbIndex, searchPattern, cursor, false);
      }
    }, [dbIndex, searchPattern, cursor, loadKeys]);

    const handleSearch = useCallback(() => {
      setKeys([]);
      setCursor(0);
      setSelectedKey(null);
      setSelectedKeys(new Set());
      setKeyDetail(null);
      void loadKeys(dbIndex, searchPattern, 0, true);
    }, [dbIndex, searchPattern, loadKeys]);

    const handleSelectKey = useCallback(
      async (key: string) => {
        setSelectedKey(key);
        setKeyDetailLoading(true);
        try {
          const detail = await invokeGetKey(connectionId, dbIndex, key);
          setKeyDetail(detail);
        } catch (e) {
          console.error('get_key failed:', e);
          setKeyDetail(null);
        } finally {
          setKeyDetailLoading(false);
        }
      },
      [connectionId, dbIndex],
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

    const handleCreateKey = async () => {
      const name = createName.trim();
      if (!name) return;
      setCreateBusy(true);
      setCreateError(null);
      try {
        await invokeCreateKey(connectionId, dbIndex, name, createType, createValue);
        setCreateOpen(false);
        setCreateName('');
        setCreateValue('');
        setCreateError(null);
        refreshKeys();
        await handleSelectKey(name);
      } catch (e) {
        setCreateError(e instanceof Error ? e.message : String(e));
      } finally {
        setCreateBusy(false);
      }
    };

    const handleFlush = async () => {
      setFlushBusy(true);
      setFlushError(null);
      try {
        if (flushDialog === 'db') {
          await redisCommandInvoke('redis', 'flush_db', {
            connectionId: connectionId,
            dbIndex: dbIndex,
            allowFlush: allowFlush,
          });
        } else if (flushDialog === 'all') {
          await redisCommandInvoke('redis', 'flush_all', {
            connectionId: connectionId,
            allowFlush: allowFlush,
          });
        }
        setFlushDialog(null);
        setFlushConfirm('');
        setFlushError(null);
        refreshKeys();
      } catch (e) {
        setFlushError(e instanceof Error ? e.message : String(e));
      } finally {
        setFlushBusy(false);
      }
    };

    const flushConfirmOk =
      flushDialog === 'all' ? flushConfirm === 'ALL' : flushConfirm === String(dbIndex);

    const closeKeyCtxDialog = useCallback(() => {
      setKeyCtxDialog(null);
      setKeyCtxTtlInput('');
      setKeyCtxRenameInput('');
      setKeyCtxError(null);
      setKeyCtxBusy(false);
    }, []);

    const handleKeyContextMenu = useCallback(
      (e: ReactMouseEvent, key: string) => {
        e.preventDefault();
        e.stopPropagation();
        void showNativeContextMenu(
          buildRedisKeyContextMenuItems({
            labels: {
              copyKey: t('schemaTree.copyName'),
              setTtl: t('redis.setTtl'),
              rename: t('redis.renameKey'),
              delete: t('common.delete'),
            },
            handlers: {
              onCopyKey: () => {
                void navigator.clipboard.writeText(key);
              },
              onSetTtl: () => {
                setKeyCtxError(null);
                setKeyCtxTtlInput('');
                setKeyCtxDialog({ mode: 'ttl', key });
              },
              onRename: () => {
                setKeyCtxError(null);
                setKeyCtxRenameInput(key);
                setKeyCtxDialog({ mode: 'rename', key });
              },
              onDelete: () => {
                setKeyCtxError(null);
                setKeyCtxDialog({ mode: 'delete', key });
              },
            },
          }),
          { x: e.clientX, y: e.clientY },
        );
      },
      [t],
    );

    const handleKeyCtxSetTtl = async () => {
      if (keyCtxDialog?.mode !== 'ttl') return;
      setKeyCtxBusy(true);
      setKeyCtxError(null);
      try {
        const secs = parseInt(keyCtxTtlInput, 10);
        if (Number.isNaN(secs) || secs < 0) {
          throw new Error(t('redis.ttlSeconds'));
        }
        await invokeSetTtl(connectionId, dbIndex, keyCtxDialog.key, secs);
        closeKeyCtxDialog();
        refreshKeys();
        if (selectedKey === keyCtxDialog.key) {
          await handleSelectKey(keyCtxDialog.key);
        }
      } catch (err) {
        setKeyCtxError(err instanceof Error ? err.message : String(err));
      } finally {
        setKeyCtxBusy(false);
      }
    };

    const handleKeyCtxPersist = async () => {
      if (keyCtxDialog?.mode !== 'ttl') return;
      setKeyCtxBusy(true);
      setKeyCtxError(null);
      try {
        await invokeSetTtl(connectionId, dbIndex, keyCtxDialog.key, -1);
        closeKeyCtxDialog();
        refreshKeys();
        if (selectedKey === keyCtxDialog.key) {
          await handleSelectKey(keyCtxDialog.key);
        }
      } catch (err) {
        setKeyCtxError(err instanceof Error ? err.message : String(err));
      } finally {
        setKeyCtxBusy(false);
      }
    };

    const handleKeyCtxRename = async () => {
      if (keyCtxDialog?.mode !== 'rename') return;
      const next = keyCtxRenameInput.trim();
      if (!next || next === keyCtxDialog.key) return;
      setKeyCtxBusy(true);
      setKeyCtxError(null);
      try {
        await invokeRename(connectionId, dbIndex, keyCtxDialog.key, next);
        closeKeyCtxDialog();
        if (selectedKey === keyCtxDialog.key) {
          setSelectedKey(next);
        }
        setSelectedKeys((prev) => {
          if (!prev.has(keyCtxDialog.key)) return prev;
          const updated = new Set(prev);
          updated.delete(keyCtxDialog.key);
          updated.add(next);
          return updated;
        });
        refreshKeys();
        await handleSelectKey(next);
      } catch (err) {
        setKeyCtxError(err instanceof Error ? err.message : String(err));
      } finally {
        setKeyCtxBusy(false);
      }
    };

    const handleKeyCtxDelete = async () => {
      if (keyCtxDialog?.mode !== 'delete') return;
      setKeyCtxBusy(true);
      setKeyCtxError(null);
      try {
        const deleted = await invokeDeleteKeys(connectionId, dbIndex, [keyCtxDialog.key]);
        setBatchSummary(t('redis.deleted').replace('{count}', String(deleted)));
        if (selectedKey === keyCtxDialog.key) {
          setSelectedKey(null);
          setKeyDetail(null);
        }
        setSelectedKeys((prev) => {
          if (!prev.has(keyCtxDialog.key)) return prev;
          const updated = new Set(prev);
          updated.delete(keyCtxDialog.key);
          return updated;
        });
        closeKeyCtxDialog();
        refreshKeys();
      } catch (err) {
        setKeyCtxError(err instanceof Error ? err.message : String(err));
      } finally {
        setKeyCtxBusy(false);
      }
    };

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
                <div className="flex-1" />
                <Button
                  variant="secondary"
                  className="h-7 gap-1 px-2 text-xs"
                  title={t('connWin.refresh')}
                  onClick={handleRefresh}
                >
                  <RefreshCw className="h-3.5 w-3.5" />
                  {t('redis.refresh')}
                </Button>
                <Button
                  variant="secondary"
                  className="h-7 gap-1 px-2 text-xs"
                  onClick={() => {
                    setCreateError(null);
                    setCreateOpen(true);
                  }}
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
                      onClick={() => {
                        setFlushConfirm('');
                        setFlushError(null);
                        setFlushDialog('db');
                      }}
                    >
                      {t('redis.flushDb')}
                    </Button>
                    <Button
                      variant="secondary"
                      className="h-7 px-2 text-xs text-danger"
                      onClick={() => {
                        setFlushConfirm('');
                        setFlushError(null);
                        setFlushDialog('all');
                      }}
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
                connectionId={connectionId}
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
                    selectedKey={selectedKey}
                    selectedKeys={selectedKeys}
                    onSelectKey={handleSelectKey}
                    onToggleKey={toggleKeySelection}
                    onToggleSelectAll={toggleSelectAll}
                    onKeyContextMenu={handleKeyContextMenu}
                    loading={keysLoading}
                    hasMore={cursor !== 0}
                    onLoadMore={handleLoadMore}
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
                          connectionId={connectionId}
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

        <Dialog
          open={createOpen}
          title={t('redis.createKey')}
          onClose={() => {
            setCreateOpen(false);
            setCreateError(null);
          }}
          footer={
            <>
              <Button
                variant="secondary"
                className="h-8 px-3 text-xs"
                onClick={() => {
                  setCreateOpen(false);
                  setCreateError(null);
                }}
              >
                {t('common.cancel')}
              </Button>
              <Button
                variant="primary"
                className="h-8 px-3 text-xs"
                disabled={createBusy || !createName.trim()}
                onClick={() => void handleCreateKey()}
              >
                {t('redis.create')}
              </Button>
            </>
          }
        >
          <div className="space-y-3">
            <Input
              value={createName}
              onChange={(e) => setCreateName(e.target.value)}
              placeholder={t('redis.keyName')}
              className="h-8 font-mono text-xs"
            />
            <Select
              value={createType}
              onChange={setCreateType}
              className="h-8 w-full text-xs"
              options={createTypes.map((type) => ({ value: type, label: type }))}
            />
            <Input
              value={createValue}
              onChange={(e) => setCreateValue(e.target.value)}
              placeholder={t('redis.value')}
              className="h-8 font-mono text-xs"
            />
            {createError && <p className="text-danger">{createError}</p>}
          </div>
        </Dialog>

        <ImportExport
          connectionId={connectionId}
          dbIndex={dbIndex}
          selectedKeys={[...selectedKeys]}
          searchPattern={searchPattern}
          open={importExportOpen}
          onOpenChange={setImportExportOpen}
          onRefresh={refreshKeys}
          onSummary={setBatchSummary}
        />

        <Dialog
          open={flushDialog !== null}
          title={flushDialog === 'all' ? t('redis.confirmFlushAll') : t('redis.confirmFlushDb')}
          description={
            flushDialog === 'all'
              ? t('redis.typeConfirmAll')
              : t('redis.typeConfirmDb').replace('{index}', String(dbIndex))
          }
          onClose={() => {
            setFlushDialog(null);
            setFlushConfirm('');
            setFlushError(null);
          }}
          footer={
            <>
              <Button
                variant="secondary"
                className="h-8 px-3 text-xs"
                onClick={() => {
                  setFlushDialog(null);
                  setFlushConfirm('');
                  setFlushError(null);
                }}
              >
                {t('common.cancel')}
              </Button>
              <Button
                variant="primary"
                className="h-8 px-3 text-xs text-danger"
                disabled={!flushConfirmOk || flushBusy}
                onClick={() => void handleFlush()}
              >
                {t('redis.flushConfirm')}
              </Button>
            </>
          }
        >
          <div className="space-y-3">
            <Input
              value={flushConfirm}
              onChange={(e) => setFlushConfirm(e.target.value)}
              placeholder={t('redis.typeConfirmPlaceholder')}
              className="h-8 font-mono text-xs"
            />
            {flushError && <p className="text-danger">{flushError}</p>}
          </div>
        </Dialog>

        <Dialog
          open={keyCtxDialog?.mode === 'ttl'}
          title={t('redis.setTtl')}
          description={keyCtxDialog?.mode === 'ttl' ? keyCtxDialog.key : undefined}
          onClose={closeKeyCtxDialog}
          footer={
            <>
              <Button variant="secondary" className="h-8 px-3 text-xs" onClick={closeKeyCtxDialog}>
                {t('common.cancel')}
              </Button>
              <Button
                variant="secondary"
                className="h-8 px-3 text-xs"
                disabled={keyCtxBusy}
                onClick={() => void handleKeyCtxPersist()}
              >
                {t('redis.persist')}
              </Button>
              <Button
                variant="primary"
                className="h-8 px-3 text-xs"
                disabled={keyCtxBusy || !keyCtxTtlInput.trim()}
                onClick={() => void handleKeyCtxSetTtl()}
              >
                {t('redis.setTtl')}
              </Button>
            </>
          }
        >
          <div className="space-y-3">
            <Input
              value={keyCtxTtlInput}
              onChange={(e) => setKeyCtxTtlInput(e.target.value)}
              placeholder={t('redis.ttlSeconds')}
              className="h-8 font-mono text-xs"
            />
            {keyCtxError && <p className="text-danger">{keyCtxError}</p>}
          </div>
        </Dialog>

        <Dialog
          open={keyCtxDialog?.mode === 'rename'}
          title={t('redis.renameKey')}
          description={keyCtxDialog?.mode === 'rename' ? keyCtxDialog.key : undefined}
          onClose={closeKeyCtxDialog}
          footer={
            <>
              <Button variant="secondary" className="h-8 px-3 text-xs" onClick={closeKeyCtxDialog}>
                {t('common.cancel')}
              </Button>
              <Button
                variant="primary"
                className="h-8 px-3 text-xs"
                disabled={
                  keyCtxBusy ||
                  !keyCtxRenameInput.trim() ||
                  (keyCtxDialog?.mode === 'rename' && keyCtxRenameInput.trim() === keyCtxDialog.key)
                }
                onClick={() => void handleKeyCtxRename()}
              >
                {t('redis.renameKey')}
              </Button>
            </>
          }
        >
          <div className="space-y-3">
            <Input
              value={keyCtxRenameInput}
              onChange={(e) => setKeyCtxRenameInput(e.target.value)}
              placeholder={t('redis.keyName')}
              className="h-8 font-mono text-xs"
            />
            {keyCtxError && <p className="text-danger">{keyCtxError}</p>}
          </div>
        </Dialog>

        <Dialog
          open={keyCtxDialog?.mode === 'delete'}
          title={t('redis.confirmDeleteKeys')}
          description={keyCtxDialog?.mode === 'delete' ? keyCtxDialog.key : undefined}
          onClose={closeKeyCtxDialog}
          footer={
            <>
              <Button variant="secondary" className="h-8 px-3 text-xs" onClick={closeKeyCtxDialog}>
                {t('common.cancel')}
              </Button>
              <Button
                variant="primary"
                className="h-8 px-3 text-xs text-danger"
                disabled={keyCtxBusy}
                onClick={() => void handleKeyCtxDelete()}
              >
                {t('common.delete')}
              </Button>
            </>
          }
        >
          {keyCtxError && <p className="text-danger">{keyCtxError}</p>}
        </Dialog>
      </div>
    );
  },
);

function KeyTable({
  keys,
  selectedKey,
  selectedKeys,
  onSelectKey,
  onToggleKey,
  onToggleSelectAll,
  onKeyContextMenu,
  loading,
  hasMore,
  onLoadMore,
}: {
  keys: KeyEntry[];
  selectedKey: string | null;
  selectedKeys: Set<string>;
  onSelectKey: (key: string) => void;
  onToggleKey: (key: string, checked: boolean) => void;
  onToggleSelectAll: () => void;
  onKeyContextMenu: (e: ReactMouseEvent, key: string) => void;
  loading: boolean;
  hasMore: boolean;
  onLoadMore: () => void;
}) {
  const { t } = useI18n();
  const scrollRef = useRef<HTMLDivElement>(null);
  const { columnWidths, onResizeStart } = useColumnResize({ count: 6 });

  const virtualizer = useVirtualizer({
    count: keys.length + (hasMore ? 1 : 0),
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 20,
  });

  const TYPE_COLORS: Record<string, string> = {
    string: 'text-success',
    hash: 'text-accent',
    list: 'text-warning',
    set: 'text-fg-secondary',
    zset: 'text-danger',
    stream: 'text-fg-muted',
  };

  const columns = [
    '',
    t('redis.key'),
    t('redis.type'),
    t('redis.ttl'),
    t('redis.size'),
    t('redis.preview'),
  ];

  const allSelected = keys.length > 0 && selectedKeys.size === keys.length;

  return (
    <div ref={scrollRef} className="min-h-0 flex-1 overflow-auto">
      <div className="min-w-max text-[13px]">
        <div className="sticky top-0 z-10 flex bg-surface-alt">
          {columns.map((col, ci) => (
            <div
              key={col || 'check'}
              className="relative shrink-0 border-b border-r border-edge px-3 py-2 text-left text-xs font-medium text-fg-secondary"
              style={{
                width: columnWidths[ci],
                ...(ci === 0 ? { width: 36, minWidth: 36 } : {}),
                ...(ci === 1 || ci === 5 ? { flex: '1 1 0', minWidth: 100 } : {}),
              }}
            >
              {ci === 0 ? (
                <input
                  type="checkbox"
                  checked={allSelected}
                  onChange={onToggleSelectAll}
                  aria-label={t('common.selectAll')}
                />
              ) : (
                col
              )}
              {ci > 0 && (
                <div
                  className="absolute right-0 top-0 z-20 h-full w-[5px] cursor-col-resize hover:bg-accent/40 active:bg-accent/60"
                  onPointerDown={(e) => {
                    e.preventDefault();
                    onResizeStart(ci, e.clientX);
                  }}
                />
              )}
            </div>
          ))}
        </div>

        <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
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
                    className="text-xs text-accent hover:underline"
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
            const isChecked = selectedKeys.has(entry.key);

            return (
              <div
                key={entry.key}
                className={cn(
                  'absolute left-0 flex w-full cursor-pointer border-b border-edge',
                  isSelected
                    ? 'bg-accent/10'
                    : vRow.index % 2 === 0
                      ? 'bg-surface'
                      : 'bg-surface-raised/50',
                  'hover:bg-accent/5',
                )}
                style={{ top: vRow.start, height: ROW_HEIGHT }}
                onClick={() => onSelectKey(entry.key)}
                onContextMenu={(e) => onKeyContextMenu(e, entry.key)}
              >
                <div
                  className="flex shrink-0 items-center justify-center border-r border-edge px-2"
                  style={{ width: 36, minWidth: 36 }}
                  onClick={(e) => e.stopPropagation()}
                >
                  <input
                    type="checkbox"
                    checked={isChecked}
                    onChange={(e) => onToggleKey(entry.key, e.target.checked)}
                  />
                </div>
                <div
                  className="flex shrink-0 items-center overflow-hidden border-r border-edge px-3 font-mono"
                  style={{ flex: '1 1 0', minWidth: 100, width: columnWidths[1] }}
                >
                  <Key className="mr-1.5 h-3 w-3 shrink-0 text-fg-muted" />
                  <span className="truncate text-fg-secondary">{entry.key}</span>
                </div>
                <div
                  className="flex shrink-0 items-center overflow-hidden border-r border-edge px-3"
                  style={{ width: columnWidths[2] }}
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
                  style={{ width: columnWidths[3] }}
                >
                  {entry.ttl < 0 ? '∞' : `${entry.ttl}s`}
                </div>
                <div
                  className="flex shrink-0 items-center overflow-hidden border-r border-edge px-3 text-xs text-fg-muted"
                  style={{ width: columnWidths[4] }}
                >
                  {formatSize(entry.size)}
                </div>
                <div
                  className="flex shrink-0 items-center overflow-hidden border-r border-edge px-3 font-mono text-fg-secondary"
                  style={{ flex: '1 1 0', minWidth: 100, width: columnWidths[5] }}
                >
                  <span className="truncate">{entry.preview}</span>
                </div>
              </div>
            );
          })}
        </div>

        {keys.length === 0 && !loading && (
          <div className="px-4 py-8 text-center text-xs text-fg-muted">{t('redis.noKeys')}</div>
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
