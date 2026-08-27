import { AlertCircle, CheckCircle2 } from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';
import { Button } from './Button';
import { Dialog } from './Dialog';

export interface ResultMessageDialogProps {
  open: boolean;
  kind: 'error' | 'success';
  message: string;
  onClose: () => void;
}

/** Compact success/error alert with an explicit dismiss button. */
export function ResultMessageDialog({ open, kind, message, onClose }: ResultMessageDialogProps) {
  const { t } = useI18n();

  return (
    <Dialog
      open={open}
      title={kind === 'error' ? t('common.error') : t('common.success')}
      onClose={onClose}
      className="max-w-sm"
      footer={
        <Button variant="primary" className="h-8 px-3 text-xs" onClick={onClose}>
          {t('common.ok')}
        </Button>
      }
    >
      <div className="flex items-start gap-3">
        {kind === 'error' ? (
          <AlertCircle className="mt-0.5 h-5 w-5 shrink-0 text-red-400" aria-hidden="true" />
        ) : (
          <CheckCircle2 className="mt-0.5 h-5 w-5 shrink-0 text-green-500" aria-hidden="true" />
        )}
        <p className="whitespace-pre-wrap break-words text-sm text-fg-secondary">{message}</p>
      </div>
    </Dialog>
  );
}
