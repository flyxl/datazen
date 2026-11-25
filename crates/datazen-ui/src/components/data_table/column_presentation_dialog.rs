//! T054-S1: `ColumnPresentationDialog` 的草稿模型 + 纯函数 ——
//! `packages/frontend/src/components/DataTable/ColumnPresentationDialog.tsx`
//! 里导出的全部纯函数与类型（800 行 React 文件的第一片）。
//!
//! # 本片范围（S1）
//!
//! - 草稿类型 [`ColumnPresentationDraft`] 与四个选择类型
//!   （[`PresentationMode`] / [`InterpretationChoice`] /
//!   [`TimeZoneKindChoice`] / [`SourceTimeZoneChoice`]）；
//! - 草稿 ↔ 配置的双向映射：[`draft_to_timestamp_options`]、
//!   [`draft_to_column_override`]、[`draft_from_column_override`]；
//! - 校验：[`validate_column_presentation_draft`]（保存与预览共用同一结果）、
//!   [`validate_timestamp_options`]、[`validate_column_presentation_override`]；
//! - 快照全局默认：[`global_timestamp_options_from_snapshot`]；
//! - 小守卫：[`is_interpretation_choice`]、[`is_output_format`]、
//!   [`is_time_zone_kind_choice`]、[`is_source_time_zone_choice`]。
//!
//! # 依赖盘点（查过，无则做最小镜像，不编 API）
//!
//! - `lib/valuePresentation/index`（`TIMESTAMP_CONTRIBUTION_ID`、
//!   `validateTimestampOptions`、`validateColumnPresentationOverride`、
//!   `isValidIanaTimeZoneName`、`isValidTimeZoneName`、
//!   `TimestampOptions` / `TimestampOutputFormat` / `TimeZoneSpec` /
//!   `ValuePresentationSnapshot`）：Rune 侧**尚无** valuePresentation 层，
//!   本片是第一个最小镜像，只含 S1 用到的部分 ——
//!   `validate_timestamp_options` 与 `validate_column_presentation_override`
//!   是 TS 未知入参（`unknown`）校验的逐行镜像，入参用
//!   `serde_json::Value`，实现在同目录的
//!   `column_presentation_validation.rs`（单文件 ≤800 行才拆出，
//!   本模块重导出，S2 与测试仍从这里一个入口导入）；
//!   [`TimestampOptions::to_json`] /
//!   [`ColumnPresentationOverride::to_json`] 只做规范序列化（仅含已识别字段，
//!   `sourceTimeZone` 缺省时不出现该键），供草稿到校验的往返用。
//!   其余导出（`buildColumnPresentationPlans`、`computeColumnPresentationPreview`、
//!   `columnOverrideFromSnapshot`、`snapshotWithoutColumnOverride`、
//!   `fallbackSampleForColumn`、`formatColumnPresentationSampleValue`、
//!   `saveColumnOverride`、`withTimestampRenderer`）是 S2 / 后续片的活，
//!   不在本模块。
//! - `resolvePresentationSemanticKind`：Rune 侧**尚无**。S1 不需要它 ——
//!   [`draft_from_column_override`] 直接收 [`PresentationSemanticKind`]
//!   入参，语义种的推导（`CellType` / `CellTypeHints` → 种）留给 S2
//!   在调用处接线（[`CellTypeHints`] 已有 Rune 版：
//!   `crate::support::cell_types::CellTypeHints`）。
//! - `ColumnPresentationOverride` / `ColumnSource` / `PresentationValue` /
//!   `ValuePresentationRegistry`（`@datazen/extension-points`）：
//!   `ColumnPresentationOverride` 在此做了最小镜像；
//!   `ColumnSource` / `PresentationValue` / `ValuePresentationRegistry`
//!   是 S2 预览与保存链路的输入，本片不需要，不镜像。
//! - `ValuePresentationSnapshot`：S1 只镜像了 `configs` 字段
//!   （[`VersionedPresentationConfig`] 亦然）；S2 已把 `revision` /
//!   `environment` / `enabledRendererIds` / `overrides` / `rules` 全部扩
//!   进来（环境与规则类型见 `column_presentation_plan.rs`）。
//! - `isValidTimeZoneName`（已知性，TS 用 Intl try/catch）：本 crate
//!   没有时区数据库（无 `chrono-tz`，`chrono` 只带本地时区探测），
//!   无法在本地复现 Intl 的已知性判断。接线口是
//!   [`validate_column_presentation_draft`] 的
//!   `is_known_time_zone: &dyn Fn(&str) -> bool` 参数 —— 调用方（S2 /
//!   宿主）传入平台的已知性检查；测试里用桩谓词覆盖两条分支。
//!   [`is_valid_iana_time_zone_name`] 则是格式校验的精确镜像
//!   （非空、≤100 字符、只含 `[A-Za-z0-9_+\-/]`），不依赖任何数据库。
//!
//! # 关键语义（别丢）
//!
//! - IANA 名只做 `trim`，不判格式 / 已知性 —— 判格式与已知性只在
//!   [`validate_column_presentation_draft`] 里做一次，保存与预览共用。
//! - BIGINT 选时间后必须明确指定 Unix 秒或毫秒：
//!   [`draft_from_column_override`] 对 `number` 种默认显式选
//!   `unix-milliseconds`（下拉里可见、可改），面板不以位数自动决定单位。
//! - 文本输入空格沿用 T07 裁决：名内空白非法（
//!   [`is_valid_iana_time_zone_name`] 的字符集不含空白；
//!   `rendererId` 同样 trim 后判空白）。
//! - 严格 TS「禁 any、不出现 `new Date(string)`」的对应约束：本模块
//!   不做任何字符串 → 时刻的解析 —— 时间值只以
//!   [`TimestampOptions`] 结构流转，时刻解析属于 timestamp 贡献 /
//!   temporalMath 层，不在本片。
//!
//! # S1 / S2 分界
//!
//! S2（对话框组件）拥有：`ColumnPresentationDraft` 状态、打开时
//! `draft_from_column_override` 重建草稿、effect、预览 / 计划 memo、
//! `handleSave` / `handleEnableTimeDisplay`、全部渲染。S2 的接线口就是
//! 本模块的公开函数：
//!
//! - 打开 / 换列：`draft_from_column_override(current, global_options, kind)`；
//! - 全局默认：`global_timestamp_options_from_snapshot(snapshot)`；
//! - 保存 / 预览候选：`draft_to_column_override(draft)`；
//! - 校验（保存与预览共用）：
//!   `validate_column_presentation_draft(draft, is_known_time_zone)`，
//!   其中 `is_known_time_zone` 由宿主提供；
//! - Select 的 `onChange`：四个 `is_*` 守卫先判字符串合法再更新草稿。

