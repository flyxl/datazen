import { useState } from 'react';
import { Button } from './Button';
import { Dialog } from './Dialog';
import { useI18n } from '../../hooks/useI18n';

export interface LimitationsDialogProps {
  open: boolean;
  onClose: () => void;
  /** i18n key for dialog title */
  titleKey: string;
  /** i18n key for "don't show again" checkbox label */
  dontShowAgainKey: string;
  /** i18n keys for limitation bullet items */
  limitationKeys: readonly string[];
  /** Prefix for data-testid attributes (e.g. "schema-diff" → "schema-diff-limitations-dialog") */
  testIdPrefix: string;
  /** Called when user checks "don't show again" and closes */
  onDismiss?: () => void;
}

export function LimitationsDialog({
  open,
  onClose,
  titleKey,
  dontShowAgainKey,
  limitationKeys,
  testIdPrefix,
  onDismiss,
}: LimitationsDialogProps) {
  const { t } = useI18n();
  const [dontShowAgain, setDontShowAgain] = useState(false);

  const handleClose = () => {
    if (dontShowAgain) {
      onDismiss?.();
    }
    setDontShowAgain(false);
    onClose();
  };

  return (
    <Dialog
      open={open}
      onClose={handleClose}
      title={t(titleKey)}
      testId={`${testIdPrefix}-limitations-dialog`}
      footer={
        <Button
          variant="primary"
          data-testid={`${testIdPrefix}-limitations-close`}
          onClick={handleClose}
        >
          {t('common.close')}
        </Button>
      }
    >
      <div
        data-testid={`${testIdPrefix}-limitations`}
        className="space-y-3 text-sm text-fg-muted"
      >
        <ul className="list-disc space-y-1 pl-4">
          {limitationKeys.map((key) => (
            <li key={key}>{t(key)}</li>
          ))}
        </ul>
        <label className="flex cursor-pointer items-center gap-2 text-sm text-fg">
          <input
            type="checkbox"
            checked={dontShowAgain}
            onChange={(e) => setDontShowAgain(e.target.checked)}
            data-testid={`${testIdPrefix}-limitations-dismiss`}
          />
          {t(dontShowAgainKey)}
        </label>
      </div>
    </Dialog>
  );
}
