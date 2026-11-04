import { useCallback, useEffect, useState } from 'react';
import { useI18n } from '@datazen/ui';
import type { KeyDetail, ValueFrame } from '../shared/types';
import { hasRedisJson, isJsonKeyType, looksLikeJsonModuleDetail } from './hasRedisJson';
import { JsonEditor } from './JsonEditor';
import { StreamEditor } from './StreamEditor';
import { invokeDeleteKey, invokeRename } from './keyEditorsInvokes';
import { invokeGetKeyRaw } from '../shared/redisInvoke';
import { useRedisGate } from '../shared/useRedisGate';
import { requestDraftLeave } from '../shared/draftGuard';
import { HashEditor } from './HashEditor';
import { ListEditor } from './ListEditor';
import { SetEditor } from './SetEditor';
import { ZsetEditor } from './ZsetEditor';
import { StringEditor } from './StringEditor';
import { TtlControls } from './TtlControls';
import { KeyHeaderRow } from './KeyHeaderRow';
import { buildRedisInsertStatement } from './redisInsertStatement';

export type { PluginInvokeFn } from './keyEditorsInvokes';
export {
  invokeCreateKey,
  invokeDeleteKey,
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
  /**
   * Reload the key detail. May resolve `false` to report an I-1 draft-guard
   * refusal (E-5 wires the guard); `void`/`true` reads as success, so the
   * host's plain `reloadDetail` stays assignable here.
   */
  onRefresh: () => boolean | void | Promise<boolean | void>;
  onRenamed?: (newKey: string) => void;
  /**
   * Unsaved-draft signal for the PRD §4 I-1 dirty gate.
   *
   * Only `StringEditor` can hold a draft today: every other type editor writes
   * through `gateWrite('write-op')` on the spot, so there is nothing pending to
   * lose there and they keep the flag at `false`. The workbench relays the value
   * to the host's per-panel `KvSlotState` — this module never touches the relay.
   */
  onDirtyChange?: (dirty: boolean) => void;
}

export function KeyDetailEditor({
  dbSessionId,
  dbIndex,
  detail,
  modules = null,
  onRefresh,
  onRenamed,
  onDirtyChange,
}: KeyDetailEditorProps) {
  const { t } = useI18n();
  const { gateWrite, gateDialog } = useRedisGate();
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

  /** Gate a write path; `true` only when it ran and refreshed successfully. */
  const run = useCallback(
    async (fn: () => Promise<void>): Promise<boolean> => {
      if (!(await gateWrite('write-op'))) return false;
      setError(null);
      try {
        await fn();
        await onRefresh();
        return true;
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
        return false;
      }
    },
    [onRefresh, gateWrite],
  );

  // Header-row actions. `refreshNow`, rename and delete all sit upstream of any
  // server write or refetch, so E-5 puts the I-1 draft guard here: a refusal
  // resolves `false`, which the header already understands (auto-refresh falls
  // to 关, the rename input stays open, the delete never runs).
  const refreshNow = useCallback(async (): Promise<boolean> => {
    if (!(await requestDraftLeave())) return false;
    await onRefresh();
    return true;
  }, [onRefresh]);

  const handleRename = useCallback(
    async (newName: string): Promise<boolean> => {
      if (!(await requestDraftLeave())) return false;
      return run(async () => {
        await invokeRename(dbSessionId, dbIndex, detail.key, newName);
        onRenamed?.(newName);
      });
    },
    [run, dbSessionId, dbIndex, detail.key, onRenamed],
  );

  const handleDelete = useCallback((): Promise<boolean> => {
    // The confirm dialog (owned by the header) has already been answered at
    // this point; the draft guard runs before the DELETE leaves the client.
    return (async () => {
      if (!(await requestDraftLeave())) return false;
      return run(async () => {
        await invokeDeleteKey(dbSessionId, dbIndex, detail.key);
      });
    })();
  }, [run, dbSessionId, dbIndex, detail.key]);

  const showJsonEditor =
    isJsonKeyType(detail.keyType) ||
    (modules !== null && hasRedisJson(modules) && looksLikeJsonModuleDetail(detail));

  return (
    <div className="space-y-3 text-xs">
      {/* Key header row (PRD §3.3): mono name + refresh split · copy · rename · delete */}
      <KeyHeaderRow
        keyName={detail.key}
        onRefresh={refreshNow}
        onRename={handleRename}
        onDelete={handleDelete}
        insertStatement={buildRedisInsertStatement(detail)}
      />

      {/* Badge row (PRD §3.3): 类型 | 大小 N B | TTL pill | truncated */}
      <div className="flex flex-wrap items-center gap-2" data-testid="redis-key-badges">
        <span className="font-medium text-fg-muted">{t('redis.type')}:</span>
        <span
          className="rounded bg-accent/10 px-1.5 py-0.5 text-accent"
          data-testid="redis-key-badge-type"
          data-key-type={detail.keyType}
        >
          {detail.keyType}
        </span>
        {frame?.memBytes != null && (
          <span
            className="rounded bg-surface-alt px-1.5 py-0.5 text-fg-muted"
            data-testid="redis-key-badge-size"
            data-i18n-key="redis.detail.badge.size"
          >
            {t('redis.detail.badge.size', { n: frame.memBytes })}
          </span>
        )}
        <TtlControls
          dbSessionId={dbSessionId}
          dbIndex={dbIndex}
          keyName={detail.key}
          ttl={detail.ttl}
          gateWrite={gateWrite}
          onChanged={() => void onRefresh()}
        />
        {frame?.truncated && (
          <span
            className="rounded bg-warning/10 px-1.5 py-0.5 text-warning"
            data-testid="redis-key-badge-truncated"
            data-i18n-key="redis.detail.badge.truncated"
          >
            {t('redis.detail.badge.truncated')}
          </span>
        )}
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
          onDirtyChange={onDirtyChange}
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
