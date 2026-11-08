//! 语言包 IPC 投影（线契约，serde camelCase）。
//!
//! 桌面与将来的 Web 宿主对同一份注册表投影出同一个 JSON；字段键变化
//! 即线契约变化。钉子测试见 `manager.rs` 尾部。

use std::collections::BTreeMap;

use serde::Serialize;

/// 已装语言包的列表条目（`list_language_packs` 应答元素）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguagePackSummary {
    pub code: String,
    pub label: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 包内全部域的译文总条数（安装/扫描时一次算好）。
    pub message_count: usize,
    /// 安装目录名（与 `code` 相同；单独投影便于将来目录名与 code 分离）。
    pub file_name: String,
}

/// 预检（inspect）应答里的单域统计。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LanguagePackDomainCount {
    pub domain: String,
    pub message_count: usize,
}

/// `read_language_pack_messages` 的应答：messages 供前端注册进共享翻译
/// 引擎，label 是下拉框显示的语言自称——一次 IPC 拿全注册所需的一切。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguagePackMessages {
    pub code: String,
    pub label: String,
    pub messages: BTreeMap<String, String>,
}
