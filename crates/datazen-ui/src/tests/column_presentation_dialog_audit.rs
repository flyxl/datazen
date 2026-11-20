//! T054-S1 独立审计：`ColumnPresentationDialog` 草稿模型 + 纯函数的
//! tester 测试。
//!
//! 与 coder 测试（`column_presentation_dialog.rs`，51 个）独立编写：
//! 不复用它的 helper 与用例，只以 React 原件
//! `packages/frontend/src/components/DataTable/ColumnPresentationDialog.tsx`
//! 与 `lib/valuePresentation/`（`timestampConfig.ts`、
//! `columnPresentation.ts`、`timestampFormat.ts`）为对照，逐项断言
//! TS 语义在 Rune 侧的镜像是否精确：
//!
//! - 文案一字不差：全部问题用 `problem(path, message)` 构造期望向量做
//!   精确相等断言，不只断言 path 或 `contains`；
//! - 边界：IANA 名 100/101 字符、JSON `null` 入参、`"options."` 尾点、
//!   `unix` 携源时区、utc/local 携 `name`、native/text 携 `unit`；
//! - 接线口：`is_known_time_zone` 谓词收到的是 trim 后的名、格式非法时
//!   不调用、非 iana 时不调用、谓词说了算（非硬编码名单）；
//! - `draft_from_column_override` 不看 `rendererId`（与 TS 一致：
//!   只校验 `current.options`）；非法 options 回落 base + custom。

use std::cell::{Cell, RefCell};

use serde_json::{json, Value};

use crate::components::data_table::column_presentation_dialog::{
    draft_from_column_override, draft_to_column_override, draft_to_timestamp_options,
    global_timestamp_options_from_snapshot, is_interpretation_choice, is_output_format,
    is_source_time_zone_choice, is_time_zone_kind_choice, is_valid_iana_time_zone_name, problem,
    validate_column_presentation_draft, validate_column_presentation_override,
    validate_timestamp_options, ColumnPresentationDraft, ColumnPresentationOverride,
    InterpretationChoice, PresentationMode, PresentationSemanticKind, SourceTimeZoneChoice,
    TimeZoneKindChoice, TimeZoneSpec, TimestampInterpretation, TimestampOptions,
    TimestampOutputFormat, UnixUnit, ValuePresentationSnapshot, VersionedPresentationConfig,
    TIMESTAMP_CONTRIBUTION_ID,
};

/// 桩已知性：只认 Asia/Shanghai 与 Europe/London，其余一律未知。
fn known(name: &str) -> bool {
    matches!(name, "Asia/Shanghai" | "Europe/London")
}

fn draft() -> ColumnPresentationDraft {
    ColumnPresentationDraft {
        mode: PresentationMode::Custom,
        interpretation: InterpretationChoice::Native,
        format: TimestampOutputFormat::IsoTz,
        display_time_zone_kind: TimeZoneKindChoice::Local,
        display_iana_name: "Asia/Shanghai".to_owned(),
        source_time_zone_kind: SourceTimeZoneChoice::None,
        source_iana_name: "Asia/Shanghai".to_owned(),
    }
}

fn valid_options_json() -> Value {
    json!({
        "interpretation": { "kind": "unix", "unit": "milliseconds" },
        "format": "iso-tz",
        "displayTimeZone": { "kind": "local" },
    })
}

fn snapshot_with(config: Value) -> ValuePresentationSnapshot {
    let mut snapshot = ValuePresentationSnapshot::default();
    snapshot.configs.insert(
        TIMESTAMP_CONTRIBUTION_ID.to_owned(),
        VersionedPresentationConfig {
            config_version: 1,
            config,
        },
    );
    snapshot
}

// ---------------------------------------------------------------------------
// is_valid_iana_time_zone_name：格式镜像
// ---------------------------------------------------------------------------

#[test]
fn audit_iana_name_allows_the_full_charset() {
    for name in [
        "Asia/Shanghai",
        "america/new_york",
        "UTC",
        "Etc/GMT+5",
        "Etc/GMT-12",
        "A",
        "a-1_b/c+d",
    ] {
        assert!(is_valid_iana_time_zone_name(name), "{name} must be valid");
    }
}

#[test]
fn audit_iana_name_boundary_is_100_chars() {
    let exactly_100 = "A".repeat(100);
    assert!(is_valid_iana_time_zone_name(&exactly_100));
    let over_100 = "A".repeat(101);
    assert!(!is_valid_iana_time_zone_name(&over_100));
    // 100 字符的合法分段名同样通过（长度按字符计，非路径段数）。
    let segmented = "Abc/".repeat(25);
    assert_eq!(segmented.len(), 100);
    assert!(is_valid_iana_time_zone_name(&segmented));
}

