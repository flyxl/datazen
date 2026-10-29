/**
 * The pure half of keyset paging: the stack, the signatures, the
 * invalidation and the re-anchoring.
 *
 * These are the rules that decide **which rows** a page holds, so they are
 * tested as rules rather than as helpers: the interesting cases are the ones
 * where a wrong answer is still a plausible-looking page.
 */
import { describe, expect, it } from 'vitest';

import {
  KEYSET_STACK_LIMIT,
  applyPagingInfo,
  cursorForPage,
  emptyKeysetPagingState,
  filterSignature,
  nextRequestCursor,
  orderSignature,
  pagingSignature,
  pushCursor,
  reanchor,
  type KeysetPagingState,
} from '../keysetPaging';
import type { CursorValue, FilterCondition, PagePagingInfo, SortCondition } from '../../types';

const ASC: SortCondition[] = [{ column: 'id', descending: false }];
const DESC: SortCondition[] = [{ column: 'id', descending: true }];
const FILTERS: FilterCondition[] = [
  { column: 'name', operator: 'eq', value: 'alice' },
];

const NEXT: CursorValue[] = [{ kind: 'integer', value: 84 }];

/** The signatures of the request the fixtures below belong to. */
const ORDER_SIG = orderSignature(ASC);
const FILTER_SIG = filterSignature(FILTERS, 'and');

/** A state that has read one page and knows the boundary of the next. */
function afterFirstPage(): KeysetPagingState {
  const pushed = pushCursor(
    emptyKeysetPagingState,
    0,
    null,
    NEXT,
    ORDER_SIG,
    FILTER_SIG,
  );
  return pushed.state;
}

describe('the signatures of a request', () => {
  it('tells an ascending sort from a descending one', () => {
    expect(orderSignature(ASC)).not.toBe(orderSignature(DESC));
  });

  it('tells one filter list from another', () => {
    const other: FilterCondition[] = [
      { column: 'name', operator: 'eq', value: 'bob' },
    ];
    expect(filterSignature(FILTERS, 'and')).not.toBe(filterSignature(other, 'and'));
    expect(filterSignature(FILTERS, 'and')).not.toBe(filterSignature(FILTERS, 'or'));
  });

  it('combines the sort and the filters into one signature', () => {
    expect(pagingSignature(ASC, FILTERS, 'and')).toContain(orderSignature(ASC));
    expect(pagingSignature(ASC, FILTERS, 'and')).toContain(filterSignature(FILTERS, 'and'));
  });
});

describe('the cursor stack', () => {
  it('cursor_for_page_returns_null_for_unvisited_page', () => {
    const state = afterFirstPage();
    expect(cursorForPage(state, 0)).not.toBeNull();
    expect(cursorForPage(state, 1)).toBeNull();
    expect(cursorForPage(state, 7)).toBeNull();
  });

  it('push_cursor_stops_at_stack_limit', () => {
    let state = emptyKeysetPagingState;
    for (let page = 0; page < KEYSET_STACK_LIMIT; page += 1) {
      const pushed = pushCursor(state, page, null, NEXT, 'o', 'f');
      expect(pushed.needsReanchor).toBe(false);
      state = pushed.state;
    }
    expect(state.stack.length).toBe(KEYSET_STACK_LIMIT);

    const pushed = pushCursor(state, KEYSET_STACK_LIMIT, null, NEXT, 'o', 'f');
    expect(pushed.needsReanchor).toBe(true);
    expect(pushed.state.stack.length).toBeLessThanOrEqual(KEYSET_STACK_LIMIT);
  });

  it('reanchor_truncates_stack_and_moves_anchor', () => {
    const state = reanchor(emptyKeysetPagingState, 500, ORDER_SIG, FILTER_SIG);
    expect(state.stack).toHaveLength(1);
    expect(state.stack[0].boundary).toBeNull();
    expect(state.anchorPage).toBe(500);
  });

  it('signature_change_invalidates_cursor', () => {
    const state = afterFirstPage();
    // The sort changed: the boundary on the stack belongs to another order.
    const plan = nextRequestCursor(state, 1, orderSignature(DESC), FILTER_SIG);
    expect(plan.fallbackToOffset).toBe(true);
    expect(plan.afterKey).toBeNull();
  });

  it('mixed_direction_order_columns_are_preserved', () => {
    const info: PagePagingInfo = {
      mode: 'keyset',
      orderColumns: [
        { column: 'created_at', descending: true },
        { column: 'id', descending: false },
      ],
      driftSuspected: false,
    };
    const state = applyPagingInfo(emptyKeysetPagingState, info);
    expect(state.orderColumns).toHaveLength(2);
    expect(state.orderColumns[0]).toEqual({ column: 'created_at', descending: true });
    expect(state.orderColumns[1]).toEqual({ column: 'id', descending: false });
  });

  it('null_boundary_is_never_sent', () => {
    const pushed = pushCursor(
      emptyKeysetPagingState,
      0,
      null,
      [{ kind: 'null' }],
      ORDER_SIG,
      FILTER_SIG,
    );
    expect(pushed.state.nextAfterKey).toBeNull();
    const plan = nextRequestCursor(pushed.state, 1, ORDER_SIG, FILTER_SIG);
    expect(plan.afterKey).toBeNull();
    expect(plan.fallbackToOffset).toBe(true);
  });
});

