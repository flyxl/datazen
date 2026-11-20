//! The three action descriptors — React `buildExplainAction`,
//! `buildFixSqlAction` and `buildRetryAction` — plus their guards.
//!
//! React stores its callbacks as closure fields on the descriptor. Rust keeps
//! the descriptor a plain data struct and exposes the same call edges as
//! methods with the same signatures and return types:
//!
//! - `ExplainActionDescriptor::invoke(on_explain) -> bool`
//! - `FixSqlActionDescriptor::apply_to_editor(on_apply) -> Option<SqlEditorDraft>`
//! - `RetryActionDescriptor::validate(current) -> RetryValidation`
//! - `RetryActionDescriptor::invoke(current, on_retry) -> RetryValidation`
//!
//! This preserves the "no IPC / no SQL execution in this module" boundary:
//! the caller owns every callback. Building a descriptor never applies SQL,
//! never executes SQL and never clones anything mutable behind the caller's
//! back.

use serde_json::Value;

use super::context::{non_empty_string, resolve_context};
use super::sanitize::canonicalize;
use super::types::{
    BoundParamValue, BoundParams, ExplainActionDescriptor, FixSqlActionDescriptor,
    QueryActionError, QueryActionErrorCode, QueryDiagnosisContextLike, QueryDiagnosisParams,
    RetryActionDescriptor, RetryRequest, RetryValidation, RetryValidationFailure,
    RetryValidationInput, RetryValidationSuccess, SqlDiff, SqlEditorDraft,
};

/// React `buildExplainAction`: reuse the existing diagnosis command input.
pub fn build_explain_action(value: &QueryDiagnosisContextLike) -> ExplainActionDescriptor {
    let (context, error) = resolve_context(value);
    ExplainActionDescriptor {
        enabled: context.is_some(),
        diagnosis_params: context.as_ref().map(|c| c.diagnosis_params.clone()),
        context_fingerprint: context.as_ref().map(|c| c.context_fingerprint.clone()),
        error,
    }
}

impl ExplainActionDescriptor {
    /// React's `invoke(onExplain)`: call the caller-owned diagnosis adapter
    /// only when a context is present; return whether it was called.
    pub fn invoke<F>(&self, on_explain: F) -> bool
    where
        F: FnOnce(QueryDiagnosisParams),
    {
        match &self.diagnosis_params {
            Some(params) => {
                on_explain(params.clone());
                true
            }
            None => false,
        }
    }
}

/// React `String.split(/\r?\n/)`: split on LF, dropping the CR of a CRLF.
fn split_sql_lines(sql: &str) -> Vec<String> {
    sql.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line).to_owned())
        .collect()
}

/// React `buildSqlDiff`: longest common prefix/suffix of the two line lists,
/// then `- removed` / `+ added` lines. No SQL text is rewritten.
fn build_sql_diff(original_sql: &str, suggested_sql: &str) -> SqlDiff {
    if original_sql == suggested_sql {
        return SqlDiff {
            changed: false,
            removed_lines: Vec::new(),
            added_lines: Vec::new(),
            diff_text: String::new(),
        };
    }
    let original = split_sql_lines(original_sql);
    let suggested = split_sql_lines(suggested_sql);
    let mut prefix = 0usize;
    while prefix < original.len()
        && prefix < suggested.len()
        && original[prefix] == suggested[prefix]
    {
        prefix += 1;
    }
    let mut suffix = 0usize;
    while suffix < original.len() - prefix
        && suffix < suggested.len() - prefix
        && original[original.len() - suffix - 1] == suggested[suggested.len() - suffix - 1]
    {
        suffix += 1;
    }
    let removed_lines = original[prefix..original.len() - suffix].to_vec();
    let added_lines = suggested[prefix..suggested.len() - suffix].to_vec();
    let mut pieces: Vec<String> = Vec::with_capacity(removed_lines.len() + added_lines.len());
    pieces.extend(removed_lines.iter().map(|line| format!("- {line}")));
    pieces.extend(added_lines.iter().map(|line| format!("+ {line}")));
    SqlDiff {
        changed: true,
        removed_lines,
        added_lines,
        diff_text: pieces.join("\n"),
    }
}

/// React `suggestedSql` acceptance: a string is used verbatim (including
/// whitespace — the emptiness check happens in the caller), an object uses
/// its trimmed `suggestedSql` alias, everything else is `null`.
fn suggestion_from(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(text)) => Some(text.clone()),
        Some(Value::Object(record)) => non_empty_string(record.get("suggestedSql")),
        _ => None,
    }
}