#[test]
fn audit_iana_name_rejects_whitespace_dot_and_non_ascii() {
    for name in [
        "",
        " ",
        "Asia/Shanghai ",
        " Asia/Shanghai",
        "Asia/Shang hai",
        "Asia\tShanghai",
        "Asia\nShanghai",
        "Asia.Shanghai",
        "Asia\\Shanghai",
        "Asia/Shanghai!",
        "Asia/上海",
        "Europe/Paris\u{00a0}",
    ] {
        assert!(
            !is_valid_iana_time_zone_name(name),
            "{name:?} must be invalid"
        );
    }
}

// ---------------------------------------------------------------------------
// validate_timestamp_options：文案与路径的精确镜像
// ---------------------------------------------------------------------------

#[test]
fn audit_validate_options_rejects_non_objects_at_root() {
    for input in [
        Value::Null,
        json!("text"),
        json!(42),
        json!(true),
        json!([]),
        json!([{ "interpretation": { "kind": "native" } }]),
    ] {
        assert_eq!(
            validate_timestamp_options(&input),
            Err(vec![problem("", "expected an object")]),
            "input {input:?}"
        );
    }
}

#[test]
fn audit_validate_options_collects_missing_fields() {
    // interpretation 与 displayTimeZone 双双缺键：两问题都报，不中断。
    assert_eq!(
        validate_timestamp_options(&json!({ "format": "iso" })),
        Err(vec![
            problem("interpretation", "expected an object"),
            problem("displayTimeZone", "expected an object"),
        ])
    );
    // 显式 null 与缺键同报（TS：readInterpretation/readTimeZoneSpec
    // 收到 null 同样报 expected an object）。
    assert_eq!(
        validate_timestamp_options(
            &json!({ "interpretation": null, "format": "iso", "displayTimeZone": null })
        ),
        Err(vec![
            problem("interpretation", "expected an object"),
            problem("displayTimeZone", "expected an object"),
        ])
    );
}

#[test]
fn audit_validate_options_rejects_bad_format_and_kind_together() {
    assert_eq!(
        validate_timestamp_options(&json!({
            "interpretation": { "kind": "sideways" },
            "format": "rfc3339",
            "displayTimeZone": { "kind": "local" },
        })),
        Err(vec![
            problem(
                "interpretation.kind",
                "expected \"native\", \"unix\" or \"text\""
            ),
            problem("format", "expected \"iso\", \"iso-tz\" or \"locale\""),
        ])
    );
}

#[test]
fn audit_validate_options_rejects_unit_on_native_and_text_verbatim() {
    for kind in ["native", "text"] {
        assert_eq!(
            validate_timestamp_options(&json!({
                "interpretation": { "kind": kind, "unit": "seconds" },
                "format": "iso",
                "displayTimeZone": { "kind": "utc" },
            })),
            Err(vec![problem(
                "interpretation.unit",
                format!("unknown field (\"{kind}\" interpretation has no unit)")
            )]),
            "kind {kind}"
        );
    }
}

#[test]
fn audit_validate_options_rejects_unix_without_unit_verbatim() {
    assert_eq!(
        validate_timestamp_options(&json!({
            "interpretation": { "kind": "unix" },
            "format": "iso",
            "displayTimeZone": { "kind": "utc" },
        })),
        Err(vec![problem(
            "interpretation.unit",
            "expected \"seconds\" or \"milliseconds\""
        )])
    );
    assert_eq!(
        validate_timestamp_options(&json!({
            "interpretation": { "kind": "unix", "unit": "nanoseconds" },
            "format": "iso",
            "displayTimeZone": { "kind": "utc" },
        })),
        Err(vec![problem(
            "interpretation.unit",
            "expected \"seconds\" or \"milliseconds\""
        )])
    );
}

#[test]
fn audit_validate_options_rejects_name_on_utc_and_local_verbatim() {
    for kind in ["utc", "local"] {
        assert_eq!(
            validate_timestamp_options(&json!({
                "interpretation": { "kind": "native" },
                "format": "iso",
                "displayTimeZone": { "kind": kind, "name": "Asia/Shanghai" },
            })),
            Err(vec![problem(
                "displayTimeZone.name",
                "unknown field (only \"iana\" carries a name)"
            )]),
            "kind {kind}"
        );
    }
}

