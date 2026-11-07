import { useEffect, useRef } from 'react';
import { TemporalValueInput } from '@datazen/ui';
import type { CellEditorControlProps } from './CellEditorHost';

/**
 * The date / time editor.
 *
 * It wraps the shared `TemporalValueInput`, which produces the canonical
 * strings (`YYYY-MM-DD`, `HH:MM:SS`, `YYYY-MM-DDTHH:MM:SS`) — the same
 * shape a database client shows.
 *
 * **A value without a zone is a wall clock.** The picker never converts the
 * text through `new Date()`: the text the user sees is the text that is
 * written. A column that carries a zone is not edited with this control at
 * all — the picker cannot hold an offset, and dropping it would move the
 * instant (05 §7 #8).
 */
export function TemporalCellEditor({
  draft,
  onDraftChange,
  invalid,
  ariaLabel,
  autoFocus,
  temporalKind = 'date',
  onKeyDown,
  onBlur,
  onCompositionStart,
  onCompositionEnd,
}: CellEditorControlProps) {
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!autoFocus) return;
    rootRef.current?.querySelector('input')?.focus();
  }, [autoFocus]);

  return (
    <div
      ref={rootRef}
      className="flex h-7 w-full items-center"
      onKeyDown={onKeyDown}
      onBlur={onBlur}
      onCompositionStart={onCompositionStart}
      onCompositionEnd={onCompositionEnd}
    >
      <TemporalValueInput
        // The test id also names the pop-up (`…-popover`), which the editor
        // host recognises as a surface it owns.
        data-testid="cell-editor-temporal"
        kind={temporalKind}
        value={draft}
        onChange={onDraftChange}
        invalid={invalid}
      />
      <span className="sr-only">{ariaLabel}</span>
    </div>
  );
}
