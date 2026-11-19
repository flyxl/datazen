//! Port of `packages/frontend/src/lib/valuePresentation/settings.ts`
//! (issue-32 T06) — the persisted valuePresentation shape.
//!
//! **Landing in two steps (documented split):** T037 (columnOverrides)
//! needs this file's *shape* — [`StoredValuePresentationSettings`] is the
//! object `with_column_override` applies to — so the types, the default
//! value and their serde spellings land with it. The lenient parse layer
//! (`parse_stored_value_presentation_settings`) and the snapshot builder
//! (`build_value_presentation_snapshot`) are settings.ts's own logic and
//! land with T182, together with the storage-boundary audits that exercise
//! them.
//!
//! ## Stored shape (React, versioned, renderer-id namespaced)
//!
//! ```text
//! StoredValuePresentationSettings = {
//!   version: number,
//!   renderers: Record<string, StoredRendererSettings>,
//!   rules?: ValuePresentationRule[],
//!   columnOverrides?: Record<string, ColumnPresentationOverride>,
//! }
//! StoredRendererSettings = { enabled: boolean, options: TimestampOptions }
//! ```
//!
//! Version adjudication (T08/T10): version stays 1 with optional
//! additive fields; an unknown top-level version keeps its `version`
//! value, is never parsed into rules/columnOverrides and is never written
//! back with defaults (that is the parse layer's business, T182).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::support::value_presentation::contracts::ColumnPresentationOverride;
use crate::support::value_presentation::value_presentation_rules::ValuePresentationRule;

/// One renderer's stored settings: the global switch plus its options in
/// the T02 TimestampOptions JSON shape (kept opaque here; deep validation
/// happens at snapshot-build time per contribution).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct StoredRendererSettings {
    pub(crate) enabled: bool,
    pub(crate) options: serde_json::Value,
}

/// The versioned valuePresentation storage shape (T06 v1 + T08 rules +
/// T10 columnOverrides, all optional additions — no version bump).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct StoredValuePresentationSettings {
    pub(crate) version: u64,
    pub(crate) renderers: BTreeMap<String, StoredRendererSettings>,
    /// Optional; absent in old payloads (consumers treat it as empty).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) rules: Option<Vec<ValuePresentationRule>>,
    /// Optional; keys are [`crate::support::value_presentation::presentation_plan::column_override_key`]
    /// strings, values store raw/custom only (inherit means no entry).
    #[serde(
        rename = "columnOverrides",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) column_overrides: Option<BTreeMap<String, ColumnPresentationOverride>>,
}

/// The v1 default stored value: everything off (a missing fields default
/// to off, design §6). A fresh value every call — no shared default to
/// accidentally mutate.
pub(crate) fn default_value_presentation_settings() -> StoredValuePresentationSettings {
    StoredValuePresentationSettings {
        version: 1,
        renderers: BTreeMap::new(),
        rules: None,
        column_overrides: None,
    }
}
