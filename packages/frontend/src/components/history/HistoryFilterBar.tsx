/**
 * Filter row for the global query-history dialog.
 *
 * Every control here maps to a predicate the backend can evaluate, so a filter
 * narrows the query rather than the page it already fetched. Purely presentational:
 * it owns no state, which keeps the "am I showing everything" decision in one
 * place (`GlobalQueryHistoryDialog`).
 */

import { useMemo } from 'react';
import { Search } from 'lucide-react';
import { Select } from '../ui/Select';
import { useI18n } from '../../hooks/useI18n';
import {
  DEFAULT_HISTORY_QUERY,
  type HistoryQueryState,
  type HistoryRange,
  type HistorySort,
} from './historyQuery';

export interface HistoryFilterBarProps {
  value: HistoryQueryState;
  onChange: (next: HistoryQueryState) => void;
  connections: { id: string; name: string }[];
  /** Distinct database / schema names found in the currently loaded page. */
  databases: string[];
  schemas: string[];
}

export function HistoryFilterBar({
  value,
  onChange,
  connections,
  databases,
  schemas,
}: Readonly<HistoryFilterBarProps>) {
  const { t } = useI18n();

  const set = <K extends keyof HistoryQueryState>(key: K, next: HistoryQueryState[K]) =>
    onChange({ ...value, [key]: next });

  /**
   * Switching connection invalidates the database and schema choices, so they
   * reset rather than silently filtering by a name from another connection.
   */
  const setConnection = (connectionId: string) =>
    onChange({ ...value, connectionId, database: '', schema: '' });

  const connectionOptions = useMemo(
    () => [
      { value: 'all', label: t('query.historyScopeAll') },
      ...connections.map((c) => ({ value: c.id, label: c.name })),
    ],
    [connections, t],
  );

  return (
    <div className="flex flex-wrap items-center gap-2 border-b border-edge pb-3">
      <div className="relative flex-1 min-w-[180px]">
        <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 h-4 w-4 text-fg-muted" />
        <input
          type="text"
          placeholder={t('query.searchHistory')}
          value={value.search}
          onChange={(e) => set('search', e.target.value)}
          className="h-8 w-full rounded-md border border-edge bg-surface pl-8 pr-3 text-xs text-fg outline-none focus:border-accent"
          data-testid="global-history-search"
        />
      </div>

      <div className="w-32">
        <Select
          value={value.connectionId}
          onChange={setConnection}
          className="h-8 text-xs"
          options={connectionOptions}
        />
      </div>

      {/* Database / schema only make sense once a connection is chosen. */}
      {value.connectionId !== 'all' && databases.length > 0 && (
        <div className="w-28">
          <Select
            value={value.database || 'all'}
            onChange={(val) => set('database', val === 'all' ? '' : val)}
            className="h-8 text-xs"
            options={[
              { value: 'all', label: t('query.historyAllDatabases') },
              ...databases.map((d) => ({ value: d, label: d })),
            ]}
          />
        </div>
      )}

      {value.connectionId !== 'all' && schemas.length > 0 && (
        <div className="w-28">
          <Select
            value={value.schema || 'all'}
            onChange={(val) => set('schema', val === 'all' ? '' : val)}
            className="h-8 text-xs"
            options={[
              { value: 'all', label: t('query.historyAllSchemas') },
              ...schemas.map((s) => ({ value: s, label: s })),
            ]}
          />
        </div>
      )}

      <div className="w-28" data-testid="global-history-status">
        <Select
          value={value.status}
          onChange={(val) => set('status', val as HistoryQueryState['status'])}
          className="h-8 text-xs"
          options={[
            { value: 'all', label: t('query.historyStatusAll') },
            { value: 'success', label: t('query.historyStatusSuccess') },
            { value: 'failed', label: t('query.historyStatusFailed') },
          ]}
        />
      </div>

      <div className="w-32" data-testid="global-history-range">
        <Select
          value={value.range}
          onChange={(val) => set('range', val as HistoryRange)}
          className="h-8 text-xs"
          options={[
            { value: 'all', label: t('query.historyRangeAll') },
            { value: 'today', label: t('query.historyRangeToday') },
            { value: '7d', label: t('query.historyRange7d') },
            { value: '30d', label: t('query.historyRange30d') },
            { value: '90d', label: t('query.historyRange90d') },
          ]}
        />
      </div>

      <div className="w-32" data-testid="global-history-sort">
        <Select
          value={value.sort}
          onChange={(val) => set('sort', val as HistorySort)}
          className="h-8 text-xs"
          options={[
            { value: 'recent', label: t('query.historySortRecent') },
            { value: 'oldest', label: t('query.historySortOldest') },
            { value: 'slowest', label: t('query.historySortSlowest') },
          ]}
        />
      </div>

      {(value.search !== DEFAULT_HISTORY_QUERY.search ||
        value.connectionId !== DEFAULT_HISTORY_QUERY.connectionId ||
        value.database !== DEFAULT_HISTORY_QUERY.database ||
        value.schema !== DEFAULT_HISTORY_QUERY.schema ||
        value.status !== DEFAULT_HISTORY_QUERY.status ||
        value.range !== DEFAULT_HISTORY_QUERY.range) && (
        <button
          type="button"
          onClick={() => onChange({ ...DEFAULT_HISTORY_QUERY, sort: value.sort })}
          className="h-8 rounded px-2 text-xs text-fg-muted hover:text-accent"
          data-testid="global-history-reset-filters"
        >
          {t('common.reset')}
        </button>
      )}
    </div>
  );
}
