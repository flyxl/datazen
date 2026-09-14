import { useCallback, useState } from 'react';
import { Button } from '@datazen/ui';
import { Input } from '@datazen/ui';
import { useI18n } from '../../../../src/hooks/useI18n';
import type { KeyDetail } from '../../../../src/types';
import { hasRedisJson, isJsonKeyType, looksLikeJsonModuleDetail } from './hasRedisJson';
import { JsonEditor } from './JsonEditor';
import { StreamEditor } from './StreamEditor';
import {
  initialStringEditorValue,
  looksLikeJsonText,
  tryPrettyJson,
  tryDecompressString,
  valueLooksCompressed,
  type DecompressResult,
} from './stringKeyValue';
import {
  invokeRename,
  invokeSetExpireAt,
  invokeSetString,
  invokeSetTtl,
} from './keyEditorsInvokes';
import { HashEditor } from './HashEditor';
import { ListEditor } from './ListEditor';
import { SetEditor } from './SetEditor';
import { ZsetEditor } from './ZsetEditor';

export type { PluginInvokeFn } from './keyEditorsInvokes';
export {
  invokeCreateKey,
  invokeHashDel,
  invokeHashSet,
  invokeListPop,
  invokeListPush,
  invokeListSet,
  invokeRename,
  invokeSetAdd,
  invokeSetExpireAt,
  invokeSetRemove,
  invokeSetString,
  invokeSetTtl,
  invokeZsetAdd,
  invokeZsetRemove,
} from './keyEditorsInvokes';

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
  const [ttlInput, setTtlInput] = useState(detail.ttl < 0 ? '' : String(detail.ttl));
  const [expireAtLocal, setExpireAtLocal] = useState(() => {
    if (detail.ttl < 0) return '';
    const d = new Date(Date.now() + detail.ttl * 1000);
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
  });
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

  const ttlText = detail.ttl < 0 ? t('redis.noExpiry') : `${detail.ttl} ${t('redis.seconds')}`;

  const showJsonEditor =
    isJsonKeyType(detail.keyType) ||
    (modules !== null && hasRedisJson(modules) && looksLikeJsonModuleDetail(detail));

  return (
    <div className="space-y-3 text-xs">
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-medium text-fg-muted">{t('redis.type')}:</span>
        <span className="rounded bg-accent/10 px-1.5 py-0.5 text-accent">{detail.keyType}</span>
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
        <div className="flex min-w-[180px] flex-1 flex-col gap-1">
          <label className="text-fg-muted">{t('redis.expireAt')}</label>
          <Input
            type="datetime-local"
            value={expireAtLocal}
            onChange={(e) => setExpireAtLocal(e.target.value)}
            className="h-7 text-xs"
          />
        </div>
        <Button
          variant="secondary"
          className="h-7 px-2 text-xs"
          disabled={busy || !expireAtLocal}
          onClick={() =>
            void run(async () => {
              const ms = Date.parse(expireAtLocal);
              if (Number.isNaN(ms)) {
                throw new Error(t('redis.expireAtInvalid'));
              }
              const unix = Math.floor(ms / 1000);
              await invokeSetExpireAt(dbSessionId, dbIndex, detail.key, unix);
            })
          }
        >
          {t('redis.setExpireAt')}
        </Button>
        <Button
          variant="secondary"
          className="h-7 px-2 text-xs"
          disabled={busy}
          onClick={() =>
            void run(async () => {
              await invokeSetTtl(dbSessionId, dbIndex, detail.key, -1);
              setTtlInput('');
              setExpireAtLocal('');
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
          disabled={busy || !renameInput.trim() || renameInput === detail.key}
          onClick={() =>
            void run(async () => {
              await invokeRename(dbSessionId, dbIndex, detail.key, renameInput.trim());
              onRenamed?.(renameInput.trim());
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
      {showJsonEditor && (
        <JsonEditor
          dbSessionId={dbSessionId}
          dbIndex={dbIndex}
          redisKey={detail.key}
        />
      )}
      {detail.keyType === 'stream' && (
        <StreamEditor
          dbSessionId={dbSessionId}
          dbIndex={dbIndex}
          redisKey={detail.key}
        />
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
  const [keepTtl, setKeepTtl] = useState(detail.ttl >= 0);
  const [decomp, setDecomp] = useState<DecompressResult | null>(null);
  const [decompBusy, setDecompBusy] = useState(false);
  const [decompError, setDecompError] = useState<string | null>(null);
  const jsonMode = looksLikeJsonText(value);
  const maybeCompressed = valueLooksCompressed(unwrapRaw(detail.value));

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
    void invokeSetString(dbSessionId, dbIndex, detail.key, value, keepTtl)
      .then(onSaved)
      .finally(() => setSaving(false));
  };

  const runDecompress = () => {
    setDecompBusy(true);
    setDecompError(null);
    void tryDecompressString(unwrapRaw(detail.value))
      .then((r) => {
        if (!r) {
          setDecompError(t('redis.decompressFailed'));
          setDecomp(null);
        } else {
          setDecomp(r);
        }
      })
      .catch((e) => {
        setDecompError(e instanceof Error ? e.message : String(e));
        setDecomp(null);
      })
      .finally(() => setDecompBusy(false));
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
      <div className="flex flex-wrap items-center gap-3">
        <label className="flex items-center gap-1.5 text-fg-secondary">
          <input
            type="checkbox"
            checked={keepTtl}
            onChange={(e) => setKeepTtl(e.target.checked)}
            className="rounded border-edge"
          />
          {t('redis.keepTtl')}
        </label>
        <span className="text-fg-muted">{t('redis.keepTtlHint')}</span>
        {jsonMode && (
          <Button
            variant="secondary"
            className="h-7 px-2 text-xs"
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
        {(maybeCompressed || decomp) && (
          <Button
            variant="secondary"
            className="h-7 px-2 text-xs"
            disabled={decompBusy}
            onClick={runDecompress}
          >
            {t('redis.decompressView')}
          </Button>
        )}
        <Button variant="primary" className="h-7 px-2 text-xs" disabled={saving} onClick={save}>
          {t('common.save')}
        </Button>
      </div>
      {decompError && (
        <div className="rounded-md border border-danger/20 bg-danger/10 px-2 py-1.5 text-danger">
          {decompError}
        </div>
      )}
      {decomp && (
        <div className="space-y-1 rounded-md border border-edge bg-surface-alt p-2">
          <div className="text-fg-muted">
            {t('redis.decompressCodec').replace('{codec}', decomp.codec)} · {decomp.bytes} B
          </div>
          <pre className="max-h-64 overflow-auto whitespace-pre-wrap break-all font-mono text-fg-secondary">
            {decomp.text}
          </pre>
        </div>
      )}
    </div>
  );
}

function unwrapRaw(value: unknown): string {
  if (typeof value === 'string') return value;
  if (value && typeof value === 'object' && 'value' in value) {
    const inner = (value as { value: unknown }).value;
    if (typeof inner === 'string') return inner;
  }
  return '';
}
