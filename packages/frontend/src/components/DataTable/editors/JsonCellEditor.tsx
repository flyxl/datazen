import { useI18n } from '../../../hooks/useI18n';
import { cn } from '../../../lib/cn';
import type { CellEditorControlProps } from './CellEditorHost';

/**
 * The JSON editor.
 *
 * Two rules make it different from the text editor:
 *
 * - **invalid JSON is refused.** The old editor caught the parse error and
 *   committed the raw text, which put a non-JSON string into a JSON column
 *   and only blew up much later, downstream (05 §7 #9). A JSON *string*
 *   (`"abc"`) is of course valid JSON and stays a string.
 * - **format and minify never change the meaning.** They only re-serialise
 *   what is already there; a draft that does not parse is left alone.
 *
 * The text area is multi-line and monospaced: a JSON document in a
 * single-line input is unreadable, and the grid row is tall enough for a
 * few lines.
 */
export function JsonCellEditor({
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
  const { t } = useI18n();

  const reformat = (indent: number): void => {
    try {
      const parsed: unknown = JSON.parse(draft);
      onDraftChange(JSON.stringify(parsed, null, indent));
    } catch {
      // Not parseable: the inline error says why, the text stays as typed.
    }
  };

  return (
    <div className="flex w-full min-w-0 flex-col gap-1">
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
        rows={3}
        className={cn(
          'w-full min-w-0 resize-none rounded-sm border border-accent bg-surface px-1.5 py-1',
          'font-mono text-xs text-fg outline-none',
          'placeholder:text-fg-muted',
          invalid && 'border-danger',
        )}
      />
      <div className="flex items-center gap-1">
        <button
          type="button"
          data-testid="cell-editor-format-json"
          onClick={() => reformat(2)}
          className="rounded-sm border border-edge px-1.5 text-[10px] text-fg-secondary hover:bg-surface-raised"
        >
          {t('cellEditor.formatJson')}
        </button>
        <button
          type="button"
          data-testid="cell-editor-minify-json"
          onClick={() => reformat(0)}
          className="rounded-sm border border-edge px-1.5 text-[10px] text-fg-secondary hover:bg-surface-raised"
        >
          {t('cellEditor.minifyJson')}
        </button>
      </div>
    </div>
  );
}
