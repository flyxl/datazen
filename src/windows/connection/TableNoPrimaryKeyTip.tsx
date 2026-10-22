/**
 * "This table has no primary key" hint, extracted from `TableView.tsx` (DB-03 pushed that
 * file over its size ceiling). The markup is moved verbatim; the condition stays with the
 * caller, because it is the caller that knows the effective order.
 */
import { Info } from 'lucide-react';

export function TableNoPrimaryKeyTip({ message }: { message: string }) {
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
    </div>
  );
}
