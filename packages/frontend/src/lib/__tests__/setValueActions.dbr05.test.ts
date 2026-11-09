import { describe, expect, it } from 'vitest';
import { resolveSetValueActions, type SetValueState } from '../setValueActions';

/**
 * The two dialog entries of the "Set Value" menu (DBR-05): the date/datetime
 * picker and the binary "load from file". They are *entries that choose a
 * value*, so the contract has three parts: they appear exactly when their
 * gate allows, they sit after the value intents, and a state that carries
 * neither keeps the menu the closed set had before DBR-05.
 */

const BASE: SetValueState = {
  canNull: true,
  canEmpty: true,
  rowKind: 'update',
  hasPending: false,
};

describe('resolveSetValueActions — the dialog entries (DBR-05)', () => {
  it('offers the picker when the column is a plain, writable date/datetime', () => {
    const ids = resolveSetValueActions({ ...BASE, canPickDate: true }).map((a) => a.id);
    expect(ids).toContain('pickDate');
  });

  it('offers the binary file loader when the column is writable binary', () => {
    const ids = resolveSetValueActions({ ...BASE, canLoadBinary: true }).map((a) => a.id);
    expect(ids).toContain('loadBinaryFromFile');
  });

  it('places the dialog entries after the value intents', () => {
    const ids = resolveSetValueActions({
      ...BASE,
      hasPending: true,
      canPickDate: true,
      canLoadBinary: true,
    }).map((a) => a.id);
    expect(ids).toEqual(['setNull', 'setEmpty', 'revertCell', 'pickDate', 'loadBinaryFromFile']);
  });

  it('keeps the pre-DBR-05 set when the state carries neither gate', () => {
    const ids = resolveSetValueActions({ ...BASE, hasPending: true }).map((a) => a.id);
    expect(ids).toEqual(['setNull', 'setEmpty', 'revertCell']);
  });

  it('does not offer a dialog entry on an explicit false gate', () => {
    const ids = resolveSetValueActions({ ...BASE, canPickDate: false, canLoadBinary: false }).map(
      (a) => a.id,
    );
    expect(ids).not.toContain('pickDate');
    expect(ids).not.toContain('loadBinaryFromFile');
  });
});
