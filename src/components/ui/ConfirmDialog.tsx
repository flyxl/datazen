import { AlertTriangle } from 'lucide-react';
import { Button } from './Button';
import { Dialog } from './Dialog';
import { useI18n } from '../../hooks/useI18n';

export interface ConfirmDialogProps {
  open: boolean;
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  kind?: 'warning' | 'info';
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmDialog({
  open,
  title,
  message,
  confirmLabel,
  cancelLabel,
  kind = 'warning',
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const { t } = useI18n();

  return (
    <Dialog
      open={open}
      title={title}
      onClose={onCancel}
      className="max-w-sm"
      footer={
        <>
          <Button variant="ghost" className="h-8 px-3 text-xs" onClick={onCancel}>
            {cancelLabel ?? t('common.cancel')}
          </Button>
          <Button
            variant={kind === 'warning' ? 'danger' : 'default'}
            className="h-8 px-3 text-xs"
            onClick={onConfirm}
            data-testid="confirm-dialog-ok"
          >
            {confirmLabel ?? t('common.confirm')}
          </Button>
        </>
      }
    >
      <div className="flex items-start gap-3">
        {kind === 'warning' && <AlertTriangle className="mt-0.5 h-5 w-5 shrink-0 text-amber-500" />}
        <p className="text-sm text-fg-secondary">{message}</p>
      </div>
    </Dialog>
  );
}
