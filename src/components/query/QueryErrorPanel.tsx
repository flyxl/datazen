import { useCallback, useState } from 'react';
import { Check, Copy, Stethoscope } from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';

interface QueryErrorPanelProps {
  message: string;
  /** When provided, renders a "diagnose" action that invokes this callback. */
  onDiagnose?: () => void;
}

/**
 * Renders a failed query's error message as selectable text with a
 * copy-to-clipboard action. Used instead of the results table when an
 * execution fails so the full message stays readable.
 */
export function QueryErrorPanel({ message, onDiagnose }: Readonly<QueryErrorPanelProps>) {
  const { t } = useI18n();
  const [errorCopied, setErrorCopied] = useState(false);

  const handleCopyError = useCallback(() => {
    void navigator.clipboard.writeText(message);
    setErrorCopied(true);
    window.setTimeout(() => setErrorCopied(false), 1500);
  }, [message]);

  return (
    <div className="rounded-md border border-red-500/20 bg-red-500/10 px-4 py-3">
      <div className="flex items-center gap-2">
        <span className="flex-1 select-none text-xs font-semibold uppercase tracking-wide text-red-400">
          {t('query.executeFailed')}
        </span>
        <button
          type="button"
          data-testid="query-copy-error"
          className="flex shrink-0 items-center gap-1 rounded px-2 py-0.5 text-[11px] text-fg-secondary hover:bg-red-500/10"
          onClick={handleCopyError}
          title={t('common.copy')}
        >
          {errorCopied ? (
            <>
              <Check className="h-3 w-3 text-green-400" />
              {t('common.copied')}
            </>
          ) : (
            <>
              <Copy className="h-3 w-3" />
              {t('common.copy')}
            </>
          )}
        </button>
      </div>
      <pre
        data-testid="query-error-message"
        className="mt-2 whitespace-pre-wrap break-words font-mono text-sm leading-relaxed text-red-400"
      >
        {message}
      </pre>
      {onDiagnose && (
        <button
          type="button"
          className="mt-3 inline-flex items-center gap-1 rounded px-2 py-1 text-[11px] text-blue-400 hover:bg-blue-500/10"
          onClick={onDiagnose}
        >
          <Stethoscope className="h-3 w-3" />
          {t('diagnosis.diagnose')}
        </button>
      )}
    </div>
  );
}
