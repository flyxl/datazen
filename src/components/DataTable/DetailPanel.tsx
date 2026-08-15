import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Maximize2, Search, X } from 'lucide-react';
import { Button } from '../ui/Button';
import { Dialog } from '../ui/Dialog';
import type { ColumnDef } from './TableHeader';
import { toEditString } from './EditableCell';
import { formatCell } from '../../lib/formatters';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';

export interface DetailPanelProps {
  open: boolean;
  columns: ColumnDef[];
  row: Record<string, unknown> | null;
  rowIndex: number | null;
  editable?: boolean;
  onFieldEdit?: (row: number, col: string, value: unknown) => void;
  /** Indices of currently selected rows — used to surface bulk-edit mode. */
  selectedRows?: Set<number>;
}

const EMPTY_SET = new Set<number>();

const BLOB_TYPE_KEYWORDS = ['blob', 'binary', 'bytea', 'image', 'raw'];

function isBlobType(type: string): boolean {
  return BLOB_TYPE_KEYWORDS.some((k) => type.includes(k));
}

function isJsonValue(v: unknown): boolean {
  if (v === null || v === undefined) return false;
  if (typeof v === 'object') return true;
  if (typeof v === 'string') {
    const s = v.trim();
    if (!s.startsWith('{') && !s.startsWith('[')) return false;
    try {
      JSON.parse(s);
      return true;
    } catch {
      return false;
    }
  }
  return false;
}

function prettyJson(v: unknown): string | null {
  if (v === null || v === undefined) return null;
  let parsed: unknown = v;
  if (typeof v === 'string') {
    const s = v.trim();
    if (!s.startsWith('{') && !s.startsWith('[')) return null;
    try {
      parsed = JSON.parse(s);
    } catch {
      return null;
    }
  }
  if (typeof parsed !== 'object') return null;
  try {
    return JSON.stringify(parsed, null, 2);
  } catch {
    return null;
  }
}

export function DetailPanel({
  open,
  columns,
  row,
  rowIndex,
  editable,
  onFieldEdit,
  selectedRows = EMPTY_SET,
}: DetailPanelProps) {
  const { t } = useI18n();
  const [query, setQuery] = useState('');

  const filteredColumns = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return columns;
    return columns.filter((c) => c.name.toLowerCase().includes(q));
  }, [columns, query]);

  if (!open) return null;

  const multiRow = rowIndex !== null && selectedRows.size > 1;

  return (
    <aside className="flex h-full w-72 shrink-0 flex-col border-l border-edge bg-surface-alt">
      <div className="flex h-10 shrink-0 items-center justify-between gap-2 border-b border-edge px-3">
        <span className="text-xs font-semibold uppercase tracking-wider text-fg-muted">
          {t('detail.title')}
        </span>
        {multiRow && (
          <span className="shrink-0 rounded-full bg-accent/15 px-2 py-0.5 text-[10px] font-medium text-accent">
            {t('detail.editingRows', { count: String(selectedRows.size) })}
          </span>
        )}
      </div>

      {row === null ? (
        <div className="flex flex-1 items-center justify-center px-4 text-center text-xs text-fg-muted">
          {t('detail.noSelection')}
        </div>
      ) : (
        <>
          <div className="relative shrink-0 border-b border-edge/50 px-3 py-2">
            <Search className="pointer-events-none absolute left-5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-fg-muted" />
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t('detail.searchColumns')}
              className="h-7 w-full rounded-sm border border-edge/50 bg-surface-raised/30 pl-8 pr-7 font-mono text-xs text-fg placeholder:text-fg-muted focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent/30"
            />
            {query && (
              <button
                type="button"
                onClick={() => setQuery('')}
                title={t('detail.clearSearch')}
                className="absolute right-4 top-1/2 -translate-y-1/2 rounded p-0.5 text-fg-muted hover:bg-surface-raised hover:text-fg"
              >
                <X className="h-3 w-3" />
              </button>
            )}
          </div>

          <div className="flex-1 overflow-y-auto">
            {filteredColumns.length === 0 ? (
              <div className="flex flex-1 items-center justify-center px-4 py-10 text-center text-xs text-fg-muted">
                {t('detail.noColumnsFound')}
              </div>
            ) : (
              filteredColumns.map((col) => (
                <FieldRow
                  key={col.id}
                  column={col}
                  value={row[col.name]}
                  editable={editable}
                  onCommit={
                    editable && onFieldEdit && rowIndex !== null
                      ? (v) => onFieldEdit(rowIndex, col.name, v)
                      : undefined
                  }
                />
              ))
            )}
          </div>
        </>
      )}
    </aside>
  );
}

