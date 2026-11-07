/**
 * Key header row of the detail editor (PRD §3.3 screen B right column,
 * ruling 8-4: mono truncatable key name + refresh split button
 * 1s/5s/10s/30s/关 · copy key · copy insert statement · inline rename ·
 * danger-color delete).
 *
 * State machine (auto-refresh), all three elements present:
 * - enter: pick an interval in the split-button menu (`data-refresh-interval-ms`
 *   leaves `0` only for 关);
 * - in state: a timer calls `onRefresh` every tick;
 * - exit: choosing 关, switching keys (unmount) — or a refused refresh
 *   (`onRefresh` resolving `false`, i.e. the I-1 draft guard answered
 *   「继续编辑」) turns the timer off instead of nagging every tick.
 *
 * Rename is inline (input replaces the name span); confirm/cancel via
 * buttons or Enter/Escape. Delete goes through the bound confirm dialog —
 * both write paths resolve `false` when the caller's guard refuses, which
 * keeps the editor mounted.
 *
 * Assertions contract: `data-testid` locators + `data-*` state + `data-i18n-key`
 * on translated labels; the key name and clipboard payloads are server data.
 */
import { useEffect, useRef, useState } from 'react';
import { Check, ChevronDown, Copy, Pencil, RefreshCw, Trash2, X } from 'lucide-react';
import { Button, Input, useI18n, useCopyFeedback } from '@datazen/ui';
import { useBoundConfirmDialog } from '@datazen/driver-sdk';

/** Refresh intervals in ms; `0` is the 关 (off) state. */
export const REFRESH_INTERVALS = [1000, 5000, 10000, 30000, 0] as const;
export type RefreshIntervalMs = (typeof REFRESH_INTERVALS)[number];

/** How long the `data-copied` marker stays on a copy button before reverting. */
const COPIED_FEEDBACK_MS = 1200;

export interface KeyHeaderRowProps {
  keyName: string;
  /** Refresh once; `false` means an I-1 guard refusal (E-5 wires the guard). */
  onRefresh: () => boolean | Promise<boolean>;
  /** Apply the inline rename; `false` means refused or failed (keep input open). */
  onRename: (newName: string) => boolean | Promise<boolean>;
  /** Confirm + delete; `false` means refused or cancelled. */
  onDelete: () => boolean | Promise<boolean>;
  /** Statement from `buildRedisInsertStatement`; `null` hides the action. */
  insertStatement: string | null;
}

function intervalLabel(t: ReturnType<typeof useI18n>['t'], ms: number): string {
  return ms === 0
    ? t('redis.detail.refresh.off')
    : t('redis.detail.refresh.interval', { n: ms / 1000 });
}

function intervalI18nKey(ms: number): string {
  return ms === 0 ? 'redis.detail.refresh.off' : 'redis.detail.refresh.interval';
}

