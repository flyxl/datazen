//! T054-S2: `ColumnPresentationDialog` 的计划 / 预览层 ——
//! `packages/frontend/src/lib/valuePresentation/` 里本面板用到的纯逻辑
//! （`presentationPlan.ts` 的计划构建、`columnPresentation.ts` 的预览与
//! 样本、`valuePresentationRules.ts` 的规则匹配），为对话框服务的最
//! 小镜像。
//!
//! # 本片范围（S2 的计划层）
//!
//! - 语义种：[`resolve_presentation_semantic_kind`]（`resolvePresentationSemanticKind`）；
//! - 来源与键：[`ColumnSource`]、[`column_override_key`]、
//!   [`column_override_from_snapshot`]、[`snapshot_without_column_override`]；
//! - 样本：[`fallback_sample_for_column`]、
//!   [`format_column_presentation_sample_value`]；
//! - 注册中心与规则见 [`super::column_presentation_rules`]：
//!   [`ValuePresentationRegistry`](super::column_presentation_rules::ValuePresentationRegistry)
//!   与 [`find_first_matching_rule`](super::column_presentation_rules::find_first_matching_rule)
//!   （精确模式全等；受限正则走 `regex` crate）。
//! - 计划：[`build_column_presentation_plan`]（决策顺序 a–e 与
//!   `presentationPlan.ts` 一致）与 [`ColumnPresentationPlan`]；
//! - 预览：[`compute_column_presentation_preview`]。
//!
//! # 接线口（不是本片的活，别在这里补）
//!
//! - **时间引擎（T253/T209/T274）**：计划 `ready` 与 custom 预览的
//!   `present` 需要时间展示器 `prepare → present`（`temporalMath` 的
//!   解析 + `timestampFormat` 的格式化 + IANA 时区偏移），本 crate
//!   没有时区数据库，写不出来。接线口是 [`TimestampPresenter`]：宿
//!   主（未来的时间引擎接线任务）传入
//!   `options + semanticKind + sample + environment → PresentationResult`，
//!   计划与预览只负责按决策顺序选生效配置、把值交给它。**没有
//!   presenter 时**，`ready` 计划的 `present` 与 custom 预览一律按
//!   TS 的防御回退走 `unchanged` / `plan-error`（
//!   `fallbackPresentationReason('ready')` 的原话：ready 一定携带
//!   prepared，不应走到回退；防御性取值与 error 一致）——预览显示原
//!   值，不编造格式化文本。
//! - **注册中心修订订阅（T11）**与**规则验证（T293）**：见
//!   [`super::column_presentation_rules`] 模块头 —— 本模块只在建
//!   计划时读注册中心与规则，不拥有它们。
//!
//! # 关键语义（别丢）
//!
//! - 覆盖 > 规则 > 全局默认（PRD §4）；首条命中规则冻结整列解释。
//! - `inherit` 的计划与预览按「清除覆盖后」的快照算（T09-BUG-001）：
//!   直接拿合并快照建计划会让待清除的 custom 覆盖优先于规则。剥离
//!   由 [`snapshot_without_column_override`] 做，调用方（对话框状
//!   态）在草稿为 `inherit` 时用剥离后的快照建计划，custom / raw
//!   草稿的来源本就是列覆盖，不依赖该计划。
//! - custom 覆盖 bypass `enabled` 门禁（具体选择胜过全局开关）；
//!   默认路由才查 `enabledRendererIds`。
//! - 规则只在时间渲染器启用时非空（快照构建时门控）；贡献缺失时规
//!   则不报规则错，落到默认路由由其报 `missing-renderer` / `default`。
//! - `present` 永不抛异常：presenter 之外的回退一律是确定性的
//!   `unchanged` + 原因码，不记录原始行值。

// The production consumers are the DataTable wiring task (registry
// mutation, presenter injection) and the future time-engine tasks;
// until then several items are exercised by
// `tests/column_presentation_dialog_s2.rs` only.
#![allow(dead_code)]

use std::rc::Rc;

use serde_json::{json, Value};

use super::column_presentation_dialog::{
    validate_timestamp_options, ColumnPresentationOverride, PresentationSemanticKind,
    TimestampOptions, ValuePresentationSnapshot, TIMESTAMP_CONTRIBUTION_ID,
};
use super::column_presentation_rules::{
    find_first_matching_rule, ValuePresentationRegistry, ValuePresentationRuleContext,
    TIMESTAMP_CONTRIBUTION_CONFIG_VERSION,
};
use crate::support::cell_types::{
    is_zoned_cell_type_with_hints, resolve_cell_type, CellType, CellTypeHints,
};
use crate::support::formatters::number_text;

