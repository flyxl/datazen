import { useEffect, useRef } from 'react';
import { cn } from '../../../lib/cn';
import type { CellEditorControlProps } from './CellEditorHost';

/**
 * The single-line text editor.
 *
 * The text is selected when the editor opens: the user is about to replace
 * the value, and a caret in the middle of a word is a trap. The numeric and
 * temporal editors do the opposite (the caret goes to the end), because
 * there the user usually appends.
 */
export function TextCellEditor({
  draft,
  onDraftChange,
  invalid,
  ariaLabel,
  autoFocus,
  onKeyDown,
  onBlur,
  onCompositionStart,
  onCompositionEnd,
}: CellEditorControlProps) {
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!autoFocus) return;
    inputRef.current?.select();
  }, [autoFocus]);

  return (
    <input
      ref={inputRef}
      data-testid="table-edit-input"
      autoFocus={autoFocus}
      value={draft}
      onChange={(event) => onDraftChange(event.target.value)}
      onKeyDown={onKeyDown}
      onBlur={onBlur}
      onCompositionStart={onCompositionStart}
      onCompositionEnd={onCompositionEnd}
      aria-label={ariaLabel}
      aria-invalid={invalid || undefined}
      spellCheck={false}
      autoCapitalize="off"
      autoCorrect="off"
      className={cn(
        // Grid-cell editor: fill the cell edge-to-edge with a compact height
        // and minimal inset so it reads as part of the table, not a heavy
        // form input.
        'w-full min-w-0 h-7 bg-surface align-middle font-mono text-xs text-fg',
        'outline-none',
        'placeholder:text-fg-muted',
        'selection:bg-accent/30',
        'rounded-sm border border-accent shadow-none',
        'px-1.5',
        invalid && 'border-danger',
      )}
    />
  );
}
