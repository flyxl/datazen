/**
 * The height of one grid row, in pixels.
 *
 * It lives in a module of its own so that the grid and its owners can agree
 * on it without importing each other: the foreign-key navigation turns a row
 * index into a scroll offset (DB-07), and the grid turns that offset back
 * into rows.
 *
 * A module of its own also keeps the value out of the `DataTable` module,
 * whose tests replace that module with a mock — a mock that does not carry
 * the constant would break every importer of it.
 */
import type { GridDensity } from '../../types/settings';
export const DATA_TABLE_ROW_HEIGHT = 28;

/**
 * How each density shifts a grid's own row height (DB-17).
 *
 * A **delta**, not an absolute: the table grid sits at 28, the query-result
 * grid at 32, and both keep their own default under `standard` — the setting
 * shifts a grid's height relative to what that grid already was, so the
 * default changes nothing anywhere.
 */
export const GRID_DENSITY_ROW_HEIGHT_DELTA: Record<GridDensity, number> = {
  compact: -6,
  standard: 0,
  relaxed: 8,
};

/** A grid's row height under a density: its own default, shifted by the delta. */
export function rowHeightForDensity(baseRowHeight: number, density: GridDensity): number {
  return baseRowHeight + GRID_DENSITY_ROW_HEIGHT_DELTA[density];
}