/// 展示环境（`PresentationEnvironment`）：`locale` 缺省 `en-US`，
/// `localTimeZone` 缺省 `UTC`（与 TS `resolvePresentationEnvironment`
/// 的回退一致）；宿主在快照构建时填真实值。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PresentationEnvironment {
    pub(crate) locale: String,
    pub(crate) local_time_zone: String,
}

impl Default for PresentationEnvironment {
    fn default() -> Self {
        Self {
            locale: "en-US".to_owned(),
            local_time_zone: "UTC".to_owned(),
        }
    }
}

impl PresentationEnvironment {
    pub(crate) fn new(locale: impl Into<String>, local_time_zone: impl Into<String>) -> Self {
        Self {
            locale: locale.into(),
            local_time_zone: local_time_zone.into(),
        }
    }
}

/// 表身份（TS 的 `Omit<ColumnSource, 'column'>`：列名不属于来源，
/// 调用时再拼；无来源即查询结果列，覆盖不可用）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ColumnSource {
    pub(crate) connection_id: String,
    pub(crate) database: Option<String>,
    pub(crate) namespace: Option<String>,
    pub(crate) table: String,
}

/// 展示值（TS 的 `PresentationValue`：`null` / 数字 / 字符串 /
/// 布尔 / 数组 / 对象；`bigint` 在 JSON 层无对应，超大整数由宿主
/// 以十进制字符串传入，见 `timestampContribution.ts` 裁决 5）。
pub(crate) type PresentationValue = Value;

/// 计划输入的列（`PresentationPlanColumn`：TS 还可带 `cellType`，
/// 对话框调用处从不传，本片只从 `dataType` + `hints` 推导）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PresentationPlanColumn {
    pub(crate) name: String,
    pub(crate) data_type: Option<String>,
    pub(crate) hints: Option<CellTypeHints>,
}

/// 表列覆盖键：`JSON.stringify([connectionId, database ?? null,
/// namespace ?? null, table, column])`，不做大小写归一；缺省字段
/// 用 null、空字符串保留为空串，二者键不同。
pub(crate) fn column_override_key(source: &ColumnSource, column: &str) -> String {
    serde_json::to_string(&json!([
        source.connection_id,
        source.database,
        source.namespace,
        source.table,
        column,
    ]))
    .unwrap_or_default()
}

/// 从快照取一列的当前覆盖：无来源（查询结果）或无条目时为 `inherit`。
pub(crate) fn column_override_from_snapshot(
    snapshot: &ValuePresentationSnapshot,
    source: Option<&ColumnSource>,
    column_name: &str,
) -> ColumnPresentationOverride {
    let Some(source) = source else {
        return ColumnPresentationOverride::Inherit;
    };
    snapshot
        .overrides
        .get(&column_override_key(source, column_name))
        .cloned()
        .unwrap_or(ColumnPresentationOverride::Inherit)
}

/// 剥离本列覆盖后的快照副本（T09-BUG-001）：键从复制的 overrides
/// 里删除（不是置 `undefined`），原快照不动；无来源列或本列无条目
/// 时返回原快照的克隆。
pub(crate) fn snapshot_without_column_override(
    snapshot: &ValuePresentationSnapshot,
    source: Option<&ColumnSource>,
    column_name: &str,
) -> ValuePresentationSnapshot {
    let Some(source) = source else {
        return snapshot.clone();
    };
    let key = column_override_key(source, column_name);
    if !snapshot.overrides.contains_key(&key) {
        return snapshot.clone();
    }
    let mut stripped = snapshot.clone();
    stripped.overrides.remove(&key);
    stripped
}

/// 语义种映射（`resolvePresentationSemanticKind`）：
/// datetime → zoned ? `instant` : `local-datetime`；date → `date`；
/// time → `time`；number → `number`；text / enum / uuid → `text`；
/// bool / json / bytes / unknown → `other`。
pub(crate) fn resolve_presentation_semantic_kind(
    data_type: Option<&str>,
    hints: Option<&CellTypeHints>,
) -> PresentationSemanticKind {
    match resolve_cell_type(data_type, hints) {
        CellType::DateTime => {
            if is_zoned_cell_type_with_hints(data_type.unwrap_or(""), hints) {
                PresentationSemanticKind::Instant
            } else {
                PresentationSemanticKind::LocalDatetime
            }
        }
        CellType::Date => PresentationSemanticKind::Date,
        CellType::Time => PresentationSemanticKind::Time,
        CellType::Number => PresentationSemanticKind::Number,
        CellType::Text | CellType::Enum | CellType::Uuid => PresentationSemanticKind::Text,
        CellType::Bool | CellType::Json | CellType::Bytes | CellType::Unknown => {
            PresentationSemanticKind::Other
        }
    }
}

