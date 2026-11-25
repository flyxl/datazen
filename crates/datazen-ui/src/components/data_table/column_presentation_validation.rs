//! T054-S1（校验半）：`ColumnPresentationDialog` 的 JSON 校验镜像 ——
//! `validateTimestampOptions`（`lib/valuePresentation/timestampConfig.ts`）
//! 与 `validateColumnPresentationOverride`
//! （`lib/valuePresentation/columnPresentation.ts`）的逐行镜像。
//!
//! 从 `column_presentation_dialog.rs` 拆出，只为守单文件 ≤800 行的推荐
//! 上限；两模块同属 T054-S1，S2 与测试仍从
//! `column_presentation_dialog` 一个入口导入（该模块重导出本模块的公开项）。
//!
//! 入参是 `serde_json::Value`（TS 的 `unknown`）：成功返回只含已识别字段
//! 的重建值，失败返回全部路径化问题，不中断于第一个错误。问题文案与
//! TS 一字不差（含 `"options."` 这一 TS 前缀拼出的尾点，见
//! [`validate_column_presentation_override`]）。

use serde_json::Value;

use super::column_presentation_dialog::{
    is_valid_iana_time_zone_name, ColumnPresentationOverride, TimeZoneSpec,
    TimestampInterpretation, TimestampOptions, TimestampOutputFormat, UnixUnit,
    TIMESTAMP_CONTRIBUTION_ID,
};

/// 路径化验证问题（与 TS 的 `TimestampOptionsProblem` /
/// `ColumnPresentationOverrideProblem` 同形：`path` 指向输入内的位置，
/// `''` 为根）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationProblem {
    pub path: String,
    pub message: String,
}

pub fn problem(path: impl Into<String>, message: impl Into<String>) -> ValidationProblem {
    ValidationProblem {
        path: path.into(),
        message: message.into(),
    }
}

/// JS 空白：`char::is_whitespace`（Unicode White_Space）之外还含
/// U+FEFF（ZERO WIDTH NO-BREAK SPACE）——TS 的 `/\s/` 与
/// `String.prototype.trim` 都把它当空白。只用于 `rendererId` 判定
/// （见 [`validate_column_presentation_override`]）。
fn is_js_whitespace(c: char) -> bool {
    c.is_whitespace() || c == '\u{FEFF}'
}

/// 按 JS 空白剥首尾空白（见 [`is_js_whitespace`]）。
fn trim_js_whitespace(value: &str) -> &str {
    value.trim_matches(is_js_whitespace)
}

/// 是否含 JS 空白（见 [`is_js_whitespace`]）。
fn contains_js_whitespace(value: &str) -> bool {
    value.chars().any(is_js_whitespace)
}

// ---------------------------------------------------------------------------
// 校验：`validateTimestampOptions` / `validateColumnPresentationOverride`
// 的逐行镜像（`unknown` 入参 → 规范值或全部路径化问题，不中断于第一个
// 错误；成功返回只含已识别字段的重建值）
// ---------------------------------------------------------------------------

