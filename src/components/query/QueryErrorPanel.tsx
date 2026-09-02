import { useCallback, useState } from 'react';
import { Check, Copy, Lightbulb, RotateCcw, Stethoscope, Wand2 } from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';

interface QueryErrorPanelProps {
  message: string;
  /** When provided, renders a "diagnose" action that invokes this callback. */
  onDiagnose?: () => void;
  onExplain?: () => void;
  onFixSql?: () => void;
  onRetry?: () => void;
  onCopy?: () => void;
}

/**
 * Renders a failed query's error message as selectable text with a
 * copy-to-clipboard action. Used instead of the results table when an
 * execution fails so the full message stays readable.
 */
export function QueryErrorPanel({
  message,
  onDiagnose,
  onExplain,
  onFixSql,
  onRetry,
  onCopy,
}: Readonly<QueryErrorPanelProps>) {
  const { t } = useI18n();
  const [errorCopied, setErrorCopied] = useState(false);

  const handleCopyError = useCallback(() => {
    void navigator.clipboard.writeText(message);
    onCopy?.();
    setErrorCopied(true);
    window.setTimeout(() => setErrorCopied(false), 1500);
  }, [message, onCopy]);

  const explain = onExplain ?? onDiagnose;

  return (
    <div className="rounded-md border border-red-500/20 bg-red-500/10 px-4 py-3">
      <div className="flex items-center gap-2">
        <span className="flex-1 select-none text-xs font-semibold uppercase tracking-wide text-red-400">
          {t('common.executionFailed')}
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
        className="selectable mt-2 whitespace-pre-wrap break-words font-mono text-sm leading-relaxed text-red-400"
      >
        {message}
      </pre>
      {(explain || onFixSql || onRetry) && (
        <div className="mt-3 flex flex-wrap items-center gap-1">
          {explain && (
            <button
              type="button"
              data-testid="query-explain-error"
              className="inline-flex items-center gap-1 rounded px-2 py-1 text-[11px] text-accent hover:bg-accent/10"
              onClick={explain}
            >
              {onExplain ? <Lightbulb className="h-3 w-3" /> : <Stethoscope className="h-3 w-3" />}
              {onExplain ? t('query.explainError') : t('diagnosis.diagnose')}
            </button>
          )}
          {onFixSql && (
            <button
              type="button"
              data-testid="query-fix-sql"
              className="inline-flex items-center gap-1 rounded px-2 py-1 text-[11px] text-accent hover:bg-accent/10"
              onClick={onFixSql}
            >
              <Wand2 className="h-3 w-3" />
              {t('query.fixSql')}
            </button>
          )}
          {onRetry && (
            <button
              type="button"
              data-testid="query-retry"
              className="inline-flex items-center gap-1 rounded px-2 py-1 text-[11px] text-accent hover:bg-accent/10"
              onClick={onRetry}
            >
              <RotateCcw className="h-3 w-3" />
              {t('common.retry')}
            </button>
          )}
        </div>
      )}
    </div>
  );
}