/// 按语义种给预览样本（PRD §3.2 双锚点；调用方有真实行值时优先用
/// 真实行值，本函数只在无行值时兜底）：
/// number → 1705316400000（Unix 毫秒锚点；秒/毫秒由用户显式选，
/// 不以位数自动决定）；instant → `'2026-01-15T14:30:00Z'`；
/// local-datetime → `'2026-01-15 14:30:00'`；date → `'2026-01-15'`；
/// time → `'14:30:00'`；text / other → `None`（无可预览的时间样本）。
pub(crate) fn fallback_sample_for_column(
    column: &PresentationPlanColumn,
) -> Option<PresentationValue> {
    let kind =
        resolve_presentation_semantic_kind(column.data_type.as_deref(), column.hints.as_ref());
    match kind {
        PresentationSemanticKind::Number => Some(json!(1705316400000i64)),
        PresentationSemanticKind::Instant => Some(json!("2026-01-15T14:30:00Z")),
        PresentationSemanticKind::LocalDatetime => Some(json!("2026-01-15 14:30:00")),
        PresentationSemanticKind::Date => Some(json!("2026-01-15")),
        PresentationSemanticKind::Time => Some(json!("14:30:00")),
        PresentationSemanticKind::Text | PresentationSemanticKind::Other => None,
    }
}

/// 样本值的展示文本：NULL 仍为 NULL；对象与数组用 JSON，不抛异常。
pub(crate) fn format_column_presentation_sample_value(value: &PresentationValue) -> String {
    match value {
        Value::Null => "NULL".to_owned(),
        Value::String(text) => text.clone(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number_text(number),
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}

// ---------------------------------------------------------------------------
// 计划
// ---------------------------------------------------------------------------

/// 列展示计划状态（`ColumnPresentationStatus`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ColumnPresentationStatus {
    Ready,
    Default,
    Raw,
    Disabled,
    MissingRenderer,
    MissingConfig,
    InvalidConfig,
    Error,
}

/// 无 prepared 时 present 的稳定回退原因（按状态取值）：
/// default → `'no-renderer'`、raw → `'raw-override'`、disabled →
/// `'disabled'`、missing-renderer → `'missing-renderer'`、
/// missing-config → `'missing-config'`、invalid-config →
/// `'invalid-config'`、error → `'plan-error'`；ready 一定携带
/// prepared，不应走到回退，防御性取值与 error 一致。
pub(crate) fn fallback_presentation_reason(status: ColumnPresentationStatus) -> &'static str {
    match status {
        ColumnPresentationStatus::Ready => "plan-error",
        ColumnPresentationStatus::Default => "no-renderer",
        ColumnPresentationStatus::Raw => "raw-override",
        ColumnPresentationStatus::Disabled => "disabled",
        ColumnPresentationStatus::MissingRenderer => "missing-renderer",
        ColumnPresentationStatus::MissingConfig => "missing-config",
        ColumnPresentationStatus::InvalidConfig => "invalid-config",
        ColumnPresentationStatus::Error => "plan-error",
    }
}

/// 一次展示的结果（`PresentationResult`）：格式化文本，或原值直出
/// 的原因。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PresentationResult {
    Text {
        text: String,
        interpretation: Option<String>,
    },
    Unchanged {
        reason: Option<String>,
    },
}

impl PresentationResult {
    fn unchanged(reason: &str) -> Self {
        Self::Unchanged {
            reason: Some(reason.to_owned()),
        }
    }
}

/// 时间引擎接线口（见模块头）：宿主传入的时间展示器 `present` ——
/// 生效配置 + 语义种 + 样本值 + 环境 → 展示结果。T253/T209/T274
/// 的时间引擎接上后由它注入；此前传 `None`，`ready` 计划按
/// `plan-error` 回退。
pub(crate) type TimestampPresenter = Rc<
    dyn Fn(
        &TimestampOptions,
        PresentationSemanticKind,
        &PresentationValue,
        &PresentationEnvironment,
    ) -> PresentationResult,
