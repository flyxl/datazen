import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FocusEvent,
  type KeyboardEvent,
  type ReactNode,
} from 'react';
import { useI18n } from '../../../hooks/useI18n';
import { isComposingEvent } from '../../../lib/gridEventGuards';
import {
  cellValueErrorMessageKey,
  coerceValue,
  type CellEditorKind,
  type CellValueErrorReason,
} from '../../../lib/cellEditorKind';
import type { CellType } from '../../../lib/cellTypes';
import { normalizeEditValue } from '../../../lib/tableChanges';
import type { Value } from '../../../types';
import { TextCellEditor } from './TextCellEditor';
import { NumberCellEditor } from './NumberCellEditor';
import { BooleanCellEditor } from './BooleanCellEditor';
import { EnumCellEditor } from './EnumCellEditor';
import { TemporalCellEditor } from './TemporalCellEditor';
import { JsonCellEditor } from './JsonCellEditor';
import { LongTextCellEditor } from './LongTextCellEditor';
import { BinaryCellEditor } from './BinaryCellEditor';
import { registerActiveCellEditor } from './activeEditor';
import { toEditString } from './toEditString';

/**
 * The one place that decides **when** a cell edit is written.
 *
 * The per-family editors only edit a value; they never commit, never
 * cancel and never look at the keyboard. Everything about timing — Enter,
 * Escape, Tab, the blur, the IME composition — lives here, so there is one
 * state machine instead of one per control (05 §4.3).
 *
 * Three rules shape it:
 *
 * 1. **The IME owns the keyboard.** While a composition is running, `Enter`
 *    confirms the candidate word and `Escape` dismisses it — neither may
 *    reach the grid. The composition is detected with the shared guard
 *    (`isComposingEvent`) *and* with a local flag, because some engines end
 *    the composition before the last `keydown` arrives.
 * 2. **A blur is not always the end.** Focus may move into a surface the
 *    editor owns (the calendar pop-up, the option list of a select), into
 *    the context menu, or out of an unfinished composition. None of those
 *    may write a value the user did not confirm.
 * 3. **Unmounting never writes.** A row that disappears takes its draft
 *    with it; committing during the unmount would write a value nobody
 *    confirmed.
 */
export interface CellEditorProps {
  /** The panel the cell belongs to; used by the navigation guard. */
  readonly panelId?: string;
  readonly columnName: string;
  readonly dataType: string | undefined;
  readonly cellType: CellType;
  readonly kind: CellEditorKind;
  readonly originalValue: unknown;
  readonly nullable: boolean;
  /** Write the value (already coerced). */
  readonly onCommit: (value: Value | null | undefined) => void;
  /** Close the editor without writing. */
  readonly onCancel: () => void;
  /** Ask for the focus to move to the next or the previous cell. */
  readonly onCommitAndMove?: (direction: 'next' | 'prev') => void;
  /** Report a refused input (for the "N cells to fix" counter). */
  readonly onInvalid?: (reason: CellValueErrorReason) => void;
  /** Read the candidate values of an enumeration column — on request only. */
  readonly loadCandidates?: () => Promise<readonly string[]>;
  /** The column holds whole numbers. */
  readonly integerOnly?: boolean;
}

/**
 * What every control receives.
 *
 * The controls are deliberately dumb: they edit a text or pick a value and
 * report it. They never commit, never cancel and never interpret a key.
 */
export interface CellEditorControlProps {
  /** The text the control shows. */
  readonly draft: string;
  readonly onDraftChange: (next: string) => void;
  /** A discrete choice was made (boolean / enum): it is written right away. */
  readonly onPick?: (value: Value | null) => void;
  /** The column accepts SQL `NULL`. */
  readonly nullable: boolean;
  /** The temporal shape of the column (for the date editor). */
  readonly temporalKind?: 'date' | 'time' | 'datetime';
  /** The current input is not acceptable. */
  readonly invalid: boolean;
  readonly error: CellValueErrorReason | null;
  readonly ariaLabel: string;
  /** The control takes the focus when the editor opens. */
  readonly autoFocus: boolean;
  /** The candidates of an enumeration column, when they are known. */
  readonly candidates?: readonly string[];
  /** Ask for the candidates. */
  readonly onLoadCandidates?: () => void;
  /** The candidates are being fetched. */
  readonly candidatesLoading?: boolean;
  /** The candidates could not be fetched. */
  readonly candidatesFailed?: boolean;
  readonly onKeyDown: (event: KeyboardEvent) => void;
  readonly onBlur: (event: FocusEvent) => void;
  readonly onCompositionStart: () => void;
  readonly onCompositionEnd: () => void;
}