#[test]
fn audit_validate_options_rejects_bad_iana_names_verbatim() {
    // 缺 name、name 非字符串、name 含空白、name 超长：同一文案。
    for display in [
        json!({ "kind": "iana" }),
        json!({ "kind": "iana", "name": 42 }),
        json!({ "kind": "iana", "name": "Asia/Shang hai" }),
        json!({ "kind": "iana", "name": "A".repeat(101) }),
        json!({ "kind": "iana", "name": "" }),
    ] {
        assert_eq!(
            validate_timestamp_options(&json!({
                "interpretation": { "kind": "native" },
                "format": "iso",
                "displayTimeZone": display,
            })),
            Err(vec![problem(
                "displayTimeZone.name",
                "expected a non-empty IANA time zone name without whitespace"
            )]),
            "display {display:?}"
        );
    }
}

#[test]
fn audit_validate_options_rejects_bad_zone_kind_verbatim() {
    assert_eq!(
        validate_timestamp_options(&json!({
            "interpretation": { "kind": "native" },
            "format": "iso",
            "displayTimeZone": { "kind": "mars" },
        })),
        Err(vec![problem(
            "displayTimeZone.kind",
            "expected \"utc\", \"local\" or \"iana\""
        )])
    );
    assert_eq!(
        validate_timestamp_options(&json!({
            "interpretation": { "kind": "native" },
            "format": "iso",
            "displayTimeZone": "local",
        })),
        Err(vec![problem("displayTimeZone", "expected an object")])
    );
}

#[test]
fn audit_validate_options_rejects_unknown_fields_at_every_level() {
    assert_eq!(
        validate_timestamp_options(&json!({
            "interpretation": { "kind": "native", "extra": 1 },
            "format": "iso",
            "displayTimeZone": { "kind": "local", "extra": 2 },
            "topExtra": true,
        })),
        Err(vec![
            problem("interpretation.extra", "unknown field"),
            problem("displayTimeZone.extra", "unknown field"),
            problem("topExtra", "unknown field"),
        ])
    );
}

#[test]
fn audit_validate_options_rejects_unix_with_source_zone_verbatim() {
    // 源时区本身合法时，只报 unix 携源这一条。
    assert_eq!(
        validate_timestamp_options(&json!({
            "interpretation": { "kind": "unix", "unit": "seconds" },
            "format": "iso",
            "displayTimeZone": { "kind": "utc" },
            "sourceTimeZone": { "kind": "utc" },
        })),
        Err(vec![problem(
            "sourceTimeZone",
            "unix interpretation must not carry a source time zone"
        )])
    );
    // 源时区本身也非法时，两问题都报：先是源规约的问题，再是 unix 携源。
    assert_eq!(
        validate_timestamp_options(&json!({
            "interpretation": { "kind": "unix", "unit": "seconds" },
            "format": "iso",
            "displayTimeZone": { "kind": "utc" },
            "sourceTimeZone": { "kind": "mars" },
        })),
        Err(vec![
            problem(
                "sourceTimeZone.kind",
                "expected \"utc\", \"local\" or \"iana\""
            ),
            problem(
                "sourceTimeZone",
                "unix interpretation must not carry a source time zone"
            ),
        ])
    );
    // 源时区显式 null：TS 视作已提供（!== undefined），读规约报错。
    assert_eq!(
        validate_timestamp_options(&json!({
            "interpretation": { "kind": "text" },
            "format": "iso",
            "displayTimeZone": { "kind": "utc" },
            "sourceTimeZone": null,
        })),
        Err(vec![problem("sourceTimeZone", "expected an object")])
    );
}

#[test]
fn audit_validate_options_returns_normalized_reconstruction() {
    let expected = TimestampOptions {
        interpretation: TimestampInterpretation::Unix(UnixUnit::Milliseconds),
        format: TimestampOutputFormat::IsoTz,
        display_time_zone: TimeZoneSpec::Local,
        source_time_zone: None,
    };
    assert_eq!(
        validate_timestamp_options(&valid_options_json()),
        Ok(expected.clone())
    );
    // 成功返回的重建值只含已识别字段：to_json 与输入同形。
    assert_eq!(expected.to_json(), valid_options_json());
    // 带源时区的 text 配置同样往返。
    let with_source = json!({
        "interpretation": { "kind": "text" },
        "format": "locale",
        "displayTimeZone": { "kind": "iana", "name": "Europe/London" },
        "sourceTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
    });
    let expected = TimestampOptions {
        interpretation: TimestampInterpretation::Text,
        format: TimestampOutputFormat::Locale,
        display_time_zone: TimeZoneSpec::Iana {
            name: "Europe/London".to_owned(),
        },
        source_time_zone: Some(TimeZoneSpec::Iana {
            name: "Asia/Shanghai".to_owned(),
        }),
    };
    assert_eq!(
        validate_timestamp_options(&with_source),
        Ok(expected.clone())
    );
    assert_eq!(expected.to_json(), with_source);
}

// ---------------------------------------------------------------------------
// validate_column_presentation_override：文案与 "options." 尾点
// ---------------------------------------------------------------------------

