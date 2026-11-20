//! Context building and field-resolution helpers — React's
//! `buildQueryDiagnosisContext` and the small readers it leans on.
//!
//! The raw input is a [`serde_json::Value`] because every React
//! `QueryActionInput` field is typed `unknown`. Only a non-empty JSON object
//! is accepted; every other shape is the safe `empty-context` failure.
//! Nothing here rewrites the editor SQL — `query_diagnosis_context.sql` is
//! the original string verbatim, while `safe_sql` is the redacted copy.

use serde_json::{Map, Value};

use super::sanitize::{
    canonicalize, redact_sensitive_text, sanitize_host, sanitize_schema_context,
};
use super::types::{
    PromptConnectionContext, QueryActionBuildResult, QueryActionError, QueryActionErrorCode,
    QueryDiagnosisContext, QueryDiagnosisContextLike, QueryDiagnosisParams, QueryPromptContext,
    SanitizedConnectionContext,
};

/// React `asRecord`: plain objects only (no arrays, no null).
fn as_record(value: &Value) -> Option<&Map<String, Value>> {
    value.as_object()
}

/// React `nonEmptyString`: trim and reject an empty result.
pub(crate) fn non_empty_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// React `firstString`: the first non-empty trimmed string in the list.
fn first_string(values: &[Option<&Value>]) -> Option<String> {
    values.iter().find_map(|value| non_empty_string(*value))
}

/// React `readRecordString` — first non-empty string among several aliases.
fn read_record_string(record: Option<&Map<String, Value>>, keys: &[&str]) -> Option<String> {
    let record = record?;
    first_string(&keys.iter().map(|key| record.get(*key)).collect::<Vec<_>>())
}

/// React `readRecordNumber` — first finite number among several aliases.
fn read_record_number(record: Option<&Map<String, Value>>, keys: &[&str]) -> Option<f64> {
    let record = record?;
    keys.iter()
        .filter_map(|key| record.get(*key))
        .filter_map(Value::as_f64)
        .find(|number| number.is_finite())
}

/// React `readRecordBoolean` — first boolean among several aliases.
fn read_record_boolean(record: Option<&Map<String, Value>>, keys: &[&str]) -> Option<bool> {
    let record = record?;
    keys.iter()
        .filter_map(|key| record.get(*key))
        .find_map(Value::as_bool)
}

/// React `normalizeErrorMessage`: a string, an `Error` (its `message`), or a
/// record with a `message`/`error`/`detail` alias. A JSON object in the port
/// covers React's object branch; a real JS `Error` is just such an object on
/// the wire.
fn normalize_error_message(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(_)) => non_empty_string(value),
        Some(Value::Object(_)) => {
            read_record_string(as_record(value?), &["message", "error", "detail"])
        }
        _ => None,
    }
}

/// React `buildConnectionContext`: the safe allowlist. Any extra field in
/// the raw connection context is ignored here by construction.
fn build_connection_context(
    input: &Map<String, Value>,
    raw_connection_context: Option<&Map<String, Value>>,
    connection_id: String,
    db_session_id: String,
) -> SanitizedConnectionContext {
    let get_raw = |key: &str| raw_connection_context.and_then(|map| map.get(key));
    let name = first_string(&[
        input.get("connectionName"),
        get_raw("connectionName"),
        get_raw("name"),
    ]);
    let host = sanitize_host(first_string(&[input.get("host"), get_raw("host")]).as_deref());
    let port = read_record_number(Some(input), &["port"])
        .or_else(|| read_record_number(raw_connection_context, &["port"]));
    let server_version = first_string(&[input.get("serverVersion"), get_raw("serverVersion")]);
    let read_only = read_record_boolean(Some(input), &["readOnly"])
        .or_else(|| read_record_boolean(raw_connection_context, &["readOnly"]));

    SanitizedConnectionContext {
        connection_id,
        db_session_id,
        name,
        host,
        port,
        server_version,
        read_only,
    }
}

