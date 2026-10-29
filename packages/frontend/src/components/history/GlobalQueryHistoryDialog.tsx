import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Clock, Trash2 } from 'lucide-react';
import { Dialog } from '../ui/Dialog';
import { Button } from '../ui/Button';
import { useCopyFeedback } from '../ui/useCopyFeedback';
import { useI18n } from '../../hooks/useI18n';
import { useConnectionStore } from '../../stores/connectionStore';
import { queryCommands } from '../../commands/query';
import type { QueryHistoryEntry, QueryHistoryPage } from '../../types';
import { HistoryFilterBar } from './HistoryFilterBar';
import { HistoryEntryCard, type HistoryEntryAction } from './HistoryEntryCard';
import {
  DEFAULT_HISTORY_QUERY,
  applyStatusFilter,
  buildSqlExport,
  exportFileName,
  truncationNotice,
  type HistoryQueryState,
} from './historyQuery';

/** How long the per-row "已复制" marker stays before reverting. */
const COPIED_FEEDBACK_MS = 2000;

export interface GlobalQueryHistoryDialogProps {
  open: boolean;
  onClose: () => void;
  onSelectQuery?: (entry: QueryHistoryEntry) => void;
  initialConnectionId?: string;
}

/** Rows requested per read. The backend reports `total` so the UI can admit it. */
const PAGE_SIZE = 200;

/** Search is a server predicate now, so keystrokes are batched before querying. */
const SEARCH_DEBOUNCE_MS = 200;

/** Trails `value` by `delay` ms; resets the timer on every change. */
function useDebounced(value: string, delay: number): string {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    if (value === settled) return;
    const timer = setTimeout(() => setSettled(value), delay);
    return () => clearTimeout(timer);
  }, [value, delay, settled]);
  return settled;
}