#[test]
fn audit_validate_override_rejects_non_objects_and_bad_modes_verbatim() {
    for input in [Value::Null, json!("inherit"), json!([])] {
        assert_eq!(
            validate_column_presentation_override(&input),
            Err(vec![problem("", "expected an object")]),
            "input {input:?}"
        );
    }
    assert_eq!(
        validate_column_presentation_override(&json!({})),
        Err(vec![problem(
            "mode",
            "expected \"inherit\", \"raw\" or \"custom\""
        )])
    );
    assert_eq!(
        validate_column_presentation_override(&json!({ "mode": "CUSTOM" })),
        Err(vec![problem(
            "mode",
            "expected \"inherit\", \"raw\" or \"custom\""
        )])
    );
}

#[test]
fn audit_validate_override_inherit_and_raw_accept_nothing_else() {
    assert_eq!(
        validate_column_presentation_override(&json!({ "mode": "inherit" })),
        Ok(ColumnPresentationOverride::Inherit)
    );
    assert_eq!(
        validate_column_presentation_override(&json!({ "mode": "raw" })),
        Ok(ColumnPresentationOverride::Raw)
    );
    assert_eq!(
        validate_column_presentation_override(&json!({ "mode": "raw", "options": {} })),
        Err(vec![problem("options", "unknown field")])
    );
}

#[test]
fn audit_validate_override_rejects_bad_renderer_ids_verbatim() {
    // 非字符串。
    assert_eq!(
        validate_column_presentation_override(
            &json!({ "mode": "custom", "rendererId": 42, "options": valid_options_json() })
        ),
        Err(vec![problem("rendererId", "expected a string")])
    );
    // trim 后为空。
    assert_eq!(
        validate_column_presentation_override(
            &json!({ "mode": "custom", "rendererId": "   ", "options": valid_options_json() })
        ),
        Err(vec![problem("rendererId", "must not be empty")])
    );
    // 名内空白（空格与制表符）非法；首尾空白 trim 后合法。
    assert_eq!(
        validate_column_presentation_override(
            &json!({ "mode": "custom", "rendererId": "datazen .timestamp", "options": valid_options_json() })
        ),
        Err(vec![problem("rendererId", "must not contain whitespace")])
    );
    assert_eq!(
        validate_column_presentation_override(
            &json!({ "mode": "custom", "rendererId": " datazen.timestamp ", "options": valid_options_json() })
        ),
        Ok(ColumnPresentationOverride::Custom {
            renderer_id: "datazen.timestamp".to_owned(),
            options: valid_options_json(),
        })
    );
    // 未知 renderer：只报这一条，不再校验 options（与 TS 的分支一致）。
    assert_eq!(
        validate_column_presentation_override(
            &json!({ "mode": "custom", "rendererId": "other.renderer", "options": { "bogus": true } })
        ),
        Err(vec![problem("rendererId", "unknown renderer")])
    );
}

#[test]
fn audit_validate_override_missing_options_reports_trailing_dot_path() {
    // TS：validateTimestampOptions(undefined) 报根问题 path ''，
    // 前缀 "options." 拼出后恰为 "options."（尾点）——原样保留。
    assert_eq!(
        validate_column_presentation_override(
            &json!({ "mode": "custom", "rendererId": "datazen.timestamp" })
        ),
        Err(vec![problem("options.", "expected an object")])
    );
}

#[test]
fn audit_validate_override_prefixes_option_problems() {
    assert_eq!(
        validate_column_presentation_override(&json!({
            "mode": "custom",
            "rendererId": "datazen.timestamp",
            "options": { "format": "rfc3339" },
        })),
        Err(vec![
            problem("options.interpretation", "expected an object"),
            problem(
                "options.format",
                "expected \"iso\", \"iso-tz\" or \"locale\""
            ),
            problem("options.displayTimeZone", "expected an object"),
        ])
    );
}

#[test]
fn audit_validate_override_rejects_unknown_top_level_fields_on_custom() {
    assert_eq!(
        validate_column_presentation_override(&json!({
            "mode": "custom",
            "rendererId": "datazen.timestamp",
            "options": valid_options_json(),
            "surprise": 1,
        })),
        Err(vec![problem("surprise", "unknown field")])
    );
}

#[test]
fn audit_validate_override_bad_renderer_id_skips_options_validation() {
    // rendererId 非法时 options 不再校验：只有 rendererId 这一条问题，
    // 即使 options 本身也是坏的（与 TS 的 else-if 分支一致）。
    assert_eq!(
        validate_column_presentation_override(&json!({
            "mode": "custom",
            "rendererId": "bad id",
            "options": { "bogus": true },
        })),
        Err(vec![problem("rendererId", "must not contain whitespace")])
    );
}