use std::collections::HashMap;

use serde_json::{json, Value};

// 校验镜像的实现在同目录的 `column_presentation_validation.rs`；这里
// 重导出，S2 与测试只从本模块一个入口导入。
pub use super::column_presentation_validation::{
    problem, validate_column_presentation_override, validate_timestamp_options, ValidationProblem,
};

/// `datazen.timestamp`（`lib/valuePresentation/timestampContribution.ts`）。
pub const TIMESTAMP_CONTRIBUTION_ID: &str = "datazen.timestamp";

/// 草稿里 IANA 名缺省时的占位（React 硬编码 `'Asia/Shanghai'` 四处，
//  这里只写一次；它是输入框的初始值，不是校验通过的保证）。
pub const DEFAULT_DISPLAY_IANA_NAME: &str = "Asia/Shanghai";

/// 面板模式：跟随规则 / 原始值 / 时间（custom）。
#[allow(dead_code)] // S1 只读 mode；S2（对话框）构造各模式的草稿。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationMode {
    Inherit,
    Raw,
    Custom,
}

/// 时间来源选择：原生 / Unix 秒 / Unix 毫秒 / 文本。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterpretationChoice {
    Native,
    UnixSeconds,
    UnixMilliseconds,
    Text,
}

impl InterpretationChoice {
    /// 字符串 → 选择；非法值返回 `None`（S2 的 Select `onChange` 先用
    /// [`is_interpretation_choice`] 守卫，合法时再用本函数取值）。
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "native" => Some(Self::Native),
            "unix-seconds" => Some(Self::UnixSeconds),
            "unix-milliseconds" => Some(Self::UnixMilliseconds),
            "text" => Some(Self::Text),
            _ => None,
        }
    }
}

