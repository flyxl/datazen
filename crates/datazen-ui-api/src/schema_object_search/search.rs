use std::cmp::Ordering;
use std::collections::HashMap;

use serde_json::json;

use crate::{DatabaseObjectKind, TableInfo, TableSqlActionKind, TableType};

use super::types::{
    object_type_order, ObjectSearchFilters, ObjectSearchMatchField, ObjectSearchObjectType,
    ObjectSearchResult, ObjectSearchSourceKind, SchemaObjectIndexEntry, SchemaObjectSearchIndex,
};

const TABLE_OBJECT_ACTIONS: [TableSqlActionKind; 5] = [
    TableSqlActionKind::OpenData,
    TableSqlActionKind::Select,
    TableSqlActionKind::Insert,
    TableSqlActionKind::Update,
    TableSqlActionKind::Ddl,
];
const ROUTINE_ACTIONS: [TableSqlActionKind; 1] = [TableSqlActionKind::Ddl];

fn normalize_text(value: Option<&str>) -> Option<String> {
    let trimmed = value.map(str::trim).unwrap_or("");
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn normalize_exact(value: Option<Option<&str>>) -> Option<Option<String>> {
    match value {
        None => None,
        Some(inner) => Some(normalize_text(inner)),
    }
}

fn lower(value: Option<&str>) -> String {
    value.unwrap_or("").to_lowercase()
}

fn relation_object_type(table_type: TableType) -> ObjectSearchObjectType {
    match table_type {
        TableType::View | TableType::MaterializedView => ObjectSearchObjectType::View,
        _ => ObjectSearchObjectType::Table,
    }
}

fn object_object_type(kind: DatabaseObjectKind) -> ObjectSearchObjectType {
    match kind {
        DatabaseObjectKind::Procedure => ObjectSearchObjectType::Routine,
        DatabaseObjectKind::Function => ObjectSearchObjectType::Function,
        DatabaseObjectKind::Trigger => ObjectSearchObjectType::Trigger,
        DatabaseObjectKind::Sequence => ObjectSearchObjectType::Sequence,
        DatabaseObjectKind::Type => ObjectSearchObjectType::Type,
    }
}

fn result_id(result: &ObjectSearchResult) -> String {
    json!([
        result.connection_id,
        result.database,
        result.schema,
        result.object_type.as_str(),
        result.table_name,
        result.name,
    ])
    .to_string()
}

fn column_names_for(
    entry: &SchemaObjectIndexEntry,
    table: &TableInfo,
    database: Option<&str>,
    schema: Option<&str>,
) -> Vec<String> {
    let mut keys = Vec::new();
    if let (Some(database), Some(schema)) = (database, schema) {
        keys.push(format!("{database}.{schema}.{}", table.name));
    }
    if let Some(schema) = schema {
        keys.push(format!("{schema}.{}", table.name));
    }
    keys.push(table.name.clone());

    let maps: [&HashMap<String, Vec<String>>; 2] = [&entry.columns, &entry.column_map];
    for map in maps {
        for key in &keys {
            if let Some(names) = map.get(key) {
                let mut out = Vec::new();
                for name in names {
                    let trimmed = name.trim();
                    if !trimmed.is_empty() && !out.iter().any(|value| value == trimmed) {
                        out.push(trimmed.to_owned());
                    }
                }
                return out;
            }
        }
    }
    Vec::new()
}

fn exact_ref(value: &Option<Option<String>>) -> Option<Option<&str>> {
    value.as_ref().map(|inner| inner.as_deref())
}

fn filter_value_matches(
    actual: Option<&str>,
    exact: Option<Option<&str>>,
    values: &[Option<String>],
) -> bool {
    let normalized_actual = normalize_text(actual);
    if let Some(normalized_exact) = normalize_exact(exact) {
        if normalized_actual != normalized_exact {
            return false;
        }
    }
    if !values.is_empty() {
        let any = values
            .iter()
            .any(|value| normalize_text(value.as_deref()) == normalized_actual);
        if !any {
            return false;
        }
    }
    true
}

pub(crate) fn type_matches(result: &ObjectSearchResult, types: &[ObjectSearchObjectType]) -> bool {
    if types.is_empty() {
        return true;
    }
    types.iter().any(|object_type| match object_type {
        ObjectSearchObjectType::Routine => {
            result.object_type == ObjectSearchObjectType::Function
                || result.object_type == ObjectSearchObjectType::Routine
        }
        ObjectSearchObjectType::Procedure => {
            result.source_kind
                == Some(ObjectSearchSourceKind::Catalog(
                    DatabaseObjectKind::Procedure,
                ))
        }
        other => result.object_type == *other,
    })
}

fn matches_filter(result: &ObjectSearchResult, filters: &ObjectSearchFilters) -> bool {
    if let Some(connection_id) = &filters.connection_id {
        if &result.connection_id != connection_id {
            return false;
        }
    }
    if !filters.connection_ids.is_empty() && !filters.connection_ids.contains(&result.connection_id)
    {
        return false;
    }
    if !filter_value_matches(
        result.database.as_deref(),
        exact_ref(&filters.database),
        &filters.databases,
    ) {
        return false;
    }
    if !filter_value_matches(
        result.schema.as_deref(),
        exact_ref(&filters.schema),
        &filters.schemas,
    ) {
        return false;
    }
    let mut types = Vec::new();
    if let Some(object_type) = filters.object_type {
        types.push(object_type);
    }
    types.extend(filters.object_types.iter().copied());
    types.extend(filters.types.iter().copied());
    type_matches(result, &types)
}

fn matching_fields_for_query(
    result: &ObjectSearchResult,
    query: &str,
) -> Vec<ObjectSearchMatchField> {
    if query.is_empty() {
        return Vec::new();
    }
    let q = lower(Some(query));
    let fields = [
        (
            ObjectSearchMatchField::Name,
            result.connection_name.as_deref(),
        ),
        (ObjectSearchMatchField::Host, result.host.as_deref()),
        (ObjectSearchMatchField::Database, result.database.as_deref()),
        (
            ObjectSearchMatchField::Type,
            Some(result.database_type.as_str()),
        ),
        (ObjectSearchMatchField::Schema, result.schema.as_deref()),
        (
            ObjectSearchMatchField::Object,
            if result.object_type == ObjectSearchObjectType::Column {
                None
            } else {
                Some(result.object_name.as_str())
            },
        ),
        (ObjectSearchMatchField::Table, result.table_name.as_deref()),
        (
            ObjectSearchMatchField::Column,
            if result.object_type == ObjectSearchObjectType::Column {
                Some(result.name.as_str())
            } else {
                None
            },
        ),
    ];
    let mut matched = Vec::new();
    for (field, value) in fields {
        if lower(value).contains(&q) {
            matched.push(field);
        }
    }
    matched
}

fn add_match_metadata(mut result: ObjectSearchResult, query: &str) -> Option<ObjectSearchResult> {
    let matched_fields = matching_fields_for_query(&result, query);
    if !query.is_empty() && matched_fields.is_empty() {
        return None;
    }
    result.match_reason = matched_fields.first().copied();
    result.matched_fields = matched_fields;
    Some(result)
}

pub(crate) fn compare_results(a: &ObjectSearchResult, b: &ObjectSearchResult) -> Ordering {
    for comparison in [
        lower(a.connection_name.as_deref()).cmp(&lower(b.connection_name.as_deref())),
        lower(Some(&a.connection_id)).cmp(&lower(Some(&b.connection_id))),
        lower(a.database.as_deref()).cmp(&lower(b.database.as_deref())),
        lower(a.schema.as_deref()).cmp(&lower(b.schema.as_deref())),
    ] {
        if comparison != Ordering::Equal {
            return comparison;
        }
    }
    let type_comparison = object_type_order(a.object_type).cmp(&object_type_order(b.object_type));
    if type_comparison != Ordering::Equal {
        return type_comparison;
    }
    let a_primary = a.table_name.as_deref().unwrap_or(&a.name);
    let b_primary = b.table_name.as_deref().unwrap_or(&b.name);
    let primary_comparison = lower(Some(a_primary)).cmp(&lower(Some(b_primary)));
    if primary_comparison != Ordering::Equal {
        return primary_comparison;
    }
    lower(Some(&a.name)).cmp(&lower(Some(&b.name)))
}

fn push_result(
    results: &mut Vec<ObjectSearchResult>,
    result: ObjectSearchResult,
    query: &str,
    filters: &ObjectSearchFilters,
) {
    if let Some(matched) = add_match_metadata(result, query) {
        if matches_filter(&matched, filters) {
            results.push(matched);
        }
    }
}

pub fn search_schema_objects(
    index: &SchemaObjectSearchIndex,
    query: &str,
    filters: &ObjectSearchFilters,
) -> Vec<ObjectSearchResult> {
    let entries: &[SchemaObjectIndexEntry] = match index {
        SchemaObjectSearchIndex::Entries(entries) => entries,
        SchemaObjectSearchIndex::Object(index) => &index.entries,
    };
    let query = query.trim();
    let mut results = Vec::new();

    for entry in entries {
        let database = normalize_text(entry.database.as_deref());
        let entry_schema = normalize_text(entry.schema.as_deref());
        let mut relation_keys = Vec::new();

        for table in entry.tables.iter().chain(entry.views.iter()) {
            let table_name = match normalize_text(Some(&table.name)) {
                Some(name) => name,
                None => continue,
            };
            let schema = normalize_text(table.schema.as_deref()).or_else(|| entry_schema.clone());
            let object_type = relation_object_type(table.table_type);
            let relation_key =
                json!([database, schema, object_type.as_str(), table_name]).to_string();
            if relation_keys.contains(&relation_key) {
                continue;
            }
            relation_keys.push(relation_key);

            let mut relation = ObjectSearchResult {
                id: String::new(),
                connection_id: entry.connection_id.clone(),
                db_session_id: entry.db_session_id.clone(),
                database_type: entry.database_type.clone(),
                connection_name: normalize_text(entry.connection_name.as_deref()),
                host: normalize_text(entry.host.as_deref()),
                database: database.clone(),
                schema: schema.clone(),
                object_type,
                name: table_name.clone(),
                object_name: table_name.clone(),
                table_name: None,
                source_kind: Some(ObjectSearchSourceKind::Relation(table.table_type)),
                matched_fields: Vec::new(),
                match_reason: None,
                actions: TABLE_OBJECT_ACTIONS.to_vec(),
            };
            relation.id = result_id(&relation);
            push_result(&mut results, relation, query, filters);

            for column_name in
                column_names_for(entry, table, database.as_deref(), schema.as_deref())
            {
                let mut column = ObjectSearchResult {
                    id: String::new(),
                    connection_id: entry.connection_id.clone(),
                    db_session_id: entry.db_session_id.clone(),
                    database_type: entry.database_type.clone(),
                    connection_name: normalize_text(entry.connection_name.as_deref()),
                    host: normalize_text(entry.host.as_deref()),
                    database: database.clone(),
                    schema: schema.clone(),
                    object_type: ObjectSearchObjectType::Column,
                    name: column_name.clone(),
                    object_name: column_name,
                    table_name: Some(table_name.clone()),
                    source_kind: None,
                    matched_fields: Vec::new(),
                    match_reason: None,
                    actions: TABLE_OBJECT_ACTIONS.to_vec(),
                };
                column.id = result_id(&column);
                push_result(&mut results, column, query, filters);
            }
        }

        for object in &entry.objects {
            let object_name = match normalize_text(Some(&object.name)) {
                Some(name) => name,
                None => continue,
            };
            let schema = normalize_text(object.schema.as_deref()).or_else(|| entry_schema.clone());
            let object_type = object_object_type(object.kind);
            let mut catalog = ObjectSearchResult {
                id: String::new(),
                connection_id: entry.connection_id.clone(),
                db_session_id: entry.db_session_id.clone(),
                database_type: entry.database_type.clone(),
                connection_name: normalize_text(entry.connection_name.as_deref()),
                host: normalize_text(entry.host.as_deref()),
                database: database.clone(),
                schema: schema.clone(),
                object_type,
                name: object_name.clone(),
                object_name: object_name,
                table_name: None,
                source_kind: Some(ObjectSearchSourceKind::Catalog(object.kind)),
                matched_fields: Vec::new(),
                match_reason: None,
                actions: ROUTINE_ACTIONS.to_vec(),
            };
            catalog.id = result_id(&catalog);
            push_result(&mut results, catalog, query, filters);
        }
    }

    results.sort_by(compare_results);
    results
}

#[cfg(test)]
mod tests {
    use crate::{DatabaseObject, TableInfo};

    use super::super::types::SchemaObjectIndex;
    use super::*;

    fn entry() -> SchemaObjectIndexEntry {
        SchemaObjectIndexEntry {
            connection_id: "c1".to_owned(),
            db_session_id: "s1".to_owned(),
            database_type: "postgresql".to_owned(),
            connection_name: Some(" Local ".to_owned()),
            host: Some(" 127.0.0.1 ".to_owned()),
            database: Some(" shop ".to_owned()),
            schema: Some(" public ".to_owned()),
            ..Default::default()
        }
    }

    fn table(name: &str, schema: Option<&str>, table_type: TableType) -> TableInfo {
        TableInfo {
            name: name.to_owned(),
            schema: schema.map(str::to_owned),
            table_type,
            row_count: None,
        }
    }

    fn object(kind: DatabaseObjectKind, schema: Option<&str>, name: &str) -> DatabaseObject {
        DatabaseObject {
            kind,
            schema: schema.map(str::to_owned),
            name: name.to_owned(),
            signature: None,
            target_schema: None,
            target_name: None,
        }
    }

    #[test]
    fn empty_query_returns_relations_columns_and_objects_sorted() {
        let mut item = entry();
        item.tables = vec![table("users", None, TableType::Table)];
        item.columns.insert(
            "shop.public.users".to_owned(),
            vec![" id ".to_owned(), "name".to_owned()],
        );
        item.objects = vec![object(DatabaseObjectKind::Function, None, "count")];
        let index = SchemaObjectSearchIndex::Entries(vec![item]);
        let results = search_schema_objects(&index, "", &ObjectSearchFilters::default());
        assert_eq!(results.len(), 4);
        assert_eq!(
            results
                .iter()
                .map(|value| value.object_type)
                .collect::<Vec<_>>(),
            [
                ObjectSearchObjectType::Table,
                ObjectSearchObjectType::Column,
                ObjectSearchObjectType::Column,
                ObjectSearchObjectType::Function,
            ]
        );
        assert_eq!(results[0].connection_name.as_deref(), Some("Local"));
        assert_eq!(results[0].host.as_deref(), Some("127.0.0.1"));
        assert_eq!(results[0].database.as_deref(), Some("shop"));
        assert_eq!(results[0].schema.as_deref(), Some("public"));
        assert_eq!(results[0].actions.len(), 5);
        assert_eq!(results[3].actions, [TableSqlActionKind::Ddl]);
    }

    #[test]
    fn relation_keys_dedupe_and_materialized_views_are_views() {
        let mut item = entry();
        item.tables = vec![
            table("users", None, TableType::Table),
            table("users", None, TableType::Table),
        ];
        item.views = vec![
            table("users", None, TableType::MaterializedView),
            table("active_users", None, TableType::View),
        ];
        let index = SchemaObjectSearchIndex::Object(SchemaObjectIndex {
            entries: vec![item],
        });
        let results = search_schema_objects(&index, "", &ObjectSearchFilters::default());
        assert_eq!(results.len(), 3);
        assert_eq!(
            results
                .iter()
                .map(|value| (value.name.as_str(), value.object_type))
                .collect::<Vec<_>>(),
            [
                ("users", ObjectSearchObjectType::Table),
                ("active_users", ObjectSearchObjectType::View),
                ("users", ObjectSearchObjectType::View),
            ]
        );
        assert_eq!(
            results[2].source_kind,
            Some(ObjectSearchSourceKind::Relation(
                TableType::MaterializedView
            ))
        );
    }

    #[test]
    fn column_lookup_prefers_qualified_keys_and_accepts_alias_map() {
        let mut item = entry();
        item.tables = vec![table("users", None, TableType::Table)];
        item.columns
            .insert("users".to_owned(), vec!["fallback".to_owned()]);
        item.columns.insert(
            "shop.public.users".to_owned(),
            vec![
                " id ".to_owned(),
                "id".to_owned(),
                "  ".to_owned(),
                "name".to_owned(),
            ],
        );
        let index = SchemaObjectSearchIndex::Entries(vec![item]);
        let results = search_schema_objects(&index, "", &ObjectSearchFilters::default());
        let columns: Vec<&str> = results
            .iter()
            .filter(|value| value.object_type == ObjectSearchObjectType::Column)
            .map(|value| value.name.as_str())
            .collect();
        assert_eq!(columns, ["id", "name"]);

        let mut aliased = entry();
        aliased.tables = vec![table("users", None, TableType::Table)];
        aliased
            .column_map
            .insert("public.users".to_owned(), vec!["alias_col".to_owned()]);
        let index = SchemaObjectSearchIndex::Entries(vec![aliased]);
        let results = search_schema_objects(&index, "", &ObjectSearchFilters::default());
        let columns: Vec<&str> = results
            .iter()
            .filter(|value| value.object_type == ObjectSearchObjectType::Column)
            .map(|value| value.name.as_str())
            .collect();
        assert_eq!(columns, ["alias_col"]);
    }

    #[test]
    fn query_matches_fields_in_stable_order_and_sets_reason() {
        let mut item = entry();
        item.tables = vec![table("users", None, TableType::Table)];
        item.columns
            .insert("users".to_owned(), vec!["email".to_owned()]);
        let index = SchemaObjectSearchIndex::Entries(vec![item]);
        let results = search_schema_objects(&index, "LOCAL", &ObjectSearchFilters::default());
        assert_eq!(results.len(), 2);
        for result in &results {
            assert_eq!(result.matched_fields, [ObjectSearchMatchField::Name]);
            assert_eq!(result.match_reason, Some(ObjectSearchMatchField::Name));
        }

        let results = search_schema_objects(&index, "email", &ObjectSearchFilters::default());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].object_type, ObjectSearchObjectType::Column);
        assert_eq!(results[0].matched_fields, [ObjectSearchMatchField::Column]);
        assert_eq!(
            results[0].match_reason,
            Some(ObjectSearchMatchField::Column)
        );
    }

    #[test]
    fn non_matching_query_drops_results() {
        let mut item = entry();
        item.tables = vec![table("users", None, TableType::Table)];
        let index = SchemaObjectSearchIndex::Entries(vec![item]);
        let results = search_schema_objects(&index, "nothing", &ObjectSearchFilters::default());
        assert!(results.is_empty());
    }

    #[test]
    fn structural_filters_narrow_results() {
        let mut first = entry();
        first.tables = vec![table("users", None, TableType::Table)];
        let mut second = entry();
        second.connection_id = "c2".to_owned();
        second.database = Some("other".to_owned());
        second.schema = Some("audit".to_owned());
        second.tables = vec![table("logs", None, TableType::Table)];
        let index = SchemaObjectSearchIndex::Entries(vec![first, second]);

        let filters = ObjectSearchFilters {
            connection_id: Some("c2".to_owned()),
            ..Default::default()
        };
        let results = search_schema_objects(&index, "", &filters);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "logs");

        let filters = ObjectSearchFilters {
            connection_ids: vec!["c1".to_owned(), "c2".to_owned()],
            databases: vec![Some("shop".to_owned()), Some("other".to_owned())],
            schemas: vec![Some("public".to_owned()), Some("audit".to_owned())],
            ..Default::default()
        };
        assert_eq!(search_schema_objects(&index, "", &filters).len(), 2);

        let filters = ObjectSearchFilters {
            database: Some(None),
            ..Default::default()
        };
        assert!(search_schema_objects(&index, "", &filters).is_empty());
    }

    #[test]
    fn routine_alias_and_procedure_source_kind_filters() {
        let mut item = entry();
        item.objects = vec![
            object(DatabaseObjectKind::Procedure, None, "proc_a"),
            object(DatabaseObjectKind::Function, None, "fn_b"),
            object(DatabaseObjectKind::Trigger, None, "trg_c"),
        ];
        let index = SchemaObjectSearchIndex::Entries(vec![item]);

        let filters = ObjectSearchFilters {
            object_type: Some(ObjectSearchObjectType::Routine),
            ..Default::default()
        };
        let results = search_schema_objects(&index, "", &filters);
        assert_eq!(
            results
                .iter()
                .map(|value| value.name.clone())
                .collect::<Vec<_>>(),
            ["fn_b", "proc_a"]
        );

        let filters = ObjectSearchFilters {
            types: vec![ObjectSearchObjectType::Procedure],
            ..Default::default()
        };
        let results = search_schema_objects(&index, "", &filters);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].object_type, ObjectSearchObjectType::Routine);
        assert_eq!(
            results[0].source_kind,
            Some(ObjectSearchSourceKind::Catalog(
                DatabaseObjectKind::Procedure
            ))
        );
    }

    #[test]
    fn result_id_is_stable_across_paths() {
        let mut item = entry();
        item.tables = vec![table("users", None, TableType::Table)];
        let index = SchemaObjectSearchIndex::Entries(vec![item]);
        let results = search_schema_objects(&index, "", &ObjectSearchFilters::default());
        assert_eq!(
            results[0].id,
            json!(["c1", "shop", "public", "table", null, "users"]).to_string()
        );
    }

    #[test]
    fn database_type_is_kept_from_the_entry() {
        let mut item: SchemaObjectIndexEntry = entry();
        item.database_type = "mysql".to_owned();
        item.tables = vec![table("t", None, TableType::Table)];
        let index = SchemaObjectSearchIndex::Entries(vec![item]);
        let results = search_schema_objects(&index, "mysql", &ObjectSearchFilters::default());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].matched_fields, [ObjectSearchMatchField::Type]);
    }
}
