import { useEffect, useState } from 'react';
import { Loader2, Pencil } from 'lucide-react';
import { databaseCommands } from '../../commands/database';
import { Button } from '../../components/ui/Button';
import type { TableSchema } from '../../types';
import { cn } from '../../lib/cn';
import { useI18n } from '../../hooks/useI18n';

interface StructureViewProps {
  connectionId: string;
  tableName: string;
  onEditStructure?: (tableName: string) => void;
}

function typeColor(dataType: string): string {
  const t = dataType.toLowerCase();
  if (/^(bigint|int|integer|smallint|serial|bigserial|numeric|decimal|real|double|float)/.test(t))
    return 'text-green-400';
  if (/^(varchar|char|text|citext|name)/.test(t)) return 'text-amber-400';
  if (/^(timestamp|date|time|interval)/.test(t)) return 'text-purple-400';
  if (/^(bool)/.test(t)) return 'text-sky-400';
  if (/^(json|jsonb)/.test(t)) return 'text-pink-400';
  if (/^(uuid)/.test(t)) return 'text-teal-400';
  if (/^(bytea|blob)/.test(t)) return 'text-red-400';
  return 'text-fg-secondary';
}

function KeyBadge({ label, tone }: { label: string; tone: 'blue' | 'amber' | 'green' }) {
  const colorMap = {
    blue: 'bg-blue-500/20 text-blue-400',
    amber: 'bg-amber-500/20 text-amber-400',
    green: 'bg-green-500/20 text-green-400',
  };
  return (
    <span className={cn('inline-flex items-center rounded px-1.5 py-0.5 text-[10px] font-semibold', colorMap[tone])}>
      {label}
    </span>
  );
}

export function StructureView({ connectionId, tableName, onEditStructure }: StructureViewProps) {
  const { t } = useI18n();
  const [schema, setSchema] = useState<TableSchema | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    console.log('[StructureView] loading schema', connectionId, tableName);

    databaseCommands
      .getTableSchema(connectionId, tableName)
      .then((result) => {
        if (!cancelled) {
          setSchema(result);
          setLoading(false);
        }
      })
      .catch((e) => {
        if (!cancelled) {
          const msg = typeof e === 'string' ? e : e instanceof Error ? e.message : t('structView.loadFailed');
          console.error('[StructureView] error', msg);
          setError(msg);
          setLoading(false);
        }
      });

    return () => { cancelled = true; };
  }, [connectionId, tableName, t]);

  if (loading) {
    return (
      <div className="flex flex-1 items-center justify-center gap-2 text-fg-muted">
        <Loader2 className="h-5 w-5 animate-spin" />
        {t('structView.loading')}
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex flex-1 items-center justify-center text-sm text-red-400">{error}</div>
    );
  }

  if (!schema) return null;

  const pkSet = new Set(schema.primaryKeys);
  const uniqueCols = new Set(
    schema.indexes.filter((idx) => idx.isUnique && !idx.isPrimary).flatMap((idx) => idx.columns),
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* Table name header */}
      <div className="flex items-center gap-2 px-4 py-3">
        <span className="text-base font-semibold text-fg">{schema.tableName}</span>
        <div className="flex-1" />
        {onEditStructure && (
          <Button
            variant="secondary"
            className="h-7 gap-1 px-2 text-xs"
            onClick={() => onEditStructure(tableName)}
          >
            <Pencil className="h-3.5 w-3.5" />
            {t('structView.editStructure')}
          </Button>
        )}
      </div>

      {/* Column table */}
      <div className="min-h-0 flex-1 overflow-auto">
        <table className="w-full border-collapse text-[13px]">
          <thead className="sticky top-0 z-10">
            <tr className="bg-surface-alt text-left text-xs font-medium text-fg-secondary">
              <th className="border-b border-edge px-4 py-2.5 font-medium">{t('structView.fieldName')}</th>
              <th className="border-b border-edge px-4 py-2.5 font-medium">{t('structView.type')}</th>
              <th className="border-b border-edge px-4 py-2.5 font-medium">{t('structView.nullable')}</th>
              <th className="border-b border-edge px-4 py-2.5 font-medium">{t('structView.defaultValue')}</th>
              <th className="border-b border-edge px-4 py-2.5 font-medium">{t('structView.primaryKey')}</th>
              <th className="border-b border-edge px-4 py-2.5 font-medium">{t('structView.comment')}</th>
            </tr>
          </thead>
          <tbody>
            {schema.columns.map((col) => {
              const isPk = pkSet.has(col.name);
              const isUq = uniqueCols.has(col.name);
              return (
                <tr key={col.name} className="border-b border-edge bg-surface hover:bg-surface-alt/50">
                  <td className="px-4 py-2.5 font-mono text-fg">{col.name}</td>
                  <td className={cn('px-4 py-2.5 font-mono', typeColor(col.dataType))}>{col.dataType}</td>
                  <td className="px-4 py-2.5">
                    {col.nullable ? (
                      <span className="text-green-400">YES</span>
                    ) : (
                      <span className="text-fg-muted">NO</span>
                    )}
                  </td>
                  <td className="px-4 py-2.5">
                    {col.defaultValue ? (
                      <span className="font-mono text-green-400">{col.defaultValue}</span>
                    ) : (
                      <span className="text-fg-muted">—</span>
                    )}
                  </td>
                  <td className="px-4 py-2.5">
                    <div className="flex gap-1">
                      {isPk && <KeyBadge label="PK" tone="blue" />}
                      {isUq && <KeyBadge label="UQ" tone="amber" />}
                      {col.isAutoIncrement && <KeyBadge label="AI" tone="green" />}
                    </div>
                  </td>
                  <td className="px-4 py-2.5 text-fg-muted">{col.comment ?? ''}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      {/* Summary bar */}
      <div className="flex items-center gap-4 border-t border-edge bg-surface-alt px-4 py-2.5 text-xs text-fg-secondary">
        <span>{t('structView.fields', { count: schema.columns.length })}</span>
        <span className="text-edge">|</span>
        <span>{t('structView.primaryKeys', { count: schema.primaryKeys.length })}</span>
        <span className="text-edge">|</span>
        <span>{t('structView.uniqueIndexes', { count: schema.indexes.filter((i) => i.isUnique && !i.isPrimary).length })}</span>
        <span className="text-edge">|</span>
        <span>{t('structView.indexCount', { count: schema.indexes.length })}</span>
        <span className="text-edge">|</span>
        <span>{t('structView.foreignKeyCount', { count: schema.foreignKeys.length })}</span>
      </div>
    </div>
  );
}
