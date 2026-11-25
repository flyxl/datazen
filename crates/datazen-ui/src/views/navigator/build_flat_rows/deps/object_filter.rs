//! `lib/objectFilter.ts`, ported verbatim for the navigator flat-row builder
//! (T570). Removal stage: when the formal `objectFilter` port lands (shard 2,
//! T567), this file moves out from under `build_flat_rows` and the call sites
//! re-point — the behaviour must not change, only the path.
//!
//! Every rule matches React exactly, including the odd ones:
//! - system-schema detection also fires on the `pg_` prefix;
//! - system-database detection is a fixed name set that includes `postgres`;
//! - a glob pattern supports `*` only (`.` etc. are literal), a pattern
//!   without a wildcard is a substring match — see [`match_name_pattern`];
//! - the prefs come off `connection.options.objectFilter`, a JSON blob the
//!   host may have written anything into, so the reader is defensive in
//!   exactly the way the React one is: a wrong-typed field reads as absent.

use serde_json::Value;

use super::super::inputs::ConnectionSnapshot;

/// React `ObjectFilterPrefs` — absent fields mean "no opinion".
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObjectFilterPrefs {
    pub hide_system_schemas: Option<bool>,
    pub table_name_include: Option<String>,
    pub table_name_exclude: Option<String>,
}

const SYSTEM_SCHEMAS: [&str; 6] = [
    "information_schema",
    "pg_catalog",
    "pg_toast",
    "mysql",
    "performance_schema",
    "sys",
];

const SYSTEM_DATABASES: [&str; 7] = [
    "information_schema",
    "mysql",
    "performance_schema",
    "sys",
    "postgres",
    "template0",
    "template1",
];

/// React `getObjectFilter` — read the prefs off the connection's opaque
/// options blob, ignoring anything with the wrong shape.
pub fn get_object_filter(conn: &ConnectionSnapshot) -> ObjectFilterPrefs {
    let Some(Value::Object(record)) = conn.options.as_ref() else {
        return ObjectFilterPrefs::default();
    };
    let Some(Value::Object(raw)) = record.get("objectFilter") else {
        return ObjectFilterPrefs::default();
    };
    ObjectFilterPrefs {
        hide_system_schemas: match raw.get("hideSystemSchemas") {
            Some(Value::Bool(true)) => Some(true),
            Some(Value::Bool(false)) => Some(false),
            _ => None,
        },
        table_name_include: match raw.get("tableNameInclude") {
            Some(Value::String(s)) => Some(s.clone()),
            _ => None,
        },
        table_name_exclude: match raw.get("tableNameExclude") {
            Some(Value::String(s)) => Some(s.clone()),
            _ => None,
        },
    }
}

/// React `isSystemSchemaName` — the fixed set, or any `pg_` prefix.
pub fn is_system_schema_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    if SYSTEM_SCHEMAS.contains(&lower.as_str()) {
        return true;
    }
    lower.starts_with("pg_")
}

/// React `isSystemDatabaseName` — the fixed set only, no prefixes.
pub fn is_system_database_name(name: &str) -> bool {
    SYSTEM_DATABASES.contains(&name.to_lowercase().as_str())
}

/// React `matchNamePattern` — glob (`*` only, everything else literal) or a
/// plain substring match when the pattern has no wildcard.
pub fn match_name_pattern(name: &str, pattern: &str) -> bool {
    let p = pattern.trim();
    if p.is_empty() {
        return true;
    }
    let n = name.to_lowercase();
    let lower = p.to_lowercase();
    if lower.contains('*') {
        // Same contract as React's escaped regex: `^pattern$` with `*` as
        // `.*` and every other metacharacter literal.
        let mut re = String::with_capacity(lower.len() * 2);
        re.push('^');
        for ch in lower.chars() {
            match ch {
                '*' => re.push_str(".*"),
                // React escapes `.+?^${}()|[]\`; the same set is literal here.
                '.' | '+' | '?' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' | '\\' => {
                    re.push('\\');
                    re.push(ch);
                }
                _ => re.push(ch),
            }
        }
        re.push('$');
        glob_match(&re, &n)
    } else {
        n.contains(&lower)
    }
}

/// Anchored `^literal(.*)literal$` matcher over the escaped pattern above.
/// Written by hand so the port keeps the regex-free dependency set the React
/// source had (it built a `RegExp` because JS has one built in). The anchors
/// are structural, not literal: they are stripped here before matching.
fn glob_match(pattern: &str, name: &str) -> bool {
    let body = pattern
        .strip_prefix('^')
        .and_then(|rest| rest.strip_suffix('$'))
        .unwrap_or(pattern);
    fn here(pattern: &[u8], name: &[u8]) -> bool {
        if pattern.is_empty() {
            return name.is_empty();
        }
        if pattern[0] == b'\\' && pattern.len() > 1 {
            return name.first() == Some(&pattern[1]) && here(&pattern[2..], &name[1..]);
        }
        if pattern[0] == b'.' && pattern[1..].first() == Some(&b'*') {
            return here(&pattern[2..], name) || (!name.is_empty() && here(pattern, &name[1..]));
        }
        name.first() == Some(&pattern[0]) && here(&pattern[1..], &name[1..])
    }
    here(body.as_bytes(), name.as_bytes())
}

/// React `matchesTableNameFilter` — include must match, exclude must not.
pub fn matches_table_name_filter(name: &str, filter: &ObjectFilterPrefs) -> bool {
    let include = filter
        .table_name_include
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let exclude = filter
        .table_name_exclude
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(include) = include {
        if !match_name_pattern(name, include) {
            return false;
        }
    }
    if let Some(exclude) = exclude {
        if match_name_pattern(name, exclude) {
            return false;
        }
    }
    true
}

/// React `shouldShowSchema`.
pub fn should_show_schema(schema_name: &str, filter: &ObjectFilterPrefs) -> bool {
    if filter.hide_system_schemas == Some(true) && is_system_schema_name(schema_name) {
        return false;
    }
    true
}

/// React `shouldShowDatabase`.
pub fn should_show_database(db_name: &str, filter: &ObjectFilterPrefs) -> bool {
    if filter.hide_system_schemas == Some(true) && is_system_database_name(db_name) {
        return false;
    }
    true
}

/// React `filterTableItems` — drop system tables and system-schema tables when
/// hiding, then apply the name filter.
pub fn filter_table_items<'a>(
    items: impl IntoIterator<Item = &'a super::super::inputs::TableInfo>,
    filter: &ObjectFilterPrefs,
) -> Vec<super::super::inputs::TableInfo> {
    items
        .into_iter()
        .filter(|item| {
            if filter.hide_system_schemas == Some(true)
                && item.table_type == super::super::inputs::TableType::SystemTable
            {
                return false;
            }
            if let Some(schema) = item.schema.as_deref() {
                if filter.hide_system_schemas == Some(true) && is_system_schema_name(schema) {
                    return false;
                }
            }
            matches_table_name_filter(&item.name, filter)
        })
        .cloned()
        .collect()
}
