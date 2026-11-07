import { useCallback, type ReactNode } from 'react';
import { CheckCircle2, Clock, Copy, Check, Rows } from 'lucide-react';
import type { StatementResult } from '../../../types';
import { useI18n } from '../../../hooks/useI18n';
import { useCopyFeedback } from '../../../components/ui/useCopyFeedback';
import { tid } from '../../../lib/tid';

/** How long the "已复制" overlay stays before reverting to "复制". */
const COPIED_FEEDBACK_MS = 2000;

export interface ExecutionSummaryCardProps {
  result: StatementResult;
  statusBar?: ReactNode;
}

export function ExecutionSummaryCard({ result, statusBar }: Readonly<ExecutionSummaryCardProps>) {
  const { t } = useI18n();
  const { copied, copy } = useCopyFeedback(COPIED_FEEDBACK_MS);

  const handleCopy = useCallback(() => {
    if (!result.sql) return;
    copy(result.sql);
  }, [copy, result.sql]);

  const rowsAffectedText =
    result.rowsAffected !== undefined && result.rowsAffected !== null
      ? t('query.rowsAffectedCount', { count: result.rowsAffected })
      : null;

  return (
    <div
      className="flex min-h-0 flex-1 flex-col justify-between bg-surface"
      {...tid('execution-summary-card')}
    >
      <div className="flex flex-1 items-center justify-center p-6">
        <div className="flex max-w-md w-full flex-col items-center rounded-xl border border-edge bg-surface-alt p-6 text-center shadow-sm">
          <div className="mb-3 flex h-12 w-12 items-center justify-center rounded-full bg-success/15 text-success">
            <CheckCircle2 className="h-6 w-6" />
          </div>

          <h3 className="text-base font-semibold text-fg">{t('query.executionSuccess')}</h3>

          <div className="mt-3 flex flex-wrap items-center justify-center gap-2">
            {rowsAffectedText && (
              <span className="inline-flex items-center gap-1.5 rounded-full border border-edge bg-surface px-2.5 py-1 text-xs font-medium text-fg-secondary">
                <Rows className="h-3.5 w-3.5 text-accent" />
                {rowsAffectedText}
              </span>
            )}
            <span className="inline-flex items-center gap-1.5 rounded-full border border-edge bg-surface px-2.5 py-1 text-xs font-medium text-fg-muted">
              <Clock className="h-3.5 w-3.5" />
              {result.executionTimeMs} ms
            </span>
          </div>

          {result.sql && (
            <div className="relative mt-4 w-full text-left">
              <pre className="max-h-28 overflow-x-auto rounded-lg border border-edge bg-surface/80 p-3 font-mono text-xs text-fg-secondary whitespace-pre-wrap break-all">
                {result.sql}
              </pre>
              <button
                type="button"
                onClick={handleCopy}
                className="absolute right-2 top-2 flex items-center gap-1 rounded bg-surface-alt px-1.5 py-0.5 text-[10px] text-fg-muted hover:text-accent border border-edge transition-colors"
                title="复制 SQL"
              >
                {copied ? (
                  <>
                    <Check className="h-3 w-3 text-success" />
                    <span className="text-success">已复制</span>
                  </>
                ) : (
                  <>
                    <Copy className="h-3 w-3" />
                    <span>复制</span>
                  </>
                )}
              </button>
            </div>
          )}
        </div>
      </div>

      {statusBar && (
        <div className="border-t border-edge bg-surface-alt px-3 py-1">{statusBar}</div>
      )}
    </div>
  );
}
