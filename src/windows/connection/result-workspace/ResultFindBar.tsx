import { useEffect, useMemo, useRef, useState } from 'react';
import { CaseSensitive, Replace, ReplaceAll, Search, X } from 'lucide-react';
import { useI18n } from '../../../hooks/useI18n';
import { Select, type SelectOption } from '@datazen/ui';
import { cn } from '../../../lib/cn';
import {
  type ResultFindOperator,
  type ResultFindQuery,
  type ResultReplacePlanItem,
} from '../../../lib/resultFindReplace';

/**
 * The find-and-replace bar of one query result (DB-14).
 *
 * The bar filters the loaded rows as the user types (the PRD's "filter the
 * loaded rows"), and — when the result may be written back — replaces the
 * needle in the matching cells, one cell or all of them, through the result
 * grid's own write path. Safe Mode and read-only results keep the find but
 * hide the replace: filtering is reading, replacing is writing.
 *
 * The rules live in `src/lib/resultFindReplace.ts`; this component owns only
 * the inputs and the cursor of a "replace next" walk.
 */

export interface ResultFindBarProps {
  /** The result's column names, in result order. */
  columns: readonly string[];
  /** The filter of the loaded rows, as the bar's query defines it. */
  query: ResultFindQuery;
  onQueryChange: (query: ResultFindQuery) => void;
  /** The text a replacement writes where the needle appears. */
  replacement: string;
  onReplacementChange: (replacement: string) => void;
  /** How many loaded rows the filter keeps. */
  matchedRows: number;
  /** How many loaded rows the result holds in total. */
  totalRows: number;
  /** The cells a replacement would write, in the statement's coordinates. */
  plan: readonly ResultReplacePlanItem[];
  /**
   * Write one planned cell. The bar hands over the plan item; the commit,
   * its errors and the view refresh stay with the result grid.
   */
  onApplyReplacement: (item: ResultReplacePlanItem) => Promise<void>;
  /** Whether the result may be written back at all (Safe Mode ⇒ `false`). */
  canReplace: boolean;
  /** Why the result cannot be written back, when it cannot. */
  replaceReason?: string | null;
  onClose: () => void;
}

