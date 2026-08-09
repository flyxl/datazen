import {
  useCallback,
  useEffect,
  useMemo,
  useState,
} from 'react';
import {
  CheckSquare,
  Loader2,
  Plus,
  RefreshCw,
  Square,
  Trash2,
} from 'lucide-react';
import { Button } from '../../../../src/components/ui/Button';
import { Input } from '../../../../src/components/ui/Input';
import { useI18n } from '../../../../src/hooks/useI18n';
import { cn } from '../../../../src/lib/cn';
import { pluginInvoke } from '../../../../src/plugins/generated';

export interface StreamEditorProps {
  connectionId: string;
  dbIndex: number;
  redisKey: string;
}

interface StreamEntry {
  id: string;
  fields: Record<string, string>;
}

interface StreamGroupInfo {
  name: string;
  consumers: number;
  pending: number;
  lastDeliveredId: string;
}

interface XpendingEntry {
  id: string;
  consumer: string;
  idleMs: number;
  deliveryCount: number;
}

type StreamTab = 'entries' | 'groups';

const ENTRY_PAGE_SIZE = 100;

export async function invokeXrange(
  connectionId: string,
  dbIndex: number,
  key: string,
  start = '-',
  end = '+',
  count?: number,
): Promise<{ entries: StreamEntry[] }> {
  return pluginInvoke('redis', 'xrange', {
    connectionId,
    dbIndex,
    key,
    start,
    end,
    count: count ?? null,
  });
}

export async function invokeXadd(
  connectionId: string,
  dbIndex: number,
  key: string,
  fields: Record<string, string>,
  id?: string,
): Promise<{ id: string }> {
  return pluginInvoke('redis', 'xadd', {
    connectionId,
    dbIndex,
    key,
    fields,
    id: id ?? null,
  });
}

export async function invokeXinfoGroups(
  connectionId: string,
  dbIndex: number,
  key: string,
): Promise<StreamGroupInfo[]> {
  return pluginInvoke('redis', 'xinfo_groups', {
    connectionId,
    dbIndex,
    key,
  });
}

export async function invokeXgroupCreate(
  connectionId: string,
  dbIndex: number,
  key: string,
  group: string,
  startId?: string,
): Promise<void> {
  await pluginInvoke('redis', 'xgroup_create', {
    connectionId,
    dbIndex,
    key,
    group,
    startId: startId ?? null,
  });
}

export async function invokeXgroupDestroy(
  connectionId: string,
  dbIndex: number,
  key: string,
  group: string,
): Promise<void> {
  await pluginInvoke('redis', 'xgroup_destroy', {
    connectionId,
    dbIndex,
    key,
    group,
  });
}

export async function invokeXpending(
  connectionId: string,
  dbIndex: number,
  key: string,
  group: string,
): Promise<{ total: number; entries: XpendingEntry[] }> {
  return pluginInvoke('redis', 'xpending', {
    connectionId,
    dbIndex,
    key,
    group,
    start: null,
    end: null,
    count: 100,
    consumer: null,
  });
}

export async function invokeXack(
  connectionId: string,
  dbIndex: number,
  key: string,
  group: string,
  ids: string[],
): Promise<number> {
  return pluginInvoke('redis', 'xack', {
    connectionId,
    dbIndex,
    key,
    group,
    ids,
  });
}

function formatFields(fields: Record<string, string>): string {
  return Object.entries(fields)
    .map(([k, v]) => `${k}=${v}`)
    .join(' · ');
}

