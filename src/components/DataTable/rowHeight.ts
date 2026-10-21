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
export const DATA_TABLE_ROW_HEIGHT = 28;
