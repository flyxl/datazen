//! T054-S2 coder tests: `ColumnPresentationDialog` 的对话框组件 +
//! 状态 + 保存/开启接线 + 渲染（`column_presentation_plan.rs`、
//! `column_presentation_rules.rs`、`column_presentation_dialog_state.rs`、
//! `column_presentation_dialog_view.rs`）。
//!
//! 覆盖：语义种、来源键与快照剥离、样本、规则匹配、计划决策顺序、
//! 预览四种结果、状态机（打开重建 / 草稿补丁 / 保存两路 / 开启两
//! 路 / outbox）、渲染（标题、总开关提示、贡献缺失 T11、来源行、
//! 模式切换、custom 表单、预览框、无来源禁用、就地报错）。
//!
//! 时间引擎是接线口：测试用桩 presenter（unix 秒/毫秒 → UTC ISO、
//! 原生带时区串 → UTC ISO，其余 `not-applicable`），只证“候选覆盖
//! 与样本走到了 presenter、结果回到了预览”，不等同真实时间引擎
//! （T253/T209/T274 的活；PRD 的 +08:00 锚点由那几片 pin）。

use std::rc::Rc;

use serde_json::{json, Value};

use datazen_ui::test_support::*;

use rune::{col, Cx, El, Key, KeyEvent, Mods};
use rune_kit::Kit;

use datazen_ui::components::data_table::column_presentation_dialog::{
    draft_to_column_override, global_timestamp_options_from_snapshot, problem,
    validate_column_presentation_override, ColumnPresentationDraft, ColumnPresentationOverride,
    InterpretationChoice, PresentationMode, PresentationSemanticKind, SourceTimeZoneChoice,
    TimeZoneKindChoice, TimeZoneSpec, TimestampInterpretation, TimestampOptions,
    TimestampOutputFormat, UnixUnit, ValuePresentationSnapshot, VersionedPresentationConfig,
    TIMESTAMP_CONTRIBUTION_ID,
};
use datazen_ui::components::data_table::column_presentation_dialog_state::{
    ColumnPresentationDialogConfig, ColumnPresentationDialogState, ColumnPresentationDraftPatch,
    ColumnPresentationEnableRequest, ColumnPresentationSaveRequest, ColumnPresentationSelect,
};
use datazen_ui::components::data_table::column_presentation_dialog_view::ColumnPresentationDialog;
use datazen_ui::components::data_table::column_presentation_plan::{
    build_column_presentation_plan, column_override_from_snapshot, column_override_key,
    compute_column_presentation_preview, fallback_sample_for_column,
    format_column_presentation_sample_value, resolve_presentation_semantic_kind,
    snapshot_without_column_override, ColumnPresentationPreview, ColumnPresentationStatus,
    ColumnSource, PresentationEnvironment, PresentationPlanColumn, PresentationResult,
    PresentationValue, TimestampPresenter,
};
use datazen_ui::components::data_table::column_presentation_rules::{
    find_first_matching_rule, match_name_pattern, match_value_presentation_rule,
    ValuePresentationContribution, ValuePresentationNamePattern, ValuePresentationRegistry,
    ValuePresentationRule, ValuePresentationRuleContext, ValuePresentationRuleMatch,
};
use datazen_ui::i18n::t;
use datazen_ui::support::formatters::format_timestamp;
use datazen_ui::ui::kit::kit;

// ---------------------------------------------------------------------------
// 桩与构造器
// ---------------------------------------------------------------------------

/// 桩已知性：只认 Asia/Shanghai 与 Europe/London（与 S1 审计同一桩）。
fn known(name: &str) -> bool {
    matches!(name, "Asia/Shanghai" | "Europe/London")
}

/// React 测试的 GLOBAL_OPTIONS：native / iso-tz / IANA Asia/Shanghai。
fn global_options_json() -> Value {
    json!({
        "interpretation": { "kind": "native" },
        "format": "iso-tz",
        "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
    })
}

fn valid_unix_ms_options() -> TimestampOptions {
    TimestampOptions {
        interpretation: TimestampInterpretation::Unix(UnixUnit::Milliseconds),
        format: TimestampOutputFormat::IsoTz,
        display_time_zone: TimeZoneSpec::Iana {
            name: "Asia/Shanghai".to_owned(),
        },
        source_time_zone: None,
    }
}

fn source() -> ColumnSource {
    ColumnSource {
        connection_id: "conn-1".to_owned(),
        database: Some("shop".to_owned()),
        namespace: Some("public".to_owned()),
        table: "orders".to_owned(),
    }
}

fn snapshot_with(
    configs: Value,
    enabled: bool,
    overrides: Vec<(String, ColumnPresentationOverride)>,
    rules: Vec<ValuePresentationRule>,
) -> ValuePresentationSnapshot {
    let snapshot = ValuePresentationSnapshot {
        revision: 1,
        environment: PresentationEnvironment::new("en-US", "Asia/Shanghai"),
        configs: [(
            TIMESTAMP_CONTRIBUTION_ID.to_owned(),
            VersionedPresentationConfig {
                config_version: 1,
                config: configs,
            },
        )]
        .into_iter()
        .collect(),
        enabled_renderer_ids: if enabled {
            vec![TIMESTAMP_CONTRIBUTION_ID.to_owned()]
        } else {
            Vec::new()
        },
        overrides: overrides.into_iter().collect(),
        rules,
    };
    snapshot
}

fn make_snapshot() -> ValuePresentationSnapshot {
    snapshot_with(global_options_json(), true, Vec::new(), Vec::new())
}

fn snapshot_without_config() -> ValuePresentationSnapshot {
    let mut snapshot = make_snapshot();
    snapshot.configs.clear();
    snapshot
}

fn snapshot_with_config_version(version: u64) -> ValuePresentationSnapshot {
    let mut snapshot = make_snapshot();
    if let Some(config) = snapshot.configs.get_mut(TIMESTAMP_CONTRIBUTION_ID) {
        config.config_version = version;
    }
    snapshot
}

fn rule_unix_ms() -> ValuePresentationRule {
    ValuePresentationRule {
        name: "unix-ms".to_owned(),
        enabled: true,
        match_: ValuePresentationRuleMatch {
            column: Some(ValuePresentationNamePattern::Exact("epoch_ms".to_owned())),
            ..ValuePresentationRuleMatch::default()
        },
        options: valid_unix_ms_options(),
    }
}

fn registry_with_timestamp() -> ValuePresentationRegistry {
    ValuePresentationRegistry::with_timestamp()
}

/// 测试桩 presenter：只证接线，不冒充时间引擎。unix 秒/毫秒数字与
/// 原生带时区串走已有的 UTC 归一化；其余一律 `not-applicable`，
/// null → `null`（与 `timestampContribution.ts` 的门禁顺序一致）。
fn presenter_stub() -> TimestampPresenter {
    Rc::new(
        |options: &TimestampOptions,
         kind: PresentationSemanticKind,
         sample: &PresentationValue,
         _environment: &PresentationEnvironment| {
            if kind == PresentationSemanticKind::Other {
                return PresentationResult::Unchanged {
                    reason: Some("not-applicable".to_owned()),
                };
            }
            if sample == &Value::Null {
                return PresentationResult::Unchanged {
                    reason: Some("null".to_owned()),
                };
            }
            match &options.interpretation {
                TimestampInterpretation::Unix(unit) => {
                    let Some(number) = sample.as_i64() else {
                        return PresentationResult::Unchanged {
                            reason: Some("not-applicable".to_owned()),
                        };
                    };
                    let millis = match unit {
                        UnixUnit::Seconds => number.saturating_mul(1000),
                        UnixUnit::Milliseconds => number,
                    };
                    PresentationResult::Text {
                        text: format_timestamp(&json!(millis)),
                        interpretation: Some("unix".to_owned()),
                    }
                }
                TimestampInterpretation::Native => {
                    if sample.is_string() {
                        PresentationResult::Text {
                            text: format_timestamp(sample),
                            interpretation: Some("native".to_owned()),
                        }
                    } else {
                        PresentationResult::Unchanged {
                            reason: Some("not-applicable".to_owned()),
                        }
                    }
                }
                TimestampInterpretation::Text => PresentationResult::Unchanged {
                    reason: Some("not-applicable".to_owned()),
                },
            }
        },
    )
}

