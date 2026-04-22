import { useCallback, useEffect, useState } from 'react';
import { GripVertical, Loader2, Plus, Trash2 } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Input } from '../../components/ui/Input';
import { Select } from '../../components/ui/Select';
import { databaseCommands } from '../../commands/database';
import { queryCommands } from '../../commands/query';
import { cn } from '../../lib/cn';
import { useI18n } from '../../hooks/useI18n';

const PG_TYPES = [
  { value: 'integer', label: 'integer' },
  { value: 'bigint', label: 'bigint' },
  { value: 'smallint', label: 'smallint' },
  { value: 'serial', label: 'serial' },
  { value: 'bigserial', label: 'bigserial' },
  { value: 'numeric', label: 'numeric' },
  { value: 'real', label: 'real' },
  { value: 'double precision', label: 'double precision' },
  { value: 'boolean', label: 'boolean' },
  { value: 'varchar(255)', label: 'varchar(255)' },
  { value: 'varchar(50)', label: 'varchar(50)' },
  { value: 'varchar(100)', label: 'varchar(100)' },
  { value: 'text', label: 'text' },
  { value: 'char(1)', label: 'char(1)' },
  { value: 'date', label: 'date' },
  { value: 'time', label: 'time' },
  { value: 'timestamp', label: 'timestamp' },
  { value: 'timestamptz', label: 'timestamptz' },
  { value: 'uuid', label: 'uuid' },
  { value: 'json', label: 'json' },
  { value: 'jsonb', label: 'jsonb' },
  { value: 'bytea', label: 'bytea' },
  { value: 'text[]', label: 'text[]' },
  { value: 'integer[]', label: 'integer[]' },
  { value: 'inet', label: 'inet' },
  { value: 'cidr', label: 'cidr' },
  { value: 'money', label: 'money' },
];

interface ColumnDef {
  id: string;
  name: string;
  type: string;
  nullable: boolean;
  defaultValue: string;
  isPrimaryKey: boolean;
  isUnique: boolean;
  comment: string;
}

function newColumnId() {
  return `col_${Math.random().toString(36).slice(2, 8)}`;
}

function emptyColumn(): ColumnDef {
  return {
    id: newColumnId(),
    name: '',
    type: 'varchar(255)',
    nullable: true,
    defaultValue: '',
    isPrimaryKey: false,
    isUnique: false,
    comment: '',
  };
}

interface TableStructureEditorProps {
  connectionId: string;
  mode: 'create' | 'alter';
  tableName?: string;
  initialColumns?: ColumnDef[];
  onSuccess: () => void;
  onCancel: () => void;
}

function quoteIdent(s: string): string {
  return `"${s.replaceAll('"', '""')}"`;
}

function generateCreateSQL(name: string, columns: ColumnDef[]): string {
  const lines: string[] = [];
  const pks = columns.filter((c) => c.isPrimaryKey).map((c) => quoteIdent(c.name));

  for (const col of columns) {
    if (!col.name.trim()) continue;
    let line = `  ${quoteIdent(col.name)} ${col.type}`;
    if (!col.nullable) line += ' NOT NULL';
    if (col.defaultValue.trim()) line += ` DEFAULT ${col.defaultValue}`;
    if (col.isUnique && !col.isPrimaryKey) line += ' UNIQUE';
    lines.push(line);
  }

  if (pks.length > 0) {
    lines.push(`  PRIMARY KEY (${pks.join(', ')})`);
  }

  let sql = `CREATE TABLE ${quoteIdent(name)} (\n${lines.join(',\n')}\n);`;

  for (const col of columns) {
    if (col.comment.trim()) {
      sql += `\nCOMMENT ON COLUMN ${quoteIdent(name)}.${quoteIdent(col.name)} IS '${col.comment.replaceAll("'", "''")}';`;
    }
  }

  return sql;
}

