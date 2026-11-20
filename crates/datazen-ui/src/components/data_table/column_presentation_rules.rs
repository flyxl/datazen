//! T054-S2: `ColumnPresentationDialog` 的注册中心与规则层 ——
//! `packages/frontend/src/lib/valuePresentation/` 里本面板用到的
//! `valuePresentationRegistry`（贡献在场与否 + `revision`）与
//! `valuePresentationRules.ts`（规则匹配）的最小镜像。
//!
//! # 范围
//!
//! - 注册中心：[`ValuePresentationRegistry`]（`get` / `register` /
//!   `unregister` / `revision`）；只有 `datazen.timestamp` 是内置
//!   贡献（[`ValuePresentationContribution::timestamp`]，
//!   `registerBundledContributions.ts` 唯一注册项）。
//! - 规则：[`ValuePresentationRule`] 与 [`find_first_matching_rule`]
//!   （精确模式全等；受限正则走 `regex` crate，见
//!   [`match_name_pattern`]）。
//!
//! # 接线口（不是本片的活，别在这里补）
//!
//! - **注册中心修订订阅（T11）**：React 用 `useSyncExternalStore`
//!   订阅 `registry.subscribe`，贡献在面板打开期间停用/重启用时缺
//!   失提示即时出现/消失。Rune 没有等价订阅：宿主持有注册中心，变
//!   化时重渲染，对话框每次 `build` 都重读
//!   [`ValuePresentationRegistry::revision`] 与 `get`，所以同一行
//!   为在对话打开期间的停用/重启用只要宿主重渲染就会即时反映 ——
//!   不依赖父级快照重建，但依赖宿主把注册中心的变化传进来。
//! - **规则验证（T293）**：[`match_name_pattern`] 假定 pattern 已
//!   经 `validateRestrictedPattern` 验证（受限子集：字面量、字符类、
//!   `(?:…)` 组、量词、交替；无环视、无反向引用）；未验证的非法
//!   pattern 编译失败时按不匹配处理（TS `matchNamePattern` 的防
//!   御性 catch）。规则本身的校验与存储是 T293 的活。
//!
//! # 关键语义（别丢）
//!
//! - 首条命中规则冻结整列解释（PRD §4）；规则只在时间渲染器启用
//!   时非空（快照构建时门控）；贡献缺失时规则不报规则错，落到默认
//!   路由由其报 `missing-renderer` / `default`。

// The production consumers are the DataTable wiring task (registry
// mutation) and the future rules task (T293); until then several items
// are exercised by `tests/column_presentation_dialog_s2.rs` only.
#![allow(dead_code)]

use std::collections::HashMap;

use regex::bytes::RegexBuilder;

use super::column_presentation_dialog::{TimestampOptions, TIMESTAMP_CONTRIBUTION_ID};

/// 时间展示器贡献的配置 schema 版本（`timestampContribution.ts` 的
/// `TIMESTAMP_CONTRIBUTION_CONFIG_VERSION`，v1）。
pub(crate) const TIMESTAMP_CONTRIBUTION_CONFIG_VERSION: u64 = 1;

// ---------------------------------------------------------------------------
// 注册中心最小镜像
// ---------------------------------------------------------------------------

/// 一个展示贡献的注册项：本片只用 id 与配置版本（`prepare` /
/// `validateConfig` 属于时间引擎接线口，不在这里建模）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValuePresentationContribution {
    pub(crate) id: String,
    pub(crate) config_version: u64,
}

impl ValuePresentationContribution {
    /// 内置时间展示器（`registerBundledContributions.ts` 唯一注册项）。
    pub(crate) fn timestamp() -> Self {
        Self {
            id: TIMESTAMP_CONTRIBUTION_ID.to_owned(),
            config_version: TIMESTAMP_CONTRIBUTION_CONFIG_VERSION,
        }
    }
}

/// 展示注册中心最小镜像：贡献在场与否 + `revision`（每次注册 /
/// 注销 +1）。宿主持有它，变化时重渲染；对话框每次 `build` 重读
/// `revision` 与 `get`，即 T11 的即时出现/消失（见模块头）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ValuePresentationRegistry {
    contributions: HashMap<String, ValuePresentationContribution>,
    revision: u64,
}

impl ValuePresentationRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// 只有 `datazen.timestamp` 的注册中心（`registerBundledContributions`
    /// 的等价物：核心内置贡献随应用启动激活、永不卸载）。
    pub(crate) fn with_timestamp() -> Self {
        let mut registry = Self::new();
        registry.register(ValuePresentationContribution::timestamp());
        registry
    }

    /// 注册一个贡献；已存在时不重复注册、不动 `revision`（与
    /// `registerBundledValuePresentationContributions` 的幂等守卫一致，
    /// TS 的 `register` 本身对重复 ID 抛错）。
    pub(crate) fn register(&mut self, contribution: ValuePresentationContribution) {
        if self.contributions.contains_key(&contribution.id) {
            return;
        }
        self.contributions
            .insert(contribution.id.clone(), contribution);
        self.revision += 1;
    }

    /// 注销一个贡献；在场时删掉并 +1，不在场时不动。
    pub(crate) fn unregister(&mut self, id: &str) {
        if self.contributions.remove(id).is_some() {
            self.revision += 1;
        }
    }

    pub(crate) fn get(&self, id: &str) -> Option<&ValuePresentationContribution> {
        self.contributions.get(id)
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }
}

