//! Split half of the `ValuePresentationSettingsSection` port
//! (T474) — the pure draft / preview family the React file exports
//! from the section itself (kept next to it the way
//! `transfer_job_run_helpers.rs` sits beside T438's machine).
//!
//! - The draft half ([`DEFAULT_TIMESTAMP_OPTIONS`],
//!   [`TimestampDisplayDraft`], [`draft_from_stored`]): the panel
//!   reads the `datazen.timestamp` entry out of the stored value —
//!   a missing entry is the default-off face, an illegal options
//!   value keeps the switch and falls the options back to the
//!   default (an illegal configuration never enters the draft; the
//!   save writes a legal one back).
//! - The preview half ([`preview_sample_for`],
//!   [`build_preview_options`], [`compute_timestamp_preview`]):
//!   the same road as the runtime — the contribution's
//!   validateConfig → prepare → present over the PRD §4
//!   double-anchor samples. The preview's time-source choice never
//!   participates in the save; a unix preview carries no source
//!   zone (the validator forbids it), a text preview borrows the
//!   draft's source zone defaulting to UTC (preview-only).
//!
//! Known deltas: `PresentationValue` is `serde_json::Value` here,
//! so the unix samples ride as JSON numbers (React's `bigint`
//! transport is a driver-side concern the serde host cannot
//! carry).

// Carried by the T474 section; this crate's tests are the callers
// meanwhile.
#![allow(dead_code)]

use serde_json::{json, Value};

use crate::support::value_presentation::contracts::{
    ConfigValidationResult, PresentationColumnContext, PresentationEnvironment,
    PresentationPrepareInput, PresentationResult, SemanticKind, ValuePresentationContribution,
    VersionedPresentationConfig,
};
use crate::support::value_presentation::settings::StoredValuePresentationSettings;
use crate::support::value_presentation::timestamp_config::{
    validate_timestamp_options, FormatSerde, TimestampInterpretation, TimestampOptions,
    TimestampOptionsValidation, UnixUnitSerde,
};
use crate::support::value_presentation::timestamp_contribution::TIMESTAMP_CONTRIBUTION_ID;
use crate::support::value_presentation::timestamp_format::TimeZoneSpec;
use crate::views::settings::value_presentation_rule_editor::InterpretationChoice;

/// The global default time options' initial value: native
/// interpretation, UTC-dispatched ISO, following the local zone.
pub(crate) const DEFAULT_TIMESTAMP_OPTIONS: TimestampOptions = TimestampOptions {
    interpretation: TimestampInterpretation::Native,
    format: FormatSerde::Iso,
    display_time_zone: TimeZoneSpec::Local,
    source_time_zone: None,
};

/// The panel draft: the switch + the global default options (the
/// interpretation stays native — see the section header's ruling 2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TimestampDisplayDraft {
    pub(crate) enabled: bool,
    pub(crate) options: TimestampOptions,
}

/// `draftFromStored(stored)`: no entry → the default-off face; an
/// entry with illegal options keeps the switch and falls the
/// options back to the default.
pub(crate) fn draft_from_stored(stored: &StoredValuePresentationSettings) -> TimestampDisplayDraft {
    let Some(entry) = stored.renderers.get(TIMESTAMP_CONTRIBUTION_ID) else {
        return TimestampDisplayDraft { enabled: false, options: DEFAULT_TIMESTAMP_OPTIONS };
    };
    TimestampDisplayDraft {
        enabled: entry.enabled,
        options: match validate_timestamp_options(&entry.options) {
            TimestampOptionsValidation::Ok(options) => options,
            TimestampOptionsValidation::Err(_) => DEFAULT_TIMESTAMP_OPTIONS,
        },
    }
}

/// One PRD §4 double-anchor sample.
pub(crate) struct PreviewSample {
    pub(crate) value: Value,
    pub(crate) semantic_kind: SemanticKind,
}

/// `previewSampleFor(preview)`.
pub(crate) fn preview_sample_for(preview: InterpretationChoice) -> PreviewSample {
    match preview {
        InterpretationChoice::Native => PreviewSample {
            value: json!("2026-01-15T14:30:00Z"),
            semantic_kind: SemanticKind::Instant,
        },
        InterpretationChoice::UnixSeconds => PreviewSample {
            value: json!(1705316400u64),
            semantic_kind: SemanticKind::Number,
        },
        InterpretationChoice::UnixMilliseconds => PreviewSample {
            value: json!(1705316400000u64),
            semantic_kind: SemanticKind::Number,
        },
        InterpretationChoice::Text => PreviewSample {
            value: json!("2026-01-15 14:30:00"),
            semantic_kind: SemanticKind::LocalDatetime,
        },
    }
}