>;

/// 一列的展示计划：状态与统一 `present` 入口。`present` 永不抛异常；
/// 回退计划的 `present` 一律返回对应原因的 `unchanged`。
pub(crate) struct ColumnPresentationPlan {
    column_name: String,
    status: ColumnPresentationStatus,
    renderer_id: Option<String>,
    reason: Option<String>,
    matched_rule_name: Option<String>,
    present: Rc<dyn Fn(&PresentationValue) -> PresentationResult>,
}

impl ColumnPresentationPlan {
    pub(crate) fn column_name(&self) -> &str {
        &self.column_name
    }

    pub(crate) fn status(&self) -> ColumnPresentationStatus {
        self.status
    }

    pub(crate) fn renderer_id(&self) -> Option<&str> {
        self.renderer_id.as_deref()
    }

    pub(crate) fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    /// 命中的全局规则名（T08，PRD §3.3 配置来源提示用）；无规则命
    /// 中时缺省。
    pub(crate) fn matched_rule_name(&self) -> Option<&str> {
        self.matched_rule_name.as_deref()
    }

    /// 展示一个值：`ready` 时走时间引擎（或无引擎时的 `plan-error`
    /// 回退），其余状态一律回退原值。
    pub(crate) fn present(&self, value: &PresentationValue) -> PresentationResult {
        (self.present)(value)
    }
}

fn fallback_plan(
    column_name: &str,
    status: ColumnPresentationStatus,
    renderer_id: Option<String>,
) -> ColumnPresentationPlan {
    let reason = fallback_presentation_reason(status);
    let reason_owned = reason.to_owned();
    ColumnPresentationPlan {
        column_name: column_name.to_owned(),
        status,
        renderer_id,
        reason: Some(reason_owned.clone()),
        matched_rule_name: None,
        present: Rc::new(move |_| PresentationResult::unchanged(&reason_owned)),
    }
}

fn ready_plan(
    column_name: &str,
    renderer_id: &str,
    options: TimestampOptions,
    semantic_kind: PresentationSemanticKind,
    environment: &PresentationEnvironment,
    presenter: Option<&TimestampPresenter>,
) -> ColumnPresentationPlan {
    let present: Rc<dyn Fn(&PresentationValue) -> PresentationResult> = match presenter {
        Some(presenter) => {
            let presenter = Rc::clone(presenter);
            let environment = environment.clone();
            Rc::new(move |value| presenter(&options, semantic_kind, value, &environment))
        }
        // 无时间引擎：TS 的防御回退（`fallbackPresentationReason('ready')`）。
        None => Rc::new(|_| PresentationResult::unchanged("plan-error")),
    };
    ColumnPresentationPlan {
        column_name: column_name.to_owned(),
        status: ColumnPresentationStatus::Ready,
        renderer_id: Some(renderer_id.to_owned()),
        reason: None,
        matched_rule_name: None,
        present,
    }
}