#[test]
fn audit_validate_override_success_normalizes_options() {
    let expected = ColumnPresentationOverride::Custom {
        renderer_id: "datazen.timestamp".to_owned(),
        options: valid_options_json(),
    };
    assert_eq!(
        validate_column_presentation_override(&json!({
            "mode": "custom",
            "rendererId": "datazen.timestamp",
            "options": valid_options_json(),
        })),
        Ok(expected.clone())
    );
    assert_eq!(
        expected.to_json(),
        json!({
            "mode": "custom",
            "rendererId": "datazen.timestamp",
            "options": valid_options_json(),
        })
    );
}

// ---------------------------------------------------------------------------
// draft_to_timestamp_options / draft_to_column_override：结构精确相等
// ---------------------------------------------------------------------------

#[test]
fn audit_draft_options_builds_the_exact_struct() {
    let mut d = draft();
    d.interpretation = InterpretationChoice::Text;
    d.format = TimestampOutputFormat::Locale;
    d.display_time_zone_kind = TimeZoneKindChoice::Iana;
    d.display_iana_name = "  Europe/London ".to_owned();
    d.source_time_zone_kind = SourceTimeZoneChoice::Iana;
    d.source_iana_name = "\tAsia/Shanghai\n".to_owned();
    assert_eq!(
        draft_to_timestamp_options(&d),
        TimestampOptions {
            interpretation: TimestampInterpretation::Text,
            format: TimestampOutputFormat::Locale,
            display_time_zone: TimeZoneSpec::Iana {
                name: "Europe/London".to_owned()
            },
            source_time_zone: Some(TimeZoneSpec::Iana {
                name: "Asia/Shanghai".to_owned()
            }),
        }
    );
    // to_json 含全部四键且源时区已 trim。
    assert_eq!(
        draft_to_timestamp_options(&d).to_json(),
        json!({
            "interpretation": { "kind": "text" },
            "format": "locale",
            "displayTimeZone": { "kind": "iana", "name": "Europe/London" },
            "sourceTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
        })
    );
}

#[test]
fn audit_draft_options_unix_seconds_drops_source_even_when_iana() {
    let mut d = draft();
    d.interpretation = InterpretationChoice::UnixSeconds;
    d.format = TimestampOutputFormat::Iso;
    d.display_time_zone_kind = TimeZoneKindChoice::Utc;
    d.source_time_zone_kind = SourceTimeZoneChoice::Iana;
    d.source_iana_name = "Europe/London".to_owned();
    let options = draft_to_timestamp_options(&d);
    assert_eq!(options.source_time_zone, None);
    assert_eq!(
        options.to_json(),
        json!({
            "interpretation": { "kind": "unix", "unit": "seconds" },
            "format": "iso",
            "displayTimeZone": { "kind": "utc" },
        })
    );
}

#[test]
fn audit_draft_override_custom_carries_trimmed_options_json() {
    let mut d = draft();
    d.interpretation = InterpretationChoice::UnixMilliseconds;
    d.display_time_zone_kind = TimeZoneKindChoice::Iana;
    d.display_iana_name = "Europe/London".to_owned();
    assert_eq!(
        draft_to_column_override(&d).to_json(),
        json!({
            "mode": "custom",
            "rendererId": "datazen.timestamp",
            "options": {
                "interpretation": { "kind": "unix", "unit": "milliseconds" },
                "format": "iso-tz",
                "displayTimeZone": { "kind": "iana", "name": "Europe/London" },
            },
        })
    );
}

// ---------------------------------------------------------------------------
// validate_column_presentation_draft：谓词接线口
// ---------------------------------------------------------------------------

#[test]
fn audit_draft_validation_predicate_receives_the_trimmed_name() {
    let seen: RefCell<Vec<String>> = RefCell::new(Vec::new());
    let mut d = draft();
    d.display_time_zone_kind = TimeZoneKindChoice::Iana;
    d.display_iana_name = "  Europe/London  ".to_owned();
    d.source_time_zone_kind = SourceTimeZoneChoice::Iana;
    d.source_iana_name = "\tAsia/Shanghai\n".to_owned();
    let problems = validate_column_presentation_draft(&d, &|name: &str| {
        seen.borrow_mut().push(name.to_owned());
        known(name)
    });
    assert!(problems.is_empty());
    // 谓词收到的是 trim 后的名（带空白的原始输入不透传）。
    assert_eq!(seen.borrow().as_slice(), ["Europe/London", "Asia/Shanghai"]);
}

