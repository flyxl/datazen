import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  Database,
  FileJson,
  Loader2,
  Plus,
  RefreshCw,
  Search,
  Trash2,
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
import {
  buildMongoDeleteCommand,
  buildMongoFindCommand,
  buildMongoInsertCommand,
  buildMongoUpdateCommand,
  cellToDisplay,
  getDocumentId,
  parseMongoDocumentJson,
  parseMongoFilterJson,
  rowToDocument,
} from '../../lib/mongodbFind';
import type { ConnectionViewProps } from '../../lib/connectionViews/types';
import type { StatementResult, TableInfo } from '../../types';

const ROW_HEIGHT = 32;
const DEFAULT_LIMIT = 50;

type ActiveTab = 'documents' | 'queries';

export function DocumentConnectionView({
  connectionId,
  connectionName,
  initialDatabase,
}: ConnectionViewProps) {
  const { t } = useI18n();
  const databases = useSchemaStore((s) => s.databases);
  const loading = useSchemaStore((s) => s.loading);
  const loadForConnection = useSchemaStore((s) => s.loadForConnection);

  const [activeTab, setActiveTab] = useState<ActiveTab>('documents');
  const [selectedDb, setSelectedDb] = useState<string | null>(null);
  const [collections, setCollections] = useState<TableInfo[]>([]);
  const [collectionsLoading, setCollectionsLoading] = useState(false);
  const [selectedCollection, setSelectedCollection] = useState<string | null>(null);
  const [filterText, setFilterText] = useState('{}');
  const [filterError, setFilterError] = useState<string | null>(null);
  const [docsLoading, setDocsLoading] = useState(false);
  const [docsError, setDocsError] = useState<string | null>(null);
  const [result, setResult] = useState<StatementResult | null>(null);
  const [selectedRowIdx, setSelectedRowIdx] = useState<number | null>(null);
  const [collectionFilter, setCollectionFilter] = useState('');
  const [editText, setEditText] = useState('');
  const [editError, setEditError] = useState<string | null>(null);
  const [mutating, setMutating] = useState(false);
  const [insertMode, setInsertMode] = useState(false);

  useEffect(() => {
    void loadForConnection(connectionId, { skipLoadTables: true });
  }, [connectionId, loadForConnection]);

  useEffect(() => {
    if (databases.length === 0 || selectedDb) return;
    const initial =
      (initialDatabase && databases.find((d) => d === initialDatabase)) || databases[0];
    if (initial) void handleSelectDb(initial);
  }, [databases, initialDatabase, selectedDb]);

  const loadCollections = useCallback(
    async (db: string) => {
      setCollectionsLoading(true);
      try {
        await databaseCommands.useDatabase(connectionId, db);
        const tables = await databaseCommands.getTables(connectionId, db);
        setCollections(tables);
      } catch (e) {
        console.error('load collections failed:', e);
        setCollections([]);
      } finally {
        setCollectionsLoading(false);
      }
    },
    [connectionId],
  );

  const handleSelectDb = useCallback(
    async (db: string) => {
      setSelectedDb(db);
      setSelectedCollection(null);
      setResult(null);
      setSelectedRowIdx(null);
      setInsertMode(false);
      setEditText('');
      setEditError(null);
      setDocsError(null);
      await loadCollections(db);
    },
    [loadCollections],
  );

  const loadDocuments = useCallback(
    async (collection: string, filter: string, opts?: { preserveId?: unknown }) => {
      if (!selectedDb) return;
      setDocsLoading(true);
      setDocsError(null);
      setFilterError(null);
      try {
        parseMongoFilterJson(filter);
        const sql = buildMongoFindCommand({
          collection,
          filterText: filter,
          limit: DEFAULT_LIMIT,
          database: selectedDb,
        });
        const multi = await databaseCommands.executeSQL(connectionId, sql);
        const nextResult = multi.results[0] ?? null;
        setResult(nextResult);

        if (opts?.preserveId !== undefined && nextResult) {
          const cols = nextResult.columns.map((c) => c.name);
          const matchIdx = nextResult.rows.findIndex((row) => {
            const doc = rowToDocument(cols, row);
            return JSON.stringify(getDocumentId(doc)) === JSON.stringify(opts.preserveId);
          });
          if (matchIdx >= 0) {
            setSelectedRowIdx(matchIdx);
            setInsertMode(false);
            return;
          }
        }

        setSelectedRowIdx(null);
        setInsertMode(false);
        setEditText('');
        setEditError(null);
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        if (msg.includes('Filter must be') || msg.includes('JSON')) {
          setFilterError(msg);
        } else {
          setDocsError(msg);
        }
        setResult(null);
      } finally {
        setDocsLoading(false);
      }
    },
    [connectionId, selectedDb],
  );

  const handleSelectCollection = useCallback(
    (name: string) => {
      setSelectedCollection(name);
      setInsertMode(false);
      setEditText('');
      setEditError(null);
      void loadDocuments(name, filterText);
    },
    [filterText, loadDocuments],
  );

  const handleApplyFilter = useCallback(() => {
    if (selectedCollection) void loadDocuments(selectedCollection, filterText);
  }, [selectedCollection, filterText, loadDocuments]);

  const handleRefresh = useCallback(() => {
    void loadForConnection(connectionId, { skipLoadTables: true });
    if (selectedDb) void loadCollections(selectedDb);
    if (selectedCollection) void loadDocuments(selectedCollection, filterText);
  }, [
    connectionId,
    selectedDb,
    selectedCollection,
    filterText,
    loadForConnection,
    loadCollections,
    loadDocuments,
  ]);

  const filteredCollections = useMemo(() => {
    const q = collectionFilter.trim().toLowerCase();
    if (!q) return collections;
    return collections.filter((c) => c.name.toLowerCase().includes(q));
  }, [collections, collectionFilter]);

  const selectedDoc = useMemo(() => {
    if (selectedRowIdx == null || !result) return null;
    const row = result.rows[selectedRowIdx];
    if (!row) return null;
    return rowToDocument(
      result.columns.map((c) => c.name),
      row,
    );
  }, [result, selectedRowIdx]);

  useEffect(() => {
    if (selectedDoc) {
      setEditText(JSON.stringify(selectedDoc, null, 2));
      setEditError(null);
      setInsertMode(false);
    } else if (!insertMode) {
      setEditText('');
      setEditError(null);
    }
  }, [selectedDoc, insertMode]);

  const handleSelectRow = useCallback((idx: number) => {
    setSelectedRowIdx(idx);
    setInsertMode(false);
  }, []);

  const handleCloseEditor = useCallback(() => {
    setSelectedRowIdx(null);
    setInsertMode(false);
    setEditText('');
    setEditError(null);
  }, []);

  const handleInsertNew = useCallback(() => {
    setSelectedRowIdx(null);
    setInsertMode(true);
    setEditText('{}');
    setEditError(null);
  }, []);

  const handleSaveDocument = useCallback(async () => {
    if (!selectedCollection || !selectedDb) return;
    setEditError(null);
    let doc: Record<string, unknown>;
    try {
      doc = parseMongoDocumentJson(editText);
    } catch (e) {
      setEditError(e instanceof Error ? e.message : String(e));
      return;
    }
    const id = getDocumentId(doc);
    if (id === undefined) {
      setEditError(t('mongo.noIdHint'));
      return;
    }
    const setFields = { ...doc };
    delete setFields._id;
    setMutating(true);
    try {
      const sql = buildMongoUpdateCommand({
        collection: selectedCollection,
        database: selectedDb,
        filter: { _id: id },
        setFields,
      });
      await databaseCommands.executeSQL(connectionId, sql);
      if (selectedCollection) {
        await loadDocuments(selectedCollection, filterText, { preserveId: id });
      }
    } catch (e) {
      setEditError(e instanceof Error ? e.message : String(e));
    } finally {
      setMutating(false);
    }
  }, [
    connectionId,
    editText,
    filterText,
    loadDocuments,
    selectedCollection,
    selectedDb,
    t,
  ]);

  const handleInsertDocument = useCallback(async () => {
    if (!selectedCollection || !selectedDb) return;
    setEditError(null);
    let doc: Record<string, unknown>;
    try {
      doc = parseMongoDocumentJson(editText);
    } catch (e) {
      setEditError(e instanceof Error ? e.message : String(e));
      return;
    }
    setMutating(true);
    try {
      const sql = buildMongoInsertCommand({
        collection: selectedCollection,
        database: selectedDb,
        documents: [doc],
      });
      await databaseCommands.executeSQL(connectionId, sql);
      setInsertMode(false);
      setEditText('');
      if (selectedCollection) await loadDocuments(selectedCollection, filterText);
    } catch (e) {
      setEditError(e instanceof Error ? e.message : String(e));
    } finally {
      setMutating(false);
    }
  }, [
    connectionId,
    editText,
    filterText,
    loadDocuments,
    selectedCollection,
    selectedDb,
  ]);

  const handleDeleteDocument = useCallback(async () => {
    if (!selectedCollection || !selectedDb) return;
    setEditError(null);
    let doc: Record<string, unknown>;
    try {
      doc = selectedDoc ?? parseMongoDocumentJson(editText);
    } catch (e) {
      setEditError(e instanceof Error ? e.message : String(e));
      return;
    }
    const id = getDocumentId(doc);
    if (id === undefined) {
      setEditError(t('mongo.noIdHint'));
      return;
    }
    setMutating(true);
    try {
      const sql = buildMongoDeleteCommand({
        collection: selectedCollection,
        database: selectedDb,
        filter: { _id: id },
      });
      await databaseCommands.executeSQL(connectionId, sql);
      handleCloseEditor();
      if (selectedCollection) await loadDocuments(selectedCollection, filterText);
    } catch (e) {
      setEditError(e instanceof Error ? e.message : String(e));
    } finally {
      setMutating(false);
    }
  }, [
    connectionId,
    editText,
    filterText,
    handleCloseEditor,
    loadDocuments,
    selectedCollection,
    selectedDb,
    selectedDoc,
    t,
  ]);

  const parsedEditDoc = useMemo(() => {
    try {
      return parseMongoDocumentJson(editText);
    } catch {
      return null;
    }
  }, [editText]);

  const hasDocumentId = parsedEditDoc ? getDocumentId(parsedEditDoc) !== undefined : false;
  const showEditor = selectedDoc !== null || insertMode;

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
        {(['documents', 'queries'] as const).map((tab) => (
          <button
            key={tab}
            type="button"
            className={cn(
              'relative px-4 py-3 text-sm transition-colors',
              activeTab === tab ? 'text-fg font-medium' : 'text-fg-secondary hover:text-fg',
            )}
            onClick={() => setActiveTab(tab)}
          >
            {tab === 'documents' ? t('mongo.documents') : t('mongo.queries')}
            <span
              className={cn(
                'absolute inset-x-0 bottom-0 h-0.5 bg-blue-500 transition-opacity duration-300',
                activeTab === tab ? 'opacity-100' : 'opacity-0',
              )}
            />
          </button>
        ))}
        <div className="flex-1" />
        <span className="text-xs text-fg-muted">{connectionName}</span>
      </div>

      {activeTab === 'documents' ? (
        <div className="flex min-h-0 flex-1">
          <aside className="flex w-56 shrink-0 flex-col overflow-hidden border-r border-edge bg-surface-alt">
            <div className="border-b border-edge p-2">
              <div className="mb-1 text-[11px] font-medium uppercase tracking-wide text-fg-muted">
                {t('mongo.databases')}
              </div>
              {loading && (
                <div className="flex items-center gap-2 py-1 text-xs text-fg-muted">
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  {t('common.loading')}
                </div>
              )}
              <div className="max-h-40 overflow-y-auto">
                {databases.map((db) => (
                  <button
                    key={db}
                    type="button"
                    className={cn(
                      'flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm',
                      selectedDb === db
                        ? 'bg-blue-500/10 font-medium text-blue-400'
                        : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
                    )}
                    onClick={() => void handleSelectDb(db)}
                  >
                    <Database className="h-3.5 w-3.5 shrink-0" />
                    <span className="truncate">{db}</span>
                  </button>
                ))}
              </div>
            </div>

            <div className="border-b border-edge p-2">
              <div className="relative">
                <Search className="pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-fg-muted" />
                <Input
                  value={collectionFilter}
                  onChange={(e) => setCollectionFilter(e.target.value)}
                  placeholder={t('mongo.searchCollections')}
                  className="h-7 pl-7 text-xs"
                />
              </div>
            </div>

            <div className="min-h-0 flex-1 overflow-y-auto p-1">
              <div className="px-2 py-1 text-[11px] font-medium uppercase tracking-wide text-fg-muted">
                {t('mongo.collections')}
              </div>
              {collectionsLoading && (
                <div className="flex items-center gap-2 px-2 py-1 text-xs text-fg-muted">
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  {t('common.loading')}
                </div>
              )}
              {!collectionsLoading && filteredCollections.length === 0 && (
                <div className="px-2 py-2 text-xs text-fg-muted">{t('mongo.noCollections')}</div>
              )}
              {filteredCollections.map((c) => (
                <button
                  key={c.name}
                  type="button"
                  className={cn(
                    'flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm',
                    selectedCollection === c.name
                      ? 'bg-blue-500/10 font-medium text-blue-400'
                      : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
                  )}
                  onClick={() => handleSelectCollection(c.name)}
                >
                  <FileJson className="h-3.5 w-3.5 shrink-0" />
                  <span className="truncate">{c.name}</span>
                </button>
              ))}
            </div>
          </aside>

          <div className="flex min-h-0 min-w-0 flex-1 flex-col">
            {selectedCollection && selectedDb ? (
              <>
                <div className="flex shrink-0 items-center gap-2 border-b border-edge bg-surface-alt px-3 py-1.5">
                  <span className="text-xs text-fg-secondary">
                    {selectedDb}.{selectedCollection}
                  </span>
                  <span className="text-edge">|</span>
                  <span className="text-xs text-fg-muted">{t('mongo.filter')}</span>
                  <Input
                    value={filterText}
                    onChange={(e) => setFilterText(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') handleApplyFilter();
                    }}
                    className="h-7 max-w-md flex-1 font-mono text-xs"
                    placeholder='{"status":"paid"}'
                  />
                  <Button
                    variant="primary"
                    className="h-7 px-2 text-xs"
                    onClick={handleApplyFilter}
                    disabled={docsLoading}
                  >
                    {t('mongo.applyFilter')}
                  </Button>
                  <Button
                    variant="secondary"
                    className="h-7 gap-1 px-2 text-xs"
                    onClick={handleInsertNew}
                    disabled={docsLoading || mutating}
                  >
                    <Plus className="h-3.5 w-3.5" />
                    {t('mongo.insert')}
                  </Button>
                  {result && (
                    <span className="text-xs text-fg-muted">
                      {t('mongo.docCount', { count: result.rows.length })}
                    </span>
                  )}
                </div>
                {filterError && (
                  <div className="border-b border-red-500/20 bg-red-500/10 px-3 py-1.5 text-xs text-red-400">
                    {filterError}
                  </div>
                )}
                {docsError && (
                  <div className="border-b border-red-500/20 bg-red-500/10 px-3 py-1.5 text-xs text-red-400">
                    {docsError}
                  </div>
                )}
                <div className="flex min-h-0 flex-1">
                  <div className="flex min-w-0 flex-1 flex-col">
                    {docsLoading ? (
                      <div className="flex flex-1 items-center justify-center gap-2 text-fg-muted">
                        <Loader2 className="h-5 w-5 animate-spin" />
                        {t('common.loading')}
                      </div>
                    ) : result ? (
                      <DocumentResultTable
                        result={result}
                        selectedRowIdx={selectedRowIdx}
                        onSelectRow={handleSelectRow}
                      />
                    ) : (
                      <div className="flex flex-1 items-center justify-center text-sm text-fg-muted">
                        {t('mongo.noDocuments')}
                      </div>
                    )}
                  </div>
                  {showEditor && (
                    <DocumentDetailEditor
                      editText={editText}
                      editError={editError}
                      mutating={mutating}
                      insertMode={insertMode}
                      canSave={hasDocumentId && !insertMode}
                      canDelete={hasDocumentId && !insertMode}
                      onEditTextChange={setEditText}
                      onClose={handleCloseEditor}
                      onSave={() => void handleSaveDocument()}
                      onInsert={() => void handleInsertDocument()}
                      onInsertNew={handleInsertNew}
                      onDelete={() => void handleDeleteDocument()}
                    />
                  )}
                </div>
              </>
            ) : (
              <div className="flex flex-1 items-center justify-center text-fg-muted">
                <div className="text-center">
                  <FileJson className="mx-auto h-10 w-10 opacity-20" />
                  <div className="mt-3 text-sm">{t('mongo.selectCollection')}</div>
                </div>
              </div>
            )}
          </div>
        </div>
      ) : (
        <DocumentQueryPanel connectionId={connectionId} />
      )}
    </div>
  );
}

