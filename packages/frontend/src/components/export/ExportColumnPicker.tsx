import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { dataTypeTextClass } from '../../lib/dataTypeColors';

/**
 * The one column picker of both export dialogs (DBR-06: 网格 DataExportDialog
 * 与表级 ExportDialog 共用列选择——同一列表、同一显示顺序、同一组动作).
 *
 * The list preserves the caller's display order — the export writes the
 * columns in exactly this order. "Reset to Default" restores the default
 * selection (every column), so a cleared or half-picked list has one obvious
 * way back instead of being re-toggled by hand.
 */
export interface ExportColumnPickerColumn {
  name: string;
  /** Data type label; greyed next to the name when known. */
  type?: string;
  isPrimaryKey?: boolean;
}

export interface ExportColumnPickerProps {
  /** Columns in display order — the order the export will write. */
  columns: ExportColumnPickerColumn[];
  selected: ReadonlySet<string>;
  onToggle: (name: string) => void;
  onSelectAll: () => void;
  onClearAll: () => void;
  /** Restore the default selection: every column. */
  onResetDefault: () => void;
}

export function ExportColumnPicker({
  columns,
  selected,
  onToggle,
  onSelectAll,
  onClearAll,
  onResetDefault,
}: ExportColumnPickerProps) {
  const { t } = useI18n();

  return (
    <div>
      <div className="mb-1 flex items-center justify-between">
        <label className="text-xs font-medium text-fg-secondary">{t('export.selectColumns')}</label>
        <div className="flex items-center gap-3">
          <button
            type="button"
            data-testid="export-columns-select-all"
            className="text-xs text-accent hover:underline"
            onClick={onSelectAll}
          >
            {t('common.selectAll')}
          </button>
          <button
            type="button"
            data-testid="export-columns-clear-all"
            className="text-xs text-accent hover:underline"
            onClick={onClearAll}
          >
            {t('common.deselectAll')}
          </button>
          <button
            type="button"
            data-testid="export-columns-reset-default"
            className="text-xs text-accent hover:underline"
            onClick={onResetDefault}
          >
            {t('export.resetColumns')}
          </button>
        </div>
      </div>
      <div
        data-testid="export-columns-list"
        className="max-h-40 overflow-y-auto rounded-md border border-edge bg-surface p-2"
      >
        {columns.map((col) => (
          <label
            key={col.name}
            className="flex cursor-pointer items-center gap-2 rounded px-1 py-0.5 hover:bg-surface-raised"
          >
            <input
              type="checkbox"
              data-testid={`export-column-${col.name}`}
              checked={selected.has(col.name)}
              onChange={() => onToggle(col.name)}
              className="accent-accent"
            />
            <span className="text-xs text-fg-secondary">{col.name}</span>
            {col.isPrimaryKey && (
              <span className="rounded bg-accent/15 px-1 text-[10px] font-medium text-accent">
                PK
              </span>
            )}
            {col.type && (
              <span className={cn('text-[10px]', dataTypeTextClass(col.type))}>{col.type}</span>
            )}
          </label>
        ))}
      </div>
    </div>
  );
}
