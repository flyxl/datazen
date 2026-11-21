use std::collections::{BTreeMap, BTreeSet};

use super::{
    keys::relation_columns_cache_key, ConnectionSchemaState, RelationColumn, RelationColumns,
    RelationRef, SessionMetadataIdentity, RELATION_COLUMN_BATCH_SIZE,
};

pub struct ReadColumnsRow {
    pub status_ok: bool,
    pub value: RelationColumns,
}

pub fn relation_batch_chunks(relations: &[RelationRef]) -> Vec<&[RelationRef]> {
    relations.chunks(RELATION_COLUMN_BATCH_SIZE).collect()
}

pub fn load_relation_columns_step<F>(
    session: &SessionMetadataIdentity,
    db_session_id: &str,
    relations: &[RelationRef],
    transport: F,
) -> Vec<RelationColumns>
where
    F: Fn(&SessionMetadataIdentity, &str, &[RelationRef]) -> Vec<ReadColumnsRow>,
{
    let mut values = Vec::new();
    for batch in relation_batch_chunks(relations) {
        let expected: BTreeSet<String> = batch
            .iter()
            .map(|relation| relation_columns_cache_key(session, db_session_id, relation))
            .collect();
        for row in transport(session, db_session_id, batch) {
            let key = relation_columns_cache_key(session, db_session_id, &row.value.relation);
            if row.status_ok && expected.contains(&key) {
                values.push(row.value);
            }
        }
    }
    values
}

pub fn resolve_column_relations<C, S>(
    state: &ConnectionSchemaState,
    names: &[String],
    db_session_id: &str,
    database: &str,
    schema_filter: Option<&str>,
    has_schema_level: C,
    schema_of: S,
) -> Vec<RelationRef>
where
    C: Fn() -> bool,
    S: Fn(Option<&str>) -> Option<String>,
{
    let catalog: Vec<super::SchemaItem> = match state.table_catalogs.get(database) {
        Some(items) => items.clone(),
        None if state.current_database.as_deref() == Some(database)
            || state.current_database.is_none() =>
        {
            let mut items = state.tables.clone();
            items.extend(state.views.iter().cloned());
            items
        }
        None => Vec::new(),
    };
    let path_items: Vec<super::SchemaItem> = if state.current_database.as_deref() == Some(database)
        || state.current_database.is_none()
    {
        state.path_items.values().flatten().cloned().collect()
    } else {
        Vec::new()
    };
    let items: Vec<super::SchemaItem> = catalog
        .iter()
        .cloned()
        .chain(path_items.iter().cloned())
        .collect();
    let known: BTreeSet<String> = items.iter().map(|item| item.name.clone()).collect();
    let session = SessionMetadataIdentity {
        connection_id: state.connection_id.clone(),
        metadata_revision: state.metadata_revision,
    };

    let mut unique = Vec::new();
    let mut seen = BTreeSet::new();
    for name in names {
        let name = name.trim();
        if name.is_empty() || !seen.insert(name.to_owned()) {
            continue;
        }
        let mut candidates: BTreeMap<String, RelationRef> = BTreeMap::new();
        for item in items.iter() {
            if item.name != name {
                continue;
            }
            let schema = schema_of(item.schema.as_deref());
            if schema_filter.is_some() && schema.as_deref() != schema_filter {
                continue;
            }
            let relation = RelationRef {
                database: database.to_owned(),
                schema,
                name: item.name.clone(),
            };
            let key = relation_columns_cache_key(&session, db_session_id, &relation);
            candidates.insert(key, relation);
        }
        if candidates.len() == 1 {
            unique.push(
                candidates
                    .into_values()
                    .next()
                    .unwrap_or_else(|| RelationRef {
                        database: database.to_owned(),
                        schema: None,
                        name: name.to_owned(),
                    }),
            );
        } else if candidates.is_empty()
            && known.contains(name)
            && !has_schema_level()
            && schema_filter.is_none()
            && (state.namespace_owned_by_plugin
                || !state.path_aliases.is_empty()
                || (catalog.is_empty() && state.current_database.is_none()))
        {
            unique.push(RelationRef {
                database: database.to_owned(),
                schema: None,
                name: name.to_owned(),
            });
        }
    }
    unique
}