function DocumentDetailEditor({
  editText,
  editError,
  mutating,
  insertMode,
  canSave,
  canDelete,
  onEditTextChange,
  onClose,
  onSave,
  onInsert,
  onInsertNew,
  onDelete,
}: {
  editText: string;
  editError: string | null;
  mutating: boolean;
  insertMode: boolean;
  canSave: boolean;
  canDelete: boolean;
  onEditTextChange: (value: string) => void;
  onClose: () => void;
  onSave: () => void;
  onInsert: () => void;
  onInsertNew: () => void;
  onDelete: () => void;
}) {
  const { t } = useI18n();

  return (
    <div className="flex w-[420px] shrink-0 flex-col border-l border-edge">
      <div className="flex shrink-0 items-center gap-1 border-b border-edge bg-surface-alt px-2 py-1.5">
        <span className="min-w-0 flex-1 truncate px-1 text-xs font-medium text-fg">
          {insertMode ? t('mongo.insert') : t('mongo.documentDetail')}
        </span>
        <Button
          variant={insertMode ? 'primary' : 'secondary'}
          className="h-7 gap-1 px-2 text-xs"
          onClick={insertMode ? onInsert : onInsertNew}
          disabled={mutating}
        >
          {mutating && insertMode ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <Plus className="h-3.5 w-3.5" />
          )}
          {t('mongo.insert')}
        </Button>
        {!insertMode && (
          <Button
            variant="primary"
            className="h-7 px-2 text-xs"
            onClick={onSave}
            disabled={mutating || !canSave}
            title={!canSave ? t('mongo.noIdHint') : undefined}
          >
            {mutating ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              t('common.save')
            )}
          </Button>
        )}
        {!insertMode && (
          <Button
            variant="secondary"
            className="h-7 gap-1 px-2 text-xs text-red-400 hover:text-red-300"
            onClick={onDelete}
            disabled={mutating || !canDelete}
          >
            <Trash2 className="h-3.5 w-3.5" />
            {t('common.delete')}
          </Button>
        )}
        <button
          type="button"
          className="rounded p-1 text-fg-muted hover:bg-surface-raised hover:text-fg"
          onClick={onClose}
        >
          <X className="h-3.5 w-3.5" />
        </button>
      </div>
      {!canSave && !insertMode && (
        <div className="shrink-0 border-b border-edge/50 bg-surface-raised/30 px-3 py-1.5 text-[11px] text-fg-muted">
          {t('mongo.noIdHint')}
        </div>
      )}
      {editError && (
        <div className="shrink-0 border-b border-red-500/20 bg-red-500/10 px-3 py-1.5 text-xs text-red-400">
          {editError}
        </div>
      )}
      <div className="min-h-0 flex-1 p-2">
        <textarea
          value={editText}
          onChange={(e) => onEditTextChange(e.target.value)}
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          className="h-full w-full resize-none rounded-md border border-edge bg-surface px-3 py-2 font-mono text-xs text-fg outline-none focus:border-blue-500"
        />
      </div>
    </div>
  );
}

