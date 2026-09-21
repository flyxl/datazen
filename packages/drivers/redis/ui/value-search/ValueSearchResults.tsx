import { Loader2, XCircle } from 'lucide-react';
import { Button, cn } from '@datazen/ui';
import { useI18n } from '@datazen/ui';
import type { ValueSearchState } from './useValueSearch';

export interface ValueSearchResultsProps {
  state: ValueSearchState;
  onSelectKey: (key: string) => void;
  onCancel: () => void;
}

function pct(scanned: number, max: number): number {
  if (max <= 0) return 0;
  return Math.min(100, Math.round((scanned / max) * 100));
}

/** Progress + hit list for a running/finished value search (R6). */
export function ValueSearchResults({ state, onSelectKey, onCancel }: ValueSearchResultsProps) {
  const { t } = useI18n();
  const running = state.status === 'running';

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 items-center gap-3 border-b border-edge bg-surface-alt px-3 py-1.5 text-xs text-fg-secondary">
        <div className="h-1.5 w-40 overflow-hidden rounded-full bg-surface-raised">
          <div
            className="h-full bg-accent transition-[width] duration-200"
            style={{ width: `${pct(state.scanned, state.maxKeys)}%` }}
          />
        </div>
        <span className="font-mono">
          {state.scanned.toLocaleString()} / {state.maxKeys.toLocaleString()}
        </span>
        <span>{t('redis.search.hits').replace('{count}', String(state.hits.length))}</span>
        {state.limitHit && (
          <span className="rounded bg-amber-500/15 px-1.5 py-0.5 text-[10px] font-medium text-amber-400">
            {t('redis.search.limitHit')}
          </span>
        )}
        {state.status === 'cancelled' && (
          <span className="text-[10px] text-fg-muted">{t('redis.search.cancelled')}</span>
        )}
        <div className="flex-1" />
        {running && (
          <Button
            variant="ghost"
            className="h-6 gap-1 px-2 text-xs text-danger"
            data-testid="redis-search-cancel"
            onClick={onCancel}
          >
            <XCircle className="h-3.5 w-3.5" />
            {t('redis.search.cancel')}
          </Button>
        )}
      </div>

      <div className="min-h-0 flex-1 overflow-auto">
        {state.error && (
          <div className="m-3 rounded-md border border-danger/20 bg-danger/10 px-3 py-2 text-sm text-danger">
            {state.error}
          </div>
        )}

        {running && state.hits.length === 0 && !state.error && (
          <div className="flex items-center gap-2 p-4 text-sm text-fg-muted">
            <Loader2 className="h-4 w-4 animate-spin" />
            {t('redis.search.scanning')}
          </div>
        )}

        {!running && state.hits.length === 0 && !state.error && (
          <div className="p-4 text-sm text-fg-muted">{t('redis.search.noResults')}</div>
        )}

        <ul className="divide-y divide-edge">
          {state.hits.map((hit) => (
            <li key={`${hit.matchedIn}:${hit.key}`}>
              <button
                type="button"
                data-testid="redis-search-hit"
                onClick={() => onSelectKey(hit.key)}
                className="flex w-full flex-col items-start gap-0.5 px-3 py-2 text-left hover:bg-surface-raised"
              >
                <div className="flex w-full items-center gap-2">
                  <span className="min-w-0 flex-1 truncate font-mono text-xs text-fg">
                    {hit.key}
                  </span>
                  <span
                    className={cn(
                      'shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium',
                      hit.matchedIn === 'key'
                        ? 'bg-accent/15 text-accent'
                        : 'bg-emerald-500/15 text-emerald-400',
                    )}
                  >
                    {hit.matchedIn === 'key'
                      ? t('redis.search.modeKey')
                      : t('redis.search.modeValue')}
                  </span>
                </div>
                <span className="w-full truncate font-mono text-[11px] text-fg-muted">
                  {hit.preview}
                </span>
              </button>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
