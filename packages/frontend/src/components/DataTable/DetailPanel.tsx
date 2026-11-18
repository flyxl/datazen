import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Maximize2, Search, X } from 'lucide-react';
import type { ColumnDef } from './TableHeader';
import { QuickLookDialog, isBlobType, isJsonValue, prettyJson } from './QuickLookDialog';
import { toEditString } from './EditableCell';
import { formatCell } from '../../lib/formatters';
import { dataTypeTextClass } from '../../lib/dataTypeColors';
import type { CellTypeHints } from '../../lib/cellTypes';
import { useI18n } from '../../hooks/useI18n';
import { useValuePresentation } from '../../hooks/useValuePresentation';
import { useSessionValuePresentationSnapshot } from '../../hooks/useColumnPresentationSession';
import { ColumnPresentationDialog } from './ColumnPresentationDialog';
import {
  createDefaultValuePresentationSnapshot,
  type ColumnPresentationPlan,
  type ValuePresentationSnapshot,
} from '../../lib/valuePresentation/presentationPlan';
import type {
  ColumnSource,
  PresentationValue,
  ValuePresentationRegistry,
} from '@datazen/extension-points';
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
  /**
   * The driver's own column-type rules (`DatabaseTypeMeta.cellTypes`).
   *
   * The drawer shows the **same columns** as the grid next to it, so its
   * type label must not answer a different question than the grid's
   * (05 §3.1). Omitted → the shared table alone decides, as before.
   */
  cellTypeHints?: CellTypeHints;
  /**
   * Value-presentation wiring (issue-32 T05, design §7) — the same three
   * props as DataTable, with the same defaults: no snapshot ⇒ the shared
   * default snapshot (revision 0, time columns `disabled`, old text);
   * no source ⇒ no table overrides; no registry ⇒ the global
   * `valuePresentationRegistry`. The aside builds its own plans from the
   * same (columns, source, snapshot) triple as the grid, so the same
   * input yields the same plan and the two read identically.
   */
  presentationSnapshot?: ValuePresentationSnapshot;
  presentationSource?: Readonly<Omit<ColumnSource, 'column'>>;
  presentationRegistry?: ValuePresentationRegistry;
}

const EMPTY_SET = new Set<number>();

/**
 * The shared default presentation snapshot (issue-32 T05) — see
 * DataTable's `DEFAULT_PRESENTATION_SNAPSHOT`: one module-level read-only
 * instance, never mutated; time columns stay `disabled` under it.
 */
const DEFAULT_PRESENTATION_SNAPSHOT = createDefaultValuePresentationSnapshot();

export function DetailPanel({
  open,
  columns,
  row,
  rowIndex,
  editable,
  onFieldEdit,
  selectedRows = EMPTY_SET,
  cellTypeHints,
  presentationSnapshot,
  presentationSource,
  presentationRegistry,
}: DetailPanelProps) {
  const { t } = useI18n();
  const [query, setQuery] = useState('');

  const filteredColumns = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return columns;
    return columns.filter((c) => c.name.toLowerCase().includes(q));
  }, [columns, query]);

  /**
   * Value-presentation plans (issue-32 T05): built from the same triple
   * the grid uses, so grid and aside agree cell for cell. This hook call
   * stays above the `!open` early return, like every other hook here.
   *
   * T09: the snapshot first passes through
   * `useSessionValuePresentationSnapshot` (same merge as DataTable),
   * so a column override saved from either entry shows here at once.
   */
  const presentationSnapshotResolved = useSessionValuePresentationSnapshot(
    presentationSnapshot ?? DEFAULT_PRESENTATION_SNAPSHOT,
  );
  /** Column presentation dialog (issue-32 T09): the aside field-menu entry. */
  const [presentationColumn, setPresentationColumn] = useState<string | null>(null);
  const presentationColumnDef = useMemo(
    () => columns.find((c) => c.name === presentationColumn),
    [columns, presentationColumn],
  );
  const planColumns = useMemo(
    () =>
      columns.map((c) => ({
        name: c.name,
        dataType: c.type,
        hints: cellTypeHints,
      })),
    [columns, cellTypeHints],
  );
  const { planByColumn: presentationPlanByColumn } = useValuePresentation({
    columns: planColumns,
    source: presentationSource,
    snapshot: presentationSnapshotResolved,
    registry: presentationRegistry,
  });

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
                  cellTypeHints={cellTypeHints}
                  presentationPlan={presentationPlanByColumn.get(col.name)}
                  onOpenColumnPresentation={() => setPresentationColumn(col.name)}
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

      {presentationColumn !== null && presentationColumnDef !== undefined && (
        <ColumnPresentationDialog
          open
          columnName={presentationColumn}
          columnType={presentationColumnDef.type}
          cellTypeHints={cellTypeHints}
          source={presentationSource}
          snapshot={presentationSnapshotResolved}
          sampleValue={row?.[presentationColumn] as PresentationValue | undefined}
          registry={presentationRegistry}
          onClose={() => setPresentationColumn(null)}
        />
      )}
    </aside>
  );
}

