/**
 * Right column of the Redis key browser: the detail drawer for one key.
 *
 * Owns nothing but rendering — `RedisWorkbench` keeps the selected key, the
 * fetched {@link KeyDetail} and the module list — so the workbench stays a
 * state owner (PRD §7-5 split).
 *
 * Two things matter for the KV workspace (PRD §4 I-1 dirty gate):
 * 1. the editor mounts with `key={detail.key}`, so switching keys cannot carry a
 *    previous key's draft — and therefore a previous key's *dirty* flag — into
 *    the newly selected key (before this split the workbench inlined the same
 *    element without a key, so a leftover draft did survive a key switch);
 * 2. draft dirtiness leaves through `onDirtyChange` and the workbench relays it
 *    to the host's per-panel `KvSlotState`; this column never touches the relay.
 *
 * `data-detail-state` / `data-selected-key` markers exist so tests can assert
 * which branch rendered without pinning any translated copy (the guard deleted
 * in W1-C means copy is a human-review concern, not an assertion target).
 */
import { useI18n, Spinner } from '@datazen/ui';
import type { KeyDetail } from '../shared/types';
import { KeyDetailEditor } from '../value-editors/KeyEditors';

export interface DetailColumnProps {
  dbSessionId: string;
  dbIndex: number;
  /** Key currently selected in the tree, `null` when nothing is selected. */
  selectedKey: string | null;
  detail: KeyDetail | null;
  detailLoading: boolean;
  modules: string[] | null;
  onRefresh: () => void;
  onRenamed: (newKey: string) => void;
  /** Unsaved-draft flag of the mounted editor (the workbench relays it to the host). */
  onDirtyChange?: (dirty: boolean) => void;
}

export function DetailColumn({
  dbSessionId,
  dbIndex,
  selectedKey,
  detail,
  detailLoading,
  modules,
  onRefresh,
  onRenamed,
  onDirtyChange,
}: DetailColumnProps) {
  const { t } = useI18n();

  if (!selectedKey) {
    return (
      <div
        className="flex flex-1 items-center justify-center px-6 text-center text-sm text-fg-muted"
        data-testid="redis-detail-column"
        data-detail-state="no-key"
        data-i18n-key="redis.selectKeyHint"
      >
        {t('redis.selectKeyHint')}
      </div>
    );
  }

  const state = detailLoading ? 'loading' : detail ? 'ready' : 'pending';

  return (
    <div
      className="flex min-h-0 flex-1 flex-col"
      data-testid="redis-detail-column"
      data-selected-key={selectedKey}
      data-detail-state={state}
    >
      <div className="flex-1 overflow-auto p-3">
        {detailLoading ? (
          <div
            className="flex items-center gap-2 text-xs text-fg-muted"
            data-testid="redis-detail-loading"
          >
            <Spinner size="md" />
            {t('common.loading')}
          </div>
        ) : detail ? (
          <KeyDetailEditor
            key={detail.key}
            dbSessionId={dbSessionId}
            dbIndex={dbIndex}
            detail={detail}
            modules={modules}
            onRefresh={onRefresh}
            onRenamed={onRenamed}
            onDirtyChange={onDirtyChange}
          />
        ) : null}
      </div>
    </div>
  );
}