/// 展示时区种：本地 / UTC / IANA 名。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeZoneKindChoice {
    Local,
    Utc,
    Iana,
}

impl TimeZoneKindChoice {
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "local" => Some(Self::Local),
            "utc" => Some(Self::Utc),
            "iana" => Some(Self::Iana),
            _ => None,
        }
    }
}

/// 源时区种：无 / 本地 / UTC / IANA 名。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceTimeZoneChoice {
    None,
    Local,
    Utc,
    Iana,
}

impl SourceTimeZoneChoice {
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "none" => Some(Self::None),
            "local" => Some(Self::Local),
            "utc" => Some(Self::Utc),
            "iana" => Some(Self::Iana),
            _ => None,
        }
    }
}

/// 时间输出格式：UTC ISO / 带时区 ISO / 本地格式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimestampOutputFormat {
    Iso,
    IsoTz,
    Locale,
}

impl TimestampOutputFormat {
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "iso" => Some(Self::Iso),
            "iso-tz" => Some(Self::IsoTz),
            "locale" => Some(Self::Locale),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Iso => "iso",
            Self::IsoTz => "iso-tz",
            Self::Locale => "locale",
        }
    }
}

/// Unix 单位：秒 / 毫秒（显式值，面板不以位数自动决定）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnixUnit {
    Seconds,
    Milliseconds,
}

/// `TimestampOptions.interpretation`（`timestampConfig.ts`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TimestampInterpretation {
    Native,
    Unix(UnixUnit),
    Text,
}

/// 时区规约（`timestampFormat.ts` 的 `TimeZoneSpec`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TimeZoneSpec {
    Utc,
    Local,
    Iana { name: String },
}

impl TimeZoneSpec {
    /// 规范 JSON（仅含已识别字段；`iana` 才带 `name`）。
    fn to_json(&self) -> Value {
        match self {
            Self::Utc => json!({ "kind": "utc" }),
            Self::Local => json!({ "kind": "local" }),
            Self::Iana { name } => json!({ "kind": "iana", "name": name }),
        }
    }
}

/// 时间展示器配置（`timestampConfig.ts` 的 `TimestampOptions`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimestampOptions {
    pub interpretation: TimestampInterpretation,
    pub format: TimestampOutputFormat,
    pub display_time_zone: TimeZoneSpec,
    /// 只出现一处：无时区文本与原生 local-datetime 的源时区；
    /// `unix` 解释绝不带它（`None` 时序列化不出现该键）。
    pub source_time_zone: Option<TimeZoneSpec>,
}

impl TimestampOptions {
    /// 规范 JSON（仅含已识别字段；`sourceTimeZone` 缺省时不出现该键，
    /// 与 TS `validateTimestampOptions` 成功返回的深冻结对象同形）。
    pub fn to_json(&self) -> Value {
        let mut object = serde_json::Map::new();
        object.insert(
            "interpretation".to_owned(),
            match &self.interpretation {
                TimestampInterpretation::Native => json!({ "kind": "native" }),
                TimestampInterpretation::Unix(unit) => json!({
                    "kind": "unix",
                    "unit": match unit {
                        UnixUnit::Seconds => "seconds",
                        UnixUnit::Milliseconds => "milliseconds",
                    },
                }),
                TimestampInterpretation::Text => json!({ "kind": "text" }),
            },
        );
        object.insert(
            "format".to_owned(),
            Value::String(self.format.as_str().to_owned()),
        );
        object.insert(
            "displayTimeZone".to_owned(),
            self.display_time_zone.to_json(),
        );
        if let Some(source) = &self.source_time_zone {
            object.insert("sourceTimeZone".to_owned(), source.to_json());
        }
        Value::Object(object)
    }
}