/// `buildPreviewOptions(draft, preview)`: only the interpretation
/// is replaced — a unix preview carries no source zone (the
/// validator forbids it), a text preview borrows the draft's source
/// zone defaulting to UTC (preview-only, never saved).
pub(crate) fn build_preview_options(
    draft: &TimestampOptions,
    preview: InterpretationChoice,
) -> TimestampOptions {
    let format = draft.format;
    let display_time_zone = draft.display_time_zone.clone();
    match preview {
        InterpretationChoice::Native => TimestampOptions {
            interpretation: TimestampInterpretation::Native,
            format,
            display_time_zone,
            source_time_zone: draft.source_time_zone.clone(),
        },
        InterpretationChoice::UnixSeconds => TimestampOptions {
            interpretation: TimestampInterpretation::Unix { unit: UnixUnitSerde::Seconds },
            format,
            display_time_zone,
            source_time_zone: None,
        },
        InterpretationChoice::UnixMilliseconds => TimestampOptions {
            interpretation: TimestampInterpretation::Unix { unit: UnixUnitSerde::Milliseconds },
            format,
            display_time_zone,
            source_time_zone: None,
        },
        InterpretationChoice::Text => TimestampOptions {
            interpretation: TimestampInterpretation::Text,
            format,
            display_time_zone,
            source_time_zone: Some(
                draft.source_time_zone.clone().unwrap_or(TimeZoneSpec::Utc),
            ),
        },
    }
}

/// `TimestampPreviewResult`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TimestampPreviewResult {
    /// The contribution is not registered.
    Unavailable { sample: Value },
    /// The configuration is illegal (the pathed problems).
    Invalid { problems: Vec<(String, String)>, sample: Value },
    /// The presented text.
    Text { text: String, interpretation: Option<String>, sample: Value },
    /// The honest fallback (with the reason).
    Unchanged { reason: Option<String>, sample: Value },
}

/// `computeTimestampPreview` — the same road as the runtime: the
/// contribution's validateConfig → prepare → present.
pub(crate) fn compute_timestamp_preview(
    contribution: Option<&std::rc::Rc<dyn ValuePresentationContribution>>,
    options: &TimestampOptions,
    preview: InterpretationChoice,
    environment: PresentationEnvironment<'_>,
) -> TimestampPreviewResult {
    let sample = preview_sample_for(preview);
    let Some(contribution) = contribution else {
        return TimestampPreviewResult::Unavailable { sample: sample.value };
    };
    let options_value = serde_json::to_value(options).unwrap_or_else(|_| Value::Null);
    match contribution.validate_config(&options_value) {
        ConfigValidationResult::Err(problems) => TimestampPreviewResult::Invalid {
            problems: problems
                .into_iter()
                .map(|problem| (problem.path, problem.message))
                .collect(),
            sample: sample.value,
        },
        ConfigValidationResult::Ok(config) => {
            let versioned = VersionedPresentationConfig {
                config_version: contribution.config_version(),
                config,
            };
            let prepared = contribution.prepare(&PresentationPrepareInput {
                config: &versioned,
                column_override: None,
                column: PresentationColumnContext {
                    column_name: "__preview__",
                    database_type_name: None,
                    semantic_kind: sample.semantic_kind,
                    source: None,
                },
                environment,
            });
            let result = prepared.present(&sample.value);
            match result {
                PresentationResult::Text { text, interpretation } => {
                    TimestampPreviewResult::Text { text, interpretation, sample: sample.value }
                }
                PresentationResult::Unchanged { reason } => {
                    TimestampPreviewResult::Unchanged { reason, sample: sample.value }
                }
            }
        }
    }
}

/// `formatPreviewSample(value)`.
pub(crate) fn format_preview_sample(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        return text.to_owned();
    }
    if let Some(number) = value.as_u64() {
        return number.to_string();
    }
    if let Some(number) = value.as_f64() {
        return number.to_string();
    }
    String::new()
}