export function KeyHeaderRow({
  keyName,
  onRefresh,
  onRename,
  onDelete,
  insertStatement,
}: KeyHeaderRowProps) {
  const { t } = useI18n();
  const [confirmDelete, confirmDeleteDialog] = useBoundConfirmDialog();
  const [intervalMs, setIntervalMs] = useState<RefreshIntervalMs>(0);
  const [menuOpen, setMenuOpen] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const [draftName, setDraftName] = useState(keyName);
  const [busy, setBusy] = useState(false);
  const [copiedKind, setCopiedKind] = useState<'key' | 'insert' | null>(null);
  const { copied, copy } = useCopyFeedback(COPIED_FEEDBACK_MS);

  // Latest callbacks for the timer without re-arming it on every render.
  const refreshRef = useRef(onRefresh);
  refreshRef.current = onRefresh;

  useEffect(() => {
    if (intervalMs === 0) return;
    const id = setInterval(() => {
      void (async () => {
        const ok = await refreshRef.current();
        // Guard refusal (继续编辑) is an exit transition: stop nagging.
        if (!ok) setIntervalMs(0);
      })();
    }, intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);

  // Key switch remounts this row (parent `key={detail.key}`), but a rename that
  // stays mounted still needs the input to follow the fresh name.
  useEffect(() => {
    setDraftName(keyName);
    setRenaming(false);
  }, [keyName]);

  /**
   * `copied` is the shared, request-bound flag and `copiedKind` names which of
   * the two copy buttons it belongs to; gating on both replaces the old
   * "only clear if still the same kind" timer callback — a stale write can no
   * longer clear a newer confirmation because the hook binds the rollback to
   * the request that started it.
   */
  const copiedButton = copied ? copiedKind : null;

  const runCopy = (kind: 'key' | 'insert', text: string) => {
    copy(text);
    setCopiedKind(kind);
  };

  const runRefresh = async () => {
    setBusy(true);
    try {
      await onRefresh();
    } finally {
      setBusy(false);
    }
  };

  const runDelete = async () => {
    setBusy(true);
    try {
      await onDelete();
    } finally {
      setBusy(false);
    }
  };

  const commitRename = async () => {
    const next = draftName.trim();
    if (!next || next === keyName) return;
    setBusy(true);
    try {
      const ok = await onRename(next);
      if (ok) setRenaming(false);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div
      className="flex flex-wrap items-center gap-1.5"
      data-testid="redis-key-header"
      data-refresh-interval-ms={intervalMs}
      data-renaming={renaming ? 'true' : 'false'}
    >
      {renaming ? (
        <Input
          value={draftName}
          onChange={(e) => setDraftName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') void commitRename();
            if (e.key === 'Escape') {
              setDraftName(keyName);
              setRenaming(false);
            }
          }}
          className="h-7 min-w-[140px] flex-1 font-mono text-xs"
          data-testid="redis-header-rename-input"
          data-i18n-key="redis.detail.header.renameInput"
          aria-label={t('redis.detail.header.renameInput')}
          autoFocus
        />
      ) : (
        <span
          className="min-w-0 flex-1 truncate font-mono text-sm text-fg"
          title={keyName}
          data-testid="redis-header-key-name"
        >
          {keyName}
        </span>
      )}

      {/* Refresh split button */}
      <span className="flex items-center rounded-md border border-edge">
        <button
          type="button"
          disabled={busy}
          title={t('redis.refresh')}
          aria-label={t('redis.refresh')}
          className="rounded-l-md p-1.5 text-fg-muted hover:bg-surface-raised hover:text-fg disabled:opacity-50"
          data-testid="redis-header-refresh"
          data-i18n-key="redis.refresh"
          onClick={() => void runRefresh()}
        >
          <RefreshCw className="h-3.5 w-3.5" />
        </button>
        <button
          type="button"
          title={t('redis.detail.header.autoRefresh')}
          aria-label={t('redis.detail.header.autoRefresh')}
          className="rounded-r-md border-l border-edge p-1.5 text-fg-muted hover:bg-surface-raised hover:text-fg"
          data-testid="redis-header-refresh-menu"
          data-i18n-key="redis.detail.header.autoRefresh"
          data-menu-open={menuOpen ? 'true' : 'false'}
          onClick={() => setMenuOpen((open) => !open)}
        >
          <ChevronDown className="h-3.5 w-3.5" />
        </button>
        {menuOpen && (
          <span
            className="z-20 flex flex-col rounded-md border border-edge bg-surface-alt py-0.5 shadow-md"
            data-testid="redis-refresh-menu"
            role="menu"
          >
            {REFRESH_INTERVALS.map((ms) => (
              <button
                key={ms}
                type="button"
                role="menuitem"
                className="px-2 py-1 text-left text-xs text-fg-secondary hover:bg-surface-raised disabled:opacity-50"
                data-testid={`redis-refresh-interval-${ms}`}
                data-i18n-key={intervalI18nKey(ms)}
                data-selected={intervalMs === ms ? 'true' : 'false'}
                disabled={busy}
                onClick={() => {
                  setIntervalMs(ms);
                  setMenuOpen(false);
                }}
              >
                {intervalLabel(t, ms)}
              </button>
            ))}
          </span>
        )}
      </span>

      {/* Copy key / copy insert statement */}
      <button
        type="button"
        title={t('redis.detail.header.copyKey')}
        aria-label={t('redis.detail.header.copyKey')}
        className="rounded p-1.5 text-fg-muted hover:bg-surface-raised hover:text-fg"
        data-testid="redis-header-copy-key"
        data-i18n-key="redis.detail.header.copyKey"
        data-copied={copiedButton === 'key' ? 'true' : 'false'}
        onClick={() => runCopy('key', keyName)}
      >
        <Copy className="h-3.5 w-3.5" />
      </button>
      {insertStatement !== null && (
        <button
          type="button"
          title={t('redis.detail.header.copyInsert')}
          aria-label={t('redis.detail.header.copyInsert')}
          className="rounded p-1.5 text-fg-muted hover:bg-surface-raised hover:text-fg"
          data-testid="redis-header-copy-insert"
          data-i18n-key="redis.detail.header.copyInsert"
          data-copied={copiedButton === 'insert' ? 'true' : 'false'}
          onClick={() => runCopy('insert', insertStatement)}
        >
          <Copy className="h-3.5 w-3.5" />
        </button>
      )}

      {/* Inline rename */}
      {renaming ? (
        <>
          <button
            type="button"
            disabled={busy || !draftName.trim() || draftName === keyName}
            title={t('common.confirm')}
            aria-label={t('common.confirm')}
            className="rounded p-1.5 text-fg-muted hover:bg-surface-raised hover:text-accent disabled:opacity-40"
            data-testid="redis-header-rename-confirm"
            onClick={() => void commitRename()}
          >
            <Check className="h-3.5 w-3.5" />
          </button>
          <button
            type="button"
            title={t('common.cancel')}
            aria-label={t('common.cancel')}
            className="rounded p-1.5 text-fg-muted hover:bg-surface-raised hover:text-fg"
            data-testid="redis-header-rename-cancel"
            onClick={() => {
              setDraftName(keyName);
              setRenaming(false);
            }}
          >
            <X className="h-3.5 w-3.5" />
          </button>
        </>
      ) : (
        <button
          type="button"
          disabled={busy}
          title={t('redis.renameKey')}
          aria-label={t('redis.renameKey')}
          className="rounded p-1.5 text-fg-muted hover:bg-surface-raised hover:text-fg disabled:opacity-50"
          data-testid="redis-header-rename"
          data-i18n-key="redis.renameKey"
          onClick={() => setRenaming(true)}
        >
          <Pencil className="h-3.5 w-3.5" />
        </button>
      )}

      {/* Danger-color delete */}
      <Button
        variant="ghost"
        size="sm"
        disabled={busy}
        title={t('redis.delete')}
        aria-label={t('redis.delete')}
        className="p-1.5 text-danger hover:bg-danger/10 hover:text-danger"
        data-testid="redis-header-delete"
        data-i18n-key="redis.delete"
        onClick={() => {
          void (async () => {
            const ok = await confirmDelete({
              title: t('redis.delete'),
              message: t('redis.deleteKeyConfirm', { key: keyName }),
              confirmLabel: t('common.confirm'),
              kind: 'warning',
            });
            if (!ok) return;
            await runDelete();
          })();
        }}
      >
        <Trash2 className="h-3.5 w-3.5" />
      </Button>
      {confirmDeleteDialog}
    </div>
  );
}