/** The temporal shape of a cell type, for the date editor. */
function temporalKindOf(cellType: CellType): 'date' | 'time' | 'datetime' {
  if (cellType === 'time') return 'time';
  if (cellType === 'datetime') return 'datetime';
  return 'date';
}

/**
 * Surfaces the editor owns.
 *
 * A blur into one of them must not write a value: the calendar pop-up and
 * the option list of a select are rendered into `document.body` (a portal),
 * so they are not inside the editor's own element and can only be told
 * apart by their markers.
 */
const OWNED_SURFACE_SELECTORS = [
  // The editor's own root, and anything it marks as its own.
  '[data-dt-editor-owned]',
  // The shared select renders its option list into a portal.
  '[data-testid="select-listbox"]',
  // The temporal picker renders its pop-up into a portal.
  '[data-testid="cell-editor-temporal-popover"]',
  // The host's web context menu (the "Set Value" menu).
  '[role="menu"]',
];

/** Whether the focus moved into a surface the editor owns. */
export function isOwnedEditorTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return OWNED_SURFACE_SELECTORS.some((selector) => target.closest(selector) !== null);
}

export function CellEditorHost({
  panelId,
  dataType: _dataType,
  cellType,
  kind,
  originalValue,
  nullable,
  onCommit,
  onCancel,
  onCommitAndMove,
  onInvalid,
  loadCandidates,
  integerOnly,
}: CellEditorProps) {
  const { t } = useI18n();
  const initial = useRef(toEditString(originalValue));
  const [draft, setDraft] = useState(() => toEditString(originalValue));
  const [error, setError] = useState<CellValueErrorReason | null>(null);
  const [candidates, setCandidates] = useState<readonly string[] | undefined>(undefined);
  const [candidatesLoading, setCandidatesLoading] = useState(false);
  const [candidatesFailed, setCandidatesFailed] = useState(false);
  /** The editor is done: it must not write twice. */
  const done = useRef(false);
  /** An IME composition is running. */
  const composingRef = useRef(false);
  /** A right-click is on its way: the blur it causes must not write. */
  const contextMenuRef = useRef(false);
  /** The last refusal, for the navigation guard. */
  const errorRef = useRef<CellValueErrorReason | null>(null);

  useEffect(() => {
    const text = toEditString(originalValue);
    initial.current = text;
    setDraft(text);
  }, [originalValue]);

  useEffect(() => {
    errorRef.current = error;
  }, [error]);

  const cancel = useCallback(() => {
    if (done.current) return;
    done.current = true;
    onCancel();
  }, [onCancel]);

  const commit = useCallback(
    (direction?: 'next' | 'prev'): boolean => {
      if (done.current) return true;
      // Nothing changed: the editor closes without writing anything.
      if (draft === initial.current) {
        done.current = true;
        onCancel();
        return true;
      }
      const result = coerceValue({
        kind,
        cellType,
        text: draft,
        nullable,
        originalValue,
        integerOnly,
      });
      if (!result.ok) {
        setError(result.reason);
        errorRef.current = result.reason;
        onInvalid?.(result.reason);
        return false;
      }
      done.current = true;
      setError(null);
      errorRef.current = null;
      onCommit(normalizeEditValue(result.value));
      if (direction) onCommitAndMove?.(direction);
      return true;
    },
    [
      cellType,
      draft,
      integerOnly,
      kind,
      nullable,
      onCancel,
      onCommit,
      onCommitAndMove,
      onInvalid,
      originalValue,
    ],
  );

  /** A discrete choice (boolean / enum) is written right away. */
  const pick = useCallback(
    (value: Value | null) => {
      if (done.current) return;
      done.current = true;
      setError(null);
      errorRef.current = null;
      onCommit(normalizeEditValue(value));
    },
    [onCommit],
  );

  const handleKeyDown = useCallback(
    (event: KeyboardEvent) => {
      // The IME owns the keyboard while it composes: `Enter` confirms the
      // candidate word and `Escape` dismisses it. Neither may write.
      if (isComposingEvent(event) || composingRef.current) return;
      if (event.key === 'Enter') {
        event.preventDefault();
        commit();
        return;
      }
      if (event.key === 'Escape') {
        event.preventDefault();
        cancel();
        return;
      }
      if (event.key === 'Tab') {
        event.preventDefault();
        commit(event.shiftKey ? 'prev' : 'next');
      }
    },
    [cancel, commit],
  );

  const handleBlur = useCallback(
    (event: FocusEvent) => {
      // An unfinished composition owns the focus.
      if (composingRef.current) return;
      // A right-click opens the in-page context menu, and the blur it causes
      // must not write: the menu may still change this very cell.
      if (contextMenuRef.current) {
        contextMenuRef.current = false;
        return;
      }
      // Focus moved into a surface the editor owns (calendar, option list).
      if (isOwnedEditorTarget(event.relatedTarget)) return;
      commit();
    },
    [commit],
  );

  const handleCompositionStart = useCallback(() => {
    composingRef.current = true;
  }, []);

  const handleCompositionEnd = useCallback(() => {
    composingRef.current = false;
  }, []);

  // The right-click that opens the context menu is detected on `mousedown`,
  // because the menu itself appears after the focus change it causes.
  useEffect(() => {
    const onMouseDown = (event: MouseEvent): void => {
      if (event.button === 2) contextMenuRef.current = true;
    };
    document.addEventListener('mousedown', onMouseDown, true);
    return () => document.removeEventListener('mousedown', onMouseDown, true);
  }, []);

  // The navigation guard asks the open editor to commit before the page
  // moves under it (05 §4.4). **Without a panel id the editor is not
  // registered, and the guard is silently disarmed** — so the omission is
  // reported instead of being swallowed.
  useEffect(() => {
    if (!panelId) {
      if (import.meta.env.DEV) {
        console.warn(
          '[grid] the cell editor has no panel id, so it is not registered with the navigation guard',
        );
      }
      return;
    }
    return registerActiveCellEditor({
      panelId,
      commit: () => {
        const written = commit();
        return written ? null : errorRef.current;
      },
    });
  }, [commit, panelId]);

  const handleLoadCandidates = useCallback(() => {
    if (!loadCandidates) return;
    setCandidatesLoading(true);
    setCandidatesFailed(false);
    void loadCandidates()
      .then((values) => {
        setCandidates(values);
        setCandidatesLoading(false);
      })
      .catch(() => {
        setCandidatesFailed(true);
        setCandidatesLoading(false);
      });
  }, [loadCandidates]);

  const controlProps: CellEditorControlProps = useMemo(
    () => ({
      draft,
      onDraftChange: (next: string) => {
        setDraft(next);
        if (error) {
          setError(null);
          errorRef.current = null;
        }
      },
      onPick: pick,
      invalid: error !== null,
      error,
      nullable,
      temporalKind: temporalKindOf(cellType),
      ariaLabel: t('cellEditor.ariaLabel'),
      autoFocus: true,
      candidates,
      onLoadCandidates: handleLoadCandidates,
      candidatesLoading,
      candidatesFailed,
      onKeyDown: handleKeyDown,
      onBlur: handleBlur,
      onCompositionStart: handleCompositionStart,
      onCompositionEnd: handleCompositionEnd,
    }),
    [
      candidates,
      candidatesFailed,
      candidatesLoading,
      draft,
      error,
      nullable,
      cellType,
      handleBlur,
      handleCompositionEnd,
      handleCompositionStart,
      handleKeyDown,
      handleLoadCandidates,
      pick,
      t,
    ],
  );

  const control: ReactNode = (() => {
    switch (kind) {
      case 'text':
        return <TextCellEditor {...controlProps} />;
      case 'longText':
        return <LongTextCellEditor {...controlProps} />;
      case 'number':
        return <NumberCellEditor {...controlProps} />;
      case 'boolean':
        return <BooleanCellEditor {...controlProps} />;
      case 'enum':
        return <EnumCellEditor {...controlProps} />;
      case 'date':
        return <TemporalCellEditor {...controlProps} />;
      case 'json':
        return <JsonCellEditor {...controlProps} />;
      case 'binary':
        return <BinaryCellEditor {...controlProps} />;
    }
  })();

  return (
    <div
      data-testid="cell-editor"
      data-dt-editor-owned="true"
      data-dt-editor-kind={kind}
      {...(error ? { 'data-dt-editor-error': error } : {})}
      className="flex w-full min-w-0 flex-col"
    >
      {control}
      {error ? (
        <span
          data-testid="cell-editor-error"
          role="alert"
          className="truncate text-[10px] text-danger"
        >
          {t(cellValueErrorMessageKey(error))}
        </span>
      ) : null}
    </div>
  );
}
