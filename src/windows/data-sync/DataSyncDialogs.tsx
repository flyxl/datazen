import { Button } from '../../components/ui/Button';
import { CopyableError } from '../../components/ui/CopyableError';
import { Dialog } from '../../components/ui/Dialog';
import { Spinner } from '../../components/ui/Spinner';
import { useI18n } from '../../hooks/useI18n';

export interface DataSyncDialogsProps {
  errorOpen: boolean;
  errorMsg: string;
  onCloseError: () => void;
  deleteConfirmOpen: boolean;
  onCloseDeleteConfirm: () => void;
  onConfirmEnableDelete: () => void;
  explainOpen: boolean;
  explainLoading: boolean;
  explainText: string;
  onCloseExplain: () => void;
  executeConfirmOpen: boolean;
  onCloseExecuteConfirm: () => void;
  onConfirmExecute: () => void;
}

/** Dialogs of the Data Sync window: errors, delete and execute confirmations. */
export function DataSyncDialogs({
  errorOpen,
  errorMsg,
  onCloseError,
  deleteConfirmOpen,
  onCloseDeleteConfirm,
  onConfirmEnableDelete,
  explainOpen,
  explainLoading,
  explainText,
  onCloseExplain,
  executeConfirmOpen,
  onCloseExecuteConfirm,
  onConfirmExecute,
}: DataSyncDialogsProps) {
  const { t } = useI18n();

  return (
    <>
      <Dialog
        open={errorOpen}
        title={t('common.hint')}
        onClose={onCloseError}
        footer={
          <Button variant="primary" onClick={onCloseError}>
            {t('common.ok')}
          </Button>
        }
      >
        <CopyableError message={errorMsg} className="error-message text-sm" data-testid="data-sync-error" />
      </Dialog>

      <Dialog
        open={deleteConfirmOpen}
        title={t('sync.deleteConfirmTitle')}
        onClose={onCloseDeleteConfirm}
        footer={
          <>
            <Button variant="ghost" onClick={onCloseDeleteConfirm}>
              {t('common.cancel')}
            </Button>
            <Button variant="danger" onClick={onConfirmEnableDelete}>
              {t('sync.enableDelete')}
            </Button>
          </>
        }
      >
        <p className="text-sm text-fg-secondary">{t('sync.deleteConfirmBody')}</p>
      </Dialog>

      <Dialog
        open={explainOpen}
        title={t('sync.explainDiffTitle')}
        onClose={onCloseExplain}
        footer={
          <Button variant="primary" onClick={onCloseExplain}>
            {t('common.close')}
          </Button>
        }
      >
        {explainLoading ? (
          <div className="flex items-center gap-2 text-sm text-fg-muted">
            <Spinner size="lg" />
            {t('sync.executing')}
          </div>
        ) : (
          <p data-testid="data-sync-explain-result" className="whitespace-pre-wrap text-sm text-fg-secondary">
            {explainText}
          </p>
        )}
      </Dialog>

      <Dialog
        open={executeConfirmOpen}
        testId="data-sync-execute-confirm-dialog"
        title={t('sync.executeDeleteTitle')}
        onClose={onCloseExecuteConfirm}
        footer={
          <>
            <Button variant="ghost" onClick={onCloseExecuteConfirm}>
              {t('common.cancel')}
            </Button>
            <Button variant="run" data-testid="data-sync-confirm-execute" onClick={onConfirmExecute}>
              {t('sync.execute')}
            </Button>
          </>
        }
      >
        <p className="text-sm text-fg-secondary">{t('sync.executeDeleteBody')}</p>
      </Dialog>
    </>
  );
}
