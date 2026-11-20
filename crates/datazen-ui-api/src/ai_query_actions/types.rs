//! Data shapes for the query-error AI action adapters (React `aiQueryActions.ts`).
//!
//! Every React interface here is a plain data bag: the callbacks the
//! descriptors expose live as *methods* on those descriptors (see `actions`),
//! not as stored `Box<dyn Fn>` fields. Keeping the data and the callable edge
//! separate means the whole layer stays `Clone + Debug + PartialEq`, which is
//! what makes the React suite's shape assertions portable.
//!
//! The React file takes `unknown` for every `QueryActionInput` field; the
//! port's raw boundary is therefore [`serde_json::Value`] (the same choice
//! `ai_memory_settings` made for persisted JSON). Typed fields below are the
//! *output* vocabulary. `SanitizedJson` is [`serde_json::Value`]: Rust's owned
//! JSON tree is exactly the seven-case union React declares.

use std::collections::BTreeMap;

use serde_json::Value;

/// React `BoundParamValue = string | number | boolean | null`.
///
/// Objects and arrays are deliberately *not* variants: [`Self::from_json`]
/// drops them, mirroring the React contract that a bound parameter is a
/// scalar. Numbers are `f64` because JS has a single number type; serde_json
/// rejects NaN/Infinity at parse time, so every `Number` we see is finite.
#[derive(Debug, Clone, PartialEq)]
pub enum BoundParamValue {
    String(String),
    Number(f64),
    Bool(bool),
    Null,
}

impl BoundParamValue {
    /// Accept only the React whitelist. Anything else (object, array) is
    /// rejected so it can never reach a retry request.
    pub fn from_json(value: &Value) -> Option<Self> {
        match value {
            Value::String(text) => Some(Self::String(text.clone())),
            Value::Number(number) => number.as_f64().map(Self::Number),
            Value::Bool(flag) => Some(Self::Bool(*flag)),
            Value::Null => Some(Self::Null),
            Value::Array(_) | Value::Object(_) => None,
        }
    }

    /// Rebuild the JSON scalar for canonicalization/comparison.
    pub fn to_json(&self) -> Value {
        match self {
            Self::String(text) => Value::String(text.clone()),
            Self::Number(number) => {
                serde_json::Number::from_f64(*number).map_or(Value::Null, Value::Number)
            }
            Self::Bool(flag) => Value::Bool(*flag),
            Self::Null => Value::Null,
        }
    }
}

/// React `BoundParams = Record<string, BoundParamValue>`. `BTreeMap` keeps
/// iteration deterministic; React's `canonicalize` sorts keys anyway, so the
/// equality/fingerprint results are unchanged.
pub type BoundParams = BTreeMap<String, BoundParamValue>;

/// React `SanitizedJson`. The owned, acyclic [`serde_json::Value`] tree is
/// the exact mirror of React's `null | boolean | number | string | array |
/// object` union; see `sanitize` for the value walker.
pub type SanitizedJson = Value;

/// React `SanitizedConnectionContext`: the safe allowlist retained from a
/// connection. Identifiers are for local routing/fingerprint checks only;
/// the prompt-facing projection is [`QueryPromptContext::connection_context`],
/// which omits them.
#[derive(Debug, Clone, PartialEq)]
pub struct SanitizedConnectionContext {
    pub connection_id: String,
    pub db_session_id: String,
    pub name: Option<String>,
    pub host: Option<String>,
    pub port: Option<f64>,
    pub server_version: Option<String>,
    pub read_only: Option<bool>,
}

/// React `QueryPromptContext`: the prompt-ready projection. It deliberately
/// excludes the two connection/session identifiers.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryPromptContext {
    pub sql: String,
    pub error_message: String,
    pub database_type: String,
    pub database: String,
    pub schema: Option<String>,
    pub schema_context: SanitizedJson,
    pub connection_context: PromptConnectionContext,
}

/// `Omit<SanitizedConnectionContext, 'connectionId' | 'dbSessionId'>`.
#[derive(Debug, Clone, PartialEq)]
pub struct PromptConnectionContext {
    pub name: Option<String>,
    pub host: Option<String>,
    pub port: Option<f64>,
    pub server_version: Option<String>,
    pub read_only: Option<bool>,
}

/// React `QueryDiagnosisParams`: the existing `ai_diagnose_error` input.
/// No new command shape is invented here.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryDiagnosisParams {
    pub db_session_id: String,
    pub database: String,
    /// Redacted SQL safe to hand to an AI command.
    pub sql: String,
    pub error_message: String,
}

/// React `QueryDiagnosisContext`: one bounded, redacted context shared by all
/// three actions. `sql` is the original editor text kept byte-for-byte for
/// Fix SQL / Retry; `safe_sql` is the redacted copy used for AI-bound fields.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryDiagnosisContext {
    pub sql: String,
    pub safe_sql: String,
    pub safe_error_message: String,
    /// Deprecated React alias; kept equal to `safe_error_message`.
    pub error_message: String,
    pub database_type: String,
    pub database: String,
    pub schema: Option<String>,
    pub schema_context: SanitizedJson,
    pub connection_context: SanitizedConnectionContext,
    pub prompt_context: QueryPromptContext,
    pub diagnosis_params: QueryDiagnosisParams,
    pub context_fingerprint: String,
}

/// React `QueryActionErrorCode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryActionErrorCode {
    EmptyContext,
    MissingSql,
    MissingError,
    MissingConnection,
    MissingSession,
    MissingDatabaseType,
    MissingDatabase,
    MissingSuggestedSql,
    ContextChanged,
    SqlChanged,
    ParamsChanged,
}

