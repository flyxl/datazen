//! Base common SQL functions in the mounted Query canvas. Registry-backed
//! driver additions and schema/semantic sources are not part of this source.
use std::collections::BTreeMap;

use datazen_ui_api::{build_function_completions, last_bare_sql_keyword};

use super::model::starts_with_ignore_ascii_case;

pub(super) type FunctionApplications = BTreeMap<String, String>;

pub(super) fn applications(doc: &str, start: usize, prefix: &str) -> FunctionApplications {
    if !plain_select_expression(doc, start, prefix) {
        return FunctionApplications::new();
    }

    // Reuse the React-port registry's common entries and exact label/apply
    // pairs. There is no live Native driver-function registry here yet.
    build_function_completions(None, &Vec::new())
        .into_iter()
        .filter(|entry| starts_with_ignore_ascii_case(&entry.label, prefix))
        .map(|entry| (entry.label, entry.apply))
        .collect()
}

pub(super) fn plain_select_expression(doc: &str, start: usize, prefix: &str) -> bool {
    // Intentionally do not guess across SQL dialect quoting/comment rules,
    // previous statements, schema-qualified names, aliases or split tokens.
    if doc.len() > 8_192 {
        return false;
    }
    let Some(before) = doc.get(..start) else { return false; };
    let Some(after) = start.checked_add(prefix.len()).and_then(|end| doc.get(end..)) else {
        return false;
    };
    if before.contains([';', '\'', '"', '`', '$', '\\', '[', ']', '#'])
        || before.contains("--") || before.contains("/*") || before.contains("*/")
        || after.chars().next().is_some_and(|c|
            c.is_alphanumeric() || matches!(c, '_' | '.' | '$' | '"' | '`' | '[' | ']'))
        || after.trim_start().starts_with('(')
    {
        return false;
    }

    let leading = before.trim_start();
    let Some(rest) = leading.get(6..) else { return false; };
    if !leading.get(..6).is_some_and(|w| w.eq_ignore_ascii_case("SELECT"))
        || !rest.chars().next().is_some_and(char::is_whitespace)
        || !last_bare_sql_keyword(before).is_some_and(|w| matches!(w.as_str(), "select" | "distinct"))
    {
        return false;
    }
    let left = rest.trim();
    left.is_empty()
        || left.eq_ignore_ascii_case("DISTINCT")
        || left.eq_ignore_ascii_case("ALL")
        || left.chars().next_back().is_some_and(|c|
            matches!(c, ',' | '(' | '+' | '-' | '*' | '/' | '%' | '~' | '=' | '<' | '>' | '|' | '&'))
}

#[cfg(test)]
#[path = "function_completion/tests.rs"]
mod tests;
