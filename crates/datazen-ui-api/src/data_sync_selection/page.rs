use crate::{row_key_string, selected_row_token, SyncOptions};

use super::{
    scope_for_table_operation, PageCursor, PageCursorTable, PageDirection, SelectionTable,
    TableSelectionScope,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CursorOutcome {
    Use(String),
    Noop,
}

pub fn table_page_key(source_table: &str, target_table: &str) -> String {
    format!("{source_table}\u{0}{target_table}")
}

pub fn loaded_page_token(key: &str, cursor: Option<&str>) -> String {
    format!("{key}\u{0}{}", cursor.unwrap_or(""))
}

pub fn resolve_cursor(
    cursors: &PageCursorTable,
    key: &str,
    first_cursor: Option<&str>,
    direction: PageDirection,
) -> CursorOutcome {
    let current = cursors.get(key).cloned().unwrap_or(PageCursor {
        current: first_cursor.map(str::to_owned),
        next: None,
        previous: Vec::new(),
    });
    match direction {
        PageDirection::Initial => current
            .current
            .map(CursorOutcome::Use)
            .unwrap_or(CursorOutcome::Noop),
        PageDirection::Next => current
            .next
            .map(CursorOutcome::Use)
            .unwrap_or(CursorOutcome::Noop),
        PageDirection::Previous => current
            .previous
            .last()
            .cloned()
            .map(CursorOutcome::Use)
            .unwrap_or(CursorOutcome::Noop),
    }
}

pub fn next_page_cursors(
    current: &PageCursor,
    cursor: Option<&str>,
    next_cursor: Option<String>,
    direction: PageDirection,
) -> PageCursor {
    let previous = match direction {
        PageDirection::Next => {
            let mut values = current.previous.clone();
            values.push(current.current.clone().unwrap_or_default());
            values
        }
        PageDirection::Previous => {
            let mut values = current.previous.clone();
            values.pop();
            values
        }
        PageDirection::Initial => Vec::new(),
    };
    PageCursor {
        current: cursor.map(str::to_owned),
        next: next_cursor,
        previous,
    }
}

pub fn next_page_index(previous: i64, direction: PageDirection) -> i64 {
    let delta = match direction {
        PageDirection::Next => 1,
        PageDirection::Previous => -1,
        PageDirection::Initial => 0,
    };
    (previous + delta).max(0)
}

pub fn page_selection_decision(
    table: &SelectionTable,
    operation: crate::DataSyncOperation,
    key: &[serde_json::Value],
    row_selected: bool,
    scopes: &[TableSelectionScope],
    options: &SyncOptions,
    first_load: bool,
    selected_tokens: &std::collections::BTreeSet<String>,
) -> bool {
    if let Some(scope) =
        scope_for_table_operation(scopes, &table.source_table, &table.target_table, operation)
    {
        if scope.selects_operation(operation, options) {
            let key_text = row_key_string(key);
            return !scope.excluded_rows.iter().any(|excluded| {
                excluded.operation == operation && row_key_string(&excluded.key) == key_text
            });
        }
    }
    if first_load {
        return row_selected;
    }
    let token = selected_row_token(&crate::DataSyncSelectedRow {
        source_table: table.source_table.clone(),
        target_table: table.target_table.clone(),
        operation,
        key: key.to_vec(),
    });
    selected_tokens.contains(&token)
}

#[cfg(test)]
mod tests {
    use crate::{ComparisonRow, ComparisonTableStatus};

    use super::*;

    fn table() -> SelectionTable {
        SelectionTable {
            source_table: "s".to_owned(),
            target_table: "t".to_owned(),
            status: ComparisonTableStatus::Matched,
            rows: None,
            first_cursor: Some("c0".to_owned()),
            page_size: Some(50),
        }
    }

    #[test]
    fn keys_join_with_null_separators() {
        assert_eq!(table_page_key("s", "t"), "s\u{0}t");
        assert_eq!(loaded_page_token("k", Some("c")), "k\u{0}c");
        assert_eq!(loaded_page_token("k", None), "k\u{0}");
    }

    #[test]
    fn cursor_resolution_reflects_direction_and_missing_cursors() {
        let mut cursors = PageCursorTable::new();
        assert_eq!(
            resolve_cursor(&cursors, "s\u{0}t", Some("c0"), PageDirection::Initial),
            CursorOutcome::Use("c0".to_owned())
        );
        assert_eq!(
            resolve_cursor(&cursors, "s\u{0}t", Some("c0"), PageDirection::Next),
            CursorOutcome::Noop
        );
        cursors.insert(
            "s\u{0}t".to_owned(),
            PageCursor {
                current: Some("c1".to_owned()),
                next: Some("c2".to_owned()),
                previous: vec!["c0".to_owned()],
            },
        );
        assert_eq!(
            resolve_cursor(&cursors, "s\u{0}t", None, PageDirection::Next),
            CursorOutcome::Use("c2".to_owned())
        );
        assert_eq!(
            resolve_cursor(&cursors, "s\u{0}t", None, PageDirection::Previous),
            CursorOutcome::Use("c0".to_owned())
        );
    }

    #[test]
    fn cursor_and_index_advance_and_rewind() {
        let current = PageCursor {
            current: Some("c1".to_owned()),
            next: Some("c2".to_owned()),
            previous: vec!["c0".to_owned()],
        };
        let next = next_page_cursors(
            &current,
            Some("c2"),
            Some("c3".to_owned()),
            PageDirection::Next,
        );
        assert_eq!(next.previous, ["c0", "c1"]);
        assert_eq!(next.next.as_deref(), Some("c3"));
        let previous = next_page_cursors(
            &current,
            Some("c0"),
            Some("c1".to_owned()),
            PageDirection::Previous,
        );
        assert!(previous.previous.is_empty());
        assert_eq!(next_page_index(2, PageDirection::Next), 3);
        assert_eq!(next_page_index(2, PageDirection::Previous), 1);
        assert_eq!(next_page_index(0, PageDirection::Previous), 0);
    }

    #[test]
    fn scoped_rows_win_then_first_load_then_cached_token() {
        let options = SyncOptions {
            insert: true,
            update: false,
            delete: false,
        };
        let scopes = vec![TableSelectionScope {
            source_table: "s".to_owned(),
            target_table: "t".to_owned(),
            selection_mode: crate::DataSyncSelectionScopeMode::All,
            operations: vec![crate::DataSyncOperation::Insert],
            excluded_rows: vec![],
        }];
        assert!(page_selection_decision(
            &table(),
            crate::DataSyncOperation::Insert,
            &[],
            false,
            &scopes,
            &options,
            true,
            &Default::default()
        ));
        assert!(page_selection_decision(
            &table(),
            crate::DataSyncOperation::Update,
            &[],
            true,
            &scopes,
            &options,
            true,
            &Default::default()
        ));
        let mut excluded = scopes.clone();
        excluded[0]
            .excluded_rows
            .push(crate::data_sync_options::ScopeExclusion {
                operation: crate::DataSyncOperation::Insert,
                key: vec![serde_json::json!(1)],
            });
        assert!(!page_selection_decision(
            &table(),
            crate::DataSyncOperation::Insert,
            &[serde_json::json!(1)],
            true,
            &excluded,
            &options,
            true,
            &Default::default()
        ));

        let mut tokens = std::collections::BTreeSet::new();
        let token = selected_row_token(&crate::DataSyncSelectedRow {
            source_table: "s".to_owned(),
            target_table: "t".to_owned(),
            operation: crate::DataSyncOperation::Update,
            key: vec![serde_json::json!(2)],
        });
        tokens.insert(token);
        assert!(page_selection_decision(
            &table(),
            crate::DataSyncOperation::Update,
            &[serde_json::json!(2)],
            false,
            &[],
            &options,
            false,
            &tokens
        ));
        let _ = ComparisonRow {
            operation: crate::DataSyncOperation::Insert,
            key: vec![],
            selected: false,
        };
    }
}
