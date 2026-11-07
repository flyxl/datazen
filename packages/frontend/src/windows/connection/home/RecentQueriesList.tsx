import { useCallback, useState } from 'react';
import { Check, ChevronRight, Clock, Copy, Play } from 'lucide-react';
import { useI18n } from '../../../hooks/useI18n';
import { useCopyFeedback } from '../../../components/ui/useCopyFeedback';
import { cn } from '../../../lib/cn';
import { getRelativeTimeParts, RELATIVE_WINDOW_MS } from '../../../lib/relativeTime';
import type { ConnectionConfig, QueryHistoryEntry } from '../../../types';

const MAX_VISIBLE = 5;

/** How long the per-row "copied" marker stays before reverting. */
const COPIED_FEEDBACK_MS = 2000;

/**
 * Builds the localized relative-time label for a history timestamp from the
 * structured {value, unit} parts returned by the lib. Returns null for
 * invalid timestamps (the segment is then omitted entirely). Falls back to a
 * short locale date beyond the 7-day relative window.
 */
function useRelativeTimeLabel() {
  const { t } = useI18n();
  return (timestamp: number): string | null => {
    if (!Number.isFinite(timestamp) || timestamp <= 0) {
      return null;
    }
    const now = Date.now();
    const parts = getRelativeTimeParts(timestamp, now);
    if (!parts) {
      return null;
    }
    if (parts.value === 0) {
      // Sub-minute delta (or near-future clock skew).
      return t('connWin.home.queries.justNow');
    }
    if (now - timestamp > RELATIVE_WINDOW_MS) {
      return new Date(timestamp).toLocaleDateString(undefined, {
        year: 'numeric',
        month: 'short',
        day: 'numeric',
      });
    }
    const count = Math.abs(parts.value);
    if (parts.unit === 'minute') {
      return t('connWin.home.queries.minutesAgo', { count });
    }
    if (parts.unit === 'hour') {
      return t('connWin.home.queries.hoursAgo', { count });
    }
    return t('connWin.home.queries.daysAgo', { count });
  };
}

interface RecentQueriesListProps {
  entries: QueryHistoryEntry[];
  savedConnections: ConnectionConfig[];
  onSelectHistoryQuery?: (entry: QueryHistoryEntry) => void;
  onOpenHistory: () => void;
}

/**
 * "Recent queries" section for the landing page: five rows with SQL, source
 * connection name, relative time and duration; hover reveals Re-run / Copy.
 */
export function RecentQueriesList({
  entries,
  savedConnections,
  onSelectHistoryQuery,
  onOpenHistory,
}: RecentQueriesListProps) {
  const { t } = useI18n();
  const [copiedSqlId, setCopiedSqlId] = useState<string | null>(null);
  const { copied, copy } = useCopyFeedback(COPIED_FEEDBACK_MS);
  const formatRelativeLabel = useRelativeTimeLabel();

  /**
   * `copied` is the shared, request-bound flag and `copiedSqlId` names the row
   * it belongs to; gating on both keeps a rolled-back write from leaving a
   * stale "copied" marker on the last attempted row.
   */
  const copiedRowId = copied ? copiedSqlId : null;

  const handleCopy = useCallback(
    (id: string, sql: string) => {
      copy(sql);
      setCopiedSqlId(id);
    },
    [copy],
  );

  const connectionNameById = new Map(savedConnections.map((c) => [c.id, c.name] as const));

  return (
    <section className="flex flex-col gap-2.5" data-testid="home-recent-queries-section">
      <div className="flex h-5 items-center justify-between px-0.5">
        <h3 className="text-xs font-semibold uppercase tracking-wider text-fg-muted">
          {t('connWin.home.recentQueries')}
        </h3>
        <button
          type="button"
          data-testid="view-all-history-button"
          onClick={onOpenHistory}
          className="inline-flex items-center gap-1 text-xs text-accent hover:underline"
        >
          <span>{t('connWin.home.viewAll')}</span>
          <ChevronRight className="h-3 w-3" />
        </button>
      </div>

      <div className="flex flex-col rounded-xl border border-edge bg-surface-alt p-4">
        {entries.length === 0 ? (
          <div className="flex flex-1 flex-col items-center justify-center text-center text-fg-muted">
            <Clock className="mb-2 h-6 w-6 opacity-40" />
            <p className="text-xs">{t('connWin.home.noRecentQueries')}</p>
          </div>
        ) : (
          <div className="flex flex-1 flex-col gap-2">
            {entries.slice(0, MAX_VISIBLE).map((item) => {
              const sourceName =
                connectionNameById.get(item.connectionId) || item.database || 'default';
              const relative = formatRelativeLabel(new Date(item.executedAt).getTime());

              return (
                <div
                  key={item.id}
                  role="button"
                  tabIndex={0}
                  onClick={() => onSelectHistoryQuery?.(item)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      onSelectHistoryQuery?.(item);
                    }
                  }}
                  className="group flex cursor-pointer flex-col gap-1 rounded-lg border border-edge/70 bg-surface p-2.5 text-left transition-colors hover:border-accent/40 hover:bg-surface-raised"
                >
                  <div className="flex items-center justify-between gap-2 text-[11px] text-fg-muted">
                    <div className="flex min-w-0 items-center gap-1.5 font-medium">
                      <span
                        className={cn(
                          'h-1.5 w-1.5 shrink-0 rounded-full',
                          item.success ? 'bg-success' : 'bg-danger',
                        )}
                      />
                      <span
                        className="truncate"
                        title={sourceName}
                        data-testid={`home-query-source-${item.id}`}
                      >
                        {sourceName}
                      </span>
                      {relative && (
                        <>
                          <span>·</span>
                          <span className="shrink-0">{relative}</span>
                        </>
                      )}
                      <span>·</span>
                      <span className="shrink-0">{item.executionTimeMs}ms</span>
                    </div>

                    <div className="flex shrink-0 items-center gap-1">
                      {onSelectHistoryQuery && (
                        <button
                          type="button"
                          data-testid={`home-query-rerun-${item.id}`}
                          onClick={(e) => {
                            e.stopPropagation();
                            onSelectHistoryQuery(item);
                          }}
                          className="flex items-center gap-1 text-[11px] text-fg-muted opacity-0 transition-opacity hover:text-accent focus-visible:opacity-100 group-hover:opacity-100"
                        >
                          <Play className="h-3 w-3 fill-current" />
                          <span>{t('connWin.home.queries.rerun')}</span>
                        </button>
                      )}
                      <button
                        type="button"
                        data-testid={`home-query-copy-${item.id}`}
                        onClick={(e) => {
                          e.stopPropagation();
                          handleCopy(item.id, item.sql);
                        }}
                        className="flex items-center gap-1 text-[11px] text-fg-muted hover:text-accent"
                      >
                        {copiedRowId === item.id ? (
                          <>
                            <Check className="h-3 w-3 text-success" />
                            <span className="text-success">
                              {t('connWin.home.aiIntegration.copied')}
                            </span>
                          </>
                        ) : (
                          <>
                            <Copy className="h-3 w-3" />
                            <span>{t('connWin.home.aiIntegration.copy')}</span>
                          </>
                        )}
                      </button>
                    </div>
                  </div>
                  <div className="truncate font-mono text-xs text-fg-secondary transition-colors group-hover:text-fg">
                    {item.sql}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </section>
  );
}
