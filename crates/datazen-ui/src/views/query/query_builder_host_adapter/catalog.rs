//! Pure ports of the catalog/identity work in `QueryBuilderHostAdapter.tsx`.
//!
//! The adapter builds a `QueryBuilderCatalog` from the active session's schema:
//! it collects catalog items + path items for the selected database, filters by
//! schema, de-duplicates by `[schema, name]`, counts name collisions, and keeps
//! column metadata only for relations that survived as a unique name. All of it
//! is deterministic and side-effect free; the host owns the real stores.

use std::collections::BTreeMap;

use super::model::{
    QueryBuilderCatalogVm, RelationVm, SessionSchemaVm,
};

/// Minimal JSON string escaping, matching `JSON.stringify` for the identifier
/// strings (table/schema names) used in the catalog and context keys.
#[allow(dead_code)] // pure helper; pinned by the adapter tests.
pub(crate) fn json_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < '\u{20}' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `JSON.stringify([a, b, c, d])` with already-stringified inputs.
#[allow(dead_code)] // pure helper; pinned by the adapter tests.
pub(crate) fn json_stringify_array(items: &[&str]) -> String {
    let body = items.iter().map(|item| json_quote(item)).collect::<Vec<_>>().join(",");
    format!("[{body}]")
}

/// `JSON.stringify([relation.schema ?? null, relation.name])`.
#[allow(dead_code)] // pure helper; pinned by the adapter tests.
pub(crate) fn relation_identity(schema: Option<&str>, name: &str) -> String {
    let schema_part = match schema {
        Some(value) => json_quote(value),
        None => "null".to_owned(),
    };
    format!("[{schema_part},{}]", json_quote(name))
}

/// The context key React passes to `openFor(panelId, contextKey)`.
#[allow(dead_code)] // pure helper; pinned by the adapter tests.
pub(crate) fn qb_context_key(
    connection_id: &str,
    db_session_id: &str,
    database: &str,
    schema: Option<&str>,
) -> String {
    json_stringify_array(&[connection_id, db_session_id, database, schema.unwrap_or("")])
}

/// `DB_REGISTRY[type]?.sqlDialect ?? type`: the host resolves the registry
/// dialect and passes it in; `None` falls back to the raw database type.
#[allow(dead_code)] // pure helper; pinned by the adapter tests.
pub(crate) fn dialect_family(database_type: &str, dialect_override: Option<&str>) -> String {
    dialect_override.unwrap_or(database_type).to_owned()
}

/// React's `catalog` `useMemo`, folded into a pure builder.
#[allow(dead_code)] // pure rule; pinned by the adapter tests.
pub(crate) fn build_catalog(
    session: Option<&SessionSchemaVm>,
    database: &str,
    schema: Option<&str>,
) -> QueryBuilderCatalogVm {
    let mut catalog = QueryBuilderCatalogVm::default();
    let session = match session {
        Some(value) => value,
        None => return catalog,
    };

    // `tableCatalogs[database]` wins; otherwise, when the session is on this
    // database, use tables + views; otherwise nothing.
    let mut catalog_items: Vec<RelationVm> = session
        .table_catalogs
        .get(database)
        .cloned()
        .unwrap_or_else(|| {
            if session.current_database == database {
                let mut items = session.tables.clone();
                items.extend(session.views.iter().cloned());
                items
            } else {
                Vec::new()
            }
        });

    // Path items contribute relations only when the session is on this db.
    if session.current_database == database {
        for relations in session.path_items.values() {
            catalog_items.extend(relations.iter().cloned());
        }
    }

    // Filter by the schema selector, then de-duplicate by [schema, name] (the
    // later relation wins, like `Map.set`). BTreeMap keeps iteration stable.
    let mut unique: BTreeMap<String, RelationVm> = BTreeMap::new();
    for relation in catalog_items {
        if schema.is_some() && relation.schema.as_deref() != schema {
            continue;
        }
        let key = relation_identity(relation.schema.as_deref(), &relation.name);
        unique.insert(key, relation);
    }

    // Count name collisions across different schemas.
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for relation in unique.values() {
        *counts.entry(relation.name.clone()).or_insert(0) += 1;
    }

    // Keep column metadata only for relations that are uniquely named and
    // present in the de-duplicated catalog for this database/schema.
    for entry in session.relation_columns.values() {
        if entry.database != database {
            continue;
        }
        if schema.is_some() && entry.schema.as_deref() != schema {
            continue;
        }
        if counts.get(&entry.name).copied() != Some(1) {
            continue;
        }
        if !unique.contains_key(&relation_identity(entry.schema.as_deref(), &entry.name)) {
            continue;
        }
        catalog.column_map.insert(
            entry.name.clone(),
            entry.columns.iter().map(|column| column.name.clone()).collect(),
        );
        let typed = entry
            .columns
            .iter()
            .map(|column| (column.name.clone(), column.data_type.clone()))
            .collect();
        catalog.typed_column_map.insert(entry.name.clone(), typed);
    }

    // Only uniquely named relations become selectable tables.
    catalog.tables = unique
        .into_values()
        .filter(|relation| counts.get(&relation.name).copied() == Some(1))
        .collect();
    catalog.tables.sort_by(|a, b| {
        (&a.name, a.schema.as_deref()).cmp(&(&b.name, b.schema.as_deref()))
    });
    catalog
}