#[test]
fn audit_draft_validation_predicate_decides_knownness_not_a_hardcoded_list() {
    // 谓词认 Mars/Olympus：TS 里 Intl 认的名在这里同样放行——已知性
    // 完全由调用方谓词决定，本模块不内置名单。
    let mut d = draft();
    d.display_time_zone_kind = TimeZoneKindChoice::Iana;
    d.display_iana_name = "Mars/Olympus".to_owned();
    assert!(
        validate_column_presentation_draft(&d, &|_: &str| true).is_empty(),
        "a predicate that knows the name must pass it"
    );
    assert_eq!(
        validate_column_presentation_draft(&d, &|_: &str| false),
        vec![problem("displayTimeZone.name", "unknown time zone name")]
    );
}

#[test]
fn audit_draft_validation_predicate_not_called_when_not_needed() {
    let calls = Cell::new(0);
    let count = |_: &str| {
        calls.set(calls.get() + 1);
        true
    };
    // 非 custom：一次都不调。
    let mut d = draft();
    d.mode = PresentationMode::Inherit;
    d.display_time_zone_kind = TimeZoneKindChoice::Iana;
    d.display_iana_name = "Europe/London".to_owned();
    assert!(validate_column_presentation_draft(&d, &count).is_empty());
    assert_eq!(calls.get(), 0);
    // 非 iana 种：不调。
    let mut d = draft();
    d.display_time_zone_kind = TimeZoneKindChoice::Utc;
    assert!(validate_column_presentation_draft(&d, &count).is_empty());
    assert_eq!(calls.get(), 0);
    // 格式非法：不调已知性（先判格式，格式不过不查已知性）。
    let mut d = draft();
    d.display_time_zone_kind = TimeZoneKindChoice::Iana;
    d.display_iana_name = "Bad Name".to_owned();
    assert_eq!(
        validate_column_presentation_draft(&d, &count),
        vec![problem(
            "displayTimeZone.name",
            "expected a valid IANA time zone name"
        )]
    );
    assert_eq!(calls.get(), 0);
}

#[test]
fn audit_draft_validation_reports_both_unknown_zones_in_order() {
    let mut d = draft();
    d.display_time_zone_kind = TimeZoneKindChoice::Iana;
    d.display_iana_name = "Mars/Olympus".to_owned();
    d.source_time_zone_kind = SourceTimeZoneChoice::Iana;
    d.source_iana_name = "Venus/Aphrodite".to_owned();
    assert_eq!(
        validate_column_presentation_draft(&d, &known),
        vec![
            problem("displayTimeZone.name", "unknown time zone name"),
            problem("sourceTimeZone.name", "unknown time zone name"),
        ]
    );
}

#[test]
fn audit_draft_validation_source_zone_ignored_for_unix_even_when_unknown() {
    // unix 下源时区根本不检查：坏名 + 未知名都不报错，且不调用谓词。
    let calls = Cell::new(0);
    let mut d = draft();
    d.interpretation = InterpretationChoice::UnixSeconds;
    d.source_time_zone_kind = SourceTimeZoneChoice::Iana;
    d.source_iana_name = "Not A Zone".to_owned();
    assert!(validate_column_presentation_draft(&d, &|_: &str| {
        calls.set(calls.get() + 1);
        false
    })
    .is_empty());
    assert_eq!(calls.get(), 0);
}

// ---------------------------------------------------------------------------
// draft_from_column_override：默认值、还原、回落
// ---------------------------------------------------------------------------

#[test]
fn audit_draft_from_override_base_defaults_are_exact() {
    for kind in [
        PresentationSemanticKind::Instant,
        PresentationSemanticKind::LocalDatetime,
        PresentationSemanticKind::Date,
        PresentationSemanticKind::Time,
        PresentationSemanticKind::Text,
        PresentationSemanticKind::Other,
    ] {
        let draft = draft_from_column_override(&ColumnPresentationOverride::Inherit, None, kind);
        assert_eq!(
            draft,
            ColumnPresentationDraft {
                mode: PresentationMode::Inherit,
                interpretation: InterpretationChoice::Native,
                format: TimestampOutputFormat::IsoTz,
                display_time_zone_kind: TimeZoneKindChoice::Local,
                display_iana_name: "Asia/Shanghai".to_owned(),
                source_time_zone_kind: SourceTimeZoneChoice::None,
                source_iana_name: "Asia/Shanghai".to_owned(),
            },
            "kind {kind:?}"
        );
    }
    // 只有 number 默认 unix-milliseconds。
    let draft = draft_from_column_override(
        &ColumnPresentationOverride::Raw,
        None,
        PresentationSemanticKind::Number,
    );
    assert_eq!(draft.mode, PresentationMode::Raw);
    assert_eq!(draft.interpretation, InterpretationChoice::UnixMilliseconds);
}

