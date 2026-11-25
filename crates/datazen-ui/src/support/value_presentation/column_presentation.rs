//! Port of `packages/frontend/src/lib/valuePresentation/columnPresentation.ts`
//! (issue-32 T09, design §5/§9, PRD §3.2) — the column-configuration
//! panel's pure logic.
//!
//! Three jobs, no state:
//!
//! 1. **Override validation** ([`validate_column_presentation_override`]):
//!    `inherit`/`raw` take no extra fields; `custom` only accepts the
//!    timestamp renderer `datazen.timestamp` (any other rendererId is
//!    refused rather than storing a config this panel cannot verify) and
//!    its options go through the full T02 validation — illegal options
//!    never save.
//! 2. **Preview** ([`compute_column_presentation_preview`]): the same road
//!    the runtime takes. `inherit` builds the plan with the column's
//!    still-stored override stripped (T09-BUG-001: an inherit draft
//!    previews the cleared state, so a pending-removal custom override
//!    does not shadow the rules); `raw` answers the raw fallback; `custom`
//!    goes validate → prepare → present. No source guessing for
//!    sourceless columns.
//! 3. **Sample values** ([`fallback_sample_for_column`]): the PRD §3.2
//!    anchors by semantic kind (Unix ms 1705316400000, native
//!    2026-01-15T14:30:00Z, wall text 2026-01-15 14:30:00, date, time) —
//!    the caller's real row value wins whenever it has one.
//!
//! # What maps where
//!
//! - `Object.freeze` is elided (ownership); the canonical custom override
//!   serializes its validated options back to the stored JSON shape
//!   (byte-identical to React's canonical object).
//! - `snapshotWithoutColumnOverride` returns an owned clone with the key
//!   removed (React returns the same object when nothing changes — an
//!   identity optimization ownership makes moot); the input snapshot is
//!   never mutated, which is the part the T09-BUG-001 regression pins.
//! - React's global-registry default parameter becomes an explicit
//!   argument.

use std::collections::BTreeMap;

use crate::support::value_presentation::contracts::{
    ColumnPresentationOverride, ColumnSource, ConfigValidationResult,
    PresentationColumnContext, PresentationEnvironment, PresentationPrepareInput,
    PresentationResult, PreparedPresentation, SemanticKind, VersionedPresentationConfig,
};
use crate::support::value_presentation::registry::ValuePresentationRegistry;
use crate::support::value_presentation::presentation_plan::{
    BuildColumnPresentationPlansInput, PresentationPlanColumn, ValuePresentationSnapshot,
    build_column_presentation_plans, build_presentation_column_context, column_override_key,
    resolve_presentation_semantic_kind,
};
use crate::support::value_presentation::timestamp_contribution::TIMESTAMP_CONTRIBUTION_ID;
use crate::support::value_presentation::timestamp_config::{
    TimestampOptions, TimestampOptionsValidation, validate_timestamp_options,
};

/// Pathed validation problem (same shape as T02's).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnPresentationOverrideProblem {
    pub path: String,
    pub message: String,
}

/// `validate_column_presentation_override`'s result.
#[derive(Debug, PartialEq)]
pub enum ColumnPresentationOverrideValidation {
    Ok(ColumnPresentationOverride),
    Err(Vec<ColumnPresentationOverrideProblem>),
}

/// The source-identity half of a [`ColumnSource`] (the column name is
/// supplied per call).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColumnSourceBase<'a> {
    pub connection_id: &'a str,
    pub database: Option<&'a str>,
    pub namespace: Option<&'a str>,
    pub table: &'a str,
}

impl<'b> ColumnSourceBase<'b> {
    pub fn with_column<'a>(&self, column: &'a str) -> ColumnSource<'a>
    where
        'b: 'a,
    {
        ColumnSource {
            connection_id: self.connection_id,
            database: self.database,
            namespace: self.namespace,
            table: self.table,
            column,
        }
    }
}

