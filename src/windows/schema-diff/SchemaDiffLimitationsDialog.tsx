import { useState } from 'react';
import { Button } from '../../components/ui/Button';
import { Dialog } from '../../components/ui/Dialog';
import { useI18n } from '../../hooks/useI18n';
import { setSchemaDiffLimitationsDismissed } from '../../lib/schemaDiffLimitationsPrefs';
import { SCHEMA_DIFF_LIMITATION_KEYS } from './schemaDiffLimitationKeys';

interface SchemaDiffLimitationsDialogProps {
  open: boolean;
  onClose: () => void;
}

export function SchemaDiffLimitationsDialog({ open, onClose }: SchemaDiffLimitationsDialogProps) {
  const { t } = useI18n();
  const [dontShowAgain, setDontShowAgain] = useState(false);

  const handleClose = () => {
    if (dontShowAgain) {
      setSchemaDiffLimitationsDismissed();
    }
    setDontShowAgain(false);
    onClose();
  };

  return (
    <Dialog
      open={open}
      onClose={handleClose}
      title={t('schemaDiff.limitations.title')}
      testId="schema-diff-limitations-dialog"
      footer={
        <Button variant="primary" data-testid="schema-diff-limitations-close" onClick={handleClose}>
          {t('common.close')}
        </Button>
      }
    >
      <div data-testid="schema-diff-limitations" className="space-y-3 text-sm text-fg-muted">
        <ul className="list-disc space-y-1 pl-4">
          {SCHEMA_DIFF_LIMITATION_KEYS.map((key) => (
            <li key={key}>{t(key)}</li>
          ))}
        </ul>
        <label className="flex cursor-pointer items-center gap-2 text-sm text-fg">
          <input
            type="checkbox"
            checked={dontShowAgain}
            onChange={(e) => setDontShowAgain(e.target.checked)}
            data-testid="schema-diff-limitations-dismiss"
          />
          {t('schemaDiff.limitations.dontShowAgain')}
        </label>
      </div>
    </Dialog>
  );
}
