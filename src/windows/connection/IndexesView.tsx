import { useCallback, useEffect, useState } from 'react';
import { Loader2, Plus, Trash2, AlertTriangle } from 'lucide-react';
import { databaseCommands } from '../../commands/database';
import { getCachedTableSchema, invalidateSchemaCache } from '../../lib/schemaCache';
import type { IndexInfo, TableSchema, ColumnSchema, DatabaseType } from '../../types';
import { cn } from '../../lib/cn';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { DB_REGISTRY } from '../../lib/databaseTypes';

interface IndexesViewProps {
  connectionId: string;
  tableName: string;
  createIndexTrigger?: number;
  databaseType?: string;
}

function TypeBadge({ type: t }: { type: string }) {
  return (
    <span className="inline-flex items-center rounded bg-surface-raised px-1.5 py-0.5 font-mono text-[11px] text-fg-secondary">
      {t}
    </span>
  );
}

// ── Create Index Dialog ──────────────────────────────────────────

interface CreateIndexDialogProps {
  columns: ColumnSchema[];
  tableName: string;
  onSubmit: (sql: string) => void;
  onCancel: () => void;
  submitting: boolean;
  databaseType?: string;
}

function CreateIndexDialog({ columns, tableName, onSubmit, onCancel, submitting, databaseType }: CreateIndexDialogProps) {
  const { t } = useI18n();
  const meta = DB_REGISTRY[databaseType as DatabaseType];
  const isMySQLDialect = meta?.sqlDialect === 'mysql';
  const [indexName, setIndexName] = useState('');
  const [selectedCols, setSelectedCols] = useState<string[]>([]);
  const [isUnique, setIsUnique] = useState(false);
  const [indexType, setIndexType] = useState<'btree' | 'hash' | 'gin' | 'gist'>('btree');

  const autoName = `idx_${tableName}_${selectedCols.join('_')}`;
  const q = meta?.quoteChar || '"';

  const toggleColumn = (col: string) => {
    setSelectedCols((prev) =>
      prev.includes(col) ? prev.filter((c) => c !== col) : [...prev, col],
    );
  };

  const handleSubmit = () => {
    if (selectedCols.length === 0) return;
    const name = indexName.trim() || autoName;
    const uniqueKw = isUnique ? 'UNIQUE ' : '';
    const usingKw = indexType !== 'btree' ? ` USING ${indexType}` : '';
    const quotedCols = selectedCols.map((c) => `${q}${c}${q}`).join(', ');
    const sql = `CREATE ${uniqueKw}INDEX ${q}${name}${q} ON ${q}${tableName}${q}${usingKw} (${quotedCols})`;
    onSubmit(sql);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" role="dialog" aria-modal="true">
      <div className="w-[480px] rounded-lg border border-edge bg-surface p-5 shadow-xl">
        <h3 className="mb-4 text-base font-semibold text-fg">{t('indexes.newIndex')}</h3>

        {/* Index name */}
        <div className="mb-3">
          <label htmlFor="idx-name" className="mb-1 block text-xs text-fg-secondary">{t('indexes.indexName')}</label>
          <input
            id="idx-name"
            className="h-8 w-full rounded border border-edge bg-surface-alt px-2.5 text-sm text-fg outline-none focus:border-blue-500"
            placeholder={autoName || 'idx_...'}
            value={indexName}
            onChange={(e) => setIndexName(e.target.value)}
          />
        </div>

        {/* Column selection */}
        <div className="mb-3">
          <label htmlFor="idx-cols" className="mb-1 block text-xs text-fg-secondary">{t('indexes.selectColumns')}</label>
          <div id="idx-cols" className="max-h-40 overflow-auto rounded border border-edge bg-surface-alt p-2">
            {columns.map((col) => {
              const checked = selectedCols.includes(col.name);
              const order = checked ? selectedCols.indexOf(col.name) + 1 : null;
              return (
                <label
                  key={col.name}
                  className={cn(
                    'flex cursor-pointer items-center gap-2 rounded px-2 py-1.5 text-sm hover:bg-surface-raised',
                    checked && 'bg-blue-500/10',
                  )}
                >
                  <input
                    type="checkbox"
                    checked={checked}
                    onChange={() => toggleColumn(col.name)}
                    className="accent-blue-500"
                  />
                  <span className="font-mono text-fg">{col.name}</span>
                  <span className="text-xs text-fg-muted">{col.dataType}</span>
                  {order && (
                    <span className="ml-auto rounded bg-blue-500/20 px-1.5 py-0.5 text-[10px] font-semibold text-blue-400">
                      #{order}
                    </span>
                  )}
                </label>
              );
            })}
          </div>
        </div>

        {/* Options row */}
        <div className="mb-4 flex items-center gap-4">
          <label className="flex items-center gap-2 text-sm text-fg">
            <input
              type="checkbox"
              checked={isUnique}
              onChange={(e) => setIsUnique(e.target.checked)}
              className="accent-blue-500"
            />
            {t('indexes.unique')}
          </label>

          <div className="flex items-center gap-2">
            <span className="text-xs text-fg-secondary">{t('indexes.indexType')}</span>
            <select
              className="h-7 rounded border border-edge bg-surface-alt px-2 text-xs text-fg outline-none"
              value={indexType}
              onChange={(e) => setIndexType(e.target.value as typeof indexType)}
            >
              <option value="btree">B-Tree</option>
              <option value="hash">Hash</option>
              {!isMySQLDialect && <option value="gin">GIN</option>}
              {!isMySQLDialect && <option value="gist">GiST</option>}
            </select>
          </div>
        </div>

        {/* SQL preview */}
        {selectedCols.length > 0 && (
          <div className="mb-4 rounded border border-edge bg-surface-alt p-2.5">
            <div className="mb-1 text-[10px] font-medium uppercase text-fg-muted">{t('indexes.sqlPreview')}</div>
            <code className="block whitespace-pre-wrap text-xs text-green-400">
              {`CREATE ${isUnique ? 'UNIQUE ' : ''}INDEX ${q}${indexName.trim() || autoName}${q} ON ${q}${tableName}${q}${indexType !== 'btree' ? ` USING ${indexType}` : ''} (${selectedCols.map((c) => `${q}${c}${q}`).join(', ')})`}
            </code>
          </div>
        )}

        {/* Actions */}
        <div className="flex justify-end gap-2">
          <Button variant="secondary" onClick={onCancel} disabled={submitting}>
            {t('common.cancel')}
          </Button>
          <Button
            onClick={handleSubmit}
            disabled={selectedCols.length === 0 || submitting}
          >
            {submitting ? <Loader2 className="h-4 w-4 animate-spin" /> : <Plus className="h-4 w-4" />}
            {t('indexes.createIndex')}
          </Button>
        </div>
      </div>
    </div>
  );
}