/// 面板草稿：模式 + 时间选项的表单状态（IANA 名为原始输入，保存时 trim）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnPresentationDraft {
    pub mode: PresentationMode,
    pub interpretation: InterpretationChoice,
    pub format: TimestampOutputFormat,
    pub display_time_zone_kind: TimeZoneKindChoice,
    pub display_iana_name: String,
    pub source_time_zone_kind: SourceTimeZoneChoice,
    pub source_iana_name: String,
}

/// 表列覆盖（`@datazen/extension-points` 的 `ColumnPresentationOverride`）。
/// `custom` 的 `options` 保持原始 JSON（TS 的 `unknown`）：宿主在边界
/// 用 [`validate_timestamp_options`] 校验，不由本类型背书合法性。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ColumnPresentationOverride {
    Inherit,
    Raw,
    Custom { renderer_id: String, options: Value },
}

impl ColumnPresentationOverride {
    /// 规范 JSON（仅含已识别字段），供
    /// [`validate_column_presentation_override`] 做往返校验。
    pub fn to_json(&self) -> Value {
        match self {
            Self::Inherit => json!({ "mode": "inherit" }),
            Self::Raw => json!({ "mode": "raw" }),
            Self::Custom {
                renderer_id,
                options,
            } => json!({
                "mode": "custom",
                "rendererId": renderer_id,
                "options": options,
            }),
        }
    }
}

/// 展示语义种（`PresentationColumnContext['semanticKind']`）。
/// S1 只拿它选默认解释；种的推导（`resolvePresentationSemanticKind`）
/// 是 S2 调用处的活，本片不做。
#[allow(dead_code)] // S2 传入推导好的种；S1 只匹配 `number` 与其余。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationSemanticKind {
    Instant,
    LocalDatetime,
    Date,
    Time,
    Number,
    Text,
    Other,
}

/// 已验证的版本化配置快照（`VersionedPresentationConfig`）。
/// `config` 是原始 JSON（`PresentationConfigValue`），宿主在边界交给
/// 目标贡献的 `validateConfig` 验证。
#[allow(dead_code)] // S2 / 宿主构建快照；S1 只读 `configs`。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionedPresentationConfig {
    pub config_version: u64,
    pub config: Value,
}

/// 展示快照的 S1 最小镜像：只有 [`global_timestamp_options_from_snapshot`]
/// 读到的 `configs`；S2 需要 `revision` / `environment` /
/// `enabledRendererIds` / `overrides` / `rules` 时再扩。
///
/// S2 已扩（T054-S2）：`revision` / `environment` /
/// `enabled_renderer_ids` / `overrides` / `rules` 全部就位 ——
/// 规则与环境类型的定义在同目录的 `column_presentation_plan.rs`
/// （计划层），本模块只存字段，不解释它们。
#[allow(dead_code)] // S2 / 宿主构建快照；S1 只读 `configs`。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ValuePresentationSnapshot {
    pub revision: u64,
    pub environment: super::column_presentation_plan::PresentationEnvironment,
    pub configs: HashMap<String, VersionedPresentationConfig>,
    pub enabled_renderer_ids: Vec<String>,
    pub overrides: HashMap<String, ColumnPresentationOverride>,
    pub rules: Vec<super::column_presentation_rules::ValuePresentationRule>,
}

// ---------------------------------------------------------------------------
// 小守卫（React 的 `isInterpretationChoice` / `isOutputFormat` /
// `isTimeZoneKindChoice` / `isSourceTimeZoneChoice`；S2 的 Select `onChange`
// 先守卫再更新草稿）
// ---------------------------------------------------------------------------

