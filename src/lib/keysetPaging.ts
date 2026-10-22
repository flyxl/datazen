/**
 * Keyset (seek) paging, on the front end.
 *
 * Reading a page by *offset* means the database counts and discards the rows
 * before the page, so the cost of a page grows with how deep the pagination
 * is. Reading it by *seek* means the page starts after a **value boundary**
 * that was read from a real row.
 *
 * The trade is that a boundary only means something when the order it was
 * taken in has not changed. So the front end keeps two things:
 *
 * * the **signatures** of the sort and the filters that were in force when a
 *   boundary was taken — a boundary taken under another order locates the
 *   wrong rows, and the only defence is to notice and start over;
 * * a **stack** of the boundaries of the pages that were visited, so that
 *   "previous" is a re-read of the same boundary rather than an offset that
 *   has drifted.
 *
 * There is no third paging semantic here. A page number is an **ordinal** —
 * "the Nth page I walked to" — and it says nothing about where in the table
 * that page starts. So a page that was never visited cannot be seeked to: it
 * is read by offset, and the stack is re-anchored there.
 *
 * This module is pure: no React, no store, no IPC. Everything it decides is
 * decided from the state it is given.
 */

import type {
  CursorValue,
  FilterCondition,
  KeysetUnavailableReason,
  OrderColumn,
  PagePagingInfo,
  PagingMode,
  SortCondition,
} from '../types';

/**
 * How many boundaries the stack may hold.
 *
 * The stack is memory (one boundary per visited page) and the boundaries
 * grow stale as the data underneath moves. Past the limit the front end
 * stops growing the stack and starts over from the page it is on.
 */
export const KEYSET_STACK_LIMIT = 200;

/** One visited page and the boundary it started at. */
export interface KeysetCursor {
  /** The boundary the page started at; `null` for the first page. */
  boundary: CursorValue[] | null;
  /** The sort that was in force when the boundary was taken. */
  orderSignature: string;
  /** The filters that were in force when the boundary was taken. */
  filterSignature: string;
}

/** The paging state of one panel. */
export interface KeysetPagingState {
  /** The strategy the back end reported. */
  mode: PagingMode;
  /** The stable total order the back end reported. */
  orderColumns: OrderColumn[];
  /**
   * The boundaries of the pages that were visited.
   *
   * `stack[i]` is the boundary of page `anchorPage + i`.
   */
  stack: KeysetCursor[];
  /** The page `stack[0]` belongs to. */
  anchorPage: number;
  /** Why the page was not read by seek, when it was not. */
  unavailable: KeysetUnavailableReason | null;
  /** Rows were deleted while the user was paging. */
  driftSuspected: boolean;
  /**
   * The boundary of the page **after** the one on screen.
   *
   * This is what the next request carries. It is `null` when the back end
   * gave none — the last page, or a page that was read by offset.
   */
  nextAfterKey: CursorValue[] | null;
}

/** The state of a panel that has never paged by seek. */
export const emptyKeysetPagingState: KeysetPagingState = {
  mode: 'offset',
  orderColumns: [],
  stack: [],
  anchorPage: 0,
  unavailable: null,
  driftSuspected: false,
  nextAfterKey: null,
};

/**
 * The signature of a sort.
 *
 * The column names **and** their directions: sorting by `name` ascending and
 * by `name` descending are two different orders, and a boundary taken under
 * one of them locates the wrong rows under the other.
 */
export function orderSignature(sorts: SortCondition[]): string {
  return sorts
    .map((sort) => `${sort.column}:${sort.descending ? 'desc' : 'asc'}`)
    .join(',');
}

/**
 * The signature of a filter list.
 *
 * The columns, the operators and the values, in order: two filter lists that
 * select different rows are different lists, even when they have the same
 * length.
 */
export function filterSignature(
  filters: FilterCondition[],
  logic: 'and' | 'or',
): string {
  const parts = filters.map((filter) => {
    const value = filter.value === undefined ? '' : JSON.stringify(filter.value);
    return `${filter.column}:${filter.operator}:${value}`;
  });
  return `${logic}(${parts.join('|')})`;
}

/**
 * The signature of a whole request: its sort and its filters.
 *
 * Two requests with the same signature select the same rows in the same
 * order, so a boundary taken under one is valid under the other.
 */
export function pagingSignature(
  sorts: SortCondition[],
  filters: FilterCondition[],
  logic: 'and' | 'or',
): string {
  return `${orderSignature(sorts)}#${filterSignature(filters, logic)}`;
}

/** A boundary that cannot locate anything. */
function isUsableBoundary(boundary: CursorValue[] | null | undefined): boundary is CursorValue[] {
  if (!boundary || boundary.length === 0) return false;
  // A NULL does not take part in comparisons: a condition built from it
  // would skip rows without saying so.
  return !boundary.some((value) => value.kind === 'null');
}

/**
 * The boundary of a page that was visited, or `null`.
 *
 * `null` means "this page was never visited" — the caller has to read it by
 * offset and start the stack over.
 */
export function cursorForPage(
  state: KeysetPagingState,
  targetPage: number,
): KeysetCursor | null {
  const index = targetPage - state.anchorPage;
  if (index < 0 || index >= state.stack.length) return null;
  return state.stack[index] ?? null;
}

/**
 * What a request for a page should carry.
 */
export interface KeysetRequestPlan {
  /** The boundary to send, or `null` for the first page. */
  afterKey: CursorValue[] | null;
  /**
   * "Read this page by offset."
   *
   * The page is `page × pageSize` rows in. It is the honest answer for a
   * page that was never visited, for a page whose boundary does not belong
   * to the current order, and for a page whose boundary is gone.
   */
  fallbackToOffset: boolean;
  /**
   * "Start the stack over at this page."
   *
   * A page that was never visited has no boundary, and the pages before it
   * are no longer reachable — the stack is re-anchored where the read
   * starts.
   */
  reanchor: boolean;
}