// ── Delete Confirm Dialog ────────────────────────────────────────

interface DeleteConfirmProps {
  indexName: string;
  onConfirm: () => void;
  onCancel: () => void;
  submitting: boolean;
}

function DeleteConfirmDialog({ indexName, onConfirm, onCancel, submitting }: DeleteConfirmProps) {
  const { t } = useI18n();
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" role="dialog" aria-modal="true">
      <div className="w-[400px] rounded-lg border border-edge bg-surface p-5 shadow-xl">
        <div className="mb-3 flex items-center gap-2 text-amber-400">
          <AlertTriangle className="h-5 w-5" />
          <h3 className="text-base font-semibold">{t('indexes.confirmDeleteTitle')}</h3>
        </div>
        <p className="mb-4 text-sm text-fg-secondary">
          {t('indexes.confirmDeleteMsg', { name: indexName })}
        </p>
        <div className="flex justify-end gap-2">
          <Button variant="secondary" onClick={onCancel} disabled={submitting}>
            {t('common.cancel')}
          </Button>
          <Button variant="danger" onClick={onConfirm} disabled={submitting}>
            {submitting ? <Loader2 className="h-4 w-4 animate-spin" /> : <Trash2 className="h-4 w-4" />}
            {t('common.delete')}
          </Button>
        </div>
      </div>
    </div>
  );
}

// ── Main IndexesView ─────────────────────────────────────────────

