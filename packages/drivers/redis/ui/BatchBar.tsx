import { useCallback, useState } from 'react';
import { Trash2, Clock, Replace } from 'lucide-react';
import { Button } from '../../../../src/components/ui/Button';
import { Input } from '../../../../src/components/ui/Input';
import { Dialog } from '../../../../src/components/ui/Dialog';
import { useI18n } from '../../../../src/hooks/useI18n';
import { redisCommandInvoke, type RedisInvokeFn } from './redisInvoke';

export interface BatchDeleteResult {
  deleted: number;
  errors: { key: string; error: string }[];
}

export interface BatchSetTtlResult {
  updated: number;
  errors: { key: string; error: string }[];
}

export interface BatchRenameResult {
  renamed: number;
  errors: { key: string; error: string }[];
}

export type PluginInvokeFn = RedisInvokeFn;

export async function invokeDeleteKeys(
  dbSessionId: string,
  dbIndex: number,
  keys: string[],
  invoke: PluginInvokeFn = redisCommandInvoke,
): Promise<number> {
  return (await invoke('redis', 'delete_keys', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    keys,
  })) as number;
}

export async function invokeBatchDeletePattern(
  dbSessionId: string,
  dbIndex: number,
  pattern: string,
  invoke: PluginInvokeFn = redisCommandInvoke,
): Promise<BatchDeleteResult> {
  return (await invoke('redis', 'batch_delete_pattern', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    pattern,
  })) as BatchDeleteResult;
}

export async function invokeBatchSetTtl(
  dbSessionId: string,
  dbIndex: number,
  keys: string[],
  ttlSeconds: number,
  invoke: PluginInvokeFn = redisCommandInvoke,
): Promise<BatchSetTtlResult> {
  return (await invoke('redis', 'batch_set_ttl', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    keys,
    ttlSeconds: ttlSeconds,
  })) as BatchSetTtlResult;
}

export async function invokeBatchRenamePrefix(
  dbSessionId: string,
  dbIndex: number,
  oldPrefix: string,
  newPrefix: string,
  keys: string[] | undefined,
  invoke: PluginInvokeFn = redisCommandInvoke,
): Promise<BatchRenameResult> {
  return (await invoke('redis', 'batch_rename_prefix', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    oldPrefix: oldPrefix,
    newPrefix: newPrefix,
    keys: keys ?? null,
  })) as BatchRenameResult;
}

export async function invokeCountMatching(
  dbSessionId: string,
  dbIndex: number,
  pattern: string,
  invoke: PluginInvokeFn = redisCommandInvoke,
): Promise<number> {
  return (await invoke('redis', 'count_matching', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    pattern,
  })) as number;
}

export interface BatchBarProps {
  dbSessionId: string;
  dbIndex: number;
  selectedKeys: string[];
  searchPattern: string;
  onClearSelection: () => void;
  onRefresh: () => void | Promise<void>;
  onSummary?: (message: string) => void;
}

type DialogMode = 'delete' | 'pattern' | 'ttl' | 'rename' | null;

