/**
 * The in-cell foreign-key selector (DB-20): the picker that writes a
 * referenced value into a foreign-key cell.
 *
 * The pure rules live here, away from the components, for the same reason
 * every other grid chapter keeps them away: a rule that can be tested
 * without a DOM is a rule that cannot rot while the DOM changes.
 *
 * Two rules run through the module:
 *
 * - **A selector is a single-column key.** A composite foreign key pairs its
 *   members positionally, and the editor writes one cell at a time — so a
 *   pick that would have to stage several cells at once is out of scope, and
 *   a composite group never becomes pickable.
 * - **The picking, not the browsing, needs uniqueness.** Browsing the target
 *   is a plain read; but picking a row whose referenced columns are not
 *   covered by a primary key or a unique index would silently write an
 *   ambiguous reference (07 §7 rule 12), so the uniqueness verdict the
 *   resolver ships with the candidates is enforced at load time.
 */

import type { Value } from '../types';

/**
 * How many target rows one selector offers (DB-20).
 *
 * The resolver's own default (07 §11 U-8): a picker is a glance, not a
 * browser, and the hard cap is the resolver's business anyway.
 */
export const FK_SELECTOR_CANDIDATE_LIMIT = 50;

/**
 * The facts the loader needs to browse one foreign key's target.
 *
 * Derived from the group affordance the grid already holds — never parsed
 * out of `referencedTable`, whose shape differs per driver.
 */
export interface FkCandidateSource {
  /** The local column the write lands on (the group's only member). */
  readonly localColumn: string;
  /** The constraint name; disambiguates two constraints on one column. */
  readonly constraintName: string | null;
}

/**
 * Whether a foreign-key group can drive the in-cell selector.
 *
 * A composite group cannot: one pick would have to write several cells, and
 * the grid's edit pipeline stages exactly one cell per edit.
 */
export function fkGroupIsPickable(localColumns: readonly string[]): boolean {
  return localColumns.length === 1;
}

/**
 * The candidate sources of a table, keyed by the grid column that carries
 * them.
 *
 * Only single-column groups produce an entry: the map is the loader's whole
 * view of what may be picked, so a composite member must not appear here even
 * by accident.
 */
export function indexFkCandidateSources(
  groups: Iterable<readonly [string, { localColumns: readonly string[]; constraintName: string }]>,
): ReadonlyMap<string, FkCandidateSource> {
  const map = new Map<string, FkCandidateSource>();
  for (const [columnName, group] of groups) {
    if (!fkGroupIsPickable(group.localColumns)) continue;
    map.set(columnName, {
      localColumn: group.localColumns[0],
      constraintName: group.constraintName || null,
    });
  }
  return map;
}

/**
 * One candidate row, as the selector displays and commits it.
 *
 * `null` for display: a referenced value cannot be `NULL` under a foreign
 * key, and a candidate that cannot be picked must not be offered — the row
 * is skipped rather than rendered as an unusable "NULL" entry.
 */
export function formatFkCandidate(value: unknown): string | null {
  if (value === null || value === undefined) return null;
  if (typeof value === 'string') return value;
  if (typeof value === 'number' || typeof value === 'bigint') return String(value);
  if (typeof value === 'boolean') return value ? 'true' : 'false';
  // A driver that reports a non-scalar key (a nested JSON key, for example)
  // keeps its text form: the write path formats it by the column's type, the
  // same way a manually typed value travels.
  return String(value);
}

/**
 * The candidates of one browse resolution, ready for the editor.
 *
 * The order is the statement's own order (07 U-8: the command never orders
 * on its own), and duplicates are kept — a picker that silently collapses
 * two identical keys into one would lie about what the table holds.
 */
export function candidatesToPickerOptions(rows: readonly (readonly (Value | null)[])[]): string[] {
  const options: string[] = [];
  for (const row of rows) {
    const formatted = formatFkCandidate(row[0]);
    if (formatted !== null) options.push(formatted);
  }
  return options;
}
