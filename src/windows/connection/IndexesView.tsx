import { useCallback, useEffect, useState } from 'react';
import { Loader2, Plus, Trash2, AlertTriangle } from 'lucide-react';
import { databaseCommands } from '../../commands/database';
import type { IndexInfo, TableSchema, ColumnSchema } from '../../types';
import { cn } from '../../lib/cn';
import { Button } from '../../components/ui/Button';

interface IndexesViewProps {
  connectionId: string;
  tableName: string;
  createIndexTrigger?: number;
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
}

function CreateIndexDialog({ columns, tableName, onSubmit, onCancel, submitting }: CreateIndexDialogProps) {
  const [indexName, setIndexName] = useState('');
  const [selectedCols, setSelectedCols] = useState<string[]>([]);
  const [isUnique, setIsUnique] = useState(false);
  const [indexType, setIndexType] = useState<'btree' | 'hash' | 'gin' | 'gist'>('btree');

  const autoName = `idx_${tableName}_${selectedCols.join('_')}`;

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
    const quotedCols = selectedCols.map((c) => `"${c}"`).join(', ');
    const sql = `CREATE ${uniqueKw}INDEX "${name}" ON "${tableName}"${usingKw} (${quotedCols})`;
    onSubmit(sql);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" role="dialog" aria-modal="true">
      <div className="w-[480px] rounded-lg border border-edge bg-surface p-5 shadow-xl">
        <h3 className="mb-4 text-base font-semibold text-fg">新建索引</h3>

        {/* Index name */}
        <div className="mb-3">
          <label htmlFor="idx-name" className="mb-1 block text-xs text-fg-secondary">索引名称</label>
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
          <label htmlFor="idx-cols" className="mb-1 block text-xs text-fg-secondary">选择列（按选择顺序排列）</label>
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
            唯一索引
          </label>

          <div className="flex items-center gap-2">
            <span className="text-xs text-fg-secondary">索引类型</span>
            <select
              className="h-7 rounded border border-edge bg-surface-alt px-2 text-xs text-fg outline-none"
              value={indexType}
              onChange={(e) => setIndexType(e.target.value as typeof indexType)}
            >
              <option value="btree">B-Tree</option>
              <option value="hash">Hash</option>
              <option value="gin">GIN</option>
              <option value="gist">GiST</option>
            </select>
          </div>
        </div>

        {/* SQL preview */}
        {selectedCols.length > 0 && (
          <div className="mb-4 rounded border border-edge bg-surface-alt p-2.5">
            <div className="mb-1 text-[10px] font-medium uppercase text-fg-muted">SQL 预览</div>
            <code className="block whitespace-pre-wrap text-xs text-green-400">
              {`CREATE ${isUnique ? 'UNIQUE ' : ''}INDEX "${indexName.trim() || autoName}" ON "${tableName}"${indexType !== 'btree' ? ` USING ${indexType}` : ''} (${selectedCols.map((c) => `"${c}"`).join(', ')})`}
            </code>
          </div>
        )}

        {/* Actions */}
        <div className="flex justify-end gap-2">
          <Button variant="secondary" onClick={onCancel} disabled={submitting}>
            取消
          </Button>
          <Button
            onClick={handleSubmit}
            disabled={selectedCols.length === 0 || submitting}
          >
            {submitting ? <Loader2 className="h-4 w-4 animate-spin" /> : <Plus className="h-4 w-4" />}
            创建索引
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
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" role="dialog" aria-modal="true">
      <div className="w-[400px] rounded-lg border border-edge bg-surface p-5 shadow-xl">
        <div className="mb-3 flex items-center gap-2 text-amber-400">
          <AlertTriangle className="h-5 w-5" />
          <h3 className="text-base font-semibold">确认删除索引</h3>
        </div>
        <p className="mb-4 text-sm text-fg-secondary">
          确定要删除索引 <code className="rounded bg-surface-alt px-1.5 py-0.5 font-mono text-fg">{indexName}</code> 吗？此操作不可撤销。
        </p>
        <div className="flex justify-end gap-2">
          <Button variant="secondary" onClick={onCancel} disabled={submitting}>
            取消
          </Button>
          <Button variant="danger" onClick={onConfirm} disabled={submitting}>
            {submitting ? <Loader2 className="h-4 w-4 animate-spin" /> : <Trash2 className="h-4 w-4" />}
            删除
          </Button>
        </div>
      </div>
    </div>
  );
}

