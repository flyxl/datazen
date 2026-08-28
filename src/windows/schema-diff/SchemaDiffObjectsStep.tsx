import { Loader2 } from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';
import type { SchemaDiffTablePick } from './schemaDiffTableNames';

interface SchemaDiffObjectsStepProps {
  loading: boolean;
  tables: SchemaDiffTablePick[];
  onToggle: (name: string) => void;
  onSelectAll: () => void;
  onSelectNone: () => void;
}

export function SchemaDiffObjectsStep({
  loading,
  tables,
  onToggle,
  onSelectAll,
  onSelectNone,
}: SchemaDiffObjectsStepProps) {
  const { t } = useI18n();

  if (loading) {
    return (
      <div data-testid="schema-diff-objects-panel" className="flex justify-center py-12">
        <Loader2 className="h-6 w-6 animate-spin text-accent" />
      </div>
    );
  }

  if (tables.length === 0) {
    return (
      <div
        data-testid="schema-diff-objects-panel"
        className="rounded-lg border border-edge bg-surface-alt px-4 py-8 text-center text-sm text-fg-muted"
      >
        {t('schemaDiff.noTablesFound')}
      </div>
    );
  }

  const enabledCount = tables.filter((row) => row.enabled).length;

  return (
    <div data-testid="schema-diff-objects-panel" className="space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-2 text-sm">
        <span className="text-fg-muted">
          {t('schemaDiff.objectsSelected', { count: enabledCount, total: tables.length })}
        </span>
        <div className="flex gap-2">
          <button
            type="button"
            className="text-xs text-accent hover:underline"
            onClick={onSelectAll}
          >
            {t('common.selectAll')}
          </button>
          <button
            type="button"
            className="text-xs text-accent hover:underline"
            onClick={onSelectNone}
          >
            {t('common.deselectAll')}
          </button>
        </div>
      </div>
      <ul className="divide-y divide-edge overflow-hidden rounded-lg border border-edge bg-surface-alt">
        {tables.map((row) => (
          <li
            key={row.name}
            data-testid="schema-diff-table-row"
            data-table-name={row.name}
            className="flex items-center gap-2 px-3 py-2 text-sm"
          >
            <input type="checkbox" checked={row.enabled} onChange={() => onToggle(row.name)} />
            <span className="flex-1 font-mono text-xs">{row.name}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}
