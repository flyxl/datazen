/**
 * Compile-time guard for the frozen `rowsCoveredBySelection` signature
 * (00-contracts §4.2.1, 落地校正): `selectedRows` is **required** on `GridSelectionState`.
 *
 * A bare `GridSelection` must not compile. If the field ever becomes optional again, the
 * `@ts-expect-error` below turns into TS2578 ("unused '@ts-expect-error' directive") and
 * `pnpm typecheck` fails — the regression cannot sneak back in as "silently empty row set".
 */
import { describe, expect, it } from 'vitest';
import { EMPTY_GRID_SELECTION, rowsCoveredBySelection } from '../gridSelection';

describe('rowsCoveredBySelection frozen signature (§4.2.1)', () => {
  it('does not accept a bare GridSelection because selectedRows is required', () => {
    const covered = rowsCoveredBySelection(
      // @ts-expect-error — selectedRows is required on GridSelectionState (00-contracts §4.2.1)
      EMPTY_GRID_SELECTION,
    );
    // The none branch still reports "no rows" at runtime, but callers must pass the set.
    expect(covered.size).toBe(0);
  });

  it('accepts the same selection once the row set is supplied', () => {
    const covered = rowsCoveredBySelection({
      ...EMPTY_GRID_SELECTION,
      selectedRows: new Set<number>(),
    });
    expect(covered.size).toBe(0);
  });
});
