import type { ReactNode } from 'react';

export interface ClauseRowProps {
  /**
   * SQL keyword shown in the gutter (SELECT, FROM, WHERE, GROUP BY, HAVING,
   * ORDER BY). Deliberately not translated: these are SQL, not prose.
   */
  label: string;
  /** Clause content — chips, condition rows, join list. */
  children: ReactNode;
  /** Tooltip on the keyword gutter (e.g. what HAVING accepts). */
  title?: string;
  /** Extra row rendered under the ClauseRow's own content (LIMIT/OFFSET). */
  testId: string;
}

/**
 * One clause of the statement list, Navicat style: a keyword gutter on the
 * left, content flowing to the right.
 *
 * The gutter is a fixed 76px so the keywords line up into a readable column,
 * and each clause takes only the height of its own content — which is what
 * leaves room for WHERE / GROUP BY / HAVING / ORDER BY below the SELECT list.
 */
export function ClauseRow({ label, children, title, testId }: ClauseRowProps) {
  return (
    <div className="flex min-w-0 items-start gap-3 px-3 py-0.5" data-testid={testId}>
      <div className="w-[72px] shrink-0 pt-0.5 text-[12px] font-semibold text-accent" title={title}>
        {label}
      </div>
      <div className="flex min-w-0 flex-1 flex-col gap-1">{children}</div>
    </div>
  );
}
