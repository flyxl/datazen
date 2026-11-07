/**
 * Key-props sidebar — the `keyPropsSidebar` KV slot (PRD §3.4, ruling 8-3 = M).
 *
 * This is the fix for P-3: the host's detail drawer used to be "applicable" for
 * a KV panel while its body was an empty row grid, i.e. a dead button. The
 * drawer now renders these server attributes instead, from the single
 * `key_object_info` pipeline: `TYPE` / `MEMORY USAGE` / `OBJECT ENCODING` /
 * `OBJECT IDLETIME` / `OBJECT FREQ` / `PTTL`, plus the `maxmemory_policy` of the
 * server behind the current `dbSessionId` (`info_filtered`: one server's fact, so
 * scoped to that session — never a session-free one; see the policy effect).
 *
 * Three contract obligations shape the component:
 * 1. **`open === false` ⇒ render `null`** — the host owns the drawer geometry
 *    and deliberately does *not* unmount this slot (keeping the list's scroll
 *    state across toggles is intended), so visibility is the driver's call. The
 *    host wrapper `conn-kv-key-props-sidebar` therefore stays in the DOM either
 *    way; nothing may treat its presence as "drawer open".
 * 2. **Selection is read, never asked for** — the key comes off the host relay
 *    (contract F-2), so this subtree has no callback into the workbench.
 * 3. **The slot owns its own container** (width / border / scroll), like
 *    `DetailPanel` does.
 *
 * A key that expired or was deleted is the command's `missing` success case and
 * renders as a named empty state (§I-11), never as an error.
 */
import { useEffect, useState } from 'react';
import { RefreshCw, X } from 'lucide-react';
import type { KeyPropsSidebarProps } from '@datazen/driver-sdk';
import { Button, useI18n } from '@datazen/ui';
import { formatSize } from '../shared/formatSize';
import {
  attributeViewState,
  describeTtl,
  formatDurationMs,
  formatIdle,
  invokeMaxmemoryPolicy,
} from './keyObjectInfo';
import { useKvSelectedKey } from './useKvSelection';
import { useKeyObjectInfo } from './useKeyObjectInfo';