export function StreamEditor({ connectionId, dbIndex, redisKey }: StreamEditorProps) {
  const { t } = useI18n();
  const [tab, setTab] = useState<StreamTab>('entries');
  const [entries, setEntries] = useState<StreamEntry[]>([]);
  const [groups, setGroups] = useState<StreamGroupInfo[]>([]);
  const [selectedGroup, setSelectedGroup] = useState<string | null>(null);
  const [pending, setPending] = useState<XpendingEntry[]>([]);
  const [selectedPending, setSelectedPending] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [newField, setNewField] = useState('');
  const [newValue, setNewValue] = useState('');
  const [newGroup, setNewGroup] = useState('');
  const [groupStartId, setGroupStartId] = useState('$');

  const tabs = useMemo(
    () =>
      [
        { id: 'entries' as const, label: t('redis.streamEntries') },
        { id: 'groups' as const, label: t('redis.streamGroups') },
      ] satisfies Array<{ id: StreamTab; label: string }>,
    [t],
  );

  const loadEntries = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await invokeXrange(
        connectionId,
        dbIndex,
        redisKey,
        '-',
        '+',
        ENTRY_PAGE_SIZE,
      );
      setEntries(result.entries);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setEntries([]);
    } finally {
      setLoading(false);
    }
  }, [connectionId, dbIndex, redisKey]);

  const loadGroups = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await invokeXinfoGroups(connectionId, dbIndex, redisKey);
      setGroups(result);
      if (selectedGroup && !result.some((g) => g.name === selectedGroup)) {
        setSelectedGroup(null);
        setPending([]);
        setSelectedPending(new Set());
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setGroups([]);
    } finally {
      setLoading(false);
    }
  }, [connectionId, dbIndex, redisKey, selectedGroup]);

  const loadPending = useCallback(
    async (group: string) => {
      setBusy(true);
      setError(null);
      try {
        const result = await invokeXpending(connectionId, dbIndex, redisKey, group);
        setPending(result.entries);
        setSelectedPending(new Set());
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
        setPending([]);
      } finally {
        setBusy(false);
      }
    },
    [connectionId, dbIndex, redisKey],
  );

  useEffect(() => {
    if (tab === 'entries') {
      void loadEntries();
    } else {
      void loadGroups();
    }
  }, [tab, loadEntries, loadGroups]);

  useEffect(() => {
    if (selectedGroup) {
      void loadPending(selectedGroup);
    } else {
      setPending([]);
      setSelectedPending(new Set());
    }
  }, [selectedGroup, loadPending]);

  const runAction = useCallback(async (fn: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await fn();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  const togglePending = useCallback((id: string) => {
    setSelectedPending((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }, []);

  return (
    <div className="space-y-3 text-xs">
      <div className="flex flex-wrap items-center gap-1">
        {tabs.map((item) => (
          <button
            key={item.id}
            type="button"
            className={cn(
              'rounded-md px-2.5 py-1 text-xs transition-colors',
              tab === item.id
                ? 'bg-surface-raised font-medium text-fg'
                : 'text-fg-secondary hover:bg-surface-raised/60 hover:text-fg',
            )}
            onClick={() => setTab(item.id)}
          >
            {item.label}
          </button>
        ))}
        <Button
          variant="secondary"
          className="ml-auto h-7 gap-1 px-2 text-xs"
          disabled={loading || busy}
          onClick={() => void (tab === 'entries' ? loadEntries() : loadGroups())}
        >
          <RefreshCw className={cn('h-3 w-3', loading && 'animate-spin')} />
          {t('redis.refresh')}
        </Button>
      </div>

      {error && (
        <div className="rounded-md border border-red-500/20 bg-red-500/10 px-2 py-1.5 text-red-400">
          {error}
        </div>
      )}

      {(loading || busy) && (
        <div className="flex items-center gap-2 text-fg-muted">
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
          {t('common.loading')}
        </div>
      )}

      {tab === 'entries' ? (
        <div className="space-y-2">
          <table className="w-full border-collapse">
            <thead>
              <tr className="border-b border-edge bg-surface-alt text-left">
                <th className="px-2 py-1.5 font-medium text-fg-muted">
                  {t('redis.streamEntryId')}
                </th>
                <th className="px-2 py-1.5 font-medium text-fg-muted">
                  {t('redis.streamFields')}
                </th>
              </tr>
            </thead>
            <tbody>
              {entries.map((entry) => (
                <tr key={entry.id} className="border-b border-edge">
                  <td className="px-2 py-1.5 font-mono text-fg-secondary">{entry.id}</td>
                  <td className="px-2 py-1.5 font-mono text-fg-secondary">
                    {formatFields(entry.fields)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {!loading && entries.length === 0 && (
            <p className="text-fg-muted">{t('redis.streamEntriesEmpty')}</p>
          )}

          <div className="flex flex-wrap items-end gap-2 rounded-md border border-edge bg-surface-alt p-2">
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
              disabled={busy || !newField.trim()}
              onClick={() =>
                void runAction(async () => {
                  await invokeXadd(connectionId, dbIndex, redisKey, {
                    [newField.trim()]: newValue,
                  });
                  setNewField('');
                  setNewValue('');
                  await loadEntries();
                })
              }
            >
              <Plus className="h-3 w-3" />
              {t('redis.streamAddEntry')}
            </Button>
          </div>
        </div>
      ) : (
        <div className="space-y-3">
          <table className="w-full border-collapse">
            <thead>
              <tr className="border-b border-edge bg-surface-alt text-left">
                <th className="px-2 py-1.5 font-medium text-fg-muted">
                  {t('redis.streamGroupName')}
                </th>
                <th className="px-2 py-1.5 font-medium text-fg-muted">
                  {t('redis.streamConsumers')}
                </th>
                <th className="px-2 py-1.5 font-medium text-fg-muted">
                  {t('redis.streamPending')}
                </th>
                <th className="px-2 py-1.5 font-medium text-fg-muted">
                  {t('redis.streamLastDeliveredId')}
                </th>
                <th className="w-24 px-2 py-1.5" />
              </tr>
            </thead>
            <tbody>
              {groups.map((group) => (
                <tr
                  key={group.name}
                  className={cn(
                    'border-b border-edge cursor-pointer',
                    selectedGroup === group.name && 'bg-blue-500/5',
                  )}
                  onClick={() => setSelectedGroup(group.name)}
                >
                  <td className="px-2 py-1.5 font-mono text-fg-secondary">{group.name}</td>
                  <td className="px-2 py-1.5 text-fg-secondary">{group.consumers}</td>
                  <td className="px-2 py-1.5 text-fg-secondary">{group.pending}</td>
                  <td className="px-2 py-1.5 font-mono text-fg-secondary">
                    {group.lastDeliveredId || '—'}
                  </td>
                  <td className="px-2 py-1.5">
                    <button
                      type="button"
                      className="rounded p-1 text-red-400 hover:bg-red-500/10"
                      title={t('redis.streamDestroyGroup')}
                      onClick={(e) => {
                        e.stopPropagation();
                        void runAction(async () => {
                          await invokeXgroupDestroy(
                            connectionId,
                            dbIndex,
                            redisKey,
                            group.name,
                          );
                          if (selectedGroup === group.name) {
                            setSelectedGroup(null);
                          }
                          await loadGroups();
                        });
                      }}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {!loading && groups.length === 0 && (
            <p className="text-fg-muted">{t('redis.streamGroupsEmpty')}</p>
          )}

          <div className="flex flex-wrap items-end gap-2 rounded-md border border-edge bg-surface-alt p-2">
            <Input
              value={newGroup}
              onChange={(e) => setNewGroup(e.target.value)}
              placeholder={t('redis.streamGroupName')}
              className="h-7 flex-1 font-mono text-xs"
            />
            <Input
              value={groupStartId}
              onChange={(e) => setGroupStartId(e.target.value)}
              placeholder={t('redis.streamStartId')}
              className="h-7 w-28 font-mono text-xs"
            />
            <Button
              variant="secondary"
              className="h-7 gap-1 px-2 text-xs"
              disabled={busy || !newGroup.trim()}
              onClick={() =>
                void runAction(async () => {
                  await invokeXgroupCreate(
                    connectionId,
                    dbIndex,
                    redisKey,
                    newGroup.trim(),
                    groupStartId.trim() || '$',
                  );
                  setNewGroup('');
                  await loadGroups();
                })
              }
            >
              <Plus className="h-3 w-3" />
              {t('redis.streamCreateGroup')}
            </Button>
          </div>

          {selectedGroup && (
            <div className="space-y-2 rounded-md border border-edge bg-surface-alt p-2">
              <div className="flex flex-wrap items-center gap-2">
                <span className="font-medium text-fg-muted">
                  {t('redis.streamPendingFor', { group: selectedGroup })}
                </span>
                <Button
                  variant="secondary"
                  className="ml-auto h-7 px-2 text-xs"
                  disabled={busy || selectedPending.size === 0}
                  onClick={() =>
                    void runAction(async () => {
                      await invokeXack(
                        connectionId,
                        dbIndex,
                        redisKey,
                        selectedGroup,
                        [...selectedPending],
                      );
                      await loadGroups();
                      await loadPending(selectedGroup);
                    })
                  }
                >
                  {t('redis.streamAck')}
                </Button>
              </div>
              <table className="w-full border-collapse">
                <thead>
                  <tr className="border-b border-edge text-left">
                    <th className="w-8 px-2 py-1" />
                    <th className="px-2 py-1 font-medium text-fg-muted">
                      {t('redis.streamEntryId')}
                    </th>
                    <th className="px-2 py-1 font-medium text-fg-muted">
                      {t('redis.streamConsumer')}
                    </th>
                    <th className="px-2 py-1 font-medium text-fg-muted">
                      {t('redis.streamIdleMs')}
                    </th>
                    <th className="px-2 py-1 font-medium text-fg-muted">
                      {t('redis.streamDeliveryCount')}
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {pending.map((entry) => {
                    const checked = selectedPending.has(entry.id);
                    return (
                      <tr key={entry.id} className="border-b border-edge">
                        <td className="px-2 py-1">
                          <button
                            type="button"
                            className="text-fg-muted hover:text-fg"
                            onClick={() => togglePending(entry.id)}
                          >
                            {checked ? (
                              <CheckSquare className="h-3.5 w-3.5" />
                            ) : (
                              <Square className="h-3.5 w-3.5" />
                            )}
                          </button>
                        </td>
                        <td className="px-2 py-1 font-mono text-fg-secondary">{entry.id}</td>
                        <td className="px-2 py-1 font-mono text-fg-secondary">
                          {entry.consumer}
                        </td>
                        <td className="px-2 py-1 text-fg-secondary">{entry.idleMs}</td>
                        <td className="px-2 py-1 text-fg-secondary">{entry.deliveryCount}</td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
              {!busy && pending.length === 0 && (
                <p className="text-fg-muted">{t('redis.streamPendingEmpty')}</p>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
