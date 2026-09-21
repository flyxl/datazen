import { useCallback, useEffect, useState } from 'react';
import { Button, Input } from '@datazen/ui';
import { useI18n } from '../../../../src/hooks/useI18n';
import { cn } from '../../../../src/lib/cn';
import type { KeyDetail, ValueFrame } from './types';
import { hasRedisJson, isJsonKeyType, looksLikeJsonModuleDetail } from './hasRedisJson';
import { JsonEditor } from './JsonEditor';
import { StreamEditor } from './StreamEditor';
import {
  initialStringEditorValue,
  looksLikeJsonText,
  tryDecompressString,
  unwrapStringKeyValue,
  valueLooksCompressed,
  type DecompressResult,
} from './stringKeyValue';
import { JsonModeBar } from './JsonModeBar';
import { formatJson, JSON_TEXT_MODES, type JsonDisplayMode, type JsonTextMode } from './jsonModes';
import { invokeRename, invokeSetString } from './keyEditorsInvokes';
import { invokeGetKeyRaw } from './redisInvoke';
import { ValueViewer } from './ValueViewer';
import { useRedisGate, type GateWriteFn } from './useRedisGate';
import { formatSize } from './formatSize';
import { HashEditor } from './HashEditor';
import { ListEditor } from './ListEditor';
import { SetEditor } from './SetEditor';
import { ZsetEditor } from './ZsetEditor';
import { TtlControls } from './TtlControls';

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
  const { gateWrite, gateDialog } = useRedisGate();
  const [renameInput, setRenameInput] = useState(detail.key);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [frame, setFrame] = useState<ValueFrame | null>(null);

  // Fetch ValueFrame on mount and when key changes.
  useEffect(() => {
    let cancelled = false;
    void invokeGetKeyRaw(dbSessionId, dbIndex, detail.key).then(
      (f) => {
        if (!cancelled) setFrame(f);
      },
      () => {
        // Silently ignore — frame is optional enrichment.
      },
    );
    return () => {
      cancelled = true;
    };
  }, [dbSessionId, dbIndex, detail.key]);

  const run = useCallback(
    async (fn: () => Promise<void>) => {
      if (!(await gateWrite('write-op'))) return;
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
    [onRefresh, gateWrite],
  );

  const showJsonEditor =
    isJsonKeyType(detail.keyType) ||
    (modules !== null && hasRedisJson(modules) && looksLikeJsonModuleDetail(detail));

  return (
    <div className="space-y-3 text-xs">
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-medium text-fg-muted">{t('redis.type')}:</span>
        <span className="rounded bg-accent/10 px-1.5 py-0.5 text-accent">{detail.keyType}</span>
        {frame && (
          <>
            {frame.memBytes != null && (
              <span className="rounded bg-surface-alt px-1.5 py-0.5 text-fg-muted">
                {formatSize(frame.memBytes)}
              </span>
            )}
            {frame.truncated && (
              <span className="rounded bg-warning/10 px-1.5 py-0.5 text-warning">
                {'truncated'}
              </span>
            )}
          </>
        )}
      </div>

      <TtlControls
        dbSessionId={dbSessionId}
        dbIndex={dbIndex}
        keyName={detail.key}
        ttl={detail.ttl}
        gateWrite={gateWrite}
        onChanged={() => void onRefresh()}
      />

      <div className="flex flex-wrap items-end gap-2 rounded-md border border-edge bg-surface-alt p-2">
        <div className="flex min-w-[120px] flex-1 flex-col gap-1">
          <label className="text-fg-muted">{t('redis.name')}</label>
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
          frame={frame}
          gateWrite={gateWrite}
          onSaved={() => void onRefresh()}
        />
      )}
      {detail.keyType === 'hash' && (
        <HashEditor
          dbSessionId={dbSessionId}
          dbIndex={dbIndex}
          detail={detail}
          gateWrite={gateWrite}
          onChanged={() => void onRefresh()}
        />
      )}
      {detail.keyType === 'list' && (
        <ListEditor
          dbSessionId={dbSessionId}
          dbIndex={dbIndex}
          detail={detail}
          gateWrite={gateWrite}
          onChanged={() => void onRefresh()}
        />
      )}
      {detail.keyType === 'set' && (
        <SetEditor
          dbSessionId={dbSessionId}
          dbIndex={dbIndex}
          detail={detail}
          gateWrite={gateWrite}
          onChanged={() => void onRefresh()}
        />
      )}
      {detail.keyType === 'zset' && (
        <ZsetEditor
          dbSessionId={dbSessionId}
          dbIndex={dbIndex}
          detail={detail}
          gateWrite={gateWrite}
          onChanged={() => void onRefresh()}
        />
      )}
      {showJsonEditor && (
        <JsonEditor
          dbSessionId={dbSessionId}
          dbIndex={dbIndex}
          redisKey={detail.key}
          gateWrite={gateWrite}
        />
      )}
      {detail.keyType === 'stream' && (
        <StreamEditor
          dbSessionId={dbSessionId}
          dbIndex={dbIndex}
          redisKey={detail.key}
          gateWrite={gateWrite}
        />
      )}
      {gateDialog}
    </div>
  );
}