export function IndexesView({ connectionId, tableName, createIndexTrigger, databaseType }: IndexesViewProps) {
  const { t } = useI18n();
  const dbMeta = DB_REGISTRY[databaseType as DatabaseType];
  const isMySQLDialect = dbMeta?.sqlDialect === 'mysql';
  const q = dbMeta?.quoteChar || '"';
  const [indexes, setIndexes] = useState<IndexInfo[]>([]);
  const [columns, setColumns] = useState<ColumnSchema[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [version, setVersion] = useState(0);

  const loadSchema = useCallback(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);

    getCachedTableSchema(connectionId, tableName)
      .then((schema: TableSchema) => {
        if (!cancelled) {
          setIndexes(schema.indexes);
          setColumns(schema.columns);
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

  useEffect(() => {
    if (createIndexTrigger && createIndexTrigger > 0) setShowCreate(true);
  }, [createIndexTrigger]);

  const handleCreateIndex = async (sql: string) => {
    setSubmitting(true);
    try {
      await databaseCommands.executeSQL(connectionId, sql);
      invalidateSchemaCache(connectionId, tableName);
      setShowCreate(false);
      setVersion((v) => v + 1);
    } catch (e) {
      const msg = typeof e === 'string' ? e : e instanceof Error ? e.message : t('indexes.createFailed');
      setError(msg);
    } finally {
      setSubmitting(false);
    }
  };

  const handleDropIndex = async () => {
    if (!deleteTarget) return;
    setSubmitting(true);
    try {
      const dropSql = isMySQLDialect
        ? `DROP INDEX ${q}${deleteTarget}${q} ON ${q}${tableName}${q}`
        : `DROP INDEX ${q}${deleteTarget}${q}`;
      await databaseCommands.executeSQL(connectionId, dropSql);
      invalidateSchemaCache(connectionId, tableName);
      setDeleteTarget(null);
      setVersion((v) => v + 1);
    } catch (e) {
      const msg = typeof e === 'string' ? e : e instanceof Error ? e.message : t('indexes.deleteFailed');
      setError(msg);
    } finally {
      setSubmitting(false);
    }
  };

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
      {/* Header */}
      <div className="flex items-center gap-2 px-4 py-3">
        <span className="text-base font-semibold text-fg">{tableName}</span>
        <span className="text-sm text-fg-muted">· {t('indexes.count', { count: indexes.length })}</span>
        <div className="flex-1" />
        <Button
          variant="secondary"
          className="h-7 gap-1 px-2 text-xs"
          onClick={() => setShowCreate(true)}
        >
          <Plus className="h-3.5 w-3.5" />
          {t('indexes.newIndex')}
        </Button>
      </div>

      {/* Table or empty state */}
      {indexes.length === 0 ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 text-fg-muted">
          <span className="text-sm">{t('indexes.noIndexes')}</span>
          <Button
            variant="secondary"
            className="h-8 gap-1 text-xs"
            onClick={() => setShowCreate(true)}
          >
            <Plus className="h-3.5 w-3.5" />
            {t('indexes.createFirst')}
          </Button>
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
                <th className="border-b border-edge px-4 py-2.5 font-medium w-16">{t('indexes.colActions')}</th>
              </tr>
            </thead>
            <tbody>
              {indexes.map((idx) => (
                <tr key={idx.name} data-index-name={idx.name} className="group border-b border-edge bg-surface hover:bg-surface-alt/50">
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
                  <td className="px-4 py-2.5">
                    {!idx.isPrimary && (
                      <button
                        className="rounded p-1 text-fg-muted opacity-0 transition hover:bg-red-500/10 hover:text-red-400 group-hover:opacity-100"
                        title={t('indexes.deleteIndex')}
                        onClick={() => setDeleteTarget(idx.name)}
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* Create Index Dialog */}
      {showCreate && (
        <CreateIndexDialog
          columns={columns}
          tableName={tableName}
          onSubmit={handleCreateIndex}
          onCancel={() => setShowCreate(false)}
          submitting={submitting}
          databaseType={databaseType}
        />
      )}

      {/* Delete Confirm Dialog */}
      {deleteTarget && (
        <DeleteConfirmDialog
          indexName={deleteTarget}
          onConfirm={handleDropIndex}
          onCancel={() => setDeleteTarget(null)}
          submitting={submitting}
        />
      )}
    </div>
  );
}
