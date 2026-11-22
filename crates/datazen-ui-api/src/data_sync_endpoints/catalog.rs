#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrefillSide {
    Source,
    Target,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EndpointPrefill {
    pub source_database: Option<String>,
    pub target_database: Option<String>,
    pub source_schema: Option<String>,
    pub target_schema: Option<String>,
}

pub fn resolve_default_database(databases: &[String], preferred: &str, previous: &str) -> String {
    if databases.iter().any(|db| db == preferred) {
        return preferred.to_owned();
    }
    if !previous.is_empty() && databases.iter().any(|db| db == previous) {
        return previous.to_owned();
    }
    databases.first().cloned().unwrap_or_default()
}

pub fn pick_prefill_database<F>(
    prefill: &mut EndpointPrefill,
    side: PrefillSide,
    databases: &[String],
    previous: &str,
    resolve: F,
) -> String
where
    F: Fn(&str) -> String,
{
    let field = match side {
        PrefillSide::Source => &mut prefill.source_database,
        PrefillSide::Target => &mut prefill.target_database,
    };
    if let Some(value) = field.as_ref() {
        if databases.contains(value) {
            let chosen = value.clone();
            *field = None;
            return chosen;
        }
    }
    resolve(previous)
}

pub fn pick_prefill_schema<F>(
    prefill: &mut EndpointPrefill,
    side: PrefillSide,
    schemas: &[String],
    previous: &str,
    resolve: F,
) -> String
where
    F: Fn(&str) -> String,
{
    let field = match side {
        PrefillSide::Source => &mut prefill.source_schema,
        PrefillSide::Target => &mut prefill.target_schema,
    };
    if let Some(value) = field.as_ref() {
        if schemas.contains(value) {
            let chosen = value.clone();
            *field = None;
            return chosen;
        }
    }
    resolve(previous)
}

pub fn unique_schemas_from_tables(schemas: &[Option<String>]) -> Vec<String> {
    let mut values: Vec<String> = schemas
        .iter()
        .filter_map(|schema| {
            schema
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .map(str::to_owned)
        .collect();
    values.sort();
    values.dedup();
    values
}

pub fn pick_default_schema(schemas: &[String], previous: &str) -> String {
    if !previous.is_empty() && schemas.iter().any(|schema| schema == previous) {
        return previous.to_owned();
    }
    if schemas.iter().any(|schema| schema == "public") {
        return "public".to_owned();
    }
    schemas.first().cloned().unwrap_or_default()
}

pub fn schema_load_allowed(
    has_endpoint: bool,
    has_database: bool,
    supports_tables: bool,
    supports_sql: bool,
    session_matches: bool,
) -> bool {
    has_endpoint && has_database && supports_tables && supports_sql && session_matches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_preference_order_is_preferred_previous_then_first() {
        let databases = ["a".to_owned(), "b".to_owned()];
        assert_eq!(resolve_default_database(&databases, "b", "a"), "b");
        assert_eq!(resolve_default_database(&databases, "missing", "b"), "b");
        assert_eq!(resolve_default_database(&databases, "missing", "?"), "a");
        assert_eq!(resolve_default_database(&[], "x", "y"), "");
    }

    #[test]
    fn prefill_is_consumed_once_and_falls_back_afterward() {
        let mut prefill = EndpointPrefill {
            source_database: Some("b".to_owned()),
            ..Default::default()
        };
        let databases = ["a".to_owned(), "b".to_owned()];
        let first =
            pick_prefill_database(&mut prefill, PrefillSide::Source, &databases, "a", |prev| {
                resolve_default_database(&databases, "", prev)
            });
        assert_eq!(first, "b");
        assert_eq!(prefill.source_database, None);
        let second =
            pick_prefill_database(&mut prefill, PrefillSide::Source, &databases, "a", |prev| {
                resolve_default_database(&databases, "", prev)
            });
        assert_eq!(second, "a");
    }

    #[test]
    fn stale_prefill_values_are_ignored() {
        let mut prefill = EndpointPrefill {
            target_schema: Some("gone".to_owned()),
            ..Default::default()
        };
        let schemas = ["public".to_owned()];
        let chosen = pick_prefill_schema(&mut prefill, PrefillSide::Target, &schemas, "", |_| {
            pick_default_schema(&schemas, "")
        });
        assert_eq!(chosen, "public");
        assert_eq!(prefill.target_schema, Some("gone".to_owned()));
    }

    #[test]
    fn schemas_are_trimmed_deduplicated_and_sorted() {
        let schemas = [
            Some(" public ".to_owned()),
            Some("audit".to_owned()),
            None,
            Some(" ".to_owned()),
            Some("audit".to_owned()),
        ];
        assert_eq!(unique_schemas_from_tables(&schemas), ["audit", "public"]);
        assert_eq!(
            pick_default_schema(&["audit".to_owned(), "public".to_owned()], ""),
            "public"
        );
        assert_eq!(pick_default_schema(&["audit".to_owned()], "gone"), "audit");
        assert_eq!(pick_default_schema(&[], ""), "");
    }

    #[test]
    fn schema_load_requires_every_guard() {
        assert!(schema_load_allowed(true, true, true, true, true));
        assert!(!schema_load_allowed(false, true, true, true, true));
        assert!(!schema_load_allowed(true, true, false, true, true));
        assert!(!schema_load_allowed(true, true, true, true, false));
    }
}
