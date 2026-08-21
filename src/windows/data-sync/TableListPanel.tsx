import { Search } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import {
  displayTableName,
  rowDiffCounts,
  tableHasRowDiffs,
  tableIsUnchanged,
  tableKey,
  type DataSyncTableResult,
  type TableDiffFilter,
} from './mappingView';

const FILTERS: TableDiffFilter[] = [
  'all',
  'insert',
  'update',
  'delete',
  'unchanged',
  'incompatible',
];

interface TableListPanelProps {
  rows: DataSyncTableResult[];
  filter: TableDiffFilter;
  search: string;
  selectedTableKey: string | null;
  onFilterChange: (f: TableDiffFilter) => void;
  onSearchChange: (q: string) => void;
  onSelectTable: (key: string) => void;
}

export function TableListPanel({
  rows,
  filter,
  search,
  selectedTableKey,
  onFilterChange,
  onSearchChange,
  onSelectTable,
}: TableListPanelProps) {
  const { t } = useI18n();

  const filtered = rows.filter((r) => {
    const name = displayTableName(r).toLowerCase();
    if (search && !name.includes(search.toLowerCase())) return false;
    if (filter === 'all') return true;
    if (filter === 'incompatible') return r.status === 'INCOMPATIBLE';
    if (r.status !== 'MATCHED') return false;
    const counts = rowDiffCounts(r);
    switch (filter) {
      case 'insert':
        return counts.inserts > 0;
      case 'update':
        return counts.updates > 0;
      case 'delete':
        return counts.deletes > 0;
      case 'unchanged':
        return tableIsUnchanged(r);
      default:
        return true;
    }
  });

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col border-r border-edge">
      <div className="shrink-0 space-y-2 border-b border-edge p-3">
        <div className="relative">
          <Search className="pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-fg-muted" />
          <input
            type="search"
            className="h-8 w-full rounded-md border border-edge bg-surface pl-8 pr-2 text-xs"
            placeholder={t('sync.searchTables')}
            value={search}
            onChange={(e) => onSearchChange(e.target.value)}
          />
        </div>
        <div className="flex flex-wrap gap-1">
          {FILTERS.map((f) => (
            <Button
              key={f}
              variant={filter === f ? 'secondary' : 'ghost'}
              size="sm"
              className="text-[10px]"
              onClick={() => onFilterChange(f)}
            >
              {t(`sync.filter.${f}` as 'sync.filter.all')}
            </Button>
          ))}
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-auto">
        {filtered.length === 0 ? (
          <div className="p-4 text-center text-xs text-fg-muted">{t('sync.noTablesMatch')}</div>
        ) : (
          filtered.map((row) => {
            const key = tableKey(row);
            const counts = rowDiffCounts(row);
            const active = selectedTableKey === key;
            return (
              <button
                key={key}
                type="button"
                className={cn(
                  'flex w-full items-center gap-2 border-b border-edge px-3 py-2 text-left text-xs hover:bg-surface-alt',
                  active && 'bg-surface-alt',
                )}
                onClick={() => onSelectTable(key)}
              >
                <span className="min-w-0 flex-1 truncate font-mono">{displayTableName(row)}</span>
                {row.status === 'MATCHED' && tableHasRowDiffs(row) ? (
                  <span className="shrink-0 tabular-nums text-fg-muted">
                    {t('sync.rowDiffs', counts)}
                  </span>
                ) : (
                  <span className="shrink-0 text-fg-muted">
                    {row.status === 'INCOMPATIBLE'
                      ? t('sync.mappingIncompatible')
                      : t('sync.identical')}
                  </span>
                )}
              </button>
            );
          })
        )}
      </div>
    </div>
  );
}
