import { cn } from '../../../lib/cn';
import type { CellEditorControlProps } from './CellEditorHost';

/**
 * The editor for a long text.
 *
 * A single-line input would show a line break as a blank, while the value
 * still carries it — that is worse than not showing it at all (05 §7 #5).
 * A text longer than {@link INLINE_TEXT_MAX_CODE_POINTS} code points, or a
 * text with a line break in it, therefore gets a multi-line editor.
 */
export function LongTextCellEditor({
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
  return (
    <textarea
      data-testid="table-edit-textarea"
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
      rows={4}
      className={cn(
        'w-full min-w-0 resize-none rounded-sm border border-accent bg-surface px-1.5 py-1',
        'font-mono text-xs text-fg outline-none',
        'placeholder:text-fg-muted',
        invalid && 'border-danger',
      )}
    />
  );
}
