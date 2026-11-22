use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContentViewCallbacks;

#[derive(Clone, Debug, PartialEq)]
pub struct QueryPanelProps {
    pub panel_id: String,
    pub focused_pane_id: Option<String>,
    pub db_session_id: String,
    pub connection_id: String,
    pub database_type: Option<String>,
    pub connection_name: Option<String>,
    pub database: String,
    pub schema: Option<String>,
    pub namespace_path: Vec<String>,
    pub callbacks: Option<ContentViewCallbacks>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct QueryDiagnosisSchemaState {
    pub current_database: Option<String>,
    pub current_schema: Option<String>,
    pub tables: Vec<String>,
    pub views: Vec<String>,
    pub column_map: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct QueryDiagnosisExecution {
    pub sql: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QueryPanelDiagnosisContextInput {
    pub execution: QueryDiagnosisExecution,
    pub connection_id: String,
    pub db_session_id: String,
    pub database_type: Option<String>,
    pub connection_name: Option<String>,
    pub database: String,
    pub schema: Option<String>,
    pub server_version: Option<String>,
    pub schema_state: QueryDiagnosisSchemaState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecuteKind {
    Full,
    Selection,
}

impl ExecuteKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Selection => "selection",
        }
    }

    pub fn from_str(value: &str) -> Self {
        if value == "selection" {
            Self::Selection
        } else {
            Self::Full
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PendingExecute {
    pub kind: ExecuteKind,
    pub sql: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueryExecutionGateRefs {
    pub editor_available: bool,
    pub pending_available: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConnectionContext {
    pub connection_id: String,
    pub db_session_id: String,
    pub database_type: Option<String>,
    pub name: Option<String>,
    pub server_version: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SchemaContext {
    pub tables: Vec<String>,
    pub views: Vec<String>,
    pub columns: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DiagnosisContextInput {
    pub sql: String,
    pub error: Option<String>,
    pub connection_id: String,
    pub db_session_id: String,
    pub database_type: Option<String>,
    pub database: String,
    pub schema: Option<String>,
    pub connection_context: ConnectionContext,
    pub schema_context: SchemaContext,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DiagnosisContext {
    pub context_fingerprint: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RetryValidationInput {
    pub sql: String,
    pub context_fingerprint: String,
    pub bound_params: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PanelView {
    pub connection_id: String,
    pub db_session_id: String,
    pub database_type: Option<String>,
    pub connection_name: Option<String>,
    pub database: Option<String>,
    pub schema: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActiveConnectionView {
    pub connection_id: String,
    pub db_session_id: Option<String>,
    pub status: String,
    pub current_database: Option<String>,
    pub server_version: Option<String>,
}

pub trait QueryContractsHost {
    fn build_diagnosis_context(&self, input: &DiagnosisContextInput) -> Option<DiagnosisContext>;

    fn find_panel_execution(
        &self,
        panel_id: &str,
        pane_id: Option<&str>,
    ) -> Option<(PanelView, QueryDiagnosisExecution)>;

    fn active_connection(&self, connection_id: &str) -> Option<ActiveConnectionView>;

    fn schema_state(&self, db_session_id: &str) -> QueryDiagnosisSchemaState;

    fn sql_parameter_policy(&self, database_type: Option<&str>) -> String;

    fn parse_sql_param_count(&self, sql: &str, policy: &str) -> usize;

    fn params_to_payload(
        &self,
        param_values: &BTreeMap<String, String>,
    ) -> BTreeMap<String, String>;
}

fn is_word(char: char) -> bool {
    char.is_ascii_alphanumeric() || char == '_'
}

fn operator_length(chars: &[char], index: usize) -> usize {
    match chars[index] {
        '=' => 1,
        '<' if chars.get(index + 1) == Some(&'>') => 2,
        '!' if chars.get(index + 1) == Some(&'=') => 2,
        _ => {
            if index > 0 && is_word(chars[index - 1]) {
                return 0;
            }
            for word in ["ILIKE", "LIKE"] {
                let candidates = word.chars();
                if chars.len() >= index + word.len()
                    && chars[index..index + word.len()]
                        .iter()
                        .zip(candidates)
                        .all(|(value, expected)| value.eq_ignore_ascii_case(&expected))
                {
                    let end = index + word.len();
                    if end == chars.len() || !is_word(chars[end]) {
                        return word.len();
                    }
                }
            }
            0
        }
    }
}

pub fn has_suspicious_postgres_double_quoted_literal(sql: &str) -> bool {
    let chars = sql.chars().collect::<Vec<_>>();
    let mut index = 0;
    while index < chars.len() {
        let operator = operator_length(&chars, index);
        if operator == 0 {
            index += 1;
            continue;
        }
        let mut quote = index + operator;
        while quote < chars.len() && chars[quote].is_whitespace() {
            quote += 1;
        }
        if quote >= chars.len() || chars[quote] != '"' {
            index += 1;
            continue;
        }
        let open = quote;
        let mut close = open + 1;
        while close < chars.len() && chars[close] != '"' {
            close += 1;
        }
        if close >= chars.len() || close == open + 1 {
            index += 1;
            continue;
        }
        let before = if open > 0 { chars[open - 1] } else { '\0' };
        let after = chars.get(close + 1).copied().unwrap_or('\0');
        if before == '.' || after == '.' {
            index = close + 1;
            continue;
        }
        return true;
    }
    false
}

pub fn build_query_panel_diagnosis_context(
    input: &QueryPanelDiagnosisContextInput,
    host: &dyn QueryContractsHost,
) -> Option<DiagnosisContext> {
    let context = DiagnosisContextInput {
        sql: input.execution.sql.clone(),
        error: input.execution.error.clone(),
        connection_id: input.connection_id.clone(),
        db_session_id: input.db_session_id.clone(),
        database_type: input.database_type.clone(),
        database: if input.database.is_empty() {
            input
                .schema_state
                .current_database
                .clone()
                .unwrap_or_default()
        } else {
            input.database.clone()
        },
        schema: input
            .schema
            .clone()
            .or_else(|| input.schema_state.current_schema.clone()),
        connection_context: ConnectionContext {
            connection_id: input.connection_id.clone(),
            db_session_id: input.db_session_id.clone(),
            database_type: input.database_type.clone(),
            name: input.connection_name.clone(),
            server_version: input.server_version.clone(),
        },
        schema_context: SchemaContext {
            tables: input.schema_state.tables.clone(),
            views: input.schema_state.views.clone(),
            columns: input.schema_state.column_map.clone(),
        },
    };
    host.build_diagnosis_context(&context)
}

pub fn read_current_query_panel_retry_validation_input(
    panel_id: &str,
    param_values: &BTreeMap<String, String>,
    pane_id: Option<&str>,
    host: &dyn QueryContractsHost,
) -> Option<RetryValidationInput> {
    let (panel, execution) = host.find_panel_execution(panel_id, pane_id)?;
    let active = host.active_connection(&panel.connection_id)?;
    let panel_has_session = !panel.db_session_id.trim().is_empty();
    let active_has_session = active
        .db_session_id
        .as_deref()
        .map(|id| !id.trim().is_empty())
        .unwrap_or(false);
    let matches = active.connection_id == panel.connection_id
        && active.status == "connected"
        && panel_has_session
        && active_has_session
        && active.db_session_id.as_deref() == Some(panel.db_session_id.as_str());
    if !matches {
        return None;
    }

    let schema_state = host.schema_state(&panel.db_session_id);
    let database = panel
        .database
        .clone()
        .or_else(|| schema_state.current_database.clone())
        .or_else(|| active.current_database.clone())
        .unwrap_or_default();
    let context_input = QueryPanelDiagnosisContextInput {
        execution,
        connection_id: panel.connection_id.clone(),
        db_session_id: panel.db_session_id.clone(),
        database_type: panel.database_type.clone(),
        connection_name: panel.connection_name.clone(),
        database,
        schema: panel.schema.clone(),
        server_version: active.server_version.clone(),
        schema_state,
    };
    let context = build_query_panel_diagnosis_context(&context_input, host)?;

    let policy = host.sql_parameter_policy(panel.database_type.as_deref());
    let count = host.parse_sql_param_count(&context_input.execution.sql, &policy);
    let bound_params = if count > 0 {
        host.params_to_payload(param_values)
    } else {
        BTreeMap::new()
    };
    Some(RetryValidationInput {
        sql: context_input.execution.sql,
        context_fingerprint: context.context_fingerprint,
        bound_params,
    })
}

pub const QUERY_CONTRACTS_SURFACE: [&str; 9] = [
    "QueryPanelProps",
    "QueryDiagnosisSchemaState",
    "QueryDiagnosisExecution",
    "ExecuteKind",
    "PendingExecute",
    "hasSuspiciousPostgresDoubleQuotedLiteral",
    "buildQueryPanelDiagnosisContext",
    "readCurrentQueryPanelRetryValidationInput",
    "QUERY_CONTRACTS_GAP",
];

pub const QUERY_CONTRACTS_GAP: [&str; 7] = [
    "React props and the SqlEditor/MutableRefObject handles stay view-side",
    "T586 ContentViewCallbacks stays a local opaque marker until that port lands",
    "buildQueryDiagnosisContext is injected as QueryContractsHost::build_diagnosis_context",
    "parseSqlParams / paramsToPayload / resolveSqlParameterPolicy are injected seams",
    "the three zustand store reads are collapsed into QueryContractsHost lookups",
    "createEmptyConnectionSchema / projectRelationColumns are host-side schema preparation",
    "no DOM or IPC calls exist in this module",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Host {
        panel: Option<(PanelView, QueryDiagnosisExecution)>,
        active: Option<ActiveConnectionView>,
        schema: QueryDiagnosisSchemaState,
        context: Option<DiagnosisContext>,
        contexts: std::cell::RefCell<Vec<DiagnosisContextInput>>,
        policies: std::cell::RefCell<Vec<String>>,
        param_counts: std::cell::RefCell<Vec<(String, String)>>,
    }

    impl QueryContractsHost for Host {
        fn build_diagnosis_context(
            &self,
            input: &DiagnosisContextInput,
        ) -> Option<DiagnosisContext> {
            self.contexts.borrow_mut().push(input.clone());
            self.context.clone()
        }

        fn find_panel_execution(
            &self,
            _panel_id: &str,
            _pane_id: Option<&str>,
        ) -> Option<(PanelView, QueryDiagnosisExecution)> {
            self.panel.clone()
        }

        fn active_connection(&self, _connection_id: &str) -> Option<ActiveConnectionView> {
            self.active.clone()
        }

        fn schema_state(&self, _db_session_id: &str) -> QueryDiagnosisSchemaState {
            self.schema.clone()
        }

        fn sql_parameter_policy(&self, database_type: Option<&str>) -> String {
            let policy = database_type.unwrap_or("standard").to_owned();
            self.policies.borrow_mut().push(policy.clone());
            policy
        }

        fn parse_sql_param_count(&self, sql: &str, policy: &str) -> usize {
            self.param_counts
                .borrow_mut()
                .push((sql.to_owned(), policy.to_owned()));
            sql.matches('?').count()
        }

        fn params_to_payload(&self, values: &BTreeMap<String, String>) -> BTreeMap<String, String> {
            values
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        }
    }

    fn panel() -> PanelView {
        PanelView {
            connection_id: "conn".to_owned(),
            db_session_id: "sess".to_owned(),
            database_type: Some("postgres".to_owned()),
            connection_name: Some("Prod".to_owned()),
            database: None,
            schema: None,
        }
    }

    fn active() -> ActiveConnectionView {
        ActiveConnectionView {
            connection_id: "conn".to_owned(),
            db_session_id: Some("sess".to_owned()),
            status: "connected".to_owned(),
            current_database: Some("app".to_owned()),
            server_version: Some("16".to_owned()),
        }
    }

    fn host() -> Host {
        Host {
            panel: Some((
                panel(),
                QueryDiagnosisExecution {
                    sql: "SELECT * FROM t WHERE id = ?".to_owned(),
                    error: None,
                },
            )),
            active: Some(active()),
            context: Some(DiagnosisContext {
                context_fingerprint: "fp".to_owned(),
            }),
            ..Host::default()
        }
    }

    fn context_input() -> QueryPanelDiagnosisContextInput {
        QueryPanelDiagnosisContextInput {
            execution: QueryDiagnosisExecution {
                sql: "SELECT 1".to_owned(),
                error: Some("boom".to_owned()),
            },
            connection_id: "conn".to_owned(),
            db_session_id: "sess".to_owned(),
            database_type: Some("postgres".to_owned()),
            connection_name: Some("Prod".to_owned()),
            database: String::new(),
            schema: None,
            server_version: Some("16".to_owned()),
            schema_state: QueryDiagnosisSchemaState {
                current_database: Some("app".to_owned()),
                current_schema: Some("public".to_owned()),
                ..QueryDiagnosisSchemaState::default()
            },
        }
    }

    #[test]
    fn execute_kind_literals_round_trip() {
        assert_eq!(ExecuteKind::Full.as_str(), "full");
        assert_eq!(ExecuteKind::from_str("selection"), ExecuteKind::Selection);
        assert_eq!(ExecuteKind::from_str("other"), ExecuteKind::Full);
    }

    #[test]
    fn a_double_quoted_comparison_literal_is_suspicious() {
        for sql in [
            "SELECT * FROM t WHERE name = \"Ann\"",
            "SELECT * FROM t WHERE n <> \"1\"",
            "SELECT * FROM t WHERE n != \"1\"",
            "SELECT * FROM t WHERE name LIKE \"Ann\"",
            "SELECT * FROM t WHERE name ilike \"Ann\"",
        ] {
            assert!(has_suspicious_postgres_double_quoted_literal(sql), "{sql}");
        }
    }

    #[test]
    fn qualified_identifiers_and_single_quotes_are_not_suspicious() {
        for sql in [
            "SELECT * FROM t1 JOIN t2 ON \"t1\".\"id\" = \"t2\".\"t1_id\"",
            "SELECT * FROM \"public\".\"users\" WHERE \"users\".\"name\" = 'Ann'",
            "SELECT * FROM t WHERE name = 'Ann'",
            "SELECT * FROM t WHERE name = \"\"",
            "SELECT * FROM t WHERE name = Ann",
            "SELECT * FROM t WHERE likely \"Ann\"",
        ] {
            assert!(!has_suspicious_postgres_double_quoted_literal(sql), "{sql}");
        }
    }

    #[test]
    fn the_diagnosis_context_falls_back_to_the_schema_state() {
        let h = host();
        let context = build_query_panel_diagnosis_context(&context_input(), &h).expect_context();
        assert_eq!(context.context_fingerprint, "fp");
        let mapped = &h.contexts.borrow()[0];
        assert_eq!(mapped.database, "app");
        assert_eq!(mapped.schema.as_deref(), Some("public"));
        assert_eq!(mapped.connection_context.name.as_deref(), Some("Prod"));
        assert_eq!(
            mapped.connection_context.server_version.as_deref(),
            Some("16")
        );
    }

    trait ExpectContext {
        fn expect_context(self) -> DiagnosisContext;
    }

    impl ExpectContext for Option<DiagnosisContext> {
        fn expect_context(self) -> DiagnosisContext {
            match self {
                Some(context) => context,
                None => panic!("expected a context"),
            }
        }
    }

    #[test]
    fn retry_validation_needs_a_live_matching_session() {
        let mut h = host();
        h.active = None;
        assert!(
            read_current_query_panel_retry_validation_input("p", &BTreeMap::new(), None, &h)
                .is_none()
        );

        h.active = Some(ActiveConnectionView {
            status: "disconnected".to_owned(),
            ..active()
        });
        assert!(
            read_current_query_panel_retry_validation_input("p", &BTreeMap::new(), None, &h)
                .is_none()
        );

        h.active = Some(ActiveConnectionView {
            db_session_id: Some("other".to_owned()),
            ..active()
        });
        assert!(
            read_current_query_panel_retry_validation_input("p", &BTreeMap::new(), None, &h)
                .is_none()
        );
        assert!(h.contexts.borrow().is_empty());
    }

    #[test]
    fn retry_validation_binds_params_only_when_the_sql_declares_them() {
        let h = host();
        let values = BTreeMap::from([("id".to_owned(), "7".to_owned())]);
        let result =
            read_current_query_panel_retry_validation_input("p", &values, Some("side"), &h)
                .expect_retry();
        assert_eq!(result.sql, "SELECT * FROM t WHERE id = ?");
        assert_eq!(result.context_fingerprint, "fp");
        assert_eq!(result.bound_params["id"], "7");
        assert_eq!(h.policies.borrow().as_slice(), &["postgres".to_owned()]);
        assert_eq!(h.param_counts.borrow()[0].1, "postgres");
    }

    #[test]
    fn retry_validation_without_params_sends_an_empty_payload() {
        let mut h = host();
        if let Some((_, execution)) = &mut h.panel {
            execution.sql = "SELECT 1".to_owned();
        }
        let values = BTreeMap::from([("ignored".to_owned(), "x".to_owned())]);
        let result =
            read_current_query_panel_retry_validation_input("p", &values, None, &h).expect_retry();
        assert!(result.bound_params.is_empty());
    }

    trait ExpectRetry {
        fn expect_retry(self) -> RetryValidationInput;
    }

    impl ExpectRetry for Option<RetryValidationInput> {
        fn expect_retry(self) -> RetryValidationInput {
            match self {
                Some(value) => value,
                None => panic!("expected retry validation"),
            }
        }
    }

    #[test]
    fn gap_and_surface_are_pinned_and_root_reexports_match() {
        assert_eq!(QUERY_CONTRACTS_SURFACE.len(), 9);
        assert!(QUERY_CONTRACTS_SURFACE.contains(&"ExecuteKind"));
        assert_eq!(QUERY_CONTRACTS_GAP.len(), 7);
        assert_eq!(crate::QUERY_CONTRACTS_SURFACE.len(), 9);
        assert_eq!(crate::QUERY_CONTRACTS_GAP.len(), 7);
        let _: Option<crate::PendingExecute> = Some(PendingExecute {
            kind: ExecuteKind::Full,
            sql: None,
        });
    }
}
