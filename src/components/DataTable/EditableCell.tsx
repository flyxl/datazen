import { CellEditorHost } from './editors/CellEditorHost';
import { countCodePoints, toEditString } from './editors/toEditString';
import { resolveCellEditorKind } from '../../lib/cellEditorKind';
import { isZonedCellType, resolveCellType } from '../../lib/cellTypes';

/**
 * The compatibility shell of the old editor.
 *
 * The editor itself moved to `editors/`: one file per control family and
 * one place that decides **when** a value is written
 * (`editors/CellEditorHost.tsx`). What is left here is the entry point the
 * new-row drafts still use, plus the re-export of `toEditString`, whose
 * import path other code depends on.
 *
 * The old editor had three defects that the new one does not have: it
 * committed an IME candidate word when the user pressed `Enter`, it wrote
 * the raw text into a JSON column when the text did not parse, and it
 * turned every empty input into a `NULL` on a `NOT NULL` column.
 */
export { toEditString };

export interface EditableCellProps {
  value: unknown;
  type: string;
  onCommit: (value: unknown) => void;
  onCancel: () => void;
}

export function EditableCell({ value, type, onCommit, onCancel }: EditableCellProps) {
  const cellType = resolveCellType(type);
  const text = toEditString(value);
  const kind = resolveCellEditorKind({
    cellType,
    zoned: isZonedCellType(type),
    hasCandidates: false,
    valueCodePoints: countCodePoints(text),
    hasLineBreak: text.includes('\n') || text.includes('\r'),
  });

  return (
    <CellEditorHost
      columnName=""
      dataType={type}
      cellType={cellType}
      kind={kind}
      originalValue={value}
      nullable
      onCommit={(next) => onCommit(next)}
      onCancel={onCancel}
    />
  );
}
