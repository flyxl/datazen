import { useCallback, useEffect, useState } from 'react';
import { Button } from '@datazen/ui';
import { Input } from '@datazen/ui';
import { Select } from '@datazen/ui';
import { Dialog } from '@datazen/ui';
import { useI18n } from '../../../../src/hooks/useI18n';
import { redisCommandInvoke } from './redisInvoke';
import { invokeDeleteKeys } from './BatchBar';
import { invokeCreateKey, invokeRename, invokeSetTtl } from './KeyEditors';

export type KeyCtxDialog =
  | { mode: 'ttl'; key: string }
  | { mode: 'rename'; key: string }
  | { mode: 'delete'; key: string }
  | null;

export interface KeyWorkbenchDialogsProps {
  dbSessionId: string;
  dbIndex: number;
  allowFlush: boolean;
  createTypes: string[];
  selectedKey: string | null;
  onRefreshKeys: () => void;
  onSelectKey: (key: string) => Promise<void>;
  onClearSelectedKey: () => void;
  onUpdateSelectedKey: (key: string) => void;
  onUpdateSelectedKeys: (updater: (prev: Set<string>) => Set<string>) => void;
  onBatchSummary: (msg: string) => void;
  createOpen: boolean;
  onCreateOpenChange: (open: boolean) => void;
  flushDialog: 'db' | 'all' | null;
  onFlushDialogChange: (v: 'db' | 'all' | null) => void;
  keyCtxDialog: KeyCtxDialog;
  onKeyCtxDialogChange: (v: KeyCtxDialog) => void;
}

