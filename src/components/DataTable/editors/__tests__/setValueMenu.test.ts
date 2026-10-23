import { describe, expect, it } from 'vitest';
import {
  LOAD_CANDIDATES_LABEL_KEY,
  resolveSetValueActions,
  type SetValueState,
} from '../setValueMenu';

const BASE: SetValueState = {
  canNull: true,
  canEmpty: true,
  rowKind: 'update',
  hasPending: false,
};

describe('resolveSetValueActions', () => {
  it('hides Set NULL for NOT NULL columns', () => {
    const actions = resolveSetValueActions({ ...BASE, canNull: false });
    expect(actions.map((action) => action.id)).not.toContain('setNull');
    expect(resolveSetValueActions(BASE).map((action) => action.id)).toContain('setNull');
  });

  it('shows Use column default only for insert rows', () => {
    expect(resolveSetValueActions({ ...BASE, rowKind: 'insert' }).map((a) => a.id)).toContain(
      'setDefault',
    );
    expect(resolveSetValueActions({ ...BASE, rowKind: 'update' }).map((a) => a.id)).not.toContain(
      'setDefault',
    );
  });

  it('shows Revert this cell only when the cell has a pending change', () => {
    expect(resolveSetValueActions({ ...BASE, hasPending: true }).map((a) => a.id)).toContain(
      'revertCell',
    );
    expect(resolveSetValueActions({ ...BASE, hasPending: false }).map((a) => a.id)).not.toContain(
      'revertCell',
    );
  });

  it('never offers Set empty string for a cell that cannot be written', () => {
    expect(resolveSetValueActions({ ...BASE, canEmpty: false }).map((a) => a.id)).not.toContain(
      'setEmpty',
    );
  });

  it('never exposes NOW or CURRENT_TIMESTAMP', () => {
    // They are SQL *expressions*, not values: the three states of a write
    // cannot express them, and a string 'CURRENT_TIMESTAMP' would be written
    // as literal text.
    for (const state of [
      BASE,
      { ...BASE, canNull: false },
      { ...BASE, rowKind: 'insert' as const },
      { ...BASE, hasPending: true },
    ]) {
      const ids = resolveSetValueActions(state).map((action) => action.id);
      for (const forbidden of ['now', 'currentTimestamp', 'current_timestamp']) {
        expect(ids, forbidden).not.toContain(forbidden);
      }
    }
  });

  it('names every entry with an i18n key', () => {
    for (const action of resolveSetValueActions({ ...BASE, rowKind: 'insert', hasPending: true })) {
      expect(action.labelKey).toMatch(/^(dataTable|cellEditor)\./);
    }
    expect(LOAD_CANDIDATES_LABEL_KEY).toBe('cellEditor.loadCandidates');
  });
});