function StringEditor({
  dbSessionId,
  dbIndex,
  detail,
  frame,
  gateWrite,
  onSaved,
}: {
  dbSessionId: string;
  dbIndex: number;
  detail: KeyDetail;
  frame: ValueFrame | null;
  gateWrite?: GateWriteFn;
  onSaved: () => void;
}) {
  const { t } = useI18n();
  const [mode, setMode] = useState<'view' | 'edit'>('view');
  const [value, setValue] = useState(() => initialStringEditorValue(detail.value));
  const [saving, setSaving] = useState(false);
  const [jsonError, setJsonError] = useState<string | null>(null);
  const [jsonDisplay, setJsonDisplay] = useState<JsonTextMode>('pretty');
  const [jsonDirty, setJsonDirty] = useState(false);
  const [keepTtl, setKeepTtl] = useState(detail.ttl >= 0);
  const [decomp, setDecomp] = useState<DecompressResult | null>(null);
  const [decompBusy, setDecompBusy] = useState(false);
  const [decompError, setDecompError] = useState<string | null>(null);
  const jsonMode = looksLikeJsonText(value);
  const rawOriginal = unwrapStringKeyValue(detail.value);
  const maybeCompressed = valueLooksCompressed(unwrapRaw(detail.value));

  const selectJsonMode = (next: JsonDisplayMode) => {
    if (next === 'tree') return;
    setJsonDisplay(next);
    setJsonError(null);
    setValue((prev) => (next === 'raw' && !jsonDirty ? rawOriginal : formatJson(prev, next)));
  };

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
    void (async () => {
      if (gateWrite && !(await gateWrite('write-op'))) {
        setSaving(false);
        return;
      }
      await invokeSetString(dbSessionId, dbIndex, detail.key, value, keepTtl)
        .then(onSaved)
        .finally(() => setSaving(false));
    })();
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
      <div className="flex items-center gap-1" data-testid="redis-string-mode-toggle">
        <button
          type="button"
          className={cn(
            'rounded px-2 py-0.5 text-[11px] transition-colors',
            mode === 'view'
              ? 'bg-accent/15 text-accent'
              : 'text-fg-secondary hover:bg-surface-raised',
          )}
          data-testid="redis-string-view"
          onClick={() => setMode('view')}
        >
          {t('redis.stringModeView')}
        </button>
        <button
          type="button"
          className={cn(
            'rounded px-2 py-0.5 text-[11px] transition-colors',
            mode === 'edit'
              ? 'bg-accent/15 text-accent'
              : 'text-fg-secondary hover:bg-surface-raised',
          )}
          data-testid="redis-string-edit"
          onClick={() => setMode('edit')}
        >
          {t('redis.stringModeEdit')}
        </button>
      </div>

      {mode === 'view' ? (
        <ValueViewer dbSessionId={dbSessionId} frame={frame} />
      ) : (
        <>
          <textarea
            value={value}
            onChange={(e) => {
              setValue(e.target.value);
              setJsonDirty(true);
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
              <JsonModeBar modes={JSON_TEXT_MODES} active={jsonDisplay} onSelect={selectJsonMode} />
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
        </>
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
