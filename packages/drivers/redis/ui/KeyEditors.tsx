import { useCallback, useState } from 'react';
import { Loader2, Plus, Trash2 } from 'lucide-react';
import { Button } from '../../../../src/components/ui/Button';
import { Input } from '../../../../src/components/ui/Input';
import { useI18n } from '../../../../src/hooks/useI18n';
import { pluginInvoke } from '../../../../src/plugins/generated';
import type { KeyDetail } from '../../../../src/types';

export type PluginInvokeFn = (
  pluginId: string,
  command: string,
  args?: Record<string, unknown>,
) => Promise<unknown>;

export async function invokeSetString(
  connectionId: string,
  dbIndex: number,
  key: string,
  value: string,
  invoke: PluginInvokeFn = pluginInvoke,
) {
  await invoke('redis', 'set_string', {
    connection_id: connectionId,
    db_index: dbIndex,
    key,
    value,
  });
}

export async function invokeHashSet(
  connectionId: string,
  dbIndex: number,
  key: string,
  field: string,
  value: string,
  invoke: PluginInvokeFn = pluginInvoke,
) {
  await invoke('redis', 'hash_set', {
    connection_id: connectionId,
    db_index: dbIndex,
    key,
    field,
    value,
  });
}

export async function invokeHashDel(
  connectionId: string,
  dbIndex: number,
  key: string,
  fields: string[],
  invoke: PluginInvokeFn = pluginInvoke,
) {
  await invoke('redis', 'hash_del', {
    connection_id: connectionId,
    db_index: dbIndex,
    key,
    fields,
  });
}

export async function invokeListPush(
  connectionId: string,
  dbIndex: number,
  key: string,
  side: 'left' | 'right',
  values: string[],
  invoke: PluginInvokeFn = pluginInvoke,
) {
  await invoke('redis', 'list_push', {
    connection_id: connectionId,
    db_index: dbIndex,
    key,
    side,
    values,
  });
}

export async function invokeListSet(
  connectionId: string,
  dbIndex: number,
  key: string,
  index: number,
  value: string,
  invoke: PluginInvokeFn = pluginInvoke,
) {
  await invoke('redis', 'list_set', {
    connection_id: connectionId,
    db_index: dbIndex,
    key,
    index,
    value,
  });
}

export async function invokeListPop(
  connectionId: string,
  dbIndex: number,
  key: string,
  side: 'left' | 'right',
  invoke: PluginInvokeFn = pluginInvoke,
) {
  await invoke('redis', 'list_pop', {
    connection_id: connectionId,
    db_index: dbIndex,
    key,
    side,
  });
}

export async function invokeSetAdd(
  connectionId: string,
  dbIndex: number,
  key: string,
  members: string[],
  invoke: PluginInvokeFn = pluginInvoke,
) {
  await invoke('redis', 'set_add', {
    connection_id: connectionId,
    db_index: dbIndex,
    key,
    members,
  });
}

export async function invokeSetRemove(
  connectionId: string,
  dbIndex: number,
  key: string,
  members: string[],
  invoke: PluginInvokeFn = pluginInvoke,
) {
  await invoke('redis', 'set_remove', {
    connection_id: connectionId,
    db_index: dbIndex,
    key,
    members,
  });
}

export async function invokeZsetAdd(
  connectionId: string,
  dbIndex: number,
  key: string,
  members: { member: string; score: number }[],
  invoke: PluginInvokeFn = pluginInvoke,
) {
  await invoke('redis', 'zset_add', {
    connection_id: connectionId,
    db_index: dbIndex,
    key,
    members,
  });
}

export async function invokeZsetRemove(
  connectionId: string,
  dbIndex: number,
  key: string,
  members: string[],
  invoke: PluginInvokeFn = pluginInvoke,
) {
  await invoke('redis', 'zset_remove', {
    connection_id: connectionId,
    db_index: dbIndex,
    key,
    members,
  });
}

export async function invokeRename(
  connectionId: string,
  dbIndex: number,
  key: string,
  newKey: string,
  invoke: PluginInvokeFn = pluginInvoke,
) {
  await invoke('redis', 'rename', {
    connection_id: connectionId,
    db_index: dbIndex,
    key,
    new_key: newKey,
  });
}

