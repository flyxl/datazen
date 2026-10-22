/**
 * The "Set Value" entries of the grid (DB-05).
 *
 * The rules live in `src/lib/setValueActions.ts` — they are write-intent
 * semantics, and the menu builder (`src/lib/dataTableContextMenu.ts`) has
 * to read them without climbing into the component layer. This module is
 * the editor's view of them: the same names, reachable from the editor's
 * own folder.
 */
export {
  LOAD_CANDIDATES_LABEL_KEY,
  resolveSetValueActions,
  type SetValueAction,
  type SetValueActionId,
  type SetValueState,
} from '../../../lib/setValueActions';
