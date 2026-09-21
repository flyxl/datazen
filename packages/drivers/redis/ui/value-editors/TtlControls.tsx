import { useState, useCallback } from 'react';
import { Button } from '@datazen/ui';
import { Input } from '@datazen/ui';
import { useI18n } from '@datazen/ui';
import { invokeSetTtl, invokeSetExpireAt, type PluginInvokeFn } from './keyEditorsInvokes';
import { redisCommandInvoke } from '../shared/redisInvoke';
import type { GateWriteFn } from '../shared/useRedisGate';

/**
 * Reusable TTL controls component for Redis key editors.
 * Shows current TTL, allows setting relative seconds, absolute datetime, and persist.
 */
export function TtlControls({
  dbSessionId,
  dbIndex,
  keyName,
  ttl,
  gateWrite,
  onChanged,
  invoke,
}: {
  dbSessionId: string;
  dbIndex: number;
  keyName: string;
  ttl: number;
  gateWrite?: GateWriteFn;
  onChanged: () => void;
  /** Optional override for testing (defaults to redisCommandInvoke). */
  invoke?: PluginInvokeFn;
}) {
  const { t } = useI18n();
  const [ttlInput, setTtlInput] = useState(ttl < 0 ? '' : String(ttl));
  const [expireAtLocal, setExpireAtLocal] = useState(() => {
    if (ttl < 0) return '';
    const d = new Date(Date.now() + ttl * 1000);
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
  });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Resolve caller: prefer injected invoke (for testing), fall back to default IPC.
  const caller = invoke ?? redisCommandInvoke;

  const run = useCallback(
    async (fn: () => Promise<void>) => {
      if (gateWrite && !(await gateWrite('write-op'))) return;
      setBusy(true);
      setError(null);
      try {
        await fn();
        onChanged();
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setBusy(false);
      }
    },
    [onChanged, gateWrite],
  );

  const ttlText = ttl < 0 ? t('redis.noExpiry') : `${ttl} ${t('redis.seconds')}`;

  return (
    <div className="rounded-md border border-edge bg-surface-alt p-2">
      <div className="flex flex-wrap items-center gap-2">
        {/* Current TTL display */}
        <span className="font-medium text-fg-muted">{t('redis.ttl')}:</span>
        <span className="text-fg-secondary">{ttlText}</span>

        {/* Relative TTL input */}
        <div className="flex min-w-[100px] flex-col gap-1">
          <Input
            value={ttlInput}
            onChange={(e) => setTtlInput(e.target.value)}
            placeholder={t('redis.ttlSeconds')}
            className="h-7 text-xs"
            data-testid="redis-ttl-input"
          />
        </div>
        <Button
          variant="secondary"
          className="h-7 px-2 text-xs"
          data-testid="redis-ttl-set"
          disabled={busy}
          onClick={() =>
            void run(async () => {
              const secs = parseInt(ttlInput, 10);
              if (Number.isNaN(secs) || secs < 0) {
                throw new Error(t('redis.ttlSeconds'));
              }
              await invokeSetTtl(dbSessionId, dbIndex, keyName, secs, caller);
            })
          }
        >
          {t('redis.setTtl')}
        </Button>

        {/* Absolute datetime input */}
        <div className="flex min-w-[160px] flex-col gap-1">
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
          data-testid="redis-ttl-expire-at"
          disabled={busy || !expireAtLocal}
          onClick={() =>
            void run(async () => {
              const ms = Date.parse(expireAtLocal);
              if (Number.isNaN(ms)) {
                throw new Error(t('redis.expireAtInvalid'));
              }
              const unix = Math.floor(ms / 1000);
              await invokeSetExpireAt(dbSessionId, dbIndex, keyName, unix, caller);
            })
          }
        >
          {t('redis.setExpireAt')}
        </Button>

        {/* Persist button */}
        <Button
          variant="secondary"
          className="h-7 px-2 text-xs"
          data-testid="redis-ttl-persist"
          disabled={busy}
          onClick={() =>
            void run(async () => {
              await invokeSetTtl(dbSessionId, dbIndex, keyName, -1, caller);
              setTtlInput('');
              setExpireAtLocal('');
            })
          }
        >
          {t('redis.persist')}
        </Button>
      </div>

      {error && <p className="mt-1 text-[10px] text-danger">{error}</p>}
    </div>
  );
}
