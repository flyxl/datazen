//! Pure query-error AI action adapters (React `lib/aiQueryActions.ts`, T272).
//!
//! React's 847-line file is a deliberate "no store, no IPC" layer: every
//! exported function turns a raw context into a descriptor whose callbacks
//! are owned by the caller. It is ported here as four high-cohesion files so
//! no source file crosses the 800-line ceiling:
//!
//! - [`types`] — the React interfaces, as `Clone + Debug + PartialEq` data
//!   shapes. `SanitizedJson` is `serde_json::Value`; `BoundParams` is a
//!   `BTreeMap` of the four allowed scalar kinds.
//! - [`sanitize`] — secret/result-row redaction. Regexes are hand-scanned so
//!   no `regex` dependency is introduced. It never rewrites SQL text.
//! - [`context`] — `buildQueryDiagnosisContext` and the alias readers that
//!   make `error`/`errorMessage`, `database`/`currentDatabase`, etc. agree.
//! - [`actions`] — the three descriptors and their guarded call edges.
//!
//! ## Safety invariants preserved
//!
//! - The original editor SQL is kept byte-for-byte in
//!   [`QueryDiagnosisContext::sql`], which is what Fix SQL's `original_sql`
//!   and Retry's `request.sql` consume. Only the separate `safe_sql` copy is
//!   redacted and handed to diagnosis commands.
//! - Connection context is an allowlist; schema context drops result-shaped
//!   keys (`rows`, `results`, …), secret-shaped keys, URI credentials and
//!   `Bearer` tokens before any text reaches a prompt.
//! - Unknown input never panics: non-objects and empty objects are
//!   `empty-context`, wrong-typed fields are `missing-*`, callbacks on a
//!   disabled descriptor are no-ops. There is no bare `unwrap`/`expect` in
//!   the production path.

mod actions;
mod context;
mod sanitize;
mod types;

pub use actions::{
    bound_params_from_json, build_explain_action, build_fix_sql_action, build_retry_action,
    equal_bound_params,
};
pub use context::build_query_diagnosis_context;
pub use types::{
    BoundParamValue, BoundParams, ExplainActionDescriptor, FixSqlActionDescriptor,
    PromptConnectionContext, QueryActionBuildFailure, QueryActionBuildResult,
    QueryActionBuildSuccess, QueryActionError, QueryActionErrorCode, QueryDiagnosisContext,
    QueryDiagnosisContextLike, QueryDiagnosisParams, QueryPromptContext, RetryActionDescriptor,
    RetryRequest, RetryValidation, RetryValidationFailure, RetryValidationInput,
    RetryValidationSuccess, SanitizedConnectionContext, SanitizedJson, SqlDiff, SqlEditorDraft,
};

#[cfg(test)]
mod tests {
    use crate::{
        bound_params_from_json, build_explain_action, build_fix_sql_action,
        build_query_diagnosis_context, build_retry_action, equal_bound_params, BoundParamValue,
        QueryActionBuildResult, QueryActionErrorCode, QueryDiagnosisContextLike,
    };
    use serde_json::{json, Value};

    fn valid() -> Value {
        json!({
            "sql": "SELECT 1",
            "errorMessage": "boom",
            "connectionId": "c",
            "dbSessionId": "s",
            "driverType": "sqlite",
            "currentDatabase": "main",
            "schemaContext": { "rows": [{"id": 1}], "tables": ["users"] },
        })
    }

    /// The crate-root surface is a shipped export; pin every name against
    /// removal the same way `ai_providers` pins its six exports.
    #[test]
    fn react_surface_is_reexported_at_the_crate_root() {
        let built = build_query_diagnosis_context(Some(&valid()));
        let source = QueryDiagnosisContextLike::from(built);
        assert!(build_explain_action(&source).enabled);
        assert!(build_fix_sql_action(&source, Some(&Value::String("SELECT 2".into()))).enabled);
        let params = bound_params_from_json(Some(&json!({"id": 1})));
        assert_eq!(params["id"], BoundParamValue::Number(1.0));
        assert!(equal_bound_params(&params, &params.clone()));
        assert!(build_retry_action(&source, Some(&params)).enabled);
        assert_eq!(QueryActionErrorCode::EmptyContext.as_str(), "empty-context");
        assert!(matches!(
            build_query_diagnosis_context(None),
            QueryActionBuildResult::Failure(_)
        ));
    }
}
