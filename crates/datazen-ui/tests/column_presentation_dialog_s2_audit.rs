//! T054-S2 独立审计：`ColumnPresentationDialog` 对话框组件 + 状态 +
//! 保存/开启接线 + 渲染的 tester 测试。
//!
//! 与 coder 测试（`column_presentation_dialog_s2.rs`，52 个）独立编写：
//! 不复用它的 helper 与用例，只以 React 原件
//! `packages/frontend/src/components/DataTable/ColumnPresentationDialog.tsx`
//! 与 `lib/valuePresentation/`（`presentationPlan.ts`、
//! `columnPresentation.ts`、`valuePresentationRules.ts`、
//! `columnOverrides.ts`、`timestampConfig.ts`）为对照，用自选的列名、
//! 规则名与样本值逐项断言：
//!
//! - U+FEFF：rendererId 一处已按 JS 空白对齐（pin 住）；IANA 草稿路径
//!   仍用 Rust `trim()`，FEFF 前缀名在 TS 下合法、在 Rust 下被拒——
//!   已知非阻塞遗留（fail-closed 方向），本文件把当前行为 pin 住，
//!   见 `audit_iana_feff_prefix_is_still_rejected`；
//! - 计划决策顺序 a–e、预览四种结果、T09-BUG-001 剥离、规则首条冻结；
//! - regex 走 bytes API：`.*` 可编译、`\d` 只认 ASCII、未知 flag 不匹配；
//! - 保存/开启 outbox 状态机（含进行中 no-op 两处有意偏差）；
//! - 渲染：提示行、来源标签插值、custom 表单显隐、预览框、无来源禁用。

use std::cell::RefCell;
use std::rc::Rc;

use serde_json::{json, Value};

use datazen_ui::test_support::*;

use rune::{col, Cx, El, Key, KeyEvent, Mods};
use rune_kit::Kit;

use datazen_ui::components::data_table::column_presentation_dialog::{
    draft_to_column_override, draft_to_timestamp_options, problem,
    validate_column_presentation_draft, validate_column_presentation_override,
    ColumnPresentationDraft, ColumnPresentationOverride, InterpretationChoice, PresentationMode,
    PresentationSemanticKind, SourceTimeZoneChoice, TimeZoneKindChoice, TimeZoneSpec,
    TimestampInterpretation, TimestampOptions, TimestampOutputFormat, UnixUnit,
    ValuePresentationSnapshot, VersionedPresentationConfig, TIMESTAMP_CONTRIBUTION_ID,
};
use datazen_ui::components::data_table::column_presentation_dialog_state::{
    ColumnPresentationDialogConfig, ColumnPresentationDialogState, ColumnPresentationDraftPatch,
    ColumnPresentationEnableRequest, ColumnPresentationSaveRequest, ColumnPresentationSelect,
};
use datazen_ui::components::data_table::column_presentation_dialog_view::ColumnPresentationDialog;
use datazen_ui::components::data_table::column_presentation_plan::{
    build_column_presentation_plan, column_override_key, compute_column_presentation_preview,
    fallback_sample_for_column, resolve_presentation_semantic_kind,
    snapshot_without_column_override, ColumnPresentationPreview, ColumnPresentationStatus,
    ColumnSource, PresentationEnvironment, PresentationPlanColumn, PresentationResult,
    PresentationValue, TimestampPresenter,
};
use datazen_ui::components::data_table::column_presentation_rules::{
    find_first_matching_rule, match_name_pattern, ValuePresentationContribution,
    ValuePresentationNamePattern, ValuePresentationRegistry, ValuePresentationRule,
    ValuePresentationRuleContext, ValuePresentationRuleMatch,
};
use datazen_ui::i18n::t;
use datazen_ui::ui::kit::kit;

// ---------------------------------------------------------------------------
// 桩与构造器（自写；列名 / 规则名 / 样本值与 coder 测试不同）
// ---------------------------------------------------------------------------

/// 桩已知性：只认 Asia/Shanghai 与 Europe/London。
fn known(name: &str) -> bool {
    matches!(name, "Asia/Shanghai" | "Europe/London")
}

fn unix_seconds_options() -> TimestampOptions {
    TimestampOptions {
        interpretation: TimestampInterpretation::Unix(UnixUnit::Seconds),
        format: TimestampOutputFormat::Iso,
        display_time_zone: TimeZoneSpec::Utc,
        source_time_zone: None,
    }
}

fn native_shanghai_options() -> TimestampOptions {
    TimestampOptions {
        interpretation: TimestampInterpretation::Native,
        format: TimestampOutputFormat::IsoTz,
        display_time_zone: TimeZoneSpec::Iana {
            name: "Asia/Shanghai".to_owned(),
        },
        source_time_zone: None,
    }
}

fn global_json() -> Value {
    json!({
        "interpretation": { "kind": "native" },
        "format": "iso-tz",
        "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
    })
}

/// 与 coder 的 conn-1/shop/public/orders 不同的来源。
fn src() -> ColumnSource {
    ColumnSource {
        connection_id: "conn-9".to_owned(),
        database: Some("billing".to_owned()),
        namespace: None,
        table: "invoices".to_owned(),
    }
}