/// 为一列构建展示计划（决策顺序见模块头；与
/// `buildOneColumnPlan` 的 a–e 一致，`prepare` 由 presenter 接线）。
///
/// - a. 查覆盖：source 存在时按键取；source 缺省时无覆盖。
/// - b. 覆盖 `raw` → 状态 `raw`，不调 presenter。
/// - c. 覆盖 `custom` → 显式选择目标渲染器，bypass `enabled` 门禁；
///   贡献缺失 → `missing-renderer`；options 非法 → `invalid-config`；
///   已注册的非时间贡献无法在本片准备（无其 `validateConfig` 镜
///   像）→ `error` / `plan-error`，见模块头。
/// - d. 规则：覆盖之后、全局默认之前。首条命中冻结整列解释；贡献
///   缺失时不报规则错，落到默认路由。
/// - e. 默认路由：非时间语义列 → `default`；时间语义列依次检查
///   `enabled` 门禁、贡献缺失、配置缺失、版本（快照配置版本与贡献
///   声明不一致 → `invalid-config`）。
pub(crate) fn build_column_presentation_plan(
    column: &PresentationPlanColumn,
    source: Option<&ColumnSource>,
    snapshot: &ValuePresentationSnapshot,
    registry: &ValuePresentationRegistry,
    presenter: Option<&TimestampPresenter>,
) -> ColumnPresentationPlan {
    let semantic_kind =
        resolve_presentation_semantic_kind(column.data_type.as_deref(), column.hints.as_ref());

    // a. 查覆盖。
    let override_: Option<&ColumnPresentationOverride> = match source {
        None => None,
        Some(source) => snapshot
            .overrides
            .get(&column_override_key(source, &column.name)),
    };

    // b. 覆盖 `raw` → 状态 `raw`。
    if matches!(override_, Some(ColumnPresentationOverride::Raw)) {
        return fallback_plan(&column.name, ColumnPresentationStatus::Raw, None);
    }

    // c. 覆盖 `custom` → 显式选择目标渲染器，bypass `enabled` 门禁。
    if let Some(ColumnPresentationOverride::Custom {
        renderer_id,
        options,
    }) = override_
    {
        let Some(contribution) = registry.get(renderer_id) else {
            return fallback_plan(
                &column.name,
                ColumnPresentationStatus::MissingRenderer,
                Some(renderer_id.clone()),
            );
        };
        if renderer_id != TIMESTAMP_CONTRIBUTION_ID {
            // 已注册的非时间贡献：本片没有它的 `validateConfig` 镜
            // 像，无法准备；按贡献异常隔离为 `error` / `plan-error`
            // （`preparePlan` 的异常兜底形状）。
            return fallback_plan(
                &column.name,
                ColumnPresentationStatus::Error,
                Some(renderer_id.clone()),
            );
        }
        let Ok(options) = validate_timestamp_options(options) else {
            return fallback_plan(
                &column.name,
                ColumnPresentationStatus::InvalidConfig,
                Some(renderer_id.clone()),
            );
        };
        // custom 的生效配置版本即贡献声明版本，版本检查恒过。
        debug_assert_eq!(
            contribution.config_version,
            TIMESTAMP_CONTRIBUTION_CONFIG_VERSION
        );
        return ready_plan(
            &column.name,
            renderer_id,
            options,
            semantic_kind,
            &snapshot.environment,
            presenter,
        );
    }

    // d. 规则：覆盖之后、全局默认之前。
    if !snapshot.rules.is_empty() {
        if let Some(rule_contribution) = registry.get(TIMESTAMP_CONTRIBUTION_ID) {
            let context = ValuePresentationRuleContext {
                connection_id: source.map(|source| source.connection_id.as_str()),
                database: source.and_then(|source| source.database.as_deref()),
                namespace: source.and_then(|source| source.namespace.as_deref()),
                table: source.map(|source| source.table.as_str()),
                column: &column.name,
                database_type_name: column.data_type.as_deref(),
            };
            if let Some(rule) = find_first_matching_rule(&snapshot.rules, &context) {
                debug_assert_eq!(
                    rule_contribution.config_version,
                    TIMESTAMP_CONTRIBUTION_CONFIG_VERSION
                );
                let mut plan = ready_plan(
                    &column.name,
                    TIMESTAMP_CONTRIBUTION_ID,
                    rule.options.clone(),
                    semantic_kind,
                    &snapshot.environment,
                    presenter,
                );
                plan.matched_rule_name = Some(rule.name.clone());
                return plan;
            }
        }
    }

    // e. 默认路由。
    let is_temporal = matches!(
        semantic_kind,
        PresentationSemanticKind::Instant
            | PresentationSemanticKind::LocalDatetime
            | PresentationSemanticKind::Date
            | PresentationSemanticKind::Time
    );
    if !is_temporal {
        return fallback_plan(&column.name, ColumnPresentationStatus::Default, None);
    }
    if !snapshot
        .enabled_renderer_ids
        .iter()
        .any(|id| id == TIMESTAMP_CONTRIBUTION_ID)
    {
        return fallback_plan(
            &column.name,
            ColumnPresentationStatus::Disabled,
            Some(TIMESTAMP_CONTRIBUTION_ID.to_owned()),
        );
    }
    let Some(contribution) = registry.get(TIMESTAMP_CONTRIBUTION_ID) else {
        return fallback_plan(
            &column.name,
            ColumnPresentationStatus::MissingRenderer,
            Some(TIMESTAMP_CONTRIBUTION_ID.to_owned()),
        );
    };
    let Some(config) = snapshot.configs.get(TIMESTAMP_CONTRIBUTION_ID) else {
        return fallback_plan(
            &column.name,
            ColumnPresentationStatus::MissingConfig,
            Some(TIMESTAMP_CONTRIBUTION_ID.to_owned()),
        );
    };
    if config.config_version != contribution.config_version {
        return fallback_plan(
            &column.name,
            ColumnPresentationStatus::InvalidConfig,
            Some(TIMESTAMP_CONTRIBUTION_ID.to_owned()),
        );
    }
    let Ok(options) = validate_timestamp_options(&config.config) else {
        return fallback_plan(
            &column.name,
            ColumnPresentationStatus::InvalidConfig,
            Some(TIMESTAMP_CONTRIBUTION_ID.to_owned()),
        );
    };
    ready_plan(
        &column.name,
        TIMESTAMP_CONTRIBUTION_ID,
        options,
        semantic_kind,
        &snapshot.environment,
        presenter,
    )
}

