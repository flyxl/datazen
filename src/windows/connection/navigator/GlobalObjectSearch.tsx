import { Search, X } from 'lucide-react';
import { useMemo, useState } from 'react';
import { useI18n } from '../../../hooks/useI18n';
import { buildTableSqlAction, type TableSqlActionKind } from '../../../lib/tableSqlActions';
import {
  searchSchemaObjects,
  type ObjectSearchResult,
  type SchemaObjectIndexEntry,
} from '../../../lib/schemaObjectSearch';
import { Dialog } from '../../../components/ui/Dialog';
import { Button } from '../../../components/ui/Button';
import type { I18nKey } from '../../../locales';

export interface GlobalObjectSearchProps {
  open: boolean;
  index: readonly SchemaObjectIndexEntry[];
  onClose: () => void;
  onOpenResult: (result: ObjectSearchResult) => void;
  onOpenTableAction: (result: ObjectSearchResult, action: TableSqlActionKind) => void;
}

function resultContext(result: ObjectSearchResult): string {
  return [result.connectionName, result.database, result.schema, result.tableName]
    .filter((value): value is string => Boolean(value))
    .join(' / ');
}

const typeLabel: Record<ObjectSearchResult['objectType'], I18nKey> = {
  table: 'objectSearch.type.table',
  view: 'objectSearch.type.view',
  column: 'objectSearch.type.column',
  function: 'objectSearch.type.function',
  routine: 'objectSearch.type.routine',
  procedure: 'objectSearch.type.procedure',
  trigger: 'objectSearch.type.trigger',
  sequence: 'objectSearch.type.sequence',
  type: 'objectSearch.type.type',
};

const actionLabels: Record<TableSqlActionKind, I18nKey> = {
  openData: 'objectSearch.openData',
  select: 'objectSearch.select',
  insert: 'objectSearch.insert',
  update: 'objectSearch.update',
  ddl: 'objectSearch.ddl',
};

export function GlobalObjectSearch({
  open,
  index,
  onClose,
  onOpenResult,
  onOpenTableAction,
}: GlobalObjectSearchProps) {
  const { t } = useI18n();
  const [query, setQuery] = useState('');
  const results = useMemo(() => searchSchemaObjects(index, query), [index, query]);

  return (
    <Dialog
      open={open}
      title={t('objectSearch.title')}
      description={t('objectSearch.description')}
      onClose={onClose}
      className="max-w-3xl"
      testId="global-object-search"
    >
      <div className="relative">
        <Search className="pointer-events-none absolute left-2 top-1/2 h-4 w-4 -translate-y-1/2 text-fg-muted" />
        <input
          autoFocus
          type="search"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder={t('objectSearch.placeholder')}
          className="h-9 w-full rounded-md border border-edge bg-surface pl-8 pr-8 text-sm text-fg outline-none focus:border-accent"
          data-testid="global-object-search-input"
        />
        {query && (
          <button
            type="button"
            className="absolute right-2 top-1/2 -translate-y-1/2 text-fg-muted hover:text-fg"
            onClick={() => setQuery('')}
            aria-label={t('common.close')}
          >
            <X className="h-4 w-4" />
          </button>
        )}
      </div>

      <div className="mt-3 max-h-[52vh] overflow-auto rounded-md border border-edge">
        {results.length === 0 ? (
          <div className="px-3 py-8 text-center text-sm text-fg-muted">
            {index.length === 0 ? t('objectSearch.noLoadedSchema') : t('objectSearch.noResults')}
          </div>
        ) : (
          results.slice(0, 100).map((result) => {
            const isTableResult = result.objectType === 'table' || result.objectType === 'view';
            return (
              <div
                key={result.id}
                className="flex items-center gap-3 border-b border-edge px-3 py-2 last:border-b-0 hover:bg-surface-raised"
                data-testid="global-object-search-result"
              >
                <button
                  type="button"
                  className="min-w-0 flex-1 text-left"
                  onClick={() => onOpenResult(result)}
                >
                  <div className="flex items-center gap-2 text-sm text-fg">
                    <span className="truncate font-medium">{result.name}</span>
                    <span className="shrink-0 rounded bg-surface-raised px-1.5 py-0.5 text-[10px] text-fg-muted">
                      {t(typeLabel[result.objectType])}
                    </span>
                  </div>
                  <div className="mt-0.5 truncate text-xs text-fg-muted">
                    {resultContext(result)} · {t('objectSearch.matched', { field: result.matchReason ?? 'name' })}
                  </div>
                </button>
                {isTableResult && (
                  <div className="flex shrink-0 items-center gap-1">
                    {result.actions.map((action) => {
                      const tableAction = buildTableSqlAction(result, action);
                      return (
                        <Button
                          key={action}
                          size="sm"
                          variant={action === 'openData' ? 'secondary' : 'ghost'}
                          className="h-6 px-1.5 text-[10px]"
                          title={tableAction.description}
                          onClick={() => onOpenTableAction(result, action)}
                          data-testid={`object-search-action-${action}`}
                        >
                          {t(actionLabels[action])}
                        </Button>
                      );
                    })}
                  </div>
                )}
              </div>
            );
          })
        )}
      </div>
    </Dialog>
  );
}
