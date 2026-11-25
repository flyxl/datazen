//! Port of `packages/frontend/src/lib/valuePresentation/timestampContribution.ts`
//! (issue-32 T03).
//!
//! The first presentation contribution, `datazen.timestamp`: assembles the
//! T02/T08 pure core (temporalMath / timestampConfig / timestampFormat)
//! into a [`ValuePresentationContribution`] the host pipeline consumes at
//! plan-build time via `prepare`.
//!
//! # What maps where (the module's six adjudications, kept verbatim)
//!
//! 1. Resolution order: configVersion → global-config validity → column
//!    override. An invalid global config is always `invalid-config` (even
//!    with a `raw` override); a version mismatch is always
//!    `unsupported-config-version` (the config body is not inspected).
//! 2. `local` time zones resolve **in prepare** from the environment
//!    snapshot (`environment.localTimeZone`), never from the runtime
//!    machine zone (design §8.7); the locale likewise comes from
//!    `environment.locale`.
//! 3. `time`-only text anchors to 1970-01-01 (the epoch day); `date`-only
//!    text takes that day's midnight. Never "today" — no clock dependence.
//! 4. Native-fallback reason codes pass the underlying T02 codes through
//!    verbatim; the present layer only adds its own codes
//!    (`raw-override`, `override-target-mismatch`,
//!    `missing-source-time-zone`, `not-applicable`, `null`).
//! 5. BigInt values go through `String(value)` into the exact string
//!    parse. That route lives on the driver transport; the Rust host's
//!    `PresentationValue` is `serde_json::Value`, whose numbers are f64
//!    at the JSON boundary — so non-safe integers answer
//!    `not-safe-integer` exactly like a JSON-transported React number
//!    would, and the (transport-only) BigInt route is documented as
//!    unreachable here.
//! 6. The semantic-kind gate runs before value interpretation: `other`
//!    columns and non-unix interpretations on `number` columns fall back
//!    *before* the null check (null on an `other` column is
//!    `not-applicable`, not `null`).
//!
//! # Known deltas (type-level)
//!
//! React's try/catch defenses (`present` never throws; `prepare` falls
//!    back on malformed snapshots) have nothing to catch here — the trait
//!    input is well-typed and the pipeline returns results instead of
//!    panicking. The `unexpected-error` code is therefore unreachable in
//!    the Rust port (kept in the React test mapping as documentation).
//!    JSON-unrepresentable values (NaN, Infinity, Symbol, functions) from
//!    the never-throws battery are likewise unconstructible.

use std::rc::Rc;

use serde_json::json;

use crate::support::value_presentation::contracts::{
    ColumnPresentationOverride, ConfigValidationProblem, ConfigValidationResult,
    PresentationEnvironment, PresentationPrepareInput, PresentationResult,
    PreparedPresentation, SemanticKind, ValuePresentationContribution,
    VersionedPresentationConfig,
};
use crate::support::value_presentation::temporal_math::{
    InstantParts, TemporalParseError, UnixUnit, WallClockTextParse, WallComponents,
    WallToInstantError, parse_unix_value, parse_wall_clock_text, parse_zoned_timestamp,
    wall_clock_to_instant,
};
use crate::support::value_presentation::timestamp_config::{
    FormatSerde, TimestampInterpretation, TimestampOptions, TimestampOptionsValidation,
    UnixUnitSerde, validate_timestamp_options,
};
use crate::support::value_presentation::temporal_math::TemporalFormatError;
use crate::support::value_presentation::timestamp_format::{
    TimeZoneSpec, format_timestamp_output,
};

/// The contribution's unique id (design §2: provider.presenter).
pub const TIMESTAMP_CONTRIBUTION_ID: &str = "datazen.timestamp";

/// The config schema version (v1: the TimestampOptions JSON shape).
pub const TIMESTAMP_CONTRIBUTION_CONFIG_VERSION: u32 = 1;