/// React `buildFixSqlAction`: build a draft without writing to the editor.
pub fn build_fix_sql_action(
    value: &QueryDiagnosisContextLike,
    suggested_sql: Option<&Value>,
) -> FixSqlActionDescriptor {
    let (context, error) = resolve_context(value);
    let suggestion = suggestion_from(suggested_sql);
    let has_suggestion = suggestion
        .as_ref()
        .is_some_and(|text| !text.trim().is_empty());
    let error = error.or_else(|| {
        if has_suggestion {
            None
        } else {
            Some(QueryActionError::new(
                QueryActionErrorCode::MissingSuggestedSql,
                "No suggested SQL was returned.",
            ))
        }
    });
    let draft = match (context.as_ref(), suggestion, has_suggestion) {
        (Some(context), Some(suggestion), true) => Some(SqlEditorDraft {
            original_sql: context.sql.clone(),
            sql: suggestion.clone(),
            draft_sql: suggestion.clone(),
            diff: build_sql_diff(&context.sql, &suggestion),
        }),
        _ => None,
    };

    FixSqlActionDescriptor {
        enabled: draft.is_some(),
        draft,
        context_fingerprint: context.as_ref().map(|c| c.context_fingerprint.clone()),
        error,
    }
}

impl FixSqlActionDescriptor {
    /// React's `applyToEditor(onApply)`: hand a deep copy of the draft to the
    /// caller and return that same copy. Never applies SQL itself.
    pub fn apply_to_editor<F>(&self, on_apply: F) -> Option<SqlEditorDraft>
    where
        F: FnOnce(SqlEditorDraft),
    {
        let draft = self.draft.clone()?;
        let copy = SqlEditorDraft {
            original_sql: draft.original_sql,
            sql: draft.sql,
            draft_sql: draft.draft_sql,
            diff: SqlDiff {
                changed: draft.diff.changed,
                removed_lines: draft.diff.removed_lines.clone(),
                added_lines: draft.diff.added_lines.clone(),
                diff_text: draft.diff.diff_text,
            },
        };
        on_apply(copy.clone());
        Some(copy)
    }
}

/// Build typed retry parameters from raw JSON, dropping non-whitelisted
/// values (objects and arrays) just as the React `BoundParams` contract does.
pub fn bound_params_from_json(value: Option<&Value>) -> BoundParams {
    let mut params = BoundParams::new();
    if let Some(Value::Object(object)) = value {
        for (key, child) in object {
            if let Some(param) = BoundParamValue::from_json(child) {
                params.insert(key.clone(), param);
            }
        }
    }
    params
}

/// React `equalBoundParams`: compare the canonical (key-sorted) form.
pub fn equal_bound_params(left: &BoundParams, right: &BoundParams) -> bool {
    fn canonical(params: &BoundParams) -> String {
        let object: serde_json::Map<String, Value> = params
            .iter()
            .map(|(key, value)| (key.clone(), value.to_json()))
            .collect();
        canonicalize(&Value::Object(object))
    }
    canonical(left) == canonical(right)
}

/// React `buildRetryAction`: no database command is called by this module.
pub fn build_retry_action(
    value: &QueryDiagnosisContextLike,
    bound_params: Option<&BoundParams>,
) -> RetryActionDescriptor {
    let (context, error) = resolve_context(value);
    let safe_bound_params = bound_params.cloned().unwrap_or_default();
    let request = context.map(|context| RetryRequest {
        sql: context.sql.clone(),
        bound_params: safe_bound_params.clone(),
        context_fingerprint: context.context_fingerprint.clone(),
    });

    RetryActionDescriptor {
        enabled: request.is_some(),
        request: request.clone(),
        input: request,
        error,
    }
}

impl RetryActionDescriptor {
    /// React `validate(current)`: the context/session/sql/params guards, in
    /// that order. A missing request falls back to the resolution error.
    pub fn validate(&self, current: &RetryValidationInput) -> RetryValidation {
        let request = match &self.request {
            Some(request) => request,
            None => {
                return RetryValidation::Failure(RetryValidationFailure {
                    error: self.error.clone().unwrap_or_else(|| {
                        QueryActionError::new(
                            QueryActionErrorCode::EmptyContext,
                            "Query action context is empty.",
                        )
                    }),
                })
            }
        };
        if current.context_fingerprint.as_deref() != Some(request.context_fingerprint.as_str()) {
            return RetryValidation::Failure(RetryValidationFailure {
                error: QueryActionError::new(
                    QueryActionErrorCode::ContextChanged,
                    "Database or schema context changed; review the query before retrying.",
                ),
            });
        }
        if current.sql != request.sql {
            return RetryValidation::Failure(RetryValidationFailure {
                error: QueryActionError::new(
                    QueryActionErrorCode::SqlChanged,
                    "SQL changed; retry uses the current SQL only.",
                ),
            });
        }
        let current_params = current
            .bound_params
            .clone()
            .or_else(|| current.params.clone())
            .unwrap_or_default();
        if !equal_bound_params(&current_params, &request.bound_params) {
            return RetryValidation::Failure(RetryValidationFailure {
                error: QueryActionError::new(
                    QueryActionErrorCode::ParamsChanged,
                    "Bound parameters changed; retry was cancelled.",
                ),
            });
        }
        RetryValidation::Success(RetryValidationSuccess {
            request: RetryRequest {
                sql: request.sql.clone(),
                bound_params: request.bound_params.clone(),
                context_fingerprint: request.context_fingerprint.clone(),
            },
        })
    }

