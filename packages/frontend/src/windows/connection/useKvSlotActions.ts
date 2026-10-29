/**
 * Host dispatcher for KV slot actions.
 *
 * A driver's context bar is the only place these controls live, but it may not
 * reach host capabilities directly, so every request it
 * makes arrives here and nowhere else. Three rules shape the implementation:
 *
 * 1. **One dispatcher.** The slot never learns who handled its action and the
 *    host never grows a second entry point, so "what can a context bar ask
 *    for?" has exactly one answer in the tree.
 * 2. **Unwired is not an error.** A request the host cannot carry out is a
 *    no-op plus one developer warning. Throwing would turn a slot that offers
 *    more than this build wires into a crashed toolbar.
 * 3. **The dangerous branch passes the existing write gate before anything
 *    else**: Safe Mode blocks hard, otherwise the shared confirm
 *    dialog asks. That gate is the host's own `useConfirmDialog` +
 *    `settingsStore.safeMode` — the very primitives `@datazen/driver-sdk` binds
 *    for driver UI (`bindConfirmDialog`), so no second confirmation system is
 *    introduced. The driver workbench keeps its typed FLUSHDB confirm because
 *    *it* is the code that can actually run `flush_db`.
 *
 * `request` keeps a stable identity across renders (latest args through a ref),
 * because the host hands it inside the memoised KV slot props bundle: a churning
 * function would re-render every driver slot on every workspace render.
 */
import { useCallback, useEffect, useRef, type ReactNode } from 'react';
import { useI18n } from '../../hooks/useI18n';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { useSettingsStore } from '../../stores/settingsStore';
import { openSettingsWindow } from '../../lib/windowManager';
import type { KvSlotAction } from '@datazen/driver-sdk';

/** Settings window section hosting the per-driver settings contributions. */
const DRIVER_SETTINGS_SECTION = 'extensions';

/**
 * Actions with a real host capability behind them today. Everything else in
 * {@link KvSlotAction} belongs to the drive side of the panel (creating a key,
 * importing/exporting keys, MONITOR, the scan-budget counter) or needs a driver
 * command the host must not name, so it warns instead of guessing.
 *
 * `selectDatabase` joined the list once the capability was found to already
 * exist — `ConnectionPage`'s `handleSelectKvDb` (the navigation tree's and 屏 A's
 * db entry point) was merely never threaded into the slot channel. A capability
 * that already exists is wiring, not a new mechanism, so this is now a real
 * branch rather than a warning.
 */
const WIRED_ACTIONS: readonly KvSlotAction['type'][] = [
  'refresh',
  'openSettings',
  'selectDatabase',
];

export interface UseKvSlotActionsArgs {
  /** Refresh the active panel — the same sink the toolbar's own refresh uses. */
  onRefresh: () => void;
  /**
   * Bind the workspace to `database` — the context bar's db selector.
   *
   * Supplied by the host with `ConnectionPage`'s `handleSelectKvDb`, i.e. the
   * **same** callback the navigation tree and 屏 A's key-space grid call, so
   * there is exactly one "activate the panel for this db / open one" path in the
   * tree. Optional: a host that has not threaded it through degrades to the
   * documented no-op + warning instead of hiding the driver's selector.
   *
   * Only the `database` label travels over the wire; the connection is the one
   * the panel belongs to, which this host already knows.
   */
  onSelectDatabase?: (database: string) => void;
}

export interface KvSlotActionDispatcher {
  /** The `request` prop handed to the context bar. */
  request: (action: KvSlotAction) => void;
  /** Confirm dialog for the dangerous branch; render next to the other dialogs. */
  dialog: ReactNode;
}

/** Copy the dangerous branch shows, by key (never inlined English). */
interface DangerCopy {
  titleKey: string;
  messageKey: string;
  /** Shown instead of the prompt while Safe Mode hard-blocks the action. */
  blockedKey: string;
}

const FLUSH_DB_COPY: DangerCopy = {
  titleKey: 'redis.kvSlot.flushTitle',
  messageKey: 'redis.kvSlot.flushMessage',
  blockedKey: 'redis.kvSlot.flushBlocked',
};

/** Warn exactly once per request, naming the action and what is wired. */
function warnUnwired(action: KvSlotAction): void {
  console.warn(
    `[kv-slot-actions] no host handler for "${action.type}" ` +
      `(wired: ${WIRED_ACTIONS.join(', ')}); ignored. A driver slot may offer it, ` +
      'but nothing will happen until the host wires it.',
  );
}

export function useKvSlotActions({
  onRefresh,
  onSelectDatabase,
}: UseKvSlotActionsArgs): KvSlotActionDispatcher {
  const { t } = useI18n();
  const [confirm, dialog] = useConfirmDialog();

  // Latest `t` / `onRefresh` / `onSelectDatabase` without folding them into
  // `request`'s identity: the callback rides inside the memoised KV slot props
  // bundle, so a churning `request` would re-render every driver slot.
  const latest = useRef({ t, onRefresh, onSelectDatabase });
  useEffect(() => {
    latest.current = { t, onRefresh, onSelectDatabase };
  });

  /**
   * PRD I-6 for a dangerous action: `true` only when the user may proceed.
   * Safe Mode is read live at call time (a dialog can sit open while the user
   * flips the setting), which is how the driver gate this reuses behaves.
   */
  const gateDangerous = useCallback(
    async (copy: DangerCopy): Promise<boolean> => {
      const translate = latest.current.t;
      if (useSettingsStore.getState().settings.safeMode) {
        await confirm({
          title: translate('settings.safeMode'),
          message: translate(copy.blockedKey),
          confirmLabel: translate('common.dismiss'),
          kind: 'info',
        });
        return false;
      }
      return confirm({
        title: translate(copy.titleKey),
        message: translate(copy.messageKey),
        confirmLabel: translate('common.confirm'),
        kind: 'warning',
      });
    },
    [confirm],
  );

  const request = useCallback(
    (action: KvSlotAction) => {
      switch (action.type) {
        case 'refresh':
          latest.current.onRefresh();
          return;
        case 'openSettings':
          openSettingsWindow(DRIVER_SETTINGS_SECTION);
          return;
        case 'selectDatabase': {
          // The workspace-level db switch. When the host threaded its own
          // `handleSelectKvDb` through, the request lands on it verbatim (one
          // implementation of "activate or open this db's panel", shared with
          // the navigation tree). Without it the request degrades like any other
          // unwired action — never a throw, and never a reason to hide the
          // driver's selector (contract F-3 ruling 1).
          const selectDatabase = latest.current.onSelectDatabase;
          if (!selectDatabase) {
            warnUnwired(action);
            return;
          }
          selectDatabase(action.database);
          return;
        }
        case 'flushDb':
          // The gate runs first, always: an unwired executor must never become a
          // way to skip it. Approving only gets as far as the warning below —
          // running FLUSHDB is the driver's `flush_db` command, and the host
          // naming a driver command itself would be the hardcoding the
          // execution gateway exists to prevent. Whoever wires an executor adds
          // it to this branch, behind the same gate.
          void gateDangerous(FLUSH_DB_COPY).then((approved) => {
            if (approved) warnUnwired(action);
          });
          return;
        case 'newKey':
        case 'import':
        case 'export':
        case 'openMonitor':
        case 'setScanBudget':
        default:
          warnUnwired(action);
      }
    },
    [gateDangerous],
  );

  return { request, dialog };
}