function generateAlterSQL(name: string, original: ColumnDef[], current: ColumnDef[]): string {
  const stmts: string[] = [];
  const qName = quoteIdent(name);
  const origMap = new Map(original.map((c) => [c.id, c]));
  const currMap = new Map(current.map((c) => [c.id, c]));

  // Dropped columns
  for (const orig of original) {
    if (!currMap.has(orig.id) && orig.name.trim()) {
      stmts.push(`ALTER TABLE ${qName} DROP COLUMN ${quoteIdent(orig.name)};`);
    }
  }

  // Added or modified columns
  for (const col of current) {
    if (!col.name.trim()) continue;
    const orig = origMap.get(col.id);
    if (!orig?.name.trim()) {
      let line = `ALTER TABLE ${qName} ADD COLUMN ${quoteIdent(col.name)} ${col.type}`;
      if (!col.nullable) line += ' NOT NULL';
      if (col.defaultValue.trim()) line += ` DEFAULT ${col.defaultValue}`;
      if (col.isUnique) line += ' UNIQUE';
      stmts.push(`${line};`);
      if (col.comment.trim()) {
        stmts.push(`COMMENT ON COLUMN ${qName}.${quoteIdent(col.name)} IS '${col.comment.replaceAll("'", "''")}';`);
      }
    } else {
      if (col.type !== orig.type) {
        stmts.push(`ALTER TABLE ${qName} ALTER COLUMN ${quoteIdent(col.name)} TYPE ${col.type};`);
      }
      if (col.nullable !== orig.nullable) {
        stmts.push(`ALTER TABLE ${qName} ALTER COLUMN ${quoteIdent(col.name)} ${col.nullable ? 'DROP NOT NULL' : 'SET NOT NULL'};`);
      }
      if (col.defaultValue !== orig.defaultValue) {
        if (col.defaultValue.trim()) {
          stmts.push(`ALTER TABLE ${qName} ALTER COLUMN ${quoteIdent(col.name)} SET DEFAULT ${col.defaultValue};`);
        } else {
          stmts.push(`ALTER TABLE ${qName} ALTER COLUMN ${quoteIdent(col.name)} DROP DEFAULT;`);
        }
      }
      if (col.comment !== orig.comment && col.comment.trim()) {
        stmts.push(`COMMENT ON COLUMN ${qName}.${quoteIdent(col.name)} IS '${col.comment.replaceAll("'", "''")}';`);
      }
    }
  }

  return stmts.join('\n');
}

