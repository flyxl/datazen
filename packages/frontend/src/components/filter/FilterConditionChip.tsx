import { X } from 'lucide-react';
import type { FilterCondition } from '../../types';
import type { I18nKey } from '../../locales';
import { conditionSummary } from './filterLabels';

/**
 * A collapsed condition: "name = alice" with an edit and a remove button.
 *
 * The chip is where the user reads the filter most of the time, so it also
 * carries the per-condition connector badge (see `FilterConnectorToggle`).
 */
export function FilterConditionChip({
  filter,
  onEdit,
  onRemove,
  disabled = false,
  t,
}: {
  filter: FilterCondition;
  onEdit: () => void;
  onRemove: () => void;
  disabled?: boolean;
  t: (key: I18nKey) => string;
}) {
  const label = conditionSummary(filter, t);
  return (
    <div
      className="inline-flex max-w-full items-center gap-0.5 rounded-full border border-accent/30 bg-accent/10 py-0.5 pl-2 pr-0.5 text-xs text-accent"
      aria-disabled={disabled || undefined}
    >
      <button
        type="button"
        className="selectable min-w-0 truncate px-0.5 text-left hover:underline"
        onClick={onEdit}
        disabled={disabled}
        aria-disabled={disabled || undefined}
        title={t('filter.editCondition')}
      >
        {label}
      </button>
      <button
        type="button"
        className="shrink-0 rounded-full p-0.5 text-accent/80 hover:bg-accent/20 hover:text-accent"
        onClick={onRemove}
        disabled={disabled}
        aria-disabled={disabled || undefined}
        aria-label={t('filter.remove')}
      >
        <X className="h-3 w-3" />
      </button>
    </div>
  );
}