fn dialog_config() -> ColumnPresentationDialogConfig {
    ColumnPresentationDialogConfig {
        column_name: "epoch_ms".to_owned(),
        column_type: Some("bigint".to_owned()),
        cell_type_hints: None,
        source: Some(source()),
        sample_value: None,
        current_override: None,
        has_on_save: false,
        has_on_enable_time_display: false,
    }
}

fn open_state(
    config: ColumnPresentationDialogConfig,
    snapshot: &ValuePresentationSnapshot,
) -> ColumnPresentationDialogState {
    let mut state = ColumnPresentationDialogState::new(config);
    state.open_dialog(snapshot);
    state
}

fn custom_draft() -> ColumnPresentationDraft {
    ColumnPresentationDraft {
        mode: PresentationMode::Custom,
        interpretation: InterpretationChoice::UnixMilliseconds,
        format: TimestampOutputFormat::IsoTz,
        display_time_zone_kind: TimeZoneKindChoice::Iana,
        display_iana_name: "Asia/Shanghai".to_owned(),
        source_time_zone_kind: SourceTimeZoneChoice::None,
        source_iana_name: "Asia/Shanghai".to_owned(),
    }
}

fn plan_column(name: &str, data_type: &str) -> PresentationPlanColumn {
    PresentationPlanColumn {
        name: name.to_owned(),
        data_type: Some(data_type.to_owned()),
        hints: None,
    }
}

// ---------------------------------------------------------------------------
// 语义种 / 来源键 / 样本
// ---------------------------------------------------------------------------

#[test]
fn semantic_kind_maps_cell_types() {
    assert_eq!(
        resolve_presentation_semantic_kind(Some("timestamptz"), None),
        PresentationSemanticKind::Instant
    );
    assert_eq!(
        resolve_presentation_semantic_kind(Some("timestamp"), None),
        PresentationSemanticKind::LocalDatetime
    );
    assert_eq!(
        resolve_presentation_semantic_kind(Some("date"), None),
        PresentationSemanticKind::Date
    );
    assert_eq!(
        resolve_presentation_semantic_kind(Some("time"), None),
        PresentationSemanticKind::Time
    );
    assert_eq!(
        resolve_presentation_semantic_kind(Some("bigint"), None),
        PresentationSemanticKind::Number
    );
    assert_eq!(
        resolve_presentation_semantic_kind(Some("varchar(10)"), None),
        PresentationSemanticKind::Text
    );
    assert_eq!(
        resolve_presentation_semantic_kind(Some("uuid"), None),
        PresentationSemanticKind::Text
    );
    assert_eq!(
        resolve_presentation_semantic_kind(Some("bool"), None),
        PresentationSemanticKind::Other
    );
    assert_eq!(
        resolve_presentation_semantic_kind(Some("jsonb"), None),
        PresentationSemanticKind::Other
    );
    assert_eq!(
        resolve_presentation_semantic_kind(None, None),
        PresentationSemanticKind::Other
    );
}