/// Rebuild validated options as a JSON config value (T08 export; the plan
/// layer passes a matched rule's options to prepare as the effective
/// config). Only validated fields appear; `sourceTimeZone` is absent when
/// unset — validateTimestampOptions' canonical shape.
pub fn timestamp_options_to_config_value(options: &TimestampOptions) -> serde_json::Value {
    let interpretation = match &options.interpretation {
        TimestampInterpretation::Unix { unit } => json!({
            "kind": "unix",
            "unit": match unit {
                UnixUnitSerde::Seconds => "seconds",
                UnixUnitSerde::Milliseconds => "milliseconds",
            },
        }),
        TimestampInterpretation::Native => json!({ "kind": "native" }),
        TimestampInterpretation::Text => json!({ "kind": "text" }),
    };
    let zone = |spec: &TimeZoneSpec| match spec {
        TimeZoneSpec::Iana { name } => json!({ "kind": "iana", "name": name }),
        TimeZoneSpec::Utc => json!({ "kind": "utc" }),
        TimeZoneSpec::Local => json!({ "kind": "local" }),
    };
    let format = match options.format {
        FormatSerde::Iso => "iso",
        FormatSerde::IsoTz => "iso-tz",
        FormatSerde::Locale => "locale",
    };
    match &options.source_time_zone {
        None => json!({
            "interpretation": interpretation,
            "format": format,
            "displayTimeZone": zone(&options.display_time_zone),
        }),
        Some(source) => json!({
            "interpretation": interpretation,
            "format": format,
            "displayTimeZone": zone(&options.display_time_zone),
            "sourceTimeZone": zone(source),
        }),
    }
}

fn validate_timestamp_config(input: &serde_json::Value) -> ConfigValidationResult {
    match validate_timestamp_options(input) {
        TimestampOptionsValidation::Err(problems) => ConfigValidationResult::Err(
            problems
                .into_iter()
                .map(|problem| ConfigValidationProblem { path: problem.path, message: problem.message })
                .collect(),
        ),
        TimestampOptionsValidation::Ok(options) => {
            ConfigValidationResult::Ok(timestamp_options_to_config_value(&options))
        }
    }
}

/// Resolve a `local` spec against the environment snapshot; other kinds
/// pass through untouched.
fn resolve_local_spec(spec: &TimeZoneSpec, local_time_zone: &str) -> TimeZoneSpec {
    match spec {
        TimeZoneSpec::Local => TimeZoneSpec::Iana { name: local_time_zone.to_string() },
        other => other.clone(),
    }
}

/// A resolved spec → the IANA name wallClockToInstant accepts.
fn spec_to_zone_name(spec: &TimeZoneSpec) -> String {
    match spec {
        TimeZoneSpec::Iana { name } => name.clone(),
        _ => "UTC".to_string(),
    }
}

/// Wall-text parse → full wall components: datetime verbatim, date-only at
/// midnight, time-only anchored to 1970-01-01 (adjudication 3).
fn wall_parse_to_components(parsed: &WallClockTextParse) -> Option<WallComponents> {
    match parsed {
        WallClockTextParse::Datetime(wall) => Some(*wall),
        WallClockTextParse::Date(date) => Some(WallComponents {
            year: date.year,
            month: date.month,
            day: date.day,
            hour: 0,
            minute: 0,
            second: 0,
            nanos: 0,
            fraction_digits: None,
        }),
        WallClockTextParse::Time(time) => Some(WallComponents {
            year: 1970,
            month: 1,
            day: 1,
            hour: time.hour,
            minute: time.minute,
            second: time.second,
            nanos: time.nanos,
            fraction_digits: time.fraction_digits,
        }),
        WallClockTextParse::Err(_) => None,
    }
}

/// Wall text → instant in the resolved source zone, passing the underlying
/// stable reason codes through (parse failure takes the parse code,
/// conversion failure the conversion code).
fn wall_text_to_instant(
    text: &str,
    source_zone_name: &str,
) -> Result<InstantParts, String> {
    let parsed = parse_wall_clock_text(text);
    let wall = match &parsed {
        WallClockTextParse::Err(reason) => return Err(reason.as_kebab().to_string()),
        parsed => wall_parse_to_components(parsed),
    }
    .ok_or_else(|| "invalid-format".to_string())?;
    match wall_clock_to_instant(&wall, source_zone_name) {
        Ok(instant) => Ok(instant),
        Err(reason) => Err(reason.as_kebab().to_string()),
    }
}