fn snap(
    enabled: bool,
    overrides: Vec<(String, ColumnPresentationOverride)>,
    rules: Vec<ValuePresentationRule>,
) -> ValuePresentationSnapshot {
    ValuePresentationSnapshot {
        revision: 7,
        environment: PresentationEnvironment::new("en-US", "UTC"),
        configs: [(
            TIMESTAMP_CONTRIBUTION_ID.to_owned(),
            VersionedPresentationConfig {
                config_version: 1,
                config: global_json(),
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
    }
}

fn plain_snapshot() -> ValuePresentationSnapshot {
    snap(true, Vec::new(), Vec::new())
}

fn col_created() -> PresentationPlanColumn {
    PresentationPlanColumn {
        name: "created_at".to_owned(),
        data_type: Some("timestamptz".to_owned()),
        hints: None,
    }
}

fn col_amount() -> PresentationPlanColumn {
    PresentationPlanColumn {
        name: "amount_cents".to_owned(),
        data_type: Some("bigint".to_owned()),
        hints: None,
    }
}

/// 命中 created_at 的规则（unix 秒），与 coder 的 epoch_ms 规则不同。
fn rule_created_unix_seconds() -> ValuePresentationRule {
    ValuePresentationRule {
        name: "created-unix-seconds".to_owned(),
        enabled: true,
        match_: ValuePresentationRuleMatch {
            column: Some(ValuePresentationNamePattern::Exact("created_at".to_owned())),
            ..ValuePresentationRuleMatch::default()
        },
        options: unix_seconds_options(),
    }
}

/// 记录每次 presenter 调用的桩：返回可识别的固定文本，不冒充时间引擎。
struct Recorder {
    calls: RefCell<Vec<(TimestampOptions, PresentationSemanticKind, PresentationValue)>>,
}

impl Recorder {
    fn presenter(self: &Rc<Self>) -> TimestampPresenter {
        let recorder = Rc::clone(self);
        Rc::new(
            move |options: &TimestampOptions,
                  kind: PresentationSemanticKind,
                  sample: &PresentationValue,
                  _env: &PresentationEnvironment| {
                recorder.calls.borrow_mut().push((
                    options.clone(),
                    kind,
                    sample.clone(),
                ));
                PresentationResult::Text {
                    text: format!("presented:{:?}", options.interpretation),
                    interpretation: Some("audit-stub".to_owned()),
                }
            },
        )
    }
}

fn registry() -> ValuePresentationRegistry {
    ValuePresentationRegistry::with_timestamp()
}

fn cfg() -> ColumnPresentationDialogConfig {
    ColumnPresentationDialogConfig {
        column_name: "created_at".to_owned(),
        column_type: Some("timestamptz".to_owned()),
        cell_type_hints: None,
        source: Some(src()),
        sample_value: None,
        current_override: None,
        has_on_save: false,
        has_on_enable_time_display: false,
    }
}

fn open_state(config: ColumnPresentationDialogConfig, snapshot: &ValuePresentationSnapshot) -> ColumnPresentationDialogState {
    let mut state = ColumnPresentationDialogState::new(config);
    state.open_dialog(snapshot);
    state
}

fn custom_draft() -> ColumnPresentationDraft {
    ColumnPresentationDraft {
        mode: PresentationMode::Custom,
        interpretation: InterpretationChoice::UnixSeconds,
        format: TimestampOutputFormat::Iso,
        display_time_zone_kind: TimeZoneKindChoice::Utc,
        display_iana_name: "Asia/Shanghai".to_owned(),
        source_time_zone_kind: SourceTimeZoneChoice::None,
        source_iana_name: "Asia/Shanghai".to_owned(),
    }
}

fn override_custom_unix_seconds() -> ColumnPresentationOverride {
    ColumnPresentationOverride::Custom {
        renderer_id: TIMESTAMP_CONTRIBUTION_ID.to_owned(),
        options: unix_seconds_options().to_json(),
    }
}

fn override_key_for(column: &str) -> String {
    column_override_key(&src(), column)
}

// ---------------------------------------------------------------------------
// U+FEFF：rendererId 一处已修；IANA 草稿路径仍为遗留（非阻塞）
// ---------------------------------------------------------------------------

#[test]
fn audit_renderer_id_feff_trailing_trims_like_js() {
    // TS：`"datazen.timestamp\u{FEFF}".trim()` → 合法名；Rust 旧实现
    // 会因 FEFF 不在 is_whitespace 内而报 unknown renderer。
    let input = json!({
        "mode": "custom",
        "rendererId": "datazen.timestamp\u{feff}",
        "options": global_json(),
    });
    let validated = validate_column_presentation_override(&input)
        .expect("FEFF-trimmed rendererId validates");
    assert_eq!(
        validated,
        ColumnPresentationOverride::Custom {
            renderer_id: TIMESTAMP_CONTRIBUTION_ID.to_owned(),
            options: native_shanghai_options().to_json(),
        }
    );
}

#[test]
fn audit_renderer_id_feff_only_is_empty() {
    let input = json!({
        "mode": "custom",
        "rendererId": "\u{feff}",
        "options": global_json(),
    });
    assert_eq!(
        validate_column_presentation_override(&input).unwrap_err(),
        vec![problem("rendererId", "must not be empty")]
    );
}

#[test]
fn audit_renderer_id_feff_inside_is_whitespace() {
    let input = json!({
        "mode": "custom",
        "rendererId": "datazen.\u{feff}timestamp",
        "options": global_json(),
    });
    assert_eq!(
        validate_column_presentation_override(&input).unwrap_err(),
        vec![problem("rendererId", "must not contain whitespace")]
    );
}

#[test]
fn audit_iana_feff_prefix_is_still_rejected() {
    // 非阻塞遗留（S1 发现的同类问题只修了 rendererId 一处）：
    // TS 的 `name.trim()` 会剥掉首部 U+FEFF 再判格式，`"\u{FEFF}Asia/Shanghai"`
    // 合法；Rust 的 `str::trim` 不含 FEFF，于是 `is_valid_iana_time_zone_name`
    // 把它拒掉。方向是 fail-closed（多拒、不少放），只在粘贴带 BOM 的
    // IANA 名时才触发；此处把当前行为 pin 住，修不修由 coordinator 定。
    let mut draft = custom_draft();
    draft.interpretation = InterpretationChoice::Native;
    draft.display_time_zone_kind = TimeZoneKindChoice::Iana;
    draft.display_iana_name = "\u{feff}Asia/Shanghai".to_owned();
    let problems = validate_column_presentation_draft(&draft, &known);
    assert_eq!(
        problems,
        vec![problem(
            "displayTimeZone.name",
            "expected a valid IANA time zone name"
        )]
    );
    // 同一名字不带 FEFF 时放行，证明拒的是 FEFF 而不是名字本身。
    draft.display_iana_name = "Asia/Shanghai".to_owned();
    assert!(validate_column_presentation_draft(&draft, &known).is_empty());
    // draft_to_timestamp_options 的 trim 同样留住了 FEFF（与 TS 不同）。
    let mut feff_draft = custom_draft();
    feff_draft.interpretation = InterpretationChoice::Native;
    feff_draft.display_time_zone_kind = TimeZoneKindChoice::Iana;
    feff_draft.display_iana_name = "\u{feff}Asia/Shanghai".to_owned();
    let options = draft_to_timestamp_options(&feff_draft);
    assert_eq!(
        options.display_time_zone,
        TimeZoneSpec::Iana {
            name: "\u{feff}Asia/Shanghai".to_owned()
        }
    );
}

// ---------------------------------------------------------------------------
// 计划决策顺序 a–e
// ---------------------------------------------------------------------------

#[test]
fn audit_raw_override_beats_matching_rule() {
    let snapshot = snap(
        true,
        vec![(override_key_for("created_at"), ColumnPresentationOverride::Raw)],
        vec![rule_created_unix_seconds()],
    );
    let plan = build_column_presentation_plan(
        &col_created(),
        Some(&src()),
        &snapshot,
        &registry(),
        None,
    );
    assert_eq!(plan.status(), ColumnPresentationStatus::Raw);
    assert_eq!(
        plan.present(&json!("2026-01-15T14:30:00Z")),
        PresentationResult::Unchanged {
            reason: Some("raw-override".to_owned())
        }
    );
}

#[test]
fn audit_custom_override_bypasses_disabled_switch() {
    // 总开关关闭时 custom 仍 ready（bypass enabled 门禁）；规则/默认
    // 路由在同一快照下会被 disabled 拦住。
    let snapshot = snap(
        false,
        vec![(override_key_for("created_at"), override_custom_unix_seconds())],
        Vec::new(),
    );
    let recorder = Rc::new(Recorder {
        calls: RefCell::new(Vec::new()),
    });
    let presenter = recorder.presenter();
    let plan = build_column_presentation_plan(
        &col_created(),
        Some(&src()),
        &snapshot,
        &registry(),
        Some(&presenter),
    );
    assert_eq!(plan.status(), ColumnPresentationStatus::Ready);
    assert_eq!(plan.renderer_id(), Some(TIMESTAMP_CONTRIBUTION_ID));
    assert_eq!(recorder.calls.borrow().len(), 0);
    let presented = plan.present(&json!(1705316400i64));
    assert!(matches!(presented, PresentationResult::Text { .. }));
    assert_eq!(recorder.calls.borrow().len(), 1);
    assert_eq!(
        recorder.calls.borrow()[0].0.interpretation,
        TimestampInterpretation::Unix(UnixUnit::Seconds)
    );
}

#[test]
fn audit_rule_beats_global_default_and_freezes() {
    // 两条都命中的规则：首条冻结，presenter 只收到首条的 options。
    let mut second = rule_created_unix_seconds();
    second.name = "second-rule".to_owned();
    second.options = native_shanghai_options();
    let snapshot = snap(
        true,
        Vec::new(),
        vec![rule_created_unix_seconds(), second],
    );
    let recorder = Rc::new(Recorder {
        calls: RefCell::new(Vec::new()),
    });
    let presenter = recorder.presenter();
    let plan = build_column_presentation_plan(
        &col_created(),
        Some(&src()),
        &snapshot,
        &registry(),
        Some(&presenter),
    );
    assert_eq!(plan.status(), ColumnPresentationStatus::Ready);
    assert_eq!(plan.matched_rule_name(), Some("created-unix-seconds"));
    let _ = plan.present(&json!("2026-01-15T14:30:00Z"));
    assert_eq!(recorder.calls.borrow().len(), 1);
    assert_eq!(
        recorder.calls.borrow()[0].0,
        unix_seconds_options(),
        "first matching rule's options win, not the global default"
    );
}

#[test]
fn audit_default_route_branches() {
    let reg = registry();
    // 非时间列 → default（不看开关与配置）。
    let plan = build_column_presentation_plan(
        &col_amount(),
        Some(&src()),
        &plain_snapshot(),
        &reg,
        None,
    );
    assert_eq!(plan.status(), ColumnPresentationStatus::Default);
    assert_eq!(
        plan.present(&json!(1705316400000i64)),
        PresentationResult::Unchanged {
            reason: Some("no-renderer".to_owned())
        }
    );
    // 时间列 + 开关关闭 → disabled。
    let plan = build_column_presentation_plan(
        &col_created(),
        Some(&src()),
        &snap(false, Vec::new(), Vec::new()),
        &reg,
        None,
    );
    assert_eq!(plan.status(), ColumnPresentationStatus::Disabled);
    assert_eq!(plan.reason(), Some("disabled"));
    // 时间列 + 开关开 + 无 configs → missing-config。
    let mut no_config = plain_snapshot();
    no_config.configs.clear();
    let plan = build_column_presentation_plan(&col_created(), Some(&src()), &no_config, &reg, None);
    assert_eq!(plan.status(), ColumnPresentationStatus::MissingConfig);
    // 时间列 + 配置版本不一致 → invalid-config。
    let mut wrong_version = plain_snapshot();
    if let Some(config) = wrong_version.configs.get_mut(TIMESTAMP_CONTRIBUTION_ID) {
        config.config_version = 99;
    }
    let plan =
        build_column_presentation_plan(&col_created(), Some(&src()), &wrong_version, &reg, None);
    assert_eq!(plan.status(), ColumnPresentationStatus::InvalidConfig);
    // 时间列 + 贡献缺失 → missing-renderer。
    let plan = build_column_presentation_plan(
        &col_created(),
        Some(&src()),
        &plain_snapshot(),
        &ValuePresentationRegistry::new(),
        None,
    );
    assert_eq!(plan.status(), ColumnPresentationStatus::MissingRenderer);
}

#[test]
fn audit_no_presenter_ready_plan_falls_back_plan_error() {
    let plan = build_column_presentation_plan(
        &col_created(),
        Some(&src()),
        &plain_snapshot(),
        &registry(),
        None,
    );
    assert_eq!(plan.status(), ColumnPresentationStatus::Ready);
    // 没有时间引擎时不编造格式化文本：present 按 TS 防御回退走 unchanged。
    assert_eq!(
        plan.present(&json!("2026-01-15T14:30:00Z")),
        PresentationResult::Unchanged {
            reason: Some("plan-error".to_owned())
        }
    );
}

#[test]
fn audit_nontimestamp_registered_override_is_error() {
    let mut reg = ValuePresentationRegistry::new();
    reg.register(ValuePresentationContribution {
        id: "other.renderer".to_owned(),
        config_version: 1,
    });
    let snapshot = snap(
        true,
        vec![(
            override_key_for("created_at"),
            ColumnPresentationOverride::Custom {
                renderer_id: "other.renderer".to_owned(),
                options: global_json(),
            },
        )],
        Vec::new(),
    );
    let plan = build_column_presentation_plan(&col_created(), Some(&src()), &snapshot, &reg, None);
    assert_eq!(plan.status(), ColumnPresentationStatus::Error);
    assert_eq!(
        plan.present(&json!("2026-01-15T14:30:00Z")),
        PresentationResult::Unchanged {
            reason: Some("plan-error".to_owned())
        }
    );
}

#[test]
fn audit_snapshot_strip_removes_key_without_touching_original() {
    let snapshot = snap(
        true,
        vec![(override_key_for("created_at"), override_custom_unix_seconds())],
        Vec::new(),
    );
    let stripped = snapshot_without_column_override(&snapshot, Some(&src()), "created_at");
    assert!(!stripped.overrides.contains_key(&override_key_for("created_at")));
    assert!(
        snapshot.overrides.contains_key(&override_key_for("created_at")),
        "original snapshot is untouched"
    );
    // 无来源列返回原快照的等价副本。
    let same = snapshot_without_column_override(&snapshot, None, "created_at");
    assert_eq!(same, snapshot);
}

// ---------------------------------------------------------------------------
// 预览四种结果 + T09-BUG-001
// ---------------------------------------------------------------------------

#[test]
fn audit_preview_inherit_uses_stripped_snapshot_not_stored_override() {
    // 快照仍存 custom 覆盖（unix 秒）+ 命中规则（native 上海）：
    // inherit 预览必须走剥离后的计划（规则），不是仍存的覆盖。
    let snapshot = snap(
        true,
        vec![(override_key_for("created_at"), override_custom_unix_seconds())],
        vec![{
            let mut rule = rule_created_unix_seconds();
            rule.options = native_shanghai_options();
            rule
        }],
    );
    let recorder = Rc::new(Recorder {
        calls: RefCell::new(Vec::new()),
    });
    let presenter = recorder.presenter();
    let preview = compute_column_presentation_preview(
        &ColumnPresentationOverride::Inherit,
        &col_created(),
        Some(&src()),
        &snapshot,
        json!("2026-01-15T14:30:00Z"),
        &registry(),
        Some(&presenter),
    );
    assert_eq!(recorder.calls.borrow().len(), 1);
    assert_eq!(
        recorder.calls.borrow()[0].0,
        native_shanghai_options(),
        "inherit preview must use the rule options after stripping the override"
    );
    assert!(matches!(preview, ColumnPresentationPreview::Text { .. }));
}

#[test]
fn audit_preview_raw_never_calls_presenter() {
    let recorder = Rc::new(Recorder {
        calls: RefCell::new(Vec::new()),
    });
    let presenter = recorder.presenter();
    let preview = compute_column_presentation_preview(
        &ColumnPresentationOverride::Raw,
        &col_created(),
        Some(&src()),
        &plain_snapshot(),
        json!("2026-01-15T14:30:00Z"),
        &registry(),
        Some(&presenter),
    );
    assert_eq!(recorder.calls.borrow().len(), 0);
    assert_eq!(
        preview,
        ColumnPresentationPreview::Unchanged {
            reason: Some("raw-override".to_owned()),
            sample: json!("2026-01-15T14:30:00Z"),
        }
    );
}

#[test]
fn audit_preview_custom_missing_contribution_is_unavailable() {
    let preview = compute_column_presentation_preview(
        &override_custom_unix_seconds(),
        &col_created(),
        Some(&src()),
        &plain_snapshot(),
        json!("2026-01-15T14:30:00Z"),
        &ValuePresentationRegistry::new(),
        None,
    );
    assert_eq!(
        preview,
        ColumnPresentationPreview::Unavailable {
            sample: json!("2026-01-15T14:30:00Z"),
        }
    );
}

#[test]
fn audit_preview_custom_invalid_options_is_invalid() {
    let bad = ColumnPresentationOverride::Custom {
        renderer_id: TIMESTAMP_CONTRIBUTION_ID.to_owned(),
        options: json!({ "interpretation": { "kind": "unix" }, "format": "iso-tz",
            "displayTimeZone": { "kind": "local" } }),
    };
    let preview = compute_column_presentation_preview(
        &bad,
        &col_created(),
        Some(&src()),
        &plain_snapshot(),
        json!(1705316400i64),
        &registry(),
        None,
    );
    match preview {
        ColumnPresentationPreview::Invalid { problems, sample } => {
            assert_eq!(sample, json!(1705316400i64));
            assert!(
                problems.iter().any(|p| p.path == "interpretation.unit"),
                "missing unix unit must be reported, got {problems:?}"
            );
        }
        other => panic!("expected Invalid preview, got {other:?}"),
    }
}

#[test]
fn audit_preview_custom_without_presenter_is_plan_error() {
    let preview = compute_column_presentation_preview(
        &override_custom_unix_seconds(),
        &col_created(),
        Some(&src()),
        &plain_snapshot(),
        json!(1705316400i64),
        &registry(),
        None,
    );
    assert_eq!(
        preview,
        ColumnPresentationPreview::Unchanged {
            reason: Some("plan-error".to_owned()),
            sample: json!(1705316400i64),
        }
    );
}

#[test]
fn audit_fallback_samples_are_prd_anchors() {
    // PRD §3.2 双锚点：number → 1705316400000；instant → 带 Z 的 ISO 串。
    assert_eq!(
        fallback_sample_for_column(&col_amount()),
        Some(json!(1705316400000i64))
    );
    assert_eq!(
        fallback_sample_for_column(&col_created()),
        Some(json!("2026-01-15T14:30:00Z"))
    );
    let text_col = PresentationPlanColumn {
        name: "label".to_owned(),
        data_type: Some("varchar".to_owned()),
        hints: None,
    };
    assert_eq!(fallback_sample_for_column(&text_col), None);
    assert_eq!(
        resolve_presentation_semantic_kind(Some("timestamptz"), None),
        PresentationSemanticKind::Instant
    );
}

// ---------------------------------------------------------------------------
// regex：bytes API 语义
// ---------------------------------------------------------------------------

#[test]
fn audit_regex_dot_star_compiles_under_bytes_api() {
    // coder 报告的 str-API 陷阱：`RegexBuilder::unicode(false)` 在 str
    // API 下会把 `.*` 认作可匹配非法 UTF-8 而拒绝编译；bytes API 必须过。
    let pattern = ValuePresentationNamePattern::Regex {
        source: "^epoch_.*".to_owned(),
        flags: None,
    };
    assert!(match_name_pattern(&pattern, "epoch_ms"));
    assert!(!match_name_pattern(&pattern, "created_at"));
}

#[test]
fn audit_regex_classes_are_ascii_only_like_js() {
    // JS 的 `\d`（无 /u 仍按 ASCII）不认全角数字；bytes + unicode(false)
    // 必须同样只认 ASCII。
    let digits = ValuePresentationNamePattern::Regex {
        source: "^\\d+$".to_owned(),
        flags: None,
    };
    assert!(match_name_pattern(&digits, "20260115"));
    assert!(!match_name_pattern(&digits, "２０２６０１１５"));
    let word = ValuePresentationNamePattern::Regex {
        source: "^\\w+$".to_owned(),
        flags: None,
    };
    assert!(match_name_pattern(&word, "order_1"));
    assert!(!match_name_pattern(&word, "café"));
}

#[test]
fn audit_regex_flags_and_bad_patterns() {
    let insensitive = ValuePresentationNamePattern::Regex {
        source: "epoch".to_owned(),
        flags: Some("i".to_owned()),
    };
    assert!(match_name_pattern(&insensitive, "EPOCH_MS"));
    let plain = ValuePresentationNamePattern::Regex {
        source: "epoch".to_owned(),
        flags: None,
    };
    assert!(!match_name_pattern(&plain, "EPOCH_MS"));
    // 未知 flag 按不匹配（TS 的 `new RegExp(source, flags)` 会抛 → catch → false）。
    let unknown_flag = ValuePresentationNamePattern::Regex {
        source: "epoch".to_owned(),
        flags: Some("g".to_owned()),
    };
    assert!(!match_name_pattern(&unknown_flag, "epoch_ms"));
    // 编译失败按不匹配（TS 防御性 catch）。
    let broken = ValuePresentationNamePattern::Regex {
        source: "(".to_owned(),
        flags: None,
    };
    assert!(!match_name_pattern(&broken, "anything"));
    // 精确模式全等。
    let exact = ValuePresentationNamePattern::Exact("created_at".to_owned());
    assert!(match_name_pattern(&exact, "created_at"));
    assert!(!match_name_pattern(&exact, "created_at_2"));
}

#[test]
fn audit_first_matching_rule_skips_disabled_and_table_needs_source() {
    let mut disabled = rule_created_unix_seconds();
    disabled.enabled = false;
    let mut second = rule_created_unix_seconds();
    second.name = "second-rule".to_owned();
    let context = ValuePresentationRuleContext {
        connection_id: Some("conn-9"),
        database: Some("billing"),
        namespace: None,
        table: Some("invoices"),
        column: "created_at",
        database_type_name: Some("timestamptz"),
    };
    let rules = [disabled, second.clone()];
    let found = find_first_matching_rule(&rules, &context);
    assert_eq!(found, Some(&second));
    // 表级条件要求来源在场：source 缺省时 table 条件不满足。
    let table_rule = ValuePresentationRule {
        name: "table-rule".to_owned(),
        enabled: true,
        match_: ValuePresentationRuleMatch {
            table: Some(ValuePresentationNamePattern::Exact("invoices".to_owned())),
            ..ValuePresentationRuleMatch::default()
        },
        options: unix_seconds_options(),
    };
    let no_source = ValuePresentationRuleContext {
        connection_id: None,
        database: None,
        namespace: None,
        table: None,
        column: "created_at",
        database_type_name: Some("timestamptz"),
    };
    assert_eq!(
        find_first_matching_rule(std::slice::from_ref(&table_rule), &no_source),
        None
    );
}

// ---------------------------------------------------------------------------
// 保存 / 开启 outbox 状态机
// ---------------------------------------------------------------------------

fn open_custom_state(has_on_save: bool) -> ColumnPresentationDialogState {
    let mut config = cfg();
    config.has_on_save = has_on_save;
    let snapshot = plain_snapshot();
    let mut state = ColumnPresentationDialogState::new(config);
    state.open_dialog(&snapshot);
    state.update_draft(ColumnPresentationDraftPatch {
        mode: Some(PresentationMode::Custom),
        ..ColumnPresentationDraftPatch::default()
    });
    state
}

#[test]
fn audit_save_persist_flow_and_inflight_noop() {
    let mut state = open_custom_state(false);
    // custom 草稿默认 unix-ms + 全局 IANA 上海：校验应放行。
    assert!(state.draft_problems(&known).is_empty());
    state.save(&known);
    assert!(state.saving());
    assert_eq!(state.save_requests().len(), 1);
    assert!(matches!(
        state.save_requests()[0],
        ColumnPresentationSaveRequest::Persist { .. }
    ));
    // 有意偏差 (a)：saving 进行中 save() 是 no-op，不重复记请求。
    state.save(&known);
    assert_eq!(state.save_requests().len(), 1);
    // 宿主回答失败：就地报错、面板保持打开、可重试。
    state.save_failed();
    assert!(!state.saving());
    assert!(state.is_open());
    assert_eq!(
        state.error(),
        Some(t("settings.valuePresentation.saveFailed"))
    );
    // 重试后成功：关闭。
    state.save(&known);
    assert_eq!(state.save_requests().len(), 2);
    state.save_succeeded();
    assert!(!state.saving());
    assert!(!state.is_open());
}

#[test]
fn audit_save_on_save_path_closes_at_once_without_saving_flag() {
    let mut state = open_custom_state(true);
    state.save(&known);
    assert!(!state.saving(), "onSave path never sets saving");
    assert!(!state.is_open());
    assert_eq!(state.save_requests().len(), 1);
    assert!(matches!(
        state.save_requests()[0],
        ColumnPresentationSaveRequest::OnSave(_)
    ));
}

#[test]
fn audit_save_blocked_without_source_and_with_draft_problems() {
    // 无来源：按钮已禁用，save() 防御性返回，不记请求。
    let mut config = cfg();
    config.source = None;
    let snapshot = plain_snapshot();
    let mut state = open_state(config, &snapshot);
    state.save(&known);
    assert!(state.save_requests().is_empty());
    assert!(state.is_open());
    // 草稿问题：就地报错、不关闭、不记请求。
    let mut state = open_custom_state(false);
    state.update_draft(ColumnPresentationDraftPatch {
        display_time_zone_kind: Some(TimeZoneKindChoice::Iana),
        ..ColumnPresentationDraftPatch::default()
    });
    state.set_display_iana_name("Not A Zone!");
    assert!(!state.draft_problems(&known).is_empty());
    state.save(&known);
    assert!(state.save_requests().is_empty());
    assert!(state.is_open());
    assert!(state.error().is_some_and(|e| e.contains("displayTimeZone.name")));
}

#[test]
fn audit_enable_options_reuse_global_or_default() {
    let mut config = cfg();
    config.has_on_enable_time_display = false;
    let snapshot = plain_snapshot();
    let mut state = open_state(config, &snapshot);
    // 快照全局合法 → 沿用 native / iso-tz / IANA 上海。
    state.enable_time_display(&snapshot);
    assert!(state.enabling());
    assert_eq!(state.enable_requests().len(), 1);
    assert_eq!(
        state.enable_requests()[0],
        ColumnPresentationEnableRequest::Persist {
            options: native_shanghai_options()
        }
    );
    // 有意偏差 (a)：enabling 进行中再次调用是 no-op。
    state.enable_time_display(&snapshot);
    assert_eq!(state.enable_requests().len(), 1);
    state.enable_failed();
    assert!(!state.enabling());
    assert_eq!(
        state.enable_error(),
        Some(t("settings.valuePresentation.saveFailed"))
    );
    // 全局配置非法 → 默认 native / iso / local（React handleEnable 的同款回退）。
    let mut bad_global = plain_snapshot();
    bad_global.configs.insert(
        TIMESTAMP_CONTRIBUTION_ID.to_owned(),
        VersionedPresentationConfig {
            config_version: 1,
            config: json!({ "format": "nope" }),
        },
    );
    let mut state2 = open_state(cfg(), &bad_global);
    state2.enable_time_display(&bad_global);
    assert_eq!(
        state2.enable_requests()[0],
        ColumnPresentationEnableRequest::Persist {
            options: TimestampOptions {
                interpretation: TimestampInterpretation::Native,
                format: TimestampOutputFormat::Iso,
                display_time_zone: TimeZoneSpec::Local,
                source_time_zone: None,
            }
        }
    );
    // 开启成功只清 enabling，面板不关闭。
    state2.enable_succeeded();
    assert!(!state2.enabling());
    assert!(state2.is_open());
}

#[test]
fn audit_enable_on_enable_path_records_callback_only() {
    let mut config = cfg();
    config.has_on_enable_time_display = true;
    let snapshot = plain_snapshot();
    let mut state = open_state(config, &snapshot);
    state.enable_time_display(&snapshot);
    assert_eq!(
        state.enable_requests(),
        &[ColumnPresentationEnableRequest::OnEnable]
    );
    assert!(state.is_open());
}

#[test]
fn audit_effective_source_label_interpolates_rule_name() {
    let snapshot = snap(true, Vec::new(), vec![rule_created_unix_seconds()]);
    // inherit 草稿 + 命中规则 → “规则：created-unix-seconds”。
    let state = open_state(cfg(), &snapshot);
    let plan = state.current_plan(&snapshot, &registry(), None);
    assert_eq!(plan.matched_rule_name(), Some("created-unix-seconds"));
    assert_eq!(
        state.effective_source_label(&plan),
        t("columnPresentation.source.rule").replace("{name}", "created-unix-seconds")
    );
    assert!(
        state.effective_source_label(&plan).contains("created-unix-seconds"),
        "the rendered label must carry the rule name"
    );
    // custom 草稿 → 列覆盖标签，与计划无关。
    let custom = open_custom_state(false);
    let plan = custom.current_plan(&snapshot, &registry(), None);
    assert_eq!(
        custom.effective_source_label(&plan),
        t("columnPresentation.source.override")
    );
    // inherit 草稿 + 无规则 → 全局默认标签。
    let no_rule = plain_snapshot();
    let state = open_state(cfg(), &no_rule);
    let plan = state.current_plan(&no_rule, &registry(), None);
    assert_eq!(
        state.effective_source_label(&plan),
        t("columnPresentation.source.global")
    );
}

#[test]
fn audit_open_rebuilds_draft_and_clears_error_but_typing_does_not() {
    let snapshot = plain_snapshot();
    let mut state = open_state(cfg(), &snapshot);
    // 打开时按当前覆盖重建：inherit 模式 + 全局 format iso-tz。
    assert_eq!(state.draft().mode, PresentationMode::Inherit);
    assert_eq!(state.draft().format, TimestampOutputFormat::IsoTz);
    // 造一个 error，再打字（IANA 名输入）：有意偏差 (b)——Rune TextField
    // 无 post-edit 回调，打字不清错；下次 Select 变化时才清。
    state.update_draft(ColumnPresentationDraftPatch {
        mode: Some(PresentationMode::Custom),
        ..ColumnPresentationDraftPatch::default()
    });
    state.set_display_iana_name("Not A Zone!");
    state.save(&known);
    assert!(state.error().is_some());
    state.set_display_iana_name("Asia/Shanghai");
    assert!(
        state.error().is_some(),
        "typing the IANA name does not clear the error (documented deviation)"
    );
    state.update_draft(ColumnPresentationDraftPatch {
        format: Some(TimestampOutputFormat::Iso),
        ..ColumnPresentationDraftPatch::default()
    });
    assert!(state.error().is_none(), "a Select change clears the error");
    // 关闭不重置草稿；重开才重建。
    state.update_draft(ColumnPresentationDraftPatch {
        mode: Some(PresentationMode::Raw),
        ..ColumnPresentationDraftPatch::default()
    });
    state.close();
    assert_eq!(state.draft().mode, PresentationMode::Raw);
    state.open_dialog(&snapshot);
    assert_eq!(state.draft().mode, PresentationMode::Inherit);
}

#[test]
fn audit_choose_select_rejects_illegal_values() {
    let snapshot = plain_snapshot();
    let mut state = open_state(cfg(), &snapshot);
    state.choose_select(ColumnPresentationSelect::Interpretation, "sideways");
    assert_eq!(
        state.draft().interpretation,
        InterpretationChoice::Native,
        "illegal Select value is ignored, like React's is* guards"
    );
    state.choose_select(
        ColumnPresentationSelect::Interpretation,
        "unix-milliseconds",
    );
    assert_eq!(
        state.draft().interpretation,
        InterpretationChoice::UnixMilliseconds
    );
    assert!(!state.is_select_open(ColumnPresentationSelect::Interpretation));
}

#[test]
fn audit_draft_to_override_round_trip_matches_react() {
    let draft = custom_draft();
    let candidate = draft_to_column_override(&draft);
    assert_eq!(
        candidate,
        ColumnPresentationOverride::Custom {
            renderer_id: TIMESTAMP_CONTRIBUTION_ID.to_owned(),
            options: unix_seconds_options().to_json(),
        }
    );
    // unix 解释不带源时区（React draftToTimestampOptions 的同款分支）。
    let options = draft_to_timestamp_options(&draft);
    assert_eq!(options.source_time_zone, None);
}

// ---------------------------------------------------------------------------
// 渲染（自写 Scene harness）
// ---------------------------------------------------------------------------

struct AuditScene {
    state: ColumnPresentationDialogState,
    snapshot: ValuePresentationSnapshot,
    registry: ValuePresentationRegistry,
    presenter: Option<TimestampPresenter>,
    known: Rc<dyn Fn(&str) -> bool>,
    dark: bool,
}

fn audit_scene_view(scene: &AuditScene, _cx: &mut Cx<AuditScene>) -> El {
    let k: &Kit = kit(scene.dark);
    let mut layer = Vec::new();
    let mut dialog = ColumnPresentationDialog::new(&scene.state)
        .snapshot(&scene.snapshot)
        .registry(&scene.registry)
        .is_known_time_zone(Rc::clone(&scene.known))
        .host_mut(|scene: &mut AuditScene| &mut scene.state);
    if let Some(presenter) = &scene.presenter {
        dialog = dialog.presenter(Rc::clone(presenter));
    }
    dialog.build(scene.dark, &mut layer);
    col()
        .key("audit-page")
        .w_full()
        .h_full()
        .child(
            col()
                .key("audit-page-body")
                .w_full()
                .h_full()
                .fill(k.c.bg_base),
        )
        .children(layer)
}

fn audit_runtime(scene: AuditScene) -> Runtime<AuditScene> {
    let mut runtime = Runtime::new(
        scene,
        audit_scene_view,
        (1440.0, 900.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    runtime.frame(0.0);
    runtime
}

fn audit_open_scene(config: ColumnPresentationDialogConfig, snapshot: ValuePresentationSnapshot) -> AuditScene {
    let mut state = ColumnPresentationDialogState::new(config);
    state.open_dialog(&snapshot);
    AuditScene {
        state,
        snapshot,
        registry: registry(),
        presenter: Some(Rc::new(
            |options: &TimestampOptions,
             _kind: PresentationSemanticKind,
             sample: &PresentationValue,
             _env: &PresentationEnvironment| {
                let _ = options;
                let _ = sample;
                PresentationResult::Unchanged {
                    reason: Some("audit-no-engine".to_owned()),
                }
            },
        )),
        known: Rc::new(known),
        dark: true,
    }
}

fn audit_maybe<S>(runtime: &Runtime<S>, key: &str) -> Option<rune::ElementInfo> {
    runtime
        .elements()
        .into_iter()
        .find(|element| !element.exiting && element.key.as_deref() == Some(key))
}

fn audit_text<S>(runtime: &Runtime<S>, key: &str) -> String {
    audit_maybe(runtime, key)
        .unwrap_or_else(|| panic!("{key} is mounted"))
        .text
        .unwrap_or_default()
}

fn audit_click<S>(runtime: &mut Runtime<S>, key: &str) {
    let (x, y) = runtime.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    runtime.frame(0.0);
}

#[test]
fn audit_render_not_enabled_notice_and_enable_records_request() {
    let snapshot = snap(false, Vec::new(), Vec::new());
    let mut runtime = audit_runtime(audit_open_scene(cfg(), snapshot));
    assert!(audit_maybe(&runtime, "column-presentation-not-enabled").is_some());
    assert!(audit_maybe(&runtime, "column-presentation-enable").is_some());
    audit_click(&mut runtime, "column-presentation-enable");
    assert_eq!(runtime.state.state.enable_requests().len(), 1);
    // 开关打开后提示消失。
    let snapshot = plain_snapshot();
    let runtime = audit_runtime(audit_open_scene(cfg(), snapshot));
    assert!(audit_maybe(&runtime, "column-presentation-not-enabled").is_none());
}

#[test]
fn audit_render_contribution_missing_notice_appears_when_unregistered() {
    let mut scene = audit_open_scene(cfg(), plain_snapshot());
    scene.registry = ValuePresentationRegistry::new();
    let runtime = audit_runtime(scene);
    assert!(audit_maybe(&runtime, "column-presentation-contribution-missing").is_some());
    let runtime = audit_runtime(audit_open_scene(cfg(), plain_snapshot()));
    assert!(audit_maybe(&runtime, "column-presentation-contribution-missing").is_none());
}

#[test]
fn audit_render_title_and_source_row_and_preview_original() {
    let runtime = audit_runtime(audit_open_scene(cfg(), plain_snapshot()));
    // 标题含列名（Dialog 底座的 title 文本由 body 外的 title 元素承载，
    // 这里只断言来源行与原值两处 body 文本）。
    assert_eq!(
        audit_text(&runtime, "column-presentation-effective-source"),
        format!(
            "{}: {}",
            t("columnPresentation.effectiveSource"),
            t("columnPresentation.source.global")
        )
    );
    assert_eq!(
        audit_text(&runtime, "column-presentation-original"),
        "2026-01-15T14:30:00Z",
        "no caller sample: PRD instant anchor is the original"
    );
}

#[test]
fn audit_render_unix_badge_only_for_custom_unix() {
    let mut scene = audit_open_scene(cfg(), plain_snapshot());
    scene.state.update_draft(ColumnPresentationDraftPatch {
        mode: Some(PresentationMode::Custom),
        interpretation: Some(InterpretationChoice::UnixMilliseconds),
        ..ColumnPresentationDraftPatch::default()
    });
    let runtime = audit_runtime(scene);
    assert!(audit_maybe(&runtime, "column-presentation-preview-unix-badge").is_some());
    assert_eq!(
        audit_text(&runtime, "column-presentation-preview-unix-badge"),
        t("settings.valuePresentation.preview.unixMilliseconds")
    );
    // inherit 草稿下无 badge。
    let runtime = audit_runtime(audit_open_scene(cfg(), plain_snapshot()));
    assert!(audit_maybe(&runtime, "column-presentation-preview-unix-badge").is_none());
}

#[test]
fn audit_render_save_error_row_keeps_dialog_open() {
    let mut scene = audit_open_scene(cfg(), plain_snapshot());
    scene.state.update_draft(ColumnPresentationDraftPatch {
        mode: Some(PresentationMode::Custom),
        ..ColumnPresentationDraftPatch::default()
    });
    scene.state.set_display_iana_name("Not A Zone!");
    scene.state.save(&known);
    assert!(scene.state.error().is_some());
    let runtime = audit_runtime(scene);
    assert!(audit_maybe(&runtime, "column-presentation-error").is_some());
    assert!(audit_maybe(&runtime, "column-presentation-dialog").is_some());
}
