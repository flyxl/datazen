/**
 * Bottom status bar — the `statusBar` KV slot, full version (PRD §3.4).
 *
 * The PRD's KV shape is: `db0 · 52 keys · loaded 52 · 扫描游标 0 · 选中 3 ·
 * 最后写操作 SET app:cache:session:1 (12ms)`. This file renders exactly that,
 * plus the two key facts the slot already carried (type / size / TTL) and the
 * dirty badge.
 *
 * Where each fact comes from — the split matters, because only one of the two
 * channels exists per fact:
 *
 * | field | source |
 * |---|---|
 * | `db0` | slot props (`database` / `dbIndex`), never a command |
 * | `52 keys` | `db_sizes`, via the shared per-panel read (`dbKeyCounts`) |
 * | `loaded 52` | F-1 `getLoadedCount()` |
 * | `扫描游标 0` | F-1 `getScanCursor()`, **paired** with `loaded` (see below) |
 * | `选中 3` | F-1 `getSelectionCount()` — the count only (F-1 hard rule) |
 * | last write | F-1 `getLastWriteCommand()` / `getLastWriteDurationMs()` |
 * | type / size / TTL | `key_object_info` for the selected key |
 *
 * The pairing the F-1 docblock insists on: `getScanCursor() === '0'` **on its
 * own does not mean the scan finished** — a fresh panel reports `'0'` too. So
 * `data-scan-state` is derived from cursor *and* loaded count together
 * (`scanStateOf` below), and the cluster reports `idle` rather than claiming
 * completion when nothing has been loaded.
 *
 * Every field degrades to *not rendered* rather than to a placeholder number:
 * an unknown key count, an unseen write, a zero selection and an idle scan all
 * leave the bar shorter, never filled with `0`/`—` (§3.4 「不渲染」).
 */
import type { ReactNode } from 'react';
import { Fragment } from 'react';
import type { KvStatusBarProps } from '@datazen/driver-sdk';
import { Badge, useI18n } from '@datazen/ui';
import { formatCompactCount, formatSize } from '../shared/formatSize';
import { useDbKeyCount } from './dbKeyCounts';
import { attributeViewState, describeTtl, formatDurationMs } from './keyObjectInfo';
import {
  useKvDirty,
  useKvLastWrite,
  useKvLoadedCount,
  useKvScanCursor,
  useKvSelectedKey,
  useKvSelectionCount,
} from './useKvSelection';
import { useKeyObjectInfo } from './useKeyObjectInfo';

/**
 * The scan line's state, derived from the relay's cursor **and** its loaded
 * count — never from the cursor alone (F-1's explicit warning).
 *
 * - `idle` — nothing has been loaded and the cursor is at `'0'`: the fresh-panel
 *   snapshot. Also the state a never-scanned `db` sits in.
 * - `done` — something was loaded and the cursor wrapped (`'0'`).
 * - `stopped` — something was loaded and the cursor is still non-zero: the scan
 *   ended early, which under PRD I-2/I-4 means a partial or budget-capped tree.
 */
export type ScanLineState = 'idle' | 'done' | 'stopped';

export function scanStateOf(cursor: string, loadedCount: number): ScanLineState {
  if (loadedCount <= 0) return 'idle';
  return cursor === '0' ? 'done' : 'stopped';
}

