import { useState } from 'react';
import { Button } from '../../components/ui/Button';
import { Dialog } from '../../components/ui/Dialog';
import { useI18n } from '../../hooks/useI18n';
import { setTransferLimitationsDismissed } from '../../lib/transferLimitationsPrefs';
import { TRANSFER_LIMITATION_KEYS } from './transferLimitationKeys';

interface TransferLimitationsDialogProps {
  open: boolean;
  onClose: () => void;
}

export function TransferLimitationsDialog({ open, onClose }: TransferLimitationsDialogProps) {
  const { t } = useI18n();
  const [dontShowAgain, setDontShowAgain] = useState(false);

  const handleClose = () => {
    if (dontShowAgain) {
      setTransferLimitationsDismissed();
    }
    setDontShowAgain(false);
    onClose();
  };

  return (
    <Dialog
      open={open}
      onClose={handleClose}
      title={t('transfer.limitations.title')}
      testId="data-transfer-limitations-dialog"
      footer={
        <Button
          variant="primary"
          data-testid="data-transfer-limitations-close"
          onClick={handleClose}
        >
          {t('common.close')}
        </Button>
      }
    >
      <div data-testid="data-transfer-limitations" className="space-y-3 text-sm text-fg-muted">
        <ul className="list-disc space-y-1 pl-4">
          {TRANSFER_LIMITATION_KEYS.map((key) => (
            <li key={key}>{t(key)}</li>
          ))}
        </ul>
        <label className="flex cursor-pointer items-center gap-2 text-sm text-fg">
          <input
            type="checkbox"
            checked={dontShowAgain}
            onChange={(e) => setDontShowAgain(e.target.checked)}
            data-testid="data-transfer-limitations-dismiss"
          />
          {t('transfer.limitations.dontShowAgain')}
        </label>
      </div>
    </Dialog>
  );
}