#[test]
fn audit_draft_from_override_global_options_drive_base() {
    let global = TimestampOptions {
        interpretation: TimestampInterpretation::Text,
        format: TimestampOutputFormat::Iso,
        display_time_zone: TimeZoneSpec::Local,
        source_time_zone: Some(TimeZoneSpec::Utc),
    };
    // 全局的 interpretation 与 sourceTimeZone 不进入 base（只有
    // format 与 displayTimeZone 被取用；源时区恒为 none）。
    let draft = draft_from_column_override(
        &ColumnPresentationOverride::Inherit,
        Some(&global),
        PresentationSemanticKind::Text,
    );
    assert_eq!(draft.format, TimestampOutputFormat::Iso);
    assert_eq!(draft.display_time_zone_kind, TimeZoneKindChoice::Local);
    assert_eq!(draft.display_iana_name, "Asia/Shanghai");
    assert_eq!(draft.source_time_zone_kind, SourceTimeZoneChoice::None);
}

#[test]
fn audit_draft_from_override_ignores_renderer_id_like_typescript() {
    // TS 的 draftFromColumnOverride 只校验 current.options，不看
    // rendererId：rendererId 是别的展示器、但 options 合法时同样还原。
    let current = ColumnPresentationOverride::Custom {
        renderer_id: "other.renderer".to_owned(),
        options: json!({
            "interpretation": { "kind": "text" },
            "format": "locale",
            "displayTimeZone": { "kind": "iana", "name": "Europe/London" },
            "sourceTimeZone": { "kind": "utc" },
        }),
    };
    let draft = draft_from_column_override(&current, None, PresentationSemanticKind::Text);
    assert_eq!(draft.mode, PresentationMode::Custom);
    assert_eq!(draft.interpretation, InterpretationChoice::Text);
    assert_eq!(draft.format, TimestampOutputFormat::Locale);
    assert_eq!(draft.display_time_zone_kind, TimeZoneKindChoice::Iana);
    assert_eq!(draft.display_iana_name, "Europe/London");
    assert_eq!(draft.source_time_zone_kind, SourceTimeZoneChoice::Utc);
}

#[test]
fn audit_draft_from_override_invalid_options_falls_back_to_base_plus_custom() {
    let global = TimestampOptions {
        interpretation: TimestampInterpretation::Native,
        format: TimestampOutputFormat::Locale,
        display_time_zone: TimeZoneSpec::Iana {
            name: "Europe/London".to_owned(),
        },
        source_time_zone: None,
    };
    let current = ColumnPresentationOverride::Custom {
        renderer_id: "datazen.timestamp".to_owned(),
        options: json!({ "format": "rfc3339" }),
    };
    let draft =
        draft_from_column_override(&current, Some(&global), PresentationSemanticKind::Number);
    // 回落：base（number → unix-milliseconds，全局 format/locale 与
    // IANA 展示区）+ mode custom。
    assert_eq!(
        draft,
        ColumnPresentationDraft {
            mode: PresentationMode::Custom,
            interpretation: InterpretationChoice::UnixMilliseconds,
            format: TimestampOutputFormat::Locale,
            display_time_zone_kind: TimeZoneKindChoice::Iana,
            display_iana_name: "Europe/London".to_owned(),
            source_time_zone_kind: SourceTimeZoneChoice::None,
            source_iana_name: "Asia/Shanghai".to_owned(),
        }
    );
}

#[test]
fn audit_draft_from_override_restores_iana_source_zone() {
    let current = ColumnPresentationOverride::Custom {
        renderer_id: "datazen.timestamp".to_owned(),
        options: json!({
            "interpretation": { "kind": "text" },
            "format": "iso-tz",
            "displayTimeZone": { "kind": "local" },
            "sourceTimeZone": { "kind": "iana", "name": "Europe/London" },
        }),
    };
    let draft = draft_from_column_override(&current, None, PresentationSemanticKind::Text);
    assert_eq!(draft.source_time_zone_kind, SourceTimeZoneChoice::Iana);
    assert_eq!(draft.source_iana_name, "Europe/London");
    // 校验侧对该还原草稿放行（IANA 名已知）。
    assert!(validate_column_presentation_draft(&draft, &known).is_empty());
}

// ---------------------------------------------------------------------------
// global_timestamp_options_from_snapshot
// ---------------------------------------------------------------------------

