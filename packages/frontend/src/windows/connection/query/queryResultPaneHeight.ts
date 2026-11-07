import type { CSSProperties } from 'react';
import type { StatementResult } from '../../../types';
import { isMutationExecution } from '../result-workspace/isMutationExecution';
import type { ResultWorkspaceView } from '../result-workspace/resultWorkspaceHelpers';

/**
 * Metrics used to estimate how tall the query result pane wants to be.
 *
 * The estimate is deliberately approximate: the pane is capped at half of the
 * query content area by CSS (`max-height: 50%`), so these numbers only decide
 * what happens *below* that cap — a handful of rows should not reserve half a
 * screen. They mirror the concrete sizes the result surface renders with:
 * `DataTable`'s `h-10` header, `ResultTableView`'s `rowHeight={32}` and its
 * status bar, and the multi-statement tab strip.
 */
const TABLE_HEADER_HEIGHT = 41; // `h-10` (40) + 1px border
const TABLE_ROW_HEIGHT = 32; // ResultTableView rowHeight
const TABLE_STATUS_BAR_HEIGHT = 29; // `py-1.5` + text-xs line-height + 1px border
const TABLE_EMPTY_BODY_HEIGHT = 112; // DataTable's empty placeholder (`min-h-28`)
const TABLE_NO_ROWS_HINT_HEIGHT = 44; // ResultTableView's extra "no rows" footer
const RESULT_TAB_STRIP_HEIGHT = 29; // one row of "结果 N" tabs + border
const CHART_CONTENT_HEIGHT = 320; // a chart needs a real plotting area
const SUMMARY_CONTENT_HEIGHT = 320; // ExecutionSummaryCard
const EXPLAIN_CONTENT_HEIGHT = 320; // ExplainPanel
const ERROR_CONTENT_HEIGHT = 320; // QueryErrorPanel + its retry / fix actions
const DIAGNOSIS_CONTENT_HEIGHT = 320; // DiagnosisPanel

/**
 * Smallest height the result pane is allowed to shrink to while the results
 * themselves matter (the pane never disappears once a statement ran).
 */
export const RESULT_PANE_MIN_HEIGHT = 140;

export interface QueryResultPaneHeightInput {
  /** Active statement result; `null` before anything was executed. */
  result?: StatementResult | null;
  /** View actually rendered — resolve it first so a chart fallback is honoured. */
  view: ResultWorkspaceView;
  /** Whether the multi-statement tab strip is shown above the result. */
  hasResultTabs: boolean;
  /** The statement failed: the pane shows the error panel, not a result. */
  hasError: boolean;
  /** Explain output replaces the result surface. */
  showExplain: boolean;
  /** The AI diagnosis panel is docked under the error panel. */
  showDiagnosis: boolean;
}

/**
 * Estimate the height the query result pane needs for its current content.
 *
 * Callers feed this into the pane's `height` while the editor/results split is
 * still automatic; the `max-height: 50%` cap and `min-height` floor are applied
 * by the caller, so this only has to be right in the "little data" regime.
 */
export function estimateQueryResultPaneHeight({
  result,
  view,
  hasResultTabs,
  hasError,
  showExplain,
  showDiagnosis,
}: QueryResultPaneHeightInput): number {
  const chrome = hasResultTabs ? RESULT_TAB_STRIP_HEIGHT : 0;

  if (showExplain) return chrome + EXPLAIN_CONTENT_HEIGHT;
  // The error panel and the diagnosis dock stack inside one scroll container.
  if (hasError) {
    return chrome + ERROR_CONTENT_HEIGHT + (showDiagnosis ? DIAGNOSIS_CONTENT_HEIGHT : 0);
  }
  if (showDiagnosis) return chrome + DIAGNOSIS_CONTENT_HEIGHT;
  if (!result) return RESULT_PANE_MIN_HEIGHT;
  if (isMutationExecution(result)) return chrome + SUMMARY_CONTENT_HEIGHT;
  if (view === 'chart') return chrome + CHART_CONTENT_HEIGHT;

  const rowCount = result.rows.length;
  const body =
    rowCount === 0
      ? TABLE_EMPTY_BODY_HEIGHT + TABLE_NO_ROWS_HINT_HEIGHT
      : rowCount * TABLE_ROW_HEIGHT;

  // `Math.max` here (rather than a CSS `min-height` on the host) is deliberate:
  // the floor has to give way when the panel itself is smaller than it, so the
  // pane never overflows the query content area.
  return Math.max(
    RESULT_PANE_MIN_HEIGHT,
    chrome + TABLE_STATUS_BAR_HEIGHT + TABLE_HEADER_HEIGHT + body,
  );
}

/**
 * Inline sizing for the element hosting the result pane.
 *
 * Automatic sizing claims only `contentHeight` — already floored at
 * `RESULT_PANE_MIN_HEIGHT` by {@link estimateQueryResultPaneHeight} — and is
 * capped at half of the query content area, so the editor keeps at least the
 * other half while results are on screen. Once the user has pinned the split,
 * the pane just fills whatever is left and this returns `undefined`.
 */
export function queryResultPaneSizing(
  pinned: boolean,
  contentHeight: number,
): CSSProperties | undefined {
  if (pinned) return undefined;

  return {
    height: contentHeight,
    maxHeight: '50%',
  };
}
