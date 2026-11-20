//! T054-S1: `ColumnPresentationDialog` 草稿模型 + 纯函数的 coder 测试。
//!
//! 只测本片的纯函数（草稿 ↔ 配置映射、两层 JSON 校验、草稿校验与还原、
//! 全局默认、守卫），不碰 S2 的对话框组件 / 状态 / 渲染。已知性检查
//! （TS 的 Intl）用桩谓词代替——本 crate 无时区数据库，
//! `validate_column_presentation_draft` 的 `is_known_time_zone` 参数
//! 就是留给宿主接线的口，见模块头「依赖盘点」。

use serde_json::{json, Value};

use crate::components::data_table::column_presentation_dialog::{
    draft_from_column_override, draft_to_column_override, draft_to_timestamp_options,
    global_timestamp_options_from_snapshot, is_interpretation_choice, is_output_format,
    is_source_time_zone_choice, is_time_zone_kind_choice, is_valid_iana_time_zone_name,
    validate_column_presentation_draft, validate_column_presentation_override,
    validate_timestamp_options, ColumnPresentationDraft, ColumnPresentationOverride,
    InterpretationChoice, PresentationMode, PresentationSemanticKind, SourceTimeZoneChoice,
    TimeZoneKindChoice, TimeZoneSpec, TimestampInterpretation, TimestampOptions,
    TimestampOutputFormat, UnixUnit, ValidationProblem, ValuePresentationSnapshot,
    VersionedPresentationConfig, TIMESTAMP_CONTRIBUTION_ID,
};

