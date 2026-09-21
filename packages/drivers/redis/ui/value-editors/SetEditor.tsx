import { useCallback, useEffect, useState } from 'react';
import { Plus, Trash2, Search, RefreshCw } from 'lucide-react';
import { Button } from '@datazen/ui';
import { Input } from '@datazen/ui';
import { useI18n } from '@datazen/ui';
import type { KeyDetail } from '../shared/types';
import { invokeSetAdd, invokeSetRemove, invokeSetScan } from './keyEditorsInvokes';
import type { GateWriteFn } from '../shared/useRedisGate';

const PAGE_SIZE = 100;

export function SetEditor({
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
  const [members, setMembers] = useState<string[]>([]);
  const [cursor, setCursor] = useState(0);
  const [hasMore, setHasMore] = useState(true);
  const [loading, setLoading] = useState(false);
  const [searchPattern, setSearchPattern] = useState('');

  // Editing state
  const [newMember, setNewMember] = useState('');
  const [editMember, setEditMember] = useState<Record<string, string>>({});

  const loadPage = useCallback(
    async (nextCursor: number, pattern: string) => {
      setLoading(true);
      try {
        const result = await invokeSetScan(
          dbSessionId,
          dbIndex,
          detail.key,
          nextCursor,
          PAGE_SIZE,
          pattern || undefined,
        );
        if (nextCursor === 0) {
          setMembers(result.members);
        } else {
          setMembers((prev) => [...prev, ...result.members]);
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
    setMembers([]);
    setCursor(0);
    setHasMore(true);
    setEditMember({});
    void loadPage(0, searchPattern);
  };

  const getEditedMember = (original: string) => editMember[original] ?? original;

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
            <th className="px-2 py-1.5 font-medium text-fg-muted">{t('redis.member')}</th>
            <th className="w-12 px-2 py-1.5" />
          </tr>
        </thead>
        <tbody>
          {members.map((member) => (
            <tr key={member} className="border-b border-edge">
              <td className="px-2 py-1.5">
                <Input
                  value={getEditedMember(member)}
                  onChange={(e) => setEditMember((prev) => ({ ...prev, [member]: e.target.value }))}
                  className="h-7 font-mono text-xs"
                />
              </td>
              <td className="px-2 py-1.5">
                <div className="flex gap-1">
                  {editMember[member] !== undefined && editMember[member] !== member && (
                    <Button
                      variant="secondary"
                      className="h-6 px-1.5 text-[10px]"
                      onClick={() =>
                        void runWrite(async () => {
                          // Remove old member, add edited version
                          await invokeSetRemove(dbSessionId, dbIndex, detail.key, [member]);
                          await invokeSetAdd(dbSessionId, dbIndex, detail.key, [
                            editMember[member],
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
                        await invokeSetRemove(dbSessionId, dbIndex, detail.key, [member]);
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
          {members.length === 0 && !loading && (
            <tr>
              <td colSpan={2} className="px-2 py-4 text-center text-fg-muted">
                {t('redis.noKeys')}
              </td>
            </tr>
          )}
        </tbody>
      </table>

      {/* Load more */}
      {hasMore && (
        <div className="flex items-center justify-between text-xs text-fg-muted">
          <span>{t('redis.totalItems', { count: String(members.length) })}</span>
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
              await invokeSetAdd(dbSessionId, dbIndex, detail.key, [newMember.trim()]);
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
