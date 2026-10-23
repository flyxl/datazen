import { memo, useCallback, useMemo } from 'react';
import { formatCell, formatTimestamp } from '../../lib/formatters';
import { cellValueTextClassFor } from '../../lib/dataTypeColors';
import { cn } from '../../lib/cn';
import { CellEditorHost } from './editors/CellEditorHost';
import { countCodePoints, toEditString } from './editors/toEditString';
import { resolveCellEditorKind } from '../../lib/cellEditorKind';
import {
  cellTypeToDataTypeFamily,
  isZonedCellType,
  resolveCellType,
  type CellType,
} from '../../lib/cellTypes';
import type { Value } from '../../types';

export interface CellRendererProps {
  columnName: string;
  dataType?: string;
  value: unknown;
  isEditing: boolean;
  /** The normalised type of the column (DB-05). */
  cellType?: CellType;
  /** The column accepts SQL `NULL` (DB-05). */
  nullable?: boolean;
  /** The column holds whole numbers (DB-05). */
  integerOnly?: boolean;
  /** The panel the cell belongs to (DB-05). */
  panelId?: string;
  onCommit: (value: Value | null | undefined) => void;
  /** Write with an explicit intent; takes precedence over `onCommit`. */
  onCellValue?: (value: Value | null | undefined) => void;
  onCancel: () => void;
  /** Read the candidate values of an enumeration column, on request only. */
  loadCandidates?: (columnName: string) => Promise<readonly string[]>;
}

export const CellRenderer = memo(function CellRenderer({
  columnName,
  dataType,
  value,
  isEditing,
  cellType,
  nullable = true,
  integerOnly = false,
  panelId,
  onCommit,
  onCellValue,
  onCancel,
  loadCandidates,
}: CellRendererProps) {
  /**
   * The editor control of this cell.
   *
   * The kind is derived from the normalised type, never from a substring
   * test of the driver's type name — that is the whole point of the type
   * table (`src/lib/cellTypes.ts`).
   */
  const editorKind = useMemo(() => {
    const resolved = cellType ?? resolveCellType(dataType);
    const text = toEditString(value);
    return resolveCellEditorKind({
      cellType: resolved,
      zoned: isZonedCellType(dataType),
      hasCandidates: false,
      valueCodePoints: countCodePoints(text),
      hasLineBreak: text.includes('\n') || text.includes('\r'),
    });
  }, [cellType, dataType, value]);

  const loadCandidatesForColumn = useCallback(
    () => (loadCandidates ? loadCandidates(columnName) : Promise.resolve([])),
    [columnName, loadCandidates],
  );

  if (isEditing) {
    return (
      <CellEditorHost
        panelId={panelId}
        columnName={columnName}
        dataType={dataType}
        cellType={cellType ?? resolveCellType(dataType)}
        kind={editorKind}
        originalValue={value}
        nullable={nullable}
        integerOnly={integerOnly}
        onCommit={onCellValue ?? onCommit}
        onCancel={onCancel}
        loadCandidates={loadCandidates ? loadCandidatesForColumn : undefined}
      />
    );
  }

  if (value === null || value === undefined) {
    return <span className="truncate italic text-dt-null">NULL</span>;
  }

  // The colour reads the **normalised** type, not the type name: the cell
  // and its editor then cannot give two answers (05 §3.4).
  const family = cellTypeToDataTypeFamily(cellType ?? resolveCellType(dataType));
  const colorClass = cellValueTextClassFor(cellType ?? resolveCellType(dataType), value);

  if (family === 'bool') {
    return <span className={cn('truncate font-mono text-sm', colorClass)}>{String(value)}</span>;
  }

  if (family === 'number') {
    return (
      <span className={cn('truncate text-right font-mono text-sm', colorClass)}>
        {String(value)}
      </span>
    );
  }

  if (family === 'datetime') {
    return (
      <span className={cn('truncate font-mono text-xs', colorClass)} title={String(value)}>
        {formatTimestamp(value)}
      </span>
    );
  }

  if (family === 'json') {
    const jsonText = formatCell(value);
    return (
      <span className={cn('truncate font-mono text-xs', colorClass)} title={jsonText}>
        {jsonText.length > 120 ? `${jsonText.slice(0, 120)}…` : jsonText}
      </span>
    );
  }

  const text = typeof value === 'object' ? JSON.stringify(value) : String(value);
  return (
    <span className={cn('truncate text-sm', colorClass)} title={text}>
      {text.length > 120 ? `${text.slice(0, 120)}…` : text}
    </span>
  );
});