/// `value` 是合法的 [`InterpretationChoice`] 字符串吗？
#[allow(dead_code)] // 消费者：S2 的 interpretation Select `onChange`。
pub fn is_interpretation_choice(value: &str) -> bool {
    InterpretationChoice::from_str(value).is_some()
}

/// `value` 是合法的 [`TimestampOutputFormat`] 字符串吗？
#[allow(dead_code)] // 消费者：S2 的 format Select `onChange`。
pub fn is_output_format(value: &str) -> bool {
    TimestampOutputFormat::from_str(value).is_some()
}

/// `value` 是合法的 [`TimeZoneKindChoice`] 字符串吗？
#[allow(dead_code)] // 消费者：S2 的 display-timezone Select `onChange`。
pub fn is_time_zone_kind_choice(value: &str) -> bool {
    TimeZoneKindChoice::from_str(value).is_some()
}

/// `value` 是合法的 [`SourceTimeZoneChoice`] 字符串吗？
#[allow(dead_code)] // 消费者：S2 的 source-timezone Select `onChange`。
pub fn is_source_time_zone_choice(value: &str) -> bool {
    SourceTimeZoneChoice::from_str(value).is_some()
}

// ---------------------------------------------------------------------------
// IANA 名格式校验（`timestampFormat.ts` 的 `isValidIanaTimeZoneName`
// 精确镜像：非空、无空白、合理字符集、长度上限 100）
// ---------------------------------------------------------------------------

/// IANA 时区名的格式校验（只看格式，不查已知性——已知性由
/// [`validate_column_presentation_draft`] 的调用方谓词判断）。
pub fn is_valid_iana_time_zone_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 100 {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-' | '/'))
}

// ---------------------------------------------------------------------------
// 草稿 → 配置 / 覆盖
// ---------------------------------------------------------------------------

fn time_zone_spec(kind: TimeZoneKindChoice, iana_name: &str) -> TimeZoneSpec {
    match kind {
        TimeZoneKindChoice::Iana => TimeZoneSpec::Iana {
            name: iana_name.trim().to_owned(),
        },
        TimeZoneKindChoice::Local => TimeZoneSpec::Local,
        TimeZoneKindChoice::Utc => TimeZoneSpec::Utc,
    }
}

fn source_time_zone_spec(kind: SourceTimeZoneChoice, iana_name: &str) -> Option<TimeZoneSpec> {
    match kind {
        SourceTimeZoneChoice::None => None,
        SourceTimeZoneChoice::Local => Some(TimeZoneSpec::Local),
        SourceTimeZoneChoice::Utc => Some(TimeZoneSpec::Utc),
        SourceTimeZoneChoice::Iana => Some(TimeZoneSpec::Iana {
            name: iana_name.trim().to_owned(),
        }),
    }
}

fn time_zone_kind_choice(spec: &TimeZoneSpec) -> TimeZoneKindChoice {
    match spec {
        TimeZoneSpec::Iana { .. } => TimeZoneKindChoice::Iana,
        TimeZoneSpec::Local => TimeZoneKindChoice::Local,
        TimeZoneSpec::Utc => TimeZoneKindChoice::Utc,
    }
}

fn source_time_zone_choice(spec: Option<&TimeZoneSpec>) -> SourceTimeZoneChoice {
    match spec {
        None => SourceTimeZoneChoice::None,
        Some(TimeZoneSpec::Iana { .. }) => SourceTimeZoneChoice::Iana,
        Some(TimeZoneSpec::Local) => SourceTimeZoneChoice::Local,
        Some(TimeZoneSpec::Utc) => SourceTimeZoneChoice::Utc,
    }
}

