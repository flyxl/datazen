/**
 * The paste outcome line under the grid (DB-03).
 *
 * One line, because paste is a high-frequency action and a modal summary after every paste
 * would be in the way. It says three things and nothing else: how many cells landed, how many
 * were skipped **and why** (the first reason, with a count), and whether staged changes were
 * overwritten. It never repeats a cell value — pasted content is user data.
 */
import { useI18n } from '../../hooks/useI18n';
import { describePasteProblem } from '../../hooks/useGridClipboard';
import type { TableClipboardBridge } from './useTableViewClipboard';

export function TableClipboardStatus({ bridge }: { bridge: TableClipboardBridge }) {
  const { t } = useI18n();
  const { lastPaste, problems, clear, confirmElement } = bridge;
  const firstProblem = problems[0];
  // "Pasted N cell(s)" is about cells that moved. Reverting staged edits moves none, so the
  // two conditions are deliberately separate: folding them together renders "Pasted 0 cell(s)".
  const hasResult = lastPaste !== null && (lastPaste.staged > 0 || lastPaste.skipped > 0);
  // `overwrittenPending` alone is still a real outcome worth a line: pasting the original
  // values back reverts staged edits without staging or skipping anything (§3.4).
  const visible =
    hasResult ||
    (lastPaste !== null && lastPaste.overwrittenPending > 0) ||
    firstProblem !== undefined;

  return (
    <>
      {confirmElement}
      {visible ? (
        <div
          data-testid="table-clipboard-status"
          data-dt-paste-staged={String(lastPaste?.staged ?? 0)}
          data-dt-paste-skipped={String(lastPaste?.skipped ?? 0)}
          role="status"
          className="flex shrink-0 items-center gap-2 border-b border-edge bg-surface-alt px-3 py-1 text-xs text-fg-secondary"
        >
          {hasResult ? (
            <span>
              {t('dataTable.pasteResult', {
                count: lastPaste?.staged ?? 0,
                skipped: lastPaste?.skipped ?? 0,
              })}
            </span>
          ) : null}
          {lastPaste !== null && lastPaste.overwrittenPending > 0 ? (
            <span className="text-fg-muted">
              {t('dataTable.pasteOverwrotePending', { count: lastPaste.overwrittenPending })}
            </span>
          ) : null}
          {firstProblem ? (
            <span className="min-w-0 truncate" title={describePasteProblem(firstProblem, t)}>
              {describePasteProblem(firstProblem, t)}
              {problems.length > 1 ? ` (+${problems.length - 1})` : ''}
            </span>
          ) : null}
          <button
            type="button"
            data-testid="table-clipboard-status-dismiss"
            className="ml-auto rounded px-1.5 py-0.5 text-fg-muted hover:bg-surface-raised hover:text-fg"
            onClick={clear}
          >
            ×
          </button>
        </div>
      ) : null}
    </>
  );
}
