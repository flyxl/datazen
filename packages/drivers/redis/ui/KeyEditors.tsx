import { useCallback, useState } from 'react';
import { Loader2, Plus, Trash2 } from 'lucide-react';
import { Button } from '../../../../src/components/ui/Button';
import { Input } from '../../../../src/components/ui/Input';
import { useI18n } from '../../../../src/hooks/useI18n';
import { redisCommandInvoke, type RedisInvokeFn } from './redisInvoke';
import type { KeyDetail } from '../../../../src/types';
import { hasRedisJson, isJsonKeyType, looksLikeJsonModuleDetail } from './hasRedisJson';
import { JsonEditor } from './JsonEditor';
import { StreamEditor } from './StreamEditor';
import {
  initialStringEditorValue,
  looksLikeJsonText,
  tryPrettyJson,
} from './stringKeyValue';

export type PluginInvokeFn = RedisInvokeFn;

export async function invokeSetString(
  connectionId: string,
  dbIndex: number,
  key: string,
  value: string,
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'set_string', {
    connectionId: connectionId,
    dbIndex: dbIndex,
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
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'hash_set', {
    connectionId: connectionId,
    dbIndex: dbIndex,
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
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'hash_del', {
    connectionId: connectionId,
    dbIndex: dbIndex,
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
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'list_push', {
    connectionId: connectionId,
    dbIndex: dbIndex,
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
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'list_set', {
    connectionId: connectionId,
    dbIndex: dbIndex,
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
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'list_pop', {
    connectionId: connectionId,
    dbIndex: dbIndex,
    key,
    side,
  });
}

export async function invokeSetAdd(
  connectionId: string,
  dbIndex: number,
  key: string,
  members: string[],
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'set_add', {
    connectionId: connectionId,
    dbIndex: dbIndex,
    key,
    members,
  });
}

export async function invokeSetRemove(
  connectionId: string,
  dbIndex: number,
  key: string,
  members: string[],
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'set_remove', {
    connectionId: connectionId,
    dbIndex: dbIndex,
    key,
    members,
  });
}

export async function invokeZsetAdd(
  connectionId: string,
  dbIndex: number,
  key: string,
  members: { member: string; score: number }[],
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'zset_add', {
    connectionId: connectionId,
    dbIndex: dbIndex,
    key,
    members,
  });
}

export async function invokeZsetRemove(
  connectionId: string,
  dbIndex: number,
  key: string,
  members: string[],
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'zset_remove', {
    connectionId: connectionId,
    dbIndex: dbIndex,
    key,
    members,
  });
}

export async function invokeRename(
  connectionId: string,
  dbIndex: number,
  key: string,
  newKey: string,
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'rename', {
    connectionId: connectionId,
    dbIndex: dbIndex,
    key,
    newKey: newKey,
  });
}

export async function invokeSetTtl(
  connectionId: string,
  dbIndex: number,
  key: string,
  ttlSeconds: number,
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'set_ttl', {
    connectionId: connectionId,
    dbIndex: dbIndex,
    key,
    ttlSeconds: ttlSeconds,
  });
}

export interface KeyDetailEditorProps {
  connectionId: string;
  dbIndex: number;
  detail: KeyDetail;
  modules?: string[] | null;
  onRefresh: () => void | Promise<void>;
  onRenamed?: (newKey: string) => void;
}

export function KeyDetailEditor({
  connectionId,
  dbIndex,
  detail,
  modules = null,
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

  const showJsonEditor =
    isJsonKeyType(detail.keyType) ||
    (modules !== null && hasRedisJson(modules) && looksLikeJsonModuleDetail(detail));

  return (
    <div className="space-y-3 text-xs">
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-medium text-fg-muted">{t('redis.type')}:</span>
        <span className="rounded bg-accent/10 px-1.5 py-0.5 text-accent">
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
        <div className="rounded-md border border-danger/20 bg-danger/10 px-2 py-1.5 text-danger">
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
        <StreamEditor
          key={detail.key}
          connectionId={connectionId}
          dbIndex={dbIndex}
          redisKey={detail.key}
        />
      )}

      {showJsonEditor && (
        <JsonEditor
          key={detail.key}
          connectionId={connectionId}
          dbIndex={dbIndex}
          redisKey={detail.key}
        />
      )}

      {!['string', 'hash', 'list', 'set', 'zset', 'stream'].includes(detail.keyType) &&
        !showJsonEditor && (
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
  const [value, setValue] = useState(() => initialStringEditorValue(detail.value));
  const [saving, setSaving] = useState(false);
  const [jsonError, setJsonError] = useState<string | null>(null);
  const jsonMode = looksLikeJsonText(value);

  const save = () => {
    if (jsonMode) {
      try {
        JSON.parse(value);
      } catch {
        setJsonError(t('redis.invalidJson'));
        return;
      }
      setJsonError(null);
    }
    setSaving(true);
    void invokeSetString(connectionId, dbIndex, detail.key, value)
      .then(onSaved)
      .finally(() => setSaving(false));
  };

  return (
    <div className="space-y-2">
      <textarea
        value={value}
        onChange={(e) => {
          setValue(e.target.value);
          setJsonError(null);
        }}
        className="min-h-[160px] w-full rounded-md border border-edge bg-surface-alt p-3 font-mono text-xs text-fg-secondary"
        spellCheck={false}
      />
      {jsonError && (
        <div className="rounded-md border border-danger/20 bg-danger/10 px-2 py-1.5 text-danger">
          {jsonError}
        </div>
      )}
      <div className="flex items-center justify-end gap-2">
        {jsonMode && (
          <Button
            variant="secondary"
            className="h-7 px-2 text-xs"
            disabled={saving}
            onClick={() => {
              const pretty = tryPrettyJson(value);
              if (!pretty) {
                setJsonError(t('redis.invalidJson'));
                return;
              }
              setJsonError(null);
              setValue(pretty);
            }}
          >
            {t('redis.formatJson')}
          </Button>
        )}
        <Button
          variant="primary"
          className="h-7 px-2 text-xs"
          disabled={saving}
          onClick={save}
        >
          {t('common.save')}
        </Button>
      </div>
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
                    className="rounded p-1 text-danger hover:bg-danger/10"
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
                    className="text-xs text-accent hover:underline"
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
                  className="rounded p-1 text-danger hover:bg-danger/10"
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
                  className="rounded p-1 text-danger hover:bg-danger/10"
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
  invoke: PluginInvokeFn = redisCommandInvoke,
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
    case 'ReJSON': {
      const trimmed = initialValue.trim();
      let jsonValue = '{}';
      if (trimmed) {
        try {
          JSON.parse(trimmed);
          jsonValue = trimmed;
        } catch {
          jsonValue = JSON.stringify(trimmed);
        }
      }
      await invoke('redis', 'json_set', {
        connectionId: connectionId,
        dbIndex: dbIndex,
        key,
        path: '$',
        value: jsonValue,
      });
      break;
    }
    default:
      throw new Error(`Unsupported key type: ${keyType}`);
  }
}
