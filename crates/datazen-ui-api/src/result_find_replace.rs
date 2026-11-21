use std::sync::LazyLock;

use crate::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultFindOperator {
    Contains,
    NotContains,
    Prefix,
    Suffix,
}

impl ResultFindOperator {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::NotContains => "notContains",
            Self::Prefix => "prefix",
            Self::Suffix => "suffix",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultFindQuery {
    pub needle: String,
    pub column: Option<String>,
    pub operator: ResultFindOperator,
    pub match_case: bool,
}

pub static EMPTY_RESULT_FIND_QUERY: LazyLock<ResultFindQuery> = LazyLock::new(|| ResultFindQuery {
    needle: String::new(),
    column: None,
    operator: ResultFindOperator::Contains,
    match_case: false,
});

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultReplacePlanItem {
    pub row: usize,
    pub column: String,
    pub value: String,
}

pub fn cell_text(value: Option<&Value>) -> Option<String> {
    match value {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) => Some(text.clone()),
        Some(Value::Number(number)) => Some(number.to_string()),
        Some(Value::Bool(flag)) => Some(flag.to_string()),
        Some(other) => serde_json::to_string(other).ok(),
    }
}

pub fn cell_matches(value: Option<&Value>, query: &ResultFindQuery) -> bool {
    let needle = if query.match_case {
        query.needle.clone()
    } else {
        query.needle.to_ascii_lowercase()
    };
    let Some(text) = cell_text(value) else {
        return query.operator == ResultFindOperator::NotContains && !needle.is_empty();
    };
    let hay = if query.match_case { text } else { text.to_ascii_lowercase() };
    match query.operator {
        ResultFindOperator::Contains => !needle.is_empty() && hay.contains(&needle),
        ResultFindOperator::NotContains => !hay.contains(&needle),
        ResultFindOperator::Prefix => !needle.is_empty() && hay.starts_with(&needle),
        ResultFindOperator::Suffix => !needle.is_empty() && hay.ends_with(&needle),
    }
}

pub fn row_matches(row: &[Option<Value>], columns: &[String], query: &ResultFindQuery) -> bool {
    if query.needle.is_empty() {
        return true;
    }
    match query.column.as_ref() {
        None => {
            if query.operator == ResultFindOperator::NotContains {
                columns
                    .iter()
                    .enumerate()
                    .all(|(index, _)| cell_matches(row.get(index).and_then(Option::as_ref), query))
            } else {
                columns
                    .iter()
                    .enumerate()
                    .any(|(index, _)| cell_matches(row.get(index).and_then(Option::as_ref), query))
            }
        }
        Some(column) => {
            let Some(index) = columns.iter().position(|candidate| candidate == column) else {
                return true;
            };
            cell_matches(row.get(index).and_then(Option::as_ref), query)
        }
    }
}

pub fn filter_result_rows(
    rows: &[Vec<Option<Value>>],
    columns: &[String],
    query: &ResultFindQuery,
) -> Vec<usize> {
    rows.iter()
        .enumerate()
        .filter(|(_, row)| row_matches(row, columns, query))
        .map(|(index, _)| index)
        .collect()
}

pub fn replace_text(text: &str, query: &ResultFindQuery, replacement: &str) -> String {
    if query.match_case {
        return text.replace(&query.needle, replacement);
    }
    if query.needle.is_empty() {
        return text.to_owned();
    }
    let needle = query.needle.to_ascii_lowercase();
    let mut result = String::new();
    let mut rest = text;
    loop {
        let folded = rest.to_ascii_lowercase();
        let Some(at) = folded.find(&needle) else {
            break;
        };
        result.push_str(&rest[..at]);
        result.push_str(replacement);
        rest = &rest[at + needle.len()..];
    }
    result.push_str(rest);
    result
}

pub fn plan_result_replacements(
    rows: &[Vec<Option<Value>>],
    columns: &[String],
    query: &ResultFindQuery,
    replacement: &str,
    limit: Option<usize>,
) -> Vec<ResultReplacePlanItem> {
    if query.needle.is_empty() || query.needle == replacement {
        return Vec::new();
    }
    let limit = limit.unwrap_or(usize::MAX);
    let mut plan = Vec::new();
    for (row_index, row) in rows.iter().enumerate() {
        if plan.len() >= limit {
            break;
        }
        for (column_index, name) in columns.iter().enumerate() {
            if plan.len() >= limit {
                break;
            }
            if query.column.as_ref().is_some_and(|column| column != name) {
                continue;
            }
            let value = row.get(column_index).and_then(Option::as_ref);
            if !cell_matches(value, query) {
                continue;
            }
            let Some(text) = cell_text(value) else {
                continue;
            };
            plan.push(ResultReplacePlanItem {
                row: row_index,
                column: name.clone(),
                value: replace_text(&text, query, replacement),
            });
        }
    }
    plan
}

