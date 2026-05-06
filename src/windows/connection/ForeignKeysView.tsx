import { useEffect, useState } from 'react';
import { Loader2 } from 'lucide-react';
import { getCachedTableSchema } from '../../lib/schemaCache';
import type { ForeignKeyInfo, TableSchema } from '../../types';
import { useI18n } from '../../hooks/useI18n';

interface ForeignKeysViewProps {
  connectionId: string;
  tableName: string;
}

export function ForeignKeysView({ connectionId, tableName }: ForeignKeysViewProps) {
  const { t } = useI18n();
  const [foreignKeys, setForeignKeys] = useState<ForeignKeyInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);

    getCachedTableSchema(connectionId, tableName)
      .then((schema: TableSchema) => {
        if (!cancelled) {
          setForeignKeys(schema.foreignKeys);
          setLoading(false);
        }
      })
      .catch((e) => {
        if (!cancelled) {
          setError(typeof e === 'string' ? e : e instanceof Error ? e.message : t('fk.loadFailed'));
          setLoading(false);
        }
      });

    return () => { cancelled = true; };
  }, [connectionId, tableName, t]);

  if (loading) {
    return (
      <div className="flex flex-1 items-center justify-center gap-2 text-fg-muted">
        <Loader2 className="h-5 w-5 animate-spin" />
        {t('fk.loading')}
      </div>
    );
  }

  if (error) {
    return <div className="flex flex-1 items-center justify-center text-sm text-red-400">{error}</div>;
  }

  if (foreignKeys.length === 0) {
    return <div className="flex flex-1 items-center justify-center text-sm text-fg-muted">{t('fk.noForeignKeys')}</div>;
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-baseline gap-2 px-4 py-3">
        <span className="text-base font-semibold text-fg">{tableName}</span>
        <span className="text-sm text-fg-muted">· {t('fk.count', { count: foreignKeys.length })}</span>
      </div>

      <div className="min-h-0 flex-1 overflow-auto">
        <table className="w-full border-collapse text-[13px]">
          <thead className="sticky top-0 z-10">
            <tr className="bg-surface-alt text-left text-xs font-medium text-fg-secondary">
              <th className="border-b border-edge px-4 py-2.5 font-medium">{t('fk.constraintName')}</th>
              <th className="border-b border-edge px-4 py-2.5 font-medium">{t('fk.localColumn')}</th>
              <th className="border-b border-edge px-4 py-2.5 font-medium">{t('fk.refTable')}</th>
              <th className="border-b border-edge px-4 py-2.5 font-medium">{t('fk.refColumn')}</th>
              <th className="border-b border-edge px-4 py-2.5 font-medium">ON UPDATE</th>
              <th className="border-b border-edge px-4 py-2.5 font-medium">ON DELETE</th>
            </tr>
          </thead>
          <tbody>
            {foreignKeys.map((fk) => (
              <tr key={fk.name} className="border-b border-edge bg-surface hover:bg-surface-alt/50">
                <td className="px-4 py-2.5 font-mono text-fg">{fk.name}</td>
                <td className="px-4 py-2.5">
                  <div className="flex flex-wrap gap-1">
                    {fk.columns.map((col) => (
                      <span key={col} className="inline-flex items-center rounded bg-amber-500/10 px-1.5 py-0.5 text-[11px] text-amber-400">
                        {col}
                      </span>
                    ))}
                  </div>
                </td>
                <td className="px-4 py-2.5 font-mono text-blue-400">{fk.referencedTable}</td>
                <td className="px-4 py-2.5">
                  <div className="flex flex-wrap gap-1">
                    {fk.referencedColumns.map((col) => (
                      <span key={col} className="inline-flex items-center rounded bg-blue-500/10 px-1.5 py-0.5 text-[11px] text-blue-400">
                        {col}
                      </span>
                    ))}
                  </div>
                </td>
                <td className="px-4 py-2.5 text-fg-secondary">{fk.onUpdate}</td>
                <td className="px-4 py-2.5 text-fg-secondary">{fk.onDelete}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
