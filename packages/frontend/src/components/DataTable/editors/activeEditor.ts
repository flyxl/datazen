/**
 * The one editor that is open, if any.
 *
 * The grid shows at most one editor at a time (05 §4.5 #1), so the open
 * editor can be registered in one place — and a caller that must not lose
 * the user's typing (a page change, a sort, a refresh) can ask it to
 * commit before the navigation happens.
 *
 * The registry holds a **callback**, not a value: the editor owns its
 * draft, and the caller only learns whether the draft is acceptable.
 */
import type { CellValueErrorReason } from '../../../lib/cellEditorKind';

export interface ActiveCellEditor {
  /** The panel the editor belongs to. */
  readonly panelId: string;
  /**
   * Commit the draft.
   *
   * Returns `null` when the draft was written (or the editor was closed),
   * and the reason when it was refused — in which case the editor stays
   * open with its inline error.
   */
  commit(): CellValueErrorReason | null;
}

let activeEditor: ActiveCellEditor | null = null;

/** The editor that is open right now, if any. */
export function getActiveCellEditor(): ActiveCellEditor | null {
  return activeEditor;
}

/**
 * Register the open editor and get the function that removes it again.
 *
 * The editor registers itself when it mounts and unregisters when it
 * unmounts; the return value is that unregistration, so it can be used as
 * the effect's cleanup.
 */
export function registerActiveCellEditor(editor: ActiveCellEditor): () => void {
  activeEditor = editor;
  return () => {
    if (activeEditor === editor) activeEditor = null;
  };
}
