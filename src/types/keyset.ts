/**
 * The keyset (seek) paging vocabulary, mirrored from the host.
 *
 * The host's `PagingMode` says how a page was read, `CursorValue` carries a
 * page boundary, and `PagePagingInfo` is what the host reports about the
 * page it just read. The front end believes the report — it does not guess
 * which strategy is in force.
 *
 * `CursorValue` is deliberately **not** the shape of the host's `Value`:
 * `Value` is untagged, so `[1, 2, 3]` reads as bytes *or* as JSON, and a
 * boundary travels "host → front end → host". Untagged it would change its
 * variant on the way back, and the seek would compare against the wrong
 * literal with no error anywhere. The tag is what keeps the two apart.
 */

/**
 * How a page was read.
 *
 * The mirror of the host's `PagingMode`: by offset (the page starts
 * `page × pageSize` rows in) or by seek (the page starts after a value
 * boundary that was read from a real row).
 */
export type PagingMode = 'offset' | 'keyset';

/**
 * Why a page could not be read by seek.
 *
 * The mirror of the host's `KeysetUnavailableReason`. Every one of them is a
 * **degradation**, not an error: the page is read by offset and the reason
 * says why.
 */
export type KeysetUnavailableReason =
  | 'driverUnsupported'
  | 'noStableOrder'
  | 'nullableKeyColumn'
  | 'nullBoundary'
  | 'predicateUnavailable'
  | 'noPaginationClause'
  | 'staleCursor'
  | 'notAdjacent';

/**
 * One boundary value, **with its tag**.
 *
 * The host's `Value` is untagged, so `[1, 2, 3]` reads as `Bytes` *or* as
 * `Json`. A boundary travels "Rust → front end → Rust", and untagged it
 * would change its variant on the way back — the seek would then compare
 * against the wrong literal, with no error anywhere. The tag is what keeps
 * the two apart.
 */
export type CursorValue =
  | { kind: 'null' }
  | { kind: 'bool'; value: boolean }
  | { kind: 'integer'; value: number }
  | { kind: 'float'; value: number }
  | { kind: 'string'; value: string }
  | { kind: 'bytes'; value: number[] }
  | { kind: 'timestamp'; value: string }
  | { kind: 'json'; value: unknown };

/** One ordering column and its direction. */
export interface OrderColumn {
  column: string;
  descending: boolean;
}

/** What the host did about the paging of one page. */
export interface PagePagingInfo {
  /**
   * The strategy that was used. **The front end believes this field**, not
   * its own guess about which strategy is in force.
   */
  mode: PagingMode;
  /** The real column order of the page, key columns included. */
  orderColumns: OrderColumn[];
  /** The key values of the last row, to be sent back for the next page. */
  nextAfterKey?: CursorValue[];
  /** Set when the page was **not** read by seek: why not. */
  unavailableReason?: KeysetUnavailableReason;
  /** Rows were deleted while the user was paging. */
  driftSuspected: boolean;
}