describe('what a request carries', () => {
  it('sends_the_boundary_of_the_page_after_the_last_visited_one', () => {
    const state = afterFirstPage();
    const plan = nextRequestCursor(state, 1, ORDER_SIG, FILTER_SIG);
    expect(plan.fallbackToOffset).toBe(false);
    expect(plan.afterKey).toEqual(NEXT);
  });

  it('re_reads_a_visited_page_from_its_own_boundary', () => {
    let state = emptyKeysetPagingState;
    state = pushCursor(state, 0, null, NEXT, ORDER_SIG, FILTER_SIG).state;
    state = pushCursor(
      state,
      1,
      NEXT,
      [{ kind: 'integer', value: 126 }],
      ORDER_SIG,
      FILTER_SIG,
    ).state;

    const plan = nextRequestCursor(state, 1, ORDER_SIG, FILTER_SIG);
    expect(plan.fallbackToOffset).toBe(false);
    expect(plan.afterKey).toEqual(NEXT);
  });

  it('reads_a_page_that_was_never_visited_by_offset', () => {
    const state = afterFirstPage();
    const plan = nextRequestCursor(state, 7, ORDER_SIG, FILTER_SIG);
    expect(plan.fallbackToOffset).toBe(true);
    expect(plan.afterKey).toBeNull();
  });

  it('reads_the_anchor_page_by_offset', () => {
    // The anchor page was reached by offset, so it has no boundary.
    const state = reanchor(emptyKeysetPagingState, 500, ORDER_SIG, FILTER_SIG);
    const plan = nextRequestCursor(state, 500, ORDER_SIG, FILTER_SIG);
    expect(plan.fallbackToOffset).toBe(true);
    expect(plan.afterKey).toBeNull();
  });
});

describe('the answer of the back end', () => {
  it('is believed over any guess', () => {
    const info: PagePagingInfo = {
      mode: 'keyset',
      orderColumns: [{ column: 'id', descending: false }],
      nextAfterKey: NEXT,
      driftSuspected: true,
    };
    const state = applyPagingInfo(emptyKeysetPagingState, info);
    expect(state.mode).toBe('keyset');
    expect(state.nextAfterKey).toEqual(NEXT);
    expect(state.driftSuspected).toBe(true);
    expect(state.unavailable).toBeNull();
  });

  it('reports_why_a_page_was_not_read_by_seek', () => {
    const info: PagePagingInfo = {
      mode: 'offset',
      orderColumns: [],
      unavailableReason: 'nullableKeyColumn',
      driftSuspected: false,
    };
    const state = applyPagingInfo(emptyKeysetPagingState, info);
    expect(state.mode).toBe('offset');
    expect(state.unavailable).toBe('nullableKeyColumn');
    expect(state.nextAfterKey).toBeNull();
  });

  it('treats_a_back_end_without_paging_as_an_offset_read', () => {
    const state = applyPagingInfo(emptyKeysetPagingState, undefined);
    expect(state.mode).toBe('offset');
    expect(state.unavailable).toBeNull();
  });
});
