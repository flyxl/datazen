import { useCallback, useEffect, useMemo, useState } from 'react';
import { Plus, Trash2, Search, RefreshCw, ArrowUpDown } from 'lucide-react';
import { Button } from '@datazen/ui';
import { Input } from '@datazen/ui';
import { useI18n } from '@datazen/ui';
import type { KeyDetail } from '../shared/types';
import {
  invokeZsetAdd,
  invokeZsetRemove,
  invokeZsetScan,
  type ZsetScanMember,
} from './keyEditorsInvokes';
import type { GateWriteFn } from '../shared/useRedisGate';

const PAGE_SIZE = 100;

type SortDirection = 'none' | 'asc' | 'desc';

export function ZsetEditor({
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
  const [allMembers, setAllMembers] = useState<ZsetScanMember[]>([]);
  const [cursor, setCursor] = useState(0);
  const [hasMore, setHasMore] = useState(true);
  const [loading, setLoading] = useState(false);
  const [searchPattern, setSearchPattern] = useState('');
  const [sortDirection, setSortDirection] = useState<SortDirection>('none');

  // Editing state
  const [newMember, setNewMember] = useState('');
  const [newScore, setNewScore] = useState('0');
  const [editScores, setEditScores] = useState<Record<string, number>>({});

  const loadPage = useCallback(
    async (nextCursor: number, pattern: string) => {
      setLoading(true);
      try {
        const result = await invokeZsetScan(
          dbSessionId,
          dbIndex,
          detail.key,
          nextCursor,
          PAGE_SIZE,
          pattern || undefined,
        );
        if (nextCursor === 0) {
          setAllMembers(result.members);
        } else {
          setAllMembers((prev) => [...prev, ...result.members]);
        }
        setCursor(result.cursor);
        setHasMore(result.cursor !== 0);
      } finally {
        setLoading(false);
      }
    },
    [dbSessionId, dbIndex, detail.key],
  );

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
    setAllMembers([]);
    setCursor(0);
    setHasMore(true);
    setEditScores({});
    void loadPage(0, searchPattern);
  };

  const toggleSort = () => {
    setSortDirection((prev) => {
      if (prev === 'none') return 'asc';
      if (prev === 'asc') return 'desc';
      return 'none';
    });
  };

  const sortedMembers = useMemo(() => {
    if (sortDirection === 'none') return allMembers;
    const sorted = [...allMembers];
    sorted.sort((a, b) => {
      const sa = editScores[a.member] ?? a.score;
      const sb = editScores[b.member] ?? b.score;
      return sortDirection === 'asc' ? sa - sb : sb - sa;
    });
    return sorted;
  }, [allMembers, sortDirection, editScores]);

  const runWrite = useCallback(
    async (fn: () => Promise<void>) => {
      if (gateWrite && !(await gateWrite('write-op'))) return;
      await fn();
    },
    [gateWrite],
  );

  return (
    <div className="space-y-2">
      {/* Search + sort + refresh toolbar */}
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
        <Button
          variant={sortDirection !== 'none' ? 'primary' : 'ghost'}
          className="h-7 px-2 text-xs"
          onClick={toggleSort}
          title={
            sortDirection === 'asc'
              ? t('redis.sortAsc')
              : sortDirection === 'desc'
                ? t('redis.sortDesc')
                : t('redis.sortAsc')
          }
        >
          <ArrowUpDown className="h-3 w-3" />
          {sortDirection === 'asc' && '↑'}
          {sortDirection === 'desc' && '↓'}
        </Button>
        <Button variant="ghost" className="h-7 px-2 text-xs" onClick={handleRefresh}>
          <RefreshCw className="h-3 w-3" />
        </Button>
      </div>

      <table className="w-full border-collapse">
        <thead>
          <tr className="border-b border-edge bg-surface-alt text-left">
            <th className="px-2 py-1.5 font-medium text-fg-muted">{t('redis.score')}</th>
            <th className="px-2 py-1.5 font-medium text-fg-muted">{t('redis.member')}</th>
            <th className="w-12 px-2 py-1.5" />
          </tr>
        </thead>
        <tbody>
          {sortedMembers.map((item) => (
            <tr key={item.member} className="border-b border-edge">
              <td className="px-2 py-1.5">
                <Input
                  value={String(editScores[item.member] ?? item.score)}
                  onChange={(e) =>
                    setEditScores((prev) => ({
                      ...prev,
                      [item.member]: parseFloat(e.target.value) || 0,
                    }))
                  }
                  className="h-7 w-24 font-mono text-xs"
                />
              </td>
              <td className="px-2 py-1.5 font-mono text-fg-secondary">{item.member}</td>
              <td className="px-2 py-1.5">
                <div className="flex gap-1">
                  {editScores[item.member] !== undefined &&
                    editScores[item.member] !== item.score && (
                      <Button
                        variant="secondary"
                        className="h-6 px-1.5 text-[10px]"
                        onClick={() =>
                          void runWrite(async () => {
                            await invokeZsetRemove(dbSessionId, dbIndex, detail.key, [item.member]);
                            await invokeZsetAdd(dbSessionId, dbIndex, detail.key, [
                              { member: item.member, score: editScores[item.member] },
                            ]);
                            handleRefresh();
                            onChanged();
                          })
                        }
                      >
                        {t('common.save')}
                      </Button>
                    )}
                  <Button
                    variant="ghost"
                    className="h-6 px-1.5 text-[10px] text-danger"
                    onClick={() =>
                      void runWrite(async () => {
                        await invokeZsetRemove(dbSessionId, dbIndex, detail.key, [item.member]);
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
          {sortedMembers.length === 0 && !loading && (
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
          <span>{t('redis.totalItems', { count: String(allMembers.length) })}</span>
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

      {/* Add new member */}
      <div className="flex flex-wrap items-end gap-2">
        <Input
          value={newScore}
          onChange={(e) => setNewScore(e.target.value)}
          placeholder={t('redis.score')}
          className="h-7 w-20 text-xs"
        />
        <Input
          value={newMember}
          onChange={(e) => setNewMember(e.target.value)}
          placeholder={t('redis.member')}
          className="h-7 flex-1 font-mono text-xs"
        />
        <Button
          variant="secondary"
          className="h-7 gap-1 px-2 text-xs"
          disabled={!newMember.trim()}
          onClick={() =>
            void runWrite(async () => {
              await invokeZsetAdd(dbSessionId, dbIndex, detail.key, [
                { member: newMember.trim(), score: parseFloat(newScore) || 0 },
              ]);
              setNewMember('');
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
