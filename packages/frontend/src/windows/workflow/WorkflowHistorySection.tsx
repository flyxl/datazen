import { Clock, X } from 'lucide-react';
import { cn } from '../../lib/cn';
import type { TranslationKey } from '../../locales';
import type { HistoryListItem, WorkflowExecutionResult } from '../../types';
import { WorkflowExecutionResultPanel } from './WorkflowExecutionResultPanel';

type TFn = (key: TranslationKey, params?: Record<string, string | number>) => string;

/** Sidebar history list (WorkflowPage). Opens detail in a separate panel via `onView`. */
export function WorkflowHistoryList({
  items,
  onView,
  onClear,
  t,
}: {
  items: HistoryListItem[];
  onView: (id: string, workflowName: string) => void;
  onClear: () => void;
  t: TFn;
}) {
  return (
    <div className="p-1.5">
      {items.length > 0 && (
        <div className="flex justify-end mb-1">
          <button
            type="button"
            onMouseDown={(e) => e.preventDefault()}
            onClick={onClear}
            className="text-[10px] text-fg-muted hover:text-red-400"
          >
            {t('workflows.history.clear')}
          </button>
        </div>
      )}
      {items.map((item) => (
        <button
          key={item.id}
          type="button"
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => onView(item.id, item.workflowName)}
          onContextMenu={(e) => {
            e.preventDefault();
            e.stopPropagation();
          }}
          className="flex w-full items-center gap-2 rounded-md px-3 py-2 text-left hover:bg-surface-raised/50 transition-colors"
        >
          <Clock className="h-3.5 w-3.5 text-fg-muted shrink-0" />
          <div className="flex-1 min-w-0">
            <div className="text-xs font-medium text-fg truncate">{item.workflowName}</div>
            <div className="text-[10px] text-fg-muted">
              {new Date(item.createdAt).toLocaleString()}
            </div>
          </div>
          <span className={cn('text-[10px]', item.success ? 'text-green-500' : 'text-red-400')}>
            {item.success ? '✓' : '✗'}
          </span>
        </button>
      ))}
      {items.length === 0 && (
        <div className="py-6 text-center text-xs text-fg-muted">{t('workflows.history.empty')}</div>
      )}
    </div>
  );
}

/** Inline history tab (WorkflowPanel): list with optional detail + back navigation. */
export function WorkflowHistoryTab({
  items,
  detail,
  onView,
  onClear,
  onCloseDetail,
  t,
}: {
  items: HistoryListItem[];
  detail: WorkflowExecutionResult | null;
  onView: (id: string) => void;
  onClear: () => void;
  onCloseDetail: () => void;
  t: TFn;
}) {
  if (detail) {
    return (
      <div>
        <button
          type="button"
          onMouseDown={(e) => e.preventDefault()}
          onClick={onCloseDetail}
          className="flex items-center gap-1 text-xs text-accent hover:underline mb-2"
        >
          <X className="h-3 w-3" />
          {t('workflows.history.back')}
        </button>
        <WorkflowExecutionResultPanel result={detail} t={t} />
      </div>
    );
  }

  return (
    <div className="space-y-2">
      {items.length > 0 && (
        <button
          type="button"
          onMouseDown={(e) => e.preventDefault()}
          onClick={onClear}
          className="text-[11px] text-red-400 hover:underline"
        >
          {t('workflows.history.clear')}
        </button>
      )}

      {items.length === 0 ? (
        <div className="py-4 text-center text-xs text-fg-muted">{t('workflows.history.empty')}</div>
      ) : (
        items.map((item) => (
          <div
            key={item.id}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => onView(item.id)}
            className="flex items-center justify-between px-3 py-2 rounded-md hover:bg-surface-raised text-xs cursor-pointer transition-colors"
          >
            <div className="min-w-0 flex-1">
              <div className="font-medium text-fg">{item.workflowName}</div>
              <div className="flex items-center gap-2 text-[10px] text-fg-muted mt-0.5">
                <Clock className="h-3 w-3 shrink-0" />
                {new Date(item.createdAt).toLocaleString()}
                <span className="text-fg-muted">{item.totalTimeMs}ms</span>
              </div>
            </div>
            <span
              className={`shrink-0 ml-2 text-[11px] font-medium ${item.success ? 'text-green-500' : 'text-red-400'}`}
            >
              {item.success ? '✓' : '✗'}
            </span>
          </div>
        ))
      )}
    </div>
  );
}
