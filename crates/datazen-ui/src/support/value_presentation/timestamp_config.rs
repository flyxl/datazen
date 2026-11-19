//! Port of `packages/frontend/src/lib/valuePresentation/timestampConfig.ts`.
//!
//! The timestamp renderer's persisted configuration shape and its complete
//! structural validation (issue-32 design §5, T02): `unknown` JSON in,
//! either a rebuilt canonical [`TimestampOptions`] (recognized fields only,
//! `sourceTimeZone` absent when unset — stable JSON) or *every* pathed
//! problem at once (no early stop, so the settings panel can show them all).
//!
//! # What maps where
//!
//! - The `unknown`-in / discriminated-union-out contract becomes
//!   `&serde_json::Value` in / [`TimestampOptionsValidation`] out. The
//!   problem paths and messages are React's byte-for-byte (the React tests
//!   pin them with `toContainEqual`).
//! - The canonical types reuse the area's earlier ports:
//!   [`TimestampOutputFormat`] / [`TimeZoneSpec`] (T272) and
//!   [`UnixUnit`] (T207). They carry serde derives with React's exact JSON
//!   field names (`kind` tags, camelCase, `iso-tz`) because the stored
//!   settings (T182/T310 tasks) persist `TimestampOptions` verbatim as
//!   renderer options.
//! - `Object.freeze` is elided (no mutation semantics in Rust — the
//!   canonical value is owned); the React test that asserts deep-frozen
//!   output is replaced by an exact-JSON-shape pin, which is what freezing
//!   protected.
//! - Key-presence semantics: JSON has no `undefined`, so React's
//!   `sourceTimeZone: undefined`-behaves-as-absent is naturally the absent
//!   key; a present `null` still fails the shape check, exactly like
//!   React's `!== undefined` guard.
//!
//! # Known deltas (type-level)
//!
//! React's `not-a-string` / runtime-garbage input guards are covered by
//! `serde_json::Value` being untyped at the same granularity — every
//! garbage shape the React tests feed (numbers, strings, booleans, arrays,
//! nulls in field positions) is representable here and produces the same
//! pathed problems.

use serde::{Deserialize, Serialize};

use crate::support::value_presentation::temporal_math::UnixUnit;
use crate::support::value_presentation::timestamp_format::{
    TimestampOutputFormat, TimeZoneSpec, is_valid_iana_time_zone_name,
};

/// How a timestamp column's raw values are interpreted
/// (React `TimestampInterpretation`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub(crate) enum TimestampInterpretation {
    Native,
    Unix {
        unit: UnixUnitSerde,
    },
    Text,
}

/// `UnixUnit` with the lowercase serde names React persists
/// (`"seconds"` / `"milliseconds"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum UnixUnitSerde {
    Seconds,
    Milliseconds,
}

impl From<UnixUnitSerde> for UnixUnit {
    fn from(value: UnixUnitSerde) -> Self {
        match value {
            UnixUnitSerde::Seconds => UnixUnit::Seconds,
            UnixUnitSerde::Milliseconds => UnixUnit::Milliseconds,
        }
    }
}

/// The validated renderer options (React `TimestampOptions`). When
/// serialized, `sourceTimeZone` is absent unless set — React's stable-JSON
/// contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TimestampOptions {
    pub(crate) interpretation: TimestampInterpretation,
    pub(crate) format: FormatSerde,
    pub(crate) display_time_zone: TimeZoneSpec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) source_time_zone: Option<TimeZoneSpec>,
}

/// `TimestampOutputFormat` with React's persisted spellings (`iso-tz`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum FormatSerde {
    #[serde(rename = "iso")]
    Iso,
    #[serde(rename = "iso-tz")]
    IsoTz,
    #[serde(rename = "locale")]
    Locale,
}

impl From<FormatSerde> for TimestampOutputFormat {
    fn from(value: FormatSerde) -> Self {
        match value {
            FormatSerde::Iso => TimestampOutputFormat::Iso,
            FormatSerde::IsoTz => TimestampOutputFormat::IsoTz,
            FormatSerde::Locale => TimestampOutputFormat::Locale,
        }
    }
}

/// A pathed validation problem: `path` points into the JSON input ('' is
/// the root), `message` says why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TimestampOptionsProblem {
    pub(crate) path: String,
    pub(crate) message: String,
}

impl TimestampOptionsProblem {
    fn new(path: &str, message: &str) -> Self {
        Self { path: path.to_string(), message: message.to_string() }
    }
}

/// The validation result: every problem at once, or the canonical options.
#[derive(Debug, PartialEq)]
pub(crate) enum TimestampOptionsValidation {
    Ok(TimestampOptions),
    Err(Vec<TimestampOptionsProblem>),
}

fn is_plain_object(value: Option<&serde_json::Value>) -> bool {
    matches!(value, Some(serde_json::Value::Object(_)))
}