#[test]
fn override_key_distinguishes_null_from_empty_string() {
    let with_null = column_override_key(
        &ColumnSource {
            database: None,
            ..source()
        },
        "epoch_ms",
    );
    let with_empty = column_override_key(
        &ColumnSource {
            database: Some(String::new()),
            ..source()
        },
        "epoch_ms",
    );
    assert_eq!(with_null, r#"["conn-1",null,"public","orders","epoch_ms"]"#);
    assert_eq!(with_empty, r#"["conn-1","","public","orders","epoch_ms"]"#);
    assert_ne!(with_null, with_empty);
}

#[test]
fn fallback_samples_follow_the_prd_anchors() {
    assert_eq!(
        fallback_sample_for_column(&plan_column("c", "bigint")),
        Some(json!(1705316400000i64))
    );
    assert_eq!(
        fallback_sample_for_column(&plan_column("c", "timestamptz")),
        Some(json!("2026-01-15T14:30:00Z"))
    );
    assert_eq!(
        fallback_sample_for_column(&plan_column("c", "timestamp")),
        Some(json!("2026-01-15 14:30:00"))
    );
    assert_eq!(
        fallback_sample_for_column(&plan_column("c", "date")),
        Some(json!("2026-01-15"))
    );
    assert_eq!(
        fallback_sample_for_column(&plan_column("c", "time")),
        Some(json!("14:30:00"))
    );
    assert_eq!(fallback_sample_for_column(&plan_column("c", "text")), None);
    assert_eq!(fallback_sample_for_column(&plan_column("c", "bool")), None);
}

#[test]
fn sample_value_text_matches_typescript() {
    assert_eq!(
        format_column_presentation_sample_value(&Value::Null),
        "NULL"
    );
    assert_eq!(
        format_column_presentation_sample_value(&json!("2026-01-15")),
        "2026-01-15"
    );
    assert_eq!(
        format_column_presentation_sample_value(&json!(1705316400000i64)),
        "1705316400000"
    );
    assert_eq!(
        format_column_presentation_sample_value(&json!(true)),
        "true"
    );
    assert_eq!(
        format_column_presentation_sample_value(&json!({"a": 1})),
        r#"{"a":1}"#
    );
    assert_eq!(
        format_column_presentation_sample_value(&json!([1, 2])),
        "[1,2]"
    );
}

#[test]
fn snapshot_without_override_removes_only_that_key() {
    let snapshot = snapshot_with(
        global_options_json(),
        true,
        vec![(
            column_override_key(&source(), "epoch_ms"),
            ColumnPresentationOverride::Raw,
        )],
        Vec::new(),
    );
    let stripped = snapshot_without_column_override(&snapshot, Some(&source()), "epoch_ms");
    assert!(stripped.overrides.is_empty());
    // 原快照不动。
    assert_eq!(snapshot.overrides.len(), 1);
    // 无条目时返回等值快照。
    let same = snapshot_without_column_override(&snapshot, Some(&source()), "other");
    assert_eq!(same, snapshot);
    // 无来源列时返回等值快照。
    let no_source = snapshot_without_column_override(&snapshot, None, "epoch_ms");
    assert_eq!(no_source, snapshot);
}

#[test]
fn override_from_snapshot_falls_back_to_inherit() {
    let snapshot = snapshot_with(
        global_options_json(),
        true,
        vec![(
            column_override_key(&source(), "epoch_ms"),
            ColumnPresentationOverride::Raw,
        )],
        Vec::new(),
    );
    assert_eq!(
        column_override_from_snapshot(&snapshot, Some(&source()), "epoch_ms"),
        ColumnPresentationOverride::Raw
    );
    assert_eq!(
        column_override_from_snapshot(&snapshot, Some(&source()), "other"),
        ColumnPresentationOverride::Inherit
    );
    assert_eq!(
        column_override_from_snapshot(&snapshot, None, "epoch_ms"),
        ColumnPresentationOverride::Inherit
    );
}

// ---------------------------------------------------------------------------
// 规则匹配
// ---------------------------------------------------------------------------

#[test]
fn rule_matching_exact_regex_and_conditions() {
    let context = ValuePresentationRuleContext {
        connection_id: Some("conn-1"),
        database: Some("shop"),
        namespace: Some("public"),
        table: Some("orders"),
        column: "epoch_ms",
        database_type_name: Some("bigint"),
    };
    assert!(match_name_pattern(
        &ValuePresentationNamePattern::Exact("epoch_ms".to_owned()),
        "epoch_ms"
    ));
    assert!(!match_name_pattern(
        &ValuePresentationNamePattern::Exact("epoch_ms".to_owned()),
        "epoch_s"
    ));
    assert!(match_name_pattern(
        &ValuePresentationNamePattern::Regex {
            source: "^epoch_.*".to_owned(),
            flags: None,
        },
        "epoch_ms"
    ));
    assert!(!match_name_pattern(
        &ValuePresentationNamePattern::Regex {
            source: "^epoch_.*".to_owned(),
            flags: None,
        },
        "created_ms"
    ));
    // `i` 只折叠 ASCII 大小写；非法 flags 按不匹配。
    assert!(match_name_pattern(
        &ValuePresentationNamePattern::Regex {
            source: "^EPOCH_.*".to_owned(),
            flags: Some("i".to_owned()),
        },
        "epoch_ms"
    ));
    assert!(!match_name_pattern(
        &ValuePresentationNamePattern::Regex {
            source: "^epoch_.*".to_owned(),
            flags: Some("g".to_owned()),
        },
        "epoch_ms"
    ));
    // 未验证的非法 pattern 编译失败 → 不匹配（TS 的防御性 catch）。
    assert!(!match_name_pattern(
        &ValuePresentationNamePattern::Regex {
            source: "(".to_owned(),
            flags: None,
        },
        "epoch_ms"
    ));
    // 表级条件缺失时不满足。
    let no_table = ValuePresentationRuleContext {
        table: None,
        ..context
    };
    let table_rule = ValuePresentationRule {
        name: "t".to_owned(),
        enabled: true,
        match_: ValuePresentationRuleMatch {
            table: Some(ValuePresentationNamePattern::Exact("orders".to_owned())),
            ..ValuePresentationRuleMatch::default()
        },
        options: valid_unix_ms_options(),
    };
    assert!(find_first_matching_rule(std::slice::from_ref(&table_rule), &context).is_some());
    assert!(find_first_matching_rule(std::slice::from_ref(&table_rule), &no_table).is_none());
}

#[test]
fn first_matching_rule_skips_disabled_and_takes_first() {
    let context = ValuePresentationRuleContext {
        column: "epoch_ms",
        ..ValuePresentationRuleContext::default()
    };
    let disabled = ValuePresentationRule {
        name: "off".to_owned(),
        enabled: false,
        ..rule_unix_ms()
    };
    let second = ValuePresentationRule {
        name: "second".to_owned(),
        ..rule_unix_ms()
    };
    let rules = vec![disabled, rule_unix_ms(), second];
    assert_eq!(
        find_first_matching_rule(&rules, &context).map(|rule| rule.name.as_str()),
        Some("unix-ms")
    );
    assert_eq!(find_first_matching_rule(&[], &context), None);
}

#[test]
fn registry_register_is_idempotent_and_unregister_bumps_revision() {
    let mut registry = ValuePresentationRegistry::new();
    assert_eq!(registry.revision(), 0);
    assert!(registry.get(TIMESTAMP_CONTRIBUTION_ID).is_none());
    registry.register(ValuePresentationContribution::timestamp());
    assert_eq!(registry.revision(), 1);
    assert!(registry.get(TIMESTAMP_CONTRIBUTION_ID).is_some());
    // 重复注册幂等：不重复、不动 revision。
    registry.register(ValuePresentationContribution::timestamp());
    assert_eq!(registry.revision(), 1);
    // 注销不在场的贡献不动 revision；在场的删掉并 +1。
    registry.unregister("datazen.nope");
    assert_eq!(registry.revision(), 1);
    registry.unregister(TIMESTAMP_CONTRIBUTION_ID);
    assert_eq!(registry.revision(), 2);
    assert!(registry.get(TIMESTAMP_CONTRIBUTION_ID).is_none());
}

#[test]
fn rule_matching_checks_every_condition() {
    let context = ValuePresentationRuleContext {
        connection_id: Some("conn-1"),
        database: Some("shop"),
        namespace: Some("public"),
        table: Some("orders"),
        column: "epoch_ms",
        database_type_name: Some("bigint"),
    };
    let full = ValuePresentationRule {
        name: "full".to_owned(),
        enabled: true,
        match_: ValuePresentationRuleMatch {
            connection_id: Some("conn-1".to_owned()),
            database: Some("shop".to_owned()),
            namespace: Some("public".to_owned()),
            table: Some(ValuePresentationNamePattern::Exact("orders".to_owned())),
            column: Some(ValuePresentationNamePattern::Exact("epoch_ms".to_owned())),
            database_type_name: Some(ValuePresentationNamePattern::Exact("bigint".to_owned())),
        },
        options: valid_unix_ms_options(),
    };
    assert!(match_value_presentation_rule(&full, &context));
    // 每个条件失配都让整条规则不命中。
    for (label, mismatch) in [
        (
            "connection",
            ValuePresentationRuleContext {
                connection_id: Some("conn-2"),
                ..context
            },
        ),
        (
            "database",
            ValuePresentationRuleContext {
                database: Some("other"),
                ..context
            },
        ),
        (
            "namespace",
            ValuePresentationRuleContext {
                namespace: Some("other"),
                ..context
            },
        ),
        (
            "table",
            ValuePresentationRuleContext {
                table: Some("items"),
                ..context
            },
        ),
        (
            "column",
            ValuePresentationRuleContext {
                column: "created_ms",
                ..context
            },
        ),
        (
            "type",
            ValuePresentationRuleContext {
                database_type_name: Some("text"),
                ..context
            },
        ),
        (
            "type-missing",
            ValuePresentationRuleContext {
                database_type_name: None,
                ..context
            },
        ),
    ] {
        assert!(
            !match_value_presentation_rule(&full, &mismatch),
            "{label} mismatch must not match"
        );
    }
}

// ---------------------------------------------------------------------------
// 计划决策顺序
// ---------------------------------------------------------------------------

#[test]
fn plan_raw_override_is_raw_without_presenter() {
    let snapshot = snapshot_with(
        global_options_json(),
        true,
        vec![(
            column_override_key(&source(), "epoch_ms"),
            ColumnPresentationOverride::Raw,
        )],
        Vec::new(),
    );
    let plan = build_column_presentation_plan(
        &plan_column("epoch_ms", "bigint"),
        Some(&source()),
        &snapshot,
        &registry_with_timestamp(),
        None,
    );
    assert_eq!(plan.status(), ColumnPresentationStatus::Raw);
    assert_eq!(
        plan.present(&json!(1705316400000i64)),
        PresentationResult::Unchanged {
            reason: Some("raw-override".to_owned())
        }
    );
}

#[test]
fn plan_custom_override_ready_invalid_and_missing() {
    let presenter = presenter_stub();
    // 合法 custom → ready，presenter 出文本。
    let snapshot = snapshot_with(
        global_options_json(),
        false,
        vec![(
            column_override_key(&source(), "epoch_ms"),
            ColumnPresentationOverride::Custom {
                renderer_id: TIMESTAMP_CONTRIBUTION_ID.to_owned(),
                options: valid_unix_ms_options().to_json(),
            },
        )],
        Vec::new(),
    );
    let plan = build_column_presentation_plan(
        &plan_column("epoch_ms", "bigint"),
        Some(&source()),
        &snapshot,
        &registry_with_timestamp(),
        Some(&presenter),
    );
    assert_eq!(plan.status(), ColumnPresentationStatus::Ready);
    assert_eq!(
        plan.present(&json!(1705316400000i64)),
        PresentationResult::Text {
            text: "2024-01-15T11:00:00.000Z".to_owned(),
            interpretation: Some("unix".to_owned()),
        }
    );
    // custom bypass enabled 门禁：快照里总开关是关的，仍 ready。
    assert!(!snapshot
        .enabled_renderer_ids
        .contains(&TIMESTAMP_CONTRIBUTION_ID.to_owned()));
    // 非法 options → invalid-config。
    let bad = snapshot_with(
        global_options_json(),
        true,
        vec![(
            column_override_key(&source(), "epoch_ms"),
            ColumnPresentationOverride::Custom {
                renderer_id: TIMESTAMP_CONTRIBUTION_ID.to_owned(),
                options: json!({"bogus": true}),
            },
        )],
        Vec::new(),
    );
    let plan = build_column_presentation_plan(
        &plan_column("epoch_ms", "bigint"),
        Some(&source()),
        &bad,
        &registry_with_timestamp(),
        Some(&presenter),
    );
    assert_eq!(plan.status(), ColumnPresentationStatus::InvalidConfig);
    // 贡献缺失 → missing-renderer。
    let plan = build_column_presentation_plan(
        &plan_column("epoch_ms", "bigint"),
        Some(&source()),
        &snapshot,
        &ValuePresentationRegistry::new(),
        Some(&presenter),
    );
    assert_eq!(plan.status(), ColumnPresentationStatus::MissingRenderer);
    assert_eq!(plan.renderer_id(), Some(TIMESTAMP_CONTRIBUTION_ID));
}

#[test]
fn plan_default_route_checks_enabled_contribution_config_version() {
    let presenter = presenter_stub();
    let registry = registry_with_timestamp();
    let column = plan_column("created_at", "timestamptz");
    // 总开关关 → disabled。
    let snapshot = snapshot_with(global_options_json(), false, Vec::new(), Vec::new());
    let plan = build_column_presentation_plan(
        &column,
        Some(&source()),
        &snapshot,
        &registry,
        Some(&presenter),
    );
    assert_eq!(plan.status(), ColumnPresentationStatus::Disabled);
    // 贡献缺失 → missing-renderer。
    let snapshot = make_snapshot();
    let plan = build_column_presentation_plan(
        &column,
        Some(&source()),
        &snapshot,
        &ValuePresentationRegistry::new(),
        Some(&presenter),
    );
    assert_eq!(plan.status(), ColumnPresentationStatus::MissingRenderer);
    // 配置缺失 → missing-config。
    let plan = build_column_presentation_plan(
        &column,
        Some(&source()),
        &snapshot_without_config(),
        &registry,
        Some(&presenter),
    );
    assert_eq!(plan.status(), ColumnPresentationStatus::MissingConfig);
    // 版本不一致 → invalid-config。
    let plan = build_column_presentation_plan(
        &column,
        Some(&source()),
        &snapshot_with_config_version(99),
        &registry,
        Some(&presenter),
    );
    assert_eq!(plan.status(), ColumnPresentationStatus::InvalidConfig);
    // 非时间列 → default。
    let plan = build_column_presentation_plan(
        &plan_column("name", "text"),
        Some(&source()),
        &make_snapshot(),
        &registry,
        Some(&presenter),
    );
    assert_eq!(plan.status(), ColumnPresentationStatus::Default);
    assert_eq!(plan.reason(), Some("no-renderer"));
    // 全通 → ready。
    let plan = build_column_presentation_plan(
        &column,
        Some(&source()),
        &make_snapshot(),
        &registry,
        Some(&presenter),
    );
    assert_eq!(plan.status(), ColumnPresentationStatus::Ready);
}

#[test]
fn plan_rule_hit_sets_matched_rule_name() {
    let presenter = presenter_stub();
    let snapshot = snapshot_with(
        global_options_json(),
        true,
        Vec::new(),
        vec![rule_unix_ms()],
    );
    let plan = build_column_presentation_plan(
        &plan_column("epoch_ms", "bigint"),
        Some(&source()),
        &snapshot,
        &registry_with_timestamp(),
        Some(&presenter),
    );
    assert_eq!(plan.status(), ColumnPresentationStatus::Ready);
    assert_eq!(plan.matched_rule_name(), Some("unix-ms"));
    assert_eq!(
        plan.present(&json!(1705316400000i64)),
        PresentationResult::Text {
            text: "2024-01-15T11:00:00.000Z".to_owned(),
            interpretation: Some("unix".to_owned()),
        }
    );
    // 贡献缺失时规则不报规则错，落到默认路由（number 列 → default）。
    let plan = build_column_presentation_plan(
        &plan_column("epoch_ms", "bigint"),
        Some(&source()),
        &snapshot,
        &ValuePresentationRegistry::new(),
        Some(&presenter),
    );
    assert_eq!(plan.status(), ColumnPresentationStatus::Default);
    assert_eq!(plan.matched_rule_name(), None);
}

#[test]
fn plan_without_presenter_falls_back_to_plan_error() {
    let plan = build_column_presentation_plan(
        &plan_column("created_at", "timestamptz"),
        Some(&source()),
        &make_snapshot(),
        &registry_with_timestamp(),
        None,
    );
    assert_eq!(plan.status(), ColumnPresentationStatus::Ready);
    assert_eq!(
        plan.present(&json!("2026-01-15T14:30:00Z")),
        PresentationResult::Unchanged {
            reason: Some("plan-error".to_owned())
        }
    );
}

// ---------------------------------------------------------------------------
// 预览
// ---------------------------------------------------------------------------

#[test]
fn preview_inherit_raw_custom_and_unavailable() {
    let presenter = presenter_stub();
    let registry = registry_with_timestamp();
    let column = plan_column("epoch_ms", "bigint");
    let sample = json!(1705316400000i64);
    // inherit + 规则命中 → presenter 文本。
    let snapshot = snapshot_with(
        global_options_json(),
        true,
        Vec::new(),
        vec![rule_unix_ms()],
    );
    let preview = compute_column_presentation_preview(
        &ColumnPresentationOverride::Inherit,
        &column,
        Some(&source()),
        &snapshot,
        sample.clone(),
        &registry,
        Some(&presenter),
    );
    assert_eq!(
        preview,
        ColumnPresentationPreview::Text {
            text: "2024-01-15T11:00:00.000Z".to_owned(),
            interpretation: Some("unix".to_owned()),
            sample: sample.clone(),
        }
    );
    // raw → unchanged raw-override。
    let preview = compute_column_presentation_preview(
        &ColumnPresentationOverride::Raw,
        &column,
        Some(&source()),
        &snapshot,
        sample.clone(),
        &registry,
        Some(&presenter),
    );
    assert_eq!(
        preview,
        ColumnPresentationPreview::Unchanged {
            reason: Some("raw-override".to_owned()),
            sample: sample.clone(),
        }
    );
    // custom → presenter 文本。
    let preview = compute_column_presentation_preview(
        &ColumnPresentationOverride::Custom {
            renderer_id: TIMESTAMP_CONTRIBUTION_ID.to_owned(),
            options: valid_unix_ms_options().to_json(),
        },
        &column,
        Some(&source()),
        &snapshot,
        sample.clone(),
        &registry,
        Some(&presenter),
    );
    assert_eq!(
        preview,
        ColumnPresentationPreview::Text {
            text: "2024-01-15T11:00:00.000Z".to_owned(),
            interpretation: Some("unix".to_owned()),
            sample: sample.clone(),
        }
    );
    // custom + 非法 options → invalid（路径与 TS validateTimestampOptions 一致）。
    let preview = compute_column_presentation_preview(
        &ColumnPresentationOverride::Custom {
            renderer_id: TIMESTAMP_CONTRIBUTION_ID.to_owned(),
            options: json!({"bogus": true}),
        },
        &column,
        Some(&source()),
        &snapshot,
        sample.clone(),
        &registry,
        Some(&presenter),
    );
    assert!(matches!(preview, ColumnPresentationPreview::Invalid { .. }));
    if let ColumnPresentationPreview::Invalid { problems, .. } = preview {
        assert!(problems
            .iter()
            .any(|problem| problem.path == "interpretation"));
    }
    // 贡献缺失 → unavailable。
    let preview = compute_column_presentation_preview(
        &ColumnPresentationOverride::Custom {
            renderer_id: TIMESTAMP_CONTRIBUTION_ID.to_owned(),
            options: valid_unix_ms_options().to_json(),
        },
        &column,
        Some(&source()),
        &snapshot,
        sample.clone(),
        &ValuePresentationRegistry::new(),
        Some(&presenter),
    );
    assert_eq!(
        preview,
        ColumnPresentationPreview::Unavailable {
            sample: sample.clone()
        }
    );
    // 无 presenter → plan-error 回退（不是编造的文本）。
    let preview = compute_column_presentation_preview(
        &ColumnPresentationOverride::Custom {
            renderer_id: TIMESTAMP_CONTRIBUTION_ID.to_owned(),
            options: valid_unix_ms_options().to_json(),
        },
        &column,
        Some(&source()),
        &snapshot,
        sample.clone(),
        &registry,
        None,
    );
    assert_eq!(
        preview,
        ColumnPresentationPreview::Unchanged {
            reason: Some("plan-error".to_owned()),
            sample,
        }
    );
}

// ---------------------------------------------------------------------------
// U+FEFF 判定（S1 tester 非阻塞发现 1 的 pin）
// ---------------------------------------------------------------------------

#[test]
fn renderer_id_with_feff_trims_like_javascript() {
    let options = valid_unix_ms_options().to_json();
    // 首尾 U+FEFF 是 JS 空白：trim 后合法。
    let trailing = json!({
        "mode": "custom",
        "rendererId": "datazen.timestamp\u{feff}",
        "options": options,
    });
    assert!(validate_column_presentation_override(&trailing).is_ok());
    let leading = json!({
        "mode": "custom",
        "rendererId": "\u{feff}datazen.timestamp",
        "options": options,
    });
    assert!(validate_column_presentation_override(&leading).is_ok());
    // 名内 U+FEFF 是空白：非法。
    let inner = json!({
        "mode": "custom",
        "rendererId": "datazen\u{feff}.timestamp",
        "options": options,
    });
    assert_eq!(
        validate_column_presentation_override(&inner),
        Err(vec![problem("rendererId", "must not contain whitespace")])
    );
}

// ---------------------------------------------------------------------------
// 状态机
// ---------------------------------------------------------------------------

#[test]
fn open_dialog_rebuilds_draft_from_stored_custom_override() {
    let mut config = dialog_config();
    config.current_override = Some(ColumnPresentationOverride::Custom {
        renderer_id: TIMESTAMP_CONTRIBUTION_ID.to_owned(),
        options: valid_unix_ms_options().to_json(),
    });
    let state = open_state(config, &make_snapshot());
    let draft = state.draft();
    assert_eq!(draft.mode, PresentationMode::Custom);
    assert_eq!(draft.interpretation, InterpretationChoice::UnixMilliseconds);
    assert_eq!(draft.format, TimestampOutputFormat::IsoTz);
    assert_eq!(draft.display_time_zone_kind, TimeZoneKindChoice::Iana);
    assert_eq!(draft.display_iana_name, "Asia/Shanghai");
}

#[test]
fn open_dialog_defaults_number_columns_to_unix_milliseconds() {
    let state = open_state(dialog_config(), &make_snapshot());
    assert_eq!(state.draft().mode, PresentationMode::Inherit);
    assert_eq!(
        state.draft().interpretation,
        InterpretationChoice::UnixMilliseconds
    );
    // 文本列默认 native。
    let mut config = dialog_config();
    config.column_name = "name".to_owned();
    config.column_type = Some("varchar".to_owned());
    let state = open_state(config, &make_snapshot());
    assert_eq!(state.draft().interpretation, InterpretationChoice::Native);
}

#[test]
fn open_dialog_clears_errors_and_rebuilds_on_reopen() {
    let mut state = open_state(dialog_config(), &make_snapshot());
    state.update_draft(ColumnPresentationDraftPatch {
        mode: Some(PresentationMode::Raw),
        ..ColumnPresentationDraftPatch::default()
    });
    assert_eq!(state.draft().mode, PresentationMode::Raw);
    // 人为置一个错误，再重开：错误清掉、草稿重建。
    state.save_failed();
    assert!(state.error().is_some());
    state.open_dialog(&make_snapshot());
    assert_eq!(state.draft().mode, PresentationMode::Inherit);
    assert!(state.error().is_none());
}

#[test]
fn update_draft_applies_patch_and_clears_error() {
    let mut state = open_state(dialog_config(), &make_snapshot());
    state.save_failed();
    assert!(state.error().is_some());
    state.update_draft(ColumnPresentationDraftPatch {
        format: Some(TimestampOutputFormat::Locale),
        ..ColumnPresentationDraftPatch::default()
    });
    assert_eq!(state.draft().format, TimestampOutputFormat::Locale);
    assert!(state.error().is_none());
}

#[test]
fn choose_select_accepts_valid_and_ignores_invalid() {
    let mut state = open_state(dialog_config(), &make_snapshot());
    state.update_draft(ColumnPresentationDraftPatch {
        mode: Some(PresentationMode::Custom),
        ..ColumnPresentationDraftPatch::default()
    });
    state.set_select_open(ColumnPresentationSelect::Interpretation, true);
    state.choose_select(ColumnPresentationSelect::Interpretation, "unix-seconds");
    assert_eq!(
        state.draft().interpretation,
        InterpretationChoice::UnixSeconds
    );
    assert!(!state.is_select_open(ColumnPresentationSelect::Interpretation));
    // 非法值忽略，草稿不动。
    state.choose_select(ColumnPresentationSelect::Interpretation, "bogus");
    assert_eq!(
        state.draft().interpretation,
        InterpretationChoice::UnixSeconds
    );
}

#[test]
fn effective_override_prefers_explicit_current_override() {
    let mut config = dialog_config();
    config.current_override = Some(ColumnPresentationOverride::Raw);
    let snapshot = snapshot_with(
        global_options_json(),
        true,
        vec![(
            column_override_key(&source(), "epoch_ms"),
            ColumnPresentationOverride::Custom {
                renderer_id: TIMESTAMP_CONTRIBUTION_ID.to_owned(),
                options: valid_unix_ms_options().to_json(),
            },
        )],
        Vec::new(),
    );
    let state = open_state(config, &snapshot);
    assert_eq!(
        state.effective_current_override(&snapshot),
        ColumnPresentationOverride::Raw
    );
}

#[test]
fn global_options_from_snapshot() {
    let state = open_state(dialog_config(), &make_snapshot());
    let expected_global = TimestampOptions {
        interpretation: TimestampInterpretation::Native,
        format: TimestampOutputFormat::IsoTz,
        display_time_zone: TimeZoneSpec::Iana {
            name: "Asia/Shanghai".to_owned(),
        },
        source_time_zone: None,
    };
    assert_eq!(
        state.global_options(&make_snapshot()),
        Some(expected_global)
    );
    let mut bad = make_snapshot();
    bad.configs.insert(
        TIMESTAMP_CONTRIBUTION_ID.to_owned(),
        VersionedPresentationConfig {
            config_version: 1,
            config: json!({"bogus": true}),
        },
    );
    assert_eq!(state.global_options(&bad), None);
    assert_eq!(state.global_options(&snapshot_without_config()), None);
    // S1 入口同义：快照扩展没动 configs 语义。
    assert_eq!(
        global_timestamp_options_from_snapshot(&make_snapshot()).map(|options| options.format),
        Some(TimestampOutputFormat::IsoTz)
    );
}

#[test]
fn time_enabled_and_contribution_missing() {
    let state = open_state(dialog_config(), &make_snapshot());
    assert!(state.time_enabled(&make_snapshot()));
    assert!(!state.time_enabled(&snapshot_with(
        global_options_json(),
        false,
        Vec::new(),
        Vec::new()
    )));
    assert!(!state.contribution_missing(&registry_with_timestamp()));
    assert!(state.contribution_missing(&ValuePresentationRegistry::new()));
}

#[test]
fn sample_prefers_caller_value_then_fallback() {
    // 调用方给真实行值。
    let mut config = dialog_config();
    config.sample_value = Some(json!("2024-01-15T10:00:00Z"));
    let state = open_state(config, &make_snapshot());
    let (sample, has_sample) = state.sample();
    assert_eq!(sample, json!("2024-01-15T10:00:00Z"));
    assert!(has_sample);
    // 缺省时按语义种给锚点。
    let state = open_state(dialog_config(), &make_snapshot());
    let (sample, has_sample) = state.sample();
    assert_eq!(sample, json!(1705316400000i64));
    assert!(has_sample);
    assert_eq!(state.sample_text(), "1705316400000");
    // 文本列无样本。
    let mut config = dialog_config();
    config.column_name = "name".to_owned();
    config.column_type = Some("varchar".to_owned());
    let state = open_state(config, &make_snapshot());
    let (sample, has_sample) = state.sample();
    assert_eq!(sample, Value::Null);
    assert!(!has_sample);
    // 显式 null 也是样本（调用方给了值）。
    let mut config = dialog_config();
    config.sample_value = Some(Value::Null);
    let state = open_state(config, &make_snapshot());
    let (sample, has_sample) = state.sample();
    assert_eq!(sample, Value::Null);
    assert!(has_sample);
}

#[test]
fn candidate_maps_draft_modes() {
    let mut state = open_state(dialog_config(), &make_snapshot());
    assert_eq!(state.candidate(), ColumnPresentationOverride::Inherit);
    state.update_draft(ColumnPresentationDraftPatch {
        mode: Some(PresentationMode::Raw),
        ..ColumnPresentationDraftPatch::default()
    });
    assert_eq!(state.candidate(), ColumnPresentationOverride::Raw);
    state.update_draft(ColumnPresentationDraftPatch {
        mode: Some(PresentationMode::Custom),
        ..ColumnPresentationDraftPatch::default()
    });
    assert_eq!(state.candidate(), draft_to_column_override(&custom_draft()));
}

#[test]
fn draft_problems_cover_iana_branches() {
    let mut state = open_state(dialog_config(), &make_snapshot());
    state.update_draft(ColumnPresentationDraftPatch {
        mode: Some(PresentationMode::Custom),
        ..ColumnPresentationDraftPatch::default()
    });
    // 默认草稿（number 列 + unix-ms + 全局 iana Shanghai）合法。
    assert!(state.draft_problems(&known).is_empty());
    // 展示 IANA 名内空白非法。
    state.set_display_iana_name("Asia/Shang hai");
    assert_eq!(
        state.draft_problems(&known),
        vec![problem(
            "displayTimeZone.name",
            "expected a valid IANA time zone name"
        )]
    );
    // 格式合法但未知 → unknown。
    state.set_display_iana_name("Mars/Olympus");
    assert_eq!(
        state.draft_problems(&known),
        vec![problem("displayTimeZone.name", "unknown time zone name")]
    );
    // 源时区只在 text / native 下校验。
    state.set_display_iana_name("Asia/Shanghai");
    state.set_source_iana_name("Mars/Olympus");
    state.update_draft(ColumnPresentationDraftPatch {
        source_time_zone_kind: Some(SourceTimeZoneChoice::Iana),
        ..ColumnPresentationDraftPatch::default()
    });
    assert!(
        state.draft_problems(&known).is_empty(),
        "unix 解释不带源时区，不校验源 IANA 名"
    );
    state.update_draft(ColumnPresentationDraftPatch {
        interpretation: Some(InterpretationChoice::Text),
        ..ColumnPresentationDraftPatch::default()
    });
    assert_eq!(
        state.draft_problems(&known),
        vec![problem("sourceTimeZone.name", "unknown time zone name")]
    );
    // 非 custom 不校验。
    state.update_draft(ColumnPresentationDraftPatch {
        mode: Some(PresentationMode::Raw),
        ..ColumnPresentationDraftPatch::default()
    });
    assert!(state.draft_problems(&known).is_empty());
}

// ---------------------------------------------------------------------------
// 保存 / 开启
// ---------------------------------------------------------------------------

fn custom_state() -> ColumnPresentationDialogState {
    let mut state = open_state(dialog_config(), &make_snapshot());
    state.update_draft(ColumnPresentationDraftPatch {
        mode: Some(PresentationMode::Custom),
        ..ColumnPresentationDraftPatch::default()
    });
    state
}

#[test]
fn save_without_source_records_nothing() {
    let mut config = dialog_config();
    config.source = None;
    let mut state = open_state(config, &make_snapshot());
    state.save(&known);
    assert!(state.save_requests().is_empty());
    assert!(state.is_open());
    assert!(state.error().is_none());
}

#[test]
fn save_with_draft_problems_reports_in_place() {
    let mut state = custom_state();
    state.set_display_iana_name("Mars/Olympus");
    state.save(&known);
    assert!(state.save_requests().is_empty());
    assert!(state.is_open());
    assert_eq!(
        state.error(),
        Some("配置无效: displayTimeZone.name: unknown time zone name")
    );
}

#[test]
fn save_through_on_save_records_callback_and_closes() {
    let mut config = dialog_config();
    config.has_on_save = true;
    let mut state = open_state(config, &make_snapshot());
    state.update_draft(ColumnPresentationDraftPatch {
        mode: Some(PresentationMode::Custom),
        ..ColumnPresentationDraftPatch::default()
    });
    state.save(&known);
    assert_eq!(state.save_requests().len(), 1);
    assert!(matches!(
        state.save_requests()[0],
        ColumnPresentationSaveRequest::OnSave(_)
    ));
    assert!(!state.is_open());
    assert!(!state.saving());
}

#[test]
fn save_default_records_persist_and_host_answers() {
    let mut state = custom_state();
    state.save(&known);
    assert!(state.saving());
    assert!(state.is_open());
    assert_eq!(state.save_requests().len(), 1);
    let ColumnPresentationSaveRequest::Persist {
        source: saved_source,
        column_name,
        column_override,
    } = &state.save_requests()[0]
    else {
        panic!("expected a Persist request");
    };
    assert_eq!(saved_source, &source());
    assert_eq!(column_name, "epoch_ms");
    assert_eq!(column_override, &draft_to_column_override(&custom_draft()));
    // 保存中二次保存是 no-op（按钮禁用 + in-flight guard）。
    state.save(&known);
    assert_eq!(state.save_requests().len(), 1);
    // 成功 → 关闭。
    state.save_succeeded();
    assert!(!state.saving());
    assert!(!state.is_open());
}

#[test]
fn save_failure_reports_in_place_and_stays_open() {
    let mut state = custom_state();
    state.save(&known);
    state.save_failed();
    assert!(!state.saving());
    assert!(state.is_open());
    assert_eq!(
        state.error(),
        Some(t("settings.valuePresentation.saveFailed"))
    );
    // 用户可重试：错误在下一次保存尝试前由 update_draft 清掉。
    state.update_draft(ColumnPresentationDraftPatch {
        format: Some(TimestampOutputFormat::Iso),
        ..ColumnPresentationDraftPatch::default()
    });
    assert!(state.error().is_none());
}

#[test]
fn enable_through_callback_and_default_paths() {
    // 回调路径：只记 OnEnable。
    let mut config = dialog_config();
    config.has_on_enable_time_display = true;
    let mut state = open_state(config, &make_snapshot());
    state.enable_time_display(&make_snapshot());
    assert!(state.enabling());
    assert_eq!(
        state.enable_requests(),
        &[ColumnPresentationEnableRequest::OnEnable]
    );
    state.enable_succeeded();
    assert!(!state.enabling());
    assert!(state.enable_error().is_none());
    // 再次开启记第二条请求，开启中二次开启是 no-op（第二调必须在
    // enabling 为真时发，否则它就是一次新的开启）。
    state.enable_time_display(&make_snapshot());
    assert!(state.enabling());
    assert_eq!(state.enable_requests().len(), 2);
    state.enable_time_display(&make_snapshot());
    assert_eq!(state.enable_requests().len(), 2);
    // 失败就地报错，不假装成功。
    state.enable_failed();
    assert!(!state.enabling());
    assert_eq!(
        state.enable_error(),
        Some(t("settings.valuePresentation.saveFailed"))
    );
}

#[test]
fn enable_default_uses_global_options_or_native_iso_local() {
    let mut state = open_state(dialog_config(), &make_snapshot());
    state.enable_time_display(&make_snapshot());
    assert_eq!(
        state.enable_requests(),
        &[ColumnPresentationEnableRequest::Persist {
            options: TimestampOptions {
                interpretation: TimestampInterpretation::Native,
                format: TimestampOutputFormat::IsoTz,
                display_time_zone: TimeZoneSpec::Iana {
                    name: "Asia/Shanghai".to_owned(),
                },
                source_time_zone: None,
            }
        }]
    );
    // 快照里没有合法全局值 → 默认 native / iso / local。
    let mut state = open_state(dialog_config(), &make_snapshot());
    state.enable_time_display(&snapshot_without_config());
    assert_eq!(
        state.enable_requests(),
        &[ColumnPresentationEnableRequest::Persist {
            options: TimestampOptions {
                interpretation: TimestampInterpretation::Native,
                format: TimestampOutputFormat::Iso,
                display_time_zone: TimeZoneSpec::Local,
                source_time_zone: None,
            }
        }]
    );
}

// ---------------------------------------------------------------------------
// 来源标签与当前计划（T09-BUG-001）
// ---------------------------------------------------------------------------

#[test]
fn effective_source_label_by_mode_and_plan() {
    let presenter = presenter_stub();
    let registry = registry_with_timestamp();
    // custom / raw → 列覆盖，不看计划。
    let mut state = custom_state();
    let plan = state.current_plan(&make_snapshot(), &registry, Some(&presenter));
    assert_eq!(
        state.effective_source_label(&plan),
        t("columnPresentation.source.override")
    );
    state.update_draft(ColumnPresentationDraftPatch {
        mode: Some(PresentationMode::Raw),
        ..ColumnPresentationDraftPatch::default()
    });
    let plan = state.current_plan(&make_snapshot(), &registry, Some(&presenter));
    assert_eq!(
        state.effective_source_label(&plan),
        t("columnPresentation.source.override")
    );
    // inherit + 规则命中 → 规则名。
    let state = open_state(dialog_config(), &make_snapshot());
    let snapshot = snapshot_with(
        global_options_json(),
        true,
        Vec::new(),
        vec![rule_unix_ms()],
    );
    let plan = state.current_plan(&snapshot, &registry, Some(&presenter));
    assert_eq!(plan.matched_rule_name(), Some("unix-ms"));
    assert_eq!(
        state.effective_source_label(&plan),
        t("columnPresentation.source.rule").replace("{name}", "unix-ms")
    );
    // inherit + 无规则 → 全局默认。
    let plan = state.current_plan(&make_snapshot(), &registry, Some(&presenter));
    assert_eq!(
        state.effective_source_label(&plan),
        t("columnPresentation.source.global")
    );
}

#[test]
fn current_plan_inherit_ignores_stored_override() {
    // 快照里存着本列的 custom 覆盖；草稿为 inherit 时计划按「清除
    // 覆盖后」建：规则命中（而不是仍存的覆盖优先）。
    let snapshot = snapshot_with(
        global_options_json(),
        true,
        vec![(
            column_override_key(&source(), "epoch_ms"),
            ColumnPresentationOverride::Custom {
                renderer_id: TIMESTAMP_CONTRIBUTION_ID.to_owned(),
                options: valid_unix_ms_options().to_json(),
            },
        )],
        vec![rule_unix_ms()],
    );
    let presenter = presenter_stub();
    let registry = registry_with_timestamp();
    let state = open_state(dialog_config(), &snapshot);
    assert_eq!(state.draft().mode, PresentationMode::Custom);
    // 草稿切到 inherit：计划命中规则，不再是 stored custom。
    let mut state = state;
    state.update_draft(ColumnPresentationDraftPatch {
        mode: Some(PresentationMode::Inherit),
        ..ColumnPresentationDraftPatch::default()
    });
    let plan = state.current_plan(&snapshot, &registry, Some(&presenter));
    assert_eq!(plan.matched_rule_name(), Some("unix-ms"));
    assert_eq!(plan.status(), ColumnPresentationStatus::Ready);
    // 反证：不剥离直接建计划会看到 stored custom（本测试只证剥离
    // 后的计划与它不同 —— stored custom 的 options 与规则相同，
    // 所以用 matched_rule_name 区分：stored custom 的计划无规则名）。
    let direct = build_column_presentation_plan(
        &plan_column("epoch_ms", "bigint"),
        Some(&source()),
        &snapshot,
        &registry,
        Some(&presenter),
    );
    assert_eq!(direct.matched_rule_name(), None);
}

// ---------------------------------------------------------------------------
// 渲染（Runtime）
// ---------------------------------------------------------------------------

struct Scene {
    state: ColumnPresentationDialogState,
    snapshot: ValuePresentationSnapshot,
    registry: ValuePresentationRegistry,
    presenter: Option<TimestampPresenter>,
    known: Rc<dyn Fn(&str) -> bool>,
    dark: bool,
}

fn scene_view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    let k: &Kit = kit(scene.dark);
    let mut layer = Vec::new();
    let mut dialog = ColumnPresentationDialog::new(&scene.state)
        .snapshot(&scene.snapshot)
        .registry(&scene.registry)
        .is_known_time_zone(Rc::clone(&scene.known))
        .host_mut(|scene: &mut Scene| &mut scene.state);
    if let Some(presenter) = &scene.presenter {
        dialog = dialog.presenter(Rc::clone(presenter));
    }
    dialog.build(scene.dark, &mut layer);
    col()
        .key("column-presentation-page")
        .w_full()
        .h_full()
        .child(
            col()
                .key("column-presentation-page-body")
                .w_full()
                .h_full()
                .fill(k.c.bg_base),
        )
        .children(layer)
}

fn scene_runtime(scene: Scene) -> Runtime<Scene> {
    let mut runtime = Runtime::new(
        scene,
        scene_view,
        (1440.0, 900.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    runtime.frame(0.0);
    runtime
}

fn open_scene(config: ColumnPresentationDialogConfig) -> Scene {
    let snapshot = make_snapshot();
    let mut state = ColumnPresentationDialogState::new(config);
    state.open_dialog(&snapshot);
    Scene {
        state,
        snapshot,
        registry: registry_with_timestamp(),
        presenter: Some(presenter_stub()),
        known: Rc::new(known),
        dark: true,
    }
}

fn element<S>(runtime: &Runtime<S>, key: &str) -> rune::ElementInfo {
    runtime
        .elements()
        .into_iter()
        .find(|element| !element.exiting && element.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is mounted"))
}

fn maybe<S>(runtime: &Runtime<S>, key: &str) -> Option<rune::ElementInfo> {
    runtime
        .elements()
        .into_iter()
        .find(|element| !element.exiting && element.key.as_deref() == Some(key))
}

fn text_of<S>(runtime: &Runtime<S>, key: &str) -> String {
    element(runtime, key).text.unwrap_or_default()
}

fn click<S>(runtime: &mut Runtime<S>, key: &str) {
    let (x, y) = runtime
        .find(key)
        .unwrap_or_else(|| panic!("{key} is laid out"));
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    runtime.frame(0.0);
}

/// 把对话框 body 滚到底：保存/取消行在 body 流末尾，body 内容超过
/// `BODY_MAX_HEIGHT`（448）时它们在折叠下方，直接点的坐标会落在面
/// 板外（`tests/dialog.rs` 的 `scrolling_the_tall_body_to_its_end_*`
/// 同款手法：对面板中心滚轮 -400，多次直到底）。
fn scroll_dialog_body_to_end(runtime: &mut Runtime<Scene>) {
    let (bx, by) = runtime
        .find("column-presentation-dialog")
        .expect("the dialog panel");
    for _ in 0..6 {
        runtime.wheel(bx, by, -400.0);
        runtime.frame(0.0);
    }
}

fn escape(runtime: &mut Runtime<Scene>) {
    runtime.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    runtime.frame(0.0);
    runtime.frame(0.0);
}

#[test]
fn closed_dialog_mounts_nothing() {
    let scene = Scene {
        state: ColumnPresentationDialogState::new(dialog_config()),
        snapshot: make_snapshot(),
        registry: registry_with_timestamp(),
        presenter: Some(presenter_stub()),
        known: Rc::new(known),
        dark: true,
    };
    let runtime = scene_runtime(scene);
    assert!(maybe(&runtime, "column-presentation-dialog").is_none());
}

#[test]
fn open_dialog_renders_title_with_column() {
    let runtime = scene_runtime(open_scene(dialog_config()));
    assert_eq!(
        text_of(&runtime, "column-presentation-dialog-title"),
        format!("{} — epoch_ms", t("columnPresentation.title"))
    );
    assert!(maybe(&runtime, "column-presentation-dialog-body").is_some());
}

#[test]
fn not_enabled_block_shows_and_hides_with_switch() {
    let mut scene = open_scene(dialog_config());
    scene.snapshot = snapshot_with(global_options_json(), false, Vec::new(), Vec::new());
    let runtime = scene_runtime(scene);
    assert!(maybe(&runtime, "column-presentation-not-enabled").is_some());
    assert!(maybe(&runtime, "column-presentation-enable").is_some());
    assert_eq!(
        text_of(&runtime, "column-presentation-not-enabled-text"),
        t("columnPresentation.notEnabled")
    );
    // 总开关开时提示消失。
    let runtime = scene_runtime(open_scene(dialog_config()));
    assert!(maybe(&runtime, "column-presentation-not-enabled").is_none());
}

#[test]
fn contribution_missing_notice_follows_registry_while_open() {
    let mut scene = open_scene(dialog_config());
    scene.registry = ValuePresentationRegistry::new();
    let mut runtime = scene_runtime(scene);
    assert!(maybe(&runtime, "column-presentation-contribution-missing").is_some());
    // 面板打开期间重注册：提示即时消失（T11，不依赖父级快照重建）。
    // 直接改 `runtime.state` 不会脏标记，必须 `settle_all()` 强制重跑 view。
    runtime.state.registry = registry_with_timestamp();
    runtime.settle_all();
    runtime.frame(0.0);
    assert!(maybe(&runtime, "column-presentation-contribution-missing").is_none());
}

#[test]
fn effective_source_row_shows_rule_name() {
    let mut scene = open_scene(dialog_config());
    scene.snapshot = snapshot_with(
        global_options_json(),
        true,
        Vec::new(),
        vec![rule_unix_ms()],
    );
    let runtime = scene_runtime(scene);
    assert_eq!(
        text_of(&runtime, "column-presentation-effective-source"),
        format!(
            "{}: {}",
            t("columnPresentation.effectiveSource"),
            t("columnPresentation.source.rule").replace("{name}", "unix-ms")
        )
    );
}

#[test]
fn pressing_raw_mode_hides_custom_form() {
    let mut scene = open_scene(dialog_config());
    // 先切到 custom：表单出现。
    scene.state.update_draft(ColumnPresentationDraftPatch {
        mode: Some(PresentationMode::Custom),
        ..ColumnPresentationDraftPatch::default()
    });
    let mut runtime = scene_runtime(scene);
    // kit Select 挂载的 trigger key 是 `{key}-trigger`（见视图模块头的 Key 约定）。
    assert!(maybe(&runtime, "column-presentation-interpretation-trigger").is_some());
    // 点 raw（第二项）：表单消失。
    click(&mut runtime, "column-presentation-mode-item-1");
    assert!(maybe(&runtime, "column-presentation-interpretation-trigger").is_none());
    assert_eq!(runtime.state.state.draft().mode, PresentationMode::Raw);
}

#[test]
fn custom_form_shows_iana_input_when_display_kind_is_iana() {
    let mut scene = open_scene(dialog_config());
    scene.state.update_draft(ColumnPresentationDraftPatch {
        mode: Some(PresentationMode::Custom),
        ..ColumnPresentationDraftPatch::default()
    });
    let mut runtime = scene_runtime(scene);
    // 全局默认显示时区就是 IANA Asia/Shanghai，所以切到 custom 后 IANA
    // 输入一开始就在；先切 local 让它消失，再切回 iana 让它出现。
    assert!(maybe(&runtime, "column-presentation-display-iana-name").is_some());
    runtime
        .state
        .state
        .choose_select(ColumnPresentationSelect::DisplayTimeZone, "local");
    runtime.settle_all();
    runtime.frame(0.0);
    assert!(maybe(&runtime, "column-presentation-display-iana-name").is_none());
    runtime
        .state
        .state
        .choose_select(ColumnPresentationSelect::DisplayTimeZone, "iana");
    runtime.settle_all();
    runtime.frame(0.0);
    assert!(maybe(&runtime, "column-presentation-display-iana-name").is_some());
    // 源时区选择只在 text / native 下出现；unix-ms 下没有（trigger key 同理）。
    assert!(maybe(&runtime, "column-presentation-source-timezone-trigger").is_none());
}

#[test]
fn preview_shows_original_and_stub_result() {
    let mut scene = open_scene(dialog_config());
    scene.snapshot = snapshot_with(
        global_options_json(),
        true,
        Vec::new(),
        vec![rule_unix_ms()],
    );
    let runtime = scene_runtime(scene);
    assert_eq!(
        text_of(&runtime, "column-presentation-original"),
        "1705316400000"
    );
    assert_eq!(
        text_of(&runtime, "column-presentation-preview-result"),
        "2024-01-15T11:00:00.000Z"
    );
}

#[test]
fn no_sample_state_for_text_column() {
    let mut config = dialog_config();
    config.column_name = "name".to_owned();
    config.column_type = Some("varchar".to_owned());
    let runtime = scene_runtime(open_scene(config));
    assert_eq!(
        text_of(&runtime, "column-presentation-preview-result"),
        t("columnPresentation.preview.noSample")
    );
}

#[test]
fn no_source_note_and_save_press_records_nothing() {
    let mut config = dialog_config();
    config.source = None;
    let mut runtime = scene_runtime(open_scene(config));
    assert!(maybe(&runtime, "column-presentation-no-source").is_some());
    click(&mut runtime, "column-presentation-save");
    assert!(runtime.state.state.save_requests().is_empty());
}

#[test]
fn pressing_save_with_on_save_closes_dialog() {
    let mut config = dialog_config();
    config.has_on_save = true;
    let mut scene = open_scene(config);
    scene.state.update_draft(ColumnPresentationDraftPatch {
        mode: Some(PresentationMode::Custom),
        ..ColumnPresentationDraftPatch::default()
    });
    let mut runtime = scene_runtime(scene);
    scroll_dialog_body_to_end(&mut runtime);
    click(&mut runtime, "column-presentation-save");
    assert!(maybe(&runtime, "column-presentation-dialog").is_none());
    assert_eq!(runtime.state.state.save_requests().len(), 1);
}

#[test]
fn pressing_enable_records_enable_request() {
    let mut scene = open_scene(dialog_config());
    scene.snapshot = snapshot_with(global_options_json(), false, Vec::new(), Vec::new());
    let mut runtime = scene_runtime(scene);
    click(&mut runtime, "column-presentation-enable");
    assert_eq!(runtime.state.state.enable_requests().len(), 1);
    assert!(matches!(
        runtime.state.state.enable_requests()[0],
        ColumnPresentationEnableRequest::Persist { .. }
    ));
}

#[test]
fn save_failure_shows_error_in_place() {
    let mut scene = open_scene(dialog_config());
    scene.state.update_draft(ColumnPresentationDraftPatch {
        mode: Some(PresentationMode::Custom),
        ..ColumnPresentationDraftPatch::default()
    });
    let mut runtime = scene_runtime(scene);
    runtime.state.state.save(&known);
    runtime.state.state.save_failed();
    // 直接调状态机不经过 runtime 事件，必须 `settle_all()` 强制重跑 view。
    runtime.settle_all();
    runtime.frame(0.0);
    assert!(maybe(&runtime, "column-presentation-error").is_some());
    assert!(maybe(&runtime, "column-presentation-dialog").is_some());
}

#[test]
fn unix_badge_shows_for_unix_milliseconds() {
    let mut scene = open_scene(dialog_config());
    scene.state.update_draft(ColumnPresentationDraftPatch {
        mode: Some(PresentationMode::Custom),
        ..ColumnPresentationDraftPatch::default()
    });
    let runtime = scene_runtime(scene);
    assert_eq!(
        text_of(&runtime, "column-presentation-preview-unix-badge"),
        t("settings.valuePresentation.preview.unixMilliseconds")
    );
}

#[test]
fn escape_closes_dialog() {
    let mut runtime = scene_runtime(open_scene(dialog_config()));
    assert!(maybe(&runtime, "column-presentation-dialog").is_some());
    escape(&mut runtime);
    assert!(maybe(&runtime, "column-presentation-dialog").is_none());
}