export async function invokeSetTtl(
  connectionId: string,
  dbIndex: number,
  key: string,
  ttlSeconds: number,
  invoke: PluginInvokeFn = pluginInvoke,
) {
  await invoke('redis', 'set_ttl', {
    connection_id: connectionId,
    db_index: dbIndex,
    key,
    ttl_seconds: ttlSeconds,
  });
}

export interface KeyDetailEditorProps {
  connectionId: string;
  dbIndex: number;
  detail: KeyDetail;
  onRefresh: () => void | Promise<void>;
  onRenamed?: (newKey: string) => void;
}

export function KeyDetailEditor({
  connectionId,
  dbIndex,
  detail,
  onRefresh,
  onRenamed,
}: KeyDetailEditorProps) {
  const { t } = useI18n();
  const [ttlInput, setTtlInput] = useState(
    detail.ttl < 0 ? '' : String(detail.ttl),
  );
  const [renameInput, setRenameInput] = useState(detail.key);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = useCallback(
    async (fn: () => Promise<void>) => {
      setBusy(true);
      setError(null);
      try {
        await fn();
        await onRefresh();
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setBusy(false);
      }
    },
    [onRefresh],
  );

  const ttlText =
    detail.ttl < 0
      ? t('redis.noExpiry')
      : `${detail.ttl} ${t('redis.seconds')}`;

  return (
    <div className="space-y-3 text-xs">
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-medium text-fg-muted">{t('redis.type')}:</span>
        <span className="rounded bg-blue-500/10 px-1.5 py-0.5 text-blue-400">
          {detail.keyType}
        </span>
        <span className="font-medium text-fg-muted">TTL:</span>
        <span className="text-fg-secondary">{ttlText}</span>
      </div>

      <div className="flex flex-wrap items-end gap-2 rounded-md border border-edge bg-surface-alt p-2">
        <div className="flex min-w-[120px] flex-1 flex-col gap-1">
          <label className="text-fg-muted">{t('redis.setTtl')}</label>
          <Input
            value={ttlInput}
            onChange={(e) => setTtlInput(e.target.value)}
            placeholder={t('redis.ttlSeconds')}
            className="h-7 text-xs"
          />
        </div>
        <Button
          variant="secondary"
          className="h-7 px-2 text-xs"
          disabled={busy}
          onClick={() =>
            void run(async () => {
              const secs = parseInt(ttlInput, 10);
              if (Number.isNaN(secs) || secs < 0) {
                throw new Error(t('redis.ttlSeconds'));
              }
              await invokeSetTtl(connectionId, dbIndex, detail.key, secs);
            })
          }
        >
          {t('redis.setTtl')}
        </Button>
        <Button
          variant="secondary"
          className="h-7 px-2 text-xs"
          disabled={busy}
          onClick={() =>
            void run(async () => {
              await invokeSetTtl(connectionId, dbIndex, detail.key, -1);
              setTtlInput('');
            })
          }
        >
          {t('redis.persist')}
        </Button>
      </div>

      <div className="flex flex-wrap items-end gap-2 rounded-md border border-edge bg-surface-alt p-2">
        <div className="flex min-w-[120px] flex-1 flex-col gap-1">
          <label className="text-fg-muted">{t('redis.renameKey')}</label>
          <Input
            value={renameInput}
            onChange={(e) => setRenameInput(e.target.value)}
            className="h-7 font-mono text-xs"
          />
        </div>
        <Button
          variant="secondary"
          className="h-7 px-2 text-xs"
          disabled={busy || renameInput === detail.key || !renameInput.trim()}
          onClick={() =>
            void run(async () => {
              const next = renameInput.trim();
              await invokeRename(connectionId, dbIndex, detail.key, next);
              onRenamed?.(next);
            })
          }
        >
          {t('redis.renameKey')}
        </Button>
      </div>

      {error && (
        <div className="rounded-md border border-red-500/20 bg-red-500/10 px-2 py-1.5 text-red-400">
          {error}
        </div>
      )}

      {busy && (
        <div className="flex items-center gap-2 text-fg-muted">
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
          {t('common.loading')}
        </div>
      )}

      {detail.keyType === 'string' && (
        <StringEditor
          connectionId={connectionId}
          dbIndex={dbIndex}
          detail={detail}
          onSaved={() => void onRefresh()}
        />
      )}

      {detail.keyType === 'hash' && (
        <HashEditor
          connectionId={connectionId}
          dbIndex={dbIndex}
          detail={detail}
          onChanged={() => void onRefresh()}
        />
      )}

      {detail.keyType === 'list' && (
        <ListEditor
          connectionId={connectionId}
          dbIndex={dbIndex}
          detail={detail}
          onChanged={() => void onRefresh()}
        />
      )}

      {detail.keyType === 'set' && (
        <SetEditor
          connectionId={connectionId}
          dbIndex={dbIndex}
          detail={detail}
          onChanged={() => void onRefresh()}
        />
      )}

      {detail.keyType === 'zset' && (
        <ZsetEditor
          connectionId={connectionId}
          dbIndex={dbIndex}
          detail={detail}
          onChanged={() => void onRefresh()}
        />
      )}

      {detail.keyType === 'stream' && (
        <div className="space-y-2">
          <p className="text-fg-muted">{t('redis.streamReadOnly')}</p>
          <div className="rounded-md border border-edge bg-surface-alt p-3">
            <pre className="whitespace-pre-wrap break-all font-mono text-fg-secondary">
              {JSON.stringify(detail.value, null, 2)}
            </pre>
          </div>
        </div>
      )}

      {!['string', 'hash', 'list', 'set', 'zset', 'stream'].includes(detail.keyType) && (
        <div className="rounded-md border border-edge bg-surface-alt p-3">
          <pre className="whitespace-pre-wrap break-all font-mono text-fg-secondary">
            {JSON.stringify(detail.value, null, 2)}
          </pre>
        </div>
      )}
    </div>
  );
}

