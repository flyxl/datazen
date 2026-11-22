use crate::{is_mutation_execution, ResultWorkspaceView, StatementResultView};

pub const TABLE_HEADER_HEIGHT: i64 = 41;
pub const TABLE_ROW_HEIGHT: i64 = 32;
pub const TABLE_STATUS_BAR_HEIGHT: i64 = 29;
pub const TABLE_EMPTY_BODY_HEIGHT: i64 = 112;
pub const TABLE_NO_ROWS_HINT_HEIGHT: i64 = 44;
pub const RESULT_TAB_STRIP_HEIGHT: i64 = 29;
pub const CHART_CONTENT_HEIGHT: i64 = 320;
pub const SUMMARY_CONTENT_HEIGHT: i64 = 320;
pub const EXPLAIN_CONTENT_HEIGHT: i64 = 320;
pub const ERROR_CONTENT_HEIGHT: i64 = 320;
pub const DIAGNOSIS_CONTENT_HEIGHT: i64 = 320;

pub const RESULT_PANE_MIN_HEIGHT: i64 = 140;

#[derive(Clone, Debug, PartialEq)]
pub struct QueryResultPaneHeightInput {
    pub result: Option<StatementResultView>,
    pub view: ResultWorkspaceView,
    pub has_result_tabs: bool,
    pub has_error: bool,
    pub show_explain: bool,
    pub show_diagnosis: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResultPaneSizing {
    pub height: i64,
    pub max_height: &'static str,
}

pub fn estimate_query_result_pane_height(input: &QueryResultPaneHeightInput) -> i64 {
    let chrome = if input.has_result_tabs {
        RESULT_TAB_STRIP_HEIGHT
    } else {
        0
    };

    if input.show_explain {
        return chrome + EXPLAIN_CONTENT_HEIGHT;
    }
    if input.has_error {
        return chrome
            + ERROR_CONTENT_HEIGHT
            + if input.show_diagnosis {
                DIAGNOSIS_CONTENT_HEIGHT
            } else {
                0
            };
    }
    if input.show_diagnosis {
        return chrome + DIAGNOSIS_CONTENT_HEIGHT;
    }
    let result = match &input.result {
        Some(result) => result,
        None => return RESULT_PANE_MIN_HEIGHT,
    };
    if is_mutation_execution(result) {
        return chrome + SUMMARY_CONTENT_HEIGHT;
    }
    if input.view == ResultWorkspaceView::Chart {
        return chrome + CHART_CONTENT_HEIGHT;
    }
    let row_count = result.rows.len() as i64;
    let body = if row_count == 0 {
        TABLE_EMPTY_BODY_HEIGHT + TABLE_NO_ROWS_HINT_HEIGHT
    } else {
        row_count * TABLE_ROW_HEIGHT
    };
    (RESULT_PANE_MIN_HEIGHT).max(chrome + TABLE_STATUS_BAR_HEIGHT + TABLE_HEADER_HEIGHT + body)
}

pub fn query_result_pane_sizing(pinned: bool, content_height: i64) -> Option<ResultPaneSizing> {
    if pinned {
        return None;
    }
    Some(ResultPaneSizing {
        height: content_height,
        max_height: "50%",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> QueryResultPaneHeightInput {
        QueryResultPaneHeightInput {
            result: Some(StatementResultView::default()),
            view: ResultWorkspaceView::Table,
            has_result_tabs: false,
            has_error: false,
            show_explain: false,
            show_diagnosis: false,
        }
    }

    #[test]
    fn no_result_uses_the_minimum_and_pinned_sizing_is_absent() {
        let mut item = input();
        item.result = None;
        assert_eq!(
            estimate_query_result_pane_height(&item),
            RESULT_PANE_MIN_HEIGHT
        );
        assert_eq!(query_result_pane_sizing(true, 300), None);
        assert_eq!(
            query_result_pane_sizing(false, 300),
            Some(ResultPaneSizing {
                height: 300,
                max_height: "50%"
            })
        );
    }

    #[test]
    fn explain_error_and_diagnosis_take_priority_in_order() {
        let mut item = input();
        item.show_explain = true;
        item.has_error = true;
        item.show_diagnosis = true;
        item.has_result_tabs = true;
        assert_eq!(estimate_query_result_pane_height(&item), 29 + 320);

        item.show_explain = false;
        assert_eq!(estimate_query_result_pane_height(&item), 29 + 320 + 320);

        item.has_error = false;
        assert_eq!(estimate_query_result_pane_height(&item), 29 + 320);

        item.has_result_tabs = false;
        item.show_diagnosis = false;
        assert_eq!(
            estimate_query_result_pane_height(&item),
            TABLE_STATUS_BAR_HEIGHT
                + TABLE_HEADER_HEIGHT
                + TABLE_EMPTY_BODY_HEIGHT
                + TABLE_NO_ROWS_HINT_HEIGHT
        );
    }

    #[test]
    fn mutation_and_chart_use_their_fixed_content_heights() {
        let mut item = input();
        item.result = Some(StatementResultView {
            sql: "DELETE FROM t".to_owned(),
            ..Default::default()
        });
        assert_eq!(estimate_query_result_pane_height(&item), 320);

        item.result = Some(StatementResultView::default());
        item.view = ResultWorkspaceView::Chart;
        assert_eq!(estimate_query_result_pane_height(&item), 320);
    }

    #[test]
    fn table_rows_scale_and_empty_tables_get_the_hint_footer() {
        let mut item = input();
        item.result = Some(StatementResultView {
            rows: vec![vec![], vec![], vec![]],
            ..Default::default()
        });
        assert_eq!(estimate_query_result_pane_height(&item), 29 + 41 + 3 * 32);

        item.result = Some(StatementResultView::default());
        assert_eq!(estimate_query_result_pane_height(&item), 29 + 41 + 112 + 44);

        item.has_result_tabs = true;
        assert_eq!(
            estimate_query_result_pane_height(&item),
            29 + 29 + 41 + 112 + 44
        );
    }

    #[test]
    fn row_estimate_is_floored_but_the_chrome_can_push_it_above() {
        let mut item = input();
        item.result = Some(StatementResultView {
            rows: vec![vec![]],
            ..Default::default()
        });
        assert_eq!(
            estimate_query_result_pane_height(&item),
            RESULT_PANE_MIN_HEIGHT
        );
        assert!(RESULT_PANE_MIN_HEIGHT == 140);
    }
}
