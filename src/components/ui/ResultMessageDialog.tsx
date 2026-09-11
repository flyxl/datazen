import { useCallback, useState } from 'react';
import { AlertCircle, Check, CheckCircle2, Copy } from 'lucide-react';
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
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback(() => {
    void navigator.clipboard.writeText(message);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  }, [message]);

  return (
    <Dialog
      open={open}
      title={kind === 'error' ? t('common.error') : t('common.success')}
      onClose={onClose}
      className="max-w-sm"
      footer={
        <div className="flex w-full items-center justify-between gap-2">
          {kind === 'error' ? (
            <button
              type="button"
              data-testid="result-message-copy"
              className="flex items-center gap-1 rounded px-2 py-1 text-xs text-fg-muted hover:bg-surface-raised hover:text-fg"
              onClick={handleCopy}
              title={t('common.copy')}
            >
              {copied ? (
                <>
                  <Check className="h-3.5 w-3.5 text-green-400" />
                  <span>{t('common.copied')}</span>
                </>
              ) : (
                <>
                  <Copy className="h-3.5 w-3.5" />
                  <span>{t('common.copy')}</span>
                </>
              )}
            </button>
          ) : (
            <div />
          )}
          <Button
            variant="primary"
            className="h-8 px-3 text-xs"
            onClick={onClose}
            data-testid="result-message-ok"
          >
            {t('common.ok')}
          </Button>
        </div>
      }
    >
      <div className="flex items-start gap-3">
        {kind === 'error' ? (
          <AlertCircle className="mt-0.5 h-5 w-5 shrink-0 text-red-400" aria-hidden="true" />
        ) : (
          <CheckCircle2 className="mt-0.5 h-5 w-5 shrink-0 text-green-500" aria-hidden="true" />
        )}
        <p className="selectable select-text whitespace-pre-wrap break-words text-sm text-fg-secondary">
          {message}
        </p>
      </div>
    </Dialog>
  );
}