impl QueryActionErrorCode {
    /// The React string literal, for callers that serialize the error back
    /// to a JS consumer.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EmptyContext => "empty-context",
            Self::MissingSql => "missing-sql",
            Self::MissingError => "missing-error",
            Self::MissingConnection => "missing-connection",
            Self::MissingSession => "missing-session",
            Self::MissingDatabaseType => "missing-database-type",
            Self::MissingDatabase => "missing-database",
            Self::MissingSuggestedSql => "missing-suggested-sql",
            Self::ContextChanged => "context-changed",
            Self::SqlChanged => "sql-changed",
            Self::ParamsChanged => "params-changed",
        }
    }
}

/// React `QueryActionError`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryActionError {
    pub code: QueryActionErrorCode,
    pub message: String,
}

impl QueryActionError {
    pub fn new(code: QueryActionErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// React `QueryActionBuildSuccess<T>`.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryActionBuildSuccess<T> {
    pub context: T,
}

/// React `QueryActionBuildFailure`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryActionBuildFailure {
    pub error: QueryActionError,
}

/// React `QueryActionBuildResult<T>` — the discriminated union `{ ok: true,
/// context } | { ok: false, error }`. The `ok` field is kept as a real field
/// so the payload shape matches React verbatim.
#[derive(Debug, Clone, PartialEq)]
pub enum QueryActionBuildResult<T> {
    Success(QueryActionBuildSuccess<T>),
    Failure(QueryActionBuildFailure),
}

impl<T> QueryActionBuildResult<T> {
    pub fn ok(context: T) -> Self {
        Self::Success(QueryActionBuildSuccess { context })
    }

    pub fn fail(code: QueryActionErrorCode, message: impl Into<String>) -> Self {
        Self::Failure(QueryActionBuildFailure {
            error: QueryActionError::new(code, message),
        })
    }

    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Success(_))
    }
}

/// React `QueryDiagnosisContextLike`: callers may pass a built context, the
/// build result it came from, or a nullish value. `From` impls make every
/// React call site expressible without an explicit conversion.
#[derive(Debug, Clone)]
pub enum QueryDiagnosisContextLike {
    Context(QueryDiagnosisContext),
    BuildResult(QueryActionBuildResult<QueryDiagnosisContext>),
    Null,
}

impl From<QueryDiagnosisContext> for QueryDiagnosisContextLike {
    fn from(context: QueryDiagnosisContext) -> Self {
        Self::Context(context)
    }
}

impl From<QueryActionBuildResult<QueryDiagnosisContext>> for QueryDiagnosisContextLike {
    fn from(result: QueryActionBuildResult<QueryDiagnosisContext>) -> Self {
        Self::BuildResult(result)
    }
}

impl From<Option<QueryDiagnosisContext>> for QueryDiagnosisContextLike {
    fn from(context: Option<QueryDiagnosisContext>) -> Self {
        context.map_or(Self::Null, Self::Context)
    }
}

/// React `SqlDiff`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlDiff {
    pub changed: bool,
    pub removed_lines: Vec<String>,
    pub added_lines: Vec<String>,
    pub diff_text: String,
}

/// React `SqlEditorDraft`. `original_sql` is the failed SQL retained for
/// undo/revert; `sql` and `draft_sql` are the suggested text verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlEditorDraft {
    pub original_sql: String,
    pub sql: String,
    pub draft_sql: String,
    pub diff: SqlDiff,
}

/// React `RetryRequest`.
#[derive(Debug, Clone, PartialEq)]
pub struct RetryRequest {
    pub sql: String,
    pub bound_params: BoundParams,
    pub context_fingerprint: String,
}

/// React `RetryValidationInput`. Both `bound_params` and its QueryPanel
/// alias `params` are accepted; nullish means "empty".
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RetryValidationInput {
    pub sql: String,
    pub context_fingerprint: Option<String>,
    pub bound_params: Option<BoundParams>,
    pub params: Option<BoundParams>,
}

/// React `RetryValidationSuccess`.
#[derive(Debug, Clone, PartialEq)]
pub struct RetryValidationSuccess {
    pub request: RetryRequest,
}

/// React `RetryValidationFailure`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryValidationFailure {
    pub error: QueryActionError,
}

/// React `RetryValidation` discriminated union.
#[derive(Debug, Clone, PartialEq)]
pub enum RetryValidation {
    Success(RetryValidationSuccess),
    Failure(RetryValidationFailure),
}

impl RetryValidation {
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Success(_))
    }

    pub fn error(&self) -> Option<&QueryActionError> {
        match self {
            Self::Success(_) => None,
            Self::Failure(failure) => Some(&failure.error),
        }
    }
}

/// React `ExplainActionDescriptor`. `invoke` is a method (see `actions`), not
/// a stored field; the struct fields are the descriptor's data payload.
#[derive(Debug, Clone, PartialEq)]
pub struct ExplainActionDescriptor {
    pub enabled: bool,
    pub diagnosis_params: Option<QueryDiagnosisParams>,
    pub context_fingerprint: Option<String>,
    pub error: Option<QueryActionError>,
}

/// React `FixSqlActionDescriptor`.
#[derive(Debug, Clone, PartialEq)]
pub struct FixSqlActionDescriptor {
    pub enabled: bool,
    pub draft: Option<SqlEditorDraft>,
    pub context_fingerprint: Option<String>,
    pub error: Option<QueryActionError>,
}

/// React `RetryActionDescriptor`. `validate`/`invoke` are methods (see
/// `actions`); `input` is the alias React exposes for `request`.
#[derive(Debug, Clone, PartialEq)]
pub struct RetryActionDescriptor {
    pub enabled: bool,
    pub request: Option<RetryRequest>,
    /// Alias for adapters that name the request `input`.
    pub input: Option<RetryRequest>,
    pub error: Option<QueryActionError>,
}