/// 草稿 → `TimestampOptions`（custom 模式用）。IANA 名只做 trim，
/// 不在这里判格式 / 已知性 —— 校验由
/// [`validate_column_presentation_draft`] 统一做，预览与保存共用同一结果。
pub fn draft_to_timestamp_options(draft: &ColumnPresentationDraft) -> TimestampOptions {
    let interpretation = match draft.interpretation {
        InterpretationChoice::UnixSeconds => TimestampInterpretation::Unix(UnixUnit::Seconds),
        InterpretationChoice::UnixMilliseconds => {
            TimestampInterpretation::Unix(UnixUnit::Milliseconds)
        }
        InterpretationChoice::Text => TimestampInterpretation::Text,
        InterpretationChoice::Native => TimestampInterpretation::Native,
    };
    let display_time_zone = time_zone_spec(draft.display_time_zone_kind, &draft.display_iana_name);
    // Unix 数值本身无墙上时间语义，不带源时区。
    let source_time_zone = match draft.interpretation {
        InterpretationChoice::UnixSeconds | InterpretationChoice::UnixMilliseconds => None,
        _ => source_time_zone_spec(draft.source_time_zone_kind, &draft.source_iana_name),
    };
    TimestampOptions {
        interpretation,
        format: draft.format,
        display_time_zone,
        source_time_zone,
    }
}

/// 草稿 → 列覆盖（非 custom 模式不带 options）。
pub fn draft_to_column_override(
    draft: &ColumnPresentationDraft,
) -> ColumnPresentationOverride {
    match draft.mode {
        PresentationMode::Inherit => ColumnPresentationOverride::Inherit,
        PresentationMode::Raw => ColumnPresentationOverride::Raw,
        PresentationMode::Custom => ColumnPresentationOverride::Custom {
            renderer_id: TIMESTAMP_CONTRIBUTION_ID.to_owned(),
            options: draft_to_timestamp_options(draft).to_json(),
        },
    }
}

// ---------------------------------------------------------------------------
// 草稿校验与还原
// ---------------------------------------------------------------------------

/// 草稿校验（保存与预览共用）：IANA 名 trim 后判格式与已知性
/// （T07 裁决：名内空白非法）；其余走 [`validate_timestamp_options`] 与
/// [`validate_column_presentation_override`]。返回问题列表，空即合法。
///
/// 有问题先返 IANA 问题，再走两层校验 —— 与 React 的返回顺序一致。
///
/// `is_known_time_zone` 是已知性检查的接线口（TS 用 Intl；本 crate
/// 无时区数据库，调用方传入平台检查；见模块头「依赖盘点」）。
#[allow(dead_code)] // 消费者：S2 的保存与预览（共用同一结果）。
pub fn validate_column_presentation_draft(
    draft: &ColumnPresentationDraft,
    is_known_time_zone: &dyn Fn(&str) -> bool,
) -> Vec<ValidationProblem> {
    if draft.mode != PresentationMode::Custom {
        return Vec::new();
    }
    let mut problems: Vec<ValidationProblem> = Vec::new();
    let check_iana =
        |kind_is_iana: bool, name: &str, path: &str, problems: &mut Vec<ValidationProblem>| {
            if !kind_is_iana {
                return;
            }
            let trimmed = name.trim();
            if trimmed.is_empty() || !is_valid_iana_time_zone_name(trimmed) {
                problems.push(problem(path, "expected a valid IANA time zone name"));
            } else if !is_known_time_zone(trimmed) {
                problems.push(problem(path, "unknown time zone name"));
            }
        };
    check_iana(
        draft.display_time_zone_kind == TimeZoneKindChoice::Iana,
        &draft.display_iana_name,
        "displayTimeZone.name",
        &mut problems,
    );
    if matches!(
        draft.interpretation,
        InterpretationChoice::Text | InterpretationChoice::Native
    ) {
        check_iana(
            draft.source_time_zone_kind == SourceTimeZoneChoice::Iana,
            &draft.source_iana_name,
            "sourceTimeZone.name",
            &mut problems,
        );
    }
    if !problems.is_empty() {
        return problems;
    }
    match validate_timestamp_options(&draft_to_timestamp_options(draft).to_json()) {
        Ok(_) => {}
        Err(option_problems) => return option_problems,
    }
    match validate_column_presentation_override(&draft_to_column_override(draft).to_json()) {
        Ok(_) => {}
        Err(override_problems) => return override_problems,
    }
    Vec::new()
}