// ---------------------------------------------------------------------------
// 规则最小镜像
// ---------------------------------------------------------------------------

/// 名称模式：精确值（全等匹配）或受限正则（只保存 source 与受限
/// flags，绝不持久化 RegExp）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ValuePresentationNamePattern {
    Exact(String),
    Regex {
        source: String,
        flags: Option<String>,
    },
}

/// 一条规则的匹配条件：全部可选，出现者为 AND；至少出现一个。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ValuePresentationRuleMatch {
    pub(crate) connection_id: Option<String>,
    pub(crate) database: Option<String>,
    pub(crate) namespace: Option<String>,
    pub(crate) table: Option<ValuePresentationNamePattern>,
    pub(crate) column: Option<ValuePresentationNamePattern>,
    pub(crate) database_type_name: Option<ValuePresentationNamePattern>,
}

/// 一条全局规则：名称、启用开关、匹配条件、时间选项（复用 T02）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValuePresentationRule {
    pub(crate) name: String,
    pub(crate) enabled: bool,
    pub(crate) match_: ValuePresentationRuleMatch,
    pub(crate) options: TimestampOptions,
}

/// 规则匹配的列上下文：表级字段缺失时表级条件不满足。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ValuePresentationRuleContext<'a> {
    pub(crate) connection_id: Option<&'a str>,
    pub(crate) database: Option<&'a str>,
    pub(crate) namespace: Option<&'a str>,
    pub(crate) table: Option<&'a str>,
    pub(crate) column: &'a str,
    pub(crate) database_type_name: Option<&'a str>,
}

/// 匹配一个名称模式：精确值全等；正则用未锚定子串匹配（TS
/// `RegExp.test`）。
///
/// 正则走 `regex` crate 的 bytes API：`unicode(false)` 让 `\d` /
/// `\w` / `\s` / `\b` 按 ASCII 解释（与 JS 一致）。必须用 bytes
/// 而不用 str API：str 的 `RegexBuilder::unicode(false)` 会把 `.`
/// / `.*` 认作可匹配非法 UTF-8 而直接拒绝编译（`^epoch_.*` 都会
/// 失败）；bytes 模式下 `.*` 按字节匹配，对合法 UTF-8 输入与 JS
/// 的 `.` 等价。残余差异（T293 复核时再看）：JS 的 `.` 不匹配
/// `\r` / `\u2028` / `\u2029`，这里只排除 `\n`；JS 的 `i` 按
/// Unicode 折叠大小写，这里只折叠 ASCII。编译失败（未经验证的
/// pattern）按不匹配处理，与 TS 的防御性 catch 一致。
pub(crate) fn match_name_pattern(pattern: &ValuePresentationNamePattern, value: &str) -> bool {
    match pattern {
        ValuePresentationNamePattern::Exact(expected) => expected == value,
        ValuePresentationNamePattern::Regex { source, flags } => {
            let case_insensitive = match flags.as_deref() {
                None | Some("") => false,
                Some("i") => true,
                Some(_) => return false,
            };
            match RegexBuilder::new(source)
                .case_insensitive(case_insensitive)
                .unicode(false)
                .build()
            {
                Ok(regex) => regex.is_match(value.as_bytes()),
                Err(_) => false,
            }
        }
    }
}

/// 一条规则是否命中：`enabled` 由调用方（`find_first_matching_rule`）
/// 先筛；这里只看匹配条件（AND）。
pub(crate) fn match_value_presentation_rule(
    rule: &ValuePresentationRule,
    context: &ValuePresentationRuleContext<'_>,
) -> bool {
    let match_ = &rule.match_;
    if let Some(expected) = match_.connection_id.as_deref() {
        if context.connection_id != Some(expected) {
            return false;
        }
    }
    if let Some(expected) = match_.database.as_deref() {
        if context.database != Some(expected) {
            return false;
        }
    }
    if let Some(expected) = match_.namespace.as_deref() {
        if context.namespace != Some(expected) {
            return false;
        }
    }
    if let Some(pattern) = &match_.table {
        match context.table {
            Some(table) if match_name_pattern(pattern, table) => {}
            _ => return false,
        }
    }
    if let Some(pattern) = &match_.column {
        if !match_name_pattern(pattern, context.column) {
            return false;
        }
    }
    if let Some(pattern) = &match_.database_type_name {
        match context.database_type_name {
            Some(name) if match_name_pattern(pattern, name) => {}
            _ => return false,
        }
    }
    true
}

/// 首条命中规则：按列表顺序，跳过禁用规则，返回第一条命中的规则；
/// 无命中返回 `None`。命中后冻结整列解释，调用方不再继续找低优先
/// 级规则（PRD §4）。
pub(crate) fn find_first_matching_rule<'a>(
    rules: &'a [ValuePresentationRule],
    context: &ValuePresentationRuleContext<'_>,
) -> Option<&'a ValuePresentationRule> {
    rules
        .iter()
        .filter(|rule| rule.enabled)
        .find(|rule| match_value_presentation_rule(rule, context))
}
