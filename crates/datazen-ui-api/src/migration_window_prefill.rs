use std::collections::BTreeMap;

pub const PREFILL_KEYS: [&str; 6] = [
    "sourceId",
    "targetId",
    "sourceDatabase",
    "targetDatabase",
    "sourceSchema",
    "targetSchema",
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MigrationWindowPrefill {
    pub source_id: Option<String>,
    pub target_id: Option<String>,
    pub source_database: Option<String>,
    pub target_database: Option<String>,
    pub source_schema: Option<String>,
    pub target_schema: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MigrationWindowKind {
    DataSync,
    DataTransfer,
    SchemaDiff,
}

impl MigrationWindowKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DataSync => "data-sync",
            Self::DataTransfer => "data-transfer",
            Self::SchemaDiff => "schema-diff",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrefillSide {
    Source,
    Target,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EndpointPrefillEffect {
    pub source_id: Option<String>,
    pub target_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationEndpointConnection {
    pub id: String,
}

pub fn read_migration_prefill_from_url(params: &BTreeMap<String, String>) -> MigrationWindowPrefill {
    let mut prefill = MigrationWindowPrefill::default();
    for key in PREFILL_KEYS {
        let Some(value) = params.get(key) else {
            continue;
        };
        if value.is_empty() {
            continue;
        }
        match key {
            "sourceId" => prefill.source_id = Some(value.clone()),
            "targetId" => prefill.target_id = Some(value.clone()),
            "sourceDatabase" => prefill.source_database = Some(value.clone()),
            "targetDatabase" => prefill.target_database = Some(value.clone()),
            "sourceSchema" => prefill.source_schema = Some(value.clone()),
            "targetSchema" => prefill.target_schema = Some(value.clone()),
            _ => {}
        }
    }
    prefill
}

pub fn build_migration_window_url_params(
    window_kind: MigrationWindowKind,
    prefill: Option<&MigrationWindowPrefill>,
) -> BTreeMap<String, String> {
    let mut params = BTreeMap::new();
    params.insert("window".to_owned(), window_kind.as_str().to_owned());
    let Some(prefill) = prefill else {
        return params;
    };
    let values = [
        ("sourceId", prefill.source_id.as_ref()),
        ("targetId", prefill.target_id.as_ref()),
        ("sourceDatabase", prefill.source_database.as_ref()),
        ("targetDatabase", prefill.target_database.as_ref()),
        ("sourceSchema", prefill.source_schema.as_ref()),
        ("targetSchema", prefill.target_schema.as_ref()),
    ];
    for (key, value) in values {
        if let Some(value) = value {
            if !value.is_empty() {
                params.insert(key.to_owned(), value.clone());
            }
        }
    }
    params
}

pub fn resolve_default_database(
    databases: Option<&[String]>,
    preferred: &str,
    prev: &str,
) -> String {
    let empty: [String; 0] = [];
    let list = databases.unwrap_or(&empty);
    if list.iter().any(|database| database == preferred) {
        return preferred.to_owned();
    }
    if !prev.is_empty() && list.iter().any(|database| database == prev) {
        return prev.to_owned();
    }
    list.first().cloned().unwrap_or_default()
}

pub fn pick_prefill_database(
    prefill: &mut MigrationWindowPrefill,
    side: PrefillSide,
    databases: &[String],
    mut fallback: impl FnMut(&str) -> String,
    prev: &str,
) -> String {
    let target = match side {
        PrefillSide::Source => prefill.source_database.clone(),
        PrefillSide::Target => prefill.target_database.clone(),
    };
    if let Some(value) = target {
        if databases.iter().any(|database| database == &value) {
            match side {
                PrefillSide::Source => prefill.source_database = None,
                PrefillSide::Target => prefill.target_database = None,
            }
            return value;
        }
    }
    fallback(prev)
}

pub fn pick_prefill_schema(
    prefill: &mut MigrationWindowPrefill,
    side: PrefillSide,
    schemas: &[String],
    mut fallback: impl FnMut(&str) -> String,
    prev: &str,
) -> String {
    let target = match side {
        PrefillSide::Source => prefill.source_schema.clone(),
        PrefillSide::Target => prefill.target_schema.clone(),
    };
    if let Some(value) = target {
        if schemas.iter().any(|schema| schema == &value) {
            match side {
                PrefillSide::Source => prefill.source_schema = None,
                PrefillSide::Target => prefill.target_schema = None,
            }
            return value;
        }
    }
    fallback(prev)
}

pub fn apply_migration_endpoint_prefill(
    prefill: &MigrationWindowPrefill,
    applied: &mut bool,
    connections: &[MigrationEndpointConnection],
) -> Option<EndpointPrefillEffect> {
    if *applied || connections.is_empty() {
        return None;
    }
    let source = prefill.source_id.as_ref();
    let target = prefill.target_id.as_ref();
    if source.is_none() && target.is_none() {
        return None;
    }
    *applied = true;
    let has_source = source.is_some_and(|id| connections.iter().any(|connection| &connection.id == id));
    let has_target = target.is_some_and(|id| connections.iter().any(|connection| &connection.id == id));
    Some(EndpointPrefillEffect {
        source_id: if has_source { source.cloned() } else { None },
        target_id: if has_target { target.cloned() } else { None },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(key, value)| ((*key).to_owned(), (*value).to_owned())).collect()
    }

    fn names(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn prefill_keys_and_window_kinds_are_exact() {
        assert_eq!(PREFILL_KEYS, ["sourceId", "targetId", "sourceDatabase", "targetDatabase", "sourceSchema", "targetSchema"]);
        assert_eq!(MigrationWindowKind::DataSync.as_str(), "data-sync");
        assert_eq!(MigrationWindowKind::DataTransfer.as_str(), "data-transfer");
        assert_eq!(MigrationWindowKind::SchemaDiff.as_str(), "schema-diff");
    }

    #[test]
    fn reading_url_params_keeps_truthy_values_only() {
        let read = read_migration_prefill_from_url(&params(&[
            ("sourceId", "c1"),
            ("targetId", ""),
            ("sourceDatabase", "shop"),
            ("targetSchema", "public"),
            ("unrelated", "x"),
        ]));
        assert_eq!(read.source_id.as_deref(), Some("c1"));
        assert_eq!(read.target_id, None);
        assert_eq!(read.source_database.as_deref(), Some("shop"));
        assert_eq!(read.target_schema.as_deref(), Some("public"));
        assert_eq!(read.target_database, None);
        assert_eq!(read.source_schema, None);
    }

    #[test]
    fn url_params_start_with_window_and_add_truthy_prefill() {
        let prefill = MigrationWindowPrefill {
            source_id: Some("c1".to_owned()),
            target_database: Some("shop".to_owned()),
            source_schema: Some("".to_owned()),
            ..Default::default()
        };
        let built = build_migration_window_url_params(MigrationWindowKind::SchemaDiff, Some(&prefill));
        assert_eq!(built.get("window").map(String::as_str), Some("schema-diff"));
        assert_eq!(built.get("sourceId").map(String::as_str), Some("c1"));
        assert_eq!(built.get("targetDatabase").map(String::as_str), Some("shop"));
        assert!(!built.contains_key("sourceSchema"));
        assert_eq!(built.len(), 3);
        let bare = build_migration_window_url_params(MigrationWindowKind::DataSync, None);
        assert_eq!(bare, params(&[("window", "data-sync")]));
    }

    #[test]
    fn default_database_prefers_preferred_then_prev_then_first() {
        let list = names(&["shop", "analytics"]);
        assert_eq!(resolve_default_database(Some(&list), "analytics", "shop"), "analytics");
        assert_eq!(resolve_default_database(Some(&list), "missing", "shop"), "shop");
        assert_eq!(resolve_default_database(Some(&list), "missing", ""), "shop");
        assert_eq!(resolve_default_database(Some(&list), "missing", "also-missing"), "shop");
        assert_eq!(resolve_default_database(None, "x", "y"), "");
        let empty: [String; 0] = [];
        assert_eq!(resolve_default_database(Some(&empty), "x", "y"), "");
    }

    #[test]
    fn picking_a_database_consumes_the_prefill_once() {
        let mut prefill = MigrationWindowPrefill {
            source_database: Some("shop".to_owned()),
            target_database: Some("missing".to_owned()),
            ..Default::default()
        };
        let picked = pick_prefill_database(
            &mut prefill,
            PrefillSide::Source,
            &names(&["shop", "analytics"]),
            |_| "fallback".to_owned(),
            "prev",
        );
        assert_eq!(picked, "shop");
        assert_eq!(prefill.source_database, None);
        let fallback = pick_prefill_database(
            &mut prefill,
            PrefillSide::Target,
            &names(&["shop"]),
            |prev| format!("used-{prev}"),
            "prev",
        );
        assert_eq!(fallback, "used-prev");
        assert_eq!(prefill.target_database.as_deref(), Some("missing"));
    }

    #[test]
    fn picking_a_schema_consumes_the_prefill_once() {
        let mut prefill = MigrationWindowPrefill {
            source_schema: Some("public".to_owned()),
            target_schema: Some("gone".to_owned()),
            ..Default::default()
        };
        let picked = pick_prefill_schema(
            &mut prefill,
            PrefillSide::Source,
            &names(&["public"]),
            |_| "fallback".to_owned(),
            "prev",
        );
        assert_eq!(picked, "public");
        assert_eq!(prefill.source_schema, None);
        let fallback = pick_prefill_schema(
            &mut prefill,
            PrefillSide::Target,
            &names(&["public"]),
            |prev| format!("used-{prev}"),
            "prev",
        );
        assert_eq!(fallback, "used-prev");
        assert_eq!(prefill.target_schema.as_deref(), Some("gone"));
    }

    #[test]
    fn endpoint_prefill_applies_once_and_only_for_loaded_connections() {
        let prefill = MigrationWindowPrefill {
            source_id: Some("c1".to_owned()),
            target_id: Some("c2".to_owned()),
            ..Default::default()
        };
        let connections = vec![MigrationEndpointConnection { id: "c1".to_owned() }];
        let mut applied = false;
        let effect = apply_migration_endpoint_prefill(&prefill, &mut applied, &connections);
        assert_eq!(effect, Some(EndpointPrefillEffect {
            source_id: Some("c1".to_owned()),
            target_id: None,
        }));
        assert!(applied);
        assert_eq!(apply_migration_endpoint_prefill(&prefill, &mut applied, &connections), None);
    }

    #[test]
    fn endpoint_prefill_never_applies_without_ids_or_connections() {
        let empty = MigrationWindowPrefill::default();
        let connections = vec![MigrationEndpointConnection { id: "c1".to_owned() }];
        let mut applied = false;
        assert_eq!(apply_migration_endpoint_prefill(&empty, &mut applied, &connections), None);
        assert!(!applied);
        let prefill = MigrationWindowPrefill { source_id: Some("c1".to_owned()), ..Default::default() };
        assert_eq!(apply_migration_endpoint_prefill(&prefill, &mut applied, &[]), None);
        assert!(!applied);
    }

    #[test]
    fn endpoint_prefill_marks_applied_even_when_no_id_matches() {
        let prefill = MigrationWindowPrefill { source_id: Some("c1".to_owned()), ..Default::default() };
        let connections = vec![MigrationEndpointConnection { id: "other".to_owned() }];
        let mut applied = false;
        let effect = apply_migration_endpoint_prefill(&prefill, &mut applied, &connections);
        assert_eq!(effect, Some(EndpointPrefillEffect { source_id: None, target_id: None }));
        assert!(applied);
    }
}