export function RedisKvStatusBar({ dbSessionId, dbIndex, database, state }: KvStatusBarProps) {
  const { t } = useI18n();
  const selectedKey = useKvSelectedKey(state);
  const dirty = useKvDirty(state);
  const loadedCount = useKvLoadedCount(state);
  const scanCursor = useKvScanCursor(state);
  const selectionCount = useKvSelectionCount(state);
  const lastWrite = useKvLastWrite(state);
  const { count: keysInDb } = useDbKeyCount(state, dbSessionId, dbIndex);
  const { info, loading, failed } = useKeyObjectInfo(state, dbSessionId, dbIndex, selectedKey);

  const dbLabel = database ?? (dbIndex === undefined ? null : `db${dbIndex}`);
  const ttl = info && !info.missing ? describeTtl(info.ttlMs) : null;
  const view = attributeViewState(selectedKey, loading, failed, info);
  const scanState = scanStateOf(scanCursor, loadedCount);

  const parts: Array<{ part: string; node: ReactNode; attrs?: Record<string, string> }> = [
    { part: 'database', node: <span className="font-mono">{dbLabel ?? '—'}</span> },
  ];

  // `52 keys` — omitted, not zeroed, when the count could not be read.
  if (keysInDb !== null) {
    parts.push({
      part: 'keys',
      attrs: { 'data-i18n-key': 'redis.dbSize', 'data-keys': String(keysInDb) },
      node: (
        <span className="font-mono">
          {t('redis.dbSize', { count: formatCompactCount(keysInDb) })}
        </span>
      ),
    });
  }

  // `loaded n` — F-1 scalar. A zero is meaningful here (an empty scan result on
  // a db that really has no keys), so it renders; the *scan* cluster below is
  // what stays silent while nothing has happened.
  parts.push({
    part: 'loaded',
    attrs: { 'data-i18n-key': 'redis.loadedCount' },
    node: (
      <span className="font-mono">
        {t('redis.loadedCount', { count: formatCompactCount(loadedCount) })}
      </span>
    ),
  });

  // `扫描游标 0` — one line, carrying the paired state as a data attribute so a
  // test (or E2E) can tell a completed scan from an untouched panel.
  parts.push({
    part: 'scan-cursor',
    attrs: {
      'data-scan-state': scanState,
      'data-scan-cursor': scanCursor,
      'data-i18n-key':
        scanState === 'stopped'
          ? 'redis.contextBar.status.scanStopped'
          : 'redis.contextBar.status.scanCursor',
    },
    node: (
      <span className="font-mono">
        {scanState === 'stopped'
          ? t('redis.contextBar.status.scanStopped', { cursor: scanCursor })
          : t('redis.contextBar.status.scanCursor', { cursor: scanCursor })}
      </span>
    ),
  });

  // `选中 3` — the count only. F-1 keeps the selected keys themselves in the
  // workbench, so there is nothing else this slot could render. Zero means "no
  // multi-selection", which is not a fact worth a pill on a 24px bar.
  if (selectionCount > 0) {
    parts.push({
      part: 'selection',
      attrs: { 'data-i18n-key': 'redis.contextBar.status.selected' },
      node: (
        <span className="font-mono">
          {t('redis.contextBar.status.selected', { count: String(selectionCount) })}
        </span>
      ),
    });
  }

  // `最后写操作 SET … (12ms)` — command and duration are recorded as one fact
  // (F-1 `recordWrite`). An unknown duration still shows the command: the name
  // is the useful half, and `(nullms)` must never appear.
  if (lastWrite.command !== null) {
    parts.push({
      part: 'last-write',
      attrs: {
        'data-last-write': lastWrite.command,
        'data-last-write-ms':
          lastWrite.durationMs === null ? 'unknown' : String(lastWrite.durationMs),
      },
      node: (
        <span className="max-w-[320px] truncate font-mono" title={lastWrite.command}>
          {lastWrite.durationMs === null
            ? lastWrite.command
            : t('redis.contextBar.status.lastWrite', {
                command: lastWrite.command,
                ms: String(lastWrite.durationMs),
              })}
        </span>
      ),
    });
  }

  if (info && !info.missing) {
    if (info.type) {
      parts.push({ part: 'type', node: <span className="font-mono">{info.type}</span> });
    }
    if (info.memoryBytes !== null) {
      parts.push({
        part: 'size',
        node: <span className="font-mono">{formatSize(info.memoryBytes)}</span>,
      });
    }
    // No-expiry keys are the norm, so the pill stays silent unless there is a
    // real deadline to count down — the sidebar states "No expiry" explicitly.
    // A key the server reported gone (`-2`, redis-kvbar-ui-BUG-004) is silent here
    // too, and the sidebar words that row as *gone*: a bar pill would have to print
    // a duration it was not given, and the one claim both surfaces must avoid is
    // presenting such a key as one that simply never expires.
    if (ttl?.kind === 'remaining') {
      parts.push({
        part: 'ttl',
        node: <span className="font-mono text-warning">{formatDurationMs(ttl.ms)}</span>,
      });
    }
  }

  // The selected key name stays last in the reading order: on a bar this dense
  // it is the longest field, so everything with a fixed width comes first.
  parts.push({
    part: 'selected-key',
    node: (
      <span className="max-w-[280px] truncate font-mono" title={selectedKey ?? undefined}>
        {selectedKey ?? t('redis.contextBar.status.noKey')}
      </span>
    ),
  });

  return (
    <div
      className="flex min-w-0 items-center gap-2"
      data-testid="redis-kv-status-bar"
      data-slot="redis-kv-status-bar"
      data-status-state={view}
      data-selected-key={selectedKey ?? ''}
      data-dirty={dirty ? 'true' : 'false'}
      data-scan-state={scanState}
      data-loaded={loadedCount}
      data-selection-count={selectionCount}
    >
      {parts.map((entry, index) => (
        <Fragment key={entry.part}>
          {index > 0 && (
            <span aria-hidden="true" className="text-fg-muted">
              ·
            </span>
          )}
          <span className="min-w-0 shrink-0" data-part={entry.part} {...entry.attrs}>
            {entry.node}
          </span>
        </Fragment>
      ))}
      {dirty && (
        <Badge
          tone="warning"
          className="shrink-0"
          data-part="dirty"
          data-i18n-key="redis.contextBar.status.unsaved"
        >
          {t('redis.contextBar.status.unsaved')}
        </Badge>
      )}
    </div>
  );
}