/// 从当前覆盖 + 全局默认还原草稿（面板每次打开时重建）。
///
/// - `number` 种默认显式选 Unix 毫秒（下拉里可见、可改，不是按位数推断）；
///   原生时间列与其余列默认 `native`（预览会说明不适用）；
/// - `format` 默认全局值，否则 `iso-tz`；展示 IANA 默认
///   `Asia/Shanghai`；源时区默认无；
/// - `custom` 模式下非法 options 时回落到 base + `custom`。
#[allow(dead_code)] // 消费者：S2 打开 / 换列时重建草稿。
pub fn draft_from_column_override(
    current: &ColumnPresentationOverride,
    global_options: Option<&TimestampOptions>,
    semantic_kind: PresentationSemanticKind,
) -> ColumnPresentationDraft {
    let base = ColumnPresentationDraft {
        mode: PresentationMode::Inherit,
        interpretation: if semantic_kind == PresentationSemanticKind::Number {
            InterpretationChoice::UnixMilliseconds
        } else {
            InterpretationChoice::Native
        },
        format: global_options.map_or(TimestampOutputFormat::IsoTz, |options| options.format),
        display_time_zone_kind: global_options.map_or(TimeZoneKindChoice::Local, |options| {
            time_zone_kind_choice(&options.display_time_zone)
        }),
        display_iana_name: match global_options.map(|options| &options.display_time_zone) {
            Some(TimeZoneSpec::Iana { name }) => name.clone(),
            _ => DEFAULT_DISPLAY_IANA_NAME.to_owned(),
        },
        source_time_zone_kind: SourceTimeZoneChoice::None,
        source_iana_name: DEFAULT_DISPLAY_IANA_NAME.to_owned(),
    };
    match current {
        ColumnPresentationOverride::Inherit => ColumnPresentationDraft {
            mode: PresentationMode::Inherit,
            ..base
        },
        ColumnPresentationOverride::Raw => ColumnPresentationDraft {
            mode: PresentationMode::Raw,
            ..base
        },
        ColumnPresentationOverride::Custom { options, .. } => {
            let Ok(options) = validate_timestamp_options(options) else {
                return ColumnPresentationDraft {
                    mode: PresentationMode::Custom,
                    ..base
                };
            };
            let interpretation = match &options.interpretation {
                TimestampInterpretation::Unix(UnixUnit::Seconds) => {
                    InterpretationChoice::UnixSeconds
                }
                TimestampInterpretation::Unix(UnixUnit::Milliseconds) => {
                    InterpretationChoice::UnixMilliseconds
                }
                TimestampInterpretation::Text => InterpretationChoice::Text,
                TimestampInterpretation::Native => InterpretationChoice::Native,
            };
            ColumnPresentationDraft {
                mode: PresentationMode::Custom,
                interpretation,
                format: options.format,
                display_time_zone_kind: time_zone_kind_choice(&options.display_time_zone),
                display_iana_name: match &options.display_time_zone {
                    TimeZoneSpec::Iana { name } => name.clone(),
                    _ => DEFAULT_DISPLAY_IANA_NAME.to_owned(),
                },
                source_time_zone_kind: source_time_zone_choice(options.source_time_zone.as_ref()),
                source_iana_name: match &options.source_time_zone {
                    Some(TimeZoneSpec::Iana { name }) => name.clone(),
                    _ => DEFAULT_DISPLAY_IANA_NAME.to_owned(),
                },
            }
        }
    }
}

/// 全局默认时间选项（快照 `configs` 里 `datazen.timestamp` 一项，
/// 非法时缺省）。
#[allow(dead_code)] // 消费者：S2 读全局默认（`useMemo` 的对应位置）。
pub fn global_timestamp_options_from_snapshot(
    snapshot: &ValuePresentationSnapshot,
) -> Option<TimestampOptions> {
    let config = snapshot.configs.get(TIMESTAMP_CONTRIBUTION_ID)?;
    validate_timestamp_options(&config.config).ok()
}