// ── Main IndexesView ─────────────────────────────────────────────

export function IndexesView({ connectionId, tableName, createIndexTrigger }: IndexesViewProps) {
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

    databaseCommands
      .getTableSchema(connectionId, tableName)
      .then((schema: TableSchema) => {
        if (!cancelled) {
          setIndexes(schema.indexes);
          setColumns(schema.columns);
          setLoading(false);
        }
      })
      .catch((e) => {
        if (!cancelled) {
          setError(typeof e === 'string' ? e : e instanceof Error ? e.message : '加载索引失败');
          setLoading(false);
        }
      });

    return () => { cancelled = true; };
  }, [connectionId, tableName, version]);

  useEffect(() => loadSchema(), [loadSchema]);

  useEffect(() => {
    if (createIndexTrigger && createIndexTrigger > 0) setShowCreate(true);
  }, [createIndexTrigger]);

  const handleCreateIndex = async (sql: string) => {
    setSubmitting(true);
    try {
      await databaseCommands.executeSQL(connectionId, sql);
      setShowCreate(false);
      setVersion((v) => v + 1);
    } catch (e) {
      const msg = typeof e === 'string' ? e : e instanceof Error ? e.message : '创建索引失败';
      setError(msg);
    } finally {
      setSubmitting(false);
    }
  };

  const handleDropIndex = async () => {
    if (!deleteTarget) return;
    setSubmitting(true);
    try {
      await databaseCommands.executeSQL(connectionId, `DROP INDEX "${deleteTarget}"`);
      setDeleteTarget(null);
      setVersion((v) => v + 1);
    } catch (e) {
      const msg = typeof e === 'string' ? e : e instanceof Error ? e.message : '删除索引失败';
      setError(msg);
    } finally {
      setSubmitting(false);
    }
  };

  if (loading) {
    return (
      <div className="flex flex-1 items-center justify-center gap-2 text-fg-muted">
        <Loader2 className="h-5 w-5 animate-spin" />
        加载索引信息…
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-2">
        <span className="text-sm text-red-400">{error}</span>
        <Button variant="secondary" className="h-7 text-xs" onClick={() => { setError(null); setVersion((v) => v + 1); }}>
          重试
        </Button>
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* Header */}
      <div className="flex items-center gap-2 px-4 py-3">
        <span className="text-base font-semibold text-fg">{tableName}</span>
        <span className="text-sm text-fg-muted">· {indexes.length} 个索引</span>
        <div className="flex-1" />
        <Button
          variant="secondary"
          className="h-7 gap-1 px-2 text-xs"
          onClick={() => setShowCreate(true)}
        >
          <Plus className="h-3.5 w-3.5" />
          新建索引
        </Button>
      </div>

      {/* Table or empty state */}
      {indexes.length === 0 ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 text-fg-muted">
          <span className="text-sm">该表没有索引</span>
          <Button
            variant="secondary"
            className="h-8 gap-1 text-xs"
            onClick={() => setShowCreate(true)}
          >
            <Plus className="h-3.5 w-3.5" />
            创建第一个索引
          </Button>
        </div>
      ) : (
        <div className="min-h-0 flex-1 overflow-auto">
          <table className="w-full border-collapse text-[13px]">
            <thead className="sticky top-0 z-10">
              <tr className="bg-surface-alt text-left text-xs font-medium text-fg-secondary">
                <th className="border-b border-edge px-4 py-2.5 font-medium">索引名</th>
                <th className="border-b border-edge px-4 py-2.5 font-medium">列</th>
                <th className="border-b border-edge px-4 py-2.5 font-medium">类型</th>
                <th className="border-b border-edge px-4 py-2.5 font-medium">唯一</th>
                <th className="border-b border-edge px-4 py-2.5 font-medium">主键</th>
                <th className="border-b border-edge px-4 py-2.5 font-medium w-16">操作</th>
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
                        title="删除索引"
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