export function RedisKeyPropsSidebar({
  open,
  onClose,
  dbSessionId,
  dbIndex,
  state,
}: KeyPropsSidebarProps) {
  const { t } = useI18n();
  const selectedKey = useKvSelectedKey(state);
  // Nothing is fetched while the drawer is collapsed: the slot stays mounted, so
  // `open` doubles as the request gate and a closed drawer costs no round trips.
  const { info, loading, failed, reload, attempt } = useKeyObjectInfo(
    state,
    dbSessionId,
    dbIndex,
    open ? selectedKey : null,
    open,
  );
  // The policy value is tagged with the session that produced it, because that is
  // the scope of its data source: `invokeMaxmemoryPolicy(dbSessionId)` reads one
  // server's `INFO`, so the row may only be attributed to the session on screen.
  // Same session ⇒ the previous value deliberately stays visible while a re-read
  // is open (it is not another key's attribute — contrast BUG-001, which is the
  // *key* dimension, the only one the earlier comment on this effect claimed to
  // cover). Different session ⇒ no known fact yet ⇒ empty row until the new
  // server answers, exactly like every other row of this list (redis-kvbar-ui-BUG-005).
  const [policy, setPolicy] = useState<{ session: string; value: string | null } | null>(null);

  // `maxmemory_policy` rides along with the key read: the refresh button is one
  // action, and a policy row that never re-reads would contradict the freq row
  // the same click just refreshed (redis-kvbar-ui-BUG-003). `attempt` is that
  // shared trigger, and it deliberately does *not* invalidate the tag above.
  // `stale` is the write-side half of the same rule: a reply that arrives after
  // its effect was superseded never enters the state at all, so a session which
  // is no longer current cannot even leave a candidate value behind.
  useEffect(() => {
    if (!open) return;
    let stale = false;
    void invokeMaxmemoryPolicy(dbSessionId).then((value) => {
      if (!stale) setPolicy({ session: dbSessionId, value });
    });
    return () => {
      stale = true;
    };
  }, [open, dbSessionId, attempt]);

  if (!open) return null;

  const view = attributeViewState(selectedKey, loading, failed, info);
  const ttl = info ? describeTtl(info.ttlMs) : null;
  // Derived while rendering, so a session swap invalidates this row at once
  // instead of waiting for the effect that runs after that paint.
  const policyValue = policy?.session === dbSessionId ? policy.value : null;

  return (
    <aside
      className="flex h-full w-[260px] shrink-0 flex-col border-l border-edge bg-surface-alt"
      data-testid="redis-kv-key-props-sidebar"
      data-slot="redis-kv-props-sidebar"
      data-props-state={view}
      data-selected-key={selectedKey ?? ''}
    >
      <header className="flex shrink-0 items-center gap-1 border-b border-edge px-3 py-2">
        <span className="min-w-0 flex-1 truncate text-xs font-semibold text-fg-secondary">
          {t('redis.keyProps.title')}
        </span>
        <Button
          variant="ghost"
          size="sm"
          className="h-6 px-1"
          aria-label={t('redis.keyProps.refresh')}
          data-testid="redis-kv-props-refresh"
          disabled={!selectedKey}
          onClick={reload}
        >
          <RefreshCw className="h-3.5 w-3.5" />
        </Button>
        <Button
          variant="ghost"
          size="sm"
          className="h-6 px-1"
          aria-label={t('common.close')}
          data-testid="redis-kv-props-close"
          onClick={onClose}
        >
          <X className="h-3.5 w-3.5" />
        </Button>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-3 py-2">
        <p
          className="mb-2 break-all font-mono text-[11px] text-fg-primary"
          data-testid="redis-kv-props-key"
        >
          {selectedKey ?? t('redis.keyProps.noKey')}
        </p>

        {view === 'no-key' && (
          <p className="text-[11px] text-fg-muted" data-i18n-key="redis.keyProps.noKeyHint">
            {t('redis.keyProps.noKeyHint')}
          </p>
        )}
        {view === 'loading' && (
          <p className="text-[11px] text-fg-muted" data-i18n-key="redis.keyProps.loading">
            {t('redis.keyProps.loading')}
          </p>
        )}
        {view === 'unavailable' && (
          <p className="text-[11px] text-fg-muted" data-i18n-key="redis.keyProps.failed">
            {t('redis.keyProps.failed')}
          </p>
        )}
        {view === 'failed' && (
          <p className="text-[11px] text-danger" data-i18n-key="redis.keyProps.failed">
            {t('redis.keyProps.failed')}
          </p>
        )}
        {view === 'missing' && (
          <p className="text-[11px] text-warning" data-i18n-key="redis.keyProps.missing">
            {t('redis.keyProps.missing')}
          </p>
        )}

        {info && !info.missing && (
          <dl className="space-y-1.5">
            <AttributeRow attr="type" labelKey="redis.keyProps.type" value={info.type} />
            <AttributeRow
              attr="memory"
              labelKey="redis.keyProps.memory"
              value={info.memoryBytes === null ? null : formatSize(info.memoryBytes)}
            />
            {/* `-1` has its own word (`redis.noExpiry`) instead of the generic
                "Unavailable": it is an answer, not a missing measurement. `-2` is a
                *different* answer — the server reported the key gone inside this very
                pipeline (`TYPE` hit, `PTTL` -2) while `missing` stayed false, so
                calling it "no expiry" would state the one thing certainly not true of
                it (redis-kvbar-ui-BUG-004). */}
            <AttributeRow
              attr="ttl"
              labelKey="redis.ttl"
              value={ttl?.kind === 'remaining' ? formatDurationMs(ttl.ms) : null}
              fallbackKey={ttl?.kind === 'missing' ? 'redis.keyProps.missing' : 'redis.noExpiry'}
            />
            <AttributeRow
              attr="encoding"
              labelKey="redis.keyProps.encoding"
              value={info.encoding}
            />
            <AttributeRow
              attr="idle"
              labelKey="redis.keyProps.idle"
              value={info.idleSeconds === null ? null : formatIdle(info.idleSeconds)}
            />
            {/* `freq: null` is the everyday non-LFU server rejecting `OBJECT FREQ`
                — a named empty state (§I-11), never a zero. */}
            <AttributeRow
              attr="freq"
              labelKey="redis.keyProps.freq"
              value={info.freq === null ? null : String(info.freq)}
              fallbackKey="redis.keyProps.freqUnavailable"
            />
            <AttributeRow
              attr="maxmemory-policy"
              labelKey="redis.keyProps.maxmemoryPolicy"
              value={policyValue}
            />
          </dl>
        )}
      </div>
    </aside>
  );
}

/**
 * One label / value line of the attribute list.
 *
 * `data-attr` names the row, `data-i18n-key` names the label and — when the
 * server gave nothing — `data-fallback-key` names the empty state. A test can
 * therefore assert "the encoding row is unavailable" without quoting a single
 * translated string (PRD §7-6).
 */
function AttributeRow({
  attr,
  labelKey,
  value,
  fallbackKey = 'redis.keyProps.unavailable',
}: {
  attr: string;
  labelKey: string;
  value: string | null;
  fallbackKey?: string;
}) {
  const { t } = useI18n();
  return (
    <div className="flex items-baseline justify-between gap-2" data-attr={attr}>
      <dt className="shrink-0 text-[11px] text-fg-muted" data-i18n-key={labelKey}>
        {t(labelKey)}
      </dt>
      <dd
        className="min-w-0 break-all text-right font-mono text-[11px] text-fg-secondary"
        data-value={value ?? ''}
        data-fallback-key={value === null ? fallbackKey : undefined}
      >
        {value ?? t(fallbackKey)}
      </dd>
    </div>
  );
}
