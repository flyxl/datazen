/**
 * One history row.
 *
 * Actions are wired through `data-*` test ids on stable identifiers (entry id
 * and action name) rather than positional or geometry-based lookups, so a test
 * can target "the delete button on the row whose id is X" without depending on
 * render order.
 */

import { Check, Copy, ExternalLink, Star, Trash2 } from 'lucide-react';
import { CopyableError } from '../ui/CopyableError';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import type { QueryHistoryEntry } from '../../types';

export type HistoryEntryAction = 'copy' | 'open' | 'delete' | 'favorite';

export interface HistoryEntryCardProps {
  entry: QueryHistoryEntry;
  connectionName: string;
  selected: boolean;
  onToggleSelect: (id: string) => void;
  onAction: (action: HistoryEntryAction, entry: QueryHistoryEntry) => void;
  copiedId: string | null;
  canOpen: boolean;
}

export function HistoryEntryCard({
  entry,
  connectionName,
  selected,
  onToggleSelect,
  onAction,
  copiedId,
  canOpen,
}: Readonly<HistoryEntryCardProps>) {
  const { t } = useI18n();
  const copied = copiedId === entry.id;

  const button = (
    action: HistoryEntryAction,
    label: string,
    title: string,
    node: React.ReactNode,
  ) => (
    <button
      type="button"
      onClick={() => onAction(action, entry)}
      className="flex items-center gap-1 rounded px-2 py-0.5 text-[11px] text-fg-muted hover:bg-surface hover:text-accent transition-colors"
      title={title}
      data-history-action={action}
      data-testid={`global-history-${action}`}
    >
      {node}
      <span>{label}</span>
    </button>
  );

  return (
    <div
      className={cn(
        'group flex flex-col gap-1.5 rounded-lg border p-3 transition-colors',
        selected
          ? 'border-accent/60 bg-accent/5'
          : 'border-edge/80 bg-surface-alt hover:border-accent/40 hover:bg-surface-raised',
      )}
      data-history-id={entry.id}
      data-testid="global-history-item"
    >
      <div className="flex items-center justify-between text-[11px] text-fg-muted">
        <div className="flex items-center gap-2 min-w-0">
          <input
            type="checkbox"
            checked={selected}
            onChange={() => onToggleSelect(entry.id)}
            className="h-3.5 w-3.5 shrink-0 accent-[var(--color-accent)]"
            aria-label={t('query.historySelectAll')}
            data-testid="global-history-select"
          />
          <span
            className={cn(
              'h-1.5 w-1.5 rounded-full shrink-0',
              entry.success ? 'bg-success' : 'bg-danger',
            )}
          />
          <span className="font-semibold text-fg-secondary truncate">{connectionName}</span>
          <span>·</span>
          <span className="truncate">{entry.database || t('query.historyDefaultDb')}</span>
          {entry.schema && (
            <>
              <span>·</span>
              <span className="truncate">{entry.schema}</span>
            </>
          )}
          <span>·</span>
          <span>{entry.executionTimeMs}ms</span>
          {entry.rowsAffected != null && (
            <>
              <span>·</span>
              <span>{t('query.historyRows', { count: entry.rowsAffected })}</span>
            </>
          )}
          <span>·</span>
          <span className="text-[10px] text-fg-muted/70">
            {new Date(entry.executedAt).toLocaleString()}
          </span>
        </div>

        <div className="flex items-center gap-1.5 opacity-90 group-hover:opacity-100 shrink-0">
          {button(
            'copy',
            copied ? t('query.historyCopied') : t('query.historyCopy'),
            t('query.historyCopyTitle'),
            copied ? <Check className="h-3 w-3 text-success" /> : <Copy className="h-3 w-3" />,
          )}

          {button(
            'favorite',
            t('query.historyFavorite'),
            t('query.historyFavoriteTitle'),
            <Star className="h-3 w-3" />,
          )}

          {canOpen &&
            button(
              'open',
              t('query.historyOpen'),
              t('query.historyOpenTitle'),
              <ExternalLink className="h-3 w-3" />,
            )}

          {button('delete', '', t('query.historyDeleteTitle'), <Trash2 className="h-3 w-3" />)}
        </div>
      </div>

      <pre className="max-h-24 overflow-x-auto rounded bg-surface/70 p-2 font-mono text-xs text-fg-secondary whitespace-pre-wrap break-all">
        {entry.sql}
      </pre>

      {entry.errorMessage && (
        <CopyableError
          message={entry.errorMessage}
          copyButton
          className="rounded bg-danger/10 px-2 py-1 text-[11px] text-danger"
        />
      )}
    </div>
  );
}
