import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';

export interface SchemaDiffTableListPanelProps {
  tables: string[];
  selectedTable: string | null;
  onSelect: (table: string) => void;
  /** Optional map of table name → whether the table has schema differences */
  tableHasDiff?: Record<string, boolean>;
  className?: string;
}

export function SchemaDiffTableListPanel({
  tables,
  selectedTable,
  onSelect,
  tableHasDiff,
  className,
}: SchemaDiffTableListPanelProps) {
  const { t } = useI18n();

  return (
    <div
      className={cn('flex min-h-0 min-w-0 flex-1 flex-col border-r border-edge bg-surface', className)}
      data-testid="schema-diff-table-list"
    >
      <div className="shrink-0 border-b border-edge px-3 py-2">
        <div className="text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
          {t('schemaDiff.tables')}
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-auto">
        {tables.length === 0 ? (
          <div className="p-4 text-center text-xs text-fg-muted">{t('schemaDiff.tableRequired')}</div>
        ) : (
          tables.map((name) => {
            const active = selectedTable === name;
            const hasDiff = tableHasDiff?.[name];
            return (
              <button
                key={name}
                type="button"
                className={cn(
                  'flex w-full items-center gap-2 border-b border-edge px-3 py-2 text-left text-xs hover:bg-surface-alt',
                  active && 'bg-surface-alt',
                )}
                onClick={() => onSelect(name)}
                data-testid={`schema-diff-table-row-${name}`}
              >
                <span className="min-w-0 flex-1 truncate font-mono">{name}</span>
                {hasDiff === true && (
                  <span className="shrink-0 rounded bg-warning/15 px-1.5 py-0.5 text-[10px] font-medium uppercase text-warning">
                    {t('sync.colChanged')}
                  </span>
                )}
                {hasDiff === false && (
                  <span className="shrink-0 text-[10px] text-fg-muted">{t('sync.identical')}</span>
                )}
              </button>
            );
          })
        )}
      </div>
    </div>
  );
}
