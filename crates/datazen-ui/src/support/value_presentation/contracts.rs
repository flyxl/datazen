//! Port of `packages/extension-points/src/valuePresentation/contracts.ts`
//! (the valuePresentation slice of the extension-points package — not a
//! ledger task of its own; it lands here beside its first in-tree
//! consumer, per the area module's boundary note).
//!
//! The React package exists to sandbox third-party extensions; the Rust
//! host has no extension sandbox, so the contract surface shrinks to
//! exactly what the presentation pipeline consumes: the value/context/
//! environment shapes, the two result kinds, the config-validation
//! contract and the contribution trait itself.
//!
//! # What maps where
//!
//! - `PresentationValue` (null | undefined | string | number | boolean |
//!   bigint | readonly unknown[] | Readonly<Record<...>>) becomes
//!   `&serde_json::Value` — JSON is the transport the values arrive on.
//!   `undefined` collapses into `Null` (both answer the `null` fallback),
//!   and bigint has no JSON form: exact integers beyond 2^53 arrive as
//!   i64/u64 `Number`s and the contribution routes them through the
//!   *string* parse path (React's BigInt rule: decimal text, no f64
//!   approximation).
//! - `PreparedPresentation` / `ValuePresentationContribution` (interfaces
//!   with methods) become object-safe traits; `prepare` returns a boxed
//!   presenter, exactly the React "process-internal, never persisted"
//!   contract.
//! - `ColumnPresentationOverride` carries serde derives with React's
//!   stored spellings (`{"mode":"custom","rendererId":…,"options":…}`) —
//!   the settings store persists these verbatim (T037/T182).

use serde::{Deserialize, Serialize};

/// The presentation semantics of a column
/// (React `PresentationColumnContext['semanticKind']`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SemanticKind {
    Instant,
    LocalDatetime,
    Date,
    Time,
    Number,
    Text,
    Other,
}

/// A value's source location (host-owned identity, no credentials).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ColumnSource<'a> {
    pub(crate) connection_id: &'a str,
    pub(crate) database: Option<&'a str>,
    pub(crate) namespace: Option<&'a str>,
    pub(crate) table: &'a str,
    pub(crate) column: &'a str,
}

/// The column context a plan is prepared for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PresentationColumnContext<'a> {
    pub(crate) column_name: &'a str,
    pub(crate) database_type_name: Option<&'a str>,
    pub(crate) semantic_kind: SemanticKind,
    pub(crate) source: Option<ColumnSource<'a>>,
}

/// The environment snapshot (no implicit machine defaults).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PresentationEnvironment<'a> {
    pub(crate) locale: &'a str,
    pub(crate) local_time_zone: &'a str,
}

/// The presentation result: text output, or an explicit fallback (illegal
/// values fall back; they are not extension errors).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PresentationResult {
    Text {
        text: String,
        interpretation: Option<String>,
    },
    Unchanged {
        reason: Option<String>,
    },
}

impl PresentationResult {
    /// React's `unchanged(reason)` helper.
    pub(crate) fn unchanged(reason: &str) -> Self {
        PresentationResult::Unchanged { reason: Some(reason.to_string()) }
    }
}

/// A pathed config-validation problem (path points into the JSON input).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConfigValidationProblem {
    pub(crate) path: String,
    pub(crate) message: String,
}

/// The validate-config contract: the validated JSON-serializable config on
/// success; every pathed problem on failure — never a panic, never a
/// partial config.
#[derive(Debug, PartialEq)]
pub(crate) enum ConfigValidationResult {
    Ok(serde_json::Value),
    Err(Vec<ConfigValidationProblem>),
}

/// A validated, versioned config snapshot. `config_version` must equal the
/// contribution's declared version.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VersionedPresentationConfig {
    pub(crate) config_version: u32,
    pub(crate) config: serde_json::Value,
}

/// A table-column override (design §5): only the display mode; `custom`'s
/// options are raw JSON the target renderer validates.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub(crate) enum ColumnPresentationOverride {
    Inherit,
    Raw,
    Custom {
        #[serde(rename = "rendererId")]
        renderer_id: String,
        options: serde_json::Value,
    },
}

/// `prepare`'s input: an immutable snapshot the host assembles.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PresentationPrepareInput<'a> {
    pub(crate) config: &'a VersionedPresentationConfig,
    pub(crate) column_override: Option<&'a ColumnPresentationOverride>,
    pub(crate) column: PresentationColumnContext<'a>,
    pub(crate) environment: PresentationEnvironment<'a>,
}

/// The process-internal presenter `prepare` produces (never persisted).
pub(crate) trait PreparedPresentation {
    fn present(&self, value: &serde_json::Value) -> PresentationResult;
}

/// A field-presentation contribution. The id follows
/// `provider.presenter` (e.g. `datazen.timestamp`).
pub(crate) trait ValuePresentationContribution {
    fn id(&self) -> &str;
    fn config_version(&self) -> u32;
    fn validate_config(&self, input: &serde_json::Value) -> ConfigValidationResult;
    fn prepare(&self, input: &PresentationPrepareInput<'_>) -> Box<dyn PreparedPresentation>;
}
