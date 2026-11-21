use std::cmp::Ordering;

use serde_json::json;

use super::search::{compare_results, type_matches};
use super::types::{
    ObjectSearchObjectType, ObjectSearchResult, ObjectSearchResultGroup, ObjectSearchSourceKind,
};

pub fn filter_schema_objects_by_type(
    results: &[ObjectSearchResult],
    object_types: &[ObjectSearchObjectType],
) -> Vec<ObjectSearchResult> {
    results
        .iter()
        .filter(|result| type_matches(result, object_types))
        .cloned()
        .collect()
}

pub fn filter_object_search_results_by_type(
    results: &[ObjectSearchResult],
    object_types: &[ObjectSearchObjectType],
) -> Vec<ObjectSearchResult> {
    filter_schema_objects_by_type(results, object_types)
}

pub fn group_schema_objects(results: &[ObjectSearchResult]) -> Vec<ObjectSearchResultGroup> {
    let mut groups: Vec<ObjectSearchResultGroup> = Vec::new();
    for result in results {
        let key = json!([
            result.connection_id,
            result.database,
            result.schema,
            result.object_type.as_str(),
        ])
        .to_string();
        if let Some(group) = groups.iter_mut().find(|group| group.key == key) {
            group.results.push(result.clone());
            continue;
        }
        groups.push(ObjectSearchResultGroup {
            key,
            connection_id: result.connection_id.clone(),
            connection_name: result.connection_name.clone(),
            database: result.database.clone(),
            schema: result.schema.clone(),
            object_type: result.object_type,
            results: vec![result.clone()],
        });
    }
    for group in &mut groups {
        group.results.sort_by(compare_results);
    }
    groups.sort_by(|a, b| match (a.results.first(), b.results.first()) {
        (Some(first), Some(second)) => compare_results(first, second),
        _ => Ordering::Equal,
    });
    groups
}

pub fn group_object_search_results(results: &[ObjectSearchResult]) -> Vec<ObjectSearchResultGroup> {
    group_schema_objects(results)
}

#[cfg(test)]
mod tests {
    use crate::DatabaseObjectKind;

    use super::*;

    fn result(
        connection_id: &str,
        connection_name: &str,
        database: Option<&str>,
        schema: Option<&str>,
        object_type: ObjectSearchObjectType,
        name: &str,
        source_kind: Option<ObjectSearchSourceKind>,
    ) -> ObjectSearchResult {
        ObjectSearchResult {
            id: format!("{connection_id}-{name}"),
            connection_id: connection_id.to_owned(),
            db_session_id: "s".to_owned(),
            database_type: "postgresql".to_owned(),
            connection_name: Some(connection_name.to_owned()),
            host: None,
            database: database.map(str::to_owned),
            schema: schema.map(str::to_owned),
            object_type,
            name: name.to_owned(),
            object_name: name.to_owned(),
            table_name: None,
            source_kind,
            matched_fields: Vec::new(),
            match_reason: None,
            actions: Vec::new(),
        }
    }

    #[test]
    fn type_filter_keeps_routine_alias_and_procedure_source() {
        let results = vec![
            result(
                "c",
                "Local",
                None,
                None,
                ObjectSearchObjectType::Table,
                "t",
                None,
            ),
            result(
                "c",
                "Local",
                None,
                None,
                ObjectSearchObjectType::Function,
                "f",
                None,
            ),
            result(
                "c",
                "Local",
                None,
                None,
                ObjectSearchObjectType::Routine,
                "r",
                None,
            ),
            result(
                "c",
                "Local",
                None,
                None,
                ObjectSearchObjectType::Routine,
                "p",
                Some(ObjectSearchSourceKind::Catalog(
                    DatabaseObjectKind::Procedure,
                )),
            ),
        ];
        let filtered = filter_schema_objects_by_type(&results, &[ObjectSearchObjectType::Routine]);
        assert_eq!(
            filtered
                .iter()
                .map(|value| value.name.as_str())
                .collect::<Vec<_>>(),
            ["f", "r", "p"]
        );
        let filtered =
            filter_schema_objects_by_type(&results, &[ObjectSearchObjectType::Procedure]);
        assert_eq!(
            filtered
                .iter()
                .map(|value| value.name.as_str())
                .collect::<Vec<_>>(),
            ["p"]
        );
        assert!(filter_schema_objects_by_type(&results, &[]).len() == results.len());
    }

    #[test]
    fn type_filter_alias_matches_the_primary() {
        let results = vec![result(
            "c",
            "Local",
            None,
            None,
            ObjectSearchObjectType::View,
            "v",
            None,
        )];
        let types = [ObjectSearchObjectType::View];
        assert_eq!(
            filter_object_search_results_by_type(&results, &types),
            filter_schema_objects_by_type(&results, &types)
        );
    }

    #[test]
    fn groups_use_the_full_path_key_and_sort_results_and_groups() {
        let results = vec![
            result(
                "c1",
                "Zeta",
                Some("d"),
                Some("s"),
                ObjectSearchObjectType::Table,
                "z",
                None,
            ),
            result(
                "c1",
                "Zeta",
                Some("d"),
                Some("s"),
                ObjectSearchObjectType::Table,
                "a",
                None,
            ),
            result(
                "c1",
                "Zeta",
                Some("d"),
                Some("s"),
                ObjectSearchObjectType::Column,
                "col",
                None,
            ),
            result(
                "c2",
                "Alpha",
                Some("d"),
                Some("s"),
                ObjectSearchObjectType::Table,
                "m",
                None,
            ),
        ];
        let groups = group_schema_objects(&results);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].key, json!(["c2", "d", "s", "table"]).to_string());
        assert_eq!(groups[0].connection_name.as_deref(), Some("Alpha"));
        assert_eq!(groups[1].results.len(), 2);
        assert_eq!(
            groups[1]
                .results
                .iter()
                .map(|value| value.name.as_str())
                .collect::<Vec<_>>(),
            ["a", "z"]
        );
        assert_eq!(groups[2].object_type, ObjectSearchObjectType::Column);
    }

    #[test]
    fn group_alias_matches_the_primary() {
        let results = vec![result(
            "c",
            "Local",
            None,
            None,
            ObjectSearchObjectType::Table,
            "t",
            None,
        )];
        assert_eq!(
            group_object_search_results(&results),
            group_schema_objects(&results)
        );
    }
}
