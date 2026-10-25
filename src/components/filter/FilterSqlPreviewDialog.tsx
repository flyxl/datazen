import { useState } from 'react';
import { Copy } from 'lucide-react';
import { Dialog } from '../ui/Dialog';
import { Button } from '../ui/Button';
import { ErrorBanner } from '../ui/ErrorBanner';
import { useI18n } from '../../hooks/useI18n';
import type { TableReadPreviewSql } from '@datazen/backend-client';

export interface FilterSqlPreviewDialogProps {
  open: boolean;
  onClose: () => void;
  /** The preview answer; `null` while the round trip is in flight. */
  preview: TableReadPreviewSql | null;
  /** Why the preview failed, when it did — the statements stay hidden. */
  error?: string | null;
}

/**
 * The statements a table read would send (DB-15).
 *
 * Every line of SQL here was assembled by the backend's own read path, so the
 * preview is what the product sends, not a description of it. The exact
 * `COUNT(*)` is always shown — an estimate falls back to it, and the user
 * deserves to see both the plan and the rescue.
 */
export function FilterSqlPreviewDialog({
  open,
  onClose,
  preview,
  error,
}: FilterSqlPreviewDialogProps) {
  const { t } = useI18n();
  const [copied, setCopied] = useState(false);

  const statements: { key: string; testId: string; label: string; sql: string }[] = [];
  if (preview) {
    statements.push({
      key: 'data',
      testId: 'filter-sql-preview-data',
      label: t('filter.previewSqlData'),
      sql: preview.dataSql,
    });
    if (preview.countSql) {
      statements.push({
        key: 'count',
        testId: 'filter-sql-preview-count',
        label: `${t('filter.previewSqlCount')} · ${t(`filter.previewSqlKind.${preview.countKind}`)}`,
        sql: preview.countSql,
      });
    }
    statements.push({
      key: 'exact-count',
      testId: 'filter-sql-preview-exact-count',
      label: t('filter.previewSqlExactCount'),
      sql: preview.exactCountSql,
    });
  }

  const copyAll = () => {
    if (!preview) return;
    const text = statements
      .map((statement) => `-- ${statement.label}\n${statement.sql}`)
      .join('\n\n');
    void navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  };

  return (
    <Dialog
      open={open}
      testId="filter-sql-preview-dialog"
      title={t('filter.previewSqlTitle')}
      description={t('filter.previewSqlDescription')}
      onClose={onClose}
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            {t('common.close')}
          </Button>
          <Button
            variant="primary"
            onClick={copyAll}
            disabled={!preview}
            data-testid="filter-sql-preview-copy"
          >
            <Copy className="h-3.5 w-3.5" />
            {copied ? t('common.copied') : t('common.copy')}
          </Button>
        </>
      }
    >
      {error && (
        <ErrorBanner className="mb-3" data-testid="filter-sql-preview-error">
          {error}
        </ErrorBanner>
      )}
      <div className="space-y-3">
        {statements.map((statement) => (
          <div key={statement.key}>
            <div className="mb-1 text-xs font-medium text-fg-secondary">{statement.label}</div>
            <pre
              data-testid={statement.testId}
              className="selectable max-h-48 overflow-auto rounded-md border border-edge bg-surface-alt px-3 py-2 font-mono text-xs whitespace-pre-wrap break-words text-fg"
            >
              {statement.sql}
            </pre>
          </div>
        ))}
        {!preview && !error && (
          <div
            data-testid="filter-sql-preview-loading"
            className="py-8 text-center text-sm text-fg-muted"
          >
            {t('filter.previewSqlLoading')}
          </div>
        )}
      </div>
    </Dialog>
  );
}