// ---------------------------------------------------------------------------
// 预览
// ---------------------------------------------------------------------------

/// 列配置预览结果：与 T07 的 `TimestampPreviewResult` 同形；
/// `unavailable` / `invalid` / `text` / `unchanged` 四种，外加
/// 无样本时对话框渲染的 `noSample`（由调用方按 `has_sample` 决定，
/// 不属于本结果）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ColumnPresentationPreview {
    Unavailable {
        sample: PresentationValue,
    },
    Invalid {
        problems: Vec<super::column_presentation_dialog::ValidationProblem>,
        sample: PresentationValue,
    },
    Text {
        text: String,
        interpretation: Option<String>,
        sample: PresentationValue,
    },
    Unchanged {
        reason: Option<String>,
        sample: PresentationValue,
    },
}

fn map_presentation_result(
    result: PresentationResult,
    sample: PresentationValue,
) -> ColumnPresentationPreview {
    match result {
        PresentationResult::Text {
            text,
            interpretation,
        } => ColumnPresentationPreview::Text {
            text,
            interpretation,
            sample,
        },
        PresentationResult::Unchanged { reason } => {
            ColumnPresentationPreview::Unchanged { reason, sample }
        }
    }
}

/// 列配置预览（与运行时同一条路，不另写解析）：
/// - `inherit`：先剥离本列仍存的覆盖再建计划并 `present(sample)` ——
///   预览的是清除覆盖后的结果（规则 / 全局默认），而不是仍存覆盖
///   的结果（T09-BUG-001）；
/// - `raw`：不调 presenter，直接 `unchanged`（原因 `raw-override`）；
/// - `custom`：贡献缺失 → `unavailable`；`validateConfig` 失败 →
///   `invalid` 与路径化问题；成功则经 presenter `present(sample)`
///   （无 presenter 时 `plan-error` 回退，见模块头）。
#[allow(clippy::too_many_arguments)]
pub(crate) fn compute_column_presentation_preview(
    candidate: &ColumnPresentationOverride,
    column: &PresentationPlanColumn,
    source: Option<&ColumnSource>,
    snapshot: &ValuePresentationSnapshot,
    sample: PresentationValue,
    registry: &ValuePresentationRegistry,
    presenter: Option<&TimestampPresenter>,
) -> ColumnPresentationPreview {
    match candidate {
        ColumnPresentationOverride::Inherit => {
            let stripped = snapshot_without_column_override(snapshot, source, &column.name);
            let plan =
                build_column_presentation_plan(column, source, &stripped, registry, presenter);
            map_presentation_result(plan.present(&sample), sample)
        }
        ColumnPresentationOverride::Raw => ColumnPresentationPreview::Unchanged {
            reason: Some("raw-override".to_owned()),
            sample,
        },
        ColumnPresentationOverride::Custom {
            renderer_id,
            options,
        } => {
            if registry.get(renderer_id).is_none() {
                return ColumnPresentationPreview::Unavailable { sample };
            }
            if renderer_id != TIMESTAMP_CONTRIBUTION_ID {
                // 见计划 c 分支：本片无法准备非时间贡献；预览按
                // S1 覆盖校验的同一 verdict 报 `invalid`。
                return ColumnPresentationPreview::Invalid {
                    problems: vec![super::column_presentation_dialog::problem(
                        "rendererId",
                        "unknown renderer",
                    )],
                    sample,
                };
            }
            match validate_timestamp_options(options) {
                Err(problems) => ColumnPresentationPreview::Invalid { problems, sample },
                Ok(options) => {
                    let semantic_kind = resolve_presentation_semantic_kind(
                        column.data_type.as_deref(),
                        column.hints.as_ref(),
                    );
                    let result = match presenter {
                        Some(presenter) => {
                            presenter(&options, semantic_kind, &sample, &snapshot.environment)
                        }
                        None => PresentationResult::unchanged("plan-error"),
                    };
                    map_presentation_result(result, sample)
                }
            }
        }
    }
}