/**
 * What the next request for `targetPage` should carry.
 *
 * The rules, in order:
 *
 * 1. a boundary taken under another sort or another filter is **not** sent —
 *    it would locate the wrong rows;
 * 2. a page that was visited is re-read from its own boundary;
 * 3. the page right after the last visited one is read from the boundary the
 *    back end reported for it;
 * 4. every other page — a page that was never visited — is read by offset.
 */
export function nextRequestCursor(
  state: KeysetPagingState,
  targetPage: number,
  orderSig: string,
  filterSig: string,
): KeysetRequestPlan {
  const top = state.stack[state.stack.length - 1];
  if (top && (top.orderSignature !== orderSig || top.filterSignature !== filterSig)) {
    // The order or the filters changed: the boundaries on the stack belong
    // to another question and may not be sent.
    return { afterKey: null, fallbackToOffset: true, reanchor: true };
  }

  const visited = cursorForPage(state, targetPage);
  if (visited) {
    if (!isUsableBoundary(visited.boundary)) {
      // The anchor page itself: it was reached by offset, and it is read
      // that way again.
      return { afterKey: null, fallbackToOffset: true, reanchor: false };
    }
    return { afterKey: visited.boundary, fallbackToOffset: false, reanchor: false };
  }

  if (targetPage === state.anchorPage + state.stack.length) {
    // The page right after the last visited one.
    if (!isUsableBoundary(state.nextAfterKey)) {
      return { afterKey: null, fallbackToOffset: true, reanchor: false };
    }
    return { afterKey: state.nextAfterKey, fallbackToOffset: false, reanchor: false };
  }

  // A page that was never visited: it cannot be seeked to.
  return { afterKey: null, fallbackToOffset: true, reanchor: true };
}

/** The result of recording a page that was read. */
export interface PushedCursor {
  state: KeysetPagingState;
  /**
   * The stack was full: the caller has to start over from this page.
   *
   * A boundary chain that grows without end describes a moment in the past
   * that the data underneath has almost certainly left.
   */
  needsReanchor: boolean;
}

/**
 * Record the page that was just read.
 *
 * `boundary` is the boundary the page was read with — the boundary of the
 * page itself — and `nextBoundary` is the boundary of the page after it, as
 * the back end reported it. A `nextBoundary` that is missing or holds a
 * `NULL` is **not** recorded: it cannot locate anything.
 */
export function pushCursor(
  state: KeysetPagingState,
  page: number,
  boundary: CursorValue[] | null,
  nextBoundary: CursorValue[] | null,
  orderSig: string,
  filterSig: string,
): PushedCursor {
  const cursor: KeysetCursor = {
    boundary: isUsableBoundary(boundary) ? boundary : null,
    orderSignature: orderSig,
    filterSignature: filterSig,
  };

  // The stack holds the pages from the anchor onwards. A page before the
  // anchor is not reachable any more.
  const index = page - state.anchorPage;
  let stack = state.stack;
  if (index < 0) {
    // The page is before the anchor: the stack no longer reaches it.
    stack = [];
  } else if (index < stack.length) {
    // The page was visited before: its boundary is replaced by the one that
    // was just used.
    stack = [...stack.slice(0, index), cursor, ...stack.slice(index + 1)];
    return {
      state: {
        ...state,
        stack,
        nextAfterKey: isUsableBoundary(nextBoundary) ? nextBoundary : null,
      },
      needsReanchor: false,
    };
  }

  if (stack.length >= KEYSET_STACK_LIMIT) {
    // The chain is full: start over from this page.
    return {
      state: {
        ...state,
        stack: [cursor],
        anchorPage: page,
        nextAfterKey: isUsableBoundary(nextBoundary) ? nextBoundary : null,
      },
      needsReanchor: true,
    };
  }

  stack = [...stack, cursor];
  return {
    state: {
      ...state,
      stack,
      nextAfterKey: isUsableBoundary(nextBoundary) ? nextBoundary : null,
    },
    needsReanchor: false,
  };
}

/**
 * Start the stack over at `anchorPage`.
 *
 * The page is read by offset — there is no boundary for a page that was
 * never visited — and the stack holds that one page until the pages after
 * it are visited.
 */
export function reanchor(
  state: KeysetPagingState,
  anchorPage: number,
  orderSig: string,
  filterSig: string,
): KeysetPagingState {
  return {
    ...state,
    stack: [{ boundary: null, orderSignature: orderSig, filterSignature: filterSig }],
    anchorPage,
    nextAfterKey: null,
  };
}

/**
 * Fold the answer of a read into the paging state.
 *
 * The back end's word is the authority: the mode, the order and the reason
 * come from it, not from a guess on this side.
 */
export function applyPagingInfo(
  state: KeysetPagingState,
  info: PagePagingInfo | null | undefined,
): KeysetPagingState {
  if (!info) {
    // A back end that knows nothing about paging: the page was read by
    // offset, which is what every read did before.
    return { ...state, mode: 'offset', unavailable: null, nextAfterKey: null };
  }
  return {
    ...state,
    mode: info.mode,
    orderColumns: info.orderColumns ?? [],
    unavailable: info.unavailableReason ?? null,
    driftSuspected: info.driftSuspected ?? false,
    nextAfterKey: isUsableBoundary(info.nextAfterKey) ? info.nextAfterKey : null,
  };
}
