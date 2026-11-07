/**
 * The row contract every ARIA tree in DataZen shares.
 *
 * Two independent trees paint `role="tree"` rows — the host's connection
 * navigator (`src/windows/connection/navigator/`) and the Redis key browser
 * (`crates/drivers/redis/ui/key-browser/`) — and each used to derive a row's
 * hierarchy, indent and announced level from private arithmetic that had
 * already drifted from the other's. These types are the one place that
 * arithmetic is written down: `geometry.ts` turns them into numbers,
 * `navigation.ts` answers the structural questions a flat pre-order row list
 * raises, and neither module knows anything about React, the DOM, or a driver.
 *
 * The shapes are deliberately *capabilities* rather than one wide row type.
 * Both trees model rows as discriminated unions whose "does this row own
 * children" answer is a `type` / `kind` fork rather than a field
 * (`namespace-node` carries `isLeaf`, but `db-loading` and `no-connections`
 * carry nothing at all), so the structural helpers take an injected
 * `isBranch` predicate — exactly like `nextNavigableIndex` has always taken an
 * injected `isNavigable` — instead of pretending a union is flat. A future row
 * model that does carry `hasChildren` gets the default and passes nothing.
 */

/**
 * `aria-level` is 1-based while a painted depth is 0-based, so the two are
 * never the same number. The conversion lives in exactly one function
 * ({@link ariaLevelOf} in `./geometry`) rather than as `depth + 1` repeated at
 * every call site, which is how the navigator and the key browser ended up
 * disagreeing about what a level meant.
 */
export const TREE_TOP_LEVEL = 1;

/** The one thing geometry needs: where a row is painted, and where it announces. */
export interface TreeRowLevel {
  /**
   * Painted depth, 0-based. Drives indentation, and deliberately does **not**
   * move when a filter removes one of the rows between a parent and its
   * children — the indent is a picture of the absolute hierarchy, not of the
   * hierarchy the user can currently see.
   */
  readonly depth: number;
  /**
   * Depth the announced level is derived from, 0-based.
   *
   * Omitted means "identical to `depth`", which is the normal case. Present
   * means the row sits under fewer *announced* ancestors than its indent
   * suggests: a search that drops a section header removes one announced level
   * while every descendant keeps its painted depth, and announcing the painted
   * depth would tell a screen reader about a parent row that is not in the
   * tree. Only the row builder gets to set this — a consumer that recomputes it
   * is exactly the drift this module exists to end.
   */
  readonly levelDepth?: number;
}

/** A row that can own a child list. */
export interface TreeRowNode extends TreeRowLevel {
  /**
   * Whether a child list exists at all. Distinct from {@link expanded}: a
   * folder that exists but has not been scanned yet is a branch that is
   * collapsed, and a row whose children are simply not in the list is a leaf.
   */
  readonly hasChildren: boolean;
  /** Whether that child list is currently visible. */
  readonly expanded: boolean;
}

/**
 * The full public contract: what a virtualized tree needs in order to key a
 * row, reuse its DOM node across a scroll, and skip it when rebuilding.
 */
export interface TreeRow extends TreeRowNode {
  /**
   * Stable identity. Must survive an unrelated sibling collapsing or
   * expanding, because a virtualizer reuses DOM nodes by key and an index
   * derived key silently re-points a row's node at a different row.
   */
  readonly key: string;
  /** Terminal node: no child list, and none can appear without a data change. */
  readonly leaf: boolean;
}