export function KeyWorkbenchDialogs({
  dbSessionId,
  dbIndex,
  allowFlush,
  createTypes,
  selectedKey,
  onRefreshKeys,
  onSelectKey,
  onClearSelectedKey,
  onUpdateSelectedKey,
  onUpdateSelectedKeys,
  onBatchSummary,
  createOpen,
  onCreateOpenChange,
  flushDialog,
  onFlushDialogChange,
  keyCtxDialog,
  onKeyCtxDialogChange,
}: KeyWorkbenchDialogsProps) {
  const { t } = useI18n();

  const [createName, setCreateName] = useState('');
  const [createType, setCreateType] = useState('string');
  const [createValue, setCreateValue] = useState('');
  const [createBusy, setCreateBusy] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);

  const [flushConfirm, setFlushConfirm] = useState('');
  const [flushBusy, setFlushBusy] = useState(false);
  const [flushError, setFlushError] = useState<string | null>(null);

  const [keyCtxTtlInput, setKeyCtxTtlInput] = useState('');
  const [keyCtxRenameInput, setKeyCtxRenameInput] = useState('');
  const [keyCtxBusy, setKeyCtxBusy] = useState(false);
  const [keyCtxError, setKeyCtxError] = useState<string | null>(null);

  const closeKeyCtxDialog = useCallback(() => {
    onKeyCtxDialogChange(null);
    setKeyCtxTtlInput('');
    setKeyCtxRenameInput('');
    setKeyCtxError(null);
    setKeyCtxBusy(false);
  }, [onKeyCtxDialogChange]);

  useEffect(() => {
    if (keyCtxDialog?.mode === 'rename') {
      setKeyCtxRenameInput(keyCtxDialog.key);
      setKeyCtxError(null);
    } else if (keyCtxDialog?.mode === 'ttl') {
      setKeyCtxTtlInput('');
      setKeyCtxError(null);
    } else if (keyCtxDialog?.mode === 'delete') {
      setKeyCtxError(null);
    }
  }, [keyCtxDialog]);

  const handleCreateKey = async () => {
    const name = createName.trim();
    if (!name) return;
    setCreateBusy(true);
    setCreateError(null);
    try {
      await invokeCreateKey(dbSessionId, dbIndex, name, createType, createValue);
      onCreateOpenChange(false);
      setCreateName('');
      setCreateValue('');
      setCreateError(null);
      onRefreshKeys();
      await onSelectKey(name);
    } catch (e) {
      setCreateError(e instanceof Error ? e.message : String(e));
    } finally {
      setCreateBusy(false);
    }
  };

  const handleFlush = async () => {
    setFlushBusy(true);
    setFlushError(null);
    try {
      if (flushDialog === 'db') {
        await redisCommandInvoke('redis', 'flush_db', {
          dbSessionId,
          dbIndex,
          allowFlush,
        });
      } else if (flushDialog === 'all') {
        await redisCommandInvoke('redis', 'flush_all', {
          dbSessionId,
          allowFlush,
        });
      }
      onFlushDialogChange(null);
      setFlushConfirm('');
      setFlushError(null);
      onRefreshKeys();
    } catch (e) {
      setFlushError(e instanceof Error ? e.message : String(e));
    } finally {
      setFlushBusy(false);
    }
  };

  const flushConfirmOk =
    flushDialog === 'all' ? flushConfirm === 'ALL' : flushConfirm === String(dbIndex);

  const handleKeyCtxSetTtl = async () => {
    if (keyCtxDialog?.mode !== 'ttl') return;
    setKeyCtxBusy(true);
    setKeyCtxError(null);
    try {
      const secs = parseInt(keyCtxTtlInput, 10);
      if (Number.isNaN(secs) || secs < 0) {
        throw new Error(t('redis.ttlSeconds'));
      }
      await invokeSetTtl(dbSessionId, dbIndex, keyCtxDialog.key, secs);
      closeKeyCtxDialog();
      onRefreshKeys();
      if (selectedKey === keyCtxDialog.key) {
        await onSelectKey(keyCtxDialog.key);
      }
    } catch (err) {
      setKeyCtxError(err instanceof Error ? err.message : String(err));
    } finally {
      setKeyCtxBusy(false);
    }
  };

  const handleKeyCtxPersist = async () => {
    if (keyCtxDialog?.mode !== 'ttl') return;
    setKeyCtxBusy(true);
    setKeyCtxError(null);
    try {
      await invokeSetTtl(dbSessionId, dbIndex, keyCtxDialog.key, -1);
      closeKeyCtxDialog();
      onRefreshKeys();
      if (selectedKey === keyCtxDialog.key) {
        await onSelectKey(keyCtxDialog.key);
      }
    } catch (err) {
      setKeyCtxError(err instanceof Error ? err.message : String(err));
    } finally {
      setKeyCtxBusy(false);
    }
  };

  const handleKeyCtxRename = async () => {
    if (keyCtxDialog?.mode !== 'rename') return;
    const next = keyCtxRenameInput.trim();
    if (!next || next === keyCtxDialog.key) return;
    setKeyCtxBusy(true);
    setKeyCtxError(null);
    try {
      await invokeRename(dbSessionId, dbIndex, keyCtxDialog.key, next);
      closeKeyCtxDialog();
      if (selectedKey === keyCtxDialog.key) {
        onUpdateSelectedKey(next);
      }
      onUpdateSelectedKeys((prev) => {
        if (!prev.has(keyCtxDialog.key)) return prev;
        const updated = new Set(prev);
        updated.delete(keyCtxDialog.key);
        updated.add(next);
        return updated;
      });
      onRefreshKeys();
      await onSelectKey(next);
    } catch (err) {
      setKeyCtxError(err instanceof Error ? err.message : String(err));
    } finally {
      setKeyCtxBusy(false);
    }
  };

  const handleKeyCtxDelete = async () => {
    if (keyCtxDialog?.mode !== 'delete') return;
    setKeyCtxBusy(true);
    setKeyCtxError(null);
    try {
      const deleted = await invokeDeleteKeys(dbSessionId, dbIndex, [keyCtxDialog.key]);
      onBatchSummary(t('redis.deleted').replace('{count}', String(deleted)));
      if (selectedKey === keyCtxDialog.key) {
        onClearSelectedKey();
      }
      onUpdateSelectedKeys((prev) => {
        if (!prev.has(keyCtxDialog.key)) return prev;
        const updated = new Set(prev);
        updated.delete(keyCtxDialog.key);
        return updated;
      });
      closeKeyCtxDialog();
      onRefreshKeys();
    } catch (err) {
      setKeyCtxError(err instanceof Error ? err.message : String(err));
    } finally {
      setKeyCtxBusy(false);
    }
  };

  return (
    <>
      <Dialog
        open={createOpen}
        title={t('redis.createKey')}
        onClose={() => {
          onCreateOpenChange(false);
          setCreateError(null);
        }}
        footer={
          <>
            <Button
              variant="secondary"
              className="h-8 px-3 text-xs"
              onClick={() => {
                onCreateOpenChange(false);
                setCreateError(null);
              }}
            >
              {t('common.cancel')}
            </Button>
            <Button
              variant="primary"
              className="h-8 px-3 text-xs"
              disabled={createBusy || !createName.trim()}
              onClick={() => void handleCreateKey()}
            >
              {t('redis.create')}
            </Button>
          </>
        }
      >
        <div className="space-y-3">
          <Input
            value={createName}
            onChange={(e) => setCreateName(e.target.value)}
            placeholder={t('redis.keyName')}
            className="h-8 font-mono text-xs"
          />
          <Select
            value={createType}
            onChange={setCreateType}
            className="h-8 w-full text-xs"
            options={createTypes.map((type) => ({ value: type, label: type }))}
          />
          <Input
            value={createValue}
            onChange={(e) => setCreateValue(e.target.value)}
            placeholder={t('redis.value')}
            className="h-8 font-mono text-xs"
          />
          {createError && <p className="text-danger">{createError}</p>}
        </div>
      </Dialog>

      <Dialog
        open={flushDialog !== null}
        title={flushDialog === 'all' ? t('redis.confirmFlushAll') : t('redis.confirmFlushDb')}
        description={
          flushDialog === 'all'
            ? t('redis.typeConfirmAll')
            : t('redis.typeConfirmDb').replace('{index}', String(dbIndex))
        }
        onClose={() => {
          onFlushDialogChange(null);
          setFlushConfirm('');
          setFlushError(null);
        }}
        footer={
          <>
            <Button
              variant="secondary"
              className="h-8 px-3 text-xs"
              onClick={() => {
                onFlushDialogChange(null);
                setFlushConfirm('');
                setFlushError(null);
              }}
            >
              {t('common.cancel')}
            </Button>
            <Button
              variant="primary"
              className="h-8 px-3 text-xs text-danger"
              disabled={!flushConfirmOk || flushBusy}
              onClick={() => void handleFlush()}
            >
              {t('redis.flushConfirm')}
            </Button>
          </>
        }
      >
        <div className="space-y-3">
          <Input
            value={flushConfirm}
            onChange={(e) => setFlushConfirm(e.target.value)}
            placeholder={t('redis.typeConfirmPlaceholder')}
            className="h-8 font-mono text-xs"
          />
          {flushError && <p className="text-danger">{flushError}</p>}
        </div>
      </Dialog>

      <Dialog
        open={keyCtxDialog?.mode === 'ttl'}
        title={t('redis.setTtl')}
        description={keyCtxDialog?.mode === 'ttl' ? keyCtxDialog.key : undefined}
        onClose={closeKeyCtxDialog}
        footer={
          <>
            <Button variant="secondary" className="h-8 px-3 text-xs" onClick={closeKeyCtxDialog}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="secondary"
              className="h-8 px-3 text-xs"
              disabled={keyCtxBusy}
              onClick={() => void handleKeyCtxPersist()}
            >
              {t('redis.persist')}
            </Button>
            <Button
              variant="primary"
              className="h-8 px-3 text-xs"
              disabled={keyCtxBusy || !keyCtxTtlInput.trim()}
              onClick={() => void handleKeyCtxSetTtl()}
            >
              {t('redis.setTtl')}
            </Button>
          </>
        }
      >
        <div className="space-y-3">
          <Input
            value={keyCtxTtlInput}
            onChange={(e) => setKeyCtxTtlInput(e.target.value)}
            placeholder={t('redis.ttlSeconds')}
            className="h-8 font-mono text-xs"
          />
          {keyCtxError && <p className="text-danger">{keyCtxError}</p>}
        </div>
      </Dialog>

      <Dialog
        open={keyCtxDialog?.mode === 'rename'}
        title={t('redis.renameKey')}
        description={keyCtxDialog?.mode === 'rename' ? keyCtxDialog.key : undefined}
        onClose={closeKeyCtxDialog}
        footer={
          <>
            <Button variant="secondary" className="h-8 px-3 text-xs" onClick={closeKeyCtxDialog}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="primary"
              className="h-8 px-3 text-xs"
              disabled={
                keyCtxBusy ||
                !keyCtxRenameInput.trim() ||
                (keyCtxDialog?.mode === 'rename' && keyCtxRenameInput.trim() === keyCtxDialog.key)
              }
              onClick={() => void handleKeyCtxRename()}
            >
              {t('redis.renameKey')}
            </Button>
          </>
        }
      >
        <div className="space-y-3">
          <Input
            value={keyCtxRenameInput}
            onChange={(e) => setKeyCtxRenameInput(e.target.value)}
            placeholder={t('redis.keyName')}
            className="h-8 font-mono text-xs"
          />
          {keyCtxError && <p className="text-danger">{keyCtxError}</p>}
        </div>
      </Dialog>

      <Dialog
        open={keyCtxDialog?.mode === 'delete'}
        title={t('redis.confirmDeleteKeys')}
        description={keyCtxDialog?.mode === 'delete' ? keyCtxDialog.key : undefined}
        onClose={closeKeyCtxDialog}
        footer={
          <>
            <Button variant="secondary" className="h-8 px-3 text-xs" onClick={closeKeyCtxDialog}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="primary"
              className="h-8 px-3 text-xs text-danger"
              disabled={keyCtxBusy}
              onClick={() => void handleKeyCtxDelete()}
            >
              {t('common.delete')}
            </Button>
          </>
        }
      >
        {keyCtxError && <p className="text-danger">{keyCtxError}</p>}
      </Dialog>
    </>
  );
}

/** Helpers to open key-context dialogs from the parent context menu. */
export function openKeyCtxTtl(key: string): KeyCtxDialog {
  return { mode: 'ttl', key };
}
export function openKeyCtxRename(key: string): KeyCtxDialog {
  return { mode: 'rename', key };
}
export function openKeyCtxDelete(key: string): KeyCtxDialog {
  return { mode: 'delete', key };
}
