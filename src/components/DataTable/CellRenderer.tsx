import { memo } from 'react';
import { formatCell, formatTimestamp } from '../../lib/formatters';
import { cellValueTextClass, classifyDataType } from '../../lib/dataTypeColors';
import { cn } from '../../lib/cn';
import { EditableCell } from './EditableCell';

export interface CellRendererProps {
  columnName: string;
  dataType?: string;
  value: unknown;
  isEditing: boolean;
  onCommit: (value: unknown) => void;
  onCancel: () => void;
}

export const CellRenderer = memo(function CellRenderer({
  columnName: _columnName,
  dataType,
  value,
  isEditing,
  onCommit,
  onCancel,
}: CellRendererProps) {
  const type = (dataType ?? '').toLowerCase();

  if (isEditing) {
    return <EditableCell value={value} type={type} onCommit={onCommit} onCancel={onCancel} />;
  }

  if (value === null || value === undefined) {
    return <span className="truncate italic text-dt-null">NULL</span>;
  }

  const family = classifyDataType(dataType);
  const colorClass = cellValueTextClass(dataType, value);

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