/// Read and validate one TimeZoneSpec; problems append to `problems` with
/// the given path prefix. `None` when anything was rejected.
fn read_time_zone_spec(
    value: Option<&serde_json::Value>,
    path: &str,
    problems: &mut Vec<TimestampOptionsProblem>,
) -> Option<TimeZoneSpec> {
    let object = match value {
        Some(serde_json::Value::Object(map)) => map,
        _ => {
            problems.push(TimestampOptionsProblem::new(path, "expected an object"));
            return None;
        }
    };
    let before = problems.len();

    let kind = object.get("kind").and_then(serde_json::Value::as_str);
    let mut spec: Option<TimeZoneSpec> = None;
    match kind {
        Some("utc") => spec = Some(TimeZoneSpec::Utc),
        Some("local") => spec = Some(TimeZoneSpec::Local),
        Some("iana") => {
            let name = object.get("name").and_then(serde_json::Value::as_str);
            match name {
                Some(name) if is_valid_iana_time_zone_name(name) => {
                    spec = Some(TimeZoneSpec::Iana { name: name.to_string() });
                }
                _ => problems.push(TimestampOptionsProblem::new(
                    &format!("{path}.name"),
                    "expected a non-empty IANA time zone name without whitespace",
                )),
            }
        }
        _ => problems.push(TimestampOptionsProblem::new(
            &format!("{path}.kind"),
            "expected \"utc\", \"local\" or \"iana\"",
        )),
    }

    for key in object.keys() {
        if key != "kind" && key != "name" {
            problems.push(TimestampOptionsProblem::new(
                &format!("{path}.{key}"),
                "unknown field",
            ));
        }
    }
    if let Some(resolved) = &spec {
        if !matches!(resolved, TimeZoneSpec::Iana { .. }) && object.contains_key("name") {
            problems.push(TimestampOptionsProblem::new(
                &format!("{path}.name"),
                "unknown field (only \"iana\" carries a name)",
            ));
        }
    }

    if problems.len() > before {
        None
    } else {
        spec
    }
}

/// Read and validate the interpretation (React `readInterpretation`).
fn read_interpretation(
    value: Option<&serde_json::Value>,
    path: &str,
    problems: &mut Vec<TimestampOptionsProblem>,
) -> Option<TimestampInterpretation> {
    let object = match value {
        Some(serde_json::Value::Object(map)) => map,
        _ => {
            problems.push(TimestampOptionsProblem::new(path, "expected an object"));
            return None;
        }
    };
    let before = problems.len();

    let kind = object.get("kind").and_then(serde_json::Value::as_str);
    let mut interpretation: Option<TimestampInterpretation> = None;
    match kind {
        Some(kind @ ("native" | "text")) => {
            interpretation = Some(if kind == "native" {
                TimestampInterpretation::Native
            } else {
                TimestampInterpretation::Text
            });
            if object.contains_key("unit") {
                problems.push(TimestampOptionsProblem::new(
                    &format!("{path}.unit"),
                    &format!("unknown field (\"{kind}\" interpretation has no unit)"),
                ));
            }
        }
        Some("unix") => {
            let unit = object.get("unit").and_then(serde_json::Value::as_str);
            interpretation = match unit {
                Some("seconds") => Some(TimestampInterpretation::Unix {
                    unit: UnixUnitSerde::Seconds,
                }),
                Some("milliseconds") => Some(TimestampInterpretation::Unix {
                    unit: UnixUnitSerde::Milliseconds,
                }),
                _ => {
                    problems.push(TimestampOptionsProblem::new(
                        &format!("{path}.unit"),
                        "expected \"seconds\" or \"milliseconds\"",
                    ));
                    None
                }
            };
        }
        _ => problems.push(TimestampOptionsProblem::new(
            &format!("{path}.kind"),
            "expected \"native\", \"unix\" or \"text\"",
        )),
    }

    for key in object.keys() {
        if key != "kind" && key != "unit" {
            problems.push(TimestampOptionsProblem::new(
                &format!("{path}.{key}"),
                "unknown field",
            ));
        }
    }

    if problems.len() > before {
        None
    } else {
        interpretation
    }
}

/// Complete structural validation of unknown JSON input. Success returns
/// the rebuilt canonical options (recognized fields only); failure returns
/// every pathed problem.
pub(crate) fn validate_timestamp_options(
    input: &serde_json::Value,
) -> TimestampOptionsValidation {
    let Some(object) = input.as_object() else {
        return TimestampOptionsValidation::Err(vec![TimestampOptionsProblem::new(
            "",
            "expected an object",
        )]);
    };
    let mut problems: Vec<TimestampOptionsProblem> = Vec::new();

    let interpretation = read_interpretation(
        object.get("interpretation"),
        "interpretation",
        &mut problems,
    );

    let mut format: Option<FormatSerde> = None;
    match object.get("format").and_then(serde_json::Value::as_str) {
        Some("iso") => format = Some(FormatSerde::Iso),
        Some("iso-tz") => format = Some(FormatSerde::IsoTz),
        Some("locale") => format = Some(FormatSerde::Locale),
        _ => problems.push(TimestampOptionsProblem::new(
            "format",
            "expected \"iso\", \"iso-tz\" or \"locale\"",
        )),
    }

    let display_time_zone =
        read_time_zone_spec(object.get("displayTimeZone"), "displayTimeZone", &mut problems);

    let mut source_time_zone: Option<TimeZoneSpec> = None;
    if let Some(source) = object.get("sourceTimeZone") {
        // JSON has no `undefined`: a present key is present, `null` fails
        // the shape check below — React's `!== undefined` guard exactly.
        source_time_zone = read_time_zone_spec(Some(source), "sourceTimeZone", &mut problems);
        if let Some(TimestampInterpretation::Unix { .. }) = &interpretation {
            problems.push(TimestampOptionsProblem::new(
                "sourceTimeZone",
                "unix interpretation must not carry a source time zone",
            ));
        }
    }

    for key in object.keys() {
        if key != "interpretation"
            && key != "format"
            && key != "displayTimeZone"
            && key != "sourceTimeZone"
        {
            problems.push(TimestampOptionsProblem::new(key, "unknown field"));
        }
    }

    if !problems.is_empty()
        || interpretation.is_none()
        || format.is_none()
        || display_time_zone.is_none()
    {
        return TimestampOptionsValidation::Err(problems);
    }

    TimestampOptionsValidation::Ok(TimestampOptions {
        interpretation: interpretation.expect("checked above"),
        format: format.expect("checked above"),
        display_time_zone: display_time_zone.expect("checked above"),
        source_time_zone,
    })
}
