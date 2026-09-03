import { Button } from '../../components/ui/Button';
import { Dialog } from '../../components/ui/Dialog';
import { useI18n } from '../../hooks/useI18n';
import type { TransferWritePlan, WriteMode } from '../../commands/transfer';

interface TransferExecuteConfirmDialogProps {
  open: boolean;
  writeMode: WriteMode;
  writePlans: TransferWritePlan[];
  onClose: () => void;
  onConfirm: () => void;
}

export function TransferExecuteConfirmDialog({
  open,
  writeMode,
  writePlans,
  onClose,
  onConfirm,
}: TransferExecuteConfirmDialogProps) {
  const { t } = useI18n();

  const introKey =
    writeMode === 'dropCreateInsert'
      ? 'transfer.executeConfirm.introDrop'
      : 'transfer.executeConfirm.introTruncate';

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={t('transfer.executeConfirm.title')}
      testId="data-transfer-execute-confirm-dialog"
      footer={
        <>
          <Button
            variant="ghost"
            data-testid="data-transfer-execute-confirm-cancel"
            onClick={onClose}
          >
            {t('common.cancel')}
          </Button>
          <Button
            variant="primary"
            data-testid="data-transfer-execute-confirm-proceed"
            onClick={onConfirm}
          >
            {t('transfer.execute')}
          </Button>
        </>
      }
    >
      <div data-testid="data-transfer-execute-confirm" className="space-y-3 text-sm text-fg-muted">
        <p className="text-fg-secondary">{t(introKey)}</p>
        <ul className="list-disc space-y-1 pl-4">
          {writePlans.map((plan) => (
            <li
              key={`${plan.sourceTable}:${plan.targetTable}`}
              data-testid={`data-transfer-execute-confirm-table-${plan.targetTable}`}
            >
              {plan.sourceTable} → {plan.targetTable}
            </li>
          ))}
        </ul>
      </div>
    </Dialog>
  );
}