#[test]
fn audit_global_options_from_snapshot_variants() {
    // 缺键 → None。
    assert_eq!(
        global_timestamp_options_from_snapshot(&ValuePresentationSnapshot::default()),
        None
    );
    // 非法配置 → None（不是 Err 透传）。
    assert_eq!(
        global_timestamp_options_from_snapshot(&snapshot_with(json!({ "format": "nope" }))),
        None
    );
    // 合法配置 → Some，且为规范值（源时区缺省时不补）。
    assert_eq!(
        global_timestamp_options_from_snapshot(&snapshot_with(valid_options_json())),
        Some(TimestampOptions {
            interpretation: TimestampInterpretation::Unix(UnixUnit::Milliseconds),
            format: TimestampOutputFormat::IsoTz,
            display_time_zone: TimeZoneSpec::Local,
            source_time_zone: None,
        })
    );
    // 别的 contribution key 不影响。
    let mut snapshot = ValuePresentationSnapshot::default();
    snapshot.configs.insert(
        "other.renderer".to_owned(),
        VersionedPresentationConfig {
            config_version: 1,
            config: valid_options_json(),
        },
    );
    assert_eq!(global_timestamp_options_from_snapshot(&snapshot), None);
}

// ---------------------------------------------------------------------------
// 守卫与 from_str：大小写与空白敏感
// ---------------------------------------------------------------------------

#[test]
fn audit_guards_reject_case_and_whitespace_variants() {
    for value in ["Native", "NATIVE", "native ", " native", "", "unix_seconds"] {
        assert!(!is_interpretation_choice(value), "{value:?}");
    }
    for value in ["ISO-TZ", "iso_tz", "iso-tz ", ""] {
        assert!(!is_output_format(value), "{value:?}");
    }
    for value in ["Local", "LOCAL", "local ", ""] {
        assert!(!is_time_zone_kind_choice(value), "{value:?}");
    }
    for value in ["None", "NONE", "none ", ""] {
        assert!(!is_source_time_zone_choice(value), "{value:?}");
    }
    assert!(is_interpretation_choice("unix-milliseconds"));
    assert!(is_output_format("locale"));
    assert!(is_time_zone_kind_choice("iana"));
    assert!(is_source_time_zone_choice("none"));
}

#[test]
fn audit_choice_from_str_round_trips_every_variant() {
    assert_eq!(
        InterpretationChoice::from_str("text"),
        Some(InterpretationChoice::Text)
    );
    assert_eq!(InterpretationChoice::from_str("bogus"), None);
    assert_eq!(
        TimeZoneKindChoice::from_str("utc"),
        Some(TimeZoneKindChoice::Utc)
    );
    assert_eq!(TimeZoneKindChoice::from_str("bogus"), None);
    assert_eq!(
        SourceTimeZoneChoice::from_str("iana"),
        Some(SourceTimeZoneChoice::Iana)
    );
    assert_eq!(SourceTimeZoneChoice::from_str("bogus"), None);
    assert_eq!(
        TimestampOutputFormat::from_str("iso"),
        Some(TimestampOutputFormat::Iso)
    );
    assert_eq!(TimestampOutputFormat::from_str("bogus"), None);
}

// ---------------------------------------------------------------------------
// 往返：draft → override → draft 稳定（独立取值）
// ---------------------------------------------------------------------------

#[test]
fn audit_round_trip_text_with_iana_zones_is_stable() {
    let mut d = draft();
    d.interpretation = InterpretationChoice::Text;
    d.format = TimestampOutputFormat::Locale;
    d.display_time_zone_kind = TimeZoneKindChoice::Iana;
    d.display_iana_name = "Europe/London".to_owned();
    d.source_time_zone_kind = SourceTimeZoneChoice::Iana;
    d.source_iana_name = "Asia/Shanghai".to_owned();
    let override_ = draft_to_column_override(&d);
    let back = draft_from_column_override(&override_, None, PresentationSemanticKind::Text);
    assert_eq!(back, d);
    // 该草稿校验通过。
    assert!(validate_column_presentation_draft(&back, &known).is_empty());
}

#[test]
fn audit_round_trip_unix_seconds_is_stable() {
    let mut d = draft();
    d.interpretation = InterpretationChoice::UnixSeconds;
    d.format = TimestampOutputFormat::Iso;
    d.display_time_zone_kind = TimeZoneKindChoice::Utc;
    // unix 下源时区填什么都会被丢弃：往返后恒为 none。
    d.source_time_zone_kind = SourceTimeZoneChoice::Iana;
    d.source_iana_name = "Asia/Shanghai".to_owned();
    let override_ = draft_to_column_override(&d);
    let back = draft_from_column_override(&override_, None, PresentationSemanticKind::Number);
    assert_eq!(back.interpretation, InterpretationChoice::UnixSeconds);
    assert_eq!(back.format, TimestampOutputFormat::Iso);
    assert_eq!(back.display_time_zone_kind, TimeZoneKindChoice::Utc);
    assert_eq!(back.source_time_zone_kind, SourceTimeZoneChoice::None);
}
