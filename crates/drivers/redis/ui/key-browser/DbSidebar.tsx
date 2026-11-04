import { Database } from 'lucide-react';
import { cn, useI18n, Spinner } from '@datazen/ui';
import { dbIndexOfName } from './workbenchDatabases';

/**
 * Database picker of the key workbench (only rendered when the host has no outer
 * navigator tree, i.e. `hideSidebar` is false).
 *
 * Presentational: it lists the merged db names with their key counts and reports
 * the user's choice. After D-1/D-2 nothing else lives here — the search-scope
 * segment control and the pattern row moved into the key-tree column header
 * (PRD §3.2 R1/R2), because this sidebar disappears in the host's normal layout
 * and the controls it hid were the entry point to the whole panel.
 */

export interface DbSidebarProps {
  loading: boolean;
  databases: string[];
  selectedDb: string | null;
  dbCounts: Record<number, number>;
  onSelectDb: (db: string) => void;
}

export function DbSidebar({
  loading,
  databases,
  selectedDb,
  dbCounts,
  onSelectDb,
}: DbSidebarProps) {
  const { t } = useI18n();

  return (
    <aside
      className="flex w-48 shrink-0 flex-col overflow-y-auto border-r border-edge bg-surface-alt"
      data-testid="redis-db-sidebar"
    >
      {loading && (
        <div className="flex items-center gap-2 px-3 py-2 text-xs text-fg-muted">
          <Spinner size="md" />
          {t('common.loading')}
        </div>
      )}

      {databases.map((db) => {
        const count = dbCounts[dbIndexOfName(db)];
        return (
          <button
            key={db}
            type="button"
            className={cn(
              'flex items-center gap-2 px-3 py-2 text-left text-sm transition-colors',
              selectedDb === db
                ? 'bg-accent/10 text-accent font-medium'
                : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
            )}
            aria-current={selectedDb === db ? 'page' : undefined}
            data-testid={`redis-db-${db}`}
            onClick={() => onSelectDb(db)}
          >
            <Database className="h-4 w-4 shrink-0" />
            <span className="min-w-0 truncate">{db}</span>
            {count != null && (
              <span className="ml-auto shrink-0 text-[11px] text-fg-muted">({count})</span>
            )}
          </button>
        );
      })}
    </aside>
  );
}
