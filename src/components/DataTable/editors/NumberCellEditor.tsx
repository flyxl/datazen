import { useEffect, useRef } from 'react';
import { cn } from '../../../lib/cn';
import type { CellEditorControlProps } from './CellEditorHost';

/**
 * The numeric editor.
 *
 * The input is a **text** input, not `type="number"`: the browser's number
 * input silently discards what it does not like, so the user gets no
 * explanation and the value they typed is gone. Here the text is kept as
 * typed and the host validates it with a regular expression, which is also
 * what decides whether the value may be written at all.
 *
 * The caret goes to the **end** of the value: appending a digit to a number
 * is the common case, and selecting the whole value would invite a
 * replacement.
 */
export function NumberCellEditor({
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
    const input = inputRef.current;
    if (!input) return;
    input.focus();
    const end = input.value.length;
    input.setSelectionRange(end, end);
  }, [autoFocus]);

  return (
    <input
      ref={inputRef}
      data-testid="table-edit-input"
      inputMode="decimal"
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
        'w-full min-w-0 h-7 bg-surface text-right align-middle font-mono text-xs text-fg',
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
