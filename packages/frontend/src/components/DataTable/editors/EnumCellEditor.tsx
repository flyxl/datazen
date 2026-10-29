import type { KeyboardEvent } from 'react';
import { Select, type SelectOption } from '@datazen/ui';
import { useI18n } from '../../../hooks/useI18n';
import { cn } from '../../../lib/cn';
import type { CellEditorControlProps } from './CellEditorHost';

/**
 * The enumeration editor.
 *
 * The candidates are **never** fetched on their own: opening the editor
 * sends no query at all. The user asks for them with the button, and only
 * then does the host run one `SELECT DISTINCT … LIMIT 200` (05 §4.3 #5).
 * A grid that queries a table on every double click is a grid that stutters.
 *
 * Until the candidates are known the editor shows the button next to a plain
 * input, so a value can always be typed by hand — and when the load is
 * refused (the selector's uniqueness refusal, DB-20) the refusal's own
 * sentence sits next to them instead of the generic failure text.
 */
export function EnumCellEditor({
  draft,
  onDraftChange,
  onPick,
  ariaLabel,
  candidates,
  onLoadCandidates,
  candidatesLoading,
  candidatesFailed,
  candidatesMessage,
  onKeyDown,
  onBlur,
}: CellEditorControlProps) {
  const { t } = useI18n();

  const handleKeyDown = (event: KeyboardEvent): void => {
    // The select owns `Enter`: it opens the option list.
    if (event.key === 'Enter') {
      event.preventDefault();
      event.stopPropagation();
      return;
    }
    onKeyDown(event);
  };

  if (!candidates || candidates.length === 0) {
    return (
      <div className="flex h-7 w-full items-center gap-1" onKeyDown={handleKeyDown} onBlur={onBlur}>
        {onLoadCandidates ? (
          <button
            type="button"
            data-testid="cell-editor-load-candidates"
            disabled={candidatesLoading}
            onClick={onLoadCandidates}
            className="truncate rounded-sm border border-edge px-1.5 text-[10px] text-fg-secondary hover:bg-surface-raised disabled:opacity-60"
          >
            {candidatesLoading ? t('cellEditor.loadingCandidates') : t('cellEditor.loadCandidates')}
          </button>
        ) : null}
        {/* A refused load never strands the editor (DB-20): the uniqueness
            refusal explains itself next to the button, and the input stays
            so the value can still be typed by hand. */}
        <input
          data-testid="table-edit-input"
          autoFocus
          value={draft}
          onChange={(event) => onDraftChange(event.target.value)}
          onKeyDown={handleKeyDown}
          onBlur={onBlur}
          aria-label={ariaLabel}
          spellCheck={false}
          className={cn(
            'w-full min-w-0 h-7 bg-surface font-mono text-xs text-fg outline-none',
            'rounded-sm border border-accent px-1.5',
          )}
        />
        {candidatesFailed ? (
          <span
            data-testid="cell-editor-candidates-failed"
            className="shrink-0 text-[10px] text-danger"
          >
            {candidatesMessage || t('cellEditor.candidatesFailed')}
          </span>
        ) : null}
      </div>
    );
  }

  const options: SelectOption[] = candidates.map((candidate) => ({
    value: candidate,
    label: candidate,
  }));
  const selected = options.some((option) => option.value === draft) ? draft : '';

  return (
    <div className="flex h-7 w-full items-center" onKeyDown={handleKeyDown} onBlur={onBlur}>
      <Select
        data-testid="cell-editor-enum"
        triggerDataAttrs={{ 'aria-label': ariaLabel }}
        searchable
        className="w-full"
        value={selected}
        options={options}
        onChange={(value) => onPick?.(value)}
      />
    </div>
  );
}