function StringEditor({
  connectionId,
  dbIndex,
  detail,
  onSaved,
}: {
  connectionId: string;
  dbIndex: number;
  detail: KeyDetail;
  onSaved: () => void;
}) {
  const { t } = useI18n();
  const initial =
    typeof detail.value === 'object'
      ? JSON.stringify(detail.value, null, 2)
      : String(detail.value ?? '');
  const [value, setValue] = useState(initial);
  const [saving, setSaving] = useState(false);

  return (
    <div className="space-y-2">
      <textarea
        value={value}
        onChange={(e) => setValue(e.target.value)}
        className="min-h-[120px] w-full rounded-md border border-edge bg-surface-alt p-3 font-mono text-fg-secondary"
      />
      <Button
        variant="primary"
        className="h-7 px-2 text-xs"
        disabled={saving}
        onClick={() => {
          setSaving(true);
          void invokeSetString(connectionId, dbIndex, detail.key, value)
            .then(onSaved)
            .finally(() => setSaving(false));
        }}
      >
        {t('common.save')}
      </Button>
    </div>
  );
}

function HashEditor({
  connectionId,
  dbIndex,
  detail,
  onChanged,
}: {
  connectionId: string;
  dbIndex: number;
  detail: KeyDetail;
  onChanged: () => void;
}) {
  const { t } = useI18n();
  const raw =
    typeof detail.value === 'object' && detail.value !== null
      ? ((detail.value as Record<string, Record<string, string>>).fields ??
        (detail.value as Record<string, string>))
      : {};
  const fields = Object.entries(raw);
  const [newField, setNewField] = useState('');
  const [newValue, setNewValue] = useState('');
  const [editValues, setEditValues] = useState<Record<string, string>>({});

  const getValue = (field: string, original: string) =>
    editValues[field] ?? original;

  return (
    <div className="space-y-2">
      <table className="w-full border-collapse">
        <thead>
          <tr className="border-b border-edge bg-surface-alt text-left">
            <th className="px-2 py-1.5 font-medium text-fg-muted">{t('redis.field')}</th>
            <th className="px-2 py-1.5 font-medium text-fg-muted">{t('redis.value')}</th>
            <th className="w-20 px-2 py-1.5" />
          </tr>
        </thead>
        <tbody>
          {fields.map(([field, val]) => (
            <tr key={field} className="border-b border-edge">
              <td className="px-2 py-1.5 font-mono text-fg-secondary">{field}</td>
              <td className="px-2 py-1.5">
                <Input
                  value={getValue(field, String(val))}
                  onChange={(e) =>
                    setEditValues((prev) => ({ ...prev, [field]: e.target.value }))
                  }
                  className="h-7 font-mono text-xs"
                />
              </td>
              <td className="px-2 py-1.5">
                <div className="flex gap-1">
                  <Button
                    variant="secondary"
                    className="h-6 px-1.5 text-[10px]"
                    onClick={() =>
                      void invokeHashSet(
                        connectionId,
                        dbIndex,
                        detail.key,
                        field,
                        getValue(field, String(val)),
                      ).then(onChanged)
                    }
                  >
                    {t('common.save')}
                  </Button>
                  <button
                    type="button"
                    className="rounded p-1 text-red-400 hover:bg-red-500/10"
                    onClick={() =>
                      void invokeHashDel(connectionId, dbIndex, detail.key, [field]).then(
                        onChanged,
                      )
                    }
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                  </button>
                </div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
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
            void invokeHashSet(
              connectionId,
              dbIndex,
              detail.key,
              newField.trim(),
              newValue,
            ).then(() => {
              setNewField('');
              setNewValue('');
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

function listItems(detail: KeyDetail): string[] {
  const v = detail.value as Record<string, unknown>;
  const items = v?.items ?? v?.members;
  return Array.isArray(items) ? (items as string[]) : [];
}

function ListEditor({
  connectionId,
  dbIndex,
  detail,
  onChanged,
}: {
  connectionId: string;
  dbIndex: number;
  detail: KeyDetail;
  onChanged: () => void;
}) {
  const { t } = useI18n();
  const items = listItems(detail);
  const [pushValue, setPushValue] = useState('');
  const [editIndex, setEditIndex] = useState<number | null>(null);
  const [editValue, setEditValue] = useState('');

  return (
    <div className="space-y-2">
      <div className="flex flex-wrap gap-1">
        <Button
          variant="secondary"
          className="h-7 px-2 text-xs"
          onClick={() =>
            void invokeListPop(connectionId, dbIndex, detail.key, 'left').then(onChanged)
          }
        >
          {t('redis.popLeft')}
        </Button>
        <Button
          variant="secondary"
          className="h-7 px-2 text-xs"
          onClick={() =>
            void invokeListPop(connectionId, dbIndex, detail.key, 'right').then(onChanged)
          }
        >
          {t('redis.popRight')}
        </Button>
      </div>
      <table className="w-full border-collapse">
        <thead>
          <tr className="border-b border-edge bg-surface-alt text-left">
            <th className="w-16 px-2 py-1.5 font-medium text-fg-muted">#</th>
            <th className="px-2 py-1.5 font-medium text-fg-muted">{t('redis.value')}</th>
            <th className="w-16 px-2 py-1.5" />
          </tr>
        </thead>
        <tbody>
          {items.map((item, i) => (
            <tr key={i} className="border-b border-edge">
              <td className="px-2 py-1.5 text-fg-muted">{i}</td>
              <td className="px-2 py-1.5">
                {editIndex === i ? (
                  <Input
                    value={editValue}
                    onChange={(e) => setEditValue(e.target.value)}
                    className="h-7 font-mono text-xs"
                  />
                ) : (
                  <span className="font-mono text-fg-secondary">{String(item)}</span>
                )}
              </td>
              <td className="px-2 py-1.5">
                {editIndex === i ? (
                  <Button
                    variant="secondary"
                    className="h-6 px-1.5 text-[10px]"
                    onClick={() =>
                      void invokeListSet(
                        connectionId,
                        dbIndex,
                        detail.key,
                        i,
                        editValue,
                      ).then(() => {
                        setEditIndex(null);
                        onChanged();
                      })
                    }
                  >
                    {t('common.save')}
                  </Button>
                ) : (
                  <button
                    type="button"
                    className="text-xs text-blue-400 hover:underline"
                    onClick={() => {
                      setEditIndex(i);
                      setEditValue(String(item));
                    }}
                  >
                    {t('common.save')}
                  </button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <div className="flex flex-wrap items-end gap-2">
        <Input
          value={pushValue}
          onChange={(e) => setPushValue(e.target.value)}
          placeholder={t('redis.value')}
          className="h-7 flex-1 font-mono text-xs"
        />
        <Button
          variant="secondary"
          className="h-7 px-2 text-xs"
          disabled={!pushValue.trim()}
          onClick={() =>
            void invokeListPush(connectionId, dbIndex, detail.key, 'left', [
              pushValue.trim(),
            ]).then(() => {
              setPushValue('');
              onChanged();
            })
          }
        >
          {t('redis.pushLeft')}
        </Button>
        <Button
          variant="secondary"
          className="h-7 px-2 text-xs"
          disabled={!pushValue.trim()}
          onClick={() =>
            void invokeListPush(connectionId, dbIndex, detail.key, 'right', [
              pushValue.trim(),
            ]).then(() => {
              setPushValue('');
              onChanged();
            })
          }
        >
          {t('redis.pushRight')}
        </Button>
      </div>
    </div>
  );
}

function setMembers(detail: KeyDetail): string[] {
  const v = detail.value as Record<string, unknown>;
  const members = v?.members ?? v?.items;
  return Array.isArray(members) ? (members as string[]) : [];
}

function SetEditor({
  connectionId,
  dbIndex,
  detail,
  onChanged,
}: {
  connectionId: string;
  dbIndex: number;
  detail: KeyDetail;
  onChanged: () => void;
}) {
  const { t } = useI18n();
  const members = setMembers(detail);
  const [newMember, setNewMember] = useState('');

  return (
    <div className="space-y-2">
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
              <td className="px-2 py-1.5 font-mono text-fg-secondary">{member}</td>
              <td className="px-2 py-1.5">
                <button
                  type="button"
                  className="rounded p-1 text-red-400 hover:bg-red-500/10"
                  onClick={() =>
                    void invokeSetRemove(connectionId, dbIndex, detail.key, [member]).then(
                      onChanged,
                    )
                  }
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
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
            void invokeSetAdd(connectionId, dbIndex, detail.key, [newMember.trim()]).then(
              () => {
                setNewMember('');
                onChanged();
              },
            )
          }
        >
          <Plus className="h-3 w-3" />
          {t('redis.add')}
        </Button>
      </div>
    </div>
  );
}

function zsetMembers(detail: KeyDetail): { member: string; score: number }[] {
  const v = detail.value as Record<string, { member: string; score: number }[]>;
  return Array.isArray(v?.members) ? v.members : [];
}

function ZsetEditor({
  connectionId,
  dbIndex,
  detail,
  onChanged,
}: {
  connectionId: string;
  dbIndex: number;
  detail: KeyDetail;
  onChanged: () => void;
}) {
  const { t } = useI18n();
  const members = zsetMembers(detail);
  const [newMember, setNewMember] = useState('');
  const [newScore, setNewScore] = useState('0');

  return (
    <div className="space-y-2">
      <table className="w-full border-collapse">
        <thead>
          <tr className="border-b border-edge bg-surface-alt text-left">
            <th className="px-2 py-1.5 font-medium text-fg-muted">{t('redis.score')}</th>
            <th className="px-2 py-1.5 font-medium text-fg-muted">{t('redis.member')}</th>
            <th className="w-12 px-2 py-1.5" />
          </tr>
        </thead>
        <tbody>
          {members.map((item) => (
            <tr key={item.member} className="border-b border-edge">
              <td className="px-2 py-1.5 text-fg-secondary">{item.score}</td>
              <td className="px-2 py-1.5 font-mono text-fg-secondary">{item.member}</td>
              <td className="px-2 py-1.5">
                <button
                  type="button"
                  className="rounded p-1 text-red-400 hover:bg-red-500/10"
                  onClick={() =>
                    void invokeZsetRemove(connectionId, dbIndex, detail.key, [
                      item.member,
                    ]).then(onChanged)
                  }
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
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
            void invokeZsetAdd(connectionId, dbIndex, detail.key, [
              { member: newMember.trim(), score: parseFloat(newScore) || 0 },
            ]).then(() => {
              setNewMember('');
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

export async function invokeCreateKey(
  connectionId: string,
  dbIndex: number,
  key: string,
  keyType: string,
  initialValue: string,
  invoke: PluginInvokeFn = pluginInvoke,
) {
  switch (keyType) {
    case 'string':
      await invokeSetString(connectionId, dbIndex, key, initialValue, invoke);
      break;
    case 'hash':
      await invokeHashSet(connectionId, dbIndex, key, 'field', initialValue || '', invoke);
      break;
    case 'list':
      await invokeListPush(
        connectionId,
        dbIndex,
        key,
        'right',
        [initialValue || ''],
        invoke,
      );
      break;
    case 'set':
      await invokeSetAdd(connectionId, dbIndex, key, [initialValue || 'member'], invoke);
      break;
    case 'zset':
      await invokeZsetAdd(
        connectionId,
        dbIndex,
        key,
        [{ member: initialValue || 'member', score: 0 }],
        invoke,
      );
      break;
    default:
      throw new Error(`Unsupported key type: ${keyType}`);
  }
}