pub fn project_relation_columns<C, S>(
    state: &ConnectionSchemaState,
    db_session_id: &str,
    database: &str,
    schema_filter: Option<&str>,
    has_schema_level: C,
    schema_of: S,
) -> (
    BTreeMap<String, Vec<String>>,
    BTreeMap<String, BTreeMap<String, String>>,
)
where
    C: Fn() -> bool,
    S: Fn(Option<&str>) -> Option<String>,
{
    let names: Vec<String> = state
        .relation_columns
        .values()
        .map(|value| value.relation.name.clone())
        .collect();
    let refs = resolve_column_relations(
        state,
        &names,
        db_session_id,
        database,
        schema_filter,
        has_schema_level,
        schema_of,
    );
    let session = SessionMetadataIdentity {
        connection_id: state.connection_id.clone(),
        metadata_revision: state.metadata_revision,
    };
    let mut column_map = BTreeMap::new();
    let mut typed_column_map = BTreeMap::new();
    for relation in refs {
        let key = relation_columns_cache_key(&session, db_session_id, &relation);
        if let Some(value) = state.relation_columns.get(&key) {
            let names: Vec<String> = value
                .columns
                .iter()
                .map(|column| column.name.clone())
                .collect();
            let typed: BTreeMap<String, String> = value
                .columns
                .iter()
                .map(|column| (column.name.clone(), column.data_type.clone()))
                .collect();
            column_map.insert(relation.name.clone(), names);
            typed_column_map.insert(relation.name, typed);
        }
    }
    (column_map, typed_column_map)
}

pub fn column(name: &str, data_type: &str) -> RelationColumn {
    RelationColumn {
        name: name.to_owned(),
        data_type: data_type.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema_column_loading::SchemaItem;

    fn session() -> SessionMetadataIdentity {
        SessionMetadataIdentity {
            connection_id: "c1".to_owned(),
            metadata_revision: 1,
        }
    }

    fn relation(name: &str) -> RelationRef {
        RelationRef {
            database: "shop".to_owned(),
            schema: Some("public".to_owned()),
            name: name.to_owned(),
        }
    }

    fn columns(name: &str) -> RelationColumns {
        RelationColumns {
            relation: relation(name),
            columns: vec![column("id", "int")],
        }
    }

    #[test]
    fn chunks_are_bounded_by_the_transport_limit() {
        let refs: Vec<RelationRef> = (0..257)
            .map(|index| relation(&format!("t{index}")))
            .collect();
        let chunks = relation_batch_chunks(&refs);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), 256);
        assert_eq!(chunks[1].len(), 1);
    }

    #[test]
    fn transport_results_are_limited_to_the_expected_chunk() {
        let refs = vec![relation("a"), relation("b")];
        let values = load_relation_columns_step(&session(), "s1", &refs, |_, _, batch| {
            assert_eq!(batch.len(), 2);
            vec![
                ReadColumnsRow {
                    status_ok: true,
                    value: columns("a"),
                },
                ReadColumnsRow {
                    status_ok: false,
                    value: columns("b"),
                },
                ReadColumnsRow {
                    status_ok: true,
                    value: columns("other"),
                },
            ]
        });
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].relation.name, "a");
    }

    #[test]
    fn legacy_names_resolve_only_when_catalog_identity_is_unique() {
        let mut state = ConnectionSchemaState {
            connection_id: "c1".to_owned(),
            metadata_revision: 1,
            ..Default::default()
        };
        state.tables.push(SchemaItem {
            name: "users".to_owned(),
            schema: Some("public".to_owned()),
        });
        state.tables.push(SchemaItem {
            name: "dup".to_owned(),
            schema: Some("a".to_owned()),
        });
        state.tables.push(SchemaItem {
            name: "dup".to_owned(),
            schema: Some("b".to_owned()),
        });
        let refs = resolve_column_relations(
            &state,
            &[
                " users ".to_owned(),
                "users".to_owned(),
                "dup".to_owned(),
                "".to_owned(),
            ],
            "s1",
            "shop",
            None,
            || false,
            |schema| schema.map(str::to_owned),
        );
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].name, "users");
    }

    #[test]
    fn path_tree_names_fall_back_to_an_unqualified_relation() {
        let mut state = ConnectionSchemaState {
            connection_id: "c1".to_owned(),
            metadata_revision: 1,
            namespace_owned_by_plugin: true,
            ..Default::default()
        };
        state.path_items.insert(
            "p".to_owned(),
            vec![SchemaItem {
                name: "orders".to_owned(),
                schema: None,
            }],
        );
        let refs = resolve_column_relations(
            &state,
            &["orders".to_owned()],
            "s1",
            "shop",
            None,
            || false,
            |schema| schema.map(str::to_owned),
        );
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].schema, None);
    }

    #[test]
    fn projection_builds_both_maps_from_resolved_refs() {
        let mut state = ConnectionSchemaState {
            connection_id: "c1".to_owned(),
            metadata_revision: 1,
            ..Default::default()
        };
        state.tables.push(SchemaItem {
            name: "users".to_owned(),
            schema: Some("public".to_owned()),
        });
        let value = columns("users");
        let key = relation_columns_cache_key(&session(), "s1", &value.relation);
        state.relation_columns.insert(key, value);
        let (plain, typed) = project_relation_columns(
            &state,
            "s1",
            "shop",
            None,
            || false,
            |schema| schema.map(str::to_owned),
        );
        assert_eq!(plain["users"], ["id"]);
        assert_eq!(typed["users"]["id"], "int");
    }
}