export function BatchBar({
  dbSessionId,
  dbIndex,
  selectedKeys,
  searchPattern,
  onClearSelection,
  onRefresh,
  onSummary,
}: BatchBarProps) {
  const { t } = useI18n();
  const [dialog, setDialog] = useState<DialogMode>(null);
  const [busy, setBusy] = useState(false);
  const [patternInput, setPatternInput] = useState(searchPattern);
  const [ttlInput, setTtlInput] = useState('');
  const [persistMode, setPersistMode] = useState(false);
  const [oldPrefix, setOldPrefix] = useState('');
  const [newPrefix, setNewPrefix] = useState('');
  const [matchCount, setMatchCount] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  const showSummary = useCallback(
    (parts: string[]) => {
      const msg = parts.filter(Boolean).join(' · ');
      onSummary?.(msg);
    },
    [onSummary],
  );

  const closeDialog = () => {
    setDialog(null);
    setError(null);
    setMatchCount(null);
  };

  const openPatternDialog = () => {
    setPatternInput(searchPattern);
    setDialog('pattern');
  };

  const loadPatternCount = async (pattern: string) => {
    try {
      const count = await invokeCountMatching(dbSessionId, dbIndex, pattern);
      setMatchCount(count);
    } catch {
      setMatchCount(null);
    }
  };

  const handleDeleteSelected = async () => {
    setBusy(true);
    setError(null);
    try {
      const deleted = await invokeDeleteKeys(dbSessionId, dbIndex, selectedKeys);
      showSummary([t('redis.deleted').replace('{count}', String(deleted))]);
      onClearSelection();
      closeDialog();
      await onRefresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleDeletePattern = async () => {
    setBusy(true);
    setError(null);
    try {
      const result = await invokeBatchDeletePattern(dbSessionId, dbIndex, patternInput);
      const errCount = result.errors.length;
      showSummary([
        t('redis.deleted').replace('{count}', String(result.deleted)),
        errCount > 0 ? t('redis.errorsCount').replace('{count}', String(errCount)) : '',
      ]);
      closeDialog();
      await onRefresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleBatchTtl = async () => {
    setBusy(true);
    setError(null);
    try {
      const ttl = persistMode ? -1 : parseInt(ttlInput, 10);
      if (!persistMode && (Number.isNaN(ttl!) || ttl! < 0)) {
        throw new Error(t('redis.ttlSeconds'));
      }
      const result = await invokeBatchSetTtl(dbSessionId, dbIndex, selectedKeys, ttl!);
      const errCount = result.errors.length;
      showSummary([
        t('redis.updated').replace('{count}', String(result.updated)),
        errCount > 0 ? t('redis.errorsCount').replace('{count}', String(errCount)) : '',
      ]);
      onClearSelection();
      closeDialog();
      await onRefresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleBatchRename = async () => {
    setBusy(true);
    setError(null);
    try {
      const keysArg = selectedKeys.length > 0 ? selectedKeys : undefined;
      const result = await invokeBatchRenamePrefix(
        dbSessionId,
        dbIndex,
        oldPrefix,
        newPrefix,
        keysArg,
      );
      const errCount = result.errors.length;
      showSummary([
        t('redis.renamed').replace('{count}', String(result.renamed)),
        errCount > 0 ? t('redis.errorsCount').replace('{count}', String(errCount)) : '',
      ]);
      onClearSelection();
      closeDialog();
      await onRefresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const hasSelection = selectedKeys.length > 0;

  return (
    <>
      <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-edge bg-surface-alt px-3 py-1.5">
        <span className="text-xs text-fg-muted">
          {hasSelection
            ? t('redis.selectedCount').replace('{count}', String(selectedKeys.length))
            : t('redis.batchHint')}
        </span>
        <span
          className="rounded border border-edge bg-surface px-1.5 py-0.5 font-mono text-[11px] text-fg-secondary"
          data-testid="redis-batch-context"
          title={`Redis db${dbIndex}`}
        >
          db{dbIndex}
        </span>
        <div className="flex-1" />
        <Button
          variant="secondary"
          className="h-7 gap-1 px-2 text-xs"
          disabled={!hasSelection}
          onClick={() => setDialog('delete')}
        >
          <Trash2 className="h-3.5 w-3.5" />
          {t('redis.batchDelete')}
        </Button>
        <Button variant="secondary" className="h-7 gap-1 px-2 text-xs" onClick={openPatternDialog}>
          <Trash2 className="h-3.5 w-3.5" />
          {t('redis.deletePattern')}
        </Button>
        <Button
          variant="secondary"
          className="h-7 gap-1 px-2 text-xs"
          disabled={!hasSelection}
          onClick={() => {
            setPersistMode(false);
            setTtlInput('');
            setDialog('ttl');
          }}
        >
          <Clock className="h-3.5 w-3.5" />
          {t('redis.batchTtl')}
        </Button>
        <Button
          variant="secondary"
          className="h-7 gap-1 px-2 text-xs"
          onClick={() => {
            setOldPrefix('');
            setNewPrefix('');
            setDialog('rename');
          }}
        >
          <Replace className="h-3.5 w-3.5" />
          {t('redis.batchRenamePrefix')}
        </Button>
      </div>

      <Dialog
        open={dialog === 'delete'}
        title={t('redis.confirmDeleteKeys')}
        description={
          t('redis.selectedCount').replace('{count}', String(selectedKeys.length)) +
          ` · db${dbIndex}`
        }
        onClose={closeDialog}
        footer={
          <>
            <Button variant="secondary" className="h-8 px-3 text-xs" onClick={closeDialog}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="primary"
              className="h-8 px-3 text-xs"
              disabled={busy}
              onClick={() => void handleDeleteSelected()}
            >
              {t('common.delete')}
            </Button>
          </>
        }
      >
        {error && <p className="text-danger">{error}</p>}
      </Dialog>

      <Dialog
        open={dialog === 'pattern'}
        title={t('redis.confirmDeletePattern')}
        description={`db${dbIndex} · ${patternInput.trim() || '*'}`}
        onClose={closeDialog}
        footer={
          <>
            <Button variant="secondary" className="h-8 px-3 text-xs" onClick={closeDialog}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="primary"
              className="h-8 px-3 text-xs"
              disabled={busy || !patternInput.trim()}
              onClick={() => void handleDeletePattern()}
            >
              {t('common.delete')}
            </Button>
          </>
        }
      >
        <div className="space-y-3">
          <Input
            value={patternInput}
            onChange={(e) => {
              setPatternInput(e.target.value);
              setMatchCount(null);
            }}
            onBlur={() => void loadPatternCount(patternInput)}
            placeholder={t('redis.pattern')}
            className="h-8 font-mono text-xs"
          />
          {matchCount !== null && (
            <p className="text-xs text-fg-muted">
              {t('redis.matchCount').replace('{count}', String(matchCount))}
            </p>
          )}
          {error && <p className="text-danger">{error}</p>}
        </div>
      </Dialog>

      <Dialog
        open={dialog === 'ttl'}
        title={t('redis.confirmBatchTtl')}
        description={`db${dbIndex} · ${t('redis.selectedCount').replace(
          '{count}',
          String(selectedKeys.length),
        )}`}
        onClose={closeDialog}
        footer={
          <>
            <Button variant="secondary" className="h-8 px-3 text-xs" onClick={closeDialog}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="primary"
              className="h-8 px-3 text-xs"
              disabled={busy}
              onClick={() => void handleBatchTtl()}
            >
              {t('common.confirm')}
            </Button>
          </>
        }
      >
        <div className="space-y-3">
          <label className="flex items-center gap-2 text-xs">
            <input
              type="checkbox"
              checked={persistMode}
              onChange={(e) => setPersistMode(e.target.checked)}
            />
            {t('redis.persist')}
          </label>
          {!persistMode && (
            <Input
              value={ttlInput}
              onChange={(e) => setTtlInput(e.target.value)}
              placeholder={t('redis.ttlSeconds')}
              className="h-8 text-xs"
            />
          )}
          {error && <p className="text-danger">{error}</p>}
        </div>
      </Dialog>

      <Dialog
        open={dialog === 'rename'}
        title={t('redis.confirmBatchRename')}
        description={`db${dbIndex}`}
        onClose={closeDialog}
        footer={
          <>
            <Button variant="secondary" className="h-8 px-3 text-xs" onClick={closeDialog}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="primary"
              className="h-8 px-3 text-xs"
              disabled={busy || !oldPrefix}
              onClick={() => void handleBatchRename()}
            >
              {t('common.confirm')}
            </Button>
          </>
        }
      >
        <div className="space-y-3">
          <Input
            value={oldPrefix}
            onChange={(e) => setOldPrefix(e.target.value)}
            placeholder={t('redis.oldPrefix')}
            className="h-8 font-mono text-xs"
          />
          <Input
            value={newPrefix}
            onChange={(e) => setNewPrefix(e.target.value)}
            placeholder={t('redis.newPrefix')}
            className="h-8 font-mono text-xs"
          />
          {hasSelection && (
            <p className="text-xs text-fg-muted">
              {t('redis.selectedCount').replace('{count}', String(selectedKeys.length))}
            </p>
          )}
          {error && <p className="text-danger">{error}</p>}
        </div>
      </Dialog>
    </>
  );
}
