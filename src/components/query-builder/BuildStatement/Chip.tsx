import type { ReactNode } from 'react';
import { X } from 'lucide-react';
import { cn } from '@datazen/ui';

export interface ChipProps {
  /** Chip text, e.g. `fa.actor_id`. */
  label: string;
  /**
   * Small muted marker rendered before the label — the `AND` / `OR` that links
   * a condition to the one above it.
   */
  badge?: string;
  /** Rendered before the label, e.g. `SUM(`. */
  prefix?: string;
  /** Rendered after the label, e.g. `)` or ` ASC`. */
  suffix?: string;
  /** Dimmed style for "part of the query but not the selected column". */
  muted?: boolean;
  title?: string;
  onClick?: () => void;
  onRemove?: () => void;
  removeTitle?: string;
  testId: string;
  removeTestId?: string;
}

/**
 * One item of a clause (a SELECT field, a FROM table, a condition, a GROUP BY
 * key, an ORDER BY key).
 *
 * The chip carries the entire per-item affordance surface: click to open its
 * options, × to drop it from the clause. That is what keeps every clause row to
 * a single line per item instead of the old grid's eight columns of controls —
 * and what makes all six clauses look and behave the same way.
 */
export function Chip({
  label,
  badge,
  prefix,
  suffix,
  muted = false,
  title,
  onClick,
  onRemove,
  removeTitle,
  testId,
  removeTestId,
}: ChipProps) {
  const body: ReactNode = (
    <>
      {prefix && <span className="text-fg-muted">{prefix}</span>}
      <span className="truncate">{label}</span>
      {suffix && <span className="text-fg-muted">{suffix}</span>}
    </>
  );

  return (
    <span
      className={cn(
        'group inline-flex max-w-[320px] items-center gap-0.5 rounded-[7px] border border-edge bg-surface-inset py-0.5 pl-2 pr-1 text-[12px]',
        muted ? 'text-fg-secondary' : 'text-fg',
        onClick && 'cursor-pointer hover:border-accent',
      )}
      title={title}
      data-testid={testId}
    >
      {badge && (
        <span className="mr-0.5 shrink-0 text-[10px] font-semibold text-fg-muted">{badge}</span>
      )}
      {onClick ? (
        <button
          type="button"
          onClick={onClick}
          className="flex min-w-0 items-center gap-0.5 text-left outline-none"
        >
          {body}
        </button>
      ) : (
        <span className="flex min-w-0 items-center gap-0.5">{body}</span>
      )}
      {onRemove && (
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
          title={removeTitle}
          className="shrink-0 rounded p-0.5 text-fg-muted transition-colors hover:text-danger"
          data-testid={removeTestId}
        >
          <X className="h-3 w-3" />
        </button>
      )}
    </span>
  );
}