/// Format a resolved instant; failures surface the format-layer reason.
fn format_instant(
    instant: &InstantParts,
    options: &TimestampOptions,
    display_time_zone: &TimeZoneSpec,
    locale: &str,
) -> PresentationResult {
    let formatted = format_timestamp_output(
        instant,
        &crate::support::value_presentation::timestamp_format::TimestampFormatOptions {
            format: options.format.into(),
            display_time_zone,
            locale,
        },
    );
    match formatted {
        Ok(text) => PresentationResult::Text {
            text,
            interpretation: Some(interpretation_kind(&options.interpretation).to_string()),
        },
        Err(reason) => PresentationResult::unchanged(reason.as_kebab()),
    }
}

fn interpretation_kind(interpretation: &TimestampInterpretation) -> &'static str {
    match interpretation {
        TimestampInterpretation::Native => "native",
        TimestampInterpretation::Unix { .. } => "unix",
        TimestampInterpretation::Text => "text",
    }
}

/// The prepared timestamp presenter: everything present needs was resolved
/// once in prepare (adjudication 2), present only interprets and formats.
struct TimestampPrepared {
    options: TimestampOptions,
    semantic_kind: SemanticKind,
    display_time_zone: TimeZoneSpec,
    source_zone_name: Option<String>,
    locale: String,
}

impl PreparedPresentation for TimestampPrepared {
    fn present(&self, value: &serde_json::Value) -> PresentationResult {
        // Applicability gate first (before interpretation): `other` columns
        // never apply; `number` columns apply only under unix (native
        // never auto-interprets numbers, design §5).
        if self.semantic_kind == SemanticKind::Other {
            return PresentationResult::unchanged("not-applicable");
        }
        if self.semantic_kind == SemanticKind::Number
            && !matches!(self.options.interpretation, TimestampInterpretation::Unix { .. })
        {
            return PresentationResult::unchanged("not-applicable");
        }

        // NULL falls back (undefined collapses into JSON null); other
        // non-temporal values do not apply.
        if value.is_null() {
            return PresentationResult::unchanged("null");
        }
        if !matches!(
            value,
            serde_json::Value::String(_) | serde_json::Value::Number(_)
        ) {
            return PresentationResult::unchanged("not-applicable");
        }

        match &self.options.interpretation {
            TimestampInterpretation::Native => {
                let Some(text) = value.as_str() else {
                    // Native never auto-interprets numbers as Unix.
                    return PresentationResult::unchanged("not-applicable");
                };
                match parse_zoned_timestamp(text) {
                    Ok(instant) => {
                        format_instant(&instant, &self.options, &self.display_time_zone, &self.locale)
                    }
                    Err(reason) => {
                        // Zoned parse failed: with a source zone, retry as
                        // zone-free wall-clock text.
                        let Some(zone) = &self.source_zone_name else {
                            return PresentationResult::unchanged(reason.as_kebab());
                        };
                        match wall_text_to_instant(text, zone) {
                            Ok(instant) => format_instant(
                                &instant,
                                &self.options,
                                &self.display_time_zone,
                                &self.locale,
                            ),
                            Err(reason) => PresentationResult::unchanged(&reason),
                        }
                    }
                }
            }
            TimestampInterpretation::Unix { unit } => {
                let unit = match unit {
                    UnixUnitSerde::Seconds => UnixUnit::Seconds,
                    UnixUnitSerde::Milliseconds => UnixUnit::Milliseconds,
                };
                let parsed = match value {
                    serde_json::Value::String(text) => parse_unix_value(
                        crate::support::value_presentation::temporal_math::UnixValue::Text(text),
                        unit,
                    ),
                    serde_json::Value::Number(number) => {
                        // The JSON number boundary: every JSON number is an
                        // f64 to the React side, so non-safe integers
                        // answer `not-safe-integer` (React's BigInt route
                        // lives on the driver transport, which this host's
                        // PresentationValue — serde_json — cannot carry).
                        let f64_value = number.as_f64();
                        match f64_value {
                            Some(f64_value)
                                if f64_value.is_finite()
                                    && f64_value.fract() == 0.0
                                    && f64_value.abs() <= 9_007_199_254_740_991.0 =>
                            {
                                parse_unix_value(
                                    crate::support::value_presentation::temporal_math::UnixValue::Number(f64_value),
                                    unit,
                                )
                            }
                            _ => {
                                return PresentationResult::unchanged("not-safe-integer");
                            }
                        }
                    }
                    _ => unreachable!("value gates already excluded other kinds"),
                };
                match parsed {
                    Ok(instant) => {
                        format_instant(&instant, &self.options, &self.display_time_zone, &self.locale)
                    }
                    Err(reason) => PresentationResult::unchanged(reason.as_kebab()),
                }
            }
            TimestampInterpretation::Text => {
                let Some(text) = value.as_str() else {
                    return PresentationResult::unchanged("not-applicable");
                };
                let parsed = parse_wall_clock_text(text);
                if let WallClockTextParse::Err(reason) = parsed {
                    return PresentationResult::unchanged(reason.as_kebab());
                }
                let Some(zone) = &self.source_zone_name else {
                    return PresentationResult::unchanged("missing-source-time-zone");
                };
                let Some(wall) = wall_parse_to_components(&parsed) else {
                    return PresentationResult::unchanged("invalid-format");
                };
                match wall_clock_to_instant(&wall, zone) {
                    Ok(instant) => {
                        format_instant(&instant, &self.options, &self.display_time_zone, &self.locale)
                    }
                    Err(reason) => PresentationResult::unchanged(reason.as_kebab()),
                }
            }
        }
    }
}