pub fn next_plan_item(plan: &[ResultReplacePlanItem], from: usize) -> Option<&ResultReplacePlanItem> {
    plan.iter().find(|item| item.row >= from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn query(needle: &str, operator: ResultFindOperator, match_case: bool) -> ResultFindQuery {
        ResultFindQuery {
            needle: needle.to_owned(),
            column: None,
            operator,
            match_case,
        }
    }

    #[test]
    fn operator_literals_and_the_empty_query() {
        assert_eq!(ResultFindOperator::Contains.as_str(), "contains");
        assert_eq!(ResultFindOperator::NotContains.as_str(), "notContains");
        assert_eq!(ResultFindOperator::Prefix.as_str(), "prefix");
        assert_eq!(ResultFindOperator::Suffix.as_str(), "suffix");
        assert_eq!(*EMPTY_RESULT_FIND_QUERY, query("", ResultFindOperator::Contains, false));
    }

    #[test]
    fn cell_text_uses_the_rendered_text_and_null_is_nothing() {
        assert_eq!(cell_text(None), None);
        assert_eq!(cell_text(Some(&Value::Null)), None);
        assert_eq!(cell_text(Some(&json!("abc"))), Some("abc".to_owned()));
        assert_eq!(cell_text(Some(&json!(42))), Some("42".to_owned()));
        assert_eq!(cell_text(Some(&json!(true))), Some("true".to_owned()));
        assert_eq!(cell_text(Some(&json!([1, 2]))), Some("[1,2]".to_owned()));
    }

    #[test]
    fn null_cells_only_satisfy_not_contains() {
        let q = query("x", ResultFindOperator::Contains, false);
        assert!(!cell_matches(None, &q));
        let not = query("x", ResultFindOperator::NotContains, false);
        assert!(cell_matches(None, &not));
        assert!(!cell_matches(None, &query("", ResultFindOperator::NotContains, false)));
    }

    #[test]
    fn operators_respect_case_and_empty_needles() {
        let text = Some(&json!("Hello World"));
        assert!(cell_matches(text, &query("hello", ResultFindOperator::Contains, false)));
        assert!(!cell_matches(text, &query("hello", ResultFindOperator::Contains, true)));
        assert!(cell_matches(text, &query("hello", ResultFindOperator::Prefix, false)));
        assert!(cell_matches(text, &query("world", ResultFindOperator::Suffix, false)));
        assert!(!cell_matches(text, &query("", ResultFindOperator::Prefix, false)));
        assert!(!cell_matches(text, &query("", ResultFindOperator::NotContains, false)));
    }

    #[test]
    fn any_column_not_contains_keeps_rows_with_a_null_cell() {
        let columns = vec!["a".to_owned(), "b".to_owned()];
        let row = vec![Some(json!("x")), None];
        let not = query("x", ResultFindOperator::NotContains, false);
        assert!(!row_matches(&row, &columns, &not));
        let other = query("zzz", ResultFindOperator::NotContains, false);
        assert!(row_matches(&row, &columns, &other));
        let contains = query("x", ResultFindOperator::Contains, false);
        assert!(row_matches(&row, &columns, &contains));
    }

    #[test]
    fn named_column_and_unknown_column_behavior() {
        let columns = vec!["a".to_owned(), "b".to_owned()];
        let row = vec![Some(json!("x")), Some(json!("y"))];
        let mut q = query("x", ResultFindOperator::Contains, false);
        q.column = Some("a".to_owned());
        assert!(row_matches(&row, &columns, &q));
        q.column = Some("b".to_owned());
        assert!(!row_matches(&row, &columns, &q));
        q.column = Some("missing".to_owned());
        assert!(row_matches(&row, &columns, &q));
    }

    #[test]
    fn an_empty_needle_keeps_every_row() {
        let rows = vec![vec![Some(json!("x"))], vec![None]];
        let columns = vec!["a".to_owned()];
        assert_eq!(filter_result_rows(&rows, &columns, &query("", ResultFindOperator::Contains, false)), vec![0, 1]);
        assert_eq!(filter_result_rows(&rows, &columns, &query("", ResultFindOperator::NotContains, false)), vec![0, 1]);
    }

    #[test]
    fn replacement_only_touches_matching_cells_in_original_coordinates() {
        let rows = vec![
            vec![Some(json!("aXbXc")), Some(json!("keep"))],
            vec![Some(json!("none"))],
        ];
        let columns = vec!["a".to_owned(), "b".to_owned()];
        let q = query("X", ResultFindOperator::Contains, true);
        let plan = plan_result_replacements(&rows, &columns, &q, "Y", None);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0], ResultReplacePlanItem { row: 0, column: "a".to_owned(), value: "aYbYc".to_owned() });
    }

    #[test]
    fn replace_all_and_no_op_rules() {
        let rows = vec![vec![Some(json!("x x"))]];
        let columns = vec!["a".to_owned()];
        let q = query("x", ResultFindOperator::Contains, false);
        assert_eq!(plan_result_replacements(&rows, &columns, &q, "y", None)[0].value, "y y");
        assert!(plan_result_replacements(&rows, &columns, &query("", ResultFindOperator::Contains, false), "y", None).is_empty());
        assert!(plan_result_replacements(&rows, &columns, &q, "x", None).is_empty());
    }

    #[test]
    fn case_insensitive_replacement_preserves_original_casing() {
        let text = "HeLLo hello";
        let q = query("hello", ResultFindOperator::Contains, false);
        assert_eq!(replace_text(text, &q, "bye"), "bye bye");
    }

    #[test]
    fn limits_and_next_plan_item_walk_in_source_order() {
        let rows = vec![
            vec![Some(json!("x")), Some(json!("x"))],
            vec![Some(json!("x"))],
        ];
        let columns = vec!["a".to_owned(), "b".to_owned()];
        let q = query("x", ResultFindOperator::Contains, false);
        let limited = plan_result_replacements(&rows, &columns, &q, "y", Some(2));
        assert_eq!(limited.len(), 2);
        assert_eq!(limited[0].row, 0);
        assert_eq!(limited[1].column, "b");
        assert_eq!(next_plan_item(&limited, 1), None);
        let plan = plan_result_replacements(&rows, &columns, &q, "y", None);
        assert_eq!(plan.len(), 3);
        assert_eq!(next_plan_item(&plan, 1).map(|item| item.row), Some(1));
        assert_eq!(next_plan_item(&plan, 2), None);
    }
}