/// React `fingerprint`: FNV-1a over the code points of `canonicalize(value)`,
/// printed as `qctx-<8 hex digits>`.
fn fingerprint(value: &Value) -> String {
    let mut hash: u32 = 0x811c_9dc5;
    for character in canonicalize(value).chars() {
        hash ^= character as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    format!("qctx-{hash:08x}")
}

fn failure(
    code: QueryActionErrorCode,
    message: &str,
) -> QueryActionBuildResult<QueryDiagnosisContext> {
    QueryActionBuildResult::fail(code, message)
}

/// React `buildQueryDiagnosisContext`.
pub fn build_query_diagnosis_context(
    value: Option<&Value>,
) -> QueryActionBuildResult<QueryDiagnosisContext> {
    let input = match value.and_then(as_record) {
        Some(input) if !input.is_empty() => input,
        _ => {
            return failure(
                QueryActionErrorCode::EmptyContext,
                "Query action context is empty.",
            )
        }
    };
    let raw_connection_context = input.get("connectionContext").and_then(as_record);

    // Only a string SQL is accepted; the original text is never trimmed or
    // rewritten — that is the byte-for-byte guarantee Fix SQL / Retry rely on.
    let sql = input.get("sql").and_then(Value::as_str);
    if sql.is_none_or(|text| text.trim().is_empty()) {
        return failure(
            QueryActionErrorCode::MissingSql,
            "Query action context has no SQL.",
        );
    }
    let sql = sql.unwrap_or_default().to_owned();

    // React `input.errorMessage ?? input.error`: the alias wins unless it is
    // nullish. Both aliases are normalized through the same reader.
    let raw_error = input
        .get("errorMessage")
        .filter(|value| !value.is_null())
        .or_else(|| input.get("error"));
    let error_message = match normalize_error_message(raw_error) {
        Some(message) => message,
        None => {
            return failure(
                QueryActionErrorCode::MissingError,
                "Query action context has no error message.",
            )
        }
    };

    let get_raw = |key: &str| raw_connection_context.and_then(|map| map.get(key));
    let connection_id = match first_string(&[
        input.get("connectionId"),
        get_raw("connectionId"),
        get_raw("id"),
    ]) {
        Some(id) => id,
        None => {
            return failure(
                QueryActionErrorCode::MissingConnection,
                "Query action context has no connection.",
            )
        }
    };
    let db_session_id = match first_string(&[input.get("dbSessionId"), get_raw("dbSessionId")]) {
        Some(id) => id,
        None => {
            return failure(
                QueryActionErrorCode::MissingSession,
                "Query action context has no database session.",
            )
        }
    };
    let database_type = match first_string(&[
        input.get("databaseType"),
        input.get("driverType"),
        get_raw("databaseType"),
        get_raw("driverType"),
    ]) {
        Some(kind) => kind,
        None => {
            return failure(
                QueryActionErrorCode::MissingDatabaseType,
                "Query action context has no database type.",
            )
        }
    };
    let database = match first_string(&[
        input.get("database"),
        input.get("currentDatabase"),
        get_raw("database"),
        get_raw("currentDatabase"),
    ]) {
        Some(database) => database,
        None => {
            return failure(
                QueryActionErrorCode::MissingDatabase,
                "Query action context has no database.",
            )
        }
    };
    let schema = first_string(&[
        input.get("schema"),
        input.get("currentSchema"),
        get_raw("schema"),
        get_raw("currentSchema"),
    ]);

    let safe_sql = redact_sensitive_text(&sql);
    let safe_error_message = redact_sensitive_text(&error_message);
    let schema_context = sanitize_schema_context(input.get("schemaContext"));
    let connection_context = build_connection_context(
        input,
        raw_connection_context,
        connection_id.clone(),
        db_session_id.clone(),
    );
    let prompt_connection_context = PromptConnectionContext {
        name: connection_context.name.clone(),
        host: connection_context.host.clone(),
        port: connection_context.port,
        server_version: connection_context.server_version.clone(),
        read_only: connection_context.read_only,
    };
    let prompt_context = QueryPromptContext {
        sql: safe_sql.clone(),
        error_message: safe_error_message.clone(),
        database_type: database_type.clone(),
        database: database.clone(),
        schema: schema.clone(),
        schema_context: schema_context.clone(),
        connection_context: prompt_connection_context,
    };
    let context_fingerprint = fingerprint(&serde_json::json!({
        "connectionId": connection_id,
        "dbSessionId": db_session_id,
        "databaseType": database_type,
        "database": database,
        "schema": schema,
        "schemaContext": schema_context,
    }));

    QueryActionBuildResult::ok(QueryDiagnosisContext {
        sql,
        safe_sql: safe_sql.clone(),
        safe_error_message: safe_error_message.clone(),
        error_message: safe_error_message.clone(),
        database_type,
        database: database.clone(),
        schema,
        schema_context,
        connection_context,
        prompt_context,
        diagnosis_params: QueryDiagnosisParams {
            db_session_id,
            database,
            sql: safe_sql,
            error_message: safe_error_message,
        },
        context_fingerprint,
    })
}

/// React `resolveContext`: unwrap a context, a build result, or a nullish
/// value into an owned `(context, error)` pair. A nullish value is the
/// `empty-context` error; a failure result carries its own error.
pub(crate) fn resolve_context(
    value: &QueryDiagnosisContextLike,
) -> (Option<QueryDiagnosisContext>, Option<QueryActionError>) {
    match value {
        QueryDiagnosisContextLike::Null => (
            None,
            Some(QueryActionError::new(
                QueryActionErrorCode::EmptyContext,
                "Query action context is empty.",
            )),
        ),
        QueryDiagnosisContextLike::Context(context) => (Some(context.clone()), None),
        QueryDiagnosisContextLike::BuildResult(QueryActionBuildResult::Success(success)) => {
            (Some(success.context.clone()), None)
        }
        QueryDiagnosisContextLike::BuildResult(QueryActionBuildResult::Failure(failure)) => {
            (None, Some(failure.error.clone()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base() -> Value {
        json!({
            "sql": "SELECT * FROM users WHERE id = 1",
            "error": "syntax error near id",
            "connectionId": "conn-1",
            "dbSessionId": "sess-1",
            "databaseType": "postgres",
            "database": "app",
        })
    }

    fn context(value: &Value) -> QueryDiagnosisContext {
        match build_query_diagnosis_context(Some(value)) {
            QueryActionBuildResult::Success(success) => success.context,
            QueryActionBuildResult::Failure(failure) => panic!("{:?}", failure.error),
        }
    }

    #[test]
    fn happy_context_keeps_sql_verbatim_and_redacts_only_the_copy() {
        let mut input = base();
        input["sql"] = json!("SELECT 'password=secret' AS x FROM t");
        let built = context(&input);
        // The editor SQL is byte-for-byte the input.
        assert_eq!(built.sql, "SELECT 'password=secret' AS x FROM t");
        assert_ne!(built.safe_sql, built.sql);
        assert!(built.safe_sql.contains("[REDACTED]"));
        assert_eq!(built.diagnosis_params.sql, built.safe_sql);
        assert_eq!(built.prompt_context.sql, built.safe_sql);
        assert_eq!(built.safe_error_message, built.error_message);
        assert!(built.context_fingerprint.starts_with("qctx-"));
        assert_eq!(built.context_fingerprint.len(), 13);
    }

    #[test]
    fn error_alias_error_message_wins_but_error_is_accepted() {
        let mut alias = base();
        alias["errorMessage"] = json!({ "message": "wrapped error" });
        // `errorMessage ?? error`: the alias wins even when it is an object.
        assert_eq!(context(&alias).safe_error_message, "wrapped error");
        let mut via_error = base();
        via_error["error"] = json!({ "detail": "detail text" });
        assert_eq!(context(&via_error).safe_error_message, "detail text");
        let mut nullish = base();
        nullish["errorMessage"] = Value::Null;
        assert_eq!(context(&nullish).safe_error_message, "syntax error near id");
        // `??` is nullish-only: an empty alias string does NOT fall through
        // to `error` — it normalizes to null and fails with `missing-error`.
        let mut empty_alias = base();
        empty_alias["errorMessage"] = json!("   ");
        assert!(matches!(
            build_query_diagnosis_context(Some(&empty_alias)),
            QueryActionBuildResult::Failure(f)
                if f.error.code == QueryActionErrorCode::MissingError
        ));
    }

    #[test]
    fn connection_context_is_a_safe_allowlist() {
        let mut input = base();
        input["connectionContext"] = json!({
            "connectionId": "ctx-conn",
            "id": "ctx-id",
            "dbSessionId": "ctx-sess",
            "connectionName": "prod",
            "host": "alice:pw@db.local",
            "port": 5432,
            "serverVersion": "16.2",
            "readOnly": true,
            "password": "hunter2",
            "connectionString": "postgres://alice:pw@db.local/app",
            "extraUnknownField": "must be dropped",
        });
        let built = context(&input);
        let ctx = &built.connection_context;
        assert_eq!(ctx.connection_id, "conn-1");
        assert_eq!(ctx.db_session_id, "sess-1");
        assert_eq!(ctx.name.as_deref(), Some("prod"));
        assert_eq!(ctx.host.as_deref(), Some("[REDACTED]@db.local"));
        assert_eq!(ctx.port, Some(5432.0));
        assert_eq!(ctx.server_version.as_deref(), Some("16.2"));
        assert_eq!(ctx.read_only, Some(true));
        // The prompt projection carries neither identifier and no extra keys.
        let prompt = &built.prompt_context.connection_context;
        assert_eq!(prompt.name.as_deref(), Some("prod"));
        assert_eq!(prompt.port, Some(5432.0));
    }

    #[test]
    fn missing_and_unknown_values_take_the_safe_failure_path() {
        assert_eq!(
            build_query_diagnosis_context(None),
            QueryActionBuildResult::fail(
                QueryActionErrorCode::EmptyContext,
                "Query action context is empty."
            )
        );
        assert_eq!(
            build_query_diagnosis_context(Some(&json!([]))),
            QueryActionBuildResult::fail(
                QueryActionErrorCode::EmptyContext,
                "Query action context is empty."
            )
        );
        assert_eq!(
            build_query_diagnosis_context(Some(&json!({}))),
            QueryActionBuildResult::fail(
                QueryActionErrorCode::EmptyContext,
                "Query action context is empty."
            )
        );
        let mut no_sql = base();
        no_sql["sql"] = json!(42);
        assert!(matches!(
            build_query_diagnosis_context(Some(&no_sql)),
            QueryActionBuildResult::Failure(f) if f.error.code == QueryActionErrorCode::MissingSql
        ));
        let mut no_conn = base();
        no_conn.as_object_mut().unwrap().remove("connectionId");
        assert!(matches!(
            build_query_diagnosis_context(Some(&no_conn)),
            QueryActionBuildResult::Failure(f) if f.error.code == QueryActionErrorCode::MissingConnection
        ));
        let mut no_session = base();
        no_session.as_object_mut().unwrap().remove("dbSessionId");
        assert!(matches!(
            build_query_diagnosis_context(Some(&no_session)),
            QueryActionBuildResult::Failure(f) if f.error.code == QueryActionErrorCode::MissingSession
        ));
    }

    #[test]
    fn fingerprint_changes_only_with_context_fields() {
        let first = context(&base());
        let mut sql_changed = base();
        sql_changed["sql"] = json!("SELECT 2");
        assert_eq!(
            context(&sql_changed).context_fingerprint,
            first.context_fingerprint
        );
        let mut db_changed = base();
        db_changed["database"] = json!("other");
        assert_ne!(
            context(&db_changed).context_fingerprint,
            first.context_fingerprint
        );
        // Every field the React fingerprint object includes is pinned: the
        // session, the schema and the sanitized schema context.
        let mut session_changed = base();
        session_changed["dbSessionId"] = json!("sess-2");
        assert_ne!(
            context(&session_changed).context_fingerprint,
            first.context_fingerprint
        );
        let mut schema_changed = base();
        schema_changed["schema"] = json!("public");
        assert_ne!(
            context(&schema_changed).context_fingerprint,
            first.context_fingerprint
        );
        let mut schema_ctx_changed = base();
        schema_ctx_changed["schemaContext"] = json!({"tables": ["users"]});
        assert_ne!(
            context(&schema_ctx_changed).context_fingerprint,
            first.context_fingerprint
        );
    }

    #[test]
    fn resolve_context_covers_all_four_arms() {
        let built = build_query_diagnosis_context(Some(&base()));
        let (some, err) = resolve_context(&QueryDiagnosisContextLike::from(built));
        assert!(some.is_some() && err.is_none());
        let (none, err) = resolve_context(&QueryDiagnosisContextLike::Null);
        assert!(none.is_none());
        assert_eq!(
            err.map(|e| e.code),
            Some(QueryActionErrorCode::EmptyContext)
        );
        let failed = build_query_diagnosis_context(Some(&json!({})));
        let (none, err) = resolve_context(&QueryDiagnosisContextLike::from(failed));
        assert!(none.is_none());
        assert_eq!(
            err.map(|e| e.code),
            Some(QueryActionErrorCode::EmptyContext)
        );
    }
}
