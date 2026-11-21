use crate::{
    CursorValue, FilterCondition, KeysetUnavailableReason, OrderColumn, PagePagingInfo, PagingMode,
    SortCondition,
};

pub const KEYSET_STACK_LIMIT: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterLogic {
    And,
    Or,
}

impl FilterLogic {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::And => "and",
            Self::Or => "or",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeysetCursor {
    pub boundary: Option<Vec<CursorValue>>,
    pub order_signature: String,
    pub filter_signature: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeysetPagingState {
    pub mode: PagingMode,
    pub order_columns: Vec<OrderColumn>,
    pub stack: Vec<KeysetCursor>,
    pub anchor_page: i64,
    pub unavailable: Option<KeysetUnavailableReason>,
    pub drift_suspected: bool,
    pub next_after_key: Option<Vec<CursorValue>>,
}

pub fn empty_keyset_paging_state() -> KeysetPagingState {
    KeysetPagingState {
        mode: PagingMode::Offset,
        order_columns: Vec::new(),
        stack: Vec::new(),
        anchor_page: 0,
        unavailable: None,
        drift_suspected: false,
        next_after_key: None,
    }
}

pub fn order_signature(sorts: &[SortCondition]) -> String {
    sorts
        .iter()
        .map(|sort| {
            format!(
                "{}:{}",
                sort.column,
                if sort.descending { "desc" } else { "asc" }
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

pub fn filter_signature(filters: &[FilterCondition], logic: FilterLogic) -> String {
    let parts: Vec<String> = filters
        .iter()
        .map(|filter| {
            let value = filter
                .value
                .as_ref()
                .map(|value| serde_json::to_string(value).unwrap_or_default())
                .unwrap_or_default();
            format!("{}:{}:{}", filter.column, filter.operator.as_str(), value)
        })
        .collect();
    format!("{}({})", logic.as_str(), parts.join("|"))
}

pub fn paging_signature(
    sorts: &[SortCondition],
    filters: &[FilterCondition],
    logic: FilterLogic,
) -> String {
    format!(
        "{}#{}",
        order_signature(sorts),
        filter_signature(filters, logic)
    )
}

pub fn is_usable_boundary(boundary: Option<&Vec<CursorValue>>) -> bool {
    match boundary {
        Some(values) if !values.is_empty() => !values
            .iter()
            .any(|value| matches!(value, CursorValue::Null)),
        _ => false,
    }
}

pub fn cursor_for_page(state: &KeysetPagingState, target_page: i64) -> Option<&KeysetCursor> {
    let index = target_page - state.anchor_page;
    if index < 0 {
        return None;
    }
    state.stack.get(index as usize)
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeysetRequestPlan {
    pub after_key: Option<Vec<CursorValue>>,
    pub fallback_to_offset: bool,
    pub reanchor: bool,
}

pub fn next_request_cursor(
    state: &KeysetPagingState,
    target_page: i64,
    order_sig: &str,
    filter_sig: &str,
) -> KeysetRequestPlan {
    if let Some(top) = state.stack.last() {
        if top.order_signature != order_sig || top.filter_signature != filter_sig {
            return KeysetRequestPlan {
                after_key: None,
                fallback_to_offset: true,
                reanchor: true,
            };
        }
    }
    if let Some(visited) = cursor_for_page(state, target_page) {
        if !is_usable_boundary(visited.boundary.as_ref()) {
            return KeysetRequestPlan {
                after_key: None,
                fallback_to_offset: true,
                reanchor: false,
            };
        }
        return KeysetRequestPlan {
            after_key: visited.boundary.clone(),
            fallback_to_offset: false,
            reanchor: false,
        };
    }
    if target_page == state.anchor_page + state.stack.len() as i64 {
        if !is_usable_boundary(state.next_after_key.as_ref()) {
            return KeysetRequestPlan {
                after_key: None,
                fallback_to_offset: true,
                reanchor: false,
            };
        }
        return KeysetRequestPlan {
            after_key: state.next_after_key.clone(),
            fallback_to_offset: false,
            reanchor: false,
        };
    }
    KeysetRequestPlan {
        after_key: None,
        fallback_to_offset: true,
        reanchor: true,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PushedCursor {
    pub state: KeysetPagingState,
    pub needs_reanchor: bool,
}

pub fn push_cursor(
    state: &KeysetPagingState,
    page: i64,
    boundary: Option<Vec<CursorValue>>,
    next_boundary: Option<Vec<CursorValue>>,
    order_sig: &str,
    filter_sig: &str,
) -> PushedCursor {
    let cursor = KeysetCursor {
        boundary: if is_usable_boundary(boundary.as_ref()) {
            boundary
        } else {
            None
        },
        order_signature: order_sig.to_owned(),
        filter_signature: filter_sig.to_owned(),
    };
    let next_after_key = if is_usable_boundary(next_boundary.as_ref()) {
        next_boundary
    } else {
        None
    };
    let index = page - state.anchor_page;
    if index < 0 {
        return PushedCursor {
            state: KeysetPagingState {
                stack: Vec::new(),
                next_after_key,
                ..state.clone()
            },
            needs_reanchor: false,
        };
    }
    if (index as usize) < state.stack.len() {
        let mut stack = state.stack.clone();
        stack[index as usize] = cursor;
        return PushedCursor {
            state: KeysetPagingState {
                stack,
                next_after_key,
                ..state.clone()
            },
            needs_reanchor: false,
        };
    }
    if state.stack.len() >= KEYSET_STACK_LIMIT {
        return PushedCursor {
            state: KeysetPagingState {
                stack: vec![cursor],
                anchor_page: page,
                next_after_key,
                ..state.clone()
            },
            needs_reanchor: true,
        };
    }
    let mut stack = state.stack.clone();
    stack.push(cursor);
    PushedCursor {
        state: KeysetPagingState {
            stack,
            next_after_key,
            ..state.clone()
        },
        needs_reanchor: false,
    }
}

pub fn reanchor(
    state: &KeysetPagingState,
    anchor_page: i64,
    order_sig: &str,
    filter_sig: &str,
) -> KeysetPagingState {
    KeysetPagingState {
        stack: vec![KeysetCursor {
            boundary: None,
            order_signature: order_sig.to_owned(),
            filter_signature: filter_sig.to_owned(),
        }],
        anchor_page,
        next_after_key: None,
        ..state.clone()
    }
}

pub fn apply_paging_info(
    state: &KeysetPagingState,
    info: Option<&PagePagingInfo>,
) -> KeysetPagingState {
    match info {
        None => KeysetPagingState {
            mode: PagingMode::Offset,
            unavailable: None,
            next_after_key: None,
            ..state.clone()
        },
        Some(info) => KeysetPagingState {
            mode: info.mode,
            order_columns: info.order_columns.clone(),
            unavailable: info.unavailable_reason,
            drift_suspected: info.drift_suspected,
            next_after_key: if is_usable_boundary(info.next_after_key.as_ref()) {
                info.next_after_key.clone()
            } else {
                None
            },
            ..state.clone()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FilterOperator;

    fn sort(column: &str, descending: bool) -> SortCondition {
        SortCondition {
            column: column.to_owned(),
            descending,
        }
    }

    fn filter(
        column: &str,
        operator: FilterOperator,
        value: Option<serde_json::Value>,
    ) -> FilterCondition {
        FilterCondition {
            column: column.to_owned(),
            operator,
            value,
            connector: None,
        }
    }

    fn int_boundary(value: i64) -> Vec<CursorValue> {
        vec![CursorValue::Integer { value }]
    }

    fn cursor(boundary: Option<Vec<CursorValue>>, order: &str, filter: &str) -> KeysetCursor {
        KeysetCursor {
            boundary,
            order_signature: order.to_owned(),
            filter_signature: filter.to_owned(),
        }
    }

    fn state_with(
        stack: Vec<KeysetCursor>,
        anchor: i64,
        next: Option<Vec<CursorValue>>,
    ) -> KeysetPagingState {
        KeysetPagingState {
            stack,
            anchor_page: anchor,
            next_after_key: next,
            ..empty_keyset_paging_state()
        }
    }

    #[test]
    fn limit_and_logic_literals_are_pinned() {
        assert_eq!(KEYSET_STACK_LIMIT, 200);
        assert_eq!(FilterLogic::And.as_str(), "and");
        assert_eq!(FilterLogic::Or.as_str(), "or");
        assert_eq!(PagingMode::Offset.as_str(), "offset");
        assert_eq!(PagingMode::Keyset.as_str(), "keyset");
    }

    #[test]
    fn empty_state_defaults_are_pinned() {
        let state = empty_keyset_paging_state();
        assert_eq!(state.mode, PagingMode::Offset);
        assert!(state.order_columns.is_empty());
        assert!(state.stack.is_empty());
        assert_eq!(state.anchor_page, 0);
        assert_eq!(state.unavailable, None);
        assert!(!state.drift_suspected);
        assert_eq!(state.next_after_key, None);
    }

    #[test]
    fn signatures_include_direction_operator_value_and_logic() {
        assert_eq!(order_signature(&[]), "");
        assert_eq!(
            order_signature(&[sort("name", false), sort("id", true)]),
            "name:asc,id:desc"
        );
        assert_eq!(filter_signature(&[], FilterLogic::And), "and()");
        let filters = vec![
            filter("age", FilterOperator::Gt, Some(serde_json::json!(18))),
            filter("name", FilterOperator::Eq, None),
        ];
        assert_eq!(
            filter_signature(&filters, FilterLogic::Or),
            "or(age:gt:18|name:eq:)"
        );
        assert_eq!(
            paging_signature(&[sort("name", false)], &filters, FilterLogic::And),
            "name:asc#and(age:gt:18|name:eq:)"
        );
    }

    #[test]
    fn boundary_usability_rejects_empty_and_null() {
        assert!(!is_usable_boundary(None));
        assert!(!is_usable_boundary(Some(&Vec::new())));
        assert!(!is_usable_boundary(Some(&vec![CursorValue::Null])));
        assert!(!is_usable_boundary(Some(&vec![
            CursorValue::Integer { value: 1 },
            CursorValue::Null
        ])));
        assert!(is_usable_boundary(Some(&int_boundary(1))));
        assert!(is_usable_boundary(Some(&vec![CursorValue::Bool {
            value: false
        }])));
    }

    #[test]
    fn cursor_lookup_respects_the_anchor_window() {
        let state = state_with(
            vec![
                cursor(None, "o", "f"),
                cursor(Some(int_boundary(5)), "o", "f"),
            ],
            3,
            None,
        );
        assert_eq!(cursor_for_page(&state, 3).unwrap().boundary, None);
        assert_eq!(
            cursor_for_page(&state, 4).unwrap().boundary,
            Some(int_boundary(5))
        );
        assert!(cursor_for_page(&state, 2).is_none());
        assert!(cursor_for_page(&state, 5).is_none());
    }

    #[test]
    fn signature_mismatch_forces_offset_and_reanchor() {
        let state = state_with(
            vec![cursor(Some(int_boundary(1)), "old", "f")],
            0,
            Some(int_boundary(2)),
        );
        let plan = next_request_cursor(&state, 1, "new", "f");
        assert_eq!(plan.after_key, None);
        assert!(plan.fallback_to_offset);
        assert!(plan.reanchor);
        let plan = next_request_cursor(&state, 0, "old", "different");
        assert!(plan.reanchor);
    }

    #[test]
    fn visited_pages_reuse_their_own_boundary() {
        let state = state_with(
            vec![
                cursor(None, "o", "f"),
                cursor(Some(int_boundary(7)), "o", "f"),
            ],
            0,
            Some(int_boundary(9)),
        );
        let plan = next_request_cursor(&state, 1, "o", "f");
        assert_eq!(plan.after_key, Some(int_boundary(7)));
        assert!(!plan.fallback_to_offset);
        assert!(!plan.reanchor);
        let plan = next_request_cursor(&state, 0, "o", "f");
        assert_eq!(plan.after_key, None);
        assert!(plan.fallback_to_offset);
        assert!(!plan.reanchor);
    }

    #[test]
    fn next_page_uses_next_after_key_or_falls_back() {
        let state = state_with(
            vec![cursor(Some(int_boundary(1)), "o", "f")],
            0,
            Some(int_boundary(2)),
        );
        let plan = next_request_cursor(&state, 1, "o", "f");
        assert_eq!(plan.after_key, Some(int_boundary(2)));
        assert!(!plan.fallback_to_offset);
        let state = state_with(
            vec![cursor(Some(int_boundary(1)), "o", "f")],
            0,
            Some(vec![CursorValue::Null]),
        );
        let plan = next_request_cursor(&state, 1, "o", "f");
        assert!(plan.fallback_to_offset);
        assert!(!plan.reanchor);
        let plan = next_request_cursor(&state, 7, "o", "f");
        assert!(plan.fallback_to_offset);
        assert!(plan.reanchor);
    }

    #[test]
    fn push_replaces_visited_and_appends_new_pages() {
        let state = state_with(vec![cursor(Some(int_boundary(1)), "o", "f")], 0, None);
        let pushed = push_cursor(
            &state,
            0,
            Some(int_boundary(2)),
            Some(int_boundary(3)),
            "o",
            "f",
        );
        assert_eq!(pushed.state.stack.len(), 1);
        assert_eq!(pushed.state.stack[0].boundary, Some(int_boundary(2)));
        assert_eq!(pushed.state.next_after_key, Some(int_boundary(3)));
        assert!(!pushed.needs_reanchor);

        let pushed = push_cursor(&pushed.state, 1, Some(int_boundary(3)), None, "o", "f");
        assert_eq!(pushed.state.stack.len(), 2);
        assert_eq!(pushed.state.next_after_key, None);
    }

    #[test]
    fn push_before_anchor_clears_and_full_stack_reanchors() {
        let state = state_with(vec![cursor(Some(int_boundary(1)), "o", "f")], 2, None);
        let pushed = push_cursor(&state, 0, Some(int_boundary(9)), None, "o", "f");
        assert!(pushed.state.stack.is_empty());
        assert!(!pushed.needs_reanchor);

        let full: Vec<_> = (0..KEYSET_STACK_LIMIT)
            .map(|_| cursor(Some(int_boundary(1)), "o", "f"))
            .collect();
        let state = state_with(full, 0, None);
        let pushed = push_cursor(&state, 200, Some(int_boundary(9)), None, "o", "f");
        assert_eq!(pushed.state.stack.len(), 1);
        assert_eq!(pushed.state.anchor_page, 200);
        assert!(pushed.needs_reanchor);
    }

    #[test]
    fn reanchor_seeds_one_boundary_less_page() {
        let state = state_with(
            vec![cursor(Some(int_boundary(1)), "old", "old")],
            0,
            Some(int_boundary(2)),
        );
        let next = reanchor(&state, 5, "new", "nf");
        assert_eq!(next.anchor_page, 5);
        assert_eq!(next.stack.len(), 1);
        assert_eq!(next.stack[0].boundary, None);
        assert_eq!(next.stack[0].order_signature, "new");
        assert_eq!(next.next_after_key, None);
        assert_eq!(next.mode, state.mode);
    }

    #[test]
    fn paging_info_is_the_authority() {
        let state = empty_keyset_paging_state();
        let info = PagePagingInfo {
            mode: PagingMode::Keyset,
            order_columns: vec![OrderColumn {
                column: "id".to_owned(),
                descending: false,
            }],
            next_after_key: Some(int_boundary(4)),
            unavailable_reason: Some(KeysetUnavailableReason::StaleCursor),
            drift_suspected: true,
        };
        let next = apply_paging_info(&state, Some(&info));
        assert_eq!(next.mode, PagingMode::Keyset);
        assert_eq!(next.order_columns.len(), 1);
        assert_eq!(next.unavailable, Some(KeysetUnavailableReason::StaleCursor));
        assert!(next.drift_suspected);
        assert_eq!(next.next_after_key, Some(int_boundary(4)));

        let degraded = PagePagingInfo {
            mode: PagingMode::Offset,
            order_columns: Vec::new(),
            next_after_key: Some(Vec::new()),
            unavailable_reason: None,
            drift_suspected: false,
        };
        let next = apply_paging_info(&next, Some(&degraded));
        assert_eq!(next.mode, PagingMode::Offset);
        assert!(next.next_after_key.is_none());
        assert!(!next.drift_suspected);

        let mut stale = empty_keyset_paging_state();
        stale.mode = PagingMode::Keyset;
        stale.drift_suspected = true;
        let next = apply_paging_info(&stale, None);
        assert_eq!(next.mode, PagingMode::Offset);
        assert_eq!(next.unavailable, None);
        assert_eq!(next.next_after_key, None);
        assert!(next.drift_suspected);
    }
}
