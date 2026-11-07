import type { KeyboardEvent } from 'react';
import { Checkbox, Select, type SelectOption } from '@datazen/ui';
import { cn } from '../../../lib/cn';
import type { CellEditorControlProps } from './CellEditorHost';

/** The option that stands for SQL `NULL` in the three-state control. */
const NULL_OPTION = '__null__';

const OPTIONS: readonly SelectOption[] = [
  { value: NULL_OPTION, label: 'NULL' },
  { value: 'true', label: 'true' },
  { value: 'false', label: 'false' },
];

/**
 * The boolean editor.
 *
 * A nullable column gets **three** states, because two of them are the same
 * wire value with a different meaning: `NULL` is not `false`. A single
 * checkbox cannot express that, so the nullable case is a three-item select
 * and the NOT NULL case is the themed checkbox (05 §3.5, §11 #2).
 *
 * `Enter` belongs to the select — it opens the option list — so it is not
 * forwarded to the host. The value is written when an option is picked.
 */
export function BooleanCellEditor({
  draft,
  onPick,
  nullable,
  ariaLabel,
  onKeyDown,
  onBlur,
}: CellEditorControlProps) {
  if (!nullable) {
    const checked = draft === 'true';
    return (
      <label className="flex h-7 w-full items-center justify-center">
        <Checkbox
          data-testid="cell-editor-boolean"
          checked={checked}
          aria-label={ariaLabel}
          onChange={(event) => onPick?.(event.target.checked)}
          onKeyDown={onKeyDown}
          onBlur={onBlur}
        />
      </label>
    );
  }

  const selected = draft === 'true' || draft === 'false' ? draft : NULL_OPTION;
  const handleKeyDown = (event: KeyboardEvent): void => {
    // The select owns `Enter`: it opens the option list.
    if (event.key === 'Enter') {
      event.preventDefault();
      event.stopPropagation();
      return;
    }
    onKeyDown(event);
  };

  return (
    <div className="flex h-7 w-full items-center" onKeyDown={handleKeyDown} onBlur={onBlur}>
      <Select
        data-testid="cell-editor-boolean"
        triggerDataAttrs={{ 'aria-label': ariaLabel }}
        className={cn('w-full')}
        value={selected}
        options={OPTIONS}
        onChange={(value) => {
          if (value === NULL_OPTION) onPick?.(null);
          else onPick?.(value === 'true');
        }}
      />
    </div>
  );
}