export function TableStructureEditor({
  connectionId,
  mode,
  tableName: initialTableName,
  initialColumns,
  onSuccess,
  onCancel,
}: TableStructureEditorProps) {
  const { t } = useI18n();
  const [tableName, setTableName] = useState(initialTableName ?? '');
  const [columns, setColumns] = useState<ColumnDef[]>(
    initialColumns?.length ? initialColumns : [
      { ...emptyColumn(), name: 'id', type: 'bigserial', nullable: false, isPrimaryKey: true },
      { ...emptyColumn(), name: '', type: 'varchar(255)', nullable: true },
    ],
  );
  const [originalColumns, setOriginalColumns] = useState<ColumnDef[]>([]);
  const [loadingSchema, setLoadingSchema] = useState(false);
  const [executing, setExecuting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [previewSql, setPreviewSql] = useState<string | null>(null);
  const [dragIdx, setDragIdx] = useState<number | null>(null);

  // Load existing table schema in alter mode
  useEffect(() => {
    if (mode !== 'alter' || !initialTableName) return;
    let cancelled = false;
    setLoadingSchema(true);
    databaseCommands
      .getTableSchema(connectionId, initialTableName)
      .then((schema) => {
        if (cancelled) return;
        const cols: ColumnDef[] = schema.columns.map((c) => ({
          id: newColumnId(),
          name: c.name,
          type: c.dataType,
          nullable: c.nullable,
          defaultValue: c.defaultValue ?? '',
          isPrimaryKey: schema.primaryKeys.includes(c.name),
          isUnique: schema.indexes.some((idx) => idx.isUnique && !idx.isPrimary && idx.columns.includes(c.name)),
          comment: c.comment ?? '',
        }));
        setColumns(cols);
        setOriginalColumns(cols.map((c) => ({ ...c })));
        setLoadingSchema(false);
      })
      .catch((e) => {
        if (cancelled) return;
        let msg = t('structEditor.loadFailed');
        if (typeof e === 'string') msg = e;
        else if (e instanceof Error) msg = e.message;
        setError(msg);
        setLoadingSchema(false);
      });
    return () => { cancelled = true; };
  }, [mode, connectionId, initialTableName, t]);

  const updateColumn = useCallback((id: string, patch: Partial<ColumnDef>) => {
    setColumns((prev) => prev.map((c) => (c.id === id ? { ...c, ...patch } : c)));
  }, []);

  const addColumn = useCallback(() => {
    setColumns((prev) => [...prev, emptyColumn()]);
  }, []);

  const removeColumn = useCallback((id: string) => {
    setColumns((prev) => prev.filter((c) => c.id !== id));
  }, []);

  const handleDragStart = useCallback((idx: number) => {
    setDragIdx(idx);
  }, []);

  const handleDragOver = useCallback((e: React.DragEvent, targetIdx: number) => {
    e.preventDefault();
    if (dragIdx === null || dragIdx === targetIdx) return;
    setColumns((prev) => {
      const next = [...prev];
      const [moved] = next.splice(dragIdx, 1);
      next.splice(targetIdx, 0, moved);
      return next;
    });
    setDragIdx(targetIdx);
  }, [dragIdx]);

  const validColumns = columns.filter((c) => c.name.trim());

  const buildSql = useCallback(() => {
    if (!tableName.trim() || validColumns.length === 0) return '';
    if (mode === 'alter') return generateAlterSQL(tableName.trim(), originalColumns, columns);
    return generateCreateSQL(tableName.trim(), validColumns);
  }, [mode, tableName, validColumns, columns, originalColumns]);

  const handlePreview = useCallback(() => {
    const sql = buildSql();
    if (sql) setPreviewSql(sql);
  }, [buildSql]);

  const handleExecute = useCallback(async () => {
    const sql = buildSql();
    if (!sql) return;
    setError(null);
    setExecuting(true);
    try {
      await queryCommands.executeQuery(connectionId, sql);
      onSuccess();
    } catch (e) {
      const msg = typeof e === 'string' ? e : (e instanceof Error ? e.message : t('structEditor.executeFailed'));
      setError(msg);
    } finally {
      setExecuting(false);
    }
  }, [connectionId, buildSql, onSuccess]);

  const isValid = tableName.trim().length > 0 && validColumns.length > 0;

  if (loadingSchema) {
    return (
      <div className="flex flex-1 items-center justify-center gap-2 text-fg-muted">
        <Loader2 className="h-5 w-5 animate-spin" />
        {t('structEditor.loading')}
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* Header */}
      <div className="flex items-center gap-4 border-b border-edge bg-surface-alt px-4 py-3">
        <span className="text-base font-semibold text-fg">
          {mode === 'create' ? t('structEditor.newTable') : `${t('structEditor.editTable')} · ${initialTableName}`}
        </span>
        <div className="flex-1" />
        <Button variant="secondary" className="h-8 text-xs" onClick={handlePreview} disabled={!isValid}>
          {t('structEditor.previewSQL')}
        </Button>
        <Button variant="secondary" className="h-8 text-xs" onClick={onCancel}>
          {t('common.cancel')}
        </Button>
        <Button
          variant="primary"
          className="h-8 text-xs"
          disabled={!isValid || executing}
          onClick={() => void handleExecute()}
        >
          {executing ? t('structEditor.executing') : (mode === 'create' ? t('structEditor.createTable') : t('structEditor.saveChanges'))}
        </Button>
      </div>

      {/* Table name */}
      {mode === 'create' && (
        <div className="flex items-center gap-3 border-b border-edge px-4 py-3">
          <label className="text-sm text-fg-secondary">{t('structEditor.tableName')}</label>
          <Input
            value={tableName}
            onChange={(e) => setTableName(e.target.value)}
            placeholder="new_table"
            className="h-8 max-w-xs text-sm"
          />
        </div>
      )}

      {/* Column editor */}
      <div className="min-h-0 flex-1 overflow-auto">
        <table className="w-full border-collapse text-[13px]">
          <thead className="sticky top-0 z-10">
            <tr className="bg-surface-alt text-left text-xs font-medium text-fg-secondary">
              <th className="w-8 border-b border-edge px-1 py-2.5" />
              <th className="min-w-[140px] border-b border-edge px-2 py-2.5 font-medium">{t('structView.fieldName')}</th>
              <th className="min-w-[160px] border-b border-edge px-2 py-2.5 font-medium">{t('structView.type')}</th>
              <th className="w-[60px] border-b border-edge px-2 py-2.5 text-center font-medium">{t('structView.nullable')}</th>
              <th className="w-[60px] border-b border-edge px-2 py-2.5 text-center font-medium">{t('structView.primaryKey')}</th>
              <th className="w-[60px] border-b border-edge px-2 py-2.5 text-center font-medium">{t('structView.unique')}</th>
              <th className="min-w-[120px] border-b border-edge px-2 py-2.5 font-medium">{t('structView.defaultValue')}</th>
              <th className="min-w-[120px] border-b border-edge px-2 py-2.5 font-medium">{t('structView.comment')}</th>
              <th className="w-10 border-b border-edge px-1 py-2.5" />
            </tr>
          </thead>
          <tbody>
            {columns.map((col, idx) => (
              <tr
                key={col.id}
                draggable
                onDragStart={() => handleDragStart(idx)}
                onDragOver={(e) => handleDragOver(e, idx)}
                onDragEnd={() => setDragIdx(null)}
                className={cn(
                  'border-b border-edge bg-surface transition-colors hover:bg-surface-alt/50',
                  dragIdx === idx && 'opacity-50',
                )}
              >
                <td className="px-1 py-1.5 text-center">
                  <GripVertical className="mx-auto h-3.5 w-3.5 cursor-grab text-fg-muted" />
                </td>
                <td className="px-2 py-1.5">
                  <Input
                    value={col.name}
                    onChange={(e) => updateColumn(col.id, { name: e.target.value })}
                    placeholder="column_name"
                    className="h-7 text-xs"
                  />
                </td>
                <td className="px-2 py-1.5">
                  <Select
                    value={col.type}
                    options={PG_TYPES}
                    onChange={(v) => updateColumn(col.id, { type: v })}
                    className="h-7 text-xs"
                  />
                </td>
                <td className="px-2 py-1.5 text-center">
                  <input
                    type="checkbox"
                    checked={col.nullable}
                    onChange={(e) => updateColumn(col.id, { nullable: e.target.checked })}
                    className="h-3.5 w-3.5 accent-accent"
                  />
                </td>
                <td className="px-2 py-1.5 text-center">
                  <input
                    type="checkbox"
                    checked={col.isPrimaryKey}
                    onChange={(e) => updateColumn(col.id, { isPrimaryKey: e.target.checked, nullable: e.target.checked ? false : col.nullable })}
                    className="h-3.5 w-3.5 accent-accent"
                  />
                </td>
                <td className="px-2 py-1.5 text-center">
                  <input
                    type="checkbox"
                    checked={col.isUnique}
                    onChange={(e) => updateColumn(col.id, { isUnique: e.target.checked })}
                    className="h-3.5 w-3.5 accent-accent"
                  />
                </td>
                <td className="px-2 py-1.5">
                  <Input
                    value={col.defaultValue}
                    onChange={(e) => updateColumn(col.id, { defaultValue: e.target.value })}
                    placeholder="NULL"
                    className="h-7 text-xs"
                  />
                </td>
                <td className="px-2 py-1.5">
                  <Input
                    value={col.comment}
                    onChange={(e) => updateColumn(col.id, { comment: e.target.value })}
                    placeholder=""
                    className="h-7 text-xs"
                  />
                </td>
                <td className="px-1 py-1.5 text-center">
                  <button
                    type="button"
                    className="rounded p-1 text-fg-muted hover:bg-red-500/10 hover:text-red-400"
                    onClick={() => removeColumn(col.id)}
                    title={t('structEditor.deleteColumn')}
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>

        {/* Add column */}
        <div className="px-4 py-3">
          <Button variant="secondary" className="h-8 gap-1 text-xs" onClick={addColumn}>
            <Plus className="h-3.5 w-3.5" />
            {t('structEditor.addColumn')}
          </Button>
        </div>
      </div>

      {/* Error */}
      {error && (
        <div className="border-t border-red-500/20 bg-red-500/10 px-4 py-2.5 text-xs text-red-400">
          {error}
        </div>
      )}

      {/* SQL Preview */}
      {previewSql && (
        <div className="border-t border-edge bg-surface-alt">
          <div className="flex items-center justify-between px-4 py-2">
            <span className="text-xs font-medium text-fg-secondary">{t('structEditor.sqlPreview')}</span>
            <button
              type="button"
              className="text-xs text-fg-muted hover:text-fg"
              onClick={() => setPreviewSql(null)}
            >
              {t('common.close')}
            </button>
          </div>
          <pre className="max-h-40 overflow-auto px-4 pb-3 font-mono text-xs leading-relaxed text-fg-secondary">
            {previewSql}
          </pre>
        </div>
      )}
    </div>
  );
}