/// A fixed-fallback presenter (config/override resolution failed): every
/// value answers the same reason.
struct FixedUnchanged {
    reason: String,
}

impl PreparedPresentation for FixedUnchanged {
    fn present(&self, _value: &serde_json::Value) -> PresentationResult {
        PresentationResult::unchanged(&self.reason)
    }
}

/// Resolve the prepare input into effective options or a fixed fallback
/// reason (adjudication 1's order).
fn resolve_effective_options(
    input: &PresentationPrepareInput<'_>,
) -> Result<TimestampOptions, &'static str> {
    if input.config.config_version != TIMESTAMP_CONTRIBUTION_CONFIG_VERSION {
        return Err("unsupported-config-version");
    }
    let global = match validate_timestamp_options(&input.config.config) {
        TimestampOptionsValidation::Ok(options) => options,
        TimestampOptionsValidation::Err(_) => return Err("invalid-config"),
    };

    match input.column_override {
        None => Ok(global),
        Some(ColumnPresentationOverride::Inherit) => Ok(global),
        Some(ColumnPresentationOverride::Raw) => Err("raw-override"),
        Some(ColumnPresentationOverride::Custom { renderer_id, options }) => {
            if renderer_id != TIMESTAMP_CONTRIBUTION_ID {
                return Err("override-target-mismatch");
            }
            match validate_timestamp_options(options) {
                TimestampOptionsValidation::Ok(custom) => Ok(custom),
                TimestampOptionsValidation::Err(_) => Err("invalid-config"),
            }
        }
    }
}

/// The timestamp presentation contribution `datazen.timestamp`.
pub struct TimestampContribution;

pub fn timestamp_contribution() -> Rc<dyn ValuePresentationContribution> {
    Rc::new(TimestampContribution)
}

impl ValuePresentationContribution for TimestampContribution {
    fn id(&self) -> &str {
        TIMESTAMP_CONTRIBUTION_ID
    }

    fn config_version(&self) -> u32 {
        TIMESTAMP_CONTRIBUTION_CONFIG_VERSION
    }

    fn validate_config(&self, input: &serde_json::Value) -> ConfigValidationResult {
        validate_timestamp_config(input)
    }

    fn prepare(&self, input: &PresentationPrepareInput<'_>) -> Box<dyn PreparedPresentation> {
        let options = match resolve_effective_options(input) {
            Ok(options) => options,
            Err(reason) => {
                return Box::new(FixedUnchanged { reason: reason.to_string() });
            }
        };
        // local zones resolve from the environment snapshot here, once —
        // present never touches the machine zone.
        let PresentationEnvironment { locale, local_time_zone } = input.environment;
        let display_time_zone =
            resolve_local_spec(&options.display_time_zone, local_time_zone);
        let source_zone_name = options.source_time_zone.as_ref().map(|source| {
            spec_to_zone_name(&resolve_local_spec(source, local_time_zone))
        });
        Box::new(TimestampPrepared {
            options,
            semantic_kind: input.column.semantic_kind,
            display_time_zone,
            source_zone_name,
            locale: locale.to_string(),
        })
    }
}

/// Helper for hosts assembling a validated config snapshot inline.
pub fn versioned_config(config: serde_json::Value) -> VersionedPresentationConfig {
    VersionedPresentationConfig {
        config_version: TIMESTAMP_CONTRIBUTION_CONFIG_VERSION,
        config,
    }
}