/// 桩已知性：测试里只认这三个名，其余一律未知。
fn known(name: &str) -> bool {
    matches!(name, "UTC" | "Asia/Shanghai" | "Europe/London")
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

fn paths(problems: &[ValidationProblem]) -> Vec<&str> {
    problems.iter().map(|p| p.path.as_str()).collect()
}

fn messages(problems: &[ValidationProblem]) -> Vec<&str> {
    problems.iter().map(|p| p.message.as_str()).collect()
}

fn valid_options_json() -> Value {
    json!({
        "interpretation": { "kind": "unix", "unit": "milliseconds" },
        "format": "iso-tz",
        "displayTimeZone": { "kind": "local" },
    })
}

fn snapshot_with(config: Option<Value>) -> ValuePresentationSnapshot {
    let mut snapshot = ValuePresentationSnapshot::default();
    if let Some(config) = config {
        snapshot.configs.insert(
            TIMESTAMP_CONTRIBUTION_ID.to_owned(),
            VersionedPresentationConfig {
                config_version: 1,
                config,
            },
        );
    }
    snapshot
}

// ---------------------------------------------------------------------------
// is_valid_iana_time_zone_name（格式镜像）
// ---------------------------------------------------------------------------

#[test]
fn iana_name_accepts_well_formed_names() {
    assert!(is_valid_iana_time_zone_name("UTC"));
    assert!(is_valid_iana_time_zone_name("Asia/Shanghai"));
    assert!(is_valid_iana_time_zone_name("America/New_York"));
    assert!(is_valid_iana_time_zone_name("Etc/GMT+5"));
    assert!(is_valid_iana_time_zone_name("a"));
}

#[test]
fn iana_name_rejects_empty_whitespace_and_illegal_chars() {
    assert!(!is_valid_iana_time_zone_name(""));
    // 名内空白非法（T07 裁决）；本函数不 trim，空白由调用处先处理。
    assert!(!is_valid_iana_time_zone_name("Asia/Shang hai"));
    assert!(!is_valid_iana_time_zone_name(" Asia/Shanghai"));
    assert!(!is_valid_iana_time_zone_name("Asia/Shanghai "));
    assert!(!is_valid_iana_time_zone_name("Asia/Shanghai!"));
    assert!(!is_valid_iana_time_zone_name("Asia/Shanghai?x=1"));
}

#[test]
fn iana_name_rejects_over_100_chars() {
    assert!(!is_valid_iana_time_zone_name(&"a".repeat(101)));
    assert!(is_valid_iana_time_zone_name(&"a".repeat(100)));
}

// ---------------------------------------------------------------------------
// draft_to_timestamp_options
// ---------------------------------------------------------------------------

#[test]
fn draft_options_maps_all_four_interpretations() {
    let mut d = draft();
    d.interpretation = InterpretationChoice::Native;
    assert_eq!(
        draft_to_timestamp_options(&d).interpretation,
        TimestampInterpretation::Native
    );
    d.interpretation = InterpretationChoice::Text;
    assert_eq!(
        draft_to_timestamp_options(&d).interpretation,
        TimestampInterpretation::Text
    );
    d.interpretation = InterpretationChoice::UnixSeconds;
    assert_eq!(
        draft_to_timestamp_options(&d).interpretation,
        TimestampInterpretation::Unix(UnixUnit::Seconds)
    );
    d.interpretation = InterpretationChoice::UnixMilliseconds;
    assert_eq!(
        draft_to_timestamp_options(&d).interpretation,
        TimestampInterpretation::Unix(UnixUnit::Milliseconds)
    );
}

#[test]
fn draft_options_drops_source_time_zone_for_unix() {
    let mut d = draft();
    d.interpretation = InterpretationChoice::UnixSeconds;
    d.source_time_zone_kind = SourceTimeZoneChoice::Iana;
    d.source_iana_name = "Asia/Shanghai".to_owned();
    assert_eq!(draft_to_timestamp_options(&d).source_time_zone, None);
    d.interpretation = InterpretationChoice::UnixMilliseconds;
    d.source_time_zone_kind = SourceTimeZoneChoice::Local;
    assert_eq!(draft_to_timestamp_options(&d).source_time_zone, None);
}

#[test]
fn draft_options_keeps_source_time_zone_for_text_and_native() {
    let mut d = draft();
    d.interpretation = InterpretationChoice::Text;
    d.source_time_zone_kind = SourceTimeZoneChoice::Iana;
    d.source_iana_name = "  Asia/Shanghai  ".to_owned();
    let options = draft_to_timestamp_options(&d);
    assert_eq!(
        options.source_time_zone,
        Some(TimeZoneSpec::Iana {
            name: "Asia/Shanghai".to_owned()
        })
    );
    d.interpretation = InterpretationChoice::Native;
    d.source_time_zone_kind = SourceTimeZoneChoice::Utc;
    assert_eq!(
        draft_to_timestamp_options(&d).source_time_zone,
        Some(TimeZoneSpec::Utc)
    );
    d.source_time_zone_kind = SourceTimeZoneChoice::Local;
    assert_eq!(
        draft_to_timestamp_options(&d).source_time_zone,
        Some(TimeZoneSpec::Local)
    );
    d.source_time_zone_kind = SourceTimeZoneChoice::None;
    assert_eq!(draft_to_timestamp_options(&d).source_time_zone, None);
}

#[test]
fn draft_options_trims_display_iana_name() {
    let mut d = draft();
    d.display_time_zone_kind = TimeZoneKindChoice::Iana;
    d.display_iana_name = "  Europe/London\t".to_owned();
    let options = draft_to_timestamp_options(&d);
    assert_eq!(
        options.display_time_zone,
        TimeZoneSpec::Iana {
            name: "Europe/London".to_owned()
        }
    );
    d.display_time_zone_kind = TimeZoneKindChoice::Utc;
    assert_eq!(
        draft_to_timestamp_options(&d).display_time_zone,
        TimeZoneSpec::Utc
    );
}

#[test]
fn draft_options_json_omits_source_time_zone_when_absent() {
    let d = draft();
    let json = draft_to_timestamp_options(&d).to_json();
    assert!(json.get("sourceTimeZone").is_none());
    assert_eq!(json["format"], "iso-tz");
    assert_eq!(json["displayTimeZone"]["kind"], "local");
    assert_eq!(json["interpretation"]["kind"], "native");
}

// ---------------------------------------------------------------------------
// draft_to_column_override
// ---------------------------------------------------------------------------

#[test]
fn draft_override_inherit_and_raw_carry_no_options() {
    let mut d = draft();
    d.mode = PresentationMode::Inherit;
    assert_eq!(
        draft_to_column_override(&d),
        ColumnPresentationOverride::Inherit
    );
    d.mode = PresentationMode::Raw;
    assert_eq!(
        draft_to_column_override(&d),
        ColumnPresentationOverride::Raw
    );
}

#[test]
fn override_to_json_covers_inherit_and_raw() {
    assert_eq!(
        ColumnPresentationOverride::Inherit.to_json(),
        json!({ "mode": "inherit" })
    );
    assert_eq!(
        ColumnPresentationOverride::Raw.to_json(),
        json!({ "mode": "raw" })
    );
}

#[test]
fn draft_override_custom_uses_timestamp_renderer() {
    let mut d = draft();
    d.interpretation = InterpretationChoice::UnixMilliseconds;
    let override_ = draft_to_column_override(&d);
    assert_eq!(
        override_.to_json(),
        json!({
            "mode": "custom",
            "rendererId": "datazen.timestamp",
            "options": {
                "interpretation": { "kind": "unix", "unit": "milliseconds" },
                "format": "iso-tz",
                "displayTimeZone": { "kind": "local" },
            },
        })
    );
}

// ---------------------------------------------------------------------------
// validate_timestamp_options（JSON 镜像）
// ---------------------------------------------------------------------------

#[test]
fn validate_options_accepts_a_full_valid_config() {
    let validated = validate_timestamp_options(&json!({
        "interpretation": { "kind": "text" },
        "format": "locale",
        "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
        "sourceTimeZone": { "kind": "utc" },
    }));
    assert_eq!(
        validated,
        Ok(TimestampOptions {
            interpretation: TimestampInterpretation::Text,
            format: TimestampOutputFormat::Locale,
            display_time_zone: TimeZoneSpec::Iana {
                name: "Asia/Shanghai".to_owned()
            },
            source_time_zone: Some(TimeZoneSpec::Utc),
        })
    );
}

#[test]
fn validate_options_rejects_non_objects() {
    for input in [
        Value::Null,
        json!("iso-tz"),
        json!(42),
        json!([1, 2]),
        json!(true),
    ] {
        assert_eq!(
            validate_timestamp_options(&input),
            Err(vec![ValidationProblem {
                path: String::new(),
                message: "expected an object".to_owned(),
            }]),
            "input: {input}"
        );
    }
}

#[test]
fn validate_options_reports_missing_fields() {
    let problems = validate_timestamp_options(&json!({ "format": "iso" })).unwrap_err();
    assert!(paths(&problems).contains(&"interpretation"));
    assert!(paths(&problems).contains(&"displayTimeZone"));
    assert!(messages(&problems)
        .iter()
        .all(|m| *m == "expected an object"));
}

#[test]
fn validate_options_rejects_a_non_object_interpretation() {
    let problems = validate_timestamp_options(&json!({
        "interpretation": "native",
        "format": "iso",
        "displayTimeZone": { "kind": "local" },
    }))
    .unwrap_err();
    assert_eq!(paths(&problems), ["interpretation"]);
    assert_eq!(messages(&problems), ["expected an object"]);
}

#[test]
fn validate_options_rejects_bad_format_and_interpretation_kind() {
    let problems = validate_timestamp_options(&json!({
        "interpretation": { "kind": "epoch" },
        "format": "rfc3339",
        "displayTimeZone": { "kind": "local" },
    }))
    .unwrap_err();
    assert_eq!(paths(&problems), ["interpretation.kind", "format"]);
    assert_eq!(
        messages(&problems),
        [
            "expected \"native\", \"unix\" or \"text\"",
            "expected \"iso\", \"iso-tz\" or \"locale\"",
        ]
    );
}

#[test]
fn validate_options_rejects_unix_without_a_valid_unit() {
    for unit in [Value::Null, json!("nanos"), json!(42)] {
        let problems = validate_timestamp_options(&json!({
            "interpretation": { "kind": "unix", "unit": unit },
            "format": "iso",
            "displayTimeZone": { "kind": "local" },
        }))
        .unwrap_err();
        assert_eq!(paths(&problems), ["interpretation.unit"]);
        assert_eq!(
            messages(&problems),
            ["expected \"seconds\" or \"milliseconds\""]
        );
    }
    // `unit` 缺省与非法值同报。
    let problems = validate_timestamp_options(&json!({
        "interpretation": { "kind": "unix" },
        "format": "iso",
        "displayTimeZone": { "kind": "local" },
    }))
    .unwrap_err();
    assert_eq!(paths(&problems), ["interpretation.unit"]);
}

#[test]
fn validate_options_rejects_unit_on_native_and_text() {
    for kind in ["native", "text"] {
        let problems = validate_timestamp_options(&json!({
            "interpretation": { "kind": kind, "unit": "seconds" },
            "format": "iso",
            "displayTimeZone": { "kind": "local" },
        }))
        .unwrap_err();
        assert_eq!(paths(&problems), ["interpretation.unit"]);
        assert_eq!(
            messages(&problems),
            [format!(
                "unknown field (\"{kind}\" interpretation has no unit)"
            )]
        );
    }
}

#[test]
fn validate_options_rejects_bad_time_zone_specs() {
    // kind 非法。
    let problems = validate_timestamp_options(&json!({
        "interpretation": { "kind": "native" },
        "format": "iso",
        "displayTimeZone": { "kind": "zone" },
    }))
    .unwrap_err();
    assert_eq!(paths(&problems), ["displayTimeZone.kind"]);
    // iana 名含空白（T07 裁决）。
    let problems = validate_timestamp_options(&json!({
        "interpretation": { "kind": "native" },
        "format": "iso",
        "displayTimeZone": { "kind": "iana", "name": "Bad Name" },
    }))
    .unwrap_err();
    assert_eq!(paths(&problems), ["displayTimeZone.name"]);
    assert_eq!(
        messages(&problems),
        ["expected a non-empty IANA time zone name without whitespace"]
    );
    // iana 名为空。
    let problems = validate_timestamp_options(&json!({
        "interpretation": { "kind": "native" },
        "format": "iso",
        "displayTimeZone": { "kind": "iana", "name": "" },
    }))
    .unwrap_err();
    assert_eq!(paths(&problems), ["displayTimeZone.name"]);
    // 非 iana 却带 name。
    let problems = validate_timestamp_options(&json!({
        "interpretation": { "kind": "native" },
        "format": "iso",
        "displayTimeZone": { "kind": "local", "name": "UTC" },
    }))
    .unwrap_err();
    assert_eq!(paths(&problems), ["displayTimeZone.name"]);
    assert_eq!(
        messages(&problems),
        ["unknown field (only \"iana\" carries a name)"]
    );
    // 时区规约不是对象。
    let problems = validate_timestamp_options(&json!({
        "interpretation": { "kind": "native" },
        "format": "iso",
        "displayTimeZone": "local",
    }))
    .unwrap_err();
    assert_eq!(paths(&problems), ["displayTimeZone"]);
}

#[test]
fn validate_options_rejects_unknown_fields_everywhere() {
    let problems = validate_timestamp_options(&json!({
        "interpretation": { "kind": "native", "zone": "x" },
        "format": "iso",
        "displayTimeZone": { "kind": "local", "extra": 1 },
        "surprise": true,
    }))
    .unwrap_err();
    assert!(paths(&problems).contains(&"interpretation.zone"));
    assert!(paths(&problems).contains(&"displayTimeZone.extra"));
    assert!(paths(&problems).contains(&"surprise"));
    assert!(messages(&problems).iter().all(|m| *m == "unknown field"));
}

#[test]
fn validate_options_rejects_source_time_zone_on_unix() {
    let problems = validate_timestamp_options(&json!({
        "interpretation": { "kind": "unix", "unit": "seconds" },
        "format": "iso",
        "displayTimeZone": { "kind": "local" },
        "sourceTimeZone": { "kind": "utc" },
    }))
    .unwrap_err();
    assert_eq!(paths(&problems), ["sourceTimeZone"]);
    assert_eq!(
        messages(&problems),
        ["unix interpretation must not carry a source time zone"]
    );
}

#[test]
fn validate_options_collects_all_problems_without_stopping_early() {
    let problems = validate_timestamp_options(&json!({
        "interpretation": { "kind": "epoch" },
        "format": "nope",
        "displayTimeZone": { "kind": "zone" },
        "extra": 1,
    }))
    .unwrap_err();
    assert_eq!(problems.len(), 4);
}

// ---------------------------------------------------------------------------
// validate_column_presentation_override（JSON 镜像）
// ---------------------------------------------------------------------------

#[test]
fn validate_override_accepts_inherit_and_raw() {
    assert_eq!(
        validate_column_presentation_override(&json!({ "mode": "inherit" })),
        Ok(ColumnPresentationOverride::Inherit)
    );
    assert_eq!(
        validate_column_presentation_override(&json!({ "mode": "raw" })),
        Ok(ColumnPresentationOverride::Raw)
    );
}

#[test]
fn validate_override_rejects_non_objects_and_bad_modes() {
    assert_eq!(
        validate_column_presentation_override(&json!("inherit")),
        Err(vec![ValidationProblem {
            path: String::new(),
            message: "expected an object".to_owned(),
        }])
    );
    for mode in [Value::Null, json!("weird"), json!(3)] {
        let problems = validate_column_presentation_override(&json!({ "mode": mode })).unwrap_err();
        assert_eq!(paths(&problems), ["mode"]);
        assert_eq!(
            messages(&problems),
            ["expected \"inherit\", \"raw\" or \"custom\""]
        );
    }
}

#[test]
fn validate_override_rejects_extra_fields_on_inherit_and_raw() {
    let problems = validate_column_presentation_override(&json!({
        "mode": "inherit",
        "rendererId": "datazen.timestamp",
    }))
    .unwrap_err();
    assert_eq!(paths(&problems), ["rendererId"]);
    assert_eq!(messages(&problems), ["unknown field"]);
}

#[test]
fn validate_override_accepts_a_valid_custom_override() {
    let validated = validate_column_presentation_override(&json!({
        "mode": "custom",
        "rendererId": " datazen.timestamp ",
        "options": valid_options_json(),
    }));
    assert_eq!(
        validated,
        Ok(ColumnPresentationOverride::Custom {
            renderer_id: "datazen.timestamp".to_owned(),
            options: valid_options_json(),
        })
    );
}

#[test]
fn validate_override_rejects_bad_renderer_ids() {
    // 非字符串。
    let problems = validate_column_presentation_override(&json!({
        "mode": "custom",
        "rendererId": 7,
        "options": valid_options_json(),
    }))
    .unwrap_err();
    assert_eq!(paths(&problems), ["rendererId"]);
    assert_eq!(messages(&problems), ["expected a string"]);
    // 空 / 仅空白。
    for id in ["", "   "] {
        let problems = validate_column_presentation_override(&json!({
            "mode": "custom",
            "rendererId": id,
            "options": valid_options_json(),
        }))
        .unwrap_err();
        assert_eq!(paths(&problems), ["rendererId"]);
        assert_eq!(messages(&problems), ["must not be empty"]);
    }
    // 名内空白非法（T07 裁决）。
    let problems = validate_column_presentation_override(&json!({
        "mode": "custom",
        "rendererId": "datazen .timestamp",
        "options": valid_options_json(),
    }))
    .unwrap_err();
    assert_eq!(paths(&problems), ["rendererId"]);
    assert_eq!(messages(&problems), ["must not contain whitespace"]);
    // 未知渲染器：不继续校验 options。
    let problems = validate_column_presentation_override(&json!({
        "mode": "custom",
        "rendererId": "other.clock",
        "options": { "format": "nope" },
    }))
    .unwrap_err();
    assert_eq!(paths(&problems), ["rendererId"]);
    assert_eq!(messages(&problems), ["unknown renderer"]);
}

#[test]
fn validate_override_prefixes_option_problems_with_options() {
    let problems = validate_column_presentation_override(&json!({
        "mode": "custom",
        "rendererId": "datazen.timestamp",
        "options": { "format": "nope" },
    }))
    .unwrap_err();
    assert!(paths(&problems).contains(&"options.format"));
    assert!(!paths(&problems).iter().any(|p| *p == "format"));
}

#[test]
fn validate_override_reports_a_missing_options_object() {
    let problems = validate_column_presentation_override(&json!({
        "mode": "custom",
        "rendererId": "datazen.timestamp",
    }))
    .unwrap_err();
    assert_eq!(paths(&problems), ["options."]);
    assert_eq!(messages(&problems), ["expected an object"]);
}

#[test]
fn validate_override_rejects_unknown_top_level_fields_on_custom() {
    let problems = validate_column_presentation_override(&json!({
        "mode": "custom",
        "rendererId": "datazen.timestamp",
        "options": valid_options_json(),
        "extra": 1,
    }))
    .unwrap_err();
    assert_eq!(paths(&problems), ["extra"]);
}

// ---------------------------------------------------------------------------
// validate_column_presentation_draft
// ---------------------------------------------------------------------------

#[test]
fn draft_validation_skips_non_custom_modes() {
    let mut d = draft();
    d.mode = PresentationMode::Inherit;
    d.display_time_zone_kind = TimeZoneKindChoice::Iana;
    d.display_iana_name = "Bad Name".to_owned();
    assert!(validate_column_presentation_draft(&d, &known).is_empty());
    d.mode = PresentationMode::Raw;
    assert!(validate_column_presentation_draft(&d, &known).is_empty());
}

#[test]
fn draft_validation_accepts_a_valid_custom_draft() {
    let mut d = draft();
    d.interpretation = InterpretationChoice::Text;
    d.display_time_zone_kind = TimeZoneKindChoice::Iana;
    d.display_iana_name = "  Asia/Shanghai  ".to_owned();
    d.source_time_zone_kind = SourceTimeZoneChoice::Iana;
    d.source_iana_name = "Europe/London".to_owned();
    assert!(validate_column_presentation_draft(&d, &known).is_empty());
}

#[test]
fn draft_validation_reports_a_malformed_display_iana_name() {
    let mut d = draft();
    d.display_time_zone_kind = TimeZoneKindChoice::Iana;
    d.display_iana_name = "Asia/Shang hai".to_owned();
    let problems = validate_column_presentation_draft(&d, &known);
    assert_eq!(paths(&problems), ["displayTimeZone.name"]);
    assert_eq!(
        messages(&problems),
        ["expected a valid IANA time zone name"]
    );
    // trim 后为空同样是格式问题。
    d.display_iana_name = "   ".to_owned();
    let problems = validate_column_presentation_draft(&d, &known);
    assert_eq!(paths(&problems), ["displayTimeZone.name"]);
    assert_eq!(
        messages(&problems),
        ["expected a valid IANA time zone name"]
    );
}

#[test]
fn draft_validation_reports_an_unknown_display_iana_name() {
    let mut d = draft();
    d.display_time_zone_kind = TimeZoneKindChoice::Iana;
    d.display_iana_name = "Mars/Olympus".to_owned();
    let problems = validate_column_presentation_draft(&d, &known);
    assert_eq!(paths(&problems), ["displayTimeZone.name"]);
    assert_eq!(messages(&problems), ["unknown time zone name"]);
}

#[test]
fn draft_validation_checks_source_zone_only_for_text_and_native() {
    let mut d = draft();
    d.source_time_zone_kind = SourceTimeZoneChoice::Iana;
    d.source_iana_name = "Mars/Olympus".to_owned();
    for interpretation in [InterpretationChoice::Text, InterpretationChoice::Native] {
        d.interpretation = interpretation;
        let problems = validate_column_presentation_draft(&d, &known);
        assert_eq!(paths(&problems), ["sourceTimeZone.name"]);
        assert_eq!(messages(&problems), ["unknown time zone name"]);
    }
    // Unix 不检查源时区（草稿里填了坏名也不报错；保存时直接丢弃）。
    for interpretation in [
        InterpretationChoice::UnixSeconds,
        InterpretationChoice::UnixMilliseconds,
    ] {
        d.interpretation = interpretation;
        assert!(
            validate_column_presentation_draft(&d, &known).is_empty(),
            "{interpretation:?} must not check the source zone"
        );
    }
}

#[test]
fn draft_validation_returns_iana_problems_before_option_problems() {
    // 展示 IANA 名既过不了草稿层格式检查，也过不了 options 层检查；
    // 草稿层的问题先返，文案是草稿层的（不是 options 层的）。
    let mut d = draft();
    d.display_time_zone_kind = TimeZoneKindChoice::Iana;
    d.display_iana_name = "Bad Name".to_owned();
    let problems = validate_column_presentation_draft(&d, &known);
    assert_eq!(paths(&problems), ["displayTimeZone.name"]);
    assert_eq!(
        messages(&problems),
        ["expected a valid IANA time zone name"]
    );
    assert!(
        !messages(&problems)
            .iter()
            .any(|m| m.contains("without whitespace")),
        "the options-level message must not leak through"
    );
}

#[test]
fn draft_validation_reports_both_zone_problems_display_first() {
    let mut d = draft();
    d.display_time_zone_kind = TimeZoneKindChoice::Iana;
    d.display_iana_name = "Bad Name".to_owned();
    d.source_time_zone_kind = SourceTimeZoneChoice::Iana;
    d.source_iana_name = "Also Bad".to_owned();
    let problems = validate_column_presentation_draft(&d, &known);
    assert_eq!(
        paths(&problems),
        ["displayTimeZone.name", "sourceTimeZone.name"]
    );
}

// ---------------------------------------------------------------------------
// draft_from_column_override
// ---------------------------------------------------------------------------

#[test]
fn draft_from_override_uses_explicit_unix_milliseconds_for_numbers() {
    let draft = draft_from_column_override(
        &ColumnPresentationOverride::Inherit,
        None,
        PresentationSemanticKind::Number,
    );
    assert_eq!(draft.mode, PresentationMode::Inherit);
    assert_eq!(draft.interpretation, InterpretationChoice::UnixMilliseconds);
    assert_eq!(draft.format, TimestampOutputFormat::IsoTz);
    assert_eq!(draft.display_time_zone_kind, TimeZoneKindChoice::Local);
    assert_eq!(draft.display_iana_name, "Asia/Shanghai");
    assert_eq!(draft.source_time_zone_kind, SourceTimeZoneChoice::None);
    assert_eq!(draft.source_iana_name, "Asia/Shanghai");
}

#[test]
fn draft_from_override_defaults_other_kinds_to_native() {
    for kind in [
        PresentationSemanticKind::Instant,
        PresentationSemanticKind::LocalDatetime,
        PresentationSemanticKind::Date,
        PresentationSemanticKind::Time,
        PresentationSemanticKind::Text,
        PresentationSemanticKind::Other,
    ] {
        let draft = draft_from_column_override(&ColumnPresentationOverride::Raw, None, kind);
        assert_eq!(draft.mode, PresentationMode::Raw);
        assert_eq!(
            draft.interpretation,
            InterpretationChoice::Native,
            "{kind:?} must default to native"
        );
    }
}

#[test]
fn draft_from_override_takes_format_and_display_zone_from_global_options() {
    let global = TimestampOptions {
        interpretation: TimestampInterpretation::Native,
        format: TimestampOutputFormat::Locale,
        display_time_zone: TimeZoneSpec::Iana {
            name: "Europe/London".to_owned(),
        },
        source_time_zone: None,
    };
    let draft = draft_from_column_override(
        &ColumnPresentationOverride::Inherit,
        Some(&global),
        PresentationSemanticKind::Text,
    );
    assert_eq!(draft.format, TimestampOutputFormat::Locale);
    assert_eq!(draft.display_time_zone_kind, TimeZoneKindChoice::Iana);
    assert_eq!(draft.display_iana_name, "Europe/London");
    // 全局展示时区是 utc 时，IANA 名仍是默认占位。
    let global_utc = TimestampOptions {
        display_time_zone: TimeZoneSpec::Utc,
        ..global
    };
    let draft = draft_from_column_override(
        &ColumnPresentationOverride::Inherit,
        Some(&global_utc),
        PresentationSemanticKind::Text,
    );
    assert_eq!(draft.display_time_zone_kind, TimeZoneKindChoice::Utc);
    assert_eq!(draft.display_iana_name, "Asia/Shanghai");
}

#[test]
fn draft_from_override_restores_a_valid_custom_override() {
    let current = ColumnPresentationOverride::Custom {
        renderer_id: "datazen.timestamp".to_owned(),
        options: json!({
            "interpretation": { "kind": "unix", "unit": "seconds" },
            "format": "iso",
            "displayTimeZone": { "kind": "iana", "name": "Europe/London" },
        }),
    };
    let draft = draft_from_column_override(&current, None, PresentationSemanticKind::Number);
    assert_eq!(draft.mode, PresentationMode::Custom);
    assert_eq!(draft.interpretation, InterpretationChoice::UnixSeconds);
    assert_eq!(draft.format, TimestampOutputFormat::Iso);
    assert_eq!(draft.display_time_zone_kind, TimeZoneKindChoice::Iana);
    assert_eq!(draft.display_iana_name, "Europe/London");
    assert_eq!(draft.source_time_zone_kind, SourceTimeZoneChoice::None);
    assert_eq!(draft.source_iana_name, "Asia/Shanghai");
}

#[test]
fn draft_from_override_restores_unix_milliseconds_and_native() {
    for (unit, expected) in [
        ("milliseconds", InterpretationChoice::UnixMilliseconds),
        ("seconds", InterpretationChoice::UnixSeconds),
    ] {
        let current = ColumnPresentationOverride::Custom {
            renderer_id: "datazen.timestamp".to_owned(),
            options: json!({
                "interpretation": { "kind": "unix", "unit": unit },
                "format": "iso-tz",
                "displayTimeZone": { "kind": "local" },
            }),
        };
        let draft = draft_from_column_override(&current, None, PresentationSemanticKind::Number);
        assert_eq!(draft.interpretation, expected, "unit {unit}");
    }
    let current = ColumnPresentationOverride::Custom {
        renderer_id: "datazen.timestamp".to_owned(),
        options: json!({
            "interpretation": { "kind": "native" },
            "format": "iso",
            "displayTimeZone": { "kind": "utc" },
            "sourceTimeZone": { "kind": "local" },
        }),
    };
    let draft = draft_from_column_override(&current, None, PresentationSemanticKind::Instant);
    assert_eq!(draft.interpretation, InterpretationChoice::Native);
    assert_eq!(draft.source_time_zone_kind, SourceTimeZoneChoice::Local);
    assert_eq!(draft.source_iana_name, "Asia/Shanghai");
}

#[test]
fn draft_from_override_restores_a_utc_source_zone() {
    let current = ColumnPresentationOverride::Custom {
        renderer_id: "datazen.timestamp".to_owned(),
        options: json!({
            "interpretation": { "kind": "native" },
            "format": "iso",
            "displayTimeZone": { "kind": "utc" },
            "sourceTimeZone": { "kind": "utc" },
        }),
    };
    let draft = draft_from_column_override(&current, None, PresentationSemanticKind::Instant);
    assert_eq!(draft.source_time_zone_kind, SourceTimeZoneChoice::Utc);
    assert_eq!(draft.display_time_zone_kind, TimeZoneKindChoice::Utc);
}

#[test]
fn draft_from_override_restores_text_with_a_source_zone() {
    let current = ColumnPresentationOverride::Custom {
        renderer_id: "datazen.timestamp".to_owned(),
        options: json!({
            "interpretation": { "kind": "text" },
            "format": "iso-tz",
            "displayTimeZone": { "kind": "local" },
            "sourceTimeZone": { "kind": "iana", "name": "America/New_York" },
        }),
    };
    let draft = draft_from_column_override(&current, None, PresentationSemanticKind::Text);
    assert_eq!(draft.interpretation, InterpretationChoice::Text);
    assert_eq!(draft.source_time_zone_kind, SourceTimeZoneChoice::Iana);
    assert_eq!(draft.source_iana_name, "America/New_York");
    assert_eq!(draft.display_time_zone_kind, TimeZoneKindChoice::Local);
    assert_eq!(draft.display_iana_name, "Asia/Shanghai");
}

#[test]
fn draft_from_override_falls_back_to_base_plus_custom_on_bad_options() {
    let global = TimestampOptions {
        interpretation: TimestampInterpretation::Native,
        format: TimestampOutputFormat::Locale,
        display_time_zone: TimeZoneSpec::Local,
        source_time_zone: None,
    };
    for bad_options in [
        json!({ "format": "nope" }),
        json!({}),
        json!("not-an-object"),
    ] {
        let current = ColumnPresentationOverride::Custom {
            renderer_id: "datazen.timestamp".to_owned(),
            options: bad_options,
        };
        let draft =
            draft_from_column_override(&current, Some(&global), PresentationSemanticKind::Number);
        // 非法 options 回落到 base + custom：全局格式保留，解释回到
        // number 种的默认 Unix 毫秒。
        assert_eq!(draft.mode, PresentationMode::Custom);
        assert_eq!(draft.interpretation, InterpretationChoice::UnixMilliseconds);
        assert_eq!(draft.format, TimestampOutputFormat::Locale);
    }
}

// ---------------------------------------------------------------------------
// global_timestamp_options_from_snapshot
// ---------------------------------------------------------------------------

#[test]
fn global_options_from_snapshot_reads_the_timestamp_config() {
    assert_eq!(
        global_timestamp_options_from_snapshot(&snapshot_with(None)),
        None
    );
    let snapshot = snapshot_with(Some(valid_options_json()));
    assert_eq!(
        global_timestamp_options_from_snapshot(&snapshot),
        Some(TimestampOptions {
            interpretation: TimestampInterpretation::Unix(UnixUnit::Milliseconds),
            format: TimestampOutputFormat::IsoTz,
            display_time_zone: TimeZoneSpec::Local,
            source_time_zone: None,
        })
    );
    // 非法配置缺省，不抛错。
    let snapshot = snapshot_with(Some(json!({ "format": "nope" })));
    assert_eq!(global_timestamp_options_from_snapshot(&snapshot), None);
    // 别的渲染器配置不影响。
    let mut snapshot = ValuePresentationSnapshot::default();
    snapshot.configs.insert(
        "other.clock".to_owned(),
        VersionedPresentationConfig {
            config_version: 1,
            config: valid_options_json(),
        },
    );
    assert_eq!(global_timestamp_options_from_snapshot(&snapshot), None);
}

// ---------------------------------------------------------------------------
// 小守卫
// ---------------------------------------------------------------------------

#[test]
fn guards_accept_only_the_exact_choice_strings() {
    for value in ["native", "unix-seconds", "unix-milliseconds", "text"] {
        assert!(is_interpretation_choice(value), "{value}");
    }
    assert!(!is_interpretation_choice("unix"));
    assert!(!is_interpretation_choice("Native"));
    assert!(!is_interpretation_choice(""));

    for value in ["iso", "iso-tz", "locale"] {
        assert!(is_output_format(value), "{value}");
    }
    assert!(!is_output_format("ISO"));
    assert!(!is_output_format(""));

    for value in ["local", "utc", "iana"] {
        assert!(is_time_zone_kind_choice(value), "{value}");
    }
    assert!(!is_time_zone_kind_choice("none"));
    assert!(!is_time_zone_kind_choice(""));

    for value in ["none", "local", "utc", "iana"] {
        assert!(is_source_time_zone_choice(value), "{value}");
    }
    assert!(!is_source_time_zone_choice("utc "));
    assert!(!is_source_time_zone_choice(""));
}

#[test]
fn choice_from_str_parses_and_rejects() {
    use InterpretationChoice as Ic;
    use SourceTimeZoneChoice as St;
    use TimeZoneKindChoice as Tk;
    assert_eq!(Ic::from_str("unix-seconds"), Some(Ic::UnixSeconds));
    assert_eq!(Ic::from_str("nope"), None);
    assert_eq!(Tk::from_str("iana"), Some(Tk::Iana));
    assert_eq!(Tk::from_str("nope"), None);
    assert_eq!(St::from_str("none"), Some(St::None));
    assert_eq!(St::from_str("nope"), None);
    assert_eq!(
        TimestampOutputFormat::from_str("iso-tz"),
        Some(TimestampOutputFormat::IsoTz)
    );
    assert_eq!(TimestampOutputFormat::from_str("nope"), None);
}

// ---------------------------------------------------------------------------
// 往返：草稿 → 配置 / 覆盖 → JSON → 校验回同一值
// ---------------------------------------------------------------------------

#[test]
fn round_trip_draft_to_options_and_back() {
    let mut d = draft();
    d.interpretation = InterpretationChoice::Text;
    d.format = TimestampOutputFormat::Locale;
    d.display_time_zone_kind = TimeZoneKindChoice::Iana;
    d.display_iana_name = "Europe/London".to_owned();
    d.source_time_zone_kind = SourceTimeZoneChoice::Utc;
    let options = draft_to_timestamp_options(&d);
    assert_eq!(validate_timestamp_options(&options.to_json()), Ok(options));
}

#[test]
fn round_trip_draft_to_override_and_back() {
    let d = draft();
    let override_ = draft_to_column_override(&d);
    assert_eq!(
        validate_column_presentation_override(&override_.to_json()),
        Ok(override_)
    );
}

#[test]
fn round_trip_draft_from_override_is_stable() {
    // custom 草稿 → 覆盖 → 再还原，字段不变。
    let mut d = draft();
    d.interpretation = InterpretationChoice::UnixSeconds;
    d.format = TimestampOutputFormat::Iso;
    d.display_time_zone_kind = TimeZoneKindChoice::Iana;
    d.display_iana_name = "Europe/London".to_owned();
    let restored = draft_from_column_override(
        &draft_to_column_override(&d),
        None,
        PresentationSemanticKind::Number,
    );
    assert_eq!(restored, d);
}