/// 读取并验证一个 `TimeZoneSpec`；发现问题时记入 `problems` 并返回 `None`
///（TS `readTimeZoneSpec`：调用内新增任何问题都视为该字段无效）。
fn read_time_zone_spec(
    value: &Value,
    path: &str,
    problems: &mut Vec<ValidationProblem>,
) -> Option<TimeZoneSpec> {
    let before = problems.len();
    let object = match value.as_object() {
        Some(object) => object,
        None => {
            problems.push(problem(path, "expected an object"));
            return None;
        }
    };
    let mut spec: Option<TimeZoneSpec> = None;
    match object.get("kind").and_then(Value::as_str) {
        Some("utc") => spec = Some(TimeZoneSpec::Utc),
        Some("local") => spec = Some(TimeZoneSpec::Local),
        Some("iana") => match object.get("name").and_then(Value::as_str) {
            Some(name) if is_valid_iana_time_zone_name(name) => {
                spec = Some(TimeZoneSpec::Iana {
                    name: name.to_owned(),
                });
            }
            _ => problems.push(problem(
                format!("{path}.name"),
                "expected a non-empty IANA time zone name without whitespace",
            )),
        },
        _ => problems.push(problem(
            format!("{path}.kind"),
            "expected \"utc\", \"local\" or \"iana\"",
        )),
    }
    for key in object.keys() {
        if key != "kind" && key != "name" {
            problems.push(problem(format!("{path}.{key}"), "unknown field"));
        }
    }
    if let Some(TimeZoneSpec::Utc | TimeZoneSpec::Local) = spec {
        if object.contains_key("name") {
            problems.push(problem(
                format!("{path}.name"),
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

/// 读取并验证一个 `TimestampInterpretation`（同上，TS `readInterpretation`）。
fn read_interpretation(
    value: &Value,
    path: &str,
    problems: &mut Vec<ValidationProblem>,
) -> Option<TimestampInterpretation> {
    let before = problems.len();
    let object = match value.as_object() {
        Some(object) => object,
        None => {
            problems.push(problem(path, "expected an object"));
            return None;
        }
    };
    let mut interpretation: Option<TimestampInterpretation> = None;
    match object.get("kind").and_then(Value::as_str) {
        Some(kind @ ("native" | "text")) => {
            interpretation = Some(if kind == "text" {
                TimestampInterpretation::Text
            } else {
                TimestampInterpretation::Native
            });
            if object.contains_key("unit") {
                problems.push(problem(
                    format!("{path}.unit"),
                    format!("unknown field (\"{kind}\" interpretation has no unit)"),
                ));
            }
        }
        Some("unix") => match object.get("unit").and_then(Value::as_str) {
            Some("seconds") => {
                interpretation = Some(TimestampInterpretation::Unix(UnixUnit::Seconds));
            }
            Some("milliseconds") => {
                interpretation = Some(TimestampInterpretation::Unix(UnixUnit::Milliseconds));
            }
            _ => problems.push(problem(
                format!("{path}.unit"),
                "expected \"seconds\" or \"milliseconds\"",
            )),
        },
        _ => problems.push(problem(
            format!("{path}.kind"),
            "expected \"native\", \"unix\" or \"text\"",
        )),
    }
    for key in object.keys() {
        if key != "kind" && key != "unit" {
            problems.push(problem(format!("{path}.{key}"), "unknown field"));
        }
    }
    if problems.len() > before {
        None
    } else {
        interpretation
    }
}

/// 完整结构验证（`unknown` 入参）。成功返回重建的规范
/// `TimestampOptions`（只含已识别字段）；失败返回全部路径化问题。
pub fn validate_timestamp_options(
    input: &Value,
) -> Result<TimestampOptions, Vec<ValidationProblem>> {
    let mut problems: Vec<ValidationProblem> = Vec::new();
    let object = match input.as_object() {
        Some(object) => object,
        None => {
            return Err(vec![problem("", "expected an object")]);
        }
    };
    let interpretation = match object.get("interpretation") {
        Some(value) => read_interpretation(value, "interpretation", &mut problems),
        // TS：`readInterpretation(undefined)` —— 缺键与非对象同报。
        None => {
            problems.push(problem("interpretation", "expected an object"));
            None
        }
    };
    let format = match object.get("format").and_then(Value::as_str) {
        Some("iso") => Some(TimestampOutputFormat::Iso),
        Some("iso-tz") => Some(TimestampOutputFormat::IsoTz),
        Some("locale") => Some(TimestampOutputFormat::Locale),
        _ => {
            problems.push(problem(
                "format",
                "expected \"iso\", \"iso-tz\" or \"locale\"",
            ));
            None
        }
    };
    let display_time_zone = match object.get("displayTimeZone") {
        Some(value) => read_time_zone_spec(value, "displayTimeZone", &mut problems),
        // TS：`readTimeZoneSpec(undefined)` —— 缺键与非对象同报。
        None => {
            problems.push(problem("displayTimeZone", "expected an object"));
            None
        }
    };
    let mut source_time_zone: Option<TimeZoneSpec> = None;
    if let Some(value) = object.get("sourceTimeZone") {
        source_time_zone = read_time_zone_spec(value, "sourceTimeZone", &mut problems);
        if matches!(interpretation, Some(TimestampInterpretation::Unix(_))) {
            problems.push(problem(
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
            problems.push(problem(key, "unknown field"));
        }
    }
    match (interpretation, format, display_time_zone) {
        (Some(interpretation), Some(format), Some(display_time_zone)) if problems.is_empty() => {
            Ok(TimestampOptions {
                interpretation,
                format,
                display_time_zone,
                source_time_zone,
            })
        }
        _ => Err(problems),
    }
}

/// 完整结构验证（`unknown` 入参）：`inherit` / `raw` 不接受除 `mode`
/// 外的任何字段；`custom` 只接受时间展示器 `datazen.timestamp`，
/// 其 options 经 [`validate_timestamp_options`] 校验，问题路径以
/// `options.` 为前缀。
pub fn validate_column_presentation_override(
    input: &Value,
) -> Result<ColumnPresentationOverride, Vec<ValidationProblem>> {
    let mut problems: Vec<ValidationProblem> = Vec::new();
    let object = match input.as_object() {
        Some(object) => object,
        None => {
            return Err(vec![problem("", "expected an object")]);
        }
    };
    match object.get("mode").and_then(Value::as_str) {
        Some("inherit") | Some("raw") => {
            let mode = object
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or("inherit");
            for key in object.keys() {
                if key != "mode" {
                    problems.push(problem(key, "unknown field"));
                }
            }
            if problems.is_empty() {
                return Ok(if mode == "raw" {
                    ColumnPresentationOverride::Raw
                } else {
                    ColumnPresentationOverride::Inherit
                });
            }
            return Err(problems);
        }
        Some("custom") => {}
        _ => {
            return Err(vec![problem(
                "mode",
                "expected \"inherit\", \"raw\" or \"custom\"",
            )]);
        }
    }
    let mut renderer_id: Option<String> = None;
    match object.get("rendererId").and_then(Value::as_str) {
        None => problems.push(problem("rendererId", "expected a string")),
        Some(raw) => {
            // T054-S2 顺手修（S1 tester 非阻塞发现 1）：TS 在这里用
            // `/\s/` 判名内空白、用 `.trim()` 剥首尾空白，二者都把
            // U+FEFF 当空白；Rust 的 `char::is_whitespace` 与 `trim()`
            // 都不含 U+FEFF，所以 `"datazen.timestamp\u{FEFF}"` 在 TS
            // 下合法、在 Rust 下会报 `unknown renderer`。只在这一处
            // 判定上按 JS 空白对齐，S1 其余 trim/空白判定不动。
            let trimmed = trim_js_whitespace(raw);
            if trimmed.is_empty() {
                problems.push(problem("rendererId", "must not be empty"));
            } else if contains_js_whitespace(trimmed) {
                problems.push(problem("rendererId", "must not contain whitespace"));
            } else {
                renderer_id = Some(trimmed.to_owned());
            }
        }
    }
    let mut options: Option<TimestampOptions> = None;
    if let Some(id) = &renderer_id {
        if id != TIMESTAMP_CONTRIBUTION_ID {
            problems.push(problem("rendererId", "unknown renderer"));
        } else if let Some(raw_options) = object.get("options") {
            match validate_timestamp_options(raw_options) {
                Ok(validated) => options = Some(validated),
                Err(option_problems) => {
                    for option_problem in option_problems {
                        problems.push(problem(
                            format!("options.{}", option_problem.path),
                            option_problem.message,
                        ));
                    }
                }
            }
        } else {
            // `options` 缺省：TS 里 `validateTimestampOptions(undefined)`
            // 报根问题（`path: ''`），前缀后为 `"options."`——原样保留。
            problems.push(problem("options.", "expected an object"));
        }
    }
    for key in object.keys() {
        if key != "mode" && key != "rendererId" && key != "options" {
            problems.push(problem(key, "unknown field"));
        }
    }
    match (renderer_id, options) {
        (Some(renderer_id), Some(options)) if problems.is_empty() => {
            Ok(ColumnPresentationOverride::Custom {
                renderer_id,
                options: options.to_json(),
            })
        }
        _ => Err(problems),
    }
}
