/**
 * The "Set Value" entries — the writes that happen **without** opening an
 * editor (DB-05, 05 §3.6).
 *
 * The rules live here, in one pure place, because they are the part that can
 * be wrong: the menu builder only turns the resulting plan into items.
 *
 * **No SQL expressions.** `NOW()` and `CURRENT_TIMESTAMP` are *expressions*,
 * not values, and the three states of a write cannot express them: the
 * string `'CURRENT_TIMESTAMP'` would be written as literal text. The entries
 * are therefore a closed set — there is no way to offer an expression until
 * the contract grows a fourth intent (05 §11 #1).
 */

/**
 * The write intents the menu can produce.
 *
 * They are the three states of a write, named:
 *
 * - `setNull` — write SQL `NULL`;
 * - `setEmpty` — write the empty string (a value, not a NULL);
 * - `setDefault` — do not write the column at all (a new-row draft keeps the
 *   database's own default);
 * - `revertCell` — the same "do not write", but named for an existing row,
 *   where it undoes a staged change.
 */
export type SetValueActionId = 'setNull' | 'setEmpty' | 'setDefault' | 'revertCell';

export interface SetValueState {
  /** The column accepts SQL `NULL`. */
  readonly canNull: boolean;
  /** The cell can be written at all (an empty string is a value). */
  readonly canEmpty: boolean;
  /** Whether the row exists (`update`) or is a new-row draft (`insert`). */
  readonly rowKind: 'update' | 'insert';
  /** The cell has a staged change that can be undone. */
  readonly hasPending: boolean;
  /** The column is an enumeration whose candidates are not known yet. */
  readonly canLoadCandidates?: boolean;
}

export interface SetValueAction {
  readonly id: SetValueActionId;
  /** The i18n key of the entry's label. */
  readonly labelKey: string;
}

/** The label key of every entry. The menu builder resolves the text. */
const ACTION_LABEL_KEYS: Record<SetValueActionId, string> = {
  setNull: 'dataTable.setNull',
  setEmpty: 'cellEditor.setEmpty',
  setDefault: 'cellEditor.setDefault',
  revertCell: 'cellEditor.revertCell',
};

/** The label key of the "load candidate values" entry. */
export const LOAD_CANDIDATES_LABEL_KEY = 'cellEditor.loadCandidates';

/**
 * The entries the menu offers, in display order.
 *
 * - **Set NULL** needs a nullable column.
 * - **Set empty string** needs a writable cell — an empty string is a
 *   value, and a value is not something a read-only cell can take.
 * - **Use column default** is for a **new row**: it leaves the column out
 *   of the INSERT, so the database fills in its own default.
 * - **Revert this cell** is the same "leave the column out" for an
 *   **existing row**, where it undoes a staged change. It is deliberately
 *   *not* called "use column default" there: an UPDATE that leaves the
 *   column out does not re-apply the DDL default, it only does not touch
 *   the column — and a label that promises a default the database will not
 *   apply is worse than no label at all.
 */
export function resolveSetValueActions(state: SetValueState): readonly SetValueAction[] {
  const actions: SetValueAction[] = [];
  if (state.canNull) actions.push({ id: 'setNull', labelKey: ACTION_LABEL_KEYS.setNull });
  if (state.canEmpty) actions.push({ id: 'setEmpty', labelKey: ACTION_LABEL_KEYS.setEmpty });
  if (state.rowKind === 'insert') {
    actions.push({ id: 'setDefault', labelKey: ACTION_LABEL_KEYS.setDefault });
  }
  if (state.rowKind === 'update' && state.hasPending) {
    actions.push({ id: 'revertCell', labelKey: ACTION_LABEL_KEYS.revertCell });
  }
  return actions;
}
