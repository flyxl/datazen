/**
 * App settings types.
 *
 * `AppSettings` — the type the app actually uses — lives in `./index`, and the
 * filter and sort types that used to be duplicated here now live there too, in
 * one place. The remaining `connection.ts` / `settings.ts` duplication is a
 * separate cleanup.
 *
 * The three settings below are the ones a table read consults before it asks
 * the host for rows. They live here — not in `@datazen/backend-client` — because
 * they are **host** preferences: the shared transport carries a whitelist of
 * portable preferences, and these two are about how a grid reads a table.
 */
export type {
  SettingsMcpPermissionMode as McpPermissionMode,
  SettingsSqlFormatOptions as SqlFormatOptions,
  SettingsSqlExecutionStrategy as SqlExecutionStrategy,
} from '@datazen/backend-client';

/**
 * How a table read obtains its row count.
 *
 * - `auto` — the host does not force a strategy: the driver declares what it
 *   can do, and the grid shows whatever the driver answered. This is today's
 *   behaviour, and it is the default for that reason.
 * - `exact` — always `COUNT(*)`, however expensive.
 * - `estimated` — ask for an estimate when the database has one; the host
 *   falls back to an exact count when it does not.
 * - `none` — do not count at all; the pager then shows the page number only.
 */
export type RowCountMode = 'auto' | 'exact' | 'estimated' | 'none';

/**
 * The order a table read uses when the user did not ask for one.
 *
 * - `primaryKeyAsc` — the driver's own default (today's behaviour).
 * - `primaryKeyDesc` — the primary key, descending.
 * - `none` — no order at all; paging may then repeat or skip rows, and the
 *   grid says so.
 */
export type DefaultTableSort = 'primaryKeyAsc' | 'primaryKeyDesc' | 'none';

/** The default row-count mode: nobody forces a strategy, the driver decides. */
export const DEFAULT_ROW_COUNT_MODE: RowCountMode = 'auto';

/**
 * The default threshold of the `auto` mode: above this many rows an automatic
 * count estimates instead of counting. A table of 200 001 rows is not worth a
 * full `COUNT(*)`.
 */
export const DEFAULT_ROW_COUNT_ESTIMATE_THRESHOLD = 200_000;

/** The default order: the driver's own default, which is the primary key ascending. */
export const DEFAULT_TABLE_SORT: DefaultTableSort = 'primaryKeyAsc';

/**
 * The row density of the data grids (DB-17).
 *
 * - `compact` — tighter rows, more of the table on screen.
 * - `standard` — what every grid did before this setting existed; the default.
 * - `relaxed` — taller rows, easier to read across.
 *
 * The setting is a **delta**, not an absolute height: each grid keeps its own
 * default row height (the table grid and the query-result grid differ), and
 * the density shifts it. `standard` is a delta of zero, so the default changes
 * nothing anywhere — the PRD's "do not change defaults" holds per grid.
 */
export type GridDensity = 'compact' | 'standard' | 'relaxed';

/** The default density: a delta of zero, the behaviour before the setting existed. */
export const DEFAULT_GRID_DENSITY: GridDensity = 'standard';

/**
 * Alternating row backgrounds (DB-17). Default `true`: the grid has striped
 * odd rows since it exists — turning it off is the opt-in.
 */
export const DEFAULT_GRID_ALTERNATING_ROWS = true;

/**
 * The row-number gutter (DB-17). Default `true`: the `#` column has been
 * there since the grid exists. Turning it off trades the gutter's
 * click-to-select affordance for one more column of data.
 */
export const DEFAULT_GRID_ROW_NUMBERS = true;

/**
 * Auto-hide grid scrollbars (DB-17). Default `false`: scrollbars stay visible —
 * the pre-setting behaviour. On, they appear only while the grid is hovered.
 */
export const DEFAULT_GRID_AUTO_HIDE_SCROLLBAR = false;
