import { useEffect, useState } from 'react';
import { TemporalValueInput } from '@datazen/ui';
import { Button } from '../../components/ui/Button';
import { Dialog } from '../../components/ui/Dialog';
import { useI18n } from '../../hooks/useI18n';

/**
 * The date / datetime picker behind the "Set Value" menu's picker entry
 * (DBR-05).
 *
 * The picker is the shared `TemporalValueInput`, so the value it composes is
 * the canonical string (`YYYY-MM-DD`, `YYYY-MM-DDTHH:MM:SS`) — the same shape
 * the in-place editor writes, never a `new Date()` conversion. **A value
 * without a zone is a wall clock** (05 §7 #8); a zoned column never reaches
 * this dialog at all (`resolveSetValueState` keeps it on the text path).
 *
 * Applying stages through the same `stageCellValue` path as every other
 * write — the dialog only chooses the value. A cleared picker applies a
 * SQL `NULL`, exactly like an editor emptied by hand.
 */
export interface SetValueDateDialogProps {
  open: boolean;
  /** The column being picked for — its name is the dialog's title. */
  columnName: string;
  /** The column's type name, shown for orientation. */
  columnType?: string;
  /** `date` or `datetime` — which picker to mount. */
  kind: 'date' | 'datetime';
  /** The cell's current value, as the picker's initial text. */
  value: string;
  onApply: (next: string) => void;
  onClose: () => void;
}

export function SetValueDateDialog({
  open,
  columnName,
  columnType,
  kind,
  value,
  onApply,
  onClose,
}: SetValueDateDialogProps) {
  const { t } = useI18n();
  const [draft, setDraft] = useState(value);

  useEffect(() => {
    if (open) setDraft(value);
  }, [open, value]);

  return (
    <Dialog
      open={open}
      title={columnName}
      description={
        columnType ? `${t('dataTable.pickDate')} · ${columnType}` : t('dataTable.pickDate')
      }
      onClose={onClose}
      className="max-w-md"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            {t('common.cancel')}
          </Button>
          <Button
            variant="primary"
            data-testid="set-value-date-apply"
            onClick={() => {
              onApply(draft);
              onClose();
            }}
          >
            {t('common.confirm')}
          </Button>
        </>
      }
    >
      <div className="p-4">
        <TemporalValueInput
          data-testid="set-value-date-picker"
          kind={kind}
          value={draft}
          onChange={setDraft}
        />
      </div>
    </Dialog>
  );
}
