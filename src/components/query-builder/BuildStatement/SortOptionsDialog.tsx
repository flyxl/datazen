import { useEffect, useRef, useState } from 'react';
import { useI18n } from '../../../hooks/useI18n';
import { Dialog } from '../../ui/Dialog';
import { Button } from '../../ui/Button';
import { Select, type SelectOption } from '../../ui/Select';
import type { ClauseEntry } from './clauseEntries';

export interface SortOptionsDialogProps {
  /** ORDER BY entry whose chip was clicked; `null` keeps the dialog closed. */
  entry: ClauseEntry | null;
  /** Rendered column reference (`SUM(s.qty)` when aggregated). */
  label: string;
  onApply: (entry: ClauseEntry, direction: 'ASC' | 'DESC') => void;
  onRemove: (entry: ClauseEntry) => void;
  onClose: () => void;
}

/**
 * Options behind one ORDER BY chip.
 *
 * Direction is the only real choice, but it lives in the same dialog pattern as
 * every other chip so the clause list stays uniform: click the item, configure
 * it, or use its × to drop it.
 */
export function SortOptionsDialog({
  entry,
  label,
  onApply,
  onRemove,
  onClose,
}: SortOptionsDialogProps) {
  const { t } = useI18n();
  const [direction, setDirection] = useState<'ASC' | 'DESC'>('ASC');
  const seededRef = useRef<string | null>(null);

  const key = entry ? `${entry.table}.${entry.column}:${entry.source}` : null;

  useEffect(() => {
    if (!entry || !key) {
      seededRef.current = null;
      return;
    }
    if (seededRef.current === key) return;
    seededRef.current = key;
    setDirection(entry.direction ?? 'ASC');
  }, [entry, key]);

  const options: SelectOption[] = [
    { value: 'ASC', label: t('query.visualBuilder.asc') },
    { value: 'DESC', label: t('query.visualBuilder.desc') },
  ];

  return (
    <Dialog
      open={!!entry}
      title={t('query.visualBuilder.sortOptionsTitle')}
      onClose={onClose}
      testId="qb-sort-options"
      className="max-w-sm"
      footer={
        <div className="flex w-full items-center justify-between gap-2">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              if (entry) onRemove(entry);
              // Dismiss before/with the action: a lingering modal overlay makes
              // every control below the dialog unclickable.
              onClose();
            }}
            data-testid="qb-sort-opt-remove"
          >
            {t('query.visualBuilder.removeJoin')}
          </Button>
          <div className="flex items-center gap-2">
            <Button variant="ghost" size="sm" onClick={onClose} data-testid="qb-sort-opt-cancel">
              {t('query.visualBuilder.cancel')}
            </Button>
            <Button
              variant="primary"
              size="sm"
              onClick={() => {
                if (entry) onApply(entry, direction);
                onClose();
              }}
              data-testid="qb-sort-opt-apply"
            >
              {t('query.visualBuilder.ok')}
            </Button>
          </div>
        </div>
      }
    >
      <div className="flex flex-col gap-3">
        <div className="flex items-center justify-between gap-2">
          <span className="text-[11px] text-fg-muted">{t('query.visualBuilder.fieldLabel')}</span>
          <span className="truncate font-mono text-[12px] text-fg" data-testid="qb-sort-opt-field">
            {label}
          </span>
        </div>
        <label className="flex items-center justify-between gap-2">
          <span className="text-[11px] text-fg-muted">{t('query.visualBuilder.sortLabel')}</span>
          <div className="w-44">
            <Select
              value={direction}
              options={options}
              onChange={(v) => setDirection(v as 'ASC' | 'DESC')}
              triggerDataAttrs={{ 'data-testid': 'qb-sort-opt-direction' }}
            />
          </div>
        </label>
      </div>
    </Dialog>
  );
}