interface FieldRowProps {
  column: ColumnDef;
  value: unknown;
  editable?: boolean;
  onCommit?: (value: unknown) => void;
}

function FieldRow({ column, value, editable, onCommit }: FieldRowProps) {
  const { t } = useI18n();
  const [quickLookOpen, setQuickLookOpen] = useState(false);

  const isNull = value === null || value === undefined;
  const type = (column.type ?? '').toLowerCase();
  const isJson = type.includes('json') || isJsonValue(value);
  const isBlob = isBlobType(type);
  const isLongText = type.includes('text') || isJson || isBlob;
  const canQuickLook = isJson || isBlob || (typeof value === 'string' && value.length > 120);
  const pretty = isJson ? prettyJson(value) : null;

  return (
    <div
      className="selectable group border-b border-edge/50 px-3 py-2.5"
      onMouseDown={(e) => {
        if (e.button === 1 && canQuickLook) {
          e.preventDefault();
          setQuickLookOpen(true);
        }
      }}
    >
      <div className="mb-1 flex items-center justify-between gap-2">
        <span className="truncate text-xs font-medium text-fg-secondary" title={column.name}>
          {column.name}
        </span>
        <div className="flex shrink-0 items-center gap-1.5">
          {column.type && (
            <span className="font-mono text-[10px] text-fg-muted">{column.type}</span>
          )}
          {canQuickLook && (
            <button
              type="button"
              onClick={() => setQuickLookOpen(true)}
              title={t('detail.quickLook')}
              className="rounded p-0.5 text-fg-muted opacity-0 transition-opacity hover:bg-surface-raised hover:text-fg group-hover:opacity-100"
            >
              <Maximize2 className="h-3 w-3" />
            </button>
          )}
        </div>
      </div>

      {editable && onCommit ? (
        <InlineFieldEditor value={value} type={type} isLongText={isLongText} onCommit={onCommit} />
      ) : isBlob && !isNull ? (
        <div className="font-mono text-xs italic text-fg-muted">
          {t('detail.blobData')} ·{' '}
          {typeof value === 'string' ? value.length : formatCell(value).length} {t('detail.bytes')}
        </div>
      ) : (
        <pre
          className={cn(
            'max-h-32 overflow-auto whitespace-pre-wrap break-all font-mono text-xs',
            isNull ? 'italic text-fg-muted' : 'text-fg',
          )}
        >
          {isNull ? 'NULL' : (pretty ?? formatCell(value))}
        </pre>
      )}

      {canQuickLook && (
        <QuickLookDialog
          open={quickLookOpen}
          column={column}
          value={value}
          editable={editable}
          onCommit={onCommit}
          onClose={() => setQuickLookOpen(false)}
        />
      )}
    </div>
  );
}

interface QuickLookDialogProps {
  open: boolean;
  column: ColumnDef;
  value: unknown;
  editable?: boolean;
  onCommit?: (value: unknown) => void;
  onClose: () => void;
}

