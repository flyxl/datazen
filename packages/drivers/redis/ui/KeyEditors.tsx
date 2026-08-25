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
  dbSessionId: string,
  dbIndex: number,
  key: string,
  value: string,
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'set_string', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    key,
    value,
  });
}

export async function invokeHashSet(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  field: string,
  value: string,
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'hash_set', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    key,
    field,
    value,
  });
}

export async function invokeHashDel(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  fields: string[],
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'hash_del', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    key,
    fields,
  });
}

export async function invokeListPush(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  side: 'left' | 'right',
  values: string[],
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'list_push', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    key,
    side,
    values,
  });
}

export async function invokeListSet(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  index: number,
  value: string,
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'list_set', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    key,
    index,
    value,
  });
}

export async function invokeListPop(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  side: 'left' | 'right',
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'list_pop', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    key,
    side,
  });
}

export async function invokeSetAdd(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  members: string[],
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'set_add', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    key,
    members,
  });
}

export async function invokeSetRemove(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  members: string[],
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'set_remove', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    key,
    members,
  });
}

export async function invokeZsetAdd(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  members: { member: string; score: number }[],
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'zset_add', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    key,
    members,
  });
}

export async function invokeZsetRemove(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  members: string[],
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'zset_remove', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    key,
    members,
  });
}

export async function invokeRename(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  newKey: string,
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'rename', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    key,
    newKey: newKey,
  });
}

export async function invokeSetTtl(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  ttlSeconds: number,
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'set_ttl', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    key,
    ttlSeconds: ttlSeconds,
  });
}

export interface KeyDetailEditorProps {
  dbSessionId: string;
  dbIndex: number;
  detail: KeyDetail;
  modules?: string[] | null;
  onRefresh: () => void | Promise<void>;
  onRenamed?: (newKey: string) => void;
}

export function KeyDetailEditor({
  dbSessionId,
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
              await invokeSetTtl(dbSessionId, dbIndex, detail.key, secs);
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
              await invokeSetTtl(dbSessionId, dbIndex, detail.key, -1);
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
              await invokeRename(dbSessionId, dbIndex, detail.key, next);
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
          dbSessionId={dbSessionId}
          dbIndex={dbIndex}
          detail={detail}
          onSaved={() => void onRefresh()}
        />
      )}

      {detail.keyType === 'hash' && (
        <HashEditor
          dbSessionId={dbSessionId}
          dbIndex={dbIndex}
          detail={detail}
          onChanged={() => void onRefresh()}
        />
      )}

      {detail.keyType === 'list' && (
        <ListEditor
          dbSessionId={dbSessionId}
          dbIndex={dbIndex}
          detail={detail}
          onChanged={() => void onRefresh()}
        />
      )}

      {detail.keyType === 'set' && (
        <SetEditor
          dbSessionId={dbSessionId}
          dbIndex={dbIndex}
          detail={detail}
          onChanged={() => void onRefresh()}
        />
      )}

      {detail.keyType === 'zset' && (
        <ZsetEditor
          dbSessionId={dbSessionId}
          dbIndex={dbIndex}
          detail={detail}
          onChanged={() => void onRefresh()}
        />
      )}

      {detail.keyType === 'stream' && (
        <StreamEditor
          key={detail.key}
          dbSessionId={dbSessionId}
          dbIndex={dbIndex}
          redisKey={detail.key}
        />
      )}

      {showJsonEditor && (
        <JsonEditor
          key={detail.key}
          dbSessionId={dbSessionId}
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
  dbSessionId,
  dbIndex,
  detail,
  onSaved,
}: {
  dbSessionId: string;
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
    void invokeSetString(dbSessionId, dbIndex, detail.key, value)
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
  dbSessionId,
  dbIndex,
  detail,
  onChanged,
}: {
  dbSessionId: string;
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
                        dbSessionId,
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
                      void invokeHashDel(dbSessionId, dbIndex, detail.key, [field]).then(
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
              dbSessionId,
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
  dbSessionId,
  dbIndex,
  detail,
  onChanged,
}: {
  dbSessionId: string;
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
            void invokeListPop(dbSessionId, dbIndex, detail.key, 'left').then(onChanged)
          }
        >
          {t('redis.popLeft')}
        </Button>
        <Button
          variant="secondary"
          className="h-7 px-2 text-xs"
          onClick={() =>
            void invokeListPop(dbSessionId, dbIndex, detail.key, 'right').then(onChanged)
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
                        dbSessionId,
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
            void invokeListPush(dbSessionId, dbIndex, detail.key, 'left', [
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
            void invokeListPush(dbSessionId, dbIndex, detail.key, 'right', [
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
  dbSessionId,
  dbIndex,
  detail,
  onChanged,
}: {
  dbSessionId: string;
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
                    void invokeSetRemove(dbSessionId, dbIndex, detail.key, [member]).then(
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
            void invokeSetAdd(dbSessionId, dbIndex, detail.key, [newMember.trim()]).then(
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
  dbSessionId,
  dbIndex,
  detail,
  onChanged,
}: {
  dbSessionId: string;
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
                    void invokeZsetRemove(dbSessionId, dbIndex, detail.key, [
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
            void invokeZsetAdd(dbSessionId, dbIndex, detail.key, [
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
  dbSessionId: string,
  dbIndex: number,
  key: string,
  keyType: string,
  initialValue: string,
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  switch (keyType) {
    case 'string':
      await invokeSetString(dbSessionId, dbIndex, key, initialValue, invoke);
      break;
    case 'hash':
      await invokeHashSet(dbSessionId, dbIndex, key, 'field', initialValue || '', invoke);
      break;
    case 'list':
      await invokeListPush(
        dbSessionId,
        dbIndex,
        key,
        'right',
        [initialValue || ''],
        invoke,
      );
      break;
    case 'set':
      await invokeSetAdd(dbSessionId, dbIndex, key, [initialValue || 'member'], invoke);
      break;
    case 'zset':
      await invokeZsetAdd(
        dbSessionId,
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
        dbSessionId: dbSessionId,
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