export function ResultFindBar({
  columns,
  query,
  onQueryChange,
  replacement,
  onReplacementChange,
  matchedRows,
  totalRows,
  plan,
  onApplyReplacement,
  canReplace,
  replaceReason,
  onClose,
}: ResultFindBarProps) {
  const { t } = useI18n();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const needleRef = useRef<HTMLInputElement | null>(null);

  // The bar opens for the hands: the needle takes the focus, the previous
  // filter (if any) keeps running.
  useEffect(() => {
    needleRef.current?.focus();
  }, []);

  const patch = (changes: Partial<ResultFindQuery>) => onQueryChange({ ...query, ...changes });
  const applyOne = async (item: ResultReplacePlanItem): Promise<boolean> => {
    setBusy(true);
    setError(null);
    try {
      await onApplyReplacement(item);
      return true;
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      return false;
    } finally {
      setBusy(false);
    }
  };

  const handleReplaceNext = () => {
    // The next match of the plan: the cursor is the plan itself, because a
    // successful write drops the cell from the recomputed plan.
    const item = plan[0];
    if (!item) return;
    void applyOne(item);
  };

  const handleReplaceAll = async () => {
    // One commit per cell, in row order: a failure stops the walk, because
    // continuing would replace cells whose siblings the user may have wanted
    // to keep once the first write refused.
    for (const item of plan) {
      // eslint-disable-next-line no-await-in-loop -- the order is the point
      const ok = await applyOne(item);
      if (!ok) return;
    }
  };

  const operatorLabel: Record<ResultFindOperator, string> = useMemo(
    () => ({
      contains: t('resultFind.operator.contains'),
      notContains: t('resultFind.operator.notContains'),
      prefix: t('resultFind.operator.prefix'),
      suffix: t('resultFind.operator.suffix'),
    }),
    [t],
  );

  return (
    <div
      className="flex shrink-0 flex-col gap-1 border-b border-edge bg-surface-alt px-2 py-1.5"
      data-testid="result-find-bar"
    >
      <div className="flex items-center gap-1.5">
        <Search className="h-3.5 w-3.5 shrink-0 text-fg-muted" />
        <input
          ref={needleRef}
          type="text"
          data-testid="result-find-needle"
          className="h-6 w-44 rounded-md bg-surface px-2 text-xs text-fg placeholder:text-fg-muted focus:outline-none focus:ring-1 focus:ring-accent"
          placeholder={t('resultFind.placeholder')}
          value={query.needle}
          onChange={(event) => patch({ needle: event.target.value })}
          onKeyDown={(event) => {
            if (event.key === 'Escape') {
              event.preventDefault();
              onClose();
            }
            // Enter walks to the replace when it is available, as the one
            // action the bar still owes.
            if (event.key === 'Enter' && canReplace && plan.length > 0) {
              event.preventDefault();
              handleReplaceNext();
            }
          }}
        />
        <Select
          triggerDataAttrs={{ 'data-testid': 'result-find-column' }}
          className="h-6 max-w-40 text-xs"
          value={query.column ?? ''}
          options={[
            { value: '', label: t('resultFind.anyColumn') },
            ...columns.map((name): SelectOption => ({ value: name, label: name })),
          ]}
          onChange={(value) => patch({ column: value || null })}
          title={t('resultFind.columnLabel')}
          fitContent
        />
        <Select
          triggerDataAttrs={{ 'data-testid': 'result-find-operator' }}
          className="h-6 text-xs"
          value={query.operator}
          options={(Object.keys(operatorLabel) as ResultFindOperator[]).map(
            (operator): SelectOption => ({ value: operator, label: operatorLabel[operator] }),
          )}
          onChange={(value) => patch({ operator: value as ResultFindOperator })}
          title={t('resultFind.operatorLabel')}
          fitContent
        />
        <button
          type="button"
          data-testid="result-find-match-case"
          aria-pressed={query.matchCase}
          title={t('resultFind.matchCase')}
          onClick={() => patch({ matchCase: !query.matchCase })}
          className={cn(
            'flex h-6 w-6 items-center justify-center rounded-md border border-edge',
            query.matchCase ? 'bg-accent/20 text-accent' : 'bg-surface text-fg-muted hover:text-fg',
          )}
        >
          <CaseSensitive className="h-3.5 w-3.5" />
        </button>
        <span className="shrink-0 text-[10px] text-fg-muted" data-testid="result-find-count">
          {t('resultFind.matchCount', { matched: matchedRows, total: totalRows })}
        </span>
        <button
          type="button"
          data-testid="result-find-close"
          className="ml-auto flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-fg-muted hover:bg-surface-raised hover:text-fg"
          title={t('resultFind.close')}
          onClick={onClose}
        >
          <X className="h-3.5 w-3.5" />
        </button>
      </div>

      {canReplace ? (
        <div className="flex items-center gap-1.5">
          <Replace className="h-3.5 w-3.5 shrink-0 text-fg-muted" />
          <input
            type="text"
            data-testid="result-find-replacement"
            className="h-6 w-44 rounded-md bg-surface px-2 text-xs text-fg placeholder:text-fg-muted focus:outline-none focus:ring-1 focus:ring-accent"
            placeholder={t('resultFind.replacementPlaceholder')}
            value={replacement}
            disabled={busy}
            onChange={(event) => onReplacementChange(event.target.value)}
          />
          <span className="shrink-0 text-[10px] text-fg-muted" data-testid="result-find-plan-count">
            {t('resultFind.planCount', { count: plan.length })}
          </span>
          <button
            type="button"
            data-testid="result-find-replace-next"
            disabled={busy || plan.length === 0}
            onClick={handleReplaceNext}
            className="flex h-6 items-center gap-1 rounded-md border border-edge px-1.5 text-[10px] text-fg-secondary hover:bg-surface-raised disabled:opacity-50"
          >
            <Replace className="h-3 w-3" />
            {t('resultFind.replaceNext')}
          </button>
          <button
            type="button"
            data-testid="result-find-replace-all"
            disabled={busy || plan.length === 0}
            onClick={() => void handleReplaceAll()}
            className="flex h-6 items-center gap-1 rounded-md border border-edge px-1.5 text-[10px] text-fg-secondary hover:bg-surface-raised disabled:opacity-50"
          >
            <ReplaceAll className="h-3 w-3" />
            {t('resultFind.replaceAll')}
          </button>
          {error && (
            <span
              role="alert"
              data-testid="result-find-error"
              className="truncate text-[10px] text-danger"
              title={error}
            >
              {error}
            </span>
          )}
        </div>
      ) : (
        replaceReason && (
          <p className="text-[10px] text-fg-muted" data-testid="result-find-replace-blocked">
            {replaceReason}
          </p>
        )
      )}
    </div>
  );
}