function DocumentResultTable({
  result,
  selectedRowIdx,
  onSelectRow,
}: {
  result: StatementResult;
  selectedRowIdx: number | null;
  onSelectRow: (idx: number) => void;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const { columns, rows } = result;
  const { columnWidths, onResizeStart } = useColumnResize({ count: columns.length });
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 20,
  });

  return (
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
        <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
          {virtualizer.getVirtualItems().map((vRow) => {
            const row = rows[vRow.index];
            return (
              <button
                key={vRow.index}
                type="button"
                className={cn(
                  'absolute left-0 flex w-full border-b border-edge text-left',
                  selectedRowIdx === vRow.index
                    ? 'bg-blue-500/15'
                    : vRow.index % 2 === 0
                      ? 'bg-surface'
                      : 'bg-surface-raised/50',
                  'hover:bg-blue-500/10',
                )}
                style={{ top: vRow.start, height: ROW_HEIGHT }}
                onClick={() => onSelectRow(vRow.index)}
              >
                {row.map((cell, ci) => (
                  <div
                    key={columns[ci]?.name ?? ci}
                    className="flex shrink-0 items-center overflow-hidden border-r border-edge px-3 font-mono"
                    style={{ width: columnWidths[ci] }}
                  >
                    <span className="truncate text-fg-secondary">{cellToDisplay(cell)}</span>
                  </div>
                ))}
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}

function DocumentQueryPanel({ connectionId }: { connectionId: string }) {
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
  const activeResult = results[activeResultIdx];

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
        <span className="text-[11px] text-fg-muted">⌘+Enter — {t('mongo.queries')}</span>
        <div className="flex-1" />
        {tab.executionTimeMs != null && (
          <span className="text-[11px] text-fg-muted">{tab.executionTimeMs} ms</span>
        )}
      </div>
      <div className="min-h-[100px] border-b border-edge" style={{ height: '30%' }}>
        <SqlEditor
          value={tab.sql}
          onChange={(v) => updateSql(tab.id, v)}
          onExecute={handleExecute}
          placeholder={
            '{\n  "collection": "orders",\n  "filter": { "status": "paid" },\n  "limit": 20\n}'
          }
          schema={{}}
        />
      </div>
      <div className="flex min-h-0 flex-1 flex-col">
        {tab.running && results.length === 0 && (
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
        {results.length > 0 && activeResult && (
          <>
            {results.length > 1 && (
              <div className="flex shrink-0 items-center border-b border-edge bg-surface-alt px-1">
                {results.map((_r, idx) => (
                  <button
                    key={idx}
                    type="button"
                    className={cn(
                      'relative px-3 py-1.5 text-xs transition-colors',
                      idx === activeResultIdx
                        ? 'font-medium text-fg'
                        : 'text-fg-muted hover:text-fg-secondary',
                    )}
                    onClick={() => setActiveResult(tab.id, idx)}
                  >
                    {t('query.result')} {idx + 1}
                    <span
                      className={cn(
                        'absolute inset-x-0 bottom-0 h-0.5 bg-accent transition-opacity duration-300',
                        idx === activeResultIdx ? 'opacity-100' : 'opacity-0',
                      )}
                    />
                  </button>
                ))}
              </div>
            )}
            <DocumentResultTable
              result={activeResult}
              selectedRowIdx={null}
              onSelectRow={() => undefined}
            />
          </>
        )}
        {results.length === 0 && !tab.running && !tab.error && (
          <div className="flex flex-1 items-center justify-center text-sm text-fg-muted">
            {t('mongo.queryHint')}
          </div>
        )}
      </div>
    </div>
  );
}
