import { useCallback, useEffect, useState } from 'react';
import { Plus, Trash2, Search, RefreshCw } from 'lucide-react';
import { Button } from '@datazen/ui';
import { Input } from '@datazen/ui';
import { useI18n } from '@datazen/ui';
import type { KeyDetail } from '../shared/types';
import {
  invokeHashDel,
  invokeHashSet,
  invokeHashScan,
  type HashScanEntry,
} from './keyEditorsInvokes';
import type { GateWriteFn } from '../shared/useRedisGate';

const PAGE_SIZE = 100;

export function HashEditor({
  dbSessionId,
  dbIndex,
  detail,
  gateWrite,
  onChanged,
}: {
  dbSessionId: string;
  dbIndex: number;
  detail: KeyDetail;
  gateWrite?: GateWriteFn;
  onChanged: () => void;
}) {
  const { t } = useI18n();

  // Cursor-based pagination state
  const [entries, setEntries] = useState<HashScanEntry[]>([]);
  const [cursor, setCursor] = useState(0);
  const [hasMore, setHasMore] = useState(true);
  const [loading, setLoading] = useState(false);
  const [, setPage] = useState(1);
  const [searchPattern, setSearchPattern] = useState('');

  // Editing state
  const [newField, setNewField] = useState('');
  const [newValue, setNewValue] = useState('');
  const [editValues, setEditValues] = useState<Record<string, string>>({});
  const [editField, setEditField] = useState<string | null>(null);

  const loadPage = useCallback(
    async (nextCursor: number, pattern: string) => {
      setLoading(true);
      try {
        const result = await invokeHashScan(
          dbSessionId,
          dbIndex,
          detail.key,
          nextCursor,
          PAGE_SIZE,
          pattern || undefined,
        );
        if (nextCursor === 0) {
          setEntries(result.entries);
        } else {
          setEntries((prev) => [...prev, ...result.entries]);
        }
        setCursor(result.cursor);
        setHasMore(result.cursor !== 0);
        if (nextCursor === 0) setPage(1);
      } finally {
        setLoading(false);
      }
    },
    [dbSessionId, dbIndex, detail.key],
  );

  // Initial load
  useEffect(() => {
    void loadPage(0, '');
  }, [loadPage]);

  const handleSearch = () => {
    void loadPage(0, searchPattern);
  };

  const handleLoadMore = () => {
    void loadPage(cursor, searchPattern);
  };

  const handleRefresh = () => {
    setEntries([]);
    setCursor(0);
    setHasMore(true);
    setEditField(null);
    void loadPage(0, searchPattern);
  };

  const getValue = (field: string, original: string) => editValues[field] ?? original;

  const runWrite = useCallback(
    async (fn: () => Promise<void>) => {
      if (gateWrite && !(await gateWrite('write-op'))) return;
      await fn();
    },
    [gateWrite],
  );

  return (
    <div className="space-y-2">
      {/* Search + refresh toolbar */}
      <div className="flex items-center gap-2">
        <div className="relative flex-1">
          <Search className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-fg-muted" />
          <Input
            value={searchPattern}
            onChange={(e) => setSearchPattern(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') handleSearch();
            }}
            placeholder={t('redis.search')}
            className="h-7 pl-7 font-mono text-xs"
          />
        </div>
        <Button variant="secondary" className="h-7 px-2 text-xs" onClick={handleSearch}>
          {t('redis.search')}
        </Button>
        <Button variant="ghost" className="h-7 px-2 text-xs" onClick={handleRefresh}>
          <RefreshCw className="h-3 w-3" />
        </Button>
      </div>

      <table className="w-full border-collapse">
        <thead>
          <tr className="border-b border-edge bg-surface-alt text-left">
            <th className="px-2 py-1.5 font-medium text-fg-muted">{t('redis.field')}</th>
            <th className="px-2 py-1.5 font-medium text-fg-muted">{t('redis.value')}</th>
            <th className="w-20 px-2 py-1.5" />
          </tr>
        </thead>
        <tbody>
          {entries.map((entry) => (
            <tr key={entry.field} className="border-b border-edge">
              <td className="px-2 py-1.5 font-mono text-fg-secondary">{entry.field}</td>
              <td className="px-2 py-1.5">
                {editField === entry.field ? (
                  <Input
                    value={getValue(entry.field, entry.value)}
                    onChange={(e) =>
                      setEditValues((prev) => ({ ...prev, [entry.field]: e.target.value }))
                    }
                    className="h-7 font-mono text-xs"
                  />
                ) : (
                  <span className="font-mono text-fg-secondary text-xs">{String(entry.value)}</span>
                )}
              </td>
              <td className="px-2 py-1.5">
                <div className="flex gap-1">
                  {editField === entry.field ? (
                    <Button
                      variant="secondary"
                      className="h-6 px-1.5 text-[10px]"
                      onClick={() =>
                        void runWrite(async () => {
                          await invokeHashSet(
                            dbSessionId,
                            dbIndex,
                            detail.key,
                            entry.field,
                            getValue(entry.field, entry.value),
                          );
                          setEditField(null);
                          handleRefresh();
                          onChanged();
                        })
                      }
                    >
                      {t('common.save')}
                    </Button>
                  ) : (
                    <Button
                      variant="ghost"
                      className="h-6 px-1.5 text-[10px] text-accent"
                      onClick={() => {
                        setEditField(entry.field);
                        setEditValues((prev) => ({ ...prev, [entry.field]: String(entry.value) }));
                      }}
                    >
                      {t('redis.edit')}
                    </Button>
                  )}
                  <Button
                    variant="ghost"
                    className="h-6 px-1.5 text-[10px] text-danger"
                    onClick={() =>
                      void runWrite(async () => {
                        await invokeHashDel(dbSessionId, dbIndex, detail.key, [entry.field]);
                        handleRefresh();
                        onChanged();
                      })
                    }
                  >
                    <Trash2 className="h-3 w-3" />
                  </Button>
                </div>
              </td>
            </tr>
          ))}
          {entries.length === 0 && !loading && (
            <tr>
              <td colSpan={3} className="px-2 py-4 text-center text-fg-muted">
                {t('redis.noKeys')}
              </td>
            </tr>
          )}
        </tbody>
      </table>

      {/* Load more */}
      {hasMore && (
        <div className="flex items-center justify-between text-xs text-fg-muted">
          <span>{t('redis.totalItems', { count: String(entries.length) })}</span>
          <Button
            variant="secondary"
            className="h-7 gap-1 px-2 text-xs"
            disabled={loading}
            onClick={handleLoadMore}
          >
            {loading ? '…' : t('redis.loadMore')}
          </Button>
        </div>
      )}

      {/* Add new field */}
      <div className="flex flex-wrap items-end gap-2">
        <Input
          value={newField}
          onChange={(e) => setNewField(e.target.value)}
          placeholder={t('redis.field')}
          className="h-7 flex-1 font-mono text-xs"
        />
        <Input
          value={newValue}
          onChange={(e) => setNewValue(e.target.value)}
          placeholder={t('redis.value')}
          className="h-7 flex-1 font-mono text-xs"
        />
        <Button
          variant="secondary"
          className="h-7 gap-1 px-2 text-xs"
          disabled={!newField.trim()}
          onClick={() =>
            void runWrite(async () => {
              await invokeHashSet(dbSessionId, dbIndex, detail.key, newField.trim(), newValue);
              setNewField('');
              setNewValue('');
              handleRefresh();
              onChanged();
            })
          }
        >
          <Plus className="h-3 w-3" />
          {t('redis.add')}
        </Button>
      </div>
    </div>
  );
}