export function GlobalQueryHistoryDialog({
  open,
  onClose,
  onSelectQuery,
  initialConnectionId,
}: Readonly<GlobalQueryHistoryDialogProps>) {
  const { t } = useI18n();
  const connections = useConnectionStore((s) => s.connections);

  const [page, setPage] = useState<QueryHistoryPage>({ entries: [], total: 0 });
  const [loading, setLoading] = useState(false);
  const [query, setQuery] = useState<HistoryQueryState>(DEFAULT_HISTORY_QUERY);
  const [selected, setSelected] = useState<Set<string>>(() => new Set());
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const { copied, copy } = useCopyFeedback(COPIED_FEEDBACK_MS);
  const [confirmClear, setConfirmClear] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const debouncedSearch = useDebounced(query.search, SEARCH_DEBOUNCE_MS);

  const connectionMap = useMemo(
    () => new Map(connections.map((c) => [c.id, c.name])),
    [connections],
  );

  // A request that resolves after a newer one must not overwrite it, or a slow
  // wide query can land on top of a fast narrow one and show the wrong rows.
  const requestId = useRef(0);

  useEffect(() => {
    if (!open) return;
    if (initialConnectionId) {
      setQuery((q) => ({ ...q, connectionId: initialConnectionId, database: '', schema: '' }));
    }
  }, [open, initialConnectionId]);

  /**
   * Only the fields the backend can evaluate. `status` is deliberately absent:
   * it has no column, so toggling it must not blank the list and re-query for
   * rows the server was going to send anyway.
   *
   * `search` is debounced because it is now a server predicate — without this,
   * every keystroke is an IPC round trip.
   */
  const serverQuery = useMemo(
    () => ({
      connectionId: query.connectionId,
      database: query.database,
      schema: query.schema,
      search: debouncedSearch.trim(),
      range: query.range,
      sort: query.sort,
    }),
    [query.connectionId, query.database, query.schema, query.range, query.sort, debouncedSearch],
  );

  useEffect(() => {
    if (!open) return;
    const id = ++requestId.current;
    setLoading(true);
    queryCommands
      .getQueryHistoryPage({
        limit: PAGE_SIZE,
        connectionId: serverQuery.connectionId === 'all' ? null : serverQuery.connectionId,
        database: serverQuery.database || null,
        schema: serverQuery.schema || null,
        search: serverQuery.search || null,
        since: rangeSince(serverQuery.range),
        order: serverQuery.sort,
      })
      .then((result) => {
        if (id !== requestId.current) return;
        setPage({ entries: result?.entries ?? [], total: result?.total ?? 0 });
      })
      .catch(() => {
        if (id !== requestId.current) return;
        setPage({ entries: [], total: 0 });
      })
      .finally(() => {
        if (id === requestId.current) setLoading(false);
      });
  }, [open, serverQuery]);

  // Status has no backend column, so it is the one filter applied here. Note
  // this can only ever shrink the page further; it can never make a truncated
  // page look complete.
  const visible = useMemo(
    () => applyStatusFilter(page.entries, query.status),
    [page.entries, query.status],
  );

  const hidden = truncationNotice(page);

  const toggleSelect = useCallback((id: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const handleAction = useCallback(
    async (action: HistoryEntryAction, entry: QueryHistoryEntry) => {
      try {
        if (action === 'copy') {
          copy(entry.sql);
          setCopiedId(entry.id);
        } else if (action === 'open') {
          onSelectQuery?.(entry);
          onClose();
        } else if (action === 'favorite') {
          const name = connectionMap.get(entry.connectionId) ?? entry.connectionId;
          await queryCommands.addFavoriteQuery(entry.connectionId, name, entry.sql);
          setNotice(t('query.historyFavoriteSaved'));
        } else if (action === 'delete') {
          // A 0 means the row was already gone; the list is refreshed either
          // way so the row cannot linger as a dead button.
          await queryCommands.deleteQueryHistoryEntry(entry.id);
          setSelected((prev) => {
            const next = new Set(prev);
            next.delete(entry.id);
            return next;
          });
        }
      } catch {
        // Every action is best-effort; a failed favorite or delete must not
        // tear down the dialog.
      }
    },
    [connectionMap, copy, onClose, onSelectQuery, t],
  );

  /**
   * `copied` is the shared, request-bound flag; `copiedId` says *which* row it
   * belongs to. Gating the marker on both means a rolled-back write drops the
   * marker even though `copiedId` still names the last attempted row.
   */
  const copiedRowId = copied ? copiedId : null;

  const handleExport = useCallback(async () => {
    const chosen = visible.filter((e) => selected.has(e.id));
    if (chosen.length === 0) return;
    const content = buildSqlExport(chosen);
    const wrote = await queryCommands.saveSqlFile(exportFileName(chosen.length), content);
    if (wrote) setNotice(t('query.historyExported', { count: chosen.length }));
  }, [selected, t, visible]);

  const handleClearHistory = useCallback(async () => {
    try {
      await queryCommands.clearQueryHistory();
      setPage({ entries: [], total: 0 });
      setSelected(new Set());
      setConfirmClear(false);
    } catch {
      setConfirmClear(false);
    }
  }, []);

  const selectedCount = visible.filter((e) => selected.has(e.id)).length;

  return (
    <Dialog
      open={open}
      title={t('query.historyTitle')}
      onClose={onClose}
      className="max-w-3xl"
      testId="global-query-history-dialog"
    >
      <div className="flex flex-col gap-3 min-h-[420px] max-h-[70vh]">
        <HistoryFilterBar
          value={query}
          onChange={setQuery}
          connections={connections}
          databases={[...new Set(page.entries.map((e) => e.database).filter(Boolean))]}
          schemas={[...new Set(page.entries.map((e) => e.schema).filter((s): s is string => !!s))]}
        />

        {notice && (
          <div className="flex items-center justify-between rounded bg-accent/10 px-2 py-1 text-[11px] text-accent">
            <span>{notice}</span>
            <button
              type="button"
              className="ml-2 text-fg-muted hover:text-fg"
              onClick={() => setNotice(null)}
            >
              ×
            </button>
          </div>
        )}

        {hidden && (
          <div
            className="rounded bg-surface-alt px-2 py-1 text-[11px] text-fg-muted"
            data-testid="history-truncation-notice"
          >
            {t('query.historyShowingOf', { shown: hidden.shown, total: hidden.total })}
          </div>
        )}

        <div className="flex items-center gap-2 text-[11px] text-fg-muted">
          <button
            type="button"
            onClick={() =>
              setSelected((prev) =>
                prev.size === visible.length ? new Set() : new Set(visible.map((e) => e.id)),
              )
            }
            className="hover:text-accent"
            data-testid="global-history-select-all"
          >
            {t('query.historySelectAll')}
          </button>
          <Button
            variant="ghost"
            size="sm"
            className="h-7 px-2 text-xs"
            disabled={selectedCount === 0}
            onClick={() => void handleExport()}
            title={t('query.historyExport')}
            data-testid="global-history-export"
          >
            {selectedCount > 0
              ? t('query.historyExportSelected', { count: selectedCount })
              : t('query.historyExport')}
          </Button>

          <div className="ml-auto">
            {confirmClear ? (
              <div className="flex items-center gap-1">
                <span className="text-[11px] text-danger">{t('query.historyClearConfirm')}</span>
                <Button
                  variant="danger"
                  size="sm"
                  className="h-7 px-2 text-xs"
                  onClick={() => void handleClearHistory()}
                >
                  {t('query.historyConfirm')}
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-7 px-2 text-xs"
                  onClick={() => setConfirmClear(false)}
                >
                  {t('query.historyCancel')}
                </Button>
              </div>
            ) : (
              <Button
                variant="ghost"
                size="sm"
                className="h-8 px-2 text-xs text-fg-muted hover:text-danger"
                onClick={() => setConfirmClear(true)}
                title={t('query.historyClearTitle')}
                data-testid="global-history-clear"
              >
                <Trash2 className="h-3.5 w-3.5 mr-1" />
                {t('query.historyClear')}
              </Button>
            )}
          </div>
        </div>

        <div className="flex-1 overflow-y-auto space-y-2 pr-1">
          {/*
            Only a cold read shows the spinner. Blanking the list on every
            filter keystroke makes the dialog feel like it is reloading when it
            is merely refiltering.
          */}
          {loading && visible.length === 0 ? (
            <div className="flex h-48 items-center justify-center text-xs text-fg-muted">
              {t('query.historyLoading')}
            </div>
          ) : visible.length === 0 ? (
            <div className="flex h-48 flex-col items-center justify-center text-center text-fg-muted">
              <Clock className="h-8 w-8 opacity-30 mb-2" />
              <p className="text-xs">
                {page.total === 0 && page.entries.length === 0
                  ? t('query.noHistory')
                  : t('query.noHistoryMatch')}
              </p>
            </div>
          ) : (
            visible.map((item) => (
              <HistoryEntryCard
                key={item.id}
                entry={item}
                connectionName={connectionMap.get(item.connectionId) ?? item.connectionId}
                selected={selected.has(item.id)}
                onToggleSelect={toggleSelect}
                onAction={(action, entry) => void handleAction(action, entry)}
                copiedId={copiedRowId}
                canOpen={Boolean(onSelectQuery)}
              />
            ))
          )}
        </div>
      </div>
    </Dialog>
  );
}

/** Mirrors `rangeSinceIso` in historyQuery, kept inline to avoid a Date per render. */
function rangeSince(range: HistoryQueryState['range']): string | null {
  const days =
    range === 'today' ? 1 : range === '7d' ? 7 : range === '30d' ? 30 : range === '90d' ? 90 : 0;
  if (days === 0) return null;
  return new Date(Date.now() - days * 86_400_000).toISOString();
}
