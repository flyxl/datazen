#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EndpointScope {
    pub source_id: String,
    pub target_id: String,
    pub source_database: String,
    pub target_database: String,
    pub source_schema: String,
    pub target_schema: String,
    pub source_databases: Vec<String>,
    pub target_databases: Vec<String>,
    pub source_schemas: Vec<String>,
    pub target_schemas: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointField {
    SourceId,
    TargetId,
    SourceDatabase,
    TargetDatabase,
    SourceSchema,
    TargetSchema,
}

pub struct ChangeEffect {
    pub field: EndpointField,
    pub value: String,
    pub reset_compare: bool,
    pub clear_pending_profile: bool,
}

pub fn apply_endpoint_change(
    scope: &mut EndpointScope,
    field: EndpointField,
    value: String,
) -> ChangeEffect {
    match field {
        EndpointField::SourceId => scope.source_id = value.clone(),
        EndpointField::TargetId => scope.target_id = value.clone(),
        EndpointField::SourceDatabase => scope.source_database = value.clone(),
        EndpointField::TargetDatabase => scope.target_database = value.clone(),
        EndpointField::SourceSchema => scope.source_schema = value.clone(),
        EndpointField::TargetSchema => scope.target_schema = value.clone(),
    }
    ChangeEffect {
        field,
        value,
        reset_compare: true,
        clear_pending_profile: true,
    }
}

pub fn swap_scope(scope: &mut EndpointScope) {
    std::mem::swap(&mut scope.source_id, &mut scope.target_id);
    std::mem::swap(&mut scope.source_database, &mut scope.target_database);
    std::mem::swap(&mut scope.source_schemas, &mut scope.target_schemas);
    std::mem::swap(&mut scope.source_schema, &mut scope.target_schema);
    std::mem::swap(&mut scope.source_databases, &mut scope.target_databases);
}

pub fn can_refresh_sessions(scope: &EndpointScope) -> bool {
    !scope.source_id.is_empty()
        && !scope.target_id.is_empty()
        && !scope.source_database.is_empty()
        && !scope.target_database.is_empty()
}

pub const CONNECT_FAILED_KEY: &str = "sync.connectFailed";
pub const LOAD_DATABASES_FAILED_KEY: &str = "sync.loadDatabasesFailed";
pub const SESSION_FAILED_KEY: &str = "sync.sessionFailed";

pub fn connect_failed_message(detail: &str) -> String {
    format!("{CONNECT_FAILED_KEY} {detail}")
}

pub fn load_databases_failed_message(detail: &str) -> String {
    format!(
        "{LOAD_DATABASES_KEY_PREFIX}{detail}",
        LOAD_DATABASES_KEY_PREFIX = LOAD_DATABASES_FAILED_KEY
    )
}

pub fn session_failed_message(detail: &str) -> String {
    format!("{SESSION_FAILED_KEY}{detail}")
}

pub fn clear_endpoint_selection(scope: &mut EndpointScope, side: super::catalog::PrefillSide) {
    match side {
        super::catalog::PrefillSide::Source => {
            scope.source_databases.clear();
            scope.source_schemas.clear();
            scope.source_schema.clear();
        }
        super::catalog::PrefillSide::Target => {
            scope.target_databases.clear();
            scope.target_schemas.clear();
            scope.target_schema.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_sync_endpoints::catalog::PrefillSide;

    fn scope() -> EndpointScope {
        EndpointScope {
            source_id: "s".to_owned(),
            target_id: "t".to_owned(),
            source_database: "d1".to_owned(),
            target_database: "d2".to_owned(),
            source_schema: "public".to_owned(),
            target_schema: "audit".to_owned(),
            source_databases: vec!["d1".to_owned()],
            target_databases: vec!["d2".to_owned()],
            source_schemas: vec!["public".to_owned()],
            target_schemas: vec!["audit".to_owned()],
        }
    }

    #[test]
    fn every_endpoint_change_resets_compare_and_pending_profile() {
        let mut item = scope();
        let effect = apply_endpoint_change(&mut item, EndpointField::SourceId, "s2".to_owned());
        assert_eq!(item.source_id, "s2");
        assert!(effect.reset_compare);
        assert!(effect.clear_pending_profile);
        assert_eq!(effect.field, EndpointField::SourceId);
    }

    #[test]
    fn swap_exchanges_all_endpoint_fields() {
        let mut item = scope();
        swap_scope(&mut item);
        assert_eq!(item.source_id, "t");
        assert_eq!(item.target_id, "s");
        assert_eq!(item.source_database, "d2");
        assert_eq!(item.target_database, "d1");
        assert_eq!(item.source_schema, "audit");
        assert_eq!(item.target_schema, "public");
        assert_eq!(item.source_databases, ["d2"]);
        assert_eq!(item.target_schemas, ["public"]);
    }

    #[test]
    fn session_refresh_needs_all_four_fields() {
        let mut item = scope();
        assert!(can_refresh_sessions(&item));
        item.target_database.clear();
        assert!(!can_refresh_sessions(&item));
    }

    #[test]
    fn error_messages_pin_the_i18n_keys_and_spacing() {
        assert_eq!(connect_failed_message("boom"), "sync.connectFailed boom");
        assert_eq!(
            load_databases_failed_message("boom"),
            "sync.loadDatabasesFailedboom"
        );
        assert_eq!(session_failed_message("boom"), "sync.sessionFailedboom");
    }

    #[test]
    fn clearing_a_side_drops_its_databases_and_schemas() {
        let mut item = scope();
        clear_endpoint_selection(&mut item, PrefillSide::Source);
        assert!(item.source_databases.is_empty());
        assert!(item.source_schemas.is_empty());
        assert_eq!(item.source_schema, "");
        assert_eq!(item.target_databases, ["d2"]);
    }
}