/// Complete structural validation of unknown JSON input. Success returns
/// the canonical override (validated fields only — custom options come
/// back normalized, never the raw unknown); failure returns every pathed
/// problem.
pub fn validate_column_presentation_override(
    input: &serde_json::Value,
) -> ColumnPresentationOverrideValidation {
    let Some(object) = input.as_object() else {
        return ColumnPresentationOverrideValidation::Err(vec![
            ColumnPresentationOverrideProblem {
                path: String::new(),
                message: "expected an object".to_string(),
            },
        ]);
    };
    let problems: &mut Vec<ColumnPresentationOverrideProblem> = &mut Vec::new();
    let mode = object.get("mode").and_then(serde_json::Value::as_str);

    if mode == Some("inherit") || mode == Some("raw") {
        for key in object.keys() {
            if key != "mode" {
                problems.push(ColumnPresentationOverrideProblem {
                    path: key.clone(),
                    message: "unknown field".to_string(),
                });
            }
        }
        if !problems.is_empty() {
            return ColumnPresentationOverrideValidation::Err(std::mem::take(problems));
        }
        return ColumnPresentationOverrideValidation::Ok(match mode {
            Some("raw") => ColumnPresentationOverride::Raw,
            _ => ColumnPresentationOverride::Inherit,
        });
    }
    if mode != Some("custom") {
        return ColumnPresentationOverrideValidation::Err(vec![
            ColumnPresentationOverrideProblem {
                path: "mode".to_string(),
                message: "expected \"inherit\", \"raw\" or \"custom\"".to_string(),
            },
        ]);
    }

    let mut renderer_id: Option<String> = None;
    match object.get("rendererId") {
        Some(serde_json::Value::String(value)) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                problems.push(ColumnPresentationOverrideProblem {
                    path: "rendererId".to_string(),
                    message: "must not be empty".to_string(),
                });
            } else if trimmed.chars().any(char::is_whitespace) {
                problems.push(ColumnPresentationOverrideProblem {
                    path: "rendererId".to_string(),
                    message: "must not contain whitespace".to_string(),
                });
            } else {
                renderer_id = Some(trimmed.to_string());
            }
        }
        _ => problems.push(ColumnPresentationOverrideProblem {
            path: "rendererId".to_string(),
            message: "expected a string".to_string(),
        }),
    }

    let mut options: Option<TimestampOptions> = None;
    match &renderer_id {
        Some(renderer) if renderer != TIMESTAMP_CONTRIBUTION_ID => {
            problems.push(ColumnPresentationOverrideProblem {
                path: "rendererId".to_string(),
                message: "unknown renderer".to_string(),
            });
        }
        Some(_) => match validate_timestamp_options(
            object.get("options").unwrap_or(&serde_json::Value::Null),
        ) {
            TimestampOptionsValidation::Ok(validated) => options = Some(validated),
            TimestampOptionsValidation::Err(option_problems) => {
                for problem in option_problems {
                    problems.push(ColumnPresentationOverrideProblem {
                        path: format!("options.{}", problem.path),
                        message: problem.message,
                    });
                }
            }
        },
        None => {}
    }

    for key in object.keys() {
        if !matches!(key.as_str(), "mode" | "rendererId" | "options") {
            problems.push(ColumnPresentationOverrideProblem {
                path: key.clone(),
                message: "unknown field".to_string(),
            });
        }
    }

    if !problems.is_empty() || renderer_id.is_none() || options.is_none() {
        return ColumnPresentationOverrideValidation::Err(std::mem::take(problems));
    }

    // The canonical override stores the validated options in their stored
    // JSON shape (byte-identical to React's canonical object).
    let canonical_options = serde_json::to_value(options.expect("checked above"))
        .expect("validated options serialize");
    ColumnPresentationOverrideValidation::Ok(ColumnPresentationOverride::Custom {
        renderer_id: renderer_id.expect("checked above"),
        options: canonical_options,
    })
}

/// The column's current override from the snapshot: `inherit` when there
/// is no source (query results) or no entry. The source identity is the
/// caller's; no guessing table sources from SQL aliases (PRD §3.2).
pub fn column_override_from_snapshot(
    snapshot: &ValuePresentationSnapshot,
    source: Option<ColumnSourceBase<'_>>,
    column_name: &str,
) -> ColumnPresentationOverride {
    let Some(source) = source else {
        return ColumnPresentationOverride::Inherit;
    };
    snapshot
        .overrides
        .get(&column_override_key(&source.with_column(column_name)))
        .cloned()
        .unwrap_or(ColumnPresentationOverride::Inherit)
}

/// A copy of the snapshot with this column's override removed
/// (T09-BUG-001): an inherit draft previews against the cleared state.
/// The input snapshot is never mutated.
pub fn snapshot_without_column_override(
    snapshot: &ValuePresentationSnapshot,
    source: Option<ColumnSourceBase<'_>>,
    column_name: &str,
) -> ValuePresentationSnapshot {
    let Some(source) = source else {
        return snapshot.clone();
    };
    let key = column_override_key(&source.with_column(column_name));
    if !snapshot.overrides.contains_key(&key) {
        return snapshot.clone();
    }
    let mut overrides: BTreeMap<String, ColumnPresentationOverride> =
        snapshot.overrides.clone();
    overrides.remove(&key);
    ValuePresentationSnapshot { overrides, ..snapshot.clone() }
}