interface FieldRowProps {
  column: ColumnDef;
  value: unknown;
  editable?: boolean;
  onCommit?: (value: unknown) => void;
  /** The driver's own column-type rules — the same ones the grid reads. */
  cellTypeHints?: CellTypeHints;
  /**
   * The column's presentation plan (issue-32 T05). Only the read-only,
   * non-blob display path consults it; the editor still initialises
   * from the raw value (`toEditString(value)`), and NULL stays `NULL`.
   */
  presentationPlan?: ColumnPresentationPlan;
  /**
   * Open the column presentation dialog for this field (issue-32 T09):
   * the aside entry of the shared panel — the same dialog the grid
   * header menu opens, editing the same column configuration.
   */
  onOpenColumnPresentation?: () => void;
}

function FieldRow({
  column,
  value,
  editable,
  onCommit,
  cellTypeHints,
  presentationPlan,
  onOpenColumnPresentation,
}: FieldRowProps) {
  const { t } = useI18n();
  const [quickLookOpen, setQuickLookOpen] = useState(false);
  /**
   * BUG-005: whether this field is being edited. Display is the default;
   * the editor is entered from the display container (click / Enter /
   * Space) and left by commit or Escape — see the branch below.
   */
  const [editing, setEditing] = useState(false);

  const isNull = value === null || value === undefined;
  const type = (column.type ?? '').toLowerCase();
  const isJson = type.includes('json') || isJsonValue(value);
  const isBlob = isBlobType(type);
  const isLongText = type.includes('text') || isJson || isBlob;
  const canQuickLook = isJson || isBlob || (typeof value === 'string' && value.length > 120);
  const pretty = isJson ? prettyJson(value) : null;
  /**
   * The presented value (issue-32 T05): consulted only by the read-only,
   * non-blob `<pre>` below. A `text` result replaces the old
   * `(pretty ?? formatCell(value))` there; `unchanged` (or no plan) keeps
   * it. NULL is decided by `isNull` before this is ever read, the blob
   * path never reads it, and the editor path never sees it.
   */
  const presented = isNull ? undefined : presentationPlan?.present(value as PresentationValue);

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
            <span
              className={cn('font-mono text-[10px]', dataTypeTextClass(column.type, cellTypeHints))}
            >
              {column.type}
            </span>
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
          {onOpenColumnPresentation && (
            <button
              type="button"
              onClick={onOpenColumnPresentation}
              title={t('dataTable.columnPresentation')}
              data-testid={`detail-field-presentation-${column.name}`}
              className="rounded p-0.5 text-fg-muted opacity-0 transition-opacity hover:bg-surface-raised hover:text-fg group-hover:opacity-100"
            >
              {t('dataTable.columnPresentation')}
            </button>
          )}
        </div>
      </div>

      {editable && onCommit && !editing ? (
        // BUG-005: the editable drawer used to render the editor outright,
        // so the presented text (T05 `<pre>` below) was unreachable in the
        // production flow and Escape could never "return to display" — there
        // was no display state to return to. Display is now the default;
        // click / Enter / Space enters the editor, commit or Escape returns.
        <div
          role="button"
          tabIndex={0}
          onClick={() => setEditing(true)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') {
              e.preventDefault();
              setEditing(true);
            }
          }}
        >
          {isBlob && !isNull ? (
            <div className="font-mono text-xs italic text-fg-muted">
              {t('detail.blobData')} ·{' '}
              {typeof value === 'string' ? value.length : formatCell(value).length}{' '}
              {t('detail.bytes')}
            </div>
          ) : (
            <pre
              className={cn(
                'max-h-32 overflow-auto whitespace-pre-wrap break-all font-mono text-xs',
                isNull ? 'italic text-fg-muted' : 'text-fg',
              )}
            >
              {isNull
                ? 'NULL'
                : presented?.kind === 'text'
                  ? presented.text
                  : (pretty ?? formatCell(value))}
            </pre>
          )}
        </div>
      ) : editable && onCommit && editing ? (
        <InlineFieldEditor
          value={value}
          type={type}
          isLongText={isLongText}
          onCommit={(v) => {
            onCommit(v);
            setEditing(false);
          }}
          onCancel={() => setEditing(false)}
          autoFocus
        />
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
          {isNull
            ? 'NULL'
            : presented?.kind === 'text'
              ? presented.text
              : (pretty ?? formatCell(value))}
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

interface InlineFieldEditorProps {
  value: unknown;
  type: string;
  isLongText: boolean;
  onCommit: (value: unknown) => void;
  /**
   * BUG-005: focus the field as soon as the editor mounts, so one click on
   * the display container lands straight in a typeable editor.
   */
  autoFocus?: boolean;
  /**
   * BUG-005: called on Escape, after the editor's own reset (local value
   * restored, internal editing cleared, no commit) — the caller uses it to
   * return to the display state.
   */
  onCancel?: () => void;
}

function InlineFieldEditor({
  value,
  type,
  isLongText,
  onCommit,
  autoFocus,
  onCancel,
}: InlineFieldEditorProps) {
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
        onCancel?.();
        return;
      }
      if (e.key === 'Enter' && !isLongText) {
        e.preventDefault();
        commit();
      }
    },
    [value, isLongText, commit, onCancel],
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
        autoFocus={autoFocus}
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
      autoFocus={autoFocus}
    />
  );
}