    /// React `invoke(current, onRetry)`: validate first, then call the
    /// caller-owned retry adapter with a copy; never call a DB command.
    pub fn invoke<F>(&self, current: &RetryValidationInput, on_retry: F) -> RetryValidation
    where
        F: FnOnce(RetryRequest),
    {
        let result = self.validate(current);
        if let RetryValidation::Success(success) = &result {
            on_retry(success.request.clone());
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_query_actions::build_query_diagnosis_context;
    use serde_json::json;

    fn source() -> QueryDiagnosisContextLike {
        let input = json!({
            "sql": "SELECT * FROM users WHERE id = 1",
            "error": "boom",
            "connectionId": "conn-1",
            "dbSessionId": "sess-1",
            "databaseType": "postgres",
            "database": "app",
        });
        QueryDiagnosisContextLike::from(build_query_diagnosis_context(Some(&input)))
    }

    #[test]
    fn explain_descriptor_reuses_diagnosis_params_and_guards_invoke() {
        let descriptor = build_explain_action(&source());
        assert!(descriptor.enabled);
        assert!(descriptor.error.is_none());
        let params = descriptor.diagnosis_params.as_ref().unwrap();
        assert_eq!(params.db_session_id, "sess-1");
        assert_eq!(params.database, "app");
        let mut called = 0;
        assert!(descriptor.invoke(|_| called += 1));
        assert_eq!(called, 1);

        let empty = build_explain_action(&QueryDiagnosisContextLike::Null);
        assert!(!empty.enabled);
        assert_eq!(
            empty.error.as_ref().map(|e| e.code),
            Some(QueryActionErrorCode::EmptyContext)
        );
        assert!(!empty.invoke(|_| panic!("disabled descriptors must not invoke")));
    }

    #[test]
    fn sql_diff_preserves_original_and_suggested_text_byte_for_byte() {
        let original = "SELECT a\r\nFROM t\nWHERE x = 1";
        let suggested = "SELECT a\r\nFROM t\nWHERE x = 2";
        let input = json!({
            "sql": original,
            "error": "boom",
            "connectionId": "c",
            "dbSessionId": "s",
            "databaseType": "postgres",
            "database": "app",
        });
        let source = QueryDiagnosisContextLike::from(build_query_diagnosis_context(Some(&input)));
        let descriptor = build_fix_sql_action(&source, Some(&Value::String(suggested.to_owned())));
        let draft = descriptor.draft.as_ref().unwrap();
        assert_eq!(draft.original_sql, original);
        assert_eq!(draft.sql, suggested);
        assert_eq!(draft.draft_sql, suggested);
        assert!(draft.diff.changed);
        assert_eq!(draft.diff.removed_lines, vec!["WHERE x = 1"]);
        assert_eq!(draft.diff.added_lines, vec!["WHERE x = 2"]);
        assert_eq!(draft.diff.diff_text, "- WHERE x = 1\n+ WHERE x = 2");

        let unchanged = build_fix_sql_action(&source, Some(&Value::String(original.to_owned())));
        assert!(!unchanged.draft.as_ref().unwrap().diff.changed);
        assert_eq!(unchanged.draft.as_ref().unwrap().diff.diff_text, "");
    }

    #[test]
    fn fix_sql_accepts_the_object_alias_and_falls_back_on_unknown_values() {
        let alias = json!({ "suggestedSql": "  SELECT 2  " });
        let descriptor = build_fix_sql_action(&source(), Some(&alias));
        assert_eq!(descriptor.draft.as_ref().unwrap().sql, "SELECT 2");
        for bad in [Value::Null, json!(42), json!({}), json!([])] {
            let descriptor = build_fix_sql_action(&source(), Some(&bad));
            assert!(!descriptor.enabled);
            assert_eq!(
                descriptor.error.as_ref().map(|e| e.code),
                Some(QueryActionErrorCode::MissingSuggestedSql)
            );
        }
        // A disabled descriptor returns None without calling the callback.
        let empty =
            build_fix_sql_action(&QueryDiagnosisContextLike::Null, Some(&json!("SELECT 2")));
        assert!(!empty.enabled);
        assert!(empty
            .apply_to_editor(|_| panic!("disabled descriptors must not apply"))
            .is_none());
    }

    #[test]
    fn apply_to_editor_hands_a_copy_and_reports_no_execution() {
        let descriptor =
            build_fix_sql_action(&source(), Some(&Value::String("SELECT 2".to_owned())));
        let mut seen = String::new();
        let applied = descriptor
            .apply_to_editor(|draft| seen = draft.sql.clone())
            .unwrap();
        assert_eq!(seen, "SELECT 2");
        assert_eq!(applied.sql, "SELECT 2");
        assert_eq!(applied.original_sql, "SELECT * FROM users WHERE id = 1");
        // Mutating the returned copy cannot change the descriptor.
        assert_eq!(descriptor.draft.as_ref().unwrap().sql, "SELECT 2");
    }

    #[test]
    fn bound_param_types_are_whitelisted_and_compared_canonically() {
        let raw = json!({
            "s": "text",
            "n": 42,
            "f": 1.5,
            "b": true,
            "nil": null,
            "object": { "x": 1 },
            "array": [1, 2],
        });
        let params = bound_params_from_json(Some(&raw));
        assert_eq!(params.len(), 5);
        assert_eq!(params["s"], BoundParamValue::String("text".to_owned()));
        assert_eq!(params["n"], BoundParamValue::Number(42.0));
        assert_eq!(params["f"], BoundParamValue::Number(1.5));
        assert_eq!(params["b"], BoundParamValue::Bool(true));
        assert_eq!(params["nil"], BoundParamValue::Null);
        assert!(!params.contains_key("object"));
        assert!(!params.contains_key("array"));

        let mut reordered = BoundParams::new();
        for (key, value) in params.iter().rev() {
            reordered.insert(key.clone(), value.clone());
        }
        assert!(equal_bound_params(&params, &reordered));
        let mut changed = params.clone();
        changed.insert("n".to_owned(), BoundParamValue::Number(43.0));
        assert!(!equal_bound_params(&params, &changed));
    }

    #[test]
    fn retry_validation_guards_context_sql_and_params_in_order() {
        let params = bound_params_from_json(Some(&json!({"id": 1})));
        let descriptor = build_retry_action(&source(), Some(&params));
        assert!(descriptor.enabled);
        assert_eq!(
            descriptor.request.as_ref().unwrap().sql,
            "SELECT * FROM users WHERE id = 1"
        );
        assert_eq!(descriptor.input, descriptor.request);

        let fingerprint = descriptor
            .request
            .as_ref()
            .unwrap()
            .context_fingerprint
            .clone();
        let valid = RetryValidationInput {
            sql: "SELECT * FROM users WHERE id = 1".to_owned(),
            context_fingerprint: Some(fingerprint.clone()),
            bound_params: Some(params.clone()),
            params: None,
        };
        assert!(descriptor.validate(&valid).is_ok());

        let mut wrong_context = valid.clone();
        wrong_context.context_fingerprint = Some("qctx-deadbeef".to_owned());
        assert_eq!(
            descriptor.validate(&wrong_context).error().map(|e| e.code),
            Some(QueryActionErrorCode::ContextChanged)
        );
        let mut wrong_sql = valid.clone();
        wrong_sql.sql = "SELECT 2".to_owned();
        assert_eq!(
            descriptor.validate(&wrong_sql).error().map(|e| e.code),
            Some(QueryActionErrorCode::SqlChanged)
        );
        let mut wrong_params = valid.clone();
        wrong_params.params = Some(bound_params_from_json(Some(&json!({"id": 2}))));
        wrong_params.bound_params = None;
        assert_eq!(
            descriptor.validate(&wrong_params).error().map(|e| e.code),
            Some(QueryActionErrorCode::ParamsChanged)
        );

        let mut called = 0;
        let result = descriptor.invoke(&valid, |_| called += 1);
        assert!(result.is_ok());
        assert_eq!(called, 1);
        let result = descriptor.invoke(&wrong_sql, |_| called += 1);
        assert!(!result.is_ok());
        assert_eq!(called, 1);
    }

    #[test]
    fn retry_on_a_nullish_context_fails_closed() {
        let descriptor = build_retry_action(&QueryDiagnosisContextLike::Null, None);
        assert!(!descriptor.enabled);
        assert!(descriptor.request.is_none());
        assert_eq!(
            descriptor.error.as_ref().map(|e| e.code),
            Some(QueryActionErrorCode::EmptyContext)
        );
        assert_eq!(
            descriptor
                .validate(&RetryValidationInput::default())
                .error()
                .map(|e| e.code),
            Some(QueryActionErrorCode::EmptyContext)
        );
    }
}