function QuickLookDialog({
  open,
  column,
  value,
  editable,
  onCommit,
  onClose,
}: QuickLookDialogProps) {
  const { t } = useI18n();
  const pretty = prettyJson(value);
  const [local, setLocal] = useState(pretty ?? toEditString(value));

  useEffect(() => {
    if (open) setLocal(pretty ?? toEditString(value));
  }, [open, value, pretty]);

  const canEdit = editable === true && onCommit !== undefined;
  const type = (column.type ?? '').toLowerCase();

  const handleSave = useCallback(() => {
    if (!onCommit) return;
    if (local === '') {
      onCommit(null);
      onClose();
      return;
    }
    if (pretty !== null) {
      try {
        onCommit(JSON.parse(local));
      } catch {
        onCommit(local);
      }
    } else if (type.includes('int') || type.includes('serial') || type.includes('bigint')) {
      onCommit(Number(local));
    } else if (type.includes('bool')) {
      onCommit(local === 'true');
    } else if (
      type.includes('float') ||
      type.includes('double') ||
      type.includes('numeric') ||
      type.includes('decimal') ||
      type.includes('real')
    ) {
      onCommit(Number(local));
    } else {
      onCommit(local);
    }
    onClose();
  }, [onCommit, local, pretty, type, onClose]);

  return (
    <Dialog
      open={open}
      title={column.name}
      description={
        column.type ? `${t('detail.quickLook')} · ${column.type}` : t('detail.quickLook')
      }
      onClose={onClose}
      className="max-w-2xl"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            {t('common.cancel')}
          </Button>
          {canEdit && (
            <Button variant="primary" onClick={handleSave}>
              {t('detail.save')}
            </Button>
          )}
        </>
      }
    >
      {canEdit ? (
        <textarea
          className="max-h-[50vh] w-full resize-y rounded-md border border-edge bg-surface p-3 font-mono text-xs text-fg focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent/30"
          value={local}
          onChange={(e) => setLocal(e.target.value)}
          rows={12}
          wrap="off"
        />
      ) : (
        <pre className="max-h-[50vh] overflow-auto whitespace-pre-wrap break-all rounded-md border border-edge bg-surface p-3 font-mono text-xs text-fg">
          {local || 'NULL'}
        </pre>
      )}
    </Dialog>
  );
}

interface InlineFieldEditorProps {
  value: unknown;
  type: string;
  isLongText: boolean;
  onCommit: (value: unknown) => void;
}

function InlineFieldEditor({ value, type, isLongText, onCommit }: InlineFieldEditorProps) {
  const isNull = value === null || value === undefined;
  const display = toEditString(value);
  const [local, setLocal] = useState(display);
  const [editing, setEditing] = useState(false);
  const ref = useRef<HTMLTextAreaElement | HTMLInputElement>(null);

  useEffect(() => {
    if (!editing) setLocal(toEditString(value));
  }, [value, editing]);

  const commit = useCallback(() => {
    setEditing(false);
    const trimmed = local;
    if (trimmed === toEditString(value)) return;
    if (trimmed === '' && isNull) return;
    if (trimmed === '') {
      onCommit(null);
      return;
    }
    if (type.includes('int') || type.includes('serial') || type.includes('bigint')) {
      onCommit(Number(trimmed));
      return;
    }
    if (type.includes('bool')) {
      onCommit(trimmed === 'true');
      return;
    }
    if (
      type.includes('float') ||
      type.includes('double') ||
      type.includes('numeric') ||
      type.includes('decimal') ||
      type.includes('real')
    ) {
      onCommit(Number(trimmed));
      return;
    }
    if (type.includes('json')) {
      try {
        onCommit(JSON.parse(trimmed));
      } catch {
        onCommit(trimmed);
      }
      return;
    }
    onCommit(trimmed);
  }, [local, value, isNull, type, onCommit]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Escape') {
        setLocal(toEditString(value));
        setEditing(false);
        return;
      }
      if (e.key === 'Enter' && !isLongText) {
        e.preventDefault();
        commit();
      }
    },
    [value, isLongText, commit],
  );

  const sharedClass = cn(
    'w-full rounded-sm border px-2 py-1 font-mono text-xs transition-colors',
    editing
      ? 'border-accent bg-surface text-fg ring-1 ring-accent/30'
      : 'border-edge/50 bg-surface-raised/30 text-fg hover:border-edge',
  );

  if (isLongText) {
    return (
      <textarea
        ref={ref as React.RefObject<HTMLTextAreaElement>}
        className={cn(sharedClass, 'min-h-[28px] resize-y')}
        value={local}
        onChange={(e) => setLocal(e.target.value)}
        onFocus={() => setEditing(true)}
        onBlur={commit}
        onKeyDown={handleKeyDown}
        rows={1}
        wrap="off"
      />
    );
  }

  return (
    <input
      ref={ref as React.RefObject<HTMLInputElement>}
      className={cn(sharedClass, 'h-7')}
      value={local}
      onChange={(e) => setLocal(e.target.value)}
      onFocus={() => setEditing(true)}
      onBlur={commit}
      onKeyDown={handleKeyDown}
    />
  );
}
