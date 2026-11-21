use std::collections::{BTreeMap, BTreeSet};

use crate::{SessionMetadataIdentity, SqlNamespace};

use super::namespace::collect_table_leaf_names;
use super::types::{
    PathDatabaseEntry, SchemaSessionState, SchemaTableInfo, VisibleDatabases,
};

const SYSTEM_DATABASES: [&str; 7] = [
    "information_schema",
    "mysql",
    "performance_schema",
    "sys",
    "postgres",
    "template0",
    "template1",
];

pub fn compute_is_multi_database(has_multi_database: bool, database_count: usize) -> bool {
    has_multi_database && database_count > 1
}

pub fn is_system_database_name(name: &str) -> bool {
    SYSTEM_DATABASES.iter().any(|system| system == &name.to_ascii_lowercase())
}

pub fn is_schema_grouping_schema(schema: Option<&str>) -> bool {
    schema.is_some_and(|name| name != "CATALOG" && name != "SCHEMA")
}

pub fn resolve_preferred_database(databases: &[String], preferred: Option<&str>) -> Option<String> {
    if let Some(preferred) = preferred {
        if databases.iter().any(|database| database == preferred) {
            return Some(preferred.to_owned());
        }
    }
    databases
        .iter()
        .find(|database| !is_system_database_name(database))
        .cloned()
        .or_else(|| databases.first().cloned())
}

pub fn resolve_visible_databases(
    all_databases: &[String],
    preferred: Option<&str>,
) -> VisibleDatabases {
    let configured = preferred.map(str::trim).filter(|name| !name.is_empty());
    if let Some(configured) = configured {
        if all_databases.iter().any(|database| database == configured) {
            return VisibleDatabases {
                databases: vec![configured.to_owned()],
                preferred: Some(configured.to_owned()),
                locked_to_configured: true,
            };
        }
    }
    VisibleDatabases {
        preferred: resolve_preferred_database(all_databases, configured),
        databases: all_databases.to_vec(),
        locked_to_configured: false,
    }
}

pub fn parse_path_hierarchy_database_entry(entry: &str) -> PathDatabaseEntry {
    let Some(colon) = entry.find(':') else {
        return PathDatabaseEntry { id: entry.to_owned(), name: entry.to_owned() };
    };
    let id = &entry[..colon];
    let rest = &entry[colon + 1..];
    if rest.ends_with(')') {
        if let Some(open) = rest[..rest.len() - 1].rfind('(') {
            let backend = &rest[open + 1..rest.len() - 1];
            if !backend.is_empty() && !backend.contains(')') {
                return PathDatabaseEntry {
                    id: id.to_owned(),
                    name: rest[..open].trim_end().to_owned(),
                };
            }
        }
    }
    PathDatabaseEntry { id: id.to_owned(), name: rest.to_owned() }
}

pub fn known_table_names(
    tree: &SqlNamespace,
    tables: &[SchemaTableInfo],
    views: &[SchemaTableInfo],
    path_items: &BTreeMap<String, Vec<SchemaTableInfo>>,
) -> BTreeSet<String> {
    let mut names = collect_table_leaf_names(tree);
    for item in tables.iter().chain(views.iter()) {
        names.insert(item.name.clone());
    }
    for items in path_items.values() {
        for item in items {
            if item.schema.as_deref() == Some("CATALOG") || item.schema.as_deref() == Some("SCHEMA") {
                continue;
            }
            let last = item.name.split('/').filter(|part| !part.is_empty()).last();
            names.insert(last.unwrap_or(&item.name).to_owned());
        }
    }
    names
}

pub fn latest_identity(entry: &SchemaSessionState) -> SessionMetadataIdentity {
    SessionMetadataIdentity {
        connection_id: entry.connection_id.clone(),
        metadata_revision: entry.metadata_revision,
    }
}
