import { useCallback, useEffect, useState } from 'react';
import { Loader2, Pencil } from 'lucide-react';
import { getCachedTableSchema } from '../../lib/schemaCache';
import type { IndexInfo, TableSchema } from '../../types';
import { cn } from '../../lib/cn';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';

interface IndexesViewProps {
  connectionId: string;
  tableName: string;
  onEditStructure?: (tableName: string) => void;
}

function TypeBadge({ type: t }: { type: string }) {
  return (
    <span className="inline-flex items-center rounded bg-surface-raised px-1.5 py-0.5 font-mono text-[11px] text-fg-secondary">
      {t}
    </span>
  );
}

export function IndexesView({ connectionId, tableName, onEditStructure }: IndexesViewProps) {
  const { t } = useI18n();
  const [indexes, setIndexes] = useState<IndexInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [version, setVersion] = useState(0);

  const loadSchema = useCallback(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);

    getCachedTableSchema(connectionId, tableName)
      .then((schema: TableSchema) => {
        if (!cancelled) {
          setIndexes(schema.indexes);
          setLoading(false);
        }
      })
      .catch((e) => {
        if (!cancelled) {
          setError(typeof e === 'string' ? e : e instanceof Error ? e.message : t('indexes.loadFailed'));
          setLoading(false);
        }
      });

    return () => { cancelled = true; };
  }, [connectionId, tableName, version, t]);

  useEffect(() => loadSchema(), [loadSchema]);

  if (loading) {
    return (
      <div className="flex flex-1 items-center justify-center gap-2 text-fg-muted">
        <Loader2 className="h-5 w-5 animate-spin" />
        {t('indexes.loading')}
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-2">
        <span className="text-sm text-red-400">{error}</span>
        <Button variant="secondary" className="h-7 text-xs" onClick={() => { setError(null); setVersion((v) => v + 1); }}>
          {t('common.retry')}
        </Button>
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-2 px-4 py-3">
        <span className="text-base font-semibold text-fg">{tableName}</span>
        <span className="text-sm text-fg-muted">· {t('indexes.count', { count: indexes.length })}</span>
        <div className="flex-1" />
        {onEditStructure && (
          <Button
            variant="secondary"
            className="h-7 gap-1 px-2 text-xs"
            onClick={() => onEditStructure(tableName)}
          >
            <Pencil className="h-3.5 w-3.5" />
            {t('indexes.editInStructure')}
          </Button>
        )}
      </div>

      {indexes.length === 0 ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 text-fg-muted">
          <span className="text-sm">{t('indexes.noIndexes')}</span>
          {onEditStructure && (
            <Button
              variant="secondary"
              className="h-8 gap-1 text-xs"
              onClick={() => onEditStructure(tableName)}
            >
              <Pencil className="h-3.5 w-3.5" />
              {t('indexes.editInStructure')}
            </Button>
          )}
        </div>
      ) : (
        <div className="min-h-0 flex-1 overflow-auto">
          <table className="w-full border-collapse text-[13px]">
            <thead className="sticky top-0 z-10">
              <tr className="bg-surface-alt text-left text-xs font-medium text-fg-secondary">
                <th className="border-b border-edge px-4 py-2.5 font-medium">{t('indexes.colName')}</th>
                <th className="border-b border-edge px-4 py-2.5 font-medium">{t('indexes.colColumns')}</th>
                <th className="border-b border-edge px-4 py-2.5 font-medium">{t('indexes.colType')}</th>
                <th className="border-b border-edge px-4 py-2.5 font-medium">{t('indexes.colUnique')}</th>
                <th className="border-b border-edge px-4 py-2.5 font-medium">{t('indexes.colPrimary')}</th>
              </tr>
            </thead>
            <tbody>
              {indexes.map((idx) => (
                <tr key={idx.name} data-index-name={idx.name} className="border-b border-edge bg-surface hover:bg-surface-alt/50">
                  <td className="px-4 py-2.5 font-mono text-fg">{idx.name}</td>
                  <td className="px-4 py-2.5">
                    <div className="flex flex-wrap gap-1">
                      {idx.columns.map((col) => (
                        <span key={col} className="inline-flex items-center rounded bg-blue-500/10 px-1.5 py-0.5 text-[11px] text-blue-400">
                          {col}
                        </span>
                      ))}
                    </div>
                  </td>
                  <td className="px-4 py-2.5"><TypeBadge type={idx.indexType ?? 'btree'} /></td>
                  <td className="px-4 py-2.5">
                    <span className={cn(idx.isUnique ? 'text-green-400' : 'text-fg-muted')}>
                      {idx.isUnique ? 'YES' : 'NO'}
                    </span>
                  </td>
                  <td className="px-4 py-2.5">
                    <span className={cn(idx.isPrimary ? 'text-blue-400' : 'text-fg-muted')}>
                      {idx.isPrimary ? 'YES' : 'NO'}
                    </span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