/// The PRD §3.2 anchor samples by semantic kind (only when the caller has
/// no real row value): number → Unix milliseconds anchor, instant → the
/// zoned anchor, local-datetime → wall text, date / time → their shapes;
/// text / other have no temporal sample.
pub fn fallback_sample_for_column(
    column: &PresentationPlanColumn<'_>,
) -> Option<serde_json::Value> {
    match resolve_presentation_semantic_kind(column) {
        SemanticKind::Number => Some(serde_json::json!(1705316400000i64)),
        SemanticKind::Instant => Some(serde_json::json!("2026-01-15T14:30:00Z")),
        SemanticKind::LocalDatetime => Some(serde_json::json!("2026-01-15 14:30:00")),
        SemanticKind::Date => Some(serde_json::json!("2026-01-15")),
        SemanticKind::Time => Some(serde_json::json!("14:30:00")),
        SemanticKind::Text | SemanticKind::Other => None,
    }
}

/// A sample value's display text: null stays NULL; arrays and objects
/// render as compact JSON; scalars as their own text.
pub fn format_column_presentation_sample_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "NULL".to_string(),
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Bool(flag) => flag.to_string(),
        number @ serde_json::Value::Number(_) => number.to_string(),
        other => serde_json::to_string(other).unwrap_or_else(|_| other.to_string()),
    }
}

/// The panel preview result (the T07 preview shape).
#[derive(Debug, PartialEq)]
pub enum ColumnPresentationPreview {
    Unavailable { sample: serde_json::Value },
    Invalid { problems: Vec<ColumnPresentationOverrideProblem>, sample: serde_json::Value },
    Text { text: String, interpretation: Option<String>, sample: serde_json::Value },
    Unchanged { reason: Option<String>, sample: serde_json::Value },
}

fn map_presentation_result(
    result: PresentationResult,
    sample: serde_json::Value,
) -> ColumnPresentationPreview {
    match result {
        PresentationResult::Text { text, interpretation } => {
            ColumnPresentationPreview::Text { text, interpretation, sample }
        }
        PresentationResult::Unchanged { reason } => {
            ColumnPresentationPreview::Unchanged { reason, sample }
        }
    }
}

/// `compute_column_presentation_preview`'s input.
pub struct ComputeColumnPresentationPreviewInput<'a> {
    /// The candidate override (the panel draft); `inherit` previews the
    /// currently-effective plan with the stored override stripped.
    pub candidate: &'a ColumnPresentationOverride,
    pub column: PresentationPlanColumn<'a>,
    /// The table identity; absent for query-result columns.
    pub source: Option<ColumnSourceBase<'a>>,
    pub snapshot: &'a ValuePresentationSnapshot,
    pub environment: PresentationEnvironment<'a>,
    pub sample_value: serde_json::Value,
    pub registry: &'a ValuePresentationRegistry,
}

/// The column-configuration preview — the same road the runtime takes:
/// inherit goes through the plan builder (with the stored override
/// stripped), raw answers the raw fallback, custom goes
/// validateConfig → prepare → present through the registry.
pub fn compute_column_presentation_preview(
    input: &ComputeColumnPresentationPreviewInput<'_>,
) -> ColumnPresentationPreview {
    let sample = input.sample_value.clone();
    match input.candidate {
        ColumnPresentationOverride::Inherit => {
            let stripped = snapshot_without_column_override(
                input.snapshot,
                input.source,
                input.column.name,
            );
            let plans = build_column_presentation_plans(
                &BuildColumnPresentationPlansInput {
                    columns: vec![input.column],
                    source: input.source.map(|source| source.with_column(input.column.name)),
                    snapshot: &stripped,
                },
                input.registry,
            );
            let Some(plan) = plans.into_iter().next() else {
                return ColumnPresentationPreview::Unchanged {
                    reason: Some("no-plan".to_string()),
                    sample,
                };
            };
            map_presentation_result(plan.present(&sample), sample)
        }
        ColumnPresentationOverride::Raw => ColumnPresentationPreview::Unchanged {
            reason: Some("raw-override".to_string()),
            sample,
        },
        ColumnPresentationOverride::Custom { renderer_id, options } => {
            let Some(contribution) = input.registry.get(renderer_id) else {
                return ColumnPresentationPreview::Unavailable { sample };
            };
            let validated = contribution.validate_config(options);
            let config = match validated {
                ConfigValidationResult::Ok(config) => config,
                ConfigValidationResult::Err(problems) => {
                    return ColumnPresentationPreview::Invalid {
                        problems: problems
                            .into_iter()
                            .map(|problem| ColumnPresentationOverrideProblem {
                                path: problem.path,
                                message: problem.message,
                            })
                            .collect(),
                        sample,
                    };
                }
            };
            let column_context: PresentationColumnContext<'_> =
                build_presentation_column_context(
                    &input.column,
                    input.source.map(|source| source.with_column(input.column.name)),
                );
            let versioned = VersionedPresentationConfig {
                config_version: contribution.config_version(),
                config,
            };
            let prepared: Box<dyn PreparedPresentation> = contribution.prepare(
                &PresentationPrepareInput {
                    config: &versioned,
                    column_override: Some(input.candidate),
                    column: column_context,
                    environment: input.environment,
                },
            );
            map_presentation_result(prepared.present(&sample), sample)
        }
    }
}
