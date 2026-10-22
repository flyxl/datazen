//! The rules that do not depend on where the views are stored.
//!
//! Every one of them is a pure function of its arguments, so the desktop host
//! and a browser build reach the same answer for the same input — and both
//! answers can be tested without a store.

use std::collections::HashSet;

use super::dto::NamedViewScope;

/// Trim a view name and collapse every run of whitespace into one space.
///
/// `"  Open   orders  "` becomes `"Open orders"`. Two names that differ only
/// in whitespace are the same name, and the user should not end up with two
/// entries that read identically.
pub fn normalize_view_name(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether two view names are the same name.
///
/// The comparison is **case-insensitive**: `Users` and `users` would be two
/// entries the user cannot tell apart in a menu.
pub fn view_names_equal(left: &str, right: &str) -> bool {
    normalized_name_key(left) == normalized_name_key(right)
}

/// The key two names are compared by: normalized and lower-cased.
pub fn normalized_name_key(name: &str) -> String {
    normalize_view_name(name).to_lowercase()
}

/// The stable identity of a scope.
///
/// The parts are separated and tagged so that a missing database (`None`) and
/// an empty one (`Some("")`) cannot produce the same key — a table in "no
/// database" and a table in the database named "" are different tables, and
/// their views must not share a scope.
pub fn scope_key(scope: &NamedViewScope) -> String {
    fn part(value: &Option<String>) -> String {
        match value {
            Some(value) => format!("s:{value}"),
            None => "n:".to_string(),
        }
    }
    format!(
        "c:{}|d:{}|s:{}|t:{}",
        scope.connection_id,
        part(&scope.database),
        part(&scope.schema),
        scope.table
    )
}

/// The name an imported view gets.
///
/// The name the file carries, unless the scope already has it — then
/// `"Users"`, `"Users (2)"`, `"Users (3)"`, … The comparison is the same
/// case-insensitive one the scope uses, so an import cannot smuggle in a
/// second `users` next to a `Users`.
pub fn resolve_import_name(name: &str, taken: &HashSet<String>) -> String {
    let base = normalize_view_name(name);
    if base.is_empty() {
        return base;
    }
    if !contains_name(taken, &base) {
        return base;
    }
    // The suffix grows until it finds a free name. The bound is a safety net:
    // a file cannot realistically hold a million views of one name, and an
    // unbounded loop would hang the import.
    for suffix in 2..100_000 {
        let candidate = format!("{base} ({suffix})");
        if !contains_name(taken, &candidate) {
            return candidate;
        }
    }
    format!("{base} (imported)")
}

/// Whether the set already holds this name, compared case-insensitively.
fn contains_name(taken: &HashSet<String>, name: &str) -> bool {
    let key = normalized_name_key(name);
    taken
        .iter()
        .any(|existing| normalized_name_key(existing) == key)
}
