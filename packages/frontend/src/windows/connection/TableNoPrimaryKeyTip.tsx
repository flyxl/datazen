/**
 * "This table has no primary key" hint, extracted from `TableView.tsx` (DB-03 pushed that
 * file over its size ceiling). The markup is moved verbatim; the condition stays with the
 * caller, because it is the caller that knows the effective order.
 *
 * DB-16 adds the optional escape hatch: on a table whose grid cannot address rows, the
 * banner carries an "Edit in SQL" action that opens a query tab with a `SELECT … WHERE`
 * draft. A caller that passes no action renders exactly what this component shipped
 * before DB-16 — that byte-for-byte shape is the regression gate for every existing
 * consumer, so the button markup lives strictly behind the prop.
 */
import { Info } from 'lucide-react';

/** The optional action slot of the banner (DB-16). */
export interface TableNoPrimaryKeyTipAction {
  /** Menu/aria label of the action, already translated. */
  label: string;
  /** What clicking it does; a query tab opens with the no-PK draft. */
  onClick: () => void;
}

export function TableNoPrimaryKeyTip({
  message,
  action,
}: {
  message: string;
  action?: TableNoPrimaryKeyTipAction;
}) {
  return (
    <div
      className="flex shrink-0 items-center gap-1.5 border-b border-edge bg-surface-alt px-3 py-1.5 text-xs text-fg-muted"
      role="status"
      data-testid="table-no-primary-key-tip"
    >
      <Info className="h-3.5 w-3.5 shrink-0" />
      <span className="min-w-0 truncate" title={message}>
        {message}
      </span>
      {action && (
        <button
          type="button"
          className="ml-auto shrink-0 rounded border border-edge px-1.5 py-0.5 text-xs text-fg hover:bg-surface"
          onClick={action.onClick}
          data-testid="table-no-primary-key-edit-in-sql"
        >
          {action.label}
        </button>
      )}
    </div>
  );
}
