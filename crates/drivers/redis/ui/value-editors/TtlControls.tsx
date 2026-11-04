/**
 * TTL pill (PRD §3.3 badge row / §4 I-table; ruling 8-4 keeps the words
 * 永不过期 via `redis.noExpiry` and `set_ttl` semantics unchanged).
 *
 * State machine, all three elements present:
 * - enter: click the collapsed pill (`data-testid="redis-ttl-value"`,
 *   `data-ttl-open="false"`), editor opens in 相对 TTL mode;
 * - in state: switch between the 3 modes (永不过期 / 相对 / 绝对 EXPIREAT),
 *   each mode runs its own apply through `gateWrite` — a successful apply
 *   calls `onChanged` (parent refetches, `ttl` prop re-syncs the inputs);
 * - exit: close button (`redis-ttl-close`), **Escape anywhere inside the
 *   editor** and **focus leaving the editor container** (BUG-004 — the state
 *   machine must not be enter-only), key switch (parent `key={detail.key}`
 *   unmount), or the parent replacing this row. The two new transitions
 *   mirror the close button: both are ignored while an apply is in flight.
 *
 * Assertions contract: `data-testid` locators, `data-ttl-state` /
 * `data-ttl-open` / `data-ttl-mode` / `data-selected` state, `data-i18n-key`
 * on mode labels. The rendered copy itself is never pinned.
 *
 * The component/export name `TtlControls` and the historical testids
 * (`redis-ttl-input` / `-set` / `-expire-at` / `-persist` / `-error`) are kept
 * so the existing journey rewrites stay minimal (§7-6).
 */
import { useCallback, useEffect, useState } from 'react';
import { Button, Input, useI18n } from '@datazen/ui';
import { invokeSetTtl, invokeSetExpireAt, type PluginInvokeFn } from './keyEditorsInvokes';
import { redisCommandInvoke } from '../shared/redisInvoke';
import type { GateWriteFn } from '../shared/useRedisGate';

type TtlMode = 'relative' | 'absolute' | 'no-expiry';

const MODES: TtlMode[] = ['no-expiry', 'relative', 'absolute'];

function modeI18nKey(mode: TtlMode): string {
  if (mode === 'no-expiry') return 'redis.noExpiry';
  if (mode === 'relative') return 'redis.detail.ttl.modeRelative';
  return 'redis.detail.ttl.modeAbsolute';
}

function defaultExpireAtLocal(ttl: number): string {
  if (ttl < 0) return '';
  const d = new Date(Date.now() + ttl * 1000);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

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
  const [open, setOpen] = useState(false);
  const [mode, setMode] = useState<TtlMode>('relative');
  const [ttlInput, setTtlInput] = useState(ttl < 0 ? '' : String(ttl));
  const [expireAtLocal, setExpireAtLocal] = useState(() => defaultExpireAtLocal(ttl));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // A successful apply refetches the key; keep the inputs in step with the
  // fresh `ttl` so the next apply starts from server truth, not a stale draft.
  useEffect(() => {
    setTtlInput(ttl < 0 ? '' : String(ttl));
    setExpireAtLocal(defaultExpireAtLocal(ttl));
  }, [ttl]);

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

  if (!open) {
    // Collapsed pill: `data-ttl-state` is the locale-independent contract.
    return (
      <button
        type="button"
        className="rounded-full border border-edge bg-surface-alt px-2 py-0.5 text-fg-secondary hover:border-edge-hi hover:text-fg"
        data-testid="redis-ttl-value"
        data-ttl-state={ttl < 0 ? 'no-expiry' : 'seconds'}
        data-ttl-open="false"
        data-i18n-key={ttl < 0 ? 'redis.noExpiry' : undefined}
        onClick={() => {
          // Enter transition always starts from 相对 TTL (the documented default).
          setMode('relative');
          setOpen(true);
        }}
      >
        {ttlText}
      </button>
    );
  }

  return (
    <div
      className="flex w-full flex-wrap items-center gap-2 rounded-md border border-edge bg-surface-alt p-2"
      // BUG-004: the two missing exit transitions, mirroring `redis-ttl-close`
      // (disabled while `busy`): Escape bubbling from anywhere inside, and
      // focus moving out of the container (`relatedTarget` inside ⇒ stay open).
      onKeyDown={(e) => {
        if (e.key === 'Escape' && !busy) setOpen(false);
      }}
      onBlur={(e) => {
        if (!busy && !e.currentTarget.contains(e.relatedTarget as Node | null)) {
          setOpen(false);
        }
      }}
    >
      <div className="flex items-center gap-2">
        <span
          className="text-fg-secondary"
          data-testid="redis-ttl-value"
          data-ttl-state={ttl < 0 ? 'no-expiry' : 'seconds'}
          data-ttl-open="true"
        >
          {ttlText}
        </span>
        {/* 3-state mode switch */}
        {MODES.map((m) => (
          <button
            key={m}
            type="button"
            className="rounded-md border px-1.5 py-0.5 text-xs disabled:opacity-50"
            data-testid={`redis-ttl-mode-${m}`}
            data-ttl-mode={m}
            data-i18n-key={modeI18nKey(m)}
            data-selected={mode === m ? 'true' : 'false'}
            aria-pressed={mode === m}
            disabled={busy}
            onClick={() => setMode(m)}
          >
            {t(modeI18nKey(m))}
          </button>
        ))}
        <button
          type="button"
          className="rounded p-1 text-fg-muted hover:bg-surface-raised hover:text-fg disabled:opacity-50"
          data-testid="redis-ttl-close"
          disabled={busy}
          onClick={() => setOpen(false)}
        >
          ×
        </button>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        {mode === 'relative' && (
          <>
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
          </>
        )}

        {mode === 'absolute' && (
          <>
            <div className="flex min-w-[160px] flex-col gap-1">
              <Input
                type="datetime-local"
                value={expireAtLocal}
                onChange={(e) => setExpireAtLocal(e.target.value)}
                className="h-7 text-xs"
                data-testid="redis-ttl-datetime"
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
          </>
        )}

        {mode === 'no-expiry' && (
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
        )}
      </div>

      {error && (
        <p className="mt-1 text-[10px] text-danger" data-testid="redis-ttl-error">
          {error}
        </p>
      )}
    </div>
  );
}
