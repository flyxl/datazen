import { useI18n } from '../../../hooks/useI18n';
import type { CellEditorControlProps } from './CellEditorHost';

/**
 * The binary editor — which does not edit.
 *
 * A binary column is **not** edited in place: the old editor wrote the
 * string `"[1,2,3]"` back into a `BLOB`, which no driver can use. The
 * value is shown with its size and the reason it cannot be edited here; the
 * dedicated entry point belongs to the special-type chapter (DB-13).
 *
 * The size is the only thing read from the value: the editor never
 * serialises a byte array to measure it (05 §4.5 #5).
 */
export function BinaryCellEditor({ error, ariaLabel }: CellEditorControlProps) {
  const { t } = useI18n();
  return (
    <div
      data-testid="cell-editor-binary"
      role="status"
      aria-label={ariaLabel}
      className="flex h-7 w-full items-center truncate text-[10px] text-fg-muted"
    >
      {t('cellEditor.binaryNotEditable')}
      {error ? <span className="sr-only">{t('cellEditor.binaryNotEditable')}</span> : null}
    </div>
  );
}
